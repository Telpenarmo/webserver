use crate::http::Request;

pub enum Error {
    Partial,
    TooManyHeaders,
    Syntax,
}

pub fn try_parse(headers_size: usize, buffer: &mut [u8]) -> Result<(Request, usize), Error> {
    let mut headers = vec![httparse::EMPTY_HEADER; headers_size];
    let mut req = httparse::Request::new(&mut headers);
    match req.parse(buffer) {
        Ok(httparse::Status::Complete(s)) => {
            // let a:Vec<u8> = buffer.into_iter().skip(s).collect();
            Ok((Request::new(req), s))
        }
        Ok(httparse::Status::Partial) => Err(Error::Partial),
        Err(httparse::Error::TooManyHeaders) => Err(Error::TooManyHeaders),
        Err(err) => {
            eprintln!("Parsing error: {}", err);
            Err(Error::Syntax)
        }
    }
}
