use std::collections::HashMap;
use std::io;
use std::io::Read;
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use http::HeaderMap;
use http::HeaderValue;
use http::StatusCode;
use http::header::AUTHORIZATION;
use pretty_assertions::assert_eq;

use super::*;

const BASE_URL: &str = "https://chatgpt.com/backend-api";
const BY_REPO_URL: &str =
    "https://chatgpt.com/backend-api/wham/environments/by-repo/github/openai/codex";
const GLOBAL_URL: &str = "https://chatgpt.com/backend-api/wham/environments";

#[tokio::test]
async fn production_http_forwards_headers_and_decodes_response() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("environment HTTP listener should bind");
    let address = listener
        .local_addr()
        .expect("environment HTTP listener should have an address");
    listener
        .set_nonblocking(true)
        .expect("environment HTTP listener should become nonblocking");
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "environment HTTP listener should receive a request"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("environment HTTP listener should accept: {error}"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("environment HTTP stream should become blocking");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("environment HTTP stream should get a read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let bytes_read = stream
                .read(&mut buffer)
                .expect("environment HTTP request should read");
            if bytes_read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..bytes_read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }

        let body = r#"[{"id":"env-real","label":"Real"}]"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("environment HTTP response should write");
        String::from_utf8(request).expect("environment HTTP request should be UTF-8")
    });
    let http = RouteAwareClientPool::new_without_request_logging(
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        ClientRouteClass::Api,
    );
    let base_url = format!("http://{address}");
    let headers =
        HeaderMap::from_iter([(AUTHORIZATION, HeaderValue::from_static("Bearer real-token"))]);

    let selection = tokio::time::timeout(
        Duration::from_secs(10),
        autodetect_environment_id_with_origins(
            &http,
            &base_url,
            &headers,
            /*desired_label*/ None,
            &[],
        ),
    )
    .await
    .expect("environment request should finish")
    .expect("environment response should decode");
    let request = server
        .join()
        .expect("environment HTTP server should finish");

    assert_eq!(
        selection,
        AutodetectSelection {
            id: "env-real".to_string(),
            label: Some("Real".to_string()),
        }
    );
    assert!(request.starts_with("GET /api/codex/environments HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer real-token\r\n")
    );
}

