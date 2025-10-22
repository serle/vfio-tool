use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use colored::Colorize;

use crate::device::{self, NetworkDevice, DeviceStatus};
use crate::config::Config;

/// Bind interfaces to VFIO
pub fn bind_interfaces(interfaces: &[&str]) -> Result<()> {
    println!("{}", "Binding interfaces to VFIO...".bright_cyan());
    println!();

    // Load VFIO module if not loaded
    ensure_vfio_module_loaded()?;

    // Collect interface -> PCI mappings BEFORE binding
    let mut pci_mappings = std::collections::HashMap::new();
    for interface in interfaces {
        if let Ok(device) = device::get_device_info(interface) {
            pci_mappings.insert(interface.to_string(), device.pci_address.clone());
        }
    }

    for interface in interfaces {
        println!("Processing: {}", interface.bright_yellow());

        // Try to get device info by interface name
        match device::get_device_info(interface) {
            Ok(device) => {
                bind_device(&device)?;
            }
            Err(_) => {
                // Interface not found - check if we have PCI address in config
                if let Some(pci_addr) = find_pci_address_in_vfio(interface) {
                    println!("  {} Interface not visible, binding by PCI address {}", "ℹ".bright_blue(), pci_addr);
                    bind_by_pci_address(&pci_addr)?;
                } else {
                    anyhow::bail!(
                        "Interface {} not found and no PCI address mapping available. \
                        Please ensure the device exists or use: sudo vfio-tool bind <pci-address>",
                        interface
                    );
                }
            }
        }
        println!();
    }

    // Save PCI mappings to config for later unbinding
    save_pci_mappings(&pci_mappings)?;

    println!("{}", "✓ All interfaces bound to VFIO".bright_green());
    println!();
    println!("Device nodes created in /dev/vfio/:");
    list_vfio_devices()?;

    Ok(())
}

/// Unbind interfaces from VFIO
pub fn unbind_interfaces(interfaces: &[&str]) -> Result<()> {
    println!("{}", "Unbinding interfaces from VFIO...".bright_cyan());
    println!();

    // Load config to get PCI mappings
    let config = crate::config::load_config().ok();

    let mut pci_addresses = Vec::new();

    for interface in interfaces {
        println!("Processing: {}", interface.bright_yellow());

        // Check if this looks like a PCI address (format: 0000:XX:XX.X)
        let pci_addr = if is_pci_address(interface) {
            // Unbind by PCI address directly (if it exists and is bound)
            let device_path = format!("/sys/bus/pci/devices/{}", interface);
            if Path::new(&device_path).exists() {
                unbind_by_pci_address(interface)?;
            } else {
                println!("  {} Device already unbound", "ℹ".bright_blue());
            }
            interface.to_string()
        } else {
            // Try to get device info by interface name
            match device::get_device_info(interface) {
                Ok(device) => {
                    let addr = device.pci_address.clone();
                    unbind_device(&device)?;
                    addr
                }
                Err(_) => {
                    // Interface not found - might be already bound to VFIO
                    // Try to find it in VFIO driver directory
                    if let Some(pci_addr) = find_pci_address_in_vfio(interface) {
                        println!("  {} Interface bound to VFIO as {}", "ℹ".bright_blue(), pci_addr);
                        unbind_by_pci_address(&pci_addr)?;
                        pci_addr
                    } else if let Some(ref cfg) = config {
                        // Try to find PCI address in config mappings
                        let interface_key = interface.to_string();
                        if let Some(pci_addr) = cfg.devices.pci_mappings.get(&interface_key) {
                            println!("  {} Using PCI address from config: {}", "ℹ".bright_blue(), pci_addr);
                            // Check if device exists but is unbound
                            let device_path = format!("/sys/bus/pci/devices/{}", pci_addr);
                            if Path::new(&device_path).exists() {
                                // Device exists but has no driver - just need to reprobe
                                println!("  {} Device currently unbound", "ℹ".bright_blue());
                                pci_addr.clone()
                            } else {
                                anyhow::bail!("Device {} not found in system", pci_addr);
                            }
                        } else {
                            anyhow::bail!(
                                "Interface {} not found and no PCI mapping in config. Use PCI address (e.g., sudo vfio-tool unbind 0000:01:00.0)",
                                interface
                            );
                        }
                    } else {
                        anyhow::bail!(
                            "Interface {} not found. If it's bound to VFIO, use PCI address (e.g., sudo vfio-tool unbind 0000:01:00.0)",
                            interface
                        );
                    }
                }
            }
        };

        pci_addresses.push(pci_addr);
        println!();
    }

    println!("{}", "✓ All interfaces unbound from VFIO".bright_green());

    // Trigger driver reprobe to let kernel drivers take over
    if !pci_addresses.is_empty() {
        println!();
        println!("{}", "Reprobing kernel drivers...".bright_cyan());

        for pci_addr in &pci_addresses {
            // Clear driver_override to allow kernel to choose driver
            let override_path = format!("/sys/bus/pci/devices/{}/driver_override", pci_addr);
            let _ = fs::write(&override_path, "\n");

            // Trigger reprobe
            let probe_path = "/sys/bus/pci/drivers_probe";
            if let Err(e) = fs::write(probe_path, pci_addr) {
                println!("  {} Warning: Could not reprobe {} - {}", "⚠".bright_yellow(), pci_addr, e);
            } else {
                println!("  {} Reprobed {}", "✓".bright_green(), pci_addr);
            }
        }

        // Wait for drivers to settle
        println!("  Waiting for drivers to load...");
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!();
        println!("{}", "✓ Kernel drivers loaded".bright_green());
    }

    Ok(())
}

