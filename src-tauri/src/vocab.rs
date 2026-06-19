//! 自定义词典 / 热词。三层纠正，互补：
//! ① 软词表：把术语正确写法作为 <词表> 注入 LLM 上下文，让它结合上下文 + 拼音判断纠错 / 保留术语；
//! ② 硬替换：在 LLM 之前对"几乎总错成同一写法"的安全情形做确定性字符串替换（零延迟、可控）；
//! ③ 拼音纠错词：在 LLM 之前对「读音相同、字面不同」的连续汉字片段做确定性归一替换
//!    （绕开 SenseVoice 这类 CTC 模型不支持解码层热词的限制；CapsWriter-Offline 同栈思路）。
//!    一条「中文强」即可纠掉「中文墙 / 中文枪」等所有同音变体，且对所有润色风格（含纯逐字稿）生效。
//!
//! 词典从文件读：env `VOCAB_PATH`，缺省 `<manifest>/vocab.txt`（开发期；生产化见任务 #5）。
//! 文件缺失 / 读失败则返回空词典，全程 no-op。
//!
//! 文件语法（每行）：
//! - `#` 开头为注释；空行忽略
//! - `误识 => 正确`：硬替换（确定性地把"误识"换成"正确"；"正确"同时计入软词表）
//! - `~正确词`：拼音纠错词（参与 ③ 的拼音模糊匹配；同时计入软词表供 LLM 参考）
//! - 其余整行：软词表里的一个术语正确写法
//!
//! 为何只有 `~` 词参与拼音匹配：像「优化(you hua)/油画」「流式(liu shi)/流逝」无声调拼音完全相同，
//! 若对所有软词做拼音替换会把「一幅油画」误伤成「一幅优化」。故拼音匹配仅对用户主动用 `~`
//! 标记的纠错词生效——既精准又可控。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 词典的结构化视图（供「词典」UI 读写、序列化回文件）。三类互不重叠：
/// - `soft_terms`：纯软术语（只注入 <词表> 供 LLM 参考）；
/// - `pinyin_terms`：拼音纠错词（即文件里的 `~` 词；同音误识强制归一，同时也注入 LLM）；
/// - `replacements`：硬替换（确定性整串替换）。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct VocabData {
    #[serde(default)]
    pub soft_terms: Vec<String>,
    #[serde(default)]
    pub pinyin_terms: Vec<String>,
    #[serde(default)]
    pub replacements: Vec<Replacement>,
}

/// 一条硬替换：把"误识"整串换成"正确"。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replacement {
    pub wrong: String,
    pub right: String,
}

impl VocabData {
    /// 解析词典文本为结构化视图（每类内部去重；语法同文件头注释）。
    pub fn parse(content: &str) -> Self {
        let mut data = VocabData::default();
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((wrong, right)) = line.split_once("=>") {
                let (wrong, right) = (wrong.trim().to_string(), right.trim().to_string());
                if wrong.is_empty() || right.is_empty() {
                    continue;
                }
                if !data.replacements.iter().any(|r| r.wrong == wrong) {
                    data.replacements.push(Replacement { wrong, right });
                }
            } else if let Some(term) = line.strip_prefix('~') {
                let term = term.trim().to_string();
                if !term.is_empty() && !data.pinyin_terms.contains(&term) {
                    data.pinyin_terms.push(term);
                }
            } else if !data.soft_terms.iter().any(|t| t == line) {
                data.soft_terms.push(line.to_string());
            }
        }
        data
    }

    /// 从文件读取并解析；缺失 / 读失败返回空。
    pub fn from_path(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(c) => Self::parse(&c),
            Err(_) => Self::default(),
        }
    }

    /// 序列化回 vocab.txt 文本（带分区注释；供 UI 保存，可继续被 parse 读回）。
    pub fn to_file_string(&self) -> String {
        let mut s = String::from(
            "# 自定义词典 / 热词（由 app「词典」页管理，也可手动编辑）\n\
             # 语法（每行一个）：软词 ｜ ~拼音纠错词 ｜ 误识 => 正确；# 注释、空行忽略\n\n",
        );
        s.push_str("# —— 软术语（仅注入 <词表> 供 LLM 参考）——\n");
        for t in &self.soft_terms {
            s.push_str(t);
            s.push('\n');
        }
        s.push_str("\n# —— 拼音纠错词（~：读音相同即强制归一，一条顶所有同音变体）——\n");
        for t in &self.pinyin_terms {
            s.push('~');
            s.push_str(t);
            s.push('\n');
        }
        s.push_str("\n# —— 硬替换（误识 => 正确）——\n");
        for r in &self.replacements {
            s.push_str(&r.wrong);
            s.push_str(" => ");
            s.push_str(&r.right);
            s.push('\n');
        }
        s
    }
}

