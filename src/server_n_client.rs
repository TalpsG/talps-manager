use crate::manager::TaskManager;
use anyhow::Result;
use std::fs::OpenOptions;
use tracing::{error, info};

use jsonrpsee::core::async_trait;
use jsonrpsee::http_client::HttpClient;
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
        .open("../app.log")
        .expect("Failed to create log file");

    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .with_env_filter(EnvFilter::new("info"))
        .init();
}

#[proc_macros::rpc(server, client)]
pub trait TalpsApi {
    #[method(name = "test")]
    async fn test(&self, msg: String) -> RpcResult<String>;
    #[method(name = "submit_task")]
    async fn submit_task(&self, name: String, cmd: String) -> RpcResult<String>;

    #[method(name = "show_tasks")]
    async fn show_tasks(&self) -> RpcResult<Vec<String>>;
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
                ret.err().unwrap().to_string()
            ));
        }
        Ok("submit success".to_string())
    }

    async fn show_tasks(&self) -> RpcResult<Vec<String>> {
        info!("rpc show tasks");
        Ok(self.manager.show_tasks())
    }
}

#[tokio::test]
async fn client_call_test() -> Result<()> {
    let mut talps_server = TalpsServer::new("54321".to_string()).await?;
    talps_server.start().await?;

    let client = HttpClient::builder().build("http://localhost:54321")?;
    let ret = client.test("abc".to_string()).await?;
    assert_eq!(ret, "abcserver reply");

    Ok(())
}
