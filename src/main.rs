use std::{collections::HashMap, error::Error, io, path::PathBuf, str::FromStr, sync::Arc};

use bottles_core::{
    Addon, Addons, Bottle, BottleManager, Bottles, CatalogEntry, Component, Config, Dependency,
    DllOverride, DllOverrideMode, GamescopeConfig, GamescopeFilter, GamescopeScaler, IndexEntry,
    Operation, Program, Storage,
};
use bottles_plugin_host::{
    API_VERSION as PLUGIN_API_VERSION, PluginManager, PluginManagerConfig, PluginManifest,
    build_source, validate_source, write_archive,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Parser)]
#[command(version, about = "Bottles Next CLI")]
struct Cli {
    #[cfg(feature = "fvs")]
    #[arg(long, global = true, value_name = "PATH")]
    fvs2d: Option<PathBuf>,

    #[arg(long)]
    component_catalog: Option<Url>,

    #[arg(long)]
    dependency_catalog: Option<Url>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Refresh, inspect, download, or delete addons and runners.
    Addons {
        #[command(subcommand)]
        command: AddonsCommand,
    },
    /// Create, list, or manage bottles.
    Bottle {
        #[command(subcommand)]
        command: BottleCommand,
    },
    /// Install, inspect, and invoke plugins.
    Plugins {
        #[command(subcommand)]
        command: PluginsCommand,
    },
}

#[derive(Subcommand)]
enum PluginsCommand {
    /// List installed plugins.
    List,
    /// Search a moderated plugin registry.
    Search {
        #[arg(long, value_name = "HTTPS_INDEX_URL")]
        registry: Url,
        #[arg(value_name = "QUERY", default_value = "")]
        query: String,
    },
    /// Install a plugin from a moderated registry.
    Install(RegistryPluginArgs),
    /// Update an installed plugin from a moderated registry.
    Update(RegistryPluginArgs),
    /// Inspect or change one plugin's JSON settings.
    Settings {
        #[arg(value_name = "PLUGIN_ID")]
        plugin: String,
        #[command(subcommand)]
        command: PluginSettingsCommand,
    },
    /// Manage named read-only directories exposed to plugins.
    Roots {
        #[command(subcommand)]
        command: PluginRootsCommand,
    },
    /// List commands exported by enabled plugins.
    Commands {
        #[arg(value_name = "PLUGIN_ID")]
        plugin: Option<String>,
    },
    /// Invoke a plugin command.
    Run(PluginRunArgs),
    /// Enable an installed plugin.
    Enable(PluginIdArgs),
    /// Disable an installed plugin.
    Disable(PluginIdArgs),
    /// Reload an enabled plugin from its installed files.
    Reload(PluginIdArgs),
    /// Recreate a failed plugin instance.
    Retry(PluginIdArgs),
    /// Uninstall a plugin.
    Uninstall(PluginIdArgs),
    /// Build and install a plugin from its source directory.
    DevInstall(PluginSourceArgs),
    /// Rebuild and reload a development plugin.
    DevRebuild(PluginIdArgs),
    /// Validate a plugin source directory.
    Validate(PluginSourceArgs),
    /// Build a distributable plugin archive from source.
    Package {
        #[arg(value_name = "SOURCE_DIRECTORY")]
        source: PathBuf,
        #[arg(short, long, value_name = "ARCHIVE")]
        output: Option<PathBuf>,
    },
    /// Create a minimal Rust plugin project.
    New {
        #[arg(value_name = "DIRECTORY")]
        directory: PathBuf,
    },
}

#[derive(Args)]
struct PluginIdArgs {
    #[arg(value_name = "PLUGIN_ID")]
    plugin: String,
}

#[derive(Args)]
struct PluginSourceArgs {
    #[arg(value_name = "SOURCE_DIRECTORY")]
    source: PathBuf,
}

#[derive(Args)]
struct RegistryPluginArgs {
    #[arg(value_name = "PLUGIN_ID")]
    plugin: String,
    #[arg(long, value_name = "HTTPS_INDEX_URL")]
    registry: Url,
}

#[derive(Subcommand)]
enum PluginSettingsCommand {
    List,
    Set {
        key: String,
        #[arg(value_name = "JSON_VALUE")]
        value: String,
    },
    Unset {
        key: String,
    },
}

#[derive(Subcommand)]
enum PluginRootsCommand {
    List,
    Add { name: String, path: PathBuf },
    Remove { name: String },
}

#[derive(Args)]
struct PluginRunArgs {
    #[arg(value_name = "PLUGIN_ID/COMMAND_ID")]
    command: PluginCommand,

    #[arg(long, value_name = "UUID_OR_NAME")]
    bottle: Option<String>,

    #[arg(last = true, allow_hyphen_values = true, value_name = "ARGUMENT")]
    arguments: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PluginCommand {
    plugin: String,
    command: String,
}

impl FromStr for PluginCommand {
    type Err = &'static str;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (plugin, command) = value
            .split_once('/')
            .filter(|(plugin, command)| {
                !plugin.is_empty() && !command.is_empty() && !command.contains('/')
            })
            .ok_or("expected PLUGIN_ID/COMMAND_ID")?;
        Ok(Self {
            plugin: plugin.into(),
            command: command.into(),
        })
    }
}

