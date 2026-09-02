[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$signingDirectory = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.kiboard\signing'
$privateKeyPath = Join-Path $signingDirectory 'kiboard-updater.key'
$passwordPath = Join-Path $signingDirectory 'kiboard-updater.password'

if ((Test-Path -LiteralPath $privateKeyPath) -or (Test-Path -LiteralPath $passwordPath)) {
    throw 'Windows updater signing already exists. Refusing to replace the key.'
}

$bytes = [byte[]]::new(36)
[Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$password = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')

New-Item -ItemType Directory -Path $signingDirectory -Force | Out-Null

try {
    & npx tauri signer generate `
        --ci `
        --password $password `
        --write-keys $privateKeyPath
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri signer failed with exit code $LASTEXITCODE"
    }

    [IO.File]::WriteAllText(
        $passwordPath,
        $password,
        [Text.UTF8Encoding]::new($false)
    )

    & icacls $privateKeyPath /inheritance:r /grant:r "${env:USERNAME}:(R,W)" | Out-Null
    & icacls $passwordPath /inheritance:r /grant:r "${env:USERNAME}:(R,W)" | Out-Null
}
catch {
    Remove-Item -LiteralPath $privateKeyPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath "$privateKeyPath.pub" -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $passwordPath -Force -ErrorAction SilentlyContinue
    throw
}

Write-Host 'Windows updater signing is configured.'
Write-Host "Private key: $privateKeyPath"
Write-Host "Public key: $privateKeyPath.pub"
Write-Host 'Back up the private key and password in a secure password manager.'
