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
    assert_eq!(ro.len(), 16);
    assert!(
        ro.iter()
            .all(|n| !n.contains("build") && !n.contains("merge"))
    );
    let rw = server(&[], std::slice::from_ref(&dir));
    assert_eq!(names(&rw).len(), 34);

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

#[test]
fn image_reduce_writes_8_bit_bmps_and_dry_run_only_reports() {
    use rgrf_formats::spr::RgbaImage;
    let dir = tmp("image-reduce");
    // 200 colours at random: lossless as 8 bits and smaller stored
    let mut pixels = Vec::new();
    for i in 0..128u64 * 128 {
        let mut r = i + 1;
        for _ in 0..3 {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
        }
        let r = (r % 200) as u8;
        pixels.extend([r, r / 2, 255 - r, 255]);
    }
    let bmp = rgrf_formats::bmp::write_rgb(&RgbaImage {
        width: 128,
        height: 128,
        pixels,
    });
    let p = make_grf(
        &dir,
        "pics.grf",
        &[("data/a.bmp", &bmp), ("data/b.txt", b"text")],
    );
    let s = server(&[], std::slice::from_ref(&dir));
    let out = dir.join("small.grf").display().to_string();
    let v = ok(
        &s,
        "grf_image_reduce",
        json!({"path": p, "output": out, "dry_run": true}),
    );
    assert_eq!(
        (
            v["matched"].clone(),
            v["reduced"].clone(),
            v["written"].clone()
        ),
        (1.into(), 1.into(), false.into())
    );
    assert!(!dir.join("small.grf").exists());
    let v = ok(&s, "grf_image_reduce", json!({"path": p, "output": out}));
    assert_eq!(
        (
            v["reduced"].clone(),
            v["lossless"].clone(),
            v["verified"].clone()
        ),
        (1.into(), 1.into(), true.into())
    );
    assert!(v["stored_bytes_after"].as_u64() < v["stored_bytes_before"].as_u64());
    let v = ok(
        &s,
        "grf_inspect",
        json!({"path": out, "entry": "data/a.bmp"}),
    );
    assert_eq!(
        (v["kind"].clone(), v["bits"].clone(), v["colors"].clone()),
        ("bmp".into(), 8.into(), 200.into())
    );
}

#[test]
fn client_refs_needs_all_archives_and_counts_per_kind() {
    use rgrf_formats::act::{Act, Action, Frame, Layer, V2_5};
    let dir = tmp("client-refs");
    let act = Act {
        version: V2_5,
        reserved: [0; 10],
        actions: vec![Action {
            frames: vec![Frame {
                layers: vec![Layer::default()],
                ..Frame::default()
            }],
            interval: 4.0,
        }],
        sounds: vec![],
        trailing: vec![],
        warnings: vec![],
    }
    .to_bytes(V2_5)
    .unwrap();
    let a = make_grf(&dir, "acts.grf", &[("data/sprite/x.act", &act)]);
    let s = server(std::slice::from_ref(&dir), &[]);
    // the sprite is missing here
    let v = ok(&s, "grf_client_refs", json!({"paths": [a]}));
    assert_eq!(v["animations_checked"], 1);
    assert_eq!(v["by_kind"]["missing_sprite"], 1);
    assert_eq!(v["animations_without_sprite_by_folder"][0]["count"], 1);
    // ... and found when the archive that has it is given too
    let b = make_grf(
        &dir,
        "sprites.grf",
        &[("data/sprite/x.spr", b"not a sprite")],
    );
    let v = ok(&s, "grf_client_refs", json!({"paths": [a, b]}));
    assert_eq!(v["by_kind"]["unreadable_sprite"], 1);
    assert!(v["by_kind"]["missing_sprite"].is_null());
    assert!(fails(&s, "grf_client_refs", json!({"paths": []})).contains("paths"));
    // the Lua tables of an archive are read too: a map the table names and the archive lacks
    let t = make_grf(
        &dir,
        "tables.grf",
        &[
            (
                "data/luafiles514/lua files/agit/agitconfig.lub",
                b"tbl_AgitInfo = { [\"here.gat\"] = {}, [\"gone.gat\"] = {} }",
            ),
            ("data/here.gat", b"x"),
        ],
    );
    let v = ok(&s, "grf_client_refs", json!({"paths": [t], "only": "lua"}));
    assert_eq!(v["tables_checked"], 1);
    assert_eq!(v["table_by_kind"]["missing_map"], 1, "{v}");
    assert_eq!(v["table_problems"][0]["missing"], "data\\gone.gat");

    // unused: a texture that a Lua file names and one nothing names
    let u = make_grf(
        &dir,
        "unused.grf",
        &[
            ("data/texture/town/used.bmp", b"x"),
            ("data/texture/town/lonely.bmp", b"y"),
            ("data/luafiles514/t.lua", b"pic = \"town\\\\Used.bmp\""),
        ],
    );
    let v = ok(&s, "grf_client_unused", json!({"paths": [u]}));
    assert_eq!(v["files_looked_at"], 2);
    assert_eq!(v["unnamed_files"], 1);
    assert_eq!(v["folders"][0]["names_are_composed"], false);
    assert!(fails(&s, "grf_client_unused", json!({"paths": []})).contains("paths"));
}

