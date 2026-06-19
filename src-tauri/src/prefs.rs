//! 应用偏好（首次引导标志等），持久化到应用配置目录 prefs.json。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 长按模式默认热键（用户未设时）。免按模式默认不绑（空）。
pub const DEFAULT_HOLD_SHORTCUT: &str = "CommandOrControl+Shift+D";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Prefs {
    /// 是否已完成首次引导。
    #[serde(default)]
    pub onboarded: bool,
    /// 长按模式热键（按住说话）。None=未设(用默认/迁移旧值)，Some("")=用户清空(禁用)。
    #[serde(default)]
    pub hold_shortcut: Option<String>,
    /// 免按模式热键（单击切换）。None=未设(默认不绑)，Some("")=禁用。
    #[serde(default)]
    pub toggle_shortcut: Option<String>,
    /// 旧版单热键字段，仅用于平滑迁移（已弃用，不再写入）。
    #[serde(default)]
    pub shortcut: Option<String>,
    /// 润色风格："default" | "bullets" | "email" | "raw"（None = default）。
    #[serde(default)]
    pub polish_style: Option<String>,
    /// 麦克风设备名（None / 空 = 系统默认输入设备）。
    #[serde(default)]
    pub microphone: Option<String>,
    /// ASR 引擎："local"(默认, 本地 SenseVoice) | "cloud"(火山 / 阿里)。
    #[serde(default)]
    pub asr_engine: Option<String>,
    /// 火山豆包 ASR 的 API Key（云端引擎用，BYOK）。
    #[serde(default)]
    pub volc_api_key: Option<String>,
    /// 火山资源 ID（None / 空 = 默认 2.0 小时版 volc.seedasr.sauc.duration）。
    #[serde(default)]
    pub volc_resource_id: Option<String>,
    /// 云端厂商："volc"(火山, 默认) | "ali"(阿里 DashScope)。
    #[serde(default)]
    pub cloud_vendor: Option<String>,
    /// 阿里 DashScope API Key（云端=阿里时用，BYOK）。
    #[serde(default)]
    pub ali_api_key: Option<String>,
    /// 阿里模型："qwen3"(默认, Qwen3-ASR-Flash 更强) | "funasr"(Fun-ASR 实时)。
    #[serde(default)]
    pub ali_model: Option<String>,
}

/// 长按模式生效热键：用户设过就用（含主动清空 ""）；没设过则迁移旧单热键，再回退默认。
pub fn effective_hold(p: &Prefs) -> String {
    match &p.hold_shortcut {
        Some(s) => s.trim().to_string(),
        None => p
            .shortcut
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_HOLD_SHORTCUT.to_string()),
    }
}

/// 免按模式生效热键：用户设过就用；没设过则默认不绑（空串 = 不注册）。
pub fn effective_toggle(p: &Prefs) -> String {
    match &p.toggle_shortcut {
        Some(s) => s.trim().to_string(),
        None => String::new(),
    }
}

/// 生效的润色风格（None / 空白 → "default"）。
pub fn effective_style(p: &Prefs) -> String {
    p.polish_style
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "default".to_string())
}

/// 生效的 ASR 引擎（None / 空白 → "local"）。
pub fn effective_asr_engine(p: &Prefs) -> String {
    p.asr_engine
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/// 生效的火山资源 ID（None / 空白 → 默认 2.0 小时版）。
pub fn effective_volc_resource_id(p: &Prefs) -> String {
    p.volc_resource_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| crate::cloud_asr::DEFAULT_RESOURCE_ID.to_string())
}

/// 生效的云端厂商（None / 空白 → "volc"）。
pub fn effective_cloud_vendor(p: &Prefs) -> String {
    p.cloud_vendor
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "volc".to_string())
}

/// 生效的阿里模型（None / 空白 → "qwen3"）。
pub fn effective_ali_model(p: &Prefs) -> String {
    p.ali_model
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "qwen3".to_string())
}

fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("prefs.json"))
}

/// 读取偏好（无文件 / 解析失败则取默认）。
pub fn load(app: &AppHandle) -> Prefs {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 保存偏好到应用配置目录。
pub fn save(app: &AppHandle, prefs: &Prefs) -> Result<(), String> {
    let p = path(app).ok_or("无法定位应用配置目录")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    let json = serde_json::to_string_pretty(prefs).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&p, json).map_err(|e| format!("写入失败: {e}"))?;
    // 含云端 API Key，权限收紧到 0600（仅本人可读），别留 world-readable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{effective_hold, effective_style, effective_toggle, Prefs};

    #[test]
    fn onboarded_serde_roundtrip() {
        let json = serde_json::to_string(&Prefs {
            onboarded: true,
            ..Default::default()
        })
        .unwrap();
        assert!(serde_json::from_str::<Prefs>(&json).unwrap().onboarded);
    }

    #[test]
    fn missing_fields_default() {
        let p: Prefs = serde_json::from_str("{}").unwrap();
        assert!(!p.onboarded);
        assert!(p.hold_shortcut.is_none());
        assert!(p.toggle_shortcut.is_none());
    }

    #[test]
    fn shortcuts_serde_roundtrip() {
        let p = Prefs {
            onboarded: true,
            hold_shortcut: Some("Alt+Space".into()),
            toggle_shortcut: Some("Control+Shift+K".into()),
            ..Default::default()
        };
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert!(back.onboarded);
        assert_eq!(back.hold_shortcut.as_deref(), Some("Alt+Space"));
        assert_eq!(back.toggle_shortcut.as_deref(), Some("Control+Shift+K"));
    }

    #[test]
    fn hold_defaults_then_migrates_then_respects_clear() {
        // 全新：长按回退默认
        assert_eq!(effective_hold(&Prefs::default()), "CommandOrControl+Shift+D");
        // 迁移旧单热键字段
        let migrated = Prefs {
            shortcut: Some("Alt+Space".into()),
            ..Default::default()
        };
        assert_eq!(effective_hold(&migrated), "Alt+Space");
        // 用户主动清空 → 禁用（空串），不回退默认
        let cleared = Prefs {
            hold_shortcut: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(effective_hold(&cleared), "");
        // 用户自定义优先于旧字段
        let custom = Prefs {
            hold_shortcut: Some("Control+Alt+D".into()),
            shortcut: Some("Alt+Space".into()),
            ..Default::default()
        };
        assert_eq!(effective_hold(&custom), "Control+Alt+D");
    }

    #[test]
    fn toggle_unbound_unless_set() {
        assert_eq!(effective_toggle(&Prefs::default()), "");
        let set = Prefs {
            toggle_shortcut: Some("Alt+Space".into()),
            ..Default::default()
        };
        assert_eq!(effective_toggle(&set), "Alt+Space");
        let cleared = Prefs {
            toggle_shortcut: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(effective_toggle(&cleared), "");
    }

    #[test]
    fn style_defaults_to_default() {
        assert_eq!(effective_style(&Prefs::default()), "default");
        let b = Prefs {
            polish_style: Some("bullets".into()),
            ..Default::default()
        };
        assert_eq!(effective_style(&b), "bullets");
    }
}
