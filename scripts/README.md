# Installation Scripts

This directory contains installation and uninstallation scripts for vfio-tool.

## Scripts

### `install.sh`

Installs vfio-tool system-wide.

**Usage:**
```bash
# From project root
sudo ./scripts/install.sh

# Or from scripts directory
cd scripts
sudo ./install.sh
```

**What it does:**
1. Checks for root privileges
2. Verifies the binary exists at `target/release/vfio-tool`
3. Installs binary to `/usr/local/bin/vfio-tool`
4. Generates man page if not present
5. Installs man page to `/usr/share/man/man1/vfio-tool.1`
6. Updates man database

**Requirements:**
- Must be run as root (with sudo)
- Binary must be built first: `cargo build --release`

**After installation:**
```bash
vfio-tool --version
man vfio-tool
```

---

### `uninstall.sh`

Removes vfio-tool from the system.

**Usage:**
```bash
# From project root
sudo ./scripts/uninstall.sh

# Or from scripts directory
cd scripts
sudo ./uninstall.sh
```

**What it does:**
1. Checks for root privileges
2. Removes `/usr/local/bin/vfio-tool`
3. Removes `/usr/share/man/man1/vfio-tool.1`
4. Updates man database
5. Reminds about configuration files

**Note:** Configuration files in `/etc/vfio-tool/` are NOT removed automatically.

---

## Alternative Installation Methods

### Using Makefile

```bash
# Install
sudo make install

# Uninstall
sudo make uninstall
```

### Manual Installation

```bash
# Install binary
sudo install -m 755 target/release/vfio-tool /usr/local/bin/

# Install man page
cargo run --release --bin generate-man > vfio-tool.1
sudo install -m 644 vfio-tool.1 /usr/share/man/man1/
sudo mandb
```

---

## Troubleshooting

**Error: Binary not found**
```
Error: Binary not found at target/release/vfio-tool
Please run: cargo build --release
```

**Solution:** Build the project first:
```bash
cargo build --release
```

**Error: Permission denied**
```
Error: This script must be run as root (use sudo)
```

**Solution:** Run with sudo:
```bash
sudo ./scripts/install.sh
```

**Script runs from wrong directory**

The scripts automatically detect their location and adjust paths accordingly. They can be run from:
- Project root: `sudo ./scripts/install.sh`
- Scripts directory: `cd scripts && sudo ./install.sh`

---

## Files Installed

After running `install.sh`, these files are installed:

```
/usr/local/bin/vfio-tool          # Main binary (755)
/usr/share/man/man1/vfio-tool.1   # Man page (644)
```

Configuration files (created by vfio-tool commands):
```
/etc/vfio-tool/config.toml        # Configuration
/etc/systemd/system/vfio-tool.service  # Systemd service (if installed)
```

---

## Development

These scripts are simple bash scripts that can be modified for custom installation locations.

**Key variables in `install.sh`:**
- `BINARY_DEST` - Where to install the binary (default: `/usr/local/bin/vfio-tool`)
- `MAN_DEST` - Where to install the man page (default: `/usr/share/man/man1/vfio-tool.1`)

For custom installation locations, use the Makefile with PREFIX:
```bash
make install PREFIX=/usr
```
