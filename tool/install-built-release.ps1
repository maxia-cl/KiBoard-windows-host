[CmdletBinding()]
param(
    [string]$InstallerPath
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$bundleDir = Join-Path $repoRoot "src-tauri\target\release\bundle\nsis"

if ([string]::IsNullOrWhiteSpace($InstallerPath)) {
    $installer = Get-ChildItem -LiteralPath $bundleDir -Filter "*-setup.exe" -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        throw "No NSIS bundle found. Run 'npx tauri build' first."
    }
} else {
    $installer = Get-Item -LiteralPath $InstallerPath
}

$resolvedBundle = [System.IO.Path]::GetFullPath($bundleDir).TrimEnd('\') + '\'
$resolvedInstaller = [System.IO.Path]::GetFullPath($installer.FullName)
if (-not $resolvedInstaller.StartsWith($resolvedBundle, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Only installers under src-tauri\target\release\bundle\nsis are accepted."
}
if ($installer.Name -notlike "*-setup.exe") {
    throw "Expected a Tauri NSIS *-setup.exe bundle."
}

$installedExe = Join-Path $env:LOCALAPPDATA "KiBoard\desktop.exe"
Get-CimInstance Win32_Process |
    Where-Object { $_.ExecutablePath -eq $installedExe } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

$result = Start-Process -FilePath $resolvedInstaller -ArgumentList "/S" -Wait -PassThru
if ($result.ExitCode -ne 0) {
    throw "KiBoard installer failed with exit code $($result.ExitCode)."
}

if (-not (Test-Path -LiteralPath $installedExe)) {
    throw "Installation completed but $installedExe was not found."
}
Start-Process -FilePath $installedExe
Write-Host "KiBoard installed from $resolvedInstaller"
