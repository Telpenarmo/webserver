use std::path::{Path, PathBuf};

pub fn match_file_type(filename: &Path) -> String {
    let guess = mime_guess::from_path(filename);
    let mime = match guess.first() {
        None => mime_guess::mime::APPLICATION_OCTET_STREAM,
        Some(mime) => mime,
    };
    let mime = if mime == mime_guess::mime::TEXT_PLAIN {
        mime_guess::mime::TEXT_PLAIN_UTF_8
    } else {
        mime
    };
    mime.to_string()
}

pub fn path_if_existing(path: PathBuf) -> Option<PathBuf> {
    if path.exists() {
        Some(path)
    } else {
        None
    }
}
