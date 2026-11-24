#!/bin/sh
# Anarc self-updater (Linux). Installs the newest signed GitHub release over an
# existing installation without a source checkout, cargo, or ./install.sh.
#
# Usage:
#   anarc-update                 # check GitHub and install a newer release
#   anarc-update --check         # report only (exit 10 when an update exists)
#   anarc-update --auto          # used by the systemd timer; quiet, safe
#   anarc-update --json          # single-line JSON result for tooling/MCP
#   anarc-update --rollback      # restore the binary backed up by the last update
#   anarc-update --disable-auto  # stop scheduled updates
#   anarc-update --enable-auto   # resume scheduled updates
#
# Overrides (mirrors/mocked releases, CI): ANARC_UPDATE_API, ANARC_UPDATE_ASSET,
# ANARC_UPDATE_ALLOW_FILE=1, ANARC_APP_DIR, ANARC_BIN_DIR, ANARC_INSTALLED_VERSION.
set -eu

API="https://api.github.com/repos/rover-001/anarc/releases/latest"
REPO_RAW="https://raw.githubusercontent.com/rover-001/anarc/main"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
APP_DIR="${ANARC_APP_DIR:-$DATA_HOME/anarc}"
STATE_DIR="$CONFIG_HOME/anarc"
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
VERSION_FILE="$APP_DIR/VERSION"
BACKUP_DIR="$APP_DIR/previous"
NO_AUTO_FILE="$STATE_DIR/update-disabled"

MODE=apply
JSON=0
FORCE=0
QUIET=0
ASSET_NAME=""

log() { [ "$QUIET" -eq 1 ] || printf '%s\n' "$*"; }
die() { printf '%s\n' "anarc-update: $*" >&2; exit 1; }
json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/\t/\\t/g' -e 's/\r//g'
}
emit() { # status installed latest message [extra-json]
    if [ "$JSON" -eq 1 ]; then
        printf '{"status":"%s","installed":"%s","latest":"%s","message":"%s"%s}\n' \
            "$(json_escape "$1")" "$(json_escape "$2")" "$(json_escape "$3")" \
            "$(json_escape "$4")" "${5:-}"
    elif [ "$QUIET" -eq 0 ]; then
        printf '%s\n' "$4"
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        --check|--no-apply) MODE=check ;;
        --auto) MODE=auto; QUIET=1 ;;
        --json) JSON=1 ;;
        --force|-f) FORCE=1 ;;
        --rollback) MODE=rollback ;;
        --disable-auto) MODE=disable-auto ;;
        --enable-auto) MODE=enable-auto ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
    shift
done

[ "$(uname -s)" = Linux ] || die "the updater supports Linux only"
case "$(uname -m)" in
    x86_64|amd64) ASSET_NAME="anarc-linux-x86_64.tar.gz" ;;
    aarch64|arm64) ASSET_NAME="anarc-linux-aarch64.tar.gz" ;;
    *) die "unsupported architecture: $(uname -m)" ;;
esac
ASSET_NAME="${ANARC_UPDATE_ASSET:-$ASSET_NAME}"
API="${ANARC_UPDATE_API:-$API}"

command -v curl >/dev/null || die "curl is required"
command -v sha256sum >/dev/null || die "sha256sum is required"
command -v tar >/dev/null || die "tar is required"
mkdir -p "$STATE_DIR" || die "cannot create $STATE_DIR"

case "$MODE" in
    disable-auto)
        : > "$NO_AUTO_FILE"
        echo "automatic Anarc updates disabled"
        exit 0
        ;;
    enable-auto)
        rm -f "$NO_AUTO_FILE"
        echo "automatic Anarc updates enabled"
        exit 0
        ;;
esac

if [ "$MODE" = rollback ]; then
    [ -f "$BACKUP_DIR/anarc" ] || die "no previous binary to restore"
    mkdir -p "$APP_DIR/bin"
    cp -f "$BACKUP_DIR/anarc" "$APP_DIR/bin/anarc.tmp"
    chmod 755 "$APP_DIR/bin/anarc.tmp"
    mv -f "$APP_DIR/bin/anarc.tmp" "$APP_DIR/bin/anarc"
    if [ -f "$BACKUP_DIR/anarc-ctl" ]; then
        cp -f "$BACKUP_DIR/anarc-ctl" "$APP_DIR/bin/anarc-ctl.tmp"
        chmod 755 "$APP_DIR/bin/anarc-ctl.tmp"
        mv -f "$APP_DIR/bin/anarc-ctl.tmp" "$APP_DIR/bin/anarc-ctl"
    fi
    [ -f "$BACKUP_DIR/VERSION" ] && cp -f "$BACKUP_DIR/VERSION" "$VERSION_FILE" || rm -f "$VERSION_FILE"
    emit rolled-back "" "" "anarc: rolled back to the previous version"
    exit 0
