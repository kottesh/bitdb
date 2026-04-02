use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, version, about = "Bitcask-style key-value store")]
pub struct Cli {
    #[arg(long, default_value = ".")]
    pub data_dir: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Put {
        key: String,
        value: String,
    },
    Get {
        key: String,
    },
    Delete {
        key: String,
    },
    Stats,
    Merge,
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
    /// Launch the interactive TUI.
    Tui,
}

#[derive(Debug, Subcommand)]
pub enum BenchCommand {
    Startup {
        #[arg(long, value_enum, default_value_t = BenchMode::Serial)]
        mode: BenchMode,
    },
    Merge {
        #[arg(long, value_enum, default_value_t = BenchMode::Serial)]
        mode: BenchMode,
    },
    Workload {
        #[arg(long, default_value_t = 1000)]
        ops: u64,
        #[arg(long, value_enum, default_value_t = BenchMode::Serial)]
        mode: BenchMode,
        #[arg(long, default_value_t = 1)]
        threads: usize,
    },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, clap::ValueEnum, Default)]
pub enum BenchMode {
    #[default]
    Serial,
    Parallel,
}

pub fn parse() -> Cli {
    Cli::parse()
}
