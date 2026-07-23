use std::{error::Error, io, path::PathBuf};

use bottles_core::{
    Context, Directories,
    bottle::{
        Bottle, BottleManager, BottleType, GamescopeConfig, GamescopeFilter, GamescopeScaler,
        Program,
    },
    compatibility::{
        components::{Component, ComponentManager},
        dependencies::{Dependency, DependencyManager},
    },
};
use clap::{Args, Parser, Subcommand, ValueEnum};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(version, about = "Bottles Next test CLI")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    fvs2d: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List locally available components.
    Components,
    /// List locally available dependencies.
    Dependencies,
    /// Create, list, or manage bottles.
    Bottle {
        #[command(subcommand)]
        command: BottleCommand,
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
    let directories = Directories::for_project("bottles-next")?;

    match cli.command {
        Command::Components => {
            for component in ComponentManager::load(&directories)?.components() {
                print_component(component);
            }
        }
        Command::Dependencies => {
            for dependency in DependencyManager::load(&directories)?.dependencies() {
                println!(
                    "{}\t{}\t{}",
                    dependency.id(),
                    dependency.name(),
                    dependency.version()
                );
            }
        }
        Command::Bottle { command } => {
            let manager = bottle_manager(directories.clone(), cli.fvs2d)?;
            match command {
                BottleCommand::Create(args) => create_bottle(&manager, &directories, args).await?,
                BottleCommand::List => {
                    for bottle in manager.list()? {
                        println!(
                            "{}\t{}\t{:?}\t{}",
                            bottle.id(),
                            bottle.name(),
                            bottle.r#type(),
                            bottle.runner().version()
                        );
                    }
                }
                BottleCommand::Manage(args) => manage_bottle(&manager, &directories, args).await?,
            }
        }
    }

    Ok(())
}

async fn create_bottle(
    manager: &BottleManager,
    directories: &Directories,
    args: CreateArgs,
) -> Result<()> {
    let components = ComponentManager::load(directories)?;
    let runner = find_component(&components, &args.runner)?;
    let winebridge = find_component(&components, &args.winebridge)?;
    let umu = args
        .umu
        .as_deref()
        .map(|id| find_component(&components, id))
        .transpose()?;
    let bottle = manager
        .create(
            args.name,
            args.storage.bottle_type(),
            runner,
            winebridge,
            umu,
        )
        .await?;
    print_bottle(&bottle);
    Ok(())
}

async fn manage_bottle(
    manager: &BottleManager,
    directories: &Directories,
    args: ManageArgs,
) -> Result<()> {
    let mut bottle = find_bottle(manager, &args.bottle)?;
    match args.command {
        ManageCommand::Show => print_bottle(&bottle),
        ManageCommand::Delete => manager.delete(bottle.id()).await?,
        ManageCommand::Stop => bottle.stop().await?,
        ManageCommand::Processes => {
            for process in bottle.processes().await? {
                println!("{}\t{}\t{}", process.pid, process.name, process.threads);
            }
        }
        ManageCommand::Install { command } => {
            match command {
                InstallCommand::Component { component, umu } => {
                    let components = ComponentManager::load(directories)?;
                    let component = find_component(&components, &component)?;
                    let umu = umu
                        .as_deref()
                        .map(|id| find_component(&components, id))
                        .transpose()?;
                    if component.kind().is_runner() {
                        bottle.install_runner(component, umu).await?;
                    } else if umu.is_some() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--umu is only valid when installing a runner",
                        )
                        .into());
                    } else {
                        bottle.install_component(component).await?;
                    }
                }
                InstallCommand::Dependency { dependency } => {
                    let dependencies = DependencyManager::load(directories)?;
                    bottle
                        .install_dependency(find_dependency(&dependencies, &dependency)?)
                        .await?;
                }
            }
            print_bottle(&bottle);
        }
        ManageCommand::Uninstall { command } => {
            match command {
                UninstallCommand::Component { component } => {
                    bottle.uninstall_component(component.parse()?).await?;
                }
            }
            print_bottle(&bottle);
        }
        ManageCommand::Program { command } => match command {
            ProgramCommand::Add(args) => {
                let mut program = Program::new(args.name, args.executable);
                program.args = args.arguments;
                program.working_directory = args.working_directory;
                program.new_console = args.new_console;
                let id = program.id;
                bottle.add_program(program)?;
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
        ManageCommand::Snapshot { command } => match command {
            SnapshotCommand::Create { message } => {
                println!("{}", bottle.create_snapshot(message).await?.state_id);
            }
            SnapshotCommand::List => {
                for snapshot in bottle.snapshots().await? {
                    println!("{}\t{}", snapshot.state_id, snapshot.message);
                }
            }
            SnapshotCommand::Restore { state } => {
                println!("{}", bottle.rollback(&state).await?);
            }
        },
        ManageCommand::Wrappers { command } => manage_wrappers(&mut bottle, command).await?,
    }
    Ok(())
}

