#!/bin/sh
# Anarc installer — works on Linux (x86_64 / aarch64).
#
# Two modes:
#   1. Run inside a source checkout: builds with `cargo build --release` first.
#   2. Run next to a release bundle (lumen binary + cef runtime dir + logo):
#      installs directly, no Rust toolchain needed.
#
# Usage: ./install.sh [--prefix DIR] [--skip-build] [--no-desktop-entry]

set -eu

PREFIX="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
PIXMAPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/pixmaps"
SKIP_BUILD=0
NO_DESKTOP=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) PREFIX="$2"; shift 2 ;;
        --bindir) BIN_DIR="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        --no-desktop-entry) NO_DESKTOP=1; shift ;;
        -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
done

OS="$(uname -s)"
ARCH="$(uname -m)"
[ "$OS" = "Linux" ] || { echo "error: only Linux is supported right now ($OS)" >&2; exit 1; }
case "$ARCH" in
    x86_64|amd64) CEF_ARCH=linux_x86_64 ;;
    aarch64|arm64) CEF_ARCH=linux_arm64 ;;
    *) echo "error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$SCRIPT_DIR"

# --- Locate payload -------------------------------------------------------
# Release-bundle layout: ./anarc (or ./lumen) + ./cef_<arch>/ next to this script.
BUNDLE_BIN=""
BUNDLE_CEF=""
if [ -f ./anarc ] && [ -d "./cef_${CEF_ARCH}" ]; then
    BUNDLE_BIN="./anarc"
elif [ -f ./lumen ] && [ -d "./cef_${CEF_ARCH}" ]; then
    BUNDLE_BIN="./lumen"
fi

if [ -z "$BUNDLE_BIN" ] && [ "$SKIP_BUILD" -eq 0 ]; then
    if ! command -v cargo >/dev/null 2>&1; then
        echo "error: cargo not found and no bundled binary present" >&2
        echo "hint: install Rust (https://rustup.rs) or use the prebuilt release bundle" >&2
        exit 1
    fi
    echo "==> Building Anarc (release)..."
    cargo build --release
fi

if [ -z "$BUNDLE_BIN" ]; then
    if [ -x "target/release/anarc" ]; then
        BUNDLE_BIN="target/release/anarc"
    else
        BUNDLE_BIN="target/release/lumen"
    fi
    [ -x "$BUNDLE_BIN" ] || { echo "error: $BUNDLE_BIN not found; build first" >&2; exit 1; }
fi

# CEF runtime: bundled dir, $CEF_PATH, or the copy the build script fetched.
if [ -z "$BUNDLE_CEF" ]; then
    if [ -n "${CEF_PATH:-}" ] && [ -d "$CEF_PATH" ]; then
        BUNDLE_CEF="$CEF_PATH"
    else
        BUNDLE_CEF="$(find target/release/build target/debug/build -maxdepth 3 -type d -name "cef_${CEF_ARCH}" 2>/dev/null | head -1)"
    fi
    [ -n "$BUNDLE_CEF" ] && [ -d "$BUNDLE_CEF" ] || {
        echo "error: CEF runtime not found." >&2
        echo "hint: set CEF_PATH to an extracted CEF distribution, or build first so cef-dll-sys downloads it" >&2
        exit 1
    }
fi
[ -f "$BUNDLE_CEF/libcef.so" ] || { echo "error: no libcef.so in $BUNDLE_CEF" >&2; exit 1; }

# --- Install ---------------------------------------------------------------
APP_DIR="$PREFIX/anarc"
CEF_DIR="$PREFIX/anarc-cef"

echo "==> Installing to $APP_DIR"
mkdir -p "$APP_DIR/bin" "$BIN_DIR"
# Install atomically via temp file to avoid 'Text file busy' (ETXTBSY) if anarc is currently running
rm -f "$APP_DIR/bin/anarc.tmp" 2>/dev/null || true
cp -f "$BUNDLE_BIN" "$APP_DIR/bin/anarc.tmp"
chmod 755 "$APP_DIR/bin/anarc.tmp"
mv -f "$APP_DIR/bin/anarc.tmp" "$APP_DIR/bin/anarc"
ln -sf anarc "$APP_DIR/bin/lumen"

# CEF runtime: copy only runtime files (skip cmake/, include/, CMakeLists, etc.)
echo "==> Installing CEF runtime to $CEF_DIR"
mkdir -p "$CEF_DIR"
for item in libcef.so libcef_dll chrome-sandbox chrome_100_percent.pak \
            chrome_200_percent.pak icudtl.dat resources.pak v8_context_snapshot.bin \
            vk_swiftshader_icd.json libvk_swiftshader.so libvulkan.so.1 locales; do
    if [ -e "$BUNDLE_CEF/$item" ]; then
        if [ -f "$BUNDLE_CEF/$item" ]; then
            cp -f "$BUNDLE_CEF/$item" "$CEF_DIR/$item.tmp" 2>/dev/null && mv -f "$CEF_DIR/$item.tmp" "$CEF_DIR/$item" || cp -rf "$BUNDLE_CEF/$item" "$CEF_DIR/"
        else
            cp -rf "$BUNDLE_CEF/$item" "$CEF_DIR/"
        fi
    fi
done

# Symlink anarc into CEF runtime directory as anarc and chrome so CEF child processes
# (renderer, gpu, utility) never search system PATH or trigger external /usr/bin/chromium.
ln -sf "$APP_DIR/bin/anarc" "$CEF_DIR/anarc"
ln -sf "$APP_DIR/bin/anarc" "$CEF_DIR/chrome"

cat > "$BIN_DIR/anarc" <<EOF
#!/bin/sh
export LD_LIBRARY_PATH="$CEF_DIR\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
exec "$APP_DIR/bin/anarc" "\$@"
EOF
chmod 755 "$BIN_DIR/anarc"
ln -sf anarc "$BIN_DIR/lumen"

# --- Desktop entry + icon ---------------------------------------------------
if [ "$NO_DESKTOP" -eq 0 ]; then
    mkdir -p "$ICON_DIR" "$PIXMAPS_DIR" "$DESKTOP_DIR"
    LOGO=""
    for candidate in ./anarc_logo.png ./assets/anarc_logo.png ../assets/anarc_logo.png; do
        [ -f "$candidate" ] && LOGO="$candidate" && break
    done
    if [ -n "$LOGO" ]; then
        cp "$LOGO" "$ICON_DIR/anarc.png"
        cp "$LOGO" "$PIXMAPS_DIR/anarc.png"
    fi
    cat > "$DESKTOP_DIR/anarc.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Anarc
Comment=Anarc browser
Exec=anarc
Icon=anarc
Terminal=false
Categories=Network;WebBrowser;
StartupNotify=true
EOF
    command -v update-desktop-database >/dev/null 2>&1 && \
        update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && \
        gtk-update-icon-cache -f "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true
fi

echo "==> Done. Run 'anarc' to start, or find it in your app launcher."
echo "    If the icon is missing, restart your shell/panel or re-login."
