use crate::api_client::ApiError;
use std::net::TcpListener;
use tokio::sync::oneshot;

fn get_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind ephemeral port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

async fn start_callback_server(
    port: u16,
    tx: oneshot::Sender<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .expect("Failed to bind callback listener");

        if let Ok((mut stream, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);

            let token = request
                .lines()
                .next()
                .and_then(|line| {
                    let path = line.split_whitespace().nth(1)?;
                    let query = path.split('?').nth(1)?;
                    query.split('&').find_map(|pair| {
                        let (key, value) = pair.split_once('=')?;
                        if key == "token" {
                            Some(value.to_string())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();

            let html = if token.is_empty() {
                "<html><body><h1>Login failed</h1><p>No token received. You can close this tab.</p></body></html>"
            } else {
                "<html><body><h1>Login successful!</h1><p>You can close this tab and return to the terminal.</p></body></html>"
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;

            if !token.is_empty() {
                let _ = tx.send(token);
            }
        }
    })
}

pub async fn browser_login(server_url: &str) -> Result<String, ApiError> {
    let port = get_available_port();
    let redirect_uri = format!("http://localhost:{}/callback", port);

    let (tx, rx) = oneshot::channel();
    let handle = start_callback_server(port, tx).await;

    let mut parsed_url = url::Url::parse(&format!("{}/auth/oauth/github/login", server_url))
        .map_err(|e| ApiError::Other(format!("Failed to parse server URL: {}", e)))?;
    parsed_url
        .query_pairs_mut()
        .append_pair("redirect_uri", &redirect_uri);
    let login_url = parsed_url.to_string();

    println!("Opening browser for login...");
    if open::that(&login_url).is_err() {
        println!("Could not open browser automatically.");
    }
    println!("If browser didn't open, visit:\n  {}\n", login_url);
    println!("Waiting for authentication...");

    let token = tokio::time::timeout(std::time::Duration::from_secs(120), rx)
        .await
        .map_err(|_| ApiError::Other("Login timed out after 120 seconds".into()))?
        .map_err(|_| ApiError::Other("Login callback channel closed unexpectedly".into()))?;

    handle.abort();

    if token.is_empty() {
        return Err(ApiError::Other("Empty token received from callback".into()));
    }

    Ok(token)
}
