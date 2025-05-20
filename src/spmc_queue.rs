use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

#[derive(Debug, Default)]
pub struct SPMCQueue<T: Send + Sync + 'static> {
    pub block_queue: Mutex<VecDeque<T>>,
    cond_var: Condvar,
}
impl<T: Send + Sync + 'static> SPMCQueue<T> {
    pub fn new() -> Self {
        Self {
            block_queue: Mutex::new(VecDeque::new()),
            cond_var: Condvar::new(),
        }
    }
    pub fn push(&self, t: T) -> Result<()> {
        let mut queue = self
            .block_queue
            .lock()
            .map_err(|_| anyhow!("Mutex poisoned"))?;
        queue.push_back(t);
        self.cond_var.notify_one();
        Ok(())
    }
    pub fn pop(&self) -> Result<T> {
        let mut queue = self
            .block_queue
            .lock()
            .map_err(|_| anyhow!("Mutex poisoned"))?;
        while queue.is_empty() {
            queue = self
                .cond_var
                .wait(queue)
                .map_err(|_| anyhow!("Mutex poisoned"))?;
        }
        match queue.pop_front() {
            Some(t) => Ok(t),
            None => Err(anyhow!("Queue is empty")),
        }
    }
}
