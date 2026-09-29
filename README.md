# mtop

`mtop` targets Linux, macOS, Windows, and BSD-family systems, with metrics and controls varying by platform. Its visual direction is inspired by `btop`, but `mtop` is an independent project—not a clone.

The monitor provides a readable dashboard for CPU, memory, swap, disks, networks, processes, uptime, and best-effort GPU information. Metric availability depends on the operating system, permissions, hardware, and drivers.

## Build and run

The minimum supported Rust version is 1.85.0. Build and run a debug binary with:

```sh
cargo build
cargo run
```

Build the release binary and run the scriptable one-shot report with:

```sh
cargo build --release
cargo run --release -- --once
./target/release/mtop --once
```

The interactive command-line options are documented by:

```sh
cargo run -- --help
```

## Workspace layout

The Cargo workspace keeps Rust packages under `crates/`:

- `crates/mtop` — CLI, composition root, and compatibility facade.
- `crates/core` — shared snapshots, ports, and formatting primitives.
- `crates/config` — config types and TOML persistence.
- `crates/platform` — system collectors and process-control adapters.
- `crates/runtime` — collector worker lifecycle and control channel.
- `crates/tui` — Ratatui rendering, themes, menus, and terminal input.

The dependency direction stays inward: platform adapters implement core
ports, runtime manages collection without knowing the platform adapter, and
the TUI consumes runtime controls without depending on `mtop-platform`.

Run the complete test suite with `cargo test --workspace`.

## Install

The reproducible Unix packaging helper installs the release binary, man page, and desktop entry. Build first, then install to `/usr/local` (root privileges are normally required for that prefix):

```sh
cargo build --release
sudo packaging/mtop.install.sh
```

A different prefix can be supplied either as an argument or with `PREFIX`:

```sh
packaging/mtop.install.sh "$HOME/.local"
# equivalent:
PREFIX="$HOME/.local" packaging/mtop.install.sh
```

Use `DESTDIR` for a rootless staged install; this does not write to the live system:

```sh
cargo build --release
DESTDIR="$PWD/stage" PREFIX=/usr/local packaging/mtop.install.sh
```

That example installs into:

- `stage/usr/local/bin/mtop`
- `stage/usr/local/share/man/man1/mtop.1`
- `stage/usr/local/share/applications/mtop.desktop`

The helper fails clearly when `target/release/mtop` is missing and touches only the packaged binary, man page, and desktop entry. It does not create or overwrite user configuration or secret files.

### Debian and Ubuntu packages

Pushing a version tag such as `v1.2.3` runs the GitHub Actions release workflow. It publishes an amd64 `.deb` plus macOS Intel and Apple Silicon archives. Install the Debian package with:

```sh
sudo apt install ./mtop_<version>_amd64.deb
```

The `.deb` requires `libc6 >= 2.35` (Ubuntu 22.04 or a compatible Debian-based distribution).

