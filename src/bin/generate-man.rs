use clap::CommandFactory;
use clap_mangen::Man;
use std::io;

fn main() -> io::Result<()> {
    // Get the CLI definition from the main binary
    let cmd = vfio_tool::cli::Cli::command();

    // Generate man page and write to stdout
    let man = Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;

    io::Write::write_all(&mut io::stdout(), &buffer)?;

    Ok(())
}
