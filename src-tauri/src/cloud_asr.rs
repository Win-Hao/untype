//! 云端语音识别（WebSocket）：火山豆包「大模型流式语音识别」 + 阿里 DashScope（Qwen3-ASR-Flash / Fun-ASR）。
//!
//! BYOK 可选引擎——默认仍走本地 SenseVoice，用户在设置里切到云端并填对应厂商 API Key 后生效。
//! 火山协议见 `notes/asr-demo/火山-doubao-流式ASR-协议参考.md`（4 字节头 + payload_size(大端) + gzip 负载）。
//! 词典软词作为热词随请求传入（解码时偏置，仅火山支持），ITN/标点由云端开启（输出比本地更干净）。

use std::io::{Read, Write};

use base64::Engine as _;
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::{HeaderName, HeaderValue};
use tungstenite::Message;

const ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel";
/// 豆包流式语音识别 2.0 小时版（免费额度走这个）。
pub const DEFAULT_RESOURCE_ID: &str = "volc.seedasr.sauc.duration";

fn gzip(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).map_err(|e| e.to_string())?;
    e.finish().map_err(|e| e.to_string())
}

fn gunzip(data: &[u8]) -> Result<Vec<u8>, String> {
    // 限制解压上限防 zip-bomb：ASR 结果 JSON 很小，8 MiB 绰绰有余
    //（tungstenite 另有 64 MiB 帧上限兜底；超限则截断 → 后续 JSON 解析失败报错，不会 OOM）。
    const MAX_INFLATED: u64 = 8 << 20;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take(MAX_INFLATED)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// 4 字节头：version=1, header_size=1。
fn header(msg_type: u8, flags: u8, serialization: u8, compression: u8) -> [u8; 4] {
    [
        (0b0001 << 4) | 0b0001,
        (msg_type << 4) | flags,
        (serialization << 4) | compression,
        0x00,
    ]
}

fn frame(h: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(8 + payload.len());
    f.extend_from_slice(&h);
    f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    f.extend_from_slice(payload);
    f
}

/// full client request：type=0x1, JSON, gzip。
fn full_client_request(config_json: &str) -> Result<Vec<u8>, String> {
    let payload = gzip(config_json.as_bytes())?;
    Ok(frame(header(0b0001, 0b0000, 0b0001, 0b0001), &payload))
}

/// audio only：type=0x2, raw, gzip；末包 flags=0x2。
fn audio_request(chunk: &[u8], last: bool) -> Result<Vec<u8>, String> {
    let payload = gzip(chunk)?;
    Ok(frame(header(0b0010, if last { 0b0010 } else { 0b0000 }, 0b0000, 0b0001), &payload))
}

/// 解析服务端帧：返回识别文本（若该帧带 result.text）；错误帧 → Err。
fn parse_response(data: &[u8]) -> Result<Option<String>, String> {
    if data.len() < 4 {
        return Ok(None);
    }
    let msg_type = (data[1] >> 4) & 0x0F;
    let flags = data[1] & 0x0F;
    let compression = data[2] & 0x0F;
    let mut body = &data[4..];

    if msg_type == 0b1001 {
        // full server response：flags 含 sequence 时先跳过 4 字节
        if (flags == 0b0001 || flags == 0b0011) && body.len() >= 4 {
            body = &body[4..];
        }
        if body.len() < 4 {
            return Ok(None);
        }
        let size = u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize;
        let end = (4 + size).min(body.len());
        let raw = &body[4..end];
        let json = if compression == 0b0001 { gunzip(raw)? } else { raw.to_vec() };
        let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        let text = v
            .get("result")
            .and_then(|r| r.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        Ok(text)
    } else if msg_type == 0b1111 {
        // error：code(4) + size(4) + msg
        if body.len() < 8 {
            return Err("火山返回错误（无详情）".to_string());
        }
        let code = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
        let size = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
        let end = (8 + size).min(body.len());
        Err(format!("火山错误 code={code}: {}", String::from_utf8_lossy(&body[8..end])))
    } else {
        Ok(None)
    }
}

fn connect_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}")
}

