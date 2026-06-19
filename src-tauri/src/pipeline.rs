//! 流式处理管线：持续排空录音缓冲，边重采样、边 VAD 切段、边识别；
//! 停录后对累计文本润色并注入光标。处理完即丢，内存恒定，支持长录音。

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::audio::{self, StreamResampler};
use crate::recorder::RecordingHandle;
use crate::vad::Vad;

/// 处理一批原始样本：下混 → 重采样 → VAD 切段 → 逐段识别 → 累计。
fn process_chunk(
    app: &AppHandle,
    model_dir: &Path,
    channels: u16,
    vad: &mut Vad,
    resampler: &mut StreamResampler,
    parts: &mut Vec<String>,
    raw: &[f32],
) {
    if raw.is_empty() {
        return;
    }
    let mono = audio::to_mono(raw, channels);
    let s16 = resampler.push(&mono);
    for seg in vad.accept(&s16) {
        match crate::transcribe_with_state(app, model_dir, &seg) {
            Ok(t) => {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    parts.push(t);
                }
            }
            Err(e) => eprintln!("识别段失败: {e}"),
        }
    }
}

/// 把各 VAD 段识别文本拼成稿。SenseVoice 自带标点 / ITN，故：
/// ① 去段首尾空白；② 丢弃空段与纯标点段；③ 合并相邻完全重复段（短段/收尾偶发重复幻觉）；
/// ④ **直接拼接**——靠模型自带标点分句，不再去尾标点、不按段换行（那是为无标点模型设计的，
/// 会把 SenseVoice 的句号删掉再塞换行，反而乱）。
fn assemble_transcript(parts: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        let p = part.trim();
        if p.is_empty() || p.chars().all(|c| "。，、；：！？.,;:!? 　".contains(c)) {
            continue;
        }
        if out.last().map(|s| s.as_str()) == Some(p) {
            continue; // 合并相邻完全重复段
        }
        out.push(p.to_string());
    }
    out.join("")
}

pub fn run(app: AppHandle, handle: RecordingHandle, model_dir: PathBuf) {
    // 录音悬浮胶囊：录音 → 润色期间显示，结束时隐藏
    crate::show_capsule(&app);
    // 等录音配置（采样率/声道）就绪
    let (rate, channels) = loop {
        if let Some(c) = *handle.config.lock().unwrap() {
            break c;
        }
        if handle.stop.load(Ordering::SeqCst) {
            // 还没拿到录音配置就被置停：多半是录音设备打不开（capture 早退会先置 failed）。
            if handle.failed.load(Ordering::SeqCst) {
                let _ = app.emit(
                    "asr-error",
                    "未能开始录音：打不开麦克风设备（请检查设备与权限）".to_string(),
                );
            }
            crate::hide_capsule(&app);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    // 选 ASR 引擎：云端（火山 / 阿里，需配 key）走 cloud；否则本地 SenseVoice。
    let p = crate::prefs::load(&app);
    let use_cloud = crate::prefs::effective_asr_engine(&p) == "cloud"
        && match crate::prefs::effective_cloud_vendor(&p).as_str() {
            "ali" => !p.ali_api_key.clone().unwrap_or_default().trim().is_empty(),
            _ => !p.volc_api_key.clone().unwrap_or_default().trim().is_empty(),
        };
    let raw_text = if use_cloud {
        match transcribe_cloud(&app, &handle, rate, channels, &p) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("云端识别失败: {e}");
                let _ = app.emit("asr-error", format!("云端识别失败：{e}"));
                crate::hide_capsule(&app);
                return;
            }
        }
    } else {
        transcribe_local(&app, &handle, &model_dir, rate, channels)
    };

    // 取消：用户按 Esc 中止——丢弃识别结果，不出稿、不注入、不学习，静默收起胶囊。
    if handle.cancel.load(Ordering::SeqCst) {
        crate::hide_capsule(&app);
        return;
    }
    if raw_text.trim().is_empty() {
        // 设备中途失败（capture 置了 failed）也会走到这里且无音频：给个温和提示，别静默。
        if handle.failed.load(Ordering::SeqCst) {
            let _ = app.emit("asr-error", "录音设备异常，未采集到音频".to_string());
        }
        crate::hide_capsule(&app);
        return;
    }
    finalize(&app, raw_text);
}

