#!/bin/sh
# Local RNDR app bundle; no daemon install, signing identity, or app restart.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
[ "$(uname -s)" = Darwin ] || { echo 'macOS is required.' >&2; exit 1; }
TARGET="$ROOT/../.cache/cargo-target"
APP="$TARGET/bundle/Medousa.app"
ICON="$ROOT/apps/medousa-home/static/brand/app-icons/png/medousa-icon-1024.png"
# Fixed target directory keeps this independent of Cargo environment overrides.
cargo build --locked --manifest-path "$ROOT/apps/medousa-native/Cargo.toml" \
    --release --bin medousa-native --target-dir "$TARGET"
# Refuse to overwrite an existing bundle: it might be running or locally signed.
[ ! -e "$APP" ] || { echo "Bundle already exists: $APP. Move it aside before packaging again." >&2; exit 1; }
STAGE=$(mktemp -d "$TARGET/medousa-bundle.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
BUNDLE="$STAGE/Medousa.app"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources" "$STAGE/AppIcon.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON" --out "$STAGE/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$ICON" --out "$STAGE/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$STAGE/AppIcon.iconset" -o "$BUNDLE/Contents/Resources/AppIcon.icns"
cp "$TARGET/release/medousa-native" "$BUNDLE/Contents/MacOS/medousa-native"
VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/apps/medousa-native/Cargo.toml" | head -1)
cat > "$BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Medousa</string>
<key>CFBundleDisplayName</key><string>Medousa</string>
<key>CFBundleIdentifier</key><string>app.medousa.native</string>
<key>CFBundleExecutable</key><string>medousa-native</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>AppIcon.icns</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
plutil -lint "$BUNDLE/Contents/Info.plist"
# Ad-hoc sign this local build, not a Developer ID/notarized distribution.
codesign --force --sign - "$BUNDLE"
codesign --verify --strict "$BUNDLE"
mkdir -p "$TARGET/bundle"
mv "$BUNDLE" "$APP"
printf 'Packaged: %s\nQuit the current app normally, then open this bundle.\n' "$APP"
