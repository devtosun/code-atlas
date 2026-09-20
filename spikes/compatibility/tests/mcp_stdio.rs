use std::{
    env,
    error::Error,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    sent: Vec<String>,
    received: Vec<String>,
}

impl Client {
    fn start() -> Result<Self, Box<dyn Error>> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codeatlas-compatibility"))
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take().ok_or("missing child stdin")?;
        let output = child.stdout.take().ok_or("missing child stdout")?;
        Ok(Self {
            child,
            input: Some(input),
            output: BufReader::new(output),
            sent: Vec::new(),
            received: Vec::new(),
        })
    }

    fn exchange(&mut self, frame: &Value) -> Result<Value, Box<dyn Error>> {
        let encoded = serde_json::to_string(&frame)?;
        let input = self.input.as_mut().ok_or("child stdin is closed")?;
        input.write_all(encoded.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        self.sent.push(encoded);

        let mut response = String::new();
        self.output.read_line(&mut response)?;
        if response.is_empty() {
            return Err("server closed stdout before a response".into());
        }
        let trimmed = response.trim_end().to_owned();
        self.received.push(trimmed.clone());
        Ok(serde_json::from_str(&trimmed)?)
    }

    fn notify(&mut self, frame: &Value) -> Result<(), Box<dyn Error>> {
        let encoded = serde_json::to_string(&frame)?;
        let input = self.input.as_mut().ok_or("child stdin is closed")?;
        input.write_all(encoded.as_bytes())?;
        input.write_all(b"\n")?;
        input.flush()?;
        self.sent.push(encoded);
        Ok(())
    }

    fn save_frames(&self, mode: &str) -> Result<(), Box<dyn Error>> {
        let Ok(directory) = env::var("COMPAT_ARTIFACT_DIR") else {
            return Ok(());
        };
        fs::create_dir_all(&directory)?;
        fs::write(
            Path::new(&directory).join(format!("mcp-{mode}-client.jsonl")),
            format!("{}\n", self.sent.join("\n")),
        )?;
        fs::write(
            Path::new(&directory).join(format!("mcp-{mode}-server.jsonl")),
            format!("{}\n", self.received.join("\n")),
        )?;
        Ok(())
    }

    fn finish(mut self) -> Result<(), Box<dyn Error>> {
        self.input.take();
        for _ in 0..100 {
            if let Some(status) = self.child.try_wait()? {
                if !status.success() {
                    return Err(format!("server exited with {status}").into());
                }
                return Ok(());
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.child.kill()?;
        Err("server did not exit within two seconds of EOF".into())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {
            "name": "codeatlas-compatibility-client",
            "version": "0.1.0"
        },
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

#[test]
fn modern_discovery_and_per_request_metadata() -> Result<(), Box<dyn Error>> {
    let mut client = Client::start()?;
    let discovery = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {"_meta": modern_meta()}
    }))?;
    assert_eq!(discovery["id"], 1);
    assert!(
        discovery["result"]["supportedVersions"]
            .as_array()
            .ok_or("missing supportedVersions")?
            .iter()
            .any(|version| version == "2026-07-28")
    );

    let listed = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {"_meta": modern_meta()}
    }))?;
    assert_eq!(listed["result"]["resultType"], "complete");
    assert_eq!(listed["result"]["tools"][0]["name"], "compatibility_status");

    let called = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "compatibility_status",
            "arguments": {},
            "_meta": modern_meta()
        }
    }))?;
    assert_eq!(called["result"]["resultType"], "complete");
    assert_eq!(called["result"]["structuredContent"]["status"], "ready");
    client.save_frames("modern")?;
    client.finish()
}

#[test]
fn legacy_initialize_lifecycle() -> Result<(), Box<dyn Error>> {
    let mut client = Client::start()?;
    let initialized = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "codeatlas-compatibility-client",
                "version": "0.1.0"
            }
        }
    }))?;
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    client.notify(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }))?;

    let listed = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }))?;
    assert!(listed["result"].get("resultType").is_none());
    assert_eq!(listed["result"]["tools"][0]["name"], "compatibility_status");

    let called = client.exchange(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "compatibility_status", "arguments": {}}
    }))?;
    assert!(called["result"].get("resultType").is_none());
    assert_eq!(called["result"]["structuredContent"]["status"], "ready");
    client.save_frames("legacy")?;
    client.finish()
}
