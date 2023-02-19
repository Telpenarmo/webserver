pub mod http;
pub mod parser;
pub mod static_server;
pub mod utils;

use std::collections::HashMap;
use std::env;
use std::fs::{canonicalize, read_dir, File};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use http::Status;

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
    StaticDir(PathBuf),
    Executable(File),
}

pub struct Config {
    pub directory: PathBuf,
    pub max_headers_number: usize,
    pub port: u16,
    pub keep_alive: u8,
}

impl Config {
    pub fn new(mut args: env::Args) -> Result<Config, String> {
        let usage = format!("Usage: {} port directory", args.next().unwrap());

        let port = match args.next() {
            None => return Err(usage),
            Some(arg) => match arg.parse() {
                Ok(p) => p,
                Err(err) => return Err(format!("Error parsing port: {}", err)),
            },
        };
        let directory = match args.next() {
            None => return Err(usage),
            Some(arg) => {
                let path = PathBuf::from(arg);
                match canonicalize(path) {
                    Ok(path) => path,
                    Err(err) => return Err(format!("Invalid directory: {}", err)),
                }
            }
        };
        Ok(Config {
            directory,
            max_headers_number: 512,
            port,
            keep_alive: 20,
        })
    }
}

pub fn get_hosts(config: &Config) -> Result<Vec<HostState>, String> {
    let mut hostnames = get_hostnames(&config.directory)?;
    let hosts = hostnames.drain(..).map(|(dir, hostname)| {
        let address: SocketAddr = (hostname.clone(), config.port)
            .to_socket_addrs()
            .expect("Invalid IP address")
            .next()
            .unwrap();
        HostState {
            handler: DomainHandler::StaticDir(dir),
            config,
            address,
            hostname,
        }
    });
    Ok(hosts.collect())
}

fn get_hostnames(root: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut hosts = Vec::new();
    let read_dir = match read_dir(root) {
        Ok(dir) => dir,
        Err(err) => return Err(format!("Error accessing directory: {}", err)),
    };

    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if path.is_dir() {
            let Ok(sub_dir) = entry.file_name().into_string() else {
                eprintln!("Non-Unicode file_name; ignoring.");
                continue;
            };
            let Ok(path) = path.canonicalize() else {
                eprintln!("Error accessing {} subdirectory; ignoring.", sub_dir);
                continue;
            };
            eprintln!("host: {}", sub_dir);
            hosts.push((path, sub_dir));
        }
    }
    Ok(hosts)
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
