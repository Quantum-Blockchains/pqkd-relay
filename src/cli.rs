use clap::Parser;
use std::path::PathBuf;
/// todo

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to config file
    #[arg(short = 'c', long = "config", value_name = "CONFIG_FILE")]
    pub config_file: PathBuf,
    // Path to file with topologi networks pqkd
    //#[arg(short = 't', long = "tapology", value_name = "TAPOLOGY_FILE")]
    //pub tapology_file: PathBuf,
}

impl Args {
    pub fn fron_args() -> Args {
        Args::parse()
    }
}