#[derive(Subcommand)]
enum AddonsCommand {
    /// Refresh the remote catalog and local state.
    Refresh,
    List,
    Download {
        #[arg(value_name = "UUID")]
        id: String,
    },
    Delete {
        #[arg(value_name = "UUID")]
        id: String,
    },
}

#[derive(Subcommand)]
enum BottleCommand {
    /// Create a bottle from a downloaded runner.
    Create(CreateArgs),
    /// List bottles.
    List,
    /// Manage one bottle selected by UUID or name.
    Manage(ManageArgs),
}

#[derive(Args)]
struct ManageArgs {
    bottle: String,

    #[command(subcommand)]
    command: ManageCommand,
}

#[derive(Subcommand)]
enum ManageCommand {
    Show,
    Delete,
    Stop,
    Processes,
    Install {
        addon: String,
    },
    Uninstall {
        component: String,
    },
    Program {
        #[command(subcommand)]
        command: ProgramCommand,
    },
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    DllOverrides {
        #[command(subcommand)]
        command: DllOverridesCommand,
    },
    #[cfg(feature = "fvs")]
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    Wrappers {
        #[command(subcommand)]
        command: WrappersCommand,
    },
}

#[derive(Subcommand)]
enum ProgramCommand {
    Add(AddProgramArgs),
    Launch {
        #[arg(value_name = "UUID")]
        program: String,
    },
    Kill {
        #[arg(value_name = "UUID")]
        program: String,
    },
}

