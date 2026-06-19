//! Silero VAD：按停顿把 16kHz 单声道音频切成语音段。
//! 提供流式 `Vad`（边喂边吐完成段，供实时管线用）和一次性 `segment`（供测试/离线用）。

use std::path::Path;

use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};

/// Silero VAD 处理窗口大小（16kHz 下的标准值）。
const WINDOW: usize = 512;

fn drain(inner: &mut SileroVad, out: &mut Vec<Vec<f32>>) {
    while !inner.is_empty() {
        out.push(inner.front().samples);
        inner.pop();
    }
}

/// 流式 VAD：持久持有一个 SileroVad，可一块一块喂入、随时吐出已完成的语音段。
pub struct Vad {
    inner: SileroVad,
}

impl Vad {
    pub fn new(model: &Path) -> Result<Self, String> {
        let config = SileroVadConfig {
            model: model.to_string_lossy().into_owned(),
            sample_rate: crate::audio::TARGET_SAMPLE_RATE,
            threshold: 0.5,
            min_silence_duration: 0.3, // 停顿 ≥0.3s 视为一句结束（更快出字幕）
            min_speech_duration: 0.25, // 丢弃过短碎片，抗噪
            max_speech_duration: 20.0, // 连续说话超 20s 强制切，保持在 ASR 擅长的段长范围
            window_size: WINDOW as i32,
            ..Default::default()
        };
        let inner =
            SileroVad::new(config, 30.0).map_err(|e| format!("加载 Silero VAD 失败: {e:?}"))?;
        Ok(Self { inner })
    }

    /// 喂入 16k 单声道样本，返回此刻新完成的语音段（按停顿切出）。
    pub fn accept(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        for w in samples.chunks(WINDOW) {
            self.inner.accept_waveform(w.to_vec());
            drain(&mut self.inner, &mut out);
        }
        out
    }

    /// 收尾：把缓冲里残留的最后一段（即使没有尾随停顿）也吐出来。
    pub fn flush(&mut self) -> Vec<Vec<f32>> {
        self.inner.flush();
        let mut out = Vec::new();
        drain(&mut self.inner, &mut out);
        out
    }
}

/// 一次性整段分段（建立在流式 Vad 之上），供测试与离线场景使用。
#[allow(dead_code)]
pub fn segment(model: &Path, samples: &[f32]) -> Result<Vec<Vec<f32>>, String> {
    let mut vad = Vad::new(model)?;
    let mut out = vad.accept(samples);
    out.extend(vad.flush());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/asr")
    }

    /// 真模型集成测试：构造「语音 + 静音 + 语音」，断言 VAD 切出 ≥2 段。
    #[test]
    #[ignore = "需要 silero_vad.onnx 与 test_wavs/zh.wav"]
    fn segments_multi_utterance() {
        let wav = dir().join("test_wavs/zh.wav");
        let mut reader = hound::WavReader::open(&wav).expect("打开 wav 失败");
        assert_eq!(reader.spec().sample_rate, 16000);
        let pcm: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        let speech = crate::audio::i16_to_f32(&pcm);

        let mut audio = Vec::new();
        audio.extend_from_slice(&speech);
        audio.extend(std::iter::repeat_n(0.0f32, 16000)); // 1s 静音
        audio.extend_from_slice(&speech);

        let segs = segment(&dir().join("silero_vad.onnx"), &audio).expect("VAD 分段失败");
        eprintln!("VAD 切出 {} 段", segs.len());
        assert!(segs.len() >= 2, "应至少切出 2 段，实际 {}", segs.len());
    }
}
