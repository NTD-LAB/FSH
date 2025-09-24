#!/bin/bash

# FSH服务器启动脚本
# 用于直接在终端启动FSH服务器

echo "========================================="
echo "FSH - Folder Shell Protocol Server"
echo "========================================="

# 检查Rust是否安装
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo/Rust not found. Please install Rust first:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# 检查项目目录
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Not in FSH project directory"
    exit 1
fi

echo "✅ Rust environment found"

# 构建项目
echo "🔨 Building FSH server..."
if ! cargo build --release; then
    echo "❌ Build failed"
    exit 1
fi

echo "✅ Build successful"

# 检查配置文件
if [ ! -f "fsh_config.toml" ]; then
    echo "📝 Creating default configuration..."
    cargo run --bin fsh-server config --output fsh_config.toml
    echo "✅ Default configuration created: fsh_config.toml"
    echo "💡 Please edit fsh_config.toml to add your folders before starting the server"
    exit 0
fi

echo "✅ Configuration file found: fsh_config.toml"

# 验证配置
echo "🔍 Validating configuration..."
if ! cargo run --bin fsh-server validate; then
    echo "❌ Configuration validation failed"
    echo "💡 Please check your fsh_config.toml file"
    exit 1
fi

echo "✅ Configuration is valid"

# 启动服务器
echo "🚀 Starting FSH server..."
echo "📋 Server will start with the following configuration:"

# 显示配置摘要
grep -E "^host|^port" fsh_config.toml | head -2

echo ""
echo "🔗 To connect from another terminal:"
echo "   cargo run --bin fsh-client connect --folder \"Your Folder Name\""
echo ""
echo "🛑 Press Ctrl+C to stop the server"
echo ""

# 启动服务器 (使用主函数)
cargo run --release

echo "👋 FSH server stopped"