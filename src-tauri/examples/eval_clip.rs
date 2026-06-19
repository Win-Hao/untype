//! 一次性 A/B：对单条 wav 跑「本地 SenseVoice + 火山 Seed-ASR + 阿里 Qwen3 + 阿里 Fun-ASR」
//! 并排对比**原始识别**（不经 LLM 润色，才是 ASR 真实水平）。
//!
//! 运行（src-tauri 下）：
//!   cargo run --example eval_clip                          # 默认 ~/Documents/内容创作/转转视频/6月18日.wav
//!   cargo run --example eval_clip -- /path/to/your.wav
//!
//! 云端需根目录 .volc-creds.env 的 VOLC_API_KEY / DASHSCOPE_API_KEY；缺哪个跳过哪个。
//! 为公平对比，云端均**不传热词**。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri_app_lib::{asr, audio, cloud_asr};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读 wav → 16k 单声道 f32（任意采样率 / 声道 / 位深，走与生产同款下混+线性重采样）。
fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("打开 wav 失败");
    let spec = r.spec();
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
    audio::prepare_for_asr(&raw, spec.channels, spec.sample_rate)
}

fn read_key(creds: &str, name: &str) -> Option<String> {
    creds
        .lines()
        .find_map(|l| l.trim().strip_prefix(&format!("{name}=")))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    let wav = std::env::args().nth(1).unwrap_or_else(|| {
        "/Users/huangyonghao/Documents/内容创作/转转视频/6月18日.wav".to_string()
    });
    let wav = PathBuf::from(wav);
    println!("\n音频：{}", wav.display());
    let samples = read_wav_16k_mono(&wav);
    let secs = samples.len() as f32 / audio::TARGET_SAMPLE_RATE as f32;
    println!("时长（重采样到 16k 后）：{secs:.1}s，{} 采样\n", samples.len());

    let root = manifest();

    // 1) 本地 SenseVoice（生产模型 models/sense-voice）
    let dir = root.join("models/sense-voice");
    match asr::Asr::load(&dir.join("model.int8.onnx"), &dir.join("tokens.txt")) {
        Ok(mut a) => {
            let t = Instant::now();
            let text = a.transcribe(audio::TARGET_SAMPLE_RATE, &samples);
            println!("───── 本地 SenseVoice ({}ms) ─────\n{text}\n", t.elapsed().as_millis());
        }
        Err(e) => println!("───── 本地 SenseVoice：加载失败 {e} ─────\n"),
    }

    let creds = fs::read_to_string(root.join("../.volc-creds.env")).unwrap_or_default();
    let volc = read_key(&creds, "VOLC_API_KEY");
    let dash = read_key(&creds, "DASHSCOPE_API_KEY");

    // 2) 火山豆包 Seed-ASR（不传热词，公平）
    match &volc {
        Some(k) => {
            let t = Instant::now();
            match cloud_asr::transcribe_volc(
                k,
                cloud_asr::DEFAULT_RESOURCE_ID,
                &[],
                audio::TARGET_SAMPLE_RATE,
                &samples,
            ) {
                Ok(text) => println!(
                    "───── 火山豆包 Seed-ASR ({}ms) ─────\n{text}\n",
                    t.elapsed().as_millis()
                ),
                Err(e) => println!("───── 火山豆包：失败 {e} ─────\n"),
            }
        }
        None => println!("───── 火山豆包：无 VOLC_API_KEY，跳过 ─────\n"),
    }

    // 3) 阿里 Qwen3-ASR-Flash   4) 阿里 Fun-ASR
    match &dash {
        Some(k) => {
            let t = Instant::now();
            match cloud_asr::transcribe_qwen(k, audio::TARGET_SAMPLE_RATE, &samples) {
                Ok(text) => println!(
                    "───── 阿里 Qwen3-ASR-Flash ({}ms) ─────\n{text}\n",
                    t.elapsed().as_millis()
                ),
                Err(e) => println!("───── 阿里 Qwen3：失败 {e} ─────\n"),
            }
            let t = Instant::now();
            match cloud_asr::transcribe_ali(k, audio::TARGET_SAMPLE_RATE, &samples) {
                Ok(text) => println!(
                    "───── 阿里 Fun-ASR ({}ms) ─────\n{text}\n",
                    t.elapsed().as_millis()
                ),
                Err(e) => println!("───── 阿里 Fun-ASR：失败 {e} ─────\n"),
            }
        }
        None => println!("───── 阿里：无 DASHSCOPE_API_KEY，跳过 ─────\n"),
    }
}