async fn manage_wrappers(bottle: &mut Bottle, command: WrappersCommand) -> Result<()> {
    match command {
        WrappersCommand::Gamescope { command } => {
            match command {
                GamescopeCommand::Show => {}
                GamescopeCommand::Enable => {
                    let mut wrappers = bottle.wrappers().clone();
                    wrappers.gamescope.enabled = true;
                    bottle.set_wrappers(wrappers).await?;
                }
                GamescopeCommand::Disable => {
                    let mut wrappers = bottle.wrappers().clone();
                    wrappers.gamescope.enabled = false;
                    bottle.set_wrappers(wrappers).await?;
                }
                GamescopeCommand::Configure(args) => {
                    let mut wrappers = bottle.wrappers().clone();
                    wrappers.gamescope = args.config(wrappers.gamescope.enabled);
                    bottle.set_wrappers(wrappers).await?;
                }
            }
            println!("{:#?}", bottle.wrappers().gamescope);
        }
        WrappersCommand::Mangohud { command } => {
            match command {
                MangohudCommand::Show => {}
                MangohudCommand::Enable => {
                    let mut wrappers = bottle.wrappers().clone();
                    wrappers.mangohud.enabled = true;
                    bottle.set_wrappers(wrappers).await?;
                }
                MangohudCommand::Disable => {
                    let mut wrappers = bottle.wrappers().clone();
                    wrappers.mangohud.enabled = false;
                    bottle.set_wrappers(wrappers).await?;
                }
            }
            println!("{:#?}", bottle.wrappers().mangohud);
        }
    }
    Ok(())
}

fn bottle_manager(directories: Directories, fvs2d: Option<PathBuf>) -> Result<BottleManager> {
    let fvs2d = fvs2d.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--fvs2d is required for bottle commands",
        )
    })?;
    Ok(BottleManager::new(Context::new(directories, fvs2d)?))
}

fn find_component<'a>(manager: &'a ComponentManager, id: &str) -> io::Result<&'a Component> {
    manager
        .components()
        .iter()
        .find(|component| component.id().to_string() == id)
        .ok_or_else(|| missing("component", id))
}

fn find_dependency<'a>(manager: &'a DependencyManager, id: &str) -> io::Result<&'a Dependency> {
    manager
        .dependencies()
        .iter()
        .find(|dependency| dependency.id().to_string() == id)
        .ok_or_else(|| missing("dependency", id))
}

fn find_program<'a>(bottle: &'a Bottle, id: &str) -> io::Result<&'a Program> {
    bottle
        .programs()
        .iter()
        .find(|program| program.id.to_string() == id)
        .ok_or_else(|| missing("program", id))
}

fn find_bottle(manager: &BottleManager, selector: &str) -> Result<Bottle> {
    Ok(manager
        .list()?
        .into_iter()
        .find(|bottle| bottle.id().to_string() == selector || bottle.name() == selector)
        .ok_or_else(|| missing("bottle", selector))?)
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

fn print_bottle(bottle: &Bottle) {
    println!("id: {}", bottle.id());
    println!("name: {}", bottle.name());
    println!("storage: {:?}", bottle.r#type());
    print_bottle_component("runner", bottle.runner());
    print_bottle_component("winebridge", bottle.components().winebridge());
    if let Some(component) = bottle.components().umu() {
        print_bottle_component("umu", component);
    }
    for (name, component) in [
        ("dxvk", bottle.components().dxvk()),
        ("vkd3d", bottle.components().vkd3d()),
        ("nvapi", bottle.components().nvapi()),
        ("latency-flex", bottle.components().latency_flex()),
    ] {
        if let Some(component) = component {
            print_bottle_component(name, component);
        }
    }
    for dependency in bottle.dependencies() {
        println!(
            "dependency: {} {} {}",
            dependency.id(),
            dependency.name(),
            dependency.version()
        );
    }
    for program in bottle.programs() {
        println!(
            "program: {} {} {}",
            program.id, program.name, program.executable
        );
    }
}

fn print_bottle_component(name: &str, component: &Component) {
    println!(
        "{name}: {} {} {}",
        component.id(),
        component.version(),
        component.path().display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_bottle_commands() {
        let cli = Cli::try_parse_from([
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
    fn parses_program_kill_command() {
        let cli = Cli::try_parse_from([
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
            let cli =
                Cli::try_parse_from(["bottles", "bottle", "manage", "test", "install", kind, id])
                    .unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Install { .. }));
        }

        let cli = Cli::try_parse_from([
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
        let cli = Cli::try_parse_from([
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
        let cli =
            Cli::try_parse_from(["bottles", "bottle", "manage", "test", "processes"]).unwrap();
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
        let cli = Cli::try_parse_from(["bottles", "bottle", "manage", "test", "stop"]).unwrap();
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
            let cli = Cli::try_parse_from(args).unwrap();
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
            let cli = Cli::try_parse_from(args).unwrap();
            let Command::Bottle {
                command: BottleCommand::Manage(args),
            } = cli.command
            else {
                panic!("expected bottle manage command");
            };
            assert!(matches!(args.command, ManageCommand::Wrappers { .. }));
        }

        let cli = Cli::try_parse_from([
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
            Cli::try_parse_from([
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
