mod activation;
pub mod asr;
pub mod audio;
pub mod cloud_asr;
mod inject;
mod keytap;
mod learn;
mod llm;
mod pipeline;
mod prefs;
mod recorder;
mod settings;
mod vad;
mod viz;
mod vocab;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use activation::{Activation, Effect, RecordingState, Trigger};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent,
};

/// sherpa 识别器内部是裸指针（!Send）。底层 C 对象可安全跨线程移动，
/// 且我们始终用 Mutex 串行访问，故手动标记 Send。
struct AsrHandle(asr::Asr);
unsafe impl Send for AsrHandle {}

struct AppState {
    activation: Mutex<Activation>,
    recorder: Mutex<recorder::Recorder>,
    asr: Mutex<Option<AsrHandle>>,
    /// 当前生效的长按 / 免按热键加速器（空串=未绑定），供 UI 读取。
    hold_shortcut: Mutex<String>,
    toggle_shortcut: Mutex<String>,
    /// 解析后的绑定；CGEventTap 回调实时读取，改键即时生效。
    bindings: Arc<Mutex<keytap::Bindings>>,
    /// 是否正处于「免按键（双击）开启」的录音；CGEventTap 回调据此决定
    /// 录音中是否拦截回车作「确认」。由键事件处理线程在状态变化时更新。
    toggle_recording: Arc<AtomicBool>,
    /// 选麦弹窗的实时电平监听器（仅弹窗打开时运行）。
    mic_monitor: Mutex<recorder::Monitor>,
    /// 胶囊显隐「代际」：每次 show +1。出场动画是延迟 hide，延迟到点比对此值，
    /// 若期间又开新一轮录音（再次 show 使其变化）就放弃这次 hide，别误收新胶囊。
    capsule_gen: AtomicU64,
}

const TRAY_ID: &str = "main-tray";

/// 模型目录：优先用打包进 app 的资源目录（生产 / 分发）；解析不到时回退源码下的 models/（开发期）。
fn model_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .map(|r| r.join("models/sense-voice"))
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/sense-voice"))
}

/// 加载自定义词典：用户配置目录覆盖 > 打包的默认词典（资源目录）> 环境变量 / 开发期回退。
pub(crate) fn load_vocab(app: &AppHandle) -> vocab::Vocab {
    if let Ok(p) = std::env::var("VOCAB_PATH") {
        return vocab::Vocab::from_path(std::path::Path::new(&p));
    }
    if let Ok(cfg) = app.path().app_config_dir() {
        let user = cfg.join("vocab.txt");
        if user.exists() {
            return vocab::Vocab::from_path(&user);
        }
    }
    if let Ok(res) = app.path().resource_dir() {
        let bundled = res.join("vocab.txt");
        if bundled.exists() {
            return vocab::Vocab::from_path(&bundled);
        }
    }
    vocab::Vocab::load() // 开发期回退（env / CARGO_MANIFEST_DIR）
}

#[cfg(target_os = "macos")]
fn set_dock_visible(app: &AppHandle, visible: bool) {
    let ah = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = ah.set_dock_visibility(visible);
    });
}

/// 显示并聚焦主窗口。关窗后窗口只是被隐藏（app 仍在后台常驻），
/// 点 Dock 图标（RunEvent::Reopen）或点菜单栏托盘图标都唤起这里，把窗口带回前台。
fn show_main_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    set_dock_visible(app, true);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn set_tray(app: &AppHandle, recording: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(if recording { "● 录音中" } else { "" }));
        let _ = tray.set_tooltip(Some(if recording {
            "Untype · 录音中"
        } else {
            "Untype · 空闲"
        }));
    }
}

/// 确保 ASR 模型已加载（懒加载）；可用于录音开始时预热，避免首句卡加载。
pub(crate) fn ensure_asr(app: &AppHandle, model_dir: &Path) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut guard = state.asr.lock().unwrap();
    if guard.is_none() {
        let loaded = asr::Asr::load(
            &model_dir.join("model.int8.onnx"),
            &model_dir.join("tokens.txt"),
        )?;
        *guard = Some(AsrHandle(loaded));
    }
    Ok(())
}