/// Unbind all VFIO devices and refresh config mappings
pub fn unbind_all() -> Result<()> {
    println!("{}", "Resetting all VFIO devices...".bright_cyan());
    println!();

    let vfio_driver_path = Path::new("/sys/bus/pci/drivers/vfio-pci");

    if !vfio_driver_path.exists() {
        println!("{}", "No VFIO devices bound.".bright_green());
        return Ok(());
    }

    // Collect PCI addresses before unbinding
    let mut pci_addresses = Vec::new();
    for entry in fs::read_dir(vfio_driver_path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only check PCI devices (format: 0000:XX:XX.X)
        if name_str.contains(':') && name_str.contains('.') {
            // Check if it's a network device
            let class_path = format!("/sys/bus/pci/devices/{}/class", name_str);
            if let Ok(class_str) = fs::read_to_string(&class_path) {
                let class_code = class_str.trim();
                if class_code.starts_with("0x02") {
                    pci_addresses.push(name_str.to_string());
                }
            }
        }
    }

    if pci_addresses.is_empty() {
        println!("{}", "No VFIO network devices found.".bright_green());
        return Ok(());
    }

    // Unbind all devices
    let mut count = 0;
    for pci_addr in &pci_addresses {
        println!("Unbinding: {}", pci_addr.bright_yellow());
        unbind_pci_device(pci_addr)?;
        count += 1;
    }

    println!("\n{} {} unbound from vfio-pci", "✓".bright_green(),
        if count == 1 { "device" } else { "devices" });

    // Trigger driver reprobe to let kernel drivers take over
    println!();
    println!("{}", "Reprobing kernel drivers...".bright_cyan());
    for pci_addr in &pci_addresses {
        // Clear driver_override to allow kernel to choose driver
        let override_path = format!("/sys/bus/pci/devices/{}/driver_override", pci_addr);
        let _ = fs::write(&override_path, "\n");

        // Trigger reprobe
        let probe_path = "/sys/bus/pci/drivers_probe";
        let _ = fs::write(probe_path, pci_addr);
    }

    // Wait for interfaces to settle
    println!("  Waiting for interfaces to appear...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Scan for interface names and update config mappings
    println!();
    println!("{}", "Updating interface mappings...".bright_cyan());

    let mut new_mappings = std::collections::HashMap::new();
    for pci_addr in &pci_addresses {
        // Check if interface reappeared
        let net_dir = format!("/sys/bus/pci/devices/{}/net", pci_addr);
        if let Ok(entries) = fs::read_dir(&net_dir) {
            for entry in entries.flatten() {
                let iface_name = entry.file_name().to_string_lossy().to_string();
                new_mappings.insert(iface_name.clone(), pci_addr.clone());
                println!("  {} → {}", pci_addr.bright_blue(), iface_name.bright_green());
            }
        }
    }

    if !new_mappings.is_empty() {
        save_pci_mappings(&new_mappings)?;
        println!();
        println!("{}", "✓ Interface mappings updated in config".bright_green());
    }

    Ok(())
}

/// Apply saved configuration
pub fn apply_config(config: &Config) -> Result<()> {
    println!("{}", "Applying VFIO configuration...".bright_cyan());
    println!();

    if config.devices.vfio.is_empty() {
        println!("{}", "No devices configured for VFIO.".bright_yellow());
        return Ok(());
    }

    // Load VFIO module
    if config.options.auto_load_module {
        ensure_vfio_module_loaded()?;
    }

    // Bind VFIO devices
    let vfio_refs: Vec<&str> = config.devices.vfio.iter().map(String::as_str).collect();
    bind_interfaces(&vfio_refs)?;

    // Set permissions
    if config.options.set_permissions {
        set_vfio_permissions()?;
    }

    Ok(())
}

/// Bind a single device to VFIO
fn bind_device(device: &NetworkDevice) -> Result<()> {
    // Check current status
    if device.is_vfio_bound() {
        println!("  {} Already bound to vfio-pci", "✓".bright_green());
        return Ok(());
    }

    // Step 1: Unbind from current driver (if any)
    if device.driver.is_some() {
        unbind_pci_device(&device.pci_address)?;
        println!("  {} Unbound from {}", "✓".bright_green(),
            device.driver.as_ref().unwrap());
    }

    // Step 2: Register device ID with VFIO
    register_device_id(&device.vendor_id, &device.device_id)?;

    // Step 3: Bind to vfio-pci
    bind_pci_device(&device.pci_address)?;
    println!("  {} Bound to vfio-pci", "✓".bright_green());

    // Step 4: Verify
    if let Some(group) = device.iommu_group {
        let vfio_dev = format!("/dev/vfio/{}", group);
        if Path::new(&vfio_dev).exists() {
            println!("  {} Device node: {}", "✓".bright_green(), vfio_dev);
        }
    }

    Ok(())
}

/// Unbind a single device from VFIO
fn unbind_device(device: &NetworkDevice) -> Result<()> {
    if !device.is_vfio_bound() {
        println!("  {} Not bound to vfio-pci", "ℹ".bright_blue());
        return Ok(());
    }

    unbind_pci_device(&device.pci_address)?;
    println!("  {} Unbound from vfio-pci", "✓".bright_green());

    Ok(())
}

/// Ensure VFIO module is loaded
fn ensure_vfio_module_loaded() -> Result<()> {
    // Check if already loaded
    let modules = fs::read_to_string("/proc/modules")
        .context("Failed to read /proc/modules")?;

    if modules.lines().any(|line| line.starts_with("vfio_pci")) {
        return Ok(());
    }

    // Load module
    println!("{}", "Loading vfio-pci module...".bright_cyan());

    let output = std::process::Command::new("modprobe")
        .arg("vfio-pci")
        .output()
        .context("Failed to run modprobe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to load vfio-pci module: {}", stderr);
    }

    println!("{}", "✓ VFIO module loaded".bright_green());
    Ok(())
}

