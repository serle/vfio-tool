use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::Colorize;
use nix::unistd::Uid;

use crate::{device, display, grub, iommu, vfio, config, systemd, frameworks};

/// Check if running as root (effective UID == 0)
fn is_root() -> bool {
    Uid::effective().is_root()
}

/// Require root privileges or exit with error
fn require_root(command: &str) {
    if !is_root() {
        eprintln!("{}", "Error: This command requires root privileges.".bright_red().bold());
        eprintln!();
        eprintln!("Run with sudo:");
        eprintln!("  {}", format!("sudo vfio-tool {}", command).bright_cyan());
        std::process::exit(4);
    }
}

/// Comprehensive VFIO management tool for kernel bypass
#[derive(Parser)]
#[command(name = "vfio-tool")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all network interfaces with their current state
    List {
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show system VFIO/IOMMU status
    Status,

    /// Show detailed information about a specific interface
    Info {
        /// Interface name (e.g., enp33s0f0np0)
        interface: String,
    },

    /// Check system readiness for VFIO
    Check {
        /// Automatically fix issues if possible
        #[arg(short, long)]
        fix: bool,
    },

    /// Bind interface(s) to VFIO immediately
    Bind {
        /// Comma-separated list of interfaces
        interfaces: String,
    },

    /// Unbind interface(s) from VFIO (return to kernel)
    Unbind {
        /// Comma-separated list of interfaces
        interfaces: String,
    },

    /// Reset all VFIO bindings (unbind all)
    Reset,

    /// Interactive configuration wizard
    Configure,

    /// Update configuration when hardware changes
    Update,

    /// Save configuration for specified interfaces
    Save {
        /// Comma-separated list of interfaces for VFIO
        #[arg(long)]
        vfio: Option<String>,

        /// Comma-separated list of interfaces for kernel
        #[arg(long)]
        kernel: Option<String>,
    },

    /// Apply saved configuration
    Apply,

    /// Show current configuration
    ShowConfig,

    /// Install systemd service for persistence
    Install,

    /// Uninstall systemd service
    Uninstall,

    /// Generate standalone bash script
    GenerateScript {
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Explain what would happen to an interface
    Explain {
        /// Interface name
        interface: String,
    },

    /// Validate configuration file
    Validate,

    /// Check if specific interfaces exist and are in the correct mode
    CheckInterfaces {
        /// Comma-separated list of interfaces that must be in VFIO mode
        #[arg(long)]
        vfio: Option<String>,

        /// Comma-separated list of interfaces that must be in kernel mode
        #[arg(long)]
        kernel: Option<String>,

        /// Comma-separated list of interfaces to check for existence only (deprecated, use --vfio or --kernel)
        interfaces: Option<String>,
    },

    /// Ensure specific interfaces are in VFIO mode (bind if needed)
    EnsureVfio {
        /// Comma-separated list of interfaces
        interfaces: String,
    },

    /// Setup GRUB for IOMMU support
    SetupGrub {
        /// Skip confirmation prompts
        #[arg(short, long)]
        yes: bool,
    },

    /// Show devices for specific framework (dpdk, rdma, tcpdirect, openonload, efvi, spdk, vpp, xdp)
    Show {
        /// Framework name
        framework: String,

        /// Show all capable devices (not just ready ones)
        #[arg(short, long)]
        capable: bool,

        /// Output format: json or args (comma-separated)
        #[arg(short, long)]
        format: Option<String>,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::List { verbose } => {
                let devices = device::list_network_devices()?;
                display::show_device_table(&devices, verbose)?;
            }

            Commands::Status => {
                let status = iommu::get_system_status()?;
                display::show_system_status(&status)?;
            }

            Commands::Info { interface } => {
                let device = device::get_device_info(&interface)?;
                display::show_device_details(&device)?;
            }

            Commands::Check { fix } => {
                if fix {
                    require_root("check --fix");
                }

                println!("{}", "Checking system readiness...".bright_cyan());
                let issues = iommu::check_system()?;

                if issues.is_empty() {
                    println!("{}", "✓ System is ready for VFIO!".bright_green());
                    return Ok(());
                }

                display::show_issues(&issues)?;

                if fix {
                    println!("\n{}", "Attempting to fix issues...".bright_yellow());
                    for issue in &issues {
                        issue.fix()?;
                    }
                    println!("{}", "✓ Issues fixed!".bright_green());
                } else {
                    println!("\n{}", "Run with --fix to automatically resolve issues.".bright_yellow());
                }
            }

            Commands::Bind { interfaces } => {
                require_root("bind");
                let ifaces: Vec<&str> = interfaces.split(',').collect();
                vfio::bind_interfaces(&ifaces)?;
            }

            Commands::Unbind { interfaces } => {
                require_root("unbind");
                let ifaces: Vec<&str> = interfaces.split(',').collect();
                vfio::unbind_interfaces(&ifaces)?;
            }

            Commands::Reset => {
                require_root("reset");
                vfio::unbind_all()?;
            }

            Commands::Configure => {
                require_root("configure");
                config::interactive_configure()?;
            }

            Commands::Update => {
                require_root("update");
                config::interactive_update()?;
            }

            Commands::Save { vfio: vfio_list, kernel } => {
                require_root("save");
                let vfio_ifaces = vfio_list
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default();
                let kernel_ifaces = kernel
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default();

                config::save_config(vfio_ifaces, kernel_ifaces)?;
            }

            Commands::Apply => {
                require_root("apply");
                let cfg = config::load_config()?;
                vfio::apply_config(&cfg)?;
            }

            Commands::ShowConfig => {
                let cfg = config::load_config()?;
                display::show_config(&cfg)?;
            }

            Commands::Install => {
                require_root("install");
                systemd::install_service()?;
            }

            Commands::Uninstall => {
                require_root("uninstall");
                systemd::uninstall_service()?;
            }

            Commands::GenerateScript { output } => {
                let cfg = config::load_config()?;
                let script = systemd::generate_bash_script(&cfg)?;

                if let Some(path) = output {
                    std::fs::write(&path, script)?;
                    println!("Script written to: {}", path);
                } else {
                    println!("{}", script);
                }
            }

            Commands::Explain { interface } => {
                let device = device::get_device_info(&interface)?;
                display::explain_device(&device)?;
            }

            Commands::Validate => {
                if let Err(e) = config::validate_config() {
                    eprintln!("{}", e);
                    eprintln!();
                    eprintln!("{}", "Validation failed.".bright_red());
                    std::process::exit(2);
                }
            }

            Commands::CheckInterfaces { vfio, kernel, interfaces } => {
                // Parse interface lists
                let vfio_list: Vec<&str> = vfio
                    .as_ref()
                    .map(|s| s.split(',').collect())
                    .unwrap_or_default();

                let kernel_list: Vec<&str> = kernel
                    .as_ref()
                    .map(|s| s.split(',').collect())
                    .unwrap_or_default();

                // For backward compatibility: if interfaces arg provided without flags
                let existence_list: Vec<&str> = interfaces
                    .as_ref()
                    .map(|s| s.split(',').collect())
                    .unwrap_or_default();

                if vfio_list.is_empty() && kernel_list.is_empty() && existence_list.is_empty() {
                    eprintln!("{}", "Error: No interfaces specified".bright_red());
                    eprintln!("Usage:");
                    eprintln!("  vfio-tool check-interfaces --vfio <list>");
                    eprintln!("  vfio-tool check-interfaces --kernel <list>");
                    eprintln!("  vfio-tool check-interfaces --vfio <list> --kernel <list>");
                    std::process::exit(3);
                }

                match vfio::check_interfaces_with_mode(&vfio_list, &kernel_list, &existence_list) {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("{}", e);
                        // Exit code based on error type
                        if e.to_string().contains("not found") || e.to_string().contains("INTERFACE NOT FOUND") {
                            std::process::exit(1);
                        } else if e.to_string().contains("not in") || e.to_string().contains("wrong mode") || e.to_string().contains("not all required") {
                            std::process::exit(2);
                        } else {
                            std::process::exit(3);
                        }
                    }
                }
            }

            Commands::EnsureVfio { interfaces } => {
                require_root("ensure-vfio");
                let iface_list: Vec<&str> = interfaces.split(',').collect();
                match vfio::ensure_vfio(&iface_list) {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("{}", e);
                        // Exit code based on error type
                        if e.to_string().contains("not found") || e.to_string().contains("INTERFACE NOT FOUND") {
                            std::process::exit(1);
                        } else if e.to_string().contains("Failed to bind") {
                            std::process::exit(2);
                        } else {
                            std::process::exit(3);
                        }
                    }
                }
            }

            Commands::SetupGrub { yes } => {
                require_root("setup-grub");
                grub::setup_iommu(yes)?;
            }

            Commands::Show { framework, capable, format } => {
                let fw = frameworks::Framework::from_str(&framework)
                    .ok_or_else(|| anyhow::anyhow!("Unknown framework: {}\nSupported: dpdk, rdma, tcpdirect, openonload, efvi, spdk, vpp, xdp", framework))?;

                let devices = if capable {
                    frameworks::get_capable_devices(fw)?
                } else {
                    frameworks::get_available_devices(fw)?
                };

                let format_type = format.as_deref().unwrap_or("default");
                display::show_framework_devices(fw, &devices, capable, format_type)?;
            }
        }

        Ok(())
    }
}
