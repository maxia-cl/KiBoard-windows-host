param(
    [string]$TargetDirectory = (Join-Path $PSScriptRoot '..\src-tauri\target\release')
)

$ErrorActionPreference = 'Stop'
$resolvedTarget = (Resolve-Path -LiteralPath $TargetDirectory).Path
$bundleDirectory = Join-Path $resolvedTarget 'bundle'
$files = @()

$appExecutable = Join-Path $resolvedTarget 'desktop.exe'
if (Test-Path -LiteralPath $appExecutable) {
    $files += Get-Item -LiteralPath $appExecutable
}

if (Test-Path -LiteralPath $bundleDirectory) {
    $files += Get-ChildItem -LiteralPath $bundleDirectory -Recurse -File |
        Where-Object { $_.Extension -in '.exe', '.msi' }
}

if ($files.Count -eq 0) {
    throw "No Windows executable or installer was found under $resolvedTarget"
}

$invalid = @()
foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid') {
        $invalid += "{0}: {1} ({2})" -f $file.FullName, $signature.Status, $signature.StatusMessage
        continue
    }

    Write-Output ("Valid Authenticode: {0} — {1}" -f $file.Name, $signature.SignerCertificate.Subject)
}

if ($invalid.Count -gt 0) {
    throw ("Windows release contains files without trusted Authenticode signatures:`n" + ($invalid -join "`n"))
}
