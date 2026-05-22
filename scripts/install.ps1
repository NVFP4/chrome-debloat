$ErrorActionPreference = "Stop"

$script:Repo = "yashgorana/chrome-debloat"
$script:Binary = "chrome-debloat.exe"
$script:Asset = ""
$script:Url = ""
$script:TempDir = ""
$script:Archive = ""
$script:App = ""
$script:ExitCode = 0

function Write-Info {
    param([string] $Message)

    Write-Host $Message -ForegroundColor Cyan
}

function Enable-Tls12 {
    if ($PSVersionTable.PSEdition -eq "Desktop") {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    }
}

function Assert-SupportedSystem {
    if ([Environment]::Is64BitOperatingSystem -eq $false) {
        throw "Chrome Debloat only ships a 64-bit Windows build."
    }
}

function Initialize-InstallPaths {
    $script:Asset = "chrome-debloat-windows-x86_64.zip"
    $script:Url = "https://github.com/$script:Repo/releases/latest/download/$script:Asset"
    $script:TempDir = Join-Path ([IO.Path]::GetTempPath()) ("chrome-debloat-" + [Guid]::NewGuid().ToString("N"))
    $script:Archive = Join-Path $script:TempDir $script:Asset
    $script:App = Join-Path $script:TempDir $script:Binary

    New-Item -ItemType Directory -Path $script:TempDir | Out-Null
}

function Download-App {
    Write-Info "Downloading $script:Asset..."
    Invoke-WebRequest -UseBasicParsing -Uri $script:Url -OutFile $script:Archive
}

function Expand-App {
    Expand-Archive -Path $script:Archive -DestinationPath $script:TempDir -Force

    if (-not (Test-Path -LiteralPath $script:App)) {
        throw "Downloaded release did not contain $script:Binary."
    }
}

function Invoke-App {
    & $script:App
    if ($null -ne $LASTEXITCODE) {
        $script:ExitCode = $LASTEXITCODE
    }
}

function Remove-InstallFiles {
    if ([string]::IsNullOrWhiteSpace($script:TempDir)) {
        return
    }

    if (Test-Path -LiteralPath $script:TempDir) {
        Remove-Item -LiteralPath $script:TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Main {
    Enable-Tls12
    Assert-SupportedSystem
    Initialize-InstallPaths
    Download-App
    Expand-App
    Invoke-App
}

try {
    Main
} finally {
    Remove-InstallFiles
}

if ($script:ExitCode -ne 0) {
    exit $script:ExitCode
}
