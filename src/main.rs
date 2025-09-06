use bottles_core::proto::NotifyRequest;
pub use bottles_core::proto::{HealthRequest, bottles_client::BottlesClient};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Check the health of the server")]
    Health,
    #[command(about = "Notify the server")]
    Notify {
        #[arg(help = "The message to send")]
        message: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Cli::parse();
    let url = "http://[::1]:50052";
    let mut client = BottlesClient::connect(url).await?;

    match args.command {
        Command::Health => {
            let request = HealthRequest {};
            let response = client.health(request).await?;
            let response = response.get_ref();
            if response.ok {
                tracing::info!("Server is healthy");
            } else {
                tracing::info!("Server is unhealthy");
            }
        }
        Command::Notify { message } => {
            let request = NotifyRequest { message };
            let response = client.notify(request).await?;
            let response = response.get_ref();
            if response.success {
                tracing::info!("Message sent successfully");
            } else {
                tracing::info!("Failed to send message");
            }
        }
    }
    Ok(())
}
