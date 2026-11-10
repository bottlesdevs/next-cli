//! Command-line access to Bottles Next prefixes and addons.
//!
//! Run `bottles-cli --help` for the available commands. For example,
//! `bottles-cli addons import ./runner.tar.xz --kind runner --name Local --version 1.0`
//! acquires a trusted local runner archive without contacting a catalog.

mod addons;
mod plugins;
mod prefixes;

#[cfg(feature = "fvs")]
use std::path::PathBuf;
use std::{error::Error, io};

use bottles_core::{Core, Operation};
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
    /// Use this catalog source instead of the defaults. Repeat for several;
    /// earlier sources win.
    #[arg(long = "catalog", global = true, value_name = "URL")]
    catalogs: Vec<Url>,
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
    /// Create and manage persistent Wine prefixes.
    Prefix {
        #[command(subcommand)]
        command: prefixes::Command,
    },
    /// Inspect installed plugins or build a local development plugin.
    Plugins {
        #[command(subcommand)]
        command: plugins::Command,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let plugins = Plugins::new()?;
    let mut core = Core::builder();
    if !cli.catalogs.is_empty() {
        core = core.catalogs(cli.catalogs);
    }
    #[cfg(feature = "fvs")]
    let core = core.fvs2d(cli.fvs2d);
    let core = core.build().await?;
    let result = match cli.command {
        Command::Addons { command } => addons::run(&core, command).await,
        Command::Prefix { command } => prefixes::run(&core, command).await,
        Command::Plugins { command } => plugins::run(&plugins, command).await,
    };
    let shutdown = core.shutdown().await;
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

fn local_addon<R: bottles_core::RoleMarker>(
    state: &bottles_core::AddonsState,
    id: Uuid,
) -> Result<bottles_core::Addon<R>> {
    state.release(id).ok_or_else(|| missing("addon", id).into())
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