/// Register device ID with VFIO driver
fn register_device_id(vendor: &str, device: &str) -> Result<()> {
    let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";

    // Extract hex values (remove 0x prefix if present)
    let vendor_hex = vendor.trim_start_matches("0x");
    let device_hex = device.trim_start_matches("0x");

    let id_string = format!("{} {}", vendor_hex, device_hex);

    // This might fail if already registered, which is fine
    let _ = fs::write(new_id_path, &id_string);

    Ok(())
}

/// Bind PCI device to vfio-pci
fn bind_pci_device(pci_address: &str) -> Result<()> {
    // Check if already bound to vfio-pci (idempotent operation)
    if is_bound_to_vfio(pci_address) {
        return Ok(());
    }

    let bind_path = "/sys/bus/pci/drivers/vfio-pci/bind";

    // Try to bind
    match fs::write(bind_path, pci_address) {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(16) => {
            // EBUSY (error 16) - check if device is already bound to vfio-pci
            // This can happen if register_device_id() auto-bound the device
            if is_bound_to_vfio(pci_address) {
                // Already bound to vfio-pci - this is actually success
                Ok(())
            } else {
                // Device is busy with something else - real error
                Err(e).context(format!(
                    "Failed to bind {} to vfio-pci: device is busy with another driver",
                    pci_address
                ))
            }
        }
        Err(e) => Err(e).context(format!("Failed to bind {} to vfio-pci", pci_address)),
    }
}

