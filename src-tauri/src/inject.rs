//! 把文本注入到当前光标处。macOS 上通过 enigo 模拟输入 Unicode 文本，
//! 需要「辅助功能」权限（系统设置 → 隐私与安全性 → 辅助功能）。

use enigo::{Enigo, Keyboard, Settings};

/// 将文本输入到当前焦点处。失败通常意味着未授予辅助功能权限。
pub fn inject_text(text: &str) -> Result<(), String> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("初始化输入模拟失败: {e:?}"))?;
    enigo
        .text(text)
        .map_err(|e| format!("注入文本失败（请检查辅助功能权限）: {e:?}"))?;
    Ok(())
}

/// 兜底：把文本写入系统剪贴板（注入失败时调用，避免识别结果丢失，用户可手动 ⌘V 粘贴）。
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("剪贴板初始化失败: {e:?}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("写入剪贴板失败: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;

    /// 执行一段 AppleScript，返回 stdout（已 trim）。
    fn osa(script: &str) -> Result<String, String> {
        let out = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| format!("运行 osascript 失败: {e:?}"))?;
        if !out.status.success() {
            return Err(format!(
                "osascript 出错（可能未授予『自动化』权限）: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// 真·端到端注入测试（仅 macOS）：
    /// 新建一个空 TextEdit 文档 → 注入文本 → 读回文档内容断言命中 → 关闭不保存。
    ///
    /// 标 #[ignore]：需要 GUI + 「辅助功能」权限（给测试二进制）+「自动化」权限（控制 TextEdit）。
    /// 运行：`cargo test --lib injects_text_into_textedit -- --ignored --nocapture`
    /// 首次运行 macOS 可能弹权限框，请点允许后重跑。
    #[test]
    #[ignore = "需要 GUI + 辅助功能/自动化权限；用 `cargo test -- --ignored` 运行"]
    fn injects_text_into_textedit() {
        let marker = "语音听写inject自测7788";

        // 1. 打开并置前一个新的空 TextEdit 文档
        osa(r#"tell application "TextEdit"
                activate
                make new document
            end tell"#)
        .expect("无法打开 TextEdit");
        sleep(Duration::from_millis(1500)); // 等窗口取得焦点

        // 2. 注入到当前焦点（即 TextEdit）
        inject_text(marker).expect("inject_text 返回错误");
        sleep(Duration::from_millis(900)); // 等输入事件落地

        // 3. 读回文档内容
        let content = osa(r#"tell application "TextEdit" to get text of front document"#)
            .unwrap_or_default();

        // 4. 清理：关闭文档不保存
        let _ = osa(r#"tell application "TextEdit" to close front document saving no"#);

        eprintln!("【TextEdit 读回】{content:?}");
        assert!(
            content.contains(marker),
            "注入未生效：TextEdit 里没读到注入文本。\
             最可能原因是未授予测试二进制『辅助功能』权限（系统设置→隐私与安全性→辅助功能）。\
             实际读回内容: {content:?}"
        );
    }

    #[test]
    #[ignore = "会改动系统剪贴板"]
    fn copy_to_clipboard_roundtrip() {
        copy_to_clipboard("剪贴板兜底测试123").expect("写剪贴板失败");
        let mut cb = arboard::Clipboard::new().expect("剪贴板初始化失败");
        assert_eq!(cb.get_text().expect("读剪贴板失败"), "剪贴板兜底测试123");
    }
}
