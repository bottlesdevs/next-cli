use bottles_core::proto::NotifyRequest;
pub use bottles_core::proto::{bottles_client::BottlesClient, HealthRequest};
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
    let args = Cli::parse();
    let url = "http://[::1]:50052";
    let mut client = BottlesClient::connect(url).await?;
    match args.command {
        Command::Health => {
            let request = HealthRequest {};
            let response = client.health(request).await?;
            let response = response.get_ref();
            if response.ok {
                println!("Server is healthy");
            } else {
                println!("Server is unhealthy");
            }
        }
        Command::Notify { message } => {
            let request = NotifyRequest { message };
            let response = client.notify(request).await?;
            let response = response.get_ref();
            if response.success {
                println!("Message sent successfully");
            } else {
                println!("Failed to send message");
            }
        }
    }
    Ok(())
}
