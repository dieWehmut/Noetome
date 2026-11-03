//! One bounded native HTTP request; receipts survive cancellation and errors.
use crate::project::ProjectError;
use std::future::Future;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;

pub(crate) struct TransportResult {
    pub(crate) status: Option<u16>,
    pub(crate) request_id: Option<String>,
    pub(crate) body: Vec<u8>,
    pub(crate) complete: bool,
    pub(crate) attempts: u32,
    pub(crate) failure: Option<ProjectError>,
}
pub(crate) fn send(
    endpoint: &str,
    bearer: &str,
    body: &[u8],
    timeout: Duration,
    max_response_bytes: usize,
    check: &dyn Fn() -> Result<(), ProjectError>,
) -> Result<TransportResult, ProjectError> {
    check()?;
    let start = Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| failure("EXTRACT_API_TRANSPORT"))?;
    let client = reqwest::Client::builder()
        .use_native_tls()
        .http1_only()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| failure("EXTRACT_API_TRANSPORT"))?;
    // Build before counting an attempt; invalid headers/URI never reach the network.
    let request = client
        .post(endpoint)
        .bearer_auth(bearer)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_vec())
        .build()
        .map_err(|_| failure("EXTRACT_API_TRANSPORT"))?;
    check()?;
    let mut result = TransportResult {
        status: None,
        request_id: None,
        body: Vec::new(),
        complete: false,
        attempts: 0,
        failure: None,
    };
    let outcome = runtime.block_on(async {
        check()?;
        let network = async {
            result.attempts = 1;
            let mut response = client
                .execute(request)
                .await
                .map_err(|_| failure("EXTRACT_API_TRANSPORT"))?;
            result.status = Some(response.status().as_u16());
            result.request_id = response
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| failure("EXTRACT_API_TRUNCATED"))?
            {
                let available = max_response_bytes.saturating_sub(result.body.len());
                if chunk.len() > available {
                    result.body.extend_from_slice(&chunk[..available]);
                    return Err(failure("EXTRACT_RESPONSE_LIMIT"));
                }
                result.body.extend_from_slice(&chunk);
            }
            // Normal EOF proves HTTP framing is complete, including a zero-byte
            // body. The wire decoder determines whether those bytes are usable.
            result.complete = true;
            match result.status.unwrap_or(0) {
                200 => Ok(()),
                401 | 403 => Err(failure("EXTRACT_API_HTTP_AUTH")),
                429 => Err(failure("EXTRACT_API_HTTP_RATE_LIMIT")),
                500..=599 => Err(failure("EXTRACT_API_HTTP_SERVER")),
                _ => Err(failure("EXTRACT_API_HTTP_STATUS")),
            }
        };
        let mut network = std::pin::pin!(network);
        let mut tick = std::pin::pin!(tokio::time::sleep(Duration::ZERO));
        futures_util::future::poll_fn(|cx| {
            if start.elapsed() >= timeout {
                return Poll::Ready(Err(failure("EXTRACT_TIMEOUT")));
            }
            if tick.as_mut().poll(cx).is_ready() {
                if let Err(error) = check() {
                    return Poll::Ready(Err(error));
                }
                if start.elapsed() >= timeout {
                    return Poll::Ready(Err(failure("EXTRACT_TIMEOUT")));
                }
                tick.as_mut()
                    .reset(tokio::time::Instant::now() + Duration::from_millis(5));
                // Register the timer waker even if the network stays pending forever.
                let _ = tick.as_mut().poll(cx);
            }
            network.as_mut().poll(cx)
        })
        .await
        // Both pinned futures drop here before ending the current-thread runtime.
    });
    result.failure = outcome.err();
    drop(client);
    // System DNS work already running in a blocking worker cannot be cancelled.
    // Do not let waiting for that worker extend the bounded network operation.
    runtime.shutdown_background();
    Ok(result)
}
fn failure(code: &'static str) -> ProjectError {
    ProjectError::new(code, 3, "文本 API 请求失败；仅保留已实际接收的有界字节。")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;
    use std::time::Instant;

    fn server(
        response: Vec<u8>,
        delay: Duration,
        accepted: Arc<AtomicBool>,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let until = Instant::now() + Duration::from_millis(800);
            let mut stream: TcpStream = loop {
                if let Ok((s, _)) = listener.accept() {
                    break s;
                }
                if Instant::now() > until {
                    return;
                }
                thread::sleep(Duration::from_millis(2));
            };
            // Windows may propagate the listener's nonblocking mode to accepted sockets.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut receipt = Vec::new();
            while !receipt.windows(4).any(|b| b == b"\r\n\r\n") {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                receipt.extend_from_slice(&buf[..n]);
            }
            assert!(receipt.starts_with(b"POST /v1/responses HTTP/1.1\r\n"));
            accepted.store(true, Ordering::SeqCst);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
            thread::sleep(delay);
            drop(stream);
            assert!(
                listener.accept().is_err(),
                "a redirect or retry opened another connection"
            );
        });
        (endpoint, worker)
    }
    fn run(bytes: &[u8], delay: Duration, max: usize, timeout: Duration) -> TransportResult {
        let (endpoint, worker) = server(bytes.to_vec(), delay, Arc::new(AtomicBool::new(false)));
        let result = send(&endpoint, "noetome-test-key", b"{}", timeout, max, &|| {
            Ok(())
        });
        worker.join().unwrap();
        result.unwrap()
    }
    #[test]
    fn one_http1_post_preserves_complete_receipt() {
        let result = run(
            b"HTTP/1.1 200 OK\r\nx-request-id: req-test\r\nContent-Length: 2\r\n\r\n{}",
            Duration::ZERO,
            20,
            Duration::from_secs(2),
        );
        assert_eq!(result.status, Some(200));
        assert_eq!(result.request_id.as_deref(), Some("req-test"));
        assert_eq!(result.body, b"{}");
        assert!(result.complete);
        assert_eq!(result.attempts, 1);
        assert!(result.failure.is_none());
    }
    #[test]
    fn http_errors_and_redirect_are_stable_and_never_retried() {
        for (status, expected) in [
            (401, "EXTRACT_API_HTTP_AUTH"),
            (403, "EXTRACT_API_HTTP_AUTH"),
            (429, "EXTRACT_API_HTTP_RATE_LIMIT"),
            (503, "EXTRACT_API_HTTP_SERVER"),
            (302, "EXTRACT_API_HTTP_STATUS"),
        ] {
            let bytes = format!(
                "HTTP/1.1 {status} Test\r\nLocation: /v1/responses\r\nContent-Length: 6\r\n\r\nsecret"
            );
            let result = run(bytes.as_bytes(), Duration::ZERO, 20, Duration::from_secs(2));
            assert_eq!(result.failure.as_ref().unwrap().code(), expected);
            assert_eq!(result.body, b"secret");
            assert!(result.complete);
            assert_eq!(result.attempts, 1);
            assert!(!result.failure.unwrap().to_string().contains("secret"));
        }
    }
    #[test]
    fn timeout_and_cancel_drop_future_and_keep_partial_body() {
        let partial = b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\nabc";
        let result = run(
            partial,
            Duration::from_millis(200),
            200,
            Duration::from_millis(100),
        );
        assert_eq!(result.failure.unwrap().code(), "EXTRACT_TIMEOUT");
        assert_eq!(result.body, b"abc");
        assert!(!result.complete);
        let observed = Arc::new(AtomicBool::new(false));
        let (endpoint, worker) = server(
            partial.to_vec(),
            Duration::from_millis(200),
            observed.clone(),
        );
        let start = Instant::now();
        let result = send(
            &endpoint,
            "noetome-test-key",
            b"{}",
            Duration::from_secs(2),
            200,
            &|| {
                if observed.load(Ordering::SeqCst) && start.elapsed() > Duration::from_millis(70) {
                    Err(ProjectError::new("EXTRACT_CANCELLED", 3, "cancelled"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert!(start.elapsed() < Duration::from_millis(180));
        worker.join().unwrap();
        assert_eq!(result.failure.unwrap().code(), "EXTRACT_CANCELLED");
        assert_eq!(result.body, b"abc");
        assert!(!result.complete);
    }
    #[test]
    fn truncated_chunked_and_oversize_receipts_fail() {
        for (bytes, max, code) in [
            (
                &b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\nabc"[..],
                100,
                "EXTRACT_API_TRUNCATED",
            ),
            (
                &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n"[..],
                100,
                "EXTRACT_API_TRUNCATED",
            ),
            (
                &b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabcdef"[..],
                3,
                "EXTRACT_RESPONSE_LIMIT",
            ),
        ] {
            let result = run(bytes, Duration::ZERO, max, Duration::from_secs(2));
            assert_eq!(result.failure.unwrap().code(), code);
            assert!(!result.complete);
            assert!(result.body.len() <= max);
        }
    }
    #[test]
    fn framed_empty_receipts_are_complete_and_keep_http_classification() {
        for (status, expected) in [
            (200, None),
            (401, Some("EXTRACT_API_HTTP_AUTH")),
            (429, Some("EXTRACT_API_HTTP_RATE_LIMIT")),
        ] {
            let bytes = format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\n\r\n");
            let result = run(
                bytes.as_bytes(),
                Duration::ZERO,
                100,
                Duration::from_secs(2),
            );
            assert_eq!(result.status, Some(status));
            assert!(result.body.is_empty());
            assert!(
                result.complete,
                "Content-Length:0 is framed complete for {status}"
            );
            assert_eq!(result.attempts, 1);
            assert_eq!(result.failure.as_ref().map(ProjectError::code), expected);
            if status == 200 {
                assert_eq!(
                    crate::extraction_api_wire::decode(&result.body)
                        .unwrap_err()
                        .code(),
                    "EXTRACT_API_RESPONSE"
                );
            }
        }
        let result = run(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            Duration::ZERO,
            100,
            Duration::from_secs(2),
        );
        assert!(result.complete);
        assert!(result.body.is_empty());
        assert!(result.failure.is_none());
    }
    #[test]
    fn pre_cancel_sends_nothing() {
        let result = send(
            "http://127.0.0.1:1/v1/responses",
            "noetome-test-key",
            b"{}",
            Duration::from_secs(1),
            100,
            &|| Err(ProjectError::new("EXTRACT_CANCELLED", 3, "cancelled")),
        );
        assert_eq!(result.err().unwrap().code(), "EXTRACT_CANCELLED");
    }
    #[test]
    fn zero_deadline_counts_no_unstarted_attempt() {
        let result = send(
            "http://127.0.0.1:1/v1/responses",
            "noetome-test-key",
            b"{}",
            Duration::ZERO,
            100,
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(result.failure.unwrap().code(), "EXTRACT_TIMEOUT");
        assert_eq!(result.attempts, 0);
    }
    #[test]
    fn runtime_shutdown_does_not_wait_for_uncancellable_resolver_work() {
        let (endpoint, worker) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec(),
            Duration::from_millis(30),
            Arc::new(AtomicBool::new(false)),
        );
        let scheduled = AtomicBool::new(false);
        let (done, wait) = std::sync::mpsc::channel();
        let start = Instant::now();
        let result = send(
            &endpoint,
            "noetome-test-key",
            b"{}",
            Duration::from_secs(2),
            100,
            &|| {
                if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                    if !scheduled.swap(true, Ordering::SeqCst) {
                        let done = done.clone();
                        runtime.spawn_blocking(move || {
                            thread::sleep(Duration::from_millis(300));
                            done.send(()).unwrap();
                        });
                    }
                }
                Ok(())
            },
        )
        .unwrap();
        let elapsed = start.elapsed();
        worker.join().unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(result.complete);
        assert!(
            elapsed < Duration::from_millis(200),
            "shutdown waited {elapsed:?}"
        );
    }
}