#[tokio::test]
async fn autodetect_requests_exact_repository_endpoint_and_decodes_selection() {
    let http = FakeHttp::new(HashMap::from([(
        BY_REPO_URL.to_string(),
        json_response(r#"[{"id":"env-repo","label":"Repository","is_pinned":true}]"#),
    )]));

    let headers = HeaderMap::from_iter([(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer forwarded-token"),
    )]);
    let selection = autodetect_environment_id_with_origins(
        &http,
        BASE_URL,
        &headers,
        Some("Repository".to_string()),
        &[SanitizedGitUrl::try_from("git@github.com:openai/codex.git").expect("valid Git remote")],
    )
    .await
    .expect("repository environment should be selected");

    assert_eq!(
        selection,
        AutodetectSelection {
            id: "env-repo".to_string(),
            label: Some("Repository".to_string()),
        }
    );
    assert_eq!(
        http.requests(),
        vec![RecordedRequest {
            url: BY_REPO_URL.to_string(),
            headers,
        }]
    );
}

/// Repository discovery must accept credential-bearing origins without exposing credentials.
#[tokio::test]
async fn autodetect_sanitizes_credential_bearing_git_origins() {
    let http = FakeHttp::new(HashMap::from([(
        BY_REPO_URL.to_string(),
        json_response(r#"[{"id":"env-repo","label":"Repository"}]"#),
    )]));

    let selection = autodetect_environment_id_with_origins(
        &http,
        BASE_URL,
        &HeaderMap::new(),
        /*desired_label*/ None,
        &[SanitizedGitUrl::try_from(
            "https://alice:synthetic-git-secret@github.com/openai/codex.git",
        )
        .expect("valid Git remote")],
    )
    .await
    .expect("repository environment should be selected");

    assert_eq!(
        selection,
        AutodetectSelection {
            id: "env-repo".to_string(),
            label: Some("Repository".to_string()),
        }
    );
    assert_eq!(http.requested_urls(), vec![BY_REPO_URL.to_string()]);
}

/// Removing an SCP remote username must not prevent repository-specific discovery.
#[tokio::test]
async fn autodetect_recognizes_sanitized_scp_git_origins() {
    let http = FakeHttp::new(HashMap::from([(
        BY_REPO_URL.to_string(),
        json_response(r#"[{"id":"env-repo","label":"Repository"}]"#),
    )]));

    let selection = autodetect_environment_id_with_origins(
        &http,
        BASE_URL,
        &HeaderMap::new(),
        /*desired_label*/ None,
        &[
            SanitizedGitUrl::try_from("org-123@github.com:openai/codex.git")
                .expect("valid Git remote"),
        ],
    )
    .await
    .expect("repository environment should be selected");

    assert_eq!(
        selection,
        AutodetectSelection {
            id: "env-repo".to_string(),
            label: Some("Repository".to_string()),
        }
    );
    assert_eq!(http.requested_urls(), vec![BY_REPO_URL.to_string()]);
}

#[tokio::test]
async fn autodetect_falls_back_to_exact_global_endpoint_and_decodes_selection() {
    let http = FakeHttp::new(HashMap::from([
        (BY_REPO_URL.to_string(), json_response("[]")),
        (
            GLOBAL_URL.to_string(),
            json_response(r#"[{"id":"env-global","label":"Global"}]"#),
        ),
    ]));

    let selection = autodetect_environment_id_with_origins(
        &http,
        BASE_URL,
        &HeaderMap::new(),
        /*desired_label*/ None,
        &[SanitizedGitUrl::try_from("git@github.com:openai/codex.git").expect("valid Git remote")],
    )
    .await
    .expect("global environment should be selected");

    assert_eq!(
        selection,
        AutodetectSelection {
            id: "env-global".to_string(),
            label: Some("Global".to_string()),
        }
    );
    assert_eq!(
        http.requested_urls(),
        vec![BY_REPO_URL.to_string(), GLOBAL_URL.to_string()]
    );
}

#[tokio::test]
async fn list_requests_exact_repository_and_global_endpoints_and_merges_results() {
    let http = FakeHttp::new(HashMap::from([
        (
            BY_REPO_URL.to_string(),
            json_response(r#"[{"id":"env-repo","label":"Repository"}]"#),
        ),
        (
            GLOBAL_URL.to_string(),
            json_response(
                r#"[{"id":"env-repo","is_pinned":true},{"id":"env-global","label":"Global"}]"#,
            ),
        ),
    ]));

    let rows = list_environments_with_origins(
        &http,
        BASE_URL,
        &HeaderMap::new(),
        &[
            SanitizedGitUrl::try_from("https://github.com/openai/codex.git")
                .expect("valid Git remote"),
        ],
    )
    .await
    .expect("environment list should decode");

    assert_eq!(
        rows.into_iter()
            .map(|row| (row.id, row.label, row.is_pinned, row.repo_hints))
            .collect::<Vec<_>>(),
        vec![
            (
                "env-repo".to_string(),
                Some("Repository".to_string()),
                true,
                Some("openai/codex".to_string()),
            ),
            (
                "env-global".to_string(),
                Some("Global".to_string()),
                false,
                None,
            ),
        ]
    );
    assert_eq!(
        http.requested_urls(),
        vec![BY_REPO_URL.to_string(), GLOBAL_URL.to_string()]
    );
}

#[tokio::test]
async fn global_list_distinguishes_empty_from_http_and_decode_failures() {
    let headers = HeaderMap::from_iter([(AUTHORIZATION, HeaderValue::from_static("Bearer test"))]);
    let empty = FakeHttp::new(HashMap::from([(
        GLOBAL_URL.to_string(),
        json_response("[]"),
    )]));
    assert!(
        list_environments_with_origins(&empty, BASE_URL, &headers, &[])
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(empty.requested_urls(), vec![GLOBAL_URL.to_string()]);
    assert_eq!(empty.requests()[0].headers, headers);

    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let http = FakeHttp::new(HashMap::from([(
            GLOBAL_URL.to_string(),
            EnvironmentResponse {
                status,
                content_type: "application/json".to_string(),
                body: "sensitive-backend-response".to_string(),
            },
        )]));
        let error = list_environments_with_origins(&http, BASE_URL, &headers, &[])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(&status.as_u16().to_string()), "{error}");
        assert!(!error.contains("sensitive-backend-response"));
    }

    let http = FakeHttp::new(HashMap::from([(
        GLOBAL_URL.to_string(),
        json_response(r#"[{"id":"sensitive-value","label":42}]"#),
    )]));
    let error = list_environments_with_origins(&http, BASE_URL, &headers, &[])
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("Invalid environment response"), "{error}");
    assert!(!error.contains("sensitive-value"));
}

/// The CLI must not turn an incomplete discovery response into "not found" or a
/// unique label match. The TUI may still use its explicitly partial fallback.
#[tokio::test]
async fn explicit_resolution_preserves_global_failure_despite_repository_results() {
    let origins = [SanitizedGitUrl::try_from("https://github.com/openai/codex.git").unwrap()];
    let headers = HeaderMap::from_iter([(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer synthetic-test-token"),
    )]);
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let responses = HashMap::from([
            (
                BY_REPO_URL.to_string(),
                json_response(r#"[{"id":"env-repo","label":"Repository"}]"#),
            ),
            (
                GLOBAL_URL.to_string(),
                EnvironmentResponse {
                    status,
                    content_type: "application/json".to_string(),
                    body: "sensitive-backend-response".to_string(),
                },
            ),
        ]);
        let partial = FakeHttp::new(responses.clone());
        assert_eq!(
            list_environments_with_origins(&partial, BASE_URL, &headers, &origins)
                .await
                .unwrap()
                .len(),
            1
        );

        for requested in ["env-global", "Repository"] {
            let http = FakeHttp::new(responses.clone());
            let error = resolve_environment_id_with_http(&http, BASE_URL, &headers, requested)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(&status.as_u16().to_string()), "{error}");
            assert!(!error.contains("not found"), "{error}");
            assert!(!error.contains("sensitive-backend-response"));
            assert_eq!(http.requested_urls(), vec![GLOBAL_URL.to_string()]);
            assert_eq!(http.requests()[0].headers, headers);
        }
    }
}

#[tokio::test]
async fn explicit_resolution_requires_an_exact_id_or_unambiguous_label() {
    let body = r#"[
        {"id":"env-1","label":"Shared"},
        {"id":"env-2","label":"Shared"},
        {"id":"env-3","label":"Unique"},
        {"id":"env-4","label":"env-1"}
    ]"#;
    for (requested, expected) in [(" env-1 ", "env-1"), ("uNiQuE", "env-3")] {
        let http = FakeHttp::new(HashMap::from([(
            GLOBAL_URL.to_string(),
            json_response(body),
        )]));
        assert_eq!(
            resolve_environment_id_with_http(&http, BASE_URL, &HeaderMap::new(), requested)
                .await
                .unwrap(),
            expected
        );
    }
    for (response, requested, expected_error) in [
        (body, "Shared", "ambiguous"),
        (body, "missing", "not found"),
        ("[]", "env-1", "no cloud environments"),
        (
            r#"[{"id":"secret","label":42}]"#,
            "env-1",
            "Invalid environment response",
        ),
    ] {
        let http = FakeHttp::new(HashMap::from([(
            GLOBAL_URL.to_string(),
            json_response(response),
        )]));
        let error = resolve_environment_id_with_http(&http, BASE_URL, &HeaderMap::new(), requested)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected_error), "{error}");
        assert!(!error.contains("secret"));
    }
    let http = FakeHttp::new(HashMap::new());
    let error = resolve_environment_id_with_http(&http, BASE_URL, &HeaderMap::new(), " ")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("must not be empty"));
    assert!(http.requests().is_empty());
}