/// Check if a PCI device is currently bound to vfio-pci
fn is_bound_to_vfio(pci_address: &str) -> bool {
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", pci_address);

    if let Ok(target) = fs::read_link(&driver_path) {
        if let Some(driver_name) = target.file_name() {
            return driver_name.to_string_lossy() == "vfio-pci";
        }
    }

    false
}

/// Unbind PCI device from its current driver
fn unbind_pci_device(pci_address: &str) -> Result<()> {
    let device_path = format!("/sys/bus/pci/devices/{}/driver/unbind", pci_address);

    // This might fail if already unbound, which is fine
    let _ = fs::write(&device_path, pci_address);

    Ok(())
}

/// Check if a string looks like a PCI address (format: 0000:XX:XX.X)
fn is_pci_address(s: &str) -> bool {
    // PCI address format: 4 hex digits : 2 hex digits : 2 hex digits . 1 hex digit
    // Example: 0000:01:00.0
    s.contains(':') && s.contains('.')
}

/// Bind device by PCI address directly (without interface name)
fn bind_by_pci_address(pci_address: &str) -> Result<()> {
    // Check if device exists
    let device_path_str = format!("/sys/bus/pci/devices/{}", pci_address);
    let device_path = Path::new(&device_path_str);
    if !device_path.exists() {
        anyhow::bail!("PCI device {} not found", pci_address);
    }

    // Get vendor and device IDs
    let vendor = fs::read_to_string(device_path.join("vendor"))
        .context("Failed to read vendor ID")?
        .trim()
        .to_string();
    let device = fs::read_to_string(device_path.join("device"))
        .context("Failed to read device ID")?
        .trim()
        .to_string();

    // Check if already bound to vfio-pci
    if is_bound_to_vfio(pci_address) {
        println!("  {} Already bound to vfio-pci", "✓".bright_green());
        return Ok(());
    }

    // Unbind from current driver if any
    let driver_path = format!("/sys/bus/pci/devices/{}/driver", pci_address);
    if Path::new(&driver_path).exists() {
        if let Ok(target) = fs::read_link(&driver_path) {
            if let Some(driver_name) = target.file_name() {
                let driver = driver_name.to_string_lossy();
                unbind_pci_device(pci_address)?;
                println!("  {} Unbound from {}", "✓".bright_green(), driver);
            }
        }
    }

    // Register device ID with VFIO
    register_device_id(&vendor, &device)?;

    // Bind to vfio-pci
    bind_pci_device(pci_address)?;
    println!("  {} Bound to vfio-pci", "✓".bright_green());

    Ok(())
}

/// Unbind device by PCI address directly
fn unbind_by_pci_address(pci_address: &str) -> Result<()> {
    // Check if device exists
    let device_path_str = format!("/sys/bus/pci/devices/{}", pci_address);
    let device_path = Path::new(&device_path_str);
    if !device_path.exists() {
        anyhow::bail!("PCI device {} not found", pci_address);
    }

    // Check if bound to vfio-pci
    if is_bound_to_vfio(pci_address) {
        unbind_pci_device(pci_address)?;
        println!("  {} Unbound {} from vfio-pci", "✓".bright_green(), pci_address);
    } else {
        // Check what driver it's bound to
        let driver_path = format!("/sys/bus/pci/devices/{}/driver", pci_address);
        if let Ok(target) = fs::read_link(&driver_path) {
            if let Some(driver_name) = target.file_name() {
                let driver = driver_name.to_string_lossy();
                println!("  {} Device {} is bound to {} (not vfio-pci)",
                    "ℹ".bright_blue(), pci_address, driver);
            }
        } else {
            println!("  {} Device {} has no driver bound", "ℹ".bright_blue(), pci_address);
        }
    }

    Ok(())
}

