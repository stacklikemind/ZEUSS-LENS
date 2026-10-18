use crate::{bits, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EyeInput {
    pub sphere: f64,
    pub cylinder: f64,
    pub axis: i32,
    #[serde(default)]
    pub rx_offset_id: u8,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AxisPolicy {
    Exact,
    #[default]
    Native,
}

#[derive(Deserialize, Serialize)]
pub struct RxEntry {
    pub id: u16,
    pub sphere_cd: i32,
    pub cylinder_cd: i32,
}
#[derive(Deserialize, Serialize)]
pub struct AxisEntry {
    pub id: u8,
    pub degrees: i32,
}
#[derive(Deserialize, Serialize)]
pub struct Limits {
    pub cylinder_centidiopters: i32,
    pub min_sphere_centidiopters: i32,
    pub max_sphere_centidiopters: i32,
}
#[derive(Deserialize)]
pub struct Tables {
    pub rx: Vec<RxEntry>,
    pub axis: Vec<AxisEntry>,
    pub calibration_limits: Vec<Limits>,
}

pub fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../data/prescription.json"))
            .expect("bundled prescription data is valid")
    })
}

struct Eye {
    rx: u16,
    axis_id: u8,
    flip: bool,
    offset: u8,
    encoded: Value,
    calibration: Value,
}

fn quarter(value: f64, field: &str) -> Result<i32> {
    if !value.is_finite() || value.abs() > 100.0 || (value * 4.0).fract() != 0.0 {
        return Err(Error::input(format!(
            "{field} must be a finite exact multiple of 0.25 diopters within the supported table"
        )));
    }
    Ok((value * 100.0) as i32)
}

fn prepare_eye(
    input: Option<&EyeInput>,
    side: &str,
    policy: AxisPolicy,
    warnings: &mut Vec<String>,
) -> Result<Eye> {
    let Some(input) = input else {
        return Ok(Eye {
            rx: 0,
            axis_id: 0,
            flip: false,
            offset: 0,
            encoded: Value::Null,
            calibration: Value::Null,
        });
    };
    if !(0..=180).contains(&input.axis) {
        return Err(Error::input(format!(
            "{side} axis must be an integer from 0 through 180"
        )));
    }
    if input.rx_offset_id > 3 {
        return Err(Error::input(format!(
            "{side} rx_offset_id must be from 0 through 3"
        )));
    }
    let mut sphere = quarter(input.sphere, &format!("{side} sphere"))?;
    let mut cylinder = quarter(input.cylinder, &format!("{side} cylinder"))?;
    let mut axis = input.axis;
    let flip = cylinder > 0;
    if flip {
        sphere += cylinder;
        cylinder = -cylinder;
        axis += 90;
        if axis > 180 {
            axis -= 180;
        }
    }
    let table = tables();
    let entry = table
        .rx
        .iter()
        .find(|x| x.sphere_cd == sphere && x.cylinder_cd == cylinder)
        .ok_or_else(|| {
            Error::input(format!(
                "{side} sphere/cylinder pair is absent from the supported prescription table"
            ))
        })?;
    let canonical_axis = ((axis + 2) / 4 % 45) * 4;
    let axis_id = table
        .axis
        .iter()
        .find(|x| x.degrees == canonical_axis)
        .expect("axis grid is complete")
        .id;
    let display_axis = if flip {
        (canonical_axis - 90).rem_euclid(180)
    } else {
        canonical_axis
    };
    if input.axis.rem_euclid(180) != display_axis {
        if policy == AxisPolicy::Exact {
            return Err(Error { code: "AXIS_QUANTIZATION", message: format!("{side} axis {} would encode as {display_axis}; exact policy forbids this change", input.axis) });
        }
        warnings.push(format!(
            "{side} axis {}° encodes as {display_axis}° on the format's axis grid{}.",
            input.axis,
            if cylinder == 0 {
                " (axis has no optical effect for zero cylinder)"
            } else {
                ""
            }
        ));
    }
    let encoded = json!({
        "sphere": if flip { (sphere+cylinder) as f64/100.0 } else { sphere as f64/100.0 },
        "cylinder": if flip { -cylinder as f64/100.0 } else { cylinder as f64/100.0 },
        "axis": display_axis, "rx_id": entry.id, "axis_id":axis_id,
        "rx_offset_id":input.rx_offset_id, "sign_flip":flip,
        "canonical":{"sphere":sphere as f64/100.0,"cylinder":cylinder as f64/100.0,"axis":canonical_axis}
    });
    let calibration = if let Some(limits) = table
        .calibration_limits
        .iter()
        .find(|x| x.cylinder_centidiopters == cylinder)
    {
        let adjusted = if input.rx_offset_id > 0 {
            let q = sphere as f64 / 100.0 + input.rx_offset_id as f64 / 4.0;
            ((q / (1.0 + 0.003 * q) * 4.0).round() * 25.0) as i32
        } else {
            sphere
        };
        let calibrated_sphere = adjusted.clamp(
            limits.min_sphere_centidiopters,
            limits.max_sphere_centidiopters,
        );
        let calibrated_id = table
            .rx
            .iter()
            .find(|x| x.sphere_cd == calibrated_sphere && x.cylinder_cd == cylinder)
            .ok_or_else(|| {
                Error::internal("Calibration result is missing from the supplied table")
            })?
            .id;
        let changed = calibrated_sphere != sphere;
        let clamped = calibrated_sphere != adjusted;
        if changed {
            warnings.push(format!("{side} calibration uses sphere {:+.2} D and cylinder {:+.2} D after the explicit offset and calibration limits; this differs from the encoded canonical prescription and is not an equivalent prescription.", calibrated_sphere as f64/100.0, cylinder as f64/100.0));
        }
        json!({"available":true,"sphere":calibrated_sphere as f64/100.0,"cylinder":cylinder as f64/100.0,"rx_id":calibrated_id,"changed":changed,"clamped":clamped,"offset_diopters":input.rx_offset_id as f64/4.0})
    } else {
        warnings.push(format!("{side} calibration is unavailable: no calibration sphere limits are supplied for cylinder {:+.2} D.", cylinder as f64/100.0));
        json!({"available":false,"reason":"No calibration range is supplied for this cylinder"})
    };
    Ok(Eye {
        rx: entry.id,
        axis_id,
        flip,
        offset: input.rx_offset_id,
        encoded,
        calibration,
    })
}

