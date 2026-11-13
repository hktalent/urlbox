# urlbox

把**任意 URL** 打包成一个跨 Windows / macOS / Linux 的**极简原生桌面窗口**。

默认目标地址：`https://vi.51pwn.com/`（vi · 见到你 / 人脸与车牌记忆）。

```bash
urlbox                                   # 打开默认地址
urlbox https://example.com               # 打开任意地址
urlbox https://vi.51pwn.com --always-on-top --permissions none
```

---

## 1. 技术选型：为什么是 Rust + wry/tao

需求本质是「一个窗口 + 一套系统 WebView + 少量原生逻辑」。不是做一个应用框架，
所以评估的重点是**产物体积、系统依赖、跨平台代码复用率、长期维护成本**。

| 方案 | 产物体积 | 常驻内存 | 三平台代码 | 系统依赖 | 评价 |
|---|---|---|---|---|---|
| **Electron** | ~120–180 MB | 200 MB+ | 一份 | 自带 Chromium | 太重，与"最小"目标相反 |
| **Tauri** | ~3–10 MB | 40–90 MB | 一份 | 系统 WebView | 好，但引入前端构建链/CLI/配置/IPC，对"就一个 URL"过度设计 |
| **Rust + wry + tao** | **~2–6 MB** | **40–80 MB** | **一份** | 系统 WebView | ✅ **本项目选择**：Tauri 的底层，去掉全部上层抽象，最薄 |
| C + 各平台原生 API | ~1–3 MB | 30–60 MB | **三份**（COM/ObjC/GTK） | 系统 WebView | 体积最优，但三套平台代码 + 手写内存管理，维护成本最高 |
| C++ + Qt WebEngine | ~80–150 MB | 150 MB+ | 一份 | 自带 Chromium + Qt | 体积和授权/分发成本都高 |
| Go + webview 库 | ~8–15 MB | 50–90 MB | 一份 | 系统 WebView | 需 CGO + 平台胶水，交叉编译麻烦，原生能力扩展受限 |
| PWA / 浏览器快捷方式 | 0 | — | — | 浏览器 | 不是桌面应用，没有独立窗口/置顶/权限控制 |

**结论：Rust + wry 0.57 + tao 0.37。**

1. **复用系统 WebView，不打包浏览器内核**：Windows→WebView2（Win10/11 自带）、
   macOS→WKWebView、Linux→WebKitGTK 4.1。这是体积能压到几 MB 的唯一途径。
2. **一份源码覆盖三平台**：C/C++ 方案要么写三套平台胶水（WebView2 的 COM 接口、
   Objective-C 的 WKWebView、GTK 的 WebKitGTK），要么背 Qt WebEngine 的 150 MB。
3. **wry 是 Tauri 的底层库**：需要的就是"窗口 + webview + 少量事件"，不需要
   Tauri 的前端脚手架、插件体系和 IPC 抽象。要加托盘/自动更新时再升级到 Tauri 即可，
   迁移成本极低。
4. **单二进制、无运行时依赖、无 Node/Python 环境**，双击即用。
5. Rust 的内存安全与零成本抽象适合这种长期挂着运行的小外壳（尤其监控类场景）。

---

## 2. 架构

```
src/main.rs     窗口、WebView、事件循环、链接策略、权限策略
src/config.rs   命令行/环境变量解析与校验（零额外依赖，不用 clap）
```

关键实现点：

- **平台差异只有一处**：Linux 必须 `window.default_vbox()` + `build_gtk()`；
  Windows/macOS 直接 `build(&window)`。
- **同源内嵌、跨域外链**：`with_navigation_handler` 里比对 host，跨域 URL 交给系统
  默认浏览器（`open` / `start` / `xdg-open`），避免用户被困在一个没有地址栏的窗口里。
  需要放行更多域名用 `--allow-origin`。
- **新窗口请求**：`with_new_window_req_handler` 一律 `Deny` + 转系统浏览器，
  不做多窗口，保持"单窗口外壳"的定位。
- **权限策略**：`with_permission_handler`，默认 `auto` 只放行 摄像头/麦克风/定位
  （这类监控站点必需），其余走平台默认；`none` 全拒，`all` 全放。
- **标题跟随**：`with_document_title_changed_handler` 通过 `EventLoopProxy`
  把标题回传主线程（窗口句柄不是 `Send`，不能直接跨线程改）。

---

## 3. 快速开始（macOS 本机已验证）

```bash
cd /Users/51pwn/MyWork/urlbox
./scripts/build.sh                 # = cargo build --release
./target/release/urlbox --help
./target/release/urlbox            # 打开 https://vi.51pwn.com/
```

本机 Rust 工具链装在 `~/.cargo/bin`（rustup 1.100 / rustc 1.100），未修改 PATH，
脚本里已显式加进 PATH。

国内网络慢时用镜像（切换源会触发一次全量重编译，二者选一后别来回切）：

