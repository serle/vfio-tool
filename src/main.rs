mod cli;
mod config;
mod device;
mod grub;
mod iommu;
mod systemd;
mod vfio;
mod display;
mod error;
mod frameworks;

use clap::Parser;
use anyhow::Result;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli.run()
}
