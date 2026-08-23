use std::{
    collections::HashMap,
    error::Error,
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use bottles_core::{
    AccountLinkInteraction, Addon, Addons, Bottle, BottleManager, Bottles, CatalogEntry, Component,
    Config, Dependency, DllOverride, DllOverrideMode, GamescopeConfig, GamescopeFilter,
    GamescopeScaler, IndexEntry, Library, Operation, PluginId, PluginInfo, Plugins, Profile,
    Profiles, Program, SearchSource, Storage,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::StreamExt;
use tokio::io::AsyncBufReadExt;
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
    /// Search installed programs and games owned through linked storefronts.
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Inspect and manage installed plugins.
    Plugins {
        #[command(subcommand)]
        command: PluginsCommand,
    },
    /// Inspect profiles and manage their storefront accounts.
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    Search {
        #[arg(value_name = "QUERY")]
        query: String,
    },
}

#[derive(Subcommand)]
enum PluginsCommand {
    List,
    /// Builds and installs a plugin source checkout.
    DevInstall {
        #[arg(value_name = "DIRECTORY")]
        source: PathBuf,
    },
    Reload {
        #[arg(value_name = "PLUGIN")]
        plugin: PluginId,
    },
    Uninstall {
        #[arg(value_name = "PLUGIN")]
        plugin: PluginId,
    },
}

#[derive(Subcommand)]
enum ProfilesCommand {
    List,
    /// Shows the selected profile, or a profile selected by UUID or name.
    Show {
        #[arg(value_name = "PROFILE")]
        profile: Option<String>,
    },
    /// Lists available account providers.
    Providers,
    /// Links an account to the selected profile, or to `--profile`.
    Link(AccountArgs),
    /// Unlinks an account without requiring its provider to be loaded.
    Unlink(AccountArgs),
}

#[derive(Args)]
struct AccountArgs {
    #[arg(value_name = "PROVIDER")]
    provider: PluginId,
    /// Profile UUID or name; defaults to the selected profile.
    #[arg(long)]
    profile: Option<String>,
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
    let bottles = Bottles::open(Config {
        #[cfg(feature = "fvs")]
        fvs2d,
        #[cfg(not(feature = "fvs"))]
        fvs2d: None,
        component_catalog,
        dependency_catalog,
    })
    .await?;
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
        Command::Library { command } => manage_library(bottles.library(), command).await,
        Command::Plugins { command } => manage_plugins(bottles.plugins(), command).await,
        Command::Profiles { command } => manage_profiles(bottles.profiles(), command).await,
    };

    bottles.close().await?;
    result
}

async fn manage_library(library: &Library, command: LibraryCommand) -> Result<()> {
    let LibraryCommand::Search { query } = command;
    let mut entries = Box::pin(library.search(query));
    while let Some(entry) = entries.next().await {
        match entry.source() {
            SearchSource::Installed(_) => {
                println!("{}\t{}\tinstalled", entry.title(), entry.source_name());
            }
            SearchSource::Storefront {
                profile_id,
                provider_id,
                game_id,
            } => println!(
                "{}\t{}\tstorefront\t{}\t{}\t{}",
                entry.title(),
                entry.source_name(),
                provider_id,
                profile_id,
                game_id
            ),
            _ => unreachable!("unknown library search source"),
        }
    }
    Ok(())
}

async fn manage_plugins(plugins: &Plugins, command: PluginsCommand) -> Result<()> {
    match command {
        PluginsCommand::List => {
            for plugin in plugins.list() {
                print_plugin(&plugin);
            }
        }
        PluginsCommand::DevInstall { source } => {
            print_plugin(&dev_install(plugins, &source).await?);
        }
        PluginsCommand::Reload { plugin } => plugins.reload(&plugin).await?,
        PluginsCommand::Uninstall { plugin } => plugins.uninstall(&plugin).await?,
    }
    Ok(())
}

async fn dev_install(plugins: &Plugins, source: &std::path::Path) -> Result<PluginInfo> {
    let cargo_manifest = tokio::fs::read_to_string(source.join("Cargo.toml")).await?;
    let target = source_target_name(&cargo_manifest).map_err(std::io::Error::other)?;
    let temporary = tempfile::tempdir()?;
    let target_directory = temporary.path().join("target");
    let output = tokio::process::Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-wasip2"])
        .arg("--manifest-path")
        .arg(source.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_directory)
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        return Err(format!(
            "plugin build failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let package = temporary.path().join("package");
    tokio::fs::create_dir(&package).await?;
    tokio::fs::copy(source.join("plugin.toml"), package.join("plugin.toml")).await?;
    tokio::fs::copy(
        target_directory
            .join("wasm32-wasip2")
            .join("release")
            .join(target)
            .with_extension("wasm"),
        package.join("plugin.wasm"),
    )
    .await?;
    Ok(plugins.install(&package).await?)
}

fn source_target_name(source: &str) -> std::result::Result<String, String> {
    let cargo: toml::Value = toml::from_str(source).map_err(|error| error.to_string())?;
    cargo
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            cargo
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
        })
        .map(|name| name.replace('-', "_"))
        .ok_or_else(|| "Cargo.toml has no package name".into())
}