/// 转写 16kHz 单声道 f32 采样。`hotwords` 作为解码偏置（词典软词）。
/// 返回云端识别文本（已含标点/ITN）。失败返回 Err（上层应回退/提示）。
pub fn transcribe_volc(
    api_key: &str,
    resource_id: &str,
    hotwords: &[String],
    sample_rate: u32,
    samples: &[f32],
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("未配置火山 API Key".to_string());
    }
    let pcm = pcm16le(samples); // f32 → 16-bit PCM (little-endian)

    let hot = serde_json::json!({
        "hotwords": hotwords.iter().map(|w| serde_json::json!({ "word": w })).collect::<Vec<_>>()
    })
    .to_string();
    let config = serde_json::json!({
        "user": { "uid": "voicetotext" },
        "audio": { "format": "pcm", "rate": sample_rate, "bits": 16, "channel": 1, "language": "zh-CN" },
        "request": {
            "model_name": "bigmodel",
            "enable_itn": true,
            "enable_punc": true,
            "enable_ddc": false,
            "corpus": { "context": hot }
        }
    })
    .to_string();

    let mut req = ENDPOINT
        .into_client_request()
        .map_err(|e| format!("构造请求失败: {e}"))?;
    {
        let h = req.headers_mut();
        let mut put = |k: &'static str, v: &str| -> Result<(), String> {
            h.insert(
                HeaderName::from_static(k),
                HeaderValue::from_str(v).map_err(|_| format!("无效请求头 {k}"))?,
            );
            Ok(())
        };
        put("x-api-key", api_key)?;
        put("x-api-resource-id", resource_id)?;
        put("x-api-connect-id", &connect_id())?;
    }

    let (mut socket, _resp) =
        tungstenite::connect(req).map_err(|e| format!("火山连接失败（检查 Key/额度）: {e}"))?;

    socket
        .send(Message::Binary(full_client_request(&config)?))
        .map_err(|e| format!("发送配置失败: {e}"))?;
    read_text(&mut socket)?; // 对 full client request 的应答（通常空）

    let step = (sample_rate as usize / 5) * 2; // 200ms * 2 字节/采样（单声道）
    let mut text = String::new();
    let mut i = 0;
    while i < pcm.len() {
        let end = (i + step).min(pcm.len());
        let last = end >= pcm.len();
        socket
            .send(Message::Binary(audio_request(&pcm[i..end], last)?))
            .map_err(|e| format!("发送音频失败: {e}"))?;
        if let Some(t) = read_text(&mut socket)? {
            if !t.is_empty() {
                text = t;
            }
        }
        i = end;
    }
    let _ = socket.close(None);
    Ok(text)
}

/// 读到下一个二进制帧并解析（跳过 ping/pong/text）。
fn read_text<S: Read + Write>(
    socket: &mut tungstenite::WebSocket<S>,
) -> Result<Option<String>, String> {
    loop {
        match socket.read().map_err(|e| format!("读取响应失败: {e}"))? {
            Message::Binary(d) => return parse_response(d.as_ref()),
            Message::Close(_) => return Ok(None),
            _ => continue,
        }
    }
}

// ---- 阿里 DashScope（百炼）Fun-ASR 实时识别 ----
// JSON 控制帧（run-task/task-started/result-generated/finish-task）+ 二进制音频，bearer 鉴权。

const ALI_ENDPOINT: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";
/// Fun-ASR 实时模型（中英 + 多方言）。
pub const ALI_DEFAULT_MODEL: &str = "fun-asr-realtime";

/// f32 采样 → 16-bit PCM (little-endian) 字节。
fn pcm16le(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|&s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect()
}

/// 读文本事件直到出现 `want`；遇 task-failed 报错。
fn ali_wait_event<S: Read + Write>(
    socket: &mut tungstenite::WebSocket<S>,
    want: &str,
) -> Result<(), String> {
    loop {
        match socket.read().map_err(|e| format!("读取失败: {e}"))? {
            Message::Text(t) => {
                let s = t.to_string();
                let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                match v["header"]["event"].as_str().unwrap_or("") {
                    ev if ev == want => return Ok(()),
                    "task-failed" => return Err(format!("阿里 task-failed: {s}")),
                    _ => continue,
                }
            }
            Message::Close(_) => return Err("连接被关闭".to_string()),
            _ => continue,
        }
    }
}

