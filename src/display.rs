use anyhow::Result;
use colored::Colorize;
use tabled::{Table, Tabled, settings::Style};
use serde_json::json;

use crate::device::{NetworkDevice, DeviceStatus};
use crate::iommu::{SystemStatus, SystemIssue};
use crate::config::Config;
use crate::grub::CpuVendor;
use crate::frameworks::{Framework, FrameworkDevice};

#[derive(Tabled)]
struct DeviceRow {
    #[tabled(rename = "INTERFACE")]
    interface: String,

    #[tabled(rename = "PCI ADDRESS")]
    pci_address: String,

    #[tabled(rename = "DRIVER")]
    driver: String,

    #[tabled(rename = "IOMMU GROUP")]
    iommu_group: String,

    #[tabled(rename = "VENDOR:DEVICE")]
    vendor_device: String,

    #[tabled(rename = "STATUS")]
    status: String,

    #[tabled(rename = "MAX SPEED")]
    max_speed: String,

    #[tabled(rename = "LINK")]
    speed: String,
}

/// Show device table
pub fn show_device_table(devices: &[NetworkDevice], verbose: bool) -> Result<()> {
    if devices.is_empty() {
        println!("{}", "No network devices found.".bright_yellow());
        return Ok(());
    }

    let rows: Vec<DeviceRow> = devices
        .iter()
        .map(|d| DeviceRow {
            interface: d.interface.clone(),
            pci_address: d.pci_address.clone(),
            driver: d.driver.clone().unwrap_or_else(|| "(none)".to_string()),
            iommu_group: d.iommu_group
                .map(|g| g.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            vendor_device: d.vendor_device(),
            status: status_to_string(&d.status),
            max_speed: d.max_speed.clone().unwrap_or_else(|| "?".to_string()),
            speed: d.speed.clone().unwrap_or_else(|| "-".to_string()),
        })
        .collect();

    let mut table = Table::new(rows);
    table.with(Style::modern());

    println!("{}", table);

    if verbose {
        println!();
        println!("Legend:");
        println!("  {} - Bound to vfio-pci (kernel bypass)", "VFIO".bright_green());
        println!("  {} - Bound to kernel driver (normal networking)", "kernel".bright_yellow());
        println!("  {} - No driver bound", "unbound".bright_red());
    }

    Ok(())
}

/// Show system status
pub fn show_system_status(status: &SystemStatus) -> Result<()> {
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("{}", "    VFIO System Status".bright_cyan().bold());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    let check = if status.iommu_enabled { "✓".bright_green() } else { "✗".bright_red() };
    println!("{} IOMMU Enabled: {}", check,
        if status.iommu_enabled { "Yes".bright_green() } else { "No".bright_red() });

    let check = if status.vfio_module_loaded { "✓".bright_green() } else { "✗".bright_red() };
    println!("{} VFIO Module Loaded: {}", check,
        if status.vfio_module_loaded { "Yes".bright_green() } else { "No".bright_red() });

    let cpu_str = match status.cpu_vendor {
        CpuVendor::Intel => "Intel",
        CpuVendor::AMD => "AMD",
        CpuVendor::Unknown => "Unknown",
    };
    println!("{} CPU Vendor: {}", "ℹ".bright_blue(), cpu_str.bright_cyan());

    println!("{} IOMMU Groups: {}", "ℹ".bright_blue(),
        status.iommu_groups_count.to_string().bright_cyan());

    println!("{} VFIO Devices: {}", "ℹ".bright_blue(),
        status.vfio_devices_count.to_string().bright_cyan());

    println!();

    if status.iommu_enabled && status.vfio_module_loaded && status.iommu_groups_count > 0 {
        println!("{}", "System is ready for VFIO!".bright_green().bold());
    } else {
        println!("{}", "System is NOT ready for VFIO.".bright_red().bold());
        println!("Run {} to check for issues.", "vfio-tool check".bright_cyan());
    }

    Ok(())
}

/// Show device details
pub fn show_device_details(device: &NetworkDevice) -> Result<()> {
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("{}  {}", "Device:".bright_cyan().bold(), device.interface.bright_white());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    println!("{:20} {}", "PCI Address:", device.pci_address);
    println!("{:20} {}", "Vendor:Device:", device.vendor_device());

    if let Some(ref driver) = device.driver {
        println!("{:20} {}", "Driver:", driver);
    } else {
        println!("{:20} {}", "Driver:", "(none)".bright_red());
    }

    if let Some(group) = device.iommu_group {
        println!("{:20} {}", "IOMMU Group:", group);

        // Show other devices in the same group
        if let Ok(group_devices) = crate::device::get_iommu_group_devices(group) {
            if group_devices.len() > 1 {
                println!("{:20}", "  Other devices:");
                for dev in group_devices {
                    if dev != device.pci_address {
                        println!("{:20}   - {}", "", dev);
                    }
                }
            } else {
                println!("{:20}   {}", "", "(isolated - only device in group)".bright_green());
            }
        }
    } else {
        println!("{:20} {}", "IOMMU Group:", "N/A".bright_red());
    }

    if let Some(ref speed) = device.speed {
        println!("{:20} {}", "Link Speed:", speed);
    }

    println!("{:20} {}", "Status:", status_to_string(&device.status));

    if device.is_vfio_bound() {
        if let Some(group) = device.iommu_group {
            println!("{:20} /dev/vfio/{}", "Device Node:", group);
        }
    }

    Ok(())
}

/// Show configuration
pub fn show_config(config: &Config) -> Result<()> {
    println!("{}", "Current Configuration:".bright_cyan());
    println!();

    println!("{}", "VFIO Devices (kernel bypass):".bright_green());
    if config.devices.vfio.is_empty() {
        println!("  {}", "(none)".bright_black());
    } else {
        for iface in &config.devices.vfio {
            println!("  - {}", iface);
        }
    }

    println!();
    println!("{}", "Kernel Devices (normal networking):".bright_yellow());
    if config.devices.kernel.is_empty() {
        println!("  {}", "(none)".bright_black());
    } else {
        for iface in &config.devices.kernel {
            println!("  - {}", iface);
        }
    }

    println!();
    println!("{}", "Options:".bright_cyan());
    println!("  Set permissions: {}", config.options.set_permissions);
    println!("  Auto-load module: {}", config.options.auto_load_module);

    Ok(())
}

/// Show issues
pub fn show_issues(issues: &[SystemIssue]) -> Result<()> {
    println!("{}", "Found issues:".bright_red().bold());
    println!();

    for (i, issue) in issues.iter().enumerate() {
        println!("{}{} {}", "  ".to_string(), format!("{}.", i + 1).bright_red(), issue.description());
        println!("     {}: {}", "Fix".bright_cyan(), issue.fix_command());
        println!();
    }

    Ok(())
}

/// Explain what would happen to a device
pub fn explain_device(device: &NetworkDevice) -> Result<()> {
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!("{}  {}", "Explanation for:".bright_cyan().bold(), device.interface.bright_white());
    println!("{}", "═══════════════════════════════════════".bright_cyan());
    println!();

    println!("{}", "Current State:".bright_cyan());
    println!("  Status: {}", status_to_string(&device.status));

    if let Some(ref driver) = device.driver {
        println!("  Driver: {}", driver);
    } else {
        println!("  Driver: (none)");
    }

    if let Some(group) = device.iommu_group {
        println!("  IOMMU Group: {}", group);
    }

    println!();

    if device.is_vfio_bound() {
        println!("{}", "Bound to VFIO:".bright_green());
        println!("  • Device is in kernel bypass mode");
        println!("  • NOT visible to kernel networking (ip link, ifconfig)");
        println!("  • Accessible by userspace applications (DPDK, SPDK)");
        println!("  • Direct hardware access via /dev/vfio/{}", device.iommu_group.unwrap());
        println!();

        println!("{}", "To return to kernel:".bright_cyan());
        println!("  vfio-tool unbind {}", device.interface);
    } else {
        println!("{}", "Using kernel networking:".bright_yellow());
        println!("  • Device controlled by kernel driver");
        println!("  • Visible to standard tools (ip, ping, etc.)");
        println!("  • Normal socket-based networking");
        println!();

        println!("{}", "To enable VFIO (kernel bypass):".bright_cyan());
        println!("  vfio-tool bind {}", device.interface);
        println!();

        println!("{}", "This will:".bright_cyan());
        println!("  1. Unbind from {} driver", device.driver.as_deref().unwrap_or("current"));
        println!("  2. Bind to vfio-pci driver");
        println!("  3. Create device node in /dev/vfio/");
        println!("  4. Interface disappears from 'ip link'");
        println!("  5. Available for userspace applications");
    }

    // Check IOMMU group isolation
    if let Some(group) = device.iommu_group {
        if let Ok(group_devices) = crate::device::get_iommu_group_devices(group) {
            if group_devices.len() > 1 {
                println!();
                println!("{}", "⚠ WARNING:".bright_yellow().bold());
                println!("  This device shares IOMMU group {} with:", group);
                for dev in group_devices {
                    if dev != device.pci_address {
                        println!("    - {}", dev);
                    }
                }
                println!("  All devices in the group must be bound to VFIO together.");
            }
        }
    }

    Ok(())
}

fn status_to_string(status: &DeviceStatus) -> String {
    match status {
        DeviceStatus::Vfio => "vfio".to_string(),
        DeviceStatus::Kernel => "kernel".to_string(),
        DeviceStatus::Unbound => "unbound".to_string(),
    }
}

/// Show framework-specific device list
pub fn show_framework_devices(
    framework: Framework,
    devices: &[FrameworkDevice],
    show_capable: bool,
    format: &str,
) -> Result<()> {
    match format {
        "json" => show_framework_json(framework, devices, show_capable),
        "args" => show_framework_args(framework, devices),
        _ => show_framework_default(framework, devices, show_capable),
    }
}

/// Show framework devices in default (human-readable) format
fn show_framework_default(
    framework: Framework,
    devices: &[FrameworkDevice],
    show_capable: bool,
) -> Result<()> {
    if show_capable {
        // Show all capable devices, grouped by state
        println!("{}", format!("{}-Capable Devices:", framework.name()).bright_cyan().bold());
        println!();

        let ready: Vec<_> = devices.iter().filter(|d| d.is_ready).collect();
        let needs_action: Vec<_> = devices.iter().filter(|d| !d.is_ready).collect();

        if !ready.is_empty() {
            let ready_label = if framework.requires_vfio() {
                "Ready (VFIO mode):"
            } else {
                "Ready (kernel mode):"
            };
            println!("{}", ready_label.bright_green());
            for dev in &ready {
                print_device_line(&dev.device, &dev.reference_string);
            }
            println!();
        }

        if !needs_action.is_empty() {
            let needs_label = if framework.requires_vfio() {
                "Needs Binding (kernel mode):"
            } else {
                "Needs Unbinding (VFIO mode):"
            };
            println!("{}", needs_label.bright_yellow());
            for dev in &needs_action {
                print_device_line(&dev.device, &dev.reference_string);
            }
            println!();
        }

        // Summary
        let ready_count = ready.len();
        let needs_count = needs_action.len();
        if needs_count > 0 {
            let action = if framework.requires_vfio() { "bind" } else { "unbind" };
            println!(
                "{} ready, {} need: vfio-tool {} <interface>",
                ready_count.to_string().bright_green(),
                needs_count.to_string().bright_yellow(),
                action
            );
        } else {
            println!("{}", format!("{} devices ready for {}", ready_count, framework.name()).bright_green());
        }
    } else {
        // Show only ready devices
        if devices.is_empty() {
            println!("{}", format!("No devices ready for {} (use --capable to see all capable devices)", framework.name()).bright_yellow());
            return Ok(());
        }

        println!("{}", format!("{}-Ready Devices:", framework.name()).bright_cyan().bold());
        for dev in devices {
            print_device_line(&dev.device, &dev.reference_string);
        }
        println!();
        println!("{}", format!("{} device(s) ready for {}", devices.len(), framework.name()).bright_green());
    }

    Ok(())
}

/// Print a single device line in human-readable format
fn print_device_line(device: &NetworkDevice, reference: &str) {
    let driver = device.driver.as_deref().unwrap_or("(none)");
    let desc = get_device_description(device);

    println!(
        "  {:15} → {:15}  ({:12}) {}",
        device.interface,
        reference,
        driver,
        desc
    );
}

/// Get a human-readable device description
fn get_device_description(device: &NetworkDevice) -> String {
    // Try to identify vendor/model from vendor:device ID
    match (device.vendor_id.as_str(), device.device_id.as_str()) {
        // Mellanox
        ("0x15b3", "0x101f") => "Mellanox ConnectX-4 Lx".to_string(),
        ("0x15b3", "0x1013") => "Mellanox ConnectX-4".to_string(),
        ("0x15b3", "0x1015") => "Mellanox ConnectX-4".to_string(),
        ("0x15b3", "0x1017") => "Mellanox ConnectX-5".to_string(),

        // Intel XXV710 - 25GbE
        ("0x8086", "0x158a") => "Intel XXV710 25GbE".to_string(),
        ("0x8086", "0x158b") => "Intel XXV710 25GbE".to_string(),

        // Intel X710 - 10GbE
        ("0x8086", "0x1572") => "Intel X710 10GbE".to_string(),
        ("0x8086", "0x15ff") => "Intel X710 10GbE".to_string(),

        // Solarflare
        ("0x1924", _) => "Solarflare NIC".to_string(),

        _ => {
            if let Some(ref speed) = device.max_speed {
                format!("{} NIC", speed)
            } else {
                "Network Card".to_string()
            }
        }
    }
}

/// Show framework devices in JSON format
fn show_framework_json(
    framework: Framework,
    devices: &[FrameworkDevice],
    show_capable: bool,
) -> Result<()> {
    let device_list: Vec<_> = devices
        .iter()
        .map(|d| {
            json!({
                "interface": d.device.interface,
                "reference": d.reference_string,
                "pci_address": d.device.pci_address,
                "driver": d.device.driver,
                "vendor": d.device.vendor_device(),
                "ready": d.is_ready,
                "max_speed": d.device.max_speed,
            })
        })
        .collect();

    let output = if show_capable {
        json!({
            "framework": framework.name(),
            "capable": device_list,
        })
    } else {
        json!({
            "framework": framework.name(),
            "available": device_list,
        })
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Show framework devices in args format (comma-separated)
fn show_framework_args(_framework: Framework, devices: &[FrameworkDevice]) -> Result<()> {
    let refs: Vec<_> = devices.iter().map(|d| d.reference_string.as_str()).collect();
    println!("{}", refs.join(","));
    Ok(())
}
