use std::process::Output;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use super::{Server, WAIT};

pub async fn reject_device_login(server: &Server, error: &str) -> Output {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let replies = [
        (
            "GET /.well-known/oauth-authorization-server ",
            "200 OK",
            json!({
                "issuer": url,
                "authorization_endpoint": format!("{url}/oauth/authorize"),
                "token_endpoint": format!("{url}/oauth/token"),
                "device_authorization_endpoint": format!("{url}/oauth/device"),
            }),
        ),
        (
            "POST /oauth/device ",
            "200 OK",
            json!({
                "device_code": "failed-device-sign-in",
                "user_code": "ABCD-EFGH",
                "verification_uri": format!("{url}/device"),
                "verification_uri_complete": format!("{url}/device?user_code=ABCD-EFGH"),
                "expires_in": 900,
                "interval": 1,
            }),
        ),
        (
            "POST /oauth/token ",
            "400 Bad Request",
            json!({"error": error}),
        ),
    ];
    let task = tokio::spawn(async move {
        for (request, status, body) in replies {
            let (socket, _) = listener.accept().await.unwrap();
            respond(socket, request, status, body).await;
        }
    });
    let output = server
        .run(&["login", &url, "--device", "--output", "json"])
        .await;
    timeout(WAIT, task).await.unwrap().unwrap();
    output
}

async fn respond(socket: TcpStream, request: &str, status: &str, body: Value) {
    let mut socket = BufReader::new(socket);
    let mut line = String::new();
    socket.read_line(&mut line).await.unwrap();
    assert!(line.starts_with(request), "Unexpected request: {line}");
    let mut length = 0;
    loop {
        line.clear();
        assert!(socket.read_line(&mut line).await.unwrap() > 0);
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().unwrap();
        }
    }
    socket.read_exact(&mut vec![0; length]).await.unwrap();
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket
        .get_mut()
        .write_all(response.as_bytes())
        .await
        .unwrap();
}
