#!/usr/bin/env bash
#
# Build pi binaries for all platforms locally.
# Mirrors .github/workflows/build-binaries.yml
#
# Usage:
#   ./scripts/build-binaries.sh [--skip-install] [--skip-deps] [--skip-build] [--offline-model-data] [--platform <platform>] [--out <dir>]
#
# Native sidecar contract:
#   每个发布目录都必须携带与 archive 目标平台、CPU 架构一致的 Rust runtime。不能复用
#   构建机的二进制；否则 manifest 的平台字段正确时仍可能在用户机器上无法执行。
#
# Options:
#   --skip-install       Skip npm ci
#   --skip-deps          Skip installing cross-platform dependencies
#   --skip-build         Skip the package build
#   --offline-model-data Build with bundled model data instead of refreshing it
#   --platform <name>    Build only for specified platform (darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64, windows-arm64)
#   --out <dir>          Output directory (default: packages/coding-agent/binaries)
#
# Output:
#   packages/coding-agent/binaries/
#     pi-darwin-arm64.tar.gz
#     pi-darwin-x64.tar.gz
#     pi-linux-x64.tar.gz
#     pi-linux-arm64.tar.gz
#     pi-windows-x64.zip
#     pi-windows-arm64.zip

set -euo pipefail

cd "$(dirname "$0")/.."

SKIP_INSTALL=false
SKIP_DEPS=false
SKIP_BUILD=false
OFFLINE_MODEL_DATA=false
PLATFORM=""
OUTPUT_DIR=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-install)
            SKIP_INSTALL=true
            shift
            ;;
        --skip-deps)
            SKIP_DEPS=true
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --offline-model-data)
            OFFLINE_MODEL_DATA=true
            shift
            ;;
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        --out)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validate platform if specified
if [[ -n "$PLATFORM" ]]; then
    case "$PLATFORM" in
        darwin-arm64|darwin-x64|linux-x64|linux-arm64|windows-x64|windows-arm64)
            ;;
        *)
            echo "Invalid platform: $PLATFORM"
            echo "Valid platforms: darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64, windows-arm64"
            exit 1
            ;;
    esac
fi

if [[ -z "$OUTPUT_DIR" ]]; then
    OUTPUT_DIR="packages/coding-agent/binaries"