/// Try to find a PCI address for an interface name
fn find_pci_address_in_vfio(interface: &str) -> Option<String> {
    // Strategy 1: Check the saved config for interface->PCI mappings
    if let Ok(config) = crate::config::load_config() {
        if let Some(pci_addr) = config.devices.pci_mappings.get(interface) {
            return Some(pci_addr.clone());
        }
    }

    // Strategy 2: Check if interface still exists in /sys/class/net
    let net_link = format!("/sys/class/net/{}/device", interface);
    if let Ok(target) = fs::read_link(&net_link) {
        if let Some(device_name) = target.file_name() {
            return Some(device_name.to_string_lossy().to_string());
        }
    }

    None
}

/// Save PCI mappings to config file
fn save_pci_mappings(mappings: &std::collections::HashMap<String, String>) -> Result<()> {
    // Load existing config
    let mut config = crate::config::load_config().unwrap_or_default();

    // Merge new mappings with existing ones
    for (iface, pci) in mappings {
        config.devices.pci_mappings.insert(iface.clone(), pci.clone());
    }

    // Save back
    crate::config::save_config_raw(&config)?;

    Ok(())
}

/// Set permissions on VFIO device nodes
fn set_vfio_permissions() -> Result<()> {
    println!("{}", "Setting VFIO device permissions...".bright_cyan());

    let vfio_dir = Path::new("/dev/vfio");

    if !vfio_dir.exists() {
        anyhow::bail!("/dev/vfio/ does not exist. Is VFIO enabled?");
    }

    // Set permissions on main vfio device
    set_file_permissions("/dev/vfio/vfio", 0o666)?;

    // Set permissions on all group devices
    for entry in fs::read_dir(vfio_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.file_name().unwrap() != "vfio" {
            set_file_permissions(&path, 0o666)?;
        }
    }

    println!("{}", "✓ Permissions set to rw-rw-rw-".bright_green());
    Ok(())
}

/// Set file permissions
fn set_file_permissions<P: AsRef<Path>>(path: P, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = path.as_ref();
    let permissions = fs::Permissions::from_mode(mode);

    fs::set_permissions(path, permissions)
        .context(format!("Failed to set permissions on {}", path.display()))?;

    Ok(())
}

/// List VFIO devices
fn list_vfio_devices() -> Result<()> {
    let vfio_dir = Path::new("/dev/vfio");

    if !vfio_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(vfio_dir)? {
        let entry = entry?;
        let name = entry.file_name();

        if name != "vfio" {
            println!("  /dev/vfio/{}", name.to_string_lossy());
        }
    }

    Ok(())
}