The project is also listed on [Launchpad](https://launchpad.net/mtop-monitor). That page is the project listing, not an Ubuntu PPA; install the published `.deb` from GitHub Releases until a PPA is available.

### Homebrew

The release workflow attaches a versioned Homebrew formula. Install the latest published formula directly:

```sh
brew install --formula https://github.com/EvarinthoSec/mtop/releases/latest/download/mtop.rb
```

This URL-based install does not require a separate Homebrew tap.

### Ratty 3D panels

Run mtop inside a [Ratty](https://github.com/orhun/ratty) terminal with the opt-in flag to render 3D bezels around the dashboard panels:

```sh
mtop --ratty-3d
```

Ratty's Graphics Protocol is terminal-specific; omit the flag in ordinary terminals. In Ratty, `Ctrl+Alt+Enter` switches the terminal surface into 3D mode. The dashboard's default rendering does not emit Ratty protocol sequences.

## Documentation and configuration

After installation, the manual is available as `man mtop`, and the desktop entry is installed at `share/applications/mtop.desktop` below the selected prefix. The desktop entry launches `mtop` in a terminal and does not enable autostart or hide the application.

Configuration is optional. The current implementation reads a TOML file only when an explicit path is passed with `--config`; it does not create a file or search a default path automatically. A missing explicit file uses built-in defaults, while malformed TOML causes a user-readable error and a non-zero exit.

Example:

```sh
mtop --config ~/.config/mtop/config.toml
```

Supported fields are:

```toml
interval_ms = 1000
theme = "neon"
process_limit = 25
process_sort = "cpu"
show_gpu = true
```

Command-line `--interval-ms` and `--theme` values override the corresponding configuration values. Available themes are `neon`, `amber`, and `mono`.

## Keyboard shortcuts

Keys follow btop. mtop starts in btop's `vim_keys` layout (so `j`/`k`
navigate); switch it off in the options panel (`o` → Vim keys) to get btop's
default layout, where `k` kills and `h` opens help.

| Key | Action |
| --- | --- |
| `Esc` or `m` | Toggle main menu (Options / Help / Quit) |
| `o` or `F2` | Options panel: theme, update ms, vim keys, per-core, GPU, tree |
| `?`, `F1`, `H` (`h` without vim keys) | Toggle help |
| `q` or `Ctrl+C` | Quit |
| `+` / `-` | Add/subtract 100 ms to/from the update timer |
| `F5` | Refresh now |
| `Ctrl+R` | Reload the settings file from disk |
| `Space` | Pause or resume all updates |
| `u` | Pause the process list only (other boxes keep updating) |
| `Up`/`Down` (`j`/`k` with vim keys) | Select in process list |
| `PgUp`/`PgDn`, `Home`/`End` (`g`/`G`) | Page / jump in process list |
| `Left`/`Right` (`h`/`l`) | Previous / next sorting column |
| `r` | Reverse sort order |
| `e` | Toggle process tree view |
| `f` or `/` | Process filter on name, command, user or pid; start with `!` for regex (`Delete` clears it) |
| `Enter` | Detailed information for the selected process |
| `t` / `K` (`k` without vim keys) | SIGTERM / SIGKILL the selected process (confirmed) |
| `s` | Pick a signal by number for the selected process |
| `1` `2` `3` `4` `5` | Toggle cpu / mem / net / proc / gpu box (layout reflows) |
| `b` / `n` | Previous / next network interface |
| `z` | Reset network totals for the current interface |
| `a` / `y` | Net graphs: auto scaling / synced download-upload scale |
| `i` | Disks box: big read/write IO graphs |
| `d` | Show / hide the disks view in the mem box |
| `F` | Follow the selected process across refreshes |
| `c` | Process cpu%: per-core vs share of total |
| `%` | Process memory column: bytes vs percent |
| `Space`, `+`, `-`, `E` (tree view) | Toggle / expand / collapse selected, collapse-expand all |
| `C` (tree view) | Collapse / expand every child of the selected process |
| `N` | Set a new nice value for the selected process (`-20`..`19`; lowering needs root) |
| `p` / `P` | Cycle view presets forwards / backwards (btop `presets` config) |
| `Ctrl+Z` | Suspend to the shell on Unix; unavailable on Windows |
| Mouse wheel / click | Scroll / select in the process list |
| Click a column header | Sort by it; click again to reverse |
| Click `per-core□` / `reverse□` / `tree□` | Toggle that option from the proc title |

Box positions follow btop's `cpu_bottom`, `proc_left` and `mem_below_net`:
toggle them in the options panel, or set the `P` flag in a preset
(e.g. `cpu:1:default,proc:1:default`).

Graphs can be drawn with btop's three symbol sets: `braille` (default,
finest), `block`, or `tty` (shade characters that render in any font).
Set the global style with the options panel's Graph symbol row or
`graph_symbol` in the config; override per box with `graph_symbol_cpu`,
`graph_symbol_mem`, `graph_symbol_net`, `graph_symbol_proc` or
`graph_symbol_gpu` (`"default"` inherits), or with a preset's third field
(`cpu:0:block,net:0:tty`).

`Enter` opens btop's detailed view: a per-process CPU graph, status,
elapsed time, user, threads, nice, parent, a memory meter and the full
command line.

## Themes

Besides the built-in `neon`, `amber` and `mono`, any btop `.theme` file
dropped into `<config dir>/mtop/themes/` shows up in the options panel's
Color theme row (all 41 themes shipped with btop load). The chosen theme is
saved as `color_theme` in the settings file.

## Settings file

Options-panel changes and box toggles are written on exit to
`<config dir>/mtop/config.toml` (macOS: `~/Library/Application Support/mtop/`,
Linux: `$XDG_CONFIG_HOME/mtop/`), or to the file given with `--config`.
Older config files keep working; missing keys fall back to btop defaults.

