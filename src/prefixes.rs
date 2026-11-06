use bottles_core::{
    Addon, Bottles, Component, Dependency, DllOverrideMode, EnvVars, GamescopeConfig,
    GamescopeFilter, GamescopeScaler, Prefix, PrefixBackend, ProgramSpec, Runner, Umu, WineBridge,
    Wrappers,
};
use clap::{Args, Subcommand, ValueEnum};
use uuid::Uuid;

use crate::{Result, local_addon, missing, run_operation};

type RuntimeSelection = (Addon<Runner>, Addon<WineBridge>, Option<Addon<Umu>>);

#[derive(Subcommand)]
pub enum Command {
    /// Create a prefix from acquired runtime releases.
    Create(CreatePrefix),
    /// List saved prefixes.
    List,
    /// Manage a prefix by UUID.
    Manage {
        prefix: Uuid,
        #[command(subcommand)]
        command: PrefixCommand,
    },
}

#[derive(Args)]
pub struct CreatePrefix {
    #[arg(long, value_enum, default_value_t = Storage::Standard)]
    storage: Storage,
    #[command(flatten)]
    runtime: RuntimeArgs,
}

#[derive(Args)]
struct RuntimeArgs {
    /// UUID of a locally acquired runner.
    #[arg(long)]
    runner: Uuid,
    /// UUID of a locally acquired WineBridge release.
    #[arg(long)]
    winebridge: Uuid,
    /// UUID of a locally acquired UMU release, if required by the runner.
    #[arg(long)]
    umu: Option<Uuid>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Storage {
    Standard,
    #[cfg(feature = "fvs")]
    Virgo,
}

impl From<Storage> for PrefixBackend {
    fn from(value: Storage) -> Self {
        match value {
            Storage::Standard => Self::Standard,
            #[cfg(feature = "fvs")]
            Storage::Virgo => Self::Virgo,
        }
    }
}

#[derive(Args)]
pub struct LaunchArgs {
    /// Process group identifier; a new one is generated when omitted.
    #[arg(long)]
    group: Option<Uuid>,
    executable: String,
    #[arg(long = "arg", allow_hyphen_values = true)]
    arguments: Vec<String>,
    #[arg(long)]
    working_directory: Option<String>,
    #[arg(long)]
    new_console: bool,
}

impl LaunchArgs {
    fn into_spec(self) -> ProgramSpec {
        let spec = ProgramSpec::new(self.executable)
            .with_args(self.arguments)
            .with_new_console(self.new_console);
        match self.working_directory {
            Some(directory) => spec.with_working_directory(directory),
            None => spec,
        }
    }
}

#[derive(Subcommand)]
pub enum PrefixCommand {
    /// Show the saved prefix state.
    Show,
    /// Launch a Windows executable and print its process group and ID.
    Launch(LaunchArgs),
    /// Terminate a process group.
    Kill { group: Uuid },
    /// Delete this prefix and its managed data.
    Delete,
    /// Stop Wine and release mounted storage.
    Stop,
    /// List processes reported by WineBridge.
    Processes,
    /// Select an acquired runtime, component, or dependency by UUID.
    Install { addon: Uuid },
    /// Clear an installed component or UMU selection by UUID.
    Uninstall { component: Uuid },
    /// List or edit user-set environment variables.
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    /// List or edit Wine DLL overrides.
    DllOverrides {
        #[command(subcommand)]
        command: DllCommand,
    },
    /// Create, list, or restore Virgo snapshots.
    #[cfg(feature = "fvs")]
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// Configure host command wrappers.
    Wrappers {
        #[command(subcommand)]
        command: WrapperCommand,
    },
}

#[derive(Subcommand)]
pub enum EnvCommand {
    List,
    Set { key: String, value: String },
    Unset { key: String },
}

#[derive(Subcommand)]
pub enum DllCommand {
    List,
    Set {
        dll: String,
        #[arg(value_enum)]
        mode: DllMode,
    },
    Unset {
        dll: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum DllMode {
    NativeBuiltin,
    BuiltinNative,
    Native,
    Builtin,
    Disabled,
}

impl From<DllMode> for DllOverrideMode {
    fn from(value: DllMode) -> Self {
        match value {
            DllMode::NativeBuiltin => Self::NativeBuiltin,
            DllMode::BuiltinNative => Self::BuiltinNative,
            DllMode::Native => Self::Native,
            DllMode::Builtin => Self::Builtin,
            DllMode::Disabled => Self::Disabled,
        }
    }
}

#[cfg(feature = "fvs")]
#[derive(Subcommand)]
pub enum SnapshotCommand {
    Create { message: String },
    List,
    Restore { state: String },
}

#[derive(Subcommand)]
pub enum WrapperCommand {
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
pub enum GamescopeCommand {
    Show,
    Enable,
    Disable,
    Configure(GamescopeArgs),
}
#[derive(Subcommand)]
pub enum MangohudCommand {
    Show,
    Enable,
    Disable,
}

#[derive(Args)]
pub struct GamescopeArgs {
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
    scaler: Option<Scaler>,
    #[arg(long, value_enum)]
    filter: Option<Filter>,
    #[arg(long)]
    sharpness: Option<u8>,
    #[arg(long)]
    borderless: bool,
    #[arg(long)]
    fullscreen: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Scaler {
    Auto,
    Integer,
    Fit,
    Fill,
    Stretch,
}
#[derive(Clone, Copy, ValueEnum)]
pub enum Filter {
    Linear,
    Nearest,
    Fsr,
    Nis,
    Pixel,
}

impl GamescopeArgs {
    fn apply(self, config: &mut GamescopeConfig) {
        if let Some(value) = self.game_width {
            config.game_width = Some(value);
        }
        if let Some(value) = self.game_height {
            config.game_height = Some(value);
        }
        if let Some(value) = self.output_width {
            config.output_width = Some(value);
        }
        if let Some(value) = self.output_height {
            config.output_height = Some(value);
        }
        if let Some(value) = self.frame_rate {
            config.frame_rate = Some(value);
        }
        if let Some(value) = self.unfocused_frame_rate {
            config.unfocused_frame_rate = Some(value);
        }
        if let Some(value) = self.scaler {
            config.scaler = Some(match value {
                Scaler::Auto => GamescopeScaler::Auto,
                Scaler::Integer => GamescopeScaler::Integer,
                Scaler::Fit => GamescopeScaler::Fit,
                Scaler::Fill => GamescopeScaler::Fill,
                Scaler::Stretch => GamescopeScaler::Stretch,
            });
        }
        if let Some(value) = self.filter {
            config.filter = Some(match value {
                Filter::Linear => GamescopeFilter::Linear,
                Filter::Nearest => GamescopeFilter::Nearest,
                Filter::Fsr => GamescopeFilter::Fsr,
                Filter::Nis => GamescopeFilter::Nis,
                Filter::Pixel => GamescopeFilter::Pixel,
            });
        }
        if let Some(value) = self.sharpness {
            config.sharpness = Some(value);
        }
        if self.borderless {
            config.borderless = true;
        }
        if self.fullscreen {
            config.fullscreen = true;
        }
    }
}

pub async fn run(bottles: &Bottles, command: Command) -> Result<()> {
    match command {
        Command::Create(args) => {
            let (runner, winebridge, umu) = runtime(bottles, args.runtime)?;
            let prefix = run_operation(bottles.prefixes().create(
                args.storage.into(),
                runner,
                winebridge,
                umu,
            ))
            .await?;
            show(&prefix)?;
        }
        Command::List => {
            for (id, state) in bottles.prefixes().inventory().iter() {
                println!("{id}\t{:?}\t{}", state.backend, state.runner.version());
            }
        }
        Command::Manage { prefix, command } => {
            let prefix = bottles.prefixes().open(prefix)?;
            manage(bottles, &prefix, command).await?
        }
    }
    Ok(())
}

fn runtime(bottles: &Bottles, args: RuntimeArgs) -> Result<RuntimeSelection> {
    let state = bottles.addons().state();
    Ok((
        local_addon(&state, args.runner)?,
        local_addon(&state, args.winebridge)?,
        args.umu.map(|id| local_addon(&state, id)).transpose()?,
    ))
}

fn show(prefix: &Prefix) -> Result<()> {
    let state = prefix.state()?;
    println!(
        "id: {}\nstorage: {:?}\nrunner: {}\nwinebridge: {}",
        prefix.id(),
        state.backend,
        state.runner.id(),
        state.winebridge.id()
    );
    if let Some(umu) = &state.umu {
        println!("umu: {}", umu.id());
    }
    for item in &state.components {
        println!(
            "component: {} {} {} {}",
            item.id(),
            item.name(),
            item.version(),
            item.slot()
        );
    }
    for item in &state.dependencies {
        println!(
            "dependency: {} {} {}",
            item.id(),
            item.name(),
            item.version()
        );
    }
    let mut vars: Vec<_> = state.env_vars.iter().collect();
    vars.sort_unstable_by_key(|(key, _)| *key);
    for (key, value) in vars {
        println!("env: {key}={value}");
    }
    Ok(())
}

async fn manage(bottles: &Bottles, target: &Prefix, command: PrefixCommand) -> Result<()> {
    match command {
        PrefixCommand::Show => show(target)?,
        PrefixCommand::Launch(args) => {
            let group = args.group.unwrap_or_else(Uuid::new_v4);
            let pid = run_operation(target.launch(group, args.into_spec())).await?;
            println!("{group}\t{pid}");
        }
        PrefixCommand::Kill { group } => target.kill(group).await?,
        PrefixCommand::Delete => {
            run_operation(bottles.prefixes().delete(target.id())).await?;
        }
        PrefixCommand::Stop => target.stop().await?,
        PrefixCommand::Processes => {
            for process in target.processes().await? {
                println!("{}\t{}\t{}", process.pid, process.name, process.threads);
            }
        }
        PrefixCommand::Install { addon } => {
            install(bottles, target, addon).await?;
            show(target)?;
        }
        PrefixCommand::Uninstall { component } => {
            uninstall(target, component).await?;
            show(target)?;
        }
        PrefixCommand::Env { command } => manage_env(target, command).await?,
        PrefixCommand::DllOverrides { command } => manage_dll(target, command).await?,
        #[cfg(feature = "fvs")]
        PrefixCommand::Snapshot { command } => manage_snapshot(target, command).await?,
        PrefixCommand::Wrappers { command } => manage_wrappers(target, command).await?,
    }
    Ok(())
}

async fn install(bottles: &Bottles, target: &Prefix, id: Uuid) -> Result<()> {
    let state = bottles.addons().state();
    if let Some(runner) = state.release::<Runner>(id) {
        run_operation(target.edit(move |edit| {
            edit.set_runner(runner);
            Ok(())
        }))
        .await?;
    } else if let Some(winebridge) = state.release::<WineBridge>(id) {
        run_operation(target.edit(move |edit| {
            edit.set_winebridge(winebridge);
            Ok(())
        }))
        .await?;
    } else if let Some(umu) = state.release::<Umu>(id) {
        run_operation(target.edit(move |edit| {
            edit.set_umu(Some(umu));
            Ok(())
        }))
        .await?;
    } else if let Some(component) = state.release::<Component>(id) {
        run_operation(target.edit(move |edit| {
            edit.set_component(component);
            Ok(())
        }))
        .await?;
    } else if let Some(dependency) = state.release::<Dependency>(id) {
        run_operation(target.edit(move |edit| {
            edit.add_dependency(dependency);
            Ok(())
        }))
        .await?;
    } else {
        return Err(missing("addon", id).into());
    }
    Ok(())
}

async fn uninstall(target: &Prefix, id: Uuid) -> Result<()> {
    let settings = target.state()?;
    if settings.umu.as_ref().is_some_and(|item| item.id() == id) {
        run_operation(target.edit(|edit| {
            edit.set_umu(None);
            Ok(())
        }))
        .await?;
    } else {
        let slot = settings
            .components
            .iter()
            .find(|item| item.id() == id)
            .map(Addon::slot)
            .ok_or_else(|| missing("installed component", id))?;
        run_operation(target.edit(move |edit| edit.remove_component(slot))).await?;
    }
    Ok(())
}

async fn manage_env(target: &Prefix, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::List => {
            let settings = target.state()?;
            let mut vars: Vec<_> = settings.env_vars.iter().collect();
            vars.sort_unstable_by_key(|(key, _)| *key);
            for (key, value) in vars {
                println!("{key}={value}");
            }
        }
        EnvCommand::Set { key, value } => {
            edit_vars(target, move |vars| {
                vars.insert(key, value);
            })
            .await?;
        }
        EnvCommand::Unset { key } => {
            edit_vars(target, move |vars| {
                vars.remove(&key);
            })
            .await?;
        }
    }
    Ok(())
}

async fn edit_vars(
    target: &Prefix,
    change: impl FnOnce(&mut EnvVars) + Send + 'static,
) -> Result<()> {
    run_operation(target.edit(move |edit| {
        change(edit.env_vars());
        Ok(())
    }))
    .await?;
    Ok(())
}

async fn manage_dll(target: &Prefix, command: DllCommand) -> Result<()> {
    match command {
        DllCommand::List => {
            let mut overrides = run_operation(target.dll_overrides()).await?;
            overrides.sort_unstable_by(|left, right| left.dll.cmp(&right.dll));
            for item in overrides {
                let mode = match item.mode() {
                    DllOverrideMode::Unspecified => "unspecified",
                    DllOverrideMode::NativeBuiltin => "native-builtin",
                    DllOverrideMode::BuiltinNative => "builtin-native",
                    DllOverrideMode::Native => "native",
                    DllOverrideMode::Builtin => "builtin",
                    DllOverrideMode::Disabled => "disabled",
                };
                println!("{}\t{mode}", item.dll);
            }
        }
        DllCommand::Set { dll, mode } => {
            run_operation(target.set_dll_override(dll, mode.into())).await?;
        }
        DllCommand::Unset { dll } => {
            run_operation(target.unset_dll_override(dll)).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "fvs")]
async fn manage_snapshot(target: &Prefix, command: SnapshotCommand) -> Result<()> {
    match command {
        SnapshotCommand::Create { message } => {
            let snapshot = run_operation(target.create_snapshot(message)).await?;
            println!("{}", snapshot.state_id);
        }
        SnapshotCommand::List => {
            for snapshot in target.snapshots().await? {
                println!("{}\t{}", snapshot.state_id, snapshot.message);
            }
        }
        SnapshotCommand::Restore { state } => {
            println!("{}", run_operation(target.rollback(&state)).await?);
        }
    }
    Ok(())
}

async fn manage_wrappers(target: &Prefix, command: WrapperCommand) -> Result<()> {
    match command {
        WrapperCommand::Gamescope { command } => {
            match command {
                GamescopeCommand::Show => {}
                GamescopeCommand::Enable => {
                    edit_wrappers(target, |state| state.gamescope.enabled = true).await?
                }
                GamescopeCommand::Disable => {
                    edit_wrappers(target, |state| state.gamescope.enabled = false).await?
                }
                GamescopeCommand::Configure(args) => {
                    edit_wrappers(target, move |state| args.apply(&mut state.gamescope)).await?
                }
            }
            println!("{:#?}", target.state()?.wrappers.gamescope);
        }
        WrapperCommand::Mangohud { command } => {
            match command {
                MangohudCommand::Show => {}
                MangohudCommand::Enable => {
                    edit_wrappers(target, |state| state.mangohud.enabled = true).await?
                }
                MangohudCommand::Disable => {
                    edit_wrappers(target, |state| state.mangohud.enabled = false).await?
                }
            }
            println!("{:#?}", target.state()?.wrappers.mangohud);
        }
    }
    Ok(())
}

async fn edit_wrappers(
    target: &Prefix,
    change: impl FnOnce(&mut Wrappers) + Send + 'static,
) -> Result<()> {
    run_operation(target.edit(move |edit| {
        change(edit.wrappers());
        Ok(())
    }))
    .await?;
    Ok(())
}
