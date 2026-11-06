use std::{collections::HashSet, path::PathBuf};

use bottles_core::{Addon, AddonRole, Component, Core, Runner, Slot, Umu, WineBridge};
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

pub async fn run(core: &Core, command: Command) -> Result<()> {
    match command {
        Command::Refresh => {
            run_operation(core.catalogs().refresh()).await?;
        }
        Command::List => list(core),
        Command::Download { id } => {
            print(&run_operation(core.catalogs().fetch::<AddonRole>(id)).await?)
        }
        Command::Import {
            source,
            kind,
            name,
            version,
        } => {
            let role: AddonRole = match kind {
                LocalKind::Runner => Runner {}.into(),
                LocalKind::Winebridge => WineBridge {}.into(),
                LocalKind::Umu => Umu {}.into(),
                LocalKind::Dxvk => Component { slot: Slot::Dxvk }.into(),
                LocalKind::Vkd3d => Component { slot: Slot::Vkd3d }.into(),
                LocalKind::Nvapi => Component { slot: Slot::Nvapi }.into(),
                LocalKind::LatencyFlex => Component {
                    slot: Slot::LatencyFlex,
                }
                .into(),
            };
            print(
                &run_operation(
                    core.addons()
                        .import(source, name, version, Vec::new(), role),
                )
                .await?,
            );
        }
        Command::Delete { id } => core.addons().remove(id).await?,
    }
    Ok(())
}

fn list(core: &Core) {
    let state = core.addons().state();
    let catalogs = core.catalogs().state();
    let mut local: Vec<_> = state.releases::<AddonRole>().iter().map(row).collect();
    local.sort_unstable_by_key(|(id, ..)| *id);
    let ids: HashSet<_> = local.iter().map(|(id, ..)| *id).collect();
    for (id, name, version, kind) in local {
        println!("{id}\t{name}\t{version}\t{kind}\tlocal");
    }
    for entry in catalogs
        .component_entries()
        .into_iter()
        .chain(catalogs.dependency_entries())
    {
        if ids.contains(&entry.id()) {
            continue;
        }
        let kind = label(entry.role());
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

fn label(role: AddonRole) -> &'static str {
    match role {
        AddonRole::Runner(_) => "runner",
        AddonRole::WineBridge(_) => "winebridge",
        AddonRole::Umu(_) => "umu",
        AddonRole::Component(component) => component.slot.as_str(),
        AddonRole::Dependency(_) => "dependency",
    }
}

fn row(item: &Addon<AddonRole>) -> (Uuid, String, String, &'static str) {
    (
        item.id(),
        item.name().to_owned(),
        item.version().to_owned(),
        label(item.role()),
    )
}

fn print(item: &Addon<AddonRole>) {
    println!(
        "{}\t{}\t{}\t{}\tlocal",
        item.id(),
        item.name(),
        item.version(),
        label(item.role())
    );
}
