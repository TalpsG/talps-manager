use anyhow::Result;
use clap::{Parser, Subcommand, command};
use talps_manager::server_n_client::TalpsManagerClient;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Test the connection to the server
    Test { msg: String },
    /// Submit a task to the manager
    SubmitTask { name: String, cmd: String },
    /// Run the manager
    Run,
    /// Stop the manager
    Stop,
    /// Show all tasks
    ShowTasks,
}
#[tokio::main]
async fn main() -> Result<()> {
    let client = TalpsManagerClient::new("54321".to_string()).await?;
    let cli = Cli::parse();
    match cli.cmd {
        // fill this match
        Commands::Test { msg } => {
            let response = client.test(msg).await?;
            println!("Response: {}", response);
        }
        Commands::SubmitTask { name, cmd } => {
            let response = client.submit_task(name, cmd).await?;
            println!("Response: {}", response);
        }
        Commands::Run => {
            let response = client.run().await?;
            println!("Response: {}", response);
        }
        Commands::Stop => {
            let response = client.stop().await?;
            println!("Response: {}", response);
        }
        Commands::ShowTasks => {
            let tasks = client.show_tasks().await?;
            println!("Current tasks:");
            for task in tasks {
                println!("{}", task);
            }
        }
    }

    Ok(())
}
