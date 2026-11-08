use std::path::{Path, PathBuf};

use bottles_plugin_host::{Manifest, Plugins};
use clap::Subcommand;

use crate::Result;

#[derive(Subcommand)]
pub enum Command {
    /// List installed plugins.
    List,
    /// Build and install a local plugin source checkout.
    DevInstall { source: PathBuf },
    /// Remove an installed plugin.
    Uninstall { plugin: String },
}

pub async fn run(plugins: &Plugins, command: Command) -> Result<()> {
    match command {
        Command::List => plugins.list()?.iter().for_each(print),
        Command::DevInstall { source } => print(&dev_install(plugins, &source).await?),
        Command::Uninstall { plugin } => plugins.uninstall(&plugin)?,
    }
    Ok(())
}

async fn dev_install(plugins: &Plugins, source: &Path) -> Result<Manifest> {
    let manifest = source.join("Cargo.toml");
    let cargo = tokio::fs::read_to_string(&manifest).await?;
    let target = target_name(&cargo)?;
    let staging = tempfile::tempdir()?;
    let target_dir = staging.path().join("target");
    let output = tokio::process::Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-wasip2"])
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(&target_dir)
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        return Err(format!(
            "plugin build failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let package = staging.path().join("package");
    tokio::fs::create_dir(&package).await?;
    tokio::fs::copy(source.join("plugin.toml"), package.join("plugin.toml")).await?;
    tokio::fs::copy(
        target_dir
            .join("wasm32-wasip2/release")
            .join(target)
            .with_extension("wasm"),
        package.join("plugin.wasm"),
    )
    .await?;
    Ok(plugins.install(&package)?)
}

fn target_name(source: &str) -> Result<String> {
    let manifest: toml::Value = toml::from_str(source)?;
    let name = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
        })
        .ok_or("Cargo.toml has no library or package name")?;
    Ok(name.replace('-', "_"))
}

fn print(manifest: &Manifest) {
    println!("{}\t{}\t{}", manifest.id, manifest.name, manifest.version);
}