/// 转写 16kHz 单声道 f32 采样（阿里 DashScope Fun-ASR）。
/// **仅离线评测用**（eval_clip / ignored 测试）：主路径云端阿里固定走 Qwen3（更准），
/// 不再分派到此。Fun-ASR 实时不支持内联热词，靠模型本身 + 下游词典纠错；返回文本已含标点。
pub fn transcribe_ali(api_key: &str, sample_rate: u32, samples: &[f32]) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("未配置阿里 DashScope API Key".to_string());
    }
    let pcm = pcm16le(samples);
    // DashScope task_id：32 位十六进制、无连字符（对齐官方 uuid4().hex[:32]）。
    let tid = connect_id();

    let mut req = ALI_ENDPOINT
        .into_client_request()
        .map_err(|e| format!("构造请求失败: {e}"))?;
    req.headers_mut().insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_str(&format!("bearer {api_key}")).map_err(|_| "无效 API Key")?,
    );
    let (mut socket, _) =
        tungstenite::connect(req).map_err(|e| format!("阿里连接失败（检查 Key/额度）: {e}"))?;

    let run = serde_json::json!({
        "header": { "action": "run-task", "task_id": tid, "streaming": "duplex" },
        "payload": {
            "task_group": "audio", "task": "asr", "function": "recognition",
            "model": ALI_DEFAULT_MODEL,
            "parameters": { "format": "pcm", "sample_rate": sample_rate, "language_hints": ["zh", "en"] },
            "input": {}
        }
    })
    .to_string();
    socket
        .send(Message::Text(run))
        .map_err(|e| format!("发送 run-task 失败: {e}"))?;
    ali_wait_event(&mut socket, "task-started")?;

    // 发送音频（二进制，100ms/包）
    let step = ((sample_rate as usize / 10) * 2).max(1);
    for chunk in pcm.chunks(step) {
        socket
            .send(Message::Binary(chunk.to_vec()))
            .map_err(|e| format!("发送音频失败: {e}"))?;
    }

    let fin = serde_json::json!({
        "header": { "action": "finish-task", "task_id": tid, "streaming": "duplex" },
        "payload": { "input": {} }
    })
    .to_string();
    socket
        .send(Message::Text(fin))
        .map_err(|e| format!("发送 finish-task 失败: {e}"))?;

    // 收结果直到 task-finished
    let mut finalized = String::new();
    let mut partial = String::new();
    loop {
        match socket.read().map_err(|e| format!("读取结果失败: {e}"))? {
            Message::Text(t) => {
                let s = t.to_string();
                let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                match v["header"]["event"].as_str().unwrap_or("") {
                    "result-generated" => {
                        let sent = &v["payload"]["output"]["sentence"];
                        let text = sent["text"].as_str().unwrap_or("");
                        if sent["sentence_end"].as_bool().unwrap_or(false) {
                            finalized.push_str(text);
                            partial.clear();
                        } else {
                            partial = text.to_string();
                        }
                    }
                    "task-finished" => break,
                    "task-failed" => return Err(format!("阿里 task-failed: {s}")),
                    _ => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = socket.close(None);
    Ok(finalized + &partial)
}

// ---- 阿里 DashScope Qwen3-ASR-Flash 实时识别 ----
// 类 OpenAI Realtime 协议：session.update / input_audio_buffer.append(base64) / session.finish。
// 与 Fun-ASR 不同：model 在 URL query、大写 Bearer + OpenAI-Beta 头、音频走 base64 JSON 帧。

const QWEN_ENDPOINT: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/realtime";
/// 千问3-ASR-Flash 实时模型。
pub const QWEN_DEFAULT_MODEL: &str = "qwen3-asr-flash-realtime";

/// 读文本事件直到 `type == want`；遇 error 报错。
fn qwen_wait_event<S: Read + Write>(
    socket: &mut tungstenite::WebSocket<S>,
    want: &str,
) -> Result<(), String> {
    loop {
        match socket.read().map_err(|e| format!("读取失败: {e}"))? {
            Message::Text(t) => {
                let s = t.to_string();
                let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                match v["type"].as_str().unwrap_or("") {
                    ev if ev == want => return Ok(()),
                    "error" => return Err(format!("阿里 Qwen 错误: {s}")),
                    _ => continue,
                }
            }
            Message::Close(_) => return Err("连接被关闭".to_string()),
            _ => continue,
        }
    }
}

/// 转写 16kHz 单声道 f32 采样（阿里 DashScope Qwen3-ASR-Flash，VAD 模式）。
/// 大模型 ASR，中英 / 技术词更强；返回文本已含标点。
pub fn transcribe_qwen(api_key: &str, sample_rate: u32, samples: &[f32]) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("未配置阿里 DashScope API Key".to_string());
    }
    let pcm = pcm16le(samples);

    let url = format!("{QWEN_ENDPOINT}?model={QWEN_DEFAULT_MODEL}");
    let mut req = url
        .into_client_request()
        .map_err(|e| format!("构造请求失败: {e}"))?;
    {
        let h = req.headers_mut();
        h.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|_| "无效 API Key")?,
        );
        h.insert(
            HeaderName::from_static("openai-beta"),
            HeaderValue::from_static("realtime=v1"),
        );
    }
    let (mut socket, _) =
        tungstenite::connect(req).map_err(|e| format!("阿里连接失败（检查 Key/额度）: {e}"))?;

    // 会话配置：Manual 模式（turn_detection=null）——批量整段上传、显式 commit 断句，
    // 不依赖服务端 VAD 检测 speech_started（瞬间灌音频 VAD 检不到会直接空结果）。
    let session = serde_json::json!({
        "event_id": "cfg",
        "type": "session.update",
        "session": {
            "modalities": ["text"],
            "input_audio_format": "pcm",
            "sample_rate": sample_rate,
            "turn_detection": null
        }
    })
    .to_string();
    socket
        .send(Message::Text(session))
        .map_err(|e| format!("发送 session.update 失败: {e}"))?;
    qwen_wait_event(&mut socket, "session.created")?;

    // 发送音频：每 100ms 一包 PCM16 → base64 → JSON append（批量快发，靠 TCP 背压）
    let step = ((sample_rate as usize / 10) * 2).max(1);
    for chunk in pcm.chunks(step) {
        let ev = serde_json::json!({
            "event_id": "a",
            "type": "input_audio_buffer.append",
            "audio": base64::engine::general_purpose::STANDARD.encode(chunk)
        })
        .to_string();
        socket
            .send(Message::Text(ev))
            .map_err(|e| format!("发送音频失败: {e}"))?;
    }

    // Manual 模式：提交整段音频缓冲 → 结束会话
    let commit =
        serde_json::json!({ "event_id": "commit", "type": "input_audio_buffer.commit" }).to_string();
    socket
        .send(Message::Text(commit))
        .map_err(|e| format!("发送 commit 失败: {e}"))?;
    let fin = serde_json::json!({ "event_id": "fin", "type": "session.finish" }).to_string();
    socket
        .send(Message::Text(fin))
        .map_err(|e| format!("发送 session.finish 失败: {e}"))?;

    // 累积所有 ...completed 的 transcript，遇 session.finished 结束
    let mut text = String::new();
    loop {
        match socket.read().map_err(|e| format!("读取结果失败: {e}"))? {
            Message::Text(t) => {
                let s = t.to_string();
                let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| e.to_string())?;
                match v["type"].as_str().unwrap_or("") {
                    "conversation.item.input_audio_transcription.completed" => {
                        if let Some(tr) = v["transcript"].as_str() {
                            text.push_str(tr);
                        }
                    }
                    "session.finished" => break,
                    "error" => return Err(format!("阿里 Qwen 错误: {s}")),
                    _ => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = socket.close(None);
    Ok(text)
}

// ---- 流式会话：录音时就连上、边录边推音频，松手后 finish 取最终稿 ----
// 让云端识别与说话重叠，把「松手后才上传整段 + 等处理」的等待大幅压掉（火山随发随出；
// 阿里 Manual 模式至少让上传/握手与录音重叠）。刻意复用上面的纯帧助手，并保留批量版
// transcribe_volc/qwen 不动作为已验证回退（pipeline 改回调用它们即可还原旧行为）。
type CloudSock = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

/// 给已连上的 WS 底层 TCP 设读超时——否则服务端 TLS 握手后「假死」（不发帧也不关闭）会让
/// `socket.read()` 永久阻塞 worker 线程、进而让 pipeline 的 `worker.join()` 永久挂起、胶囊卡死。
/// 正常每包响应 <1s，20s 只在真·假死时触发 → 作错误返回、可恢复（而非永久卡住）。
fn set_read_timeout(socket: &mut CloudSock) {
    use tungstenite::stream::MaybeTlsStream;
    let t = Some(std::time::Duration::from_secs(20));
    match socket.get_mut() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(t);
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.get_mut().set_read_timeout(t);
        }
        _ => {}
    }
}

