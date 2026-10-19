#!/usr/bin/env bash
# package-macos.sh — join the two macOS slices into one unsigned universal
# .app bundle and zip it.
#
# Usage: ./scripts/package-macos.sh --version <version>
#   <version>  release version without the leading "v" (tag v0.1.0 -> 0.1.0).
#              It fills CFBundleShortVersionString/CFBundleVersion and the zip
#              name. The release workflow derives it from the tag.
#              Prerelease suffixes are passed through verbatim; Apple expects
#              dotted integers there, so cut releases with a plain vX.Y.Z tag.
#
# Expects both slices to be built already:
#   target/aarch64-apple-darwin/release/droid_tui
#   target/x86_64-apple-darwin/release/droid_tui
#
# Writes, relative to the repository root:
#   target/macos-universal/droid_tui.app/Contents/MacOS/droid_tui
#   target/macos-universal/droid_tui.app/Contents/Info.plist
#   target/droid_tui-<version>-macos-universal.zip
#
# The staging directory is under target/ on purpose: the repository root stays
# clean, and `rm -rf target/...` is always safe.
#
# stdout carries only the produced zip path, so callers can capture it:
#   zip="$(./scripts/package-macos.sh --version 0.1.0)"
# Progress and diagnostics go to stderr.
#
# The bundle is unsigned and not notarized on purpose (design decision 6):
# signing needs an Apple developer account and a certificate this project does
# not have. docs/platforms.md documents the Gatekeeper first-launch prompt.
#
# Requires macOS: `lipo` to fuse the slices and `ditto` to zip while keeping
# the .app directory. Everything else is plain bash, so `bash -n` works on any
# host.
set -euo pipefail

die() {
  echo "package-macos.sh: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
Usage: scripts/package-macos.sh --version <version>

  <version>  release version without the leading "v", e.g. 0.1.0

Builds target/aarch64-apple-darwin/release/droid_tui and
target/x86_64-apple-darwin/release/droid_tui must exist beforehand.
Prints the produced zip path on stdout.
EOF
}

version=""
while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      [ $# -ge 2 ] || die "--version needs a value"
      version="$2"
      shift 2
      ;;
    --version=*)
      version="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (try --help)"
      ;;
  esac
done

[ -n "$version" ] || { usage; die "--version is required"; }

# The tag becomes part of a filename and of an Info.plist value; keep it boring.
case "$version" in
  *[!A-Za-z0-9._-]*) die "version '$version' contains characters outside [A-Za-z0-9._-]" ;;
  .*|*.) die "version '$version' must not start or end with '.'" ;;
esac

command -v lipo >/dev/null 2>&1 || die "lipo not found - run this on macOS with the Xcode command line tools installed"
command -v ditto >/dev/null 2>&1 || die "ditto not found - run this on macOS (it ships with the OS)"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

arm_bin="$ROOT/target/aarch64-apple-darwin/release/droid_tui"
x86_bin="$ROOT/target/x86_64-apple-darwin/release/droid_tui"

for bin in "$arm_bin" "$x86_bin"; do
  [ -f "$bin" ] || die "missing $bin - build it with:
  cargo build --release --locked --target $(basename "$(dirname "$(dirname "$bin")")")"
done

staging="$ROOT/target/macos-universal"
app_dir="$staging/droid_tui.app"
contents="$app_dir/Contents"
binary="$contents/MacOS/droid_tui"
zip_path="$ROOT/target/droid_tui-${version}-macos-universal.zip"

echo "Assembling $app_dir" >&2
rm -rf "$app_dir"
mkdir -p "$contents/MacOS"

echo "Joining slices with lipo" >&2
lipo -create "$arm_bin" "$x86_bin" -output "$binary"
chmod 755 "$binary"

archs="$(lipo -archs "$binary")"
for want in arm64 x86_64; do
  case " $archs " in
    *" $want "*) ;;
    *) die "joined binary is missing the $want slice (lipo -archs: $archs)" ;;
  esac
done

cat > "$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>droid_tui</string>
	<key>CFBundleExecutable</key>
	<string>droid_tui</string>
	<key>CFBundleIdentifier</key>
	<string>com.moonstruxx.droid-tui</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>droid_tui</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${version}</string>
	<key>CFBundleVersion</key>
	<string>${version}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

echo "Zipping with ditto (unsigned, no notarization)" >&2
rm -f "$zip_path"
# --keepParent puts droid_tui.app itself at the zip root, so unpacking yields
# the bundle rather than its Contents directory.
ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$zip_path"

[ -f "$zip_path" ] || die "ditto did not produce $zip_path"

# stdout: the caller's only machine-readable output.
printf '%s\n' "$zip_path"
