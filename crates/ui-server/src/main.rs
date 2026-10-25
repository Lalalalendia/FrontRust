//! Local reference server; do not expose on the public internet.
use std::io::{Read,Write};
use std::net::{TcpListener,TcpStream};
const HTML:&str=include_str!("../../../web/index.html");
const APP:&str=include_str!("../../../web/app.js");
const CSS:&str=include_str!("../../../web/style.css");
fn send(mut stream:TcpStream,status:&str,mime:&str,body:&[u8])->std::io::Result<()> {
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",body.len())?;
    stream.write_all(body)
}
fn home()->String {
    let meta=r#"<title>Rust UI Platform — HTML и Canvas</title>
<meta name="description" content="Независимый Rust UI prototype: HTML, SSR, Canvas и WASM.">
<meta name="robots" content="noindex,nofollow">
<link rel="canonical" href="http://127.0.0.1:8787/">
<meta property="og:title" content="Rust UI Platform">
<meta property="og:type" content="website">
<meta property="og:url" content="http://127.0.0.1:8787/">"#;
    let schema=r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"WebApplication","name":"Rust UI Platform"}</script>"#;
    HTML.replace("<!--SSR_HEAD-->",meta).replace("<!--SSR_SCHEMA-->",schema)
}
fn serve(mut stream:TcpStream)->std::io::Result<()> {
    let mut request=[0_u8;8192];
    let size=stream.read(&mut request)?;
    let first=String::from_utf8_lossy(&request[..size]);
    let path=first.lines().next().and_then(|s|s.split_whitespace().nth(1)).unwrap_or("");
    match path {
        "/" => send(stream,"200 OK","text/html; charset=utf-8",home().as_bytes()),
        "/app.js" => send(stream,"200 OK","text/javascript; charset=utf-8",APP.as_bytes()),
        "/style.css" => send(stream,"200 OK","text/css; charset=utf-8",CSS.as_bytes()),
        "/healthz" => send(stream,"200 OK","text/plain",b"ok"),
        "/robots.txt" => send(stream,"200 OK","text/plain",b"User-agent: *\nDisallow: /\n"),
        "/assets/ui_wasm.wasm" => match std::fs::read("dist/ui_wasm.wasm") {
            Ok(bytes) => send(stream,"200 OK","application/wasm",&bytes),
            Err(_) => send(stream,"404 Not Found","text/plain",b"Build rui-wasm first"),
        },
        _ => send(stream,"404 Not Found","text/plain",b"not found"),
    }
}
fn main()->std::io::Result<()> {
    let listener=TcpListener::bind("127.0.0.1:8787")?;
    for result in listener.incoming() {
        match result {
            Ok(stream)=>if let Err(e)=serve(stream){eprintln!("request error: {e}")},
            Err(e)=>eprintln!("connection error: {e}")
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn html_is_server_rendered_and_noindex() {
        let h=home();assert!(h.contains("<main"));assert!(h.contains("<h1"));
        assert!(h.contains("noindex,nofollow"));assert!(h.contains("application/ld+json"));
        assert!(h.contains("rel=\"canonical\""));
        assert!(!h.contains("<!--SSR_HEAD-->"));
    }
}