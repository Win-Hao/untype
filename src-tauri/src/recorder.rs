//! 麦克风录音：cpal 在独立线程上采集，把样本不断 append 到一个共享缓冲。
//! 处理管线（pipeline）会持续把缓冲排空、处理后丢弃，从而恒定内存、支持长录音。
//!
//! cpal::Stream 在 macOS 上 !Send，故录音线程独占它整个生命周期，
//! 主线程只通过原子标志通知停止。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// 交给处理管线的录音句柄：共享缓冲、配置、停止标志。
pub struct RecordingHandle {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub config: Arc<Mutex<Option<(u32, u16)>>>, // (sample_rate, channels)，流打开后由录音线程写入
    pub stop: Arc<AtomicBool>,
    /// 录音线程打开设备 / 建流失败时置 true（并连带置 stop），让管线能报错收尾而非空转。
    pub failed: Arc<AtomicBool>,
    /// 用户按 Esc 取消：管线读到后丢弃识别结果、不出稿。置 cancel 时连带置 stop。
    pub cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Recorder {
    active: Option<Active>,
}

struct Active {
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 开始录音；返回供管线消费的句柄。已在录音则报错。
    pub fn start(&mut self, device_name: Option<String>) -> Result<RecordingHandle, String> {
        if self.active.is_some() {
            return Err("已在录音".to_string());
        }
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let config = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let (b, c, s, f) = (buffer.clone(), config.clone(), stop.clone(), failed.clone());
        let handle = std::thread::spawn(move || capture(b, c, s, f, device_name));
        self.active = Some(Active {
            stop: stop.clone(),
            cancel: cancel.clone(),
            handle,
        });
        Ok(RecordingHandle {
            buffer,
            config,
            stop,
            failed,
            cancel,
        })
    }

    /// 停止录音并等待采集线程退出。
    pub fn stop(&mut self) {
        if let Some(a) = self.active.take() {
            a.stop.store(true, Ordering::SeqCst);
            let _ = a.handle.join();
        }
    }

    /// 取消录音：丢弃本次结果（不出稿）。先置 cancel 再置 stop，
    /// 保证管线读到 stop=true 退出循环时 cancel 已可见。
    pub fn cancel(&mut self) {
        if let Some(a) = self.active.take() {
            a.cancel.store(true, Ordering::SeqCst);
            a.stop.store(true, Ordering::SeqCst);
            let _ = a.handle.join();
        }
    }
}

/// 枚举可用的输入设备名（供 UI 选择麦克风）。
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(it) => it
            .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn capture(
    buffer: Arc<Mutex<Vec<f32>>>,
    config_out: Arc<Mutex<Option<(u32, u16)>>>,
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    device_name: Option<String>,
) {
    // 任一早退（设备 / 配置 / 建流失败）都先置 failed 再置 stop——先 failed 后 stop，
    // 保证管线读到 stop=true 时 failed 已可见，从而报错收尾而非空转、胶囊卡「录音中」。
    let fail = |msg: String| {
        eprintln!("{msg}");
        failed.store(true, Ordering::SeqCst);
        stop.store(true, Ordering::SeqCst);
    };
    // 优先用指定设备（按名匹配），找不到 / 未指定则回退系统默认
    let device = match find_input_device(device_name) {
        Some(d) => d,
        None => {
            fail("找不到麦克风输入设备".to_string());
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            fail(format!("获取输入配置失败: {e}"));
            return;
        }
    };

    let sample_rate = supported.sample_rate();
    let channels = supported.channels();
    *config_out.lock().unwrap() = Some((sample_rate, channels));

    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err_fn = |e| eprintln!("录音流错误: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let b = buffer.clone();
            device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    b.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let b = buffer.clone();
            device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    b.lock().unwrap().extend_from_slice(&crate::audio::i16_to_f32(data));
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let b = buffer.clone();
            device.build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mut g = b.lock().unwrap();
                    g.extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                },
                err_fn,
                None,
            )
        }
        other => {
            fail(format!("不支持的采样格式: {other:?}"));
            return;
        }
    };

    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            fail(format!("创建录音流失败: {e}"));
            return;
        }
    };
    if let Err(e) = stream.play() {
        fail(format!("启动录音流失败: {e}"));
        return;
    }

    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
}

