//! 自学（步骤③）：从 LLM 的同音纠正里自动攒「正确词」，越用越准。
//!
//! 出稿后比对「LLM 输入（ASR 经词典纠正后的稿）vs LLM 输出（润色稿）」，用拼音对齐找出
//! LLM 改掉的「读音相同、字面不同」的连续汉字片段（同音纠正），按 (误识 → 正确) 计数持久化
//! 到 app 配置目录 `learned.json`。计数达阈值即作为「建议」浮到词典 UI，由用户一键加入或忽略——
//! **不自动改纠错引擎**（避免学到噪声直接误伤；保持用户可控，与 ② 同理）。
//!
//! 边界（说清，别当银弹）：只能学「LLM 纠对过的短片段」（2..=MAX_LEN 字、且整段只发生了同音替换）；
//! 长句里嵌入的纠正、或 LLM 从没纠对的词，学不到 → 仍靠 ② 词典 UI 手动加。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 学习片段的最大汉字数：更长的连续同音片段多半是整句、过于含糊（无法在无分词下切出词），不学。
const MAX_LEN: usize = 5;
/// 同一 (误识 → 正确) 对累计出现达此次数，即作为建议浮现（仅建议，不自动应用）。
const SUGGEST_THRESHOLD: u32 = 2;
/// 输入过长时放弃学习，避免 O(n*m) 对齐在异常长文本上的开销（正常听写远小于此）。
const MAX_INPUT_CHARS: usize = 1000;

/// 一条学到的同音替换对及其累计次数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnedPair {
    pub wrong: String,
    pub right: String,
    pub count: u32,
}

/// 自学持久化存储（`learned.json`）。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LearnStore {
    #[serde(default)]
    pub pairs: Vec<LearnedPair>,
    /// 用户已忽略的对（key = "误识\u{1}正确"），不再建议。
    #[serde(default)]
    pub dismissed: Vec<String>,
}

fn dismiss_key(wrong: &str, right: &str) -> String {
    format!("{wrong}\u{1}{right}")
}

impl LearnStore {
    /// 累计记录一批同音对（同对则次数 +1）。
    pub fn record_pairs(&mut self, pairs: &[(String, String)]) {
        for (wrong, right) in pairs {
            match self
                .pairs
                .iter_mut()
                .find(|p| &p.wrong == wrong && &p.right == right)
            {
                Some(p) => p.count = p.count.saturating_add(1),
                None => self.pairs.push(LearnedPair {
                    wrong: wrong.clone(),
                    right: right.clone(),
                    count: 1,
                }),
            }
        }
    }

    /// 达阈值、未被忽略、且「正确词」尚不在词典里的建议，按次数降序。
    /// `existing` 为词典里已有的正确写法集合（软词 / 拼音纠错词 / 硬替换右侧），避免重复建议。
    pub fn suggestions(&self, existing: &[String]) -> Vec<LearnedPair> {
        let mut v: Vec<LearnedPair> = self
            .pairs
            .iter()
            .filter(|p| p.count >= SUGGEST_THRESHOLD)
            .filter(|p| {
                let k = dismiss_key(&p.wrong, &p.right);
                !self.dismissed.contains(&k)
            })
            .filter(|p| !existing.iter().any(|e| e == &p.right))
            .cloned()
            .collect();
        // 次数降序；同次数按正确词稳定排列，便于测试与展示
        v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.right.cmp(&b.right)));
        v
    }

    /// 忽略某建议（持久化，不再浮现）。
    pub fn dismiss(&mut self, wrong: &str, right: &str) {
        let k = dismiss_key(wrong, right);
        if !self.dismissed.contains(&k) {
            self.dismissed.push(k);
        }
    }

    /// 接受某「正确词」后，把相关对从待建议库移除（按正确词，多个误识都归并到它）。
    pub fn remove_by_right(&mut self, right: &str) {
        self.pairs.retain(|p| p.right != right);
    }
}

/// 取单个汉字的无声调拼音；非汉字返回 None。
fn syllable(c: char) -> Option<&'static str> {
    use pinyin::ToPinyin;
    c.to_pinyin().map(|p| p.plain())
}

