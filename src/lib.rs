pub mod http;
pub mod logging;
pub mod reader;
pub mod static_server;
pub mod utils;

use std::collections::HashMap;
use std::fs::{canonicalize, read_dir, File};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use clap::Parser;
use http::Status;
use tracing::warn;

pub struct ServerState<'a> {
    pub config: Config,
    pub hosts: HashMap<String, HostState<'a>>,
}

pub struct HostState<'a> {
    pub handler: DomainHandler,
    pub config: &'a Config,
    pub address: SocketAddr,
    pub hostname: String,
}

pub enum DomainHandler {
    StaticDir(static_server::Data),
    Executable(File),
}

/// Simple, near-minimal static HTTP server.
///
/// Detailed notes on usage are included in the README.
#[derive(Parser)]
pub struct Config {
    /// Path to directory containg content to be hosted
    #[arg(value_parser = Config::verify_dir)]
    pub directory: PathBuf,

    /// Port under which content is served.
    #[arg(short, long)]
    pub port: u16,

    /// How long to keep TCP connection active, in seconds
    #[arg(long, default_value_t = 2)]
    pub keep_alive: u8,

    /// Maximal number of headers included in a request
    #[arg(long, default_value_t = 512)]
    pub max_headers_number: usize,

    /// How many concurrent requests can one host handle
    #[arg(long, default_value_t = 4)]
    pub threads_per_connection: u8,
}

impl Config {
    fn verify_dir(dir: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(dir);
        match canonicalize(path) {
            Ok(path) => match path.read_dir() {
                Ok(_) => Ok(path),
                Err(err) => Err(format!("Directory inaccessible: {}", err)),
            },
            Err(err) => Err(format!("Invalid directory: {}", err)),
        }
    }
}

pub fn get_hosts(config: &Config) -> Vec<HostState> {
    let mut hostnames = get_hostnames(&config.directory);
    let hosts = hostnames.drain(..).map(|(dir, hostname)| {
        let address: SocketAddr = (hostname.clone(), config.port)
            .to_socket_addrs()
            .map_err(|_err| warn!("Invalid IP address for host {}; ignoring", hostname))
            .ok()?
            .next()
            .unwrap();
        let server_data = static_server::Data::new(dir);
        Some(HostState {
            handler: DomainHandler::StaticDir(server_data),
            config,
            address,
            hostname,
        })
    });
    hosts.flatten().collect()
}

fn get_hostnames(root: &Path) -> Vec<(PathBuf, String)> {
    let mut hosts = Vec::new();
    let read_dir = read_dir(root).expect("Error accessing directory");

    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if path.is_dir() {
            let Ok(sub_dir) = entry.file_name().into_string() else {
                warn!("Non-Unicode file_name; ignoring.");
                continue;
            };
            let Ok(path) = path.canonicalize() else {
                warn!("Error accessing {} subdirectory; ignoring.", sub_dir);
                continue;
            };
            hosts.push((path, sub_dir));
        }
    }
    hosts
}

pub fn get_error_page(status: &Status, config: &Config) -> Option<PathBuf> {
    let file_path = status.code().to_string() + ".html";
    let file_path = PathBuf::from(file_path);

    let mut path = config.directory.clone();
    path.push(file_path);

    if path.exists() {
        Some(path)
    } else {
        None
    }
}
