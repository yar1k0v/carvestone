//! A temp store with files written as given, and a client that talks to the server over the real protocol.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use carvestone_engine::Repo;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A temp store with prefix `CS`, removed on drop.
pub struct Store(PathBuf);

impl Store {
    pub fn new() -> Store {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("stone-mcp-features-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let store = Store(dir);
        store.write("stone/config.yml", "project: t\ntask_prefix: CS\n");
        store
    }

    pub fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    pub fn repo(&self) -> Repo {
        Repo::open(&self.0).unwrap().with_author("test".into())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What a tool call answered: whether the client reads it as a failure, and its text.
pub struct Reply {
    pub failed: bool,
    pub text: String,
}

impl Reply {
    /// The text as JSON, for the tools that answer with a value.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.text).unwrap_or_else(|err| panic!("not JSON ({err}): {}", self.text))
    }
}

/// One JSON-RPC session with a server that runs in this process.
pub struct Client {
    writer: WriteHalf<DuplexStream>,
    reader: Lines<BufReader<ReadHalf<DuplexStream>>>,
    next_id: u64,
}

impl Client {
    /// Starts the server on `repo` and finishes the initialize handshake.
    pub async fn connect(repo: Repo) -> Client {
        let mut client = Client::start(repo);
        client.initialize().await;
        client
    }

    /// Starts the server and sends nothing yet.
    pub fn start(repo: Repo) -> Client {
        let (ours, theirs) = tokio::io::duplex(1 << 20);
        tokio::spawn(carvestone_mcp::serve_stream(repo, theirs));
        let (read, writer) = tokio::io::split(ours);
        Client {
            writer,
            reader: BufReader::new(read).lines(),
            next_id: 1,
        }
    }

    /// The `result` of the initialize request; the client then announces it is ready.
    pub async fn initialize(&mut self) -> Value {
        let params = json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "features", "version": "0"},
        });
        let result = self.request("initialize", params).await;
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        result
    }

    /// Sends one request and returns its `result`, or fails the test on a protocol error.
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        let line = self
            .reader
            .next_line()
            .await
            .unwrap()
            .expect("server closed the stream");
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], id, "{reply}");
        assert!(reply.get("error").is_none(), "protocol error: {reply}");
        reply["result"].clone()
    }

    /// Calls a tool and returns its single text block.
    pub async fn call(&mut self, tool: &str, arguments: Value) -> Reply {
        let result = self
            .request("tools/call", json!({"name": tool, "arguments": arguments}))
            .await;
        let blocks = result["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1, "{result}");
        Reply {
            failed: result["isError"] == true,
            text: blocks[0]["text"].as_str().unwrap().to_string(),
        }
    }

    async fn send(&mut self, message: Value) {
        self.writer.write_all(format!("{message}\n").as_bytes()).await.unwrap();
    }
}
