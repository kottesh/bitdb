fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> bitdb::error::Result<()> {
    let cli = bitdb::cli::parse();
    let mut engine = bitdb::engine::Engine::open(&cli.data_dir, bitdb::config::Options::default())?;

    match cli.command {
        bitdb::cli::Command::Put { key, value } => {
            engine.put(key.as_bytes(), value.as_bytes())?;
            println!("OK");
        }
        bitdb::cli::Command::Get { key } => match engine.get(key.as_bytes())? {
            Some(value) => {
                println!("{}", String::from_utf8_lossy(&value));
            }
            None => {
                println!("NOT_FOUND");
            }
        },
        bitdb::cli::Command::Delete { key } => {
            engine.delete(key.as_bytes())?;
            println!("OK");
        }
        bitdb::cli::Command::Stats => {
            let stats = engine.stats();
            println!(
                "live_keys={} tombstones={}",
                stats.live_keys, stats.tombstones
            );
        }
        bitdb::cli::Command::Merge => {
            engine.merge()?;
            println!("OK");
        }
        bitdb::cli::Command::Bench { command } => match command {
            bitdb::cli::BenchCommand::Startup { mode } => {
                let out = bitdb::bench::bench_startup(&cli.data_dir, mode)?;
                println!("{out}");
            }
            bitdb::cli::BenchCommand::Merge { mode } => {
                let out = bitdb::bench::bench_merge(&cli.data_dir, mode)?;
                println!("{out}");
            }
            bitdb::cli::BenchCommand::Workload { ops, mode, threads } => {
                let out = bitdb::bench::bench_workload(&cli.data_dir, ops, mode, threads)?;
                println!("{out}");
            }
        },
    }

    Ok(())
}
