//! Command-line access to Bottles Next environments, addons, and library entries.
//!
//! Run `bottles-cli --help` for the available commands. For example,
//! `bottles-cli addons import ./runner.tar.xz --kind runner --name Local --version 1.0`
//! acquires a trusted local runner archive without contacting a catalog.

mod addons;
mod environments;
mod library;
mod plugins;
mod profiles;

#[cfg(feature = "fvs")]
use std::path::PathBuf;
use std::{error::Error, io, sync::Arc};

use bottles_core::{Bottles, Config, Directories, Operation};
use bottles_plugin_host::Plugins;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use url::Url;
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Parser)]
#[command(version, about = "Bottles Next CLI")]
struct Cli {
    #[cfg(feature = "fvs")]
    /// Path to the FVS daemon executable.
    #[arg(long, global = true, value_name = "PATH")]
    fvs2d: Option<PathBuf>,
    /// Override the component catalog URL.
    #[arg(long, global = true)]
    component_catalog: Option<Url>,
    /// Override the dependency catalog URL.
    #[arg(long, global = true)]
    dependency_catalog: Option<Url>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Refresh, download, import, and inspect addon releases.
    Addons {
        #[command(subcommand)]
        command: addons::Command,
    },
    /// Create and manage persistent Wine bottles.
    Bottle {
        #[command(subcommand)]
        command: environments::BottleCommand,
    },
    /// Create and manage standalone Virgo programs.
    #[cfg(feature = "fvs")]
    Program {
        #[command(subcommand)]
        command: environments::ProgramCommand,
    },
    /// List, search, and launch registered applications.
    Library {
        #[command(subcommand)]
        command: library::Command,
    },
    /// Inspect installed plugins or build a local development plugin.
    Plugins {
        #[command(subcommand)]
        command: plugins::Command,
    },
    /// Manage profiles and linked storefront accounts.
    Profiles {
        #[command(subcommand)]
        command: profiles::Command,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let directories = Directories::new().await?;
    let plugins = Plugins::open(directories.plugins(), directories.staging()).await?;
    let bottles = Bottles::open(
        Config {
            #[cfg(feature = "fvs")]
            fvs2d: cli.fvs2d,
            component_catalog: cli.component_catalog,
            dependency_catalog: cli.dependency_catalog,
        },
        Arc::clone(&plugins),
    )
    .await?;
    let result = match cli.command {
        Command::Addons { command } => addons::run(bottles.addons(), command).await,
        Command::Bottle { command } => environments::run_bottle(&bottles, command).await,
        #[cfg(feature = "fvs")]
        Command::Program { command } => environments::run_program(&bottles, command).await,
        Command::Library { command } => library::run(bottles.library(), command).await,
        Command::Plugins { command } => plugins::run(&plugins, command).await,
        Command::Profiles { command } => profiles::run(bottles.profiles(), command).await,
    };
    let shutdown = bottles.shutdown().await;
    result?;
    shutdown?;
    Ok(())
}

fn missing(kind: &str, value: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{kind} not found: {value}"),
    )
}

fn local_addon<K: Clone>(
    id: Uuid,
    releases: Vec<Arc<bottles_core::Addon<K>>>,
) -> Result<bottles_core::Addon<K>> {
    releases
        .into_iter()
        .find(|item| item.id() == id)
        .map(|item| item.as_ref().clone())
        .ok_or_else(|| missing("addon", id).into())
}

async fn run_operation<T>(mut operation: Operation<T>) -> Result<T> {
    let mut progress = Box::pin(operation.progress());
    let reporter = tokio::spawn(async move {
        while let Some(update) = progress.next().await {
            if let Some(transfer) = update.transfer {
                if let Some(total) = transfer.total {
                    eprintln!("{}: {}/{}", update.stage, transfer.current, total);
                } else {
                    eprintln!("{}: {}", update.stage, transfer.current);
                }
            } else {
                eprintln!("{}", update.stage);
            }
        }
    });
    let result = tokio::select! {
        result = &mut operation => result,
        signal = tokio::signal::ctrl_c() => {
            signal?;
            eprintln!("cancelling...");
            operation.cancel().await
        }
    };
    reporter.abort();
    let _ = reporter.await;
    Ok(result?)
}
