<div align="center">

# Untype

**轻量「忠实听写」语音输入工具 · macOS**
<br/>
<sub><i>A lightweight, faithful speech-to-text input tool for macOS.</i></sub>

[![CI](https://github.com/Win-Hao/untype/actions/workflows/ci.yml/badge.svg)](https://github.com/Win-Hao/untype/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/Win-Hao/untype?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/Win-Hao/untype?style=flat-square&label=release)](https://github.com/Win-Hao/untype/releases)
![Platform](https://img.shields.io/badge/platform-macOS-111?style=flat-square)
![Built with](https://img.shields.io/badge/built%20with-Tauri%202%20·%20Rust%20·%20Svelte-24C8DB?style=flat-square)

</div>

## 是什么

Untype 是一个常驻菜单栏的语音输入工具：**按住（或双击）热键说话，松手即把识别 + 轻整理后的文字注入到当前光标处**。定位像系统语音输入法那样轻、快、忠实——AI 只把口语清理成通顺文字，**不增删、不回答、不编造**。

- 🎙️ **本地离线识别** — 内置 SenseVoice，离线、免费、隐私（音频不出本机）。
- ☁️ **可选云端流式识别** — 火山豆包 / 阿里 Qwen3，边录边出（自带 Key，BYOK）。
- ✍️ **可选 AI 轻整理** — 接任意 OpenAI 兼容 LLM（默认智谱 glm-4-flash），去口癖、补标点，不改原意。
- ⌨️ **按住 / 双击两种触发** — 双击模式回车确认、Esc 取消。
- 🫧 **录音胶囊** — 悬浮胶囊显示实时频谱波形与处理状态，极简不打扰。
- 📖 **同音字纠错词典** — 自定义热词 / 纠错，提升专有名词准确率。

> 🔒 **隐私**：本地识别音频不出本机；启用云端识别 / AI 整理时，仅把音频或文本发往**你自己配置**的服务商。所有 API Key 仅以 `0600` 权限存于本地，不入库、不上传。

## 安装

> macOS 应用未做苹果签名 / 公证（开源自构建项目）。首次打开若被 Gatekeeper 拦下：**右键 →「打开」**，或终端执行 `xattr -dr com.apple.quarantine /Applications/Untype.app`。

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

- **应用内检查更新**：设置 →「关于」会查最新 release，有新版即提示并一键跳转下载页。
- 点本仓库右上角 **Watch → Custom → ✅ Releases**：发布新版时 GitHub 会邮件 / 站内通知你。
- 或订阅 RSS：`https://github.com/Win-Hao/untype/releases.atom`。

## 配置

- **识别引擎**：默认本地 SenseVoice，开箱即用；想更快可在设置切到云端（填火山或阿里 Key）。
- **AI 整理**（可选）：设置里填 OpenAI 兼容 LLM 的 base_url + Key + 模型；默认智谱 glm-4-flash（快、有免费额度）。
- **热键 / 麦克风 / 词典**：均在设置面板。

## 开发

技术栈 **Tauri 2 + Rust + Svelte 5**。

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
