use std::path::PathBuf;

use http_server::{run_server, ServerConfig};
use runtime_core::RuntimeConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config_path = std::env::var("MAI_CONFIG").ok().map(PathBuf::from);
    let mut runtime_config = RuntimeConfig::load(config_path.as_deref())?;

    if std::env::var("MAI_AFRICA_MODE").ok().as_deref() == Some("1") {
        runtime_config.africa_mode = true;
    }

    let port = std::env::var("MAI_HTTP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(11434);
    let lan_mode = std::env::var("MAI_HTTP_LAN").ok().as_deref() == Some("1");
    let tls_cert_path = std::env::var("MAI_HTTP_TLS_CERT").ok().map(PathBuf::from);
    let tls_key_path = std::env::var("MAI_HTTP_TLS_KEY").ok().map(PathBuf::from);

    let server_config = ServerConfig {
        port,
        lan_mode,
        api_key: std::env::var("MAI_API_KEY").ok(),
        tls_cert_path,
        tls_key_path,
        ..Default::default()
    };

    run_server(server_config, runtime_config).await?;
    Ok(())
}
