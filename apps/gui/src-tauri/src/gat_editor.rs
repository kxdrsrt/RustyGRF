//! Editing the cells of a GAT (walkability and height) from a rectangle. Works on file bytes, so
//! `backend` can apply it to a staged copy.

use rgrf_formats::gat::Gat;
use serde::Deserialize;

/// A rectangle of cells in game coordinates (x east, y north, origin south-west), corners in any
/// order, and what to set in it.
#[derive(Deserialize, Debug, Clone)]
pub struct GatEdit {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub kind: Option<i32>,
    pub height: Option<f32>,
}

/// The edited file and how many cells changed.
pub fn apply(data: &[u8], edit: &GatEdit) -> Result<(Vec<u8>, usize), String> {
    if edit.kind.is_none() && edit.height.is_none() {
        return Err(err!("err-gat-need"));
    }
    if edit
        .height
        .is_some_and(|h| !h.is_finite() || h.abs() > 10_000.0)
    {
        return Err(err!("err-gat-height"));
    }
    if edit.kind.is_some_and(|k| !(0..=6).contains(&k)) {
        return Err(err!("err-gat-type"));
    }
    let mut g = Gat::parse(data).map_err(|e| e.to_string())?;
    let changed = g
        .set_cells(
            (edit.x0, edit.y0),
            (edit.x1, edit.y1),
            edit.kind,
            edit.height,
        )
        .map_err(|e| e.to_string())?;
    Ok((g.to_bytes(), changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgrf_formats::gat::Cell;
    use rgrf_formats::version::Version;

    fn sample() -> Vec<u8> {
        let cell = |kind| Cell {
            heights: [0.0; 4],
            kind,
        };
        Gat {
            version: Version::new(1, 2),
            width: 4,
            height: 3,
            cells: (0..12).map(|i| cell(i % 2)).collect(),
        }
        .to_bytes()
    }

    #[test]
    fn a_rectangle_is_set_and_bad_input_is_refused() {
        let edit = |v: serde_json::Value| serde_json::from_value::<GatEdit>(v).unwrap();
        let (out, n) = apply(
            &sample(),
            &edit(
                serde_json::json!({"x0": 3, "y0": 2, "x1": 1, "y1": 0, "kind": 5, "height": 2.5}),
            ),
        )
        .unwrap();
        assert_eq!(n, 9, "a 3 x 3 rectangle, corners in any order");
        let g = Gat::parse(&out).unwrap();
        assert_eq!(g.cell(1, 0).unwrap().kind, 5);
        assert_eq!(g.cell(3, 2).unwrap().heights, [2.5; 4]);
        assert_eq!(g.cell(0, 0).unwrap().kind, 0, "outside the rectangle");
        for bad in [
            serde_json::json!({"x0": 0, "y0": 0, "x1": 1, "y1": 1}),
            serde_json::json!({"x0": 0, "y0": 0, "x1": 1, "y1": 1, "kind": 9}),
            serde_json::json!({"x0": 0, "y0": 0, "x1": 1, "y1": 1, "height": 1e9}),
            serde_json::json!({"x0": 50, "y0": 0, "x1": 60, "y1": 1, "kind": 1}),
        ] {
            assert!(apply(&sample(), &edit(bad.clone())).is_err(), "{bad}");
        }
        assert!(
            apply(
                &[1, 2, 3],
                &edit(serde_json::json!({"x0": 0, "y0": 0, "x1": 0, "y1": 0, "kind": 1}))
            )
            .is_err()
        );
    }
}
