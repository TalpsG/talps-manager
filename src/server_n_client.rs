use crate::manager::TaskManager;
use anyhow::Result;
use std::fs::OpenOptions;
use tracing::{error, info};

use jsonrpsee::core::async_trait;
use jsonrpsee::server::ServerHandle;
use jsonrpsee::{core::RpcResult, proc_macros, server::ServerBuilder};
use tracing_subscriber::EnvFilter;

pub struct TalpsServer {
    port: String,
    server: Option<ServerHandle>,
}
impl TalpsServer {
    pub async fn new(port: String) -> Result<TalpsServer> {
        Ok(TalpsServer { server: None, port })
    }
    pub async fn start(&mut self) -> Result<()> {
        let server = ServerBuilder::default()
            .build(format!("0.0.0.0:{}", self.port))
            .await?;
        let server = server.start(
            TalpsApiImpl {
                manager: TaskManager::new(),
            }
            .into_rpc(),
        );
        self.server = Some(server);
        Ok(())
    }
}

pub fn log_init() {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("./app.log")
        .expect("Failed to create log file");

    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .with_env_filter(EnvFilter::new("info"))
        .init();
    info!("Logging initialized");
}

#[proc_macros::rpc(server, client)]
pub trait TalpsApi {
    #[method(name = "test")]
    async fn test(&self, msg: String) -> RpcResult<String>;
    #[method(name = "submit_task")]
    async fn submit_task(&self, name: String, cmd: String) -> RpcResult<String>;

    #[method(name = "show_tasks")]
    async fn show_tasks(&self) -> RpcResult<Vec<String>>;
    #[method(name = "run")]
    async fn run(&self) -> RpcResult<String>;
    #[method(name = "stop")]
    async fn stop(&self) -> RpcResult<String>;
}
pub struct TalpsApiImpl {
    manager: TaskManager,
}

#[async_trait]
impl TalpsApiServer for TalpsApiImpl {
    async fn test(&self, msg: String) -> RpcResult<String> {
        info!("rpc test : {}", msg);
        Ok(msg + "server reply")
    }

    async fn submit_task(&self, name: String, cmd: String) -> RpcResult<String> {
        info!("rpc submit task {} : {}", name, cmd);
        let ret = self.manager.submit(name.clone(), cmd);
        if ret.is_err() {
            error!("submit task {} failed", name);
            return Ok(format!(
                "submit task {} failed : {}",
                name,
                ret.err().unwrap()
            ));
        }
        Ok("submit success".to_string())
    }

    async fn show_tasks(&self) -> RpcResult<Vec<String>> {
        info!("rpc show tasks");
        Ok(self.manager.show_tasks())
    }
    async fn run(&self) -> RpcResult<String> {
        let ret = self.manager.run();
        match ret {
            Ok(_) => Ok("Start to run".to_string()),
            Err(e) => Ok(e.to_string()),
        }
    }
    async fn stop(&self) -> RpcResult<String> {
        let ret = self.manager.run();
        match ret {
            Ok(_) => Ok("Start to run".to_string()),
            Err(e) => Ok(e.to_string()),
        }
    }
}

pub struct TalpsManagerClient {
    client: jsonrpsee::http_client::HttpClient,
}

impl TalpsManagerClient {
    pub async fn new(port: String) -> Result<TalpsManagerClient> {
        let client = jsonrpsee::http_client::HttpClient::builder()
            .build(format!("http://localhost:{}", port))?;
        Ok(TalpsManagerClient { client })
    }

    pub async fn test(&self, msg: String) -> Result<String> {
        self.client.test(msg).await.map_err(|e| {
            error!("RPC call failed: {}", e);
            e.into()
        })
    }
    // submit task to the manager
    pub async fn submit_task(&self, name: String, cmd: String) -> Result<String> {
        self.client.submit_task(name, cmd).await.map_err(|e| {
            error!("RPC call failed: {}", e);
            e.into()
        })
    }
    // run
    pub async fn run(&self) -> Result<String> {
        self.client.run().await.map_err(|e| {
            error!("RPC call failed: {}", e);
            e.into()
        })
    }
    // stop
    pub async fn stop(&self) -> Result<String> {
        self.client.stop().await.map_err(|e| {
            error!("RPC call failed: {}", e);
            e.into()
        })
    }
    // show task
    pub async fn show_tasks(&self) -> Result<Vec<String>> {
        self.client.show_tasks().await.map_err(|e| {
            error!("RPC call failed: {}", e);
            e.into()
        })
    }
}

#[tokio::test]
async fn client_call_test() -> Result<()> {
    use jsonrpsee::http_client::HttpClient;

    let mut talps_server = TalpsServer::new("54321".to_string()).await?;
    talps_server.start().await?;

    let client = HttpClient::builder().build("http://localhost:54321")?;
    let ret = client.test("abc".to_string()).await?;
    assert_eq!(ret, "abcserver reply");

    Ok(())
}
