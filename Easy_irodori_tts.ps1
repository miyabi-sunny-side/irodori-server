param(
    [ValidateSet('auto', 'cu128', 'cpu', 'xpu')][string]$Backend = 'auto',
    [ValidateRange(1024, 65535)][int]$Port = 7860,
    [switch]$NoBrowser,
    [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = $PSScriptRoot
$revision = '8224dafb46d0aba89209a8f905f1cb7e3299d9c1'
$app = Join-Path $root 'Irodori-TTS'
$toolDir = Join-Path $root 'dev_tools'
$uv = Join-Path $toolDir 'uv\uv.exe'
$git = Join-Path $toolDir 'PortableGit\cmd\git.exe'

if ($CheckOnly) {
    Write-Host "Root: $root"
    Write-Host "Repository revision: $revision"
    Write-Host "Backend: $Backend / Port: $Port"
    Write-Host 'Check only: no downloads, installs, or environment changes.'
    exit 0
}

function Run-Checked([string]$File, [string[]]$Arguments) {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Command failed ($LASTEXITCODE): $File $Arguments" }
}

function Download-Verified([string]$Url, [string]$Destination, [string]$Sha256) {
    if ($Sha256 -notmatch '^[a-fA-F0-9]{64}$') { throw 'Missing valid download checksum.' }
    if ((Test-Path -LiteralPath $Destination) -and
        ((Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -eq $Sha256)) { return }
    $partial = "$Destination.partial"
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $partial
            if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $Sha256) {
                throw "Checksum mismatch: $Url"
            }
            Move-Item -LiteralPath $partial -Destination $Destination -Force
            return
        } catch {
            if ($attempt -eq 3) { throw }
            Write-Host "Download interrupted. Retrying ($attempt/3)..."
            Start-Sleep -Seconds 2
        }
    }
}

