use bottles_core::proto::NotifyRequest;
pub use bottles_core::proto::{HealthRequest, bottles_client::BottlesClient};
use bottles_core::{Layer, VirgoDaemon};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::io;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about = "CLI for managing Bottles wine environments", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Manage bottle life cycles")]
    Management(ManagementArgs),
    #[command(about = "Configure bottle settings and environment variables")]
    Configuration(ConfigurationArgs),
    #[command(about = "Install or remove components like DXVK, VKD3D, or dependencies")]
    Installer(InstallerArgs),
    #[command(about = "Run and monitor programs within a bottle")]
    Runtime(RuntimeArgs),
    #[command(about = "Check system health and send notifications")]
    System(SystemArgs),
    #[command(about = "Manage prefix snapshots and layers through fvs2d (Virgo)")]
    Virgo(VirgoArgs),
}

#[derive(Debug, Args)]
struct VirgoArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Path to the fvs2d binary; spawns a private daemon for this command"
    )]
    fvs2d: Option<PathBuf>,
    #[arg(
        long,
        value_name = "ADDR",
        help = "Connect to a running fvs2d manager instead (e.g. http://127.0.0.1:50151)"
    )]
    connect: Option<String>,
    #[command(subcommand)]
    command: VirgoSubcommands,
}

#[derive(Debug, Subcommand)]
enum VirgoSubcommands {
    #[command(about = "Show daemon version and host capabilities")]
    Probe,
    #[command(about = "Initialize snapshot tracking for a prefix")]
    Init {
        #[arg(help = "Prefix directory")]
        prefix: PathBuf,
    },
    #[command(about = "Create a restore point of a prefix")]
    Snapshot {
        #[arg(help = "Prefix directory")]
        prefix: PathBuf,
        #[arg(short, long, default_value = "", help = "Snapshot label")]
        message: String,
    },
    #[command(about = "List the restore points of a prefix")]
    States {
        #[arg(help = "Prefix directory")]
        prefix: PathBuf,
    },
    #[command(about = "Roll a prefix back to a restore point (exact checkout)")]
    Rollback {
        #[arg(help = "Prefix directory")]
        prefix: PathBuf,
        #[arg(help = "State id (full or prefix)")]
        state: String,
    },
    #[command(about = "Mount a stack of layers (requires --connect)")]
    Mount {
        #[arg(help = "Mountpoint directory")]
        mount_point: PathBuf,
        #[arg(
            long = "layer",
            required = true,
            help = "Layer, low to high: repo | repo@state | repo#branch"
        )]
        layers: Vec<String>,
        #[arg(long, help = "Writable upper layer directory")]
        upper: Option<PathBuf>,
    },
    #[command(about = "List the mounts owned by the daemon (requires --connect)")]
    Mounts,
    #[command(about = "Unmount a mount by id (requires --connect)")]
    Unmount {
        #[arg(help = "Mount id")]
        mount_id: String,
        #[arg(long, help = "Detach even if the mountpoint is busy")]
        lazy: bool,
    },
}

#[derive(Debug, Args)]
struct ManagementArgs {
    #[command(subcommand)]
    command: ManagementSubcommands,
}

#[derive(Debug, Subcommand)]
enum ManagementSubcommands {
    #[command(about = "Create a new bottle environment")]
    Create {
        #[arg(help = "The unique name for the new bottle")]
        name: String,
        #[arg(value_enum, help = "The environment preset to use")]
        bottle_type: BottleTypeCli,
        #[arg(help = "Optional runner override (e.g. soda-7.0-9)")]
        runner: Option<String>,
    },
    #[command(about = "Permanently delete a bottle")]
    Delete {
        #[arg(help = "Name of the bottle to delete")]
        name: String,
    },
    #[command(about = "List all existing bottles")]
    List,
    #[command(about = "Get detailed information about a specific bottle")]
    Get {
        #[arg(help = "Name of the bottle to inspect")]
        name: String,
    },
    #[command(about = "Start the bottle agent")]
    Start {
        #[arg(help = "Name of the bottle to start")]
        name: String,
    },
    #[command(about = "Stop the bottle agent and running programs")]
    Stop {
        #[arg(help = "Name of the bottle to stop")]
        name: String,
    },
    #[command(about = "Restart the bottle agent")]
    Restart {
        #[arg(help = "Name of the bottle to restart")]
        name: String,
    },
}

