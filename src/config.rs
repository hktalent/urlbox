//! 命令行参数与运行配置。
//!
//! 不依赖 clap，保持零额外依赖；支持 `--key value` 与 `--key=value` 两种写法，
//! 并对每个参数做显式校验，避免"静默忽略"导致的误配置。

use std::fmt;

pub const DEFAULT_URL: &str = "https://vi.51pwn.com/";
pub const DEFAULT_TITLE: &str = "vi · 见到你";

/// 权限策略：决定网页能拿到哪些系统能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permissions {
    /// 默认：放行摄像头 / 麦克风 / 定位，其余走平台默认。
    Auto,
    /// 全部拒绝。
    None,
    /// 全部放行。
    All,
}

impl Permissions {
    fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "auto" | "default" => Ok(Self::Auto),
            "none" | "deny" => Ok(Self::None),
            "all" | "allow" => Ok(Self::All),
            other => Err(format!(
                "未知权限策略 `{other}`，可选：auto | none | all"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub maximized: bool,
    pub fullscreen: bool,
    pub always_on_top: bool,
    pub resizable: bool,
    pub devtools: bool,
    /// 隐藏调试开关：注入诊断脚本，验证定位/摄像头/麦克风能否自动放行。
    pub diag: bool,
    /// macOS：用原生 CoreLocation 桥接管 navigator.geolocation（默认开）。
    pub geo_bridge: bool,
    pub follow_title: bool,
    pub incognito: bool,
    pub user_agent: Option<String>,
    pub permissions: Permissions,
    pub extra_hosts: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            title: DEFAULT_TITLE.to_string(),
            width: 1280,
            height: 800,
            min_width: 480,
            min_height: 320,
            maximized: false,
            fullscreen: false,
            always_on_top: false,
            resizable: true,
            devtools: false,
            diag: false,
            geo_bridge: true,
            follow_title: true,
            incognito: false,
            user_agent: None,
            permissions: Permissions::Auto,
            extra_hosts: Vec::new(),
        }
    }
}

impl Config {
    /// 解析命令行。返回 `Ok(None)` 表示已打印帮助/版本，正常退出。
    pub fn parse<I>(args: I) -> Result<Option<Self>, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut cfg = Config::default();
        let mut positional: Option<String> = None;
        let mut it = args.into_iter().peekable();

        // 环境变量兜底：优先级低于命令行。
        if let Ok(v) = std::env::var("URLBOX_URL") {
            if !v.trim().is_empty() {
                cfg.url = v;
            }
        }
        if let Ok(v) = std::env::var("URLBOX_TITLE") {
            if !v.trim().is_empty() {
                cfg.title = v;
            }
        }
        if let Ok(v) = std::env::var("URLBOX_WIDTH") {
            if let Ok(n) = v.parse() {
                cfg.width = n;
            }
        }
        if let Ok(v) = std::env::var("URLBOX_HEIGHT") {
            if let Ok(n) = v.parse() {
                cfg.height = n;
            }
        }
        if let Ok(v) = std::env::var("URLBOX_PERMISSIONS") {
            cfg.permissions = Permissions::parse(&v)?;
        }

        while let Some(arg) = it.next() {
            // 拆出 `--key=value` 形式
            let (key, inline) = match arg.split_once('=') {
                Some((k, v)) if k.starts_with('-') => (k.to_string(), Some(v.to_string())),
                _ => (arg.clone(), None),
            };

            // 取下一个值：优先 `=` 内联值
            let mut value = |name: &str| -> Result<String, String> {
                if let Some(v) = inline.clone() {
                    return Ok(v);
                }
                it.next()
                    .ok_or_else(|| format!("参数 `{name}` 缺少取值"))
            };

            match key.as_str() {
                "-h" | "--help" => {
                    print_help();
                    return Ok(None);
                }
                "-V" | "--version" => {
                    println!("urlbox {}", env!("CARGO_PKG_VERSION"));
                    return Ok(None);
                }
                "--url" => cfg.url = value("--url")?,
                "--title" => cfg.title = value("--title")?,
                "--width" => cfg.width = parse_dim("--width", &value("--width")?)?,
                "--height" => cfg.height = parse_dim("--height", &value("--height")?)?,
                "--min-width" => cfg.min_width = parse_dim("--min-width", &value("--min-width")?)?,
                "--min-height" => {
                    cfg.min_height = parse_dim("--min-height", &value("--min-height")?)?
                }
                "--user-agent" => cfg.user_agent = Some(value("--user-agent")?),
                "--permissions" => cfg.permissions = Permissions::parse(&value("--permissions")?)?,
                "--allow-origin" => cfg.extra_hosts.push(value("--allow-origin")?),
                "--maximized" => cfg.maximized = true,
                "--fullscreen" => cfg.fullscreen = true,
                "--always-on-top" => cfg.always_on_top = true,
                "--no-resize" => cfg.resizable = false,
                "--devtools" => cfg.devtools = true,
                "--diag" => cfg.diag = true,
                "--no-geo-bridge" => cfg.geo_bridge = false,
                "--incognito" => cfg.incognito = true,
                "--static-title" => cfg.follow_title = false,
                other if other.starts_with('-') => {
                    return Err(format!("未知参数 `{other}`"));
                }
                _ => {
                    if positional.is_some() {
                        return Err(format!("多余的位置参数 `{arg}`，URL 只能给一个"));
                    }
                    positional = Some(arg);
                }
            }
        }

