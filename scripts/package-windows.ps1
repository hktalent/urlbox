# 生成 dist\urlbox-windows-x64.zip（exe + 运行说明）
# 前置：Rust MSVC 工具链；运行机需 WebView2 Runtime（Win10 1803+/Win11 已自带）
set-strictmode -version latest
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

cargo build --release

$Stage = Join-Path $Root "dist\urlbox"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

Copy-Item "target\release\urlbox.exe" (Join-Path $Stage "urlbox.exe")
Copy-Item "packaging\windows\README.txt" (Join-Path $Stage "README.txt")

if ($env:VERSION) {
  $Ver = $env:VERSION -replace "^v", ""
  $Zip = Join-Path $Root "dist\urlbox-$Ver-windows-x64.zip"
} else {
  $Zip = Join-Path $Root "dist\urlbox-windows-x64.zip"
}
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Zip

Write-Host "==> 完成: $Zip"
Get-Item $Zip | Format-List Name, Length
