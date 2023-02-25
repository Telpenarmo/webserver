use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{get_error_page, http::*, Config, HostState};

pub fn handle_request(request: Request, server_data: &HostState, content_dir: &Path) -> Response {
    if request.method.as_str() != "GET" {
        let mut resp = Response::new(Status::MethodNotAllowed);
        resp.set_header("Allow", "GET");
        return resp;
    }

    let mut rel_res_path = content_dir.to_path_buf();
    let mut path = request.path.to_string();
    path.remove(0);
    rel_res_path.push(&path);
    eprintln!("requested resource: {}", rel_res_path.display());

    handle_get_request(&request, server_data, content_dir, rel_res_path)
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
            let mut resp = Response::new(Status::Ok);
            resp.load_file(&res_path);
            resp
        }
        Err(_) => load_error(Status::Forbidden, server_data.config),
    }
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
        eprintln!("loading error page from file");
        response.load_file(path.as_path())
    } else {
        response.add_content("unknown error");
    }
    response
}
