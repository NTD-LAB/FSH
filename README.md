# FSH - Folder Shell Protocol

FSH (Folder Shell Protocol) is a secure folder-level remote shell protocol implemented in Rust. It provides SSH-like functionality but restricts access to specific folders, making it ideal for secure remote development and file management.

## Features

### 🔒 Security First
- **Folder-level isolation**: Each connection is restricted to a specific folder
- **Command sandboxing**: Configurable allow/block lists for commands
- **Path validation**: Prevents directory traversal attacks
- **Rate limiting**: Protection against DoS attacks
- **Audit logging**: Complete security event logging
- **Multi-layer authentication**: Token and password-based auth

### 🚀 Developer Friendly
- **Multiple shell support**: PowerShell, CMD, Bash, Git Bash
- **Cross-platform**: Windows, Linux, macOS
- **Interactive terminal**: Full SSH-like terminal experience
- **File operations**: List, read, write files within folders
- **Project awareness**: Auto-detection of project types (Node.js, Rust, Python, etc.)

### ⚡ High Performance
- **Async Rust**: Built with Tokio for high concurrency
- **Binary protocol**: Efficient message encoding
- **Connection pooling**: Support for multiple concurrent sessions
- **Resource management**: Automatic cleanup of expired sessions

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/fsh-protocol/fsh
cd fsh

# Build the project
cargo build --release

# Install binaries
cargo install --path .
```

### Server Setup

1. **Generate default configuration:**
```bash
fsh-server config --output fsh_config.toml
```

2. **Edit configuration to add your folders:**
```toml
[[folders]]
name = "My Project"
path = "C:\\Users\\YourName\\Projects\\MyProject"
permissions = ["read", "write", "execute"]
shell_type = "powershell"
description = "My development project"
```

3. **Start the server:**
```bash
fsh-server start
```

### Client Usage

#### Interactive Terminal
```bash
# Connect to server with interactive terminal
fsh-client connect --folder "My Project" --token default
```

#### Execute Single Commands
```bash
# Execute a single command
fsh-client exec --folder "My Project" --token default "npm install"

# List files
fsh-client list --folder "My Project" --token default
```

#### Test Connection
```bash
# Test server connectivity
fsh-client test
```

## Configuration

### Server Configuration (`fsh_config.toml`)

```toml
[server]
host = "127.0.0.1"           # Server bind address
port = 2222                  # Server port
max_connections = 10         # Maximum concurrent connections
connection_timeout_seconds = 30
session_timeout_minutes = 60

[security]
require_authentication = true
auth_methods = ["token"]     # Available: ["token", "password"]
max_failed_attempts = 3
enable_logging = true
log_file = "fsh_server.log"

[[folders]]
name = "Development Projects"
path = "/home/user/projects"
permissions = ["read", "write", "execute"]
shell_type = "bash"
description = "Development workspace"
readonly = false

# Allowed commands (empty = allow all except blocked)
allowed_commands = [
    "ls", "cat", "echo", "pwd", "cd", "mkdir", "cp", "mv", "rm",
    "git", "npm", "cargo", "python", "code", "vim"
]

# Blocked commands
blocked_commands = [
    "sudo", "su", "passwd", "chmod", "chown", "format", "fdisk"
]

# Environment variables for this folder
[folders.environment_vars]
NODE_ENV = "development"
PROJECT_TYPE = "nodejs"
```

### Folder Management

```bash
# Add a new folder
fsh-server folder add "Web Project" /var/www/html --shell bash --description "Web server files"

# List configured folders
fsh-server folder list

# Remove a folder
fsh-server folder remove "Web Project"

