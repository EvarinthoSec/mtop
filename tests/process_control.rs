use mtop::process_control::{ProcessController, RecordingController, Signal};

#[test]
fn recording_controller_captures_signal_requests() {
    let ctl = RecordingController::default();
    ctl.send(1234, Signal::Term).unwrap();
    ctl.send(1234, Signal::Kill).unwrap();
    assert_eq!(ctl.sent(), vec![(1234, Signal::Term), (1234, Signal::Kill)]);
}

#[test]
fn signal_maps_to_posix_numbers() {
    assert_eq!(Signal::Term.number(), 15);
    assert_eq!(Signal::Kill.number(), 9);
    assert_eq!(Signal::Int.number(), 2);
}

#[test]
fn signal_round_trips_through_number() {
    // Named signals resolve to their canonical variant.
    assert_eq!(Signal::from_number(15), Some(Signal::Term));
    assert_eq!(Signal::from_number(9), Some(Signal::Kill));
    // Any other valid signal becomes Raw and keeps its number + label.
    let s = Signal::from_number(19).unwrap();
    assert_eq!(s, Signal::Raw(19));
    assert_eq!(s.number(), 19);
    assert_eq!(s.label(), "SIG19");
    // Out-of-range numbers are rejected.
    assert_eq!(Signal::from_number(0), None);
    assert_eq!(Signal::from_number(65), None);
}
