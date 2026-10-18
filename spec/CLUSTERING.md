# Format 8 arc component ordering

This is an interoperability specification for the arc-color ordering of circular format 8 symbols. It describes mathematical objects, protocol constants and observed behavior. It contains no implementation source or instruction listings. The numeric constants and example results are in `fixtures/clustering.json`.

## Domain and slot numbering

The input is a vector of exactly 128 binary gap values. The five rings contain 17, 23, 26, 29 and 33 slots, in that order. Their global slot intervals are 0–16, 17–39, 40–65, 66–94 and 95–127. Within a ring, increasing slot numbers follow the drawing direction specified by the ring geometry.

A gap value of zero starts a visible arc component. A value of one continues the preceding component. Rings are cyclic: the slots before a ring's first zero belong to that ring's final component. Components never cross between rings.

For the supported format 8 domain, each ring must contain at least one zero, and the whole symbol must contain at least 67 zero values. Reject wrong-length vectors, nonbinary values, a ring without a zero, or fewer than 67 total components. The native decoding caller applies the component-count prerequisite; behavior of its internal clustering helper outside that prerequisite is not part of this specification.

## Components and their IDs

Enumerate all zero-valued slots in increasing global slot order. Assign consecutive component IDs starting at zero to those starts. A component consists of its start slot and every following continuation slot, stopping immediately before the next zero in the same ring. Slot membership wraps at the ring boundary.

The component ID therefore refers to the component's starting zero, even when that component also owns earlier-numbered slots through wraparound. When representing a component as a list of slot numbers, use increasing numerical order. The input has as many components as it has zero values.

For example, a 17-slot ring whose zero values occur at local slots 2, 5 and 12 has three components. Their local memberships, in component-ID order, are `[2,3,4]`, `[5,6,7,8,9,10,11]` and `[0,1,12,13,14,15,16]`.

Let C(i) be the component ID containing slot i. Let A(c) be the set of slots belonging to component c. These definitions completely determine the `component_of_slot` and `components` fields in the fixtures.

## Fixed protocol preference orders

`constants.priority_order` is a permutation of slot numbers 0–127. It determines which still-available slot seeds the next six-component group. Its first entries are 98, 114, 123, 106, 86 and 127.

`constants.neighbor_order_by_slot` contains 128 rows. Row s is the ordered list of candidate slots considered when slot s seeds a group. This is fixed protocol data, independent of payloads, gap values, arc lengths and component IDs.

If s has rank r in the priority permutation, its row has 128−r entries. It starts with s and contains each member of the priority suffix beginning at rank r exactly once. In particular, there are 8,256 neighbor-order entries in total. These invariants are useful for detecting a damaged constant file.

The row order is authoritative, including all preferences that would otherwise appear to be geometric ties. Apply no additional sorting or component-ID tie-break. Do not replace it with distances recomputed from the SVG geometry: such a substitution does not reproduce the observed ordering exactly. The lists have spatial locality, but this specification does not assume an unverified coordinate or distance formula.

These lists are universal per-slot ordering constants, not lookup tables of complete symbols or answers for particular payloads. They support arbitrary binary topologies within the stated domain through the procedure below.

## Selection of the 66 data components

Initially no component is used. Form eleven ordered groups, each containing six components.

For each group:

1. Its seed is the first slot in the fixed priority permutation whose component has not already been used.
2. Read the seed's fixed neighbor row in its given order. Ignore any slot whose component is already used. At the first remaining slot, append its component ID to the current group and mark the entire component used. Thus every slot in A(c) becomes unavailable when component c is selected.
3. Continue through the same row until the group contains six component IDs. Selecting one component can make several later slots in that row unavailable. Do not select a component twice, and do not restart the neighbor row from a newly selected component.
4. Preserve this within-group order, then choose the next group's seed using the priority permutation again.

Concatenate the eleven groups in selection order. The resulting 66 component IDs are `data_components`. Position 0 is the most significant bit of the first six-bit arc symbol; positions 1–5 are its subsequent bits. Positions 6–11 represent the second symbol, continuing through all eleven six-bit symbols.

The fixed neighbor rows contain only their priority suffix. This is sufficient: when a seed is chosen, every earlier-priority slot already belongs to a used component. With valid constants, valid component construction and at least 67 components, six available components remain for each of the eleven groups. Exhaustion indicates invalid input or damaged constants; do not silently emit a partial result.

## Redundant components and colors

After all 66 data components have been chosen, scan spatial slots in increasing numerical order, from 0 through 127. At each slot, consider its component ID. If that component is unused and has not already been included in the redundant list, append its ID. The resulting first-occurrence order is `redundant_components`. Equivalently, sort the unused components by their smallest owned slot. Its length is the total component count minus 66, and is at least one.

This is not always numeric component-ID order. A ring's final component may own leading slots through wraparound. If that component is unused, it appears before the other unused components of its ring. For example, when component 27 owns slots `[17,39]` and component 18 owns slot `[27]`, component 27 must precede component 18 in the redundant list. Priority rank, neighbor-row order and component size do not determine redundant order.

The first redundant component receives color bit zero, meaning white. All supplied real samples use this value; the inspected native redundant-color check skips this first component rather than testing it.

For each subsequent redundant component at list position j+1, use physical gap-codeword bit j as its color bit. “Physical gap-codeword” means the ordered 105-bit encoded gap block after applying the metadata inversion flag, before placement through the gap-slot permutation. It does not mean the 128-element spatial gap vector in numerical slot order. Color zero is white; color one is gray.

Generation uses exact agreement and the stated white/gray polarity. No tolerance or color-polarity search is required.

## Fixture interpretation

The fixture document contains normative constants and test cases. Each case includes the input `gaps`, component mapping and component membership lists. Valid cases also give the exact 66 `data_components`, the ordered `redundant_components` and the eleven `seed_slots`. Invalid examples give a rejection reason. The `redundant-wrap-regression` case specifically exercises first-occurrence ordering of a component that crosses a ring boundary.

The all-zero example creates 128 single-slot components and makes the selection order particularly easy to inspect. Its first three groups are `[98,68,99,97,69,67]`, `[114,82,115,113,83,81]` and `[123,90,122,124,89,91]`. Other examples cover joined components, cyclic wraparound, nonuniform ring membership and the 67-component boundary. The two insufficient-component examples require rejection even though each ring has a start.

## Verification scope

The fixtures exercise cyclic boundaries, component ordering, and minimum component counts. Generated symbols must satisfy the permutation, checksum, and redundant-color invariants above. These tests do not establish camera recognition or headset acceptance.
