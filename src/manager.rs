use crate::manager::ManagerStatus::{Running, Shutdown, Stopped};
use crate::task::{Status, Task};
use anyhow::{Result, anyhow};
use std::cmp::PartialEq;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, RwLock};
use std::{fs, sync::Arc, thread};
use tracing::info;

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

    // all task will be record on this map
    task_map: Arc<RwLock<BTreeMap<usize, Arc<RwLock<Task>>>>>,

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
        let task_map: Arc<RwLock<BTreeMap<usize, Arc<RwLock<Task>>>>> =
            Arc::new(RwLock::new(BTreeMap::new()));
        let condvar = Arc::new(Condvar::new());

        let t_status = status.clone();
        let t_ready_q = ready_q.clone();
        let t_task_map = task_map.clone();
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
                let mut q_guard = t_ready_q.write().unwrap();
                let task_guard = q_guard.pop_front().unwrap();
                let task = task_guard.write().unwrap();
                info!("task {} : {} is RUNNING !", task.id, task.name);

                if task.test {
                    TaskManager::task_test_run(&task)?;
                } else {
                    todo!()
                }
                info!("task {} : {} is DONE !", task.id, task.name);

                let mut map = t_task_map.write().unwrap();
                map.remove(&task.id);
            }
            Ok(())
        });
        TaskManager {
            status,
            ready_q,
            task_map,
            condvar,
            worker: Some(worker),
            task_id: AtomicUsize::new(0),
        }
    }
    pub fn submit_task(&mut self, task: Task) -> Result<()> {
        let id = task.id;
        let task_arc = Arc::new(RwLock::new(task));
        let status = *self.status.lock().unwrap().deref();

        match status {
            Running | Stopped => {
                self.ready_q.write().unwrap().push_back(task_arc.clone());
                self.task_map.write().unwrap().insert(id, task_arc);
                self.condvar.notify_all();

                Ok(())
            }
            Shutdown => Err(anyhow!(
                "Talps-Manager is Shutdown , cannot submit task anymore"
            )),
        }
    }
    pub fn submit(&mut self, task_name: String, deps: Vec<usize>, exec_file: String) -> Result<()> {
        let task = Task {
            id: self.task_id.load(Ordering::Relaxed),
            name: task_name,
            status: Status::Pending,
            file_name: exec_file,
            test: false,
        };
        self.submit_task(task)
    }

    pub fn run(&mut self) -> Result<()> {
        let status = *self.status.lock().unwrap();
        match status {
            Running => {
                info!("Status has been Running already , no need to run again");
                Ok(())
            }
            Stopped => {
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
                    Some(worker) => worker.join().unwrap().unwrap(),
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
        let path = Path::new(&task.file_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{:?}", task))?;
        Ok(())
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
                file_name: "wwt".to_string(),
                test: true,
            };
            manager.submit_task(task).unwrap();
        }
        assert_eq!(manager.task_map.read().unwrap().len(), n);
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
                file_name: format!("./run_no_dep/{}", i),
                test: true,
            };
            manager.submit_task(task).unwrap();
        }
        manager.run().expect("Running Err");
        {
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Running);
        }
        sleep(Duration::from_secs(5));

        let dir = read_dir("./run_no_dep").expect("output folder");
        assert_eq!(dir.count(), n);
        assert_eq!(manager.task_map.read().unwrap().len(), 0);

        remove_dir_all("./run_no_dep").unwrap();

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