fi

if [ "$MODE" = auto ] && { [ "${ANARC_AUTO_UPDATE:-1}" = 0 ] || [ -f "$NO_AUTO_FILE" ]; }; then
    log "anarc: automatic updates disabled"
    exit 0
fi

# Single-instance guard: concurrent checks/updates must not race on the install.
LOCK_DIR="$RUNTIME_DIR/anarc-update.lock"
mkdir "$LOCK_DIR" 2>/dev/null || die "another update is already in progress"
trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT
WORK="${TMPDIR:-/tmp}/anarc-update.$$"
trap 'rmdir "$LOCK_DIR" 2>/dev/null || true; rm -rf "$WORK"' EXIT
mkdir -p "$WORK"

INSTALLED="${ANARC_INSTALLED_VERSION:-}"
if [ -z "$INSTALLED" ] && [ -f "$VERSION_FILE" ]; then
    INSTALLED=$(sed -n '1p' "$VERSION_FILE" | tr -d '[:space:]')
fi
[ -n "$INSTALLED" ] || INSTALLED="unknown"

# Fetch release metadata. A failed/non-JSON response is never fatal in auto mode:
# offline users must be able to keep using the browser.
if ! curl -fsSL --retry 2 --connect-timeout 20 \
        -H 'Accept: application/vnd.github+json' -H 'User-Agent: anarc-updater' \
        "$API" -o "$WORK/release.json"; then
    if [ "$MODE" = auto ]; then
        log "anarc: update check unavailable (offline?)"
        exit 0
    fi
    die "could not fetch release metadata from $API"
fi

# Split the API response into one JSON object per line so both GitHub's pretty
# output and compact/mirrored responses parse identically without jq/python.
objects() { tr -d '\n' < "$WORK/release.json" | tr '{' '\n'; }
release_field() { # tag_name | draft | prerelease
    objects | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\([^,}]*\).*/\1/p" | head -n 1 |
        sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//' -e 's/[[:space:]]//g'
}
asset_field() { # browser_download_url | digest
    objects | awk -v want="$ASSET_NAME" -v field="$1" '
        index($0, "\"" want "\"") == 0 { next }
        {
            line = $0
            sub(".*\"" field "\"[[:space:]]*:[[:space:]]*", "", line)
            sub(/[,}].*$/, "", line)
            gsub(/^[ \t]*"|"[ \t]*$/, "", line)
            print line
            exit
        }
    '
}

TAG=$(release_field tag_name)
DRAFT=$(release_field draft)
PRERELEASE=$(release_field prerelease)
PUBLISHED=$(release_field published_at)
URL=$(asset_field browser_download_url)
DIGEST=$(asset_field digest)

if [ -z "$TAG" ] || [ -z "$URL" ]; then
    if [ "$MODE" = auto ]; then
        log "anarc: no release asset for this platform"
        exit 0
    fi
    die "no '$ASSET_NAME' asset in the latest release"
fi
if [ "$DRAFT" = true ] || [ "$PRERELEASE" = true ]; then
    if [ "$MODE" = auto ]; then
        log "anarc: latest release is not stable; skipping"
        exit 0
    fi
fi

normalize() { printf '%s' "$1" | sed 's/^v//'; }
case "$INSTALLED" in
    dev-*)
        # A source build is newer than any release that predates it. Accept
        # only releases published after BUILD_TIME (or an explicit --force).
        NEWER=0
        if [ "$FORCE" -eq 1 ]; then
            NEWER=1
        elif [ -f "$APP_DIR/BUILD_TIME" ] && [ -n "$PUBLISHED" ]; then
            BUILT_AT=$(sed -n '1p' "$APP_DIR/BUILD_TIME" | tr -d '[:space:]')
            PUBLISHED_AT=$(date -u -d "$PUBLISHED" +%s 2>/dev/null || echo 0)
            case "$BUILT_AT:$PUBLISHED_AT" in
                *[!0-9:]*) ;;
                *) [ -n "$BUILT_AT" ] && [ "$PUBLISHED_AT" -gt "$BUILT_AT" ] && NEWER=1 ;;
            esac
        fi
        if [ "$NEWER" -ne 1 ]; then
            emit skipped "$INSTALLED" "$TAG" "anarc: development build newer than release $TAG; skipped (use --force to replace it)"
            exit 0
        fi
        ;;
    unknown)
        NEWER=1
        ;;
    *)
        NEWER=$(awk -v a="$(normalize "$TAG")" -v b="$(normalize "$INSTALLED")" 'BEGIN {
            na = split(a, av, /[.-]/); nb = split(b, bv, /[.-]/)
            for (i = 1; i <= na && i <= nb; i++) {
                x = av[i] + 0; y = bv[i] + 0
                if (x > y) { print 1; exit }
                if (x < y) { print 0; exit }
            }
            print (na > nb) ? 1 : 0
        }')
        ;;
