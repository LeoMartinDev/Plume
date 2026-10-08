use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type Abort = Box<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<Control>);

#[derive(Default)]
struct Control {
    cancelled: AtomicBool,
    aborts: Mutex<Vec<Abort>>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        for abort in self.0.aborts.lock().unwrap().iter() {
            abort();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub fn on_cancel(&self, abort: impl Fn() + Send + Sync + 'static) {
        let mut aborts = self.0.aborts.lock().unwrap();
        if self.is_cancelled() {
            abort();
        }
        aborts.push(Box::new(abort));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_after_cancellation_also_aborts() {
        let token = CancellationToken::default();
        token.cancel();
        let called = Arc::new(AtomicBool::new(false));
        let observed = called.clone();
        token.on_cancel(move || {
            observed.store(true, Ordering::Release);
        });
        assert!(called.load(Ordering::Acquire));
        assert!(token.clone().is_cancelled());
    }
}
