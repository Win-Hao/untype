<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="branding/untype-light.svg">
  <img src="branding/untype-dark.svg" alt="Untype" width="120" height="120">
</picture>

# Untype

**Faithful, lightweight voice input · macOS**

Hold (or double-tap) a hotkey and speak; release, and your words land at the cursor — transcribed and lightly tidied.<br/>
The AI is just a typist: it cleans up your speech, but **never adds, answers, or invents**.

<sub><i>轻量「忠实听写」语音输入工具 · macOS。按住热键说话，松手即出文字。</i></sub>

<br/>

[![CI](https://github.com/Win-Hao/untype/actions/workflows/ci.yml/badge.svg)](https://github.com/Win-Hao/untype/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/Win-Hao/untype?style=flat-square)](LICENSE)
[![Release](https://img.shields.io/github/v/release/Win-Hao/untype?style=flat-square&label=release)](https://github.com/Win-Hao/untype/releases)
[![Stars](https://img.shields.io/github/stars/Win-Hao/untype?style=flat-square)](https://github.com/Win-Hao/untype/stargazers)
![Platform](https://img.shields.io/badge/platform-macOS-111?style=flat-square)
![Built with](https://img.shields.io/badge/built%20with-Tauri%202%20·%20Rust%20·%20Svelte-24C8DB?style=flat-square)

[简体中文](README.md) · **English**

</div>

## What is it

Untype is a menu-bar voice-input tool: **hold or double-tap a hotkey, speak, release — and the text appears** at your cursor. It's designed to feel like a system dictation method: light, fast, and faithful. It turns what you said into clean text without getting in the way.

- 🎯 **Faithful — no rewriting.** The AI only removes filler words, adds punctuation, and smooths word order. It **won't add content, answer you, or "improve" your sentences.**
- 🔒 **Local & private.** Built-in SenseVoice runs offline and free; your audio never leaves the machine. API keys are stored locally with `0600` permissions only.
- ⚡ **Fast, low-latency.** A lightweight model by default, with optional cloud streaming — speak and it appears, just like an input method.
- 🪶 **Light & always-on.** One small menu-bar icon. Install and go, with near-zero configuration.

> Also includes: optional cloud streaming ASR (Volcengine Doubao / Alibaba Qwen3, BYOK), hold-to-talk or double-tap triggers (Enter to confirm, Esc to cancel in double-tap mode), a floating "recording capsule" with a live spectrum, and a homophone-correction dictionary.

## A concrete example

You say into the mic (with fillers and false starts):

> Um… so, is our meeting tomorrow, uh, moved to 3 p.m. now?

Untype types at your cursor:

> Is our meeting tomorrow moved to 3 p.m.?

It **dropped** the fillers ("um… so… uh") and smoothed the phrasing — but it **did not** answer the question, add anything you didn't say, or "rewrite it into a nicer sentence." That's faithful dictation: the AI is a typist, not an author.

<!-- TODO demo: this is the best spot for a demo GIF or screenshots (recording capsule + settings panel) — just drag an image in and reference it here. Biggest first-impression boost. -->

## How it compares

Tools like Typeless, Wispr Flow, and Superwhisper are polished, but most are **closed-source and subscription-based**, and often cloud-first. Untype aims to be their **open-source, local-first, faithful** alternative — free, private, and light.

|  | **Untype** | **Paid dictation tools**<br/>(Typeless / Wispr Flow / Superwhisper, etc.) |
| --- | --- | --- |
| Price | Free & open-source (MIT) | Subscription |
| Source | Open, auditable | Mostly closed |
| Offline | Built-in local engine; works out of the box, audio can stay on device | Partial; mostly cloud-leaning |
| Philosophy | Faithful — tidies, never rewrites | Often adds polish / rewriting / command modes |
| Footprint | Light macOS menu-bar app, near-zero config | Varies; heavier, more features |

> In short: if you want a dictation tool that's **free, open-source, works offline, and stays faithful to what you actually said** — that's exactly what Untype is for.

> 🔒 **Privacy:** with local recognition, audio never leaves your machine. When you enable cloud recognition / AI tidy-up, audio or text is sent only to **the provider you configured yourself**. All API keys are stored locally with `0600` permissions — never committed, never uploaded.

## Install

> The macOS app is not Apple-signed / notarized (it's an open-source, self-built project). If Gatekeeper blocks the first launch: **right-click → "Open"**, or run `xattr -dr com.apple.quarantine /Applications/Untype.app` in the terminal.

**Option 1 · Download a release:** grab the latest `.dmg` from [Releases](https://github.com/Win-Hao/untype/releases) and drag it into Applications.

**Option 2 · Build from source:**

```bash
git clone https://github.com/Win-Hao/untype.git
cd untype
npm install
npm run tauri build      # downloads the local ASR model (~228MB) on first run; output in src-tauri/target/release/bundle/
```

On first launch the app requests **Microphone** and **Accessibility** permissions (the latter is needed to inject text). Click "Allow".

## Staying updated

- **In-app update check:** Settings → "About" checks for the latest release and offers a one-click jump to the download page.
- **Watch → Custom → ✅ Releases:** GitHub will email / notify you when a new version ships.
- **RSS:** subscribe to `https://github.com/Win-Hao/untype/releases.atom`.

## Configuration

- **Recognition engine:** local SenseVoice by default (works out of the box); switch to cloud in Settings for more speed (add a Volcengine or Alibaba key).
- **AI tidy-up** (optional): add the base_url + key + model of any OpenAI-compatible LLM; defaults to Zhipu glm-4-flash (fast, with a free tier).
- **Hotkey / microphone / dictionary:** all in the Settings panel.

## Development

Built with **Tauri 2 + Rust + Svelte 5**.

```bash
npm install
npm run tauri dev        # dev run (downloads the local ASR model ~228MB on first run)
```

On the first build, `build.rs` automatically fetches the local recognition models (SenseVoice int8 + Silero VAD, ~228MB) into `src-tauri/models/sense-voice/`. If HuggingFace is slow for you, use a mirror: `export HF_ENDPOINT=https://hf-mirror.com` before building (or run `bash scripts/fetch-models.sh` manually). Already have the models? Set `UNTYPE_SKIP_MODEL_DOWNLOAD=1` to skip.

Checks: frontend `npm run check`; for Rust, run `cargo check` / `cargo clippy` in `src-tauri/`.

## Philosophy

**Faithful dictation**, like a system input method — transcription should be fast, accurate, and true to what you said. The AI is a typist, not an author.

## License

[MIT](LICENSE) © 2026 Win-Hao