/// 用共享的 ASR 转写一段 16k 单声道样本。供 pipeline 调用。
pub(crate) fn transcribe_with_state(
    app: &AppHandle,
    model_dir: &Path,
    samples: &[f32],
) -> Result<String, String> {
    ensure_asr(app, model_dir)?;
    let state = app.state::<AppState>();
    let mut guard = state.asr.lock().unwrap();
    Ok(guard
        .as_mut()
        .unwrap()
        .0
        .transcribe(audio::TARGET_SAMPLE_RATE, samples))
}

/// 胶囊出场动画时长（前端回缩下沉+淡出）。延迟此时长后再真正 hide 窗口，让动画播完。
const CAPSULE_LEAVE_MS: u64 = 210;

/// 显示录音悬浮胶囊（屏幕底部居中）。窗口 focus:false，只 show 不 set_focus，
/// 绝不抢焦点——否则停录后注入会打到胶囊而非用户的目标输入框。
pub(crate) fn show_capsule(app: &AppHandle) {
    if let Some(cap) = app.get_webview_window("capsule") {
        if let (Ok(Some(monitor)), Ok(win)) = (cap.primary_monitor(), cap.outer_size()) {
            let ms = monitor.size();
            let x = (ms.width as i32 - win.width as i32) / 2;
            let bottom_margin = (monitor.scale_factor() * 80.0) as i32;
            let y = ms.height as i32 - win.height as i32 - bottom_margin;
            let _ = cap.set_position(PhysicalPosition::new(x, y));
        }
        // 代际 +1：让延迟 hide 能识别「这之后又开了新胶囊」（见 hide_capsule / capsule_gen）。
        app.state::<AppState>().capsule_gen.fetch_add(1, Ordering::SeqCst);
        let _ = cap.show();
        let _ = app.emit("capsule-show", ()); // 前端播入场浮现（升起+放大+淡入）
    }
}

/// 隐藏录音悬浮胶囊：先让前端播出场动画（回缩下沉+淡出），延迟到动画结束再真正 hide。
/// 延迟期间若用户又开始新一轮录音（show_capsule 使 capsule_gen 变化），就放弃这次 hide——新胶囊该留着。
pub(crate) fn hide_capsule(app: &AppHandle) {
    let _ = app.emit("capsule-hide", ()); // 前端开始播出场动画
    let generation = app.state::<AppState>().capsule_gen.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(CAPSULE_LEAVE_MS));
        // 出场动画期间又开了新一轮录音 → gen 变化 → 那是新胶囊，别收。
        if app.state::<AppState>().capsule_gen.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(cap) = app.get_webview_window("capsule") {
            let _ = cap.hide();
        }
        // hide 之后才归位内容（窗口已不可见）：phase 回录音态、entered 复位，下次显示不闪残留。
        let _ = app.emit("capsule-reset", ());
    });
}

/// 当前是否在录音。供胶囊收尾延迟判断：若停留期间用户已开始新一轮录音，就别把新胶囊收掉。
pub(crate) fn is_recording(app: &AppHandle) -> bool {
    app.state::<AppState>().activation.lock().unwrap().state() == RecordingState::Recording
}

/// 解析当前 LLM 配置：优先用 UI 保存的设置（BYOK），其次（仅开发期）环境变量。
pub(crate) fn llm_config(app: &AppHandle) -> Option<llm::LlmConfig> {
    if let Some(s) = settings::load(app) {
        if !s.api_key.is_empty() && !s.base_url.is_empty() {
            return Some(llm::LlmConfig {
                base_url: s.base_url,
                api_key: s.api_key,
                model: s.model,
                disable_thinking: s.disable_thinking,
            });
        }
    }
    llm::LlmConfig::from_env()
}

fn apply_effect(app: &AppHandle, effect: Effect) {
    match effect {
        Effect::StartRecording => {
            let mic = prefs::load(app).microphone.unwrap_or_default();
            let mic = if mic.trim().is_empty() { None } else { Some(mic) };
            let started = app.state::<AppState>().recorder.lock().unwrap().start(mic);
            match started {
                Ok(handle) => {
                    set_tray(app, true);
                    let _ = app.emit("recording-state", "recording");
                    // 流式处理管线在独立线程跑：边录边识别 emit 字幕，停录后润色+注入。
                    let app2 = app.clone();
                    let dir = model_dir(app);
                    std::thread::spawn(move || pipeline::run(app2, handle, dir));
                    // 预热 ASR：与用户说第一句并行加载模型，避免首句字幕卡在加载上
                    let warm_app = app.clone();
                    let warm_dir = model_dir(app);
                    std::thread::spawn(move || {
                        if let Err(e) = ensure_asr(&warm_app, &warm_dir) {
                            eprintln!("预热 ASR 失败: {e}");
                        }
                    });
                }
                Err(e) => {
                    eprintln!("启动录音失败: {e}");
                    let _ = app.emit("asr-error", e);
                }
            }
        }
        Effect::StopRecording => {
            app.state::<AppState>().recorder.lock().unwrap().stop();
            set_tray(app, false);
            let _ = app.emit("recording-state", "idle");
            // 收尾（flush + 润色 + 注入）由 pipeline 线程在缓冲排空后自行完成。
        }
        Effect::CancelRecording => {
            // 取消：中止录音并丢弃。pipeline 线程读到 cancel 后跳过出稿、静默收起胶囊。
            app.state::<AppState>().recorder.lock().unwrap().cancel();
            set_tray(app, false);
            let _ = app.emit("recording-state", "idle");
        }
        Effect::None => {}
    }
}

