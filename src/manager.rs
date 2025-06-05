use crate::manager::ManagerStatus::{Running, Shutdown, Stopped};
use crate::task::{Status, Task};
use anyhow::{Result, anyhow};
use std::cmp::PartialEq;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, RwLock};
use std::{fs, sync::Arc, thread};
use tokio::time::Instant;
use tracing::{info, warn};

#[derive(Debug, PartialEq, Copy, Clone, Default)]
enum ManagerStatus {
    Running,
    #[default]
    Stopped,
    Shutdown,
}

#[derive(Debug)]
pub struct TaskManager {
    task_id: AtomicUsize,
    // wait when stopped
    status: Arc<Mutex<ManagerStatus>>,

    // ready task
    ready_q: Arc<RwLock<VecDeque<Arc<RwLock<Task>>>>>,
    worker: Option<thread::JoinHandle<Result<()>>>,

    // condvar
    condvar: Arc<Condvar>,
}
impl Default for TaskManager {
    fn default() -> Self {
        TaskManager::new()
    }
}

impl TaskManager {
    pub fn new() -> TaskManager {
        let status = Arc::new(Mutex::new(Stopped));
        let ready_q = Arc::new(RwLock::new(VecDeque::<Arc<RwLock<Task>>>::new()));
        let condvar = Arc::new(Condvar::new());

        let t_status = status.clone();
        let t_ready_q = ready_q.clone();
        let t_condvar = condvar.clone();
        let worker = thread::spawn(move || -> Result<()> {
            loop {
                {
                    let mut status_guard = t_status.lock().unwrap();
                    if *status_guard == Shutdown {
                        break;
                    }
                    while *status_guard == Stopped
                        || (*status_guard == Running && t_ready_q.read().unwrap().len() == 0)
                    {
                        status_guard = t_condvar.wait(status_guard).unwrap();
                    }
                    if *status_guard == Shutdown {
                        break;
                    }
                }
                // *status must be running
                let task_arc = t_ready_q.write().unwrap().front().unwrap().clone();
                let task_value;
                {
                    let mut task = task_arc.write().unwrap();
                    task.status = Status::Running;
                    task_value = task.clone();
                }
                info!("task {} : {} is RUNNING !", task_value.id, task_value.name);

                if task_value.test {
                    TaskManager::task_test_run(&task_value)?;
                } else {
                    todo!()
                }
                info!("task {} : {} is DONE !", task_value.id, task_value.name);

                t_ready_q.write().unwrap().pop_front();
            }
            Ok(())
        });
        TaskManager {
            status,
            ready_q,
            condvar,
            worker: Some(worker),
            task_id: AtomicUsize::new(0),
        }
    }
    pub fn submit_task(&self, task: Task) -> Result<()> {
        let task_arc = Arc::new(RwLock::new(task));
        let status = *self.status.lock().unwrap().deref();

        match status {
            Running | Stopped => {
                self.ready_q.write().unwrap().push_back(task_arc.clone());
                self.condvar.notify_all();

                Ok(())
            }
            Shutdown => Err(anyhow!(
                "Talps-Manager is Shutdown , cannot submit task anymore"
            )),
        }
    }
    pub fn submit(&self, task_name: String, exec_file: String) -> Result<()> {
        let task = Task {
            id: self.task_id.load(Ordering::Relaxed),
            name: task_name,
            status: Status::Pending,
            cmd: exec_file,
            test: false,
            timestamp: Instant::now(),
        };
        self.submit_task(task)
    }
    pub fn len(&self) -> usize {
        self.ready_q.read().unwrap().len()
    }

