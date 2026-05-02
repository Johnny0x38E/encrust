#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_NAME="Encrust"
EXECUTABLE_NAME="encrust"

VERSION=$(cd "$PROJECT_ROOT" && cargo pkgid | sed 's/.*#//')

echo "=========================================="
echo "Encrust Linux 打包脚本"
echo "版本: $VERSION"
echo "=========================================="

cargo build --release

# 准备 AppDir
APPDIR="$PROJECT_ROOT/target/release/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"

cp "$PROJECT_ROOT/target/release/$EXECUTABLE_NAME" "$APPDIR/usr/bin/"

# 复制图标（如果存在）
ICON_SRC="$PROJECT_ROOT/assets/appicon.png"
if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$APPDIR/$EXECUTABLE_NAME.png"
    echo "已复制应用图标: $(basename "$ICON_SRC")"
else
    echo "未找到 $ICON_SRC，AppImage 将使用默认图标"
fi

# 创建 .desktop 文件
cat > "$APPDIR/$EXECUTABLE_NAME.desktop" <<EOF
[Desktop Entry]
Name=$APP_NAME
Exec=$EXECUTABLE_NAME
Icon=$EXECUTABLE_NAME
Type=Application
Categories=Utility;Security;
EOF

# 创建 AppRun 脚本
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
export PATH="${HERE}/usr/bin:${PATH}"
exec "${HERE}/usr/bin/encrust" "$@"
EOF
chmod +x "$APPDIR/AppRun"
chmod +x "$APPDIR/usr/bin/$EXECUTABLE_NAME"

# 获取 appimagetool
APPIMAGETOOL=""
if command -v appimagetool &> /dev/null; then
    APPIMAGETOOL="appimagetool"
    echo "使用系统 appimagetool"
else
    APPIMAGETOOL_CACHE="$PROJECT_ROOT/target/release/appimagetool"
    if [ ! -f "$APPIMAGETOOL_CACHE" ]; then
        echo "正在下载 appimagetool..."
        wget -q "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" -O "$APPIMAGETOOL_CACHE"
        chmod +x "$APPIMAGETOOL_CACHE"
    fi
    APPIMAGETOOL="$APPIMAGETOOL_CACHE"
    echo "使用缓存的 appimagetool"
fi

APPIMAGE_PATH="$PROJECT_ROOT/target/release/${APP_NAME}-${VERSION}-x86_64.AppImage"
rm -f "$APPIMAGE_PATH"

echo ""
echo "正在创建 AppImage..."
"$APPIMAGETOOL" "$APPDIR" "$APPIMAGE_PATH"

rm -rf "$APPDIR"

echo ""
echo "=========================================="
echo "AppImage 打包完成"
echo "输出路径: $APPIMAGE_PATH"
echo "=========================================="
