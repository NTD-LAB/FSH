use clap::{Parser, Subcommand};
use fsh::client::{FshClient, Terminal};
use std::collections::HashMap;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "fsh-client")]
#[command(about = "FSH (Folder Shell Protocol) Client")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Server address
    #[arg(short, long, default_value = "127.0.0.1:2222")]
    server: String,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to FSH server with interactive terminal
    Connect {
        /// Folder to bind to
        #[arg(short, long)]
        folder: Option<String>,

        /// Authentication token
        #[arg(short, long)]
        token: Option<String>,

        /// Preferred shell type (powershell, cmd, bash, git-bash)
        #[arg(long)]
        shell: Option<String>,
    },

    /// Execute a single command and exit
    Exec {
        /// Folder to bind to
        #[arg(short, long)]
        folder: String,

        /// Authentication token
        #[arg(short, long)]
        token: Option<String>,

        /// Command to execute
        command: String,

        /// Command arguments
        args: Vec<String>,
    },

    /// List files in a folder
    List {
        /// Folder to bind to
        #[arg(short, long)]
        folder: String,

        /// Authentication token
        #[arg(short, long)]
        token: Option<String>,

        /// Path to list (relative to folder root)
        #[arg(default_value = ".")]
        path: String,

        /// Show hidden files
        #[arg(long)]
        hidden: bool,
    },

    /// Test connection to server
    Test,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose);

    let result = match cli.command {
        Commands::Connect { folder, token, shell } => {
            connect_interactive(cli.server, folder, token, shell).await
        }
        Commands::Exec { folder, token, command, args } => {
            execute_command(cli.server, folder, token, command, args).await
        }
        Commands::List { folder, token, path, hidden } => {
            list_files(cli.server, folder, token, path, hidden).await
        }
        Commands::Test => {
            test_connection(cli.server).await
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
                .unwrap_or_else(|_| format!("fsh={},fsh_client={}", level, level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn connect_interactive(
    server_addr: String,
    _folder: Option<String>,
    _token: Option<String>,
    _shell: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting interactive FSH client");

    let mut terminal = Terminal::new(server_addr);

    // Run the interactive terminal
    terminal.run().await?;

    Ok(())
}

async fn execute_command(
    server_addr: String,
    folder: String,
    token: Option<String>,
    command: String,
    args: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Executing single command: {} {:?}", command, args);

    let mut client = FshClient::new(server_addr);

    // Connect
    client.connect().await?;

    // Authenticate if token provided
    if let Some(token) = token {
        let mut credentials = HashMap::new();
        credentials.insert("token".to_string(), token);
        client.authenticate("token", credentials).await?;
    }

    // Bind to folder
    let shell_type = None; // Use default shell type

    client.bind_folder(&folder, shell_type).await?;

    // Wait for session ready
    client.wait_for_session_ready().await?;

    // Execute command
    let mut output_rx = client.execute_command(&command, args).await?;

    // Print output
    while let Some(output) = output_rx.recv().await {
        match output.output_type {
            fsh::client::CommandOutputType::Stdout => {
                print!("{}", output.data);
            }
            fsh::client::CommandOutputType::Stderr => {
                eprint!("{}", output.data);
            }
            fsh::client::CommandOutputType::Complete => {
                break;
            }
            fsh::client::CommandOutputType::Error => {
                eprintln!("Error: {}", output.data);
                break;
            }
        }
    }

    // Disconnect
    client.disconnect().await?;

    Ok(())
}

async fn list_files(
    server_addr: String,
    folder: String,
    token: Option<String>,
    path: String,
    show_hidden: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Listing files in folder: {}, path: {}", folder, path);

    let mut client = FshClient::new(server_addr);

    // Connect
    client.connect().await?;

    // Authenticate if token provided
    if let Some(token) = token {
        let mut credentials = HashMap::new();
        credentials.insert("token".to_string(), token);
        client.authenticate("token", credentials).await?;
    }

    // Bind to folder
    client.bind_folder(&folder, None).await?;

    // Wait for session ready
    client.wait_for_session_ready().await?;

    // List files
    let files = client.list_files(&path, show_hidden).await?;

    // Print file list
    println!("Files in {}:", path);
    for file in files {
        let file_type = if file.is_directory { "DIR" } else { "FILE" };
        let size = if file.is_directory { "-".to_string() } else { file.size.to_string() };

        println!("{:>6} {:>10} {:>20} {}",
                file_type,
                size,
                file.modified.format("%Y-%m-%d %H:%M"),
                file.name);
    }

    // Disconnect
    client.disconnect().await?;

    Ok(())
}

async fn test_connection(server_addr: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Testing connection to {}", server_addr);

    let mut client = FshClient::new(server_addr.clone());

    match client.connect().await {
        Ok(_) => {
            println!("✓ Successfully connected to {}", server_addr);

            // Try to disconnect gracefully
            if let Err(e) = client.disconnect().await {
                eprintln!("Warning: Failed to disconnect gracefully: {}", e);
            } else {
                println!("✓ Disconnected gracefully");
            }

            Ok(())
        }
        Err(e) => {
            println!("✗ Failed to connect to {}: {}", server_addr, e);
            Err(e.into())
        }
    }
}

// Helper function to get shell type from string
fn parse_shell_type(shell: &str) -> Option<fsh::protocol::ShellType> {
    match shell.to_lowercase().as_str() {
        "powershell" => Some(fsh::protocol::ShellType::PowerShell),
        "cmd" => Some(fsh::protocol::ShellType::Cmd),
        "bash" => Some(fsh::protocol::ShellType::Bash),
        "git-bash" => Some(fsh::protocol::ShellType::GitBash),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_type_parsing() {
        assert!(matches!(parse_shell_type("powershell"), Some(fsh::protocol::ShellType::PowerShell)));
        assert!(matches!(parse_shell_type("cmd"), Some(fsh::protocol::ShellType::Cmd)));
        assert!(matches!(parse_shell_type("bash"), Some(fsh::protocol::ShellType::Bash)));
        assert!(matches!(parse_shell_type("git-bash"), Some(fsh::protocol::ShellType::GitBash)));
        assert!(parse_shell_type("invalid").is_none());
    }
}