use crate::{
    bits::{self, ARC_FIELD, ARC_POLY, GAP_FIELD, GAP_POLY, META_FIELD},
    clustering::{self, Clusters, RING_SIZES},
    Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fmt::Write, sync::OnceLock};

#[derive(Deserialize)]
struct Positions {
    metadata_positions: Vec<usize>,
    gap_positions: Vec<usize>,
}
fn positions() -> &'static Positions {
    static POSITIONS: OnceLock<Positions> = OnceLock::new();
    POSITIONS.get_or_init(|| {
        let p: Positions = serde_json::from_str(include_str!("../data/symbol.json"))
            .expect("bundled spatial data parse");
        assert_eq!(p.metadata_positions.len(), 16);
        assert_eq!(p.gap_positions.len(), 112);
        let mut slots = p.metadata_positions.clone();
        slots.extend(&p.gap_positions);
        slots.sort_unstable();
        assert_eq!(slots, (0..128).collect::<Vec<_>>());
        p
    })
}

pub fn scramble(payload: &[u8; 16]) -> Vec<u8> {
    let outer = outer(payload);
    let scrambled: Vec<u8> = outer.into_iter().rev().map(|b| b ^ 0xa5).collect();
    bits::unpack(&scrambled, 8)[..138].to_vec()
}

fn outer(payload: &[u8; 16]) -> Vec<u8> {
    let mut out = vec![0, 0];
    out.extend(payload);
    out
}

fn unscramble(message: &[u8]) -> Result<[u8; 16]> {
    let mut padded = message.to_vec();
    // The omitted suffix is the low six bits of scrambled zero prefix byte.
    padded.extend([1, 0, 0, 1, 0, 1]);
    let packed = bits::pack(&padded, 8);
    let outer: Vec<u8> = packed.into_iter().rev().map(|b| b ^ 0xa5).collect();
    if outer.len() != 18 || outer[..2] != [0, 0] {
        return Err(Error::symbol("Decoded outer prefix is unsupported"));
    }
    Ok(outer[2..].try_into().expect("16-byte payload"))
}

fn place(gap_bits: &[u8], inversion: u8) -> (Vec<u8>, Vec<u8>) {
    let p = positions();
    let metadata = META_FIELD.encode(&[1, inversion]);
    let mut spatial = vec![0; 128];
    for (&slot, bit) in p.metadata_positions.iter().zip(bits::unpack(&metadata, 4)) {
        spatial[slot] = bit;
    }
    for (i, &slot) in p.gap_positions.iter().take(105).enumerate() {
        spatial[slot] = gap_bits[i] ^ inversion;
    }
    for (i, &slot) in p.gap_positions[105..].iter().enumerate() {
        spatial[slot] = (0x11 >> i) & 1;
    }
    (spatial, metadata)
}

pub fn encode(payload: &[u8; 16]) -> Result<Value> {
    let message = scramble(payload);
    let gap_symbols = GAP_FIELD.encode(&bits::pack(
        &bits::append_checksum(&message[..90], GAP_POLY),
        5,
    ));
    let arc_symbols = ARC_FIELD.encode(&bits::pack(
        &bits::append_checksum(&message[90..], ARC_POLY),
        6,
    ));
    let gap_bits = bits::unpack(&gap_symbols, 5);
    let mut options: Vec<_> = (0..=1)
        .map(|inversion| {
            let (gaps, metadata) = place(&gap_bits, inversion);
            let count = gaps.iter().filter(|&&b| b == 0).count();
            (count, inversion, gaps, metadata)
        })
        .collect();
    options.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (_, inversion, gaps, _) in options {
        let clustered = match clustering::cluster(&gaps) {
            Ok(c) => c,
            Err(e) if e.code == "TOPOLOGY" => continue,
            Err(e) => return Err(e),
        };
        let mut colors = vec![0; clustered.starts.len()];
        for (&component, bit) in clustered
            .data_components
            .iter()
            .zip(bits::unpack(&arc_symbols, 6))
        {
            colors[component] = bit;
        }
        for (j, &component) in clustered.redundant_components.iter().skip(1).enumerate() {
            colors[component] = gap_bits[j] ^ inversion;
        }
        let mut result = decode(&gaps, &colors)?;
        if result["payload_hex"] != bits::hex(payload) {
            return Err(Error::internal(
                "Generated symbol failed payload round trip",
            ));
        }
        result["svg"] = Value::String(render(&clustered, &colors));
        result["validation"]["payload_roundtrip"] = Value::Bool(true);
        return Ok(result);
    }
    Err(Error::topology(
        "Neither inversion produces a valid arc topology; use a fresh synthetic identity",
    ))
}

