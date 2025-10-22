use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use colored::Colorize;
use dialoguer::{MultiSelect, Confirm};

use crate::device;

const CONFIG_DIR: &str = "/etc/vfio-tool";
const CONFIG_FILE: &str = "/etc/vfio-tool/config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub devices: DeviceConfig,
    pub options: Options,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    #[serde(default)]
    pub vfio: Vec<String>,

    #[serde(default)]
    pub kernel: Vec<String>,

    /// Mapping of interface names to PCI addresses
    /// This allows us to unbind by interface name even when interface disappeared
    #[serde(default)]
    pub pci_mappings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    #[serde(default = "default_true")]
    pub set_permissions: bool,

    #[serde(default = "default_true")]
    pub auto_load_module: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            devices: DeviceConfig {
                vfio: Vec::new(),
                kernel: Vec::new(),
                pci_mappings: HashMap::new(),
            },
            options: Options {
                set_permissions: true,
                auto_load_module: true,
            },
        }
    }
}

/// Validate configuration against current hardware
pub fn validate_config() -> Result<()> {
    println!("{}", "Validating configuration against current hardware...".bright_cyan());
    println!();

    let cfg = load_config()?;
    let current_devices = device::list_network_devices()?;

    let current_interfaces: Vec<String> = current_devices
        .iter()
        .map(|d| d.interface.clone())
        .collect();

    let mut has_issues = false;

    // Check VFIO devices
    println!("{}", "VFIO devices (kernel bypass):".bright_green());
    if cfg.devices.vfio.is_empty() {
        println!("  {}", "(none)".bright_black());
    } else {
        for iface in &cfg.devices.vfio {
            if current_interfaces.contains(iface) {
                println!("  ✓ {} - {}", iface, "present".bright_green());
            } else {
                println!("  ✗ {} - {}", iface, "MISSING".bright_red().bold());
                has_issues = true;
            }
        }
    }

    println!();

    // Check kernel devices
    println!("{}", "Kernel devices (normal networking):".bright_yellow());
    if cfg.devices.kernel.is_empty() {
        println!("  {}", "(none)".bright_black());
    } else {
        for iface in &cfg.devices.kernel {
            if current_interfaces.contains(iface) {
                println!("  ✓ {} - {}", iface, "present".bright_green());
            } else {
                println!("  ✗ {} - {}", iface, "MISSING".bright_red().bold());
                has_issues = true;
            }
        }
    }

    println!();

    // Show unconfigured interfaces
    let configured_interfaces: Vec<&String> = cfg.devices.vfio
        .iter()
        .chain(cfg.devices.kernel.iter())
        .collect();

    let unconfigured: Vec<&device::NetworkDevice> = current_devices
        .iter()
        .filter(|d| !configured_interfaces.contains(&&d.interface))
        .collect();

    if !unconfigured.is_empty() {
        println!("{}", "New/unconfigured interfaces:".bright_cyan());
        for dev in &unconfigured {
            println!("  + {} ({} - {})",
                dev.interface.bright_white(),
                dev.pci_address,
                dev.driver.as_deref().unwrap_or("no driver"));
        }
        println!();
        println!("{}", "These interfaces are not in your configuration.".bright_yellow());
        has_issues = true;
    }

    println!();

    if has_issues {
        println!("{}", "⚠ Configuration does not match current hardware".bright_yellow().bold());
        println!();
        println!("Options:");
        println!("  1. Run {} to reconfigure", "sudo vfio-tool configure".bright_cyan());
        println!("  2. Run {} to add/remove interfaces", "sudo vfio-tool update".bright_cyan());
        println!("  3. Manually edit {}", CONFIG_FILE.bright_cyan());
        return Err(anyhow::anyhow!("Configuration validation failed"));
    } else {
        println!("{}", "✓ Configuration matches current hardware".bright_green().bold());
    }

    Ok(())
}

