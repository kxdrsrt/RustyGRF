//! The real binary over pipes: framing, a clean exit on EOF, and cancelling a running call.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Session {
    fn start(args: &[&str]) -> Self {
        Self::start_with_env(args, &[])
    }

    fn start_with_env(args: &[&str], env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rgrf-mcp"))
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start rgrf-mcp");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, v: &Value) {
        let w = self.stdin.as_mut().unwrap();
        writeln!(w, "{v}").unwrap();
        w.flush().unwrap();
    }

    fn next(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.stdout.read_line(&mut line).unwrap() > 0,
            "server closed stdout"
        );
        assert!(!line[..line.len() - 1].contains('\n'));
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout must carry only JSON ({e}): {line}"))
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let v = self.next();
            if v.get("id") == Some(&json!(id)) {
                return v;
            }
        }
    }

    fn finish(mut self) -> i32 {
        drop(self.stdin.take());
        let start = Instant::now();
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s.code().unwrap_or(-1);
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "server did not exit at EOF"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rgrf-mcp-io-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

fn text(n: usize, seed: usize) -> Vec<u8> {
    (0..n)
        .map(|i| b'a' + ((i * 7 + seed + i / 13) % 23) as u8)
        .collect()
}

fn fill(dir: &Path, files: usize, size: usize) {
    for i in 0..files {
        let p = dir.join(format!("data/f{i}.txt"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text(size, i)).unwrap();
    }
}

#[test]
fn handshake_list_call_and_clean_exit_at_eof() {
    let mut s = Session::start(&[]);
    let r = s.request(1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
    assert_eq!(r["result"]["serverInfo"]["name"], "rgrf-mcp");
    s.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let r = s.request(2, "tools/list", json!({}));
    assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 14);
    let r = s.request(
        3,
        "tools/call",
        json!({"name": "grf_info", "arguments": {"path": "/does/not/exist.grf"}}),
    );
    assert_eq!(r["result"]["isError"], true);

    // garbage gets a parse error and the session goes on
    let w = s.stdin.as_mut().unwrap();
    writeln!(w, "this is not json").unwrap();
    w.flush().unwrap();
    assert_eq!(s.next()["error"]["code"], -32700);
    assert_eq!(s.request(4, "ping", json!({}))["result"], json!({}));
    assert_eq!(s.finish(), 0);
}

#[test]
fn a_running_call_can_be_cancelled_and_the_server_stays_usable() {
    let base = tmp("cancel");
    let (src, out) = (base.join("src"), base.join("out"));
    std::fs::create_dir_all(&out).unwrap();
    // 150 files with seconds of Zopfli work each: minutes in total if not stopped
    fill(&src, 150, 150_000);
    let allow = out.to_str().unwrap().to_string();
    let mut s = Session::start(&[
        "--allow-write",
        &allow,
        "--allow-read",
        base.to_str().unwrap(),
    ]);
    s.request(1, "initialize", json!({"protocolVersion": "2025-06-18"}));

    let target = out.join("slow.grf");
    s.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "grf_build",
        "arguments": {"source_dir": src.to_str().unwrap(), "output": target.to_str().unwrap(), "method": "zopfli", "level": 5},
    }}));
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    s.send(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 2}}),
    );
    // the cancelled call sends no reply; the next request is answered normally
    let pong = s.request(3, "ping", json!({}));
    assert_eq!(pong["result"], json!({}));
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "cancel took {:?}",
        started.elapsed()
    );
    assert!(!target.exists(), "no half-written archive is left");
    let leftovers: Vec<_> = std::fs::read_dir(&out).unwrap().collect();
    assert!(
        leftovers.is_empty(),
        "no temporary file is left: {leftovers:?}"
    );
    assert_eq!(s.finish(), 0);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn progress_is_reported_when_the_client_asks_for_it() {
    let base = tmp("progress");
    let (src, out) = (base.join("src"), base.join("out"));
    std::fs::create_dir_all(&out).unwrap();
    fill(&src, 40, 200_000);
    let allow = out.to_str().unwrap().to_string();
    let mut s = Session::start(&["--allow-write", &allow]);
    s.request(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    s.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "_meta": {"progressToken": "tok"},
        "name": "grf_build",
        "arguments": {"source_dir": src.to_str().unwrap(), "output": out.join("p.grf").to_str().unwrap(), "method": "lzma"},
    }}));
    let mut progress = 0;
    let reply = loop {
        let v = s.next();
        if v["method"] == "notifications/progress" {
            assert_eq!(v["params"]["progressToken"], "tok");
            progress += 1;
        } else if v["id"] == 2 {
            break v;
        }
    };
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert!(progress >= 1, "at least the final progress message arrives");
    assert!(out.join("p.grf").exists());
    assert_eq!(s.finish(), 0);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn single_files_are_encrypted_with_the_password_of_the_environment() {
    use rgrf_core::build::{BuildOptions, build};
    use rgrf_core::progress::Control;
    use rgrf_core::{Archive, GrfPath};
    let base = tmp("crypt");
    fill(&base.join("src"), 3, 3000);
    let plain = base.join("plain.grf");
    build(
        &base.join("src"),
        &plain,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let out = base.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let args = json!({"path": plain.to_str().unwrap(), "output": out.join("mix.grf").to_str().unwrap(), "encrypt_entries": ["f1"]});

    // without a password in the environment the tool refuses
    let mut s = Session::start(&[
        "--allow-write",
        out.to_str().unwrap(),
        "--allow-read",
        base.to_str().unwrap(),
    ]);
    s.request(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    let r = s.request(
        2,
        "tools/call",
        json!({"name": "grf_crypt", "arguments": args}),
    );
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("RGRF_PASSWORD")
    );
    assert_eq!(s.finish(), 0);

    let mut s = Session::start_with_env(
        &[
            "--allow-write",
            out.to_str().unwrap(),
            "--allow-read",
            base.to_str().unwrap(),
        ],
        &[("RGRF_PASSWORD", "geheim123")],
    );
    s.request(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    let r = s.request(
        2,
        "tools/call",
        json!({"name": "grf_crypt", "arguments": args}),
    );
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(r["result"]["structuredContent"]["encrypted_entries"], 1);
    assert_eq!(s.finish(), 0);

    let key = rgrf_core::grfcrypt::key_from_password("geheim123").unwrap();
    let g = Archive::open(out.join("mix.grf")).unwrap();
    assert_eq!(g.encryption().unwrap().encrypted_entries, 1);
    let g = Archive::open_with_key(out.join("mix.grf"), &key).unwrap();
    let f1 = g.find(&GrfPath::new(&b"data\\f1.txt"[..])).unwrap();
    assert!(g.entry_encrypted(f1));
    assert_eq!(g.read(f1).unwrap().data, text(3000, 1));
    let f0 = g.find(&GrfPath::new(&b"data\\f0.txt"[..])).unwrap();
    assert!(!g.entry_encrypted(f0));

    // the table too: the archive no longer opens without the key
    let mut s = Session::start_with_env(
        &[
            "--allow-write",
            out.to_str().unwrap(),
            "--allow-read",
            base.to_str().unwrap(),
        ],
        &[("RGRF_PASSWORD", "geheim123")],
    );
    s.request(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    let args = json!({"path": out.join("mix.grf").to_str().unwrap(), "output": out.join("table.grf").to_str().unwrap(), "encrypt_table": true});
    let r = s.request(
        2,
        "tools/call",
        json!({"name": "grf_crypt", "arguments": args}),
    );
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["file_table_encrypted"],
        true
    );
    assert_eq!(s.finish(), 0);
    assert!(Archive::open(out.join("table.grf")).is_err());
    let t = Archive::open_with_key(out.join("table.grf"), &key).unwrap();
    assert!(t.table_encrypted() && t.encryption().unwrap().encrypted_entries == 1);
}