#[derive(Default)]
pub struct Vocab {
    /// 术语正确写法（软层：注入 <词表> 供 LLM 参考）。
    pub terms: Vec<String>,
    /// 确定性替换（硬层）：(误识, 正确)；按"误识"长度降序，先替换长词，避免短词抢匹配。
    replacements: Vec<(String, String)>,
    /// 拼音纠错词（用户用 `~` 标记）：(正确词, 无声调拼音序列)，按汉字数降序。
    /// 只有这些参与 `apply_pinyin_corrections`——避免对普通软词（如"优化"）误伤同音的"油画"。
    pinyin_terms: Vec<(String, Vec<String>)>,
}

impl Vocab {
    /// 解析词典文本（纯函数，便于单测）。
    pub fn parse(content: &str) -> Self {
        Self::from_data(&VocabData::parse(content))
    }

    /// 由结构化词典构造运行期引擎：
    /// - `terms` = 软词 ∪ 拼音纠错词 ∪ 硬替换右侧（去重，注入 <词表> 给 LLM）；
    /// - `replacements` 按"误识"长度降序（先替长词，避免短词抢匹配）；
    /// - `pinyin_terms` 预算无声调拼音、剔除 <2 字或含非汉字者、按字数降序（长词优先）。
    pub fn from_data(data: &VocabData) -> Self {
        let mut terms: Vec<String> = Vec::new();
        let mut add_term = |t: &str| {
            if !t.is_empty() && !terms.iter().any(|x| x == t) {
                terms.push(t.to_string());
            }
        };
        for t in &data.soft_terms {
            add_term(t);
        }
        for t in &data.pinyin_terms {
            add_term(t);
        }
        for r in &data.replacements {
            add_term(&r.right);
        }

        let mut replacements: Vec<(String, String)> = data
            .replacements
            .iter()
            .map(|r| (r.wrong.clone(), r.right.clone()))
            .collect();
        replacements.sort_by_key(|r| std::cmp::Reverse(r.0.chars().count()));

        let mut pinyin_terms: Vec<(String, Vec<String>)> = Vec::new();
        for t in &data.pinyin_terms {
            if pinyin_terms.iter().any(|(x, _)| x == t) {
                continue;
            }
            if let Some(py) = term_pinyin(t) {
                pinyin_terms.push((t.clone(), py));
            }
        }
        pinyin_terms.sort_by_key(|t| std::cmp::Reverse(t.1.len()));

        Self {
            terms,
            replacements,
            pinyin_terms,
        }
    }