/// Interactive configuration update (preserves existing config where possible)
pub fn interactive_update() -> Result<()> {
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("{}", "    Update VFIO Configuration".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    // Load existing config
    let existing_cfg = match load_config() {
        Ok(cfg) => {
            println!("{}", "✓ Loaded existing configuration".bright_green());
            Some(cfg)
        }
        Err(_) => {
            println!("{}", "No existing configuration found. Starting fresh.".bright_yellow());
            None
        }
    };

    println!();

    // Get all network devices
    let devices = device::list_network_devices()?;

    if devices.is_empty() {
        println!("{}", "No network devices found.".bright_red());
        return Ok(());
    }

    // Check what's changed
    if let Some(ref cfg) = existing_cfg {
        println!("{}", "Checking for hardware changes...".bright_cyan());
        println!();

        let current_interfaces: Vec<String> = devices
            .iter()
            .map(|d| d.interface.clone())
            .collect();

        // Check for missing interfaces
        let missing_vfio: Vec<&String> = cfg.devices.vfio
            .iter()
            .filter(|iface| !current_interfaces.contains(iface))
            .collect();

        let missing_kernel: Vec<&String> = cfg.devices.kernel
            .iter()
            .filter(|iface| !current_interfaces.contains(iface))
            .collect();

        if !missing_vfio.is_empty() || !missing_kernel.is_empty() {
            println!("{}", "⚠ Some configured interfaces are missing:".bright_yellow());
            for iface in &missing_vfio {
                println!("  - {} (was: VFIO)", iface.bright_red());
            }
            for iface in &missing_kernel {
                println!("  - {} (was: Kernel)", iface.bright_red());
            }
            println!();
        }

        // Check for new interfaces
        let configured_interfaces: Vec<&String> = cfg.devices.vfio
            .iter()
            .chain(cfg.devices.kernel.iter())
            .collect();

        let new_interfaces: Vec<&device::NetworkDevice> = devices
            .iter()
            .filter(|d| !configured_interfaces.contains(&&d.interface))
            .collect();

        if !new_interfaces.is_empty() {
            println!("{}", "✓ Found new interfaces:".bright_green());
            for dev in &new_interfaces {
                println!("  + {} ({} - {})",
                    dev.interface.bright_white(),
                    dev.pci_address,
                    dev.driver.as_deref().unwrap_or("no driver"));
            }
            println!();
        }

        if missing_vfio.is_empty() && missing_kernel.is_empty() && new_interfaces.is_empty() {
            println!("{}", "✓ No hardware changes detected".bright_green());
            println!();

            let should_continue = Confirm::new()
                .with_prompt("Continue anyway to modify configuration?")
                .default(false)
                .interact()?;

            if !should_continue {
                println!("Update cancelled.");
                return Ok(());
            }
        }
    }

    // Show current status
    println!("{}", "Current network interfaces:".bright_cyan());
    println!();

    for dev in &devices {
        let status_str = match dev.status {
            device::DeviceStatus::Vfio => "VFIO".bright_green(),
            device::DeviceStatus::Kernel => "Kernel".bright_yellow(),
            device::DeviceStatus::Unbound => "Unbound".bright_red(),
        };

        println!("  {} - {} - {} - Group {}",
            dev.interface.bright_white(),
            dev.vendor_device(),
            dev.driver.as_deref().unwrap_or("no driver"),
            dev.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "N/A".to_string()),
        );
        println!("    Current: {}", status_str);

        // Show previous config if exists
        if let Some(ref cfg) = existing_cfg {
            if cfg.devices.vfio.contains(&dev.interface) {
                println!("    Configured as: {}", "VFIO".bright_green());
            } else if cfg.devices.kernel.contains(&dev.interface) {
                println!("    Configured as: {}", "Kernel".bright_yellow());
            } else {
                println!("    Configured as: {}", "Not configured".bright_black());
            }
        }
        println!();
    }

    println!();
    println!("Select interfaces for {} (kernel bypass):", "VFIO".bright_green());
    println!("Use {} to navigate, {} to select/deselect, {} when done",
        "↑↓".bright_cyan(), "Space".bright_cyan(), "Enter".bright_cyan());
    println!();

    let options: Vec<String> = devices
        .iter()
        .map(|d| format!("{} - {} - {} - Group {}",
            d.interface,
            d.vendor_device(),
            d.driver.as_deref().unwrap_or("no driver"),
            d.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "N/A".to_string())
        ))
        .collect();

    // Pre-select based on existing config
    let defaults: Vec<bool> = if let Some(ref cfg) = existing_cfg {
        devices
            .iter()
            .map(|d| cfg.devices.vfio.contains(&d.interface))
            .collect()
    } else {
        vec![false; devices.len()]
    };

    let selections = MultiSelect::new()
        .items(&options)
        .defaults(&defaults)
        .interact()?;

    let vfio_interfaces: Vec<String> = selections
        .iter()
        .map(|&i| devices[i].interface.clone())
        .collect();

    // Kernel interfaces are everything not selected for VFIO
    let kernel_interfaces: Vec<String> = devices
        .iter()
        .enumerate()
        .filter(|(i, _)| !selections.contains(i))
        .map(|(_, d)| d.interface.clone())
        .collect();

    println!();
    println!("{}", "Updated configuration:".bright_cyan());
    println!();

    println!("  {} (kernel bypass):", "VFIO devices".bright_green());
    if vfio_interfaces.is_empty() {
        println!("    {}", "(none)".bright_black());
    } else {
        for iface in &vfio_interfaces {
            println!("    - {}", iface);
        }
    }

    println!();
    println!("  {} (normal networking):", "Kernel devices".bright_yellow());
    if kernel_interfaces.is_empty() {
        println!("    {}", "(all will use VFIO)".bright_black());
    } else {
        for iface in &kernel_interfaces {
            println!("    - {}", iface);
        }
    }

    println!();

    // Preserve existing options or use defaults
    let options = if let Some(cfg) = existing_cfg {
        cfg.options
    } else {
        Options {
            set_permissions: true,
            auto_load_module: true,
        }
    };

    // Save configuration with preserved options
    save_config_with_options(
        vfio_interfaces.clone(),
        kernel_interfaces.clone(),
        options.set_permissions
    )?;

    println!();
    println!("{}", "✓ Configuration updated and saved".bright_green());
    println!();

    // Ask to apply immediately
    let should_apply = Confirm::new()
        .with_prompt("Apply updated configuration now?")
        .default(true)
        .interact()?;

    if should_apply {
        println!();
        use crate::vfio;
        let cfg = load_config()?;
        vfio::apply_config(&cfg)?;
    }

    // Ask about persistence
    let should_install = Confirm::new()
        .with_prompt("Update systemd service to use new configuration?")
        .default(true)
        .interact()?;

    if should_install {
        println!();
        println!("{}", "Systemd service will use updated configuration on next boot.".bright_cyan());
        println!("If service is already installed, no action needed.");
        println!("If not installed, run {} when ready.", "sudo vfio-tool install".bright_cyan());
    }

    Ok(())
}

