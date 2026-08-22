[CmdletBinding()]
param(
    [string]$InstallRoot = "D:\code_project\RoxyGPTSoVITS",
    [ValidateSet("ModelScope", "HF-Mirror", "HF")]
    [string]$ModelSource = "ModelScope",
    [switch]$SkipModels
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$SourceRepository = "https://github.com/RVC-Boss/GPT-SoVITS.git"
$SourceCommit = "48b1a0169a28582a8984402f82cf438d3bfa6aca"
$MicromambaVersion = "2.0.5"
$MicromambaSha256 = "73CA8990AC0BBD2249365158D34C5231688F3A6CC0ACDDAD87CD3DD49523A1FA"
$PythonVersion = "3.11.13"
$TorchVersion = "2.7.0+cu128"
$TorchAudioVersion = "2.7.0+cu128"
$OnnxRuntimeGpuVersion = "1.22.0"
$NumpyVersion = "1.26.4"
$CondaForgeMirror = "https://mirrors.tuna.tsinghua.edu.cn/anaconda/cloud/conda-forge"
$PyPiMirror = "https://pypi.tuna.tsinghua.edu.cn/simple"

$ToolsRoot = Join-Path $InstallRoot ".runtime-tools"
$RuntimeRoot = Join-Path $InstallRoot ".runtime\py311-cu128-v1"
$DownloadRoot = Join-Path $InstallRoot ".downloads"
$MicromambaArchive = Join-Path $DownloadRoot "micromamba-$MicromambaVersion.tar.bz2"
$MicromambaExe = Join-Path $ToolsRoot "Library\bin\micromamba.exe"
$PythonExe = Join-Path $RuntimeRoot "python.exe"

$TorchWheelName = "torch-2.7.0+cu128-cp311-cp311-win_amd64.whl"
$TorchAudioWheelName = "torchaudio-2.7.0+cu128-cp311-cp311-win_amd64.whl"
$TorchWheel = Join-Path $DownloadRoot $TorchWheelName
$TorchAudioWheel = Join-Path $DownloadRoot $TorchAudioWheelName
$TorchWheelUrl = "https://download.pytorch.org/whl/cu128/torch-2.7.0%2Bcu128-cp311-cp311-win_amd64.whl"
$TorchAudioWheelUrl = "https://download.pytorch.org/whl/cu128/torchaudio-2.7.0%2Bcu128-cp311-cp311-win_amd64.whl"

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory)] [string]$FilePath,
        [Parameter(ValueFromRemainingArguments)] [string[]]$Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed ($LASTEXITCODE): $FilePath $($Arguments -join ' ')"
    }
}

function Invoke-ResumableDownload {
    param(
        [Parameter(Mandatory)] [string]$Uri,
        [Parameter(Mandatory)] [string]$Destination
    )

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    Write-Host "Downloading: $Uri"
    Write-Host "       Into: $Destination"
    Invoke-Checked curl.exe "--fail" "--location" "--continue-at" "-" "--retry" "20" "--retry-delay" "3" "--retry-all-errors" "--connect-timeout" "30" "--output" $Destination $Uri
}

function Expand-ZipDownload {
    param(
        [Parameter(Mandatory)] [string]$Uri,
        [Parameter(Mandatory)] [string]$ArchiveName,
        [Parameter(Mandatory)] [string]$Destination
    )

    $archive = Join-Path $DownloadRoot $ArchiveName
    Invoke-ResumableDownload -Uri $Uri -Destination $archive
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $Destination -Force
}

Write-Step "检查操作系统与 NVIDIA 驱动"
if (-not [Environment]::Is64BitOperatingSystem) {
    throw "Only Windows x64 is supported."
}
Invoke-Checked nvidia-smi

