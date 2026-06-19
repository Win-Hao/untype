use std::path::Path;
use std::process::Command;

fn main() {
    ensure_models();
    tauri_build::build()
}

/// 本地 ASR 模型不入库（GB 级）。缺失时自动调用 scripts/fetch-models.sh 下载（幂等）。
/// build.rs 的 CWD = src-tauri/，故模型路径相对它、脚本在 ../scripts。
/// CI 用占位空文件 → 检测到「存在」即跳过；可设 UNTYPE_SKIP_MODEL_DOWNLOAD=1 自备模型。
fn ensure_models() {
    if Path::new("models/sense-voice/model.int8.onnx").exists()
        || std::env::var_os("UNTYPE_SKIP_MODEL_DOWNLOAD").is_some()
    {
        return;
    }
    let script = Path::new("../scripts/fetch-models.sh");
    if !script.exists() {
        return; // 无脚本则不拦构建，交给 tauri_build 的 resource 校验去报缺失
    }
    println!(
        "cargo:warning=本地 ASR 模型缺失，自动下载中（scripts/fetch-models.sh，约 228MB，首次较慢）…"
    );
    match Command::new("bash").arg(script).status() {
        Ok(s) if s.success() => {}
        Ok(s) => panic!(
            "模型下载失败（退出码 {:?}）。可手动 `bash scripts/fetch-models.sh`，或设 UNTYPE_SKIP_MODEL_DOWNLOAD=1 自备模型。",
            s.code()
        ),
        Err(e) => panic!("无法执行 scripts/fetch-models.sh（{e}）。需 bash + curl，或手动准备模型。"),
    }
}