#[tauri::command]
fn get_recording(state: tauri::State<AppState>) -> bool {
    state.activation.lock().unwrap().state() == RecordingState::Recording
}

/// 启动 macOS 全局键监听（CGEventTap），把命中的热键事件喂给激活状态机。
/// `bindings` 由 AppState 共享，改键即时生效；事件经 channel 串行处理，回调不阻塞键盘。
fn start_keytap(app: &AppHandle) {
    let (tx, rx) = mpsc::channel::<keytap::KeyAction>();
    let st = app.state::<AppState>();
    let bindings = st.bindings.clone();
    let toggle_recording = st.toggle_recording.clone();
    keytap::spawn(bindings, toggle_recording, tx);

    let handle = app.clone();
    std::thread::spawn(move || {
        while let Ok(action) = rx.recv() {
            let effect = {
                let state = handle.state::<AppState>();
                let mut a = state.activation.lock().unwrap();
                let effect = match action {
                    keytap::KeyAction::Hotkey(trigger, hk) => a.handle(trigger, hk),
                    keytap::KeyAction::Confirm => a.confirm(),
                    keytap::KeyAction::Cancel => a.cancel(),
                };
                // 同步「是否免按键录音中」给键盘回调，决定其是否拦截回车。
                state
                    .toggle_recording
                    .store(a.is_toggle_recording(), Ordering::SeqCst);
                effect
            };
            apply_effect(&handle, effect);
        }
    });
}

/// 返回 (长按键, 免按键) 当前加速器；空串表示该模式未绑定。
#[tauri::command]
fn get_shortcuts(state: tauri::State<AppState>) -> (String, String) {
    (
        state.hold_shortcut.lock().unwrap().clone(),
        state.toggle_shortcut.lock().unwrap().clone(),
    )
}

/// 改键 / 清除某个模式的热键。which: "hold"|"toggle"；accelerator 为空串 = 清除该键。
/// 校验能解析后更新共享绑定（CGEventTap 即时生效）并持久化；非法键则拒绝、旧键不动。
#[tauri::command]
fn set_shortcut(
    app: AppHandle,
    state: tauri::State<AppState>,
    which: String,
    accelerator: String,
) -> Result<(), String> {
    let trigger = match which.as_str() {
        "hold" => Trigger::Hold,
        "toggle" => Trigger::Toggle,
        other => return Err(format!("未知的热键类型: {other}")),
    };
    let accel = accelerator.trim().to_string();
    // 非空必须能解析成合法键，否则拒绝（旧键不动）。
    if !accel.is_empty() && keytap::parse_spec(&accel).is_none() {
        return Err(format!("无法识别的快捷键: {accel}"));
    }
    let spec = keytap::parse_spec(&accel);
    // 更新共享绑定（CGEventTap 回调下次读到即生效，无需重启 tap）。
    {
        let mut b = state.bindings.lock().unwrap();
        match trigger {
            Trigger::Hold => b.hold = spec,
            Trigger::Toggle => b.toggle = spec,
        }
    }
    // 更新供 UI 读取的字符串。
    match trigger {
        Trigger::Hold => *state.hold_shortcut.lock().unwrap() = accel.clone(),
        Trigger::Toggle => *state.toggle_shortcut.lock().unwrap() = accel.clone(),
    }
    // 持久化（load-modify-save：只动对应字段）。
    let mut p = prefs::load(&app);
    match trigger {
        Trigger::Hold => p.hold_shortcut = Some(accel),
        Trigger::Toggle => p.toggle_shortcut = Some(accel),
    }
    prefs::save(&app, &p)
}

