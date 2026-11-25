$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$changes = git status --porcelain --untracked-files=no
if ($LASTEXITCODE -ne 0 -or $changes) {
    throw 'Build Windows release installers from a clean checkout.'
}
$tree = git rev-parse 'HEAD^{tree}'
if ($LASTEXITCODE -ne 0) { throw 'Could not identify the release source tree.' }

$app = Join-Path $root 'crates\gtfs_validator_gui'
$version = (Get-Content -Raw (Join-Path $app 'tauri.conf.json') | ConvertFrom-Json).version
$output = Join-Path $root 'dist\windows-signed-build'
New-Item -ItemType Directory -Force -Path $output | Out-Null
$updaterConfig = Join-Path $output 'skip-updater.json'
'{"bundle":{"createUpdaterArtifacts":false}}' | Set-Content -Encoding ASCII $updaterConfig

Push-Location $app
try {
    cargo tauri build --config tauri.windows-signing.conf.json --config $updaterConfig --bundles 'msi,nsis' --ci -- --locked
    if ($LASTEXITCODE -ne 0) { throw 'The signed Tauri build failed.' }
} finally {
    Pop-Location
}

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
$bundle = Join-Path $target 'release\bundle'
$msi = "GTFS.Guru_${version}_x64_en-US.msi"
$nsis = "GTFS.Guru_${version}_x64-setup.exe"
Copy-Item -LiteralPath (Join-Path $bundle "msi\GTFS Guru_${version}_x64_en-US.msi") -Destination (Join-Path $output $msi)
Copy-Item -LiteralPath (Join-Path $bundle "nsis\GTFS Guru_${version}_x64-setup.exe") -Destination (Join-Path $output $nsis)

$config = Get-Content -Raw (Join-Path $app 'tauri.windows-signing.conf.json') | ConvertFrom-Json
$thumbprint = $config.bundle.windows.certificateThumbprint
$hashes = @{}
$installers = foreach ($name in $msi, $nsis) {
    $file = Join-Path $output $name
    $signature = Get-AuthenticodeSignature -LiteralPath $file
    if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate -or $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
        throw "Invalid publisher signature or timestamp for $name."
    }
    $hashes[$name] = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
    $file
}

$manifest = Join-Path $output 'windows-build.json'
@{ version = $version; sourceTree = $tree; sha256 = $hashes } |
    ConvertTo-Json | Set-Content -Encoding ASCII $manifest
$archive = Join-Path $root 'dist\gtfs-guru-windows-signed-build.zip'
Compress-Archive -LiteralPath ($installers + $manifest) -DestinationPath $archive -Force
Write-Output $archive
