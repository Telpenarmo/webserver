use std::collections::HashMap;

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
            .into_iter()
            .map(|header| (header.name.to_owned(), header.value.to_owned()))
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
        Response {
            status,
            headers: HashMap::with_capacity(5),
            content: None,
        }
    }

    pub fn render(mut self) -> Vec<u8> {
        let status_line = format!("HTTP/1.1 {}", self.status.code());
        let status_line = status_line.as_bytes().to_vec();
        let mut lines = Vec::with_capacity(self.headers.len() + 3);
        lines.push(status_line);
        let headers = self.headers.drain().map(Response::render_header);
        lines.extend(headers);
        lines.push("\r\n".into());
        if let Some(content) = self.content {
            lines.push(content);
        }
        lines.join("\r\n".as_bytes())
    }

    fn render_header((name, value): (String, Vec<u8>)) -> Vec<u8> {
        let new_value = unsafe { String::from_utf8_unchecked(value) };
        format!("{}: {}", name, new_value).into_bytes()
    }

    pub fn set_header(&mut self, name: String, value: Vec<u8>) {
        self.headers.insert(name, value);
    }

    pub fn add_content(&mut self, content: Vec<u8>) {
        let length: Vec<u8> = content.len().to_string().into_bytes();
        self.headers.insert("Content-Length".into(), length);
        self.content = Some(content);
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
    NotImplemented,
    HTTPVersionNotSupported,
}

impl Status {
    fn code(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::Moved => 301,
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::RequestTimeout => 408,
            Status::RequestURITooLong => 415,
            Status::NotImplemented => 501,
            Status::HTTPVersionNotSupported => 505,
        }
    }
}