pub enum CloudSession {
    /// 火山豆包大模型流式：攒够 ~200ms 发一包并读累积文本（识别随发随出）。
    Volc {
        socket: CloudSock,
        text: String,
        step: usize,
        buf: Vec<u8>,
    },
    /// 阿里 Qwen3-ASR-Flash（Manual 模式）：base64 append 边录边发，commit/finish 在松手时。
    Qwen { socket: CloudSock },
}

impl CloudSession {
    pub fn start_volc(
        api_key: &str,
        resource_id: &str,
        hotwords: &[String],
        sample_rate: u32,
    ) -> Result<Self, String> {
        if api_key.is_empty() {
            return Err("未配置火山 API Key".to_string());
        }
        let hot = serde_json::json!({
            "hotwords": hotwords.iter().map(|w| serde_json::json!({ "word": w })).collect::<Vec<_>>()
        })
        .to_string();
        let config = serde_json::json!({
            "user": { "uid": "voicetotext" },
            "audio": { "format": "pcm", "rate": sample_rate, "bits": 16, "channel": 1, "language": "zh-CN" },
            "request": { "model_name": "bigmodel", "enable_itn": true, "enable_punc": true, "enable_ddc": false, "corpus": { "context": hot } }
        })
        .to_string();
        let mut req = ENDPOINT
            .into_client_request()
            .map_err(|e| format!("构造请求失败: {e}"))?;
        {
            let h = req.headers_mut();
            let mut put = |k: &'static str, v: &str| -> Result<(), String> {
                h.insert(
                    HeaderName::from_static(k),
                    HeaderValue::from_str(v).map_err(|_| format!("无效请求头 {k}"))?,
                );
                Ok(())
            };
            put("x-api-key", api_key)?;
            put("x-api-resource-id", resource_id)?;
            put("x-api-connect-id", &connect_id())?;
        }
        let (mut socket, _) =
            tungstenite::connect(req).map_err(|e| format!("火山连接失败（检查 Key/额度）: {e}"))?;
        set_read_timeout(&mut socket); // 防服务端假死把 worker/胶囊永久挂住
        socket
            .send(Message::Binary(full_client_request(&config)?))
            .map_err(|e| format!("发送配置失败: {e}"))?;
        read_text(&mut socket)?; // 配置应答（通常空）
        Ok(CloudSession::Volc {
            socket,
            text: String::new(),
            step: (sample_rate as usize / 5) * 2, // 200ms * 2 字节/采样
            buf: Vec::new(),
        })
    }

    pub fn start_qwen(api_key: &str, sample_rate: u32) -> Result<Self, String> {
        if api_key.is_empty() {
            return Err("未配置阿里 DashScope API Key".to_string());
        }
        let url = format!("{QWEN_ENDPOINT}?model={QWEN_DEFAULT_MODEL}");
        let mut req = url
            .into_client_request()
            .map_err(|e| format!("构造请求失败: {e}"))?;
        {
            let h = req.headers_mut();
            h.insert(
                HeaderName::from_static("authorization"),
                HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|_| "无效 API Key")?,
            );
            h.insert(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static("realtime=v1"),
            );
        }
        let (mut socket, _) =
            tungstenite::connect(req).map_err(|e| format!("阿里连接失败（检查 Key/额度）: {e}"))?;
        set_read_timeout(&mut socket); // 防服务端假死把 worker/胶囊永久挂住
        let session = serde_json::json!({
            "event_id": "cfg", "type": "session.update",
            "session": { "modalities": ["text"], "input_audio_format": "pcm", "sample_rate": sample_rate, "turn_detection": null }
        })
        .to_string();
        socket
            .send(Message::Text(session))
            .map_err(|e| format!("发送 session.update 失败: {e}"))?;
        qwen_wait_event(&mut socket, "session.created")?;
        Ok(CloudSession::Qwen { socket })
    }

    /// 推一段 16kHz 单声道 f32 采样（边录边发）。
    pub fn push(&mut self, samples_16k: &[f32]) -> Result<(), String> {
        match self {
            CloudSession::Volc { socket, text, step, buf } => {
                buf.extend_from_slice(&pcm16le(samples_16k));
                while buf.len() >= *step {
                    let chunk: Vec<u8> = buf.drain(..*step).collect();
                    socket
                        .send(Message::Binary(audio_request(&chunk, false)?))
                        .map_err(|e| format!("发送音频失败: {e}"))?;
                    if let Some(t) = read_text(socket)? {
                        if !t.is_empty() {
                            *text = t;
                        }
                    }
                }
                Ok(())
            }
            CloudSession::Qwen { socket } => {
                if samples_16k.is_empty() {
                    return Ok(());
                }
                let ev = serde_json::json!({
                    "event_id": "a", "type": "input_audio_buffer.append",
                    "audio": base64::engine::general_purpose::STANDARD.encode(pcm16le(samples_16k))
                })
                .to_string();
                socket
                    .send(Message::Text(ev))
                    .map_err(|e| format!("发送音频失败: {e}"))?;
                Ok(())
            }
        }
    }

    /// 松手后取最终稿。
    pub fn finish(mut self) -> Result<String, String> {
        match &mut self {
            CloudSession::Volc { socket, text, buf, .. } => {
                let tail = std::mem::take(buf); // 剩余不足一包的尾音作为末包（last=true）发出
                socket
                    .send(Message::Binary(audio_request(&tail, true)?))
                    .map_err(|e| format!("发送末包失败: {e}"))?;
                if let Some(t) = read_text(socket)? {
                    if !t.is_empty() {
                        *text = t;
                    }
                }
                let out = std::mem::take(text);
                let _ = socket.close(None);
                Ok(out)
            }
            CloudSession::Qwen { socket } => {
                let commit = serde_json::json!({ "event_id": "commit", "type": "input_audio_buffer.commit" })
                    .to_string();
                socket
                    .send(Message::Text(commit))
                    .map_err(|e| format!("发送 commit 失败: {e}"))?;
                let fin = serde_json::json!({ "event_id": "fin", "type": "session.finish" }).to_string();
                socket
                    .send(Message::Text(fin))
                    .map_err(|e| format!("发送 session.finish 失败: {e}"))?;
                let mut text = String::new();
                loop {
                    match socket.read().map_err(|e| format!("读取结果失败: {e}"))? {
                        Message::Text(m) => {
                            let v: serde_json::Value =
                                serde_json::from_str(&m.to_string()).map_err(|e| e.to_string())?;
                            match v["type"].as_str().unwrap_or("") {
                                "conversation.item.input_audio_transcription.completed" => {
                                    if let Some(tr) = v["transcript"].as_str() {
                                        text.push_str(tr);
                                    }
                                }
                                "session.finished" => break,
                                "error" => return Err(format!("阿里 Qwen 错误: {m}")),
                                _ => {}
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                let _ = socket.close(None);
                Ok(text)
            }
        }
    }

    /// 取消：直接关连接、不取结果。
    pub fn abort(self) {
        match self {
            CloudSession::Volc { mut socket, .. } => {
                let _ = socket.close(None);
            }
            CloudSession::Qwen { mut socket } => {
                let _ = socket.close(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_layout() {
        // full client request 头：0x11, 0x10, 0x11, 0x00
        let f = full_client_request("{}").unwrap();
        assert_eq!(&f[0..4], &[0x11, 0x10, 0x11, 0x00]);
        // payload_size 大端，且后面就是 gzip 数据（0x1f 0x8b 魔数）
        let size = u32::from_be_bytes([f[4], f[5], f[6], f[7]]) as usize;
        assert_eq!(f.len(), 8 + size);
        assert_eq!(&f[8..10], &[0x1f, 0x8b]);
    }

    #[test]
    fn audio_last_packet_flag() {
        let normal = audio_request(b"abc", false).unwrap();
        let last = audio_request(b"abc", true).unwrap();
        assert_eq!(normal[1], 0x20); // type=2, flags=0
        assert_eq!(last[1], 0x22); // type=2, flags=2(末包)
    }

    #[test]
    fn parse_error_frame() {
        // 构造一个错误帧：header(type=0xF) + code + size + msg
        let mut data = vec![0x11, 0xF0, 0x00, 0x00];
        data.extend_from_slice(&45000001u32.to_be_bytes());
        let msg = "bad".as_bytes();
        data.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        data.extend_from_slice(msg);
        let err = parse_response(&data).unwrap_err();
        assert!(err.contains("45000001"), "应含错误码: {err}");
    }

    /// 真集成测试：对 real-0618.wav 跑火山，断言含「幂等/列要点」。
    /// 需 ../.volc-creds.env 里的 VOLC_API_KEY + 网络；用 `cargo test -- --ignored` 跑。
    #[test]
    #[ignore = "需要火山 API Key（../.volc-creds.env）与网络"]
    fn transcribe_real_clip() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // 读 ../.volc-creds.env 拿 key
        let creds = std::fs::read_to_string(root.join("../.volc-creds.env")).expect("缺 .volc-creds.env");
        let key = creds
            .lines()
            .find_map(|l| l.trim().strip_prefix("VOLC_API_KEY="))
            .expect("缺 VOLC_API_KEY")
            .trim()
            .to_string();

        let wav = root.join("models/eval/wavs/real-0618.wav");
        let mut r = hound::WavReader::open(&wav).expect("打开 wav 失败");
        let pcm: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
        let samples = crate::audio::i16_to_f32(&pcm);

        let hot = ["并发".to_string(), "读写分离".to_string(), "列要点".to_string()];
        let text = transcribe_volc(&key, DEFAULT_RESOURCE_ID, &hot, 16000, &samples).expect("云端识别失败");
        eprintln!("【火山识别】{text}");
        assert!(!text.is_empty());
        assert!(text.contains("幂等") || text.contains("列要点"), "应识别出技术词: {text}");
    }

    /// 阿里 DashScope 真集成测试：需 ../.volc-creds.env 的 DASHSCOPE_API_KEY + 网络。
    #[test]
    #[ignore = "需要阿里 DashScope API Key（../.volc-creds.env 的 DASHSCOPE_API_KEY）与网络"]
    fn transcribe_ali_real_clip() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let creds = std::fs::read_to_string(root.join("../.volc-creds.env")).expect("缺 .volc-creds.env");
        let key = creds
            .lines()
            .find_map(|l| l.trim().strip_prefix("DASHSCOPE_API_KEY="))
            .expect("缺 DASHSCOPE_API_KEY")
            .trim()
            .to_string();
        let wav = root.join("models/eval/wavs/real-0618.wav");
        let mut r = hound::WavReader::open(&wav).expect("打开 wav 失败");
        let pcm: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
        let samples = crate::audio::i16_to_f32(&pcm);
        let text = transcribe_ali(&key, 16000, &samples).expect("阿里识别失败");
        eprintln!("【阿里 Fun-ASR 识别】{text}");
        assert!(!text.is_empty(), "阿里转写不应为空");
    }

    /// 阿里 Qwen3-ASR-Flash 真集成测试：需 ../.volc-creds.env 的 DASHSCOPE_API_KEY + 网络。
    #[test]
    #[ignore = "需要阿里 DashScope API Key（../.volc-creds.env 的 DASHSCOPE_API_KEY）与网络"]
    fn transcribe_qwen_real_clip() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let creds =
            std::fs::read_to_string(root.join("../.volc-creds.env")).expect("缺 .volc-creds.env");
        let key = creds
            .lines()
            .find_map(|l| l.trim().strip_prefix("DASHSCOPE_API_KEY="))
            .expect("缺 DASHSCOPE_API_KEY")
            .trim()
            .to_string();
        let wav = root.join("models/eval/wavs/real-0618.wav");
        let mut r = hound::WavReader::open(&wav).expect("打开 wav 失败");
        let pcm: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
        let samples = crate::audio::i16_to_f32(&pcm);
        let text = transcribe_qwen(&key, 16000, &samples).expect("Qwen 识别失败");
        eprintln!("【阿里 Qwen3 识别】{text}");
        assert!(!text.is_empty(), "Qwen 转写不应为空");
    }
}
