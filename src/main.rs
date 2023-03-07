#![warn(clippy::pedantic)]
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

use clap::Parser;
use scoped_threadpool::Pool;
use tracing::{error, info, info_span, warn};

use webserver::http::{Request, Response, Status};
use webserver::reader::{read_request, ReadError};
use webserver::{get_hosts, logging, static_server, HostState};
use webserver::{Config, DomainHandler, ServerState};

fn main() {
    logging::init();

    let config = Config::parse();
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
                .spawn_scoped(scope, || listen(host))
                .expect("Failed to spawn listener thread.");
        }
    });

    info!("Exiting");
}

fn listen(host: &HostState) {
    let listener = match TcpListener::bind(host.address) {
        Ok(listener) => listener,
        Err(err) => {
            warn!("Failed to bind an address ({}): {err}.", host.address);
            return;
        }
    };
    println!(
        "Server is listening on http://{}:{} (http://{})\n",
        host.hostname, host.config.port, host.address
    );

    let mut pool = Pool::new(host.config.threads_per_connection.into());
    pool.scoped(|scope| {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => scope.execute(|| handle_connection(host, stream)),
                Err(err) => error!("connection failed: {err}"),
            }
        }
    });
    info!("Closing listener");
}

fn handle_connection(host: &HostState, mut stream: TcpStream) {
    let peer = match stream.peer_addr() {
        Ok(addr) => addr,
        Err(err) => {
            error!("Error checking peer address: {err}");
            return;
        }
    };
    let span = info_span!("connection", peer = peer.to_string());
    let _enter = span.enter();

    info!("Connected");
    
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
                close_connection = true;
                Some(resp)
            }
            Err(ReadError::BadSyntax | ReadError::TooManyHeaders) => {
                Some(Response::new(Status::BadRequest))
            }
        };
        if let Some(mut response) = response {
            write_connection_header(close_connection, &mut response);

            info!(response = response.status_line(), "Responded");
            let response = response.render();
            stream
                .write_all(&response)
                .unwrap_or_else(|err| error!("Error writing response: {err}"));

            stream
                .flush()
                .unwrap_or_else(|err| error!("Error flushing response: {err}"));
        }
        if close_connection {
            info!("Disconnected");
            return;
        }
    }
}

fn write_connection_header(close: bool, response: &mut Response) {
    let connection_header = if close { "close" } else { "keep-alive" };
    response.set_header("Connection", connection_header);
}

fn handle_request(host_data: &HostState, request: Request) -> (Response, bool) {
    let target = format!("{} {}", request.method, request.path);
    let span = info_span!("request", target);
    let _enter = span.enter();

    info!("Request received");

    let mut close = request
        .headers
        .get("close")
        .map_or(false, |v| v.eq("close".as_bytes()));

    let response = match &host_data.handler {
        DomainHandler::StaticDir(data) => static_server::handle_request(request, host_data, data),
        DomainHandler::Executable(_) => {
            close = true;
            Response::with_content(
                Status::NotImplemented,
                "Dynamic http servers not yet supported",
            )
        }
    };

    (response, close)
}
