//! LLM 润色层：把 ASR 原始文本经聊天大模型清理——纠正同音字、去口水词、
//! 必要时分点排版。走 OpenAI 兼容接口（BYOK），厂商无关；配置优先用 UI 存的 `llm.json`，
//! 仅开发期回退环境变量（见 LlmConfig::from_env）；未配置 key 时上层跳过润色、直接用原始识别文本。

use serde::Deserialize;

/// 系统提示词。放在独立的 system_prompt.txt 便于维护（改它会触发重新编译）。
const SYSTEM_PROMPT: &str = include_str!("system_prompt.txt");

pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 关闭推理模型（智谱 GLM-4.5/4.6 等）的"深度思考"：28s → ~3s，且仍能纠错。
    pub disable_thinking: bool,
}

impl LlmConfig {
    /// 从环境变量读取配置；未设置 LLM_API_KEY 时返回 None（表示跳过润色）。
    /// **仅开发期生效**：dev 里 `set -a; . src-tauri/.env` 会把变量注入进程；打包 / 双击启动的
    /// app 不加载 .env（无 dotenvy），故生产配置一律走 UI 存的 `llm.json`，此分支返回 None。
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("LLM_API_KEY").ok().filter(|k| !k.is_empty())?;
        Some(Self {
            // 默认对齐当前推荐：智谱 GLM 免费档（glm-4-flash），不再默认 DeepSeek。
            base_url: std::env::var("LLM_BASE_URL")
                .unwrap_or_else(|_| "https://open.bigmodel.cn/api/paas/v4".to_string()),
            api_key,
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "glm-4-flash".to_string()),
            // 默认开启「关思考」：语音输入要及时性，默认不让模型深度思考。
            // 显式设 LLM_DISABLE_THINKING=0 / false 才恢复思考。
            disable_thinking: std::env::var("LLM_DISABLE_THINKING")
                .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
                .unwrap_or(true),
        })
    }
}

/// 生成无声调拼音提示串：汉字→拼音音节（空格分隔），英文/数字原样分组保留，标点忽略。
/// 作为 <拼音> 块随 <转写> 一起发给 LLM，帮助它判断同音/近音字错误（PY-GEC 思路），
/// 从而泛化纠正提示词术语表里没列出的同音词。
fn pinyin_hint(text: &str) -> String {
    use pinyin::ToPinyin;
    let mut tokens: Vec<String> = Vec::new();
    let mut latin = String::new();
    for (ch, py) in text.chars().zip(text.to_pinyin()) {
        if let Some(p) = py {
            if !latin.is_empty() {
                tokens.push(std::mem::take(&mut latin));
            }
            tokens.push(p.plain().to_string());
        } else if ch.is_alphanumeric() {
            latin.push(ch); // 英文/数字累积成一组
        } else if !latin.is_empty() {
            tokens.push(std::mem::take(&mut latin)); // 标点/空白：冲掉缓冲
        }
    }
    if !latin.is_empty() {
        tokens.push(latin);
    }
    tokens.join(" ")
}

/// 不同润色风格追加到 user 消息末尾的格式指令（default 用 system_prompt 既有规则、不追加）。
fn style_instruction(style: &str) -> &'static str {
    match style {
        "bullets" => "\n\n【输出格式】整理成简洁的要点列表：每个要点单独一行、以「- 」开头，只保留关键信息，不要前言后语。",
        "email" => "\n\n【输出格式】整理成一封礼貌、得体、条理清晰的邮件正文；不要写主题行、称呼或落款占位符，只给正文。",
        _ => "",
    }
}

/// 构造 OpenAI 兼容的 chat completions 请求体。
/// `terms` 非空时附 <词表>（用户词典里的术语正确写法），供 LLM 纠错 / 保留术语。
/// `style` 决定输出格式（见 style_instruction）。
fn build_request_body(
    model: &str,
    text: &str,
    terms: &[String],
    style: &str,
    disable_thinking: bool,
) -> serde_json::Value {
    let mut user = format!("<转写>\n{text}\n</转写>");
    let pinyin = pinyin_hint(text);
    if !pinyin.is_empty() {
        // 附转写拼音，帮 LLM 纠正未列入词表的同音/近音字
        user.push_str(&format!("\n<拼音>\n{pinyin}\n</拼音>"));
    }
    if !terms.is_empty() {
        // 附用户词典：遇同音/近音词优先采用这些写法、并保留术语原样
        user.push_str(&format!("\n<词表>\n{}\n</词表>", terms.join("、")));
    }
    user.push_str(style_instruction(style));
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user }
        ],
        "temperature": 0.1,
        "stream": false
    });
    if disable_thinking {
        // 关掉推理模型的深度思考以提速；非推理模型会忽略此字段。
        body["thinking"] = serde_json::json!({ "type": "disabled" });
    }
    body
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

/// 从响应 JSON 提取整理后的文本。
fn parse_response(json: &serde_json::Value) -> Result<String, String> {
    let resp: ChatResponse =
        serde_json::from_value(json.clone()).map_err(|e| format!("解析 LLM 响应失败: {e}"))?;
    resp.choices
        .into_iter()
        .next()
        .map(|c| c.message.content.trim().to_string())
        .ok_or_else(|| "LLM 响应没有 choices".to_string())
}

