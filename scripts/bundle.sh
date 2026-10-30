#!/bin/sh
# Construit dist/Darkstar.app (icône, Info.plist, signature ad hoc) et dist/Darkstar-macos.zip.
set -e
cd "$(dirname "$0")/.."
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/Rust/toolchain/rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/Rust/toolchain/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release

APP=dist/Darkstar.app
rm -rf dist
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/darkstar_sim "$APP/Contents/MacOS/"

ICONSET=dist/Darkstar.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z $d $d assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Darkstar.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Darkstar</string>
    <key>CFBundleDisplayName</key><string>Darkstar</string>
    <key>CFBundleExecutable</key><string>darkstar_sim</string>
    <key>CFBundleIdentifier</key><string>com.sandro642.darkstar</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleIconFile</key><string>Darkstar</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.simulation-games</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep -s - "$APP"
ditto -c -k --keepParent "$APP" dist/Darkstar-macos.zip
echo "Darkstar.app v$VERSION -> dist/"