fi
if [[ "$OUTPUT_DIR" != /* ]]; then
    OUTPUT_DIR="$(pwd)/$OUTPUT_DIR"
fi

if [[ "$SKIP_INSTALL" == "false" ]]; then
    echo "==> Installing dependencies..."
    npm ci --ignore-scripts
else
    echo "==> Skipping npm ci (--skip-install)"
fi

if [[ "$SKIP_DEPS" == "false" ]]; then
    echo "==> Installing cross-platform native bindings..."
    CLIPBOARD_VERSION=$(node -p "require('./packages/coding-agent/package.json').optionalDependencies['@mariozechner/clipboard']")
    # npm ci only installs optional deps for the current platform. Install the
    # cross-platform packages in isolation so npm does not re-resolve and mutate
    # the workspace dependency graph, which can trigger npm/arborist failures.
    NATIVE_DEPS_DIR=$(mktemp -d)
    cleanup_native_deps() {
        rm -rf "$NATIVE_DEPS_DIR"
    }
    trap cleanup_native_deps EXIT
    printf '%s\n' '{"private":true}' > "$NATIVE_DEPS_DIR/package.json"
    # Use --force to bypass platform checks (os/cpu restrictions in package.json).
    npm install --prefix "$NATIVE_DEPS_DIR" --include=optional --no-save --package-lock=false --force --ignore-scripts \
        @mariozechner/clipboard@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-darwin-arm64@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-darwin-x64@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-linux-x64-gnu@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-linux-arm64-gnu@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-win32-x64-msvc@"$CLIPBOARD_VERSION" \
        @mariozechner/clipboard-win32-arm64-msvc@"$CLIPBOARD_VERSION"
    mkdir -p node_modules/@mariozechner
    for package in \
        clipboard \
        clipboard-darwin-arm64 \
        clipboard-darwin-x64 \
        clipboard-linux-x64-gnu \
        clipboard-linux-arm64-gnu \
        clipboard-win32-x64-msvc \
        clipboard-win32-arm64-msvc; do
        rm -rf "node_modules/@mariozechner/$package"
        cp -R "$NATIVE_DEPS_DIR/node_modules/@mariozechner/$package" node_modules/@mariozechner/
    done
    cleanup_native_deps
    trap - EXIT
else
    echo "==> Skipping cross-platform native bindings (--skip-deps)"
fi

if [[ "$SKIP_BUILD" == "false" ]]; then
    if [[ "$OFFLINE_MODEL_DATA" == "true" ]]; then
        echo "==> Building all packages with bundled model data..."
        npm run build:offline
    else
        echo "==> Building all packages..."
        npm run build
    fi
else
    echo "==> Skipping package build (--skip-build)"
fi

echo "==> Building binaries..."

# 先构建 Provider Host。它是纯 JavaScript payload，所有目标平台共用同一份已编译入口；
# Rust runtime 则必须在下方按 platform 分别构建。
npm run build --workspace @mariozechner/provider-host
cd packages/coding-agent
# archive 创建阶段会切换到输出目录，故在此固化验证脚本的绝对路径，绝不依赖当时 cwd。
MANIFEST_WRITER="$(cd ../.. && pwd)/scripts/write-native-runtime-manifest.mjs"

# Clean previous builds
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"/{darwin-arm64,darwin-x64,linux-x64,linux-arm64,windows-x64,windows-arm64}

# Determine which platforms to build
if [[ -n "$PLATFORM" ]]; then
    PLATFORMS=("$PLATFORM")
else
    PLATFORMS=(darwin-arm64 darwin-x64 linux-x64 linux-arm64 windows-x64 windows-arm64)
fi

# 由发布目标映射到 Rust target triple。此映射是 release layout 的唯一事实来源，避免 Bun
# target 与 sidecar target 分散维护后悄然产生架构错配。
rust_target_for_platform() {
    case "$1" in
        darwin-arm64) echo "aarch64-apple-darwin" ;;
        darwin-x64) echo "x86_64-apple-darwin" ;;
        linux-x64) echo "x86_64-unknown-linux-gnu" ;;
        linux-arm64) echo "aarch64-unknown-linux-gnu" ;;
        windows-x64) echo "x86_64-pc-windows-gnu" ;;
        windows-arm64) echo "aarch64-pc-windows-gnullvm" ;;
    esac
}

native_binary_name_for_platform() {
    if [[ "$1" == windows-* ]]; then
        echo "pi-runtime.exe"
    else
        echo "pi-runtime"
    fi
}

runtime_platform_for_release_platform() {
    case "$1" in
        darwin-*) echo "darwin" ;;
        linux-*) echo "linux" ;;
        windows-*) echo "win32" ;;
    esac
}

build_native_runtime() {
    local platform="$1"
    local rust_target
    rust_target="$(rust_target_for_platform "$platform")"
    local binary_name
    binary_name="$(native_binary_name_for_platform "$platform")"

    echo "Building native runtime for $platform ($rust_target)..."
    # 宿主 target 直接用 Cargo，保证开发机只构建本机 archive 时无需额外安装 Zig。其余 target
    # 使用 cargo-zigbuild 取得确定的交叉链接行为；缺失时明确失败而不是复制构建机二进制。
    if [[ "$(rustc -vV | sed -n 's/^host: //p')" == "$rust_target" ]]; then
        cargo build --release --target "$rust_target" --package cli --bin pi-runtime --manifest-path ../../makima-runtime/Cargo.toml
    else
        command -v cargo-zigbuild >/dev/null 2>&1 || {
            echo "cargo-zigbuild is required to build the native runtime for $platform" >&2
            exit 1
        }
        cargo zigbuild --release --target "$rust_target" --package cli --bin pi-runtime --manifest-path ../../makima-runtime/Cargo.toml
    fi
    cp "../../makima-runtime/target/$rust_target/release/$binary_name" "$OUTPUT_DIR/$platform/$binary_name"
}

for platform in "${PLATFORMS[@]}"; do
    echo "Building for $platform..."
    bun_target="bun-$platform"
    if [[ "$platform" == *-x64 ]]; then
        bun_target="${bun_target}-baseline"
    fi

    # Bun compiled executables only embed indirect worker/Provider Host modules when they are passed as
    # explicit build entrypoints. Provider Host 子进程会重新执行同一个 Bun binary 并带上受控
    # `--provider-host-child` 标记；没有该 entry 时 archive native mode 会在 child 中缺少 SDK。
    #
    # Disable cwd bunfig.toml autoload so project preload scripts cannot crash the
    # standalone binary before pi starts (see #7684).
    if [[ "$platform" == windows-* ]]; then
        bun build --compile --no-compile-autoload-bunfig --target="$bun_target" ./dist/bun/cli.js ./src/utils/image-resize-worker.ts ./src/bun/provider-host-main.ts --outfile "$OUTPUT_DIR/$platform/pi.exe"
    else
        bun build --compile --no-compile-autoload-bunfig --target="$bun_target" ./dist/bun/cli.js ./src/utils/image-resize-worker.ts ./src/bun/provider-host-main.ts --outfile "$OUTPUT_DIR/$platform/pi"
    fi

    build_native_runtime "$platform"
done

echo "==> Creating release archives..."

# Copy shared files to each platform directory
for platform in "${PLATFORMS[@]}"; do
    cp package.json "$OUTPUT_DIR/$platform/"
    cp README.md "$OUTPUT_DIR/$platform/"
    cp CHANGELOG.md "$OUTPUT_DIR/$platform/"
    cp ../../node_modules/@silvia-odwyer/photon-node/photon_rs_bg.wasm "$OUTPUT_DIR/$platform/"

    # 固定 sidecar 布局与 Node package 完全一致。Provider Host 只复制已构建产物，避免 archive
    # 在运行时依赖工作区源文件或 node_modules。
    native_binary_name="$(native_binary_name_for_platform "$platform")"
    mkdir -p "$OUTPUT_DIR/$platform/native/provider-host"
    mv "$OUTPUT_DIR/$platform/$native_binary_name" "$OUTPUT_DIR/$platform/native/$native_binary_name"
    cp -R ../provider-host/dist/. "$OUTPUT_DIR/$platform/native/provider-host/"
    runtime_platform="$(runtime_platform_for_release_platform "$platform")"
    node "$MANIFEST_WRITER" \
        --native-dir "$OUTPUT_DIR/$platform/native" \
        --platform "$runtime_platform" \
        --executable "$native_binary_name" \
        --provider-host "provider-host/main.js"
    # 这里故意只做 hash/layout 验证：交叉目标无法在 Ubuntu runner 上执行。最终 release
    # archive 解压后仍会再次执行同一验证，防止压缩或解压过程遗漏 sidecar 文件。
    node -e 'import(process.argv[1]).then(({ validateNativeRuntimeManifest }) => validateNativeRuntimeManifest(process.argv[2], process.argv[3]))' \
        "$MANIFEST_WRITER" "$OUTPUT_DIR/$platform/native" "$runtime_platform"

    mkdir -p "$OUTPUT_DIR/$platform/theme"
    cp dist/modes/interactive/theme/*.json "$OUTPUT_DIR/$platform/theme/"
    mkdir -p "$OUTPUT_DIR/$platform/assets"
    cp dist/modes/interactive/assets/* "$OUTPUT_DIR/$platform/assets/"
    cp -r dist/core/export-html "$OUTPUT_DIR/$platform/"
    cp -r docs "$OUTPUT_DIR/$platform/"
    cp -r examples "$OUTPUT_DIR/$platform/"

    case "$platform" in
        darwin-arm64)
            clipboard_native_package="clipboard-darwin-arm64"
            clipboard_native_file="clipboard.darwin-arm64.node"
            ;;
        darwin-x64)
            clipboard_native_package="clipboard-darwin-x64"
            clipboard_native_file="clipboard.darwin-x64.node"
            ;;
        linux-x64)
            clipboard_native_package="clipboard-linux-x64-gnu"
            clipboard_native_file="clipboard.linux-x64-gnu.node"
            ;;
        linux-arm64)
            clipboard_native_package="clipboard-linux-arm64-gnu"
            clipboard_native_file="clipboard.linux-arm64-gnu.node"
            ;;
        windows-x64)
            clipboard_native_package="clipboard-win32-x64-msvc"
            clipboard_native_file="clipboard.win32-x64-msvc.node"
            ;;
        windows-arm64)
            clipboard_native_package="clipboard-win32-arm64-msvc"
            clipboard_native_file="clipboard.win32-arm64-msvc.node"
            ;;
    esac
    mkdir -p "$OUTPUT_DIR/$platform/node_modules/@mariozechner"
    cp -r ../../node_modules/@mariozechner/clipboard "$OUTPUT_DIR/$platform/node_modules/@mariozechner/"
    cp -r ../../node_modules/@mariozechner/$clipboard_native_package "$OUTPUT_DIR/$platform/node_modules/@mariozechner/"
    cp "../../node_modules/@mariozechner/$clipboard_native_package/$clipboard_native_file" \
        "$OUTPUT_DIR/$platform/node_modules/@mariozechner/clipboard/"

    # Copy terminal input native helpers next to compiled binaries.
    if [[ "$platform" == darwin-* ]]; then
        mkdir -p "$OUTPUT_DIR/$platform/native/darwin/prebuilds/$platform"
        cp ../tui/native/darwin/prebuilds/$platform/darwin-modifiers.node "$OUTPUT_DIR/$platform/native/darwin/prebuilds/$platform/"
    fi
    if [[ "$platform" == windows-* ]]; then
        if [[ "$platform" == "windows-arm64" ]]; then
            win32_arch_dir="win32-arm64"
        else
            win32_arch_dir="win32-x64"
        fi
        mkdir -p "$OUTPUT_DIR/$platform/native/win32/prebuilds/$win32_arch_dir"
        cp ../tui/native/win32/prebuilds/$win32_arch_dir/win32-console-mode.node "$OUTPUT_DIR/$platform/native/win32/prebuilds/$win32_arch_dir/"
    fi
done

# Create archives
cd "$OUTPUT_DIR"

for platform in "${PLATFORMS[@]}"; do
    if [[ "$platform" == windows-* ]]; then
        # Windows (zip)
        echo "Creating pi-$platform.zip..."
        (cd "$platform" && zip -r ../pi-$platform.zip .)
    else
        # Unix platforms (tar.gz) - use wrapper directory for mise compatibility
        echo "Creating pi-$platform.tar.gz..."
        mv "$platform" pi && tar -czf pi-$platform.tar.gz pi && mv pi "$platform"
    fi
done

# Extract archives for easy local testing
echo "==> Extracting archives for testing..."
for platform in "${PLATFORMS[@]}"; do
    rm -rf "$platform"
    if [[ "$platform" == windows-* ]]; then
        mkdir -p "$platform" && (cd "$platform" && unzip -q ../pi-$platform.zip)
    else
        tar -xzf pi-$platform.tar.gz && mv pi "$platform"
    fi

    runtime_platform="$(runtime_platform_for_release_platform "$platform")"
    node -e 'import(process.argv[1]).then(({ validateNativeRuntimeManifest }) => validateNativeRuntimeManifest(process.argv[2], process.argv[3]))' \
        "$MANIFEST_WRITER" "$platform/native" "$runtime_platform"
done

# 当前构建机只 smoke 其同平台 archive。跨平台发布物仍已在上方通过 manifest/hash 验证，
# 不会因为尝试运行异构二进制而把发布流程误判为失败。
host_os="$(uname -s)"
case "$host_os" in
    Darwin) host_runtime_platform="darwin" ;;
    Linux) host_runtime_platform="linux" ;;
    *) host_runtime_platform="" ;;
esac
if [[ -n "$host_runtime_platform" ]]; then
    host_arch="$(uname -m)"
    case "$host_arch" in
        arm64|aarch64) host_release_arch="arm64" ;;
        x86_64|amd64) host_release_arch="x64" ;;
        *) host_release_arch="" ;;
    esac
    if [[ -n "$host_release_arch" ]]; then
        host_archive_dir="$host_runtime_platform-$host_release_arch"
        if [[ " ${PLATFORMS[*]} " == *" $host_archive_dir "* ]]; then
            echo "==> Smoke testing extracted native sidecar for $host_archive_dir..."
            "$host_archive_dir/native/pi-runtime" --help >/dev/null
        fi
    fi
fi

echo ""
echo "==> Build complete!"
echo "Archives available in $OUTPUT_DIR/"
ls -lh *.tar.gz *.zip 2>/dev/null || true
echo ""
echo "Extracted directories for testing:"
for platform in "${PLATFORMS[@]}"; do
    if [[ "$platform" == windows-* ]]; then
        echo "  $OUTPUT_DIR/$platform/pi.exe"
    else
        echo "  $OUTPUT_DIR/$platform/pi"
    fi
done
