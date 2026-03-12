#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0

set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=${TARGET:?TARGET is required}
BINARY_NAME=${BINARY_NAME:-saide}
BINARY_PATH=${BINARY_PATH:?BINARY_PATH is required}
DIST_DIR=${DIST_DIR:-"$ROOT_DIR/dist"}
ARCHIVE_BASENAME=${ARCHIVE_BASENAME:-"saide-$TARGET"}
ARCHIVE_FORMAT=${ARCHIVE_FORMAT:-tar.gz}
PACKAGE_DIR_NAME=${PACKAGE_DIR_NAME:-"$ARCHIVE_BASENAME"}
STAGE_DIR=${STAGE_DIR:-"$DIST_DIR/stage/$PACKAGE_DIR_NAME"}
EXTRA_RUNTIME_LIB_DIRS=${EXTRA_RUNTIME_LIB_DIRS:-}
SCRCPY_SERVER_ASSET_PATH=${SCRCPY_SERVER_ASSET_PATH:-}
SCRCPY_SERVER_ASSET_NAME=${SCRCPY_SERVER_ASSET_NAME:-}

if [[ ! -f "$BINARY_PATH" ]]; then
    printf 'Binary not found: %s\n' "$BINARY_PATH" >&2
    exit 1
fi

mkdir -p "$STAGE_DIR/lib" "$DIST_DIR"
rm -rf "${STAGE_DIR:?}"/*
mkdir -p "$STAGE_DIR/lib"

copy_if_exists() {
    local source_path=$1
    local destination_path=$2

    if [[ -f "$source_path" ]]; then
        mkdir -p "$(dirname -- "$destination_path")"
        cp -f "$source_path" "$destination_path"
    fi
}

copy_extra_runtime_libs_linux() {
    local lib_dir=$1
    local source_dir
    local lib_path
    local -a extra_dirs

    if [[ -z "$EXTRA_RUNTIME_LIB_DIRS" ]]; then
        return 0
    fi

    IFS=':' read -r -a extra_dirs <<<"$EXTRA_RUNTIME_LIB_DIRS"

    for source_dir in "${extra_dirs[@]}"; do
        [[ -d "$source_dir" ]] || continue

        shopt -s nullglob
        for lib_path in "$source_dir"/*.so*; do
            [[ -f "$lib_path" || -L "$lib_path" ]] || continue
            cp -Lf "$lib_path" "$lib_dir/"
        done
        shopt -u nullglob
    done
}

collect_runtime_deps_linux() {
    local input_path=$1
    local ldd_output

    if ! ldd_output=$(LD_LIBRARY_PATH="$STAGE_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ldd "$input_path" 2>/dev/null); then
        return 0
    fi

    printf '%s\n' "$ldd_output" |
        awk '
            /=>/ && $3 ~ /^\// { print $3 }
            /^[[:space:]]*\// { print $1 }
        ' |
        sort -u
}

copy_runtime_deps_linux() {
    local binary=$1
    local lib_dir=$2
    local dep_path

    while IFS= read -r dep_path; do
        [[ -z "$dep_path" ]] && continue

        case "$(basename -- "$dep_path")" in
        linux-vdso.so.* | ld-linux-*.so* | libc.so.* | libm.so.* | libpthread.so.* | libdl.so.* | librt.so.* | libgcc_s.so.*)
            continue
            ;;
        esac

        cp -Lf "$dep_path" "$lib_dir/"
    done < <(collect_runtime_deps_linux "$binary")

}

copy_transitive_runtime_libs_linux() {
    local lib_dir=$1
    local changed=1
    local lib_path
    local dep_path

    while [[ $changed -eq 1 ]]; do
        changed=0
        shopt -s nullglob
        for lib_path in "$lib_dir"/*.so*; do
            [[ -f "$lib_path" ]] || continue

            while IFS= read -r dep_path; do
                [[ -z "$dep_path" ]] && continue

                case "$(basename -- "$dep_path")" in
                linux-vdso.so.* | ld-linux-*.so* | libc.so.* | libm.so.* | libpthread.so.* | libdl.so.* | librt.so.* | libgcc_s.so.*)
                    continue
                    ;;
                esac

                if [[ ! -e "$lib_dir/$(basename -- "$dep_path")" ]]; then
                    cp -Lf "$dep_path" "$lib_dir/"
                    changed=1
                fi
            done < <(collect_runtime_deps_linux "$lib_path")
        done
        shopt -u nullglob
    done

}

patch_runtime_rpaths_linux() {
    local binary=$1
    local lib_dir=$2
    local lib_path

    if ! command -v patchelf >/dev/null 2>&1; then
        return 0
    fi

    patchelf --set-rpath '$ORIGIN/lib' "$binary"

    shopt -s nullglob
    for lib_path in "$lib_dir"/*.so*; do
        [[ -f "$lib_path" ]] || continue
        patchelf --set-rpath '$ORIGIN' "$lib_path"
    done
    shopt -u nullglob
}

copy_runtime_deps_windows() {
    local output_dir=$1
    local source_dir

    if [[ -n "${VCPKG_INSTALLED_DIR:-}" ]]; then
        source_dir="$VCPKG_INSTALLED_DIR"
    elif [[ -n "${VCPKG_ROOT:-}" ]]; then
        source_dir="$VCPKG_ROOT/installed/x64-windows"
    else
        return 0
    fi

    if [[ -d "$source_dir/bin" ]]; then
        cp -f "$source_dir/bin"/*.dll "$output_dir/" 2>/dev/null || true
    fi
}

copy_if_exists "$BINARY_PATH" "$STAGE_DIR/$BINARY_NAME"
copy_if_exists "$ROOT_DIR/README.md" "$STAGE_DIR/README.md"
copy_if_exists "$ROOT_DIR/LICENSE-MIT" "$STAGE_DIR/LICENSE-MIT"
copy_if_exists "$ROOT_DIR/LICENSE-APACHE" "$STAGE_DIR/LICENSE-APACHE"

if [[ -n "$SCRCPY_SERVER_ASSET_PATH" ]]; then
    scrcpy_asset_name=${SCRCPY_SERVER_ASSET_NAME:-$(basename -- "$SCRCPY_SERVER_ASSET_PATH")}
    copy_if_exists "$SCRCPY_SERVER_ASSET_PATH" "$STAGE_DIR/$scrcpy_asset_name"
fi

case "$TARGET" in
x86_64-unknown-linux-gnu)
    copy_runtime_deps_linux "$STAGE_DIR/$BINARY_NAME" "$STAGE_DIR/lib"
    copy_extra_runtime_libs_linux "$STAGE_DIR/lib"
    copy_transitive_runtime_libs_linux "$STAGE_DIR/lib"
    patch_runtime_rpaths_linux "$STAGE_DIR/$BINARY_NAME" "$STAGE_DIR/lib"
    ;;
x86_64-pc-windows-msvc)
    copy_runtime_deps_windows "$STAGE_DIR"
    ;;
esac

if [[ -d "$STAGE_DIR/lib" ]] && [[ -z "$(ls -A "$STAGE_DIR/lib")" ]]; then
    rmdir "$STAGE_DIR/lib"
fi

archive_path="$DIST_DIR/$ARCHIVE_BASENAME"
case "$ARCHIVE_FORMAT" in
zip)
    archive_path+='.zip'
    rm -f "$archive_path"
    if command -v zip >/dev/null 2>&1; then
        (
            cd "$DIST_DIR/stage"
            zip -qr "$archive_path" "$PACKAGE_DIR_NAME"
        )
    else
        python3 - "$DIST_DIR/stage" "$PACKAGE_DIR_NAME" "$archive_path" <<'PY'
import pathlib
import sys
import zipfile

stage_dir = pathlib.Path(sys.argv[1])
package_dir = pathlib.Path(sys.argv[2])
archive_path = pathlib.Path(sys.argv[3])

with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for path in (stage_dir / package_dir).rglob("*"):
        archive.write(path, path.relative_to(stage_dir))
PY
    fi
    ;;
tar.gz)
    archive_path+='.tar.gz'
    tar -C "$DIST_DIR/stage" -czf "$archive_path" "$PACKAGE_DIR_NAME"
    ;;
*)
    printf 'Unsupported archive format: %s\n' "$ARCHIVE_FORMAT" >&2
    exit 1
    ;;
esac
if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf 'ARCHIVE_PATH=%s\n' "$archive_path" >>"$GITHUB_ENV"
else
    printf 'ARCHIVE_PATH=%s\n' "$archive_path"
fi