/// 按名查找输入设备（None / 空白 = 系统默认）。录音与电平监听共用。
fn find_input_device(device_name: Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    device_name
        .filter(|n| !n.trim().is_empty())
        .and_then(|name| {
            host.input_devices().ok().and_then(|mut it| {
                it.find(|d| {
                    d.description()
                        .ok()
                        .map(|desc| name.as_str() == desc.name())
                        .unwrap_or(false)
                })
            })
        })
        .or_else(|| host.default_input_device())
}

/// 麦克风频谱监听器：选麦弹窗打开时临时开一个输入流做 FFT 频谱（与录音胶囊同款 viz），把 N_BANDS 段能量回报给上层（emit 给前端）。
/// 不录音、不写缓冲，关弹窗即停。cpal Stream 留在监听线程上（!Send 不外泄），故 Monitor 本身可跨线程持有。
#[derive(Default)]
pub struct Monitor {
    active: Option<MonitorActive>,
}

struct MonitorActive {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// 开始监听指定设备（None / 空 = 系统默认）；已在监听则先停旧的。
    /// `on_bands` 每 ~60ms 收到一次 N_BANDS 段频谱能量（各 0..1）。
    pub fn start<F>(&mut self, device_name: Option<String>, on_bands: F)
    where
        F: Fn(Vec<f32>) + Send + 'static,
    {
        self.stop();
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let handle = std::thread::spawn(move || monitor_loop(device_name, s, on_bands));
        self.active = Some(MonitorActive { stop, handle });
    }

    /// 停止监听并等监听线程退出。
    pub fn stop(&mut self) {
        if let Some(a) = self.active.take() {
            a.stop.store(true, Ordering::SeqCst);
            let _ = a.handle.join();
        }
    }
}

fn monitor_loop<F: Fn(Vec<f32>)>(device_name: Option<String>, stop: Arc<AtomicBool>, on_bands: F) {
    let zeros = || vec![0.0f32; crate::viz::N_BANDS];
    let device = match find_input_device(device_name) {
        Some(d) => d,
        None => {
            on_bands(zeros());
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("监听获取输入配置失败: {e}");
            return;
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let sample_rate = config.sample_rate;
    let channels = config.channels;
    let err_fn = |e| eprintln!("监听流错误: {e}");
    // 实时回调只把样本塞进缓冲（不在音频线程算 FFT）；轮询线程每 60ms 排空 → 一次频谱，与胶囊同款节奏。
    let buf = Arc::new(Mutex::new(Vec::<f32>::new()));

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let b = buf.clone();
            device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    b.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let b = buf.clone();
            device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    b.lock().unwrap().extend(crate::audio::i16_to_f32(data));
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let b = buf.clone();
            device.build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    b.lock()
                        .unwrap()
                        .extend(data.iter().map(|&s| (s as f32 - 32768.0) / 32768.0));
                },
                err_fn,
                None,
            )
        }
        other => {
            eprintln!("监听不支持的采样格式: {other:?}");
            return;
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            eprintln!("创建监听流失败: {e}");
            return;
        }
    };
    if let Err(e) = stream.play() {
        eprintln!("启动监听流失败: {e}");
        return;
    }
    let mut viz = crate::viz::SpectrumViz::new(sample_rate); // 与胶囊同款频谱
    while !stop.load(Ordering::SeqCst) {
        // 排空累积音频做一次 FFT；静默期设备仍持续供帧、raw 非空 → 噪声门自校准到近零。
        let raw: Vec<f32> = std::mem::take(&mut *buf.lock().unwrap());
        on_bands(if raw.is_empty() { zeros() } else { viz.bands(&raw, channels) });
        std::thread::sleep(Duration::from_millis(60));
    }
    drop(stream);
    on_bands(zeros()); // 收尾归零
}
