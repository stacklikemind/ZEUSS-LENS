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

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiopterPolicy {
    #[default]
    Exact,
    ReduceCylinder,
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

fn profile_input(input: &EyeInput, side: &str, policy: DiopterPolicy) -> Result<EyeInput> {
    let sphere = quarter(input.sphere, &format!("{side} sphere"))?;
    let cylinder = quarter(input.cylinder, &format!("{side} cylinder"))?;
    let supported = |c: i32| {
        let (s, c) = if c > 0 { (sphere + c, -c) } else { (sphere, c) };
        tables()
            .rx
            .iter()
            .any(|entry| entry.sphere_cd == s && entry.cylinder_cd == c)
    };
    if supported(cylinder) {
        return Ok(input.clone());
    }
    if policy == DiopterPolicy::ReduceCylinder {
        // Enumerate the finite code table, not an optical-distance model.
        // Keep entered sphere/notation and reduce |cylinder| toward zero.
        let candidate = tables()
            .rx
            .iter()
            .map(|entry| {
                if cylinder > 0 {
                    (entry.sphere_cd + entry.cylinder_cd, -entry.cylinder_cd)
                } else {
                    (entry.sphere_cd, entry.cylinder_cd)
                }
            })
            .filter(|(s, c)| *s == sphere && c.abs() < cylinder.abs())
            .map(|(_, c)| c)
            .max_by_key(|c| c.abs());
        if let Some(c) = candidate {
            return Ok(EyeInput {
                cylinder: c as f64 / 100.0,
                ..input.clone()
            });
        }
        return Err(Error::input(format!("{side}: no encodable profile can keep sphere {:+.2} D while reducing cylinder. Check your entered values.", input.sphere)));
    }
    Err(Error::input(format!(
        "{side} sphere/cylinder pair is absent from the supported prescription table"
    )))
}

fn prepare_eye(
    input: Option<&EyeInput>,
    side: &str,
    policy: AxisPolicy,
    diopter_policy: DiopterPolicy,
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
    let selected = profile_input(input, side, diopter_policy)?;
    if selected.cylinder != input.cylinder {
        warnings.push(format!("{side}: experimental code profile changes cylinder from {:+.2} D to {:+.2} D while keeping sphere {:+.2} D. Physical inserts and entered values stay unchanged; tracking benefit is untested.", input.cylinder, selected.cylinder, input.sphere));
    }
    let input = &selected;
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
    pub profile_adjustments: Value,
}

pub fn prepare(
    left: Option<&EyeInput>,
    right: Option<&EyeInput>,
    policy: AxisPolicy,
    lens_type: u8,
    random_bits: u8,
    secret_hex: &str,
) -> Result<Prepared> {
    prepare_with_policy(
        left,
        right,
        policy,
        DiopterPolicy::Exact,
        lens_type,
        random_bits,
        secret_hex,
    )
}

pub fn prepare_with_policy(
    left: Option<&EyeInput>,
    right: Option<&EyeInput>,
    policy: AxisPolicy,
    diopter_policy: DiopterPolicy,
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
    let l = prepare_eye(left, "Left", policy, diopter_policy, &mut warnings)?;
    let r = prepare_eye(right, "Right", policy, diopter_policy, &mut warnings)?;
    let mut adjustments = Vec::new();
    for (side, input, eye) in [("left", left, &l), ("right", right, &r)] {
        if let Some(input) = input {
            if eye.encoded["cylinder"] != json!(input.cylinder) {
                adjustments.push(json!({"side":side,"field":"cylinder","entered":input.cylinder,"encoded":eye.encoded["cylinder"],"policy":"reduce_cylinder"}));
            }
        }
    }
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
        profile_adjustments: json!(adjustments),
    })
}

/// Review both eyes without entropy, a symbol, or a factory lookup.
pub fn review(left: Option<&EyeInput>, right: Option<&EyeInput>, policy: AxisPolicy) -> Value {
    let mut eyes = json!({});
    let mut can_generate = left.is_some() || right.is_some();
    let mut needs_approximation = false;
    for (side, label, input) in [("left", "Left", left), ("right", "Right", right)] {
        let mut warnings = Vec::new();
        match prepare_eye(
            input,
            label,
            policy,
            DiopterPolicy::ReduceCylinder,
            &mut warnings,
        ) {
            Ok(eye) => {
                let approximate =
                    input.is_some_and(|i| eye.encoded["cylinder"] != json!(i.cylinder));
                needs_approximation |= approximate;
                eyes[side] = json!({"status":if approximate {"approximate"} else {"exact"},"entered":input,"encoded":eye.encoded,"calibration":eye.calibration,"warnings":warnings});
            }
            Err(error) => {
                can_generate = false;
                eyes[side] =
                    json!({"status":"unavailable","error":error.message,"code":error.code});
            }
        }
    }
    json!({"eyes":eyes,"can_generate":can_generate,"needs_approximation":needs_approximation,
        "method":"Keep entered sphere and cylinder sign; reduce cylinder magnitude to the nearest encodable value toward zero. This is not a validated optical match."})
}