# Show folder details
fsh-server folder show "Web Project"
```

## Security Features

### Multi-Layer Security

1. **Protocol Layer**: Path validation, command filtering
2. **Shell Layer**: Sandboxed execution environment
3. **System Layer**: Operating system permissions

### Authentication Methods

- **Token Authentication**: Simple token-based auth
- **Password Authentication**: Username/password (planned)
- **Certificate Authentication**: Client certificates (planned)

### Audit Logging

All security events are logged:
- Connection attempts
- Authentication events
- Command executions
- File access
- Permission denials
- Suspicious activities

Example log entry:
```json
{
  "timestamp": 1640995200,
  "event_type": "CommandExecution",
  "source_ip": "192.168.1.100",
  "session_id": "session-123",
  "resource": "npm install",
  "details": "Executed command: npm install"
}
```

## Protocol Overview

FSH uses a binary protocol over TCP with the following message flow:

```
Client                          Server
  |                               |
  |-------- Connect ------------->|
  |<----- ConnectResponse --------|
  |                               |
  |------ Authenticate ---------->|
  |<----- AuthResponse -----------|
  |                               |
  |------- FolderBind ----------->|
  |<----- FolderBound ------------|
  |                               |
  |------ SessionStart ---------->|
  |<----- SessionReady -----------|
  |                               |
  |------- Command -------------->|
  |<---- CommandOutput -----------| (streaming)
  |<--- CommandComplete ----------|
```

## API Examples

### Rust API

```rust
use fsh::{client::FshClient, config::Config, server::FshServer};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let mut client = FshClient::new("127.0.0.1:2222".to_string());

    // Connect and authenticate
    client.connect().await?;

    let mut credentials = HashMap::new();
    credentials.insert("token".to_string(), "your-token".to_string());
    client.authenticate("token", credentials).await?;

    // Bind to folder
    let folder_info = client.bind_folder("My Project", None).await?;

    // Wait for session
    let (prompt, working_dir) = client.wait_for_session_ready().await?;

    // Execute command
    let mut output_rx = client.execute_command("ls", vec!["-la".to_string()]).await?;

    while let Some(output) = output_rx.recv().await {
        match output.output_type {
            fsh::client::CommandOutputType::Stdout => print!("{}", output.data),
            fsh::client::CommandOutputType::Complete => break,
            _ => {}
        }
    }

    client.disconnect().await?;
    Ok(())
}
```

## Development

### Building from Source

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run --bin fsh-server start
```

### Running Examples

```bash
# Basic usage example
cargo run --example basic_usage

# Advanced server configuration
cargo run --example advanced_config
```

### Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature-name`
3. Commit changes: `git commit -am 'Add feature'`
4. Push to branch: `git push origin feature-name`
5. Submit a pull request

## Use Cases

### Remote Development
- Secure access to development environments
- Isolated project workspaces
- Team collaboration on shared resources

### File Management
- Secure file operations on remote servers
- Backup and synchronization scripts
- Automated deployment pipelines

### DevOps Automation
- CI/CD pipeline execution
- Remote build environments
- Container orchestration

### Educational Environments
- Student project sandboxes
- Controlled learning environments
- Assignment submission systems

## Comparison with SSH

| Feature | SSH | FSH |
|---------|-----|-----|
| Access Scope | Full system | Single folder |
| Security | System-level | Folder-level |
| Setup Complexity | High | Low |
| User Management | OS users | Configuration-based |
| Command Filtering | Limited | Extensive |
| Audit Logging | Basic | Comprehensive |
| Cross-platform | Good | Excellent |

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Roadmap

- [ ] Web-based client interface
- [ ] File synchronization capabilities
- [ ] Advanced authentication methods
- [ ] Cluster support for high availability
- [ ] Plugin system for custom commands
- [ ] Mobile client applications
- [ ] Integration with popular IDEs

## Support

- GitHub Issues: [Report bugs and feature requests](https://github.com/fsh-protocol/fsh/issues)
- Documentation: [Full documentation](https://docs.fsh-protocol.org)
- Discord: [Join our community](https://discord.gg/fsh-protocol)

---

**FSH - Secure, Simple, Folder-level Remote Shell Access** 🚀