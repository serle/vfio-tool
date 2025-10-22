use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::device::{NetworkDevice, DeviceStatus};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Framework {
    Dpdk,
    Rdma,
    TcpDirect,
    OpenOnload,
    EfVi,
    Spdk,
    Vpp,
    Xdp,
}

impl Framework {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dpdk" => Some(Framework::Dpdk),
            "rdma" => Some(Framework::Rdma),
            "tcpdirect" => Some(Framework::TcpDirect),
            "openonload" => Some(Framework::OpenOnload),
            "efvi" => Some(Framework::EfVi),
            "spdk" => Some(Framework::Spdk),
            "vpp" => Some(Framework::Vpp),
            "xdp" => Some(Framework::Xdp),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Framework::Dpdk => "DPDK",
            Framework::Rdma => "RDMA",
            Framework::TcpDirect => "TCPDirect",
            Framework::OpenOnload => "OpenOnload",
            Framework::EfVi => "ef_vi",
            Framework::Spdk => "SPDK",
            Framework::Vpp => "VPP",
            Framework::Xdp => "XDP",
        }
    }

    pub fn requires_vfio(&self) -> bool {
        matches!(self, Framework::Dpdk | Framework::TcpDirect | Framework::Spdk | Framework::Vpp)
    }

    pub fn requires_kernel(&self) -> bool {
        matches!(self, Framework::Rdma | Framework::OpenOnload | Framework::EfVi | Framework::Xdp)
    }
}

#[derive(Debug, Clone)]
pub struct FrameworkDevice {
    pub device: NetworkDevice,
    pub is_ready: bool,
    pub reference_string: String, // What the app would use (PCI address, RDMA name, interface name)
}

/// Check if device is capable of supporting the framework
pub fn is_device_capable(device: &NetworkDevice, framework: Framework) -> bool {
    match framework {
        // DPDK: All NICs are capable
        Framework::Dpdk => true,

        // RDMA: Only Mellanox and Broadcom with RoCE
        Framework::Rdma => is_rdma_capable(device),

        // Solarflare frameworks: Only Solarflare NICs
        Framework::TcpDirect | Framework::OpenOnload | Framework::EfVi => is_solarflare(device),

        // SPDK: All NICs are capable
        Framework::Spdk => true,

        // VPP: All NICs are capable (uses DPDK underneath)
        Framework::Vpp => true,

        // XDP: Check if driver supports XDP
        Framework::Xdp => is_xdp_capable(device),
    }
}

/// Check if device is ready to use with the framework RIGHT NOW
pub fn is_device_ready(device: &NetworkDevice, framework: Framework) -> bool {
    if !is_device_capable(device, framework) {
        return false;
    }

    if framework.requires_vfio() {
        // Must be in VFIO mode
        device.status == DeviceStatus::Vfio
    } else if framework.requires_kernel() {
        // Must be in kernel mode
        device.status == DeviceStatus::Kernel
    } else {
        false
    }
}

/// Get the reference string that applications would use
pub fn get_reference_string(device: &NetworkDevice, framework: Framework) -> Result<String> {
    match framework {
        // DPDK, SPDK, VPP, TCPDirect: Use PCI addresses
        Framework::Dpdk | Framework::Spdk | Framework::Vpp | Framework::TcpDirect => {
            Ok(device.pci_address.clone())
        }

        // RDMA: Use RDMA device name (mlx5_0, mlx5_1, etc.)
        Framework::Rdma => {
            get_rdma_device_name(&device.pci_address)
        }

        // OpenOnload, ef_vi, XDP: Use interface name
        Framework::OpenOnload | Framework::EfVi | Framework::Xdp => {
            Ok(device.interface.clone())
        }
    }
}

/// Check if device is RDMA-capable (Mellanox or Broadcom with RoCE)
fn is_rdma_capable(device: &NetworkDevice) -> bool {
    match device.vendor_id.as_str() {
        // Mellanox (all modern cards support RDMA)
        "0x15b3" => true,

        // Broadcom (many support RoCE)
        "0x14e4" => {
            // Common Broadcom NICs with RoCE support
            matches!(device.device_id.as_str(),
                "0x16d7" | "0x16d8" | "0x16dc" | "0x16e1" | "0x16e2" | "0x16e3"
            )
        }

        _ => false,
    }
}

/// Check if device is Solarflare
fn is_solarflare(device: &NetworkDevice) -> bool {
    device.vendor_id == "0x1924"
}

/// Check if device supports XDP
fn is_xdp_capable(device: &NetworkDevice) -> bool {
    // XDP support depends on the driver
    // Most modern drivers support XDP, but not all
    if let Some(ref driver) = device.driver {
        matches!(driver.as_str(),
            "i40e" | "ice" | "ixgbe" | "ixgbevf" |
            "mlx5_core" | "mlx4_core" |
            "virtio_net" | "veth" |
            "tun" | "tap" |
            "nfp" | "qede" | "bnxt_en" |
            "thunderx" | "ena"
        )
    } else {
        false
    }
}

/// Get RDMA device name from PCI address
fn get_rdma_device_name(pci_address: &str) -> Result<String> {
    // RDMA devices are listed in /sys/class/infiniband/
    let infiniband_path = Path::new("/sys/class/infiniband");

    if !infiniband_path.exists() {
        anyhow::bail!("RDMA subsystem not available (no /sys/class/infiniband)");
    }

    // Iterate through RDMA devices
    for entry in fs::read_dir(infiniband_path)? {
        let entry = entry?;
        let rdma_name = entry.file_name().to_string_lossy().to_string();

        // Get the device symlink to find PCI address
        let device_path = entry.path().join("device");
        if let Ok(target) = fs::read_link(&device_path) {
            if let Some(dev_name) = target.file_name() {
                if dev_name.to_string_lossy() == pci_address {
                    return Ok(rdma_name);
                }
            }
        }
    }

    anyhow::bail!("No RDMA device found for PCI address {}", pci_address)
}

/// Get all capable devices for a framework
pub fn get_capable_devices(framework: Framework) -> Result<Vec<FrameworkDevice>> {
    let all_devices = crate::device::list_network_devices()?;
    let mut result = Vec::new();

    for device in all_devices {
        if is_device_capable(&device, framework) {
            let is_ready = is_device_ready(&device, framework);
            let reference_string = if is_ready {
                get_reference_string(&device, framework).unwrap_or_else(|_| device.pci_address.clone())
            } else {
                // For devices not ready, still try to get reference string if possible
                device.pci_address.clone()
            };

            result.push(FrameworkDevice {
                device,
                is_ready,
                reference_string,
            });
        }
    }

    Ok(result)
}

/// Get only ready (available) devices for a framework
pub fn get_available_devices(framework: Framework) -> Result<Vec<FrameworkDevice>> {
    let capable = get_capable_devices(framework)?;
    Ok(capable.into_iter().filter(|d| d.is_ready).collect())
}