async fn manage_profiles(profiles: &Profiles, command: ProfilesCommand) -> Result<()> {
    match command {
        ProfilesCommand::List => {
            let selected = profiles.selected().id();
            for profile in profiles.list() {
                print_profile(&profile, profile.id() == selected);
            }
        }
        ProfilesCommand::Show { profile } => {
            let profile = find_profile(profiles, profile.as_deref())?;
            print_profile(&profile, profile.id() == profiles.selected().id());
        }
        ProfilesCommand::Providers => {
            for provider in profiles.account_providers() {
                println!("{}\t{}", provider.id, provider.name);
            }
        }
        ProfilesCommand::Link(args) => {
            let profile = find_profile(profiles, args.profile.as_deref())?;
            let profile = profiles
                .link_account(
                    profile.id(),
                    args.provider,
                    Arc::new(TerminalAccountLinkInteraction),
                )
                .await?;
            print_profile(&profile, profile.id() == profiles.selected().id());
        }
        ProfilesCommand::Unlink(args) => {
            let profile = find_profile(profiles, args.profile.as_deref())?;
            let profile = profiles.unlink_account(profile.id(), args.provider).await?;
            print_profile(&profile, profile.id() == profiles.selected().id());
        }
    }
    Ok(())
}

struct TerminalAccountLinkInteraction;

#[async_trait]
impl AccountLinkInteraction for TerminalAccountLinkInteraction {
    async fn request_input(
        &self,
        url: Url,
        instructions: String,
    ) -> std::result::Result<String, String> {
        println!("{instructions}\n{url}");
        print!("Authorization code: ");
        io::stdout().flush().map_err(|error| error.to_string())?;

        let mut input = String::new();
        let read = tokio::io::BufReader::new(tokio::io::stdin())
            .read_line(&mut input)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("standard input closed".into());
        }
        Ok(input.trim().to_owned())
    }
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
                let mut program = Program::new(args.name, args.executable)?
                    .with_args(args.arguments)
                    .with_new_console(args.new_console);
                if let Some(working_directory) = args.working_directory {
                    program = program.with_working_directory(working_directory)?;
                }
                let id = program.id();
                let mut edit = bottle.edit();
                edit.add_program(program);
                edit.commit().await?;
                println!("{id}");
            }
            ProgramCommand::Launch { program } => {
                let id = find_program(&bottle, &program)?.id();
                println!("{}", bottle.launch_program(id).await?);
            }
            ProgramCommand::Kill { program } => {
                let id = find_program(&bottle, &program)?.id();
                bottle.kill_program(id).await?;
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
        .find(|program| program.id().to_string() == id)
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

fn find_profile(profiles: &Profiles, selector: Option<&str>) -> Result<Profile> {
    let Some(selector) = selector else {
        return Ok(profiles.selected());
    };
    profiles
        .list()
        .into_iter()
        .find(|profile| profile.id().to_string() == selector || profile.name() == selector)
        .ok_or_else(|| missing("profile", selector).into())
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
            program.id(),
            program.name(),
            program.executable()
        );
    }
    Ok(())
}

fn print_plugin(plugin: &PluginInfo) {
    println!(
        "{}\t{}\t{}",
        plugin.manifest.id, plugin.manifest.name, plugin.manifest.version
    );
}

fn print_profile(profile: &Profile, selected: bool) {
    println!(
        "{}\t{}{}",
        profile.id(),
        profile.name(),
        if selected { "\tselected" } else { "" }
    );
    for account in profile.accounts() {
        println!(
            "account\t{}\t{}\t{}\t{}",
            account.provider.id,
            account.provider.name,
            account.identity.account_id,
            account.identity.display_name
        );
    }
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
    fn parses_plugin_commands() {
        let cli = parse([
            "bottles",
            "plugins",
            "dev-install",
            "plugins/random-storefront",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Plugins {
                command: PluginsCommand::DevInstall { source }
            } if source == PathBuf::from("plugins/random-storefront")
        ));
        assert!(matches!(
            parse(["bottles", "plugins", "reload", "epic-games-store"]).unwrap().command,
            Command::Plugins {
                command: PluginsCommand::Reload { plugin }
            } if plugin.as_str() == "epic-games-store"
        ));
        assert!(matches!(
            parse(["bottles", "plugins", "uninstall", "epic-games-store"])
                .unwrap()
                .command,
            Command::Plugins {
                command: PluginsCommand::Uninstall { plugin }
            } if plugin.as_str() == "epic-games-store"
        ));
    }

    #[test]
    fn parses_library_search() {
        assert!(matches!(
            parse(["bottles", "library", "search", "fortnite"])
                .unwrap()
                .command,
            Command::Library {
                command: LibraryCommand::Search { query }
            } if query == "fortnite"
        ));
    }

    #[test]
    fn derives_the_wasm_artifact_name() {
        assert_eq!(
            source_target_name("[package]\nname = \"epic-games\"").unwrap(),
            "epic_games"
        );
        assert_eq!(
            source_target_name("[package]\nname = \"epic-games\"\n[lib]\nname = \"custom_guest\"")
                .unwrap(),
            "custom_guest"
        );
    }

    #[test]
    fn parses_profile_account_commands() {
        let cli = parse([
            "bottles",
            "profiles",
            "link",
            "epic-games-store",
            "--profile",
            "Player",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Profiles {
                command: ProfilesCommand::Link(AccountArgs { provider, profile })
            } if provider.to_string() == "epic-games-store"
                && profile.as_deref() == Some("Player")
        ));
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
