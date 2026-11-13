#!/usr/bin/env bash
# 生成 dist/UrlBox.app（可直接拖进"应用程序"）
# 用法:
#   ./scripts/package-macos.sh              # 只构建本机架构
#   UNIVERSAL=1 ./scripts/package-macos.sh  # 同时构建 x86_64 + arm64 并合并
# 环境变量:
#   SIGN_ID  正式证书名（如 "Developer ID Application: xxx"）；留空则用本地稳定证书/ad-hoc
#   BUNDLE_ID  Bundle Identifier，默认 com.51pwn.urlbox
#   VERSION   版本号（如 1.2.3）；写入 Info.plist 并用于产物命名
#   CI=1      CI 环境：跳过本地签名身份创建，直接 ad-hoc 签名
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

# 国内网络较慢时可 CARGO_MIRROR=1 走 rsproxy 镜像（切换源会触发一次全量重编译）
CARGO_ARGS=()
if [ "${CARGO_MIRROR:-0}" = "1" ]; then
  CARGO_ARGS=(--config "$ROOT/.cargo/mirror.toml")
fi

APP_NAME="UrlBox"
BIN="urlbox"
DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
BUNDLE_ID="${BUNDLE_ID:-com.51pwn.urlbox}"

build_arch() {
  local target="$1"
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build --release ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} --target "$target"
  echo "target/$target/release/$BIN"
}

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [ "${UNIVERSAL:-0}" = "1" ]; then
  X86=$(build_arch x86_64-apple-darwin)
  ARM=$(build_arch aarch64-apple-darwin)
  lipo -create -output "$APP/Contents/MacOS/$BIN" "$X86" "$ARM"
  echo "==> 已合并为 universal binary"
else
  cargo build --release ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"}
  cp "target/release/$BIN" "$APP/Contents/MacOS/$BIN"
fi
chmod +x "$APP/Contents/MacOS/$BIN"

sed "s/<string>com.51pwn.urlbox<\/string>/<string>$BUNDLE_ID<\/string>/" \
  packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# 版本号：优先 VERSION 环境变量，其次 Cargo.toml
VER="${VERSION:-}"
if [ -z "$VER" ]; then
  VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
fi
VER="${VER#v}"
if [ -n "$VER" ]; then
  # 同时更新 ShortVersionString 与 CFBundleVersion（只改这两个 key 后面的值）
  sed -i '' \
    -e "/<key>CFBundleShortVersionString<\/key>/{n;s|<string>.*</string>|<string>$VER</string>|;}" \
    -e "/<key>CFBundleVersion<\/key>/{n;s|<string>.*</string>|<string>$VER</string>|;}" \
    "$APP/Contents/Info.plist"
fi

if [ -f assets/icon.icns ]; then
  cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
  echo "==> 已写入图标"
else
  echo "==> 未找到 assets/icon.icns，使用系统默认图标"
fi

printf 'APPL????' > "$APP/Contents/PkgInfo"

# 签名策略：优先用稳定的本地证书，实在不行才退化到 ad-hoc。
# 原因：TCC（摄像头/麦克风/定位）按 designated requirement 记账。ad-hoc 的 DR 是
# cdhash，重编译就变 → 系统当成新 App 重新弹授权；证书签名的 DR 只认 bundle id +
# 证书根，所以**授权一次后永久有效**。
SIGN_KEYCHAIN="${SIGN_KEYCHAIN:-}"
if [ -z "${SIGN_ID:-}" ] && [ "${CI:-}" = "true" ]; then
  echo "==> CI 环境：跳过本地签名身份，直接 ad-hoc 签名"
elif [ -z "${SIGN_ID:-}" ]; then
  # 不指定证书时：优先用可自解锁的本地稳定身份（保证 TCC 授权不重置），
  # 其次才用机器上已有的其它身份，最后才 ad-hoc。
  if [ -x scripts/setup-signing.sh ]; then
    SIGN_KEYCHAIN=$(scripts/setup-signing.sh 2>/dev/null | tail -1)
  fi
  if [ -n "$SIGN_KEYCHAIN" ]; then
    SIGN_ID="UrlBox Local Signing"
  else
    SIGN_ID=$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(.*\)".*/\1/p' | head -1)
  fi
fi

if [ -n "${SIGN_ID:-}" ]; then
  # 只有 Apple 正式证书才需要（也才支持）向苹果打时间戳
  TS_ARGS=(--timestamp=none)
  case "$SIGN_ID" in
    *"Developer ID"*) TS_ARGS=(--timestamp) ;;
  esac
  KC_ARGS=()
  if [ -n "$SIGN_KEYCHAIN" ]; then KC_ARGS=(--keychain "$SIGN_KEYCHAIN"); fi
  if codesign --force --deep --options runtime ${TS_ARGS[@]+"${TS_ARGS[@]}"} \
    ${KC_ARGS[@]+"${KC_ARGS[@]}"} \
    --entitlements packaging/macos/urlbox.entitlements \
    --sign "$SIGN_ID" "$APP" 2>/dev/null; then
    echo "==> 已用证书签名: ${SIGN_ID}（TCC 授权跨重编译保留，不再重复弹窗）"
  else
    echo "!! 证书 \"$SIGN_ID\" 签名失败（多为钥匙串未解锁或权限不足）"
    codesign --force --deep --sign - "$APP"
    echo "==> 已回退 ad-hoc 签名（重编译后系统会重新问一次权限）"
  fi
else
  codesign --force --deep --sign - "$APP"
  echo "==> 已 ad-hoc 签名（本机自用足够；分发请设置 SIGN_ID 做正式签名）"
fi

codesign --verify --verbose=2 "$APP"

ZIP_NAME="$APP_NAME-macos.zip"
if [ -n "${VERSION:-}" ]; then
  ZIP_NAME="$APP_NAME-${VERSION#v}-macos-universal.zip"
fi
rm -f "$DIST/$ZIP_NAME"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$DIST/$ZIP_NAME"

echo "==> 完成: $APP"
echo "==> 压缩包: $DIST/$ZIP_NAME"
echo
echo "提示：首次打开如被 Gatekeeper 拦截，可执行"
echo "  xattr -dr com.apple.quarantine \"$APP\""
echo "摄像头/定位授权请从 Finder 启动 .app，不要在终端里直接跑裸二进制。"
