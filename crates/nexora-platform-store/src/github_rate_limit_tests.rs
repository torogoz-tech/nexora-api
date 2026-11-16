use crate::{GitNetworkPolicy, SecretGenerator, github_client::GithubClient};
use nexora_platform::PlatformError;
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};
use zeroize::Zeroizing;

#[test]
fn github_rate_limits_are_temporary_without_turning_a_real_denial_into_a_grant() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let cases = [
            (
                "Retry-After: 1\r\n",
                "{\"message\":\"temporarily limited\"}",
            ),
            (
                "X-RateLimit-Remaining: 0\r\n",
                "{\"message\":\"request limited\"}",
            ),
            (
                "",
                "{\"message\":\"You have exceeded a secondary rate limit\"}",
            ),
            (
                "",
                "{\"message\":\"Resource not accessible by integration\"}",
            ),
        ];
        for (headers, body) in cases {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 512];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = socket.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() + count <= 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            let response = format!(
                "HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    let policy = GitNetworkPolicy {
        github_api_origin: origin,
        ..GitNetworkPolicy::default()
    };
    let client = GithubClient::new(&policy);
    let token = Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    let results = (0..4)
        .map(|index| client.get(&format!("/fixture/{index}"), &token))
        .collect::<Vec<_>>();
    server.join().unwrap();
    for result in &results[..3] {
        assert!(
            matches!(result, Err(PlatformError::Unavailable)),
            "Rate limit was classified as a definitive authorization denial"
        );
    }
    assert!(matches!(results[3], Err(PlatformError::Forbidden)));
}
