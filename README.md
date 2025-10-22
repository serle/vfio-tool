# vfio-tool

**Simple, reliable CLI tool for managing VFIO device bindings and kernel bypass on Linux.**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](Cargo.toml)

---

## Overview

**vfio-tool** simplifies VFIO (Virtual Function I/O) setup for kernel bypass networking. It replaces manual 17-step processes with simple, intuitive commands.

### Quick Example

```bash
# See everything at once
vfio-tool list

# One-time IOMMU setup
sudo vfio-tool setup-grub
sudo reboot

# Configure with interactive wizard
sudo vfio-tool configure
# Done! Persistent across reboots
```

### What Problems Does It Solve?

**Without vfio-tool:** Find PCI addresses, map interfaces, unbind drivers, edit GRUB, register IDs, set permissions, write scripts, create systemd service... 17+ manual steps.

**With vfio-tool:** 3 commands. Interactive wizard. Automatic error handling. Bulletproof.

---

## Features

- **📋 Single-glance overview** - See all interfaces, drivers, IOMMU groups, speeds in one table
- **🔧 Automatic GRUB configuration** - Detects CPU (Intel/AMD) and configures IOMMU
- **⚡ Immediate operations** - Bind/unbind devices instantly (with or without config)
- **💾 Persistent configuration** - TOML config + systemd service for boot persistence
- **🎨 Interactive wizards** - Configure through guided interfaces
- **🔍 Hardware change detection** - Validate and update when cards are added/removed
- **🔌 Application integration** - Check interfaces required by your app with proper exit codes
- **🧹 Automatic cleanup** - Detects and removes conflicting services
- **✅ CI/CD ready** - Proper exit codes, non-interactive modes, validation commands

### Use Cases

- **DPDK applications** - High-performance packet processing
- **SPDK applications** - Storage performance development
- **Custom userspace drivers** - Direct hardware access
- **Network function virtualization** - Bypass kernel for performance
- **Development and testing** - Quick bind/unbind cycles
- **Production deployment** - Reliable, automated setup

---

## Installation

See [INSTALL.md](INSTALL.md) for detailed installation and testing instructions.

### Quick Install

```bash
# Clone and build
cd /path/to/vfio-tool
cargo build --release

# Install
sudo ./scripts/install.sh

# Verify
vfio-tool --version
man vfio-tool
```

### Requirements

- Linux kernel with IOMMU support
- IOMMU enabled in BIOS (VT-d for Intel, AMD-Vi for AMD)
- Root access for device operations
- Rust 2024 edition (for building)

---

## Quick Start

### First-Time Setup

```bash
# 1. Check current state
vfio-tool list
vfio-tool status

# 2. Setup IOMMU (one-time, requires reboot)
sudo vfio-tool setup-grub
sudo reboot

# 3. Verify IOMMU enabled
vfio-tool status

# 4. Configure (interactive wizard)
sudo vfio-tool configure
# - Select interfaces for VFIO
# - Test configuration
# - Install systemd service
# Done!

# 5. Verify
vfio-tool list
ls /dev/vfio/
```

### Quick Test (No Persistence)

```bash
# Bind interface to VFIO right now
sudo vfio-tool bind enp33s0f0np0

# Verify
vfio-tool list

# Return to kernel
sudo vfio-tool unbind enp33s0f0np0
```

---

## Command Reference

### Information Commands

```bash
vfio-tool list                      # Show all interfaces (table)
vfio-tool list --verbose            # Show with legend
vfio-tool status                    # System VFIO/IOMMU status
vfio-tool info <interface>          # Detailed device info
vfio-tool explain <interface>       # Explain what binding does
vfio-tool check                     # Validate system readiness
vfio-tool check --fix               # Auto-fix issues
```

**Example output:**
```
┌───────────────┬──────────────┬───────────┬─────────────┬───────────────┬────────┬───────────┬─────────┐
│ INTERFACE     │ PCI ADDRESS  │ DRIVER    │ IOMMU GROUP │ VENDOR:DEVICE │ STATUS │ MAX SPEED │ LINK    │
├───────────────┼──────────────┼───────────┼─────────────┼───────────────┼────────┼───────────┼─────────┤
│ enp1s0f0np0   │ 0000:01:00.0 │ vfio-pci  │ 34          │ 0x15b3:0x101f │ vfio   │ 25G       │ -       │
│ enp33s0f0np0  │ 0000:21:00.0 │ vfio-pci  │ 57          │ 0x8086:0x158b │ vfio   │ 25G       │ -       │
│ enp209s0f0np0 │ 0000:d1:00.0 │ i40e      │ 19          │ 0x8086:0x15ff │ kernel │ 10G       │ no link │
└───────────────┴──────────────┴───────────┴─────────────┴───────────────┴────────┴───────────┴─────────┘
```

