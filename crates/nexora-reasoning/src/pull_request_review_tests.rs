use crate::{OpenAiClient, PullRequestReview};
use serde_json::{Value, json};
#[test]
fn manual_review_uses_one_response_request_and_validates_evidence() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let body = loop {
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]);
                let size: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + size])
                        .unwrap();
                }
            }
        };
        assert_eq!(body["store"], false);
        assert_eq!(body["tools"], json!([]));
        assert_eq!(body["text"]["format"]["strict"], true);
        assert!(body["instructions"].as_str().unwrap().contains("untrusted"));
        let report = json!({"summary":"Static review only.","findings":[],"limitations":["No tests were executed."]});
        let response=json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":report.to_string()}]}]}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
    });
    let client = OpenAiClient::test_client(format!("http://{address}/responses"));
    let report = PullRequestReview::run(
        &client,
        &json!({"files":[{"path":"source.rs","patch":"@@ -1 +1 @@\n-old\n+new"}]}),
        "medium",
    )
    .unwrap();
    assert_eq!(report["findings"], json!([]));
    task.join().unwrap();
}
