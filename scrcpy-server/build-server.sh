#!/usr/bin/env bash
# Build scrcpy-server.jar from source (v3.3.4)
# Requires: Android SDK, Java 17+, ANDROID_HOME or ANDROID_SDK_ROOT
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT=""

usage() {
    echo "Usage: $0 [-o OUTPUT_JAR] [--check]"
    echo "  -o    Output jar path (default: $SCRIPT_DIR/scrcpy-server.jar)"
    echo "  --check  Only verify dependencies, don't build"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -o) OUTPUT="$2"; shift 2 ;;
        --check) CHECK_ONLY=1; shift ;;
        *) usage ;;
    esac
done

OUTPUT="${OUTPUT:-$SCRIPT_DIR/scrcpy-server.jar}"

# Check dependencies
if [ -z "${ANDROID_HOME:-}" ] && [ -z "${ANDROID_SDK_ROOT:-}" ]; then
    echo "ERROR: ANDROID_HOME or ANDROID_SDK_ROOT must be set" >&2
    exit 1
fi

if ! command -v java &>/dev/null; then
    echo "ERROR: java not found in PATH" >&2
    exit 1
fi
JAVA_VER=$(java -version 2>&1 | head -1 | grep -oP '\d+' | head -1 || echo "0")
if [ "$JAVA_VER" -lt 17 ]; then
    echo "WARNING: Java 17+ recommended, found version $JAVA_VER" >&2
fi

if [ "${CHECK_ONLY:-0}" = "1" ]; then
    echo "Dependencies check passed."
    exit 0
fi

echo "Building scrcpy-server (v3.3.4)..."
cd "$SCRIPT_DIR"

# Make gradlew executable if needed
[ -x ./gradlew ] || chmod +x ./gradlew

# Build
./gradlew assembleRelease 2>&1

# Copy output jar
BUILT_JAR="build/outputs/apk/release/scrcpy-server-release-unsigned.apk"
if [ -f "$BUILT_JAR" ]; then
    cp "$BUILT_JAR" "$OUTPUT"
    echo "Built: $OUTPUT"
else
    echo "ERROR: Build output not found at $BUILT_JAR" >&2
    exit 1
fi