pub struct Prepared {
    pub payload: [u8; 16],
    pub entered: Value,
    pub encoded: Value,
    pub calibration: Value,
    pub warnings: Vec<String>,
}

pub fn prepare(
    left: Option<&EyeInput>,
    right: Option<&EyeInput>,
    policy: AxisPolicy,
    lens_type: u8,
    random_bits: u8,
    secret_hex: &str,
) -> Result<Prepared> {
    if left.is_none() && right.is_none() {
        return Err(Error::input("At least one eye must be present"));
    }
    if !(1..=3).contains(&lens_type) {
        return Err(Error::input("lens_type must be 1, 2, or 3"));
    }
    if random_bits > 3 {
        return Err(Error::input("random_bits must be from 0 through 3"));
    }
    let secret = bits::parse_hex::<10>(secret_hex, "secret_hex")?;
    let mut warnings = Vec::new();
    let l = prepare_eye(left, "Left", policy, &mut warnings)?;
    let r = prepare_eye(right, "Right", policy, &mut warnings)?;
    let mut payload_bits = Vec::with_capacity(128);
    for (value, width) in [
        (5, 4),
        (l.flip as u32, 1),
        (r.flip as u32, 1),
        (l.offset as u32, 2),
        (r.offset as u32, 2),
        (lens_type as u32, 2),
        (l.rx as u32, 11),
        (l.axis_id as u32, 6),
        (r.rx as u32, 11),
        (r.axis_id as u32, 6),
        (random_bits as u32, 2),
    ] {
        bits::push_number(&mut payload_bits, value, width);
    }
    payload_bits.extend(bits::unpack(&secret, 8));
    let payload: [u8; 16] = bits::pack(&payload_bits, 8)
        .try_into()
        .expect("fixed 128-bit payload");
    Ok(Prepared {
        payload,
        entered: json!({"left":left,"right":right}),
        encoded: json!({"left":l.encoded,"right":r.encoded}),
        calibration: json!({"left":l.calibration,"right":r.calibration}),
        warnings,
    })
}

