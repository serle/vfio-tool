#!/bin/bash
# Uninstallation script for vfio-tool

set -e

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

# Configuration
BINARY_DEST="/usr/local/bin/vfio-tool"
MAN_DEST="/usr/share/man/man1/vfio-tool.1"

echo "Uninstalling vfio-tool..."

# Remove binary
if [ -f "$BINARY_DEST" ]; then
    echo "  → Removing binary: $BINARY_DEST"
    rm -f "$BINARY_DEST"
else
    echo "  ℹ Binary not found: $BINARY_DEST"
fi

# Remove man page
if [ -f "$MAN_DEST" ]; then
    echo "  → Removing man page: $MAN_DEST"
    rm -f "$MAN_DEST"
else
    echo "  ℹ Man page not found: $MAN_DEST"
fi

# Update man database
if command -v mandb &> /dev/null; then
    echo "  → Updating man database..."
    mandb -q 2>/dev/null || true
fi

echo "✓ Uninstallation complete!"
echo ""
echo "Note: Configuration files remain at /etc/vfio-tool/"
echo "      Run 'sudo rm -rf /etc/vfio-tool' to remove them"
echo ""
echo "Note: Systemd service may still be installed"
echo "      Run 'vfio-tool uninstall' before uninstalling if service is active"