/// Interactive configuration wizard
pub fn interactive_configure() -> Result<()> {
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("{}", "    VFIO Configuration Wizard".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    // Get all network devices
    let devices = device::list_network_devices()?;

    if devices.is_empty() {
        println!("{}", "No network devices found.".bright_red());
        return Ok(());
    }

    // Show current status
    println!("{}", "Current network interfaces:".bright_cyan());
    println!();

    for dev in &devices {
        let status_str = match dev.status {
            device::DeviceStatus::Vfio => "VFIO".bright_green(),
            device::DeviceStatus::Kernel => "Kernel".bright_yellow(),
            device::DeviceStatus::Unbound => "Unbound".bright_red(),
        };

        println!("  {} - {} ({})",
            dev.interface.bright_white(),
            dev.pci_address,
            status_str
        );

        if let Some(ref driver) = dev.driver {
            println!("    Driver: {}", driver);
        }

        if let Some(group) = dev.iommu_group {
            println!("    IOMMU Group: {}", group);
        }

        if let Some(ref speed) = dev.speed {
            println!("    Speed: {}", speed);
        }

        println!();
    }

    // Select interfaces for VFIO
    let items: Vec<String> = devices
        .iter()
        .map(|d| {
            format!("{} - {} - {} - Group {}",
                d.interface,
                d.vendor_device(),
                d.driver.as_deref().unwrap_or("no driver"),
                d.iommu_group.map(|g| g.to_string()).unwrap_or_else(|| "?".to_string())
            )
        })
        .collect();

    println!("{}", "Select interfaces to bind to VFIO for kernel bypass:".bright_cyan());
    println!("{}", "(Use Space to select, Enter to confirm)".bright_black());

    let defaults: Vec<bool> = devices.iter().map(|d| d.is_vfio_bound()).collect();

    let selections = MultiSelect::new()
        .items(&items)
        .defaults(&defaults)
        .interact()?;

    let vfio_interfaces: Vec<String> = selections
        .iter()
        .map(|&i| devices[i].interface.clone())
        .collect();

    let kernel_interfaces: Vec<String> = devices
        .iter()
        .enumerate()
        .filter(|(i, _)| !selections.contains(i))
        .map(|(_, d)| d.interface.clone())
        .collect();

    println!();
    println!("{}", "Configuration:".bright_cyan());
    println!();
    println!("  VFIO (kernel bypass): {}", vfio_interfaces.join(", "));
    println!("  Kernel (normal networking): {}", kernel_interfaces.join(", "));
    println!();

    // Options
    let apply_now = Confirm::new()
        .with_prompt("Apply changes immediately?")
        .default(true)
        .interact()?;

    let make_persistent = Confirm::new()
        .with_prompt("Make persistent (install systemd service)?")
        .default(true)
        .interact()?;

    let set_permissions = Confirm::new()
        .with_prompt("Set /dev/vfio/* permissions for non-root access?")
        .default(true)
        .interact()?;

    // Save configuration
    save_config_with_options(vfio_interfaces.clone(), kernel_interfaces, set_permissions)?;

    // Apply now if requested
    if apply_now {
        println!();
        println!("{}", "Applying configuration...".bright_cyan());

        let vfio_refs: Vec<&str> = vfio_interfaces.iter().map(String::as_str).collect();
        crate::vfio::bind_interfaces(&vfio_refs)?;
    }

    // Install service if requested
    if make_persistent {
        println!();
        crate::systemd::install_service()?;
    }

    println!();
    println!("{}", "✓ Configuration complete!".bright_green());

    Ok(())
}

/// Save configuration
pub fn save_config(vfio: Vec<String>, kernel: Vec<String>) -> Result<()> {
    save_config_with_options(vfio, kernel, true)
}

/// Save configuration with options
fn save_config_with_options(
    vfio: Vec<String>,
    kernel: Vec<String>,
    set_permissions: bool,
) -> Result<()> {
    // Create config directory if it doesn't exist
    fs::create_dir_all(CONFIG_DIR)
        .context("Failed to create config directory")?;

    // Load existing config to preserve PCI mappings
    let existing_mappings = if let Ok(existing_config) = load_config() {
        existing_config.devices.pci_mappings
    } else {
        HashMap::new()
    };

    // Build new PCI mappings for all interfaces
    let mut pci_mappings = existing_mappings.clone();

    // Get all network devices (including unbound ones)
    let all_devices = crate::device::list_network_devices()
        .unwrap_or_default();

    // Add/update mappings for interfaces in vfio list
    for iface in &vfio {
        // Try to find device in current device list
        if let Some(device) = all_devices.iter().find(|d| &d.interface == iface) {
            pci_mappings.insert(iface.clone(), device.pci_address.clone());
        } else if !pci_mappings.contains_key(iface) {
            // Interface not found and not in existing mappings
            // This is OK - might be temporarily unavailable or specified by mistake
            println!("  {} Warning: Interface {} not found, no PCI mapping available", "⚠".bright_yellow(), iface);
        }
        // If already in pci_mappings, preserve the existing mapping
    }

    // Add/update mappings for interfaces in kernel list
    for iface in &kernel {
        if let Some(device) = all_devices.iter().find(|d| &d.interface == iface) {
            pci_mappings.insert(iface.clone(), device.pci_address.clone());
        } else if !pci_mappings.contains_key(iface) {
            println!("  {} Warning: Interface {} not found, no PCI mapping available", "⚠".bright_yellow(), iface);
        }
    }

    let config = Config {
        devices: DeviceConfig {
            vfio,
            kernel,
            pci_mappings,
        },
        options: Options {
            set_permissions,
            auto_load_module: true,
        },
    };

    let toml = toml::to_string_pretty(&config)
        .context("Failed to serialize config")?;

    fs::write(CONFIG_FILE, toml)
        .context("Failed to write config file")?;

    println!("{}", "✓ Configuration saved to /etc/vfio-tool/config.toml".bright_green());

    Ok(())
}

/// Save raw config structure (used internally to preserve all fields)
pub fn save_config_raw(config: &Config) -> Result<()> {
    // Create config directory if it doesn't exist
    fs::create_dir_all(CONFIG_DIR)
        .context("Failed to create config directory")?;

    let toml = toml::to_string_pretty(config)
        .context("Failed to serialize config")?;

    fs::write(CONFIG_FILE, toml)
        .context("Failed to write config file")?;

    Ok(())
}

/// Load configuration
pub fn load_config() -> Result<Config> {
    if !Path::new(CONFIG_FILE).exists() {
        anyhow::bail!("Configuration file not found: {}\nRun 'vfio-tool configure' to create one.", CONFIG_FILE);
    }

    let content = fs::read_to_string(CONFIG_FILE)
        .context("Failed to read config file")?;

    let config: Config = toml::from_str(&content)
        .context("Failed to parse config file")?;

    Ok(config)
}

/// Get config file path
#[allow(dead_code)]
pub fn get_config_path() -> PathBuf {
    PathBuf::from(CONFIG_FILE)
}
