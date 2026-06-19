//! 录音胶囊频谱波形：每块音频做一次小 FFT，按对数人声频段输出 N_BANDS 个 0..1 能量值，
//! 前端镜像成中心对称的均衡器条。每条 = 一个真实频段 → 高低参差、随人声动态起伏
//! （齿音抬高频段、元音抬低频段），不是单标量喂所有条那种「一坨齐涨落」。
//!
//! 自适应噪声门：每段各自跟踪安静基线（快速跟低、极慢回升），对不同麦克风/环境自动校准，
//! 不写死绝对电平。本计算在 `capsule-level` 发送链路上、**不在 ASR 识别链路上**；
//! 1024 点 FFT 亚毫秒级、开销可忽略（参考 Handy 同款思路）。

use std::sync::Arc;

use rustfft::{num_complex::Complex, Fft, FftPlanner};

const FFT_SIZE: usize = 1024;
/// 频段数；前端镜像后 = 2*N-1 条（11 → 21 条，与胶囊原波形条数一致）。
/// **改这里要同步改 `src/routes/capsule/+page.svelte` 的 N_BANDS。**
pub const N_BANDS: usize = 11;
const F_LO: f32 = 150.0; // 人声下限（再低基本是隆隆底噪）
const F_HI: f32 = 5000.0; // 人声 + 齿音上限

pub struct SpectrumViz {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,               // Hann 窗
    band_bins: Vec<(usize, usize)>, // 各频段对应的 FFT bin 区间 [lo, hi)
    floor: [f32; N_BANDS],          // 各频段自适应噪声门（dB）
}

impl SpectrumViz {
    pub fn new(sample_rate: u32) -> Self {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE as f32 - 1.0)).cos()
            })
            .collect();
        let bin_hz = sample_rate as f32 / FFT_SIZE as f32;
        let nyq = FFT_SIZE / 2;
        let band_bins = (0..N_BANDS)
            .map(|b| {
                // 对数分段：人声在低中频更密，等比划分更贴感知
                let f0 = F_LO * (F_HI / F_LO).powf(b as f32 / N_BANDS as f32);
                let f1 = F_LO * (F_HI / F_LO).powf((b + 1) as f32 / N_BANDS as f32);
                let lo = ((f0 / bin_hz).floor() as usize).max(1);
                let hi = ((f1 / bin_hz).ceil() as usize).clamp(lo + 1, nyq);
                (lo, hi)
            })
            .collect();
        // 噪声门起点设高：开头几帧一律判为「低于门」→ 快速下探到真实安静基线，
        // 故胶囊出现时是静的、约 1 秒内自校准，不会无端乱跳。
        Self { fft, window, band_bins, floor: [0.0; N_BANDS] }
    }

    /// 交错音频块（任意声道/采样率，原始未重采样）→ N_BANDS 个 0..1 能量值。
    pub fn bands(&mut self, interleaved: &[f32], channels: u16) -> Vec<f32> {
        let mono = crate::audio::to_mono(interleaved, channels);
        let mut buf = vec![Complex::<f32>::new(0.0, 0.0); FFT_SIZE];
        // 取最近 FFT_SIZE 个样本加 Hann 窗（不足则只用现有的，其余为零）
        let n = mono.len().min(FFT_SIZE);
        let src = &mono[mono.len() - n..];
        for i in 0..n {
            buf[i].re = src[i] * self.window[i];
        }
        self.fft.process(&mut buf);

        let mut out = vec![0.0f32; N_BANDS];
        for (b, &(lo, hi)) in self.band_bins.iter().enumerate() {
            let mut power = 0.0f32;
            for c in &buf[lo..hi] {
                power += c.re * c.re + c.im * c.im;
            }
            let avg = power / (hi - lo) as f32; // 每 bin 平均功率（抹平不同频段 bin 数差异）
            let db = 10.0 * (avg + 1e-12).log10();
            // 自适应噪声门：低于门→快速跟低(0.3)；高于门→极慢回升(0.002)。
            // 于是它锁住安静基线、不被语音峰值抬走；说话时 db 远高于门 → 条窜起来。
            let f = &mut self.floor[b];
            *f += (if db < *f { 0.3 } else { 0.002 }) * (db - *f);
            let norm = ((db - *f - 4.0) / 34.0).clamp(0.0, 1.0); // 门上 4dB 起跳、34dB 到顶
            out[b] = norm.powf(0.76); // 感知曲线（指数越低波动越夸张；0.76 比 0.7 收一点）
        }

        // 跨频段轻度平滑，去单段毛刺、让轮廓更顺
        let raw = out.clone();
        for i in 0..N_BANDS {
            let l = raw[i.saturating_sub(1)];
            let r = raw[(i + 1).min(N_BANDS - 1)];
            out[i] = raw[i] * 0.6 + (l + r) * 0.2;
        }
        out
    }
}