Write-Step "准备官方 GPT-SoVITS 源码"
if (-not (Test-Path (Join-Path $InstallRoot ".git"))) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $InstallRoot) | Out-Null
    Invoke-Checked git.exe "clone" $SourceRepository $InstallRoot
}
Invoke-Checked git.exe "-C" $InstallRoot "fetch" "origin" $SourceCommit
Invoke-Checked git.exe "-C" $InstallRoot "checkout" "--detach" $SourceCommit
$actualCommit = (& git.exe -C $InstallRoot rev-parse HEAD).Trim()
if ($actualCommit -ne $SourceCommit) {
    throw "Unexpected GPT-SoVITS commit: $actualCommit"
}

New-Item -ItemType Directory -Force -Path $ToolsRoot, $DownloadRoot | Out-Null

Write-Step "安装私有 Micromamba $MicromambaVersion"
if (-not (Test-Path $MicromambaArchive)) {
    Invoke-ResumableDownload -Uri "https://micro.mamba.pm/api/micromamba/win-64/$MicromambaVersion" -Destination $MicromambaArchive
}
$actualMicromambaHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $MicromambaArchive).Hash
if ($actualMicromambaHash -ne $MicromambaSha256) {
    throw "Micromamba SHA-256 mismatch. Expected $MicromambaSha256, got $actualMicromambaHash"
}
if (-not (Test-Path $MicromambaExe)) {
    tar.exe -xjf $MicromambaArchive -C $ToolsRoot
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract Micromamba." }
}
Invoke-Checked $MicromambaExe "--version"

