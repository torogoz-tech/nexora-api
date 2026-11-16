use crate::{ContributorRating, OpenAiClient};
use serde_json::{Value, json};
#[test]
fn independent_private_rating_has_no_tools_or_default_score() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let request = loop {
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
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
        assert_eq!(request["store"], false);
        assert_eq!(request["tools"], json!([]));
        assert_eq!(
            request["text"]["format"]["name"],
            "private_contributor_rating"
        );
        assert!(
            request["instructions"]
                .as_str()
                .unwrap()
                .contains("No default score")
        );
        assert!(
            request["instructions"]
                .as_str()
                .unwrap()
                .contains("untrusted")
        );
        let report = json!({"status":"insufficient_evidence","reason":"Only incomplete static evidence was supplied.","rubric":[],"limitations":["No executed verification."]});
        let reply=json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":report.to_string()}]}]}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).unwrap();
    });
    let client = OpenAiClient::test_client(format!("http://{address}/responses"));
    let result = ContributorRating::run(
        &client,
        &json!({"actor_id":"developer","criteria":[],"commits":[]}),
        "medium",
    )
    .unwrap();
    assert_eq!(result["status"], "insufficient_evidence");
    assert!(!result.as_object().unwrap().contains_key("score"));
    server.join().unwrap();
}
