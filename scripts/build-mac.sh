#!/usr/bin/env bash
# macOS 本地打包。
# 若存在 scripts/signing.local.env（不提交，含你的 Developer ID 证书名），用它签名
#   → 辅助功能授权在版本更新后保留；否则回退 tauri.conf 的 ad-hoc 签名（任何人都能 build）。
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -f scripts/signing.local.env ]; then
  set -a; . scripts/signing.local.env; set +a
  echo "→ 用证书签名: ${APPLE_SIGNING_IDENTITY:-(未设)}"
else
  echo "→ 无 scripts/signing.local.env，回退 ad-hoc 签名"
fi
exec npm run tauri build "$@"
