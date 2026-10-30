//! HTTP/1.1 tối giản tới hub trên loopback (`/healthz`, `GET /approvals/{id}/input`, sự kiện thử của trình cài đặt).
//! Không kéo cả thư viện HTTP vào app: một request, `Connection: close`, đọc tới EOF.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Hub giới hạn body 1 MB; `tool_input` trả về không lớn hơn thế nhiều.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// GET `path` trên `addr`, kèm `Authorization: Bearer <token>` nếu có.
pub fn get(
    addr: SocketAddr,
    path: &str,
    token: Option<&str>,
    timeout: Duration,
) -> Result<Response, String> {
    let headers: Vec<(String, String)> = token
        .map(|t| vec![("Authorization".to_owned(), format!("Bearer {t}"))])
        .unwrap_or_default();
    request(addr, "GET", path, &headers, None, timeout)
}

/// POST một body JSON (tự kiểm tra của trình cài đặt gửi sự kiện hook giả, `setup.rs`).
pub fn post_json(
    addr: SocketAddr,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    timeout: Duration,
) -> Result<Response, String> {
    request(addr, "POST", path, headers, Some(body), timeout)
}

fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<Response, String> {
    let mut s = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| format!("không kết nối được hub: {e}"))?;
    s.set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    }
    req.push_str("Connection: close\r\n\r\n");
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(body.unwrap_or_default());
    s.write_all(&bytes)
        .map_err(|e| format!("không gửi được yêu cầu tới hub: {e}"))?;
    let mut buf = Vec::new();
    s.take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("không đọc được phản hồi của hub: {e}"))?;
    if buf.len() > MAX_RESPONSE_BYTES {
        return Err("phản hồi của hub quá lớn".to_owned());
    }
    parse_response(&buf)
}

/// Tách mã trạng thái và body. Hỗ trợ `Content-Length` và `Transfer-Encoding: chunked`; không có cả hai thì body là
/// phần còn lại (đã đọc tới EOF).
pub fn parse_response(buf: &[u8]) -> Result<Response, String> {
    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("phản hồi HTTP thiếu phần đầu")?;
    let head = std::str::from_utf8(&buf[..split]).map_err(|_| "phần đầu HTTP không phải UTF-8")?;
    let rest = &buf[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or("dòng trạng thái HTTP sai")?;
    let mut length = None;
    let mut chunked = false;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.eq_ignore_ascii_case("content-length") {
            length = v.parse::<usize>().ok();
        } else if k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked") {
            chunked = true;
        }
    }
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = length {
        rest.get(..n)
            .ok_or("body ngắn hơn Content-Length")?
            .to_vec()
    } else {
        rest.to_vec()
    };
    Ok(Response { status, body })
}

fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("chunk thiếu độ dài")?;
        let size_txt = std::str::from_utf8(&rest[..eol]).map_err(|_| "độ dài chunk sai")?;
        let size_txt = size_txt.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_txt, 16).map_err(|_| "độ dài chunk sai")?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        out.extend_from_slice(rest.get(..size).ok_or("chunk bị cắt")?);
        rest = rest.get(size + 2..).ok_or("chunk bị cắt")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_length_body() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nx: y\r\n\r\nhelloEXTRA")
            .unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"hello");
    }

    #[test]
    fn chunked_body() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=1\r\nde\r\n0\r\n\r\n")
            .unwrap();
        assert_eq!(r.body, b"abcde");
    }

    #[test]
    fn body_until_eof_and_errors() {
        let r = parse_response(b"HTTP/1.1 404 Not Found\r\n\r\nkhong co").unwrap();
        assert_eq!((r.status, r.body.as_slice()), (404, b"khong co".as_slice()));
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n").is_err());
        assert!(parse_response(b"rac\r\n\r\n").is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nngan").is_err());
    }
}
