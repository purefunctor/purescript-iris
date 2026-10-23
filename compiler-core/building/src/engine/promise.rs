use std::mem;
use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

#[derive(Debug)]
struct Slot<T> {
    state: Mutex<State<T>>,
    condvar: Condvar,
}

impl<T> Slot<T> {
    fn new() -> Slot<T> {
        let state = Mutex::new(State::Empty);
        let condvar = Condvar::new();
        Slot { state, condvar }
    }
}

#[derive(Debug)]
enum State<T> {
    Empty,
    Full(T),
    Transferred,
    Dead,
}

impl<T> State<T> {
    fn is_empty(&self) -> bool {
        matches!(self, State::Empty)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolution<T> {
    Fulfilled(T),
    Transferred,
    Abandoned,
}

/// The read side of an asynchronous computation.
#[derive(Debug)]
pub(crate) struct Future<T> {
    slot: Arc<Slot<T>>,
}

/// The write side of an asynchronous computation.
#[derive(Debug)]
pub(crate) struct Promise<T> {
    slot: Arc<Slot<T>>,
    resolved: bool,
}

impl<T> Future<T> {
    /// Create a new [`Future`] + [`Promise`] pair.
    pub(crate) fn new() -> (Future<T>, Promise<T>) {
        let slot = Arc::new(Slot::new());
        let future = Future { slot: Arc::clone(&slot) };
        let promise = Promise { slot: Arc::clone(&slot), resolved: false };
        (future, promise)
    }

    /// Block and wait for a value to be available.
    ///
    /// # Panics
    ///
    /// This function will panic if the corresponding [`Promise`] is fulfilled
    /// as [`State::Empty`], which is an invariant violation. [`Promise::fulfill`]
    /// and the [`Drop`] implementation guarantees that the [`Future`] always
    /// sees either [`State::Full`], [`State::Transferred`], or [`State::Dead`].
    pub(crate) fn wait(self) -> Resolution<T> {
        let mut guard = self.slot.state.lock();
        if guard.is_empty() {
            self.slot.condvar.wait(&mut guard);
        }
        match mem::replace(&mut *guard, State::Dead) {
            State::Empty => {
                unreachable!("invariant violated: Promise fulfilled with State::Empty")
            }
            State::Full(value) => Resolution::Fulfilled(value),
            State::Transferred => Resolution::Transferred,
            State::Dead => Resolution::Abandoned,
        }
    }
}

impl<T> Promise<T> {
    pub(crate) fn fulfill(mut self, value: T) {
        self.resolved = true;
        self.transition(State::Full(value));
    }

    pub(crate) fn transfer(mut self) {
        self.resolved = true;
        self.transition(State::Transferred);
    }

    fn transition(&mut self, state: State<T>) {
        let mut guard = self.slot.state.lock();
        *guard = state;
        self.slot.condvar.notify_all();
    }
}

impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        if !self.resolved {
            self.transition(State::Dead);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Future, Resolution};

    #[test]
    fn test_future_promise() {
        let (future, promise) = Future::new();
        promise.fulfill(123);
        assert_eq!(future.wait(), Resolution::Fulfilled(123));
    }

    #[test]
    fn test_future_promise_transfers() {
        let (future, promise) = Future::<()>::new();
        promise.transfer();
        assert_eq!(future.wait(), Resolution::Transferred);
    }

    #[test]
    fn test_future_promise_is_dead() {
        let (future, _) = Future::<()>::new();
        assert_eq!(future.wait(), Resolution::Abandoned);
    }

    #[test]
    fn test_future_promise_dies() {
        let (future, _) = Future::<()>::new();
        std::thread::spawn(move || {
            assert_eq!(future.wait(), Resolution::Abandoned);
        });
    }

    #[test]
    fn test_future_promise_fulfills() {
        let (future, promise) = Future::<()>::new();
        std::thread::spawn(|| promise.fulfill(()));
        std::thread::spawn(move || assert_eq!(future.wait(), Resolution::Fulfilled(())));
    }
}
