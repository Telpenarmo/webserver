use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::{collections::HashMap, fmt::Display};
use tracing::{debug, error};

use crate::utils::match_file_type;

pub struct Request {
    pub method: String,
    pub path: String,
    pub version: u8,
    pub headers: HashMap<String, Vec<u8>>,
}

impl Request {
    pub fn new(req: httparse::Request) -> Request {
        let headers: HashMap<_, _> = req
            .headers
            .iter_mut()
            .map(|header| (header.name.into(), header.value.into()))
            .collect();
        Request {
            method: req.method.unwrap().to_owned(),
            path: req.path.unwrap().to_owned(),
            version: req.version.unwrap().to_owned(),
            headers,
        }
    }
}

pub struct Response {
    status: Status,
    headers: HashMap<String, Vec<u8>>,
    content: Option<Vec<u8>>,
}

impl Response {
    pub fn new(status: Status) -> Response {
        let mut headers = HashMap::with_capacity(5);
        headers.insert("Server".into(), "Telpenarmo's webserver".into());
        Response {
            status,
            headers,
            content: None,
        }
    }

    pub fn with_content<C>(status: Status, content: C) -> Response
    where
        C: Into<Vec<u8>>,
    {
        let mut resp = Response::new(status);
        let mut content: Vec<u8> = content.into();
        content.push(b'\n');
        resp.add_content(content);
        resp
    }

    pub fn render(mut self) -> Vec<u8> {
        let status_line = self.status_line();
        let mut lines = Vec::with_capacity(self.headers.len() + 3);
        lines.push(status_line.into());
        let headers = self.headers.drain().map(Response::render_header);
        lines.extend(headers);
        lines.push(vec![]);
        if let Some(content) = self.content {
            lines.push(content);
        }
        lines.join("\r\n".as_bytes())
    }

    pub fn status_line(&self) -> String {
        format!("HTTP/1.1 {}", self.status.code())
    }

    fn render_header((name, value): (String, Vec<u8>)) -> Vec<u8> {
        let new_value = unsafe { String::from_utf8_unchecked(value) };
        format!("{}: {}", name, new_value).into()
    }

    pub fn set_header<H, V>(&mut self, name: H, value: V)
    where
        H: Into<String>,
        V: Into<Vec<u8>>,
    {
        self.headers.insert(name.into(), value.into());
    }

    pub fn add_content<C>(&mut self, content: C)
    where
        C: Into<Vec<u8>>,
    {
        let content = content.into();
        let length = content.len().to_string();
        self.headers.insert("Content-Length".into(), length.into());
        self.content = Some(content);
    }

    pub fn load_file(mut self, path: &Path) -> Response {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(err) => {
                return server_error(format!("Error on opening file {}: {}", path.display(), err))
            }
        };
        let mut buffer = Vec::new();
        match file.read_to_end(&mut buffer) {
            Ok(_) => (),
            Err(err) => {
                return server_error(format!("Error on reading file {}: {}", path.display(), err))
            }
        };
        self.add_content(buffer);
        self.set_header("Content-Type", match_file_type(path));
        debug!("File {} loaded", path.display());
        self
    }

    pub fn to_head(mut self) -> Response {
        self.content = None;
        self
    }
}

#[derive(Clone, Copy)]
pub enum Status {
    Ok,
    Moved,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    RequestTimeout,
    RequestURITooLong,
    InternalServerError,
    NotImplemented,
    HTTPVersionNotSupported,
}

impl Status {
    pub fn code(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::Moved => 301,
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::RequestTimeout => 408,
            Status::RequestURITooLong => 415,
            Status::InternalServerError => 500,
            Status::NotImplemented => 501,
            Status::HTTPVersionNotSupported => 505,
        }
    }
}

pub fn server_error<M>(msg: M) -> Response
where
    M: Display,
{
    error!("server error: {}", msg);
    Response::with_content(Status::InternalServerError, "Internal server error.")
}
