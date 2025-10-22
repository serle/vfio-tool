use std::fs;
use std::process::Command;
use anyhow::{Result, Context};
use colored::Colorize;
use dialoguer::Confirm;

use crate::config::Config;

const SERVICE_FILE: &str = "/etc/systemd/system/vfio-tool.service";
const SERVICE_BINARY: &str = "/usr/local/bin/vfio-tool";

/// Detect existing VFIO-related systemd services
fn detect_vfio_services() -> Result<Vec<String>> {
    let output = Command::new("systemctl")
        .args(["list-unit-files", "--type=service", "--no-legend"])
        .output()
        .context("Failed to list systemd services")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let vfio_services: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(service_name) = parts.first() {
                // Look for VFIO-related services (but not our own)
                let name = service_name.to_lowercase();
                if (name.contains("vfio") || name.contains("kernel-bypass") || name.contains("dpdk"))
                    && !name.contains("vfio-tool")
                {
                    return Some(service_name.to_string());
                }
            }
            None
        })
        .collect();

    Ok(vfio_services)
}

/// Clean up an old VFIO service
fn cleanup_service(service_name: &str) -> Result<()> {
    println!("  Cleaning up {}...", service_name.bright_yellow());

    // Stop the service
    let _ = Command::new("systemctl")
        .args(["stop", service_name])
        .output();

    // Disable the service
    let _ = Command::new("systemctl")
        .args(["disable", service_name])
        .output();

    // Try to find and remove the service file
    let service_path = format!("/etc/systemd/system/{}", service_name);
    if std::path::Path::new(&service_path).exists() {
        fs::remove_file(&service_path)
            .context(format!("Failed to remove {}", service_path))?;
        println!("    ✓ Removed {}", service_path);
    }

    // Also check for service files in other locations
    let service_path_usr = format!("/usr/lib/systemd/system/{}", service_name);
    if std::path::Path::new(&service_path_usr).exists() {
        println!("    ⚠ Found service at {} (system location, not removing)", service_path_usr.bright_yellow());
    }

    Ok(())
}