/// 本地 SenseVoice：边录边 VAD 切段识别，停录后拼接成稿。
fn transcribe_local(
    app: &AppHandle,
    handle: &RecordingHandle,
    model_dir: &Path,
    rate: u32,
    channels: u16,
) -> String {
    let mut vad = match Vad::new(&model_dir.join("silero_vad.onnx")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("VAD 初始化失败: {e}");
            let _ = app.emit("asr-error", e);
            return String::new();
        }
    };
    let mut resampler = StreamResampler::new(rate, audio::TARGET_SAMPLE_RATE);
    let mut parts: Vec<String> = Vec::new();
    let mut viz = crate::viz::SpectrumViz::new(rate); // 胶囊频谱波形

    loop {
        let raw: Vec<f32> = {
            let mut b = handle.buffer.lock().unwrap();
            std::mem::take(&mut *b)
        };
        if !raw.is_empty() {
            let _ = app.emit("capsule-level", viz.bands(&raw, channels)); // 录音胶囊频谱波形
        }
        process_chunk(app, model_dir, channels, &mut vad, &mut resampler, &mut parts, &raw);

        if handle.stop.load(Ordering::SeqCst) {
            // 停录后再排空一次，处理最后到达的音频
            let tail: Vec<f32> = {
                let mut b = handle.buffer.lock().unwrap();
                std::mem::take(&mut *b)
            };
            process_chunk(app, model_dir, channels, &mut vad, &mut resampler, &mut parts, &tail);
            break;
        }
        std::thread::sleep(Duration::from_millis(60));
    }

    // 取消：跳过 flush 与拼稿；run() 会据 cancel 静默收起。
    if handle.cancel.load(Ordering::SeqCst) {
        return String::new();
    }

    // flush VAD：把最后一段（即使没尾随停顿）也识别掉
    for seg in vad.flush() {
        if let Ok(t) = crate::transcribe_with_state(app, model_dir, &seg) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                parts.push(t);
            }
        }
    }
    assemble_transcript(&parts)
}

