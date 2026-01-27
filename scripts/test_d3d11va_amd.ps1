# Test script for AMD GPU D3D11VA compatibility
# 用于测试 AMD GPU 在 Windows 上的 D3D11VA 硬件加速兼容性

param(
    [switch]$Verbose
)

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "AMD GPU D3D11VA Compatibility Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Detect GPU
Write-Host "[1/5] Detecting GPU..." -ForegroundColor Yellow
$gpu = Get-WmiObject Win32_VideoController | Select-Object -First 1
if ($gpu.Name -match "AMD|Radeon") {
    Write-Host "  ✅ AMD GPU detected: $($gpu.Name)" -ForegroundColor Green
} else {
    Write-Host "  ⚠️  Non-AMD GPU: $($gpu.Name)" -ForegroundColor Yellow
    Write-Host "     This test is for AMD GPUs, but will continue anyway." -ForegroundColor Gray
}
Write-Host "  Driver Version: $($gpu.DriverVersion)" -ForegroundColor Gray
Write-Host ""

# 2. Check FFmpeg
Write-Host "[2/5] Checking FFmpeg..." -ForegroundColor Yellow
try {
    $ffmpegVersion = & ffmpeg -version 2>&1 | Select-String "ffmpeg version" | Select-Object -First 1
    Write-Host "  ✅ FFmpeg found: $ffmpegVersion" -ForegroundColor Green
} catch {
    Write-Host "  ❌ FFmpeg not found in PATH" -ForegroundColor Red
    Write-Host "     Please install FFmpeg: https://ffmpeg.org/download.html" -ForegroundColor Yellow
    exit 1
}
Write-Host ""

# 3. Check D3D11VA support
Write-Host "[3/5] Checking D3D11VA hardware support..." -ForegroundColor Yellow
$d3d11Check = & ffmpeg -hide_banner -hwaccels 2>&1 | Select-String "d3d11va"
if ($d3d11Check) {
    Write-Host "  ✅ D3D11VA supported by FFmpeg" -ForegroundColor Green
} else {
    Write-Host "  ❌ D3D11VA not supported" -ForegroundColor Red
    Write-Host "     Your FFmpeg build may not include D3D11VA support" -ForegroundColor Yellow
    exit 1
}
Write-Host ""

# 4. Build SAide (release mode)
Write-Host "[4/5] Building SAide (release)..." -ForegroundColor Yellow
$buildOutput = cargo build --release 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "  ❌ Build failed:" -ForegroundColor Red
    Write-Host $buildOutput -ForegroundColor Red
    exit 1
}
Write-Host "  ✅ Build successful" -ForegroundColor Green
Write-Host ""

# 5. Run SAide with D3D11VA logging
Write-Host "[5/5] Testing D3D11VA decoder..." -ForegroundColor Yellow
Write-Host "     Starting SAide with verbose logging..." -ForegroundColor Gray
Write-Host "     (Press Ctrl+C to stop after 10 seconds)" -ForegroundColor Gray
Write-Host ""

$env:RUST_LOG = "debug"
$process = Start-Process -FilePath ".\target\release\saide.exe" -NoNewWindow -PassThru -RedirectStandardError "d3d11va_test.log"

Start-Sleep -Seconds 10

if (!$process.HasExited) {
    Stop-Process -Id $process.Id -Force
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Test Results" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Analyze log
$logContent = Get-Content "d3d11va_test.log" -Raw

if ($logContent -match "D3D11VA device context created successfully") {
    Write-Host "  ✅ D3D11VA device context created" -ForegroundColor Green
} else {
    Write-Host "  ❌ D3D11VA device context creation failed" -ForegroundColor Red
}

if ($logContent -match "D3D11VA hardware support verified") {
    Write-Host "  ✅ Hardware support verified" -ForegroundColor Green
} else {
    Write-Host "  ❌ Hardware support verification failed" -ForegroundColor Red
}

if ($logContent -match "Using D3D11VA hardware decoder") {
    Write-Host "  ✅ D3D11VA decoder selected" -ForegroundColor Green
} else {
    Write-Host "  ⚠️  D3D11VA decoder NOT selected" -ForegroundColor Yellow
}

if ($logContent -match "Failed setup for format d3d11") {
    Write-Host "  ❌ D3D11VA decode initialization failed" -ForegroundColor Red
    Write-Host "     This indicates AMD GPU compatibility issues" -ForegroundColor Yellow
} elseif ($logContent -match "Decoded frame \(D3D11VA\)") {
    Write-Host "  ✅ D3D11VA decode successful" -ForegroundColor Green
}

if ($logContent -match "consecutive.*failures") {
    Write-Host "  ❌ Consecutive decode failures detected" -ForegroundColor Red
}

Write-Host ""
Write-Host "Full log saved to: d3d11va_test.log" -ForegroundColor Gray

if ($Verbose) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "Detailed Log Output" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Get-Content "d3d11va_test.log" | Select-String "D3D11VA|decoder|Failed|error" | ForEach-Object {
        Write-Host $_ -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "Recommendation:" -ForegroundColor Cyan
if ($logContent -match "Using D3D11VA hardware decoder" -and $logContent -match "Decoded frame \(D3D11VA\)") {
    Write-Host "  ✅ Your AMD GPU fully supports D3D11VA hardware acceleration!" -ForegroundColor Green
} elseif ($logContent -match "D3D11VA unavailable, falling back to software") {
    Write-Host "  ⚠️  D3D11VA not working, using software decoder" -ForegroundColor Yellow
    Write-Host "     1. Update AMD GPU drivers to latest version" -ForegroundColor Gray
    Write-Host "     2. If issue persists, disable hwdecode in config.toml:" -ForegroundColor Gray
    Write-Host "        [scrcpy.video]" -ForegroundColor Gray
    Write-Host "        hwdecode = false" -ForegroundColor Gray
} else {
    Write-Host "  ⚠️  Inconclusive results, check d3d11va_test.log manually" -ForegroundColor Yellow
}

Write-Host ""