#[test]
fn flat_maps_writes_only_the_changed_map_files() {
    use rgrf_formats::rsw::{Light, Rsw, Water};
    use rgrf_formats::version::Version;
    let dir = tmp("flat-maps");
    let name = |s: &str| {
        let mut b = [0u8; 40];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    };
    let rsw = Rsw {
        version: Version::new(1, 9),
        build_number: 0,
        unknown_byte: 0,
        ini: [0; 40],
        ground: name("m.gnd"),
        altitude: name("m.gat"),
        source: [0; 40],
        water: Some(Water {
            level: 0.0,
            kind: 0,
            wave_height: 1.0,
            wave_speed: 2.0,
            wave_pitch: 50.0,
            texture_cycling: 3,
        }),
        light: Some(Light {
            longitude: 45,
            latitude: 45,
            diffuse: [1.0; 3],
            ambient: [0.3; 3],
            intensity: 0.5,
        }),
        bounds: Some([0; 4]),
        extra_ints: vec![],
        objects: vec![],
        tail: vec![],
    }
    .to_bytes();
    // 1 x 1 cube standing 4 units high, one lightmap, one tile
    let mut gnd = b"GRGN\x01\x07".to_vec();
    for v in [1u32, 1, 10.0f32.to_bits(), 1, 80] {
        gnd.extend(v.to_le_bytes());
    }
    gnd.extend([0u8; 80]);
    for v in [1i32, 8, 8, 1] {
        gnd.extend(v.to_le_bytes());
    }
    gnd.extend([128u8; 256]);
    gnd.extend(1u32.to_le_bytes());
    gnd.extend([0u8; 40]);
    for v in [-4.0f32; 4] {
        gnd.extend(v.to_le_bytes());
    }
    for t in [0i32, -1, -1] {
        gnd.extend(t.to_le_bytes());
    }
    let p = make_grf(
        &dir,
        "maps.grf",
        &[
            ("data/m.rsw", &rsw),
            ("data/m.gnd", &gnd),
            ("data/x.txt", b"x"),
        ],
    );
    let s = server(&[], std::slice::from_ref(&dir));
    let out = dir.join("flat.grf").display().to_string();
    let v = ok(
        &s,
        "grf_flat_maps",
        json!({"path": p, "output": out, "dry_run": true}),
    );
    assert_eq!(
        (v["changed_maps"].clone(), v["written"].clone()),
        (1.into(), false.into())
    );
    // defaults: flat ground, no light, no shadows; the world's water goes below the ground
    let v = ok(&s, "grf_flat_maps", json!({"path": p, "output": out}));
    assert_eq!(
        (v["files"].clone(), v["written"].clone()),
        (2.into(), true.into())
    );
    let v = ok(&s, "grf_list", json!({"path": out}));
    assert_eq!(v["total"], 2, "{v}");
    let msg = fails(
        &s,
        "grf_flat_maps",
        json!({"path": p, "output": dir.join("y.grf").display().to_string(), "maps": ["nowhere"]}),
    );
    assert!(msg.contains("no matching"), "{msg}");
}

