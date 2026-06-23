<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="branding/untype-light.svg">
  <img src="branding/untype-dark.svg" alt="Untype" width="120" height="120">
</picture>

# Untype

**轻量「忠实听写」语音输入 · macOS**

按住（或双击）热键说话，松手即把「识别 + 轻整理」后的文字注入到光标处。<br/>
AI 只是打字员——把口语理顺，**不增删、不回答、不编造**。

<sub><i>A lightweight, faithful speech-to-text input tool for macOS. Hold a key, speak, release.</i></sub>

<br/>

[![CI](https://github.com/Win-Hao/untype/actions/workflows/ci.yml/badge.svg)](https://github.com/Win-Hao/untype/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/Win-Hao/untype?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/Win-Hao/untype?style=flat-square&label=release)](https://github.com/Win-Hao/untype/releases)
[![Stars](https://img.shields.io/github/stars/Win-Hao/untype?style=flat-square)](https://github.com/Win-Hao/untype/stargazers)
![Platform](https://img.shields.io/badge/platform-macOS-111?style=flat-square)
![Built with](https://img.shields.io/badge/built%20with-Tauri%202%20·%20Rust%20·%20React-24C8DB?style=flat-square)

**简体中文** · [English](README.en.md)

</div>

## 是什么

Untype 是一个常驻菜单栏的语音输入工具：**按住或双击热键说话，松手即出文字**，落在当前光标处。定位像系统语音输入法那样轻、快、忠实——只把你说的话变成通顺文字，不打断工作流。

- 🎯 **忠实听写，不瞎改** — AI 只清理口癖、补标点、理顺语序，**不增删内容、不回答你说的话、不自作主张改写**。
- 🔒 **本地离线，重隐私** — 内置 SenseVoice 离线识别，免费、音频不出本机；API Key 仅以 `0600` 权限存本地。
- ⚡ **快，低延迟** — 默认走轻量模型，可选云端边录边出，像输入法一样即说即得。
- 🪶 **轻量常驻，开箱即用** — 一个菜单栏小图标，装好即用，几乎零配置。

> 还包含：可选云端流式识别（火山豆包 / 阿里 Qwen3，BYOK）、按住或双击两种触发（双击模式回车确认、Esc 取消）、悬浮「录音胶囊」实时频谱、同音字纠错词典。

## 一个例子

你对着麦克风说（带口癖、有卡顿）：

> 嗯…那个，我们明天的会是不是，呃，改到下午三点来着？

Untype 注入到光标处：

> 我们明天的会是不是改到下午三点？

它**去掉了**「嗯、那个、呃」这些口癖、理顺了语序——但**没有**回答你的问题、没有补充你没说的内容、也没有把它「改写成更漂亮的句子」。这就是「忠实听写」：AI 是打字员，不是作者。

<!-- TODO 演示：此处最适合放一段 demo GIF 或「录音胶囊 + 设置面板」截图（直接把图片拖进来引用即可），第一眼最抓人。 -->

## 它和别的工具不一样在哪

Typeless / Wispr Flow / Superwhisper 这类语音转写工具做得很精致，但多是**闭源 + 订阅收费**，识别也常以云端为主。Untype 想做的，是它们的**开源、本地优先、忠实**替代——免费、隐私、轻。

|  | **Untype** | **收费转写工具**<br/>（Typeless / Wispr Flow / Superwhisper 等） |
| --- | --- | --- |
| 价格 | 免费、开源（MIT） | 订阅收费 |
| 源码 | 开源可审计 | 多为闭源 |
| 本地离线 | 内置本地识别，开箱即用，音频可不出本机 | 部分支持，多数偏云端 |
| 理念 | 忠实听写，只理顺、不改写 | 常带润色 / 改写 / 指令增强 |
| 体量 | 轻量 macOS 菜单栏，几乎零配置 | 各异，功能更多更重 |

> 一句话：想要一个**免费、开源、能离线、且忠实于你原话**的听写工具，Untype 就是为此而生。

> 🔒 **隐私**：本地识别音频不出本机；启用云端识别 / AI 整理时，仅把音频或文本发往**你自己配置**的服务商。所有 API Key 仅以 `0600` 权限存于本地，不入库、不上传。

## 安装

> 发行版已做 Apple 签名 + 公证，下载后双击直接打开即可（无需右键或 `xattr`）。若自行从源码构建被 Gatekeeper 拦下，可 **右键 →「打开」**。

**方式一 · 下载发行版**：到 [Releases](https://github.com/Win-Hao/untype/releases) 下载最新 `.dmg`，拖入「应用程序」。

**方式二 · 源码构建**：

```bash
git clone https://github.com/Win-Hao/untype.git
cd untype
npm install
npm run tauri build      # 首次自动下载本地 ASR 模型 ~228MB；产物在 src-tauri/target/release/bundle/
```

首次启动会请求**麦克风**与**辅助功能**权限（注入文字用），按「允许」即可。

## 获取更新提醒

- **应用内自动更新**：有新版本时启动会自动提示，点「立即更新」即在应用内下载、安装并自动重启（也可在设置 →「关于」手动检查）。
- **Watch → Custom → ✅ Releases**：发布新版时 GitHub 会邮件 / 站内通知你。
- **RSS**：订阅 `https://github.com/Win-Hao/untype/releases.atom`。

## 配置

- **识别引擎**：默认本地 SenseVoice，开箱即用、离线也够快，只是准确率没那么高；想要更准（且更快）可在设置切到云端（填火山或阿里 Key）。
- **AI 整理**（可选）：设置里填 OpenAI 兼容 LLM 的 base_url + Key + 模型；默认智谱 glm-4-flash（快、有免费额度）。
- **热键 / 麦克风 / 词典**：均在设置面板。

## 开发

技术栈 **Tauri 2 + Rust + React 19 + Tailwind 4**。

```bash
npm install
npm run tauri dev        # 开发运行（首次自动下载本地 ASR 模型 ~228MB）
```

首次构建时 `build.rs` 会自动拉取本地识别模型（SenseVoice int8 + Silero VAD，约 228MB）到 `src-tauri/models/sense-voice/`。国内访问 HuggingFace 慢可走镜像：先 `export HF_ENDPOINT=https://hf-mirror.com` 再构建（或手动 `bash scripts/fetch-models.sh`）。已自备模型可设 `UNTYPE_SKIP_MODEL_DOWNLOAD=1` 跳过。

验证：前端 `npm run check`；Rust 在 `src-tauri/` 跑 `cargo check` / `cargo clippy`。

## 理念

像系统语音输入法那样的**忠实听写**——转写要快、要准、要忠于你说的话。AI 是打字员，不是作者。

## License

[MIT](LICENSE) © 2026 Win-Hao
