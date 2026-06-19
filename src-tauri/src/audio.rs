//! 音频预处理：把任意采样率/声道/格式的 PCM 统一成 ASR 需要的
//! 16kHz 单声道 f32。纯函数，便于确定性单元测试（不依赖麦克风）。

/// ASR（SenseVoice）要求的输入采样率。
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// i16 PCM 采样转 f32，归一化到 [-1.0, 1.0]。
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// 把交错的多声道采样下混成单声道（按帧求平均）。
/// `channels` 为 0 或 1 时原样返回。
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// 线性插值重采样到目标采样率（一次性整段）。
/// 本地流式路径用 StreamResampler；此一次性版供云端整段路径（prepare_for_asr）与测试用。
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let last = input.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input[idx.min(last)];
        let b = input[(idx + 1).min(last)];
        out.push(a + (b - a) * frac);
    }
    out
}

/// 一次到位：把采集到的 PCM 整段处理成 16kHz 单声道 f32。
/// 云端引擎走整段路径（pipeline::transcribe_cloud）用它；本地流式路径用 StreamResampler。
pub fn prepare_for_asr(interleaved: &[f32], channels: u16, sample_rate: u32) -> Vec<f32> {
    let mono = to_mono(interleaved, channels);
    resample_linear(&mono, sample_rate, TARGET_SAMPLE_RATE)
}

/// 有状态的流式线性重采样器：可以一小块一小块地喂入（单声道），
/// 跨块保持连续，不在块边界产生断裂。用于流式管线。
pub struct StreamResampler {
    step: f64,     // 每个输出样本对应的输入样本数 = from/to
    pos: f64,      // 下一个输出样本在 buf 中的输入位置
    buf: Vec<f32>, // 尚未消费完的输入样本（单声道）
    passthrough: bool,
}

impl StreamResampler {
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            step: from_hz as f64 / to_hz as f64,
            pos: 0.0,
            buf: Vec::new(),
            passthrough: from_hz == to_hz,
        }
    }

    /// 喂入一块单声道样本，返回这块（连同之前残留）能产出的 16k 样本。
    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passthrough {
            return input.to_vec();
        }
        self.buf.extend_from_slice(input);
        let mut out = Vec::new();
        // 需要 idx 和 idx+1 做插值
        while (self.pos.floor() as usize) + 1 < self.buf.len() {
            let idx = self.pos.floor() as usize;
            let frac = (self.pos - idx as f64) as f32;
            let a = self.buf[idx];
            let b = self.buf[idx + 1];
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        // 丢弃已消费的整数部分，保留小数偏移以维持连续。
        // pos 可能越过当前缓冲（步长较大时），需钳制避免 drain 越界。
        let consumed = (self.pos.floor() as usize).min(self.buf.len());
        if consumed > 0 {
            self.buf.drain(0..consumed);
            self.pos -= consumed as f64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_to_f32_maps_range() {
        let out = i16_to_f32(&[0, 16384, -16384, i16::MIN]);
        assert!((out[0] - 0.0).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-3);
        assert!((out[2] + 0.5).abs() < 1e-3);
        assert!((out[3] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn to_mono_averages_stereo() {
        let stereo = [1.0, 0.0, 0.0, 1.0];
        assert_eq!(to_mono(&stereo, 2), vec![0.5, 0.5]);
    }

    #[test]
    fn to_mono_passes_through_mono() {
        let mono_in = [0.1, 0.2, 0.3];
        assert_eq!(to_mono(&mono_in, 1), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let x = [0.1, 0.2, 0.3, 0.4];
        assert_eq!(resample_linear(&x, 16000, 16000), x.to_vec());
    }

    #[test]
    fn resample_downsamples_length() {
        let x = vec![0.0f32; 4800];
        let out = resample_linear(&x, 48000, 16000);
        assert!((out.len() as i64 - 1600).abs() <= 1, "len was {}", out.len());
    }

    #[test]
    fn prepare_for_asr_outputs_16k_mono() {
        let interleaved = vec![0.5f32; 4800 * 2];
        let out = prepare_for_asr(&interleaved, 2, 48000);
        assert!((out.len() as i64 - 1600).abs() <= 1, "len was {}", out.len());
    }

    #[test]
    fn stream_resampler_passthrough() {
        let mut r = StreamResampler::new(16000, 16000);
        assert_eq!(r.push(&[0.1, 0.2, 0.3]), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn stream_resampler_chunked_matches_total_length() {
        // 48k → 16k：分块喂入与一次喂入，总输出长度应一致（约 1/3）
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.001).sin()).collect();

        let mut all = StreamResampler::new(48000, 16000);
        let out_all = all.push(&input);

        let mut chunked = StreamResampler::new(48000, 16000);
        let mut out_chunked = Vec::new();
        for c in input.chunks(160) {
            out_chunked.extend(chunked.push(c));
        }

        assert!((out_all.len() as i64 - 1600).abs() <= 1, "一次喂入 {}", out_all.len());
        assert!(
            (out_chunked.len() as i64 - out_all.len() as i64).abs() <= 1,
            "分块 {} vs 一次 {}",
            out_chunked.len(),
            out_all.len()
        );
    }
}