#[derive(Subcommand)]
enum EnvCommand {
    List,
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Subcommand)]
enum DllOverridesCommand {
    List,
    Set {
        dll: String,
        #[arg(value_enum)]
        mode: CliDllOverrideMode,
    },
    Unset {
        dll: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliDllOverrideMode {
    NativeBuiltin,
    BuiltinNative,
    Native,
    Builtin,
    Disabled,
}

impl From<CliDllOverrideMode> for DllOverrideMode {
    fn from(value: CliDllOverrideMode) -> Self {
        match value {
            CliDllOverrideMode::NativeBuiltin => Self::NativeBuiltin,
            CliDllOverrideMode::BuiltinNative => Self::BuiltinNative,
            CliDllOverrideMode::Native => Self::Native,
            CliDllOverrideMode::Builtin => Self::Builtin,
            CliDllOverrideMode::Disabled => Self::Disabled,
        }
    }
}

#[cfg(feature = "fvs")]
#[derive(Subcommand)]
enum SnapshotCommand {
    Create {
        message: String,
    },
    List,
    Restore {
        #[arg(value_name = "STATE_ID_OR_PREFIX")]
        state: String,
    },
}

#[derive(Subcommand)]
enum WrappersCommand {
    Gamescope {
        #[command(subcommand)]
        command: GamescopeCommand,
    },
    Mangohud {
        #[command(subcommand)]
        command: MangohudCommand,
    },
}

#[derive(Subcommand)]
enum GamescopeCommand {
    Show,
    Enable,
    Disable,
    Configure(GamescopeArgs),
}

#[derive(Subcommand)]
enum MangohudCommand {
    Show,
    Enable,
    Disable,
}

#[derive(Args)]
struct GamescopeArgs {
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    game_width: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    game_height: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    output_width: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    output_height: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    frame_rate: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    unfocused_frame_rate: Option<u32>,
    #[arg(long, value_enum)]
    scaler: Option<CliGamescopeScaler>,
    #[arg(long, value_enum)]
    filter: Option<CliGamescopeFilter>,
    #[arg(long)]
    sharpness: Option<u8>,
    #[arg(long)]
    borderless: bool,
    #[arg(long)]
    fullscreen: bool,
}

impl GamescopeArgs {
    fn config(self, enabled: bool) -> GamescopeConfig {
        GamescopeConfig {
            enabled,
            game_width: self.game_width,
            game_height: self.game_height,
            output_width: self.output_width,
            output_height: self.output_height,
            frame_rate: self.frame_rate,
            unfocused_frame_rate: self.unfocused_frame_rate,
            scaler: self.scaler.map(Into::into),
            filter: self.filter.map(Into::into),
            sharpness: self.sharpness,
            borderless: self.borderless,
            fullscreen: self.fullscreen,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CliGamescopeScaler {
    Auto,
    Integer,
    Fit,
    Fill,
    Stretch,
}

impl From<CliGamescopeScaler> for GamescopeScaler {
    fn from(value: CliGamescopeScaler) -> Self {
        match value {
            CliGamescopeScaler::Auto => Self::Auto,
            CliGamescopeScaler::Integer => Self::Integer,
            CliGamescopeScaler::Fit => Self::Fit,
            CliGamescopeScaler::Fill => Self::Fill,
            CliGamescopeScaler::Stretch => Self::Stretch,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CliGamescopeFilter {
    Linear,
    Nearest,
    Fsr,
    Nis,
    Pixel,
}

impl From<CliGamescopeFilter> for GamescopeFilter {
    fn from(value: CliGamescopeFilter) -> Self {
        match value {
            CliGamescopeFilter::Linear => Self::Linear,
            CliGamescopeFilter::Nearest => Self::Nearest,
            CliGamescopeFilter::Fsr => Self::Fsr,
            CliGamescopeFilter::Nis => Self::Nis,
            CliGamescopeFilter::Pixel => Self::Pixel,
        }
    }
}

#[derive(Args)]
struct AddProgramArgs {
    name: String,
    executable: String,
    #[arg(long = "arg", allow_hyphen_values = true)]
    arguments: Vec<String>,
    #[arg(long)]
    working_directory: Option<String>,
    #[arg(long)]
    new_console: bool,
}

#[derive(Args)]
struct CreateArgs {
    name: String,

    #[arg(long, value_enum, default_value_t = StorageArg::Standard)]
    storage: StorageArg,

    #[arg(long, value_name = "UUID")]
    runner: String,
}

#[derive(Clone, Copy, ValueEnum)]
enum StorageArg {
    Standard,
    #[cfg(feature = "fvs")]
    Virgo,
}

impl From<StorageArg> for Storage {
    fn from(storage: StorageArg) -> Self {
        match storage {
            StorageArg::Standard => Self::Standard,
            #[cfg(feature = "fvs")]
            StorageArg::Virgo => Self::Virgo,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cli {
        #[cfg(feature = "fvs")]
        fvs2d,
        component_catalog,
        dependency_catalog,
        command,
    } = Cli::parse();

    if matches!(
        &command,
        Command::Plugins {
            command: PluginsCommand::Validate(_)
                | PluginsCommand::Package { .. }
                | PluginsCommand::New { .. }
        }
    ) {
        let Command::Plugins { command } = command else {
            unreachable!();
        };
        return manage_plugin_source(command).await;
    }

    let bottles = Bottles::open(Config {
        #[cfg(feature = "fvs")]
        fvs2d,
        component_catalog,
        dependency_catalog,
    })
    .await?;
    let plugins = PluginManager::open(PluginManagerConfig::new(bottles.bottles().clone())?).await?;

    let result = match command {
        Command::Addons { command } => manage_addons(bottles.addons(), command).await,
        Command::Bottle { command } => match command {
            BottleCommand::Create(args) => create_bottle(&bottles, args).await,
            BottleCommand::List => {
                for bottle in bottles.bottles().list() {
                    let state = bottle.state()?;
                    println!(
                        "{}\t{}\t{:?}\t{}",
                        state.id(),
                        state.name(),
                        state.storage(),
                        state.runner().version()
                    );
                }
                Ok(())
            }
            BottleCommand::Manage(args) => manage_bottle(&bottles, args).await,
        },
        Command::Plugins { command } => manage_plugins(&bottles, &plugins, command).await,
    };

    bottles.close().await?;
    result
}

async fn manage_plugins(
    bottles: &Bottles,
    plugins: &PluginManager,
    command: PluginsCommand,
) -> Result<()> {
    match command {
        PluginsCommand::List => {
            for plugin in plugins.list().await {
                println!(
                    "{}\t{}\t{}\t{:?}",
                    plugin.manifest.id,
                    plugin.manifest.name,
                    plugin.manifest.version,
                    plugin.status
                );
            }
        }
        PluginsCommand::Search { registry, query } => {
            for (id, plugin) in plugins.search_registry(&registry, &query).await? {
                println!(
                    "{}\t{}\t{}\t{}",
                    id, plugin.name, plugin.version, plugin.description
                );
            }
        }
        PluginsCommand::Install(args) => {
            let plugin = plugins
                .install_registry(&args.registry, &args.plugin)
                .await?;
            println!("{}\t{}", plugin.manifest.id, plugin.manifest.version);
        }
        PluginsCommand::Update(args) => {
            let plugin = plugins
                .update_registry(&args.registry, &args.plugin)
                .await?;
            println!("{}\t{}", plugin.manifest.id, plugin.manifest.version);
        }
        PluginsCommand::Settings { plugin, command } => match command {
            PluginSettingsCommand::List => {
                for (key, value) in plugins.plugin_settings(&plugin).await? {
                    println!("{key}\t{value}");
                }
            }
            PluginSettingsCommand::Set { key, value } => {
                plugins.set_plugin_setting(&plugin, key, value).await?;
            }
            PluginSettingsCommand::Unset { key } => {
                plugins.unset_plugin_setting(&plugin, &key).await?;
            }
        },
        PluginsCommand::Roots { command } => match command {
            PluginRootsCommand::List => {
                for (name, path) in plugins.read_roots().await {
                    println!("{name}\t{}", path.display());
                }
            }
            PluginRootsCommand::Add { name, path } => {
                println!("{}", plugins.add_read_root(name, path).await?.display());
            }
            PluginRootsCommand::Remove { name } => {
                plugins.remove_read_root(&name).await?;
            }
        },
        PluginsCommand::Commands { plugin } => {
            for command in plugins.commands().await {
                if plugin
                    .as_ref()
                    .is_some_and(|plugin| plugin != &command.plugin_id)
                {
                    continue;
                }
                println!(
                    "{}/{}\t{}\t{}",
                    command.plugin_id,
                    command.command_id,
                    command.command.title,
                    command.command.usage
                );
            }
        }
        PluginsCommand::Run(args) => {
            let bottle = args
                .bottle
                .map(|bottle| find_bottle(bottles.bottles(), &bottle))
                .transpose()?
                .map(|bottle| bottle.state().map(|state| state.id()))
                .transpose()?;
            println!(
                "{}",
                plugins
                    .invoke(
                        &args.command.plugin,
                        &args.command.command,
                        args.arguments,
                        bottle,
                    )
                    .await?
            );
        }
        PluginsCommand::Enable(args) => {
            plugins.enable(&args.plugin).await?;
        }
        PluginsCommand::Disable(args) => {
            plugins.disable(&args.plugin).await?;
        }
        PluginsCommand::Reload(args) => {
            plugins.reload(&args.plugin).await?;
        }
        PluginsCommand::Retry(args) => {
            plugins.retry(&args.plugin).await?;
        }
        PluginsCommand::Uninstall(args) => {
            plugins.uninstall(&args.plugin).await?;
        }
        PluginsCommand::DevInstall(args) => {
            let plugin = plugins.dev_install(&args.source).await?;
            println!("{}\t{}", plugin.manifest.id, plugin.manifest.version);
        }
        PluginsCommand::DevRebuild(args) => {
            let plugin = plugins.dev_rebuild(&args.plugin).await?;
            println!("{}\t{}", plugin.manifest.id, plugin.manifest.version);
        }
        PluginsCommand::Validate(_)
        | PluginsCommand::Package { .. }
        | PluginsCommand::New { .. } => unreachable!("source command dispatched with core"),
    }
    Ok(())
}

async fn manage_plugin_source(command: PluginsCommand) -> Result<()> {
    match command {
        PluginsCommand::Validate(args) => {
            let manifest = validate_source(&args.source).await?;
            println!("{}\t{}", manifest.id, manifest.version);
        }
        PluginsCommand::Package { source, output } => {
            let package = build_source(&source).await?;
            let output = output.unwrap_or_else(|| {
                PathBuf::from(format!(
                    "{}-{}.tar.gz",
                    package.manifest.id, package.manifest.version
                ))
            });
            write_archive(&package, &output).await?;
            println!("{}", output.display());
        }
        PluginsCommand::New { directory } => create_plugin_project(&directory).await?,
        _ => unreachable!("runtime plugin command dispatched without core"),
    }
    Ok(())
}

async fn create_plugin_project(directory: &std::path::Path) -> Result<()> {
    if tokio::fs::try_exists(directory).await? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", directory.display()),
        )
        .into());
    }
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid plugin directory"))?;
    let manifest = format!(
        r#"schema_version = 1
id = "{id}"
name = "{id}"
version = "0.1.0"
description = "A Bottles plugin"
authors = ["Your Name"]
license = "GPL-3.0"
repository = "https://example.invalid/{id}"
api_version = "{}"

[commands.hello]
title = "Hello"
description = "Say hello"
usage = "hello [name]"
requires_bottle = false
"#,
        PLUGIN_API_VERSION
    );
    PluginManifest::parse(&manifest)?;
    let cargo = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
license = "GPL-3.0"

[lib]
crate-type = ["cdylib"]

[dependencies]
bottles-plugin-api = "{}"
"#,
        id.replace('.', "-"),
        PLUGIN_API_VERSION
    );
    let source = r#"use bottles_plugin_api::{register_plugin, Bottle, Command, Plugin};

struct ExamplePlugin;

impl Plugin for ExamplePlugin {
    fn new() -> Self {
        Self
    }

    fn run_command(
        &mut self,
        _command: Command,
        arguments: Vec<String>,
        _bottle: Option<&Bottle>,
    ) -> Result<String, String> {
        let name = arguments.first().map(String::as_str).unwrap_or("world");
        Ok(format!("Hello, {name}!"))
    }
}

register_plugin!(ExamplePlugin);
"#;

    tokio::fs::create_dir_all(directory.join("src")).await?;
    let result = async {
        tokio::fs::write(directory.join("plugin.toml"), manifest).await?;
        tokio::fs::write(directory.join("Cargo.toml"), cargo).await?;
        tokio::fs::write(directory.join("src/lib.rs"), source).await?;
        tokio::fs::write(directory.join("LICENSE"), include_str!("../LICENSE")).await?;
        tokio::fs::write(directory.join("README.md"), format!("# {id}\n")).await?;
        Result::<()>::Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(directory).await;
    }
    result?;
    println!("{}", directory.display());
    Ok(())
}

async fn manage_addons(addons: &Addons, command: AddonsCommand) -> Result<()> {
    match command {
        AddonsCommand::Refresh => run_operation(addons.refresh()).await?,
        AddonsCommand::List => {
            let mut components = addons
                .components()
                .into_iter()
                .map(|component| (component.id(), component))
                .collect::<HashMap<_, _>>();
            for entry in addons.component_entries() {
                if let Some(component) = components.remove(&entry.id()) {
                    print_component(&component);
                } else {
                    print_component_entry(&entry);
                }
            }
            for component in components.values() {
                print_component(component);
            }

            let mut dependencies = addons
                .dependencies()
                .into_iter()
                .map(|dependency| (dependency.id(), dependency))
                .collect::<HashMap<_, _>>();
            for entry in addons.dependency_entries() {
                if let Some(dependency) = dependencies.remove(&entry.id()) {
                    print_dependency(&dependency);
                } else {
                    print_dependency_entry(&entry);
                }
            }
            for dependency in dependencies.values() {
                print_dependency(dependency);
            }
        }
        AddonsCommand::Download { id } => {
            let id = id.parse()?;
            if addons.component_entry(id).is_some() {
                let component = run_operation(addons.fetch_component(id)).await?;
                print_component(&component);
            } else if addons.dependency_entry(id).is_some() {
                let dependency = run_operation(addons.fetch_dependency(id)).await?;
                print_dependency(&dependency);
            } else {
                return Err(missing("catalog entry", &id.to_string()).into());
            }
        }
        AddonsCommand::Delete { id } => {
            let id = id.parse()?;
            if addons.component(id).is_some() {
                addons.remove_component(id).await?;
            } else if addons.dependency(id).is_some() {
                addons.remove_dependency(id).await?;
            } else {
                return Err(missing("addon", &id.to_string()).into());
            }
        }
    }
    Ok(())
}

async fn create_bottle(bottles: &Bottles, args: CreateArgs) -> Result<()> {
    let runner = find_component(bottles.addons(), &args.runner)?;
    let bottle = run_operation(bottles.bottles().create(
        args.name,
        args.storage.into(),
        runner.id(),
    ))
    .await?;
    print_bottle(&bottle)?;
    Ok(())
}

async fn manage_bottle(bottles: &Bottles, args: ManageArgs) -> Result<()> {
    let bottle = find_bottle(bottles.bottles(), &args.bottle)?;
    match args.command {
        ManageCommand::Show => print_bottle(&bottle)?,
        ManageCommand::Delete => {
            run_operation(bottles.bottles().delete(bottle.state()?.id())).await?;
        }
        ManageCommand::Stop => bottle.stop().await?,
        ManageCommand::Processes => {
            for process in bottle.processes().await? {
                println!("{}\t{}\t{}", process.pid, process.name, process.threads);
            }
        }
        ManageCommand::Install { addon } => {
            let id = addon.parse().map_err(|_| missing("addon", &addon))?;
            if bottles.addons().component(id).is_some() {
                run_operation(bottle.set_component(id)).await?;
            } else if bottles.addons().dependency(id).is_some() {
                run_operation(bottle.install(id)).await?;
            } else {
                return Err(missing("addon", &addon).into());
            }
            print_bottle(&bottle)?;
        }
        ManageCommand::Uninstall { component } => {
            let slot = bottle
                .state()?
                .components()
                .values()
                .find(|installed| installed.id().to_string() == component)
                .map(Addon::slot)
                .ok_or_else(|| missing("installed component", &component))?;
            run_operation(bottle.remove_component(slot)).await?;
            print_bottle(&bottle)?;
        }
        ManageCommand::Program { command } => match command {
            ProgramCommand::Add(args) => {
                let mut program = Program::new(args.name, args.executable);
                program.args = args.arguments;
                program.working_directory = args.working_directory;
                program.new_console = args.new_console;
                let id = program.id;
                let mut edit = bottle.edit();
                edit.add_program(program);
                edit.commit().await?;
                println!("{id}");
            }
            ProgramCommand::Launch { program } => {
                let id = find_program(&bottle, &program)?.id;
                println!("{}", bottle.run(id).await?);
            }
            ProgramCommand::Kill { program } => {
                let id = find_program(&bottle, &program)?.id;
                bottle.kill(id).await?;
            }
        },
        ManageCommand::Env { command } => match command {
            EnvCommand::List => {
                let state = bottle.state()?;
                let mut environment = state.environment().iter().collect::<Vec<_>>();
                environment.sort_unstable_by_key(|(name, _)| *name);
                for (key, value) in environment {
                    println!("{key}={value}");
                }
            }
            EnvCommand::Set { key, value } => {
                let mut edit = bottle.edit();
                edit.set_env(&key, &value);
                edit.commit().await?;
            }
            EnvCommand::Unset { key } => {
                let mut edit = bottle.edit();
                edit.unset_env(&key);
                edit.commit().await?;
            }
        },
        ManageCommand::DllOverrides { command } => manage_dll_overrides(&bottle, command).await?,
        #[cfg(feature = "fvs")]
        ManageCommand::Snapshot { command } => match command {
            SnapshotCommand::Create { message } => {
                println!(
                    "{}",
                    run_operation(bottle.create_snapshot(message))
                        .await?
                        .state_id
                );
            }
            SnapshotCommand::List => {
                for snapshot in bottle.snapshots().await? {
                    println!("{}\t{}", snapshot.state_id, snapshot.message);
                }
            }
            SnapshotCommand::Restore { state } => {
                println!("{}", run_operation(bottle.rollback(&state)).await?);
            }
        },
        ManageCommand::Wrappers { command } => manage_wrappers(&bottle, command).await?,
    }
    Ok(())
}

async fn manage_dll_overrides(bottle: &Bottle, command: DllOverridesCommand) -> Result<()> {
    match command {
        DllOverridesCommand::List => {
            let mut overrides = bottle.dll_overrides().await?;
            overrides.sort_unstable_by(|left, right| left.dll.cmp(&right.dll));
            for dll_override in &overrides {
                print_dll_override(dll_override);
            }
        }
        DllOverridesCommand::Set { dll, mode } => {
            bottle.set_dll_override(dll, mode.into()).await?;
        }
        DllOverridesCommand::Unset { dll } => {
            bottle.unset_dll_override(dll).await?;
        }
    }
    Ok(())
}

fn print_dll_override(dll_override: &DllOverride) {
    println!(
        "{}\t{}",
        dll_override.dll,
        dll_override_mode_name(dll_override.mode())
    );
}

fn dll_override_mode_name(mode: DllOverrideMode) -> &'static str {
    match mode {
        DllOverrideMode::Unspecified => "unspecified",
        DllOverrideMode::NativeBuiltin => "native-builtin",
        DllOverrideMode::BuiltinNative => "builtin-native",
        DllOverrideMode::Native => "native",
        DllOverrideMode::Builtin => "builtin",
        DllOverrideMode::Disabled => "disabled",
    }
}

async fn manage_wrappers(bottle: &Bottle, command: WrappersCommand) -> Result<()> {
    match command {
        WrappersCommand::Gamescope { command } => {
            match command {
                GamescopeCommand::Show => {}
                GamescopeCommand::Enable => {
                    let mut config = bottle.state()?.wrappers().gamescope.clone();
                    config.enabled = true;
                    let mut edit = bottle.edit();
                    edit.set_gamescope(config);
                    edit.commit().await?;
                }
                GamescopeCommand::Disable => {
                    let mut config = bottle.state()?.wrappers().gamescope.clone();
                    config.enabled = false;
                    let mut edit = bottle.edit();
                    edit.set_gamescope(config);
                    edit.commit().await?;
                }
                GamescopeCommand::Configure(args) => {
                    let config = args.config(bottle.state()?.wrappers().gamescope.enabled);
                    let mut edit = bottle.edit();
                    edit.set_gamescope(config);
                    edit.commit().await?;
                }
            }
            println!("{:#?}", bottle.state()?.wrappers().gamescope);
        }
        WrappersCommand::Mangohud { command } => {
            match command {
                MangohudCommand::Show => {}
                MangohudCommand::Enable => {
                    let mut config = bottle.state()?.wrappers().mangohud.clone();
                    config.enabled = true;
                    let mut edit = bottle.edit();
                    edit.set_mangohud(config);
                    edit.commit().await?;
                }
                MangohudCommand::Disable => {
                    let mut config = bottle.state()?.wrappers().mangohud.clone();
                    config.enabled = false;
                    let mut edit = bottle.edit();
                    edit.set_mangohud(config);
                    edit.commit().await?;
                }
            }
            println!("{:#?}", bottle.state()?.wrappers().mangohud);
        }
    }
    Ok(())
}

fn find_component(addons: &Addons, id: &str) -> Result<Arc<IndexEntry<Component>>> {
    let parsed = id.parse().map_err(|_| missing("addon", id))?;
    addons
        .component(parsed)
        .ok_or_else(|| missing("component", id))
        .map_err(Into::into)
}

fn find_program(bottle: &Bottle, id: &str) -> Result<Program> {
    bottle
        .state()?
        .programs()
        .iter()
        .find(|program| program.id.to_string() == id)
        .cloned()
        .ok_or_else(|| missing("program", id))
        .map_err(Into::into)
}

fn find_bottle(bottles: &BottleManager, selector: &str) -> Result<Bottle> {
    for bottle in bottles.list() {
        let state = bottle.state()?;
        if state.id().to_string() == selector || state.name() == selector {
            return Ok(bottle);
        }
    }
    Err(missing("bottle", selector).into())
}

fn missing(kind: &str, value: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{kind} not found: {value}"),
    )
}

fn print_component(addon: &IndexEntry<Component>) {
    println!(
        "{}\t{}\t{}\t{}\tdownloaded",
        addon.id(),
        addon.name(),
        addon.version(),
        addon.slot(),
    );
}

fn print_dependency(addon: &IndexEntry<Dependency>) {
    println!(
        "{}\t{}\t{}\tdependency\tdownloaded",
        addon.id(),
        addon.name(),
        addon.version(),
    );
}

fn print_component_entry(entry: &CatalogEntry<Component>) {
    println!(
        "{}\t{}\t{}\t{}\t{}",
        entry.id(),
        entry.name(),
        entry.version(),
        entry.slot(),
        if entry.is_supported() {
            "downloadable"
        } else {
            "unsupported"
        },
    );
}

fn print_dependency_entry(entry: &CatalogEntry<Dependency>) {
    println!(
        "{}\t{}\t{}\tdependency\t{}",
        entry.id(),
        entry.name(),
        entry.version(),
        if entry.is_supported() {
            "downloadable"
        } else {
            "unsupported"
        },
    );
}

fn print_bottle(bottle: &Bottle) -> Result<()> {
    let state = bottle.state()?;
    println!("id: {}", state.id());
    println!("name: {}", state.name());
    println!("storage: {:?}", state.storage());
    for component in state.components().values() {
        println!(
            "component: {} {} {} {}",
            component.id(),
            component.name(),
            component.version(),
            component.slot()
        );
    }
    for dependency in state.dependencies() {
        println!(
            "dependency: {} {} {}",
            dependency.id(),
            dependency.name(),
            dependency.version()
        );
    }
    for program in state.programs() {
        println!(
            "program: {} {} {}",
            program.id, program.name, program.executable
        );
    }
    Ok(())
}

async fn run_operation<T>(mut operation: Operation<T>) -> Result<T> {
    let mut progress = Box::pin(operation.progress());
    let reporter = tokio::spawn(async move {
        while let Some(progress) = progress.next().await {
            eprintln!("progress: {progress:?}");
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn parse<I, T>(args: I) -> std::result::Result<Cli, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        Cli::try_parse_from(args.into_iter().map(Into::into))
    }

    #[test]
    fn parses_grouped_bottle_commands() {
        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "program",
            "add",
            "Game",
            "C:\\game.exe",
            "--arg",
            "--windowed",
        ])
        .unwrap();

        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        let ManageCommand::Program {
            command: ProgramCommand::Add(program),
        } = args.command
        else {
            panic!("expected program add command");
        };
        assert_eq!(args.bottle, "test");
        assert_eq!(program.arguments, ["--windowed"]);
    }

    #[test]
    fn parses_plugin_run_command() {
        let cli = parse([
            "bottles",
            "plugins",
            "run",
            "example/hello",
            "--bottle",
            "test",
            "--",
            "--verbose",
            "player one",
        ])
        .unwrap();

        let Command::Plugins {
            command: PluginsCommand::Run(args),
        } = cli.command
        else {
            panic!("expected plugin run command");
        };
        assert_eq!(args.command.plugin, "example");
        assert_eq!(args.command.command, "hello");
        assert_eq!(args.bottle.as_deref(), Some("test"));
        assert_eq!(args.arguments, ["--verbose", "player one"]);

        assert!(parse(["bottles", "plugins", "run", "missing-slash"]).is_err());
        assert!(parse(["bottles", "plugins", "run", "a/b/c"]).is_err());
    }

    #[test]
    fn parses_plugin_management_commands() {
        for action in [
            "enable",
            "disable",
            "reload",
            "retry",
            "uninstall",
            "dev-rebuild",
        ] {
            parse(["bottles", "plugins", action, "example"]).unwrap();
        }

        let cli = parse(["bottles", "plugins", "commands", "example"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Plugins {
                command: PluginsCommand::Commands { plugin: Some(plugin) }
            } if plugin == "example"
        ));
    }

    #[test]
    fn parses_plugin_registry_commands() {
        let registry = "https://plugins.example/index.json";
        parse([
            "bottles",
            "plugins",
            "search",
            "wine",
            "--registry",
            registry,
        ])
        .unwrap();
        parse([
            "bottles",
            "plugins",
            "install",
            "example",
            "--registry",
            registry,
        ])
        .unwrap();
        parse([
            "bottles",
            "plugins",
            "update",
            "example",
            "--registry",
            registry,
        ])
        .unwrap();
        assert!(
            parse([
                "bottles",
                "plugins",
                "install",
                "example",
                "--registry",
                "not-a-url",
            ])
            .is_err()
        );
    }

    #[test]
    fn parses_plugin_configuration_commands() {
        for args in [
            vec!["bottles", "plugins", "settings", "example", "list"],
            vec![
                "bottles", "plugins", "settings", "example", "set", "enabled", "true",
            ],
            vec![
                "bottles", "plugins", "settings", "example", "unset", "enabled",
            ],
            vec!["bottles", "plugins", "roots", "list"],
            vec!["bottles", "plugins", "roots", "add", "games", "./games"],
            vec!["bottles", "plugins", "roots", "remove", "games"],
        ] {
            parse(args).unwrap();
        }
    }

    #[test]
    fn parses_plugin_source_commands() {
        for action in ["dev-install", "validate"] {
            parse(["bottles", "plugins", action, "./example"]).unwrap();
        }

        let cli = parse([
            "bottles",
            "plugins",
            "package",
            "./example",
            "--output",
            "example.tar.gz",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Plugins {
                command: PluginsCommand::Package { source, output }
            } if source == PathBuf::from("./example")
                && output == Some(PathBuf::from("example.tar.gz"))
        ));

        let cli = parse(["bottles", "plugins", "new", "./example"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Plugins {
                command: PluginsCommand::New { directory }
            } if directory == PathBuf::from("./example")
        ));
    }

    #[tokio::test]
    async fn creates_source_only_plugin_project() {
        let directory = std::env::temp_dir().join(format!(
            "bottles-cli-plugin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        create_plugin_project(&directory).await.unwrap();
        assert!(directory.join("plugin.toml").is_file());
        assert!(directory.join("Cargo.toml").is_file());
        assert!(directory.join("src/lib.rs").is_file());
        assert!(!directory.join("plugin.wasm").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parses_addons_download_command() {
        let cli = parse(["bottles", "addons", "download", "addon-id"]).unwrap();

        assert!(cli.component_catalog.is_none());
        assert!(cli.dependency_catalog.is_none());
        assert!(matches!(
            cli.command,
            Command::Addons {
                command: AddonsCommand::Download { id }
            } if id == "addon-id"
        ));
    }

    #[test]
    fn parses_program_kill_command() {
        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "program",
            "kill",
            "program-id",
        ])
        .unwrap();

        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(
            args.command,
            ManageCommand::Program {
                command: ProgramCommand::Kill { program }
            } if program == "program-id"
        ));
    }

    #[test]
    fn parses_addon_install_command() {
        let cli = parse(["bottles", "bottle", "manage", "test", "install", "addon-id"]).unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(
            args.command,
            ManageCommand::Install { addon } if addon == "addon-id"
        ));
    }

    #[test]
    fn parses_addon_uninstall_command() {
        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "uninstall",
            "addon-id",
        ])
        .unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(
            args.command,
            ManageCommand::Uninstall { component } if component == "addon-id"
        ));
    }

    #[test]
    fn parses_processes_command() {
        let cli = parse(["bottles", "bottle", "manage", "test", "processes"]).unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(args.command, ManageCommand::Processes));
    }