    pub fn run(&self) -> Result<()> {
        let status = *self.status.lock().unwrap();
        match status {
            Running => {
                info!("Status has been Running already , no need to run again");
                Ok(())
            }
            Stopped => {
                info!("Talps-Manager start to run");
                self.condvar.notify_all();
                *self.status.lock().expect("Mutex Poisoned") = Running;
                Ok(())
            }
            Shutdown => {
                info!("Status is shutdown , cannot run anymore");
                Err(anyhow!("Status is shutdown , cannot run anymore"))
            }
        }
    }
    pub fn stop(&mut self) -> Result<()> {
        let status = *self.status.lock().unwrap();
        match status {
            Running => {
                *self.status.lock().expect("Mutex Poisoned") = Stopped;
                Ok(())
            }
            Stopped => {
                info!("Status has been Stopped already , no need to stop again");
                Ok(())
            }
            Shutdown => {
                info!("Status is shutdown , no need to stop");
                Err(anyhow!("Status is shutdown , no need to stop"))
            }
        }
    }
    pub fn shutdown(&mut self) -> Result<()> {
        let status = *self.status.lock().unwrap();
        match status {
            Running | Stopped => {
                *self.status.lock().expect("Mutex Poisoned") = Shutdown;
                match self.worker.take() {
                    Some(worker) => {
                        self.condvar.notify_all();
                        worker.join().unwrap().unwrap();
                    }
                    None => panic!("JoinHandle must be some in this case"),
                }
                Ok(())
            }
            Shutdown => {
                info!("Status is shutdown , no need to shutdown again");
                Err(anyhow!("Status is shutdown , no need to shutdown again"))
            }
        }
    }
    fn task_test_run(task: &Task) -> Result<()> {
        let path = Path::new(&task.cmd);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{:?}", task))?;
        Ok(())
    }
    pub fn show_tasks(&self) -> Vec<String> {
        let mut vec = Vec::with_capacity(self.len());
        for task in self.ready_q.read().unwrap().iter() {
            vec.push(format!("{:?}", task.read().unwrap()))
        }
        vec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Status;
    use std::fs::{read_dir, remove_dir_all};
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn stop_status_test() {
        {
            let mut manager = TaskManager::new();
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Stopped);

            {
                manager.run().expect("Run Err");
                let status = *manager.status.lock().unwrap();
                // run
                assert_eq!(status, Running);
            }
            {
                manager.stop().expect("Run Err");
                let status = *manager.status.lock().unwrap();
                // stop after run
                assert_eq!(status, Stopped);
            }
            {
                manager.run().expect("Run Err");
                let mut status = *manager.status.lock().unwrap();
                // run
                assert_eq!(status, Running);

                manager.stop().expect("Run Err");
                status = *manager.status.lock().unwrap();
                // stop after run
                assert_eq!(status, Stopped);

                manager.shutdown().expect("Shutdown Err");
                status = *manager.status.lock().unwrap();
                // shutdown after stop
                assert_eq!(status, Shutdown);

                manager.run().expect_err("Run after Shutdown Err");
                status = *manager.status.lock().unwrap();
                // run
                assert_eq!(status, Shutdown);
            }
        }
    }
    #[test]
    fn stop_test() {
        let mut manager = TaskManager::new();
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 100;
        for i in 0..n {
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                status: Status::Pending,
                cmd: "wwt".to_string(),
                test: true,
                timestamp: Instant::now(),
            };
            manager.submit_task(task).unwrap();
        }
        assert_eq!(manager.ready_q.read().unwrap().len(), n);
        {
            manager.stop().expect("Stop Err");
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Stopped);
        }
        {
            manager.shutdown().expect("Shutdown Err");
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Shutdown);
        }
    }
    #[test]
    fn run_test() {
        let _ = remove_dir_all("./run_test");
        let mut manager = TaskManager::new();
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 100;
        for i in 0..n {
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                status: Status::Pending,
                cmd: format!("./run_test/{}", i),
                test: true,
                timestamp: Instant::now(),
            };
            manager.submit_task(task).unwrap();
        }
        {
            assert_eq!(manager.show_tasks().len(), n);
        }

        manager.run().expect("Running Err");
        {
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Running);
        }
        sleep(Duration::from_secs(5));

        let dir = read_dir("./run_test").expect("output folder");
        assert_eq!(dir.count(), n);

        remove_dir_all("./run_test").unwrap();
        {
            assert_eq!(manager.show_tasks().len(), 0);
        }

        {
            manager.stop().expect("Stop Err");
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Stopped);
        }
        {
            manager.shutdown().expect("Shutdown Err");
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Shutdown);
        }
        {
            manager.stop().expect_err("Stop After Shutdown");
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Shutdown);
        }
    }
}