```bash
CARGO_MIRROR=1 ./scripts/build.sh
# 或手动：
cargo --config .cargo/mirror.toml build --release
```

### 本机实测（已完成）

| 项 | 结果 |
|---|---|
| `cargo build --release` | 成功，产物 `target/release/urlbox`，**776 KB** |
| `cargo test --release` | 8/8 通过（config 解析、URL 校验、参数边界） |
| 真实启动 | `URLBOX_TRACE=1` 输出 `load started` → `load finished https://vi.51pwn.com/` |
| `.app` 打包 | `dist/UrlBox.app` + `dist/UrlBox-macos.zip`，ad-hoc 签名 `valid on disk` |
| Bundle 校验 | `CFBundleIdentifier=com.51pwn.urlbox`，`LSMinimumSystemVersion=13.0`，图标已嵌入 |

诊断技巧：加 `URLBOX_TRACE=1` 启动，会把页面加载事件打到 stderr，适合无人值守/远程验证。

### ⚠️ 本机踩坑（已修进 `.cargo/config.toml`）

macOS 26（Intel, 25G342）上，rustc 默认把 Mach-O 的 `minos` 写成 **10.12**，
产物**运行即 SIGSEGV**（连 `fn main(){}` 都崩），而 C 程序正常。
把 `MACOSX_DEPLOYMENT_TARGET` 抬到 **13.0** 后一切正常。

项目已用 `.cargo/config.toml` 的 `[env]` 固定该值（`force = false`，外部可覆盖）。
如果你在本机跑别的 Rust 项目也遇到"编译通过、运行就崩"，先查这个变量：

```bash
otool -l target/release/xxx | grep -A4 LC_BUILD_VERSION   # 看 minos
```

---

## 4. 三平台构建

### macOS

```bash
xcode-select --install          # 只需命令行工具
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/package-macos.sh                  # 本机架构
UNIVERSAL=1 ./scripts/package-macos.sh      # 通用二进制（Intel + Apple Silicon）
```

### Linux

```bash
sudo apt update && sudo apt install -y \
  build-essential curl file libssl-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev
cargo build --release
```

Fedora：`sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel`。
运行期只需 `libwebkit2gtk-4.1-0` + `libgtk-3-0`。
Wayland 下如遇问题可 `GDK_BACKEND=x11 ./urlbox`。

### Windows

- 安装 **MSVC Build Tools** + Rust `stable-x86_64-pc-windows-msvc`。
- 运行依赖 **WebView2 Runtime**（Win10 1803+/Win11 已预装；Win7/8 需手动装）。
- 打包：

```powershell
.\scripts\package-windows.ps1
```

产物 `dist\urlbox-windows-x64.zip`（exe + 说明）。需要图标可用 `rcedit` 附加 `.ico`。

---

## 5. 摄像头 / 麦克风 / 定位权限

目标：**页面里的 `getUserMedia()`、`navigator.geolocation` 拿到即用，不出现网页级弹窗，也不需要人工点系统授权框。**

| 层 | 谁在管 | 本项目的处理 |
|---|---|---|
| 网页层（WebView 内部权限） | wry `with_permission_handler` | `--permissions auto`（默认）对 Camera/Microphone/Geolocation 直接返回 `Allow`，**不出现网页权限对话框** |
| 系统层（macOS TCC） | 操作系统，按签名记账 | Info.plist 写好 `NSCameraUsageDescription` / `NSMicrophoneUsageDescription` / `NSLocation*UsageDescription`；首次使用系统会问一次 |
| macOS 定位 | WKWebView 无定位回调 | **自建 CoreLocation 原生桥**（`src/platform/macos_geo.m` + `src/geo.rs`），注入 JS 覆盖 `navigator.geolocation`，走 IPC 回传坐标 |

### 为什么 macOS 定位必须自己写桥

wry 0.57 未实现 `PermissionKind::Geolocation`，WebKit 在 macOS 上也没有
`requestGeolocationPermissionForOrigin` 回调，`navigator.geolocation` 会直接返回
`User denied Geolocation`（实测 TCC 日志里连一条 `kTCCServiceLocation` 记录都没有，
说明请求根本没进系统）。所以本项目：

1. `build.rs` 在 macOS 下用 `cc` 编译 `src/platform/macos_geo.m`，链接 CoreLocation；
2. 网页调用定位 → JS shim 经 `window.ipc.postMessage` 发给 Rust → 调 `CLLocationManager`；
3. 回调在主线程 `evaluate_script` 注回 `window.__urlboxGeo(id, payload)`，
   语义与浏览器一致：`{coords:{latitude,longitude,accuracy}, timestamp}` / `{code,message}`，
   错误码 1=拒绝、2=不可用、3=超时；`watchPosition` 以 5s 间隔轮询。

禁用该桥：`--no-geo-bridge`（退回 WebKit 自带实现，macOS 上会被拒）。

