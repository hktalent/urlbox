urlbox for Windows
==================

直接运行 urlbox.exe 即可，默认打开 https://vi.51pwn.com/

运行依赖
--------
WebView2 Runtime。Windows 11 与 Windows 10 1803+ 通常已预装。
如提示缺少 WebView2，安装：
  https://developer.microsoft.com/microsoft-edge/webview2/

常用参数
--------
  urlbox.exe https://example.com
  urlbox.exe --always-on-top --permissions none
  urlbox.exe --help

摄像头权限
----------
由 WebView2 继承 Edge 的权限模型，网页首次请求时会弹系统权限框，无需额外配置。

卸载
----
本程序免安装，删除 exe 即可。