    /// 从指定文件加载；缺失 / 读失败则返回空词典（no-op）。
    pub fn from_path(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => Self::parse(&content),
            Err(_) => Self::default(),
        }
    }

    /// 开发期 / 测试便捷加载：env VOCAB_PATH，缺省 <manifest>/vocab.txt。
    /// 生产期的多级路径解析（用户配置目录 > 打包资源）见 crate::load_vocab。
    pub fn load() -> Self {
        let path = std::env::var("VOCAB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vocab.txt"));
        Self::from_path(&path)
    }

    /// 硬层：对文本做确定性替换（误识 → 正确），返回替换后的文本。
    pub fn apply_replacements(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (wrong, right) in &self.replacements {
            if out.contains(wrong) {
                out = out.replace(wrong, right);
            }
        }
        out
    }

    /// 拼音层：把文本中「与某拼音纠错词读音相同但字面不同」的连续汉字片段，
    /// 归一替换为该正确词。只对用户用 `~` 标记的纠错词生效（不碰普通软词，避免误伤同音常用词）；
    /// 长词优先；只处理连续汉字、英文/数字/标点原样保留。在 LLM / 直出之前调用，对所有风格生效。
    pub fn apply_pinyin_corrections(&self, text: &str) -> String {
        if self.pinyin_terms.is_empty() {
            return text.to_string();
        }
        use pinyin::ToPinyin;
        let chars: Vec<char> = text.chars().collect();
        // 每个字符的无声调拼音（非汉字为 None）；与 chars 逐字符对齐。
        let pys: Vec<Option<&str>> = text.to_pinyin().map(|o| o.map(|p| p.plain())).collect();
        if chars.len() != pys.len() {
            return text.to_string(); // 理论不会发生：to_pinyin 逐字符产出
        }
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            // pinyin_terms 已按长度降序：从位置 i 起第一个「整窗读音相等」的词即最长匹配。
            let hit = self.pinyin_terms.iter().find(|(_, py)| {
                let l = py.len();
                i + l <= chars.len()
                    && (0..l).all(|k| matches!(pys[i + k], Some(s) if s == py[k].as_str()))
            });
            match hit {
                Some((term, py)) => {
                    out.push_str(term); // 读音相同 → 归一到正确词（字面已正确则等价于原样）
                    i += py.len();
                }
                None => {
                    out.push(chars[i]);
                    i += 1;
                }
            }
        }
        out
    }
}

/// 计算词的无声调拼音序列；含非汉字或不足 2 个汉字则返回 None（不参与拼音匹配）。
fn term_pinyin(term: &str) -> Option<Vec<String>> {
    use pinyin::ToPinyin;
    let mut syllables: Vec<String> = Vec::new();
    for py in term.to_pinyin() {
        syllables.push(py?.plain().to_string());
    }
    (syllables.len() >= 2).then_some(syllables)
}

#[cfg(test)]
mod tests {
    use super::{Replacement, Vocab, VocabData};

    #[test]
    fn parses_soft_terms_and_hard_replacements_with_dedup() {
        let v = Vocab::parse("# 注释\n流式\n停录\n停路 => 停录\n\n  优化  \n");
        assert!(v.terms.contains(&"流式".to_string()));
        assert!(v.terms.contains(&"停录".to_string()));
        assert!(v.terms.contains(&"优化".to_string()));
        // "停录" 同时来自裸词与 => 右侧，应去重只出现一次
        assert_eq!(v.terms.iter().filter(|t| *t == "停录").count(), 1);
    }

    #[test]
    fn applies_hard_replacement() {
        let v = Vocab::parse("停路 => 停录");
        assert_eq!(v.apply_replacements("记得点一下停路"), "记得点一下停录");
        assert_eq!(v.apply_replacements("没有误识"), "没有误识");
    }

    #[test]
    fn replaces_longest_wrong_first() {
        let v = Vocab::parse("AB => X\nABC => Y");
        // 应整体匹配 "ABC" → "Y"，而不是先 "AB"→"X" 留下 "XC"
        assert_eq!(v.apply_replacements("ABC"), "Y");
    }

    #[test]
    fn empty_dictionary_is_noop() {
        let v = Vocab::parse("# 只有注释\n\n   \n");
        assert!(v.terms.is_empty());
        assert_eq!(v.apply_replacements("原样不动"), "原样不动");
    }

    #[test]
    fn from_path_missing_is_empty() {
        let v = Vocab::from_path(std::path::Path::new("/no/such/vocab.txt"));
        assert!(v.terms.is_empty());
    }

    // —— ③ 拼音纠错词 ——

    #[test]
    fn pinyin_term_also_enters_soft_terms() {
        // `~词` 既参与拼音匹配，也进软词表（LLM 仍能看到）
        let v = Vocab::parse("~中文强\n优化");
        assert!(v.terms.contains(&"中文强".to_string()));
        assert!(v.terms.contains(&"优化".to_string()));
    }

    #[test]
    fn pinyin_correction_fixes_homophone_variants() {
        let v = Vocab::parse("~中文强");
        // 一条规则纠掉所有同音变体（墙/枪…拼音同为 qiang）
        assert_eq!(v.apply_pinyin_corrections("我觉得中文墙很重要"), "我觉得中文强很重要");
        assert_eq!(v.apply_pinyin_corrections("中文枪"), "中文强");
    }

