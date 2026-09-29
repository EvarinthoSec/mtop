use crate::model::SystemSnapshot;

/// Port implemented by platform adapters that can sample system state.
pub trait SnapshotProvider: Send {
    fn collect(&mut self) -> SystemSnapshot;
}
