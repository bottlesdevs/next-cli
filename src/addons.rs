use std::{collections::HashSet, path::PathBuf, sync::Arc};

use bottles_core::{Addon, AddonKind, Addons, Slot};
use clap::{Subcommand, ValueEnum};
use uuid::Uuid;

use crate::{Result, missing, run_operation};

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
        Command::Download { id } => {
            let state = addons.state();
            let entry = state
                .component_entry(id)
                .or_else(|| state.dependency_entry(id))
                .ok_or_else(|| missing("catalog entry", id))?;
            match entry.kind() {
                AddonKind::Runner => {
                    print(&run_operation(addons.fetch_runner(id)).await?, "runner")
                }
                AddonKind::WineBridge => print(
                    &run_operation(addons.fetch_winebridge(id)).await?,
                    "winebridge",
                ),
                AddonKind::Umu => print(&run_operation(addons.fetch_umu(id)).await?, "umu"),
                AddonKind::Component { slot } => print(
                    &run_operation(addons.fetch_component(id)).await?,
                    slot.as_str(),
                ),
                AddonKind::Dependency => print(
                    &run_operation(addons.fetch_dependency(id)).await?,
                    "dependency",
                ),
            }
        }
        Command::Import {
            source,
            kind,
            name,
            version,
        } => match kind {
            LocalKind::Runner => print(
                &run_operation(addons.import_runner(source, name, version)).await?,
                "runner",
            ),
            LocalKind::Winebridge => print(
                &run_operation(addons.import_winebridge(source, name, version)).await?,
                "winebridge",
            ),
            LocalKind::Umu => print(
                &run_operation(addons.import_umu(source, name, version)).await?,
                "umu",
            ),
            kind => {
                let slot = match kind {
                    LocalKind::Dxvk => Slot::Dxvk,
                    LocalKind::Vkd3d => Slot::Vkd3d,
                    LocalKind::Nvapi => Slot::Nvapi,
                    LocalKind::LatencyFlex => Slot::LatencyFlex,
                    _ => return Err("invalid component kind".into()),
                };
                print(
                    &run_operation(addons.import_component(source, slot, name, version)).await?,
                    slot.as_str(),
                );
            }
        },
        Command::Delete { id } => addons.remove(id).await?,
    }
    Ok(())
}

fn list(addons: &Addons) {
    let state = addons.state();
    let mut local = Vec::new();
    local.extend(state.runners().iter().map(|item| row(item, "runner")));
    local.extend(
        state
            .winebridges()
            .iter()
            .map(|item| row(item, "winebridge")),
    );
    local.extend(state.umus().iter().map(|item| row(item, "umu")));
    local.extend(
        state
            .components()
            .iter()
            .map(|item| row(item, item.slot().as_str())),
    );
    local.extend(
        state
            .dependencies()
            .iter()
            .map(|item| row(item, "dependency")),
    );
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
        let kind = match entry.kind() {
            AddonKind::Runner => "runner",
            AddonKind::WineBridge => "winebridge",
            AddonKind::Umu => "umu",
            AddonKind::Component { slot } => slot.as_str(),
            AddonKind::Dependency => "dependency",
        };
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

fn row<K>(item: &Addon<K>, kind: &'static str) -> (Uuid, String, String, &'static str) {
    (
        item.id(),
        item.name().to_owned(),
        item.version().to_owned(),
        kind,
    )
}

fn print<K>(item: &Arc<Addon<K>>, kind: &str) {
    println!(
        "{}\t{}\t{}\t{kind}\tlocal",
        item.id(),
        item.name(),
        item.version()
    );
}
