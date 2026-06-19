//! 离线 ASR 模型评测 harness（一次性研究用，不进生产）。
//!
//! 对 `models/eval/wavs/` 下的一组 wav 跑多个候选本地模型，打印转写 + 单条耗时；
//! transducer 额外对比「greedy 无热词」vs「modified_beam_search + 热词偏置」，
//! 验证「像豆包那样解码时往术语偏」在我们现有的 sherpa-rs 绑定里是否可行、是否有效。
//!
//! 运行（在 src-tauri 下）：
//!   cargo run --example eval_asr
//! 说明：debug 即可——重计算在优化过的原生 sherpa-onnx 库里，profile 不影响推理延迟。
//!
//! 模型路径：
//!   SenseVoice（现用基线）  models/sense-voice/{model.int8.onnx,tokens.txt}
//!   Paraformer-zh           models/eval/sherpa-onnx-paraformer-zh-2023-03-28/
//!   Zipformer transducer    models/eval/sherpa-onnx-zipformer-multi-zh-hans-2023-9-2/

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sherpa_rs::paraformer::{ParaformerConfig, ParaformerRecognizer};
use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};
use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

/// 读 wav → (采样率, 单声道 f32)。sherpa 接受原采样率并内部重采样。
fn read_wav(path: &Path) -> (u32, Vec<f32>) {
    let mut r = hound::WavReader::open(path).expect("打开 wav 失败");
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, b) if b <= 16 => {
            r.samples::<i16>().map(|x| x.unwrap() as f32 / 32768.0).collect()
        }
        (hound::SampleFormat::Int, _) => r
            .samples::<i32>()
            .map(|x| x.unwrap() as f32 / 2_147_483_648.0)
            .collect(),
        (hound::SampleFormat::Float, _) => r.samples::<f32>().map(|x| x.unwrap()).collect(),
    };
    let mono = if ch <= 1 {
        raw
    } else {
        raw.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect()
    };
    (spec.sample_rate, mono)
}

fn list_wavs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map_or(false, |x| x == "wav"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// 在目录里找第一个文件名以 `prefix` 开头的 .onnx，int8 优先。
fn find_onnx(dir: &Path, prefix: &str) -> Option<String> {
    let mut cands: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
            n.starts_with(prefix) && n.ends_with(".onnx")
        })
        .collect();
    cands.sort_by_key(|p| {
        let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("").to_string();
        if n.contains("int8") { 0 } else { 1 }
    });
    cands.first().map(|p| s(p))
}

type Clip = (String, u32, Vec<f32>);

fn main() {
    let root = manifest();
    let wavs_dir = root.join("models/eval/wavs");
    let wavs = list_wavs(&wavs_dir);
    if wavs.is_empty() {
        eprintln!("没有 wav，请放到 {:?}", wavs_dir);
        return;
    }
    let clips: Vec<Clip> = wavs
        .iter()
        .map(|p| {
            let (sr, m) = read_wav(p);
            (p.file_name().unwrap().to_string_lossy().into_owned(), sr, m)
        })
        .collect();
    eprintln!(
        "评测 {} 条 wav：{:?}\n",
        clips.len(),
        clips.iter().map(|c| c.0.clone()).collect::<Vec<_>>()
    );

    // 热词（演示豆包式解码偏置）：cjkchar 模型每行用空格分隔单字
    let terms = ["中文强", "列要点", "读写分离", "并发"];
    let hot_path = root.join("models/eval/hotwords.txt");
    let hot_content = terms
        .iter()
        .map(|t| t.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(&hot_path, &hot_content);

    run_sensevoice(&root, &clips);
    run_paraformer(&root, &clips);
    run_transducer(&root, &clips, &hot_path);
}

fn run_sensevoice(root: &Path, clips: &[Clip]) {
    let dir = root.join("models/sense-voice");
    let model = dir.join("model.int8.onnx");
    if !model.exists() {
        eprintln!("[SenseVoice] 模型缺失，跳过");
        return;
    }
    let cfg = SenseVoiceConfig {
        model: s(&model),
        tokens: s(&dir.join("tokens.txt")),
        language: "auto".into(),
        use_itn: true,
        num_threads: Some(4),
        ..Default::default()
    };
    let mut rec = match SenseVoiceRecognizer::new(cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[SenseVoice] 加载失败: {e:?}");
            return;
        }
    };
    println!("\n========== SenseVoice-small（现用基线，int8 CTC）==========");
    for (name, sr, m) in clips {
        let t = Instant::now();
        let text = rec.transcribe(*sr, m).text.trim().to_string();
        println!("[{name}] ({}ms)\n  {text}", ms(t));
    }
}