/// Check interfaces with specific mode requirements
/// Exit codes: 0 = all good, 1 = not found, 2 = wrong mode, 3 = other error
pub fn check_interfaces_with_mode(vfio_ifaces: &[&str], kernel_ifaces: &[&str], existence_ifaces: &[&str]) -> Result<()> {
    println!("{}", "Checking interfaces...".bright_cyan());
    println!();

    let mut all_ok = true;
    let mut not_found = false;
    let mut wrong_mode = false;

    // Check VFIO interfaces (must be in VFIO mode)
    if !vfio_ifaces.is_empty() {
        println!("{}", "Interfaces that must be in VFIO mode:".bright_green());
        for interface in vfio_ifaces {
            match device::get_device_info(interface) {
                Ok(dev) => {
                    if dev.status == DeviceStatus::Vfio {
                        println!("{} {} - {}", "✓".bright_green(), interface.bright_white(), "VFIO mode".bright_green());
                        println!("  PCI: {} | Driver: {} | IOMMU Group: {}",
                            dev.pci_address,
                            dev.driver.as_deref().unwrap_or("unknown"),
                            dev.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "N/A".to_string())
                        );
                    } else {
                        println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "NOT in VFIO mode".bright_red());
                        println!("  PCI: {} | Driver: {} | Current mode: {}",
                            dev.pci_address,
                            dev.driver.as_deref().unwrap_or("unknown"),
                            match dev.status {
                                DeviceStatus::Kernel => "kernel".bright_yellow(),
                                DeviceStatus::Unbound => "unbound".bright_red(),
                                _ => "unknown".bright_red(),
                            }
                        );
                        all_ok = false;
                        wrong_mode = true;
                    }
                }
                Err(_) => {
                    println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "INTERFACE NOT FOUND".bright_red().bold());
                    all_ok = false;
                    not_found = true;
                }
            }
        }
        println!();
    }

    // Check kernel interfaces (must be in kernel mode)
    if !kernel_ifaces.is_empty() {
        println!("{}", "Interfaces that must be in kernel mode:".bright_yellow());
        for interface in kernel_ifaces {
            match device::get_device_info(interface) {
                Ok(dev) => {
                    if dev.status == DeviceStatus::Kernel {
                        println!("{} {} - {}", "✓".bright_green(), interface.bright_white(), "kernel mode".bright_yellow());
                        println!("  PCI: {} | Driver: {} | IOMMU Group: {}",
                            dev.pci_address,
                            dev.driver.as_deref().unwrap_or("unknown"),
                            dev.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "N/A".to_string())
                        );
                    } else {
                        println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "NOT in kernel mode".bright_red());
                        println!("  PCI: {} | Driver: {} | Current mode: {}",
                            dev.pci_address,
                            dev.driver.as_deref().unwrap_or("unknown"),
                            match dev.status {
                                DeviceStatus::Vfio => "VFIO".bright_green(),
                                DeviceStatus::Unbound => "unbound".bright_red(),
                                _ => "unknown".bright_red(),
                            }
                        );
                        all_ok = false;
                        wrong_mode = true;
                    }
                }
                Err(_) => {
                    println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "INTERFACE NOT FOUND".bright_red().bold());
                    all_ok = false;
                    not_found = true;
                }
            }
        }
        println!();
    }

    // Check existence only (any mode is okay)
    if !existence_ifaces.is_empty() {
        println!("{}", "Interfaces that must exist (any mode):".bright_cyan());
        for interface in existence_ifaces {
            match device::get_device_info(interface) {
                Ok(dev) => {
                    let mode_str = match dev.status {
                        DeviceStatus::Vfio => "VFIO".bright_green(),
                        DeviceStatus::Kernel => "kernel".bright_yellow(),
                        DeviceStatus::Unbound => "unbound".bright_red(),
                    };
                    println!("{} {} - exists in {} mode", "✓".bright_green(), interface.bright_white(), mode_str);
                    println!("  PCI: {} | Driver: {}",
                        dev.pci_address,
                        dev.driver.as_deref().unwrap_or("none")
                    );
                }
                Err(_) => {
                    println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "INTERFACE NOT FOUND".bright_red().bold());
                    all_ok = false;
                    not_found = true;
                }
            }
        }
        println!();
    }

    if all_ok {
        println!("{}", "✓ All interface checks passed".bright_green().bold());
        Ok(())
    } else if not_found {
        println!("{}", "✗ One or more interfaces not found".bright_red().bold());
        anyhow::bail!("One or more interfaces not found")
    } else if wrong_mode {
        println!("{}", "✗ One or more interfaces in wrong mode".bright_red().bold());
        println!();
        println!("To fix:");
        if !vfio_ifaces.is_empty() {
            println!("  {} {}", "sudo vfio-tool ensure-vfio".bright_cyan(), vfio_ifaces.join(","));
        }
        if !kernel_ifaces.is_empty() {
            println!("  {} {}", "sudo vfio-tool unbind".bright_cyan(), kernel_ifaces.join(","));
        }
        anyhow::bail!("One or more interfaces in wrong mode")
    } else {
        anyhow::bail!("Unknown error checking interfaces")
    }
}

