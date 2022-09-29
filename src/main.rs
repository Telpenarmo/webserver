use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::{Duration};
use std::{env, io};

use webserver::http::{Request, Response, Status};
use webserver::{match_file_type, parser, UriStatus};
use webserver::{verify_uri, Config};

fn main() {
    let config = Config::new(env::args()).unwrap();

    let addr: SocketAddr = (("localhost", config.port))
        .to_socket_addrs()
        .expect("Invalid IP address")
        .next()
        .unwrap();

    let listener = TcpListener::bind(addr).unwrap();
    println!(
        "Server is listening on {}",
        ["http://localhost", &(config.port.to_string())].join(":")
    );
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        eprintln!("New connection from {}", stream.peer_addr().unwrap());
        handle_connection(stream, &config);
    }
}

fn handle_connection(mut stream: TcpStream, config: &Config) {
    loop {
        let resp = match read_request(&mut stream, config) {
            Ok(request) => handle_request(request, config),
            Err(ReadError::ConnectionClosed) => {
                let peer = stream.peer_addr().unwrap();
                eprintln!("{} closed the connection.", peer);
                return;
            }
            Err(ReadError::Timeout) => {
                let resp = Response::new(Status::RequestTimeout);
                let peer = stream.peer_addr().unwrap();
                eprintln!("Timeout for {}", peer);
                resp
            }
            Err(ReadError::BadSyntax) => {
                Response::new(Status::BadRequest)
            }
            Err(ReadError::TooManyHeaders) => {
                Response::new(Status::BadRequest)
            }
        };
        let resp = resp.render();
        stream
            .write_all(&resp)
            .unwrap_or_else(|err| panic!("writing error: {}", err))
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
    stream.set_read_timeout(Some(Duration::new(2, 0))).unwrap();
    loop {
        match stream.read(&mut read_buf) {
            Ok(0) => {
                break Err(ReadError::ConnectionClosed); // connection closed
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::TimedOut {
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

fn handle_request(request: Request, config: &Config) -> Response {
    let host = match request.headers.get("Host") {
        Some(v) => v,
        None => return Response::new(Status::BadRequest),
    };
    let host = match String::from_utf8(host.to_vec()) {
        Ok(h) => h,
        Err(_) => return Response::new(Status::BadRequest),
    };
    let hostname = host.split_once(':').unwrap().0;
    if request.method.as_str() != "GET" {
        let mut resp = Response::new(Status::MethodNotAllowed);
        resp.set_header("Allow".to_owned(), "GET".to_owned().into_bytes());
        return resp;
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
        UriStatus::Ok(path) => {
            let path = path.as_path();
            let mut file = File::open(path).unwrap_or_else(|err| {
                panic!("file::open: {}", err)
            });
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            response.add_content(buffer);
            response.set_header("Content-Type".into(), match_file_type(path).into());
        }
        UriStatus::NonExistent => {
            response.add_content("<h1>404</h1><p>Requested page was not found.</p>".into())
        }
        UriStatus::OutOfRange => {}
        UriStatus::Directory => {
            let sep = if request.path.ends_with("/") { "" } else { "/" };
            let value = ["http://", &host, &request.path, sep, "index.html"].concat();
            eprintln!("{}", value);
            response.set_header("Location".into(), value.into())
        }
    };

    response
}
