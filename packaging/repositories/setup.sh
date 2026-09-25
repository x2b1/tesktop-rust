#!/bin/sh
# tesktop2 Package Repository Setup Script
# Detects distribution (Ubuntu/Debian, Fedora, openSUSE, Arch Linux),
# imports and verifies the GPG signing key, and configures the package repository.
set -eu

BASE_URL="${TESKTOP2_REPO_BASE_URL:-https://viceverse-cz.github.io/tesktop2}"
CHANNEL="${TESKTOP2_CHANNEL:-nightly}"
EXPECTED_FINGERPRINT="${TESKTOP2_FINGERPRINT:-CA19DA939E9BCAB500751CE480FE95CAD86141A5}"

if [ "$(id -u)" -ne 0 ]; then
    SUDO="sudo"
else
    SUDO=""
fi

# Styling & Colors
BOLD=$(printf '\033[1m')
DIM=$(printf '\033[2m')
BLUE=$(printf '\033[1;34m')
CYAN=$(printf '\033[1;36m')
GREEN=$(printf '\033[1;32m')
PURPLE=$(printf '\033[1;35m')
YELLOW=$(printf '\033[1;33m')
RED=$(printf '\033[1;31m')
NC=$(printf '\033[0m')

# Disable colors if not running in a terminal
if [ ! -t 1 ]; then
    BOLD=""
    DIM=""
    BLUE=""
    CYAN=""
    GREEN=""
    PURPLE=""
    YELLOW=""
    RED=""
    NC=""
fi

banner() {
    printf '%b' "${CYAN}"
    cat << 'EOF'
     _____ _____ ____  _____ ___ _   _ 
    /  ___|  ___|  _ \| ____|_ _| \ | |
    \ `--.| |__ | |_) | |__  | ||  \| |
     `--. \  __||  _ <|  __| | || |\  |
    /\__/ / |___| | \ \ |___ | || | \ |
    \____/\____/|_|  \_\____/|___|_| \_|
EOF
    printf '%b\n' "${DIM} Lightweight, native Discord client in Rust${NC}\n"
}

log() {
    printf " %b::%b %s\n" "${BLUE}" "${NC}" "$1"
}

success() {
    printf " %b✔%b %s\n" "${GREEN}" "${NC}" "$1"
}

warn() {
    printf " %b!%b %s\n" "${YELLOW}" "${NC}" "$1"
}

error() {
    printf " %b✖ error:%b %s\n" "${RED}" "${NC}" "$1" >&2
    exit 1
}

# Ensure required download tools exist
if command -v curl >/dev/null 2>&1; then
    download() { curl --fail --silent --show-error --location "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    download() { wget --quiet -O "$2" "$1"; }
else
    error "Either curl or wget is required to download repository configuration."
fi

# Verify GPG key fingerprint matches expected signing key
verify_key() {
    keyfile="$1"
    if ! command -v gpg >/dev/null 2>&1; then
        error "gpg is required to verify the signing key."
    fi
    fingerprint=$(gpg --batch --show-keys --with-colons "$keyfile" 2>/dev/null | awk -F: '$1 == "fpr" {print $10; exit}')
    if [ -z "$fingerprint" ]; then
        error "Could not extract fingerprint from downloaded signing key."
    fi
    # Normalize uppercase
    fingerprint=$(echo "$fingerprint" | tr '[:lower:]' '[:upper:]')
    expected=$(echo "$EXPECTED_FINGERPRINT" | tr '[:lower:]' '[:upper:]')
    if [ "$fingerprint" != "$expected" ]; then
        error "GPG fingerprint mismatch! Expected: $expected, Got: $fingerprint. Refusing to install untrusted key."
    fi
    success "Cryptographic key verified (${DIM}$fingerprint${NC})"
}

banner

if [ ! -f /etc/os-release ]; then
    error "/etc/os-release not found. Unsupported Linux distribution."
fi

. /etc/os-release

DISTRO_ID="$ID"
case " ${ID_LIKE:-} " in
    *" arch "*) DISTRO_ID="arch" ;;
esac

ARCH=$(uname -m)
case "$ARCH" in
    x86_64) ;;
    *)
        error "Architecture $ARCH is not currently supported by tesktop2 package repositories."
        ;;
esac

# Native packages must match the distribution that built their shared libraries.
case "$DISTRO_ID:${VERSION_ID:-}" in
    ubuntu:26.04) REPO_PATH="ubuntu-26.04/amd64/apt" ;;
    fedora:43|fedora:44) REPO_PATH="fedora-$VERSION_ID/$ARCH/rpm" ;;
    opensuse-tumbleweed:*) REPO_PATH="opensuse-tumbleweed/$ARCH/rpm" ;;
    arch:*) REPO_PATH="arch/$ARCH/arch" ;;
    *) error "No matching native repository for $ID ${VERSION_ID:-rolling}. Use the Flatpak bundle." ;;