### 系统授权只弹一次的保证

TCC 按签名的 **designated requirement** 记账：

- ad-hoc 签名 → DR 是 `cdhash H"..."`，**重新编译就变**，系统当成新 App 重新问；
- 证书签名 → DR 是 `identifier "com.51pwn.urlbox" and certificate root = H"..."`，
  与二进制无关，**授权一次后重编译、重打包都不再弹**。

`scripts/setup-signing.sh` 会自建钥匙串 + 自签代码签名证书（口令本机自用、无需人工解锁），
打包脚本默认就用它，所以自动化会话里也不会再出现 `errSecInternalComponent`。

验证（无人值守）：

```bash
open -n dist/UrlBox.app --args --diag
cat /tmp/urlbox-diag.log
# {"type":"geolocation","ok":true,"lat":30.67378,"lng":103.96252,"acc":35}
# {"type":"media","ok":true,"tracks":["audio:VIBRO-26W","video:FaceTime HD Camera (Built-in)"]}
```

---

## 6. 打包与签名（macOS）

```bash
BUNDLE_ID=com.51pwn.urlbox SIGN_ID="Developer ID Application: 你的名字 (TEAMID)" \
  ./scripts/package-macos.sh
```

- 不设 `SIGN_ID` 时自动用 `scripts/setup-signing.sh` 创建的 **本地自签证书**
  （`UrlBox Local Signing`，钥匙串 `~/Library/Keychains/urlbox-signing.keychain-db`）：
  DR 稳定，系统授权一次后永久有效。
- 该脚本失败时才回退 **ad-hoc 签名**：ad-hoc 每次重编译签名都会变，
  系统会把它当成"另一个 App"，摄像头/麦克风/定位都要重新确认。
- 对外分发需要 Developer ID + 公证（notarytool），否则 Gatekeeper 会拦。
  临时绕过：`xattr -dr com.apple.quarantine dist/UrlBox.app`。
- entitlements 模板见 `packaging/macos/urlbox.entitlements`（Hardened Runtime 时用）。

---

## 7. 已知限制（都是平台事实，不是 bug）

1. **macOS 系统授权无法由程序自动批准**：摄像头/麦克风/定位属 Apple TCC，
   首次使用必须由用户在系统弹框里点一次"允许"，程序侧没有 API 可以代批。
   本项目已做到：网页层零弹窗 + 签名稳定，所以**只点这一次，之后永久静默**。
   定位在 macOS 上额外走自建 CoreLocation 桥（见第 5 节）。
2. **跨域链接走系统浏览器**：这是有意为之（单窗口无地址栏，防止用户被困）。
   `--allow-origin <host>` 可追加白名单。
3. **Linux 的 WebRTC** 依赖发行版编译 WebKitGTK 时的特性，部分发行版摄像头不可用。
4. 不做托盘、单实例、自动更新、多标签——这些属于 Tauri 或完整浏览器壳的范围。

---

## 8. 体积实测

| 平台 | 二进制 | 说明 |
|---|---|---|
| macOS x86_64 release | **776 KB**（实测） | 动态链接系统 WKWebView / AppKit，无额外资源 |
| Linux x86_64 | 约 1–3 MB | 依赖 WebKitGTK 运行时 |
| Windows x64 | 约 1–3 MB | 依赖 WebView2 Runtime |

对比 Electron 同功能壳（~150 MB + 自带 Chromium），体积差约 **200 倍**。
macOS 上 `.app` 打包后约 1 MB（含 1.9 MB 图标时约 2.8 MB）。

---

## 9. 安全提醒

这个壳把远端页面当作"应用程序"运行，**信任级别等同浏览器**：

- 只打包你自己控制/信任的 URL；被入侵的站点可以在你的机器上申请摄像头、定位。
- 不确定时用 `--permissions none` 先跑。
- 建议页面侧保持 HTTPS + `Content-Security-Policy`；本项目不做任何注入或证书绕过。

---

## 10. 目录结构

```
urlbox/
├── Cargo.toml
├── .cargo/config.toml            # 本机 deployment target 修复
├── src/main.rs                   # 窗口 / WebView / 事件循环
├── src/config.rs                 # CLI 与环境变量
├── packaging/
│   ├── macos/Info.plist          # 权限用途说明 + Bundle 元信息
│   ├── macos/urlbox.entitlements
│   ├── linux/urlbox.desktop
│   └── windows/README.txt
├── assets/
│   ├── icon.png                  # 1024+ 源图
│   ├── icon.icns                 # macOS 图标
│   └── icon_256.png              # Linux 图标
├── scripts/
│   ├── build.sh
│   ├── package-macos.sh          # 产出 dist/UrlBox.app + zip
│   ├── package-linux.sh          # 产出 tar.gz + install.sh
│   └── package-windows.ps1       # 产出 zip
└── .github/workflows/release.yml # 三平台 CI 构建
```

## License

MIT
