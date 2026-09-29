use clap::{CommandFactory, Parser};
use mtop::cli::Cli;
use std::path::PathBuf;

#[test]
fn parses_defaults() {
    let cli = Cli::try_parse_from(["mtop"]).expect("default arguments should parse");

    assert_eq!(cli.interval_ms, None);
    assert_eq!(cli.theme, None);
    assert_eq!(cli.resolved_interval_ms(), 1000);
    assert_eq!(cli.resolved_theme(), "neon");
    assert_eq!(cli.config, None);
    assert!(!cli.once);
}

#[test]
fn parses_interval() {
    let cli = Cli::try_parse_from(["mtop", "--interval-ms", "250"])
        .expect("interval argument should parse");

    assert_eq!(cli.interval_ms, Some(250));
}

#[test]
fn explicit_default_interval_is_present() {
    let cli = Cli::try_parse_from(["mtop", "--interval-ms", "1000"])
        .expect("explicit default interval should parse");

    assert_eq!(cli.interval_ms, Some(1000));
    assert_eq!(cli.resolved_interval_ms(), 1000);
}

#[test]
fn parses_theme() {
    let cli =
        Cli::try_parse_from(["mtop", "--theme", "mono"]).expect("theme argument should parse");

    assert_eq!(cli.theme, Some("mono".to_owned()));
}

#[test]
fn explicit_default_theme_is_present() {
    let cli = Cli::try_parse_from(["mtop", "--theme", "neon"])
        .expect("explicit default theme should parse");

    assert_eq!(cli.theme, Some("neon".to_owned()));
    assert_eq!(cli.resolved_theme(), "neon");
}

#[test]
fn rejects_invalid_theme() {
    assert!(Cli::try_parse_from(["mtop", "--theme", "invalid"]).is_err());
}

#[test]
fn parses_config_path() {
    let cli = Cli::try_parse_from(["mtop", "--config", "/tmp/mtop.toml"])
        .expect("config argument should parse");

    assert_eq!(cli.config, Some(PathBuf::from("/tmp/mtop.toml")));
}

#[test]
fn parses_once() {
    let cli = Cli::try_parse_from(["mtop", "--once"]).expect("once argument should parse");

    assert!(cli.once);
}

#[test]
fn help_lists_supported_options() {
    let help = Cli::command().render_help().to_string();

    for option in ["--interval-ms", "--theme", "--config", "--once"] {
        assert!(help.contains(option), "help should contain {option}");
    }
}
