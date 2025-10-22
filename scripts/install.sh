#!/bin/bash
# Installation script for vfio-tool

set -e

# Get the project root directory (parent of scripts directory)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

# Configuration
BINARY_SRC="$PROJECT_ROOT/target/release/vfio-tool"
BINARY_DEST="/usr/local/bin/vfio-tool"
MAN_SRC="$PROJECT_ROOT/vfio-tool.1"
MAN_DEST="/usr/share/man/man1/vfio-tool.1"

echo "Installing vfio-tool..."

# Check if binary exists
if [ ! -f "$BINARY_SRC" ]; then
    echo "Error: Binary not found at $BINARY_SRC"
    echo "Please run: cargo build --release"
    exit 1
fi

# Install binary
echo "  → Installing binary to $BINARY_DEST"
install -m 755 "$BINARY_SRC" "$BINARY_DEST"

# Generate and install man page
if [ ! -f "$MAN_SRC" ]; then
    echo "  → Generating man page..."
    cd "$PROJECT_ROOT"
    cargo run --quiet --release --bin generate-man > "$MAN_SRC"
fi

echo "  → Installing man page to $MAN_DEST"
install -m 644 "$MAN_SRC" "$MAN_DEST"

# Update man database
if command -v mandb &> /dev/null; then
    echo "  → Updating man database..."
    mandb -q 2>/dev/null || true
fi

echo "✓ Installation complete!"
echo ""
echo "Test the installation:"
echo "  vfio-tool --version"
echo "  man vfio-tool"
echo ""
echo "To uninstall, run: sudo ./scripts/uninstall.sh"
