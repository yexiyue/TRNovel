param([string]$InstallDir = "$env:USERPROFILE\.cargo\bin", [string]$BaseUrl = "https://github.com/yexiyue/TRNovel/releases/download/trnovel-v@VERSION@")
$ErrorActionPreference = 'Stop'
if ($env:TRNOVEL_INSTALL_DIR) { $InstallDir = $env:TRNOVEL_INSTALL_DIR }
if ($env:TRNOVEL_RELEASE_BASE_URL) { $BaseUrl = $env:TRNOVEL_RELEASE_BASE_URL }
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [System.Runtime.InteropServices.Architecture]::X64) { throw 'Only Windows x64 MSVC is supported by this installer.' }
$Asset = 'trnovel-basic-x86_64-pc-windows-msvc.zip'
$Temporary = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Temporary | Out-Null
try {
  Invoke-WebRequest "$BaseUrl/$Asset" -OutFile (Join-Path $Temporary $Asset)
  Invoke-WebRequest "$BaseUrl/$Asset.sha256" -OutFile (Join-Path $Temporary "$Asset.sha256")
  $Expected = ((Get-Content (Join-Path $Temporary "$Asset.sha256") -Raw) -split '\s+')[0]
  $Actual = (Get-FileHash (Join-Path $Temporary $Asset) -Algorithm SHA256).Hash
  if ($Actual.ToLowerInvariant() -ne $Expected.ToLowerInvariant()) { throw 'Archive checksum mismatch.' }
  Expand-Archive (Join-Path $Temporary $Asset) -DestinationPath $Temporary
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  foreach ($Binary in @('trnovel.exe', 'trn.exe')) {
    Copy-Item (Join-Path $Temporary "trnovel-basic-x86_64-pc-windows-msvc/$Binary") (Join-Path $InstallDir $Binary) -Force
  }
  Write-Output "Installed basic reader in $InstallDir (add this directory to PATH)."
} finally { Remove-Item $Temporary -Recurse -Force }
