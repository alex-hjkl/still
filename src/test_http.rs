//! Small scripted HTTP server shared by tests of the Navidrome adapter and callers.
use reqwest::Url;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type RequestCheck = Box<dyn FnOnce(&Url) + Send>;

pub struct Response {
    path: String,
    query: Vec<(String, String)>,
    body: String,
    check: Option<RequestCheck>,
    before_reply: Option<Box<dyn FnOnce() + Send>>,
}

impl Response {
    pub fn subsonic(method: &str, body: Value) -> Self {
        let mut response = json!({"status": "ok", "version": "1.16.1"});
        for (key, value) in body.as_object().expect("fixture response object") {
            response[key] = value.clone();
        }
        Self {
            path: format!("/rest/{method}.view"),
            query: Vec::new(),
            body: json!({"subsonic-response": response}).to_string(),
            check: None,
            before_reply: None,
        }
    }

    pub fn prefix(mut self, prefix: &str) -> Self {
        self.path = format!("{prefix}{}", self.path);
        self
    }

    pub fn query(mut self, key: &str, value: impl ToString) -> Self {
        self.query.push((key.to_owned(), value.to_string()));
        self
    }

    pub fn check(mut self, check: impl FnOnce(&Url) + Send + 'static) -> Self {
        self.check = Some(Box::new(check));
        self
    }

    pub fn before_reply(mut self, action: impl FnOnce() + Send + 'static) -> Self {
        self.before_reply = Some(Box::new(action));
        self
    }

    fn serve(self, mut socket: TcpStream) {
        // Accepted sockets may inherit the listener's nonblocking mode.
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        let mut request = first.split_whitespace();
        assert_eq!(request.next(), Some("GET"));
        let url = Url::parse(&format!("http://localhost{}", request.next().unwrap())).unwrap();
        assert_eq!(url.path(), self.path);
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        for (key, expected) in self.query {
            assert_eq!(query.get(&key), Some(&expected), "query parameter {key}");
        }
        if let Some(check) = self.check {
            check(&url);
        }
        loop {
            let mut header = String::new();
            assert_ne!(
                reader.read_line(&mut header).unwrap(),
                0,
                "incomplete request"
            );
            if header.trim().is_empty() {
                break;
            }
        }
        if let Some(action) = self.before_reply {
            action();
        }
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", self.body.len(), self.body).unwrap();
    }
}

pub struct Server {
    pub url: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(responses: impl IntoIterator<Item = Response>) -> Self {
        let responses: Vec<_> = responses.into_iter().collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if stopping.load(Ordering::Relaxed) {
                        return;
                    }
                    match listener.accept() {
                        Ok((socket, _)) => {
                            response.serve(socket);
                            break;
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "expected HTTP request did not arrive"
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("fixture accept failed: {error}"),
                    }
                }
            }
        });
        Self {
            url,
            stop,
            worker: Some(worker),
        }
    }

    pub fn finish(mut self) {
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("HTTP fixture failed");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.expect("HTTP fixture failed");
            }
        }
    }
}