/// 云端（火山 / 阿里）：收全量音频 → 下混重采样到 16k → WebSocket 流式识别。
/// 词典软词作为热词随请求传入（解码时偏置，仅火山支持），云端自带 VAD/断句，不走本地切段。
fn transcribe_cloud(
    app: &AppHandle,
    handle: &RecordingHandle,
    rate: u32,
    channels: u16,
    p: &crate::prefs::Prefs,
) -> Result<String, String> {
    // 流式：把云端会话的「连接 + 收发（含每包阻塞读）」整个放到独立 worker 线程；
    // 本循环只负责「画波形 + 把音频块塞进 channel」，绝不碰网络——于是波形不再被网络往返卡住。
    // worker 自己边收边发（也顺带避免响应帧积压背压）。批量版 transcribe_volc/qwen 仍在，可随时回退。
    let vendor = crate::prefs::effective_cloud_vendor(p);
    let vocab = crate::load_vocab(app);
    let volc_key = p.volc_api_key.clone().unwrap_or_default();
    let resource_id = crate::prefs::effective_volc_resource_id(p);
    let ali_key = p.ali_api_key.clone().unwrap_or_default();
    let terms = vocab.terms.clone();
    let cancel = handle.cancel.clone();

    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    let worker = std::thread::spawn(move || -> Result<String, String> {
        // 连接也在 worker 线程，故不阻塞主循环起步（早期音频先在 channel 里排队，连上即推）。
        let mut session = if vendor == "ali" {
            crate::cloud_asr::CloudSession::start_qwen(&ali_key, audio::TARGET_SAMPLE_RATE)?
        } else {
            crate::cloud_asr::CloudSession::start_volc(
                &volc_key,
                &resource_id,
                &terms,
                audio::TARGET_SAMPLE_RATE,
            )?
        };
        while let Ok(chunk) = rx.recv() {
            session.push(&chunk)?; // 阻塞读发生在这条线程上，与波形无关
        }
        if cancel.load(Ordering::SeqCst) {
            session.abort();
            Ok(String::new())
        } else {
            session.finish()
        }
    });

    let mut resampler = StreamResampler::new(rate, audio::TARGET_SAMPLE_RATE); // 流式下混 + 重采样到 16k
    let mut viz = crate::viz::SpectrumViz::new(rate); // 胶囊频谱波形
    let mut any_audio = false;

    loop {
        let raw: Vec<f32> = {
            let mut b = handle.buffer.lock().unwrap();
            std::mem::take(&mut *b)
        };
        if !raw.is_empty() {
            let _ = app.emit("capsule-level", viz.bands(&raw, channels)); // 波形：不被网络阻塞
            let mono16k = resampler.push(&audio::to_mono(&raw, channels));
            if !mono16k.is_empty() {
                let _ = tx.send(mono16k); // 非阻塞交给 worker
                any_audio = true;
            }
        }
        if handle.stop.load(Ordering::SeqCst) {
            // 排空最后一段交给 worker
            let tail: Vec<f32> = {
                let mut b = handle.buffer.lock().unwrap();
                std::mem::take(&mut *b)
            };
            if !tail.is_empty() {
                let mono16k = resampler.push(&audio::to_mono(&tail, channels));
                if !mono16k.is_empty() {
                    let _ = tx.send(mono16k);
                    any_audio = true;
                }
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    drop(tx); // 通知 worker：音频发完 → 去 finish / abort

    // 取消：等 worker 收尾（已据 cancel abort，很快）后丢结果；run() 会据 cancel 静默收起。
    if handle.cancel.load(Ordering::SeqCst) {
        let _ = worker.join();
        return Ok(String::new());
    }
    if !any_audio {
        let _ = worker.join();
        return Ok(String::new()); // 真静音：用户没说话，静默收起（不报错）
    }
    // 松手 → 进 thinking（此刻识别已随录随出大半，worker.finish 只取尾巴 + 最终稿，很快）。
    let _ = app.emit("polishing", true);
    let text = worker
        .join()
        .map_err(|_| "云端识别线程异常退出".to_string())??;
    if text.trim().is_empty() {
        // 有音频却拿到空结果：多半是云端异常 / 没听清，别和真静音一样静默——给温和提示。
        eprintln!("云端识别返回空结果");
        let _ = app.emit(
            "asr-error",
            "云端没识别到内容，可能没说清或网络波动，请重试".to_string(),
        );
    }
    Ok(text)
}

/// 出稿：词典硬替换 + 拼音纠错 → 按风格润色/直出 → 注入 → 自学 → 收起胶囊。
/// 本地与云端两条引擎产出 raw_text 后共用此尾部。
fn finalize(app: &AppHandle, raw_text: String) {
    // 进入处理阶段：胶囊切「处理中」（thinking 涵盖识别完成后的整理 / 直出，直到注入）。
    let _ = app.emit("polishing", true);
    // 自定义词典：先做确定性硬替换 → 再做拼音纠错词归一 → 软词表随后喂给 LLM
    let vocab = crate::load_vocab(app);
    let raw_text = vocab.apply_replacements(&raw_text);
    let raw_text = vocab.apply_pinyin_corrections(&raw_text);
    let style = crate::prefs::effective_style(&crate::prefs::load(app));
    let mut polish_failed = false;
    let final_text = if style == "raw" {
        // 纯逐字稿：跳过 LLM，直接用原始识别文本
        raw_text.clone()
    } else {
        match crate::llm_config(app) {
            Some(cfg) => match crate::llm::polish(&cfg, &raw_text, &vocab.terms, &style) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("润色失败，回退原文: {e}");
                    let _ = app.emit("llm-error", e);
                    polish_failed = true;
                    raw_text.clone()
                }
            },
            None => raw_text.clone(),
        }
    };
    let inject_ok = match crate::inject::inject_text(&final_text) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("注入失败: {e}");
            // 兜底：注入失败时把文本放进剪贴板，用户可手动 ⌘V 粘贴，避免丢失识别结果
            let _ = crate::inject::copy_to_clipboard(&final_text);
            let _ = app.emit("inject-error", e);
            false
        }
    };

    // 自学（步骤③）：从「输入(raw_text) vs 输出(final_text)」里学同音纠正。
    let learned = crate::learn::extract_homophone_pairs(&raw_text, &final_text);
    if !learned.is_empty() {
        let mut store = crate::learn::load(app);
        store.record_pairs(&learned);
        if let Err(e) = crate::learn::save(app, &store) {
            eprintln!("自学保存失败: {e}");
        }
    }

    // 胶囊收尾：成功时文字已注入光标，不再显示「完成」态——thinking 一结束直接收起（Typeless 风格）。
    // 仅失败仍需提示：润色失败「已输出原文」/ 注入失败「已复制 ⌘V」，发结果态并停留久点让用户注意到。
    // （这点停留发生在文本注入之后，不影响听写延迟。）
    // 两种收尾前都判一下：这期间若用户已开始新一轮录音，别把新录音的胶囊收掉（防竞态）。
    if inject_ok && !polish_failed {
        // 成功：文字已注入光标，thinking 一结束直接收起（无完成态/进度条，spinner 随胶囊淡出）。
        if !crate::is_recording(app) {
            crate::hide_capsule(app);
        }
    } else {
        let result = if !inject_ok { "clipboard" } else { "raw" };
        let _ = app.emit("capsule-result", result);
        std::thread::sleep(Duration::from_millis(1200));
        if !crate::is_recording(app) {
            crate::hide_capsule(app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::assemble_transcript;

    #[test]
    fn joins_segments_keeping_punctuation() {
        // SenseVoice 自带标点：直接拼接、保留标点、不按段换行
        let parts = vec!["  你好。 ".to_string(), "今天天气不错。".to_string()];
        assert_eq!(assemble_transcript(&parts), "你好。今天天气不错。");
    }

    #[test]
    fn dedups_adjacent_duplicate_segments() {
        // 短段/收尾偶发的相邻完全重复应被合并
        let parts = vec![
            "这个项目的进展。".to_string(),
            "这个项目的进展。".to_string(),
            "下一步计划。".to_string(),
        ];
        assert_eq!(assemble_transcript(&parts), "这个项目的进展。下一步计划。");
    }

    #[test]
    fn drops_empty_and_punct_only_segments() {
        let parts = vec![
            "正文。".to_string(),
            "  ".to_string(),
            "。".to_string(),
            String::new(),
        ];
        assert_eq!(assemble_transcript(&parts), "正文。");
    }

    #[test]
    fn keeps_non_adjacent_repeats() {
        // 非相邻的重复多是有意重复，保留
        let parts = vec!["好。".to_string(), "走。".to_string(), "好。".to_string()];
        assert_eq!(assemble_transcript(&parts), "好。走。好。");
    }

    #[test]
    #[ignore = "需要 LLM_API_KEY 环境变量"]
    fn assemble_then_polish_merges_fragments_and_dedups() {
        // 端到端：相邻重复段被去重；按停顿拆开的片段被 LLM 重组；口述三点成编号列表
        let parts = vec![
            "我今天想聊一下".to_string(),
            "这个项目的进展".to_string(),
            "这个项目的进展".to_string(),
            "然后说三点第一性能".to_string(),
            "第二稳定性".to_string(),
            "第三文档".to_string(),
        ];
        let raw = assemble_transcript(&parts);
        assert_eq!(
            raw.matches("这个项目的进展").count(),
            1,
            "相邻重复段应被去重: {raw}"
        );
        let cfg = crate::llm::LlmConfig::from_env().expect("请先设置 LLM_API_KEY");
        let vocab = crate::vocab::Vocab::load();
        let out = crate::llm::polish(&cfg, &raw, &vocab.terms, "default").expect("polish 失败");
        eprintln!("【assembled】\n{raw}\n【polished】{out}");
        assert!(!out.is_empty());
        assert!(
            out.contains("1.") && out.contains("2.") && out.contains("3."),
            "口述的三点应整理成编号列表: {out}"
        );
    }
}
