# Installation and Testing Guide

Complete guide for installing, testing, and verifying vfio-tool.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Building from Source](#building-from-source)
- [Installation Methods](#installation-methods)
- [Verification](#verification)
- [Testing Without Installing](#testing-without-installing)
- [Testing After Installation](#testing-after-installation)
- [System Setup](#system-setup)
- [Configuration Workflow](#configuration-workflow)
- [Troubleshooting Installation](#troubleshooting-installation)
- [Uninstallation](#uninstallation)

---

## Prerequisites

### Hardware Requirements

- CPU with IOMMU support:
  - **Intel:** VT-d capable processor
  - **AMD:** AMD-Vi capable processor
- IOMMU enabled in BIOS/UEFI firmware
- Network cards in PCIe slots

### Software Requirements

**For running vfio-tool:**
- Linux kernel 4.0+ (VFIO support required)
- Modern kernel 5.x+ recommended
- systemd (for persistent configuration)
- Root/sudo access

**For building from source:**
- Rust 2024 edition or later
- cargo (Rust package manager)
- Standard build tools (gcc, make)

### Check IOMMU Support

```bash
# Check if CPU has IOMMU
grep -E 'vmx|svm' /proc/cpuinfo

# Check if IOMMU is enabled (after BIOS + GRUB setup)
dmesg | grep -i iommu
```

---

## Building from Source

### Clone or Navigate to Project

```bash
cd /home/serle/projects/rust/vfio-tool
# Or: git clone https://github.com/your-repo/vfio-tool.git
```

### Build Release Version

```bash
# Clean build
cargo clean
cargo build --release

# Verify binaries are built
ls -lh target/release/vfio-tool
ls -lh target/release/generate-man
```

**Expected output:**
```
-rwxr-xr-x  vfio-tool         # Main binary (~2-5 MB)
-rwxr-xr-x  generate-man      # Man page generator
```

**Build time:** ~30-60 seconds on modern hardware

---

## Installation Methods

### Method 1: Using Install Script (Recommended)

The install script handles everything automatically:

```bash
sudo ./scripts/install.sh
```

**What it does:**
- Builds release version if needed
- Installs binary to `/usr/local/bin/vfio-tool`
- Generates man page
- Installs man page to `/usr/share/man/man1/vfio-tool.1`
- Updates man database
- Verifies installation
- Shows helpful next steps

**Expected output:**
```
Installing vfio-tool...
  → Installing binary to /usr/local/bin/vfio-tool
  → Generating man page...
  → Installing man page to /usr/share/man/man1/vfio-tool.1
  → Updating man database...
✓ Installation complete!

Verify installation:
  vfio-tool --version
  man vfio-tool

Next steps:
  vfio-tool list           # See current interfaces
  vfio-tool status         # Check IOMMU status
  vfio-tool --help         # View all commands
```

### Method 2: Using Makefile

```bash
# Build and install in one command
make release
sudo make install
```

### Method 3: Manual Installation

For complete control:

```bash
# 1. Build
cargo build --release

# 2. Install binary
sudo install -m 755 target/release/vfio-tool /usr/local/bin/

# 3. Generate and install man page
cargo run --release --bin generate-man > vfio-tool.1
sudo install -m 644 vfio-tool.1 /usr/share/man/man1/
sudo mandb

# 4. Verify
vfio-tool --version
man vfio-tool
```

---

## Verification

After installation, verify everything works:

```bash
# Check binary is in PATH
which vfio-tool
# Expected: /usr/local/bin/vfio-tool

# Check version
vfio-tool --version
# Expected: vfio-tool 0.1.0

# Check help works
vfio-tool --help
# Should show command list

# Check man page
man vfio-tool
# Press 'q' to quit

# Test basic command (no sudo needed)
vfio-tool list
# Should show your network interfaces
```

---

## Testing Without Installing

It's recommended to test the tool before system-wide installation:

### Step 1: Build

```bash
cargo build --release
```

### Step 2: Test Read-Only Commands

These commands are safe and don't modify anything:

```bash
# Test basic commands (no sudo needed)
./target/release/vfio-tool --version
./target/release/vfio-tool --help
./target/release/vfio-tool list
./target/release/vfio-tool status
./target/release/vfio-tool check

# Get info about specific interface
./target/release/vfio-tool info enp33s0f0np0

# Explain what binding would do
./target/release/vfio-tool explain enp33s0f0np0
```

**What to look for:**
- Tool runs without errors
- Shows your network interfaces correctly
- Displays IOMMU status
- No permission errors (these are read-only)

### Step 3: Test Bind/Unbind (Optional)

Test VFIO binding on ONE interface:

```bash
# Bind ONE interface to VFIO (temporary)
sudo ./target/release/vfio-tool bind enp33s0f0np0

# Check status
./target/release/vfio-tool list
# Interface should show status=vfio, driver=vfio-pci

# Verify device node exists
ls -l /dev/vfio/
# Should show device nodes

# Return to kernel driver
sudo ./target/release/vfio-tool unbind enp33s0f0np0

# Verify it's back
./target/release/vfio-tool list
# Interface should show original driver (i40e, mlx5_core, etc.)
```

**Expected behavior:**
1. After bind: Interface disappears from `ip link`, shows as `vfio` status in `vfio-tool list`
2. After unbind: Interface returns, shows original driver

### Step 4: Install if Tests Pass

If everything works:

```bash
sudo ./scripts/install.sh
```

---

## Testing After Installation

Once installed system-wide, perform comprehensive testing:

### Phase 1: Read-Only Operations

```bash
# List all interfaces
vfio-tool list

# Check system status
vfio-tool status

# Validate system readiness
vfio-tool check

# Get detailed info
vfio-tool info enp33s0f0np0
```

### Phase 2: VFIO Binding Test

Test with ONE interface first:

```bash
# Bind one interface
sudo vfio-tool bind enp33s0f0np0

# Verify
vfio-tool list
ls -l /dev/vfio/

# Test with your application (if available)
# sudo ./your-dpdk-app

# Unbind
sudo vfio-tool unbind enp33s0f0np0

# Verify returned to kernel
vfio-tool list
```

### Phase 3: Configuration Test

Test the configuration system:

```bash
# Run interactive configuration wizard
sudo vfio-tool configure

# It will:
# 1. Show all your interfaces
# 2. Ask which ones for VFIO (multi-select)
# 3. Ask which ones for kernel (multi-select)
# 4. Offer to test immediately
# 5. Ask about making persistent

# View saved configuration
vfio-tool show-config

# Validate configuration
vfio-tool validate

# Apply configuration
sudo vfio-tool apply

# Check results
vfio-tool list
```

### Phase 4: Reset Test

Test the reset functionality:

```bash
# Unbind all VFIO devices and update mappings
sudo vfio-tool reset

# Verify all returned to kernel
vfio-tool list
# All should show kernel drivers, not vfio-pci
```

### Phase 5: Systemd Service Test (Optional)

Only test persistence if previous phases worked:

```bash
# Install systemd service
sudo vfio-tool install

# Check service status
systemctl status vfio-tool.service

# Test reboot (OPTIONAL - only if confident)
sudo reboot

# After reboot, verify
vfio-tool list
# Should show same VFIO bindings
```

---

## System Setup

### Enable IOMMU (One-Time Setup)

IOMMU must be enabled for VFIO to work:

#### Step 1: Enable in BIOS

Reboot and enter BIOS/UEFI setup:
- **Intel systems:** Enable "VT-d" or "Intel Virtualization Technology for Directed I/O"
- **AMD systems:** Enable "AMD-Vi" or "IOMMU"

#### Step 2: Configure GRUB

After BIOS is configured:

```bash
# Automatic configuration (detects Intel/AMD)
sudo vfio-tool setup-grub

# Skip confirmation prompt
sudo vfio-tool setup-grub --yes
```

**What it does:**
- Detects CPU vendor automatically
- Backs up `/etc/default/grub`
- Adds appropriate parameters:
  - Intel: `intel_iommu=on iommu=pt`
  - AMD: `amd_iommu=on iommu=pt`
- Runs `update-grub`
- Prompts for reboot

#### Step 3: Reboot

```bash
sudo reboot
```

#### Step 4: Verify IOMMU is Enabled

```bash
# Check with vfio-tool
vfio-tool status
# Should show: ✓ IOMMU Enabled: Yes

# Check kernel messages
dmesg | grep -i iommu
# Should show IOMMU initialization messages
```

---

## Configuration Workflow

### Option A: Interactive Wizard (Recommended)

Use the built-in wizard for guided setup:

```bash
sudo vfio-tool configure
```

**Steps in the wizard:**
1. Shows all available network interfaces with details
2. Multi-select which interfaces for VFIO (kernel bypass)
3. Multi-select which interfaces for kernel (normal networking)
4. Saves configuration to `/etc/vfio-tool/config.toml`
5. Offers to test configuration immediately
6. Offers to install systemd service for persistence

### Option B: Command-Line Configuration

For scripting or automation:

```bash
# Save configuration
sudo vfio-tool save \
  --vfio enp33s0f0np0,enp33s0f1np1 \
  --kernel enp209s0f0np0,enp209s0f1np1

# Validate
vfio-tool validate

# Apply
sudo vfio-tool apply

# Install systemd service
sudo vfio-tool install
```

### Option C: Manual Configuration

Edit the config file directly:

```bash
# Create directory
sudo mkdir -p /etc/vfio-tool

# Edit configuration
sudo nano /etc/vfio-tool/config.toml
```

**Example config:**
```toml
[devices]
vfio = [
    "enp33s0f0np0",
    "enp33s0f1np1",
]
kernel = [
    "enp209s0f0np0",
    "enp209s0f1np1",
]

[devices.pci_mappings]
enp33s0f0np0 = "0000:21:00.0"
enp33s0f1np1 = "0000:21:00.1"
enp209s0f0np0 = "0000:d1:00.0"
enp209s0f1np1 = "0000:d1:00.1"

[options]
set_permissions = true
auto_load_module = true
```

Then apply:
```bash
vfio-tool validate
sudo vfio-tool apply
```

---

## Troubleshooting Installation

### Build Errors

**Error:** `cargo: command not found`
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Error:** Build fails with dependency errors
```bash
cargo clean
cargo update
cargo build --release
```

### Installation Errors

**Error:** Permission denied
```bash
# Make sure using sudo
sudo ./scripts/install.sh
```

**Error:** `/usr/local/bin/` doesn't exist
```bash
sudo mkdir -p /usr/local/bin
sudo ./scripts/install.sh
```

### Runtime Errors

**Error:** `vfio-tool: command not found` after installation
```bash
# Check PATH
echo $PATH | grep /usr/local/bin

# If not in PATH, add to ~/.bashrc or ~/.zshrc
export PATH="/usr/local/bin:$PATH"
source ~/.bashrc

# Or use full path
/usr/local/bin/vfio-tool --version
```

**Error:** Permission denied when running commands
```bash
# Commands that modify system state need sudo
sudo vfio-tool bind enp33s0f0np0

# Read-only commands don't need sudo
vfio-tool list
vfio-tool status
```

### IOMMU Issues

**Error:** IOMMU not enabled after setup
```bash
# Check BIOS settings
# - Intel: VT-d must be enabled
# - AMD: AMD-Vi must be enabled

# Check GRUB configuration
cat /etc/default/grub | grep GRUB_CMDLINE_LINUX

# Manually verify parameters are present:
# Intel: intel_iommu=on iommu=pt
# AMD: amd_iommu=on iommu=pt

# If missing, run setup again
sudo vfio-tool setup-grub --yes
sudo update-grub
sudo reboot
```

---

## Uninstallation

### Using Uninstall Script

```bash
sudo ./scripts/uninstall.sh
```

### Using Makefile

```bash
sudo make uninstall
```

### Manual Uninstallation

```bash
# Remove binary
sudo rm /usr/local/bin/vfio-tool

# Remove man page
sudo rm /usr/share/man/man1/vfio-tool.1
sudo mandb

# Remove systemd service (if installed)
sudo systemctl stop vfio-tool
sudo systemctl disable vfio-tool
sudo rm /etc/systemd/system/vfio-tool.service
sudo systemctl daemon-reload

# Remove configuration (optional)
sudo rm -rf /etc/vfio-tool/

# Verify
which vfio-tool
# Should return nothing
```

---

## Quick Install Script

For automated installations, use this script:

```bash
#!/bin/bash
set -e

echo "Installing vfio-tool..."

# Navigate to project
cd /path/to/vfio-tool

# Build
cargo build --release

# Install
sudo ./scripts/install.sh

# Verify
vfio-tool --version
vfio-tool list

echo "✓ Installation complete!"
```

Save as `quick-install.sh`, make executable with `chmod +x quick-install.sh`, and run `./quick-install.sh`.

---

## Safe Testing Checklist

Follow this order for safest testing:

- [ ] Build from source
- [ ] Test `vfio-tool list` (read-only)
- [ ] Test `vfio-tool status` (read-only)
- [ ] Test bind/unbind on ONE interface
- [ ] Verify interface returns to kernel after unbind
- [ ] Install system-wide
- [ ] Test configuration wizard
- [ ] Test `vfio-tool apply`
- [ ] Verify all interfaces in correct state
- [ ] Only then: install systemd service
- [ ] Test reboot persistence

**Never skip steps!** Each step validates the previous one.

---

## Post-Installation

After successful installation:

```bash
# Quick reference
vfio-tool --help
man vfio-tool

# Start using it
vfio-tool list
sudo vfio-tool configure
```

See [README.md](README.md) for usage examples and workflows.