#[test]
fn sounds_are_read_and_edited_as_wav_and_mp3() {
    use rgrf_formats::audio::Audio;
    let dir = tmp("audio");
    let tone = Audio {
        rate: 22050,
        channels: vec![(0..22050).map(|i| (i as f32 * 0.1).sin() * 0.5).collect()],
    };
    let wav = tone.to_wav();
    put(&dir, "s.wav", &wav);
    let grf = make_grf(&dir, "snd.grf", &[("data/wav/s.wav", &wav)]);
    let s = server(std::slice::from_ref(&dir), std::slice::from_ref(&dir));
    let loose = dir.join("s.wav").display().to_string();
    let v = ok(&s, "grf_audio_info", json!({"path": loose}));
    assert_eq!(
        (
            v["format"].clone(),
            v["duration_ms"].clone(),
            v["channels"].clone()
        ),
        ("wav".into(), 1000.into(), 1.into())
    );
    // the same sound inside an archive
    let v = ok(
        &s,
        "grf_audio_info",
        json!({"path": grf.display().to_string(), "entry": "data\\wav\\s.wav"}),
    );
    assert_eq!(v["sample_rate"], 22050);

    // trim, then write as MP3, then edit the MP3 (it stays an MP3)
    let (a, b) = (dir.join("a.mp3"), dir.join("b.mp3"));
    let v = ok(
        &s,
        "grf_audio_edit",
        json!({"path": loose, "ops": ["trim:0-800"], "to": "mp3", "kbps": 64, "output": a.display().to_string()}),
    );
    assert_eq!(v["format"], "mp3");
    let v = ok(
        &s,
        "grf_audio_edit",
        json!({"path": a.display().to_string(), "ops": ["trim:100-500", "gain:-3"], "output": b.display().to_string()}),
    );
    assert_eq!(v["format"], "mp3");
    assert!(
        (350..=450).contains(&v["duration_ms"].as_u64().unwrap()),
        "{v}"
    );
    let v = ok(
        &s,
        "grf_audio_info",
        json!({"path": b.display().to_string()}),
    );
    assert_eq!(v["format"], "mp3");

    // errors: nothing to do, bad operation, existing output, the input as output
    assert!(
        fails(
            &s,
            "grf_audio_edit",
            json!({"path": loose, "ops": [], "output": dir.join("x.wav").display().to_string()})
        )
        .contains("ops")
    );
    assert!(fails(&s, "grf_audio_edit", json!({"path": loose, "ops": ["wobble"], "output": dir.join("x.wav").display().to_string()})).contains("wobble"));
    assert!(
        fails(
            &s,
            "grf_audio_edit",
            json!({"path": loose, "ops": ["reverse"], "output": a.display().to_string()})
        )
        .contains("exists")
    );
    assert!(
        fails(
            &s,
            "grf_audio_edit",
            json!({"path": loose, "ops": ["reverse"], "output": loose, "overwrite": true})
        )
        .contains("input")
    );
}

#[test]
fn downgrade_maps_writes_the_old_world_and_ground_files() {
    use rgrf_formats::rsw::{Light, Rsw, Water};
    use rgrf_formats::version::Version;
    let dir = tmp("downgrade");
    let name = |s: &str| {
        let mut b = [0u8; 40];
        b[..s.len()].copy_from_slice(s.as_bytes());
        b
    };
    let rsw = Rsw {
        version: Version::new(1, 9),
        build_number: 0,
        unknown_byte: 0,
        ini: [0; 40],
        ground: name("m.gnd"),
        altitude: name("m.gat"),
        source: [0; 40],
        water: Some(Water {
            level: 0.0,
            kind: 0,
            wave_height: 1.0,
            wave_speed: 2.0,
            wave_pitch: 50.0,
            texture_cycling: 3,
        }),
        light: Some(Light {
            longitude: 45,
            latitude: 45,
            diffuse: [1.0; 3],
            ambient: [0.3; 3],
            intensity: 0.5,
        }),
        bounds: Some([0; 4]),
        extra_ints: vec![],
        objects: vec![],
        tail: vec![],
    }
    .to_bytes();
    let mut gnd = b"GRGN\x01\x07".to_vec();
    for v in [1u32, 1, 10.0f32.to_bits(), 1, 80] {
        gnd.extend(v.to_le_bytes());
    }
    gnd.extend([0u8; 80]);
    for v in [1i32, 8, 8, 1] {
        gnd.extend(v.to_le_bytes());
    }
    gnd.extend([128u8; 256]);
    gnd.extend(1u32.to_le_bytes());
    gnd.extend([0u8; 40]);
    for v in [-4.0f32; 4] {
        gnd.extend(v.to_le_bytes());
    }
    for t in [0i32, -1, -1] {
        gnd.extend(t.to_le_bytes());
    }
    let p = make_grf(
        &dir,
        "maps.grf",
        &[
            ("data/m.rsw", &rsw),
            ("data/m.gnd", &gnd),
            ("data/m.gat", b"GRAT"),
        ],
    );
    let s = server(&[], std::slice::from_ref(&dir));
    let out = dir.join("old.grf").display().to_string();
    // without names only maps with RSM2 models count: this one has none
    let v = ok(&s, "grf_downgrade_maps", json!({"path": p, "output": out}));
    assert_eq!(
        (v["changed_maps"].clone(), v["written"].clone()),
        (0.into(), false.into())
    );
    let v = ok(
        &s,
        "grf_downgrade_maps",
        json!({"path": p, "output": out, "maps": ["m"]}),
    );
    assert_eq!(
        (v["files"].clone(), v["written"].clone()),
        (2.into(), true.into()),
        "{v}"
    );
    let listed = ok(&s, "grf_list", json!({"path": out}));
    assert_eq!(listed["total"], 2, "{listed}");
    assert!(fails(&s, "grf_downgrade_maps", json!({"path": p, "output": dir.join("z.grf").display().to_string(), "maps": ["nowhere"]})).contains("no matching"));
}

