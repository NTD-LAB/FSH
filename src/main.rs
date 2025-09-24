use fsh::{config::Config, server::FshServer};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fsh=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting FSH (Folder Shell Protocol) Server");

    // 加载或创建默认配置
    let config_path = "fsh_config.toml";
    let config = match Config::load_or_create_default(config_path) {
        Ok(config) => {
            info!("Configuration loaded from {}", config_path);
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            info!("Using default configuration");
            Config::default()
        }
    };

    // 显示服务器信息
    info!("FSH Server Configuration:");
    info!("  Host: {}", config.server.host);
    info!("  Port: {}", config.server.port);
    info!("  Max connections: {}", config.server.max_connections);
    info!("  Authentication required: {}", config.security.require_authentication);
    info!("  Available folders: {}", config.folders.len());

    for folder in &config.folders {
        info!("    - {} -> {}", folder.name, folder.path);
    }

    // 创建并启动服务器
    let mut server = match FshServer::new(config) {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            std::process::exit(1);
        }
    };

    info!("FSH server starting on {}:{}", server.config().server.host, server.config().server.port);
    info!("Press Ctrl+C to stop the server");

    // 优雅关闭处理
    tokio::select! {
        result = server.start() => {
            match result {
                Ok(_) => info!("FSH server stopped normally"),
                Err(e) => error!("FSH server error: {}", e),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully...");
            if let Err(e) = server.stop().await {
                error!("Error during shutdown: {}", e);
            } else {
                info!("FSH server stopped successfully");
            }
        }
    }
}