        if let Some(url) = positional {
            cfg.url = url;
        }

        cfg.validate()?;
        Ok(Some(cfg))
    }

    fn validate(&mut self) -> Result<(), String> {
        let trimmed = self.url.trim().to_string();
        self.url = trimmed;
        let parsed = url::Url::parse(&self.url)
            .map_err(|e| format!("URL `{}` 无法解析：{e}", self.url))?;
        match parsed.scheme() {
            "http" | "https" | "file" => {}
            other => {
                return Err(format!(
                    "不支持 `{other}` 协议，只接受 http / https / file"
                ))
            }
        }
        if self.width == 0 || self.height == 0 {
            return Err("窗口宽高必须大于 0".to_string());
        }
        if self.min_width > self.width || self.min_height > self.height {
            return Err("最小尺寸不能大于初始尺寸".to_string());
        }
        if self.title.trim().is_empty() {
            self.title = DEFAULT_TITLE.to_string();
        }
        Ok(())
    }
}

fn parse_dim(name: &str, raw: &str) -> Result<u32, String> {
    let n: u32 = raw
        .trim()
        .parse()
        .map_err(|_| format!("{name} 需要正整数，收到 `{raw}`"))?;
    if !(1..=16_384).contains(&n) {
        return Err(format!("{name} 超出合理范围（1..=16384）：{n}"));
    }
    Ok(n)
}

pub fn print_help() {
    println!(
        "\
urlbox {} — 把任意 Web URL 打包成一个极简跨平台桌面窗口

用法:
  urlbox [URL] [选项]

位置参数:
  URL                     要打开的地址，默认 {DEFAULT_URL}

选项:
  --url <URL>             同上（显式写法）
  --title <TITLE>         窗口标题，默认 `{DEFAULT_TITLE}`
  --width <N>             初始宽（逻辑像素），默认 1280
  --height <N>            初始高（逻辑像素），默认 800
  --min-width <N>         最小宽，默认 480
  --min-height <N>        最小高，默认 320
  --maximized             启动即最大化
  --fullscreen            全屏无边框启动
  --always-on-top         窗口置顶（看板/监控场景）
  --no-resize             禁止拖拽改变窗口大小
  --devtools              允许打开开发者工具（Linux/Windows 需 --features devtools 编译）
  --incognito             不落盘 Cookie/缓存（一次性会话）
  --user-agent <UA>       覆盖 User-Agent
  --permissions <MODE>    权限策略: auto（默认，放行摄像头/麦克风/定位）| none | all
  --allow-origin <HOST>   额外允许在窗口内跳转的域名，可重复指定
  --static-title          标题固定，不跟随网页标题
  --diag                  调试：主动申请定位/摄像头/麦克风，结果写入 /tmp/urlbox-diag.log
  --no-geo-bridge         macOS：禁用原生定位桥，退回 WebKit 自带（会被系统拒绝）
  -h, --help              显示本帮助
  -V, --version           显示版本

环境变量（低于命令行优先级）:
  URLBOX_URL, URLBOX_TITLE, URLBOX_WIDTH, URLBOX_HEIGHT, URLBOX_PERMISSIONS

说明:
  · 同源链接在窗口内跳转；跨域链接交给系统默认浏览器打开。
  · 摄像头/定位等能力需要系统授权，macOS 还需以 .app 形式运行并声明用途说明。",
        env!("CARGO_PKG_VERSION")
    );
}

impl fmt::Display for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::All => "all",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Config>, String> {
        Config::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults_to_target_site() {
        let cfg = parse(&[]).unwrap().unwrap();
        assert_eq!(cfg.url, DEFAULT_URL);
        assert_eq!(cfg.width, 1280);
        assert_eq!(cfg.permissions, Permissions::Auto);
    }

    #[test]
    fn positional_url_wins_over_default() {
        let cfg = parse(&["https://example.com/a?b=1"]).unwrap().unwrap();
        assert_eq!(cfg.url, "https://example.com/a?b=1");
    }

    #[test]
    fn supports_inline_and_separate_values() {
        let cfg = parse(&["--width=1024", "--height", "700", "--url", "https://x.test/"])
            .unwrap()
            .unwrap();
        assert_eq!(cfg.width, 1024);
        assert_eq!(cfg.height, 700);
        assert_eq!(cfg.url, "https://x.test/");
    }

    #[test]
    fn rejects_unknown_flag() {
        let err = parse(&["--nope"]).unwrap_err();
        assert!(err.contains("未知参数"), "{err}");
    }

    #[test]
    fn rejects_non_http_scheme() {
        let err = parse(&["ftp://x.test/"]).unwrap_err();
        assert!(err.contains("不支持"), "{err}");
    }

    #[test]
    fn rejects_min_larger_than_initial() {
        let err = parse(&["--width", "400", "--min-width", "900"]).unwrap_err();
        assert!(err.contains("最小尺寸"), "{err}");
    }

    #[test]
    fn collects_multiple_allow_origin() {
        let cfg = parse(&["--allow-origin", "a.test", "--allow-origin=b.test"])
            .unwrap()
            .unwrap();
        assert_eq!(cfg.extra_hosts, vec!["a.test", "b.test"]);
    }

    #[test]
    fn help_and_version_exit_without_config() {
        assert!(parse(&["--help"]).unwrap().is_none());
        assert!(parse(&["-V"]).unwrap().is_none());
    }
}
