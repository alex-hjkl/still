use anyhow::{Result, bail};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            bail!("Loading canceled");
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Requests {
    id: u64,
    cancellation: Cancellation,
}

impl Requests {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn start(&mut self) -> (u64, Cancellation) {
        self.invalidate();
        self.cancellation = Cancellation::default();
        (self.id, self.cancellation.clone())
    }

    pub fn invalidate(&mut self) {
        self.cancellation.cancel();
        self.id = self.id.wrapping_add(1);
    }

    pub fn is_current(&self, id: u64) -> bool {
        self.id == id
    }
}

impl Drop for Requests {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_or_dropping_a_request_cancels_its_worker() {
        let mut requests = Requests::default();
        let (first_id, first) = requests.start();
        let (second_id, second) = requests.start();
        assert!(first.check().is_err());
        assert!(!requests.is_current(first_id));
        assert!(requests.is_current(second_id));
        assert!(second.check().is_ok());
        drop(requests);
        assert!(second.check().is_err());
    }
}