esac

if [ "$NEWER" -ne 1 ]; then
    emit up-to-date "$INSTALLED" "$TAG" "anarc: already up to date ($INSTALLED)"
    exit 0
fi

if [ "$MODE" = check ]; then
    emit update-available "$INSTALLED" "$TAG" "anarc: $TAG is available (installed: $INSTALLED)" \
        ,\"asset\":\"$(json_escape "$ASSET_NAME")\"
    exit 10
fi

case "$URL" in
    https://*) ;;
    file://*) [ -n "${ANARC_UPDATE_API:-}" ] || die "refusing non-HTTPS asset URL" ;;
    *) die "refusing unsafe asset URL: $URL" ;;
esac

# An open browser has libcef.so and the binary mapped. Defer until it exits.
if [ "$FORCE" -eq 0 ] && command -v pgrep >/dev/null 2>&1 && pgrep -x anarc >/dev/null 2>&1; then
    if [ "$MODE" = auto ]; then
        log "anarc: browser running; update deferred to next launch"
        exit 0
    fi
    die "anarc is running; close it before updating (or use --force)"
fi

log "==> Downloading Anarc $TAG"
curl -fL --retry 3 --retry-delay 2 --connect-timeout 30 -o "$WORK/$ASSET_NAME" "$URL"

if [ -n "$DIGEST" ]; then
    case "$DIGEST" in
        sha256:*) EXPECTED=$(printf '%s' "${DIGEST#sha256:}" | tr '[:upper:]' '[:lower:]') ;;
        *) die "unsupported digest format: $DIGEST" ;;
    esac
    ACTUAL=$(sha256sum "$WORK/$ASSET_NAME" | cut -d ' ' -f 1)
    [ "$ACTUAL" = "$EXPECTED" ] || die "SHA256 mismatch: expected $EXPECTED, got $ACTUAL"
else
    die "release asset has no SHA256 digest; refusing to install unverified update"
fi

log "==> Extracting release"
mkdir -p "$WORK/unpack"
tar -xzf "$WORK/$ASSET_NAME" -C "$WORK/unpack"
INSTALLER=$(find "$WORK/unpack" -maxdepth 3 -type f -name install.sh | head -n 1)
[ -n "$INSTALLER" ] || die "release archive has no install.sh"
BUNDLE_DIR=$(dirname "$INSTALLER")
[ -f "$BUNDLE_DIR/anarc" ] || die "release archive has no anarc binary"

# Keep the old binaries so --rollback can recover from a bad update.
mkdir -p "$BACKUP_DIR"
if [ -f "$APP_DIR/bin/anarc" ]; then
    cp -f "$APP_DIR/bin/anarc" "$BACKUP_DIR/anarc.tmp"
    mv -f "$BACKUP_DIR/anarc.tmp" "$BACKUP_DIR/anarc"
fi
if [ -f "$APP_DIR/bin/anarc-ctl" ]; then
    cp -f "$APP_DIR/bin/anarc-ctl" "$BACKUP_DIR/anarc-ctl.tmp"
    mv -f "$BACKUP_DIR/anarc-ctl.tmp" "$BACKUP_DIR/anarc-ctl"
fi
if [ -f "$VERSION_FILE" ]; then
    cp -f "$VERSION_FILE" "$BACKUP_DIR/VERSION"
else
    rm -f "$BACKUP_DIR/VERSION"
fi

log "==> Installing Anarc $TAG"
# Release bundles carry the correct CEF runtime (including proprietary codecs),
# so skip both the source build and the redundant codec download.
sh "$INSTALLER" --skip-build --no-codecs
printf '%s\n' "$TAG" > "$VERSION_FILE"

# Make the updater itself available to future installs, including this one.
SELF=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
mkdir -p "$APP_DIR/bin"
if [ "$SELF" != "$APP_DIR/bin/anarc-update" ]; then
    cp -f "$SELF" "$APP_DIR/bin/anarc-update.tmp"
    chmod 755 "$APP_DIR/bin/anarc-update.tmp"
    mv -f "$APP_DIR/bin/anarc-update.tmp" "$APP_DIR/bin/anarc-update"
fi
ln -sf "$APP_DIR/bin/anarc-update" "$BIN_DIR/anarc-update"

emit updated "$INSTALLED" "$TAG" "anarc: updated $INSTALLED -> $TAG" \
    ,\"asset\":\"$(json_escape "$ASSET_NAME")\"
