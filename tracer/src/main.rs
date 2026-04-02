use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> std::io::Result<()> {
    let data_dir = parse_data_dir();
    std::fs::create_dir_all(&data_dir)?;
    tracer::tui::run(data_dir)
}

/// Parse `--data-dir <path>` from argv, defaulting to `./tracer-data`.
fn parse_data_dir() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--data-dir" && i + 1 < args.len() {
            return PathBuf::from(&args[i + 1]);
        }
        i += 1;
    }
    PathBuf::from("./tracer-data")
}
