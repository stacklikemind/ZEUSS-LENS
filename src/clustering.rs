use crate::{Error, Result};
use serde::Deserialize;
use std::sync::OnceLock;

pub const RING_SIZES: [usize; 5] = [17, 23, 26, 29, 33];

#[derive(Deserialize)]
struct Constants {
    ring_sizes: Vec<usize>,
    priority_order: Vec<usize>,
    neighbor_order_by_slot: Vec<Vec<usize>>,
}

fn constants() -> &'static Constants {
    static TABLE: OnceLock<Constants> = OnceLock::new();
    TABLE.get_or_init(|| {
        let c: Constants = serde_json::from_str(include_str!("../data/clustering.json"))
            .expect("bundled clustering constants parse");
        assert_eq!(c.ring_sizes, RING_SIZES);
        assert_eq!(c.priority_order.len(), 128);
        assert_eq!(c.neighbor_order_by_slot.len(), 128);
        let mut permutation = c.priority_order.clone();
        permutation.sort_unstable();
        assert_eq!(permutation, (0..128).collect::<Vec<_>>());
        for (rank, &slot) in c.priority_order.iter().enumerate() {
            let row = &c.neighbor_order_by_slot[slot];
            assert_eq!(row.first(), Some(&slot));
            let mut actual = row.clone();
            actual.sort_unstable();
            let mut expected = c.priority_order[rank..].to_vec();
            expected.sort_unstable();
            assert_eq!(actual, expected);
        }
        c
    })
}

#[derive(Debug)]
pub struct Clusters {
    pub starts: Vec<(usize, usize)>,
    #[cfg(test)]
    pub component_of_slot: Vec<usize>,
    #[cfg(test)]
    pub components: Vec<Vec<usize>>,
    pub data_components: Vec<usize>,
    pub redundant_components: Vec<usize>,
    #[cfg(test)]
    pub seed_slots: Vec<usize>,
}

pub fn cluster(gaps: &[u8]) -> Result<Clusters> {
    if gaps.len() != 128 || gaps.iter().any(|&b| b > 1) {
        return Err(Error::input(
            "gaps must contain exactly 128 binary integers",
        ));
    }
    if gaps.iter().filter(|&&b| b == 0).count() < 67 {
        return Err(Error::topology(
            "Symbol has fewer than 67 visible arc components",
        ));
    }
    let mut component_of_slot = vec![0; 128];
    let mut components: Vec<Vec<usize>> = Vec::new();
    let mut starts = Vec::new();
    let mut offset = 0;
    for (ring, &size) in RING_SIZES.iter().enumerate() {
        let ring_starts: Vec<usize> = (0..size).filter(|&i| gaps[offset + i] == 0).collect();
        if ring_starts.is_empty() {
            return Err(Error::topology(format!(
                "Ring {} has no arc start",
                ring + 1
            )));
        }
        for (i, &start) in ring_starts.iter().enumerate() {
            let end = if i + 1 < ring_starts.len() {
                ring_starts[i + 1]
            } else {
                ring_starts[0] + size
            };
            let id = components.len();
            let mut members: Vec<usize> = (start..end).map(|slot| offset + slot % size).collect();
            members.sort_unstable();
            for &slot in &members {
                component_of_slot[slot] = id;
            }
            components.push(members);
            starts.push((ring, start));
        }
        offset += size;
    }
    let c = constants();
    let mut used = vec![false; components.len()];
    let mut data_components = Vec::with_capacity(66);
    let mut seed_slots = Vec::with_capacity(11);
    for _ in 0..11 {
        let seed = *c
            .priority_order
            .iter()
            .find(|&&slot| !used[component_of_slot[slot]])
            .ok_or_else(|| Error::internal("Clustering seed exhausted"))?;
        seed_slots.push(seed);
        let group_end = data_components.len() + 6;
        for &slot in &c.neighbor_order_by_slot[seed] {
            let id = component_of_slot[slot];
            if !used[id] {
                used[id] = true;
                data_components.push(id);
                if data_components.len() == group_end {
                    break;
                }
            }
        }
        if data_components.len() != group_end {
            return Err(Error::internal("Clustering neighbor row exhausted"));
        }
    }
    let mut redundant_components: Vec<_> = (0..components.len()).filter(|&i| !used[i]).collect();
    // A wrapping component may own the earliest spatial slot of its ring.
    redundant_components.sort_unstable_by_key(|&id| components[id][0]);
    Ok(Clusters {
        starts,
        #[cfg(test)]
        component_of_slot,
        #[cfg(test)]
        components,
        data_components,
        redundant_components,
        #[cfg(test)]
        seed_slots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn held_out_wrapping_redundant_components() {
        let f: Value =
            serde_json::from_str(include_str!("../fixtures/regression-symbol-35.json")).unwrap();
        let gaps: Vec<u8> = serde_json::from_value(f["gaps"].clone()).unwrap();
        let got = cluster(&gaps).unwrap();
        assert_eq!(
            serde_json::json!(got.data_components),
            f["expected_data_components"]
        );
        assert_eq!(
            serde_json::json!(got.redundant_components),
            f["expected_redundant_components"]
        );
    }
    #[test]
    fn black_box_clustering_cases() {
        let f: Value = serde_json::from_str(include_str!("../fixtures/clustering.json")).unwrap();
        for case in f["cases"].as_array().unwrap() {
            let gaps: Vec<u8> = serde_json::from_value(case["gaps"].clone()).unwrap();
            let got = cluster(&gaps);
            if case.get("data_components").is_none() {
                assert!(got.is_err(), "{}", case["name"]);
                continue;
            }
            let got = got.unwrap();
            assert_eq!(
                serde_json::json!(got.component_of_slot),
                case["component_of_slot"]
            );
            assert_eq!(serde_json::json!(got.components), case["components"]);
            assert_eq!(
                serde_json::json!(got.data_components),
                case["data_components"]
            );
            assert_eq!(
                serde_json::json!(got.redundant_components),
                case["redundant_components"]
            );
            assert_eq!(serde_json::json!(got.seed_slots), case["seed_slots"]);
        }
    }
}
