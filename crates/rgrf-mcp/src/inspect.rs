//! Format-aware tools: `grf_inspect` (what is inside a SPR, ACT, map or model file) and
//! `grf_lua_decompile` (compiled Lua 5.1 back to source). Both only read.

use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::act::Act;
use rgrf_formats::gat::{self, Gat};
use rgrf_formats::gnd::GndSummary;
use rgrf_formats::pal::Pal;
use rgrf_formats::rsw::{self, Rsw};
use rgrf_formats::spr::Spr;
use rgrf_formats::{luac, luadec, rsm_model, str_effect};
use serde_json::{Value, json};

use crate::params::Args;
use crate::tools::{Ctx, entry_data};

type Out = Result<Value, String>;

/// Names inside the client's files are Korean (CP949) text.
fn korean(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    GrfPath::new(b[..end].to_vec()).display(NameMode::Cp949)
}

/// Names the format readers return as Latin-1 text: the original bytes shown as Korean.
fn relatin(s: &str) -> String {
    match s.chars().map(u8::try_from).collect::<Result<Vec<u8>, _>>() {
        Ok(b) => korean(&b),
        Err(_) => s.to_string(),
    }
}

fn extension(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

pub fn inspect(ctx: &Ctx, a: &Args) -> Out {
    let (name, data) = entry_data(ctx, a)?;
    let kind = extension(&name);
    let bad =
        |what: &str, e: &dyn std::fmt::Display| format!("{name} is not a readable {what}: {e}");
    let mut v = match kind.as_str() {
        "spr" => {
            let s = Spr::parse(&data).map_err(|e| bad("SPR", &e))?;
            json!({
                "kind": "spr", "version": s.version.to_string(),
                "indexed_images": s.indexed.iter().map(|i| json!([i.width, i.height])).collect::<Vec<_>>(),
                "rgba_images": s.rgba.iter().map(|i| json!([i.width, i.height])).collect::<Vec<_>>(),
                "has_palette": s.palette.is_some(), "warnings": s.warnings,
            })
        }
        "act" => {
            let act = Act::parse(&data).map_err(|e| bad("ACT", &e))?;
            json!({
                "kind": "act", "version": act.version.to_string(),
                "actions": act.actions.iter().map(|x| json!({"frames": x.frames.len(), "interval_ms": (x.interval * 25.0).round()})).collect::<Vec<_>>(),
                "layers": act.layers().count(),
                "sounds": act.sounds.iter().map(|s| korean(s)).collect::<Vec<_>>(),
                "warnings": act.warnings,
            })
        }
        "pal" => {
            Pal::parse(&data).map_err(|e| bad("palette", &e))?;
            json!({"kind": "pal", "colors": 256})
        }
        "gat" => {
            let g = Gat::parse(&data).map_err(|e| bad("GAT", &e))?;
            let (lo, hi) = g.height_range();
            json!({
                "kind": "gat", "version": g.version.to_string(), "width": g.width, "height": g.height,
                "height_min": lo, "height_max": hi,
                "types": g.type_counts().iter().map(|(k, n)| json!({"type": k, "name": gat::type_name(*k), "cells": n})).collect::<Vec<_>>(),
            })
        }
        "gnd" => {
            let g = GndSummary::parse(&data).map_err(|e| bad("GND", &e))?;
            json!({
                "kind": "gnd", "version": g.version.to_string(), "width": g.width,
                "height": g.height, "cell_size": g.cell_size, "textures": g.texture_names().iter().map(|t| relatin(t)).collect::<Vec<_>>(),
            })
        }
        "rsw" => {
            let r = Rsw::parse(&data).map_err(|e| bad("RSW", &e))?;
            let (models, lights, sounds, effects) = r.object_counts();
            json!({
                "kind": "rsw", "version": r.version.to_string(),
                "ground": relatin(&rsw::text(&r.ground)), "altitude": relatin(&rsw::text(&r.altitude)),
                "models": models, "lights": lights, "sounds": sounds, "effects": effects,
                "model_files": r.model_files().iter().map(|t| relatin(t)).collect::<Vec<_>>(),
                "sound_files": r.sound_files().iter().map(|t| relatin(t)).collect::<Vec<_>>(),
                "water": r.water.as_ref().map(|w| json!({"level": w.level, "type": w.kind})),
            })
        }
        "rsm" | "rsm2" => {
            let m = rsm_model::parse(&data).map_err(|e| bad("RSM model", &e))?;
            let moving = |n: &rsm_model::Node| {
                n.rot_keys.len() > 1 || n.pos_keys.len() > 1 || n.scale_keys.len() > 1
            };
            json!({
                "kind": "rsm", "version": m.version.to_string(),
                "triangles": m.nodes.iter().map(|n| n.faces.len()).sum::<usize>(),
                "animation_loop_ms": rsm_model::duration_ms(&m).round(),
                "nodes": m.nodes.iter().map(|n| json!({
                    "name": korean(&n.name), "parent": korean(&n.parent),
                    "vertices": n.vertices.len(), "faces": n.faces.len(), "animated": moving(n),
                    "textures": n.textures.iter().map(|t| korean(t)).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        }
        "str" => {
            let s = str_effect::parse(&data).map_err(|e| bad("STR effect", &e))?;
            json!({
                "kind": "str", "version": s.version, "fps": s.fps, "last_frame": s.last_frame,
                "layers": s.layers.len(),
                "textures": str_effect::texture_names(&s).iter().map(|t| korean(t)).collect::<Vec<_>>(),
            })
        }
        "lub" | "lua" if data.starts_with(b"\x1bLua") => {
            let c = luac::parse(&data).map_err(|e| bad("compiled Lua", &e))?;
            json!({"kind": "lub", "compiled": true, "functions": count_functions(&c.main)})
        }
        _ => {
            return Err(format!(
                "no format reader for .{kind} files (supported: spr, act, pal, gat, gnd, rsw, rsm, rsm2, str, compiled lub); use grf_read_summary"
            ));
        }
    };
    v["entry"] = name.into();
    v["size"] = data.len().into();
    Ok(v)
}

fn count_functions(f: &luac::Function) -> usize {
    1 + f.protos.iter().map(count_functions).sum::<usize>()
}

pub fn lua_decompile(ctx: &Ctx, a: &Args) -> Out {
    let (name, data) = entry_data(ctx, a)?;
    if !data.starts_with(b"\x1bLua") {
        return Err(format!(
            "{name} is not compiled Lua (it is plain text; use grf_read_text)"
        ));
    }
    let chunk = luac::parse(&data).map_err(|e| format!("{name} cannot be read: {e}"))?;
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    let listing = a.flag("listing");
    let text = if listing {
        luac::disassemble(&chunk, &show)
    } else {
        luadec::decompile(&chunk, &show)
    };
    let complete = listing || luadec::is_complete(&text);
    let max = a.opt_u64("max_bytes").unwrap_or(65_536).clamp(1, 1 << 20) as usize;
    let truncated = text.len() > max;
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(json!({
        "entry": name, "size": data.len(), "listing": listing, "complete": complete,
        "truncated": truncated, "text": &text[..end],
    }))
}
