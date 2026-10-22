# How a code is generated

zeuss-lens explores interoperability and compatibility with third-party optical inserts and lenses for Apple Vision Pro. It takes supported optical inputs and produces a circular symbol using an independently written Rust encoder. The same encoder runs as a native CLI or as WebAssembly in a static browser page.

The process is:

```text
Optical inputs + 10-byte synthetic identity
    → 128-bit version 5 payload
    → 138 transformed message bits
    → gap and color checksums + Reed–Solomon parity
    → gap placement, inversion choice, and arc-component ordering
    → five-ring SVG + validation report
```

This document explains the stages. The authoritative behavioral details are in [FORMAT.md](../spec/FORMAT.md), with component ordering in [CLUSTERING.md](../spec/CLUSTERING.md). Generating a symbol is separate from establishing headset acceptance, a manufacturer identity, or suitable optical calibration.

## 1. Validate and represent the optical inputs

Each present eye supplies sphere, cylinder, axis, and an explicit VR add. Sphere and cylinder must be exact quarter-diopter increments. The supported sphere/cylinder pairs come from a finite numeric lookup table; the pair's identifier is an arbitrary format value, not a diopter formula. Unsupported pairs are rejected by default. The optional `reduce_cylinder` policy keeps entered sphere and cylinder notation and reduces cylinder magnitude to the nearest encodable value toward zero. Interfaces preview that difference and require an explicit selection; edits clear it. Original inputs remain in `entered`, the code values in `encoded`, and cylinder changes in `profile_adjustments`. This is an experimental encoding rule, not a validated optical similarity metric.

The table uses non-positive cylinder notation. Positive-cylinder input is transposed before lookup:

```text
canonical sphere   = sphere + cylinder
canonical cylinder = −cylinder
canonical axis     = axis + 90°, wrapping within a 180° cycle
```

A sign-flip bit records that conversion so the result can also be displayed in the entered notation. Axis is quantized to a four-degree grid in canonical notation, with halfway ties rounded upward and the 178°–180° bucket wrapped to 0°. In positive-cylinder notation, undoing the transpose shifts that grid by 90°. The native axis policy reports rounding; the exact policy rejects a change that is not equivalent modulo 180°. For zero cylinder, the axis field is still stored even though it has no optical meaning.

The VR add is explicitly 0, +0.25, +0.50, or +0.75 D. It is not inferred from age. An absent eye has reserved zero fields; a present zero-power lens has its own table entry and is not treated as absent. Both eyes cannot be absent.

Calibration reporting is a separate calculation from the encoded inputs. It applies the specified add/vertex transformation when the add is nonzero, rounds to quarter diopters, and applies the specified sphere limits for that cylinder. With zero add, sphere limits still apply. Any adjustment is reported separately and is not described as an equivalent prescription. If a supported pair has no known calibration-limit row, calibration is reported as unavailable. Neither this calculation nor the resulting code supplies manufacturer calibration records.

## 2. Pack the 128-bit version 5 payload

The fields are packed most significant bit first, in this order:

| Field | Bits |
| --- | ---: |
| Version, value 5 | 4 |
| Left and right cylinder sign-flip flags | 1 + 1 |
| Left and right VR-add identifiers | 2 + 2 |
| Lens type: reader, developer, or personal | 2 |
| Left sphere/cylinder identifier and axis identifier | 11 + 6 |
| Right sphere/cylinder identifier and axis identifier | 11 + 6 |
| Random-bits field, default 0 | 2 |
| Synthetic identity | 80 |
| **Total** | **128** |

The browser uses Web Crypto to create a fresh 10-byte identity for each generation attempt. That identity changes the symbol's bits and topology. It is synthetic: randomness does not provision a factory record or make the code authentic. Advanced mode can supply a fixed identity for reproducible tests; the native API requires the caller to supply one.

## 3. Transform and protect the bits

The outer representation is 18 bytes: two zero bytes followed by the 16 payload bytes. Its format prefix corresponds to URL coding version 1 and prefix index 0. This is an encoding detail; the application does not open or request the encoded Apple URL.

Removing the six padding bits conceptually leaves ten zero prefix bits and 128 payload bits: 138 message bits. The transformation inserts six zero bits after the first two bits, packs the resulting 144 bits into 18 bytes, reverses the byte order, XORs each byte with `0xa5`, and takes the first 138 bits. This is a reversible format transformation, not encryption or an authenticity check.

The transformed message is split into two blocks:

