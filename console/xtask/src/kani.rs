//! kani-shard: the share of the Kani harnesses that one runner of the
//! release gate's proofs job checks. It reads the `kani-list.json` that
//! `cargo kani list --format json` writes and prints the names of shard
//! `<index>` of `<count>`, one per line, for `--exact --harness`.
//!
//! The names are sorted and dealt out in turn, so every harness is in exactly
//! one shard and the harnesses of one module spread over all of them. A list
//! that does not add up to its own totals is refused, and so is a shard with
//! no harness: a harness left out would pass unchecked.

use std::collections::BTreeSet;

/// The names of shard `index` (1-based) of `count`, from the text of a
/// `kani-list.json`.
pub fn shard(list: &str, index: usize, count: usize) -> Result<Vec<String>, String> {
    if count == 0 || index == 0 || index > count {
        return Err(format!("no shard {index} of {count}"));
    }
    let names = harnesses(list)?;
    let mine: Vec<String> = names
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % count == index - 1)
        .map(|(_, n)| n)
        .collect();
    if mine.is_empty() {
        return Err(format!("shard {index} of {count} has no harness"));
    }
    Ok(mine)
}

/// Every harness the list names, sorted, each once. Two crates can hold a
/// harness of the same path; `--exact --harness` checks both.
fn harnesses(list: &str) -> Result<BTreeSet<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(list).map_err(|e| format!("not a kani list: {e}"))?;
    let mut names = BTreeSet::new();
    let mut listed = 0;
    let mut total = 0;
    for kind in ["standard-harnesses", "contract-harnesses"] {
        let files = v
            .get(kind)
            .and_then(|f| f.as_object())
            .ok_or_else(|| format!("the list has no {kind}"))?;
        for (file, ns) in files {
            let ns = ns
                .as_array()
                .ok_or_else(|| format!("{kind} of {file} is not a list"))?;
            for n in ns {
                let n = n
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| format!("a harness of {file} has no name"))?;
                names.insert(n.to_string());
                listed += 1;
            }
        }
        total += v
            .get("totals")
            .and_then(|t| t.get(kind))
            .and_then(|t| t.as_u64())
            .ok_or_else(|| format!("the list has no total of {kind}"))?;
    }
    if listed != total {
        return Err(format!(
            "the list names {listed} harnesses, its totals say {total}"
        ));
    }
    if names.is_empty() {
        return Err("the list has no harness".to_string());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(std: &[(&str, &[&str])], total: usize) -> String {
        let files: serde_json::Map<String, serde_json::Value> = std
            .iter()
            .map(|(f, ns)| (f.to_string(), serde_json::json!(ns)))
            .collect();
        serde_json::json!({
            "kani-version": "0.68.0",
            "file-version": "0.1",
            "standard-harnesses": files,
            "contract-harnesses": {},
            "contracts": [],
            "totals": {"standard-harnesses": total, "contract-harnesses": 0, "functions-under-contract": 0},
        })
        .to_string()
    }

    fn seven() -> String {
        list(
            &[
                ("core/src/slice.rs", &["slice::b", "slice::a", "slice::c"]),
                ("attach/src/zip.rs", &["zip::a", "zip::b"]),
                ("dispatch/src/place.rs", &["place::a", "place::b"]),
            ],
            7,
        )
    }

    #[test]
    fn the_shards_hold_every_harness_once() {
        let l = seven();
        for count in 1..=7 {
            let mut all: Vec<String> = (1..=count)
                .flat_map(|i| shard(&l, i, count).unwrap())
                .collect();
            all.sort();
            let want: Vec<String> = harnesses(&l).unwrap().into_iter().collect();
            assert_eq!(all, want, "{count} shards");
        }
    }

    #[test]
    fn a_module_spreads_over_the_shards() {
        let l = seven();
        // Sorted: place::a place::b slice::a slice::b slice::c zip::a zip::b
        assert_eq!(shard(&l, 1, 3).unwrap(), ["place::a", "slice::b", "zip::b"]);
        assert_eq!(shard(&l, 2, 3).unwrap(), ["place::b", "slice::c"]);
        assert_eq!(shard(&l, 3, 3).unwrap(), ["slice::a", "zip::a"]);
    }

    #[test]
    fn contract_harnesses_are_shared_out_too() {
        let l = serde_json::json!({
            "standard-harnesses": {"a.rs": ["a::x"]},
            "contract-harnesses": {"b.rs": ["b::y"]},
            "totals": {"standard-harnesses": 1, "contract-harnesses": 1},
        })
        .to_string();
        assert_eq!(shard(&l, 1, 2).unwrap(), ["a::x"]);
        assert_eq!(shard(&l, 2, 2).unwrap(), ["b::y"]);
    }

    #[test]
    fn a_name_in_two_crates_is_given_once() {
        let l = list(&[("a/src/m.rs", &["m::x"]), ("b/src/m.rs", &["m::x"])], 2);
        assert_eq!(shard(&l, 1, 1).unwrap(), ["m::x"]);
    }

    #[test]
    fn a_shard_out_of_range_is_refused() {
        let l = seven();
        for (i, n) in [(0, 2), (3, 2), (1, 0)] {
            assert_eq!(shard(&l, i, n), Err(format!("no shard {i} of {n}")));
        }
    }

    #[test]
    fn an_empty_shard_is_refused() {
        let l = list(&[("a.rs", &["a::x", "a::y"])], 2);
        assert_eq!(
            shard(&l, 3, 3),
            Err("shard 3 of 3 has no harness".to_string())
        );
    }

    #[test]
    fn a_list_that_does_not_add_up_is_refused() {
        let l = list(&[("a.rs", &["a::x"])], 2);
        assert_eq!(
            shard(&l, 1, 1),
            Err("the list names 1 harnesses, its totals say 2".to_string())
        );
    }

    #[test]
    fn a_broken_list_is_refused() {
        assert!(shard("{", 1, 1).unwrap_err().starts_with("not a kani list"));
        assert_eq!(
            shard("{}", 1, 1),
            Err("the list has no standard-harnesses".to_string())
        );
        let l = list(&[], 0);
        assert_eq!(shard(&l, 1, 1), Err("the list has no harness".to_string()));
        let l = serde_json::json!({
            "standard-harnesses": {"a.rs": [""]},
            "contract-harnesses": {},
            "totals": {"standard-harnesses": 1, "contract-harnesses": 0},
        })
        .to_string();
        assert_eq!(
            shard(&l, 1, 1),
            Err("a harness of a.rs has no name".to_string())
        );
    }
}
