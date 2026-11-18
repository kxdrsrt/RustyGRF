//! RSM models with their geometry, for the 3D preview: nodes (vertices, texture coordinates,
//! faces, transforms) and a static pose of the whole model as triangles grouped by texture.
//!
//! The layout is the one `rsm.rs` verifies byte for byte; this module keeps what that one skips.
//! The transform chain follows the client's static pose as observed in roBrowser's model loader
//! (GPL-3.0, behaviour only, no code copied): a node's matrix is its parent's matrix, moved by the
//! node position, turned by its rotation (axis and angle, or the first rotation key) and scaled;
//! the vertices then go through the node's offset and its 3 x 3 matrix. Versions 2.2 and 2.3 store
//! no position, angle or scale in the node; their first key frames are used instead (to be checked
//! against real models).

use crate::rsm::{Reader, RsmError, V1_2, V1_4, V1_6, V2_2, V2_3};
use crate::version::Version;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub vertices: [u16; 3],
    pub tex_vertices: [u16; 3],
    pub texture: u16,
    pub two_sided: bool,
    pub smooth_group: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: Vec<u8>,
    pub parent: Vec<u8>,
    /// Texture names this node's faces refer to by number (resolved from the model list before 2.3).
    pub textures: Vec<Vec<u8>>,
    pub mat3: [f32; 9],
    pub offset: [f32; 3],
    pub pos: [f32; 3],
    pub rot_angle: f32,
    pub rot_axis: [f32; 3],
    pub scale: [f32; 3],
    pub vertices: Vec<[f32; 3]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub faces: Vec<Face>,
    /// (frame, x, y, z, w)
    pub rot_keys: Vec<(i32, [f32; 4])>,
    pub pos_keys: Vec<(i32, [f32; 3])>,
    pub scale_keys: Vec<(i32, [f32; 3])>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub version: Version,
    pub animation_length: i32,
    pub shade_type: i32,
    pub alpha: f32,
    /// Frames per second of the key frames (0 before version 2.2, where they count milliseconds).
    pub frame_rate: f32,
    pub main_name: Vec<u8>,
    pub nodes: Vec<Node>,
}

pub fn parse(bytes: &[u8]) -> Result<Model, RsmError> {
    if bytes.len() < 6 || (&bytes[..4] != b"GRSM" && &bytes[..4] != b"GRSX") {
        return Err(RsmError::BadMagic);
    }
    let version = Version::new(bytes[4], bytes[5]);
    if version < Version::new(1, 0) || version > V2_3 {
        return Err(RsmError::UnsupportedVersion(version));
    }
    let mut r = Reader { d: bytes, pos: 6 };
    let animation_length = r.i32()?;
    let shade_type = r.i32()?;
    let alpha = if version >= V1_4 {
        f32::from(r.take(1)?[0]) / 255.0
    } else {
        1.0
    };
    let mut textures = Vec::new();
    let mut main_name = Vec::new();
    let mut frame_rate = 0.0;
    let n_nodes = if version >= V2_3 {
        frame_rate = r.f32()?;
        let n = r.count("main mesh name", 4)?;
        for i in 0..n {
            let name = r.lstr()?;
            if i == 0 {
                main_name = name;
            }
        }
        r.count("mesh", 8)?
    } else if version >= V2_2 {
        frame_rate = r.f32()?;
        let n = r.count("texture", 4)?;
        for _ in 0..n {
            textures.push(r.lstr()?);
        }
        let n = r.count("main mesh name", 4)?;
        for i in 0..n {
            let name = r.lstr()?;
            if i == 0 {
                main_name = name;
            }
        }
        r.count("mesh", 8)?
    } else {
        r.skip(16)?;
        let n = r.count("texture", 40)?;
        for _ in 0..n {
            textures.push(r.fstr(40)?);
        }
        main_name = r.fstr(40)?;
        r.count("mesh", 80)?
    };
    let mut nodes = Vec::with_capacity(n_nodes.min(256));
    for _ in 0..n_nodes {
        nodes.push(read_node(&mut r, version, &textures)?);
    }
    Ok(Model {
        version,
        animation_length,
        shade_type,
        alpha,
        frame_rate,
        main_name,
        nodes,
    })
}

