use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub lan_mode: bool,
    pub api_key: Option<String>,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 11434,
            lan_mode: false,
            api_key: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl ServerConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        if self.lan_mode {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.port)
        } else {
            SocketAddr::new(self.host, self.port)
        }
    }

    pub fn tls_enabled(&self) -> bool {
        self.tls_cert_path.is_some() && self.tls_key_path.is_some()
    }
}