/// Check if specific interfaces exist and are in VFIO mode (backward compatibility)
/// Exit codes: 0 = all good, 1 = not found, 2 = not in VFIO mode, 3 = other error
#[allow(dead_code)]
pub fn check_interfaces(interfaces: &[&str]) -> Result<()> {
    println!("{}", "Checking required interfaces...".bright_cyan());
    println!();

    let mut all_ok = true;
    let mut not_found = false;
    let mut not_vfio = false;

    for interface in interfaces {
        match device::get_device_info(interface) {
            Ok(dev) => {
                if dev.status == DeviceStatus::Vfio {
                    println!("{} {}", "✓".bright_green(), interface.bright_white());
                    println!("  PCI: {}", dev.pci_address);
                    println!("  Driver: {}", dev.driver.as_deref().unwrap_or("unknown"));
                    if let Some(group) = dev.iommu_group {
                        println!("  IOMMU Group: {}", group);
                    }
                    println!("  Status: {} {}", "VFIO".bright_green(), "✓".bright_green());
                } else {
                    println!("{} {}", "✗".bright_red(), interface.bright_white());
                    println!("  PCI: {}", dev.pci_address);
                    println!("  Driver: {}", dev.driver.as_deref().unwrap_or("unknown"));
                    let status_str = match dev.status {
                        DeviceStatus::Kernel => "kernel mode (not VFIO)".bright_yellow(),
                        DeviceStatus::Unbound => "unbound (no driver)".bright_red(),
                        _ => "unknown".bright_red(),
                    };
                    println!("  Status: {}", status_str);
                    all_ok = false;
                    not_vfio = true;
                }
            }
            Err(_) => {
                println!("{} {}", "✗".bright_red(), interface.bright_white());
                println!("  Status: {}", "INTERFACE NOT FOUND".bright_red().bold());
                all_ok = false;
                not_found = true;
            }
        }
        println!();
    }

    if all_ok {
        println!("{}", "✓ All required interfaces are in VFIO mode".bright_green().bold());
        Ok(())
    } else if not_found {
        println!("{}", "✗ Not all required interfaces are available".bright_red().bold());
        anyhow::bail!("One or more interfaces not found")
    } else if not_vfio {
        println!("{}", "✗ Not all required interfaces are in VFIO mode".bright_red().bold());
        println!();
        println!("To bind interfaces to VFIO:");
        println!("  {} {}", "sudo vfio-tool ensure-vfio".bright_cyan(), interfaces.join(","));
        anyhow::bail!("One or more interfaces not in VFIO mode")
    } else {
        anyhow::bail!("Unknown error checking interfaces")
    }
}

/// Ensure interfaces are in VFIO mode, binding them if necessary
/// Exit codes: 0 = success, 1 = not found, 2 = failed to bind, 3 = other error
pub fn ensure_vfio(interfaces: &[&str]) -> Result<()> {
    println!("{}", "Ensuring interfaces are in VFIO mode...".bright_cyan());
    println!();

    // Load VFIO module if not loaded
    ensure_vfio_module_loaded()?;

    let mut all_ok = true;
    let mut not_found = false;
    let mut bind_failed = false;

    for interface in interfaces {
        match device::get_device_info(interface) {
            Ok(dev) => {
                if dev.status == DeviceStatus::Vfio {
                    println!("{} {} - {}", "✓".bright_green(), interface.bright_white(), "already in VFIO mode".bright_green());
                } else {
                    println!("{} {} - {}", "○".bright_yellow(), interface.bright_white(), "currently in kernel mode, binding...".bright_yellow());

                    match bind_device(&dev) {
                        Ok(()) => {
                            println!("  {} {} ({}) bound to vfio-pci", "✓".bright_green(), interface, dev.pci_address);
                        }
                        Err(e) => {
                            println!("  {} Failed to bind {}: {}", "✗".bright_red(), interface, e);
                            all_ok = false;
                            bind_failed = true;
                        }
                    }
                }
            }
            Err(_) => {
                println!("{} {} - {}", "✗".bright_red(), interface.bright_white(), "INTERFACE NOT FOUND".bright_red().bold());
                all_ok = false;
                not_found = true;
            }
        }
        println!();
    }

    if all_ok {
        println!("{}", "✓ All interfaces are now in VFIO mode".bright_green().bold());
        Ok(())
    } else if not_found {
        println!("{}", "✗ One or more interfaces not found".bright_red().bold());
        anyhow::bail!("One or more interfaces not found")
    } else if bind_failed {
        println!("{}", "✗ Failed to bind one or more interfaces".bright_red().bold());
        anyhow::bail!("Failed to bind one or more interfaces")
    } else {
        anyhow::bail!("Unknown error ensuring VFIO mode")
    }
}
