#!/bin/sh
# Install a CEF runtime with proprietary video codecs (H.264/AAC) for Anarc.
#
# cef-dll-sys downloads the Spotify CDN "minimal" CEF build, which is compiled
# with proprietary_codecs=false. Web apps such as video editors then report
# "AVC cannot be decoded in this browser" for ordinary H.264 MP4 files.
# This script swaps in a matching, checksum-pinned community build.
#
# Usage: tools/install-cef-codecs.sh
#        CEF_INSTALL_DIR=/custom/runtime tools/install-cef-codecs.sh
set -eu

# Must match the CEF version pinned in Cargo.lock (cef 156.4.0+156.0.26).
CEF_VERSION='156.0.26+g8ea0c73+chromium-156.0.8078.88'
RELEASE_TAG='cef-156.0.26-proprietary-codecs-2'
ARCHIVE="cef_binary_${CEF_VERSION}_linux64_minimal.zip"
SHA256='027473508ec25227972a8bc9182b15723f98cfa2e301746f8e2224c6a58517d5'
URL="https://github.com/tobagin/karere/releases/download/${RELEASE_TAG}/$(printf '%s' "$ARCHIVE" | sed 's/+/%2B/g')"
# Offline / mirrored installs: CEF_ARCHIVE_URL=file:///path/to/archive.zip
URL="${CEF_ARCHIVE_URL:-$URL}"
DEST="${CEF_INSTALL_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/anarc-cef}"
WORK="${TMPDIR:-/tmp}/anarc-cef-codecs.$$"

[ "$(uname -s)" = Linux ] || { echo "error: this script supports Linux only" >&2; exit 1; }
case "$(uname -m)" in
    x86_64|amd64) ;;
    *) echo "error: this script supports x86_64 only" >&2; exit 1 ;;
esac
command -v curl >/dev/null || { echo "error: curl required" >&2; exit 1; }
command -v unzip >/dev/null || { echo "error: unzip required" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "error: sha256sum required" >&2; exit 1; }
[ -d "$DEST" ] || { echo "error: CEF runtime dir not found: $DEST (run install.sh first)" >&2; exit 1; }

trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK"

echo "==> Downloading CEF $CEF_VERSION with proprietary codecs"
curl -fL --retry 10 --retry-delay 2 --connect-timeout 30 -o "$WORK/$ARCHIVE" "$URL"

echo "==> Verifying SHA256"
printf '%s  %s\n' "$SHA256" "$WORK/$ARCHIVE" | sha256sum -c -

echo "==> Extracting"
mkdir -p "$WORK/extracted"
unzip -q -o "$WORK/$ARCHIVE" -d "$WORK/extracted"
SRC="$WORK/extracted/cef_binary_${CEF_VERSION}_linux64_minimal"
[ -f "$SRC/Release/libcef.so" ] || { echo "error: unexpected archive layout" >&2; exit 1; }

# Keep the original runtime so this swap is reversible.
BACKUP="${DEST}-minimal-backup"
mkdir -p "$BACKUP"
[ -e "$BACKUP/libcef.so" ] || cp -a "$DEST/libcef.so" "$BACKUP/libcef.so"

echo "==> Installing codec-enabled runtime to $DEST"
for item in libcef.so libvk_swiftshader.so libvulkan.so.1 v8_context_snapshot.bin \
            vk_swiftshader_icd.json chrome-sandbox chrome_100_percent.pak \
            chrome_200_percent.pak icudtl.dat resources.pak; do
    if [ -f "$SRC/Release/$item" ]; then
        cp -f "$SRC/Release/$item" "$DEST/$item.tmp" && mv -f "$DEST/$item.tmp" "$DEST/$item"
    elif [ -f "$SRC/Resources/$item" ]; then
        cp -f "$SRC/Resources/$item" "$DEST/$item.tmp" && mv -f "$DEST/$item.tmp" "$DEST/$item"
    fi
done
cp -a "$SRC/Resources/locales" "$DEST/locales.tmp"
rm -rf "$DEST/locales"
mv -f "$DEST/locales.tmp" "$DEST/locales"

echo "==> Done. Restart Anarc to load the new libcef.so."
echo "    Original runtime backed up at $BACKUP"
echo "    Note: H.264/AAC are patent-licensed codecs; review licensing before redistribution."