Write-Step "创建私有 Python $PythonVersion 环境"
if (-not (Test-Path $PythonExe)) {
    Invoke-Checked $MicromambaExe "create" "-y" "-p" $RuntimeRoot "-c" $CondaForgeMirror "python=$PythonVersion" "pip" "ffmpeg=7.1" "cmake"
}
$actualPython = (& $PythonExe -c "import sys; print(sys.executable)").Trim()
if (-not $actualPython.StartsWith($RuntimeRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Python escaped private runtime: $actualPython"
}

Write-Step "下载固定 PyTorch CU128 轮子（约 3.4 GB，支持断点续传）"
Invoke-ResumableDownload -Uri $TorchWheelUrl -Destination $TorchWheel
Invoke-ResumableDownload -Uri $TorchAudioWheelUrl -Destination $TorchAudioWheel

Write-Step "安装固定 PyTorch 与官方推理依赖"
Invoke-Checked $PythonExe "-m" "pip" "install" "--index-url" $PyPiMirror $TorchWheel $TorchAudioWheel
Invoke-Checked $PythonExe "-m" "pip" "install" "--index-url" $PyPiMirror "-r" (Join-Path $InstallRoot "extra-req.txt") "--no-deps"
Invoke-Checked $PythonExe "-m" "pip" "install" "--index-url" $PyPiMirror "-r" (Join-Path $InstallRoot "requirements.txt")
Invoke-Checked $PythonExe "-m" "pip" "install" "--index-url" $PyPiMirror "--no-deps" "onnxruntime-gpu==$OnnxRuntimeGpuVersion" "numpy==$NumpyVersion"

if (-not $SkipModels) {
    Write-Step "下载 GPT-SoVITS 基础模型与文本模型"
    switch ($ModelSource) {
        "ModelScope" {
            $base = "https://www.modelscope.cn/models/XXXXRT/GPT-SoVITS-Pretrained/resolve/master"
        }
        "HF-Mirror" {
            $base = "https://hf-mirror.com/XXXXRT/GPT-SoVITS-Pretrained/resolve/main"
        }
        "HF" {
            $base = "https://huggingface.co/XXXXRT/GPT-SoVITS-Pretrained/resolve/main"
        }
    }

    if (-not (Test-Path (Join-Path $InstallRoot "GPT_SoVITS\pretrained_models\sv"))) {
        Expand-ZipDownload -Uri "$base/pretrained_models.zip" -ArchiveName "pretrained_models.zip" -Destination (Join-Path $InstallRoot "GPT_SoVITS")
    }
    if (-not (Test-Path (Join-Path $InstallRoot "GPT_SoVITS\text\G2PWModel"))) {
        Expand-ZipDownload -Uri "$base/G2PWModel.zip" -ArchiveName "G2PWModel.zip" -Destination (Join-Path $InstallRoot "GPT_SoVITS\text")
    }

    $nltkArchive = Join-Path $DownloadRoot "nltk_data.zip"
    Invoke-ResumableDownload -Uri "$base/nltk_data.zip" -Destination $nltkArchive
    Expand-Archive -LiteralPath $nltkArchive -DestinationPath $RuntimeRoot -Force

    $openJtalkArchive = Join-Path $DownloadRoot "open_jtalk_dic_utf_8-1.11.tar.gz"
    Invoke-ResumableDownload -Uri "$base/open_jtalk_dic_utf_8-1.11.tar.gz" -Destination $openJtalkArchive
    $openJtalkRoot = (& $PythonExe -c "import os, pyopenjtalk; print(os.path.dirname(pyopenjtalk.__file__))").Trim()
    tar.exe -xzf $openJtalkArchive -C $openJtalkRoot
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract Open JTalk dictionary." }
}

if ($SkipModels) {
    Write-Step "Installing required NLTK and Open JTalk runtime data"
    switch ($ModelSource) {
        "ModelScope" { $runtimeDataBase = "https://www.modelscope.cn/models/XXXXRT/GPT-SoVITS-Pretrained/resolve/master" }
        "HF-Mirror" { $runtimeDataBase = "https://hf-mirror.com/XXXXRT/GPT-SoVITS-Pretrained/resolve/main" }
        "HF" { $runtimeDataBase = "https://huggingface.co/XXXXRT/GPT-SoVITS-Pretrained/resolve/main" }
    }
    $nltkArchive = Join-Path $DownloadRoot "nltk_data.zip"
    Invoke-ResumableDownload -Uri "$runtimeDataBase/nltk_data.zip" -Destination $nltkArchive
    Expand-Archive -LiteralPath $nltkArchive -DestinationPath $RuntimeRoot -Force
    $openJtalkArchive = Join-Path $DownloadRoot "open_jtalk_dic_utf_8-1.11.tar.gz"
    Invoke-ResumableDownload -Uri "$runtimeDataBase/open_jtalk_dic_utf_8-1.11.tar.gz" -Destination $openJtalkArchive
    $openJtalkRoot = (& $PythonExe -c "import os, pyopenjtalk; print(os.path.dirname(pyopenjtalk.__file__))").Trim()
    tar.exe -xzf $openJtalkArchive -C $openJtalkRoot
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract Open JTalk dictionary." }
}
Write-Step "执行 CUDA 与依赖自检"
& $PythonExe -c @"
import sys
import torch
import torchaudio

assert sys.executable.lower().startswith(r'$($RuntimeRoot.ToLower())'), sys.executable
assert torch.__version__ == '$TorchVersion', torch.__version__
assert torchaudio.__version__ == '$TorchAudioVersion', torchaudio.__version__
assert torch.cuda.is_available(), 'torch.cuda.is_available() is false'
x = torch.ones(1, device='cuda')
print('python:', sys.executable)
print('torch:', torch.__version__)
print('torchaudio:', torchaudio.__version__)
print('cuda runtime:', torch.version.cuda)
print('gpu:', torch.cuda.get_device_name(0))
print('cuda tensor:', x.item())
"@
if ($LASTEXITCODE -ne 0) { throw "CUDA self-test failed." }

Write-Step "导出当前依赖快照"
& $PythonExe -m pip freeze | Set-Content -Encoding utf8 (Join-Path $InstallRoot "runtime-requirements.lock.txt")

Write-Host "`nGPT-SoVITS development runtime is ready." -ForegroundColor Green
Write-Host "Source commit : $SourceCommit"
Write-Host "Python        : $PythonExe"
Write-Host "Runtime root  : $RuntimeRoot"
Write-Host "API entry     : $(Join-Path $InstallRoot 'api_v2.py')"
