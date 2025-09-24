use crate::client::{FshClient, CommandOutputType};
use crate::protocol::{FshError, FshResult};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::collections::HashMap;
use std::io::{Write, stdout};
use tracing::debug;

pub struct Terminal {
    client: FshClient,
    current_prompt: String,
    current_directory: String,
    command_history: Vec<String>,
    history_index: usize,
    input_buffer: String,
    cursor_position: usize,
}

impl Terminal {
    pub fn new(server_addr: String) -> Self {
        Self {
            client: FshClient::new(server_addr),
            current_prompt: "FSH> ".to_string(),
            current_directory: "/".to_string(),
            command_history: Vec::new(),
            history_index: 0,
            input_buffer: String::new(),
            cursor_position: 0,
        }
    }

    pub async fn run(&mut self) -> FshResult<()> {
        // Setup terminal
        terminal::enable_raw_mode()
            .map_err(|e| FshError::NetworkError(format!("Failed to enable raw mode: {}", e)))?;

        execute!(stdout(), terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))
            .map_err(|e| FshError::NetworkError(format!("Terminal setup failed: {}", e)))?;

        // Show welcome message
        self.print_welcome().await?;

        // Connect to server
        if let Err(e) = self.connect_and_setup().await {
            self.print_error(&format!("Connection failed: {}", e)).await?;
            self.cleanup_terminal()?;
            return Err(e);
        }

        // Main terminal loop
        let result = self.terminal_loop().await;

        // Cleanup
        self.cleanup_terminal()?;

