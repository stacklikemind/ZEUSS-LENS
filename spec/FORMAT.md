# Circular optical-code interoperability format

This is a behavioral specification, not source code. Implement independently. Constants are supplied in `data/*.json`. All bit strings below are written in transmission order. "MSB first" means a byte's bit7 precedes bit0. No Apple code or operating-system service may be required at build time or runtime.

## Product/API scope

Create a standalone Rust crate (native library + CLI + wasm32-unknown-unknown cdylib) and static browser page. The browser uses Web Crypto to provide a fresh 10-byte synthetic identity and calls WASM locally. No server-side generation, external service or filesystem read is allowed during generation. Native CLI reads JSON from stdin and emits JSON. Core generation is deterministic given inputs; browser retries at most64 new identities only for topology errors. Do not change optical parameters to find a topology.

Freeze this JSON contract with the browser implementer:
- Request `{"op":"generate","left":{"sphere":-0.5,"cylinder":-2.75,"axis":170,"rx_offset_id":0},"right":{...},"axis_policy":"native","secret_hex":"20 hexadecimal characters"}`. Version5, lens_type3/personal, random_bits0 defaults. Optional lens_type1=reader,2=developer,3=personal; offsets0..3. Missing/null eye may mean absent, but both absent is rejected.
- Request `{"op":"encode_payload","payload_hex":"32 hex characters"}` generates a circular symbol from exactly16 bytes without interpreting its prescription version.
- Request `{"op":"config"}` returns supported input parameters/ranges.
- Request `{"op":"review","left":{...},"right":{...},"axis_policy":"native"}` previews both eyes independently, optional cylinder reduction and nominal calibration without a secret or symbol. Its result contains `eyes`, `can_generate`, `needs_approximation`, and the selection `method`. Per-eye fields are `status` (`exact`, `approximate`, or `unavailable`), original `entered`, candidate `encoded`, `calibration`, and `warnings`; unavailable eyes return an `error` and `code`. Both absent eyes set `can_generate` false.
- Request `{"op":"decode_symbol","gaps":[128 bits],"colors":[one bit per visible arc]}` is useful for independent interoperability tests and format checks, without claiming camera detection.
- Response envelope success `{"ok":true,"result":{...}}`; failure `{"ok":false,"error":{"code":"...","message":"..."}}`. Distinguish `TOPOLOGY` from bad input. Suggested result keys: `svg`, `payload_hex`, `outer_hex`, `inverted`, `gaps`, `colors`, `metadata`, `validation`, `entered`, `encoded`, `warnings`, `calibration`.

The raw WASM ABI may avoid wasm-bindgen. Implementer owns and documents allocation/deallocation and response lifetime; coordinate exact exports with browser implementer. Input sizes must be bounded and malformed JSON/NaN, invalid dimensions and invalid bits must return errors. No unbounded loops.

## Prescription input and payload version5

`data/prescription.json` contains mappings `(sphere_cd,cylinder_cd)->id` in integer hundredths of a diopter; axisID->degrees; calibration sphere limits by cylinder. These tables are arbitrary interoperability values, not equations. Never infer diopters directly from RXID.

Input sphere/cylinder must be finite exact multiples of0.25D and exactly match a table pair after canonicalization. Input axis must be an integer in0..180. Offsets are integers0..3, representing explicit VR adds0,.25,.50,.75D; default0 means no supplied add, not an age-based calculation. Default `diopter_policy=exact` rejects unsupported pairs. Explicit `diopter_policy=reduce_cylinder` keeps entered sphere and notation and selects the largest cylinder magnitude below the entered magnitude among supported pairs with that same entered sphere/sign, or fails if none exists. Search only the finite supplied table. Already supported pairs are unchanged. Original `entered` values are preserved, candidate values are `encoded`, and `profile_adjustments` records every cylinder change with side, field, entered, encoded and policy. Interfaces must preview and explicitly select this experiment; editing inputs clears the selection. Axis policy and numeric validation still apply. This selection rule is not a validated optical match. Never alter the selected optical profile to fix a topology error.

The canonical table uses non-positive cylinders. For positive cylinder input S,C,A, first use canonical sphere S+C, cylinder−C, and axis A+90 (subtract180 if greater than180); remember a sign-flip flag. Quantize canonical axis to the closest multiple of4, with exact halfway ties toward the larger value, and wrap the178..180 bucket to0. In arithmetic: bucket=floor((axis+2)/4) modulo45; canonical encoded axis=4*bucket. Convert via axis table. For positive-cylinder display, undo the sign convention: display sphere canonicalS+canonicalC, display cylinder−canonicalC, display axis=(canonicalAxis−90) modulo180. This produces an axis grid shifted by90 for positive-cylinder notation.

Return entered and encoded values. `axis_policy=exact` rejects a non-equivalent axis change modulo180; `native` accepts with an explicit warning. 0 and180 are equivalent axis representations. With cylinder0 the axis has no optical meaning, but the field is still stored. For absent eye, RXID0, axisID0, offset0 and flip0; do not confuse a present zero-power lens with absence.

Version5 field order, MSB-first, totaling128bits:
- version5:4bits
- left sign-flip:1; right sign-flip:1
- left offset:2; right offset:2
- lens_type:2
- left RXID:11; left axisID:6
- right RXID:11; right axisID:6
- random_bits:2 (default0)
- synthetic identity:80bits, exactly10 caller-provided bytes

Lens types:1 reader,2 developer,3 personal. UI defaults3. Synthetic identity does not provision factory calibration data or establish successful headset pairing. Never reuse someone else's code identity by default.

