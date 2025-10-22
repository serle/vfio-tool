use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};

#[derive(Debug, Clone)]
pub struct NetworkDevice {
    pub interface: String,
    pub pci_address: String,
    pub driver: Option<String>,
    pub iommu_group: Option<u32>,
    pub vendor_id: String,
    pub device_id: String,
    pub speed: Option<String>,
    pub max_speed: Option<String>,
    pub status: DeviceStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceStatus {
    Kernel,      // Bound to kernel driver
    Vfio,        // Bound to vfio-pci
    Unbound,     // No driver bound
}

impl NetworkDevice {
    pub fn vendor_device(&self) -> String {
        format!("{}:{}", self.vendor_id, self.device_id)
    }

    pub fn is_vfio_bound(&self) -> bool {
        self.status == DeviceStatus::Vfio
    }
}

/// List all network devices on the system
pub fn list_network_devices() -> Result<Vec<NetworkDevice>> {
    let mut devices = Vec::new();
    let mut seen_pci_addresses = std::collections::HashSet::new();

    // Load config to get interface name mappings
    let config = crate::config::load_config().ok();

    // Scan ALL PCI devices to find network controllers
    let pci_devices_path = Path::new("/sys/bus/pci/devices");
    if !pci_devices_path.exists() {
        return Ok(devices);
    }

    for entry in fs::read_dir(pci_devices_path)? {
        let entry = entry?;
        let pci_address = entry.file_name().to_string_lossy().to_string();

        // Only check PCI devices (format: 0000:XX:XX.X)
        if !pci_address.contains(':') || !pci_address.contains('.') {
            continue;
        }

        // Check if it's a network device (PCI class 0x020000 = network controller)
        let class_path = entry.path().join("class");
        if let Ok(class_str) = fs::read_to_string(&class_path) {
            let class_code = class_str.trim();
            // Network controllers have class code 0x02xxxx
            if !class_code.starts_with("0x02") {
                continue;
            }
        } else {
            continue;
        }

        // Found a network device - get its details
        if let Ok(device) = get_device_info_by_pci(&pci_address, &config) {
            // Skip if we already found this device (shouldn't happen, but be safe)
            if !seen_pci_addresses.contains(&pci_address) {
                seen_pci_addresses.insert(pci_address.clone());
                devices.push(device);
            }
        }
    }

    devices.sort_by(|a, b| a.interface.cmp(&b.interface));
    Ok(devices)
}

/// Get device info by PCI address (handles kernel, VFIO, and unbound states)
fn get_device_info_by_pci(pci_address: &str, config: &Option<crate::config::Config>) -> Result<NetworkDevice> {
    // Get vendor and device IDs
    let (vendor_id, device_id) = get_vendor_device_id(pci_address)?;

    // Get IOMMU group
    let iommu_group = get_iommu_group(pci_address);

    // Get maximum capable speed based on device ID
    let max_speed = get_max_speed(&vendor_id, &device_id);

    // Get driver
    let driver = get_driver(pci_address);

    // Determine status
    let status = match &driver {
        Some(d) if d == "vfio-pci" => DeviceStatus::Vfio,
        Some(_) => DeviceStatus::Kernel,
        None => DeviceStatus::Unbound,
    };

    // Try to get interface name
    let interface = if status == DeviceStatus::Kernel {
        // Device has kernel driver - check for interface in /sys/bus/pci/devices/{pci}/net/
        let net_dir = format!("/sys/bus/pci/devices/{}/net", pci_address);
        if let Ok(entries) = fs::read_dir(&net_dir) {
            // Get first interface name
            if let Some(Ok(entry)) = entries.into_iter().next() {
                entry.file_name().to_string_lossy().to_string()
            } else {
                format!("({})", pci_address)
            }
        } else {
            format!("({})", pci_address)
        }
    } else if status == DeviceStatus::Vfio {
        // Device bound to VFIO - try to get name from config
        if let Some(cfg) = config {
            cfg.devices.pci_mappings
                .iter()
                .find(|(_, pci)| pci.as_str() == pci_address)
                .map(|(iface, _)| iface.clone())
                .unwrap_or_else(|| format!("({})", pci_address))
        } else {
            format!("({})", pci_address)
        }
    } else {
        // Device unbound - try to get name from config
        if let Some(cfg) = config {
            cfg.devices.pci_mappings
                .iter()
                .find(|(_, pci)| pci.as_str() == pci_address)
                .map(|(iface, _)| iface.clone())
                .unwrap_or_else(|| format!("({})", pci_address))
        } else {
            format!("({})", pci_address)
        }
    };

    // Get link speed (only works for kernel mode)
    let speed = if status == DeviceStatus::Kernel {
        let base_path = format!("/sys/bus/pci/devices/{}/net/{}", pci_address, &interface);
        get_link_speed(&PathBuf::from(&base_path))
    } else {
        None
    };

    Ok(NetworkDevice {
        interface,
        pci_address: pci_address.to_string(),
        driver,
        iommu_group,
        vendor_id,
        device_id,
        speed,
        max_speed,
        status,
    })
}

/// List network devices bound to vfio-pci
fn list_vfio_network_devices() -> Result<Vec<NetworkDevice>> {
    let vfio_driver_path = Path::new("/sys/bus/pci/drivers/vfio-pci");
    let mut devices = Vec::new();

    if !vfio_driver_path.exists() {
        return Ok(devices);
    }

    // Load config to get interface name mappings
    let config = crate::config::load_config().ok();

    for entry in fs::read_dir(vfio_driver_path)? {
        let entry = entry?;
        let name = entry.file_name();
        let pci_address = name.to_string_lossy().to_string();

        // Only check PCI devices (format: 0000:XX:XX.X)
        if !pci_address.contains(':') || !pci_address.contains('.') {
            continue;
        }

        // Check if it's a network device (PCI class 0x020000 = network controller)
        let class_path = format!("/sys/bus/pci/devices/{}/class", pci_address);
        if let Ok(class_str) = fs::read_to_string(&class_path) {
            let class_code = class_str.trim();
            // Network controllers have class code 0x02xxxx
            if !class_code.starts_with("0x02") {
                continue;
            }
        } else {
            continue;
        }

        // Try to get interface name from config mappings
        let interface = if let Some(ref cfg) = config {
            // Search for this PCI address in mappings
            cfg.devices.pci_mappings
                .iter()
                .find(|(_, pci)| pci.as_str() == pci_address)
                .map(|(iface, _)| iface.clone())
                .unwrap_or_else(|| format!("({})", pci_address))
        } else {
            format!("({})", pci_address)
        };

        // Get device info by PCI address
        if let Ok(device) = get_vfio_device_info(&pci_address, &interface) {
            devices.push(device);
        }
    }

    Ok(devices)
}

/// Get device info for a VFIO-bound device by PCI address
fn get_vfio_device_info(pci_address: &str, interface: &str) -> Result<NetworkDevice> {
    // Get vendor and device IDs
    let (vendor_id, device_id) = get_vendor_device_id(pci_address)?;

    // Get IOMMU group
    let iommu_group = get_iommu_group(pci_address);

    // Get maximum capable speed based on device ID
    let max_speed = get_max_speed(&vendor_id, &device_id);

    Ok(NetworkDevice {
        interface: interface.to_string(),
        pci_address: pci_address.to_string(),
        driver: Some("vfio-pci".to_string()),
        iommu_group,
        vendor_id,
        device_id,
        speed: None,  // No link speed available when bound to VFIO
        max_speed,
        status: DeviceStatus::Vfio,
    })
}

/// Get detailed information about a specific network interface
pub fn get_device_info(interface: &str) -> Result<NetworkDevice> {
    let base_path = PathBuf::from(format!("/sys/class/net/{}", interface));

    if !base_path.exists() {
        anyhow::bail!("Interface {} not found", interface);
    }

    let device_path = base_path.join("device");
    if !device_path.exists() {
        anyhow::bail!("Interface {} is not a physical device", interface);
    }

    // Get PCI address
    let pci_address = get_pci_address(&device_path)?;

    // Get driver
    let driver = get_driver(&pci_address);

    // Get IOMMU group
    let iommu_group = get_iommu_group(&pci_address);

    // Get vendor and device IDs
    let (vendor_id, device_id) = get_vendor_device_id(&pci_address)?;

    // Get link speed
    let speed = get_link_speed(&base_path);

    // Get maximum capable speed based on device ID
    let max_speed = get_max_speed(&vendor_id, &device_id);

    // Determine status
    let status = match &driver {
        Some(d) if d == "vfio-pci" => DeviceStatus::Vfio,
        Some(_) => DeviceStatus::Kernel,
        None => DeviceStatus::Unbound,
    };

    Ok(NetworkDevice {
        interface: interface.to_string(),
        pci_address,
        driver,
        iommu_group,
        vendor_id,
        device_id,
        speed,
        max_speed,
        status,
    })
}

fn get_pci_address(device_path: &Path) -> Result<String> {
    let target = fs::read_link(device_path)
        .context("Failed to read device symlink")?;

    target
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .context("Invalid PCI address")
}

fn get_driver(pci_address: &str) -> Option<String> {
    let driver_path = PathBuf::from(format!("/sys/bus/pci/devices/{}/driver", pci_address));

    if !driver_path.exists() {
        return None;
    }

    fs::read_link(&driver_path)
        .ok()
        .and_then(|target| {
            target
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
}

fn get_iommu_group(pci_address: &str) -> Option<u32> {
    let iommu_path = PathBuf::from(format!("/sys/bus/pci/devices/{}/iommu_group", pci_address));

    if !iommu_path.exists() {
        return None;
    }

    fs::read_link(&iommu_path)
        .ok()
        .and_then(|target| {
            target
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.parse::<u32>().ok())
        })
}

fn get_vendor_device_id(pci_address: &str) -> Result<(String, String)> {
    let base = PathBuf::from(format!("/sys/bus/pci/devices/{}", pci_address));

    let vendor = fs::read_to_string(base.join("vendor"))
        .context("Failed to read vendor ID")?
        .trim()
        .to_string();

    let device = fs::read_to_string(base.join("device"))
        .context("Failed to read device ID")?
        .trim()
        .to_string();

    Ok((vendor, device))
}

fn get_link_speed(interface_path: &Path) -> Option<String> {
    let speed_path = interface_path.join("speed");

    match fs::read_to_string(speed_path) {
        Ok(s) => {
            match s.trim().parse::<i32>() {
                Ok(speed) => {
                    if speed < 0 {
                        // -1 typically means no link (no carrier)
                        Some("no link".to_string())
                    } else if speed >= 1000 {
                        Some(format!("{}G", speed / 1000))
                    } else {
                        Some(format!("{}M", speed))
                    }
                }
                Err(_) => Some("?".to_string()),
            }
        }
        Err(e) => {
            // Check if it's "Invalid argument" which means interface is down
            if e.kind() == std::io::ErrorKind::InvalidInput {
                Some("down".to_string())
            } else {
                // Other errors (permission denied, etc.)
                Some("?".to_string())
            }
        }
    }
}

/// Get maximum capable speed based on vendor:device ID
fn get_max_speed(vendor_id: &str, device_id: &str) -> Option<String> {
    // Common network card vendor:device ID mappings
    match (vendor_id, device_id) {
        // Intel XXV710 - 25GbE
        ("0x8086", "0x158a") => Some("25G".to_string()),
        ("0x8086", "0x158b") => Some("25G".to_string()),

        // Intel X710 - 10GbE
        ("0x8086", "0x1572") => Some("10G".to_string()),
        ("0x8086", "0x1580") => Some("10G".to_string()),
        ("0x8086", "0x1581") => Some("10G".to_string()),
        ("0x8086", "0x1585") => Some("10G".to_string()),
        ("0x8086", "0x1586") => Some("10G".to_string()),
        ("0x8086", "0x1589") => Some("10G".to_string()),

        // Intel XL710 - 40GbE
        ("0x8086", "0x1583") => Some("40G".to_string()),
        ("0x8086", "0x1584") => Some("40G".to_string()),

        // Intel E810 - 100GbE
        ("0x8086", "0x1591") => Some("100G".to_string()),
        ("0x8086", "0x1592") => Some("100G".to_string()),
        ("0x8086", "0x1593") => Some("100G".to_string()),

        // Intel 82599 - 10GbE
        ("0x8086", "0x10fb") => Some("10G".to_string()),
        ("0x8086", "0x10fc") => Some("10G".to_string()),

        // Intel X540/X550 - 10GbE
        ("0x8086", "0x1528") => Some("10G".to_string()),
        ("0x8086", "0x1563") => Some("10G".to_string()),
        ("0x8086", "0x15ac") => Some("10G".to_string()),
        ("0x8086", "0x15ad") => Some("10G".to_string()),

        // Intel I350 - 1GbE
        ("0x8086", "0x1521") => Some("1G".to_string()),
        ("0x8086", "0x1522") => Some("1G".to_string()),
        ("0x8086", "0x1523") => Some("1G".to_string()),
        ("0x8086", "0x1524") => Some("1G".to_string()),

        // Intel 82576 - 1GbE
        ("0x8086", "0x10c9") => Some("1G".to_string()),
        ("0x8086", "0x10e6") => Some("1G".to_string()),
        ("0x8086", "0x10e7") => Some("1G".to_string()),
        ("0x8086", "0x10e8") => Some("1G".to_string()),

        // Intel 82580 - 1GbE
        ("0x8086", "0x150e") => Some("1G".to_string()),
        ("0x8086", "0x150f") => Some("1G".to_string()),
        ("0x8086", "0x1510") => Some("1G".to_string()),
        ("0x8086", "0x1511") => Some("1G".to_string()),

        // Intel E810 - 25GbE variants
        ("0x8086", "0x159b") => Some("25G".to_string()),

        // Intel X710 for 10GBASE-T
        ("0x8086", "0x15ff") => Some("10G".to_string()),

        // Mellanox ConnectX-3 - 10/40GbE
        ("0x15b3", "0x1003") => Some("40G".to_string()),
        ("0x15b3", "0x1007") => Some("40G".to_string()),

        // Mellanox ConnectX-4 - 10/25/40/50/100GbE (varies by SKU)
        ("0x15b3", "0x1013") => Some("100G".to_string()),
        ("0x15b3", "0x1014") => Some("100G".to_string()),
        ("0x15b3", "0x1015") => Some("100G".to_string()),
        ("0x15b3", "0x1016") => Some("100G".to_string()),
        ("0x15b3", "0x1017") => Some("50G".to_string()),
        ("0x15b3", "0x1018") => Some("100G".to_string()),
        ("0x15b3", "0x1019") => Some("40G".to_string()),
        ("0x15b3", "0x101a") => Some("40G".to_string()),
        ("0x15b3", "0x101b") => Some("40G".to_string()),
        ("0x15b3", "0x101c") => Some("40G".to_string()),
        ("0x15b3", "0x101d") => Some("25G".to_string()),
        ("0x15b3", "0x101e") => Some("100G".to_string()),
        ("0x15b3", "0x101f") => Some("25G".to_string()), // User's ConnectX-4 Lx

        // Broadcom - 10/25/40/50/100GbE
        ("0x14e4", "0x16d7") => Some("25G".to_string()),
        ("0x14e4", "0x16d8") => Some("25G".to_string()),
        ("0x14e4", "0x16dc") => Some("100G".to_string()),
        ("0x14e4", "0x16e1") => Some("50G".to_string()),
        ("0x14e4", "0x16e2") => Some("50G".to_string()),
        ("0x14e4", "0x16e3") => Some("50G".to_string()),

        // Chelsio - 10/25/40/100GbE
        ("0x1425", "0x5400") => Some("10G".to_string()),
        ("0x1425", "0x5401") => Some("10G".to_string()),
        ("0x1425", "0x5410") => Some("40G".to_string()),
        ("0x1425", "0x5411") => Some("40G".to_string()),
        ("0x1425", "0x5680") => Some("100G".to_string()),
        ("0x1425", "0x5681") => Some("100G".to_string()),

        // Solarflare - 10/40GbE
        ("0x1924", "0x0803") => Some("10G".to_string()),
        ("0x1924", "0x0813") => Some("10G".to_string()),
        ("0x1924", "0x0903") => Some("40G".to_string()),

        // QLogic/Marvell - 10/25/40/100GbE
        ("0x1077", "0x8070") => Some("10G".to_string()),
        ("0x1077", "0x8090") => Some("40G".to_string()),

        // Default - unknown
        _ => None,
    }
}

/// Get all devices in an IOMMU group
pub fn get_iommu_group_devices(group_id: u32) -> Result<Vec<String>> {
    let group_path = PathBuf::from(format!("/sys/kernel/iommu_groups/{}/devices", group_id));

    if !group_path.exists() {
        anyhow::bail!("IOMMU group {} not found", group_id);
    }

    let mut devices = Vec::new();

    for entry in fs::read_dir(&group_path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        devices.push(name);
    }

    Ok(devices)
}
