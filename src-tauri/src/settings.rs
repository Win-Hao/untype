//! LLM 设置的持久化（BYOK）：保存到应用配置目录的 llm.json，供设置页读写、pipeline 读取。
//! 优先级：UI 保存的设置 > 环境变量（见 crate::llm_config）。无文件即视为未配置。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct LlmSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 关闭模型深度思考；默认 true（语音输入要及时性）。
    #[serde(default = "default_disable_thinking")]
    pub disable_thinking: bool,
    /// 每家供应商各自的 Key：base_url → api_key（切换带回、各自持久化）。
    #[serde(default)]
    pub keys: std::collections::HashMap<String, String>,
}

/// disable_thinking 缺省为 true：语音输入默认不让模型思考、保证及时。
fn default_disable_thinking() -> bool {
    true
}

fn config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("llm.json"))
}

/// 读取已保存的 LLM 设置（无文件 / 解析失败则 None）。
pub fn load(app: &AppHandle) -> Option<LlmSettings> {
    let content = std::fs::read_to_string(config_path(app)?).ok()?;
    serde_json::from_str(&content).ok()
}

/// 保存 LLM 设置到应用配置目录。
pub fn save(app: &AppHandle, settings: &LlmSettings) -> Result<(), String> {
    let path = config_path(app).ok_or("无法定位应用配置目录")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入设置失败: {e}"))?;
    // llm.json 含 API Key，权限收紧到 0600（仅本人可读）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::LlmSettings;

    #[test]
    fn serde_roundtrip() {
        let s = LlmSettings {
            base_url: "https://x/v1".into(),
            api_key: "k".into(),
            model: "m".into(),
            disable_thinking: true,
            keys: Default::default(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: LlmSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base_url, "https://x/v1");
        assert_eq!(back.model, "m");
        assert!(back.disable_thinking);
    }

    #[test]
    fn disable_thinking_defaults_true_when_missing() {
        // 缺字段时默认开启「关思考」（语音输入要及时性）
        let s: LlmSettings =
            serde_json::from_str(r#"{"base_url":"u","api_key":"k","model":"m"}"#).unwrap();
        assert!(s.disable_thinking);
    }
}