btop display keys supported in the settings file:

| Key | Default | Effect |
|---|---|---|
| `rounded_corners` | `true` | Rounded or square box corners |
| `theme_background` | `true` | `false` shows the terminal's own background |
| `show_uptime` / `show_battery` / `show_cpu_freq` | `true` | Cpu box details |
| `check_temp` | `true` | Per-core temperatures in the cpu rail |
| `temp_scale` | `"celsius"` | `fahrenheit`, `kelvin` or `rankine` |
| `custom_cpu_name` | `""` | Replace the detected cpu name |
| `clock_format` | `"%X"` | strftime clock in the cpu title; `""` hides it |
| `net_iface` | `""` | Interface the net box starts on |
| `disable_mouse` | `false` | Ignore mouse input |
| `save_config_on_exit` | `true` | `false` never rewrites the file |
| `show_swap` / `mem_graphs` / `show_disks` | `true` | Mem box: swap meter, history graphs, disks view |
| `io_mode` | `false` | Start the disks view in IO graph mode |
| `disks_filter` | `""` | Mount points to show, or `exclude=/a /b` to hide them |
| `show_coretemp` | `true` | `false` shows only the package temp on the CPU line |
| `swap_upload_download` | `false` | Put the upload graph on top |
| `net_download` / `net_upload` | `100` | Fixed net graph scale in Mebibits (when `net_auto` is off) |
| `proc_reversed` / `proc_mem_bytes` | `false` / `true` | Starting sort direction and memory column |
| `proc_colors` / `proc_gradient` | `true` | Color names by cpu, fade lower rows |
| `proc_aggregate` | `false` | Tree parents include their children's cpu (collapsed nodes always do) |
| `truecolor` | `true` | `false` maps colors to the 256-color palette |
| `force_tty` | `false` | tty graphs and square corners for limited terminals |

btop's own key names `update_ms`, `proc_tree` and `shown_boxes` are accepted
too, so most of a `btop.conf` can be copied over.

## Supported metrics and GPU behavior

The dashboard may show:

- CPU utilization, per-core utilization, and load averages
- Memory and swap utilization
- Disk usage and available I/O rates
- Network receive/transmit rates
- Process activity and resource usage
- System uptime and wall clock; battery (macOS `pmset`, Linux sysfs, Windows `GetSystemPowerStatus`)
- GPU utilization and memory when an optional supported provider is available

GPU information is optional. Unavailable GPU data is normal and does not indicate that `mtop` is malfunctioning. On macOS, utilization and GPU memory in use come from `ioreg` (IOAccelerator) with no extra dependencies or root; Apple does not expose GPU temperature there. The optional `gpu-nvidia` feature compiles the NVIDIA backend on supported Linux builds; it still requires compatible hardware, drivers, and NVML. When a GPU is present its box sits beside the CPU box; toggle it with `5`.

## Platform status

These status labels describe verification in this repository, not a promise that every metric exists on every host:

- **macOS:** native-tested in the current development environment.
- **Windows:** cross-compilation verified for `x86_64-pc-windows-gnu` and `x86_64-pc-windows-msvc`; native runtime testing is still required. CPU, memory, process, disk, network, and system battery status are supported; load average and GPU metrics are unavailable. `TERM`/`KILL` use hard process termination, other POSIX signals are unavailable, and process priority changes map approximately to Windows priority classes.
- **Linux:** compile-only or otherwise unverified in the current development environment; runtime support is not claimed from compilation alone.
- **FreeBSD, OpenBSD, and NetBSD:** compile-only/untested unless a separate run is recorded; runtime support is not claimed from compilation alone.
- **Other targets:** not a supported release target; compilation may work when the Rust dependencies support it.

The common collector degrades unavailable or permission-limited metrics to `N/A` or warnings instead of fabricating values. macOS/BSD native application bundles are out of scope; use Cargo or the Unix packaging helper.

## License

mtop is licensed under the MIT License; see [LICENSE](LICENSE).

The UI takes visual inspiration from btop++ and adapts selected rendering
details. Its upstream copyright and Apache-2.0 attribution are retained in
`crates/tui/src/ui.rs`; the Apache-2.0 license copy is included at
[`THIRD_PARTY_LICENSES/btop-Apache-2.0.txt`](THIRD_PARTY_LICENSES/btop-Apache-2.0.txt)
to satisfy the applicable redistribution notice requirements.
