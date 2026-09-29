use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mtop::app::Action;
use mtop::model::{
    CpuSnapshot, DiskSnapshot, MemorySnapshot, NetworkSnapshot, ProcessSnapshot, SystemSnapshot,
};
use mtop::theme::Theme;
use mtop::ui::{AppView, HISTORY_LIMIT, draw_dashboard, key_to_action, mouse_to_action};
use ratatui::{Terminal, backend::TestBackend};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn valid_graph_view() -> AppView {
    let mut view = AppView::new(
        SystemSnapshot {
            disks: vec![DiskSnapshot {
                name: "disk0".into(),
                mount_point: "/".into(),
                total_bytes: 1000,
                available_bytes: 250,
                read_bytes_per_second: Some(4096),
                write_bytes_per_second: Some(2048),
            }],
            networks: vec![NetworkSnapshot {
                received_bytes_total: None,
                transmitted_bytes_total: None,
                interface: "en0".into(),
                received_bytes_per_second: 4096,
                transmitted_bytes_per_second: 2048,
            }],
            ..SystemSnapshot::default()
        },
        Theme::from_name("mono"),
    );
    view.record_snapshot();
    view
}

fn rendered_text(view: &AppView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, view)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn sort_cycles_and_reorders_visible_processes() {
    use mtop::config::ProcessSort;
    let snapshot = SystemSnapshot {
        processes: vec![
            ProcessSnapshot {
                pid: 1,
                name: "alpha".into(),
                cpu_percent: 5.0,
                memory_bytes: 900,
                status: "R".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
                parent_pid: None,
                command: String::new(),
            },
            ProcessSnapshot {
                pid: 2,
                name: "beta".into(),
                cpu_percent: 90.0,
                memory_bytes: 100,
                status: "R".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
                parent_pid: None,
                command: String::new(),
            },
        ],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    // default sort = Cpu desc -> beta(90) before alpha(5)
    assert_eq!(view.sort, ProcessSort::Cpu);
    let ordered = view.sorted_process_indices();
    assert_eq!(view.snapshot.processes[ordered[0]].name, "beta");
    // cycle Cpu -> Memory: alpha(900) before beta(100)
    view.cycle_sort(true);
    assert_eq!(view.sort, ProcessSort::Memory);
    let ordered = view.sorted_process_indices();
    assert_eq!(view.snapshot.processes[ordered[0]].name, "alpha");
    // reverse toggles ascending
    view.toggle_sort_reverse();
    let ordered = view.sorted_process_indices();
    assert_eq!(view.snapshot.processes[ordered[0]].name, "beta");
}

#[test]
fn proc_header_marks_active_sort_column_with_arrow() {
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "p".into(),
            cpu_percent: 1.0,
            memory_bytes: 1,
            status: "R".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    view.sort = mtop::config::ProcessSort::Memory;
    view.sort_reverse = false;
    let text = rendered_text(&view, 160, 48);
    // Active column (MemB) carries the descending arrow; Cpu% loses its arrow.
    assert!(text.contains("MemB↓"), "expected MemB↓ in buffer");
    assert!(!text.contains("Cpu%↑"), "Cpu% should not carry the arrow");
}

#[test]
fn arrow_keys_map_to_sort_actions() {
    assert_eq!(key_to_action(key(KeyCode::Right)), Some(Action::SortNext));
    assert_eq!(key_to_action(key(KeyCode::Left)), Some(Action::SortPrev));
}

#[test]
fn signal_request_requires_confirmation_then_dispatches() {
    use mtop::process_control::{RecordingController, Signal};
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 4242,
            name: "victim".into(),
            cpu_percent: 1.0,
            memory_bytes: 1,
            status: "R".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    assert!(view.pending_signal.is_none());
    // request terminate on the selected process -> stages a confirmation
    view.request_signal(Signal::Term);
    assert_eq!(view.pending_signal, Some((4242, Signal::Term)));
    // cancel clears it, sends nothing
    let ctl = RecordingController::default();
    view.cancel_signal();
    assert!(view.pending_signal.is_none());
    assert!(ctl.sent().is_empty());
    // request again, confirm -> dispatches exactly once and clears
    view.request_signal(Signal::Kill);
    view.confirm_signal(&ctl);
    assert!(view.pending_signal.is_none());
    assert_eq!(ctl.sent(), vec![(4242, Signal::Kill)]);
}

#[test]
fn confirmation_overlay_shows_pid_signal_and_keys() {
    use mtop::process_control::Signal;
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 999,
            name: "doomed".into(),
            cpu_percent: 1.0,
            memory_bytes: 1,
            status: "R".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    view.request_signal(Signal::Kill);
    let text = rendered_text(&view, 120, 40);
    assert!(text.contains("SIGKILL"), "overlay must name the signal");
    assert!(text.contains("999"), "overlay must name the pid");
    assert!(text.contains("doomed"), "overlay must name the process");
    let lower = text.to_lowercase();
    assert!(
        lower.contains("enter") && lower.contains("esc"),
        "overlay must list confirm/cancel keys"
    );
}

#[test]
fn signal_keys_map_to_actions() {
    assert_eq!(
        key_to_action(key(KeyCode::Char('t'))),
        Some(Action::RequestTerminate)
    );
    assert_eq!(
        key_to_action(KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT)),
        Some(Action::RequestKill)
    );
}

