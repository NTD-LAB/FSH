use clap::{Parser, Subcommand};
use fsh::{config::Config, server::FshServer};
use std::path::PathBuf;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "fsh-server")]
#[command(about = "FSH (Folder Shell Protocol) Server")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the FSH server
    Start {
        /// Override server host
        #[arg(long)]
        host: Option<String>,

        /// Override server port
        #[arg(long)]
        port: Option<u16>,

        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },

    /// Stop the FSH server
    Stop,

    /// Restart the FSH server
    Restart,

    /// Show server status
    Status,

    /// Manage folder configurations
    #[command(subcommand)]
    Folder(FolderCommands),

    /// Generate default configuration file
    Config {
        /// Output path for config file
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },

    /// Validate configuration file
    Validate,
}

#[derive(Subcommand)]
enum FolderCommands {
    /// List configured folders
    List,

    /// Add a new folder
    Add {
        /// Folder name
        name: String,

        /// Folder path
        path: PathBuf,

        /// Shell type (powershell, cmd, bash, git-bash)
        #[arg(long, default_value = "powershell")]
        shell: String,

        /// Description
        #[arg(long)]
        description: Option<String>,

        /// Make folder read-only
        #[arg(long)]
        readonly: bool,
    },

    /// Remove a folder
    Remove {
        /// Folder name to remove
        name: String,
    },

    /// Show folder details
    Show {
        /// Folder name
        name: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose);

    // Load configuration
    let config_path = cli.config.unwrap_or_else(|| {
        Config::get_default_config_path().unwrap_or_else(|_| PathBuf::from("fsh_config.toml"))
    });

    let result = match cli.command {
        Commands::Start { host, port, foreground } => {
            start_server(config_path, host, port, foreground).await
        }
        Commands::Stop => {
            stop_server().await
        }
        Commands::Restart => {
            restart_server().await
        }
        Commands::Status => {
            show_status().await
        }
        Commands::Folder(folder_cmd) => {
            handle_folder_command(config_path, folder_cmd).await
        }
        Commands::Config { output, force } => {
            generate_config(output.unwrap_or(config_path), force).await
        }
        Commands::Validate => {
            validate_config(config_path).await
        }
    };

    if let Err(e) = result {
        error!("Command failed: {}", e);
        std::process::exit(1);
    }
}

fn init_logging(verbose: bool) {
    let level = if verbose { "debug" } else { "info" };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("fsh={},fsh_server={}", level, level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn start_server(
    config_path: PathBuf,
    host_override: Option<String>,
    port_override: Option<u16>,
    _foreground: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting FSH server...");

    // Load configuration
    let mut config = if config_path.exists() {
        Config::load_from_file(&config_path)?
    } else {
        warn!("Configuration file not found at {:?}, using defaults", config_path);
        Config::default()
    };

    // Apply command line overrides
    if let Some(host) = host_override {
        config.server.host = host;
    }
    if let Some(port) = port_override {
        config.server.port = port;
    }

    // Validate configuration
    config.validate().map_err(|e| format!("Configuration validation failed: {}", e))?;

    // Create and start server
    let mut server = FshServer::new(config)?;

    info!("FSH server configuration loaded from {:?}", config_path);
    info!("Starting FSH server on {}:{}", server.config().server.host, server.config().server.port);

    // Handle Ctrl+C gracefully
    tokio::select! {
        result = server.start() => {
            match result {
                Ok(_) => info!("FSH server stopped normally"),
                Err(e) => error!("FSH server error: {}", e),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            if let Err(e) = server.stop().await {
                error!("Error during shutdown: {}", e);
            }
        }
    }

    Ok(())
}

async fn stop_server() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement server stop functionality
    // This would typically involve sending a signal to a running daemon
    println!("Stop command not yet implemented");
    Ok(())
}

async fn restart_server() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement server restart functionality
    println!("Restart command not yet implemented");
    Ok(())
}

async fn show_status() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Implement status checking
    // This would typically check if the server is running and show stats
    println!("Status command not yet implemented");
    Ok(())
}

