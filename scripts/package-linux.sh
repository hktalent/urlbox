#!/usr/bin/env bash
# 生成 dist/urlbox-linux-x86_64.tar.gz（含二进制、desktop 文件、图标）
# 依赖: 见 README「Linux 构建前置」
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

CARGO_ARGS=()
if [ "${CARGO_MIRROR:-0}" = "1" ]; then
  CARGO_ARGS=(--config "$ROOT/.cargo/mirror.toml")
fi

cargo build --release ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"}

STAGE="$ROOT/dist/urlbox"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/icons/hicolor/256x256/apps"

cp target/release/urlbox "$STAGE/bin/urlbox"
cp packaging/linux/urlbox.desktop "$STAGE/share/applications/urlbox.desktop"
[ -f assets/icon.png ] && cp assets/icon.png "$STAGE/share/icons/hicolor/256x256/apps/urlbox.png"

cat > "$STAGE/install.sh" <<'EOS'
#!/usr/bin/env bash
# 安装到当前用户目录（无需 root）
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
PREFIX="${PREFIX:-$HOME/.local}"
install -Dm755 "$HERE/bin/urlbox" "$PREFIX/bin/urlbox"
install -Dm644 "$HERE/share/applications/urlbox.desktop" "$PREFIX/share/applications/urlbox.desktop"
if [ -f "$HERE/share/icons/hicolor/256x256/apps/urlbox.png" ]; then
  install -Dm644 "$HERE/share/icons/hicolor/256x256/apps/urlbox.png" \
    "$PREFIX/share/icons/hicolor/256x256/apps/urlbox.png"
fi
echo "已安装到 $PREFIX/bin/urlbox"
echo "如需系统级安装: sudo PREFIX=/usr/local $0"
EOS
chmod +x "$STAGE/install.sh"

cat > "$STAGE/README.txt" <<'EOS'
urlbox —— 极简跨平台桌面外壳
运行依赖: GTK3 + WebKitGTK 4.1（Ubuntu 22.04+/Debian 12+/Fedora 38+ 通常已具备）
  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0
安装: ./install.sh
运行: urlbox https://vi.51pwn.com/
EOS

ARCH=$(uname -m)
if [ -n "${VERSION:-}" ]; then
  TAR="$ROOT/dist/urlbox-${VERSION#v}-linux-$ARCH.tar.gz"
else
  TAR="$ROOT/dist/urlbox-linux-$ARCH.tar.gz"
fi
rm -f "$TAR"
tar -C "$ROOT/dist" -czf "$TAR" urlbox
echo "==> 完成: $TAR"
ls -lh "$TAR"
