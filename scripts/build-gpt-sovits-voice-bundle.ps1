[CmdletBinding()]
param(
    [string]$SourceRoot = "D:\code_project\RoxyGPTSoVITS",
    [string]$RoxyModelRoot = "D:\code_project\洛琪希GSV模型260426\RoxyPro（新版）",
    [string]$OutputRoot = "",
    [string]$BundleVersion = "1.0.0",
    [switch]$ReuseStage
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($OutputRoot)) { $OutputRoot = Join-Path $ProjectRoot "artifacts" }
$OutputRoot = [IO.Path]::GetFullPath($OutputRoot)
$StageRoot = Join-Path $OutputRoot "voice-stage"
$VoiceRoot = Join-Path $StageRoot "voice"
$Archive = Join-Path $OutputRoot "RoxyVoice-v2ProPlus-win-x64-cu128-$BundleVersion.zip"
$RuntimeSource = Join-Path $SourceRoot ".runtime\py311-cu128-v1"
$ReferenceSource = Join-Path $ProjectRoot "src-tauri\resources\voice\roxy\references"

if (-not $OutputRoot.StartsWith($ProjectRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputRoot must stay inside the project: $OutputRoot"
}
foreach ($required in @(
    (Join-Path $RuntimeSource "python.exe"),
    (Join-Path $SourceRoot "api_v2.py"),
    (Join-Path $RoxyModelRoot "Roxy_Pro.pth"),
    (Join-Path $RoxyModelRoot "Roxy_Pro.ckpt"),
    (Join-Path $ReferenceSource "manifest.json")
)) {
    if (-not (Test-Path -LiteralPath $required)) { throw "Required file not found: $required" }
}

function Copy-Tree([string]$Source, [string]$Destination, [string[]]$Excluded = @()) {
    $arguments = @($Source, $Destination, "/E", "/NFL", "/NDL", "/NJH", "/NJS", "/NP")
    if ($Excluded.Count -gt 0) { $arguments += "/XD"; $arguments += $Excluded }
    & robocopy.exe @arguments | Out-Null
    if ($LASTEXITCODE -gt 7) { throw "Robocopy failed ($LASTEXITCODE): $Source" }
}

if (-not $ReuseStage) {
    if (Test-Path -LiteralPath $StageRoot) {
        Remove-Item -LiteralPath $StageRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $VoiceRoot | Out-Null

Copy-Tree $RuntimeSource (Join-Path $VoiceRoot "runtime")
Copy-Tree $SourceRoot (Join-Path $VoiceRoot "GPT-SoVITS") @(
    (Join-Path $SourceRoot ".git"),
    (Join-Path $SourceRoot ".downloads"),
    (Join-Path $SourceRoot ".runtime"),
    (Join-Path $SourceRoot ".runtime-tools")
)

$Pretrained = Join-Path $VoiceRoot "GPT-SoVITS\GPT_SoVITS\pretrained_models"
$AllowedPretrained = @(
    "chinese-roberta-wwm-ext-large",
    "chinese-hubert-base",
    "fast_langdetect",
    "s1v3.ckpt",
    "sv",
    "v2Pro"
)
Get-ChildItem -LiteralPath $Pretrained -Force | Where-Object {
    $AllowedPretrained -notcontains $_.Name
} | ForEach-Object {
    Remove-Item -LiteralPath $_.FullName -Recurse -Force
}

$ModelDestination = Join-Path $VoiceRoot "models\roxy"
New-Item -ItemType Directory -Force -Path $ModelDestination | Out-Null
Copy-Item -LiteralPath (Join-Path $RoxyModelRoot "Roxy_Pro.pth") -Destination $ModelDestination
Copy-Item -LiteralPath (Join-Path $RoxyModelRoot "Roxy_Pro.ckpt") -Destination $ModelDestination
Copy-Tree $ReferenceSource (Join-Path $VoiceRoot "references")
} elseif (-not (Test-Path -LiteralPath (Join-Path $VoiceRoot "runtime\python.exe"))) {
    throw "Reusable voice stage is incomplete: $VoiceRoot"
}

$ConfigRoot = Join-Path $VoiceRoot "config"
New-Item -ItemType Directory -Force -Path $ConfigRoot | Out-Null
$Config = @"
custom:
  bert_base_path: GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large
  cnhuhbert_base_path: GPT_SoVITS/pretrained_models/chinese-hubert-base
  device: cuda
  is_half: true
  t2s_weights_path: ../models/roxy/Roxy_Pro.ckpt
  version: v2ProPlus
  vits_weights_path: ../models/roxy/Roxy_Pro.pth
"@
[IO.File]::WriteAllText((Join-Path $ConfigRoot "tts_infer.yaml"), $Config, [Text.UTF8Encoding]::new($false))

$Manifest = [ordered]@{
    bundle_version = $BundleVersion
    model_version = "v2ProPlus"
    source_commit = "48b1a0169a28582a8984402f82cf438d3bfa6aca"
    python = "runtime/python.exe"
    api_entry = "GPT-SoVITS/api_v2.py"
    config = "config/tts_infer.yaml"
    references = "references"
    torch = "2.7.0+cu128"
}
[IO.File]::WriteAllText((Join-Path $VoiceRoot "manifest.json"), ($Manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))

& (Join-Path $VoiceRoot "runtime\python.exe") -c "import torch; assert torch.__version__ == '2.7.0+cu128'; assert torch.cuda.is_available(); print(torch.cuda.get_device_name(0))"
if ($LASTEXITCODE -ne 0) { throw "Packaged CUDA runtime self-test failed." }

if (Test-Path -LiteralPath $Archive) {
    Remove-Item -LiteralPath $Archive -Force
}
Push-Location $StageRoot
try {
    & tar.exe -a -c -f $Archive "voice"
    if ($LASTEXITCODE -ne 0) { throw "Failed to create voice ZIP." }
} finally {
    Pop-Location
}

$Hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash
Set-Content -LiteralPath "$Archive.sha256" -Value "$Hash  $(Split-Path -Leaf $Archive)" -Encoding ascii
Write-Host "Voice package ready: $Archive"
Write-Host "Extract it beside RoxyDesktopPet.exe so that voice\\manifest.json exists."
