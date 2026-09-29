//! Application configuration.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Neon,
    Amber,
    Mono,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessSort {
    Cpu,
    Memory,
    Pid,
    Name,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "default_interval_ms", alias = "update_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_theme")]
    pub theme: ThemeName,
    #[serde(default = "default_process_limit")]
    pub process_limit: usize,
    #[serde(default = "default_process_sort")]
    pub process_sort: ProcessSort,
    #[serde(default = "default_show_gpu")]
    pub show_gpu: bool,
    #[serde(default)]
    pub compact: bool,
    /// btop vim_keys (h/j/k/l navigation, K kill, H help).
    #[serde(default = "yes")]
    pub vim_keys: bool,
    /// Per-core meters in the cpu box.
    #[serde(default = "yes")]
    pub show_cores: bool,
    /// Process tree view.
    #[serde(default, alias = "proc_tree")]
    pub tree: bool,
    /// btop proc_per_core: per-core process cpu%.
    #[serde(default = "yes")]
    pub proc_per_core: bool,
    /// btop net_auto: auto-scale net graphs.
    #[serde(default = "yes")]
    pub net_auto: bool,
    /// btop net_sync: shared scale for download/upload.
    #[serde(default)]
    pub net_sync: bool,
    /// btop shown_boxes: space-separated subset of "cpu mem net proc gpu".
    #[serde(default = "default_boxes", alias = "shown_boxes")]
    pub boxes: String,
    /// btop color_theme: name of a user `.theme` file; overrides `theme`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_theme: Option<String>,
    /// btop presets: space-separated `box:pos:graph,...` groups for p / P.
    #[serde(default = "default_presets")]
    pub presets: String,
    /// btop cpu_bottom: cpu box at the bottom instead of the top.
    #[serde(default)]
    pub cpu_bottom: bool,
    /// btop proc_left: proc box on the left instead of the right.
    #[serde(default)]
    pub proc_left: bool,
    /// btop mem_below_net: mem box below net instead of above.
    #[serde(default)]
    pub mem_below_net: bool,
    /// btop graph_symbol: "braille", "block" or "tty".
    #[serde(default = "default_graph_symbol")]
    pub graph_symbol: String,
    /// btop graph_symbol_<box>: "default" inherits `graph_symbol`.
    #[serde(default = "default_box_symbol")]
    pub graph_symbol_cpu: String,
    #[serde(default = "default_box_symbol")]
    pub graph_symbol_mem: String,
    #[serde(default = "default_box_symbol")]
    pub graph_symbol_net: String,
    #[serde(default = "default_box_symbol")]
    pub graph_symbol_proc: String,
    #[serde(default = "default_box_symbol")]
    pub graph_symbol_gpu: String,
    /// btop rounded_corners.
    #[serde(default = "yes")]
    pub rounded_corners: bool,
    /// btop theme_background: false leaves the terminal background visible.
    #[serde(default = "yes")]
    pub theme_background: bool,
    /// btop show_uptime (cpu box).
    #[serde(default = "yes")]
    pub show_uptime: bool,
    /// btop show_battery (cpu title).
    #[serde(default = "yes")]
    pub show_battery: bool,
    /// btop check_temp (cpu temperatures).
    #[serde(default = "yes")]
    pub check_temp: bool,
    /// btop show_cpu_freq.
    #[serde(default = "yes")]
    pub show_cpu_freq: bool,
    /// btop disable_mouse.
    #[serde(default)]
    pub disable_mouse: bool,
    /// btop save_config_on_exit.
    #[serde(default = "yes")]
    pub save_config_on_exit: bool,
    /// btop custom_cpu_name ("" = detected name).
    #[serde(default)]
    pub custom_cpu_name: String,
    /// btop temp_scale: celsius, fahrenheit, kelvin or rankine.
    #[serde(default = "default_temp_scale")]
    pub temp_scale: String,
    /// btop net_iface: interface to show first ("" = first reported).
    #[serde(default)]
    pub net_iface: String,
    /// btop clock_format: strftime format in the cpu title ("" hides it).
    #[serde(default = "default_clock_format")]
    pub clock_format: String,
    /// btop proc_reversed.
    #[serde(default)]
    pub proc_reversed: bool,
    /// btop proc_mem_bytes (false = percent column).
    #[serde(default = "yes")]
    pub proc_mem_bytes: bool,
    /// btop proc_colors: color process names by cpu usage.
    #[serde(default = "yes")]
    pub proc_colors: bool,
    /// btop proc_gradient: darken lower rows of the process list.
    #[serde(default = "yes")]
    pub proc_gradient: bool,
    /// btop proc_aggregate: tree parents include their children's usage.
    #[serde(default)]
    pub proc_aggregate: bool,
    /// btop show_swap.
    #[serde(default = "yes")]
    pub show_swap: bool,
    /// btop mem_graphs (false = meters only).
    #[serde(default = "yes")]
    pub mem_graphs: bool,
    /// btop show_disks.
    #[serde(default = "yes")]
    pub show_disks: bool,
    /// btop io_mode.
    #[serde(default)]
    pub io_mode: bool,
    /// btop disks_filter: mount points to show, or "exclude=" to hide them.
    #[serde(default)]
    pub disks_filter: String,
    /// btop show_coretemp (false = package temp on the CPU line only).
    #[serde(default = "yes")]
    pub show_coretemp: bool,
    /// btop swap_upload_download: upload graph on top.
    #[serde(default)]
    pub swap_upload_download: bool,
    /// btop net_download / net_upload: fixed scale in Mebibits (net_auto off).
    #[serde(default = "default_net_mbit")]
    pub net_download: u64,
    #[serde(default = "default_net_mbit")]
    pub net_upload: u64,
    /// btop truecolor (false = 256-color approximations).
    #[serde(default = "yes")]
    pub truecolor: bool,
    /// btop force_tty: tty graphs and square corners for limited terminals.
    #[serde(default)]
    pub force_tty: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_ms: default_interval_ms(),
            theme: default_theme(),
            process_limit: default_process_limit(),
            process_sort: default_process_sort(),
            show_gpu: default_show_gpu(),
            compact: false,
            vim_keys: true,
            show_cores: true,
            tree: false,
            proc_per_core: true,
            net_auto: true,
            net_sync: false,
            boxes: default_boxes(),
            color_theme: None,
            presets: default_presets(),
            cpu_bottom: false,
            proc_left: false,
            mem_below_net: false,
            graph_symbol: default_graph_symbol(),
            graph_symbol_cpu: default_box_symbol(),
            graph_symbol_mem: default_box_symbol(),
            graph_symbol_net: default_box_symbol(),
            graph_symbol_proc: default_box_symbol(),
            graph_symbol_gpu: default_box_symbol(),
            rounded_corners: true,
            theme_background: true,
            show_uptime: true,
            show_battery: true,
            check_temp: true,
            show_cpu_freq: true,
            disable_mouse: false,
            save_config_on_exit: true,
            custom_cpu_name: String::new(),
            temp_scale: default_temp_scale(),
            net_iface: String::new(),
            clock_format: default_clock_format(),
            proc_reversed: false,
            proc_mem_bytes: true,
            proc_colors: true,
            proc_gradient: true,
            proc_aggregate: false,
            show_swap: true,
            mem_graphs: true,
            show_disks: true,
            io_mode: false,
            disks_filter: String::new(),
            show_coretemp: true,
            swap_upload_download: false,
            net_download: default_net_mbit(),
            net_upload: default_net_mbit(),
            truecolor: true,
            force_tty: false,
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };

        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(ConfigError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };

        toml::from_str(&contents).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Default location: `<config dir>/mtop/config.toml`
    /// (macOS `~/Library/Application Support`, Linux `$XDG_CONFIG_HOME`).
    pub fn default_path() -> Option<PathBuf> {
        directories::BaseDirs::new().map(|d| d.config_dir().join("mtop").join("config.toml"))
    }

    /// Write the config as TOML, creating the parent directory. Writes to a
    /// temp file then renames so a crash never leaves a half-written config.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let io = |source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).map_err(io)?;
        fs::rename(&tmp, path).map_err(io)
    }

    pub fn merge_cli(&mut self, cli: &impl ConfigOverrides) {
        if let Some(interval_ms) = cli.interval_ms() {
            self.interval_ms = interval_ms;
        }
        if let Some(theme) = cli.theme() {
            self.theme = match theme {
                "neon" => ThemeName::Neon,
                "amber" => ThemeName::Amber,
                "mono" => ThemeName::Mono,
                _ => unreachable!("CLI theme is validated by clap"),
            };
        }
        // CLI-only flags such as `once` are intentionally not config fields.
    }
}

/// Minimal CLI overrides accepted by configuration without coupling it to
/// a particular argument parser.
pub trait ConfigOverrides {
    fn interval_ms(&self) -> Option<u64>;
    fn theme(&self) -> Option<&str>;
}

fn default_interval_ms() -> u64 {
    1000
}

fn default_theme() -> ThemeName {
    ThemeName::Neon
}

fn default_process_limit() -> usize {
    25
}

fn default_process_sort() -> ProcessSort {
    ProcessSort::Cpu
}

fn default_show_gpu() -> bool {
    true
}

fn yes() -> bool {
    true
}

fn default_boxes() -> String {
    "cpu mem net proc gpu".to_owned()
}

/// btop's shipped default presets (btop_config.cpp).
fn default_presets() -> String {
    "cpu:1:default,proc:0:default cpu:0:default,mem:0:default,net:0:default cpu:0:block,net:0:tty"
        .to_owned()
}

fn default_graph_symbol() -> String {
    "braille".to_owned()
}

fn default_box_symbol() -> String {
    "default".to_owned()
}

fn default_temp_scale() -> String {
    "celsius".to_owned()
}

fn default_clock_format() -> String {
    "%X".to_owned()
}

fn default_net_mbit() -> u64 {
    100
}
