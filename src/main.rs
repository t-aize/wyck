use std::path::PathBuf;

use clap::Parser;

use wyck::config::AppPaths;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(long)]
    config_dir: Option<PathBuf>,
    #[arg(long)]
    reset: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let paths = match args.config_dir {
        Some(dir) => AppPaths::at(dir),
        None => AppPaths::discover()?,
    };
    let _log = init_logging(&paths)?;
    wyck::tui::run(paths, args.reset).await
}

fn init_logging(paths: &AppPaths) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    std::fs::create_dir_all(paths.data_dir())?;
    let file = tracing_appender::rolling::never(paths.data_dir(), "wyck.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "warn,wyck=info".into());
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
    Ok(guard)
}