#[derive(Debug, Args)]
struct ConfigurationArgs {
    #[command(subcommand)]
    command: ConfigurationSubcommands,
}

#[derive(Debug, Subcommand)]
enum ConfigurationSubcommands {
    #[command(about = "Retrieve the current configuration for a bottle")]
    GetConfig {
        #[arg(help = "Name of the bottle")]
        name: String,
    },
    #[command(about = "Update bottle settings")]
    UpdateConfig {
        #[arg(help = "Name of the bottle to update")]
        name: String,
        #[arg(help = "The new runner version to assign")]
        runner: String,
    },
    #[command(about = "List environment variables for a bottle")]
    GetEnv {
        #[arg(help = "Name of the bottle")]
        name: String,
    },
    #[command(about = "Set environment variables using KEY=VALUE format")]
    SetEnv {
        #[arg(help = "Name of the bottle")]
        name: String,
        #[arg(help = "Variables to set (e.g. PROTON_USE_WINE_DXGI=1)")]
        vars: Vec<String>,
    },
}

#[derive(Debug, Args)]
struct InstallerArgs {
    #[command(subcommand)]
    command: InstallerSubcommands,
}

#[derive(Debug, Subcommand)]
enum InstallerSubcommands {
    #[command(about = "Install a component into a bottle")]
    Install {
        #[arg(help = "Target bottle name")]
        bottle_name: String,
        #[arg(help = "ID of the component (e.g. dxvk, vkd3d)")]
        component_id: String,
        #[arg(short, long, help = "Specific version to install (defaults to latest)")]
        version: Option<String>,
    },
    #[command(about = "List available components")]
    List {
        #[arg(short, long, help = "Filter components by type (e.g. runner, layer)")]
        filter: Option<String>,
    },
    #[command(about = "Uninstall a component from a bottle")]
    Uninstall {
        #[arg(help = "Target bottle name")]
        bottle_name: String,
        #[arg(help = "ID of the component to remove")]
        component_id: String,
    },
}

#[derive(Debug, Args)]
struct RuntimeArgs {
    #[command(subcommand)]
    command: RuntimeSubcommands,
}

#[derive(Debug, Subcommand)]
enum RuntimeSubcommands {
    #[command(about = "Launch a Windows program inside the bottle")]
    Launch {
        #[arg(help = "Name of the bottle")]
        bottle_name: String,
        #[arg(help = "Path to the executable file")]
        program_path: String,
        #[arg(last = true, help = "Arguments passed directly to the program")]
        args: Vec<String>,
        #[arg(long, help = "The working directory for the process")]
        work_dir: Option<String>,
        #[arg(long, help = "Run the program within a visible terminal")]
        terminal: bool,
    },
    #[command(about = "Forcefully terminate a running program")]
    Terminate {
        #[arg(help = "Name of the bottle")]
        bottle_name: String,
        #[arg(help = "Process ID (PID) to kill")]
        pid: u32,
    },
    #[command(about = "List all active processes in a bottle")]
    ListProcesses {
        #[arg(help = "Name of the bottle")]
        name: String,
    },
}

#[derive(Debug, Args)]
struct SystemArgs {
    #[command(subcommand)]
    command: SystemSubcommands,
}

#[derive(Debug, Subcommand)]
enum SystemSubcommands {
    #[command(about = "Check if the gRPC server is responding")]
    Health,
    #[command(about = "Send a notification to the system")]
    Notify {
        #[arg(help = "The message text to display")]
        message: String,
    },
}