/// 两个汉字读音是否相同（无声调）；任一非汉字则为否。
fn same_pinyin(b: char, a: char) -> bool {
    matches!((syllable(b), syllable(a)), (Some(x), Some(y)) if x == y)
}

/// 字符级对齐：先 LCS 回溯，再把相邻的「删/插」按位配成替换列。
/// 返回列序列：每列为 (Option<前文字符>, Option<后文字符>)——
/// 匹配列两侧相同，替换列两侧不同，纯删/插列仅一侧有值。
fn align(b: &[char], a: &[char]) -> Vec<(Option<char>, Option<char>)> {
    let (n, m) = (b.len(), a.len());
    // dp[i][j] = LCS(b[i..], a[j..])
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if b[i] == a[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    // 回溯生成删/插/匹配列
    let mut raw: Vec<(Option<char>, Option<char>)> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if b[i] == a[j] {
            raw.push((Some(b[i]), Some(a[j])));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            raw.push((Some(b[i]), None));
            i += 1;
        } else {
            raw.push((None, Some(a[j])));
            j += 1;
        }
    }
    while i < n {
        raw.push((Some(b[i]), None));
        i += 1;
    }
    while j < m {
        raw.push((None, Some(a[j])));
        j += 1;
    }
    merge_subs(raw)
}

/// 把相邻的「删块 + 插块」按位配成替换列（min(删,插) 个 (Some,Some)），多出的保留为删/插。
fn merge_subs(cols: Vec<(Option<char>, Option<char>)>) -> Vec<(Option<char>, Option<char>)> {
    fn flush(
        out: &mut Vec<(Option<char>, Option<char>)>,
        dels: &mut Vec<char>,
        inss: &mut Vec<char>,
    ) {
        let k = dels.len().min(inss.len());
        for t in 0..k {
            out.push((Some(dels[t]), Some(inss[t])));
        }
        for &d in &dels[k..] {
            out.push((Some(d), None));
        }
        for &s in &inss[k..] {
            out.push((None, Some(s)));
        }
        dels.clear();
        inss.clear();
    }

    let mut out = Vec::with_capacity(cols.len());
    let mut dels: Vec<char> = Vec::new();
    let mut inss: Vec<char> = Vec::new();
    for c in cols {
        match c {
            (Some(x), None) => dels.push(x),
            (None, Some(y)) => inss.push(y),
            (Some(x), Some(y)) => {
                flush(&mut out, &mut dels, &mut inss);
                out.push((Some(x), Some(y)));
            }
            (None, None) => {}
        }
    }
    flush(&mut out, &mut dels, &mut inss);
    out
}

/// 该列是否为「两侧都是汉字、且读音相同」的列（匹配列天然满足；同音替换列也满足）。
fn is_homophone_han_col(col: &(Option<char>, Option<char>)) -> bool {
    matches!(col, (Some(b), Some(a)) if same_pinyin(*b, *a))
}

/// 提取 LLM 改掉的同音片段对 (误识, 正确)。
///
/// 做法：对齐 `before`(LLM 输入) 与 `after`(LLM 输出)，找「极大的：每列都是同音汉字、
/// 且含 ≥1 处字面不同」的列段——即一段里 LLM 只做了同音替换、没有增删/换词。
/// 仅当该段长度在 2..=MAX_LEN 时学习（太短无意义、太长视为整句过于含糊）。同一次出稿内去重。
pub fn extract_homophone_pairs(before: &str, after: &str) -> Vec<(String, String)> {
    if before.is_empty() || after.is_empty() {
        return Vec::new();
    }
    let b: Vec<char> = before.chars().collect();
    let a: Vec<char> = after.chars().collect();
    if b.len() > MAX_INPUT_CHARS || a.len() > MAX_INPUT_CHARS {
        return Vec::new();
    }
    let cols = align(&b, &a);
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        if !is_homophone_han_col(&cols[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut has_diff = false;
        while i < cols.len() && is_homophone_han_col(&cols[i]) {
            if cols[i].0 != cols[i].1 {
                has_diff = true;
            }
            i += 1;
        }
        let len = i - start;
        if has_diff && (2..=MAX_LEN).contains(&len) {
            let wrong: String = cols[start..i].iter().filter_map(|c| c.0).collect();
            let right: String = cols[start..i].iter().filter_map(|c| c.1).collect();
            if !pairs.iter().any(|(w, r)| *w == wrong && *r == right) {
                pairs.push((wrong, right));
            }
        }
    }
    pairs
}

// ---- 持久化（app 配置目录 learned.json）----

fn store_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("learned.json"))
}