pub fn config() -> Value {
    let t = tables();
    json!({
        "abi_version":1,"payload_version":5,"default_lens_type":3,
        "lens_types":[{"id":1,"name":"reader"},{"id":2,"name":"developer"},{"id":3,"name":"personal"}],
        "axis":{"min":0,"max":180,"integer":true,"canonical_step":4,"policies":["native","exact"],"default_policy":"native"},
        "diopter_step":0.25,"rx_offset_ids":[0,1,2,3],"rx_offset_diopters":[0.0,0.25,0.5,0.75],
        "canonical_sphere_range":[-11.0,7.0],"canonical_cylinder_range":[-4.75,0.0],
        "supported_pairs":t.rx,"axis_table":t.axis,"calibration_limits":t.calibration_limits,
        "secret_bytes":10,"max_request_bytes":crate::MAX_REQUEST_BYTES,
        "verification":{"symbol_bits":true,"camera_scanner":false,"hardware_enrollment":false}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SECRET: &str = "0102030405060708090a";
    fn eye(s: f64, c: f64, a: i32) -> EyeInput {
        EyeInput {
            sphere: s,
            cylinder: c,
            axis: a,
            rx_offset_id: 0,
        }
    }
    #[test]
    fn absent_is_distinct_from_present_zero_power() {
        let p = prepare(
            Some(&eye(0.0, 0.0, 0)),
            None,
            AxisPolicy::Native,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_ne!(p.encoded["left"]["rx_id"], 0);
        assert!(p.encoded["right"].is_null());
        assert!(prepare(None, None, AxisPolicy::Native, 3, 0, SECRET).is_err());
    }
    #[test]
    fn positive_cylinder_and_ties_are_canonicalized() {
        let p = prepare(
            Some(&eye(-3.25, 2.75, 80)),
            None,
            AxisPolicy::Native,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_eq!(p.encoded["left"]["canonical"]["axis"], 172);
        assert_eq!(p.encoded["left"]["axis"], 82);
        assert_eq!(p.encoded["left"]["sphere"], -3.25);
        assert_eq!(p.encoded["left"]["canonical"]["sphere"], -0.5);
        assert!(prepare(
            Some(&eye(-3.25, 2.75, 80)),
            None,
            AxisPolicy::Exact,
            3,
            0,
            SECRET
        )
        .is_err());
        assert!(prepare(
            Some(&eye(0.0, -1.0, 180)),
            None,
            AxisPolicy::Exact,
            3,
            0,
            SECRET
        )
        .is_ok());
    }
    #[test]
    fn invalid_fraction_and_offset_are_rejected() {
        assert!(prepare(
            Some(&eye(-0.1, -1.0, 0)),
            None,
            AxisPolicy::Native,
            3,
            0,
            SECRET
        )
        .is_err());
        let mut e = eye(0.0, 0.0, 0);
        e.rx_offset_id = 4;
        assert!(prepare(Some(&e), None, AxisPolicy::Native, 3, 0, SECRET).is_err());
    }

    #[test]
    fn calibration_reports_explicit_offset_clamp_and_missing_range() {
        let mut offset_eye = eye(-8.0, -1.0, 0);
        offset_eye.rx_offset_id = 2;
        let offset = prepare(Some(&offset_eye), None, AxisPolicy::Native, 3, 0, SECRET).unwrap();
        assert_eq!(offset.encoded["left"]["sphere"], -8.0);
        assert_eq!(offset.calibration["left"]["sphere"], -7.75);
        assert_eq!(offset.calibration["left"]["changed"], true);
        let clamped = prepare(
            Some(&eye(2.25, -4.0, 0)),
            None,
            AxisPolicy::Native,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_eq!(clamped.encoded["left"]["sphere"], 2.25);
        assert_eq!(clamped.calibration["left"]["sphere"], 1.75);
        assert_eq!(clamped.calibration["left"]["clamped"], true);
        let missing = prepare(
            Some(&eye(0.0, -4.75, 0)),
            None,
            AxisPolicy::Native,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_eq!(missing.calibration["left"]["available"], false);
        assert!(!missing.warnings.is_empty());
    }
}
