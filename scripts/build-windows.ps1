$ErrorActionPreference = "Stop"

$PROJECT_ROOT = Split-Path -Parent $PSScriptRoot
$VERSION = (cargo pkgid | ForEach-Object { $_ -replace '.*#', '' })
$RELEASE_DIR = "$PROJECT_ROOT/target/release"
$APP_NAME = "Encrust"
$EXECUTABLE_NAME = "encrust"

echo "=========================================="
echo "Encrust Windows 打包脚本"
echo "版本: $VERSION"
echo "=========================================="

cargo build --release

# 创建分发目录
$DIST_DIR = "$RELEASE_DIR/windows_dist"
New-Item -ItemType Directory -Force -Path $DIST_DIR | Out-Null

Copy-Item "$RELEASE_DIR/$EXECUTABLE_NAME.exe" "$DIST_DIR/"

# 复制图标（如果存在）
$ICON_SRC = "$PROJECT_ROOT/assets/appicon.png"
if (Test-Path $ICON_SRC) {
    Copy-Item $ICON_SRC "$DIST_DIR/"
    echo "已复制应用图标: $(Split-Path -Leaf $ICON_SRC)"
} else {
    echo "未找到 $ICON_SRC，使用默认图标"
}

# 打包 ZIP
$ZIP_PATH = "$RELEASE_DIR/${APP_NAME}-${VERSION}-windows.zip"
if (Test-Path $ZIP_PATH) { Remove-Item $ZIP_PATH }
Compress-Archive -Path "$DIST_DIR/*" -DestinationPath $ZIP_PATH -Force

Remove-Item -Recurse -Force $DIST_DIR

echo ""
echo "=========================================="
echo "Windows ZIP 打包完成"
echo "输出路径: $ZIP_PATH"
echo "=========================================="
