use std::io::{BufReader, Read};
use std::str;

use http::{
    header::{HeaderName, HeaderValue, TRANSFER_ENCODING},
    HeaderMap, StatusCode,
};

use crate::error::{Error, Result};
use crate::parsing::buffers::{self, trim_byte};
use crate::parsing::{BodyReader, CompressedReader, ResponseReader};
use crate::request::PreparedRequest;
use crate::streams::BaseStream;

pub fn parse_response_head<R>(reader: &mut BufReader<R>) -> Result<(StatusCode, HeaderMap)>
where
    R: Read,
{
    let mut line = Vec::new();
    let mut headers = HeaderMap::new();

    // status line
    let status: StatusCode = {
        buffers::read_line(reader, &mut line)?;
        let mut parts = line.split(|&b| b == b' ').filter(|x| !x.is_empty());

        let _ = parts.next().ok_or(Error::InvalidResponse("invalid status line"))?;
        let code = parts.next().ok_or(Error::InvalidResponse("invalid status line"))?;

        str::from_utf8(code)
            .map_err(|_| Error::InvalidResponse("cannot decode code"))?
            .parse()
            .map_err(|_| Error::InvalidResponse("invalid status code"))?
    };

    loop {
        buffers::read_line(reader, &mut line)?;
        if line.is_empty() {
            break;
        }

        let col = line
            .iter()
            .position(|&c| c == b':')
            .ok_or(Error::InvalidResponse("header has no colon"))?;

        let header = trim_byte(b' ', &line[..col]);
        let value = trim_byte(b' ', &line[col + 1..]);

        headers.append(
            HeaderName::from_bytes(header).map_err(http::Error::from)?,
            HeaderValue::from_bytes(value).map_err(http::Error::from)?,
        );
    }

    Ok((status, headers))
}

pub fn parse_response(
    reader: BaseStream,
    request: &PreparedRequest,
) -> Result<(StatusCode, HeaderMap, ResponseReader)> {
    let mut reader = BufReader::new(reader);
    let (status, mut headers) = parse_response_head(&mut reader)?;
    let body_reader = BodyReader::new(&headers, reader)?;
    let compressed_reader = CompressedReader::new(&headers, request, body_reader)?;
    let response_reader = ResponseReader::new(&headers, request, compressed_reader);

    // Remove HOP-BY-HOP headers
    headers.remove(TRANSFER_ENCODING);

    Ok((status, headers, response_reader))
}

#[test]
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let mut reader = BufReader::new(&response[..]);
    let (status, headers) = parse_response_head(&mut reader).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}