#[test]
fn tree_order_nests_children_under_parents() {
    let procs = vec![
        ProcessSnapshot {
            pid: 1,
            name: "init".into(),
            parent_pid: None,
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            status: "S".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
        },
        ProcessSnapshot {
            pid: 10,
            name: "shell".into(),
            parent_pid: Some(1),
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            status: "S".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
        },
        ProcessSnapshot {
            pid: 20,
            name: "editor".into(),
            parent_pid: Some(10),
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            status: "S".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
        },
    ];
    let flat = mtop::ui::tree_order(&procs);
    let names: Vec<(&str, usize)> = flat
        .iter()
        .map(|&(i, d)| (procs[i].name.as_str(), d))
        .collect();
    assert_eq!(names, vec![("init", 0), ("shell", 1), ("editor", 2)]);
}

#[test]
fn tree_view_toggles_and_indents_children() {
    assert_eq!(
        key_to_action(key(KeyCode::Char('e'))),
        Some(Action::ToggleTree)
    );
    let snapshot = SystemSnapshot {
        processes: vec![
            ProcessSnapshot {
                pid: 1,
                name: "initproc".into(),
                parent_pid: None,
                command: String::new(),
                cpu_percent: 0.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
            ProcessSnapshot {
                pid: 10,
                name: "childproc".into(),
                parent_pid: Some(1),
                command: String::new(),
                cpu_percent: 0.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
        ],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    assert!(!view.tree);
    view.apply_public(Action::ToggleTree);
    assert!(view.tree);
    let text = rendered_text(&view, 160, 48);
    // child row carries a tree branch prefix
    assert!(text.contains("└─"), "expected tree branch glyph in buffer");
    assert!(text.contains("childproc"), "child must still render");
}

#[test]
fn mouse_wheel_maps_to_selection() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mk = |kind| MouseEvent {
        kind,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        mouse_to_action(mk(MouseEventKind::ScrollDown)),
        Some(Action::SelectNext)
    );
    assert_eq!(
        mouse_to_action(mk(MouseEventKind::ScrollUp)),
        Some(Action::SelectPrevious)
    );
    assert_eq!(mouse_to_action(mk(MouseEventKind::Moved)), None);
    assert_eq!(
        mouse_to_action(mk(MouseEventKind::Down(MouseButton::Right))),
        None
    );
}

#[test]
fn mouse_click_selects_process_row() {
    let snapshot = SystemSnapshot {
        processes: (0..5)
            .map(|i| ProcessSnapshot {
                pid: 100 + i,
                name: format!("proc{i}"),
                parent_pid: None,
                command: String::new(),
                cpu_percent: 0.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            })
            .collect(),
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    // Must draw once so the proc list hit-zone geometry is recorded.
    let backend = TestBackend::new(160, 48);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw_dashboard(f, &view)).unwrap();
    let z = view.proc_hit();
    assert!(
        z.rows >= 5,
        "expected at least 5 visible proc rows, got {}",
        z.rows
    );
    // Click the third data row (offset 2) inside the table columns.
    let hit = view.proc_row_at(z.x0, z.data_y0 + 2);
    assert_eq!(hit, Some(2));
    // Clicks outside the table are ignored.
    assert_eq!(view.proc_row_at(z.x0, z.data_y0.saturating_sub(1)), None);
    view.select_visible(hit.unwrap());
    assert_eq!(view.selected_process, 2);
}

#[test]
fn signal_picker_collects_number_then_stages_confirmation() {
    use mtop::process_control::Signal;
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 4242,
            name: "target".into(),
            parent_pid: None,
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            status: "S".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    // 's' opens the picker.
    assert_eq!(
        key_to_action(key(KeyCode::Char('s'))),
        Some(Action::SignalPicker)
    );
    view.apply_public(Action::SignalPicker);
    assert!(view.signal_picker.is_some());
    // Type "19" and confirm -> stages SIGSTOP(19) for the selected process.
    view.signal_picker_input('1');
    view.signal_picker_input('9');
    view.signal_picker_confirm();
    assert_eq!(view.pending_signal, Some((4242, Signal::Raw(19))));
    assert!(view.signal_picker.is_none());
    // An invalid number cancels cleanly (no pending signal).
    view.pending_signal = None;
    view.apply_public(Action::SignalPicker);
    view.signal_picker_input('9');
    view.signal_picker_input('9');
    view.signal_picker_input('9');
    view.signal_picker_confirm();
    assert_eq!(view.pending_signal, None);
    assert!(view.signal_picker.is_none());
}

#[test]
fn signal_picker_overlay_shows_prompt_and_buffer() {
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 7,
            name: "svc".into(),
            parent_pid: None,
            command: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            status: "S".into(),
            user: None,
            elapsed_secs: None,
            threads: None,
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    view.apply_public(Action::SignalPicker);
    view.signal_picker_input('1');
    view.signal_picker_input('5');
    let text = rendered_text(&view, 120, 40);
    assert!(text.contains("signal"), "overlay must mention signal");
    assert!(text.contains("15"), "overlay must echo the typed number");
}

#[test]
fn r_key_toggles_sort_reverse() {
    assert_eq!(
        key_to_action(key(KeyCode::Char('R'))),
        Some(Action::ToggleReverse)
    );
    let snapshot = SystemSnapshot {
        processes: vec![
            ProcessSnapshot {
                pid: 1,
                name: "low".into(),
                parent_pid: None,
                command: String::new(),
                cpu_percent: 1.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
            ProcessSnapshot {
                pid: 2,
                name: "high".into(),
                parent_pid: None,
                command: String::new(),
                cpu_percent: 99.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
        ],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    // Default sort = cpu desc -> "high" first.
    let first = view.sorted_process_indices()[0];
    assert_eq!(view.snapshot.processes[first].name, "high");
    view.apply_public(Action::ToggleReverse);
    assert!(view.sort_reverse);
    let first = view.sorted_process_indices()[0];
    assert_eq!(view.snapshot.processes[first].name, "low");
}

#[test]
fn tree_mode_signal_targets_the_displayed_selection() {
    use mtop::process_control::{RecordingController, Signal};
    // Processes whose tree order differs from cpu-sorted order.
    let snapshot = SystemSnapshot {
        processes: vec![
            ProcessSnapshot {
                pid: 100,
                name: "root".into(),
                parent_pid: None,
                command: String::new(),
                cpu_percent: 5.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
            ProcessSnapshot {
                pid: 200,
                name: "child".into(),
                parent_pid: Some(100),
                command: String::new(),
                cpu_percent: 99.0,
                memory_bytes: 0,
                status: "S".into(),
                user: None,
                elapsed_secs: None,
                threads: None,
            },
        ],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    view.apply_public(Action::ToggleTree);
    // Tree order = [root(100), child(200)]. Select row 0 -> must target root(100),
    // NOT the cpu-sorted first row which would be child(200).
    view.select_visible(0);
    view.request_signal(Signal::Term);
    assert_eq!(view.pending_signal, Some((100, Signal::Term)));
    let ctl = RecordingController::default();
    view.confirm_signal(&ctl);
    assert_eq!(ctl.sent(), vec![(100, Signal::Term)]);
}

#[test]
fn maps_required_keyboard_controls() {
    let cases = [
        (KeyCode::Char('q'), Action::Quit),
        // btop: Esc opens the main menu (quit lives inside it and on q).
        (KeyCode::Esc, Action::OpenMenu),
        (KeyCode::Char('?'), Action::ToggleHelp),
        // btop vim_keys layout (mtop default): help on H, h sorts left.
        (KeyCode::Char('H'), Action::ToggleHelp),
        (KeyCode::F(5), Action::RefreshNow),
        (KeyCode::Char('u'), Action::ToggleProcPause),
        (KeyCode::Char(' '), Action::TogglePause),
        (KeyCode::Char('+'), Action::IncreaseInterval),
        (KeyCode::Char('-'), Action::DecreaseInterval),
        (KeyCode::Char('j'), Action::SelectNext),
        (KeyCode::Down, Action::SelectNext),
        (KeyCode::Char('k'), Action::SelectPrevious),
        (KeyCode::Up, Action::SelectPrevious),
    ];
    for (input, expected) in cases {
        assert_eq!(key_to_action(key(input)), Some(expected));
    }
}

#[test]
fn renders_representative_snapshot_and_help_overlay() {
    let snapshot = SystemSnapshot {
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 42.5,
            ..CpuSnapshot::default()
        },
        memory: MemorySnapshot {
            total_bytes: 1024 * 1024 * 1024,
            used_bytes: 512 * 1024 * 1024,
            ..MemorySnapshot::default()
        },
        processes: vec![ProcessSnapshot {
            user: None,
            elapsed_secs: None,
            pid: 7,
            name: "worker".into(),
            cpu_percent: 12.0,
            memory_bytes: 2048,
            status: "running".into(),
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("neon"));
    view.show_help = true;
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for expected in ["42.5%", "Description:", "cpu", "mem", "net", "q", "h"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
}

#[test]
fn renders_dense_graph_labels_and_selected_process_at_wide_size() {
    let snapshot = SystemSnapshot {
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 73.0,
            per_core_percent: vec![20.0, 80.0, 55.0, 91.0],
            frequency_mhz: Some(2400),
            ..CpuSnapshot::default()
        },
        memory: MemorySnapshot {
            total_bytes: 8 * 1024 * 1024 * 1024,
            used_bytes: 4 * 1024 * 1024 * 1024,
            cached_bytes: None,
            swap_total_bytes: 2 * 1024 * 1024 * 1024,
            swap_used_bytes: 256 * 1024 * 1024,
        },
        disks: vec![
            DiskSnapshot {
                name: "disk0".into(),
                mount_point: "/".into(),
                total_bytes: 1000,
                available_bytes: 250,
                read_bytes_per_second: Some(4096),
                write_bytes_per_second: Some(2048),
            },
            DiskSnapshot {
                name: "disk1".into(),
                mount_point: "/Volumes/Data".into(),
                total_bytes: 2000,
                available_bytes: 500,
                read_bytes_per_second: Some(1000),
                write_bytes_per_second: Some(500),
            },
        ],
        networks: vec![
            NetworkSnapshot {
                received_bytes_total: None,
                transmitted_bytes_total: None,
                interface: "en0".into(),
                received_bytes_per_second: 4096,
                transmitted_bytes_per_second: 2048,
            },
            NetworkSnapshot {
                received_bytes_total: None,
                transmitted_bytes_total: None,
                interface: "utun9".into(),
                received_bytes_per_second: 1024,
                transmitted_bytes_per_second: 512,
            },
        ],
        processes: vec![ProcessSnapshot {
            user: None,
            elapsed_secs: None,
            pid: 42,
            name: "selected-worker".into(),
            cpu_percent: 12.0,
            memory_bytes: 2048,
            status: "running".into(),
            threads: None,
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("neon"));
    view.record_snapshot();
    view.show_detail = true; // show detail panel so PID line is visible
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    for expected in [
        "cpu",
        "mem",
        "disks",
        "net",
        "proc",
        "▼",
        "▲",
        "PID",
        "selected-worker",
        "PID 42",
    ] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
    assert!(!text.contains("history"));
}

#[test]
fn cpu_graph_uses_main_area_and_core_rail() {
    let mut view = valid_graph_view();
    view.snapshot.cpu = CpuSnapshot {
        available: true,
        overall_percent: 73.0,
        per_core_percent: vec![20.0, 80.0, 55.0, 91.0],
        ..CpuSnapshot::default()
    };
    view.record_snapshot();
    let text = rendered_text(&view, 120, 40);
    for expected in ["CPU", "C0", "C1", "C2", "C3"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
}

#[test]
fn medium_layout_contains_both_network_and_disk_directions() {
    let view = valid_graph_view();
    let text = rendered_text(&view, 80, 24);
    for expected in ["▼", "▲", "disks"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
    assert!(!text.contains("history"));
}

#[test]
fn small_terminal_render_never_panics_and_shows_compact_hint() {
    let view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Resize terminal") || text.contains("N/A"));
}

#[test]
fn history_is_bounded_and_empty_data_is_explicit() {
    let mut view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    for value in 0..(HISTORY_LIMIT as u64 + 15) {
        let mut snapshot = SystemSnapshot {
            captured_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(value),
            ..SystemSnapshot::default()
        };
        snapshot.cpu.available = true;
        snapshot.cpu.overall_percent = value as f32;
        view.accept_snapshot(snapshot);
    }
    assert_eq!(view.history.cpu.len(), HISTORY_LIMIT);
    assert_eq!(view.history.memory.len(), 0);
    assert!(view.history.cpu.front().is_some());

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("N/A"));
    assert!(text.contains("No processes") || text.contains("N/A"));
}

#[test]
fn unavailable_cpu_renders_na_instead_of_zero_percent() {
    let view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Overall  N/A") || text.contains("CPU N/A"));
    assert!(text.contains("Cores    N/A") || text.contains("N/A"));
}

#[test]
fn unavailable_samples_do_not_create_history_entries() {
    let mut view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    view.record_snapshot();

    assert!(view.history.cpu.is_empty());
    assert!(view.history.memory.is_empty());
    assert!(view.history.net.is_empty());
    assert!(view.history.disk_read.is_empty());
    assert!(view.history.disk_write.is_empty());
}

#[test]
fn stale_history_is_hidden_when_current_metrics_become_unavailable() {
    let valid = SystemSnapshot {
        captured_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1),
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 75.0,
            ..CpuSnapshot::default()
        },
        memory: MemorySnapshot {
            total_bytes: 100,
            used_bytes: 75,
            ..MemorySnapshot::default()
        },
        disks: vec![DiskSnapshot {
            name: "disk0".into(),
            mount_point: "/".into(),
            total_bytes: 100,
            available_bytes: 25,
            read_bytes_per_second: Some(100),
            write_bytes_per_second: Some(50),
        }],
        networks: vec![NetworkSnapshot {
            received_bytes_total: None,
            transmitted_bytes_total: None,
            interface: "en0".into(),
            received_bytes_per_second: 100,
            transmitted_bytes_per_second: 50,
        }],
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(valid, Theme::from_name("mono"));
    view.record_snapshot();

    let unavailable = SystemSnapshot {
        captured_at: std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2),
        ..SystemSnapshot::default()
    };
    assert!(view.accept_snapshot(unavailable));

    let text = rendered_text(&view, 160, 48);
    for expected in ["CPU N/A", "mem", "net"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
}

#[test]
fn valid_histories_render_labeled_rx_tx_and_disk_graphs() {
    let mut view = AppView::new(
        SystemSnapshot {
            cpu: CpuSnapshot {
                available: true,
                overall_percent: 25.0,
                ..CpuSnapshot::default()
            },
            memory: MemorySnapshot {
                total_bytes: 100,
                used_bytes: 25,
                ..MemorySnapshot::default()
            },
            disks: vec![DiskSnapshot {
                name: "disk0".into(),
                mount_point: "/".into(),
                total_bytes: 100,
                available_bytes: 50,
                read_bytes_per_second: Some(100),
                write_bytes_per_second: Some(50),
            }],
            networks: vec![NetworkSnapshot {
                received_bytes_total: None,
                transmitted_bytes_total: None,
                interface: "en0".into(),
                received_bytes_per_second: 100,
                transmitted_bytes_per_second: 50,
            }],
            ..SystemSnapshot::default()
        },
        Theme::from_name("mono"),
    );
    view.record_snapshot();
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    for expected in ["▼", "▲"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
}

#[test]
fn duplicate_captured_at_is_rejected_without_history_growth() {
    let captured_at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(7);
    let mut view = AppView::new(
        SystemSnapshot {
            captured_at,
            cpu: CpuSnapshot {
                available: true,
                overall_percent: 10.0,
                ..CpuSnapshot::default()
            },
            ..SystemSnapshot::default()
        },
        Theme::from_name("mono"),
    );
    view.record_snapshot();
    let before = view.history.cpu.len();

    let accepted = view.accept_snapshot(SystemSnapshot {
        captured_at,
        cpu: CpuSnapshot {
            available: true,
            overall_percent: 90.0,
            ..CpuSnapshot::default()
        },
        ..SystemSnapshot::default()
    });

    assert!(!accepted);
    assert_eq!(view.history.cpu.len(), before);
}

#[test]
fn history_stays_bounded_when_many_valid_snapshots_arrive() {
    let mut view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    for value in 1..=HISTORY_LIMIT + 10 {
        view.accept_snapshot(SystemSnapshot {
            captured_at: std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(value as u64),
            cpu: CpuSnapshot {
                available: true,
                overall_percent: value as f32,
                ..CpuSnapshot::default()
            },
            ..SystemSnapshot::default()
        });
    }
    assert_eq!(view.history.cpu.len(), HISTORY_LIMIT);
}

#[test]
fn valid_metric_graphs_render_at_all_supported_dashboard_sizes() {
    let view = valid_graph_view();
    for (width, height) in [(80, 24), (120, 40), (160, 48)] {
        let text = rendered_text(&view, width, height);
        assert!(text.contains("net") || text.contains("io") || text.contains("▼"));
    }
}

#[test]
fn unavailable_metric_graphs_render_na_instead_of_zero() {
    let text = rendered_text(
        &AppView::new(SystemSnapshot::default(), Theme::from_name("mono")),
        80,
        24,
    );
    for expected in ["N/A", "▼", "download"] {
        assert!(
            text.contains(expected),
            "rendered buffer should contain {expected:?}"
        );
    }
    // ▲ may be clipped at small terminal sizes; "download"/"upload" headers are the btop markers
    assert!(!text.contains("RX 0 B/s") && !text.contains("R 0 B/s"));
}

#[test]
fn edge_size_renders_never_panic() {
    let view = AppView::new(SystemSnapshot::default(), Theme::from_name("mono"));
    for (width, height) in [(40, 8), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    }
}

#[test]
fn rendered_graph_applies_category_color_and_label() {
    use ratatui::style::Color;
    let mut view = valid_graph_view();
    view.theme = Theme::from_name("neon");
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    // btop disk color = c_up_end = #dcafde
    let disk_color = Color::Rgb(0xdc, 0xaf, 0xde);
    assert!(buffer.content().iter().any(|cell| cell.fg == disk_color));
    let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("net"));
}

#[test]
fn neon_theme_uses_near_black_surfaces_and_distinct_semantic_borders() {
    let theme = Theme::from_name("neon");
    assert!(matches!(
        theme.background,
        ratatui::style::Color::Rgb(0..=12, 0..=12, 0..=12)
    ));
    assert!(matches!(
        theme.panel,
        ratatui::style::Color::Rgb(0..=12, 0..=12, 0..=12)
    ));
    assert_ne!(theme.border, theme.good);
    assert_ne!(theme.border, theme.warn);
    assert_ne!(theme.border, theme.violet);
    assert_ne!(theme.border, theme.bad);
}

#[test]
fn proc_title_toggle_boxes_reflect_state() {
    // btop renders toggle buttons with a filled box when active, empty when off.
    let snapshot = SystemSnapshot {
        processes: vec![ProcessSnapshot {
            pid: 1,
            name: "init".into(),
            cpu_percent: 1.0,
            memory_bytes: 1024,
            status: "S".into(),
            user: Some("root".into()),
            elapsed_secs: Some(1),
            threads: Some(1),
            parent_pid: None,
            command: String::new(),
        }],
        ..SystemSnapshot::default()
    };
    let render = |view: &AppView| -> String {
        let mut t = Terminal::new(TestBackend::new(150, 45)).unwrap();
        t.draw(|f| draw_dashboard(f, view)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    };

    // Default: tree off, reverse off -> both boxes empty.
    let mut view = AppView::new(snapshot.clone(), Theme::from_name("mono"));
    let off = render(&view);
    assert!(
        off.contains("tree□"),
        "tree toggle should be empty when off"
    );
    assert!(
        off.contains("reverse□"),
        "reverse toggle should be empty when off"
    );

    // Enable tree and reverse -> boxes filled.
    view.apply_public(Action::ToggleTree);
    view.apply_public(Action::ToggleReverse);
    let on = render(&view);
    assert!(on.contains("tree■"), "tree toggle should be filled when on");
    assert!(
        on.contains("reverse■"),
        "reverse toggle should be filled when on"
    );
}

#[test]
fn proc_action_bar_shows_real_key_hints_not_placeholder() {
    let view = valid_graph_view();
    let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    // The old bar rendered the integral glyph as a bogus key placeholder.
    assert!(
        !text.contains('\u{222b}'),
        "proc action bar must not contain the ∫ placeholder glyph"
    );
    // Real key hints must be present.
    assert!(text.contains("t term"), "expected 't term' hint");
    assert!(text.contains("K kill"), "expected 'K kill' hint");
    assert!(text.contains("s sig"), "expected 's sig' hint");
}

#[test]
fn net_graphs_render_gradient_colors() {
    let mut view = valid_graph_view();
    for _ in 0..HISTORY_LIMIT {
        view.record_snapshot();
    }
    let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    let braille = |c: char| ('\u{2800}'..='\u{28FF}').contains(&c);
    let mut colors = std::collections::HashSet::new();
    // net box is the bottom-left panel; braille rows ~37..43, x from ~5.
    for y in 37..43u16 {
        for x in 5..40u16 {
            let cell = buffer.cell((x, y)).unwrap();
            if cell.symbol().chars().next().is_some_and(braille) {
                colors.insert(cell.fg);
            }
        }
    }
    assert!(
        colors.len() >= 3,
        "net graphs should be gradient-colored, got {}",
        colors.len()
    );
}

#[test]
fn mem_box_renders_history_graph_when_space_remains() {
    // btop shows a memory-usage history graph. With a tall mem box and recorded
    // history, braille glyphs must appear inside the mem panel (left column).
    let snapshot = SystemSnapshot {
        memory: MemorySnapshot {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 8 * 1024 * 1024 * 1024,
            cached_bytes: Some(2 * 1024 * 1024 * 1024),
            swap_total_bytes: 0,
            swap_used_bytes: 0,
        },
        ..SystemSnapshot::default()
    };
    let mut view = AppView::new(snapshot.clone(), Theme::from_name("mono"));
    for i in 0..HISTORY_LIMIT {
        view.snapshot.memory.used_bytes = ((4 + (i % 8)) as u64) * 1024 * 1024 * 1024;
        view.record_snapshot();
    }
    let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    let braille = |c: char| ('\u{2800}'..='\u{28FF}').contains(&c);
    // mem box is the upper-left panel; scan its interior rows/cols.
    let mut found = false;
    for y in 15..30u16 {
        for x in 1..66u16 {
            if buffer
                .cell((x, y))
                .unwrap()
                .symbol()
                .chars()
                .next()
                .is_some_and(braille)
            {
                found = true;
            }
        }
    }
    assert!(found, "mem box should render a braille history graph");
}

#[test]
fn filter_mode_captures_letters_as_text_not_actions() {
    // Regression: while the filter is active, typing letters that also map to
    // actions (e=tree, t=term, s=picker, 2=cores) must land in the filter text,
    // not trigger the action. btop routes all printable keys to the filter.
    let mut snapshot = SystemSnapshot::default();
    snapshot.cpu.available = true;
    snapshot.processes = vec![ProcessSnapshot {
        pid: 1,
        name: "systemd".into(),
        cpu_percent: 1.0,
        memory_bytes: 1024,
        status: "S".into(),
        user: Some("root".into()),
        elapsed_secs: Some(1),
        threads: Some(1),
        parent_pid: None,
        command: String::new(),
    }];
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    view.apply_public(Action::ToggleFilter);
    assert!(view.filter_active);
    // Type "set2e" — every char also has an action binding.
    for c in "set2e".chars() {
        view.feed_key(key(KeyCode::Char(c)));
    }
    assert_eq!(view.filter, "set2e", "all letters must enter the filter");
    assert!(!view.tree, "e must not toggle tree while filtering");
    assert!(view.show_cores, "2 must not toggle cores while filtering");
    assert!(
        view.pending_signal.is_none(),
        "t/s must not stage a signal while filtering"
    );
    // Backspace edits the filter; Enter/Esc exit filter mode.
    view.feed_key(key(KeyCode::Backspace));
    assert_eq!(view.filter, "set2");
    view.feed_key(key(KeyCode::Esc));
    assert!(!view.filter_active, "Esc exits filter mode");
}

#[test]
fn partial_history_fills_graph_width_with_baseline_not_blanks() {
    use mtop::ui::braille_graph;
    use std::collections::VecDeque;
    // Cold start: only a few samples but a wide graph. btop shows a full-width
    // baseline; missing history must extend the OLDEST sample back, not render
    // as blank braille (which left the graph 90% empty with a cluster at right).
    let mut values: VecDeque<u64> = VecDeque::new();
    for v in [20u64, 25, 22, 30, 28] {
        values.push_back(v);
    }
    let width = 120usize;
    let height = 8usize;
    let rows = braille_graph(&values, 100, width, height, false);
    // The BOTTOM row is the clearest signal: with a baseline pad it is drawn
    // across the full width; with the old zero-pad only ~3 rightmost columns
    // were non-blank (the 5 real samples), leaving a near-empty graph.
    let bottom = rows.last().expect("has rows");
    let bottom_drawn = bottom.chars().filter(|&c| c != ' ').count();
    assert!(
        bottom_drawn >= width * 3 / 4,
        "baseline should fill the bottom row across the width, got {bottom_drawn}/{width}"
    );
    // The leftmost column must be non-blank (baseline reaches the left edge).
    let left_col_drawn = rows.iter().any(|r| !r.starts_with(' '));
    assert!(
        left_col_drawn,
        "left edge should show the baseline, not blank"
    );
}

#[test]
fn per_core_toggle_hides_and_shows_core_meters() {
    // Per-core CPU meters toggle (options panel; key 2 now toggles the mem
    // box like btop). When hidden, no "C0"/"C1" rows.
    let mut snapshot = SystemSnapshot::default();
    snapshot.cpu.available = true;
    snapshot.cpu.overall_percent = 40.0;
    snapshot.cpu.per_core_percent = vec![10.0, 20.0, 30.0, 40.0];
    let render = |view: &AppView| -> String {
        let mut t = Terminal::new(TestBackend::new(150, 45)).unwrap();
        t.draw(|f| draw_dashboard(f, view)).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    };
    let mut view = AppView::new(snapshot, Theme::from_name("mono"));
    // Default: cores shown.
    assert!(view.show_cores, "cores shown by default");
    assert!(
        render(&view).contains("C0"),
        "expected C0 core row when shown"
    );
    // '2' is the btop mem-box toggle, not per-core.
    assert_eq!(
        key_to_action(key(KeyCode::Char('2'))),
        Some(Action::ToggleBox(2))
    );
    view.apply_public(Action::ToggleCores);
    assert!(!view.show_cores, "cores hidden after toggle");
    assert!(
        !render(&view).contains("C0"),
        "C0 core row must be gone when hidden"
    );
}

#[test]
fn meter_cell_color_depends_on_position_not_value() {
    use mtop::ui::{cpu_gradient, meter_spans};
    // At 60% fill over width 20, the 12th filled cell (i=12) must carry the
    // gradient color for its POSITION (12/20 -> 60%), independent of value.
    let spans = meter_spans(60.0, 20, cpu_gradient);
    // 60% of 20 = 12 filled cells.
    let twelfth = &spans[11];
    let expected = cpu_gradient(12.0 * 100.0 / 20.0); // grad(60.0)
    assert_eq!(
        twelfth.style.fg,
        Some(expected),
        "meter cell color must be gradient-at-position, not scaled by value"
    );
}

#[test]
fn cpu_graph_has_vertical_gradient_not_two_flat_blocks() {
    // btop colors the CPU graph bottom(green)->top(red). A full history at a
    // varied level must produce MANY distinct row colors, not just 2 blocks.
    let mut view = valid_graph_view();
    view.snapshot.cpu.available = true;
    for i in 0..HISTORY_LIMIT {
        // vary the level so the graph has body across its full height
        view.snapshot.cpu.overall_percent = 20.0 + (i % 60) as f32;
        view.record_snapshot();
    }
    let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    // Collect distinct fg colors of braille cells in the CPU graph region
    // (left of the rail; rows 2..=10 inside the cpu box).
    let braille = |c: char| ('\u{2800}'..='\u{28FF}').contains(&c);
    let mut colors = std::collections::HashSet::new();
    for y in 2..11u16 {
        for x in 1..80u16 {
            let cell = buffer.cell((x, y)).unwrap();
            if cell.symbol().chars().next().is_some_and(braille) {
                colors.insert(cell.fg);
            }
        }
    }
    assert!(
        colors.len() >= 4,
        "CPU graph should show a multi-step vertical gradient, got {} colors",
        colors.len()
    );
}

#[test]
fn cpu_graph_fills_full_width_when_history_is_full() {
    // Regression: with a short HISTORY_LIMIT the braille graph only covered the
    // right ~half of the box, leaving the left blank. History must be long
    // enough to fill the widest graph area (2 samples per column).
    let mut view = valid_graph_view();
    view.snapshot.cpu.available = true;
    view.snapshot.cpu.overall_percent = 80.0;
    for _ in 0..HISTORY_LIMIT {
        view.record_snapshot();
    }
    let mut terminal = Terminal::new(TestBackend::new(150, 45)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    // The CPU graph occupies the left of the cpu box, row 3 (inside border,
    // below the chip line which is only in the right rail). Column 2 is well
    // inside the graph area; with a full history it must carry a braille glyph,
    // not a blank space.
    let braille = |c: char| ('\u{2800}'..='\u{28FF}').contains(&c);
    let mut found = false;
    for y in 2..11u16 {
        let sym = buffer.cell((2, y)).unwrap().symbol();
        if sym.chars().next().is_some_and(braille) {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "CPU graph must render braille at the left edge (col 2) when history is full"
    );
}

#[test]
fn wide_layout_has_btop_geometry_and_semantic_panel_borders() {
    use ratatui::style::Color;
    let mut view = valid_graph_view();
    view.snapshot.cpu.available = true;
    view.snapshot.cpu.overall_percent = 50.0;
    for _ in 0..HISTORY_LIMIT {
        view.record_snapshot();
    }
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    // btop Default_theme border colors (hardcoded in renderer):
    let c_cpu = Color::Rgb(0x55, 0x6d, 0x59); // cpu_box
    let c_mem = Color::Rgb(0x6c, 0x6c, 0x4b); // mem_box
    let c_proc = Color::Rgb(0x80, 0x52, 0x52); // proc_box
    // CPU box top-left corner
    assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "╭");
    assert_eq!(buffer.cell((0, 0)).unwrap().fg, c_cpu);
    // calc cpu_h for 120x40 terminal: body height=39, 39*32/100=12, clamped max(8)=12
    let cpu_h: u16 = 12; // (39*32/100=12, >=8 ok)
    let mem_y = cpu_h;
    assert_eq!(buffer.cell((0, mem_y)).unwrap().symbol(), "╭");
    assert_eq!(buffer.cell((0, mem_y)).unwrap().fg, c_mem);
    // proc right column corner: 45% of 120 = 54
    assert_eq!(buffer.cell((54, mem_y)).unwrap().symbol(), "╭");
    assert_eq!(buffer.cell((54, mem_y)).unwrap().fg, c_proc);
    assert_eq!(buffer.cell((0, 38)).unwrap().symbol(), "╰");
    // CPU graph uses btop cpu_start color #77ca9b
    let c_graph = Color::Rgb(0x77, 0xca, 0x9b);
    let cpu_graph_cells = (0u16..84)
        .flat_map(|x| (2u16..cpu_h.saturating_sub(1)).map(move |y| (x, y)))
        .filter(|pos| buffer.cell(*pos).unwrap().fg == c_graph)
        .count();
    assert!(
        cpu_graph_cells > 0,
        "CPU graph should have #77ca9b colored cells"
    );
    assert!(
        (84..120).any(|x: u16| {
            (1..cpu_h).any(|y| buffer.cell((x, y)).unwrap().symbol().contains('C'))
        })
    );
    // proc box border color #805252 should appear in right column
    assert!(
        (54..120)
            .any(|x: u16| { (cpu_h..38u16).any(|y| buffer.cell((x, y)).unwrap().fg == c_proc) })
    );
}

#[test]
fn medium_layout_keeps_cpu_left_io_and_process_right() {
    use ratatui::style::Color;
    let view = valid_graph_view();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| draw_dashboard(frame, &view)).unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    // btop cpu_box color
    let c_cpu = Color::Rgb(0x55, 0x6d, 0x59);
    let c_proc = Color::Rgb(0x80, 0x52, 0x52);
    assert_eq!(buffer.cell((0, 0)).unwrap().fg, c_cpu);
    assert!(text.contains("cpu") && text.contains("mem") && text.contains("proc"));
    assert!(
        text.contains("▼") && text.contains("▲") && text.contains("disks") && text.contains("proc")
    );
    // proc border somewhere in right column around y=8+ on 24-row terminal
    assert!((40..80).any(|x: u16| (8..23u16).any(|y| buffer.cell((x, y)).unwrap().fg == c_proc)));
    assert!(!text.contains("history"));
}
