//! "Flat maps": a map with a flat ground, without light, shadows, tile colours, water or objects,
//! as chosen. Players use such maps for a clearer view or weak computers; the rules follow GRF
//! Editor's flat maps maker (`GRFEditor/Tools/Map/MapEditor.cs`).
//!
//! Differences to GRF Editor, on purpose:
//! - Versions are kept (GRF Editor lowers GND to 1.7 and RSW to 1.9); our writers handle all.
//! - Water is moved below the lowest ground point, in the RSW and in the GND (from GND 1.8 the
//!   client reads it from there).
//! - Shadows from the GAT are computed per cell and then merged into shared lightmaps, so large
//!   maps don't run out of the 65 536 lightmap numbers before merging.
//! - Its custom cell textures ("textures from GAT types"), quadrant shadows and gutter lines are
//!   not offered.
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::gat::Gat;
use crate::gnd_map::{self, Ground};
use crate::rsw::{Light, Object, Rsw};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlatOptions {
    /// Ground and walk grid at height 0, no walls; also removes objects and water.
    pub flatten: bool,
    /// Coloured light of the lightmaps.
    pub remove_light: bool,
    /// Brightness (shadows) of the lightmaps.
    pub remove_shadow: bool,
    /// Tile colours become white.
    pub remove_color: bool,
    /// New shadows from the walk grid: blocked cells dark, snipable cells with a dot.
    pub gat_shadows: bool,
    /// The map's sun and ambient light set to GRF Editor's defaults.
    pub reset_light: bool,
    /// Every object: models, lights, sounds and effects.
    pub remove_objects: bool,
    /// Only these kinds of objects.
    pub remove_models: bool,
    pub remove_sounds: bool,
    pub remove_effects: bool,
    pub remove_water: bool,
}

