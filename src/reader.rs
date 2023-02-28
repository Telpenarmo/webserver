use std::io::{self, Read};
use std::net::TcpStream;
use std::time::Duration;

use crate::{http::Request, Config};

pub enum ReadError {
    ConnectionClosed,
    Timeout,
    BadSyntax,
    TooManyHeaders,
}

pub fn read_request(stream: &mut TcpStream, config: &Config) -> Result<Request, ReadError> {
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
                match try_read(&mut buffer, config.max_headers_number) {
                    ReadResult::Partial => continue,
                    ReadResult::Err(err) => break Err(err),
                    ReadResult::Ok(res) => break Ok(res),
                }
            }
        }
    }
}

enum ReadResult {
    Partial,
    Ok(Request),
    Err(ReadError),
}

fn try_read(buffer: &mut [u8], max_headers_count: usize) -> ReadResult {
    let mut headers_size = 16;
    loop {
        match try_parse(headers_size, buffer) {
            Err(ParsingError::Partial) => break ReadResult::Partial,
            Err(ParsingError::TooManyHeaders) => {
                if headers_size < max_headers_count {
                    headers_size = usize::min(2 * headers_size, max_headers_count);
                } else {
                    break ReadResult::Err(ReadError::TooManyHeaders);
                }
            }
            Err(ParsingError::Syntax) => break ReadResult::Err(ReadError::BadSyntax),
            Ok((req, _s)) => {
                if let Err(err) = get_content_length(&req) {
                    break err;
                }
                break ReadResult::Ok(req);
            }
        }
    }
}

enum ParsingError {
    Partial,
    TooManyHeaders,
    Syntax,
}

fn try_parse(headers_size: usize, buffer: &mut [u8]) -> Result<(Request, usize), ParsingError> {
    let mut headers = vec![httparse::EMPTY_HEADER; headers_size];
    let mut req = httparse::Request::new(&mut headers);
    match req.parse(buffer) {
        Ok(httparse::Status::Complete(s)) => {
            // let a:Vec<u8> = buffer.into_iter().skip(s).collect();
            Ok((Request::new(req), s))
        }
        Ok(httparse::Status::Partial) => Err(ParsingError::Partial),
        Err(httparse::Error::TooManyHeaders) => Err(ParsingError::TooManyHeaders),
        Err(err) => {
            eprintln!("Parsing error: {}", err);
            Err(ParsingError::Syntax)
        }
    }
}

fn get_content_length(req: &Request) -> Result<u32, ReadResult> {
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
    match content_length {
        Ok(len) => Ok(len),
        Err(err) => Err(ReadResult::Err(err)),
    }
}