For calibration reporting (separate from displayed values): start with canonical sphere/cylinder. When offset>0, let q=S+offset/4; adjusted sphere is q/(1+0.003*q), rounded to0.25 using nearest with half ties away from zero. Cylinder is likewise quarter-rounded. Then clamp sphere to the limits for that cylinder and use the reverse RX table. With offset0, the same sphere clamp still applies. Show any change prominently; do not describe it as an equivalent prescription. No clinical recommendation is implied.

## Outer bytes and scrambling

The final outer stream is18bytes: two zero bytes followed by the16payload bytes. It represents URL coding version1, prefix index0 (`https://appclip.apple.com/rx?p=`) followed by six ignored padding bits and lowercase hexadecimal payload text. Do not request this URL.

To produce the138 message bits before checksums: conceptually the outer stream omits its six padding bits, leaving ten zero prefix bits followed by128payload bits. Insert six zero bits immediately after the first two bits of this138bit stream, giving144bits. Pack as18 MSB-first bytes, reverse byte order, XOR each byte with0xa5, then retain the first138bits. The inverse reconstructs prefix0+payload exactly.

Split the138bits into90 gap message bits and48 arc-color message bits.

## Checksums and error correction

A checksum polynomial is an explicit bit vector including any trailing zeros:
- gap: [1,0,1,1,0,0], degree5
- arc: [1,0,1,1,0,1,1], degree6

For a message, append degree zero bits, divide by the polynomial using binary polynomial long division, and append the resulting degree-bit remainder to the original message. A verification step that appends degree zeros to the codeword and divides again yields an all-zero remainder. Preserve the explicit degree; the gap polynomial's trailing zeros matter.

Pack checksummed gap bits into19 symbols of5bits, arc bits into9 symbols of6bits, both MSB first. Add two systematic Reed–Solomon parity symbols:
- gap: GF(32), primitive polynomial0x25, field generator2, generator roots alpha^1 and alpha^2; RS(21,19)
- arc: GF(64), primitive polynomial0x43, generator2, roots alpha^1 and alpha^2; RS(11,9)
- metadata: GF(16), primitive0x13, generator2, roots alpha^0 and alpha^1; RS(4,2)

All RS words are highest-degree coefficient first, data symbols followed by parity. Correct generated output has both syndromes exactlyzero; no correction is needed for generation or exact validation.

## Spatial gap placement

There are128 binary gap positions in five rings, concatenated from inner ring to outer ring. Ring position counts are17,23,26,29,33. A spatial bit0 starts a visible arc component; bit1 joins the adjacent positions into the same component. At least67 arc components are required. Component enumeration and selection are specified separately in CLUSTERING.md.

Choose inversion0 or1. Metadata data nibbles are [1,inversion]; RS-encode to4nibbles/16bits and place in `metadata_positions`. Encode gap RS word to105bits; XOR each with inversion and place in the first105 `gap_positions`. The last7 `gap_positions` carry magic0x11, least-significant bit first, unaffected by inversion. All128 positions are covered exactly once by metadata+gap+magic.

Evaluate both inversion choices. Prefer more visible components; for a tie prefer inversion1. If clustering rejects one, try the other. Return a topology error if neither works. The priority and grouping algorithm is specified in CLUSTERING.md; do not replace it with stored fixture answers.

## Colors

Each visible component has a color bit:0 white,1 gray. The clustering algorithm returns66 ordered payload component indexes plus remaining redundant indexes. Assign the66 MSB-first bits of the11-symbol arc codeword to the payload indexes. Set the first redundant component towhite. For redundant element1+j, assign physical gap-codeword bitj (after inversion, before its spatial placement). The native compatibility rule allows some mismatch, but generated output and exact validation must match all redundant bits.

## Rendering geometry

An880×880 SVG canvas has a white background and a black circle centered(440,440), radius400. All ring coordinates are relative to that center. Ring radii(inner toouter):177.2016,224.1012,271.0008,317.9004,364.8. Rotations in degrees clockwise from positive x:-78,-85,-70,-63,-70. Per-end angular gaps:7.5,5.6,5.0,4.2,3.5degrees. Visible arc stroke width23.5, round caps, white#ffffff or gray#858585.

For each ring enumerate start indexes (spatial bit0) in increasing index order. An arc begins at startIndex*360/count plus the per-end gap, and ends at the next start (wrapping through360) minus the per-end gap. Thus its span covers any intervening spatial bits1. Its color index follows this natural inner-toouter/start-index enumeration, irrespective of the separate clustering permutation. Draw increasing angles clockwise. Preserve white margin and high-quality vector edges.

Center circle is white radius107. Draw an independently authored simple gray headset silhouette with black outline within it. Do not copy a source logo path. The reference pixel decoder does not use this center mark; full hardware detector acceptance remains untested and cannot be guaranteed by these tests.

## Validation

Generated output must check its decoded symbol bits/payload, exact metadata/gap/arc syndromes, checksums and redundant colors. Distinguish in-memory verification from raster-image verification.

Document known limits: no guaranteed hardware enrollment, factory-record provisioning, authenticity, or camera scanner; finite optical table; axis rounding; calibration clamping; fresh synthetic identities; no clinical substitution recommendation.

## Clarifications

If an encodable RX table pair has no calibration-limit row (currently cylinder−4.75D), return calibration unavailable with an explicit warning. Do not infer or invent a limit. Encoding remains possible; it does not imply a valid downstream calibration path.

The WASM ABI agreed for the browser is: exported memory; `vc_alloc(len:u32)->u32`, `vc_free(ptr:u32,len:u32)`, `vc_process(ptr:u32,len:u32)->u32` returning a response pointer, `vc_response_len()->u32`, `vc_abi_version()->u32` returning1. Input is caller-owned and freed with the same length. Output is module-owned until the next processing call. Maximum input JSON is64KiB. Read output bytes before another processing call. Memory may grow; obtain a fresh typed-array view after calls that can allocate.