#[tauri::command]
fn get_llm_settings(app: AppHandle) -> settings::LlmSettings {
    // UI 保存过的优先；没存过则回退环境变量（仅开发期 source .env 生效，打包后无 .env → 视为未配置）
    if let Some(s) = settings::load(&app) {
        return s;
    }
    if let Some(c) = llm::LlmConfig::from_env() {
        let mut keys = std::collections::HashMap::new();
        if !c.base_url.is_empty() && !c.api_key.is_empty() {
            keys.insert(c.base_url.clone(), c.api_key.clone());
        }
        return settings::LlmSettings {
            base_url: c.base_url,
            api_key: c.api_key,
            model: c.model,
            disable_thinking: c.disable_thinking,
            keys,
        };
    }
    settings::LlmSettings::default()
}

/// 阿里百炼 Base URL（OpenAI 兼容模式），用于「ASR / 润色共用一个阿里 Key」的同步。
const QWEN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

/// 阿里百炼 Key 通用：在 ASR（prefs.ali_api_key）与润色（llm.json 通义栏）间同步，省得填两次。
/// from_asr=true：ASR→润色；false：润色→ASR。
/// 边界：只「带过去」、不「清过去」——传空值直接返回，清空一边不联动清另一边
///（两边是同一把 Key 的两处用途，清掉 ASR 用法时不该顺手抹掉润色配置）。
fn sync_ali_key(app: &AppHandle, key: &str, from_asr: bool) {
    let key = key.trim();
    if key.is_empty() {
        return; // 见上：不联动清除
    }
    if from_asr {
        // 始终把 Key 写进 llm.json 的 keys 映射（即使当前润色不是通义），这样以后切到通义能带回；
        // 仅当当前生效供应商已是通义时，才同时更新生效的 api_key。无 llm.json 则新建——半空无害：
        // llm_config 要求 base_url + api_key 同时非空才采用，否则照常回退、不影响开发期 env。
        let mut s = settings::load(app).unwrap_or_default();
        s.keys.insert(QWEN_BASE_URL.to_string(), key.to_string());
        if s.base_url == QWEN_BASE_URL {
            s.api_key = key.to_string();
        }
        let _ = settings::save(app, &s);
    } else {
        let mut p = prefs::load(app);
        p.ali_api_key = Some(key.to_string());
        let _ = prefs::save(app, &p);
    }
}

#[tauri::command]
fn set_llm_settings(app: AppHandle, cfg: settings::LlmSettings) -> Result<(), String> {
    let is_qwen = cfg.base_url == QWEN_BASE_URL;
    let ali_key = cfg.api_key.clone();
    settings::save(&app, &cfg)?;
    if is_qwen {
        sync_ali_key(&app, &ali_key, false);
    }
    Ok(())
}

/// 词典 UI 读写的活动路径：VOCAB_PATH（开发期覆盖）> 用户配置目录 `vocab.txt`。
/// 与 load_vocab 的「可写层」一致，保证 UI 改完即被下次说话读到。
fn vocab_active_path(app: &AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("VOCAB_PATH") {
        return PathBuf::from(p);
    }
    app.path()
        .app_config_dir()
        .map(|d| d.join("vocab.txt"))
        .unwrap_or_else(|_| PathBuf::from("vocab.txt"))
}

/// 打包默认词典路径（不含用户层）：用户词典尚不存在时作为初始内容来源。
fn default_vocab_path(app: &AppHandle) -> PathBuf {
    if let Ok(res) = app.path().resource_dir() {
        let bundled = res.join("vocab.txt");
        if bundled.exists() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vocab.txt")
}

/// 读取词典结构化数据：用户词典存在则读它，否则读打包默认（首次让用户看到内置词）。
fn read_vocab_data(app: &AppHandle) -> vocab::VocabData {
    let path = vocab_active_path(app);
    if path.exists() {
        vocab::VocabData::from_path(&path)
    } else {
        vocab::VocabData::from_path(&default_vocab_path(app))
    }
}

/// 把词典结构化数据覆盖写到活动路径（用户配置目录）。
fn write_vocab(app: &AppHandle, data: &vocab::VocabData) -> Result<(), String> {
    let path = vocab_active_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {e}"))?;
    }
    std::fs::write(&path, data.to_file_string()).map_err(|e| format!("写入词典失败: {e}"))
}

