use std::sync::Arc;

use parking_lot::Mutex;

use super::SnapshotId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TestQueryPoint {
    BeforeUpgradableLookup,
    WaiterEnrolled,
    WaitCompleted,
}

#[derive(Default)]
pub(super) struct TestQueryHooks {
    pub(super) callback: Mutex<Option<Arc<dyn Fn(SnapshotId, TestQueryPoint) + Send + Sync>>>,
}

impl TestQueryHooks {
    pub(super) fn run(&self, id: SnapshotId, point: TestQueryPoint) {
        let callback = self.callback.lock().clone();
        if let Some(callback) = callback {
            callback(id, point);
        }
    }
}