/// Install systemd service
pub fn install_service() -> Result<()> {
    println!("{}", "Installing VFIO systemd service...".bright_cyan());
    println!();

    // Step 1: Validate configuration exists
    println!("{}", "Step 1: Validating configuration...".bright_cyan());
    let cfg = match crate::config::load_config() {
        Ok(cfg) => {
            println!("  ✓ Configuration file found and valid");
            cfg
        }
        Err(e) => {
            println!();
            eprintln!("{}", "✗ No valid configuration found!".bright_red().bold());
            println!();
            println!("Before installing the systemd service, you need to create a configuration.");
            println!();
            println!("Options:");
            println!("  1. Run {} to create config interactively", "vfio-tool configure".bright_cyan());
            println!("  2. Run {} to manually save config", "vfio-tool save --vfio <interfaces>".bright_cyan());
            println!();
            return Err(e);
        }
    };

    // Show what will be made persistent
    println!();
    println!("{}", "Configuration that will be applied on boot:".bright_cyan());

    if !cfg.devices.vfio.is_empty() {
        println!();
        println!("  {} (kernel bypass):", "VFIO devices".bright_green());
        for iface in &cfg.devices.vfio {
            println!("    - {}", iface.bright_white());
        }
    }

    if !cfg.devices.kernel.is_empty() {
        println!();
        println!("  {} (normal networking):", "Kernel devices".bright_yellow());
        for iface in &cfg.devices.kernel {
            println!("    - {}", iface.bright_white());
        }
    }

    println!();
    println!("  Options:");
    println!("    - Set permissions: {}", cfg.options.set_permissions);
    println!("    - Auto-load module: {}", cfg.options.auto_load_module);
    println!();

    // Step 2: Offer to test configuration first
    println!("{}", "Step 2: Testing configuration (recommended)".bright_cyan());
    println!();
    println!("It's recommended to test the configuration before making it persistent.");
    println!("This will apply the config now so you can verify everything works.");
    println!();

    let should_test = Confirm::new()
        .with_prompt("Would you like to test the configuration now?")
        .default(true)
        .interact()?;

    if should_test {
        println!();
        println!("{}", "Applying configuration for testing...".bright_cyan());

        use crate::vfio;
        match vfio::apply_config(&cfg) {
            Ok(()) => {
                println!();
                println!("{}", "✓ Configuration applied successfully!".bright_green().bold());
                println!();
                println!("Please verify:");
                println!("  1. Run {} to see device status", "vfio-tool list".bright_cyan());
                println!("  2. Check {} exists", "/dev/vfio/<group>".bright_cyan());
                println!("  3. Test your application with VFIO");
                println!();

                let proceed = Confirm::new()
                    .with_prompt("Configuration working correctly? Proceed with installation?")
                    .default(true)
                    .interact()?;

                if !proceed {
                    println!();
                    println!("{}", "Installation cancelled. You can:".bright_yellow());
                    println!("  - Modify config with {}", "vfio-tool configure".bright_cyan());
                    println!("  - Run {} when ready", "vfio-tool install".bright_cyan());
                    return Ok(());
                }
            }
            Err(e) => {
                println!();
                eprintln!("{}", "✗ Failed to apply configuration!".bright_red().bold());
                eprintln!("Error: {}", e);
                println!();
                println!("Please fix the configuration issues before installing.");
                println!("  - Run {} to check system", "vfio-tool check".bright_cyan());
                println!("  - Run {} to reconfigure", "vfio-tool configure".bright_cyan());
                return Err(e);
            }
        }
    } else {
        println!();
        println!("{}", "⚠ Skipping test (not recommended)".bright_yellow());
        println!();

        let force_proceed = Confirm::new()
            .with_prompt("Install without testing? Service may fail on boot")
            .default(false)
            .interact()?;

        if !force_proceed {
            println!();
            println!("Installation cancelled. Test the configuration first:");
            println!("  - Run {} to test", "sudo vfio-tool apply".bright_cyan());
            println!("  - Run {} when ready", "vfio-tool install".bright_cyan());
            return Ok(());
        }
    }

    println!();
    println!("{}", "Step 3: Installing systemd service...".bright_cyan());
    println!();

    // Step 3: Check for existing VFIO services
    println!("{}", "Checking for existing VFIO services...".bright_cyan());
    let existing_services = detect_vfio_services()?;

    if !existing_services.is_empty() {
        println!();
        println!("{}", "⚠ Found existing VFIO-related services:".bright_yellow().bold());
        for service in &existing_services {
            println!("  - {}", service.bright_yellow());
        }
        println!();
        println!("These may conflict with vfio-tool.");
        println!();

        let should_cleanup = Confirm::new()
            .with_prompt("Would you like to stop and disable these services?")
            .default(true)
            .interact()?;

        if should_cleanup {
            println!();
            for service in &existing_services {
                if let Err(e) = cleanup_service(service) {
                    println!("    {} Failed to clean up {}: {}", "⚠".bright_yellow(), service, e);
                    println!("    You may need to clean this up manually.");
                } else {
                    println!("    ✓ Cleaned up {}", service.bright_green());
                }
            }
            println!();
        } else {
            println!();
            println!("{}", "⚠ Proceeding without cleanup. Services may conflict.".bright_yellow());
            println!();
        }
    } else {
        println!("  ✓ No conflicting services found");
        println!();
    }

    // Check if binary is installed
    if !std::path::Path::new(SERVICE_BINARY).exists() {
        println!("{}", "Installing vfio-tool binary...".bright_cyan());

        // Get current executable path
        let current_exe = std::env::current_exe()
            .context("Failed to get current executable path")?;

        // Copy to /usr/local/bin
        fs::copy(&current_exe, SERVICE_BINARY)
            .context("Failed to copy binary. Try running with sudo.")?;

        // Make executable (0o755 = rwxr-xr-x)
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(SERVICE_BINARY, permissions)
            .context("Failed to set executable permissions on binary")?;

        println!("  ✓ Binary installed to {}", SERVICE_BINARY);
    }

    // Generate service file
    let service_content = generate_service_file();

    // Write service file
    fs::write(SERVICE_FILE, service_content)
        .context("Failed to write service file. Try running with sudo.")?;

    println!("  ✓ Service file created: {}", SERVICE_FILE);

    // Reload systemd
    Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .context("Failed to reload systemd")?;

    println!("  ✓ Systemd reloaded");

    // Enable service
    Command::new("systemctl")
        .args(["enable", "vfio-tool.service"])
        .status()
        .context("Failed to enable service")?;

    println!("  ✓ Service enabled (will run on boot)");

    println!();
    println!("{}", "✓ VFIO systemd service installed!".bright_green());
    println!();
    println!("Service commands:");
    println!("  {} - Start service now", "sudo systemctl start vfio-tool".bright_cyan());
    println!("  {} - Stop service", "sudo systemctl stop vfio-tool".bright_cyan());
    println!("  {} - Check status", "sudo systemctl status vfio-tool".bright_cyan());
    println!("  {} - View logs", "sudo journalctl -u vfio-tool".bright_cyan());

    Ok(())
}

