use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use tracing::info;

use crate::{get_error_page, http::*, Config, HostState};

pub struct Data {
    content_dir: PathBuf,
    handlers: HashMap<String, MethodHandler>,
}

impl Data {
    pub fn new(content_dir: PathBuf) -> Data {
        Data {
            content_dir,
            handlers: get_handlers(),
        }
    }
}

type MethodHandler = Box<dyn Fn(&Request, &HostState, &Path, PathBuf) -> Response + Sync>;

pub fn handle_request(request: Request, server_data: &HostState, data: &Data) -> Response {
    let Some(handler) = data.handlers.get(&request.method) else {
            let mut resp = Response::new(Status::MethodNotAllowed);
            let allowed_methods = data.handlers.keys().map(|s| &**s).collect::<Vec<_>>().join(", ");
            resp.set_header("Allow", allowed_methods);
            return resp;
        };

    let content_dir = &data.content_dir;
    let mut rel_res_path = content_dir.to_path_buf();
    let mut path = request.path.to_string();
    path.remove(0);
    rel_res_path.push(&path);
    info!("requested resource: {}", rel_res_path.display());

    handler(&request, server_data, content_dir, rel_res_path)
}

fn get_handlers() -> HashMap<String, MethodHandler> {
    let mut handlers: HashMap<String, MethodHandler> = HashMap::new();
    handlers.insert("GET".into(), Box::new(handle_get_request));
    handlers.insert("HEAD".into(), Box::new(handle_head_request));
    handlers
}

fn handle_get_request(
    _request: &Request,
    server_data: &HostState,
    content_dir: &Path,
    rel_res_path: PathBuf,
) -> Response {
    let res_path = match std::fs::canonicalize(rel_res_path) {
        Ok(path) => path,
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => return load_error(Status::NotFound, server_data.config),
            io::ErrorKind::PermissionDenied => {
                return load_error(Status::Forbidden, server_data.config);
            }
            _ => return server_error(err.to_string()),
        },
    };

    match res_path.strip_prefix(content_dir) {
        Ok(rel_res_path) => {
            if res_path.is_dir() {
                return redirect_dir(rel_res_path, server_data);
            }
            let resp = Response::new(Status::Ok);
            resp.load_file(&res_path)
        }
        Err(_) => load_error(Status::Forbidden, server_data.config),
    }
}

fn handle_head_request(
    request: &Request,
    server_data: &HostState,
    content_dir: &Path,
    rel_res_path: PathBuf,
) -> Response {
    let get_response = handle_get_request(request, server_data, content_dir, rel_res_path);
    get_response.to_head()
}

fn redirect_dir(path: &Path, server_data: &HostState) -> Response {
    let mut resp = Response::new(Status::Moved);
    let Some(path) = path.to_str() else {
        return load_error(Status::BadRequest, server_data.config);
    };
    let index_location = format!(
        "http://{}:{}{}/index.html",
        server_data.hostname, server_data.config.port, path
    );
    resp.set_header("Location", index_location);
    resp
}

fn load_error(status: Status, config: &Config) -> Response {
    let mut response = Response::new(status);
    let error_file = get_error_page(&status, config);
    if let Some(path) = error_file {
        info!("loading error page from file");
        response.load_file(path.as_path())
    } else {
        response.add_content("unknown error");
        response
    }
}
