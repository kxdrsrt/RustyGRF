//! Downgrading maps for old clients (GRF Editor's "Downgrade" on a map): RSM2 models (versions 2.2
//! and 2.3) become RSM 1.4 models, the world file becomes version 2.1 with the water of the ground
//! file, the ground file becomes version 1.7.
//!
//! The models are written the way GRF Editor's "flat" conversion does: the static pose is baked
//! into the vertices, so **animations of the nodes are lost** (a flag that waves is a still
//! flag), and a hidden root node `__RSM2RSM1` with the scale (-1, -1, 1) turns the model around the
//! way the old client expects. The map's model entries get the new name (`.rsm`), a changed
//! position (the old client centres a model on its vertices, the new one does not) and a negative
//! x scale. All of this is read from GRF Editor's `RenamingService.DowngradeMap`, `Rsm2Converter`
//! and its renderer (used with permission, see `docs/05-licensing.md`).

use crate::gnd_map;
use crate::rsm::V2_2;
use crate::rsm_model::{self, Model};
use crate::rsw::{Object, Rsw, Water};
use crate::version::Version;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DowngradeError {
    #[error("model: {0}")]
    Model(String),
    #[error("world file: {0}")]
    World(String),
    #[error("ground file: {0}")]
    Ground(String),
    #[error("{0} is not an RSM2 model (version 2.2 or 2.3)")]
    NotRsm2(String),
}

/// The name of the hidden root node that turns an RSM2 model into an RSM1 one.
pub const ROOT: &[u8] = b"__RSM2RSM1";

/// A model as RSM 1.4 and the centre of its vertices, which the map objects need.
#[derive(Debug, Clone, PartialEq)]
pub struct Rsm1 {
    pub bytes: Vec<u8>,
    /// Centre (x, z) and lowest y of the baked vertices of the RSM2 model.
    pub center: [f32; 3],
}

fn fixed(out: &mut Vec<u8>, name: &[u8], len: usize) {
    let n = name.len().min(len - 1);
    out.extend(&name[..n]);
    out.extend(std::iter::repeat_n(0u8, len - n));
}

fn put_f32s(out: &mut Vec<u8>, v: &[f32]) {
    v.iter().for_each(|x| out.extend(x.to_le_bytes()));
}

fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend(v.to_le_bytes());
}

/// Converts the bytes of an RSM2 model (version 2.2 or 2.3) to RSM 1.4.
pub fn model_to_rsm1(bytes: &[u8]) -> Result<Rsm1, DowngradeError> {
    let m = rsm_model::parse(bytes).map_err(|e| DowngradeError::Model(e.to_string()))?;
    if m.version < V2_2 {
        return Err(DowngradeError::NotRsm2(format!("version {}", m.version)));
    }
    Ok(flatten(&m))
}

