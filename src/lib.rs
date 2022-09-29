pub mod http;
pub mod parser;

use std::ffi::OsStr;
use std::fs::canonicalize;
use std::path::{Path, PathBuf};
use std::{env, io, panic};

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
    eprintln!("{}", rel_res_path);
    let dir_path = canonicalize(rel_dir_path).unwrap();
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

pub fn match_file_type(filename: &Path) -> &str {
    match filename.extension().and_then(OsStr::to_str) {
        Some("txt") => "text/plain, charset=utf-8",
        Some("html") => "text/html, charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("jpg") => "image/jpeg",
        Some("jpeg") => "image/jpeg",
        Some("png") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}
