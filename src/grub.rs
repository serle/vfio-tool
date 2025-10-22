use std::fs;
use std::process::Command;
use anyhow::{Result, Context};
use colored::Colorize;
use dialoguer::Confirm;

const GRUB_DEFAULT: &str = "/etc/default/grub";
const GRUB_BACKUP: &str = "/etc/default/grub.vfio-tool.backup";

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum CpuVendor {
    Intel,
    AMD,
    Unknown,
}

/// Detect CPU vendor
pub fn detect_cpu_vendor() -> CpuVendor {
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        if cpuinfo.contains("GenuineIntel") {
            return CpuVendor::Intel;
        } else if cpuinfo.contains("AuthenticAMD") {
            return CpuVendor::AMD;
        }
    }
    CpuVendor::Unknown
}

/// Check if IOMMU is enabled in current kernel parameters
pub fn is_iommu_enabled() -> Result<bool> {
    let cmdline = fs::read_to_string("/proc/cmdline")
        .context("Failed to read /proc/cmdline")?;

    let has_iommu = cmdline.contains("intel_iommu=on") || cmdline.contains("amd_iommu=on");
    let has_passthrough = cmdline.contains("iommu=pt");

    Ok(has_iommu && has_passthrough)
}

/// Get required IOMMU parameters for current CPU
pub fn get_required_iommu_params() -> Result<Vec<String>> {
    let vendor = detect_cpu_vendor();

    match vendor {
        CpuVendor::Intel => Ok(vec!["intel_iommu=on".to_string(), "iommu=pt".to_string()]),
        CpuVendor::AMD => Ok(vec!["amd_iommu=on".to_string(), "iommu=pt".to_string()]),
        CpuVendor::Unknown => {
            anyhow::bail!("Unknown CPU vendor. Cannot determine IOMMU parameters.");
        }
    }
}

/// Setup IOMMU in GRUB configuration
pub fn setup_iommu(skip_confirm: bool) -> Result<()> {
    // Check if already enabled
    if is_iommu_enabled()? {
        println!("{}", "✓ IOMMU is already enabled".bright_green());
        return Ok(());
    }

    println!("{}", "IOMMU is not enabled in kernel parameters".bright_yellow());
    println!();

    // Detect CPU
    let vendor = detect_cpu_vendor();
    let vendor_str = match vendor {
        CpuVendor::Intel => "Intel",
        CpuVendor::AMD => "AMD",
        CpuVendor::Unknown => "Unknown",
    };

    println!("Detected CPU: {}", vendor_str.bright_cyan());

    let params = get_required_iommu_params()?;
    println!("Required parameters: {}", params.join(" ").bright_cyan());
    println!();

    // Read current GRUB config
    let grub_content = fs::read_to_string(GRUB_DEFAULT)
        .context("Failed to read /etc/default/grub. Are you running as root?")?;

    // Check if already has the parameters
    if params.iter().all(|p| grub_content.contains(p)) {
        println!("{}", "✓ GRUB already has IOMMU parameters".bright_green());
        println!("{}", "  You may need to reboot for changes to take effect.".bright_yellow());
        return Ok(());
    }

    println!("{}", "This will:".bright_cyan());
    println!("  1. Backup current GRUB config to {}", GRUB_BACKUP);
    println!("  2. Add IOMMU parameters to GRUB_CMDLINE_LINUX_DEFAULT");
    println!("  3. Run update-grub to regenerate boot configuration");
    println!("  4. Require a reboot to take effect");
    println!();

    if !skip_confirm {
        let proceed = Confirm::new()
            .with_prompt("Proceed with GRUB configuration?")
            .default(false)
            .interact()?;

        if !proceed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Backup current config
    println!("\n{}", "Creating backup...".bright_cyan());
    fs::copy(GRUB_DEFAULT, GRUB_BACKUP)
        .context("Failed to backup GRUB config")?;
    println!("✓ Backup created: {}", GRUB_BACKUP);

    // Modify GRUB config
    println!("\n{}", "Updating GRUB configuration...".bright_cyan());
    let new_content = add_iommu_params(&grub_content, &params)?;

    fs::write(GRUB_DEFAULT, new_content)
        .context("Failed to write GRUB config")?;
    println!("✓ GRUB config updated");

    // Run update-grub
    println!("\n{}", "Running update-grub...".bright_cyan());
    let output = Command::new("update-grub")
        .output()
        .context("Failed to run update-grub")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("update-grub failed: {}", stderr);
    }

    println!("✓ GRUB boot configuration regenerated");

    println!("\n{}", "═══════════════════════════════════════════════".bright_green());
    println!("{}", "✓ GRUB configuration complete!".bright_green());
    println!("{}", "═══════════════════════════════════════════════".bright_green());
    println!();
    println!("{}", "IMPORTANT: You MUST reboot for changes to take effect.".bright_yellow().bold());
    println!();
    println!("After reboot, run:");
    println!("  {} to verify IOMMU is enabled", "vfio-tool check".bright_cyan());
    println!();

    Ok(())
}

/// Add IOMMU parameters to GRUB config
fn add_iommu_params(grub_content: &str, params: &[String]) -> Result<String> {
    let mut lines: Vec<String> = grub_content.lines().map(String::from).collect();
    let params_str = params.join(" ");

    for line in &mut lines {
        // Find GRUB_CMDLINE_LINUX_DEFAULT line
        if line.trim_start().starts_with("GRUB_CMDLINE_LINUX_DEFAULT") {
            // Extract the quoted value
            if let Some(start_quote) = line.find('"') {
                if let Some(end_quote) = line.rfind('"') {
                    if start_quote < end_quote {
                        let current_params = &line[start_quote + 1..end_quote];

                        // Add new parameters
                        let new_params = if current_params.is_empty() {
                            params_str.clone()
                        } else {
                            format!("{} {}", current_params, params_str)
                        };

                        *line = format!(
                            "GRUB_CMDLINE_LINUX_DEFAULT=\"{}\"",
                            new_params
                        );

                        break;
                    }
                }
            }
        }
    }

    Ok(lines.join("\n") + "\n")
}

/// Check current GRUB configuration
#[allow(dead_code)]
pub fn check_grub_config() -> Result<()> {
    println!("{}", "Current GRUB configuration:".bright_cyan());
    println!();

    let grub_content = fs::read_to_string(GRUB_DEFAULT)
        .context("Failed to read /etc/default/grub")?;

    for line in grub_content.lines() {
        if line.trim_start().starts_with("GRUB_CMDLINE_LINUX") {
            println!("{}", line);
        }
    }

    println!();
    println!("{}", "Current kernel parameters:".bright_cyan());

    let cmdline = fs::read_to_string("/proc/cmdline")?;
    println!("{}", cmdline);

    Ok(())
}
