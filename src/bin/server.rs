use anyhow::Result;
use std::fs::OpenOptions;
use talps_manager::manager::TaskManager;
use tracing_subscriber::EnvFilter;

struct TalpsServer {
    manager: TaskManager,
}
impl TalpsServer {}

fn log_init() {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("app.log")
        .expect("Failed to create log file");

    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

fn main() -> Result<()> {
    Ok(())
}
