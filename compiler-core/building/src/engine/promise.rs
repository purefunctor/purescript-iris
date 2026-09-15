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
    Dead,
}

impl<T> State<T> {
    fn is_empty(&self) -> bool {
        matches!(self, State::Empty)
    }
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
    fulfilled: bool,
}

pub(crate) enum WaitResult<T> {
    Fulfilled(T),
    Abandoned,
}

impl<T> Future<T> {
    /// Create a new [`Future`] + [`Promise`] pair.
    pub(crate) fn new() -> (Future<T>, Promise<T>) {
        let slot = Arc::new(Slot::new());
        let future = Future { slot: Arc::clone(&slot) };
        let promise = Promise { slot: Arc::clone(&slot), fulfilled: false };
        (future, promise)
    }

    /// Block and wait for a value to be available.
    ///
    /// # Panics
    ///
    /// This function will panic if the corresponding [`Promise`] is fulfilled
    /// as [`State::Empty`], which is an invariant violation. [`Promise::fulfill`]
    /// and the [`Drop`] implementation guarantees that the [`Future`] always
    /// sees either [`State::Full`] or [`State::Dead`].
    pub(crate) fn wait(self) -> WaitResult<T> {
        let mut guard = self.slot.state.lock();
        if guard.is_empty() {
            self.slot.condvar.wait(&mut guard);
        }
        match mem::replace(&mut *guard, State::Dead) {
            State::Empty => {
                unreachable!("invariant violated: Promise fulfilled with State::Empty")
            }
            State::Full(value) => WaitResult::Fulfilled(value),
            State::Dead => WaitResult::Abandoned,
        }
    }
}

impl<T> Promise<T> {
    pub(crate) fn fulfill(mut self, value: T) {
        self.fulfilled = true;
        self.transition(State::Full(value));
    }

    fn transition(&mut self, state: State<T>) {
        let mut guard = self.slot.state.lock();
        *guard = state;
        self.slot.condvar.notify_all();
    }
}

impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        if !self.fulfilled {
            self.transition(State::Dead);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Future, WaitResult};

    #[test]
    fn test_future_promise() {
        let (future, promise) = Future::new();
        promise.fulfill(123);
        assert!(matches!(future.wait(), WaitResult::Fulfilled(123)));
    }

    #[test]
    fn test_future_promise_is_dead() {
        let (future, _) = Future::<()>::new();
        assert!(matches!(future.wait(), WaitResult::Abandoned));
    }

    #[test]
    fn test_future_promise_dies() {
        let (future, _) = Future::<()>::new();
        std::thread::spawn(move || {
            assert!(matches!(future.wait(), WaitResult::Abandoned));
        });
    }

    #[test]
    fn test_future_promise_fulfills() {
        let (future, promise) = Future::<()>::new();
        std::thread::spawn(|| promise.fulfill(()));
        std::thread::spawn(move || assert!(matches!(future.wait(), WaitResult::Fulfilled(()))));
    }
}
