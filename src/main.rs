use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use std::{env, io, thread};

use scoped_threadpool::Pool;

use webserver::http::{Request, Response, Status};
use webserver::*;
use webserver::{Config, DomainHandler, ServerState};

fn main() -> Result<(), String> {
    let config = Config::new(env::args())?;
    let hosts = HashMap::new();
    let mut server_state = ServerState { config, hosts };
    let hosts = get_hosts(&server_state.config)?;
    for host in hosts {
        server_state.hosts.insert(host.hostname.clone(), host);
    }
    let server_state = &server_state;

    thread::scope(|scope| {
        for host in server_state.hosts.values() {
            thread::Builder::new()
                .name(format!("webserver: {} listener", host.address))
                .spawn_scoped(scope, || listen(host))
                .expect("Failed to spawn listener thread.");
        }
    });

    Ok(())
}

fn listen(host: &HostState) {
    let listener = TcpListener::bind(host.address).expect("Failed to bind an address.");
    println!(
        "Server is listening on http://{}:{} (http://{})",
        host.hostname, host.config.port, host.address
    );

    let mut pool = Pool::new(host.config.threads_per_connection.into());
    pool.scoped(|scope| {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => scope.execute(|| handle_connection(host, stream)),
                Err(err) => eprintln!("connection failed: {}", err),
            }
        }
    });
}

fn handle_connection(host: &HostState, mut stream: TcpStream) {
    let peer = match stream.peer_addr() {
        Ok(addr) => addr,
        Err(err) => {
            eprintln!("Error checking peer address: {}", err);
            return;
        }
    };
    eprintln!("New connection from {}", peer);

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
                eprintln!("Timeout for {}", peer);
                close_connection = true;
                Some(resp)
            }
            Err(ReadError::BadSyntax) => Some(Response::new(Status::BadRequest)),
            Err(ReadError::TooManyHeaders) => Some(Response::new(Status::BadRequest)),
        };
        if let Some(mut response) = response {
            write_connection_header(close_connection, &mut response);

            let response = response.render();
            stream
                .write_all(&response)
                .unwrap_or_else(|err| eprintln!("Error writing response: {}", err));

            stream
                .flush()
                .unwrap_or_else(|err| eprintln!("Error flushing response: {}", err))
        }
        if close_connection {
            eprintln!("{} closed the connection.", peer);
            return;
        }
    }
}

fn write_connection_header(close: bool, response: &mut Response) {
    let connection_header = if close { "close" } else { "keep-alive" };
    response.set_header("Connection".into(), connection_header.into());
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
                let content_length = req
                    .headers
                    .get("Content-Length")
                    .map(|v| match String::from_utf8(v.to_owned()) {
                        Ok(s) => match s.parse() {
                            Ok(d) => Ok(d),
                            Err(_) => Err(ReadError::BadSyntax),
                        },
                        Err(_) => Err(ReadError::BadSyntax),
                    })
                    .unwrap_or(Ok(0));
                let _content_length: u32 = match content_length {
                    Ok(len) => len,
                    Err(err) => break Some(Err(err)),
                };
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
    let mut close = request
        .headers
        .get("close")
        .map_or(false, |v| v.eq("close".as_bytes()));

    let response = match &host_data.handler {
        DomainHandler::StaticDir(dir) => static_server::handle_request(request, host_data, dir),
        DomainHandler::Executable(_) => {
            close = true;
            Response::with_content(
                Status::NotImplemented,
                "Dynamic http servers not yet supported".into(),
            )
        }
    };

    (response, close)
}
