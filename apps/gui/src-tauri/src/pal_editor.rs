//! Editing a palette file: single colours and hue/saturation/brightness of a range. Works on file
//! bytes, so `backend` can apply it to a staged copy.

use rgrf_formats::pal::Pal;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PalEdit {
    /// One colour, `[r, g, b]`.
    Set { index: usize, rgb: [u8; 3] },
    /// Hue shift in degrees, saturation and brightness as factors, for `from..=to`.
    Adjust {
        from: usize,
        to: usize,
        hue: f32,
        saturation: f32,
        brightness: f32,
    },
}

pub fn apply(data: &[u8], edit: &PalEdit) -> Result<Vec<u8>, String> {
    let mut pal = Pal::parse(data).map_err(|e| e.to_string())?;
    match edit {
        PalEdit::Set { index, rgb } => {
            let i = u8::try_from(*index).map_err(|_| "the colour number is 0 to 255")?;
            pal.set_color(i, *rgb);
        }
        PalEdit::Adjust {
            from,
            to,
            hue,
            saturation,
            brightness,
        } => {
            let (Ok(from), Ok(to)) = (u8::try_from(*from), u8::try_from(*to)) else {
                return Err("the colour numbers are 0 to 255".into());
            };
            if from > to {
                return Err("the first colour is above the last".into());
            }
            if !(hue.is_finite() && saturation.is_finite() && brightness.is_finite()) {
                return Err("hue, saturation and brightness must be numbers".into());
            }
            if *saturation < 0.0 || *brightness < 0.0 || hue.abs() > 3600.0 {
                return Err("saturation and brightness are factors of at least 0".into());
            }
            pal.adjust(from, to, *hue, *saturation, *brightness);
        }
    }
    Ok(pal.to_bytes())
}

/// The 256 colours as `RRGGBB`.
pub fn colors(data: &[u8]) -> Result<Vec<String>, String> {
    let pal = Pal::parse(data).map_err(|e| e.to_string())?;
    Ok((0..256)
        .map(|i| {
            format!(
                "{:02x}{:02x}{:02x}",
                pal.colors[4 * i],
                pal.colors[4 * i + 1],
                pal.colors[4 * i + 2]
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(v: serde_json::Value) -> PalEdit {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn colours_are_set_and_adjusted_and_bad_input_is_refused() {
        let pal = vec![0u8; 1024];
        let a = apply(
            &pal,
            &edit(serde_json::json!({"op": "set", "index": 7, "rgb": [255, 0, 0]})),
        )
        .unwrap();
        assert_eq!(colors(&a).unwrap()[7], "ff0000");
        let b = apply(
            &a,
            &edit(
                serde_json::json!({"op": "adjust", "from": 7, "to": 7, "hue": 120.0,
                "saturation": 1.0, "brightness": 1.0}),
            ),
        )
        .unwrap();
        assert_eq!(colors(&b).unwrap()[7], "00ff00");
        assert_eq!(colors(&b).unwrap()[6], "000000");
        for bad in [
            serde_json::json!({"op": "set", "index": 256, "rgb": [0, 0, 0]}),
            serde_json::json!({"op": "adjust", "from": 9, "to": 3, "hue": 0.0, "saturation": 1.0, "brightness": 1.0}),
            serde_json::json!({"op": "adjust", "from": 0, "to": 3, "hue": 0.0, "saturation": -1.0, "brightness": 1.0}),
        ] {
            assert!(apply(&a, &edit(bad.clone())).is_err(), "{bad}");
        }
        assert!(
            apply(
                &[1, 2, 3],
                &edit(serde_json::json!({"op": "set", "index": 0, "rgb": [0, 0, 0]}))
            )
            .is_err()
        );
    }
}