fn run_paraformer(root: &Path, clips: &[Clip]) {
    let dir = root.join("models/eval/sherpa-onnx-paraformer-zh-2023-03-28");
    let Some(model) = find_onnx(&dir, "model") else {
        eprintln!("[Paraformer] 模型缺失，跳过 ({dir:?})");
        return;
    };
    let cfg = ParaformerConfig {
        model,
        tokens: s(&dir.join("tokens.txt")),
        num_threads: Some(4),
        ..Default::default()
    };
    let mut rec = match ParaformerRecognizer::new(cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[Paraformer] 加载失败: {e:?}");
            return;
        }
    };
    println!("\n========== Paraformer-zh-2023-03-28（int8）==========");
    for (name, sr, m) in clips {
        let t = Instant::now();
        let text = rec.transcribe(*sr, m).text.trim().to_string();
        println!("[{name}] ({}ms)\n  {text}", ms(t));
    }
}

fn run_transducer(root: &Path, clips: &[Clip], hot_path: &Path) {
    let dir = root.join("models/eval/sherpa-onnx-zipformer-multi-zh-hans-2023-9-2");
    let (Some(encoder), Some(decoder), Some(joiner)) = (
        find_onnx(&dir, "encoder"),
        find_onnx(&dir, "decoder"),
        find_onnx(&dir, "joiner"),
    ) else {
        eprintln!("[Transducer] encoder/decoder/joiner 缺失，跳过 ({dir:?})");
        return;
    };
    let tokens = s(&dir.join("tokens.txt"));
    let mk = |decoding: &str, hot: &str, score: f32| TransducerConfig {
        encoder: encoder.clone(),
        decoder: decoder.clone(),
        joiner: joiner.clone(),
        tokens: tokens.clone(),
        num_threads: 4,
        sample_rate: 16000,
        feature_dim: 80,
        decoding_method: decoding.into(),
        hotwords_file: hot.into(),
        hotwords_score: score,
        modeling_unit: "cjkchar".into(),
        model_type: "transducer".into(),
        ..Default::default()
    };

    match TransducerRecognizer::new(mk("greedy_search", "", 0.0)) {
        Ok(mut rec) => {
            println!("\n========== Zipformer transducer（greedy，无热词）==========");
            for (name, sr, m) in clips {
                let t = Instant::now();
                let text = rec.transcribe(*sr, m).trim().to_string();
                println!("[{name}] ({}ms)\n  {text}", ms(t));
            }
        }
        Err(e) => eprintln!("[Transducer greedy] 加载失败: {e:?}"),
    }

    match TransducerRecognizer::new(mk("modified_beam_search", &s(hot_path), 2.0)) {
        Ok(mut rec) => {
            println!("\n========== Zipformer transducer（modified_beam_search + 热词偏置）==========");
            println!("(热词：中文强 / 列要点 / 读写分离 / 并发)");
            for (name, sr, m) in clips {
                let t = Instant::now();
                let text = rec.transcribe(*sr, m).trim().to_string();
                println!("[{name}] ({}ms)\n  {text}", ms(t));
            }
        }
        Err(e) => eprintln!("[Transducer hotwords] 加载失败: {e:?}"),
    }
}