async fn handle_folder_command(
    config_path: PathBuf,
    folder_cmd: FolderCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::load_or_create_default(&config_path)?;

    match folder_cmd {
        FolderCommands::List => {
            println!("Configured folders:");
            for folder in &config.folders {
                println!("  {} - {} ({:?})", folder.name, folder.path, folder.shell_type);
                if let Some(desc) = &folder.description {
                    println!("    Description: {}", desc);
                }
                if folder.readonly {
                    println!("    [Read-only]");
                }
            }
        }

        FolderCommands::Add { name, path, shell, description, readonly } => {
            use fsh::protocol::ShellType;

            let shell_type = match shell.to_lowercase().as_str() {
                "powershell" => ShellType::PowerShell,
                "cmd" => ShellType::Cmd,
                "bash" => ShellType::Bash,
                "git-bash" => ShellType::GitBash,
                _ => {
                    error!("Invalid shell type: {}. Valid options: powershell, cmd, bash, git-bash", shell);
                    return Err("Invalid shell type".into());
                }
            };

            let folder = fsh::config::FolderConfig::new(name.clone(), &path)
                .with_shell_type(shell_type)
                .with_readonly(readonly);

            let folder = if let Some(desc) = description {
                folder.with_description(desc)
            } else {
                folder
            };

            config.add_folder(folder)?;
            config.save_to_file(&config_path)?;

            println!("Folder '{}' added successfully", name);
        }

        FolderCommands::Remove { name } => {
            config.remove_folder(&name)?;
            config.save_to_file(&config_path)?;
            println!("Folder '{}' removed successfully", name);
        }

        FolderCommands::Show { name } => {
            if let Some(folder) = config.find_folder_by_name(&name) {
                println!("Folder: {}", folder.name);
                println!("  Path: {}", folder.path);
                println!("  Shell: {:?}", folder.shell_type);
                println!("  Permissions: {:?}", folder.permissions);
                println!("  Read-only: {}", folder.readonly);
                if let Some(desc) = &folder.description {
                    println!("  Description: {}", desc);
                }
                println!("  Allowed commands: {}", folder.allowed_commands.join(", "));
                if !folder.blocked_commands.is_empty() {
                    println!("  Blocked commands: {}", folder.blocked_commands.join(", "));
                }
                if !folder.environment_vars.is_empty() {
                    println!("  Environment variables:");
                    for (key, value) in &folder.environment_vars {
                        println!("    {}={}", key, value);
                    }
                }
            } else {
                error!("Folder '{}' not found", name);
                return Err("Folder not found".into());
            }
        }
    }

    Ok(())
}

async fn generate_config(
    output_path: PathBuf,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if output_path.exists() && !force {
        error!("Configuration file already exists at {:?}. Use --force to overwrite.", output_path);
        return Err("File exists".into());
    }

    let config = Config::default();
    config.save_to_file(&output_path)?;

    println!("Default configuration file generated at {:?}", output_path);
    println!("Edit the file to add your folder configurations and then start the server.");

    Ok(())
}

async fn validate_config(config_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("Validating configuration file: {:?}", config_path);

    let config = Config::load_from_file(&config_path)?;
    config.validate()?;

    println!("✓ Configuration is valid");
    println!("Server settings:");
    println!("  Host: {}", config.server.host);
    println!("  Port: {}", config.server.port);
    println!("  Max connections: {}", config.server.max_connections);

    println!("Security settings:");
    println!("  Authentication required: {}", config.security.require_authentication);
    println!("  Auth methods: {:?}", config.security.auth_methods);

    println!("Configured folders: {}", config.folders.len());
    for folder in &config.folders {
        println!("  {} -> {}", folder.name, folder.path);

        // Validate folder existence
        if let Err(e) = folder.validate() {
            warn!("  ⚠ Warning: {}", e);
        } else {
            println!("  ✓ Valid");
        }
    }

    Ok(())
}