struct FakeHttp {
    responses: HashMap<String, EnvironmentResponse>,
    requests: Mutex<Vec<RecordedRequest>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedRequest {
    url: String,
    headers: HeaderMap,
}

impl FakeHttp {
    fn new(responses: HashMap<String, EnvironmentResponse>) -> Self {
        Self {
            responses,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("request lock").clone()
    }

    fn requested_urls(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("request lock")
            .iter()
            .map(|request| request.url.clone())
            .collect()
    }
}

impl EnvironmentHttp for FakeHttp {
    async fn get(&self, url: &str, headers: &HeaderMap) -> anyhow::Result<EnvironmentResponse> {
        self.requests
            .lock()
            .expect("request lock")
            .push(RecordedRequest {
                url: url.to_string(),
                headers: headers.clone(),
            });
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected URL: {url}"))
    }
}

fn json_response(body: &str) -> EnvironmentResponse {
    EnvironmentResponse {
        status: StatusCode::OK,
        content_type: "application/json".to_string(),
        body: body.to_string(),
    }
}

mod r36_native_http {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::net::TcpListener;
    use std::net::TcpStream;
    use std::thread::JoinHandle;

    const DEADLINE: Duration = Duration::from_secs(5);
    const PRIVATE_BODY: &str = "synthetic-private-body-must-not-appear";