pub fn config() -> Value {
    let t = tables();
    json!({
        "abi_version":1,"payload_version":5,"default_lens_type":3,
        "lens_types":[{"id":1,"name":"reader"},{"id":2,"name":"developer"},{"id":3,"name":"personal"}],
        "axis":{"min":0,"max":180,"integer":true,"canonical_step":4,"policies":["native","exact"],"default_policy":"native"},
        "diopter_step":0.25,"rx_offset_ids":[0,1,2,3],"rx_offset_diopters":[0.0,0.25,0.5,0.75],
        "diopter_policies":["exact","reduce_cylinder"],"default_diopter_policy":"exact",
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
    fn approximate_profile_requires_opt_in_and_preserves_original() {
        let input = eye(-11.0, -3.75, 170);
        assert!(prepare(Some(&input), None, AxisPolicy::Native, 3, 0, SECRET).is_err());
        let preview = review(Some(&input), None, AxisPolicy::Native);
        assert_eq!(preview["can_generate"], true);
        assert_eq!(preview["needs_approximation"], true);
        let p = prepare_with_policy(
            Some(&input),
            None,
            AxisPolicy::Native,
            DiopterPolicy::ReduceCylinder,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_eq!(p.entered["left"]["cylinder"], -3.75);
        assert_eq!(p.encoded["left"]["sphere"], -11.0);
        assert_eq!(p.encoded["left"]["cylinder"], -1.75);
        assert_eq!(p.encoded["left"]["axis"], 172);
        assert_eq!(p.calibration["left"]["sphere"], -10.0);
        assert_eq!(preview["eyes"]["left"]["encoded"], p.encoded["left"]);
        assert_eq!(
            preview["eyes"]["left"]["calibration"],
            p.calibration["left"]
        );
        assert_eq!(p.profile_adjustments[0]["entered"], -3.75);
        assert_eq!(input.cylinder, -3.75);
    }
    #[test]
    fn supported_pairs_stay_identical_with_experimental_policy() {
        for entry in &tables().rx {
            let input = eye(
                entry.sphere_cd as f64 / 100.0,
                entry.cylinder_cd as f64 / 100.0,
                172,
            );
            let exact = prepare(Some(&input), None, AxisPolicy::Native, 3, 0, SECRET).unwrap();
            let alternative = prepare_with_policy(
                Some(&input),
                None,
                AxisPolicy::Native,
                DiopterPolicy::ReduceCylinder,
                3,
                0,
                SECRET,
            )
            .unwrap();
            assert_eq!(exact.payload, alternative.payload);
            assert_eq!(alternative.profile_adjustments, json!([]));
            assert_eq!(
                review(Some(&input), None, AxisPolicy::Native)["needs_approximation"],
                false
            );
        }
    }
    #[test]
    fn invalid_values_and_unavailable_spheres_have_no_profile() {
        for input in [
            eye(-20.0, -3.75, 170),
            eye(-11.1, -3.75, 170),
            eye(-11.0, -3.75, 181),
            eye(-11.0, f64::NAN, 170),
        ] {
            assert_eq!(
                review(Some(&input), None, AxisPolicy::Native)["can_generate"],
                false
            );
            assert!(prepare_with_policy(
                Some(&input),
                None,
                AxisPolicy::Native,
                DiopterPolicy::ReduceCylinder,
                3,
                0,
                SECRET
            )
            .is_err());
        }
        assert_eq!(
            review(None, None, AxisPolicy::Native)["can_generate"],
            false
        );
        assert_eq!(
            review(Some(&eye(-11.0, -3.75, 170)), None, AxisPolicy::Exact)["can_generate"],
            false
        );
    }
    #[test]
    fn positive_cylinder_and_independent_eyes_keep_notation_and_offsets() {
        let left = eye(6.0, 4.0, 170);
        let mut right = eye(-0.5, -2.75, 170);
        right.rx_offset_id = 3;
        let preview = review(Some(&left), Some(&right), AxisPolicy::Native);
        let p = prepare_with_policy(
            Some(&left),
            Some(&right),
            AxisPolicy::Native,
            DiopterPolicy::ReduceCylinder,
            3,
            0,
            SECRET,
        )
        .unwrap();
        assert_eq!(p.encoded["left"]["sphere"], 6.0);
        assert!(p.encoded["left"]["cylinder"].as_f64().unwrap() >= 0.0);
        assert!(p.encoded["left"]["cylinder"].as_f64().unwrap() < 4.0);
        assert_eq!(p.encoded["right"]["cylinder"], -2.75);
        assert_eq!(p.encoded["right"]["rx_offset_id"], 3);
        assert_eq!(
            preview["eyes"]["right"]["calibration"],
            p.calibration["right"]
        );
        assert_eq!(preview["eyes"]["right"]["status"], "exact");
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
