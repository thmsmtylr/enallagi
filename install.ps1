# Installs enallagi into WSL with install.sh. The binary shells to sh and git, so no native Windows build exists.
#   $env:ENALLAGI_VERSION = 'vX.Y.Z'  pins a release (default: the latest)
$ErrorActionPreference = 'Stop'

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine('install.ps1: enallagi needs sh and git, so on Windows it runs only under WSL. Install WSL, then run this again.')
    exit 1
}

$pin = ''
if ($env:ENALLAGI_VERSION) {
    # the value is spliced into a shell command, so only a version tag passes
    if ($env:ENALLAGI_VERSION -notmatch '^v\d+\.\d+\.\d+$') {
        [Console]::Error.WriteLine("install.ps1: ENALLAGI_VERSION must read vX.Y.Z, not $($env:ENALLAGI_VERSION)")
        exit 1
    }
    $pin = "ENALLAGI_VERSION=$($env:ENALLAGI_VERSION) "
}

$url = 'https://raw.githubusercontent.com/thmsmtylr/enallagi/main/install.sh'
wsl.exe -e sh -c "curl -fsSL $url | ${pin}sh"
exit $LASTEXITCODE
