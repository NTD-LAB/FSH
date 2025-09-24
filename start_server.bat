@echo off
title FSH Server

echo =========================================
echo FSH - Folder Shell Protocol Server
echo =========================================

REM 检查Rust是否安装
where cargo >nul 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Cargo/Rust not found. Please install Rust first:
    echo    https://rustup.rs/
    pause
    exit /b 1
)

REM 检查项目目录
if not exist "Cargo.toml" (
    echo ❌ Not in FSH project directory
    pause
    exit /b 1
)

echo ✅ Rust environment found

REM 构建项目
echo 🔨 Building FSH server...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Build failed
    pause
    exit /b 1
)

echo ✅ Build successful

REM 检查配置文件
if not exist "fsh_config.toml" (
    echo 📝 Creating default configuration...
    cargo run --bin fsh-server config --output fsh_config.toml
    echo ✅ Default configuration created: fsh_config.toml
    echo 💡 Please edit fsh_config.toml to add your folders before starting the server
    pause
    exit /b 0
)

echo ✅ Configuration file found: fsh_config.toml

REM 验证配置
echo 🔍 Validating configuration...
cargo run --bin fsh-server validate
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Configuration validation failed
    echo 💡 Please check your fsh_config.toml file
    pause
    exit /b 1
)

echo ✅ Configuration is valid

REM 启动服务器
echo 🚀 Starting FSH server...
echo 📋 Server will start with the following configuration:

REM 显示配置摘要
findstr /r "^host ^port" fsh_config.toml

echo.
echo 🔗 To connect from another terminal:
echo    cargo run --bin fsh-client connect --folder "Your Folder Name"
echo.
echo 🛑 Press Ctrl+C to stop the server
echo.

REM 启动服务器 (使用主函数)
cargo run --release

echo 👋 FSH server stopped
pause