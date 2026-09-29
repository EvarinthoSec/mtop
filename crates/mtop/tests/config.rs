use clap::Parser;
use mtop::cli::Cli;
use mtop::config::{Config, ConfigError, ProcessSort, ThemeName};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_config_path(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("mtop-{name}-{}-{nonce}.toml", std::process::id()))
}

fn write_temp_config(name: &str, contents: &str) -> PathBuf {
    let path = temp_config_path(name);
    fs::write(&path, contents).expect("temporary config should be writable");
    path
}

fn remove_temp_config(path: &Path) {
    let _ = fs::remove_file(path);
}

#[test]
fn config_defaults_are_safe() {
    assert_eq!(
        Config::load(None).expect("defaults should load"),
        Config {
            interval_ms: 1000,
            theme: ThemeName::Neon,
            process_limit: 25,
            process_sort: ProcessSort::Cpu,
            show_gpu: true,
            compact: false,
            ..Config::default()
        }
    );
}

#[test]
fn missing_config_file_returns_defaults() {
    let path = temp_config_path("missing");

    assert_eq!(
        Config::load(Some(&path)).expect("missing config is optional"),
        Config::load(None).unwrap()
    );
}

#[test]
fn valid_toml_loads_all_fields() {
    let path = write_temp_config(
        "valid",
        r#"
interval_ms = 250
theme = "amber"
process_limit = 10
process_sort = "memory"
show_gpu = false
compact = true
"#,
    );

    let config = Config::load(Some(&path)).expect("valid config should load");
    remove_temp_config(&path);

    assert_eq!(
        config,
        Config {
            interval_ms: 250,
            theme: ThemeName::Amber,
            process_limit: 10,
            process_sort: ProcessSort::Memory,
            show_gpu: false,
            compact: true,
            ..Config::default()
        }
    );
}

#[test]
fn malformed_toml_returns_parse_error_with_path() {
    let path = write_temp_config("malformed", "interval_ms = [not valid");

    let error = Config::load(Some(&path)).expect_err("malformed config should fail");
    remove_temp_config(&path);

    match error {
        ConfigError::Parse {
            path: error_path, ..
        } => assert_eq!(error_path, path),
        other => panic!("expected parse error, got {other:?}"),
    }
}

#[test]
fn omitted_fields_use_defaults() {
    let path = write_temp_config("omitted", "theme = \"mono\"\ncompact = true\n");

    let config = Config::load(Some(&path)).expect("partial config should load");
    remove_temp_config(&path);

    assert_eq!(
        config,
        Config {
            interval_ms: 1000,
            theme: ThemeName::Mono,
            process_limit: 25,
            process_sort: ProcessSort::Cpu,
            show_gpu: true,
            compact: true,
            ..Config::default()
        }
    );
}

#[test]
fn cli_non_default_values_override_config() {
    let mut config = Config {
        interval_ms: 2000,
        theme: ThemeName::Amber,
        process_limit: 10,
        process_sort: ProcessSort::Memory,
        show_gpu: false,
        compact: true,
        ..Config::default()
    };
    let cli = Cli::try_parse_from(["mtop", "--interval-ms", "250", "--theme", "mono", "--once"])
        .expect("CLI arguments should parse");

    config.merge_cli(&cli);

    assert_eq!(config.interval_ms, 250);
    assert_eq!(config.theme, ThemeName::Mono);
    assert_eq!(config.process_limit, 10);
    assert_eq!(config.process_sort, ProcessSort::Memory);
    assert!(!config.show_gpu);
    assert!(config.compact);
}

#[test]
fn cli_default_values_do_not_override_config() {
    let mut config = Config {
        interval_ms: 2000,
        theme: ThemeName::Amber,
        process_limit: 10,
        process_sort: ProcessSort::Memory,
        show_gpu: false,
        compact: true,
        ..Config::default()
    };
    let cli = Cli::try_parse_from(["mtop"]).expect("default CLI arguments should parse");

    config.merge_cli(&cli);

    assert_eq!(config.interval_ms, 2000);
    assert_eq!(config.theme, ThemeName::Amber);
    assert_eq!(config.process_limit, 10);
    assert_eq!(config.process_sort, ProcessSort::Memory);
    assert!(!config.show_gpu);
    assert!(config.compact);
    // `once` is intentionally parsed by the CLI but is not represented in Config.
}

#[test]
fn cli_explicit_default_values_override_config() {
    let mut config = Config {
        interval_ms: 2000,
        theme: ThemeName::Amber,
        process_limit: 10,
        process_sort: ProcessSort::Memory,
        show_gpu: false,
        compact: true,
        ..Config::default()
    };
    let cli = Cli::try_parse_from(["mtop", "--interval-ms", "1000", "--theme", "neon"])
        .expect("explicit default CLI arguments should parse");

    config.merge_cli(&cli);

    assert_eq!(config.interval_ms, 1000);
    assert_eq!(config.theme, ThemeName::Neon);
}