#[derive(Debug, ValueEnum, Clone)]
enum BottleTypeCli {
    #[value(help = "A bottle with no predefined settings")]
    Custom,
    #[value(help = "Optimized for gaming with DXVK and high-performance settings")]
    Gaming,
    #[value(help = "Optimized for general purpose software")]
    Software,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("bottles_cli=trace")),
        )
        .init();

    let args = Cli::parse();
    match args.command {
        Command::Virgo(virgo) => return run_virgo(virgo).await,
        command => run_bottles(command).await,
    }
}

async fn run_bottles(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    let url = "http://[::1]:50052";
    let mut client = BottlesClient::connect(url).await?;

    match command {
        Command::Virgo(_) => unreachable!("handled before connecting"),
        Command::Management(m) => match m.command {
            ManagementSubcommands::Create {
                name,
                bottle_type,
                runner,
            } => {
                println!(
                    "Action: CreateBottle | Name: {} | Type: {:?} | Runner: {:?}",
                    name, bottle_type, runner
                );
            }
            ManagementSubcommands::Delete { name } => {
                println!("Action: DeleteBottle | Name: {}", name);
            }
            ManagementSubcommands::List => {
                println!("Action: ListBottles");
            }
            ManagementSubcommands::Get { name } => {
                println!("Action: GetBottle | Name: {}", name);
            }
            ManagementSubcommands::Start { name } => {
                println!("Action: StartBottle | Name: {}", name);
            }
            ManagementSubcommands::Stop { name } => {
                println!("Action: StopBottle | Name: {}", name);
            }
            ManagementSubcommands::Restart { name } => {
                println!("Action: RestartBottle | Name: {}", name);
            }
        },

        Command::Configuration(c) => match c.command {
            ConfigurationSubcommands::GetConfig { name } => {
                println!("Action: GetConfig | Bottle: {}", name);
            }
            ConfigurationSubcommands::UpdateConfig { name, runner } => {
                println!(
                    "Action: UpdateConfig | Bottle: {} | New Runner: {}",
                    name, runner
                );
            }
            ConfigurationSubcommands::GetEnv { name } => {
                println!("Action: GetEnvironmentVariables | Bottle: {}", name);
            }
            ConfigurationSubcommands::SetEnv { name, vars } => {
                println!(
                    "Action: SetEnvironmentVariables | Bottle: {} | Variables: {:?}",
                    name, vars
                );
            }
        },

        Command::Installer(i) => match i.command {
            InstallerSubcommands::Install {
                bottle_name,
                component_id,
                version,
            } => {
                println!(
                    "Action: InstallComponent | Bottle: {} | ID: {} | Version: {:?}",
                    bottle_name, component_id, version
                );
            }
            InstallerSubcommands::List { filter } => {
                println!("Action: ListComponents | Filter: {:?}", filter);
            }
            InstallerSubcommands::Uninstall {
                bottle_name,
                component_id,
            } => {
                println!(
                    "Action: UninstallComponent | Bottle: {} | ID: {}",
                    bottle_name, component_id
                );
            }
        },

        Command::Runtime(r) => match r.command {
            RuntimeSubcommands::Launch {
                bottle_name,
                program_path,
                args,
                work_dir,
                terminal,
            } => {
                println!("Action: LaunchProgram");
                println!(" -> Bottle: {}", bottle_name);
                println!(" -> Path: {}", program_path);
                println!(" -> Args: {:?}", args);
                println!(" -> WorkDir: {:?}", work_dir);
                println!(" -> Terminal: {}", terminal);
            }
            RuntimeSubcommands::Terminate { bottle_name, pid } => {
                println!(
                    "Action: TerminateProgram | Bottle: {} | PID: {}",
                    bottle_name, pid
                );
            }
            RuntimeSubcommands::ListProcesses { name } => {
                println!("Action: ListRunningProcesses | Bottle: {}", name);
            }
        },

        Command::System(s) => match s.command {
            SystemSubcommands::Health => {
                println!("Action: SystemHealth");
                let request = HealthRequest {};
                let response = client.health(request).await?;
                let response = response.get_ref();
                if response.ok {
                    tracing::info!("Server is healthy");
                } else {
                    tracing::info!("Server is unhealthy");
                }
            }
            SystemSubcommands::Notify { message } => {
                println!("Action: SystemNotify | Message: {}", message);
                let request = NotifyRequest { message };
                let response = client.notify(request).await?;
                let response = response.get_ref();
                if response.success {
                    tracing::info!("Message sent successfully");
                } else {
                    tracing::info!("Failed to send message");
                }
            }
        },
    }
    Ok(())
}