/// Uninstall systemd service
pub fn uninstall_service() -> Result<()> {
    println!("{}", "Uninstalling VFIO systemd service...".bright_cyan());

    // Stop service if running
    let _ = Command::new("systemctl")
        .args(["stop", "vfio-tool.service"])
        .status();

    // Disable service
    let _ = Command::new("systemctl")
        .args(["disable", "vfio-tool.service"])
        .status();

    // Remove service file
    if std::path::Path::new(SERVICE_FILE).exists() {
        fs::remove_file(SERVICE_FILE)
            .context("Failed to remove service file")?;
        println!("  ✓ Service file removed");
    }

    // Reload systemd
    Command::new("systemctl")
        .arg("daemon-reload")
        .status()
        .context("Failed to reload systemd")?;

    println!("  ✓ Systemd reloaded");

    println!();
    println!("{}", "✓ VFIO systemd service uninstalled".bright_green());

    Ok(())
}

/// Generate systemd service file
fn generate_service_file() -> String {
    format!(
        r#"[Unit]
Description=VFIO Device Binding for Kernel Bypass
Documentation=https://github.com/your-repo/vfio-tool
After=network.target multi-user.target

[Service]
Type=oneshot
ExecStart={} apply
RemainAfterExit=yes
StandardOutput=journal
StandardError=journal
TimeoutStartSec=300
Restart=no

[Install]
WantedBy=multi-user.target
"#,
        SERVICE_BINARY
    )
}

/// Generate standalone bash script
pub fn generate_bash_script(config: &Config) -> Result<String> {
    let mut script = String::new();

    script.push_str("#!/bin/bash\n");
    script.push_str("#\n");
    script.push_str("# VFIO Device Binding Script\n");
    script.push_str("# Generated by vfio-tool\n");
    script.push_str("#\n");
    script.push_str("# This script binds network interfaces to VFIO for kernel bypass.\n");
    script.push_str("#\n\n");

    script.push_str("set -e\n\n");

    script.push_str("echo \"===== VFIO Device Binding =====\"\n");
    script.push_str("echo\n\n");

    // Load VFIO module
    if config.options.auto_load_module {
        script.push_str("# Load VFIO module\n");
        script.push_str("echo \"Loading VFIO module...\"\n");
        script.push_str("modprobe -q vfio-pci || true\n");
        script.push_str("echo \"✓ VFIO module loaded\"\n");
        script.push_str("echo\n\n");
    }

    // Bind each interface
    if !config.devices.vfio.is_empty() {
        script.push_str("# Bind interfaces to VFIO\n");

        for interface in &config.devices.vfio {
            script.push_str(&format!("echo \"Binding {}...\"\n", interface));

            script.push_str(&format!(
                "# Get PCI address and device IDs for {}\n",
                interface
            ));

            script.push_str(&format!(
                r#"if [ -e /sys/class/net/{}/device ]; then
    PCI_ADDR=$(basename $(readlink /sys/class/net/{}/device))
    VENDOR=$(cat /sys/bus/pci/devices/$PCI_ADDR/vendor | sed 's/0x//')
    DEVICE=$(cat /sys/bus/pci/devices/$PCI_ADDR/device | sed 's/0x//')

    # Unbind from current driver
    if [ -e /sys/bus/pci/devices/$PCI_ADDR/driver ]; then
        echo "$PCI_ADDR" > /sys/bus/pci/devices/$PCI_ADDR/driver/unbind 2>/dev/null || true
    fi

    # Register with VFIO
    echo "$VENDOR $DEVICE" > /sys/bus/pci/drivers/vfio-pci/new_id 2>/dev/null || true

    # Bind to VFIO
    echo "$PCI_ADDR" > /sys/bus/pci/drivers/vfio-pci/bind 2>/dev/null || true

    echo "  ✓ {} bound to vfio-pci"
else
    echo "  ✗ {} not found"
fi
echo

"#,
                interface, interface, interface, interface
            ));
        }
    }

    // Set permissions
    if config.options.set_permissions {
        script.push_str("# Set VFIO device permissions\n");
        script.push_str("echo \"Setting VFIO device permissions...\"\n");
        script.push_str("chmod 666 /dev/vfio/vfio 2>/dev/null || true\n");
        script.push_str(
            r#"for dev in /dev/vfio/*; do
    if [ "$dev" != "/dev/vfio/vfio" ]; then
        chmod 666 "$dev" 2>/dev/null || true
    fi
done
"#,
        );
        script.push_str("echo \"✓ Permissions set\"\n");
        script.push_str("echo\n\n");
    }

    script.push_str("echo \"✓ VFIO binding complete\"\n");
    script.push_str("echo\n");

    Ok(script)
}