/// 读取词典（结构化）：保存后即落到用户配置目录。
#[tauri::command]
fn get_vocab(app: AppHandle) -> vocab::VocabData {
    read_vocab_data(&app)
}

/// 保存词典：把完整结构化词典覆盖写到活动路径（UI 持有全量，故覆盖式安全）。下次说话即重载生效。
#[tauri::command]
fn set_vocab(app: AppHandle, data: vocab::VocabData) -> Result<(), String> {
    write_vocab(&app, &data)
}

/// 自学建议：达阈值、未忽略、且正确词尚不在词典里的同音对（按次数降序）。
#[tauri::command]
fn get_learned_suggestions(app: AppHandle) -> Vec<learn::LearnedPair> {
    let data = read_vocab_data(&app);
    let mut existing: Vec<String> = data.pinyin_terms;
    existing.extend(data.soft_terms);
    existing.extend(data.replacements.into_iter().map(|r| r.right));
    learn::load(&app).suggestions(&existing)
}

/// 接受建议：把「正确词」加为拼音纠错词（≥2 字），写回词典，并从自学库移除该建议。
#[tauri::command]
fn accept_learned_suggestion(app: AppHandle, right: String) -> Result<(), String> {
    let mut data = read_vocab_data(&app);
    if !right.trim().is_empty() && !data.pinyin_terms.contains(&right) {
        data.pinyin_terms.push(right.clone());
    }
    write_vocab(&app, &data)?;
    let mut store = learn::load(&app);
    store.remove_by_right(&right);
    learn::save(&app, &store)
}

/// 忽略建议：持久化忽略，不再浮现。
#[tauri::command]
fn dismiss_learned_suggestion(app: AppHandle, wrong: String, right: String) -> Result<(), String> {
    let mut store = learn::load(&app);
    store.dismiss(&wrong, &right);
    learn::save(&app, &store)
}

#[tauri::command]
fn get_polish_style(app: AppHandle) -> String {
    prefs::effective_style(&prefs::load(&app))
}

#[tauri::command]
fn set_polish_style(app: AppHandle, style: String) -> Result<(), String> {
    let mut p = prefs::load(&app);
    p.polish_style = Some(style);
    prefs::save(&app, &p)
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    recorder::list_input_devices()
}

#[tauri::command]
fn get_microphone(app: AppHandle) -> String {
    prefs::load(&app).microphone.unwrap_or_default()
}

#[tauri::command]
fn set_microphone(app: AppHandle, name: String) -> Result<(), String> {
    let mut p = prefs::load(&app);
    p.microphone = Some(name);
    prefs::save(&app, &p)
}

/// 选麦弹窗：开始监听某设备的实时频谱（device 空串 = 系统默认），每 ~60ms emit `mic-level`(N_BANDS 段 0..1)。
#[tauri::command]
fn start_mic_monitor(app: AppHandle, state: tauri::State<AppState>, device: String) {
    let dev = if device.trim().is_empty() {
        None
    } else {
        Some(device)
    };
    let app2 = app.clone();
    state.mic_monitor.lock().unwrap().start(dev, move |bands| {
        let _ = app2.emit("mic-level", bands);
    });
}

/// 选麦弹窗关闭：停止电平监听。
#[tauri::command]
fn stop_mic_monitor(state: tauri::State<AppState>) {
    state.mic_monitor.lock().unwrap().stop();
}

/// 自动更新装完后的可靠重启：兜底 Tauri v2 在 macOS 上 relaunch() 的已知 bug
/// （装好新包却没能重启、卡在旧版本）。spawn 一个脱离的 helper 轮询父进程退出，
/// 父进程退出后再 `open -n` 重开新 .app；本进程随后 exit(0)。
/// 必须轮询父进程退出：否则 single-instance/同名进程仍在，新进程会被判为重复实例而退出。
#[cfg(target_os = "macos")]
#[tauri::command]
fn force_quit_and_relaunch(app: tauri::AppHandle) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
    let ppid = std::process::id();
    let app_bundle = current_exe
        .ancestors()
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("app"))
        .ok_or_else(|| "current_exe 祖先里没有 .app bundle".to_string())?;
    let bundle_str = app_bundle.to_string_lossy();
    // 单引号包裹 + 转义，安全塞进 sh -c
    let escaped = format!("'{}'", bundle_str.replace('\'', "'\\''"));
    let cmd = format!(
        "i=0; while kill -0 {ppid} 2>/dev/null && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done; sleep 0.3; open -n {app}",
        ppid = ppid,
        app = escaped
    );
    std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn relaunch helper failed: {e}"))?;
    // 给前端一点时间显示「重启中」，再退出本进程触发 helper 重开。
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        app.exit(0);
    });
    Ok(())
}