fn parse_layer(spec: &str) -> Layer {
    if let Some((repo, state)) = spec.rsplit_once('@') {
        return Layer::new(repo).state(state);
    }
    if let Some((repo, branch)) = spec.rsplit_once('#') {
        return Layer::new(repo).branch(branch);
    }
    Layer::new(spec)
}

async fn run_virgo(args: VirgoArgs) -> Result<(), Box<dyn std::error::Error>> {
    let owned = args.connect.is_none();
    let daemon = match (args.connect, args.fvs2d) {
        (Some(addr), _) => VirgoDaemon::connect(addr).await?,
        (None, Some(bin)) => VirgoDaemon::spawn(bin).await?,
        (None, None) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "pass --fvs2d to spawn a daemon or --connect to reach a running one",
            )
            .into());
        }
    };

    let result = match args.command {
        VirgoSubcommands::Probe => {
            let info = daemon.probe().await?;
            println!(
                "version={} pid={} dev_fuse={} fusermount={} flatpak={}",
                info.daemon_version,
                info.pid,
                info.dev_fuse_accessible,
                info.fusermount_available,
                info.running_in_flatpak
            );
            Ok(())
        }
        VirgoSubcommands::Init { prefix } => {
            let repo = daemon.init_repository(prefix).await?;
            println!("initialized {}", repo.repository_path);
            Ok(())
        }
        VirgoSubcommands::Snapshot { prefix, message } => {
            let revision = daemon.create_restore_point(prefix, message).await?;
            println!(
                "restore point {}",
                &revision.state_id[..12.min(revision.state_id.len())]
            );
            Ok(())
        }
        VirgoSubcommands::States { prefix } => {
            let states = daemon.list_commits(prefix).await?;
            if states.is_empty() {
                println!("(no restore points)");
            }
            for state in states {
                let label = if state.message.is_empty() {
                    "(no label)"
                } else {
                    &state.message
                };
                println!(
                    "{}  {}",
                    &state.state_id[..12.min(state.state_id.len())],
                    label
                );
            }
            Ok(())
        }
        VirgoSubcommands::Rollback { prefix, state } => {
            let restored = daemon.rollback(prefix, state).await?;
            println!(
                "rolled back to {} in {}",
                &restored.state_id[..12.min(restored.state_id.len())],
                restored.destination_path
            );
            Ok(())
        }
        VirgoSubcommands::Mount {
            mount_point,
            layers,
            upper,
        } => {
            if owned {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "mount requires --connect: the mount lives as long as the daemon",
                )
                .into());
            }
            let layers = layers.iter().map(|s| parse_layer(s)).collect();
            let mount = daemon.create_mount(mount_point, layers, upper).await?;
            println!(
                "mounted {} at {} (nodes {})",
                mount.id,
                mount.spec.map(|s| s.mount_point).unwrap_or_default(),
                mount.node_count
            );
            Ok(())
        }
        VirgoSubcommands::Mounts => {
            let mounts = daemon.list_mounts().await?;
            if mounts.is_empty() {
                println!("(no mounts)");
            }
            for mount in mounts {
                println!(
                    "{}  {}",
                    mount.id,
                    mount.spec.map(|s| s.mount_point).unwrap_or_default()
                );
            }
            Ok(())
        }
        VirgoSubcommands::Unmount { mount_id, lazy } => {
            daemon.unmount(mount_id, lazy).await?;
            println!("unmounted");
            Ok(())
        }
    };

    if owned {
        daemon.shutdown().await?;
    }
    result
}
