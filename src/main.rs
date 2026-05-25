use bottles_core::WineBridgeClient;
use bottles_core::runner::{PrefixArch, PrefixConfig, Proton, Runner, Wine};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{io, path::PathBuf, process::ExitCode};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about = "CLI for managing Bottles wine environments", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Initialize a Wine prefix")]
    Init(InitArgs),
    #[command(about = "Launch a Windows program through WineBridge")]
    Launch(LaunchArgs),
    #[command(about = "Inspect and control WineBridge processes")]
    Process(ProcessArgs),
    #[command(about = "Manage named bottles")]
    Bottle(BottleArgs),
    #[command(about = "Inspect and change bottle configuration")]
    Config(ConfigArgs),
    #[command(about = "Install, list, and remove bottle components")]
    Component(ComponentArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    runner: RunnerArgs,
}

#[derive(Debug, Args)]
struct LaunchArgs {
    #[command(flatten)]
    bridge: BridgeArgs,
    #[arg(help = "Windows executable path to launch through WineBridge")]
    executable: PathBuf,
    #[arg(last = true, help = "Arguments passed to the executable")]
    args: Vec<String>,
    #[arg(long, help = "Working directory for the process")]
    work_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ProcessArgs {
    #[command(flatten)]
    bridge: BridgeArgs,
    #[command(subcommand)]
    command: ProcessCommand,
}

#[derive(Debug, Subcommand)]
enum ProcessCommand {
    #[command(about = "Kill a process through WineBridge")]
    Kill {
        #[arg(help = "Process ID to terminate")]
        pid: u32,
    },
    #[command(about = "List active processes reported by WineBridge")]
    List,
}

#[derive(Debug, Args)]
struct BridgeArgs {
    #[command(flatten)]
    runner: RunnerArgs,
    #[arg(
        long,
        value_name = "PATH",
        help = "Path to the WineBridge Windows executable"
    )]
    winebridge: PathBuf,
}

#[derive(Debug, Args)]
struct BottleArgs {
    #[command(subcommand)]
    command: BottleCommand,
}

#[derive(Debug, Subcommand)]
enum BottleCommand {
    #[command(about = "Create a named bottle")]
    Create {
        #[arg(help = "Unique bottle name")]
        name: String,
        #[arg(value_enum, help = "Bottle preset to use")]
        preset: BottlePreset,
        #[arg(long, help = "Optional runner override")]
        runner: Option<String>,
    },
    #[command(about = "Delete a named bottle")]
    Delete {
        #[arg(help = "Bottle name")]
        name: String,
    },
    #[command(about = "List bottles")]
    List,
    #[command(about = "Show bottle details")]
    Info {
        #[arg(help = "Bottle name")]
        name: String,
    },
    #[command(about = "Start a bottle agent")]
    Start {
        #[arg(help = "Bottle name")]
        name: String,
    },
    #[command(about = "Stop a bottle agent and running programs")]
    Stop {
        #[arg(help = "Bottle name")]
        name: String,
    },
    #[command(about = "Restart a bottle agent")]
    Restart {
        #[arg(help = "Bottle name")]
        name: String,
    },
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(about = "Show bottle configuration")]
    Show {
        #[arg(help = "Bottle name")]
        bottle: String,
    },
    #[command(about = "Set the runner configured for a bottle")]
    SetRunner {
        #[arg(help = "Bottle name")]
        bottle: String,
        #[arg(help = "Runner identifier")]
        runner: String,
    },
    #[command(about = "List environment variables configured for a bottle")]
    Env {
        #[arg(help = "Bottle name")]
        bottle: String,
    },
    #[command(about = "Set bottle environment variables using KEY=VALUE pairs")]
    SetEnv {
        #[arg(help = "Bottle name")]
        bottle: String,
        #[arg(help = "Environment variables to set")]
        vars: Vec<String>,
    },
}

#[derive(Debug, Args)]
struct ComponentArgs {
    #[command(subcommand)]
    command: ComponentCommand,
}

#[derive(Debug, Subcommand)]
enum ComponentCommand {
    #[command(about = "Install a component into a bottle")]
    Install {
        #[arg(help = "Target bottle name")]
        bottle: String,
        #[arg(help = "Component identifier")]
        component: String,
        #[arg(short, long, help = "Specific version to install")]
        version: Option<String>,
    },
    #[command(about = "List available components")]
    List {
        #[arg(short, long, help = "Filter components by kind")]
        filter: Option<String>,
    },
    #[command(about = "Remove a component from a bottle")]
    Remove {
        #[arg(help = "Target bottle name")]
        bottle: String,
        #[arg(help = "Component identifier")]
        component: String,
    },
}

