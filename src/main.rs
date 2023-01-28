use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use std::{env, io, thread};

use webserver::http::{Request, Response, Status};
use webserver::*;
use webserver::{Config, DomainHandler, ServerState};

fn main() {
    let config = Config::new(env::args()).unwrap();
    let hosts = HashMap::new();
    let mut server_state = ServerState { config, hosts };
    let hosts = get_hosts(&server_state.config);
    for host in hosts {
        server_state.hosts.insert(host.hostname.clone(), host);
    }
    let server_state = &server_state;

    thread::scope(|scope| {
        for host in server_state.hosts.values() {
            thread::Builder::new()
                .name(format!("webserver: {} listener", host.address))
                .spawn_scoped(scope, move || listen(host))
                .unwrap();
        }
    });
}

fn listen(host: &HostState) {
    let listener = TcpListener::bind(host.address).unwrap();
    println!("Server is listening on {}", host.address);

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        handle_connection(host, stream);
    }
}

fn handle_connection(host: &HostState, mut stream: TcpStream) {
    eprintln!("New connection from {}", stream.peer_addr().unwrap());

    loop {
        let mut close_connection = false;
        let response = match read_request(&mut stream, host.config) {
            Ok(request) => {
                let (response, close) = handle_request(host, request);
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
    buffer: &mut [u8],
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

fn handle_request(host_data: &HostState, request: Request) -> (Response, bool) {
    let close = request
        .headers
        .get("close")
        .map_or(false, |v| v.eq("close".as_bytes()));

    let response = match &host_data.handler {
        DomainHandler::StaticDir(dir) => static_server::handle_request(request, host_data, dir),
        DomainHandler::Executable(_) => panic!("dynamic http servers not yet supported"),
    };

    (response, close)
}
