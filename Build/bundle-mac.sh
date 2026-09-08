#!/usr/bin/env bash
# Wrap the release binary in a thin macOS .app so it gets a Dock icon and a
# proper name. This is a *thin* bundle: it does NOT vendor the GTK /
# libadwaita dylibs, so it runs only on a machine that already has them
# (e.g. Homebrew `gtk4` + `libadwaita`). That matches how Rhymr is built
# and published manually — full redistribution would need `dylibbundler`
# and rpath fixing, which is out of scope here.
#
# Windows: no equivalent yet — a .ico embedded via a build script is the
# path, deferred until it can be tested.
#
# Usage:  ./Build/bundle-mac.sh        -> dist/Rhymr.app
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

app_name="Rhymr"
bundle_id="org.gtk_rs.Rhymr"
bin_name="rhymr-rs"
svg="assets/icons/rhymr-icon.svg"

out="dist/${app_name}.app"
contents="${out}/Contents"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "error: rsvg-convert not found (install librsvg: brew install librsvg)" >&2
  exit 1
fi

echo "==> building release binary"
cargo build --release

echo "==> rendering icon"
iconset="$(mktemp -d)/${app_name}.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  rsvg-convert -w "$size"          -h "$size"          "$svg" -o "$iconset/icon_${size}x${size}.png"
  rsvg-convert -w "$((size * 2))"  -h "$((size * 2))"  "$svg" -o "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$(dirname "$iconset")/${app_name}.icns"

echo "==> assembling ${out}"
rm -rf "$out"
mkdir -p "${contents}/MacOS" "${contents}/Resources"
cp "target/release/${bin_name}" "${contents}/MacOS/${bin_name}"
cp "$(dirname "$iconset")/${app_name}.icns" "${contents}/Resources/${app_name}.icns"

version="$(./Build/version.sh 2>/dev/null || echo 0.0.0)"

cat > "${contents}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>            <string>${app_name}</string>
	<key>CFBundleDisplayName</key>     <string>${app_name}</string>
	<key>CFBundleIdentifier</key>      <string>${bundle_id}</string>
	<key>CFBundleExecutable</key>      <string>${bin_name}</string>
	<key>CFBundleIconFile</key>        <string>${app_name}</string>
	<key>CFBundlePackageType</key>     <string>APPL</string>
	<key>CFBundleShortVersionString</key> <string>${version}</string>
	<key>CFBundleVersion</key>         <string>${version}</string>
	<key>NSHighResolutionCapable</key> <true/>
	<key>LSMinimumSystemVersion</key>  <string>11.0</string>
</dict>
</plist>
PLIST

echo "==> done: ${out}"
echo "    open '${out}'   (needs Homebrew gtk4 + libadwaita on the machine)"