        result
    }

    async fn print_welcome(&mut self) -> FshResult<()> {
        self.print_colored(
            "FSH - Folder Shell Protocol Client\n",
            Color::Cyan,
        ).await?;

        self.print_colored(
            "Type 'help' for available commands, 'exit' to quit.\n\n",
            Color::Grey,
        ).await?;

        Ok(())
    }

    async fn connect_and_setup(&mut self) -> FshResult<()> {
        self.print_status("Connecting to FSH server...").await?;

        // Connect
        self.client.connect().await?;

        // Authenticate (simple token for now)
        let mut credentials = HashMap::new();
        credentials.insert("token".to_string(), "default".to_string());

        if let Err(e) = self.client.authenticate("token", credentials).await {
            // Authentication might not be required
            debug!("Authentication not required or failed: {}", e);
        }

        // Get available folders and let user choose
        self.print_status("Getting available folders...").await?;

        // For now, just try to bind to the first available folder
        // In a real implementation, you'd show a list and let the user choose
        let folder_name = self.prompt_for_folder().await?;

        // Bind to folder
        let folder_info = self.client.bind_folder(&folder_name, None).await?;

        self.print_status(&format!("Bound to folder: {}", folder_info.name)).await?;

        // Wait for session to be ready
        let (prompt, working_dir) = self.client.wait_for_session_ready().await?;

        self.current_prompt = prompt;
        self.current_directory = working_dir;

        self.print_success(&format!("Session ready! Working directory: {}", self.current_directory)).await?;

        Ok(())
    }

    async fn prompt_for_folder(&mut self) -> FshResult<String> {
        // For now, just use a default folder name
        // In a real implementation, this would be interactive
        Ok("default".to_string())
    }

    async fn terminal_loop(&mut self) -> FshResult<()> {
        loop {
            // Display prompt and current input
            self.display_prompt().await?;

            // Handle input
            match self.read_input().await? {
                InputResult::Command(command) => {
                    if command.trim().is_empty() {
                        continue;
                    }

                    // Add to history
                    self.command_history.push(command.clone());
                    self.history_index = self.command_history.len();

                    // Handle built-in commands
                    if self.handle_builtin_command(&command).await? {
                        continue;
                    }

                    // Execute command on server
                    if let Err(e) = self.execute_remote_command(&command).await {
                        self.print_error(&format!("Command failed: {}", e)).await?;
                    }
                }
                InputResult::Exit => {
                    break;
                }
                InputResult::Continue => {
                    continue;
                }
            }
        }

        // Disconnect from server
        if let Err(e) = self.client.disconnect().await {
            self.print_error(&format!("Disconnect error: {}", e)).await?;
        }

        Ok(())
    }

    async fn display_prompt(&mut self) -> FshResult<()> {
        execute!(
            stdout(),
            Print("\r"),
            terminal::Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Green),
            Print(&self.current_prompt),
            ResetColor,
            Print(&self.input_buffer),
        ).map_err(|e| FshError::NetworkError(format!("Display error: {}", e)))?;

        // Position cursor
        let prompt_len = self.current_prompt.len();
        execute!(
            stdout(),
            cursor::MoveTo((prompt_len + self.cursor_position) as u16, cursor::position().unwrap().1)
        ).map_err(|e| FshError::NetworkError(format!("Cursor error: {}", e)))?;

        stdout().flush()
            .map_err(|e| FshError::NetworkError(format!("Flush error: {}", e)))?;

        Ok(())
    }

    async fn read_input(&mut self) -> FshResult<InputResult> {
        loop {
            if let Ok(event) = event::read() {
                match event {
                    Event::Key(KeyEvent { code, modifiers, .. }) => {
                        match (code, modifiers) {
                            // Ctrl+C
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                return Ok(InputResult::Exit);
                            }

                            // Ctrl+D
                            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                                if self.input_buffer.is_empty() {
                                    return Ok(InputResult::Exit);
                                }
                            }

                            // Enter
                            (KeyCode::Enter, _) => {
                                println!(); // New line
                                let command = self.input_buffer.clone();
                                self.input_buffer.clear();
                                self.cursor_position = 0;
                                return Ok(InputResult::Command(command));
                            }

                            // Backspace
                            (KeyCode::Backspace, _) => {
                                if self.cursor_position > 0 {
                                    self.input_buffer.remove(self.cursor_position - 1);
                                    self.cursor_position -= 1;
                                }
                            }

                            // Delete
                            (KeyCode::Delete, _) => {
                                if self.cursor_position < self.input_buffer.len() {
                                    self.input_buffer.remove(self.cursor_position);
                                }
                            }

                            // Arrow keys
                            (KeyCode::Left, _) => {
                                if self.cursor_position > 0 {
                                    self.cursor_position -= 1;
                                }
                            }

                            (KeyCode::Right, _) => {
                                if self.cursor_position < self.input_buffer.len() {
                                    self.cursor_position += 1;
                                }
                            }

                            (KeyCode::Up, _) => {
                                if self.history_index > 0 {
                                    self.history_index -= 1;
                                    if let Some(cmd) = self.command_history.get(self.history_index) {
                                        self.input_buffer = cmd.clone();
                                        self.cursor_position = self.input_buffer.len();
                                    }
                                }
                            }

                            (KeyCode::Down, _) => {
                                if self.history_index < self.command_history.len() {
                                    self.history_index += 1;
                                    if self.history_index == self.command_history.len() {
                                        self.input_buffer.clear();
                                        self.cursor_position = 0;
                                    } else if let Some(cmd) = self.command_history.get(self.history_index) {
                                        self.input_buffer = cmd.clone();
                                        self.cursor_position = self.input_buffer.len();
                                    }
                                }
                            }

                            // Tab completion (placeholder)
                            (KeyCode::Tab, _) => {
                                // TODO: Implement tab completion
                            }

                            // Regular character input
                            (KeyCode::Char(c), _) => {
                                self.input_buffer.insert(self.cursor_position, c);
                                self.cursor_position += 1;
                            }

                            _ => {}
                        }

                        self.display_prompt().await?;
                        return Ok(InputResult::Continue);
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_builtin_command(&mut self, command: &str) -> FshResult<bool> {
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        if parts.is_empty() {
            return Ok(false);
        }

        match parts[0] {
            "exit" | "quit" => {
                self.print_status("Goodbye!").await?;
                return Ok(true); // This will cause exit
            }

            "help" => {
                self.show_help().await?;
                return Ok(true);
            }

            "clear" => {
                execute!(stdout(), terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))
                    .map_err(|e| FshError::NetworkError(format!("Clear failed: {}", e)))?;
                return Ok(true);
            }

            "history" => {
                self.show_history().await?;
                return Ok(true);
            }

            "ls" | "dir" => {
                // Handle file listing
                if let Err(e) = self.list_files(parts.get(1).unwrap_or(&".")).await {
                    self.print_error(&format!("Failed to list files: {}", e)).await?;
                }
                return Ok(true);
            }

            _ => {
                return Ok(false); // Not a built-in command
            }
        }
    }

    async fn execute_remote_command(&mut self, command: &str) -> FshResult<()> {
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        let cmd = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        // Execute command
        let mut output_rx = self.client.execute_command(cmd, args).await?;

        // Display output as it comes
        while let Some(output) = output_rx.recv().await {
            match output.output_type {
                CommandOutputType::Stdout => {
                    print!("{}", output.data);
                    stdout().flush().unwrap();
                }
                CommandOutputType::Stderr => {
                    self.print_colored(&output.data, Color::Red).await?;
                }
                CommandOutputType::Complete => {
                    debug!("{}", output.data);
                    break;
                }
                CommandOutputType::Error => {
                    self.print_error(&output.data).await?;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn list_files(&mut self, path: &str) -> FshResult<()> {
        let files = self.client.list_files(path, false).await?;

        for file in files {
            let color = if file.is_directory { Color::Blue } else { Color::White };
            let prefix = if file.is_directory { "d" } else { "-" };

            execute!(
                stdout(),
                SetForegroundColor(color),
                Print(format!("{} {:>10} {}\n", prefix, file.size, file.name)),
                ResetColor
            ).map_err(|e| FshError::NetworkError(format!("Print error: {}", e)))?;
        }

        Ok(())
    }

    async fn show_help(&mut self) -> FshResult<()> {
        let help_text = r#"
FSH Client Commands:

Built-in commands:
  help          - Show this help message
  exit, quit    - Disconnect and exit
  clear         - Clear the screen
  history       - Show command history
  ls, dir       - List files and directories

Remote commands:
  All other commands are executed on the remote folder.
  The available commands depend on the folder configuration.

Navigation:
  ↑/↓           - Navigate command history
  ←/→           - Move cursor in input line
  Tab           - Auto-complete (coming soon)
  Ctrl+C        - Exit
  Ctrl+D        - Exit (if input is empty)

"#;

        self.print_colored(help_text, Color::Cyan).await?;
        Ok(())
    }

    async fn show_history(&mut self) -> FshResult<()> {
        if self.command_history.is_empty() {
            self.print_status("No command history").await?;
            return Ok(());
        }

        for (i, cmd) in self.command_history.iter().enumerate() {
            println!("{:3}: {}", i + 1, cmd);
        }

        Ok(())
    }

    async fn print_status(&self, message: &str) -> FshResult<()> {
        execute!(
            stdout(),
            SetForegroundColor(Color::Yellow),
            Print(format!("[INFO] {}\n", message)),
            ResetColor
        ).map_err(|e| FshError::NetworkError(format!("Print error: {}", e)))?;

        Ok(())
    }

    async fn print_success(&self, message: &str) -> FshResult<()> {
        execute!(
            stdout(),
            SetForegroundColor(Color::Green),
            Print(format!("[SUCCESS] {}\n", message)),
            ResetColor
        ).map_err(|e| FshError::NetworkError(format!("Print error: {}", e)))?;

        Ok(())
    }

    async fn print_error(&self, message: &str) -> FshResult<()> {
        execute!(
            stdout(),
            SetForegroundColor(Color::Red),
            Print(format!("[ERROR] {}\n", message)),
            ResetColor
        ).map_err(|e| FshError::NetworkError(format!("Print error: {}", e)))?;

        Ok(())
    }

    async fn print_colored(&self, message: &str, color: Color) -> FshResult<()> {
        execute!(
            stdout(),
            SetForegroundColor(color),
            Print(message),
            ResetColor
        ).map_err(|e| FshError::NetworkError(format!("Print error: {}", e)))?;

        Ok(())
    }

    fn cleanup_terminal(&self) -> FshResult<()> {
        terminal::disable_raw_mode()
            .map_err(|e| FshError::NetworkError(format!("Failed to disable raw mode: {}", e)))?;

        execute!(stdout(), ResetColor, cursor::Show)
            .map_err(|e| FshError::NetworkError(format!("Terminal cleanup failed: {}", e)))?;

        Ok(())
    }
}

#[derive(Debug)]
enum InputResult {
    Command(String),
    Exit,
    Continue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_creation() {
        let terminal = Terminal::new("127.0.0.1:2222".to_string());
        assert_eq!(terminal.current_prompt, "FSH> ");
        assert_eq!(terminal.current_directory, "/");
        assert!(terminal.command_history.is_empty());
    }
}