    #[test]
    fn parses_stop_command() {
        let cli = parse(["bottles", "bottle", "manage", "test", "stop"]).unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(args.command, ManageCommand::Stop));
    }

    #[cfg(feature = "fvs")]
    #[test]
    fn parses_snapshot_commands() {
        for command in [
            &["snapshot", "create", "before-upgrade"][..],
            &["snapshot", "list"][..],
            &["snapshot", "restore", "abc123"][..],
        ] {
            let mut args = vec!["bottles", "bottle", "manage", "test"];
            args.extend_from_slice(command);
            let cli = parse(args).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Snapshot { .. }));
        }
    }

    #[cfg(not(feature = "fvs"))]
    #[test]
    fn rejects_fvs_commands() {
        assert!(parse(["bottles", "--fvs2d", "fvs2d", "bottle", "list"]).is_err());
        assert!(parse(["bottles", "bottle", "manage", "test", "snapshot", "list"]).is_err());
    }

    #[test]
    fn parses_environment_commands_and_empty_values() {
        for command in [&["list"][..], &["unset", "WAYLAND_DISPLAY"][..]] {
            let mut args = vec!["bottles", "bottle", "manage", "test", "env"];
            args.extend_from_slice(command);
            let cli = parse(args).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Env { .. }));
        }

        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "env",
            "set",
            "WAYLAND_DISPLAY",
            "",
        ])
        .unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        assert!(matches!(
            args.command,
            ManageCommand::Env {
                command: EnvCommand::Set { key, value }
            } if key == "WAYLAND_DISPLAY" && value.is_empty()
        ));
    }

    #[test]
    fn parses_dll_override_commands_and_modes() {
        for command in [&["list"][..], &["unset", "d3d11"][..]] {
            let mut args = vec!["bottles", "bottle", "manage", "test", "dll-overrides"];
            args.extend_from_slice(command);
            let cli = parse(args).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::DllOverrides { .. }));
        }

        for (name, expected) in [
            ("native-builtin", DllOverrideMode::NativeBuiltin),
            ("builtin-native", DllOverrideMode::BuiltinNative),
            ("native", DllOverrideMode::Native),
            ("builtin", DllOverrideMode::Builtin),
            ("disabled", DllOverrideMode::Disabled),
        ] {
            let cli = parse([
                "bottles",
                "bottle",
                "manage",
                "test",
                "dll-overrides",
                "set",
                "d3d11",
                name,
            ])
            .unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(
                args.command,
                ManageCommand::DllOverrides {
                    command: DllOverridesCommand::Set { dll, mode }
                } if dll == "d3d11" && DllOverrideMode::from(mode) == expected
            ));
        }
    }

    #[test]
    fn parses_wrapper_commands_and_validates_gamescope_values() {
        for command in [
            &["gamescope", "show"][..],
            &["gamescope", "enable"][..],
            &["gamescope", "disable"][..],
            &["mangohud", "show"][..],
            &["mangohud", "enable"][..],
            &["mangohud", "disable"][..],
        ] {
            let mut args = vec!["bottles", "bottle", "manage", "test", "wrappers"];
            args.extend_from_slice(command);
            let cli = parse(args).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Wrappers { .. }));
        }

        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "wrappers",
            "gamescope",
            "configure",
            "--game-width",
            "1280",
            "--game-height",
            "720",
            "--output-width",
            "1920",
            "--output-height",
            "1080",
            "--frame-rate",
            "60",
            "--unfocused-frame-rate",
            "30",
            "--scaler",
            "fit",
            "--filter",
            "fsr",
            "--sharpness",
            "5",
            "--borderless",
            "--fullscreen",
        ])
        .unwrap();
        let Command::Bottle {
            command: BottleCommand::Manage(args),
        } = cli.command
        else {
            panic!("expected bottle manage command");
        };
        let ManageCommand::Wrappers {
            command:
                WrappersCommand::Gamescope {
                    command: GamescopeCommand::Configure(args),
                },
        } = args.command
        else {
            panic!("expected gamescope configure command");
        };
        assert_eq!(args.game_width, Some(1280));
        assert_eq!(args.unfocused_frame_rate, Some(30));
        assert!(args.borderless);
        assert!(args.fullscreen);

        assert!(
            parse([
                "bottles",
                "bottle",
                "manage",
                "test",
                "wrappers",
                "gamescope",
                "configure",
                "--game-width",
                "0",
            ])
            .is_err()
        );
    }
}
