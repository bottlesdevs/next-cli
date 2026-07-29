use std::{error::Error, fmt::Debug, io, path::PathBuf, sync::Arc};

use bottles_core::{
    Bottle, BottleManager, BottleType, Component, ComponentKind, Core, Dependency, DllOverride,
    DllOverrideMode, GamescopeConfig, GamescopeFilter, GamescopeScaler, Library, Operation, Paths,
    Program, RunnerKind, RunnerSelection,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use download_manager::manager::{DownloadManager, DownloadManagerConfig};
use futures_util::StreamExt;
use http_client::ReqwestClient;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Parser)]
#[command(version, about = "Bottles Next CLI")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    fvs2d: Option<PathBuf>,

    #[arg(long)]
    component_catalog: Url,

    #[arg(long)]
    dependency_catalog: Url,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Refresh, inspect, download, or delete Library content.
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Create, list, or manage bottles.
    Bottle {
        #[command(subcommand)]
        command: BottleCommand,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// Refresh both remote catalogs.
    Refresh,
    /// Rescan locally installed content.
    Scan,
    Components {
        #[command(subcommand)]
        command: LibraryItemCommand,
    },
    Dependencies {
        #[command(subcommand)]
        command: LibraryItemCommand,
    },
}

#[derive(Subcommand)]
enum LibraryItemCommand {
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
    /// Create a bottle from locally available components.
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
        #[command(subcommand)]
        command: InstallCommand,
    },
    Uninstall {
        #[command(subcommand)]
        command: UninstallCommand,
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
enum InstallCommand {
    Component {
        #[arg(value_name = "UUID")]
        component: String,
        #[arg(long, value_name = "UUID")]
        umu: Option<String>,
    },
    Dependency {
        #[arg(value_name = "UUID")]
        dependency: String,
    },
}

#[derive(Subcommand)]
enum UninstallCommand {
    Component {
        #[arg(value_name = "UUID")]
        component: String,
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

    #[arg(long, value_enum, default_value_t = Storage::Standard)]
    storage: Storage,

    #[arg(long, value_name = "UUID")]
    runner: String,

    #[arg(long, value_name = "UUID")]
    winebridge: String,

    #[arg(long, value_name = "UUID")]
    umu: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Storage {
    Standard,
    Virgo,
}

impl Storage {
    fn bottle_type(self) -> BottleType {
        match self {
            Self::Standard => BottleType::Standard,
            Self::Virgo => BottleType::Virgo,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::for_project("bottles-next").await?;
    let fvs2d = cli
        .fvs2d
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "--fvs2d is required"))?;
    let (downloads, scheduler) = DownloadManager::new(
        Arc::new(ReqwestClient::new()?),
        DownloadManagerConfig::default(),
    );
    let downloads = Arc::new(downloads);
    let scheduler = tokio::spawn(scheduler);
    let core = Core::open(
        paths,
        fvs2d,
        cli.component_catalog,
        cli.dependency_catalog,
        downloads.clone(),
    )
    .await?;

    let result = match cli.command {
        Command::Library { command } => manage_library(core.library(), command).await,
        Command::Bottle { command } => match command {
            BottleCommand::Create(args) => create_bottle(&core, args).await,
            BottleCommand::List => {
                for bottle in core.bottles().list().await? {
                    let bottle = bottle?;
                    let state = bottle.state()?;
                    println!(
                        "{}\t{}\t{:?}\t{}",
                        state.id(),
                        state.name(),
                        state.kind(),
                        state.runner().runner().version()
                    );
                }
                Ok(())
            }
            BottleCommand::Manage(args) => manage_bottle(&core, args).await,
        },
    };

    drop(core);
    drop(downloads);
    scheduler.await?;
    result
}

async fn manage_library(library: &Library, command: LibraryCommand) -> Result<()> {
    match command {
        LibraryCommand::Refresh => run_operation(library.refresh_catalogs()).await?,
        LibraryCommand::Scan => library.refresh().await?,
        LibraryCommand::Components { command } => match command {
            LibraryItemCommand::List => {
                for status in library.state().components() {
                    if let Some(component) = status.downloaded() {
                        print_component(component);
                    } else {
                        println!(
                            "{}\t{:?}\t{}\tnot-downloaded",
                            status.id(),
                            status.kind(),
                            status.version()
                        );
                    }
                }
            }
            LibraryItemCommand::Download { id } => {
                print_component(&run_operation(library.download_component(id.parse()?)).await?);
            }
            LibraryItemCommand::Delete { id } => {
                library.delete_component(id.parse()?).await?;
            }
        },
        LibraryCommand::Dependencies { command } => match command {
            LibraryItemCommand::List => {
                for status in library.state().dependencies() {
                    println!(
                        "{}\t{}\t{}\t{}",
                        status.id(),
                        status.name(),
                        status.version(),
                        if status.downloaded().is_some() {
                            "downloaded"
                        } else {
                            "not-downloaded"
                        }
                    );
                }
            }
            LibraryItemCommand::Download { id } => {
                let dependency = run_operation(library.download_dependency(id.parse()?)).await?;
                println!(
                    "{}\t{}\t{}",
                    dependency.id(),
                    dependency.name(),
                    dependency.version()
                );
            }
            LibraryItemCommand::Delete { id } => {
                library.delete_dependency(id.parse()?).await?;
            }
        },
    }
    Ok(())
}

async fn create_bottle(core: &Core, args: CreateArgs) -> Result<()> {
    let runner = find_component(core.library(), &args.runner)?;
    let winebridge = find_component(core.library(), &args.winebridge)?;
    let umu = args
        .umu
        .as_deref()
        .map(|id| find_component(core.library(), id))
        .transpose()?;
    let selection = runner_selection(&runner, umu.as_ref())?;
    let bottle = run_operation(core.bottles().create(
        args.name,
        args.storage.bottle_type(),
        selection,
        &winebridge,
    ))
    .await?;
    print_bottle(&bottle)?;
    Ok(())
}

async fn manage_bottle(core: &Core, args: ManageArgs) -> Result<()> {
    let bottle = find_bottle(core.bottles(), &args.bottle).await?;
    match args.command {
        ManageCommand::Show => print_bottle(&bottle)?,
        ManageCommand::Delete => {
            run_operation(core.bottles().delete(bottle.state()?.id())).await?;
        }
        ManageCommand::Stop => bottle.stop().await?,
        ManageCommand::Processes => {
            for process in bottle.processes().await? {
                println!("{}\t{}\t{}", process.pid, process.name, process.threads);
            }
        }
        ManageCommand::Install { command } => {
            match command {
                InstallCommand::Component { component, umu } => {
                    let component = find_component(core.library(), &component)?;
                    let umu = umu
                        .as_deref()
                        .map(|id| find_component(core.library(), id))
                        .transpose()?;
                    if component.kind().is_runner() {
                        run_operation(
                            bottle.set_runner(runner_selection(&component, umu.as_ref())?),
                        )
                        .await?;
                    } else if umu.is_some() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--umu is only valid when installing a runner",
                        )
                        .into());
                    } else if component.kind() == ComponentKind::Winebridge {
                        run_operation(bottle.set_winebridge(&component)).await?;
                    } else if component.kind() == ComponentKind::Umu {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "UMU must be selected with a Proton runner",
                        )
                        .into());
                    } else {
                        run_operation(bottle.install_component(&component)).await?;
                    }
                }
                InstallCommand::Dependency { dependency } => {
                    let dependency = find_dependency(core.library(), &dependency)?;
                    run_operation(bottle.install_dependency(&dependency)).await?;
                }
            }
            print_bottle(&bottle)?;
        }
        ManageCommand::Uninstall { command } => {
            match command {
                UninstallCommand::Component { component } => {
                    run_operation(bottle.uninstall_component(component.parse()?)).await?;
                }
            }
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

fn find_component(library: &Library, id: &str) -> Result<Component> {
    library
        .state()
        .component(id.parse()?)
        .and_then(|status| status.downloaded())
        .cloned()
        .ok_or_else(|| missing("component", id))
        .map_err(Into::into)
}

fn find_dependency(library: &Library, id: &str) -> Result<Dependency> {
    library
        .state()
        .dependency(id.parse()?)
        .and_then(|status| status.downloaded())
        .cloned()
        .ok_or_else(|| missing("dependency", id))
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

async fn find_bottle(manager: &BottleManager, selector: &str) -> Result<Bottle> {
    for bottle in manager.list().await? {
        let bottle = bottle?;
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

fn print_component(component: &Component) {
    println!(
        "{}\t{:?}\t{}\t{}",
        component.id(),
        component.kind(),
        component.version(),
        component.path().display()
    );
}

fn print_bottle(bottle: &Bottle) -> Result<()> {
    let state = bottle.state()?;
    println!("id: {}", state.id());
    println!("name: {}", state.name());
    println!("storage: {:?}", state.kind());
    print_bottle_component("runner", state.runner().runner());
    print_bottle_component("winebridge", state.winebridge());
    if let Some(component) = state.runner().umu() {
        print_bottle_component("umu", component);
    }
    for (name, component) in [
        ("dxvk", state.dxvk()),
        ("vkd3d", state.vkd3d()),
        ("nvapi", state.nvapi()),
        ("latency-flex", state.latency_flex()),
    ] {
        if let Some(component) = component {
            print_bottle_component(name, component);
        }
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

fn print_bottle_component(name: &str, component: &Component) {
    println!(
        "{name}: {} {} {}",
        component.id(),
        component.version(),
        component.path().display()
    );
}

fn runner_selection(runner: &Component, umu: Option<&Component>) -> Result<RunnerSelection> {
    match runner.kind().runner_kind() {
        Some(RunnerKind::Wine) if umu.is_none() => Ok(RunnerSelection::wine(runner.clone())?),
        Some(RunnerKind::Wine) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--umu is only valid with a Proton runner",
        )
        .into()),
        Some(RunnerKind::Proton) => Ok(RunnerSelection::proton(
            runner.clone(),
            umu.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a Proton runner requires --umu",
                )
            })?
            .clone(),
        )?),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the selected component is not a runner",
        )
        .into()),
    }
}