    #[test]
    fn pinyin_correction_fixes_multichar_term() {
        let v = Vocab::parse("~列要点");
        assert_eq!(v.apply_pinyin_corrections("帮我猎药点"), "帮我列要点");
    }

    #[test]
    fn pinyin_correction_noop_when_already_correct() {
        let v = Vocab::parse("~中文强");
        assert_eq!(v.apply_pinyin_corrections("中文强"), "中文强");
    }

    #[test]
    fn pinyin_correction_only_marked_terms_participate() {
        // 关键防误伤：~中文强 参与拼音；"优化"仅软词、不参与
        // → "中文墙"被纠、同音的"油画"不被误伤成"优化"
        let v = Vocab::parse("~中文强\n优化\n流式");
        assert_eq!(
            v.apply_pinyin_corrections("中文墙上挂着一幅油画"),
            "中文强上挂着一幅油画"
        );
        // "流式"是普通软词，不参与 → "流逝"(liu shi 同音)不被误伤
        assert_eq!(v.apply_pinyin_corrections("时光流逝得真快"), "时光流逝得真快");
    }

    #[test]
    fn pinyin_correction_noop_without_marked_terms() {
        // 没有任何 ~词 → 全程 no-op（不会因软词/硬替换而误碰）
        let v = Vocab::parse("优化\n停路 => 停录");
        assert_eq!(v.apply_pinyin_corrections("中文墙"), "中文墙");
    }

    #[test]
    fn pinyin_correction_skips_latin_and_unrelated() {
        let v = Vocab::parse("~中文强");
        // 英文 / 数字 / 无关汉字原样
        assert_eq!(v.apply_pinyin_corrections("用 API 调用 3 次"), "用 API 调用 3 次");
        assert_eq!(v.apply_pinyin_corrections("今天天气不错"), "今天天气不错");
    }

    #[test]
    fn pinyin_correction_longest_term_wins() {
        // 同时有 2 字与 4 字纠错词时，长词优先：整体匹配"读写分离"，而非先吃掉"读写"
        let v = Vocab::parse("~读写\n~读写分离");
        assert_eq!(v.apply_pinyin_corrections("做好堵谢分梨"), "做好读写分离");
    }

    #[test]
    fn pinyin_correction_skips_single_char_terms() {
        // 单字词太易碰撞，不参与拼音匹配（term_pinyin 要求 ≥2 字）
        let v = Vocab::parse("~强");
        assert_eq!(v.apply_pinyin_corrections("墙"), "墙");
    }

    // —— VocabData（结构化视图：词典 UI 读写）——

    #[test]
    fn vocab_data_parse_buckets_three_kinds() {
        let d = VocabData::parse("优化\n~中文强\n停路 => 停录\n# 注释\n\n");
        assert_eq!(d.soft_terms, vec!["优化"]);
        assert_eq!(d.pinyin_terms, vec!["中文强"]);
        assert_eq!(d.replacements, vec![Replacement { wrong: "停路".into(), right: "停录".into() }]);
    }

    #[test]
    fn vocab_data_roundtrips_through_file_string() {
        // 写出去再读回来应完全一致（保证 UI 保存不丢词、不变形）
        let d = VocabData {
            soft_terms: vec!["优化".into(), "流式".into()],
            pinyin_terms: vec!["中文强".into(), "列要点".into()],
            replacements: vec![Replacement { wrong: "停路".into(), right: "停录".into() }],
        };
        assert_eq!(VocabData::parse(&d.to_file_string()), d);
    }

    #[test]
    fn from_data_unions_all_kinds_into_terms_and_keeps_pinyin() {
        let d = VocabData {
            soft_terms: vec!["优化".into()],
            pinyin_terms: vec!["中文强".into()],
            replacements: vec![Replacement { wrong: "停路".into(), right: "停录".into() }],
        };
        let v = Vocab::from_data(&d);
        // 三类都进 <词表> 供 LLM 参考
        for t in ["优化", "中文强", "停录"] {
            assert!(v.terms.contains(&t.to_string()), "<词表> 缺少 {t}");
        }
        // 但拼音纠错仍只认 ~ 词，且硬替换照常
        assert_eq!(v.apply_pinyin_corrections("中文墙"), "中文强");
        assert_eq!(v.apply_replacements("停路"), "停录");
    }
}