pub fn decode(gaps: &[u8], colors: &[u8]) -> Result<Value> {
    if gaps.len() != 128 || gaps.iter().any(|&b| b > 1) {
        return Err(Error::input(
            "gaps must contain exactly 128 binary integers",
        ));
    }
    if colors.len() > 128 || colors.iter().any(|&b| b > 1) {
        return Err(Error::input(
            "colors must contain at most 128 binary integers",
        ));
    }
    let p = positions();
    let metadata_bits: Vec<_> = p
        .metadata_positions
        .iter()
        .map(|&slot| gaps[slot])
        .collect();
    let metadata = bits::pack(&metadata_bits, 4);
    let meta_syndromes = META_FIELD.syndromes(&metadata);
    if meta_syndromes != [0, 0] {
        return Err(Error::symbol("Metadata Reed–Solomon syndromes are nonzero"));
    }
    if metadata[0] != 1 || metadata[1] > 1 {
        return Err(Error::symbol(
            "Unsupported metadata version or inversion flag",
        ));
    }
    let inversion = metadata[1];
    for (i, &slot) in p.gap_positions[105..].iter().enumerate() {
        if gaps[slot] != (0x11 >> i) & 1 {
            return Err(Error::symbol("Gap magic bits do not match"));
        }
    }
    let physical_gap_bits: Vec<_> = p.gap_positions[..105]
        .iter()
        .map(|&slot| gaps[slot])
        .collect();
    let gap_bits: Vec<_> = physical_gap_bits.iter().map(|&b| b ^ inversion).collect();
    let gap_symbols = bits::pack(&gap_bits, 5);
    let gap_syndromes = GAP_FIELD.syndromes(&gap_symbols);
    if gap_syndromes != [0, 0] {
        return Err(Error::symbol("Gap Reed–Solomon syndromes are nonzero"));
    }
    if bits::append_checksum(&gap_bits[..90], GAP_POLY) != gap_bits[..95]
        || bits::checksum(&gap_bits[..95], GAP_POLY)
            .iter()
            .any(|&b| b != 0)
    {
        return Err(Error::symbol("Gap checksum mismatch"));
    }
    let clustered = clustering::cluster(gaps)?;
    if colors.len() != clustered.starts.len() {
        return Err(Error::input(
            "colors length must equal the number of visible arc components",
        ));
    }
    let arc_bits: Vec<_> = clustered
        .data_components
        .iter()
        .map(|&id| colors[id])
        .collect();
    let arc_symbols = bits::pack(&arc_bits, 6);
    let arc_syndromes = ARC_FIELD.syndromes(&arc_symbols);
    if arc_syndromes != [0, 0] {
        return Err(Error::symbol("Arc Reed–Solomon syndromes are nonzero"));
    }
    if bits::append_checksum(&arc_bits[..48], ARC_POLY) != arc_bits[..54]
        || bits::checksum(&arc_bits[..54], ARC_POLY)
            .iter()
            .any(|&b| b != 0)
    {
        return Err(Error::symbol("Arc checksum mismatch"));
    }
    if colors[clustered.redundant_components[0]] != 0 {
        return Err(Error::symbol("First redundant component must be white"));
    }
    for (j, &id) in clustered.redundant_components.iter().skip(1).enumerate() {
        if colors[id] != physical_gap_bits[j] {
            return Err(Error::symbol("Redundant arc color mismatch"));
        }
    }
    let mut message = gap_bits[..90].to_vec();
    message.extend_from_slice(&arc_bits[..48]);
    let payload = unscramble(&message)?;
    if scramble(&payload) != message {
        return Err(Error::symbol("Scrambled payload round trip mismatch"));
    }
    Ok(json!({
        "payload_hex":bits::hex(&payload),"outer_hex":bits::hex(&outer(&payload)),"inverted":inversion==1,
        "gaps":gaps,"colors":colors,
        "metadata":{"symbols":metadata,"gap_symbols":gap_symbols,"arc_symbols":arc_symbols,"magic":17,"arc_count":colors.len(),"redundant_arc_count":clustered.redundant_components.len()},
        "validation":{"in_memory":true,"metadata_syndromes":meta_syndromes,"gap_syndromes":gap_syndromes,"arc_syndromes":arc_syndromes,"gap_checksum":true,"arc_checksum":true,"redundant_colors":true,"payload_roundtrip":true,"raster_verified":false,"hardware_verified":false}
    }))
}

