#!/usr/bin/env bash
# 本地快速构建：产出 target/release/urlbox
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

export PATH="$HOME/.cargo/bin:$PATH"

# 国内网络较慢时可 CARGO_MIRROR=1 走 rsproxy 镜像（切换源会触发一次全量重编译）
CARGO_ARGS=()
if [ "${CARGO_MIRROR:-0}" = "1" ]; then
  CARGO_ARGS=(--config "$ROOT/.cargo/mirror.toml")
fi

FEATURES=()
if [ "${WITH_DEVTOOLS:-0}" = "1" ]; then
  FEATURES=(--features devtools)
fi

echo "==> cargo build --release ${FEATURES[*]:-}"
cargo build --release ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} ${FEATURES[@]+"${FEATURES[@]}"}

BIN="$ROOT/target/release/urlbox"
echo "==> 产物: $BIN"
ls -lh "$BIN"
echo "==> 试运行: $BIN --url https://vi.51pwn.com/"