#[derive(Debug, Args)]
struct RunnerArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Path to the Wine prefix. Wine only; Proton uses --steam-compat-data-path/pfx"
    )]
    prefix: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = PrefixArchCli::Win64, help = "Wine prefix architecture")]
    arch: PrefixArchCli,
    #[arg(long, value_enum, default_value_t = RunnerKind::Wine, help = "Compatibility runner to use")]
    runner: RunnerKind,
    #[arg(long, value_name = "PATH", help = "Path to the runner executable")]
    runner_path: PathBuf,
    #[arg(
        long,
        value_name = "PATH",
        help = "Steam compatibility data path for Proton runners"
    )]
    steam_compat_data_path: Option<PathBuf>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Steam client install path for Proton runners"
    )]
    steam_compat_client_install_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PrefixArchCli {
    Win32,
    Win64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RunnerKind {
    Wine,
    Proton,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BottlePreset {
    #[value(help = "A bottle with no predefined settings")]
    Custom,
    #[value(help = "Optimized for gaming")]
    Gaming,
    #[value(help = "Optimized for general purpose software")]
    Software,
}

impl From<PrefixArchCli> for PrefixArch {
    fn from(value: PrefixArchCli) -> Self {
        match value {
            PrefixArchCli::Win32 => PrefixArch::Win32,
            PrefixArchCli::Win64 => PrefixArch::Win64,
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("bottles_cli=info")),
        )
        .init();

    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Init(args) => {
            let (runner, prefix) = create_runner(args.runner)?;
            runner.initialize_prefix(&prefix)?;
            println!("Initialized prefix at {}", prefix_path_from_env(&prefix));
            Ok(ExitCode::SUCCESS)
        }
        Command::Launch(args) => {
            if !args.args.is_empty() {
                unimplemented!("WineBridge launch arguments are not implemented in next-core yet");
            }

            if args.work_dir.is_some() {
                unimplemented!(
                    "WineBridge launch working directories are not implemented in next-core yet"
                );
            }

            let (runner, prefix) = create_runner(args.bridge.runner)?;
            let client =
                WineBridgeClient::new(runner.as_ref(), &prefix, args.bridge.winebridge).await?;
            let pid = client.launch_process(args.executable).await?;
            println!("Launched process with pid {pid}");

            client.shutdown().await?;

            println!("Process {pid} exited");
            Ok(ExitCode::SUCCESS)
        }
        Command::Process(args) => match args.command {
            ProcessCommand::Kill { pid } => {
                let (runner, prefix) = create_runner(args.bridge.runner)?;
                let client =
                    WineBridgeClient::new(runner.as_ref(), &prefix, args.bridge.winebridge).await?;
                client.kill_process(pid).await?;
                client.shutdown().await?;
                Ok(ExitCode::SUCCESS)
            }
            ProcessCommand::List => {
                unimplemented!("WineBridge process listing is not implemented in next-core yet")
            }
        },
        Command::Bottle(_) => {
            unimplemented!("named bottle management is not implemented in next-core yet")
        }
        Command::Config(_) => {
            unimplemented!("bottle configuration is not implemented in next-core yet")
        }
        Command::Component(_) => {
            unimplemented!("component management is not implemented in next-core yet")
        }
    }
}

fn create_runner(
    args: RunnerArgs,
) -> Result<(Box<dyn Runner>, PrefixConfig), Box<dyn std::error::Error>> {
    match args.runner {
        RunnerKind::Wine => {
            let prefix_path = args.prefix.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--prefix is required when using --runner wine",
                )
            })?;

            if args.steam_compat_data_path.is_some()
                || args.steam_compat_client_install_path.is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Steam compatibility paths are only valid when using --runner proton",
                )
                .into());
            }

            let runner: Box<dyn Runner> = Box::new(Wine::new(args.runner_path)?);
            let prefix = PrefixConfig::builder()
                .path(prefix_path)?
                .arch(args.arch.into())
                .build()?;

            Ok((runner, prefix))
        }
        RunnerKind::Proton => {
            if args.prefix.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--prefix must not be passed when using --runner proton; Proton uses --steam-compat-data-path/pfx",
                )
                .into());
            }

            let compat_data_path = args.steam_compat_data_path.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--steam-compat-data-path is required when using --runner proton",
                )
            })?;
            let compat_client_install_path =
                args.steam_compat_client_install_path.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--steam-compat-client-install-path is required when using --runner proton",
                    )
                })?;

            let runner: Box<dyn Runner> = Box::new(Proton::new(args.runner_path)?);
            let prefix = PrefixConfig::builder()
                .compat_data_path(compat_data_path)?
                .compat_client_install_path(compat_client_install_path)
                .arch(args.arch.into())
                .build()?;

            Ok((runner, prefix))
        }
    }
}

fn prefix_path_from_env(prefix: &PrefixConfig) -> String {
    prefix
        .to_env()
        .remove("WINEPREFIX")
        .unwrap_or_else(|| String::from("<unknown>"))
}
