#!/usr/bin/env bash
# 创建/复用一套**稳定的本地代码签名身份**。
#
# 为什么需要：macOS 的 TCC（摄像头/麦克风/定位授权）按签名的 designated requirement
# 记账。ad-hoc 签名的 DR 就是 cdhash，重新编译一次 cdhash 就变，系统会把新包当成
# "另一个 App"重新弹授权框。改用自签证书后 DR 变成
#   identifier "com.51pwn.urlbox" and certificate root = H"...."
# 与二进制无关，所以**只要授权过一次，以后重编译、重打包都不再弹窗**。
#
# 本机钥匙串在自动化会话里通常没解锁（codesign 报 errSecInternalComponent），
# 所以这里自建一个专用钥匙串 + 自己的口令，全程不需要人工介入。
#
# 用法：scripts/setup-signing.sh   # 输出签名钥匙串路径，已存在则直接复用
set -euo pipefail

CERT_CN="UrlBox Local Signing"
KEYCHAIN_NAME="urlbox-signing.keychain-db"
KEYCHAIN="$HOME/Library/Keychains/$KEYCHAIN_NAME"
PASS="urlbox-local"   # 仅本机自用的开发签名口令，证书私钥不出本机
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/.signing"

if security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null | grep -q "$CERT_CN"; then
  :
else
  mkdir -p "$DIR"
  cat > "$DIR/openssl.cnf" <<'EOF'
[req]
distinguished_name = dn
prompt = no

[dn]
CN = UrlBox Local Signing
O  = urlbox

[ext]
basicConstraints = CA:FALSE
keyUsage = digitalSignature
extendedKeyUsage = codeSigning
EOF

  if [ ! -f "$DIR/urlbox.p12" ]; then
    openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
      -keyout "$DIR/urlbox.key" -out "$DIR/urlbox.crt" \
      -config "$DIR/openssl.cnf" -extensions ext >/dev/null 2>&1
    openssl pkcs12 -export -inkey "$DIR/urlbox.key" -in "$DIR/urlbox.crt" \
      -out "$DIR/urlbox.p12" -passout "pass:$PASS" >/dev/null 2>&1
  fi

  security create-keychain -p "$PASS" "$KEYCHAIN" 2>/dev/null || true
  security unlock-keychain -p "$PASS" "$KEYCHAIN"
  security import "$DIR/urlbox.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null 2>&1
  security set-key-partition-list -S apple-tool:,apple: -s -k "$PASS" "$KEYCHAIN" >/dev/null 2>&1
  # 自签证书要显式标记为受信任，否则 codesign 认为"无有效身份"
  security add-trusted-cert -r trustRoot -k "$KEYCHAIN" "$DIR/urlbox.crt" >/dev/null 2>&1 || true
  echo "==> 已创建本地签名身份: $CERT_CN" >&2
fi

security unlock-keychain -p "$PASS" "$KEYCHAIN" >/dev/null 2>&1 || true

# codesign 只在自己搜索列表里的钥匙串中找身份
if ! security list-keychains -d user | grep -q "$KEYCHAIN_NAME"; then
  # shellcheck disable=SC2046
  security list-keychains -d user -s $(security list-keychains -d user | tr -d '"') "$KEYCHAIN_NAME"
fi

echo "$KEYCHAIN"