esac
REPO_URL="$BASE_URL/$CHANNEL/$REPO_PATH"
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT HUP INT TERM
KEY_URL="$REPO_URL/tesktop2.asc"
KEY_FILE="$TEMP_DIR/tesktop2.asc"

log "Configuring tesktop2 ${BOLD}${CHANNEL}${NC} repository for ${BOLD}${PRETTY_NAME:-$ID}${NC} (${ARCH})..."
log "Fetching official signing key..."
download "$KEY_URL" "$KEY_FILE" || error "Signing key unavailable for this distribution; its signed repository must be published before setup."
verify_key "$KEY_FILE"

INSTALL_CMD=""

case "$DISTRO_ID" in
    ubuntu)
        log "Installing APT keyring and source list..."
        $SUDO install -Dm644 "$KEY_FILE" /etc/apt/keyrings/tesktop2.asc

        printf 'deb [arch=amd64 signed-by=/etc/apt/keyrings/tesktop2.asc] %s ./\n' "$REPO_URL" | \
            $SUDO tee /etc/apt/sources.list.d/tesktop2.list >/dev/null

        log "Updating APT package lists..."
        $SUDO apt-get update -o Dir::Etc::sourcelist="sources.list.d/tesktop2.list" -o Dir::Etc::sourceparts="-" >/dev/null 2>&1 || $SUDO apt-get update >/dev/null 2>&1

        INSTALL_CMD="$SUDO apt install tesktop2"
        ;;

    fedora)
        log "Importing RPM key and configuring DNF repository..."

        REPO_FILE="$TEMP_DIR/tesktop2.repo"
        download "$REPO_URL/tesktop2.repo" "$REPO_FILE"
        $SUDO rpm --import "$KEY_FILE"
        $SUDO install -m644 "$REPO_FILE" /etc/yum.repos.d/tesktop2.repo

        INSTALL_CMD="$SUDO dnf install tesktop2"
        ;;

    opensuse-tumbleweed)
        log "Importing RPM key and configuring Zypper repository..."
        $SUDO rpm --import "$KEY_FILE"

        REPO_FILE="$TEMP_DIR/tesktop2.repo"
        download "$REPO_URL/tesktop2.repo" "$REPO_FILE"
        $SUDO install -m644 "$REPO_FILE" /etc/zypp/repos.d/tesktop2.repo
        $SUDO zypper --non-interactive refresh tesktop2-$CHANNEL >/dev/null 2>&1 || true

        INSTALL_CMD="$SUDO zypper install tesktop2"
        ;;

    arch)
        log "Importing key into Pacman keyring..."
        $SUDO pacman-key --add "$KEY_FILE" >/dev/null 2>&1
        $SUDO pacman-key --lsign-key "$EXPECTED_FINGERPRINT" >/dev/null 2>&1

        PACMAN_CONF="/etc/pacman.conf"

        if grep -q "\[tesktop2\]" "$PACMAN_CONF"; then
            log "Repository [tesktop2] already present in $PACMAN_CONF."
        else
            printf '\n[tesktop2]\nSigLevel = Required\nServer = %s\n' "$REPO_URL" | \
                $SUDO tee -a "$PACMAN_CONF" >/dev/null
        fi

        INSTALL_CMD="$SUDO pacman -Syu tesktop2"
        ;;

    *)
        error "Distribution '$ID' is not automatically supported by this script. See packaging/repositories/README.md for manual instructions."
        ;;
esac

success "Repository configuration complete!"
printf '\n'

# Prompt to install if interactive terminal is attached
DO_INSTALL="false"

# When run via `curl ... | sh`, stdin is the script itself. We can read from /dev/tty if available.
if [ -t 0 ]; then
    TTY_INPUT=1
elif ( : </dev/tty ) 2>/dev/null; then
    TTY_INPUT=1
else
    TTY_INPUT=0
fi

if [ "$TTY_INPUT" -eq 1 ]; then
    printf "%b?%b Would you like to install %bSerein%b now? [Y/n]: " "${PURPLE}" "${NC}" "${BOLD}" "${NC}"
    if [ -t 0 ]; then
        read -r answer || answer=n
    else
        read -r answer </dev/tty || answer=n
    fi
    case "$answer" in
        [nN][oO]|[nN])
            DO_INSTALL="false"
            ;;
        *)
            DO_INSTALL="true"
            ;;
    esac
fi

if [ "$DO_INSTALL" = "true" ]; then
    log "Installing tesktop2 (${INSTALL_CMD})..."
    # Package managers also prompt for keys/transactions when the script is piped.
    if [ -t 0 ]; then
        $INSTALL_CMD
    else
        $INSTALL_CMD </dev/tty
    fi
    printf '\n'
    success "${BOLD}tesktop2 installed successfully!${NC}"
    log "Launch it from your desktop application launcher or run ${BOLD}tesktop2${NC}."
else
    log "To install tesktop2 later, run:"
    printf '\n    %b%s%b\n\n' "${CYAN}" "$INSTALL_CMD" "${NC}"
fi
