use std::{collections::HashSet, path::PathBuf};

use bottles_core::{Addon, AddonKind, Addons, Slot};
use clap::{Subcommand, ValueEnum};
use uuid::Uuid;

use crate::{Result, run_operation};

#[derive(Subcommand)]
pub enum Command {
    /// Refresh cached remote catalogs and local addon state.
    Refresh,
    /// Show local releases and catalog entries.
    List,
    /// Acquire one catalog release by UUID.
    Download { id: Uuid },
    /// Import a trusted local tar archive.
    Import {
        source: PathBuf,
        #[arg(long, value_enum)]
        kind: LocalKind,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: String,
    },
    /// Remove a locally acquired release by UUID.
    Delete { id: Uuid },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum LocalKind {
    Runner,
    Winebridge,
    Umu,
    Dxvk,
    Vkd3d,
    Nvapi,
    LatencyFlex,
}

pub async fn run(addons: &Addons, command: Command) -> Result<()> {
    match command {
        Command::Refresh => {
            run_operation(addons.refresh()).await?;
        }
        Command::List => list(addons),
        Command::Download { id } => print(&run_operation(addons.fetch::<AddonKind>(id)).await?),
        Command::Import {
            source,
            kind,
            name,
            version,
        } => {
            let kind = match kind {
                LocalKind::Runner => AddonKind::Runner,
                LocalKind::Winebridge => AddonKind::WineBridge,
                LocalKind::Umu => AddonKind::Umu,
                LocalKind::Dxvk => AddonKind::Component { slot: Slot::Dxvk },
                LocalKind::Vkd3d => AddonKind::Component { slot: Slot::Vkd3d },
                LocalKind::Nvapi => AddonKind::Component { slot: Slot::Nvapi },
                LocalKind::LatencyFlex => AddonKind::Component {
                    slot: Slot::LatencyFlex,
                },
            };
            print(&run_operation(addons.import(source, name, version, Vec::new(), kind)).await?);
        }
        Command::Delete { id } => addons.remove(id).await?,
    }
    Ok(())
}

fn list(addons: &Addons) {
    let state = addons.state();
    let mut local: Vec<_> = state.releases::<AddonKind>().iter().map(row).collect();
    local.sort_unstable_by_key(|(id, ..)| *id);
    let ids: HashSet<_> = local.iter().map(|(id, ..)| *id).collect();
    for (id, name, version, kind) in local {
        println!("{id}\t{name}\t{version}\t{kind}\tlocal");
    }
    for entry in state
        .component_entries()
        .into_iter()
        .chain(state.dependency_entries())
    {
        if ids.contains(&entry.id()) {
            continue;
        }
        let kind = label(entry.kind());
        let status = if entry.is_supported() {
            "downloadable"
        } else {
            "unsupported"
        };
        println!(
            "{}\t{}\t{}\t{kind}\t{status}",
            entry.id(),
            entry.name(),
            entry.version()
        );
    }
}

fn label(kind: AddonKind) -> &'static str {
    match kind {
        AddonKind::Runner => "runner",
        AddonKind::WineBridge => "winebridge",
        AddonKind::Umu => "umu",
        AddonKind::Component { slot } => slot.as_str(),
        AddonKind::Dependency => "dependency",
    }
}

fn row(item: &Addon<AddonKind>) -> (Uuid, String, String, &'static str) {
    (
        item.id(),
        item.name().to_owned(),
        item.version().to_owned(),
        label(item.kind()),
    )
}

fn print(item: &Addon<AddonKind>) {
    println!(
        "{}\t{}\t{}\t{}\tlocal",
        item.id(),
        item.name(),
        item.version(),
        label(item.kind())
    );
}
