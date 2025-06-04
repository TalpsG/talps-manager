use anyhow::Result;
use talps_manager::server_n_client::{TalpsServer, log_init};
use tracing::info;
#[tokio::main]
async fn main() -> Result<()> {
    log_init();
    let mut talps_server = TalpsServer::new("54321".to_string()).await?;
    talps_server.start().await?;
    info!("Talps server started at port 54321");
    Ok(())
}
