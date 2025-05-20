use crate::manager::ManagerStatus::{Running, Shutdown, Stopped};
use crate::spmc_queue::SPMCQueue;
use crate::task::Task;
use anyhow::{anyhow, Result};
use std::cmp::PartialEq;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::AtomicU32;
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
    task_id: AtomicU32,
    // wait when stopped
    condvar: Arc<Condvar>,
    status: Arc<Mutex<ManagerStatus>>,

    // ready task
    ready_q: Arc<SPMCQueue<Arc<RwLock<Task>>>>,
    degree_of_parallel: usize,
    threads: Mutex<Vec<thread::JoinHandle<Result<()>>>>,

    // all task will be record on this map
    task_map: Arc<Mutex<BTreeMap<usize, Arc<RwLock<Task>>>>>,
}
impl Default for TaskManager {
    fn default() -> Self {
        TaskManager::new(10)
    }
}

impl TaskManager {
    pub fn new(degree_of_parallel: usize) -> TaskManager {
        let mut threads = Vec::with_capacity(degree_of_parallel);
        let status = Arc::new(Mutex::new(Stopped));
        let ready_q = Arc::new(SPMCQueue::new());
        let condvar = Arc::new(Condvar::new());
        let task_map: Arc<Mutex<BTreeMap<usize, Arc<RwLock<Task>>>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        for _ in 0..degree_of_parallel {
            let status = status.clone();
            let ready_q = ready_q.clone();
            let condvar = condvar.clone();
            let task_map = task_map.clone();
            threads.push(thread::spawn(move || -> Result<()> {
                let status = status;
                let ready_q = ready_q;
                let condvar = condvar;
                let task_map = task_map;
                let mut status_v = *status.lock().unwrap();
                while status_v != Shutdown {
                    while status_v == Stopped {
                        status_v = *condvar.wait(status.lock().unwrap()).unwrap();
                    }

                    if status_v == Shutdown {
                        break;
                    }
                    let task: Arc<RwLock<Task>> = ready_q.pop()?;
                    let task = task.read().unwrap().clone();
                    info!("task : {} is RUNNING !", task.id);

                    if task.test {
                        TaskManager::task_test_run(&task)?;
                    }
                    info!("task : {} is DONE !", task.id);

                    {
                        let mut map = task_map.lock().unwrap();
                        map.remove(&task.id);

                        let mut ready_list = Vec::new();
                        for next in task.next {
                            if let Some(next) = map.get(&next) {
                                let mut next_task = next.write().unwrap();
                                assert_ne!(next_task.in_degree, 0);
                                next_task.in_degree -= 1;
                                if next_task.in_degree == 0 {
                                    ready_list.push(next_task.id);
                                    ready_q.push(next.clone()).unwrap();
                                }
                            }
                        }
                        for ready in ready_list {
                            map.remove(&ready);
                        }
                    }
                }
                Ok(())
            }));
        }
        TaskManager {
            degree_of_parallel,
            status,
            ready_q,
            condvar,
            task_map,
            threads: Mutex::new(threads),
            task_id: AtomicU32::new(0),
        }
    }
    pub fn submit(&mut self, task: Task) -> Result<()> {
        let task_arc = Arc::new(RwLock::new(task));
        let status = *self.status.lock().unwrap().deref();

        match status {
            Running | Stopped => {
                let deps = task_arc.write();
                match deps {
                    Ok(mut deps) => {
                        // push task to queue
                        if self.is_ready(&mut deps)? {
                            let _ = self.ready_q.push(task_arc.clone())?;
                        }
                        // add task to map after push task to queue
                        // avoid task self depend
                        self.task_map
                            .lock()
                            .unwrap()
                            .insert(deps.id, task_arc.clone());
                        Ok(())
                    }
                    Err(e) => Err(anyhow!(format!("{}", e))),
                }
            }
            Shutdown => Err(anyhow!(
                "Talps-Manager is Shutdown , cannot submit task anymore"
            )),
        }
    }

    fn is_ready(&self, task: &mut Task) -> Result<bool> {
        if task.depend.is_empty() {
            return Ok(true);
        }
        let mut is_ready = true;
        for dep in &task.depend {
            if let Some(dep) = self.task_map.lock().unwrap().get(dep) {
                is_ready = false;
                task.in_degree += 1;
                dep.write()
                    .map_err(|_| anyhow!("Mutex poisoned"))?
                    .next
                    .push(task.id);
            }
        }
        Ok(is_ready)
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
                self.condvar.notify_all();
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
            let mut manager = TaskManager::new(1);
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
    fn stop_test_no_dep() {
        let mut manager = TaskManager::new(1);
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 100;
        for i in 0..n {
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                depend: vec![101, 102, 103],
                status: Status::Pending,
                file_name: "wwt".to_string(),
                in_degree: 0,
                next: vec![],
                test: true,
            };
            manager.submit(task).unwrap();
        }
        assert_eq!(manager.task_map.lock().unwrap().len(), n);
        assert_eq!(manager.ready_q.block_queue.lock().unwrap().len(), n);
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
    fn stop_test_with_dep() {
        let mut manager = TaskManager::new(1);
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 100;
        for i in 0..n {
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                depend: vec![0, 1, 2],
                status: Status::Pending,
                file_name: format!("./trash/{}", i),
                in_degree: 0,
                next: vec![],
                test: true,
            };
            manager.submit(task).unwrap();
        }
        assert_eq!(manager.task_map.lock().unwrap().len(), n);
        assert_eq!(manager.ready_q.block_queue.lock().unwrap().len(), 1);

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
    #[test]
    fn run_test_no_dep() {
        let _ = remove_dir_all("./run_no_dep");
        let mut manager = TaskManager::new(100);
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 100;
        for i in 0..n {
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                depend: vec![101, 102, 103],
                status: Status::Pending,
                file_name: format!("./run_no_dep/{}", i),
                in_degree: 0,
                next: vec![],
                test: true,
            };
            manager.submit(task).unwrap();
        }
        manager.run().expect("Running Err");
        {
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Running);
        }
        sleep(Duration::from_secs(5));

        let dir = read_dir("./run_no_dep").expect("output folder");
        assert_eq!(dir.count(), n);
        assert_eq!(manager.task_map.lock().unwrap().len(), 0);

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
    #[test]
    fn run_test_with_dep() {
        remove_dir_all("./run_with_dep");
        let mut manager = TaskManager::new(100);
        let status = *manager.status.lock().unwrap();
        assert_eq!(status, Stopped);
        let n = 30;
        for i in 0..n {
            let mut deps = vec![];
            if i != 0 {
                deps.push(i - 1);
            }
            let task = Task {
                id: i,
                name: "wwt".to_string(),
                depend: deps,
                status: Status::Pending,
                file_name: format!("./run_with_dep/{}", i),
                in_degree: 0,
                next: vec![],
                test: true,
            };
            manager.submit(task).unwrap();
        }
        manager.run().expect("Running Err");
        {
            let status = *manager.status.lock().unwrap();
            assert_eq!(status, Running);
        }

        sleep(Duration::from_secs(5));
        let dir = read_dir("./run_with_dep").expect("output folder");
        assert_eq!(dir.count(), n);
        assert_eq!(manager.task_map.lock().unwrap().len(), 0);

        remove_dir_all("./run_with_dep");

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