impl FlatOptions {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FlatError {
    #[error("GND: {0}")]
    Ground(String),
    #[error("RSW: {0}")]
    World(String),
    #[error("GAT: {0}")]
    Altitude(String),
    #[error("shadows from the walk grid need the map's GAT")]
    NeedsGat,
    #[error("the map would need more than 65 536 different lightmaps")]
    TooManyLightmaps,
}

/// The rewritten files; `None` where a file was not given or did not change.
#[derive(Debug, Default, PartialEq)]
pub struct FlatMap {
    pub gnd: Option<Vec<u8>>,
    pub rsw: Option<Vec<u8>>,
    pub gat: Option<Vec<u8>>,
}

const LM: usize = 256;
const SHADOW_BLOCKED: u8 = 100;
const SHADOW_SNIPABLE: u8 = 50;

fn white_lightmap() -> [u8; LM] {
    let mut l = [0u8; LM];
    l[..64].fill(255);
    l
}

/// Paths of the world, ground and altitude files of the map `base` (path without extension, e.g.
/// `data\\prontera`): the ground and altitude the world file names, else the ones with the same
/// name as the world.
pub fn map_paths(base: &[u8], rsw: Option<&[u8]>) -> [Vec<u8>; 3] {
    let world = rsw.and_then(|d| Rsw::parse(d).ok());
    let named = |field: Option<&[u8; 40]>, ext: &[u8]| {
        let t: Vec<u8> = field
            .map(|f| f.iter().take_while(|&&b| b != 0).copied().collect())
            .unwrap_or_default();
        if t.is_empty() {
            [base, ext].concat()
        } else {
            [b"data\\".as_slice(), &t].concat()
        }
    };
    [
        [base, b".rsw"].concat(),
        named(world.as_ref().map(|w| &w.ground), b".gnd"),
        named(world.as_ref().map(|w| &w.altitude), b".gat"),
    ]
}

/// One map by its path without extension: reads the world, ground and altitude through `read`
/// (archive path to bytes, `None` when missing), applies `o` and returns the changed files with
/// their paths. The inner error says why a map was left out (no ground, damaged files); the
/// outer one is `read` failing.
#[allow(clippy::type_complexity)]
pub fn apply_map<E>(
    base: &[u8],
    read: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, E>,
    o: &FlatOptions,
) -> Result<Result<Vec<(Vec<u8>, Vec<u8>)>, String>, E> {
    let latin1 = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    let rsw = read(&[base, b".rsw"].concat())?;
    let [rsw_path, gnd_path, gat_path] = map_paths(base, rsw.as_deref());
    let Some(gnd) = read(&gnd_path)? else {
        return Ok(Err(format!("no {}", latin1(&gnd_path))));
    };
    let gat = read(&gat_path)?;
    Ok(apply(&gnd, rsw.as_deref(), gat.as_deref(), o)
        .map(|r| {
            [(rsw_path, r.rsw), (gnd_path, r.gnd), (gat_path, r.gat)]
                .into_iter()
                .filter_map(|(p, d)| d.map(|d| (p, d)))
                .collect()
        })
        .map_err(|e| e.to_string()))
}

/// Applies `o` to one map. `gnd` is required; the RSW and GAT are rewritten when given.
pub fn apply(
    gnd: &[u8],
    rsw: Option<&[u8]>,
    gat: Option<&[u8]>,
    o: &FlatOptions,
) -> Result<FlatMap, FlatError> {
    let mut g = gnd_map::parse(gnd).map_err(|e| FlatError::Ground(e.to_string()))?;
    let mut w = rsw
        .map(Rsw::parse)
        .transpose()
        .map_err(|e| FlatError::World(e.to_string()))?;
    let mut a = gat
        .map(Gat::parse)
        .transpose()
        .map_err(|e| FlatError::Altitude(e.to_string()))?;
    if o.gat_shadows && a.is_none() {
        return Err(FlatError::NeedsGat);
    }

    if o.flatten {
        for c in &mut g.cubes {
            c.height = [0.0; 4];
            c.front = -1;
            c.right = -1;
        }
        if let Some(a) = &mut a {
            for c in &mut a.cells {
                c.heights = [0.0; 4];
            }
        }
    }
    if o.gat_shadows {
        shadows_from_gat(&mut g, a.as_ref().expect("checked above"), o.flatten)?;
    } else if o.remove_light && o.remove_shadow {
        g.lightmaps = white_lightmap().to_vec();
        g.lightmap_count = 1;
        for t in &mut g.tiles {
            t.lightmap = 0;
        }
    } else if o.remove_light || o.remove_shadow {
        for l in g.lightmaps.as_chunks_mut::<LM>().0 {
            if o.remove_light {
                l[64..].fill(0);
            }
            if o.remove_shadow {
                l[..64].fill(255);
            }
        }
        merge_lightmaps(&mut g)?;
    }
    if o.remove_color || o.gat_shadows {
        for t in &mut g.tiles {
            t.color = [255; 4];
        }
    }

    let water = o.remove_water || o.flatten;
    let below = g.cubes.iter().flat_map(|c| c.height).fold(0.0f32, f32::max) + 100.0;
    if water {
        set_gnd_water_level(&mut g, below);
    }
    if let Some(w) = &mut w {
        if o.reset_light {
            w.light = w.light.as_ref().map(|_| Light {
                longitude: 45,
                latitude: 45,
                diffuse: [1.0; 3],
                ambient: [1.0; 3],
                intensity: 0.5,
            });
        }
        if o.remove_objects || o.flatten {
            w.objects.clear();
        }
        w.objects.retain(|ob| match ob {
            Object::Model(_) => !o.remove_models,
            Object::Sound(_) => !o.remove_sounds,
            Object::Effect(_) => !o.remove_effects,
            Object::Light(_) => true,
        });
        if water && let Some(wt) = &mut w.water {
            wt.level = below;
        }
        if o.flatten {
            let wave = w.water.as_ref().map(|wt| wt.level - wt.wave_height);
            recompute_quad_tree(&mut w.tail, &g, a.as_ref(), wave);
        }
    }

    let changed = |new: Vec<u8>, old: &[u8]| (new != old).then_some(new);
    Ok(FlatMap {
        gnd: changed(g.to_bytes(), gnd),
        rsw: w.and_then(|w| changed(w.to_bytes(), rsw.unwrap_or_default())),
        gat: a.and_then(|a| changed(a.to_bytes(), gat.unwrap_or_default())),
    })
}

/// Nodes of the quad tree after the objects of an RSW (from 2.1): 6 levels, 1 + 4 + ... + 1024.
const QUAD_NODES: usize = 1365;
const QUAD_NODE: usize = 48;

/// Height range (lowest, highest raw value) of the ground below a leaf of the quad tree, from the node's bytes.
type LeafRange<'a> = &'a dyn Fn(&[u8], usize) -> (f32, f32);

/// The client finds the cell under the mouse through the bounding boxes of the RSW quad tree. When the ground moves (flat
/// maps), boxes that still hold the old heights miss the new ground: those cells cannot be clicked and show the "no walk"
/// cursor although they are walkable. So the heights of every box are computed again from the ground, the walk grid and
/// the water, like BrowEdit does (`Rsw::recalculateQuadtree`); the horizontal extent stays. Only for a map without models
/// (flattening removes them), whose boxes would be missing here. A tail of another size is left as it is.
fn recompute_quad_tree(tail: &mut [u8], g: &Ground, a: Option<&Gat>, wave: Option<f32>) {
    if tail.len() != QUAD_NODES * QUAD_NODE || g.width == 0 || g.height == 0 {
        return;
    }
    let cube_range = |x: usize, y: usize| {
        let c = &g.cubes[y * g.width + x];
        let mut lo = c.height.iter().copied().fold(f32::MAX, f32::min);
        let mut hi = c.height.iter().copied().fold(f32::MIN, f32::max);
        if let Some(a) = a {
            for k in 0..4 {
                let (gx, gy) = (x * 2 + k % 2, y * 2 + k / 2);
                if let Some(cell) = (gx < a.width as usize)
                    .then(|| a.cells.get(gy * a.width as usize + gx))
                    .flatten()
                {
                    lo = cell.heights.iter().copied().fold(lo, f32::min);
                    hi = cell.heights.iter().copied().fold(hi, f32::max);
                }
            }
        }
        if let Some(wv) = wave
            && c.up > -1
            && c.height.iter().all(|&h| h > wv)
        {
            lo = lo.min(wv);
            hi = hi.max(wv);
        }
        (lo, hi)
    };
    let f = |t: &[u8], at: usize| f32::from_le_bytes(t[at..at + 4].try_into().expect("4 bytes"));
    let zoom = if g.zoom > 0.0 { g.zoom } else { 10.0 };
    // pre-order: a node, then its four children; leaves are on level 5
    fn walk(tail: &mut [u8], at: &mut usize, level: u32, leaf: LeafRange) -> (f32, f32) {
        let me = *at;
        *at += 1;
        let (lo, hi) = if level >= 5 {
            leaf(tail, me * QUAD_NODE)
        } else {
            let mut r = (f32::MAX, f32::MIN);
            for _ in 0..4 {
                let c = walk(tail, at, level + 1, leaf);
                r = (r.0.min(c.0), r.1.max(c.1));
            }
            r
        };
        // max (x, y, z), min (x, y, z), half size, centre
        let o = me * QUAD_NODE;
        tail[o + 4..o + 8].copy_from_slice(&hi.to_le_bytes());
        tail[o + 16..o + 20].copy_from_slice(&lo.to_le_bytes());
        let rd = |i: usize| f32::from_le_bytes(tail[o + i..o + i + 4].try_into().expect("4 bytes"));
        let (max, min) = ([rd(0), rd(4), rd(8)], [rd(12), rd(16), rd(20)]);
        for i in 0..3 {
            let half = (max[i] - min[i]) / 2.0;
            tail[o + 24 + i * 4..o + 28 + i * 4].copy_from_slice(&half.to_le_bytes());
            tail[o + 36 + i * 4..o + 40 + i * 4].copy_from_slice(&(max[i] - half).to_le_bytes());
        }
        (lo, hi)
    }
    let (w, h) = (g.width as i64, g.height as i64);
    let leaf = |t: &[u8], o: usize| {
        let (max_x, max_z, min_x, min_z) = (f(t, o), f(t, o + 8), f(t, o + 12), f(t, o + 20));
        let mut r = (f32::MAX, f32::MIN);
        for x in (w / 2 + (min_x / zoom).floor() as i64)..(w / 2 + (max_x / zoom).ceil() as i64) {
            for y in (h / 2 + (min_z / zoom).floor() as i64)..(h / 2 + (max_z / zoom).ceil() as i64)
            {
                if (0..w).contains(&x) && (0..h).contains(&y) {
                    let c = cube_range(x as usize, y as usize);
                    r = (r.0.min(c.0), r.1.max(c.1));
                }
            }
        }
        if r.0 > r.1 { (0.0, 0.0) } else { r }
    };
    let mut at = 0;
    walk(tail, &mut at, 0, &leaf);
}

/// Water stored after the cubes (GND 1.8 and later): a default water, the zone grid size, then
/// per zone the full water (1.9) or only its level (1.8). Every level is set to `level`.
fn set_gnd_water_level(g: &mut Ground, level: f32) {
    if g.tail.len() < 32 {
        return;
    }
    let int = |b: &[u8], at: usize| i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    let zones = usize::try_from(int(&g.tail, 24))
        .unwrap_or(0)
        .saturating_mul(usize::try_from(int(&g.tail, 28)).unwrap_or(0));
    let step = if g.version >= crate::version::Version::new(1, 9) {
        24
    } else {
        4
    };
    if g.tail.len() != 32 + zones * step {
        return;
    }
    let mut at = vec![0];
    at.extend((0..zones).map(|z| 32 + z * step));
    for p in at {
        g.tail[p..p + 4].copy_from_slice(&level.to_le_bytes());
    }
}

/// Every top, right and front surface gets its own white lightmap; blocked walk cells darken a
/// quarter of their cube's top lightmap, snipable ones get a dot (GRF Editor's
/// `_gndMatchShadowWithGatCells`, including its handling of the lightmap borders). Equal
/// lightmaps are merged at the end.
fn shadows_from_gat(g: &mut Ground, gat: &Gat, flat: bool) -> Result<(), FlatError> {
    let (w, h) = (g.width, g.height);
    let mut top: Vec<Option<[u8; LM]>> = vec![None; g.cubes.len()];
    let mut tiles = Vec::new();
    // walls keep lightmap 0 (white); tops get their own one, numbered after merging
    let mut top_tile: Vec<Option<usize>> = vec![None; g.cubes.len()];
    for (i, c) in g.cubes.iter_mut().enumerate() {
        if flat {
            c.front = -1;
            c.right = -1;
        }
        for (k, t) in [&mut c.up, &mut c.right, &mut c.front]
            .into_iter()
            .enumerate()
        {
            let Some(src) = usize::try_from(*t).ok().and_then(|t| g.tiles.get(t)) else {
                continue;
            };
            let mut copy = src.clone();
            copy.color = [255; 4];
            copy.lightmap = 0;
            *t = tiles.len() as i32;
            if k == 0 {
                top[i] = Some(white_lightmap());
                top_tile[i] = Some(tiles.len());
            }
            tiles.push(copy);
        }
    }
    g.tiles = tiles;

    let mut shade = |xx: i64, yy: i64, v: u8| {
        if xx < 0 || yy < 0 || xx >= (w * 8) as i64 || yy >= (h * 8) as i64 {
            return;
        }
        let (cx, cy) = (xx as usize / 8, yy as usize / 8);
        if let Some(l) = &mut top[cy * w + cx] {
            l[(yy as usize % 8) * 8 + xx as usize % 8] = v;
        }
    };
    for y in 0..gat.height as i64 {
        for x in 0..gat.width as i64 {
            let kind = gat.cells[(y as usize) * gat.width as usize + x as usize].kind;
            match kind {
                1 | 2 | 4 => {
                    for sx in 0..4i64 {
                        for sy in 0..4i64 {
                            let (mut xx, mut yy) = (x * 4 + sx, y * 4 + sy);
                            let subx = sx + if x % 2 == 0 { 0 } else { 4 };
                            let suby = sy + if y % 2 == 0 { 0 } else { 4 };
                            if subx <= 1 || subx >= 6 {
                                let extra = if subx <= 1 { -2 } else { 2 };
                                if subx == 0 {
                                    xx += 1;
                                } else if subx == 7 {
                                    xx -= 1;
                                }
                                if suby == 0 {
                                    yy += 1;
                                } else if suby == 7 {
                                    yy -= 1;
                                }
                                shade(xx + extra, yy, SHADOW_BLOCKED);
                                if suby <= 1 {
                                    shade(xx, yy - 2, SHADOW_BLOCKED);
                                    shade(xx + extra, yy - 2, SHADOW_BLOCKED);
                                } else if suby >= 6 {
                                    shade(xx, yy + 2, SHADOW_BLOCKED);
                                    shade(xx + extra, yy + 2, SHADOW_BLOCKED);
                                }
                            } else if suby <= 1 || suby >= 6 {
                                if suby == 0 {
                                    yy += 1;
                                } else if suby == 7 {
                                    yy -= 1;
                                }
                                shade(xx, yy + if suby <= 1 { -2 } else { 2 }, SHADOW_BLOCKED);
                            }
                            shade(xx, yy, SHADOW_BLOCKED);
                        }
                    }
                }
                5 => {
                    let xx = x * 4 + if x % 2 == 0 { 2 } else { 1 };
                    let yy = y * 4 + if y % 2 == 0 { 2 } else { 1 };
                    shade(xx, yy, SHADOW_SNIPABLE);
                }
                _ => {}
            }
        }
    }

    let mut lightmaps: Vec<[u8; LM]> = vec![white_lightmap()];
    let mut seen: HashMap<[u8; LM], u16> = HashMap::from([(white_lightmap(), 0)]);
    for (i, l) in top.into_iter().enumerate() {
        let (Some(l), Some(t)) = (l, top_tile[i]) else {
            continue;
        };
        let n = match seen.get(&l) {
            Some(&n) => n,
            None => {
                let n = u16::try_from(lightmaps.len()).map_err(|_| FlatError::TooManyLightmaps)?;
                seen.insert(l, n);
                lightmaps.push(l);
                n
            }
        };
        g.tiles[t].lightmap = n;
    }
    g.lightmap_count = lightmaps.len();
    g.lightmaps = lightmaps.concat();
    Ok(())
}

/// Merges equal lightmaps and drops unused ones.
fn merge_lightmaps(g: &mut Ground) -> Result<(), FlatError> {
    let old: Vec<&[u8]> = g
        .lightmaps
        .as_chunks::<LM>()
        .0
        .iter()
        .map(|l| &l[..])
        .collect();
    let mut seen: HashMap<&[u8], u16> = HashMap::new();
    let mut out: Vec<u8> = Vec::new();
    let mut map: HashMap<u16, u16> = HashMap::new();
    for t in &mut g.tiles {
        let Some(l) = old.get(usize::from(t.lightmap)) else {
            continue;
        };
        let n = match map.get(&t.lightmap) {
            Some(&n) => n,
            None => {
                let n = match seen.get(l) {
                    Some(&n) => n,
                    None => {
                        let n = u16::try_from(out.len() / LM)
                            .map_err(|_| FlatError::TooManyLightmaps)?;
                        seen.insert(l, n);
                        out.extend_from_slice(l);
                        n
                    }
                };
                map.insert(t.lightmap, n);
                n
            }
        };
        t.lightmap = n;
    }
    if out.is_empty() {
        out = white_lightmap().to_vec();
    }
    g.lightmap_count = out.len() / LM;
    g.lightmaps = out;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2 x 2 cubes, one tile per surface, two lightmaps (one dark, one with colour).
    fn gnd() -> Vec<u8> {
        let mut b = b"GRGN\x01\x07".to_vec();
        for v in [2u32, 2] {
            b.extend(v.to_le_bytes());
        }
        b.extend(10.0f32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(80u32.to_le_bytes());
        let mut name = b"a.bmp".to_vec();
        name.resize(80, 0);
        b.extend(name);
        b.extend(2u32.to_le_bytes());
        for v in [8i32, 8, 1] {
            b.extend(v.to_le_bytes());
        }
        b.extend([40u8; 64]);
        b.extend([0u8; 192]);
        b.extend([255u8; 64]);
        b.extend([90u8; 192]);
        b.extend(12u32.to_le_bytes());
        for i in 0..12u16 {
            for v in [0.0f32, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0] {
                b.extend(v.to_le_bytes());
            }
            b.extend(0u16.to_le_bytes());
            b.extend((i % 2).to_le_bytes());
            b.extend([10, 20, 30, 255]);
        }
        for i in 0..4i32 {
            for v in [-5.0f32, -5.0, 3.0, 3.0] {
                b.extend(v.to_le_bytes());
            }
            for t in [i * 3, i * 3 + 1, i * 3 + 2] {
                b.extend(t.to_le_bytes());
            }
        }
        b
    }

    fn world_with(objects: Vec<Object>) -> Vec<u8> {
        Rsw {
            version: crate::version::Version::new(1, 9),
            build_number: 0,
            unknown_byte: 0,
            ini: [0; 40],
            ground: [0; 40],
            altitude: [0; 40],
            source: [0; 40],
            water: Some(crate::rsw::Water {
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
            objects,
            tail: vec![],
        }
        .to_bytes()
    }

    fn gat(kinds: [i32; 16]) -> Vec<u8> {
        let mut b = b"GRAT\x01\x02".to_vec();
        for v in [4i32, 4] {
            b.extend(v.to_le_bytes());
        }
        for k in kinds {
            for h in [-5.0f32, -5.0, 3.0, 3.0] {
                b.extend(h.to_le_bytes());
            }
            b.extend(k.to_le_bytes());
        }
        b
    }

    #[test]
    fn map_files_follow_the_names_in_the_world_file() {
        let mut w = Rsw::parse(&world_with(vec![])).unwrap();
        w.ground[..9].copy_from_slice(b"other.gnd");
        let p = map_paths(b"data\\x", Some(&w.to_bytes()));
        assert_eq!(p[0], b"data\\x.rsw");
        assert_eq!(p[1], b"data\\other.gnd");
        assert_eq!(p[2], b"data\\x.gat", "empty name: same as the world");
        assert_eq!(map_paths(b"data\\x", None)[1], b"data\\x.gnd");
    }

    #[test]
    fn nothing_chosen_changes_nothing() {
        let g = gnd();
        let r = apply(&g, None, Some(&gat([0; 16])), &FlatOptions::default()).unwrap();
        assert_eq!(r, FlatMap::default());
    }

    #[test]
    fn flatten_zeroes_heights_and_drops_walls() {
        let o = FlatOptions {
            flatten: true,
            ..Default::default()
        };
        let r = apply(&gnd(), None, Some(&gat([0; 16])), &o).unwrap();
        let g = gnd_map::parse(&r.gnd.unwrap()).unwrap();
        assert!(g.cubes.iter().all(|c| c.height == [0.0; 4]));
        assert!(g.cubes.iter().all(|c| c.front == -1 && c.right == -1));
        assert_eq!(g.cubes[1].up, 3);
        let a = Gat::parse(&r.gat.unwrap()).unwrap();
        assert!(a.cells.iter().all(|c| c.heights == [0.0; 4] && c.kind == 0));
    }

    /// A quad tree like the client's for a 2 x 2 ground, every box at the old height -30 (raised ground).
    fn quad_tree_at(y: f32) -> Vec<u8> {
        fn node(t: &mut Vec<u8>, x: f32, z: f32, w: f32, level: u32, y: f32) {
            let (max, min) = ([x + w, y, z + w], [x, y, z]);
            for v in max.iter().chain(&min) {
                t.extend(v.to_le_bytes());
            }
            for i in 0..3 {
                t.extend(((max[i] - min[i]) / 2.0).to_le_bytes());
            }
            for i in 0..3 {
                t.extend((max[i] - (max[i] - min[i]) / 2.0).to_le_bytes());
            }
            if level < 5 {
                for (dx, dz) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                    node(t, x + dx * w / 2.0, z + dz * w / 2.0, w / 2.0, level + 1, y);
                }
            }
        }
        let mut t = Vec::new();
        node(&mut t, -10.0, -10.0, 19.0, 0, y);
        t
    }

    #[test]
    fn flatten_moves_the_quad_tree_boxes_to_the_new_ground() {
        let mut w = Rsw::parse(&world_with(vec![])).unwrap();
        w.tail = quad_tree_at(-30.0);
        assert_eq!(w.tail.len(), QUAD_NODES * QUAD_NODE);
        let o = FlatOptions {
            flatten: true,
            ..Default::default()
        };
        let r = apply(&gnd(), Some(&w.to_bytes()), Some(&gat([0; 16])), &o).unwrap();
        let t = Rsw::parse(&r.rsw.unwrap()).unwrap().tail;
        let f = |i: usize| f32::from_le_bytes(t[i..i + 4].try_into().unwrap());
        for n in 0..QUAD_NODES {
            let o = n * QUAD_NODE;
            // the client misses a cell whose box does not reach the ground (0 after flattening)
            assert_eq!((f(o + 4), f(o + 16)), (0.0, 0.0), "node {n}: y range");
            assert_eq!(
                (f(o + 28), f(o + 40)),
                (0.0, 0.0),
                "node {n}: half size and centre in y"
            );
            assert_eq!(
                (f(o), f(o + 12)),
                (f(o + 36) + f(o + 24), f(o + 36) - f(o + 24)),
                "node {n}: x kept"
            );
        }
        // without flattening the tree is not touched
        let r = apply(
            &gnd(),
            Some(&w.to_bytes()),
            Some(&gat([0; 16])),
            &FlatOptions {
                remove_light: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            Rsw::parse(&r.rsw.unwrap_or_else(|| w.to_bytes()))
                .unwrap()
                .tail,
            quad_tree_at(-30.0)
        );
    }

    #[test]
    fn light_and_shadow_are_removed_separately_or_together() {
        let only_light = FlatOptions {
            remove_light: true,
            ..Default::default()
        };
        let g =
            gnd_map::parse(&apply(&gnd(), None, None, &only_light).unwrap().gnd.unwrap()).unwrap();
        assert_eq!(g.lightmap_count, 2);
        assert!(g.lightmaps[..64].iter().all(|&b| b == 40));
        assert!(g.lightmaps[64..256].iter().all(|&b| b == 0));

        let only_shadow = FlatOptions {
            remove_shadow: true,
            ..Default::default()
        };
        let g = gnd_map::parse(
            &apply(&gnd(), None, None, &only_shadow)
                .unwrap()
                .gnd
                .unwrap(),
        )
        .unwrap();
        assert!(g.lightmaps.chunks(256).all(|l| l[..64] == [255; 64]));
        assert_eq!(g.lightmaps[64..256], [0u8; 192]);

        let both = FlatOptions {
            remove_light: true,
            remove_shadow: true,
            remove_color: true,
            ..Default::default()
        };
        let g = gnd_map::parse(&apply(&gnd(), None, None, &both).unwrap().gnd.unwrap()).unwrap();
        assert_eq!(g.lightmap_count, 1);
        assert_eq!(g.lightmaps, white_lightmap());
        assert!(
            g.tiles
                .iter()
                .all(|t| t.lightmap == 0 && t.color == [255; 4])
        );
    }

    #[test]
    fn blocked_cells_cast_shadows_and_equal_lightmaps_are_shared() {
        let mut kinds = [0; 16];
        kinds[0] = 1; // south-west cell: blocked
        let o = FlatOptions {
            gat_shadows: true,
            ..Default::default()
        };
        let r = apply(&gnd(), None, Some(&gat(kinds)), &o).unwrap();
        let g = gnd_map::parse(&r.gnd.unwrap()).unwrap();
        // the white one plus the shadowed top of cube 0
        assert_eq!(g.lightmap_count, 2);
        let top0 = &g.tiles[g.cubes[0].up as usize];
        assert_eq!(top0.lightmap, 1);
        let l = &g.lightmaps[256..512];
        assert_eq!(l[8 + 1], SHADOW_BLOCKED);
        assert_eq!(l[7 * 8 + 7], 255);
        assert!(
            g.cubes[1..]
                .iter()
                .all(|c| g.tiles[c.up as usize].lightmap == 0)
        );
        assert!(g.tiles.iter().all(|t| t.color == [255; 4]));
        assert_eq!(
            apply(&gnd(), None, None, &o),
            Err(FlatError::NeedsGat),
            "needs the walk grid"
        );
    }

    #[test]
    fn only_the_chosen_kinds_of_objects_are_removed() {
        use crate::rsw::{LightObject, SoundObject};
        let world = world_with(vec![
            Object::Light(LightObject {
                name: [0; 80],
                position: [0.0; 3],
                color: [1.0; 3],
                range: 5.0,
            }),
            Object::Sound(SoundObject {
                name: [0; 80],
                wave_name: [0; 80],
                position: [0.0; 3],
                volume: 1.0,
                width: 1,
                height: 1,
                range: 1.0,
                cycle: 1.0,
            }),
        ]);
        let o = FlatOptions {
            remove_sounds: true,
            ..Default::default()
        };
        let r = apply(&gnd(), Some(&world), None, &o).unwrap();
        assert_eq!(r.gnd, None);
        let w = Rsw::parse(&r.rsw.unwrap()).unwrap();
        assert!(matches!(w.objects[..], [Object::Light(_)]));
    }

    #[test]
    fn water_goes_below_the_lowest_ground_in_the_gnd_tail() {
        let mut f = gnd();
        f[4..6].copy_from_slice(&[1, 9]);
        f.extend(50.0f32.to_le_bytes());
        f.extend(3i32.to_le_bytes());
        for v in [1.0f32, 2.0, 50.0] {
            f.extend(v.to_le_bytes());
        }
        f.extend(3i32.to_le_bytes());
        f.extend(1i32.to_le_bytes());
        f.extend(1i32.to_le_bytes());
        f.extend(f[f.len() - 32..f.len() - 8].to_vec());
        let o = FlatOptions {
            remove_water: true,
            ..Default::default()
        };
        let g = gnd_map::parse(&apply(&f, None, None, &o).unwrap().gnd.unwrap()).unwrap();
        let level = |at: usize| f32::from_le_bytes(g.tail[at..at + 4].try_into().unwrap());
        assert_eq!((level(0), level(32)), (103.0, 103.0));
        assert_eq!(g.tail[4..8], 3i32.to_le_bytes());
    }
}
