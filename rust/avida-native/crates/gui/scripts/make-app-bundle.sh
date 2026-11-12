#!/bin/sh
# Wraps the release avida-gui binary into Avida.app.
#
# macOS starts a bundled app through LaunchServices, in the logged-in
# desktop session, whatever shell asked for it. A bare binary started
# from a shell outside that session (inside tmux, or over ssh:
# `launchctl managername` prints "Background" there) cannot take the
# focus, and its window opens behind the others. The bundle does not
# have that problem:
#
#     crates/gui/scripts/make-app-bundle.sh /tmp/avida-app
#     open /tmp/avida-app/Avida.app --args "$PWD/config" -s 7
#
# Everything after --args goes to avida-gui unchanged, as on its command
# line. LaunchServices starts the app in /, so give the experiment folder
# as an absolute path. `open -n` starts a second copy; `open -W` waits
# for it to quit; `open --stderr "$(tty)"` shows its log in this
# terminal.
#
# usage: make-app-bundle.sh [--binary PATH] [--bundle-id ID] OUTPUT_DIR
#
#   --binary PATH     the avida-gui executable to wrap (default:
#                     $CARGO_TARGET_DIR/release/avida-gui, or
#                     target/release/avida-gui in the workspace)
#   --bundle-id ID    CFBundleIdentifier (default: org.avida.avida-gui)
#
# Build the binary first: cargo build --release -p avida-gui

set -eu

usage() {
    sed -n 's/^# \{0,1\}//; /^usage:/,/^Build/p' "$0"
}

script_dir=$(cd "$(dirname "$0")" && pwd)
workspace=$(cd "$script_dir/../../.." && pwd)
binary=""
bundle_id="org.avida.avida-gui"
out=""

while [ $# -gt 0 ]; do
    case "$1" in
        --binary) binary=${2:?--binary needs a path}; shift 2 ;;
        --bundle-id) bundle_id=${2:?--bundle-id needs an identifier}; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "make-app-bundle.sh: unknown option $1" >&2; usage >&2; exit 2 ;;
        *)
            if [ -n "$out" ]; then
                echo "make-app-bundle.sh: one output folder, please" >&2
                exit 2
            fi
            out=$1; shift ;;
    esac
done

if [ -z "$out" ]; then
    usage >&2
    exit 2
fi
if [ -z "$binary" ]; then
    binary="${CARGO_TARGET_DIR:-$workspace/target}/release/avida-gui"
fi
if [ ! -x "$binary" ]; then
    echo "make-app-bundle.sh: no executable at $binary" >&2
    echo "  build it with: cargo build --release -p avida-gui" >&2
    exit 1
fi

# The workspace version, from [workspace.package] in the root Cargo.toml.
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' \
    "$workspace/Cargo.toml" | head -n 1)
version=${version:-0.0.0}

app="$out/Avida.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/avida-gui"
chmod 755 "$app/Contents/MacOS/avida-gui"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Avida</string>
    <key>CFBundleExecutable</key>
    <string>avida-gui</string>
    <key>CFBundleIdentifier</key>
    <string>$bundle_id</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Avida</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.education</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST
printf 'APPL????' > "$app/Contents/PkgInfo"

if command -v plutil >/dev/null 2>&1; then
    plutil -lint -s "$app/Contents/Info.plist"
fi
# Sign the bundle as a whole (ad hoc), so its signature covers Info.plist
# as well as the executable the linker signed on its own.
if command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - "$app" 2>/dev/null ||
        echo "make-app-bundle.sh: ad-hoc signing failed; the bundle may still open" >&2
fi

echo "$app"