    fn pool() -> RouteAwareClientPool {
        // The external test process must use a minimal environment without
        // proxy/CA/auth variables; NO_PROXY=127.0.0.1,localhost.
        RouteAwareClientPool::new_without_redirects_or_request_logging(
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
            ClientRouteClass::Api,
        )
    }

    fn headers() -> HeaderMap {
        HeaderMap::from_iter([(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer r36-self-created-synthetic-only"),
        )])
    }

    fn accept(listener: &TcpListener) -> TcpStream {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match listener.accept() {
                Ok((stream, peer)) => {
                    assert!(peer.ip().is_loopback());
                    stream
                        .set_nonblocking(false)
                        .expect("blocking fixture stream");
                    stream
                        .set_read_timeout(Some(DEADLINE))
                        .expect("read deadline");
                    stream
                        .set_write_timeout(Some(DEADLINE))
                        .expect("write deadline");
                    return stream;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "no actual HTTP connection");
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        }
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).expect("read actual HTTP request");
            assert!(count > 0, "client closed before complete HTTP request");
            request.extend_from_slice(&buffer[..count]);
            assert!(
                request.len() <= 32_768,
                "unexpected oversized fixture request"
            );
        }
        let request = String::from_utf8(request).expect("ASCII fixture request");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer r36-self-created-synthetic-only\r\n")
        );
        request
    }

    fn listener() -> (TcpListener, String) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("own loopback listener");
        let address = listener.local_addr().expect("own listener address");
        listener
            .set_nonblocking(true)
            .expect("nonblocking fixture listener");
        (listener, format!("http://{address}/backend-api"))
    }

    fn fixture(responses: Vec<(u16, &'static str)>) -> (String, JoinHandle<Vec<String>>) {
        let (listener, base) = listener();
        let task = std::thread::spawn(move || {
            responses.into_iter().map(|(status, body)| {
                let mut stream = accept(&listener);
                let request = read_request(&mut stream);
                write!(stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()).expect("write exact fixture response");
                request.lines().next().expect("HTTP request line").to_string()
            }).collect()
        });
        (base, task)
    }

    fn assert_global_request(task: JoinHandle<Vec<String>>) {
        assert_eq!(
            task.join().expect("fixture thread completed"),
            vec!["GET /backend-api/wham/environments HTTP/1.1"]
        );
    }

    #[tokio::test]
    async fn actual_http_normal_and_empty_are_distinct() {
        for (body, expected) in [
            (
                r#"[{"id":"env-native","label":"Native","is_pinned":true}]"#,
                1,
            ),
            ("[]", 0),
        ] {
            let (base, server) = fixture(vec![(200, body)]);
            let rows =
                tokio::time::timeout(DEADLINE, list_all_environments(&pool(), &base, &headers()))
                    .await
                    .expect("request deadline")
                    .expect("valid HTTP 200 response");
            assert_eq!(rows.len(), expected);
            if expected == 1 {
                assert_eq!(rows[0].id, "env-native");
                assert_eq!(rows[0].label.as_deref(), Some("Native"));
                assert!(rows[0].is_pinned);
            }
            assert_global_request(server);
            println!("r36-native-http normal-empty rows={expected} requests=1");
        }
    }

    #[tokio::test]
    async fn actual_http_401_403_500_are_errors_not_empty_success() {
        for status in [401, 403, 500] {
            let (base, server) = fixture(vec![(status, PRIVATE_BODY)]);
            let error =
                tokio::time::timeout(DEADLINE, list_all_environments(&pool(), &base, &headers()))
                    .await
                    .expect("request deadline")
                    .expect_err("HTTP error must propagate");
            let text = error.to_string();
            assert!(text.contains(&status.to_string()), "status missing: {text}");
            assert!(!text.contains(PRIVATE_BODY));
            assert_global_request(server);
            println!("r36-native-http status={status} error=true bodyRedacted=true requests=1");
        }
    }

    #[tokio::test]
    async fn actual_http_malformed_200_is_decode_error() {
        let (base, server) = fixture(vec![(200, r#"[{"id":"synthetic-private-id","label":42}]"#)]);
        let error =
            tokio::time::timeout(DEADLINE, list_all_environments(&pool(), &base, &headers()))
                .await
                .expect("request deadline")
                .expect_err("invalid schema must fail");
        let text = error.to_string();
        assert!(text.contains("Invalid environment response"), "{text}");
        assert!(!text.contains("synthetic-private-id"));
        assert_global_request(server);
        println!("r36-native-http malformed200 decodeError=true bodyRedacted=true requests=1");
    }

    #[tokio::test]
    async fn actual_http_partial_tui_and_strict_explicit_resolution_are_distinct() {
        for status in [401, 403, 500] {
            let (base, server) = fixture(vec![
                (200, r#"[{"id":"env-repo","label":"Repository"}]"#),
                (status, PRIVATE_BODY),
            ]);
            let rows = tokio::time::timeout(
                DEADLINE,
                list_environments_with_origins(
                    &pool(),
                    &base,
                    &headers(),
                    &[
                        SanitizedGitUrl::try_from("https://github.com/openai/codex.git")
                            .expect("synthetic git origin only"),
                    ],
                ),
            )
            .await
            .expect("request deadline")
            .expect("documented TUI partial fallback");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, "env-repo");
            assert_eq!(
                server.join().expect("fixture thread completed"),
                vec![
                    "GET /backend-api/wham/environments/by-repo/github/openai/codex HTTP/1.1",
                    "GET /backend-api/wham/environments HTTP/1.1",
                ]
            );
            // Same production transport and global status, but explicit CLI
            // resolver must not use the partial repository result as completeness.
            let (base, server) = fixture(vec![(status, PRIVATE_BODY)]);
            let error = tokio::time::timeout(
                DEADLINE,
                resolve_environment_id_with_http(&pool(), &base, &headers(), "Repository"),
            )
            .await
            .expect("request deadline")
            .expect_err("global failure must propagate");
            let text = error.to_string();
            assert!(text.contains(&status.to_string()), "{text}");
            assert!(!text.contains("not found"));
            assert!(!text.contains(PRIVATE_BODY));
            assert_global_request(server);
            println!(
                "r36-native-http partial status={status} tuiRows=1 explicitError=true requests=2+1"
            );
        }
    }

    #[tokio::test]
    async fn actual_http_closed_loopback_port_is_connect_error() {
        let (listener, base) = listener();
        // This is our own freshly released endpoint, never an operational port.
        // External rebind between drop and connect is a fixture/environment failure.
        drop(listener);
        let error =
            tokio::time::timeout(DEADLINE, list_all_environments(&pool(), &base, &headers()))
                .await
                .expect("connect must finish before deadline")
                .expect_err("closed loopback port must not become an empty list");
        let transport = error
            .downcast_ref::<codex_http_client::RouteAwareRequestError>()
            .expect("connection failure must retain typed transport cause");
        assert!(
            transport.is_connect(),
            "unexpected transport category: {transport}"
        );
        println!("r36-native-http closed-port connectError=true emptySuccess=false");
    }

    #[tokio::test]
    async fn actual_http_dropped_request_closes_incomplete_body_connection() {
        let (listener, base) = listener();
        let (headers_sent, ready) = tokio::sync::oneshot::channel();
        let server = std::thread::spawn(move || {
            let mut stream = accept(&listener);
            let request = read_request(&mut stream);
            // Deliberately incomplete body leaves the real production get future pending.
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: keep-alive\r\n\r\n")
                .expect("write response headers");
            stream.flush().expect("flush response headers");
            headers_sent
                .send(())
                .expect("report actual response has started");
            let mut next = [0_u8; 1];
            match stream.read(&mut next) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                    ) => {}
                other => panic!("request drop must close peer before deadline: {other:?}"),
            }
            request
                .lines()
                .next()
                .expect("HTTP request line")
                .to_string()
        });
        // Keep the original client pool alive: dropping the whole pool must not
        // be what makes the incomplete in-flight socket disappear.
        let http = pool();
        let request_http = http.clone();
        let request_headers = headers();
        let request = tokio::spawn(async move {
            list_all_environments(&request_http, &base, &request_headers).await
        });
        tokio::time::timeout(DEADLINE, ready)
            .await
            .expect("server must observe actual HTTP request")
            .expect("server readiness");
        assert!(
            !request.is_finished(),
            "incomplete body must remain pending"
        );
        request.abort();
        let cancelled = tokio::time::timeout(DEADLINE, request)
            .await
            .expect("request abort deadline")
            .expect_err("aborted request must not return a list");
        assert!(cancelled.is_cancelled());
        let observed = tokio::time::timeout(
            DEADLINE + Duration::from_secs(1),
            tokio::task::spawn_blocking(move || server.join()),
        )
        .await
        .expect("peer close observation deadline")
        .expect("blocking observer task")
        .expect("peer must close before read deadline");
        assert_eq!(observed, "GET /backend-api/wham/environments HTTP/1.1");
        drop(http);
        println!(
            "r36-native-http cancel requests=1 joinCancelled=true peerClosed=true poolAliveAtClose=true"
        );
    }
}