### Immediate Operations

```bash
sudo vfio-tool bind <interface>          # Bind to VFIO now
sudo vfio-tool bind <if1>,<if2>          # Bind multiple
sudo vfio-tool unbind <interface>        # Return to kernel
sudo vfio-tool unbind <if1>,<if2>        # Unbind multiple
sudo vfio-tool reset                     # Unbind all + update mappings
```

**Note:** Interfaces bound to VFIO will show in `vfio-tool list` but disappear from `ip link` (this is expected - they're in kernel bypass mode).

### Configuration Management

```bash
sudo vfio-tool configure                # Interactive wizard (fresh)
sudo vfio-tool update                   # Update existing (preserves settings)
sudo vfio-tool save --vfio <list>       # Save config manually
sudo vfio-tool apply                    # Apply saved config
vfio-tool show-config                   # Display current config
vfio-tool validate                      # Validate config vs hardware
```

### Application Integration

```bash
# Check if interfaces are in correct modes (validation)
vfio-tool check-interfaces --vfio <list> --kernel <list>
# Exit 0=all good, 1=not found, 2=wrong mode

# Ensure interfaces are in VFIO mode (bind if needed)
sudo vfio-tool ensure-vfio <if1>,<if2>
# Exit 0=success, non-zero=failure
```

**Example:**
```bash
# Validate exact configuration before starting app
vfio-tool check-interfaces \
  --vfio enp33s0f0np0,enp33s0f1np1 \
  --kernel enp209s0f0np0

if [ $? -eq 0 ]; then
    echo "Configuration validated, starting application"
    ./my-dpdk-app
else
    echo "ERROR: Interface configuration mismatch"
    exit 1
fi
```

### Persistence

```bash
sudo vfio-tool install                  # Install systemd service
sudo vfio-tool uninstall                # Remove systemd service
vfio-tool generate-script               # Generate bash script
```

### System Setup

```bash
sudo vfio-tool setup-grub               # Configure GRUB for IOMMU
sudo vfio-tool setup-grub --yes         # Skip confirmation
```

---

## Common Workflows

### Production Setup

```bash
# One-time system setup
sudo vfio-tool setup-grub
sudo reboot

# Configure with wizard
sudo vfio-tool configure
# - Select interfaces
# - Test configuration
# - Install systemd service

# Verify persistence
sudo reboot
vfio-tool list
```

### Development/Testing

```bash
# Quick bind for testing
sudo vfio-tool bind enp33s0f0np0
./test-app
sudo vfio-tool unbind enp33s0f0np0
```

### Hardware Changes

```bash
# After adding/removing network cards
vfio-tool validate              # Check for mismatches
sudo vfio-tool update           # Update configuration
vfio-tool list                  # Verify
```

### Reset Everything

```bash
# Unbind all VFIO devices and restore mappings
sudo vfio-tool reset
# - Unbinds all network devices from vfio-pci
# - Triggers driver reprobe (kernel drivers take over)
# - Scans for interface names
# - Updates config with interface→PCI mappings
# - Useful after manual binding or config corruption
```

---

## Troubleshooting

### IOMMU Not Enabled

```
✗ IOMMU Enabled: No
```

**Fix:**
1. Enable VT-d (Intel) or AMD-Vi (AMD) in BIOS
2. Run `sudo vfio-tool setup-grub`
3. Reboot
4. Verify with `vfio-tool status`

### Interface Disappeared

**This is normal!** When bound to VFIO, interfaces disappear from `ip link` but appear in `vfio-tool list` with `vfio` status.

**Verify:**
```bash
vfio-tool list           # Should show status=vfio
ls /dev/vfio/           # Should show device nodes
```

### Device Busy Error

Interface has active connections or is up:
```bash
sudo ip link set <interface> down
sudo systemctl stop NetworkManager
sudo vfio-tool bind <interface>
```

### Configuration Mismatch After Hardware Changes

```bash
vfio-tool validate      # Shows mismatches
sudo vfio-tool update   # Fix configuration
```

### Missing Interface Names in Config

If devices were bound manually (not via vfio-tool), interface names won't be in config:

```bash
sudo vfio-tool reset
# Unbinds all VFIO devices
# Triggers driver reprobe
# Scans for interface names
# Updates config mappings
```

### Permission Denied

Commands requiring root automatically check permissions and show helpful errors:
```bash
vfio-tool bind enp33s0f0np0
# Error: This command requires root privileges.
# Run with sudo: sudo vfio-tool bind enp33s0f0np0
```

### Systemd Service Failed

```bash
sudo journalctl -u vfio-tool -n 50
vfio-tool validate
vfio-tool list
```

---

## Application Integration

### Startup Script Template

```bash
#!/bin/bash
set -e

VFIO_IFACES="enp33s0f0np0,enp33s0f1np1"
KERNEL_IFACES="enp209s0f0np0"

echo "Validating interface configuration..."
if ! vfio-tool check-interfaces --vfio $VFIO_IFACES --kernel $KERNEL_IFACES; then
    echo "ERROR: Configuration mismatch"
    exit 1
fi

echo "Starting application..."
exec ./my-dpdk-app
```

### Systemd Service

```ini
[Unit]
Description=My DPDK Application
After=vfio-tool.service
Requires=vfio-tool.service

[Service]
Type=simple
ExecStartPre=/usr/local/bin/vfio-tool check-interfaces --vfio enp33s0f0np0
ExecStart=/opt/my-app/run.sh
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

---

## Exit Codes

| Code | Meaning | Commands |
|------|---------|----------|
| `0` | Success | All commands |
| `1` | Interface not found | `check-interfaces`, `ensure-vfio` |
| `2` | Wrong mode | `check-interfaces`, `validate` |
| `3` | Other errors | Config missing, general errors |
| `4` | Permission denied | Commands requiring root without sudo |

---

## Configuration File

Location: `/etc/vfio-tool/config.toml`

```toml
[devices]
vfio = [
    "enp1s0f0np0",
    "enp33s0f0np0",
]
kernel = [
    "enp209s0f0np0",
]

[devices.pci_mappings]
enp1s0f0np0 = "0000:01:00.0"
enp33s0f0np0 = "0000:21:00.0"
enp209s0f0np0 = "0000:d1:00.0"

[options]
set_permissions = true
auto_load_module = true
```

**Options:**
- `set_permissions` - Set `/dev/vfio/*` to 666 for non-root access
- `auto_load_module` - Automatically load vfio-pci module
- `pci_mappings` - Interface→PCI address mappings (auto-managed)

---

## How It Works

### Device Discovery

Scans `/sys/class/net/` and `/sys/bus/pci/drivers/vfio-pci/` to find:
- Network interfaces (kernel mode)
- VFIO-bound devices (kernel bypass mode)
- PCI addresses, drivers, IOMMU groups
- Link speeds and hardware capabilities

### Binding Process

1. Unbind from current driver
2. Register vendor:device ID with vfio-pci
3. Bind to vfio-pci driver
4. Handle race conditions (idempotent)
5. Set permissions on `/dev/vfio/*`

### PCI Address Mappings

The tool maintains `interface→PCI address` mappings in config:
- Captured when interfaces are visible (before binding)
- Allows unbind/bind by interface name even after interface disappears
- Updated automatically by `reset` command
- Enables reliable operations regardless of interface visibility

---

## Documentation

- **README.md** (this file) - Overview and quick reference
- **INSTALL.md** - Detailed installation and testing guide
- **Man page** - `man vfio-tool` (comprehensive reference)
- **Built-in help** - `vfio-tool --help`, `vfio-tool <cmd> --help`

---

## License

MIT OR Apache-2.0

---

## Support

**Documentation:**
- VFIO kernel docs: https://www.kernel.org/doc/html/latest/driver-api/vfio.html
- DPDK docs: https://doc.dpdk.org/
- SPDK docs: https://spdk.io/doc/

**Getting Help:**
```bash
vfio-tool --help              # General help
vfio-tool <command> --help    # Command-specific help
man vfio-tool                 # Full manual
```
