#[cfg(feature = "fvs")]
use bottles_core::Program;
use bottles_core::{
    Addon, Bottle, Bottles, DllOverrideMode, EnvVars, EnvironmentConfig, GamescopeConfig,
    GamescopeFilter, GamescopeScaler, Manager, PrefixBackend, ProgramSpec, Runner, Umu, WineBridge,
    Wrappers,
};
use clap::{Args, Subcommand, ValueEnum};
use uuid::Uuid;

use crate::{Result, local_addon, missing, run_operation};

type RuntimeSelection = (Addon<Runner>, Addon<WineBridge>, Option<Addon<Umu>>);

#[derive(Subcommand)]
pub enum BottleCommand {
    /// Create a bottle from acquired runtime releases.
    Create(CreateBottle),
    /// List saved bottles.
    List,
    /// Manage a bottle by UUID or name.
    Manage {
        bottle: String,
        #[command(subcommand)]
        command: BottleManage,
    },
}

#[derive(Args)]
pub struct CreateBottle {
    name: String,
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

#[derive(Subcommand)]
pub enum BottleManage {
    /// Manage launch definitions registered inside this bottle.
    Program {
        #[command(subcommand)]
        command: BottleProgram,
    },
    #[command(flatten)]
    Environment(EnvironmentCommand),
}

#[derive(Subcommand)]
pub enum BottleProgram {
    /// Register a launch definition.
    Add(LaunchArgs),
    /// Replace a registered launch definition.
    Edit {
        program: Uuid,
        #[command(flatten)]
        launch: LaunchArgs,
    },
    /// Remove a registered launch definition.
    Remove { program: Uuid },
    /// Launch a registered program and print its process ID.
    Launch { program: Uuid },
    /// Terminate a registered program's process group.
    Kill { program: Uuid },
}

#[cfg(feature = "fvs")]
#[derive(Subcommand)]
pub enum ProgramCommand {
    /// Create a standalone Virgo program.
    Create(CreateProgram),
    /// List standalone programs.
    List,
    /// Manage a standalone program by UUID or name.
    Manage {
        program: String,
        #[command(subcommand)]
        command: ProgramManage,
    },
}

#[cfg(feature = "fvs")]
#[derive(Args)]
pub struct CreateProgram {
    #[command(flatten)]
    launch: LaunchArgs,
    #[command(flatten)]
    runtime: RuntimeArgs,
}

#[cfg(feature = "fvs")]
#[derive(Subcommand)]
pub enum ProgramManage {
    /// Launch the program and print its process ID.
    Launch,
    /// Terminate the program's process group.
    Kill,
    /// Replace the program's launch definition.
    Edit(LaunchArgs),
    #[command(flatten)]
    Environment(EnvironmentCommand),
}

#[derive(Args)]
pub struct LaunchArgs {
    name: String,
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
        let spec = ProgramSpec::new(self.name, self.executable)
            .with_args(self.arguments)
            .with_new_console(self.new_console);
        match self.working_directory {
            Some(directory) => spec.with_working_directory(directory),
            None => spec,
        }
    }
}

