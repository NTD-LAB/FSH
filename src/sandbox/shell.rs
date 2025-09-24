use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::protocol::{FshError, FshResult, ShellType};
use super::{PathValidator, SandboxConfig};

#[derive(Debug)]
pub struct SandboxedShell {
    session_id: String,
    config: SandboxConfig,
    validator: PathValidator,
    current_process: Option<Child>,
    working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ShellOutput {
    pub output_type: OutputType,
    pub data: String,
}

#[derive(Debug, Clone)]
pub enum OutputType {
    Stdout,
    Stderr,
}

impl SandboxedShell {
    pub fn new(config: SandboxConfig) -> FshResult<Self> {
        let validator = PathValidator::new(config.root_path.clone())?;
        let session_id = Uuid::new_v4().to_string();

        Ok(Self {
            session_id,
            working_directory: config.root_path.clone(),
            config,
            validator,
            current_process: None,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn working_directory(&self) -> &PathBuf {
        &self.working_directory
    }

    pub fn get_shell_prompt(&self) -> String {
        let relative_dir = self.validator
            .get_relative_path(&self.working_directory)
            .unwrap_or_else(|_| PathBuf::from("."));

        match self.config.shell_type {
            ShellType::PowerShell => format!("PS {}> ", relative_dir.display()),
            ShellType::Cmd => format!("{}> ", relative_dir.display()),
            ShellType::Bash | ShellType::GitBash => format!("{}$ ", relative_dir.display()),
        }
    }

    pub async fn execute_command(
        &mut self,
        command: &str,
        args: &[String],
    ) -> FshResult<(mpsc::Receiver<ShellOutput>, mpsc::Receiver<CommandResult>)> {
        // Validate command
        let validated_command = self.validator.validate_command_path(command)?;

        if !self.config.is_command_allowed(&validated_command) {
            return Err(FshError::PermissionDenied(
                format!("Command '{}' is not allowed", command)
            ));
        }

        // Handle special built-in commands
        if let Some(result) = self.handle_builtin_command(command, args).await? {
            let (output_tx, output_rx) = mpsc::channel(100);
            let (result_tx, result_rx) = mpsc::channel(1);

            tokio::spawn(async move {
                let _ = output_tx.send(ShellOutput {
                    output_type: OutputType::Stdout,
                    data: result.stdout.clone(),
                }).await;

                if !result.stderr.is_empty() {
                    let _ = output_tx.send(ShellOutput {
                        output_type: OutputType::Stderr,
                        data: result.stderr.clone(),
                    }).await;
                }

                let _ = result_tx.send(result).await;
            });

            return Ok((output_rx, result_rx));
        }

        // Execute external command
        self.execute_external_command(command, args).await
    }

    async fn handle_builtin_command(
        &mut self,
        command: &str,
        args: &[String],
    ) -> FshResult<Option<CommandResult>> {
        let start_time = std::time::Instant::now();

        match command.to_lowercase().as_str() {
            "cd" => {
                let target_dir = if args.is_empty() {
                    self.config.root_path.clone()
                } else {
                    let target = &args[0];

                    // Special handling for cd ..
                    if target == ".." {
                        let parent = self.working_directory.parent();
                        if let Some(parent) = parent {
                            if parent.starts_with(&self.config.root_path) {
                                parent.to_path_buf()
                            } else {
                                return Ok(Some(CommandResult {
                                    exit_code: 1,
                                    stdout: String::new(),
                                    stderr: "Access denied: Cannot navigate above project folder".to_string(),
                                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                                }));
                            }
                        } else {
                            self.config.root_path.clone()
                        }
                    } else {
                        let absolute_path = if PathBuf::from(target).is_absolute() {
                            self.validator.validate_path(target)?
                        } else {
                            self.working_directory.join(target)
                        };

                        self.validator.validate_path(&absolute_path.to_string_lossy())?
                    }
                };

                if target_dir.is_dir() {
                    self.working_directory = target_dir;
                    Ok(Some(CommandResult {
                        exit_code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    }))
                } else {
                    Ok(Some(CommandResult {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("Directory not found: {}", args[0]),
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    }))
                }
            }
            "pwd" => {
                let relative_path = self.validator
                    .get_relative_path(&self.working_directory)
                    .unwrap_or_else(|_| PathBuf::from("."));

                Ok(Some(CommandResult {
                    exit_code: 0,
                    stdout: format!("{}\n", relative_path.display()),
                    stderr: String::new(),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                }))
            }
            _ => Ok(None), // Not a built-in command
        }
    }

    async fn execute_external_command(
        &mut self,
        command: &str,
        args: &[String],
    ) -> FshResult<(mpsc::Receiver<ShellOutput>, mpsc::Receiver<CommandResult>)> {
        let (output_tx, output_rx) = mpsc::channel(100);
        let (result_tx, result_rx) = mpsc::channel(1);

        // Check if this is a system-aware command
        let is_system_aware = self.config.is_system_aware_command(command);

        // Prepare command based on shell type
        let (shell_cmd, shell_args) = self.prepare_shell_command(command, args)?;

        let mut cmd = Command::new(&shell_cmd);
        cmd.args(&shell_args)
            .current_dir(&self.working_directory)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::piped());

        // For system-aware commands, inherit system environment
        if is_system_aware {
            // Use system environment variables
            cmd.env_clear();
            for (key, value) in std::env::vars() {
                cmd.env(&key, &value);
            }
            // Override with custom environment vars if needed
            for (key, value) in &self.config.environment_vars {
                cmd.env(key, value);
            }
            // Ensure the working directory is in PATH for local executables
            if let Ok(path) = std::env::var("PATH") {
                let new_path = format!("{};{}", self.working_directory.display(), path);
                cmd.env("PATH", new_path);
            }
        } else {
            // Regular sandboxed mode: only use configured environment
            for (key, value) in &self.config.environment_vars {
                cmd.env(key, value);
            }
        }

        let start_time = std::time::Instant::now();
        let mut child = cmd.spawn()
            .map_err(|e| FshError::ShellError(format!("Failed to spawn command: {}", e)))?;

        let stdout = child.stdout.take()
            .ok_or_else(|| FshError::ShellError("Failed to capture stdout".to_string()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| FshError::ShellError("Failed to capture stderr".to_string()))?;

        let validator = self.validator.clone();

        // Handle stdout
        let output_tx_stdout = output_tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let sanitized_line = validator.sanitize_output_path(&line);
                let _ = output_tx_stdout.send(ShellOutput {
                    output_type: OutputType::Stdout,
                    data: format!("{}\n", sanitized_line),
                }).await;
            }
        });

        // Handle stderr
        let output_tx_stderr = output_tx.clone();
        let validator_stderr = self.validator.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let sanitized_line = validator_stderr.sanitize_output_path(&line);
                let _ = output_tx_stderr.send(ShellOutput {
                    output_type: OutputType::Stderr,
                    data: format!("{}\n", sanitized_line),
                }).await;
            }
        });

        // Wait for process completion
        tokio::spawn(async move {
            let result = match child.wait().await {
                Ok(status) => CommandResult {
                    exit_code: status.code().unwrap_or(-1),
                    stdout: String::new(),
                    stderr: String::new(),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                },
                Err(e) => CommandResult {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!("Process execution failed: {}", e),
                    execution_time_ms: start_time.elapsed().as_millis() as u64,
                },
            };

            let _ = result_tx.send(result).await;
        });

        Ok((output_rx, result_rx))
    }

    fn prepare_shell_command(&self, command: &str, args: &[String]) -> FshResult<(String, Vec<String>)> {
        let full_command = if args.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, args.join(" "))
        };

        match self.config.shell_type {
            ShellType::PowerShell => {
                Ok(("powershell".to_string(), vec![
                    "-NoExit".to_string(),
                    "-Command".to_string(),
                    full_command,
                ]))
            }
            ShellType::Cmd => {
                Ok(("cmd".to_string(), vec![
                    "/c".to_string(),
                    full_command,
                ]))
            }
            ShellType::Bash => {
                Ok(("bash".to_string(), vec![
                    "-c".to_string(),
                    full_command,
                ]))
            }
            ShellType::GitBash => {
                Ok(("bash".to_string(), vec![
                    "-c".to_string(),
                    full_command,
                ]))
            }
        }
    }

    pub async fn kill_current_process(&mut self) -> FshResult<()> {
        if let Some(mut process) = self.current_process.take() {
            process.kill().await
                .map_err(|e| FshError::ShellError(format!("Failed to kill process: {}", e)))?;
        }
        Ok(())
    }

    pub fn list_files(&self, path: Option<&str>, show_hidden: bool) -> FshResult<Vec<crate::protocol::message::FileEntry>> {
        let target_path = if let Some(path) = path {
            self.validator.validate_path(path)?
        } else {
            self.working_directory.clone()
        };

        let mut entries = Vec::new();

        for entry in std::fs::read_dir(&target_path)
            .map_err(|e| FshError::ShellError(format!("Failed to read directory: {}", e)))? {
            let entry = entry.map_err(|e| FshError::ShellError(format!("Failed to read entry: {}", e)))?;
            let metadata = entry.metadata()
                .map_err(|e| FshError::ShellError(format!("Failed to read metadata: {}", e)))?;

            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files if not requested
            if !show_hidden && file_name.starts_with('.') {
                continue;
            }

            let relative_path = self.validator.get_relative_path(&entry.path())
                .unwrap_or_else(|_| entry.path().strip_prefix(&self.config.root_path).unwrap_or(&entry.path()).to_path_buf());

            entries.push(crate::protocol::message::FileEntry {
                name: file_name,
                path: relative_path.to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified()
                    .map(|time| chrono::DateTime::from(time))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                permissions: None, // TODO: Implement permission strings
            });
        }

        // Sort entries: directories first, then files, alphabetically within each group
        entries.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sandboxed_shell_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SandboxConfig::new(temp_dir.path().to_path_buf(), ShellType::Bash);
        let shell = SandboxedShell::new(config);
        assert!(shell.is_ok());
    }

    #[tokio::test]
    async fn test_builtin_cd_command() {
        let temp_dir = TempDir::new().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();

        let config = SandboxConfig::new(temp_dir.path().to_path_buf(), ShellType::Bash);
        let mut shell = SandboxedShell::new(config).unwrap();

        // Test cd to subdirectory
        let result = shell.handle_builtin_command("cd", &["subdir".to_string()]).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().exit_code, 0);
        assert_eq!(shell.working_directory, sub_dir);

        // Test cd .. (should work)
        let result = shell.handle_builtin_command("cd", &["..".to_string()]).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().exit_code, 0);

        // Test cd .. beyond root (should fail)
        let result = shell.handle_builtin_command("cd", &["..".to_string()]).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().exit_code, 1);
    }
}