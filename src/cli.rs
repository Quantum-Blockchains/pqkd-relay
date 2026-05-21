use clap::Parser;
use std::path::PathBuf;
/// todo

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to config file
    #[arg(short = 'c', long = "config", value_name = "CONFIG_FILE")]
    pub config_file: PathBuf,
    /// Path to mesh topology file
    #[arg(short = 't', long = "topology", value_name = "TOPOLOGY_FILE")]
    pub topology_file: PathBuf,
}

impl Args {
    pub fn fron_args() -> Args {
        Args::parse()
    }
}
