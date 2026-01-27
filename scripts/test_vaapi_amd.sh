#!/usr/bin/env bash
# Test script for AMD GPU VAAPI compatibility (Linux)
# 用于测试 AMD GPU 在 Linux 上的 VAAPI 硬件加速兼容性

set -euo pipefail

COLOR_RESET='\033[0m'
COLOR_CYAN='\033[0;36m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[0;33m'
COLOR_RED='\033[0;31m'
COLOR_GRAY='\033[0;90m'

echo -e "${COLOR_CYAN}========================================"
echo -e "AMD GPU VAAPI Compatibility Test"
echo -e "========================================${COLOR_RESET}"
echo ""

# 1. Detect GPU
echo -e "${COLOR_YELLOW}[1/5] Detecting GPU...${COLOR_RESET}"
if lspci | grep -iE "VGA|Display" | grep -iq "AMD\|ATI"; then
    gpu_info=$(lspci | grep -iE "VGA|Display" | grep -iE "AMD|ATI" | head -1)
    echo -e "  ${COLOR_GREEN}✅ AMD GPU detected: ${gpu_info}${COLOR_RESET}"
else
    echo -e "  ${COLOR_YELLOW}⚠️  Non-AMD GPU detected${COLOR_RESET}"
    echo -e "     ${COLOR_GRAY}This test is for AMD GPUs, but will continue anyway.${COLOR_RESET}"
fi
echo ""

# 2. Check VAAPI device
echo -e "${COLOR_YELLOW}[2/5] Checking VAAPI device...${COLOR_RESET}"
if [ -e /dev/dri/renderD128 ]; then
    echo -e "  ${COLOR_GREEN}✅ /dev/dri/renderD128 found${COLOR_RESET}"
else
    echo -e "  ${COLOR_RED}❌ No VAAPI device found in /dev/dri${COLOR_RESET}"
    echo -e "     ${COLOR_YELLOW}Please install mesa-va-drivers:${COLOR_RESET}"
    echo -e "     ${COLOR_GRAY}sudo apt install mesa-va-drivers  # Debian/Ubuntu${COLOR_RESET}"
    exit 1
fi
echo ""

# 3. Check FFmpeg VAAPI support
echo -e "${COLOR_YELLOW}[3/5] Checking FFmpeg VAAPI support...${COLOR_RESET}"
if ffmpeg -hide_banner -hwaccels 2>&1 | grep -q "vaapi"; then
    echo -e "  ${COLOR_GREEN}✅ VAAPI supported by FFmpeg${COLOR_RESET}"
else
    echo -e "  ${COLOR_RED}❌ VAAPI not supported${COLOR_RESET}"
    echo -e "     ${COLOR_YELLOW}Your FFmpeg build may not include VAAPI support${COLOR_RESET}"
    exit 1
fi

# Test VAAPI decode capability
if command -v vainfo &> /dev/null; then
    echo -e "  ${COLOR_GRAY}Running vainfo to check H.264 decode support...${COLOR_RESET}"
    if vainfo 2>&1 | grep -q "VAProfileH264"; then
        echo -e "  ${COLOR_GREEN}✅ H.264 hardware decode supported${COLOR_RESET}"
    else
        echo -e "  ${COLOR_YELLOW}⚠️  H.264 hardware decode may not be supported${COLOR_RESET}"
    fi
else
    echo -e "  ${COLOR_GRAY}vainfo not installed, skipping detailed check${COLOR_RESET}"
fi
echo ""

# 4. Build SAide (release mode)
echo -e "${COLOR_YELLOW}[4/5] Building SAide (release)...${COLOR_RESET}"
if cargo build --release &> /dev/null; then
    echo -e "  ${COLOR_GREEN}✅ Build successful${COLOR_RESET}"
else
    echo -e "  ${COLOR_RED}❌ Build failed${COLOR_RESET}"
    exit 1
fi
echo ""

# 5. Run SAide with VAAPI logging
echo -e "${COLOR_YELLOW}[5/5] Testing VAAPI decoder...${COLOR_RESET}"
echo -e "     ${COLOR_GRAY}Starting SAide with verbose logging...${COLOR_RESET}"
echo -e "     ${COLOR_GRAY}(Will auto-stop after 10 seconds)${COLOR_RESET}"
echo ""

RUST_LOG=debug timeout 10s ./target/release/saide 2> vaapi_test.log || true

echo ""
echo -e "${COLOR_CYAN}========================================"
echo -e "Test Results"
echo -e "========================================${COLOR_RESET}"
echo ""

# Analyze log
if grep -q "VAAPI device context created successfully" vaapi_test.log; then
    echo -e "  ${COLOR_GREEN}✅ VAAPI device context created${COLOR_RESET}"
else
    echo -e "  ${COLOR_RED}❌ VAAPI device context creation failed${COLOR_RESET}"
fi

if grep -q "Using VAAPI hardware decoder" vaapi_test.log; then
    echo -e "  ${COLOR_GREEN}✅ VAAPI decoder selected${COLOR_RESET}"
else
    echo -e "  ${COLOR_YELLOW}⚠️  VAAPI decoder NOT selected${COLOR_RESET}"
fi

if grep -q "Decoded frame (VAAPI)" vaapi_test.log; then
    echo -e "  ${COLOR_GREEN}✅ VAAPI decode successful${COLOR_RESET}"
fi

if grep -q "Failed to transfer frame from GPU" vaapi_test.log; then
    echo -e "  ${COLOR_RED}❌ GPU transfer failures detected${COLOR_RESET}"
fi

echo ""
echo -e "${COLOR_GRAY}Full log saved to: vaapi_test.log${COLOR_RESET}"

if [ "${1:-}" = "-v" ] || [ "${1:-}" = "--verbose" ]; then
    echo ""
    echo -e "${COLOR_CYAN}========================================"
    echo -e "Detailed Log Output"
    echo -e "========================================${COLOR_RESET}"
    grep -iE "VAAPI|decoder|Failed|error" vaapi_test.log | while IFS= read -r line; do
        echo -e "${COLOR_GRAY}${line}${COLOR_RESET}"
    done
fi

echo ""
echo -e "${COLOR_CYAN}Recommendation:${COLOR_RESET}"
if grep -q "Using VAAPI hardware decoder" vaapi_test.log && grep -q "Decoded frame (VAAPI)" vaapi_test.log; then
    echo -e "  ${COLOR_GREEN}✅ Your AMD GPU fully supports VAAPI hardware acceleration!${COLOR_RESET}"
elif grep -q "VAAPI unavailable, falling back to software" vaapi_test.log; then
    echo -e "  ${COLOR_YELLOW}⚠️  VAAPI not working, using software decoder${COLOR_RESET}"
    echo -e "     ${COLOR_GRAY}1. Update mesa drivers: sudo apt update && sudo apt upgrade${COLOR_RESET}"
    echo -e "     ${COLOR_GRAY}2. Verify /dev/dri/renderD128 permissions: ls -l /dev/dri${COLOR_RESET}"
    echo -e "     ${COLOR_GRAY}3. If issue persists, disable hwdecode in config.toml:${COLOR_RESET}"
    echo -e "        ${COLOR_GRAY}[scrcpy.video]${COLOR_RESET}"
    echo -e "        ${COLOR_GRAY}hwdecode = false${COLOR_RESET}"
else
    echo -e "  ${COLOR_YELLOW}⚠️  Inconclusive results, check vaapi_test.log manually${COLOR_RESET}"
fi

echo ""