$lock = $null
$transcribing = $false
try {
    if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
        throw 'This launcher requires x64 Windows.'
    }
    foreach ($folder in @('dev_tools', 'cache', 'temp', 'models', 'outputs', 'logs', 'config')) {
        New-Item -ItemType Directory -Path (Join-Path $root $folder) -Force | Out-Null
    }
    try {
        $lock = [IO.File]::Open((Join-Path $root 'temp\launcher.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    } catch { throw 'Another launcher is running in this folder. Close it first.' }
    $log = Join-Path $root ('logs\launch-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
    Start-Transcript -LiteralPath $log | Out-Null
    $transcribing = $true
    Write-Host 'Easy_Irodori_TTS - first setup can take a while.'
    Write-Host "All application data will be stored under: $root"

    # Process-local settings only; no setx, registry changes, or user-profile replacement.
    $settings = @{
        TEMP = 'temp'; TMP = 'temp'; TMPDIR = 'temp'
        UV_CACHE_DIR = 'cache\uv'; UV_PYTHON_INSTALL_DIR = 'dev_tools\python'
        UV_PYTHON_BIN_DIR = 'dev_tools\python-bin'; UV_TOOL_DIR = 'dev_tools\uv-tools'
        UV_TOOL_BIN_DIR = 'dev_tools\uv-tools-bin'; UV_PROJECT_ENVIRONMENT = '.venv'
        PIP_CACHE_DIR = 'cache\pip'; HF_HOME = 'models\huggingface'
        HF_HUB_CACHE = 'models\huggingface\hub'; HF_XET_CACHE = 'cache\hf-xet'
        HF_DATASETS_CACHE = 'cache\datasets'; TORCH_HOME = 'models\torch'
        TORCH_EXTENSIONS_DIR = 'cache\torch-extensions'; TORCHINDUCTOR_CACHE_DIR = 'cache\torchinductor'
        TRITON_CACHE_DIR = 'cache\triton'; NUMBA_CACHE_DIR = 'cache\numba'
        CUDA_CACHE_PATH = 'cache\cuda'; GRADIO_TEMP_DIR = 'temp\gradio'
        XDG_CACHE_HOME = 'cache'; XDG_CONFIG_HOME = 'config'; XDG_DATA_HOME = 'cache\data'
        MPLCONFIGDIR = 'cache\matplotlib'; WANDB_DIR = 'logs\wandb'
        WANDB_CACHE_DIR = 'cache\wandb'; WANDB_CONFIG_DIR = 'config\wandb'
    }
    foreach ($name in $settings.Keys) {
        $path = Join-Path $root $settings[$name]
        New-Item -ItemType Directory -Path $path -Force | Out-Null
        [Environment]::SetEnvironmentVariable($name, $path, 'Process')
    }
    $env:PYTHONNOUSERSITE = '1'
    $env:PYTHONUTF8 = '1'
    $env:PYTHONUNBUFFERED = '1'
    $env:PYTHONPATH = $null
    $env:PYTHONHOME = $null
    $env:VIRTUAL_ENV = $null
    $env:UV_PYTHON_PREFERENCE = 'only-managed'
    $env:UV_NO_CONFIG = '1'
    $env:UV_LINK_MODE = 'copy'
    $env:PIP_CONFIG_FILE = 'NUL'
    $env:GRADIO_ANALYTICS_ENABLED = 'False'
    $env:HF_HUB_DISABLE_TELEMETRY = '1'
    $env:DO_NOT_TRACK = '1'
    $env:WANDB_MODE = 'disabled'
    $env:GIT_CONFIG_GLOBAL = Join-Path $root 'config\gitconfig'
    $env:GIT_CONFIG_NOSYSTEM = '1'
    $env:GIT_TERMINAL_PROMPT = '0'
    $env:EASY_IRODORI_ROOT = $root
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    if (-not (Test-Path -LiteralPath (Join-Path $toolDir 'uv\ready.txt'))) {
        Write-Host '[1/5] Downloading local uv...'
        $zip = Join-Path $root 'cache\uv-bootstrap.zip'
        Download-Verified 'https://github.com/astral-sh/uv/releases/download/0.12.10/uv-x86_64-pc-windows-msvc.zip' $zip 'f65744f94072152b1f86ba2aace4d01f1124d9a8ecb235805039e3718c36cac2'
        Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $toolDir 'uv') -Force
        Run-Checked $uv @('--version')
        Set-Content -LiteralPath (Join-Path $toolDir 'uv\ready.txt') -Value '0.12.10'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $toolDir 'PortableGit\ready.txt'))) {
        Write-Host '[2/5] Downloading local Git (MinGit)...'
        $zip = Join-Path $root 'cache\git-bootstrap.zip'
        Download-Verified 'https://github.com/git-for-windows/git/releases/download/v2.55.0.windows.5/MinGit-2.55.0.5-64-bit.zip' $zip '56d7b226b7693196cfc71fef26568f536c4a021ab6c37ff2db4287bed908e96e'
        Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $toolDir 'PortableGit') -Force
        Run-Checked $git @('--version')
        Set-Content -LiteralPath (Join-Path $toolDir 'PortableGit\ready.txt') -Value '2.55.0.5'
    }
    $env:PATH = "$(Split-Path $git);$(Split-Path $uv);$env:PATH"

    Write-Host '[3/5] Preparing pinned Irodori-TTS source...'
    if (-not (Test-Path -LiteralPath (Join-Path $app '.git'))) {
        if ((Test-Path -LiteralPath $app) -and (Get-ChildItem -LiteralPath $app -Force)) {
            throw 'Irodori-TTS folder is not an empty folder or Git checkout. Move it aside before retrying.'
        }
        Run-Checked $git @('init', $app)
    }
    $sourceMarker = Join-Path $app '.easy-source-ready'
    if (-not (Test-Path -LiteralPath $sourceMarker)) {
        Run-Checked $git @('-C', $app, 'fetch', '--depth', '1', 'https://github.com/Aratako/Irodori-TTS.git', $revision)
        Run-Checked $git @('-C', $app, 'checkout', '--detach', $revision)
        Set-Content -LiteralPath $sourceMarker -Value $revision
    }
    $head = & $git -C $app rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $head -ne $revision) { throw 'Unexpected source revision. Restore the pinned checkout before launching.' }

    # TorchCodec needs shared FFmpeg DLLs. Save the selected release metadata locally.
    $ffmpegDir = Join-Path $toolDir 'ffmpeg'
    $ffmpegMarker = Join-Path $ffmpegDir 'ready.txt'
    if (-not (Test-Path -LiteralPath $ffmpegMarker)) {
        Write-Host 'Preparing local FFmpeg shared libraries...'
        $manifest = Join-Path $root 'config\ffmpeg-download.json'
        if (Test-Path -LiteralPath $manifest) {
            $asset = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
        } else {
            $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest'
            $asset = $release.assets | Where-Object name -eq 'ffmpeg-n8.1-latest-win64-lgpl-shared-8.1.zip' | Select-Object -First 1
            if (-not $asset) { throw 'Compatible FFmpeg 8.1 shared build was not found.' }
            $asset | Select-Object name,browser_download_url,digest | ConvertTo-Json | Set-Content -LiteralPath $manifest
        }
        $zip = Join-Path $root 'cache\ffmpeg-bootstrap.zip'
        Download-Verified $asset.browser_download_url $zip ($asset.digest -replace '^sha256:', '')
        Expand-Archive -LiteralPath $zip -DestinationPath $ffmpegDir -Force
        $ffexe = Get-ChildItem -LiteralPath $ffmpegDir -Filter ffmpeg.exe -Recurse | Select-Object -First 1
        if (-not $ffexe) { throw 'FFmpeg extraction failed.' }
        Run-Checked $ffexe.FullName @('-version')
        Set-Content -LiteralPath $ffmpegMarker -Value 'ready'
    }
    $ffexe = Get-ChildItem -LiteralPath $ffmpegDir -Filter ffmpeg.exe -Recurse | Select-Object -First 1
    if (-not $ffexe) { throw 'Local FFmpeg is missing.' }
    $env:EASY_FFMPEG_BIN = $ffexe.DirectoryName
    $env:PATH = "$($ffexe.DirectoryName);$env:PATH"

    $backendFile = Join-Path $root 'config\backend.txt'
    if ($Backend -eq 'auto' -and (Test-Path -LiteralPath $backendFile)) {
        $Backend = (Get-Content -LiteralPath $backendFile -Raw).Trim()
    }
    if ($Backend -eq 'auto') {
        $Backend = 'cpu'
        $smi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue
        if ($smi) {
            $gpuInfo = & $smi.Source --query-gpu=name --format=csv,noheader 2>$null
            if ($LASTEXITCODE -eq 0 -and $gpuInfo) { $Backend = 'cu128' }
        }
    }
    if ($Backend -notin @('cpu', 'cu128', 'xpu')) { throw 'Invalid config/backend.txt. Use cpu, cu128, or xpu.' }
    Write-Host "[4/5] Preparing isolated Python 3.11 environment ($Backend)..."
    Push-Location -LiteralPath $app
    try {
        Run-Checked $uv @('sync', '--frozen', '--no-dev', '--extra', $Backend, '--python', '3.11')
    } finally { Pop-Location }
    $python = Join-Path $root '.venv\Scripts\python.exe'
    Write-Host 'Checking audio libraries and GPU availability...'
    Run-Checked $python @((Join-Path $root 'easy_launcher.py'), '--check', '--backend', $Backend)
    Set-Content -LiteralPath $backendFile -Value $Backend
    Write-Host "[5/5] Opening http://127.0.0.1:$Port"
    Write-Host 'Select Base or Anime in the UI. First use downloads that model.'
    Write-Host 'Keep this window open. Press Ctrl+C to stop.'
    $launchArgs = @((Join-Path $root 'easy_launcher.py'), '--port', "$Port")
    if ($NoBrowser) { $launchArgs += '--no-browser' }
    Run-Checked $python $launchArgs
} catch {
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host 'See logs in this folder. Retry the same batch after correcting the error.'
    Write-Host 'For CPU mode: Easy_irodori_tts.bat -Backend cpu'
    exit 1
} finally {
    if ($transcribing) { Stop-Transcript | Out-Null }
    if ($lock) { $lock.Dispose() }
}