/// 非 macOS（Windows）：上面的 .app + open -n 重启逻辑是 macOS 专属。Windows 上 updater
/// 是惰性的（latest.json 只含 darwin），正常不会调到这里；兜底直接走 Tauri 的进程重启。
#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn force_quit_and_relaunch(app: tauri::AppHandle) -> Result<(), String> {
    app.restart();
    #[allow(unreachable_code)]
    Ok(())
}

/// ASR 引擎配置（前端读写）：引擎选择 + 火山 / 阿里 BYOK 凭证 + 阿里模型。
#[derive(serde::Serialize, serde::Deserialize)]
struct AsrConfig {
    engine: String,
    cloud_vendor: String,
    volc_api_key: String,
    volc_resource_id: String,
    ali_api_key: String,
    ali_model: String,
}

#[tauri::command]
fn get_asr_config(app: AppHandle) -> AsrConfig {
    let p = prefs::load(&app);
    let engine = prefs::effective_asr_engine(&p);
    let cloud_vendor = prefs::effective_cloud_vendor(&p);
    let volc_resource_id = prefs::effective_volc_resource_id(&p);
    let ali_model = prefs::effective_ali_model(&p);
    AsrConfig {
        engine,
        cloud_vendor,
        volc_resource_id,
        ali_model,
        volc_api_key: p.volc_api_key.unwrap_or_default(),
        ali_api_key: p.ali_api_key.unwrap_or_default(),
    }
}

#[tauri::command]
fn set_asr_config(app: AppHandle, cfg: AsrConfig) -> Result<(), String> {
    let mut p = prefs::load(&app);
    p.asr_engine = Some(cfg.engine);
    p.cloud_vendor = Some(cfg.cloud_vendor);
    p.volc_api_key = Some(cfg.volc_api_key);
    p.volc_resource_id = Some(cfg.volc_resource_id);
    p.ali_api_key = Some(cfg.ali_api_key.clone());
    p.ali_model = Some(cfg.ali_model);
    prefs::save(&app, &p)?;
    sync_ali_key(&app, &cfg.ali_api_key, true);
    Ok(())
}

#[tauri::command]
fn get_onboarded(app: AppHandle) -> bool {
    prefs::load(&app).onboarded
}

#[tauri::command]
fn complete_onboarding(app: AppHandle) -> Result<(), String> {
    let mut p = prefs::load(&app);
    p.onboarded = true;
    prefs::save(&app, &p)
}

/// 重置本 app 的辅助功能 TCC 授权记录。ad-hoc 签名每次构建 cdhash 变、旧授权失效，
/// 但系统设置仍显示「已勾选」——清掉旧记录可消除「开关开着却无效」的矛盾，让用户干净地
/// 重新授权当前版本（参考 OpenLess 的兜底做法）。
#[tauri::command]
fn reset_accessibility_tcc() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("tccutil")
            .args(["reset", "Accessibility", "com.voicetotext.app"])
            .status();
    }
}

/// 重启 app：辅助功能授权后 CGEventTap 需重启进程才生效。重启前清掉 .app 的 quarantine，
/// 避免分发 / 更新后 Gatekeeper 拦「已损坏」。
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(app_path) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
            let _ = std::process::Command::new("xattr").args(["-cr"]).arg(app_path).status();
        }
    }
    app.restart();
}

