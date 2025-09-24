use fsh::{
    config::{Config, FolderConfig},
    server::FshServer,
    client::FshClient,
    protocol::ShellType,
};
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("FSH Basic Usage Example");

    // Create a temporary directory for the example
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().to_path_buf();

    info!("Created temporary folder: {:?}", temp_path);

    // Create some example files
    std::fs::write(temp_path.join("README.md"), "# Example Project\n\nThis is a test folder for FSH.\n")?;
    std::fs::write(temp_path.join("hello.txt"), "Hello, FSH!\n")?;
    std::fs::create_dir(temp_path.join("src"))?;
    std::fs::write(temp_path.join("src").join("main.rs"), "fn main() {\n    println!(\"Hello, world!\");\n}\n")?;

    // Create server configuration
    let mut config = Config::default();
    config.server.port = 12345; // Use a different port for the example

    // Add the temporary folder as an available folder
    let folder_config = FolderConfig::new("example".to_string(), &temp_path)
        .with_description("Example temporary folder".to_string())
        .with_shell_type(ShellType::default());

    config.add_folder(folder_config)?;

    info!("Starting FSH server on port {}", config.server.port);

    // Start the server in a background task
    let mut server = FshServer::new(config)?;
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            error!("Server error: {}", e);
        }
    });

    // Give the server time to start
    sleep(Duration::from_millis(500)).await;

    // Create and connect a client
    info!("Connecting FSH client");
    let mut client = FshClient::new("127.0.0.1:12345".to_string());

    // Connect to server
    client.connect().await?;
    info!("✓ Connected to server");

    // Authenticate (using default token)
    let mut credentials = HashMap::new();
    credentials.insert("token".to_string(), "default".to_string());
    client.authenticate("token", credentials).await?;
    info!("✓ Authenticated");

    // Bind to the example folder
    let folder_info = client.bind_folder("example", None).await?;
    info!("✓ Bound to folder: {}", folder_info.name);

    // Wait for session to be ready
    let (prompt, working_dir) = client.wait_for_session_ready().await?;
    info!("✓ Session ready");
    info!("  Prompt: {}", prompt);
    info!("  Working directory: {}", working_dir);

    // List files in the folder
    info!("Listing files:");
    let files = client.list_files(".", false).await?;
    for file in &files {
        let file_type = if file.is_directory { "DIR " } else { "FILE" };
        info!("  {} {:<15} {:>8} bytes", file_type, file.name, file.size);
    }

    // Execute some commands
    let commands = vec![
        ("pwd", vec![]),
        ("ls", vec!["-la".to_string()]),
    ];

    for (cmd, args) in commands {
        info!("Executing command: {} {:?}", cmd, args);

        let mut output_rx = client.execute_command(cmd, args).await?;

        // Collect output
        let mut stdout_output = String::new();
        let mut stderr_output = String::new();

        while let Some(output) = output_rx.recv().await {
            match output.output_type {
                fsh::client::CommandOutputType::Stdout => {
                    stdout_output.push_str(&output.data);
                }
                fsh::client::CommandOutputType::Stderr => {
                    stderr_output.push_str(&output.data);
                }
                fsh::client::CommandOutputType::Complete => {
                    info!("  Command completed: {}", output.data);
                    break;
                }
                fsh::client::CommandOutputType::Error => {
                    error!("  Command error: {}", output.data);
                    break;
                }
            }
        }

        if !stdout_output.is_empty() {
            info!("  Output:\n{}", stdout_output);
        }
        if !stderr_output.is_empty() {
            error!("  Errors:\n{}", stderr_output);
        }
    }

    // Disconnect from server
    client.disconnect().await?;
    info!("✓ Disconnected from server");

    // Stop the server
    server_handle.abort();

    info!("Example completed successfully!");
    info!("Temporary folder was: {:?}", temp_path);

    Ok(())
}