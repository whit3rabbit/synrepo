# synrepo install script for Windows
# https://github.com/whit3rabbit/synrepo
#
# Downloads the synrepo Windows binary from the GitHub release, verifies its
# SHA256 against the published SHA256SUMS, installs to
# %LOCALAPPDATA%\synrepo\synrepo.exe, and adds that directory to the user's
# PATH if it isn't already there.
#
# Usage:
#   irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/whit3rabbit/synrepo/main/scripts/install.ps1))) -Version 0.0.1
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Version 0.0.1

[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
    [switch]$SkipPathUpdate
)

$ErrorActionPreference = 'Stop'
$ProgressPreference    = 'SilentlyContinue'

$Repo   = 'whit3rabbit/synrepo'
$Binary = 'synrepo.exe'

function Write-Info {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Resolve-Version {
    param([string]$Requested)

    if ($Requested) { return $Requested.TrimStart('v') }
    if ($env:INSTALL_VERSION) { return $env:INSTALL_VERSION.TrimStart('v') }

    $apiUrl  = "https://api.github.com/repos/$Repo/releases/latest"
    $release = Invoke-RestMethod -Uri $apiUrl -UseBasicParsing `
        -Headers @{ 'User-Agent' = 'synrepo-install' }
    if (-not $release.tag_name) {
        throw "Could not determine latest synrepo release from GitHub."
    }
    return ($release.tag_name).TrimStart('v')
}

function Get-ExpectedChecksum {
    param(
        [string]$SumsPath,
        [string]$FileName
    )
    foreach ($line in Get-Content -LiteralPath $SumsPath) {
        if ($line -match '^([0-9a-fA-F]{64})\s+(?:\./)?(.+)$') {
            if ($Matches[2] -eq $FileName) { return $Matches[1].ToLower() }
        }
    }
    return $null
}

function Add-ToUserPath {
    param([string]$Dir)

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $segments = @()
    if ($userPath) {
        $segments = $userPath.Split(';') | Where-Object { $_ -ne '' }
    }

    $already = $false
    foreach ($seg in $segments) {
        if ($seg.TrimEnd('\').Equals($Dir.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
            $already = $true
            break
        }
    }

    if ($already) {
        Write-Info "$Dir is already on the user PATH."
        return
    }

    $newPath = if ($userPath) { "$userPath;$Dir" } else { $Dir }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$Dir"
    Write-Info "Added $Dir to user PATH. Open a new shell for other processes to pick it up."
}

function ConvertTo-TomlString {
    param([string]$Value)
    return $Value.Replace('\', '\\').Replace('"', '\"')
}

function Write-BinaryInstallRecord {
    param(
        [string]$Path,
        [string]$Method
    )

    $registryDir = Join-Path $env:USERPROFILE '.synrepo'
    $registry    = Join-Path $registryDir 'projects.toml'
    New-Item -ItemType Directory -Force -Path $registryDir | Out-Null

    $kept = @()
    if (Test-Path -LiteralPath $registry) {
        $skip = $false
        foreach ($line in Get-Content -LiteralPath $registry) {
            if ($line -eq '[binary]') {
                $skip = $true
                continue
            }
            if ($line.StartsWith('[')) { $skip = $false }
            if (-not $skip) { $kept += $line }
        }
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    $kept += ''
    $kept += '[binary]'
    $kept += "path = `"$(ConvertTo-TomlString -Value $Path)`""
    $kept += "install_method = `"$(ConvertTo-TomlString -Value $Method)`""
    $kept += "installed_at = `"$stamp`""
    Set-Content -LiteralPath $registry -Value $kept -Encoding UTF8
}

$ver = Resolve-Version -Requested $Version

if (-not $InstallDir) {
    if ($env:SYNREPO_INSTALL_DIR) {
        $InstallDir = $env:SYNREPO_INSTALL_DIR
    } else {
        $InstallDir = Join-Path $env:LOCALAPPDATA 'synrepo'
    }
}
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$assetName = "synrepo-$ver-windows-amd64.exe"
$baseUrl   = "https://github.com/$Repo/releases/download/v$ver"
$tempRoot  = Join-Path ([IO.Path]::GetTempPath()) ("synrepo-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null

try {
    $assetPath = Join-Path $tempRoot $assetName
    $sumsPath  = Join-Path $tempRoot 'SHA256SUMS'

    Write-Info "Downloading $assetName..."
    Invoke-WebRequest -Uri "$baseUrl/$assetName" -OutFile $assetPath -UseBasicParsing
    Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -OutFile $sumsPath  -UseBasicParsing

    Write-Info "Verifying checksum..."
    $expected = Get-ExpectedChecksum -SumsPath $sumsPath -FileName $assetName
    if (-not $expected) {
        throw "No checksum entry for $assetName in SHA256SUMS."
    }
    $actual = (Get-FileHash -Path $assetPath -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) {
        throw "Checksum mismatch for $assetName`n  expected: $expected`n  actual:   $actual"
    }

    $dest = Join-Path $InstallDir $Binary
    Move-Item -LiteralPath $assetPath -Destination $dest -Force
    Write-Info "Installed synrepo $ver to $dest"
    Write-BinaryInstallRecord -Path $dest -Method 'windows-direct'

    if (-not $SkipPathUpdate) {
        Add-ToUserPath -Dir $InstallDir
    }

    try {
        & $dest --version
    } catch {
        Write-Warning "synrepo installed, but running '--version' failed: $_"
    }
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
