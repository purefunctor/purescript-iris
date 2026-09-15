//! Deterministic execution gates for the command-sequence suite. Not enabled in production.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Point {
    BeforePreparation,
    BeforeAcknowledgement,
    BeforeAnalysis,
    SnapshotActive,
    BeforeReplySettlement,
    AfterReplySettlement,
    BeforeDiagnostics,
    AfterDiagnostics,
    Idle,
    BeforeFailure,
}

#[derive(Clone, Default)]
pub struct Hooks {
    #[cfg(feature = "test-support")]
    gates: std::sync::Arc<parking_lot::Mutex<std::collections::BTreeMap<Point, Gate>>>,
}

#[cfg(feature = "test-support")]
struct Gate {
    entered: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(feature = "test-support")]
pub struct Pause {
    entered: Option<tokio::sync::oneshot::Receiver<()>>,
    release: std::sync::mpsc::Sender<()>,
}

impl Hooks {
    #[cfg(feature = "test-support")]
    pub fn pause_next(&self, point: Point) -> Pause {
        let (entered, receiver) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        assert!(
            self.gates.lock().insert(point, Gate { entered, release: wait }).is_none(),
            "gate already installed"
        );
        Pause { entered: Some(receiver), release }
    }

    pub(crate) fn reach(&self, point: Point) {
        #[cfg(feature = "test-support")]
        {
            let gate = self.gates.lock().remove(&point);
            if let Some(gate) = gate {
                let _ = gate.entered.send(());
                let _ = gate.release.recv();
            }
        }
        #[cfg(not(feature = "test-support"))]
        let _ = point;
    }
}

#[cfg(feature = "test-support")]
impl Pause {
    pub async fn entered(&mut self) {
        self.entered
            .take()
            .expect("gate already awaited")
            .await
            .expect("worker exited before gate");
    }
}

#[cfg(feature = "test-support")]
impl Drop for Pause {
    fn drop(&mut self) {
        let _ = self.release.send(());
    }
}
