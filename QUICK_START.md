# FSH 快速开始指南

## 直接在终端启动服务器

### 前提条件

1. **安装 Rust**
   ```bash
   # Linux/macOS
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # Windows
   # 下载并安装：https://rustup.rs/
   ```

2. **克隆项目**
   ```bash
   git clone <repository-url>
   cd FSH
   ```

### 方法一：使用启动脚本（推荐）

#### Linux/macOS
```bash
chmod +x start_server.sh
./start_server.sh
```

#### Windows
```cmd
start_server.bat
```

### 方法二：手动启动

#### 1. 构建项目
```bash
cargo build --release
```

#### 2. 生成默认配置
```bash
cargo run --bin fsh-server config --output fsh_config.toml
```

#### 3. 编辑配置文件
编辑 `fsh_config.toml`，添加您的文件夹：

```toml
[[folders]]
name = "我的项目"
path = "C:\\Users\\你的用户名\\Documents\\MyProject"  # Windows路径
# path = "/home/你的用户名/projects/myproject"        # Linux路径
permissions = ["read", "write", "execute"]
shell_type = "powershell"  # Windows: powershell/cmd, Linux: bash
description = "我的开发项目"

[folders.environment_vars]
PROJECT_TYPE = "development"
```

#### 4. 验证配置
```bash
cargo run --bin fsh-server validate
```

#### 5. 启动服务器
```bash
# 方法1: 使用主函数直接启动
cargo run --release

# 方法2: 使用服务器二进制
cargo run --bin fsh-server start

# 方法3: 后台启动
cargo run --bin fsh-server start --foreground
```

## 连接到服务器

### 使用FSH客户端

#### 1. 交互式终端
```bash
# 在另一个终端窗口中
cargo run --bin fsh-client connect --folder "我的项目" --token default
```

#### 2. 执行单个命令
```bash
cargo run --bin fsh-client exec --folder "我的项目" --token default "ls -la"
```

#### 3. 列出文件
```bash
cargo run --bin fsh-client list --folder "我的项目" --token default
```

#### 4. 测试连接
```bash
cargo run --bin fsh-client test
```

### 示例会话

```bash
# 终端1: 启动服务器
$ cargo run --release
[2024-01-01T12:00:00Z INFO  fsh] Starting FSH (Folder Shell Protocol) Server
[2024-01-01T12:00:00Z INFO  fsh] FSH Server Configuration:
[2024-01-01T12:00:00Z INFO  fsh]   Host: 127.0.0.1
[2024-01-01T12:00:00Z INFO  fsh]   Port: 2222
[2024-01-01T12:00:00Z INFO  fsh]   Available folders: 1
[2024-01-01T12:00:00Z INFO  fsh]     - 我的项目 -> C:\Users\用户\Documents\MyProject
[2024-01-01T12:00:00Z INFO  fsh] FSH server starting on 127.0.0.1:2222

# 终端2: 连接客户端
$ cargo run --bin fsh-client connect --folder "我的项目" --token default
[INFO] Connecting to FSH server...
[SUCCESS] Connected to server
[SUCCESS] Authenticated
[SUCCESS] Bound to folder: 我的项目
[SUCCESS] Session ready! Working directory: C:\Users\用户\Documents\MyProject

PS C:\Users\用户\Documents\MyProject> ls
    Directory: C:\Users\用户\Documents\MyProject

Mode                 LastWriteTime         Length Name
----                 -------------         ------ ----
d-----        01/01/2024     12:00                src
-a----        01/01/2024     12:00           1234 README.md
-a----        01/01/2024     12:00            567 package.json

PS C:\Users\用户\Documents\MyProject> git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean

PS C:\Users\用户\Documents\MyProject> exit
[INFO] Goodbye!
```

## 高级配置

### 1. 多文件夹配置
```toml
[[folders]]
name = "Web项目"
path = "/var/www/html"
shell_type = "bash"
readonly = true  # 只读模式

[[folders]]
name = "Rust项目"
path = "/home/user/rust-projects"
shell_type = "bash"
allowed_commands = ["cargo", "rustc", "git", "ls", "cat", "vim"]

[[folders]]
name = "Node.js项目"
path = "C:\\Projects\\NodeApp"
shell_type = "powershell"
[folders.environment_vars]
NODE_ENV = "development"
```

### 2. 安全配置
```toml
[security]
require_authentication = true
auth_methods = ["token"]
max_failed_attempts = 3
enable_logging = true
log_file = "fsh_audit.log"
```

### 3. 服务器配置
```toml
[server]
host = "0.0.0.0"        # 监听所有接口
port = 2222
max_connections = 20
connection_timeout_seconds = 60
session_timeout_minutes = 120
```

## 常用命令

### 服务器管理
```bash
# 生成配置
cargo run --bin fsh-server config --output my_config.toml

# 验证配置
cargo run --bin fsh-server validate

# 添加文件夹
cargo run --bin fsh-server folder add "新项目" /path/to/project --shell bash

# 列出文件夹
cargo run --bin fsh-server folder list

# 移除文件夹
cargo run --bin fsh-server folder remove "项目名"

# 查看文件夹详情
cargo run --bin fsh-server folder show "项目名"

# 启动服务器（指定端口）
cargo run --bin fsh-server start --port 2223
```

### 客户端操作
```bash
# 连接到特定服务器
cargo run --bin fsh-client connect --server "192.168.1.100:2222" --folder "项目名"

# 使用特定shell
cargo run --bin fsh-client connect --folder "项目名" --shell bash

# 执行命令并退出
cargo run --bin fsh-client exec --folder "项目名" "npm install && npm test"

# 列出隐藏文件
cargo run --bin fsh-client list --folder "项目名" --hidden

# 测试远程连接
cargo run --bin fsh-client test --server "remote.example.com:2222"
```

## 故障排除

### 1. 端口冲突
```bash
# 使用不同端口
cargo run --bin fsh-server start --port 2223
```

### 2. 权限问题
```bash
# 检查文件夹权限
ls -la /path/to/folder

# Windows: 以管理员身份运行
```

### 3. 配置问题
```bash
# 验证配置
cargo run --bin fsh-server validate

# 重新生成配置
cargo run --bin fsh-server config --output fsh_config.toml --force
```

### 4. 连接问题
```bash
# 测试网络连接
telnet 127.0.0.1 2222

# 检查防火墙设置
# Windows: Windows Defender防火墙
# Linux: ufw status
```

## 开发和调试

### 启用详细日志
```bash
# 设置日志级别
export RUST_LOG=debug
cargo run --bin fsh-server start

# 或者
RUST_LOG=trace cargo run --bin fsh-client connect --folder "项目名"
```

### 运行测试
```bash
cargo test
```

### 运行示例
```bash
cargo run --example basic_usage
```

## SSH协议兼容性

FSH支持大部分SSH协议特性：

- ✅ 密钥交换和加密
- ✅ 多种认证方法
- ✅ 通道复用
- ✅ PTY分配和终端控制
- ✅ 环境变量传递
- ✅ 信号处理
- ✅ SFTP子系统
- ✅ 端口转发（限制在文件夹内）

### 与标准SSH客户端兼容（计划中）
```bash
# 未来版本将支持
ssh -p 2222 user@localhost:/path/to/folder
```

这样您就可以直接在终端启动FSH服务器，享受类似SSH但更安全的文件夹级别访问体验！