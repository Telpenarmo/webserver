use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use tracing::info;

use crate::{http::*, utils::path_if_existing, Config, HostData};

pub struct Data<'a> {
    content_dir: PathBuf,
    handlers: HashMap<String, MethodHandler>,
    config: &'a Config,
    address: SocketAddr,
    hostname: String,
}

impl HostData<'_> for Data<'_> {
    fn get_config(&self) -> &Config {
        self.config
    }

    fn get_address(&self) -> &SocketAddr {
        &self.address
    }

    fn get_hostname(&self) -> &String {
        &self.hostname
    }
}

impl<'a> Data<'a> {
    pub fn new(
        content_dir: PathBuf,
        config: &'a Config,
        address: SocketAddr,
        hostname: String,
    ) -> Data {
        Data {
            content_dir,
            handlers: get_handlers(),
            config,
            address,
            hostname,
        }
    }
}

type MethodHandler = Box<dyn Fn(&Data, &Request) -> Response + Sync>;

pub fn handle_request(request: Request, data: &Data) -> Response {
    let Some(handler) = data.handlers.get(&request.method) else {
            let mut resp = Response::new(Status::MethodNotAllowed);
            let allowed_methods = data.handlers.keys().map(|s| &**s).collect::<Vec<_>>().join(", ");
            resp.set_header("Allow", allowed_methods);
            return resp;
        };

    handler(data, &request)
}

fn get_relative_resource_path(content_dir: &Path, request: &Request) -> PathBuf {
    let mut rel_res_path = content_dir.to_path_buf();
    let mut path = request.path.to_string();
    path.remove(0);
    rel_res_path.push(&path);
    rel_res_path
}

fn get_handlers() -> HashMap<String, MethodHandler> {
    let mut handlers: HashMap<String, MethodHandler> = HashMap::new();
    handlers.insert("GET".into(), Box::new(handle_get_request));
    handlers.insert("HEAD".into(), Box::new(handle_head_request));
    handlers
}

fn handle_get_request(data: &Data, request: &Request) -> Response {
    let rel_res_path = get_relative_resource_path(&data.content_dir, request);
    let res_path = match std::fs::canonicalize(rel_res_path) {
        Ok(path) => path,
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => return load_error(Status::NotFound, data),
            io::ErrorKind::PermissionDenied => {
                return load_error(Status::Forbidden, data);
            }
            _ => return server_error(err.to_string()),
        },
    };

    match res_path.strip_prefix(&data.content_dir) {
        Ok(rel_res_path) => {
            if res_path.is_dir() {
                return redirect_dir(rel_res_path, data);
            }
            let resp = Response::new(Status::Ok);
            resp.load_file(&res_path)
        }
        Err(_) => load_error(Status::Forbidden, data),
    }
}

fn handle_head_request(data: &Data, request: &Request) -> Response {
    let get_response = handle_get_request(data, request);
    get_response.to_head()
}

fn redirect_dir(path: &Path, data: &Data) -> Response {
    info!("Redirecting");

    let mut resp = Response::new(Status::Moved);
    let Some(path) = path.to_str() else {
        return load_error(Status::BadRequest, data);
    };
    let index_location = format!(
        "http://{}:{}{}/index.html",
        data.hostname, data.config.port, path
    );
    resp.set_header("Location", index_location);
    resp
}

fn load_error(status: Status, data: &Data) -> Response {
    info!("loading error");
    let mut response = Response::new(status);
    let error_file = get_error_page(&status, data);
    if let Some(path) = error_file {
        response.load_file(path.as_path())
    } else {
        response.add_content(format!("Error: {}", status.code()));
        response
    }
}

pub fn get_error_page(status: &Status, data: &Data) -> Option<PathBuf> {
    let file_name = status.code().to_string() + ".html";
    let file_name = PathBuf::from(file_name);

    let local_path = data.content_dir.join(&file_name);

    path_if_existing(local_path).or_else(|| {
        let global_path = data.config.directory.join(&file_name);
        path_if_existing(global_path)
    })
}
