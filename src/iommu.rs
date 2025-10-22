use std::fs;
use std::path::Path;
use anyhow::Result;
use colored::Colorize;

use crate::grub;

#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub iommu_enabled: bool,
    pub vfio_module_loaded: bool,
    pub iommu_groups_count: usize,
    pub vfio_devices_count: usize,
    pub cpu_vendor: grub::CpuVendor,
}

#[derive(Debug, Clone)]
pub enum SystemIssue {
    IommuNotEnabled,
    VfioModuleNotLoaded,
    NoIommuGroups,
}

impl SystemIssue {
    pub fn description(&self) -> &str {
        match self {
            SystemIssue::IommuNotEnabled => "IOMMU is not enabled in kernel parameters",
            SystemIssue::VfioModuleNotLoaded => "VFIO kernel module is not loaded",
            SystemIssue::NoIommuGroups => "No IOMMU groups found",
        }
    }

    pub fn fix_command(&self) -> &str {
        match self {
            SystemIssue::IommuNotEnabled => "vfio-tool setup-grub",
            SystemIssue::VfioModuleNotLoaded => "sudo modprobe vfio-pci",
            SystemIssue::NoIommuGroups => "Enable IOMMU in BIOS/UEFI (VT-d for Intel, AMD-Vi for AMD)",
        }
    }

    pub fn fix(&self) -> Result<()> {
        match self {
            SystemIssue::IommuNotEnabled => {
                println!("{}", "Fixing: IOMMU not enabled".bright_yellow());
                println!("This requires GRUB configuration and reboot.");
                println!("Run: sudo vfio-tool setup-grub");
                anyhow::bail!("Manual intervention required");
            }
            SystemIssue::VfioModuleNotLoaded => {
                println!("{}", "Loading VFIO module...".bright_cyan());
                std::process::Command::new("modprobe")
                    .arg("vfio-pci")
                    .status()?;
                println!("{}", "✓ VFIO module loaded".bright_green());
                Ok(())
            }
            SystemIssue::NoIommuGroups => {
                println!("{}", "Cannot automatically fix: No IOMMU groups".bright_red());
                println!("You must:");
                println!("  1. Enable VT-d (Intel) or AMD-Vi (AMD) in BIOS/UEFI");
                println!("  2. Reboot");
                println!("  3. Run: vfio-tool setup-grub");
                println!("  4. Reboot again");
                anyhow::bail!("Manual intervention required");
            }
        }
    }
}

/// Get overall system status
pub fn get_system_status() -> Result<SystemStatus> {
    let iommu_enabled = grub::is_iommu_enabled()?;
    let vfio_module_loaded = is_vfio_module_loaded();
    let iommu_groups_count = count_iommu_groups();
    let vfio_devices_count = count_vfio_devices();
    let cpu_vendor = grub::detect_cpu_vendor();

    Ok(SystemStatus {
        iommu_enabled,
        vfio_module_loaded,
        iommu_groups_count,
        vfio_devices_count,
        cpu_vendor,
    })
}

/// Check system for issues
pub fn check_system() -> Result<Vec<SystemIssue>> {
    let mut issues = Vec::new();

    // Check IOMMU
    if !grub::is_iommu_enabled()? {
        issues.push(SystemIssue::IommuNotEnabled);
    }

    // Check VFIO module
    if !is_vfio_module_loaded() {
        issues.push(SystemIssue::VfioModuleNotLoaded);
    }

    // Check IOMMU groups
    if count_iommu_groups() == 0 {
        issues.push(SystemIssue::NoIommuGroups);
    }

    Ok(issues)
}

/// Check if VFIO module is loaded
fn is_vfio_module_loaded() -> bool {
    if let Ok(modules) = fs::read_to_string("/proc/modules") {
        return modules.lines().any(|line| line.starts_with("vfio_pci"));
    }
    false
}

/// Count IOMMU groups
fn count_iommu_groups() -> usize {
    let groups_dir = Path::new("/sys/kernel/iommu_groups");

    if !groups_dir.exists() {
        return 0;
    }

    fs::read_dir(groups_dir)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// Count devices bound to VFIO
fn count_vfio_devices() -> usize {
    let vfio_dir = Path::new("/sys/bus/pci/drivers/vfio-pci");

    if !vfio_dir.exists() {
        return 0;
    }

    fs::read_dir(vfio_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy();
                    // Count only PCI addresses (format: 0000:XX:XX.X)
                    name_str.contains(':') && name_str.contains('.')
                })
                .count()
        })
        .unwrap_or(0)
}

/// Check if /dev/vfio/vfio exists
#[allow(dead_code)]
pub fn is_vfio_available() -> bool {
    Path::new("/dev/vfio/vfio").exists()
}