/// 调用 LLM 润色文本。失败时上层应回退到原始识别文本。
pub fn polish(
    config: &LlmConfig,
    text: &str,
    terms: &[String],
    style: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let body = build_request_body(&config.model, text, terms, style, config.disable_thinking);

    // 设连接/读超时：ureq 默认读超时无限，云端 LLM 卡住会让润色（胶囊 thinking）永久挂起。
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();
    let resp = agent
        .post(&url)
        .set("Authorization", &format!("Bearer {}", config.api_key))
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Status(code, resp) => {
                let detail = resp.into_string().unwrap_or_default();
                format!("LLM 接口返回 {code}: {detail}")
            }
            other => format!("LLM 请求失败: {other}"),
        })?;

    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("读取 LLM 响应失败: {e}"))?;
    parse_response(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_system_and_user_with_text() {
        let body = build_request_body("deepseek-chat", "今天天气不错", &[], "default", false);
        assert_eq!(body["model"], "deepseek-chat");
        assert_eq!(body["stream"], false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        let user = msgs[1]["content"].as_str().unwrap();
        assert!(user.contains("今天天气不错"));
        assert!(user.contains("<拼音>"), "user 消息应附带拼音块");
        assert!(!user.contains("<词表>"), "terms 为空时不应有词表块");
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn disable_thinking_adds_field() {
        let body = build_request_body("glm-4.6", "x", &[], "default", true);
        assert_eq!(body["thinking"]["type"], "disabled");
    }

    #[test]
    fn request_body_includes_vocab_block_when_terms_present() {
        let terms = vec!["流式".to_string(), "停录".to_string()];
        let body = build_request_body("glm-4.6", "测试", &terms, "default", false);
        let user = body["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("<词表>"), "terms 非空时应附词表块");
        assert!(user.contains("流式") && user.contains("停录"));
    }

    #[test]
    fn style_appends_format_instruction() {
        let b = build_request_body("m", "x", &[], "bullets", false);
        assert!(b["messages"][1]["content"].as_str().unwrap().contains("要点"));
        let d = build_request_body("m", "x", &[], "default", false);
        assert!(!d["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("【输出格式】"));
    }

    #[test]
    fn pinyin_hint_converts_hanzi_keeps_latin() {
        assert_eq!(pinyin_hint("流式"), "liu shi");
        assert_eq!(pinyin_hint("做油画"), "zuo you hua");
        // 英文/数字成组保留，标点忽略
        assert_eq!(pinyin_hint("用API，对吧"), "yong API dui ba");
        assert_eq!(pinyin_hint(""), "");
    }

    #[test]
    fn system_prompt_is_loaded() {
        assert!(SYSTEM_PROMPT.contains("语音听写"));
        assert!(SYSTEM_PROMPT.contains("编号"));
    }

    #[test]
    fn parse_response_extracts_and_trims_content() {
        let json = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "  今天天气不错。  " } }]
        });
        assert_eq!(parse_response(&json).unwrap(), "今天天气不错。");
    }

    #[test]
    fn parse_response_errors_on_empty_choices() {
        let json = serde_json::json!({ "choices": [] });
        assert!(parse_response(&json).is_err());
    }

    #[test]
    #[ignore = "需要 LLM_API_KEY 环境变量"]
    fn polish_real_api() {
        let config = LlmConfig::from_env().expect("请先设置 LLM_API_KEY");
        // 含口水词 + 已知同音字误识（流逝→流式、停路→停录、油画→优化），
        // 作为润色提示词的回归测试：验证这三类术语同音纠正生效。
        let raw = "嗯这个接口那个就是说支持流逝传输的然后呢记得点一下停路最后对代码做一下油画";
        let vocab = crate::vocab::Vocab::load();
        let out = polish(&config, raw, &vocab.terms, "default").expect("LLM 调用失败");
        eprintln!("【原文】{raw}");
        eprintln!("【润色】{out}");
        assert!(!out.is_empty(), "润色结果不应为空");
        // 同音字应被纠正为正确术语
        assert!(out.contains("流式"), "应把『流逝』纠正为『流式』，实际: {out}");
        assert!(out.contains("停录"), "应把『停路』纠正为『停录』，实际: {out}");
        assert!(out.contains("优化"), "应把『油画』纠正为『优化』，实际: {out}");
        // 错误写法不应残留
        assert!(!out.contains("流逝"), "不应残留『流逝』，实际: {out}");
        assert!(!out.contains("停路"), "不应残留『停路』，实际: {out}");
        assert!(!out.contains("油画"), "不应残留『油画』，实际: {out}");
    }

    #[test]
    #[ignore = "需要 LLM_API_KEY 环境变量"]
    fn polish_generalizes_unlisted_homophone_via_pinyin() {
        // 「病发」是「并发」的同音误识，且不在 system_prompt 术语表里——
        // 验证 <拼音> 注入能泛化纠正术语表外的同音词。
        let config = LlmConfig::from_env().expect("请先设置 LLM_API_KEY");
        let raw = "我们这个后端服务要支持高病发然后做读写分离";
        let vocab = crate::vocab::Vocab::load();
        let out = polish(&config, raw, &vocab.terms, "default").expect("LLM 调用失败");
        eprintln!("【原文】{raw}");
        eprintln!("【润色】{out}");
        assert!(out.contains("并发"), "应借助拼音把『病发』纠正为『并发』，实际: {out}");
        assert!(!out.contains("病发"), "不应残留『病发』，实际: {out}");
    }
}
