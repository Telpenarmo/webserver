pub mod http;
pub mod parser;
pub mod utils;

use std::fs::{canonicalize, read_dir};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::{env, io, panic};

use http::Status;

pub struct Config {
    pub directory: String,
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
            Some(arg) => arg,
        };
        Ok(Config {
            directory,
            max_headers_number: 512,
            port,
            keep_alive: 20,
        })
    }
}

pub enum UriStatus {
    Ok(PathBuf),
    NonExistent,
    OutOfRange,
    Directory,
}

pub fn verify_uri(dir: &str, domain: &str, uri: &str) -> UriStatus {
    let rel_dir_path = [dir, domain].join("/");
    let rel_res_path = rel_dir_path.clone() + uri;
    eprintln!("requested resource: {}", rel_res_path);
    let dir_path = match canonicalize(rel_dir_path) {
        Ok(path) => path,
        Err(err) => return UriStatus::NonExistent,
    };
    let res_path = match canonicalize(rel_res_path) {
        Ok(v) => v,
        Err(err) => {
            return match err.kind() {
                io::ErrorKind::NotFound => UriStatus::NonExistent, // 404
                // io::ErrorKind::FilenameTooLong => None,
                _ => panic!("canonicalize: {}", err),
            };
        }
    };

    if !res_path.starts_with(dir_path) {
        return UriStatus::OutOfRange; // 403
    }
    if res_path.is_dir() {
        return UriStatus::Directory; // 301
    }
    UriStatus::Ok(res_path)
}

pub fn get_addrs(config: &Config) -> Vec<SocketAddr> {
    let mut addrs = Vec::new();
    for hostname in get_hostnames(&config.directory) {
        let addr: SocketAddr = ((hostname, config.port))
            .to_socket_addrs()
            .expect("Invalid IP address")
            .next()
            .unwrap();
        addrs.push(addr);
    }
    addrs
}

fn get_hostnames(root: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for entry in read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            let sub_dir = entry.file_name().into_string().unwrap();
            eprintln!("host: {}", sub_dir);
            hosts.push(sub_dir);
        }
    }
    hosts
}

pub fn get_error_page(status: &Status, config: &Config) -> Option<PathBuf> {
    let file_path = status.code().to_string() + ".html";
    let file = [config.directory.clone(), file_path].join("/");
    let path = PathBuf::from(file);
    if path.try_exists().unwrap() {
        Some(path)
    } else {
        None
    }
}
