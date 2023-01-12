use std::ffi::OsStr;
use std::fmt::Debug;
use std::path::Path;

pub fn match_file_type(filename: &Path) -> &str {
    match filename.extension().and_then(OsStr::to_str) {
        Some("txt") => "text/plain, charset=utf-8",
        Some("html") => "text/html, charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("jpg") => "image/jpeg",
        Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

pub trait ResultExtension<T> {
    fn unwrap_with_note(self, annotation: &str) -> T;
}

impl<T, E> ResultExtension<T> for Result<T, E>
where
    E: Debug,
{
    fn unwrap_with_note(self, annotation: &str) -> T {
        self.unwrap_or_else(|err| panic!("{}: {:#?}", annotation, err))
    }
}
