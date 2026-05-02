#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_NAME="Encrust"
EXECUTABLE_NAME="encrust"

VERSION=$(cd "$PROJECT_ROOT" && cargo pkgid | sed 's/.*#//')
UNIVERSAL_APP="$PROJECT_ROOT/target/release/${APP_NAME}.app"
DMG_PATH="$PROJECT_ROOT/target/release/${APP_NAME}-${VERSION}-macOS-universal.dmg"
ICON_ICNS="$PROJECT_ROOT/assets/appicon/appicon.icns"

# 应用图标交给 cargo-bundle 统一处理，避免运行时 Dock 图标和 Bundle 图标不一致。
echo "=========================================="
echo "Encrust macOS 打包脚本"
echo "版本: $VERSION"
echo "=========================================="

echo ""
echo "说明：应用图标来自 Cargo.toml 的 [package.metadata.bundle].icon"

# 安装 cargo-bundle（如已安装则跳过）
if ! cargo bundle --help > /dev/null 2>&1; then
    echo "正在安装 cargo-bundle..."
    cargo install cargo-bundle
fi

# 检查并安装目标平台
check_target() {
    local target=$1
    if ! rustup target list --installed | grep -q "$target"; then
        echo "正在安装 $target 目标..."
        rustup target add "$target"
    else
        echo "$target 目标已安装"
    fi
}

build_bundle() {
    local target=$1
    echo ""
    echo "正在打包 $target 版本..."
    (cd "$PROJECT_ROOT" && cargo bundle --release --format osx --target "$target")
}

if [ ! -f "$ICON_ICNS" ]; then
    echo "未找到 $ICON_ICNS，无法进行 macOS 打包"
    exit 1
fi

check_target "x86_64-apple-darwin"
check_target "aarch64-apple-darwin"

build_bundle "x86_64-apple-darwin"
build_bundle "aarch64-apple-darwin"

# 取出两个架构的可执行文件，合并成 Universal Binary
echo ""
echo "正在创建 Universal Binary..."
X86_APP="$PROJECT_ROOT/target/x86_64-apple-darwin/release/bundle/osx/${APP_NAME}.app"
ARM_APP="$PROJECT_ROOT/target/aarch64-apple-darwin/release/bundle/osx/${APP_NAME}.app"

rm -rf "$UNIVERSAL_APP"
# 以 ARM 版的 .app 为基础，保留 cargo-bundle 生成的 Info.plist、图标和 bundle 结构。
ditto "$ARM_APP" "$UNIVERSAL_APP"

lipo -create \
    "$X86_APP/Contents/MacOS/${EXECUTABLE_NAME}" \
    "$ARM_APP/Contents/MacOS/${EXECUTABLE_NAME}" \
    -output "$UNIVERSAL_APP/Contents/MacOS/${EXECUTABLE_NAME}"

echo "Universal Binary 信息："
lipo -info "$UNIVERSAL_APP/Contents/MacOS/${EXECUTABLE_NAME}"

# Image2icon 生成的 .icns 可能带 FinderInfo / ResourceFork / quarantine 等扩展属性，
# 这些属性会导致 codesign 拒绝签名。先递归清理最终 app bundle 再签名。
echo ""
echo "正在清理扩展属性并进行 ad-hoc 代码签名..."
xattr -cr "$UNIVERSAL_APP"
codesign --force --deep --sign - "$UNIVERSAL_APP"

echo ""
echo "=========================================="
echo "App Bundle 打包完成"
echo "输出路径: $UNIVERSAL_APP"
echo "=========================================="

echo ""
echo "签名验证："
codesign -dv "$UNIVERSAL_APP" 2>&1 | head -n 5 || true

echo ""
echo "架构验证："
file "$UNIVERSAL_APP/Contents/MacOS/${EXECUTABLE_NAME}"

# 创建 DMG
echo ""
rm -f "$DMG_PATH"

create_dmg_source() {
    local src
    src=$(mktemp -d)
    ditto "$UNIVERSAL_APP" "$src/${APP_NAME}.app"
    ln -s /Applications "$src/Applications"
    echo "$src"
}

DMG_SOURCE=$(create_dmg_source)
cleanup_dmg_source() {
    rm -rf "$DMG_SOURCE"
}
trap cleanup_dmg_source EXIT

echo "正在使用 hdiutil 创建 DMG..."
hdiutil create -volname "${APP_NAME} ${VERSION}" -srcfolder "$DMG_SOURCE" -ov -format UDZO "$DMG_PATH"

cleanup_dmg_source
trap - EXIT

if [ -f "$DMG_PATH" ]; then
    echo ""
    echo "=========================================="
    echo "DMG 打包完成"
    echo "输出路径: $DMG_PATH"
    echo "=========================================="
fi

echo ""
echo "全部完成！"
