#!/usr/bin/env bash
# 下载本地 ASR 所需模型到 src-tauri/models/sense-voice/（SenseVoice int8 + tokens + Silero VAD，约 228MB）。
# 幂等：已存在的文件跳过。build.rs 在模型缺失时自动调用；也可手动 `bash scripts/fetch-models.sh`。
#
# 国内访问 HuggingFace 慢，可设镜像：
#   HF_ENDPOINT=https://hf-mirror.com bash scripts/fetch-models.sh
# 自定义落地目录（测试用）：MODELS_DIR=/tmp/x bash scripts/fetch-models.sh
set -euo pipefail

HF="${HF_ENDPOINT:-https://huggingface.co}"
SV_REPO="csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
GH="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="${MODELS_DIR:-$ROOT/src-tauri/models/sense-voice}"
mkdir -p "$DIR"

fetch() { # fetch <url> <dest>
  if [ -s "$2" ]; then echo "✓ 已存在，跳过 $(basename "$2")"; return 0; fi
  echo "↓ 下载 $(basename "$2") …"
  curl -fL --retry 3 --progress-bar -o "$2.tmp" "$1"
  mv "$2.tmp" "$2"
}

# 先小后大：tokens / VAD 秒下，最后才是 228MB 的主模型
fetch "$HF/$SV_REPO/resolve/main/tokens.txt"      "$DIR/tokens.txt"
fetch "$GH/silero_vad.onnx"                        "$DIR/silero_vad.onnx"
fetch "$HF/$SV_REPO/resolve/main/model.int8.onnx" "$DIR/model.int8.onnx"

echo "✓ 模型就绪：$DIR"
