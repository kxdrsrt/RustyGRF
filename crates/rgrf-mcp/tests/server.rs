//! The server driven in-process: protocol behaviour, every tool, and the write boundaries.

use std::path::{Path, PathBuf};

use rgrf_core::build::{BuildOptions, build};
use rgrf_core::progress::{CancelToken, Control};
use rgrf_mcp::policy::Policy;
use rgrf_mcp::server::Server;
use serde_json::{Value, json};

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rgrf-mcp-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}

fn put(root: &Path, name: &str, data: &[u8]) {
    let p = root.join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

fn make_grf(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let src = dir.join(format!("{name}-src"));
    for (n, d) in files {
        put(&src, n, d);
    }
    let out = dir.join(name);
    build(&src, &out, &BuildOptions::default(), &Control::default()).unwrap();
    out
}

fn server(read: &[PathBuf], write: &[PathBuf]) -> Server {
    Server::new(Policy::new(read, write).unwrap())
}

fn rpc(s: &Server, method: &str, params: Value) -> Value {
    let msg = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    s.handle(&msg, &CancelToken::new(), &|_| {})
        .expect("a request gets a reply")
}

fn call(s: &Server, tool: &str, args: Value) -> Value {
    let r = rpc(s, "tools/call", json!({"name": tool, "arguments": args}));
    r["result"].clone()
}

fn ok(s: &Server, tool: &str, args: Value) -> Value {
    let r = call(s, tool, args);
    assert_eq!(r["isError"], false, "{tool}: {r}");
    r["structuredContent"].clone()
}

fn fails(s: &Server, tool: &str, args: Value) -> String {
    let r = call(s, tool, args);
    assert_eq!(r["isError"], true, "{tool} should fail: {r}");
    r["content"][0]["text"].as_str().unwrap().to_string()
}

fn sample(dir: &Path) -> PathBuf {
    make_grf(
        dir,
        "sample.grf",
        &[
            ("data/hello.txt", b"hello world"),
            ("data/Sub/notes.txt", b"line one\nline two\n"),
            ("data/korean.txt", &[0xC7, 0xD1, 0xB1, 0xDB]),
            ("data/blob.bin", &[0, 1, 2, 3, 0, 0, 9]),
            ("data/sprite/a.spr", b"SP\x01\x02rest"),
            ("data/sprite/a.act", b"AC\x01\x02rest"),
        ],
    )
}

#[test]
fn lifecycle_negotiation_errors_and_notifications() {
    let s = server(&[], &[]);
    let r = rpc(&s, "initialize", json!({"protocolVersion": "2025-03-26"}));
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert!(r["result"]["capabilities"]["tools"].is_object());
    assert_eq!(r["result"]["serverInfo"]["name"], "rgrf-mcp");
    let r = rpc(&s, "initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");

    assert_eq!(rpc(&s, "ping", json!({}))["result"], json!({}));
    assert_eq!(
        rpc(&s, "resources/list", json!({}))["error"]["code"],
        -32601
    );

    let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    assert!(s.handle(&note, &CancelToken::new(), &|_| {}).is_none());
    let bad = s.handle(&json!([1]), &CancelToken::new(), &|_| {}).unwrap();
    assert_eq!(bad["error"]["code"], -32600);

    let r = rpc(&s, "tools/call", json!({"name": "nope"}));
    assert_eq!(r["error"]["code"], -32602);
    let msg = fails(&s, "grf_info", json!({"path": "x", "bogus": 1}));
    assert!(msg.contains("unknown argument"), "{msg}");
    let msg = fails(&s, "grf_info", json!({}));
    assert!(msg.contains("missing argument 'path'"), "{msg}");
}

#[test]
fn read_only_by_default_and_writing_tools_only_with_a_root() {
    let names = |s: &Server| -> Vec<String> {
        rpc(s, "tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    let dir = tmp("list");
    let ro = names(&server(&[], &[]));
    assert_eq!(ro.len(), 9);
    assert!(
        ro.iter()
            .all(|n| !n.contains("build") && !n.contains("merge"))
    );
    let rw = server(&[], std::slice::from_ref(&dir));
    assert_eq!(names(&rw).len(), 15);

    // a write tool is not just hidden but refused
    let ro = server(&[], &[]);
    let r = rpc(
        &ro,
        "tools/call",
        json!({"name": "grf_build", "arguments": {}}),
    );
    assert_eq!(r["error"]["code"], -32602);

    // every tool has a schema with a description for each property
    for t in rpc(&rw, "tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
    {
        let props = t["inputSchema"]["properties"].as_object().unwrap();
        assert!(!props.is_empty());
        assert!(props.values().all(|p| p["description"].is_string()), "{t}");
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn info_list_search_and_paging() {
    let dir = tmp("read");
    let g = sample(&dir);
    let s = server(&[], &[]);
    let p = g.to_str().unwrap();

    let v = ok(&s, "grf_info", json!({"path": p, "codecs": true}));
    assert_eq!(
        (v["version"].clone(), v["files"].clone()),
        ("0x200".into(), 6.into())
    );
    assert_eq!(v["codecs"]["zlib"], 6);

    let v = ok(&s, "grf_list", json!({"path": p}));
    assert_eq!(
        (v["total"].clone(), v["truncated"].clone()),
        (6.into(), false.into())
    );
    let a = ok(&s, "grf_list", json!({"path": p, "limit": 4}));
    assert_eq!(
        (a["returned"].clone(), a["next_offset"].clone()),
        (4.into(), 4.into())
    );
    let b = ok(&s, "grf_list", json!({"path": p, "limit": 4, "offset": 4}));
    assert_eq!(
        (b["returned"].clone(), b["truncated"].clone()),
        (2.into(), false.into())
    );
    // search by substring (case-insensitive, / = \), glob and extension
    let v = ok(&s, "grf_list", json!({"path": p, "pattern": "data/sub"}));
    assert_eq!(v["total"], 1);
    let v = ok(&s, "grf_list", json!({"path": p, "pattern": "*.txt"}));
    assert_eq!(v["total"], 3);
    let v = ok(&s, "grf_list", json!({"path": p, "ext": ["SPR", ".act"]}));
    assert_eq!(v["total"], 2);

    let msg = fails(
        &s,
        "grf_info",
        json!({"path": dir.join("missing.grf").to_str().unwrap()}),
    );
    assert!(msg.contains("missing.grf"), "{msg}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reading_text_binary_and_summaries() {
    let dir = tmp("text");
    let g = sample(&dir);
    let s = server(&[], &[]);
    let p = g.to_str().unwrap();

    let v = ok(
        &s,
        "grf_read_text",
        json!({"path": p, "entry": "DATA/Hello.TXT"}),
    );
    assert_eq!(
        (v["text"].clone(), v["encoding"].clone()),
        ("hello world".into(), "utf8".into())
    );
    let v = ok(
        &s,
        "grf_read_text",
        json!({"path": p, "entry": "data\\korean.txt"}),
    );
    assert_eq!(
        (v["text"].clone(), v["encoding"].clone()),
        ("한글".into(), "cp949".into())
    );
    let v = ok(
        &s,
        "grf_read_text",
        json!({"path": p, "entry": "data/hello.txt", "max_bytes": 5}),
    );
    assert_eq!(
        (v["text"].clone(), v["truncated"].clone()),
        ("hello".into(), true.into())
    );
    let msg = fails(
        &s,
        "grf_read_text",
        json!({"path": p, "entry": "data/blob.bin"}),
    );
    assert!(msg.contains("binary"), "{msg}");
    let msg = fails(
        &s,
        "grf_read_text",
        json!({"path": p, "entry": "data/none.txt"}),
    );
    assert!(msg.contains("no entry named"), "{msg}");

    let v = ok(
        &s,
        "grf_read_summary",
        json!({"path": p, "entry": "data/sprite/a.spr"}),
    );
    assert_eq!(v["type"], "SPR sprite");
    assert_eq!(v["decoded_size"], 8);
    assert_eq!(v["preview_hex"], "5350010272657374");
    assert_eq!(v["crc32"].as_str().unwrap().len(), 8);
    std::fs::remove_dir_all(dir).unwrap();
}

fn tiny_gat() -> Vec<u8> {
    let mut b = b"GRAT\x01\x02".to_vec();
    b.extend(2i32.to_le_bytes());
    b.extend(1i32.to_le_bytes());
    for kind in [0i32, 1] {
        for h in [0.0f32, 1.0, 2.0, 3.0] {
            b.extend(h.to_le_bytes());
        }
        b.extend(kind.to_le_bytes());
    }
    b
}

#[test]
fn inspect_reads_the_format_and_lua_decompile_refuses_plain_text() {
    let dir = tmp("inspect");
    let p = make_grf(
        &dir,
        "fmt.grf",
        &[
            ("data/map.gat", &tiny_gat()),
            ("data/pal.pal", &[0u8; 1024]),
            ("data/a.lub", b"print('hi')"),
            ("data/a.bin", &[1, 2, 3]),
        ],
    );
    let (p, s) = (p.to_str().unwrap(), server(&[], &[]));
    let v = ok(
        &s,
        "grf_inspect",
        json!({"path": p, "entry": "data/map.gat"}),
    );
    assert_eq!(
        (v["kind"].clone(), v["width"].clone(), v["height"].clone()),
        ("gat".into(), 2.into(), 1.into())
    );
    assert_eq!(v["types"].as_array().unwrap().len(), 2);
    let v = ok(
        &s,
        "grf_inspect",
        json!({"path": p, "entry": "data/pal.pal"}),
    );
    assert_eq!(v["colors"], 256);
    let msg = fails(&s, "grf_inspect", json!({"path": p, "entry": "data/a.bin"}));
    assert!(msg.contains("no format reader"), "{msg}");
    // a map is resolved from its world file; here there is only the altitude file
    let msg = fails(&s, "grf_map_files", json!({"path": p, "map": "map"}));
    assert!(msg.contains("not in the archive"), "{msg}");
    let msg = fails(
        &s,
        "grf_lua_decompile",
        json!({"path": p, "entry": "data/a.lub"}),
    );
    assert!(msg.contains("not compiled Lua"), "{msg}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn verify_and_diff() {
    let dir = tmp("vd");
    let a = sample(&dir);
    let b = make_grf(
        &dir,
        "other.grf",
        &[("data/hello.txt", b"HELLO world"), ("data/new.txt", b"new")],
    );
    let s = server(&[], &[]);
    let v = ok(&s, "grf_verify", json!({"path": a.to_str().unwrap()}));
    assert_eq!(
        (v["ok"].clone(), v["errors"].clone(), v["decoded"].clone()),
        (true.into(), 0.into(), 6.into())
    );

    let v = ok(
        &s,
        "grf_diff",
        json!({"a": a.to_str().unwrap(), "b": b.to_str().unwrap(), "limit": 2}),
    );
    assert_eq!(v["identical"], false);
    assert_eq!(v["only_in_a"]["count"], 5);
    assert_eq!(v["only_in_a"]["truncated"], true);
    assert_eq!(v["only_in_a"]["paths"].as_array().unwrap().len(), 2);
    assert_eq!(v["only_in_b"]["paths"], json!(["data\\new.txt"]));
    assert_eq!(v["different_content"]["paths"], json!(["data\\hello.txt"]));
    let v = ok(
        &s,
        "grf_diff",
        json!({"a": a.to_str().unwrap(), "b": a.to_str().unwrap()}),
    );
    assert_eq!(v["identical"], true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn writing_tools_work_inside_the_root_and_refuse_everything_else() {
    let base = tmp("write");
    let (allowed, elsewhere) = (base.join("out"), base.join("elsewhere"));
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let g = sample(&base);
    let s = server(&[], std::slice::from_ref(&allowed));
    let out = |n: &str| allowed.join(n).to_str().unwrap().to_string();
    let p = g.to_str().unwrap();

    // extract
    let dest = out("files");
    let v = ok(
        &s,
        "grf_extract",
        json!({"path": p, "dest": dest, "pattern": "*.txt"}),
    );
    assert_eq!(
        (v["selected"].clone(), v["written"].clone()),
        (3.into(), 3.into())
    );
    assert_eq!(
        std::fs::read(allowed.join("files/data/hello.txt")).unwrap(),
        b"hello world"
    );
    let msg = fails(
        &s,
        "grf_extract",
        json!({"path": p, "dest": dest, "pattern": "hello"}),
    );
    assert!(
        msg.contains("exists"),
        "an existing file stops the extraction: {msg}"
    );
    let v = ok(
        &s,
        "grf_extract",
        json!({"path": p, "dest": dest, "pattern": "hello", "on_collision": "skip"}),
    );
    assert_eq!(v["skipped_collisions"], 1);

    // build from a folder, then the other tools on the result
    let v = ok(
        &s,
        "grf_build",
        json!({"source_dir": dest, "output": out("built.grf")}),
    );
    assert_eq!(
        (v["files"].clone(), v["verified"].clone()),
        (3.into(), true.into())
    );
    let v = ok(
        &s,
        "grf_build",
        json!({"source_dir": dest, "output": out("lzma.grf"), "method": "lzma", "dedupe": true}),
    );
    assert_eq!(v["format"], "0x200");
    let v = ok(
        &s,
        "grf_repack",
        json!({"path": p, "output": out("re.grf"), "method": "lzma", "dedupe": "content"}),
    );
    assert_eq!(
        (v["files"].clone(), v["verified"].clone()),
        (6.into(), true.into())
    );

    // merge: the first archive wins
    let custom = make_grf(&base, "custom.grf", &[("data/hello.txt", b"custom hello")]);
    let v = ok(
        &s,
        "grf_merge",
        json!({"paths": [custom.to_str().unwrap(), p], "output": out("merged.grf")}),
    );
    assert_eq!(v["files"], 6);
    assert_eq!(v["inputs"][1]["replaced_by_higher_priority"], 1);
    let v = ok(
        &s,
        "grf_read_text",
        json!({"path": out("merged.grf"), "entry": "data/hello.txt"}),
    );
    assert_eq!(v["text"], "custom hello");

    // edit: replace from a file, add text, remove; the source stays unchanged
    let new_hello = allowed.join("new-hello.txt");
    std::fs::write(&new_hello, b"edited hello").unwrap();
    let v = ok(
        &s,
        "grf_edit",
        json!({
            "path": p, "output": out("edited.grf"),
            "put": [format!("/data/hello.txt={}", new_hello.display())],
            "put_text": ["data/new/한글.txt=안녕"],
            "text_encoding": "cp949", "names": "cp949",
            "remove": ["data/sub/NOTES.txt"],
        }),
    );
    assert_eq!(
        (
            v["replaced"].clone(),
            v["added"].clone(),
            v["removed"].clone(),
            v["files"].clone(),
            v["verified"].clone()
        ),
        (1.into(), 1.into(), 1.into(), 6.into(), true.into())
    );
    let v2 = ok(
        &s,
        "grf_read_text",
        json!({"path": out("edited.grf"), "entry": "data/hello.txt"}),
    );
    assert_eq!(v2["text"], "edited hello");
    let v2 = ok(
        &s,
        "grf_read_text",
        json!({"path": out("edited.grf"), "entry": "data/new/한글.txt", "names": "cp949"}),
    );
    assert_eq!(v2["text"], "안녕");
    let msg = fails(
        &s,
        "grf_edit",
        json!({"path": p, "output": out("none.grf")}),
    );
    assert!(msg.contains("nothing to do"), "{msg}");
    let msg = fails(
        &s,
        "grf_edit",
        json!({"path": p, "output": out("edited.grf"), "remove": ["data/hello.txt"]}),
    );
    assert!(msg.contains("already exists"), "{msg}");
    let msg = fails(
        &s,
        "grf_edit",
        json!({"path": p, "output": out("bad.grf"), "remove": ["data/../x"]}),
    );
    assert!(msg.contains("not a usable archive path"), "{msg}");

    // patch: only what differs
    let v = ok(
        &s,
        "grf_patch",
        json!({"old": p, "new": out("merged.grf"), "output": out("patch.grf")}),
    );
    assert_eq!(
        (v["files"].clone(), v["changed"].clone(), v["added"].clone()),
        (1.into(), 1.into(), 0.into())
    );

    // boundaries
    let msg = fails(
        &s,
        "grf_build",
        json!({"source_dir": dest, "output": elsewhere.join("x.grf").to_str().unwrap()}),
    );
    assert!(msg.contains("outside"), "{msg}");
    let dots = format!("{}/../elsewhere/x.grf", allowed.display());
    let msg = fails(&s, "grf_build", json!({"source_dir": dest, "output": dots}));
    assert!(msg.contains("'..'"), "{msg}");
    let msg = fails(
        &s,
        "grf_build",
        json!({"source_dir": dest, "output": out("built.grf")}),
    );
    assert!(msg.contains("already exists"), "{msg}");
    ok(
        &s,
        "grf_build",
        json!({"source_dir": dest, "output": out("built.grf"), "overwrite": true}),
    );
    let msg = fails(
        &s,
        "grf_extract",
        json!({"path": p, "dest": elsewhere.to_str().unwrap()}),
    );
    assert!(msg.contains("outside"), "{msg}");
    // an archive is never written over one of its inputs, even with overwrite
    let msg = fails(
        &s,
        "grf_repack",
        json!({"path": out("re.grf"), "output": out("re.grf"), "overwrite": true}),
    );
    assert!(msg.contains("must not be one of the input"), "{msg}");
    let msg = fails(
        &s,
        "grf_merge",
        json!({"paths": [out("re.grf"), p], "output": out("re.grf"), "overwrite": true}),
    );
    assert!(msg.contains("must not be one of the input"), "{msg}");
    assert!(!elsewhere.join("x.grf").exists());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn dry_run_writes_nothing_and_read_roots_restrict_reading() {
    let base = tmp("dry");
    let (allowed, secret) = (base.join("out"), base.join("secret"));
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    let g = sample(&secret);
    let open = server(&[], std::slice::from_ref(&allowed));
    let target = allowed.join("plan.grf");
    let v = ok(
        &open,
        "grf_repack",
        json!({"path": g.to_str().unwrap(), "output": target.to_str().unwrap(), "dry_run": true}),
    );
    assert_eq!(v["dry_run"], true);
    assert!(!target.exists());

    let restricted = server(
        std::slice::from_ref(&allowed),
        std::slice::from_ref(&allowed),
    );
    let msg = fails(
        &restricted,
        "grf_info",
        json!({"path": g.to_str().unwrap()}),
    );
    assert!(msg.contains("not allowed"), "{msg}");
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_cancelled_call_gets_no_reply() {
    let dir = tmp("cancel");
    let g = sample(&dir);
    let s = server(&[], &[]);
    let cancel = CancelToken::new();
    cancel.cancel();
    let msg = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": {"name": "grf_verify", "arguments": {"path": g.to_str().unwrap()}}});
    assert!(s.handle(&msg, &cancel, &|_| {}).is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