| Block | Message bits | Checksum bits | Data symbols | Reed–Solomon codeword |
| --- | ---: | ---: | --- | --- |
| Gaps | 90 | 5 | 19 × 5 bits | RS(21,19) over GF(32): 105 bits |
| Arc colors | 48 | 6 | 9 × 6 bits | RS(11,9) over GF(64): 66 bits |

The checksums use binary polynomial division with the exact polynomials specified in the format, including the gap polynomial's trailing zeros. Each Reed–Solomon block appends two parity symbols to its data. A separate metadata block encodes two four-bit values—format metadata value 1 and the inversion flag—as RS(4,2) over GF(16), producing 16 bits.

These checks detect internal inconsistencies in generated or supplied symbol bits. The shipped decoder validates exact bits; it is not a camera scanner or a general damaged-image recovery system.

## 4. Place gaps and choose inversion

Five concentric rings contain 17, 23, 26, 29, and 33 slots, totaling 128. A zero at a slot starts a visible arc component. A one joins that slot to the preceding component, wrapping around within its ring.

Fixed position permutations place the protected data into those slots:

- 16 metadata bits;
- 105 gap-codeword bits, optionally inverted;
- 7 fixed marker bits carrying `0x11` least significant bit first.

Metadata records the inversion choice. The marker bits are not inverted. Both inversion choices are evaluated; the encoder prefers the one with more visible components and chooses inversion 1 on a tie. Each ring needs at least one component, and the whole symbol needs at least 67. If the preferred candidate fails the component-ordering requirements, the other candidate is tried.

If neither candidate is usable, the engine returns a `TOPOLOGY` error. In random-identity mode, the browser may try another identity, up to 64 attempts in total. It retries only topology errors and keeps the optical inputs unchanged. Fixed-identity mode tries once.

## 5. Order the components and assign colors

Components have a natural drawing order: inner ring to outer ring, then increasing start-slot index. The payload's color bits use a separate ordering determined by fixed, universal priority and neighbor tables.

The algorithm selects eleven ordered groups of six components. For each group, it finds the first unused component through the fixed slot-priority order, then follows that seed slot's fixed neighbor row until six unused components are selected. Selecting a component marks every slot it owns as used. Concatenating the groups gives the 66 component indexes that carry the protected arc-color bits.

These tables encode protocol preferences. They are not complete-symbol answers, and recomputing a geometric nearest-neighbor order does not reproduce all of their tie choices.

The remaining components carry redundancy. Their order is the first occurrence of each unused component while scanning slots from 0 through 127. Because a component can wrap around a ring, this is not always numeric component-ID order. The first redundant component is white; subsequent components repeat the gap-codeword bits after inversion but before spatial placement. Generated output requires exact agreement with this rule.

The two color values are white and gray. Both the color ordering and the redundant ordering matter even when the ring geometry looks plausible.

## 6. Render and validate

The renderer draws an 880 × 880 SVG with a white margin, a black circular background, and five rings of round-capped white or gray arcs. Fixed radii, rotations, angular gaps, and stroke width determine the geometry. The central headset illustration was independently authored; it is not a copied source logo.

Before returning a successful result, the engine decodes the in-memory gap and color bits back to the payload and checks metadata, Reed–Solomon syndromes, checksums, and redundant colors. The browser can export the SVG, a 1760 × 1760 PNG rasterization, or a JSON report containing inputs, encoded values, identity, validation, and attempt count.

The checks shown on the page operate on symbol bits. They do not prove that a camera can locate the symbol, that a headset will enroll it, or that a downstream system has the records it expects. Separate development-time raster comparisons are described in the project's [interoperability report](../spec/INTEROPERABILITY_REPORT.json); they are not part of the browser runtime and are not hardware acceptance tests.

## Local processing and privacy

The static page loads its Rust WebAssembly module and runs generation on the user's device. There is no server-side generation, analytics, remote font, or external service call. A hosting provider serves the page and module, but optical values and generated identities are not sent with generation requests because there are no such requests. The application does not save inputs or identities in browser storage.

Downloaded SVG, PNG, and JSON files are deliberate local exports. The JSON report includes the exact request and synthetic identity, so sharing an export shares its contents. A local server is also provided for use without a public hosting service.

## Implementation and scope

The encoder is a standalone Rust library shared by the browser and native frontends. Numeric format tables are embedded at compile time; [DATA_MANIFEST.json](../spec/DATA_MANIFEST.json) lists their hashes. Generation requires no manufacturer service or runtime.

This independent project is not affiliated with or endorsed by Apple or a lens manufacturer. Its purpose is compatibility research with third-party lenses. It makes no claim of manufacturer authenticity, factory calibration, clinical suitability, or successful use with a physical headset.