#[test]
fn hash_lists_thor_patchers_and_rare_files_work_through_the_tools() {
    let dir = tmp("grfcl-tools");
    let client = dir.join("client");
    put(&client, "ro.exe", b"exe");
    put(&client, "data/a.txt", b"a");
    let s = server(std::slice::from_ref(&dir), std::slice::from_ref(&dir));
    let p = |n: &str| dir.join(n).display().to_string();
    let v = ok(
        &s,
        "grf_hashlist_create",
        json!({"folder": client.display().to_string(), "output": p("m.hash")}),
    );
    assert_eq!(v["files"], 2);
    put(&client, "data/a.txt", b"changed");
    put(&client, "extra.txt", b"e");
    ok(
        &s,
        "grf_hashlist_create",
        json!({"folder": client.display().to_string(), "output": p("u.hash")}),
    );
    let v = ok(
        &s,
        "grf_hashlist_compare",
        json!({"master": p("m.hash"), "user": p("u.hash")}),
    );
    assert_eq!(v["issues"], 2, "{v}");
    let issues: Vec<String> = v["differences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["issue"].as_str().unwrap().to_string())
        .collect();
    assert!(issues.contains(&"different".into()) && issues.contains(&"extra".into()));
    // an existing output is not replaced
    fails(
        &s,
        "grf_hashlist_create",
        json!({"folder": client.display().to_string(), "output": p("m.hash")}),
    );

    put(&dir, "cfg/config.ini", b"[CONFIG]\n");
    put(&dir, "cfg/LanguageMap.ini", b"x");
    put(&dir, "cfg/images/a.png", b"png");
    put(&dir, "patcher.exe", b"MZ");
    let v = ok(
        &s,
        "grf_thor_pack",
        json!({"program": p("patcher.exe"), "config_dir": p("cfg"), "folders": ["images"], "output": p("packed.exe")}),
    );
    assert_eq!(v["files"], 3);
    ok(
        &s,
        "grf_thor_unpack",
        json!({"packed": p("packed.exe"), "dest": p("out")}),
    );
    assert_eq!(std::fs::read(dir.join("out/images/a.png")).unwrap(), b"png");
    assert_eq!(
        std::fs::read(dir.join("out/config.ini")).unwrap(),
        b"[CONFIG]\n"
    );
    let e = fails(
        &s,
        "grf_thor_pack",
        json!({"program": p("patcher.exe"), "config_dir": p("cfg"), "folders": ["../x"], "output": p("p2.exe"), "overwrite": true}),
    );
    assert!(e.contains("not a folder"), "{e}");

    // a BMP as emblem and back, a BSON as JSON and back
    put(&dir, "e.bmp", b"BM0123456789");
    ok(
        &s,
        "grf_rare_convert",
        json!({"path": p("e.bmp"), "kind": "bmp_to_ebm", "output": p("e.ebm")}),
    );
    ok(
        &s,
        "grf_rare_convert",
        json!({"path": p("e.ebm"), "kind": "ebm_to_bmp", "output": p("e2.bmp")}),
    );
    assert_eq!(std::fs::read(dir.join("e2.bmp")).unwrap(), b"BM0123456789");
    let bson: Vec<u8> = vec![12, 0, 0, 0, 0x10, b'a', 0, 7, 0, 0, 0, 0];
    put(&dir, "a.bson", &bson);
    let v = ok(
        &s,
        "grf_rare_info",
        json!({"path": p("a.bson"), "kind": "bson"}),
    );
    assert!(v["json"].as_str().unwrap().contains("\"a\": 7"));
    ok(
        &s,
        "grf_rare_convert",
        json!({"path": p("a.bson"), "kind": "bson_to_json", "output": p("a.json")}),
    );
    ok(
        &s,
        "grf_rare_convert",
        json!({"path": p("a.json"), "kind": "json_to_bson", "output": p("b.bson")}),
    );
    assert_eq!(std::fs::read(dir.join("b.bson")).unwrap(), bson);
    let e = fails(
        &s,
        "grf_rare_info",
        json!({"path": p("e.bmp"), "kind": "imf"}),
    );
    assert!(e.contains("IMF"), "{e}");
    // the effect table needs no files
    let v = ok(&s, "grf_effect_str", json!({"effect": 30}));
    assert_eq!(v["effects"][0]["str"], "thunderstorm.str");
    let v = ok(&s, "grf_effect_str", json!({"str_file": "Stonecurse.str"}));
    assert_eq!(v["effects"].as_array().unwrap().len(), 2);
    assert_eq!(
        ok(&s, "grf_effect_str", json!({}))["effects"]
            .as_array()
            .unwrap()
            .len(),
        118
    );
    assert!(
        ok(&s, "grf_effect_str", json!({"effect": 1}))["effects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn data_ini_is_shown_and_edited_inside_the_write_folder_only() {
    let dir = tmp("dataini");
    let client = dir.join("client");
    put(
        &client,
        "data.ini",
        b"[Data]\r\n1=cdata.grf\r\n2=data.grf\r\n",
    );
    for f in ["cdata.grf", "data.grf", "odin.grf"] {
        put(&client, f, b"x");
    }
    let c = client.to_string_lossy().into_owned();
    let s = server(&[], std::slice::from_ref(&dir));

    let v = ok(&s, "grf_data_ini", json!({"path": c}));
    assert_eq!(v["archives"][0]["name"], "cdata.grf");
    assert_eq!(v["missing"], json!([]));

    let v = ok(
        &s,
        "grf_data_ini_edit",
        json!({"path": c, "action": "add", "archives": ["odin.grf", "gone.grf"], "place": "first", "dry_run": true}),
    );
    assert_eq!(v["changed"], true);
    assert_eq!(v["missing"], json!(["gone.grf"]));
    assert_eq!(
        std::fs::read(client.join("data.ini")).unwrap(),
        b"[Data]\r\n1=cdata.grf\r\n2=data.grf\r\n",
        "a dry run writes nothing"
    );

    let v = ok(
        &s,
        "grf_data_ini_edit",
        json!({"path": c, "action": "add", "archives": ["odin.grf"], "place": "first"}),
    );
    assert_eq!(v["archives"][0]["name"], "odin.grf");
    assert_eq!(
        std::fs::read(client.join("data.ini")).unwrap(),
        b"[Data]\r\n1=odin.grf\r\n2=cdata.grf\r\n3=data.grf\r\n"
    );
    assert_eq!(
        std::fs::read(client.join("data.ini.bak")).unwrap(),
        b"[Data]\r\n1=cdata.grf\r\n2=data.grf\r\n"
    );

    ok(
        &s,
        "grf_data_ini_edit",
        json!({"path": c, "action": "move", "archives": ["data.grf"], "place": "after", "relative_to": "odin.grf"}),
    );
    ok(
        &s,
        "grf_data_ini_edit",
        json!({"path": c, "action": "replace", "archives": ["cdata.grf"], "with": "odin.grf2"}),
    );
    let v = ok(&s, "grf_data_ini", json!({"path": c}));
    let order: Vec<&str> = v["archives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["odin.grf", "data.grf", "odin.grf2"]);
    ok(
        &s,
        "grf_data_ini_edit",
        json!({"path": c, "action": "remove", "archives": ["odin.grf2"]}),
    );

    assert!(
        fails(
            &s,
            "grf_data_ini_edit",
            json!({"path": c, "action": "move", "archives": ["data.grf"]})
        )
        .contains("place")
    );
    assert!(
        fails(
            &s,
            "grf_data_ini_edit",
            json!({"path": c, "action": "remove", "archives": ["nope.grf"]})
        )
        .contains("not in DATA.INI")
    );

    // outside the write folder: readable, not editable
    let other = tmp("dataini-other");
    put(&other, "DATA.INI", b"[Data]\n0=a.grf\n");
    let o = other.to_string_lossy().into_owned();
    ok(&s, "grf_data_ini", json!({"path": o}));
    assert!(
        fails(
            &s,
            "grf_data_ini_edit",
            json!({"path": o, "action": "normalize"})
        )
        .contains("outside")
    );
}
