//! Command-line interface handling.

use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::platform::{SnapshotProvider, SysinfoCollector};
use crate::theme::Theme;
use crate::ui::{AppView, run_tui};

#[derive(Debug, Parser, PartialEq)]
#[command(name = "mtop", version, about = "A fast terminal system monitor")]
pub struct Cli {
    #[arg(
        short,
        long,
        value_name = "MILLISECONDS",
        help = "Refresh interval in milliseconds (default: 1000)"
    )]
    pub interval_ms: Option<u64>,
    #[arg(
        long,
        value_parser = ["neon", "amber", "mono"],
        help = "Color theme (default: neon)"
    )]
    pub theme: Option<String>,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub once: bool,
}

impl Cli {
    pub fn resolved_interval_ms(&self) -> u64 {
        self.interval_ms.unwrap_or(1000)
    }
    pub fn resolved_theme(&self) -> &str {
        self.theme.as_deref().unwrap_or("neon")
    }
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // btop keeps its settings in a config file it rewrites on exit; mtop
    // does the same at <config dir>/mtop/config.toml unless --config is set.
    let config_path = cli.config.clone().or_else(Config::default_path);
    let mut config = Config::load(config_path.as_deref())?;
    config.merge_cli(&cli);
    let mut collector = SysinfoCollector::new(&config);
    if cli.once {
        let snapshot = collector.collect();
        println!("mtop snapshot");
        println!(
            "CPU: {}",
            if snapshot.cpu.available {
                crate::format::format_percent(snapshot.cpu.overall_percent)
            } else {
                "N/A".to_owned()
            }
        );
        if snapshot.memory.total_bytes == 0 {
            println!("Memory: N/A");
        } else {
            println!(
                "Memory: {} / {}",
                crate::format::format_bytes(snapshot.memory.used_bytes),
                crate::format::format_bytes(snapshot.memory.total_bytes)
            );
        }
        println!("Processes: {}", snapshot.processes.len());
        return Ok(());
    }

    let interval = Duration::from_millis(config.interval_ms.max(100));
    let theme_name = match config.theme {
        crate::config::ThemeName::Neon => "neon",
        crate::config::ThemeName::Amber => "amber",
        crate::config::ThemeName::Mono => "mono",
    };
    // Install the render palette before the first frame so `--theme` recolors
    // the whole dashboard.
    crate::ui::set_palette(crate::ui::Palette::from_name(theme_name));
    let theme = Theme::from_name(theme_name);
    let (receiver, commands, worker) = crate::app::spawn_collector(collector, interval);
    let mut view = AppView::new(receiver.latest(), theme);
    view.theme_dir = crate::ui::theme_dir();
    view.apply_config(&config);
    view.refresh_interval = interval;
    view.hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_owned());
    let result = run_tui(
        receiver,
        commands,
        view,
        Box::new(crate::process_control::LibcController),
        config_path.as_deref(),
    );
    let _ = worker.join();
    // Persist what the user changed in the options panel / box toggles.
    // btop save_config_on_exit=false keeps the file as the user wrote it.
    let save = config.save_config_on_exit;
    if let (true, Ok(final_view), Some(path)) = (save, &result, config_path.as_deref()) {
        if let Err(error) = final_view.to_config(&config).save(path) {
            eprintln!("mtop: could not save config: {error}");
        }
    }
    result.map(|_| ())
}
