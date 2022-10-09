use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{env, io, thread};

use webserver::http::{Request, Response, Status};
use webserver::{get_addrs, match_file_type, parser, UriStatus};
use webserver::{verify_uri, Config};

fn main() {
    let config = Config::new(env::args()).unwrap();

    let addrs = get_addrs(&config);

    let mut handlers = Vec::new();
    for addr in addrs {
        let handler = spawn_listener(addr);
        handlers.push(handler);
    }
    for handler in handlers {
        handler.join().unwrap();
    }
}

fn spawn_listener(addr: SocketAddr) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("webserver: {} listener", addr))
        .spawn(move || {
            let config = Config::new(env::args()).unwrap();

            let listener = TcpListener::bind(addr).unwrap();
            println!("Server is listening on {}", addr);

            for stream in listener.incoming() {
                let stream = stream.unwrap();
                handle_connection(stream, &config);
            }
        })
        .unwrap()
}

fn handle_connection(mut stream: TcpStream, config: &Config) {
    eprintln!("New connection from {}", stream.peer_addr().unwrap());

    loop {
        let mut close_connection = false;
        let response = match read_request(&mut stream, config) {
            Ok(request) => {
                let (response, close) = handle_request(request, config);
                close_connection = close;
                Some(response)
            }
            Err(ReadError::ConnectionClosed) => {
                close_connection = true;
                None
            }
            Err(ReadError::Timeout) => {
                let resp = Response::new(Status::RequestTimeout);
                let peer = stream.peer_addr().unwrap();
                eprintln!("Timeout for {}", peer);
                close_connection = true;
                Some(resp)
            }
            Err(ReadError::BadSyntax) => Some(Response::new(Status::BadRequest)),
            Err(ReadError::TooManyHeaders) => Some(Response::new(Status::BadRequest)),
        };
        if let Some(mut response) = response {
            let connection_header = match close_connection {
                true => "close",
                false => "keep-alive",
            };
            response.set_header("Connection".into(), connection_header.into());

            let response = response.render();
            stream
                .write_all(&response)
                .unwrap_or_else(|err| panic!("writing error: {}", err));
        }
        if close_connection {
            let peer = stream.peer_addr().unwrap();
            eprintln!("{} closed the connection.", peer);
            return;
        }
    }
}

enum ReadError {
    ConnectionClosed,
    Timeout,
    BadSyntax,
    TooManyHeaders,
}

fn get_res_or_partial(
    buffer: &mut Vec<u8>,
    max_headers_count: usize,
) -> Option<Result<Request, ReadError>> {
    let mut headers_size = 16;
    loop {
        match parser::try_parse(headers_size, buffer) {
            Err(parser::Error::Partial) => break None,
            Err(parser::Error::TooManyHeaders) => {
                if headers_size < max_headers_count {
                    headers_size = usize::min(2 * headers_size, max_headers_count);
                } else {
                    break Some(Err(ReadError::TooManyHeaders)); // 400
                }
            }
            Err(parser::Error::Syntax) => break Some(Err(ReadError::BadSyntax)), // 400
            Ok((req, _s)) => {
                let _len: u32 = req
                    .headers
                    .get("Content-Length")
                    .map(|v| match String::from_utf8(v.to_owned()) {
                        Ok(s) => s.parse().unwrap(),
                        Err(_) => panic!(""),
                    })
                    .unwrap_or_default();
                break Some(Ok(req));
            }
        }
    }
}

fn read_request(stream: &mut TcpStream, config: &Config) -> Result<Request, ReadError> {
    let mut read_buf = [0; 1024];
    let mut buffer = Vec::with_capacity(1024);
    stream
        .set_read_timeout(Some(Duration::new(config.keep_alive.into(), 0)))
        .unwrap();
    loop {
        match stream.read(&mut read_buf) {
            Ok(0) => {
                break Err(ReadError::ConnectionClosed); // connection closed
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::TimedOut || err.kind() == io::ErrorKind::WouldBlock
                {
                    break Err(ReadError::Timeout);
                } // 408
                eprintln!("err: {}", err.kind());
            }
            Ok(bytes_read) => {
                buffer.extend_from_slice(&read_buf[..bytes_read]);
                match get_res_or_partial(&mut buffer, config.max_headers_number) {
                    None => continue,
                    Some(res) => break res,
                }
            }
        }
    }
}

fn handle_request(request: Request, config: &Config) -> (Response, bool) {
    let mut bad_request = (Response::new(Status::BadRequest), false);
    let host = match request.headers.get("Host") {
        Some(v) => v,
        None => {
            bad_request.0.add_content("Host header is required".into());
            return bad_request;
        }
    };
    let host = match String::from_utf8(host.to_vec()) {
        Ok(h) => h,
        Err(err) => {
            bad_request
                .0
                .add_content(format!("Utf-8 error: {}", err.utf8_error()).into());
            return bad_request;
        }
    };
    let hostname = host.split_once(':').unwrap().0;
    if request.method.as_str() != "GET" {
        let mut resp = Response::new(Status::MethodNotAllowed);
        resp.set_header("Allow".to_owned(), "GET".to_owned().into_bytes());
        return (resp, false);
    }

    let uri_status = verify_uri(&config.directory, &hostname, &request.path);

    let status = match uri_status {
        UriStatus::Ok(_) => Status::Ok,
        UriStatus::NonExistent => Status::NotFound,
        UriStatus::OutOfRange => Status::Forbidden,
        UriStatus::Directory => Status::Moved,
    };

    let mut response = Response::new(status);

    match uri_status {
        UriStatus::Ok(path_buf) => {
            let path = path_buf.as_path();
            let mut file = File::open(path).unwrap_or_else(|err| panic!("file::open: {}", err));
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            response.add_content(buffer);
            response.set_header("Content-Type".into(), match_file_type(path).into());
        }
        UriStatus::NonExistent => {
            response.add_content("<h1>404</h1><p>Requested page was not found.</p>".into())
        }
        UriStatus::OutOfRange => {
            response.add_content("<h1>404</h1><p>Requested resource cannot be accessed.</p>".into())
        }
        UriStatus::Directory => {
            let sep = if request.path.ends_with("/") { "" } else { "/" };
            let value = ["http://", &host, &request.path, sep, "index.html"].concat();
            eprintln!("{}", value);
            response.set_header("Location".into(), value.into())
        }
    };

    let close = request
        .headers
        .get("close")
        .map_or(false, |v| v.eq("close".as_bytes()));

    (response, close)
}