fn render(clustered: &Clusters, colors: &[u8]) -> String {
    const RADII: [f64; 5] = [177.2016, 224.1012, 271.0008, 317.9004, 364.8];
    const ROTATIONS: [f64; 5] = [-78.0, -85.0, -70.0, -63.0, -70.0];
    const GAPS: [f64; 5] = [7.5, 5.6, 5.0, 4.2, 3.5];
    let mut svg=String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"880\" height=\"880\" viewBox=\"0 0 880 880\" role=\"img\" aria-label=\"Synthetic optical prescription code\"><rect width=\"880\" height=\"880\" fill=\"#ffffff\"/><circle cx=\"440\" cy=\"440\" r=\"400\" fill=\"#000000\"/><g fill=\"none\" stroke-width=\"23.5\" stroke-linecap=\"round\">");
    for (id, &(ring, start)) in clustered.starts.iter().enumerate() {
        let next = clustered
            .starts
            .get(id + 1)
            .filter(|&&(r, _)| r == ring)
            .map(|&(_, s)| s)
            .unwrap_or_else(|| {
                clustered
                    .starts
                    .iter()
                    .find(|&&(r, _)| r == ring)
                    .unwrap()
                    .1
                    + RING_SIZES[ring]
            });
        let begin = ROTATIONS[ring] + start as f64 * 360.0 / RING_SIZES[ring] as f64 + GAPS[ring];
        let end = ROTATIONS[ring] + next as f64 * 360.0 / RING_SIZES[ring] as f64 - GAPS[ring];
        let point = |angle: f64| {
            let radians = angle.to_radians();
            (
                440.0 + RADII[ring] * radians.cos(),
                440.0 + RADII[ring] * radians.sin(),
            )
        };
        let (x1, y1) = point(begin);
        let (x2, y2) = point(end);
        let color = if colors[id] == 0 {
            "#ffffff"
        } else {
            "#858585"
        };
        write!(svg,"<path d=\"M {x1:.6} {y1:.6} A {r:.4} {r:.4} 0 {large} 1 {x2:.6} {y2:.6}\" stroke=\"{color}\"/>",r=RADII[ring],large=u8::from(end-begin>180.0)).unwrap();
    }
    // Independently drawn simple headset emblem: a rounded visor, two eye panels, and a strap.
    svg.push_str("</g><circle cx=\"440\" cy=\"440\" r=\"107\" fill=\"#ffffff\"/><path d=\"M 365 424 Q 351 406 365 396 Q 440 365 515 396 Q 529 406 515 424\" fill=\"none\" stroke=\"#000000\" stroke-width=\"8\"/><path d=\"M 372 408 Q 440 395 508 408 Q 524 414 516 450 L 510 472 Q 504 483 486 477 L 454 464 Q 440 457 426 464 L 394 477 Q 376 483 370 472 L 364 450 Q 356 414 372 408 Z\" fill=\"#858585\" stroke=\"#000000\" stroke-width=\"8\" stroke-linejoin=\"round\"/><path d=\"M 384 423 Q 405 416 425 424 M 455 424 Q 475 416 496 423\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"6\" stroke-linecap=\"round\"/></svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_out_wraparound_colors_match() {
        let f: Value =
            serde_json::from_str(include_str!("../fixtures/regression-symbol-35.json")).unwrap();
        let payload = bits::parse_hex(f["payload_hex"].as_str().unwrap(), "fixture").unwrap();
        let result = encode(&payload).unwrap();
        assert_eq!(result["gaps"], f["gaps"]);
        assert_eq!(result["colors"], f["expected_colors"]);
        assert_eq!(result["inverted"], f["inverted"]);
    }
    #[test]
    fn all_black_box_symbols_match_exactly() {
        let fixture: Value =
            serde_json::from_str(include_str!("../fixtures/symbols.json")).unwrap();
        for vector in fixture["vectors"].as_array().unwrap() {
            let payload =
                bits::parse_hex(vector["payload_hex"].as_str().unwrap(), "fixture").unwrap();
            assert_eq!(json!(scramble(&payload)), vector["scrambled_bits"]);
            let result = encode(&payload).unwrap();
            for key in [
                "payload_hex",
                "outer_hex",
                "inverted",
                "gaps",
                "colors",
                "metadata",
            ] {
                assert_eq!(
                    result[key], vector[key],
                    "{key} for {}",
                    vector["payload_hex"]
                );
            }
        }
    }
    #[test]
    fn altered_bits_and_dimensions_are_rejected() {
        let result = encode(&[0; 16]).unwrap();
        let gaps: Vec<u8> = serde_json::from_value(result["gaps"].clone()).unwrap();
        let colors: Vec<u8> = serde_json::from_value(result["colors"].clone()).unwrap();
        for slot in 0..128 {
            let mut broken = gaps.clone();
            broken[slot] ^= 1;
            assert!(decode(&broken, &colors).is_err());
        }
        for slot in 0..colors.len() {
            let mut broken = colors.clone();
            broken[slot] ^= 1;
            assert!(decode(&gaps, &broken).is_err());
        }
        assert!(decode(&gaps[..127], &colors).is_err());
        assert!(decode(&gaps, &colors[..colors.len() - 1]).is_err());
    }
}