/// 读取自学库；缺失 / 解析失败返回空。
pub fn load(app: &AppHandle) -> LearnStore {
    let Some(path) = store_path(app) else {
        return LearnStore::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => LearnStore::default(),
    }
}

/// 保存自学库。
pub fn save(app: &AppHandle, store: &LearnStore) -> Result<(), String> {
    let path = store_path(app).ok_or("无法定位配置目录")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    // 含从你口述里学到的纠错词片段（派生 PII），权限收紧到 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_learns_short_homophone_word() {
        // 单字差（墙→强）嵌在 中文 里：整段只发生同音替换 → 学到「中文强」
        assert_eq!(
            extract_homophone_pairs("中文墙", "中文强"),
            vec![("中文墙".to_string(), "中文强".to_string())]
        );
    }

    #[test]
    fn extract_learns_multichar_diff() {
        assert_eq!(
            extract_homophone_pairs("猎药点", "列要点"),
            vec![("猎药点".to_string(), "列要点".to_string())]
        );
    }

    #[test]
    fn extract_learns_amid_filler_edits() {
        // LLM 同时删了口水词「嗯那个」「啊」，同音片段仍被对齐出来
        assert_eq!(
            extract_homophone_pairs("嗯那个中文墙啊", "中文强"),
            vec![("中文墙".to_string(), "中文强".to_string())]
        );
    }

    #[test]
    fn extract_ignores_non_homophone_edits() {
        // 「不错」→「很好」既非同音、长度也不等 → 不学
        assert!(extract_homophone_pairs("今天不错", "今天很好").is_empty());
    }

    #[test]
    fn extract_skips_too_long_runs() {
        // 6 字连续同音片段（馆/管 同音 guan）超过 MAX_LEN=5 → 视为整句，不学
        assert!(extract_homophone_pairs("我想去图书馆", "我想去图书管").is_empty());
    }

    #[test]
    fn extract_skips_single_char_runs() {
        // 仅 1 字的同音段（强/墙）不足 2 字 → 不学
        assert!(extract_homophone_pairs("强", "墙").is_empty());
    }

    #[test]
    fn extract_dedups_within_one_utterance() {
        // 同一次出稿里同对出现两次（被非汉字逗号分隔成两段），只记一份
        // （阈值语义为「≥2 次不同出稿」，不该被单次内重复刷满）
        assert_eq!(
            extract_homophone_pairs("中文墙，中文墙", "中文强，中文强"),
            vec![("中文墙".to_string(), "中文强".to_string())]
        );
    }

    #[test]
    fn record_and_suggest_respects_threshold() {
        let mut s = LearnStore::default();
        let pair = vec![("中文墙".to_string(), "中文强".to_string())];
        s.record_pairs(&pair);
        assert!(s.suggestions(&[]).is_empty(), "出现 1 次不应建议");
        s.record_pairs(&pair);
        let sug = s.suggestions(&[]);
        assert_eq!(sug.len(), 1);
        assert_eq!(sug[0].right, "中文强");
        assert_eq!(sug[0].count, 2);
    }

    #[test]
    fn suggestions_filter_existing_and_dismissed() {
        let mut s = LearnStore::default();
        let pair = vec![("中文墙".to_string(), "中文强".to_string())];
        s.record_pairs(&pair);
        s.record_pairs(&pair);
        // 已在词典里 → 不再建议
        assert!(s.suggestions(&["中文强".to_string()]).is_empty());
        // 被忽略 → 不再建议
        s.dismiss("中文墙", "中文强");
        assert!(s.suggestions(&[]).is_empty());
    }

    #[test]
    fn remove_by_right_clears_pair() {
        let mut s = LearnStore::default();
        s.record_pairs(&[("中文墙".to_string(), "中文强".to_string())]);
        s.remove_by_right("中文强");
        assert!(s.pairs.is_empty());
    }
}
