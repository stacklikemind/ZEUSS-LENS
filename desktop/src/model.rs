use serde_json::{json, Value};

pub struct Output {
    pub report: Value,
    pub svg: String,
    pub png: Vec<u8>,
    pub attempts: usize,
}

pub fn eye(values: &[String]) -> Result<Value, String> {
    let number = |index: usize, label: &str| -> Result<f64, String> {
        let n: f64 = values[index]
            .trim()
            .parse()
            .map_err(|_| format!("{label} must be a number"))?;
        if !n.is_finite() {
            return Err(format!("{label} must be finite"));
        }
        Ok(n)
    };
    let axis = number(2, "Axis")?;
    let add = number(3, "VR add")?;
    let sphere = number(0, "Sphere")?;
    let cylinder = number(1, "Cylinder")?;
    if axis.fract() != 0.0 || !(0.0..=180.0).contains(&axis) {
        return Err("Axis must be a whole number from 0 to 180".into());
    }
    if !(0.0..=0.75).contains(&add) || (add * 4.0).fract() != 0.0 {
        return Err("VR add must be 0, 0.25, 0.50 or 0.75 D".into());
    }
    let sphere_cd = quarter_centidiopters(sphere, "Sphere")?;
    let cylinder_cd = quarter_centidiopters(cylinder, "Cylinder")?;
    if !supported_pairs().contains(&(sphere_cd, cylinder_cd)) {
        let mut choices: Vec<_> = supported_pairs()
            .iter()
            .filter(|(s, _)| *s == sphere_cd)
            .map(|(_, c)| *c)
            .collect();
        choices.sort_unstable();
        choices.dedup();
        let detail = if choices.is_empty() {
            format!("Sphere {sphere:+.2} D has no entries in the supported table")
        } else {
            format!(
                "At sphere {sphere:+.2} D, supported cylinder values: {}",
                choices
                    .iter()
                    .map(|c| format!("{:+.2}", *c as f64 / 100.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return Err(format!(
            "Sphere {sphere:+.2} D / cylinder {cylinder:+.2} D is outside the encoder's supported optical table. {detail}. Values are never changed automatically."
        ));
    }
    Ok(
        json!({"sphere":sphere,"cylinder":cylinder,"axis":axis as u16,"rx_offset_id":(add*4.0) as u8}),
    )
}

fn quarter_centidiopters(value: f64, name: &str) -> Result<i32, String> {
    if (value * 4.0).fract() != 0.0 {
        return Err(format!("{name} must use 0.25 D steps"));
    }
    Ok((value * 100.0) as i32)
}

fn supported_pairs() -> &'static Vec<(i32, i32)> {
    static PAIRS: std::sync::OnceLock<Vec<(i32, i32)>> = std::sync::OnceLock::new();
    PAIRS.get_or_init(|| {
        let config: Value =
            serde_json::from_str(&vision_code_wasm::process_json(br#"{"op":"config"}"#))
                .expect("encoder configuration");
        config["result"]["supported_pairs"]
            .as_array()
            .expect("supported optical pairs")
            .iter()
            .flat_map(|p| {
                let sphere = p["sphere_cd"].as_i64().expect("sphere") as i32;
                let cylinder = p["cylinder_cd"].as_i64().expect("cylinder") as i32;
                [(sphere, cylinder), (sphere + cylinder, -cylinder)]
            })
            .collect()
    })
}

pub fn generate(request: Value, fixed: Option<String>) -> Result<Output, String> {
    generate_with(request, fixed, || {
        let mut bytes = [0u8; 10];
        getrandom::fill(&mut bytes).map_err(|e| format!("Could not create identity: {e}"))?;
        Ok(bytes.iter().map(|v| format!("{v:02x}")).collect())
    })
}

fn generate_with(
    mut request: Value,
    fixed: Option<String>,
    mut entropy: impl FnMut() -> Result<String, String>,
) -> Result<Output, String> {
    let limit = if fixed.is_some() { 1 } else { 64 };
    for attempt in 1..=limit {
        request["secret_hex"] = json!(match &fixed {
            Some(v) => v.clone(),
            None => entropy()?,
        });
        let response: Value = serde_json::from_str(&vision_code_wasm::process_json(
            request.to_string().as_bytes(),
        ))
        .map_err(|e| e.to_string())?;
        if response["ok"] != true {
            if response["error"]["code"] == "TOPOLOGY" && attempt < limit {
                continue;
            }
            return Err(format!(
                "{}: {}",
                response["error"]["code"].as_str().unwrap_or("ERROR"),
                response["error"]["message"]
                    .as_str()
                    .unwrap_or("Generation failed")
            ));
        }
        let svg = response["result"]["svg"]
            .as_str()
            .ok_or("Missing SVG")?
            .to_owned();
        let png = rasterize(&svg)?;
        return Ok(Output {
            report: json!({"request":request,"result":response["result"],"attempts":attempt}),
            svg,
            png,
            attempts: attempt,
        });
    }
    Err("No usable topology after 64 identities".into())
}

fn rasterize(svg: &str) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(1760, 1760).ok_or("Could not allocate PNG")?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

pub fn neutral_request() -> Value {
    let eye = json!({"sphere":0,"cylinder":0,"axis":0,"rx_offset_id":0});
    json!({"op":"generate","right":eye,"left":eye,"axis_policy":"native","lens_type":3})
}

pub fn self_test() -> Result<(), String> {
    let output = generate(neutral_request(), Some("00010203040506070809".into()))?;
    let r = &output.report["result"];
    let decoded: Value = serde_json::from_str(&vision_code_wasm::process_json(
        json!({"op":"decode_symbol","gaps":r["gaps"],"colors":r["colors"]})
            .to_string()
            .as_bytes(),
    ))
    .map_err(|e| e.to_string())?;
    if decoded["ok"] != true || decoded["result"]["payload_hex"] != r["payload_hex"] {
        return Err("Payload round trip failed".into());
    }
    let png = image::load_from_memory(&output.png).map_err(|e| e.to_string())?;
    if png.width() != 1760 || png.height() != 1760 {
        return Err("PNG dimensions failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packaged_engine_and_png_round_trip() {
        self_test().unwrap();
    }
    #[test]
    fn invalid_optics_do_not_retry() {
        let mut request = neutral_request();
        request["right"]["sphere"] = json!(0.1);
        let mut calls = 0;
        assert!(generate_with(request, None, || {
            calls += 1;
            Ok("00010203040506070809".into())
        })
        .is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn invalid_fixed_identity_does_not_use_entropy() {
        assert!(
            generate_with(neutral_request(), Some("no".into()), || panic!(
                "entropy used"
            ))
            .is_err()
        );
    }
    #[test]
    fn numeric_validation() {
        for values in [
            ["NaN", "0", "0", "0"],
            ["0", "0", "1.5", "0"],
            ["0", "0", "0", "0.1"],
        ] {
            assert!(eye(&values.map(String::from)).is_err());
        }
        assert!(eye(&["-1.25", "-0.5", "170", "0.25"].map(String::from)).is_ok());
        for values in [
            ["-8.50", "-4.00", "12", "0"],
            ["-8.00", "-3.50", "173", "0"],
        ] {
            let error = eye(&values.map(String::from)).unwrap_err();
            assert!(error.contains("outside the encoder's supported optical table"));
        }
    }
}

/// Ranges come from the encoder's public configuration, including positive-cylinder equivalents.
pub fn parameter_hint(field: usize, sphere: &str, cylinder: &str) -> String {
    if field == 2 {
        return "Min 0 · max 180 · step 1°".into();
    }
    if field == 3 {
        return "Min 0 · max 0.75 · step 0.25 D".into();
    }
    let other = if field == 0 { cylinder } else { sphere }
        .trim()
        .parse::<f64>()
        .ok();
    let values: Vec<_> = supported_pairs()
        .iter()
        .filter(|(s, c)| {
            other.is_none_or(|v| {
                let other_cd = (v * 100.0).round() as i32;
                if field == 0 {
                    *c == other_cd
                } else {
                    *s == other_cd
                }
            })
        })
        .map(|(s, c)| if field == 0 { *s } else { *c })
        .collect();
    if values.is_empty() {
        return "No supported values for the other optical value".into();
    }
    let min = values.iter().min().unwrap();
    let max = values.iter().max().unwrap();
    format!(
        "Min {:+.2} · max {:+.2} D",
        *min as f64 / 100.0,
        *max as f64 / 100.0
    )
}
