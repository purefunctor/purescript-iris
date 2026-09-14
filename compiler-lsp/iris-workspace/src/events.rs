use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::{Delivery, Event};

/// One consumer of workspace publications. Status, progress and per-URI diagnostics coalesce;
/// terminal outcomes and rejected inputs remain ordered and must be drained by the consumer.
pub struct EventReceiver {
    pending: Arc<Mutex<VecDeque<Delivery<Event>>>>,
    wake: mpsc::Receiver<()>,
}

pub(crate) struct EventSender {
    pending: Arc<Mutex<VecDeque<Delivery<Event>>>>,
    wake: mpsc::Sender<()>,
}

impl EventSender {
    pub(crate) fn channel() -> (EventSender, EventReceiver) {
        let pending = Arc::new(Mutex::new(VecDeque::new()));
        let (sender, receiver) = mpsc::channel(1);
        (
            EventSender { pending: Arc::clone(&pending), wake: sender },
            EventReceiver { pending, wake: receiver },
        )
    }

    pub(crate) fn send(&self, delivery: Delivery<Event>) {
        if self.wake.is_closed() {
            return;
        }
        let mut pending = self.pending.lock();
        pending.retain(|previous| !replaces(&delivery.value, &previous.value));
        pending.push_back(delivery);
        drop(pending);
        let _ = self.wake.try_send(());
    }
}

impl EventReceiver {
    pub async fn recv(&mut self) -> Option<Delivery<Event>> {
        loop {
            if let Some(event) = self.pending.lock().pop_front() {
                return Some(event);
            }
            self.wake.recv().await?;
        }
    }
}

fn replaces(next: &Event, previous: &Event) -> bool {
    match (next, previous) {
        (Event::StatusChanged(_), Event::StatusChanged(_)) => true,
        (Event::Diagnostics { uri: next, .. }, Event::Diagnostics { uri: previous, .. }) => {
            next == previous
        }
        (
            Event::Progress { generation: next, .. },
            Event::Progress { generation: previous, .. },
        ) => next == previous,
        _ => false,
    }
}
