use anyhow::{bail, Result};
use framecap::layer::memory_temperature::{read_bios, read_hotspot};
use std::env;
use std::process::ExitCode;

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<bool> {
    let mut bios_only = false;
    for arg in args {
        match arg.as_str() {
            "--bios-only" if !bios_only => bios_only = true,
            _ => bail!("unknown or repeated argument: {arg}"),
        }
    }
    Ok(bios_only)
}

fn run() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!(
            "Usage: bc250-memory-probe [--bios-only]\n\
             Reads the BIOS version and the GDDR6 hotspot from the bc250_memory \
             hwmon driver. The SMU patch is not applied by this tool."
        );
        return Ok(());
    }
    let bios_only = parse_args(args)?;
    println!("BIOS: {}", read_bios()?);
    if bios_only {
        return Ok(());
    }

    let hotspot = read_hotspot()?;
    let prefix = if hotspot.saturated { ">=" } else { "" };
    println!("GDDR6 hotspot: {prefix}{} °C", hotspot.temperature_c);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_cli_arguments() {
        assert!(!parse_args(Vec::<String>::new()).unwrap());
        assert!(parse_args(["--bios-only".into()]).unwrap());
        assert!(parse_args(["--repo".into()]).is_err());
        assert!(parse_args(["--bios-only".into(), "--bios-only".into()]).is_err());
        assert!(parse_args(["--unknown".into()]).is_err());
    }
}
