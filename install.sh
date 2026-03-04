#!/bin/sh
set -e

REPO="quangtruongnb/disktree"
BIN="disk-tree"
INSTALL_DIR="/usr/local/bin"

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
  arm64)  TARGET="aarch64-apple-darwin" ;;
  x86_64) TARGET="x86_64-apple-darwin" ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

# Fetch latest release tag
echo "Fetching latest release..."
TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

if [ -z "$TAG" ]; then
  echo "Failed to fetch latest release tag." >&2
  exit 1
fi

echo "Installing ${BIN} ${TAG} (${TARGET})..."

URL="https://github.com/${REPO}/releases/download/${TAG}/${BIN}-${TARGET}"
TMP="$(mktemp)"

curl -fsSL "$URL" -o "$TMP"
chmod +x "$TMP"

# Install (may require sudo)
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "${INSTALL_DIR}/${BIN}"
else
  sudo mv "$TMP" "${INSTALL_DIR}/${BIN}"
fi

echo "Installed to ${INSTALL_DIR}/${BIN}"
echo "Run: ${BIN}"