fn flatten(m: &Model) -> Rsm1 {
    let baked = rsm_model::baked_vertices(m);
    // box over all vertices, also unused ones (what GRF Editor's `VerticesBox` is for RSM2)
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in baked.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if lo[0] > hi[0] {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let center = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];

    // one texture list for the whole model, each node refers to it
    let mut textures: Vec<Vec<u8>> = Vec::new();
    let mut ids: Vec<Vec<i32>> = Vec::new();
    for n in &m.nodes {
        ids.push(
            n.textures
                .iter()
                .map(|t| {
                    let at = textures.iter().position(|x| x == t).unwrap_or_else(|| {
                        textures.push(t.clone());
                        textures.len() - 1
                    });
                    at as i32
                })
                .collect(),
        );
    }

    let mut o = Vec::new();
    o.extend(b"GRSM");
    o.extend([1, 4]);
    // the animation length is in milliseconds before 2.2 and counts frames from there on
    let length = if m.frame_rate > 0.0 {
        (m.animation_length as f32 * 1000.0 / m.frame_rate) as i32
    } else {
        m.animation_length
    };
    put_i32(&mut o, length);
    put_i32(&mut o, m.shade_type);
    o.push((m.alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
    o.extend([0u8; 16]);
    put_i32(&mut o, textures.len() as i32);
    for t in &textures {
        fixed(&mut o, t, 40);
    }
    fixed(&mut o, ROOT, 40);
    put_i32(&mut o, m.nodes.len() as i32 + 1);

    let identity = [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let node = |o: &mut Vec<u8>,
                name: &[u8],
                parent: &[u8],
                tex: &[i32],
                scale: [f32; 3],
                verts: &[[f32; 3]],
                n: Option<&rsm_model::Node>| {
        fixed(o, name, 40);
        fixed(o, parent, 40);
        put_i32(o, tex.len() as i32);
        tex.iter().for_each(|&t| put_i32(o, t));
        put_f32s(o, &identity);
        put_f32s(o, &[0.0; 3]); // offset
        put_f32s(o, &[0.0; 3]); // position
        put_f32s(o, &[0.0]); // rotation angle
        put_f32s(o, &[0.0; 3]); // rotation axis
        put_f32s(o, &scale);
        put_i32(o, verts.len() as i32);
        verts.iter().for_each(|v| put_f32s(o, v));
        let tc = n.map_or(&[][..], |n| &n.tex_coords[..]);
        put_i32(o, tc.len() as i32);
        for t in tc {
            o.extend(0xFFFF_FFFFu32.to_le_bytes());
            put_f32s(o, t);
        }
        let faces = n.map_or(&[][..], |n| &n.faces[..]);
        put_i32(o, faces.len() as i32);
        for f in faces {
            f.vertices.iter().for_each(|v| o.extend(v.to_le_bytes()));
            f.tex_vertices
                .iter()
                .for_each(|v| o.extend(v.to_le_bytes()));
            let tex_count = tex.len().max(1);
            let texture = if usize::from(f.texture) < tex_count {
                f.texture
            } else {
                0
            };
            o.extend(texture.to_le_bytes());
            o.extend(0u16.to_le_bytes());
            put_i32(o, i32::from(f.two_sided));
            put_i32(o, f.smooth_group);
        }
        put_i32(o, 0); // rotation key frames
    };
    node(&mut o, ROOT, b"", &[], [-1.0, -1.0, 1.0], &[], None);
    let names: Vec<&[u8]> = m.nodes.iter().map(|n| n.name.as_slice()).collect();
    for (i, n) in m.nodes.iter().enumerate() {
        // every node without a parent of its own hangs below the new root
        let rooted =
            n.parent.is_empty() || !names.contains(&n.parent.as_slice()) || n.parent == n.name;
        let parent: &[u8] = if rooted { ROOT } else { &n.parent };
        node(
            &mut o,
            &n.name,
            parent,
            &ids[i],
            [1.0, 1.0, 1.0],
            &baked[i],
            Some(n),
        );
    }
    put_i32(&mut o, 0); // scale key frames (written after the meshes before version 1.6)
    put_i32(&mut o, 0); // volume boxes
    Rsm1 { bytes: o, center }
}

type Mat3 = [[f32; 3]; 3];

fn mul3(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn rot_x(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}
fn rot_y(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}
fn rot_z(a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// The position and scale an object of the world file gets when its model becomes an RSM1 one:
/// the position moves by the centre of the model's vertices (seen from the turned model), the x
/// scale changes sign. `rotation` is in degrees, as stored.
pub fn move_instance(
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
    center: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let d = |x: f32| x.to_radians();
    // mirror z, turn about z, x and y (signs as the old renderer uses them), scale
    let turn = mul3(
        &mul3(&rot_z(d(rotation[2])), &rot_x(-d(rotation[0]))),
        &rot_y(-d(rotation[1])),
    );
    let u = [
        center[0] * scale[0],
        -center[1] * scale[1],
        -center[2] * scale[2],
    ];
    let mut r = [0.0f32; 3];
    for (i, ri) in r.iter_mut().enumerate() {
        *ri = (0..3).map(|k| turn[i][k] * u[k]).sum();
    }
    r[2] = -r[2];
    (
        [position[0] + r[0], position[1] + r[1], position[2] + r[2]],
        [-scale[0], scale[1], scale[2]],
    )
}

fn is_rsm2(name: &[u8]) -> bool {
    name.len() >= 5 && name[name.len() - 5..].eq_ignore_ascii_case(b".rsm2")
}

fn name_of(raw: &[u8]) -> Vec<u8> {
    raw.iter().take_while(|&&b| b != 0).copied().collect()
}

fn set_name(slot: &mut [u8], name: &[u8]) {
    slot.fill(0);
    let n = name.len().min(slot.len() - 1);
    slot[..n].copy_from_slice(&name[..n]);
}

/// What converting one map gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapDowngrade {
    pub rsw: Vec<u8>,
    pub gnd: Vec<u8>,
    /// New RSM1 models: archive path (below `data\model\`) and bytes.
    pub models: Vec<(Vec<u8>, Vec<u8>)>,
    /// Instances moved to an RSM1 model.
    pub instances: usize,
    /// RSM2 models the archive does not have (their instances stay as they are).
    pub missing: Vec<String>,
    /// RSM2 models that could not be converted, with the reason.
    pub failed: Vec<String>,
}

/// The water of a ground file of version 1.8 or 1.9 (the first zone, else the default).
fn ground_water(g: &gnd_map::Ground) -> Option<Water> {
    if g.version < Version::new(1, 8) || g.tail.len() < 32 {
        return None;
    }
    let t = &g.tail;
    let f = |i: usize| f32::from_le_bytes([t[i], t[i + 1], t[i + 2], t[i + 3]]);
    let n = |i: usize| i32::from_le_bytes([t[i], t[i + 1], t[i + 2], t[i + 3]]);
    let default = Water {
        level: f(0),
        kind: n(4),
        wave_height: f(8),
        wave_speed: f(12),
        wave_pitch: f(16),
        texture_cycling: n(20),
    };
    let (zw, zh) = (n(24), n(28));
    if zw <= 0 || zh <= 0 {
        return Some(default);
    }
    // the first zone: the whole water (1.9) or only the level (1.8)
    if g.version >= Version::new(1, 9) && t.len() >= 32 + 24 {
        return Some(Water {
            level: f(32),
            kind: n(36),
            wave_height: f(40),
            wave_speed: f(44),
            wave_pitch: f(48),
            texture_cycling: n(52),
        });
    }
    if t.len() >= 36 {
        return Some(Water {
            level: f(32),
            ..default
        });
    }
    Some(default)
}

/// Downgrades one map. `read_model` gets the model name as the world file writes it (below
/// `data\model\`) and returns the bytes of the model, `None` when the archive does not have it.
pub fn downgrade_map(
    rsw: &[u8],
    gnd: &[u8],
    ground_name: &[u8],
    altitude_name: &[u8],
    read_model: &mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
) -> Result<MapDowngrade, DowngradeError> {
    let mut w = Rsw::parse(rsw).map_err(|e| DowngradeError::World(e.to_string()))?;
    let mut g = gnd_map::parse(gnd).map_err(|e| DowngradeError::Ground(e.to_string()))?;
    let mut out = MapDowngrade::default();

    let mut converted: std::collections::HashMap<Vec<u8>, Option<Rsm1>> = Default::default();
    for obj in &mut w.objects {
        let Object::Model(m) = obj else { continue };
        let name = name_of(&m.model_name);
        if !is_rsm2(&name) {
            continue;
        }
        let key: Vec<u8> = name.to_ascii_lowercase();
        if !converted.contains_key(&key) {
            let result = match read_model(&name) {
                None => {
                    out.missing
                        .push(String::from_utf8_lossy(&name).into_owned());
                    None
                }
                Some(bytes) => match model_to_rsm1(&bytes) {
                    Ok(r) => {
                        let mut new = name.clone();
                        new.truncate(new.len() - 1);
                        out.models.push((new, r.bytes.clone()));
                        Some(r)
                    }
                    Err(e) => {
                        out.failed
                            .push(format!("{}: {e}", String::from_utf8_lossy(&name)));
                        None
                    }
                },
            };
            converted.insert(key.clone(), result);
        }
        let Some(Some(r)) = converted.get(&key) else {
            continue;
        };
        let (position, scale) = move_instance(m.position, m.rotation, m.scale, r.center);
        m.position = position;
        m.scale = scale;
        let mut new = name.clone();
        new.truncate(new.len() - 1);
        set_name(&mut m.model_name, &new);
        out.instances += 1;
    }

    w.version = Version::new(2, 1);
    w.build_number = 0;
    set_name(&mut w.ground, ground_name);
    set_name(&mut w.altitude, altitude_name);
    if let Some(water) = ground_water(&g) {
        w.water = Some(water);
    }
    w.water.get_or_insert(Water {
        level: 0.0,
        kind: 0,
        wave_height: 1.0,
        wave_speed: 2.0,
        wave_pitch: 50.0,
        texture_cycling: 3,
    });
    g.version = Version::new(1, 7);
    g.tail.clear();
    out.rsw = w.to_bytes();
    out.gnd = g.to_bytes();
    Ok(out)
}

/// One map by its path without extension (`data\\prontera`): reads the world, ground and altitude
/// files and the RSM2 models through `read` (archive path to bytes, `None` when missing) and
/// returns the files to write with their paths: the world and ground file, and every converted
/// model below `data\\model\\`. The inner error says why the map was left out.
#[allow(clippy::type_complexity)]
pub fn apply_map<E>(
    base: &[u8],
    read: impl Fn(&[u8]) -> Result<Option<Vec<u8>>, E>,
) -> Result<Result<(Vec<(Vec<u8>, Vec<u8>)>, MapDowngrade), String>, E> {
    let Some(rsw) = read(&[base, b".rsw"].concat())? else {
        return Ok(Err("no world file".into()));
    };
    let [rsw_path, gnd_path, gat_path] = crate::flat_map::map_paths(base, Some(&rsw));
    let Some(gnd) = read(&gnd_path)? else {
        return Ok(Err("no ground file".into()));
    };
    let file_name = |p: &[u8]| p.rsplit(|&b| b == b'\\').next().unwrap_or(p).to_vec();
    let dir = b"data\\model\\".as_slice();
    let mut failure = None;
    let mut model = |name: &[u8]| -> Option<Vec<u8>> {
        match read(&[dir, name].concat()) {
            Ok(d) => d,
            Err(e) => {
                failure = Some(e);
                None
            }
        }
    };
    let result = downgrade_map(
        &rsw,
        &gnd,
        &file_name(&gnd_path),
        &file_name(&gat_path),
        &mut model,
    );
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(result.map_err(|e| e.to_string()).map(|d| {
        let mut files = vec![(rsw_path, d.rsw.clone()), (gnd_path, d.gnd.clone())];
        files.extend(d.models.iter().map(|(n, b)| ([dir, n].concat(), b.clone())));
        (files, d)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model of version 2.2 with two nodes whose second one hangs below the first.
    fn rsm2(vertices: &[[f32; 3]]) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend(b"GRSM");
        o.extend([2, 3]);
        put_i32(&mut o, 0); // animation length
        put_i32(&mut o, 1); // shade type
        o.push(255);
        put_f32s(&mut o, &[30.0]); // frame rate
        put_i32(&mut o, 1); // main mesh names
        put_i32(&mut o, 4);
        o.extend(b"root");
        put_i32(&mut o, 2); // meshes
        for (name, parent, v, pos) in [
            ("root", "", vertices, [0.0f32; 3]),
            ("kid", "root", &vertices[..3], [10.0, 0.0, 0.0]),
        ] {
            put_i32(&mut o, name.len() as i32);
            o.extend(name.as_bytes());
            put_i32(&mut o, parent.len() as i32);
            o.extend(parent.as_bytes());
            put_i32(&mut o, 1); // textures of the mesh
            put_i32(&mut o, 5);
            o.extend(b"a.bmp");
            put_f32s(&mut o, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
            put_f32s(&mut o, &pos); // offset: the node's own position (2.2 and later)
            put_i32(&mut o, v.len() as i32);
            v.iter().for_each(|x| put_f32s(&mut o, x));
            put_i32(&mut o, 3); // texture vertices
            for t in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
                o.extend(0xFFFF_FFFFu32.to_le_bytes());
                put_f32s(&mut o, &t);
            }
            put_i32(&mut o, 1); // faces
            put_i32(&mut o, 24);
            for k in [0u16, 1, 2, 0, 1, 2, 0, 0] {
                o.extend(k.to_le_bytes());
            }
            put_i32(&mut o, 1);
            put_i32(&mut o, 0);
            put_i32(&mut o, 0); // scale key frames
            put_i32(&mut o, 0); // rotation key frames
            put_i32(&mut o, 0); // position key frames
            put_i32(&mut o, 0); // texture animations
        }
        put_i32(&mut o, 0); // volume boxes
        o
    }

    const BOX: [[f32; 3]; 4] = [
        [-4.0, 2.0, -1.0],
        [8.0, 3.0, 5.0],
        [0.0, 12.0, 2.0],
        [1.0, 1.0, 1.0],
    ];

    #[test]
    fn an_rsm2_model_becomes_a_readable_rsm1_with_the_baked_pose() {
        let r = model_to_rsm1(&rsm2(&BOX)).unwrap();
        assert_eq!(&r.bytes[..6], b"GRSM\x01\x04");
        let m = rsm_model::parse(&r.bytes).unwrap();
        assert_eq!((m.version, m.nodes.len()), (Version::new(1, 4), 3));
        assert_eq!(m.nodes[0].name, ROOT);
        assert_eq!(m.nodes[0].scale, [-1.0, -1.0, 1.0]);
        assert_eq!(m.nodes[1].parent, ROOT);
        assert_eq!(m.nodes[2].parent, b"root");
        // the second node's own position is baked into its vertices
        assert_eq!(m.nodes[2].vertices[0], [6.0, 2.0, -1.0]);
        assert_eq!(m.nodes[1].textures, vec![b"a.bmp".to_vec()]);
        assert_eq!(m.nodes[1].faces.len(), 1);
        // box over everything: x -4..18 (the second node sits 10 further), z -1..5, lowest y 1
        assert_eq!(r.center, [7.0, 1.0, 2.0]);
        assert!(model_to_rsm1(&r.bytes).is_err(), "an RSM1 is not an RSM2");
    }

    /// The old client's matrix for an RSM1 object, the new one's for an RSM2 object: the same
    /// vertex ends up in the same place.
    #[test]
    fn the_moved_instance_puts_every_vertex_where_it_was() {
        type M = [[f32; 4]; 4];
        let mul = |a: &M, b: &M| {
            let mut r = [[0.0f32; 4]; 4];
            for i in 0..4 {
                for j in 0..4 {
                    r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
                }
            }
            r
        };
        let id = || -> M {
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        };
        let scale = |x: f32, y: f32, z: f32| -> M {
            let mut m = id();
            (m[0][0], m[1][1], m[2][2]) = (x, y, z);
            m
        };
        let trans = |x: f32, y: f32, z: f32| -> M {
            let mut m = id();
            (m[0][3], m[1][3], m[2][3]) = (x, y, z);
            m
        };
        let rot = |axis: usize, a: f32| -> M {
            let r = [rot_x(a), rot_y(a), rot_z(a)][axis];
            let mut m = id();
            for i in 0..3 {
                for j in 0..3 {
                    m[i][j] = r[i][j];
                }
            }
            m
        };
        let apply = |m: &M, v: [f32; 3]| {
            [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3])
        };
        for (pos, rotation, sc) in [
            ([30.0, 12.0, -7.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]),
            ([-40.0, 5.0, 22.0], [10.0, 75.0, 20.0], [1.5, 0.8, 2.0]),
            ([3.0, -2.0, 9.0], [90.0, 30.0, 180.0], [1.0, 2.0, 1.0]),
        ] {
            let r = model_to_rsm1(&rsm2(&BOX)).unwrap();
            let m2 = rsm_model::parse(&rsm2(&BOX)).unwrap();
            let m1 = rsm_model::parse(&r.bytes).unwrap();
            // GRF Editor's renderer: mirror z, move, turn about z, x, y, scale
            let place = |p: [f32; 3], s: [f32; 3], tail: &M| {
                let d = |x: f32| x.to_radians();
                let mut m = scale(1.0, 1.0, -1.0);
                m = mul(&m, &trans(p[0], -p[1], p[2]));
                m = mul(&m, &rot(2, -d(rotation[2])));
                m = mul(&m, &rot(0, -d(rotation[0])));
                m = mul(&m, &rot(1, d(rotation[1])));
                m = mul(&m, &scale(s[0], -s[1], s[2]));
                mul(&m, tail)
            };
            let new_model = place(pos, sc, &scale(1.0, -1.0, 1.0));
            let (pos1, sc1) = move_instance(pos, rotation, sc, r.center);
            // the old client centres an RSM1 on the box of its vertices (y turned); the nodes'
            // matrices already include the root node's scale
            let baked1: Vec<[f32; 3]> = rsm_model::baked_vertices(&m1)
                .into_iter()
                .flatten()
                .collect();
            let shown: Vec<[f32; 3]> = baked1.iter().map(|v| [v[0], -v[1], v[2]]).collect();
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &shown {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            let tail1 = trans(-(lo[0] + hi[0]) / 2.0, lo[1], -(lo[2] + hi[2]) / 2.0);
            let old_model = place(pos1, sc1, &tail1);
            let originals: Vec<[f32; 3]> = rsm_model::baked_vertices(&m2)
                .into_iter()
                .flatten()
                .collect();
            assert_eq!(originals.len(), baked1.len());
            for (a, b) in originals.iter().zip(&baked1) {
                let want = apply(&new_model, *a);
                let got = apply(&old_model, *b);
                for k in 0..3 {
                    assert!(
                        (want[k] - got[k]).abs() < 1e-3,
                        "{want:?} vs {got:?} (pos {pos:?}, rot {rotation:?})"
                    );
                }
            }
        }
    }
}
