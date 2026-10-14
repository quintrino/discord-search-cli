use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, version, about = "Search cached Discord channels and DMs")]
pub struct Cli {
    #[arg(long, global = true, env = "DISCORD_TOKEN")]
    pub token: Option<String>,

    #[arg(long, global = true)]
    pub auth_prefix: Option<String>,

    #[arg(long, global = true)]
    pub raw_token: bool,

    #[arg(long, global = true)]
    pub cache_path: Option<std::path::PathBuf>,

    #[arg(long, global = true)]
    pub config_path: Option<std::path::PathBuf>,

    #[arg(long = "no-channels", global = true)]
    pub no_channels: bool,

    #[arg(long = "no-dms", global = true)]
    pub no_dms: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Search {
        query: Option<String>,
    },
    Refresh,
    Open {
        url: String,
    },
    CachePath,
}
