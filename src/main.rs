#![warn(clippy::pedantic)]
use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
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
    let addresses: Vec<_> = hosts.iter().map(|h| h.address).collect();
    let mut senders = Vec::new();
    for host in hosts {
        let (tx, rx) = crossbeam_channel::bounded(1);
        server_state.hosts.insert(host.hostname.clone(), (host, rx));
        senders.push(tx);
    }
    let server_state = &server_state;

    // That's bizarre, so let me describe the mechanism of graceful-shotdown applied here.
    // The problem is that main doesn't have direct access to thread pools, as they are created per host.
    // To workaround this, we use channels, and after receiving termination signal, we push unit
    // to all listener threads.
    // Unfortunately, because listening for connections is being done in non-blocking mode,
    // listeners get termination message on nearest wake-up.
    // So, after sending that message, we initialize connection to listeners by hand
    ctrlc::set_handler(move || {
        info!("Attempting to terminate threads");
        for sender in &senders {
            sender.send(()).expect("Failed to send kill message");
        }
        for addr in &addresses {
            TcpStream::connect(addr).unwrap();
        }
    })
    .expect("Failed to set termination handler");

    thread::scope(|scope| {
        for (host, recv) in server_state.hosts.values() {
            thread::Builder::new()
                .name(format!("webserver: {} listener", host.address))
                .spawn_scoped(scope, || listen(host, recv))
                .expect("Failed to spawn listener thread.");
        }
    });

    info!("Exiting");
}

fn listen(host: &HostState, recv: &crossbeam_channel::Receiver<()>) {
    let span = info_span!("", host = host.hostname);
    let _enter = span.enter();
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
    pool.scoped(|scope| loop {
        if recv.try_recv().is_ok() {
            info!("Closing listener");
            break;
        };
        let stream = listener.accept();
        match stream {
            Ok((stream, peer)) => scope.execute(move || handle_connection(host, stream, peer)),
            Err(err) => error!("connection failed: {err}"),
        }
    });
}

fn handle_connection(host: &HostState, mut stream: TcpStream, peer: SocketAddr) {
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
