param(
    [string]$CoreDirectory = 'D:\Projects\umiray-core\dist',
    [switch]$Launch,
    [switch]$Elevated
)
$ErrorActionPreference = 'Stop'
$administrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Launch -and -not $administrator) {
    Start-Process -FilePath 'powershell.exe' -WindowStyle Hidden -Verb RunAs -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-CoreDirectory', "`"$CoreDirectory`"", '-Launch', '-Elevated')
    exit
}
$project = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $env:LOCALAPPDATA 'umiray-dev\volt'
$binary = Join-Path $project 'src-tauri\target\debug\umiray-dev.exe'
if ($Launch) {
    if (-not (Test-Path -LiteralPath $binary)) { throw 'Build the debug client first: cargo build --manifest-path src-tauri/Cargo.toml' }
    $installed = Join-Path $env:LOCALAPPDATA 'umiray-dev\umiray-dev.exe'
    if (Get-Process -Name 'umiray-dev' -ErrorAction SilentlyContinue) {
        Start-Process -FilePath $installed -ArgumentList '--scheduled', '--replace' -Wait -WindowStyle Hidden
        $deadline = (Get-Date).AddSeconds(15)
        while ((Get-Process -Name 'umiray-dev' -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
        if (Get-Process -Name 'umiray-dev' -ErrorAction SilentlyContinue) { throw 'Close the existing dev client before relaunching it.' }
    }
}
foreach ($name in @('volt.exe', 'volt-relay.exe', 'WinDivert.dll', 'WinDivert64.sys')) {
    $source = Join-Path $CoreDirectory $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing local asset: $source" }
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null
foreach ($name in @('volt.exe', 'volt-relay.exe', 'WinDivert.dll', 'WinDivert64.sys')) {
    $source = Join-Path $CoreDirectory $name
    $target = Join-Path $destination $name
    if ((Test-Path -LiteralPath $target) -and (Get-FileHash -LiteralPath $source).Hash -eq (Get-FileHash -LiteralPath $target).Hash) { continue }
    Copy-Item -LiteralPath $source -Destination $target -Force
}
foreach ($item in (Get-ChildItem -LiteralPath $CoreDirectory -File | Where-Object Name -Match 'LICENSE|NOTICE')) {
    Copy-Item -LiteralPath $item.FullName -Destination $destination -Force
}
Write-Output "Local VOLT assets installed: $destination"
if ($Launch) {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
    Start-Process -FilePath $binary -WorkingDirectory $project -ArgumentList '--scheduled'
}