#[derive(Subcommand)]
pub enum EnvironmentCommand {
    /// Show the saved environment configuration.
    Show,
    /// Change the environment's display name.
    Rename { name: String },
    /// Delete this environment and its managed data.
    Delete,
    /// Stop Wine and release mounted storage.
    Stop,
    /// List processes reported by WineBridge.
    Processes,
    /// Select an acquired runtime, component, or dependency by UUID.
    Install { addon: Uuid },
    /// Clear an installed component or UMU selection by UUID.
    Uninstall { component: Uuid },
    /// List or edit owner-level environment variables.
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

enum Target {
    Bottle(Bottle),
    #[cfg(feature = "fvs")]
    Program(Program),
}

macro_rules! owner {
    ($target:expr, |$owner:ident| $body:expr) => {
        match $target {
            Target::Bottle($owner) => $body,
            #[cfg(feature = "fvs")]
            Target::Program($owner) => $body,
        }
    };
}

pub async fn run_bottle(bottles: &Bottles, command: BottleCommand) -> Result<()> {
    match command {
        BottleCommand::Create(args) => {
            let (runner, winebridge, umu) = runtime(bottles, args.runtime)?;
            let bottle = run_operation(bottles.bottles().create(
                args.name,
                args.storage.into(),
                runner,
                winebridge,
                umu,
            ))
            .await?;
            show(&Target::Bottle(bottle))?;
        }
        BottleCommand::List => {
            let mut states = bottles
                .bottles()
                .list()
                .into_iter()
                .map(|bottle| bottle.state())
                .collect::<bottles_core::error::Result<Vec<_>>>()?;
            states.sort_unstable_by_key(|state| state.id());
            for state in states {
                println!(
                    "{}\t{}\t{:?}\t{}",
                    state.id(),
                    state.name(),
                    state.backend(),
                    state.config().runner.version()
                );
            }
        }
        BottleCommand::Manage { bottle, command } => {
            let bottle = find_bottle(bottles.bottles(), &bottle)?;
            match command {
                BottleManage::Program { command } => {
                    manage_bottle_program(&bottle, command).await?
                }
                BottleManage::Environment(command) => {
                    manage_environment(bottles, Target::Bottle(bottle), command).await?
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "fvs")]
pub async fn run_program(bottles: &Bottles, command: ProgramCommand) -> Result<()> {
    match command {
        ProgramCommand::Create(args) => {
            let (runner, winebridge, umu) = runtime(bottles, args.runtime)?;
            let program = run_operation(bottles.programs().create(
                args.launch.into_spec(),
                runner,
                winebridge,
                umu,
            ))
            .await?;
            show(&Target::Program(program))?;
        }
        ProgramCommand::List => {
            let mut states = bottles
                .programs()
                .list()
                .into_iter()
                .map(|program| program.state())
                .collect::<bottles_core::error::Result<Vec<_>>>()?;
            states.sort_unstable_by_key(|state| state.id());
            for state in states {
                println!(
                    "{}\t{}\t{}",
                    state.id(),
                    state.name(),
                    state.launch().executable()
                );
            }
        }
        ProgramCommand::Manage { program, command } => {
            let program = find_program(bottles.programs(), &program)?;
            match command {
                ProgramManage::Launch => println!("{}", run_operation(program.launch()).await?),
                ProgramManage::Kill => program.kill().await?,
                ProgramManage::Edit(args) => {
                    let spec = args.into_spec();
                    run_operation(program.edit(move |edit| {
                        *edit.launch() = spec;
                        Ok(())
                    }))
                    .await?;
                    show(&Target::Program(program))?;
                }
                ProgramManage::Environment(command) => {
                    manage_environment(bottles, Target::Program(program), command).await?
                }
            }
        }
    }
    Ok(())
}

fn runtime(bottles: &Bottles, args: RuntimeArgs) -> Result<RuntimeSelection> {
    let state = bottles.addons().state();
    Ok((
        local_addon(args.runner, state.runners())?,
        local_addon(args.winebridge, state.winebridges())?,
        args.umu
            .map(|id| local_addon(id, state.umus()))
            .transpose()?,
    ))
}

fn find_bottle(manager: &Manager<Bottle>, selector: &str) -> Result<Bottle> {
    if let Ok(id) = Uuid::parse_str(selector) {
        return Ok(manager.open(id)?);
    }
    let mut found = None;
    for bottle in manager.list() {
        if bottle.state()?.name() == selector {
            if found.is_some() {
                return Err(format!("ambiguous bottle name: {selector}; use its UUID").into());
            }
            found = Some(bottle);
        }
    }
    found.ok_or_else(|| missing("bottle", selector).into())
}

#[cfg(feature = "fvs")]
fn find_program(manager: &Manager<Program>, selector: &str) -> Result<Program> {
    if let Ok(id) = Uuid::parse_str(selector) {
        return Ok(manager.open(id)?);
    }
    let mut found = None;
    for program in manager.list() {
        if program.state()?.name() == selector {
            if found.is_some() {
                return Err(format!("ambiguous program name: {selector}; use its UUID").into());
            }
            found = Some(program);
        }
    }
    found.ok_or_else(|| missing("program", selector).into())
}

async fn manage_bottle_program(bottle: &Bottle, command: BottleProgram) -> Result<()> {
    match command {
        BottleProgram::Add(args) => {
            let spec = args.into_spec();
            let id = run_operation(bottle.edit(move |edit| Ok(edit.add_program(spec)))).await?;
            println!("{id}");
        }
        BottleProgram::Edit { program, launch } => {
            let spec = launch.into_spec();
            run_operation(bottle.edit(move |edit| {
                *edit
                    .program(program)
                    .ok_or(bottles_core::BottleError::ProgramNotFound(program))? = spec;
                Ok(())
            }))
            .await?;
        }
        BottleProgram::Remove { program } => {
            run_operation(bottle.edit(move |edit| {
                edit.remove_program(program)
                    .ok_or(bottles_core::BottleError::ProgramNotFound(program))?;
                Ok(())
            }))
            .await?;
        }
        BottleProgram::Launch { program } => {
            println!("{}", run_operation(bottle.launch_program(program)).await?)
        }
        BottleProgram::Kill { program } => bottle.kill_program(program).await?,
    }
    Ok(())
}

fn config(target: &Target) -> Result<EnvironmentConfig> {
    Ok(owner!(target, |item| item.state()?.config().clone()))
}

fn show(target: &Target) -> Result<()> {
    match target {
        Target::Bottle(bottle) => {
            let state = bottle.state()?;
            println!(
                "id: {}\nname: {}\nstorage: {:?}",
                state.id(),
                state.name(),
                state.backend()
            );
            show_config(state.config());
            for (id, spec) in state.programs() {
                println!("program: {} {} {}", id, spec.name(), spec.executable());
            }
        }
        #[cfg(feature = "fvs")]
        Target::Program(program) => {
            let state = program.state()?;
            println!(
                "id: {}\nname: {}\nexecutable: {}\nstorage: Virgo",
                state.id(),
                state.name(),
                state.launch().executable()
            );
            show_config(state.config());
        }
    }
    Ok(())
}

fn show_config(config: &EnvironmentConfig) {
    println!(
        "runner: {}\nwinebridge: {}",
        config.runner.id(),
        config.winebridge.id()
    );
    if let Some(umu) = &config.umu {
        println!("umu: {}", umu.id());
    }
    let mut components: Vec<_> = config.components.values().collect();
    components.sort_unstable_by_key(|item| item.slot().as_str());
    for item in components {
        println!(
            "component: {} {} {} {}",
            item.id(),
            item.name(),
            item.version(),
            item.slot()
        );
    }
    for item in &config.dependencies {
        println!(
            "dependency: {} {} {}",
            item.id(),
            item.name(),
            item.version()
        );
    }
    let mut vars: Vec<_> = config.env_vars.iter().collect();
    vars.sort_unstable_by_key(|(key, _)| *key);
    for (key, value) in vars {
        println!("env: {key}={value}");
    }
}

async fn manage_environment(
    bottles: &Bottles,
    target: Target,
    command: EnvironmentCommand,
) -> Result<()> {
    match command {
        EnvironmentCommand::Show => show(&target)?,
        EnvironmentCommand::Rename { name } => {
            run_operation(owner!(&target, |item| item.edit(move |edit| {
                edit.rename(name);
                Ok(())
            })))
            .await?;
            show(&target)?;
        }
        EnvironmentCommand::Delete => match &target {
            Target::Bottle(item) => {
                run_operation(bottles.bottles().delete(item.id()?)).await?;
            }
            #[cfg(feature = "fvs")]
            Target::Program(item) => {
                run_operation(bottles.programs().delete(item.id()?)).await?;
            }
        },
        EnvironmentCommand::Stop => owner!(&target, |item| item.stop().await)?,
        EnvironmentCommand::Processes => {
            for process in owner!(&target, |item| item.processes().await)? {
                println!("{}\t{}\t{}", process.pid, process.name, process.threads);
            }
        }
        EnvironmentCommand::Install { addon } => {
            install(bottles, &target, addon).await?;
            show(&target)?;
        }
        EnvironmentCommand::Uninstall { component } => {
            uninstall(&target, component).await?;
            show(&target)?;
        }
        EnvironmentCommand::Env { command } => manage_env(&target, command).await?,
        EnvironmentCommand::DllOverrides { command } => manage_dll(&target, command).await?,
        #[cfg(feature = "fvs")]
        EnvironmentCommand::Snapshot { command } => manage_snapshot(&target, command).await?,
        EnvironmentCommand::Wrappers { command } => manage_wrappers(&target, command).await?,
    }
    Ok(())
}

async fn install(bottles: &Bottles, target: &Target, id: Uuid) -> Result<()> {
    let state = bottles.addons().state();
    if let Some(item) = state.runners().into_iter().find(|item| item.id() == id) {
        let runner = item.as_ref().clone();
        run_operation(owner!(target, |owner| owner.edit(move |edit| {
            edit.set_runner(runner);
            Ok(())
        })))
        .await?;
    } else if let Some(item) = state.winebridges().into_iter().find(|item| item.id() == id) {
        let winebridge = item.as_ref().clone();
        run_operation(owner!(target, |owner| owner.edit(move |edit| {
            edit.set_winebridge(winebridge);
            Ok(())
        })))
        .await?;
    } else if let Some(item) = state.umus().into_iter().find(|item| item.id() == id) {
        let umu = item.as_ref().clone();
        run_operation(owner!(target, |owner| owner.edit(move |edit| {
            edit.set_umu(Some(umu));
            Ok(())
        })))
        .await?;
    } else if let Some(item) = state.components().into_iter().find(|item| item.id() == id) {
        let component = item.as_ref().clone();
        run_operation(owner!(target, |owner| owner.edit(move |edit| {
            edit.set_component(component);
            Ok(())
        })))
        .await?;
    } else if let Some(item) = state
        .dependencies()
        .into_iter()
        .find(|item| item.id() == id)
    {
        let dependency = item.as_ref().clone();
        run_operation(owner!(target, |owner| owner.edit(move |edit| {
            edit.add_dependency(dependency);
            Ok(())
        })))
        .await?;
    } else {
        return Err(missing("addon", id).into());
    }
    Ok(())
}

async fn uninstall(target: &Target, id: Uuid) -> Result<()> {
    let settings = config(target)?;
    if settings.umu.as_ref().is_some_and(|item| item.id() == id) {
        run_operation(owner!(target, |owner| owner.edit(|edit| {
            edit.set_umu(None);
            Ok(())
        })))
        .await?;
    } else {
        let slot = settings
            .components
            .values()
            .find(|item| item.id() == id)
            .map(Addon::slot)
            .ok_or_else(|| missing("installed component", id))?;
        run_operation(owner!(target, |owner| owner
            .edit(move |edit| edit.remove_component(slot))))
        .await?;
    }
    Ok(())
}

async fn manage_env(target: &Target, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::List => {
            let settings = config(target)?;
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
    target: &Target,
    change: impl FnOnce(&mut EnvVars) + Send + 'static,
) -> Result<()> {
    run_operation(owner!(target, |owner| owner.edit(move |edit| {
        change(edit.env_vars());
        Ok(())
    })))
    .await?;
    Ok(())
}

async fn manage_dll(target: &Target, command: DllCommand) -> Result<()> {
    match command {
        DllCommand::List => {
            let mut overrides = run_operation(owner!(target, |item| item.dll_overrides())).await?;
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
            run_operation(owner!(target, |item| item.set_dll_override(dll, mode.into()))).await?;
        }
        DllCommand::Unset { dll } => {
            run_operation(owner!(target, |item| item.unset_dll_override(dll))).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "fvs")]
async fn manage_snapshot(target: &Target, command: SnapshotCommand) -> Result<()> {
    match command {
        SnapshotCommand::Create { message } => {
            let snapshot =
                run_operation(owner!(target, |item| item.create_snapshot(message))).await?;
            println!("{}", snapshot.state_id);
        }
        SnapshotCommand::List => {
            for snapshot in owner!(target, |item| item.snapshots().await)? {
                println!("{}\t{}", snapshot.state_id, snapshot.message);
            }
        }
        SnapshotCommand::Restore { state } => {
            println!(
                "{}",
                run_operation(owner!(target, |item| item.rollback(&state))).await?
            );
        }
    }
    Ok(())
}

async fn manage_wrappers(target: &Target, command: WrapperCommand) -> Result<()> {
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
            println!("{:#?}", config(target)?.wrappers.gamescope);
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
            println!("{:#?}", config(target)?.wrappers.mangohud);
        }
    }
    Ok(())
}

async fn edit_wrappers(
    target: &Target,
    change: impl FnOnce(&mut Wrappers) + Send + 'static,
) -> Result<()> {
    run_operation(owner!(target, |owner| owner.edit(move |edit| {
        change(edit.wrappers());
        Ok(())
    })))
    .await?;
    Ok(())
}