fn floats<const N: usize>(r: &mut Reader<'_>) -> Result<[f32; N], RsmError> {
    let mut out = [0.0; N];
    for v in &mut out {
        *v = r.f32()?;
    }
    Ok(out)
}

fn read_node(r: &mut Reader<'_>, v: Version, model_textures: &[Vec<u8>]) -> Result<Node, RsmError> {
    let (name, parent) = if v >= V2_2 {
        (r.lstr()?, r.lstr()?)
    } else {
        (r.fstr(40)?, r.fstr(40)?)
    };
    let mut textures = Vec::new();
    if v >= V2_3 {
        let n = r.count("mesh texture", 4)?;
        for _ in 0..n {
            textures.push(r.lstr()?);
        }
    } else {
        let n = r.count("texture index", 4)?;
        for _ in 0..n {
            let i = r.i32()?;
            textures.push(
                usize::try_from(i)
                    .ok()
                    .and_then(|i| model_textures.get(i))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
    }
    let mat3 = floats::<9>(r)?;
    let offset = floats::<3>(r)?;
    let (mut pos, mut rot_angle, mut rot_axis, mut scale) =
        ([0.0; 3], 0.0, [0.0, 1.0, 0.0], [1.0; 3]);
    if v < V2_2 {
        pos = floats::<3>(r)?;
        rot_angle = r.f32()?;
        rot_axis = floats::<3>(r)?;
        scale = floats::<3>(r)?;
    }
    let n = r.count("vertex", 12)?;
    let mut vertices = Vec::with_capacity(n);
    for _ in 0..n {
        vertices.push(floats::<3>(r)?);
    }
    let n = r.count("texture vertex", 8)?;
    let mut tex_coords = Vec::with_capacity(n);
    for _ in 0..n {
        if v >= V1_2 {
            r.skip(4)?; // vertex colour
        }
        tex_coords.push(floats::<2>(r)?);
    }
    let n = r.count("face", 20)?;
    let mut faces = Vec::with_capacity(n);
    for _ in 0..n {
        let len = if v >= V2_2 {
            r.count("face length", 1)?.max(24)
        } else if v >= V1_2 {
            24
        } else {
            20
        };
        let b = r.take(len)?;
        let u = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let i32_at = |i: usize| i32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        faces.push(Face {
            vertices: [u(0), u(2), u(4)],
            tex_vertices: [u(6), u(8), u(10)],
            texture: u(12),
            two_sided: i32_at(16) != 0,
            smooth_group: if len >= 24 { i32_at(20) } else { 0 },
        });
    }
    let mut scale_keys = Vec::new();
    if v >= V1_6 {
        let n = r.count("scale key frame", 20)?;
        for _ in 0..n {
            let frame = r.i32()?;
            let k = floats::<4>(r)?;
            scale_keys.push((frame, [k[0], k[1], k[2]]));
        }
    }
    let n = r.count("rotation key frame", 20)?;
    let mut rot_keys = Vec::with_capacity(n);
    for _ in 0..n {
        let frame = r.i32()?;
        rot_keys.push((frame, floats::<4>(r)?));
    }
    let mut pos_keys = Vec::new();
    if v >= V2_2 {
        let n = r.count("position key frame", 20)?;
        for _ in 0..n {
            let frame = r.i32()?;
            let k = floats::<4>(r)?;
            pos_keys.push((frame, [k[0], k[1], k[2]]));
        }
    }
    if v >= V2_3 {
        let n = r.count("texture animation", 8)?;
        for _ in 0..n {
            r.skip(4)?;
            let kinds = r.count("texture animation kind", 8)?;
            for _ in 0..kinds {
                r.skip(4)?;
                let frames = r.count("texture key frame", 8)?;
                r.skip(frames * 8)?;
            }
        }
    }
    Ok(Node {
        name,
        parent,
        textures,
        mat3,
        offset,
        pos,
        rot_angle,
        rot_axis,
        scale,
        vertices,
        tex_coords,
        faces,
        rot_keys,
        pos_keys,
        scale_keys,
    })
}

// ---------- small column-major 4 x 4 matrix helpers ----------

type Mat4 = [f32; 16];

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut o = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

fn translation(t: [f32; 3]) -> Mat4 {
    let mut m = IDENTITY;
    m[12..15].copy_from_slice(&t);
    m
}

fn scaling(s: [f32; 3]) -> Mat4 {
    let mut m = IDENTITY;
    m[0] = s[0];
    m[5] = s[1];
    m[10] = s[2];
    m
}

fn from_mat3(m: &[f32; 9]) -> Mat4 {
    // the file stores the 3 x 3 matrix row by row; glMatrix reads it as columns
    [
        m[0], m[1], m[2], 0.0, m[3], m[4], m[5], 0.0, m[6], m[7], m[8], 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// Rotation by `angle` radians around `axis`.
fn axis_angle(angle: f32, axis: [f32; 3]) -> Mat4 {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len < 1e-6 || angle == 0.0 {
        return IDENTITY;
    }
    let (x, y, z) = (axis[0] / len, axis[1] / len, axis[2] / len);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [
        x * x * t + c,
        y * x * t + z * s,
        z * x * t - y * s,
        0.0,
        x * y * t - z * s,
        y * y * t + c,
        z * y * t + x * s,
        0.0,
        x * z * t + y * s,
        y * z * t - x * s,
        z * z * t + c,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn from_quat(q: [f32; 4]) -> Mat4 {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-6 {
        return IDENTITY;
    }
    let (x, y, z, w) = (q[0] / n, q[1] / n, q[2] / n, q[3] / n);
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
        0.0,
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
        0.0,
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// The matrix that places a model in a map, for vertices in the map's own space (y down, the model
/// standing on y = 0 with up as negative y): move to `position` (+ `offset`), turn about z, x and y
/// (degrees, in that order), then scale. Column-major.
pub fn instance_matrix(
    position: [f32; 3],
    offset: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
) -> [f32; 16] {
    let m = translation([
        position[0] + offset[0],
        position[1] + offset[1],
        position[2] + offset[2],
    ]);
    let m = mul(&m, &axis_angle(rotation[2].to_radians(), [0.0, 0.0, 1.0]));
    let m = mul(&m, &axis_angle(rotation[0].to_radians(), [1.0, 0.0, 0.0]));
    let m = mul(&m, &axis_angle(rotation[1].to_radians(), [0.0, 1.0, 0.0]));
    mul(&m, &scaling(scale))
}

/// `a * b` for two column-major 4 x 4 matrices.
pub fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    mul(a, b)
}

/// Triangles of one texture, in view space: y up, the model centred on x and z, standing on y = 0.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub texture: Vec<u8>,
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticModel {
    pub parts: Vec<Part>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub alpha: f32,
    pub triangles: usize,
}

fn node_matrices(m: &Model) -> Vec<Mat4> {
    // a node's matrix needs its parent's; nodes can come in any order and parents can be missing
    let by_name: HashMap<&[u8], usize> = m
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_slice(), i))
        .collect();
    let mut done: Vec<Option<Mat4>> = vec![None; m.nodes.len()];
    fn go(
        i: usize,
        m: &Model,
        by_name: &HashMap<&[u8], usize>,
        done: &mut Vec<Option<Mat4>>,
        depth: usize,
    ) -> Mat4 {
        if let Some(d) = done[i] {
            return d;
        }
        let n = &m.nodes[i];
        let parent = by_name
            .get(n.parent.as_slice())
            .copied()
            .filter(|&p| p != i && depth < 64);
        let base = parent.map_or(IDENTITY, |p| go(p, m, by_name, done, depth + 1));
        let pos = n.pos_keys.first().map_or(n.pos, |k| k.1);
        let rot = n
            .rot_keys
            .first()
            .map_or_else(|| axis_angle(n.rot_angle, n.rot_axis), |k| from_quat(k.1));
        let scale = n.scale_keys.first().map_or(n.scale, |k| k.1);
        let mat = mul(&mul(&mul(&base, &translation(pos)), &rot), &scaling(scale));
        done[i] = Some(mat);
        mat
    }
    (0..m.nodes.len())
        .map(|i| go(i, m, &by_name, &mut done, 0))
        .collect()
}

/// The model's static pose as triangles grouped by texture.
pub fn static_pose(m: &Model) -> StaticModel {
    let mats = node_matrices(m);
    let only = m.nodes.len() == 1;
    struct Raw {
        texture: Vec<u8>,
        pos: [[f32; 3]; 3],
        normal: [[f32; 3]; 3],
        uv: [[f32; 2]; 3],
    }
    let mut raw: Vec<Raw> = Vec::new();
    for (ni, n) in m.nodes.iter().enumerate() {
        let mut vm = mats[ni];
        if !only {
            vm = mul(&vm, &translation(n.offset));
        }
        vm = mul(&vm, &from_mat3(&n.mat3));
        let world: Vec<[f32; 3]> = n.vertices.iter().map(|&v| apply(&vm, v)).collect();
        // face normals in the transformed space
        let face_normal: Vec<[f32; 3]> = n
            .faces
            .iter()
            .map(|f| {
                let get = |i: u16| world.get(usize::from(i)).copied().unwrap_or([0.0; 3]);
                let (a, b, c) = (get(f.vertices[0]), get(f.vertices[1]), get(f.vertices[2]));
                let (u, v) = (
                    [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                    [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
                );
                let x = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
                if l < 1e-9 {
                    [0.0, -1.0, 0.0]
                } else {
                    [x[0] / l, x[1] / l, x[2] / l]
                }
            })
            .collect();
        let mut smooth: HashMap<(u16, i32), [f32; 3]> = HashMap::new();
        if m.shade_type == 2 {
            for (f, fnrm) in n.faces.iter().zip(&face_normal) {
                for &vi in &f.vertices {
                    let e = smooth.entry((vi, f.smooth_group)).or_insert([0.0; 3]);
                    for k in 0..3 {
                        e[k] += fnrm[k];
                    }
                }
            }
        }
        for (f, fnrm) in n.faces.iter().zip(&face_normal) {
            let mut pos = [[0.0; 3]; 3];
            let mut normal = [[0.0; 3]; 3];
            let mut uv = [[0.0; 2]; 3];
            for k in 0..3 {
                pos[k] = world
                    .get(usize::from(f.vertices[k]))
                    .copied()
                    .unwrap_or([0.0; 3]);
                uv[k] = n
                    .tex_coords
                    .get(usize::from(f.tex_vertices[k]))
                    .copied()
                    .unwrap_or([0.0; 2]);
                let nn = match m.shade_type {
                    0 => [0.0, -1.0, 0.0],
                    2 => smooth
                        .get(&(f.vertices[k], f.smooth_group))
                        .copied()
                        .unwrap_or(*fnrm),
                    _ => *fnrm,
                };
                let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2])
                    .sqrt()
                    .max(1e-9);
                normal[k] = [nn[0] / l, nn[1] / l, nn[2] / l];
            }
            raw.push(Raw {
                texture: n
                    .textures
                    .get(usize::from(f.texture))
                    .cloned()
                    .unwrap_or_default(),
                pos,
                normal,
                uv,
            });
        }
    }
    // bounds in the file's space (y points down), then to view space: centre x and z, stand on y = 0
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
    for r in &raw {
        for p in &r.pos {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    if raw.is_empty() {
        min = [0.0; 3];
        max = [0.0; 3];
    }
    let (cx, cz, bottom) = ((min[0] + max[0]) / 2.0, (min[2] + max[2]) / 2.0, max[1]);
    let to_view = |p: [f32; 3]| [p[0] - cx, bottom - p[1], p[2] - cz];
    let mut parts: Vec<Part> = Vec::new();
    let mut index: HashMap<Vec<u8>, usize> = HashMap::new();
    for r in &raw {
        let at = *index.entry(r.texture.clone()).or_insert_with(|| {
            parts.push(Part {
                texture: r.texture.clone(),
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
            });
            parts.len() - 1
        });
        let part = &mut parts[at];
        for k in 0..3 {
            part.positions.extend(to_view(r.pos[k]));
            part.normals
                .extend([r.normal[k][0], -r.normal[k][1], r.normal[k][2]]);
            part.uvs.extend(r.uv[k]);
        }
    }
    StaticModel {
        triangles: raw.len(),
        parts,
        min: [min[0] - cx, bottom - max[1], min[2] - cz],
        max: [max[0] - cx, bottom - min[1], max[2] - cz],
        alpha: m.alpha,
    }
}

// ---------- animation: node matrices over time, geometry in node space ----------

/// Time in milliseconds of a key frame number.
fn key_ms(m: &Model, frame: i32) -> f32 {
    if m.version >= V2_2 && m.frame_rate > 0.0 {
        frame as f32 * 1000.0 / m.frame_rate
    } else {
        frame as f32
    }
}

/// Length of the animation loop in milliseconds (0 when the model does not move).
pub fn duration_ms(m: &Model) -> f32 {
    let moves = m
        .nodes
        .iter()
        .any(|n| n.rot_keys.len() > 1 || n.pos_keys.len() > 1 || n.scale_keys.len() > 1);
    if !moves {
        return 0.0;
    }
    let last_key = m
        .nodes
        .iter()
        .flat_map(|n| {
            n.rot_keys
                .iter()
                .map(|k| k.0)
                .chain(n.pos_keys.iter().map(|k| k.0))
                .chain(n.scale_keys.iter().map(|k| k.0))
        })
        .max()
        .unwrap_or(0);
    let from_header = key_ms(m, m.animation_length);
    from_header.max(key_ms(m, last_key)).max(1.0)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Spherical interpolation of two quaternions (x, y, z, w) on the shorter arc.
fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = b.map(|v| -v);
        dot = -dot;
    }
    let (wa, wb) = if dot > 0.9995 {
        (1.0 - t, t)
    } else {
        let theta = dot.clamp(-1.0, 1.0).acos();
        let s = theta.sin();
        (((1.0 - t) * theta).sin() / s, (t * theta).sin() / s)
    };
    std::array::from_fn(|i| a[i] * wa + b[i] * wb)
}

/// The two key frames around `t` and how far between them `t` is (clamped at both ends).
fn bracket<T: Copy>(m: &Model, keys: &[(i32, T)], t: f32) -> Option<(T, T, f32)> {
    let first = keys.first()?;
    let last = keys.last()?;
    if t <= key_ms(m, first.0) || keys.len() == 1 {
        return Some((first.1, first.1, 0.0));
    }
    if t >= key_ms(m, last.0) {
        return Some((last.1, last.1, 0.0));
    }
    let i = keys.partition_point(|k| key_ms(m, k.0) <= t);
    let (a, b) = (&keys[i - 1], &keys[i]);
    let (ta, tb) = (key_ms(m, a.0), key_ms(m, b.0));
    Some((a.1, b.1, if tb > ta { (t - ta) / (tb - ta) } else { 0.0 }))
}

/// World matrices of all nodes at `t` milliseconds into the loop (position, rotation and scale
/// interpolated between the key frames), in the file's own space.
pub fn node_matrices_at(m: &Model, t: f32) -> Vec<[f32; 16]> {
    let by_name: HashMap<&[u8], usize> = m
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_slice(), i))
        .collect();
    let mut done: Vec<Option<Mat4>> = vec![None; m.nodes.len()];
    fn go(
        i: usize,
        m: &Model,
        t: f32,
        by_name: &HashMap<&[u8], usize>,
        done: &mut Vec<Option<Mat4>>,
        depth: usize,
    ) -> Mat4 {
        if let Some(d) = done[i] {
            return d;
        }
        let n = &m.nodes[i];
        let parent = by_name
            .get(n.parent.as_slice())
            .copied()
            .filter(|&p| p != i && depth < 64);
        let base = parent.map_or(IDENTITY, |p| go(p, m, t, by_name, done, depth + 1));
        let pos = bracket(m, &n.pos_keys, t).map_or(n.pos, |(a, b, f)| lerp3(a, b, f));
        let rot = bracket(m, &n.rot_keys, t).map_or_else(
            || axis_angle(n.rot_angle, n.rot_axis),
            |(a, b, f)| from_quat(slerp(a, b, f)),
        );
        let scale = bracket(m, &n.scale_keys, t).map_or(n.scale, |(a, b, f)| lerp3(a, b, f));
        let mat = mul(&mul(&mul(&base, &translation(pos)), &rot), &scaling(scale));
        done[i] = Some(mat);
        mat
    }
    (0..m.nodes.len())
        .map(|i| go(i, m, t, &by_name, &mut done, 0))
        .collect()
}

/// The part of a node's vertex transform that does not move: offset and the 3 x 3 matrix.
pub fn local_matrix(m: &Model, node: usize) -> [f32; 16] {
    let n = &m.nodes[node];
    let mut vm = IDENTITY;
    if m.nodes.len() != 1 {
        vm = mul(&vm, &translation(n.offset));
    }
    mul(&vm, &from_mat3(&n.mat3))
}

/// Triangles of one node in node space (offset and 3 x 3 matrix applied, the node's own movement
/// not), grouped by texture, in the file's space (y down). Normals are made in the same space.
pub fn node_parts(m: &Model, node: usize) -> Vec<Part> {
    let n = &m.nodes[node];
    let lm = local_matrix(m, node);
    let local: Vec<[f32; 3]> = n.vertices.iter().map(|&v| apply(&lm, v)).collect();
    let get = |i: u16| local.get(usize::from(i)).copied().unwrap_or([0.0; 3]);
    let fnormals: Vec<[f32; 3]> = n
        .faces
        .iter()
        .map(|f| {
            let (a, b, c) = (get(f.vertices[0]), get(f.vertices[1]), get(f.vertices[2]));
            let (u, v) = (
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            let x = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
            if l < 1e-9 {
                [0.0, -1.0, 0.0]
            } else {
                [x[0] / l, x[1] / l, x[2] / l]
            }
        })
        .collect();
    let mut smooth: HashMap<(u16, i32), [f32; 3]> = HashMap::new();
    if m.shade_type == 2 {
        for (f, fnrm) in n.faces.iter().zip(&fnormals) {
            for &vi in &f.vertices {
                let e = smooth.entry((vi, f.smooth_group)).or_insert([0.0; 3]);
                for k in 0..3 {
                    e[k] += fnrm[k];
                }
            }
        }
    }
    let mut parts: Vec<Part> = Vec::new();
    let mut index: HashMap<Vec<u8>, usize> = HashMap::new();
    for (f, fnrm) in n.faces.iter().zip(&fnormals) {
        let tex = n
            .textures
            .get(usize::from(f.texture))
            .cloned()
            .unwrap_or_default();
        let at = *index.entry(tex.clone()).or_insert_with(|| {
            parts.push(Part {
                texture: tex,
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
            });
            parts.len() - 1
        });
        let part = &mut parts[at];
        for k in 0..3 {
            part.positions.extend(get(f.vertices[k]));
            let nn = match m.shade_type {
                0 => [0.0, -1.0, 0.0],
                2 => smooth
                    .get(&(f.vertices[k], f.smooth_group))
                    .copied()
                    .unwrap_or(*fnrm),
                _ => *fnrm,
            };
            let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2])
                .sqrt()
                .max(1e-9);
            part.normals.extend([nn[0] / l, nn[1] / l, nn[2] / l]);
            part.uvs.extend(
                n.tex_coords
                    .get(usize::from(f.tex_vertices[k]))
                    .copied()
                    .unwrap_or([0.0; 2]),
            );
        }
    }
    parts
}

/// The matrix from the file's space (y down) to the viewer's (y up, centred on x and z, standing
/// on y = 0), made from the static pose's bounds.
pub fn view_matrix(m: &Model) -> [f32; 16] {
    let mats = node_matrices_at(m, 0.0);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (i, n) in m.nodes.iter().enumerate() {
        let vm = mul(&mats[i], &local_matrix(m, i));
        for &v in &n.vertices {
            let p = apply(&vm, v);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    }
    if lo[0] > hi[0] {
        return IDENTITY;
    }
    let (cx, cz, bottom) = ((lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0, hi[1]);
    // y' = bottom - y, x' = x - cx, z' = z - cz
    let shift = translation([-cx, -bottom, -cz]);
    mul(&scaling([1.0, -1.0, 1.0]), &shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(s: &str, n: usize) -> Vec<u8> {
        let mut a = vec![0u8; n];
        a[..s.len()].copy_from_slice(s.as_bytes());
        a
    }

    /// Version 1.4 model: `nodes` are (name, parent, pos, vertices); one triangle per node.
    type TestNode<'a> = (&'a str, &'a str, [f32; 3], [[f32; 3]; 3]);

    fn model(nodes: &[TestNode]) -> Vec<u8> {
        let mut b = b"GRSM\x01\x04".to_vec();
        b.extend(0i32.to_le_bytes());
        b.extend(2i32.to_le_bytes()); // smooth shading
        b.push(255);
        b.extend([0u8; 16]);
        b.extend(1i32.to_le_bytes());
        b.extend(fixed("t.bmp", 40));
        b.extend(fixed(nodes[0].0, 40));
        b.extend((nodes.len() as i32).to_le_bytes());
        for (name, parent, pos, verts) in nodes {
            b.extend(fixed(name, 40));
            b.extend(fixed(parent, 40));
            b.extend(1i32.to_le_bytes());
            b.extend(0i32.to_le_bytes()); // texture 0
            for m in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
                b.extend(m.to_le_bytes());
            }
            for v in [0.0f32; 3] {
                b.extend(v.to_le_bytes()); // offset
            }
            for v in pos {
                b.extend(v.to_le_bytes());
            }
            for v in [0.0f32, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0] {
                b.extend(v.to_le_bytes()); // angle, axis, scale
            }
            b.extend(3i32.to_le_bytes());
            for v in verts {
                for c in v {
                    b.extend(c.to_le_bytes());
                }
            }
            b.extend(3i32.to_le_bytes());
            for (u, v) in [(0.0f32, 0.0f32), (1.0, 0.0), (0.0, 1.0)] {
                b.extend([255u8; 4]);
                b.extend(u.to_le_bytes());
                b.extend(v.to_le_bytes());
            }
            b.extend(1i32.to_le_bytes());
            for i in [0u16, 1, 2, 0, 1, 2, 0, 0] {
                b.extend(i.to_le_bytes());
            }
            b.extend(0i32.to_le_bytes()); // two sided
            b.extend(0i32.to_le_bytes()); // smooth group
            b.extend(0i32.to_le_bytes()); // rotation key frames
        }
        b.extend(0i32.to_le_bytes()); // scale keys (version < 1.5)
        b.extend(0i32.to_le_bytes()); // volume boxes
        b
    }

    #[test]
    fn reads_geometry_and_agrees_with_the_structural_reader() {
        let tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, -10.0, 0.0]];
        let bytes = model(&[("root", "", [0.0; 3], tri)]);
        let m = parse(&bytes).unwrap();
        assert_eq!(
            (
                m.nodes.len(),
                m.nodes[0].vertices.len(),
                m.nodes[0].faces.len()
            ),
            (1, 3, 1)
        );
        assert_eq!(m.nodes[0].textures, [b"t.bmp".to_vec()]);
        let s = crate::rsm::Rsm::parse(&bytes).unwrap();
        assert_eq!(
            (s.trailing, s.meshes[0].vertices, s.meshes[0].faces),
            (0, 3, 1)
        );
    }

    #[test]
    fn static_pose_stands_the_model_on_the_floor_with_y_up() {
        let tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, -10.0, 0.0]];
        let m = parse(&model(&[("root", "", [0.0; 3], tri)])).unwrap();
        let s = static_pose(&m);
        assert_eq!(s.triangles, 1);
        // the file's y points down: -10 is up; the lowest point (y = 0 in the file) lands on 0
        assert_eq!((s.min[1], s.max[1]), (0.0, 10.0));
        assert_eq!((s.min[0], s.max[0]), (-5.0, 5.0), "centred on x");
        assert_eq!(s.parts.len(), 1);
        assert_eq!(s.parts[0].positions.len(), 9);
        assert_eq!(s.parts[0].uvs.len(), 6);
    }

    #[test]
    fn children_follow_their_parents() {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let m = parse(&model(&[
            ("body", "", [100.0, 0.0, 0.0], tri),
            ("arm", "body", [0.0, 0.0, 50.0], tri),
        ]))
        .unwrap();
        let s = static_pose(&m);
        // the arm sits 50 in front of its parent, which sits 100 to the side
        assert_eq!(s.max[2] - s.min[2], 50.0 + 0.0);
        assert!(s.max[0] - s.min[0] <= 1.0 + 1e-3);
        // a parent loop does not hang
        let looped = parse(&model(&[
            ("a", "b", [0.0; 3], tri),
            ("b", "a", [0.0; 3], tri),
        ]))
        .unwrap();
        assert_eq!(static_pose(&looped).triangles, 2);
    }

    fn spinner(keys: Vec<(i32, [f32; 4])>) -> Model {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let mut m = parse(&model(&[("root", "", [0.0; 3], tri)])).unwrap();
        m.nodes[0].rot_keys = keys;
        m
    }

    #[test]
    fn rotation_keys_are_interpolated_over_time() {
        // identity at 0 ms, a quarter turn about y at 1000 ms
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let m = spinner(vec![(0, [0.0, 0.0, 0.0, 1.0]), (1000, [0.0, s, 0.0, s])]);
        assert_eq!(duration_ms(&m), 1000.0);
        let at = |t: f32| apply(&node_matrices_at(&m, t)[0], [1.0, 0.0, 0.0]);
        let p = at(0.0);
        assert!((p[0] - 1.0).abs() < 1e-5 && p[2].abs() < 1e-5, "{p:?}");
        let p = at(500.0); // an eighth of a turn
        assert!((p[0] - s).abs() < 1e-4 && (p[2] + s).abs() < 1e-4, "{p:?}");
        let p = at(1000.0);
        assert!(p[0].abs() < 1e-4 && (p[2] + 1.0).abs() < 1e-4, "{p:?}");
        // after the last key it stays there; a model without keys does not move
        assert_eq!(at(5000.0), at(1000.0));
        let still = spinner(vec![]);
        assert_eq!(duration_ms(&still), 0.0);
    }

    #[test]
    fn node_space_parts_and_matrices_rebuild_the_static_pose() {
        let tri = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, -10.0, 0.0]];
        let m = parse(&model(&[
            ("body", "", [100.0, 0.0, 0.0], tri),
            ("arm", "body", [0.0, 0.0, 50.0], tri),
        ]))
        .unwrap();
        let st = static_pose(&m);
        let view = view_matrix(&m);
        let mats = node_matrices_at(&m, 0.0);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for (i, mat) in mats.iter().enumerate() {
            let full = multiply(&view, mat);
            for p in node_parts(&m, i) {
                for v in p.positions.as_chunks::<3>().0 {
                    let w = apply(&full, [v[0], v[1], v[2]]);
                    for k in 0..3 {
                        lo[k] = lo[k].min(w[k]);
                        hi[k] = hi[k].max(w[k]);
                    }
                }
            }
        }
        for k in 0..3 {
            assert!(
                (lo[k] - st.min[k]).abs() < 1e-3 && (hi[k] - st.max[k]).abs() < 1e-3,
                "axis {k}: {lo:?} {hi:?} vs {:?} {:?}",
                st.min,
                st.max
            );
        }
    }

    #[test]
    fn bad_files_are_errors() {
        assert_eq!(parse(b"nope"), Err(RsmError::BadMagic));
        let good = model(&[("root", "", [0.0; 3], [[0.0; 3]; 3])]);
        assert!(parse(&good[..good.len() - 30]).is_err());
    }
}
