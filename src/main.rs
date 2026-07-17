use std::{error::Error, io, path::PathBuf};

use bottles_core::{
    bottle::{Bottle, BottleManager, BottleType, Program},
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
}

#[derive(Subcommand)]
enum InstallCommand {
    Component {
        #[arg(value_name = "UUID")]
        component: String,
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

    match cli.command {
        Command::Components => {
            for component in ComponentManager::new()?.components() {
                print_component(component);
            }
        }
        Command::Dependencies => {
            for dependency in DependencyManager::new()?.dependencies() {
                println!(
                    "{}\t{}\t{}",
                    dependency.id(),
                    dependency.name(),
                    dependency.version()
                );
            }
        }
        Command::Bottle { command } => match command {
            BottleCommand::Create(args) => create_bottle(cli.fvs2d, args).await?,
            BottleCommand::List => {
                for bottle in bottle_manager(cli.fvs2d)?.list()? {
                    println!(
                        "{}\t{}\t{:?}\t{}",
                        bottle.id(),
                        bottle.name(),
                        bottle.r#type(),
                        bottle.runner().version()
                    );
                }
            }
            BottleCommand::Manage(args) => manage_bottle(cli.fvs2d, args).await?,
        },
    }

    Ok(())
}

async fn create_bottle(fvs2d: Option<PathBuf>, args: CreateArgs) -> Result<()> {
    let manager = bottle_manager(fvs2d)?;
    let components = ComponentManager::new()?;
    let runner = find_component(&components, &args.runner)?;
    let winebridge = find_component(&components, &args.winebridge)?;
    let bottle = manager
        .create(args.name, args.storage.bottle_type(), runner, winebridge)
        .await?;
    print_bottle(&bottle);
    Ok(())
}

async fn manage_bottle(fvs2d: Option<PathBuf>, args: ManageArgs) -> Result<()> {
    let manager = bottle_manager(fvs2d)?;
    let mut bottle = find_bottle(&manager, &args.bottle)?;
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
                InstallCommand::Component { component } => {
                    let components = ComponentManager::new()?;
                    bottle
                        .install_component(find_component(&components, &component)?)
                        .await?;
                }
                InstallCommand::Dependency { dependency } => {
                    let dependencies = DependencyManager::new()?;
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
                let id = bottle
                    .programs()
                    .iter()
                    .find(|candidate| candidate.id.to_string() == program)
                    .map(|program| program.id)
                    .ok_or_else(|| missing("program", &program))?;
                println!("{}", bottle.run(id).await?);
            }
        },
    }
    Ok(())
}

fn bottle_manager(fvs2d: Option<PathBuf>) -> Result<BottleManager> {
    Ok(BottleManager::new(fvs2d)?)
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
}
