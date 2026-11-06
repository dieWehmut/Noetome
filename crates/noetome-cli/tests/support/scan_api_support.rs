//! Artificial loopback infrastructure shared by CLI and private capability tests.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
pub const KEY: &str = "noetome-test-scan-secret";
pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
pub fn read(p: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
pub fn write(p: impl AsRef<Path>, v: &Value) {
    fs::write(p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
pub fn response(tex: Option<&str>) -> Value {
    let content = if let Some(tex) = tex {
        json!({"schema_version":1,"recognized_tex":tex,"uncertainties":[]})
    } else {
        json!({"schema_version":1,"recognized_tex":null,"uncertainties":[{"kind":"sign","description":"A key position is obscured."}]})
    };
    json!({"id":"chat-artificial","model":"noetome-test-returned","object":"chat.completion","created":1,"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content.to_string(),"reasoning_content":null}}],"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}})
}
pub struct Case {
    pub root: PathBuf,
    pub case: PathBuf,
    pub api: PathBuf,
    pub profile: PathBuf,
    pub output: PathBuf,
}
impl Case {
    pub fn new(name: &str) -> Self {
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("scan-api-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("case");
        fs::create_dir(&case).unwrap();
        for e in fs::read_dir(repo().join("fixtures/scan-formula/cases").join(name)).unwrap() {
            let p = e.unwrap().path();
            fs::copy(&p, case.join(p.file_name().unwrap())).unwrap();
        }
        let api = root.join("api.json");
        write(
            &api,
            &json!({"schema_version":1,"provider":"openai_chat_completions","transport_mode":"local_http_test","endpoint":"http://127.0.0.1:1/v1/chat/completions","model":"noetome-test-scan","credential_env":"NOETOME_TEST_SCAN_CREDENTIAL","required_input":"vision","capabilities":{"text":true,"vision":true,"structured_output":true},"limits":{"max_input_bytes":1048576,"max_response_bytes":65536,"max_output_tokens":2048,"timeout_ms":3000},"budget":{"max_requests":0,"max_total_tokens":1048576,"max_cost_usd":null}}),
        );
        let profile = root.join("profile.json");
        write(
            &profile,
            &json!({"schema_version":1,"app":"Obsidian","app_version":"1.6.7","mathjax_version":"3.2.2","native_math":true,"compatibility_dfrac":true,"local_images":true,"verified":true}),
        );
        fs::write(root.join("outside-sentinel.txt"), b"human boundary bytes").unwrap();
        Self {
            output: root.join("new-vault"),
            root,
            case,
            api,
            profile,
        }
    }
    pub fn activate(&self, s: &Server) {
        let mut cfg = read(&self.api);
        cfg["endpoint"] = json!(s.endpoint);
        cfg["budget"]["max_requests"] = json!(1);
        write(&self.api, &cfg);
    }
    pub fn repin(&self) {
        let mut v = read(self.case.join("case.json"));
        for f in ["pdf", "page", "region", "semantics"] {
            if !v[f].is_null() {
                let bytes = fs::read(self.case.join(v[f]["file"].as_str().unwrap())).unwrap();
                use sha2::{Digest, Sha256};
                v[f]["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
            }
        }
        write(self.case.join("case.json"), &v);
    }
    pub fn casefold_collision(&self) {
        self.casefold_collision_names("page.PNG", "page.png");
    }
    pub fn casefold_collision_names(&self, page_name: &str, region_name: &str) {
        let mut v = read(self.case.join("case.json"));
        fs::rename(self.case.join("page.png"), self.case.join(page_name)).unwrap();
        // A case-sensitive host can hold both distinct PNGs. Windows exposes
        // the two references as aliases, which must fail the same early schema gate.
        #[cfg(not(windows))]
        fs::rename(self.case.join("region.png"), self.case.join(region_name)).unwrap();
        v["page"]["file"] = json!(page_name);
        v["region"]["file"] = json!(region_name);
        write(self.case.join("case.json"), &v);
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub struct Server {
    pub endpoint: String,
    pub accepted: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Vec<u8>>>,
    stopping: Arc<AtomicBool>,
}
impl Server {
    pub fn new(value: Value) -> Self {
        Self::http(
            200,
            serde_json::to_vec(&value).unwrap(),
            None,
            Duration::ZERO,
        )
    }
    pub fn http(status: u16, body: Vec<u8>, declared: Option<usize>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let accepted = Arc::new(AtomicBool::new(false));
        let observed = accepted.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let worker = thread::spawn(move || {
            let end = Instant::now() + Duration::from_secs(5);
            let mut stream: TcpStream = loop {
                if stop.load(Ordering::SeqCst) {
                    return vec![];
                }
                if let Ok((s, _)) = listener.accept() {
                    break s;
                }
                assert!(Instant::now() < end, "expected one artificial HTTP request");
                thread::sleep(Duration::from_millis(2));
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut raw = vec![];
            let (header_end, content_length) = loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&b[..n]);
                if let Some(at) = raw.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&raw[..at]);
                    let size = header
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (at + 4, size);
                }
                assert!(raw.len() < 65536);
            };
            while raw.len() < header_end + content_length {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&b[..n]);
            }
            assert!(raw.starts_with(b"POST /v1/chat/completions HTTP/1.1\r\n"));
            observed.store(true, Ordering::SeqCst);
            let header = format!(
                "HTTP/1.1 {status} Artificial\r\nx-request-id: artificial-http-request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                declared.unwrap_or(body.len())
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            thread::sleep(delay);
            drop(stream);
            assert!(
                listener.accept().is_err(),
                "retry or redirect unexpectedly opened another connection"
            );
            raw[header_end..header_end + content_length].to_vec()
        });
        Self {
            endpoint,
            accepted,
            worker: Some(worker),
            stopping,
        }
    }
    pub fn finish(mut self) -> Vec<u8> {
        let body = self.worker.take().unwrap().join().unwrap();
        assert!(
            self.accepted.load(Ordering::SeqCst),
            "one actual artificial POST must be observed"
        );
        body
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