async fn run_operation<T, P>(mut operation: Operation<T, P>) -> Result<T>
where
    P: Clone + Debug + Send + Sync + 'static,
{
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
        let mut args = args.into_iter().map(Into::into);
        let executable = args.next().expect("test command has an executable");
        Cli::try_parse_from(
            std::iter::once(executable)
                .chain([
                    OsString::from("--component-catalog"),
                    OsString::from("https://example.test/components.json"),
                    OsString::from("--dependency-catalog"),
                    OsString::from("https://example.test/dependencies.json"),
                ])
                .chain(args),
        )
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
    fn parses_library_download_command() {
        let cli = parse([
            "bottles",
            "library",
            "components",
            "download",
            "component-id",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Command::Library {
                command: LibraryCommand::Components {
                    command: LibraryItemCommand::Download { id }
                }
            } if id == "component-id"
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
    fn parses_component_and_dependency_install_commands() {
        for (kind, id) in [
            ("component", "component-id"),
            ("dependency", "dependency-id"),
        ] {
            let cli = parse(["bottles", "bottle", "manage", "test", "install", kind, id]).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Install { .. }));
        }

        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "install",
            "component",
            "runner-id",
            "--umu",
            "umu-id",
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
            ManageCommand::Install {
                command: InstallCommand::Component {
                    component,
                    umu: Some(umu),
                }
            } if component == "runner-id" && umu == "umu-id"
        ));
    }

    #[test]
    fn parses_component_uninstall_command() {
        let cli = parse([
            "bottles",
            "bottle",
            "manage",
            "test",
            "uninstall",
            "component",
            "component-id",
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
            ManageCommand::Uninstall {
                command: UninstallCommand::Component { .. }
            }
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
