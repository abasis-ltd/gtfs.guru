$ErrorActionPreference = "Stop"

$Repo = "abasis-ltd/gtfs.guru"
$BinName = "gtfs-guru.exe"

$InstallDir = $env:INSTALL_DIR
if (-not $InstallDir) {
  $InstallDir = Join-Path $env:LocalAppData "Programs\\gtfs-guru\\bin"
}

$Version = $env:GTFS_GURU_VERSION
if ($Version) {
  $BaseUrl = "https://github.com/$Repo/releases/download/$Version"
} else {
  $BaseUrl = "https://github.com/$Repo/releases/latest/download"
}

$Asset = "gtfs-guru-windows-x64.zip"
$Url = "$BaseUrl/$Asset"
$ChecksumsFile = "gtfs-guru-SHA256SUMS.txt"
$ChecksumsUrl = "$BaseUrl/$ChecksumsFile"
$VerifyChecksums = $true

$TempDir = Join-Path $env:TEMP "gtfs-guru-install"
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

$ZipPath = Join-Path $TempDir $Asset
Invoke-WebRequest -Uri $Url -OutFile $ZipPath

$ChecksumsPath = Join-Path $TempDir $ChecksumsFile
try {
  Invoke-WebRequest -Uri $ChecksumsUrl -OutFile $ChecksumsPath
} catch {
  Write-Warning "$ChecksumsFile not found. Skipping verification."
  $VerifyChecksums = $false
}

if ($VerifyChecksums) {
  $Expected = Select-String -Path $ChecksumsPath -Pattern ("^([A-Fa-f0-9]{64})\\s+$Asset$") |
    ForEach-Object { $_.Matches[0].Groups[1].Value } |
    Select-Object -First 1

  if (-not $Expected) {
    throw "Checksum for $Asset not found in $ChecksumsFile."
  }

  $Actual = (Get-FileHash -Algorithm SHA256 -Path $ZipPath).Hash.ToLower()
  if ($Actual -ne $Expected.ToLower()) {
    throw "Checksum verification failed for $Asset."
  }
}

Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Path (Join-Path $TempDir $BinName) -Destination (Join-Path $InstallDir $BinName) -Force

Write-Host "Installed gtfs-guru to $InstallDir"
Write-Host "Add $InstallDir to your PATH to run gtfs-guru."
