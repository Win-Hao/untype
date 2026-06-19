//! 本地语音识别：封装 sherpa-onnx 的 SenseVoice 模型。
//! 输入 16kHz 单声道 f32 采样，输出中文/中英混合文本（**自带标点 + ITN**）。完全本地、零联网。
//!
//! 2026-06-18 从 Paraformer 换回 SenseVoice：Paraformer 普通话略准但**无标点/ITN**，
//! 纯逐字稿（不经 LLM）下没标点很难读；SenseVoice 自带标点/ITN（`use_itn`）、中英混说 OK、
//! 又快又小（int8 ~228MB）。准确率略低一点，但纯逐字稿可读性更好、整理路 LLM 也会纠。

use std::path::Path;

use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};

pub struct Asr {
    recognizer: SenseVoiceRecognizer,
}

impl Asr {
    /// 从模型文件与 tokens 加载 SenseVoice（开启标点 + ITN）。
    pub fn load(model: &Path, tokens: &Path) -> Result<Self, String> {
        let config = SenseVoiceConfig {
            model: model.to_string_lossy().into_owned(),
            tokens: tokens.to_string_lossy().into_owned(),
            language: "auto".into(), // 自动语种（中英混说）
            use_itn: true,           // 标点 + 反向文本规范化（数字/单位）
            num_threads: Some(4),
            ..Default::default()
        };
        let recognizer = SenseVoiceRecognizer::new(config)
            .map_err(|e| format!("加载 SenseVoice 失败: {e:?}"))?;
        Ok(Self { recognizer })
    }

    /// 转写 16kHz 单声道 f32 采样，返回去除首尾空白的识别文本（含标点）。
    pub fn transcribe(&mut self, sample_rate: u32, samples: &[f32]) -> String {
        self.recognizer
            .transcribe(sample_rate, samples)
            .text
            .trim()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/sense-voice")
    }

    /// 真模型集成测试：喂固定中文 WAV，断言能转出含中文的非空文本（带标点）。
    /// 标 #[ignore]，让默认 `cargo test` 保持快速；验证真识别用 `cargo test -- --ignored`。
    #[test]
    #[ignore = "需要已下载的 SenseVoice 模型；用 `cargo test -- --ignored` 运行"]
    fn transcribes_chinese_test_wav() {
        let dir = model_dir();
        let model = dir.join("model.int8.onnx");
        let tokens = dir.join("tokens.txt");
        let wav = dir.join("test_wavs/zh.wav");
        assert!(model.exists(), "模型缺失: {model:?}");

        let mut reader = hound::WavReader::open(&wav).expect("打开测试 wav 失败");
        let spec = reader.spec();
        let pcm_i16: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();

        // 走与生产同一条音频预处理链
        let f32_samples = crate::audio::i16_to_f32(&pcm_i16);
        let samples = crate::audio::prepare_for_asr(&f32_samples, spec.channels, spec.sample_rate);

        let mut asr = Asr::load(&model, &tokens).expect("加载 SenseVoice 失败");
        let text = asr.transcribe(crate::audio::TARGET_SAMPLE_RATE, &samples);

        eprintln!("【识别结果】{text}");
        assert!(!text.is_empty(), "转写结果不应为空");
        assert!(
            text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "转写结果应包含中文字符，实际: {text}"
        );
    }
}