/// 查询是否已授予 macOS 辅助功能权限（注入用）。
#[tauri::command]
fn check_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// 弹出系统授权框请求辅助功能权限；返回当前是否已授权。
#[tauri::command]
fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                eprintln!("[DIAG] CloseRequested label={:?}", window.label());
            }
            // 关闭主窗口 = 隐藏到后台常驻（不退出，符合 macOS 习惯）；
            // 之后点 Dock 图标或菜单栏托盘图标都可重新唤起。capsule 等其它窗口不拦截。
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    #[cfg(target_os = "macos")]
                    set_dock_visible(window.app_handle(), false);
                }
            }
        })
        .manage(AppState {
            activation: Mutex::new(Activation::new()),
            recorder: Mutex::new(recorder::Recorder::new()),
            asr: Mutex::new(None),
            // setup 里会用持久化的（或默认）热键覆盖。
            hold_shortcut: Mutex::new(String::new()),
            toggle_shortcut: Mutex::new(String::new()),
            bindings: Arc::new(Mutex::new(keytap::Bindings::default())),
            toggle_recording: Arc::new(AtomicBool::new(false)),
            mic_monitor: Mutex::new(recorder::Monitor::new()),
            capsule_gen: AtomicU64::new(0),
        })
        .invoke_handler(tauri::generate_handler![
            get_recording,
            get_shortcuts,
            set_shortcut,
            get_polish_style,
            set_polish_style,
            list_microphones,
            get_microphone,
            set_microphone,
            start_mic_monitor,
            stop_mic_monitor,
            get_asr_config,
            set_asr_config,
            get_llm_settings,
            set_llm_settings,
            get_vocab,
            set_vocab,
            get_learned_suggestions,
            accept_learned_suggestion,
            dismiss_learned_suggestion,
            get_onboarded,
            complete_onboarding,
            check_accessibility,
            request_accessibility,
            reset_accessibility_tcc,
            restart_app,
            force_quit_and_relaunch
        ])
        .setup(|app| {
            eprintln!("[DIAG] window labels = {:?}", app.webview_windows().keys().cloned().collect::<Vec<_>>());
            // ---- 系统托盘 ----
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit])?;
            // 托盘图标：macOS 菜单栏用单色模板 glyph（按浅/深自动着色），不用近无色的主图标；
            // Windows 任务栏托盘里模板 glyph 几乎看不见，改用彩色 app 图标。
            #[cfg(target_os = "macos")]
            let tray_icon = tauri::include_image!("icons/tray.png");
            #[cfg(not(target_os = "macos"))]
            let tray_icon = tauri::include_image!("icons/32x32.png");
            TrayIconBuilder::with_id(TRAY_ID)
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                // 左键点击不弹菜单，而是唤起主窗口；右键仍弹菜单（含「退出」）
                .show_menu_on_left_click(false)
                .tooltip("Untype · 空闲")
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键单击（抬起）唤起主窗口 —— 即「点状态栏图标打开窗口」
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // ---- 全局热键：CGEventTap 监听（支持单键/单修饰键，区分左右；改键即时生效）----
            let p = prefs::load(app.handle());
            let hold = prefs::effective_hold(&p);
            let toggle = prefs::effective_toggle(&p);
            {
                let st = app.state::<AppState>();
                *st.bindings.lock().unwrap() = keytap::Bindings::from_strs(&hold, &toggle);
                *st.hold_shortcut.lock().unwrap() = hold;
                *st.toggle_shortcut.lock().unwrap() = toggle;
            }
            start_keytap(app.handle());

            // 启动即后台预加载 ASR 模型：像输入法那样常驻就绪，每次唤起都能立即识别。
            let warm = app.handle().clone();
            let warm_dir = model_dir(&warm);
            std::thread::spawn(move || {
                if let Err(e) = ensure_asr(&warm, &warm_dir) {
                    eprintln!("启动预热 ASR 失败: {e}");
                }
            });

            // 冷启动：主窗 visible:false 创建，setup 里按系统深浅设原生 themed 底色后「立即」显示。
            // 不做开场动画——WKWebView 在 SPA 启动那 ~0.5s 不绘制页内内容，且冷启动期对新窗口渲染有
            // 挂起（实测独立 splash 窗 frames 冻在 1~2、内容从不合成，靠它绕不过去）。改用原生底色
            //（非 WebView 内容、可靠绘制）兜住这段：用户看到一块干净的 themed 深色，SPA 就绪即填上 UI。
            // setup 跑在主线程，窗口操作可直接调。
            if let Some(w) = app.get_webview_window("main") {
                let dark = w.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(true);
                let bg = if dark {
                    tauri::window::Color(27, 28, 31, 255)
                } else {
                    tauri::window::Color(243, 244, 246, 255)
                };
                let _ = w.set_background_color(Some(bg));
                #[cfg(target_os = "macos")]
                let _ = app.handle().set_dock_visibility(true);
                let _ = w.show();
                let _ = w.set_focus();
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // 点 Dock 图标（macOS Reopen 事件）唤起主窗口，与点菜单栏托盘图标行为一致
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &_event {
                show_main_window(_app);
            }
        });
}
