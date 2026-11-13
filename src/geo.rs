//! macOS 原生定位桥（CoreLocation）。
//!
//! 原因：WKWebView 在 macOS 上没有可用的定位授权回调，`navigator.geolocation`
//! 会直接返回 `User denied Geolocation`（wry 0.57 也已标注 macOS 不支持该权限）。
//! 这里用一小段 Objective-C（见 `src/platform/macos_geo.m`）走 CoreLocation，
//! 把坐标通过 `evaluate_script` 回注给网页里被替换掉的 `navigator.geolocation`。
//!
//! 全程只在主线程执行：IPC handler、CoreLocation 回调、WebView 句柄都是主线程对象。

use std::cell::RefCell;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

thread_local! {
    /// WebView 句柄（非 Send，只能放主线程局部变量里）。
    static WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
}

extern "C" {
    fn urlbox_geo_request(
        callback: extern "C" fn(i32, f64, f64, f64, *const c_char, *mut c_void),
        ctx: *mut c_void,
    );
}

/// 接管 WebView（之后由本模块负责其生命周期）。
pub fn install(webview: wry::WebView) {
    WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));
}

/// 网页发来一次定位请求：`id` 用于把结果对回对应的 JS 回调。
pub fn request(id: u64) {
    if std::env::var_os("URLBOX_TRACE").is_some() {
        eprintln!("[urlbox-geo] ipc request id={id}");
    }
    let ctx = Box::into_raw(Box::new(id)).cast::<c_void>();
    unsafe { urlbox_geo_request(on_result, ctx) };
}

extern "C" fn on_result(
    ok: i32,
    lat: f64,
    lng: f64,
    acc: f64,
    err: *const c_char,
    ctx: *mut c_void,
) {
    let id = unsafe { Box::from_raw(ctx.cast::<u64>()) };
    let message = if err.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(err) }.to_string_lossy().into_owned()
    };

    let payload = if ok == 1 {
        format!(r#"{{"ok":true,"lat":{lat},"lng":{lng},"acc":{acc}}}"#)
    } else {
        // 与浏览器语义对齐：1 = 权限被拒，2 = 定位不可用
        let code = if message.contains("拒绝") { 1 } else { 2 };
        format!(
            r#"{{"ok":false,"code":{code},"msg":"{}"}}"#,
            escape_js(&message)
        )
    };

    let js = format!("window.__urlboxGeo && window.__urlboxGeo({}, {});", *id, payload);
    WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            if let Err(e) = webview.evaluate_script(&js) {
                eprintln!("urlbox: 注入定位结果失败 {e}");
            }
        }
    });
}

fn escape_js(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', " ")
}

/// 注入脚本：把 `navigator.geolocation` 换成走原生桥的实现。
///
/// 保留浏览器语义：成功回调给 `{coords:{latitude,longitude,accuracy},timestamp}`，
/// 失败回调给 `{code,message}` 并附带 `PERMISSION_DENIED/POSITION_UNAVAILABLE/TIMEOUT` 常量。
pub const GEO_SHIM_JS: &str = r#"
(function () {
  if (window.__urlboxGeoInstalled) { return; }
  if (!window.ipc || !window.ipc.postMessage) { return; }
  window.__urlboxGeoInstalled = true;

  var seq = 0;
  var pending = {};
  var watchers = {};

  function send(id) {
    try { window.ipc.postMessage(JSON.stringify({type:'geo', op:'get', id:id})); }
    catch (e) { /* 忽略 */ }
  }

  function get(opts, onOk, onErr) {
    var id = ++seq;
    var timeout = (opts && opts.timeout) || 15000;
    var timer = setTimeout(function () {
      if (pending[id]) {
        delete pending[id];
        onErr({code: 3, message: 'Timeout expired'});
      }
    }, timeout);
    pending[id] = {ok: onOk, err: onErr, timer: timer};
    send(id);
  }

  window.__urlboxGeo = function (id, payload) {
    var p = pending[id];
    if (!p) { return; }
    delete pending[id];
    clearTimeout(p.timer);
    if (payload && payload.ok) {
      p.ok({
        coords: {
          latitude: payload.lat,
          longitude: payload.lng,
          accuracy: payload.acc,
          altitude: null,
          altitudeAccuracy: null,
          heading: null,
          speed: null
        },
        timestamp: Date.now()
      });
    } else {
      p.err({
        code: (payload && payload.code) || 2,
        message: (payload && payload.msg) || 'Position unavailable'
      });
    }
  };

  var geo = {
    getCurrentPosition: function (onOk, onErr, opts) {
      get(opts || {}, onOk || function () {}, onErr || function () {});
    },
    watchPosition: function (onOk, onErr, opts) {
      var wid = ++seq;
      var tick = function () { get(opts || {}, onOk || function () {}, onErr || function () {}); };
      tick();
      watchers[wid] = setInterval(tick, 5000);
      return wid;
    },
    clearWatch: function (wid) {
      if (watchers[wid]) { clearInterval(watchers[wid]); delete watchers[wid]; }
    }
  };

  try {
    Object.defineProperty(navigator, 'geolocation', {configurable: true, value: geo});
  } catch (e) {
    try { navigator.geolocation = geo; } catch (e2) { /* 放弃 */ }
  }
})();
"#;
