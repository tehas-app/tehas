//! The graph of the holds the dispatcher's last poll read: which issue waits
//! on which. Pure, so `tehas blockers` and its tests ask the same function.
//!
//! An edge `(a, b)` says the poll holds `a` while `b` is open. An issue in
//! `linked` is held by blocked-by links whose issues the poll does not name.

use std::collections::{BTreeMap, BTreeSet};

/// What the graph says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    /// Each cycle among the edges, as its issues lowest first; the cycles in
    /// order of their lowest issue. An issue that names itself is a cycle of one.
    pub cycles: Vec<Vec<u64>>,
    /// For each held issue, the issues at the ends of its chains: an issue it
    /// reaches that names no blocker, or one held by a link. Lowest first.
    pub ends: BTreeMap<u64, Vec<u64>>,
    /// The held issues that reach a cycle: they never start until it is broken.
    pub reach_cycle: BTreeSet<u64>,
}

/// The cycles, the chain ends and what reaches a cycle, for every issue that
/// has an edge or a link.
pub fn analyse(edges: &[(u64, u64)], linked: &BTreeSet<u64>) -> Graph {
    let mut out: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for &(a, b) in edges {
        out.entry(a).or_default().insert(b);
    }
    let cycles = cycles(&out);
    let cyclic: BTreeSet<u64> = cycles.iter().flatten().copied().collect();
    let held: BTreeSet<u64> = out.keys().chain(linked).copied().collect();
    let mut g = Graph {
        cycles,
        ..Graph::default()
    };
    for &x in &held {
        let mut seen = BTreeSet::from([x]);
        let mut todo = vec![x];
        let mut ends = BTreeSet::new();
        while let Some(y) = todo.pop() {
            if cyclic.contains(&y) {
                g.reach_cycle.insert(x);
            }
            if linked.contains(&y) {
                ends.insert(y);
            }
            match out.get(&y) {
                None if y != x => {
                    ends.insert(y);
                }
                None => {}
                Some(next) => {
                    for &z in next {
                        if seen.insert(z) {
                            todo.push(z);
                        }
                    }
                }
            }
        }
        g.ends.insert(x, ends.into_iter().collect());
    }
    g
}

/// The strongly connected parts of more than one issue, and the issues that
/// name themselves (Tarjan's, without recursion).
fn cycles(out: &BTreeMap<u64, BTreeSet<u64>>) -> Vec<Vec<u64>> {
    let nodes: BTreeSet<u64> = out
        .iter()
        .flat_map(|(a, bs)| std::iter::once(*a).chain(bs.iter().copied()))
        .collect();
    let mut index: BTreeMap<u64, usize> = BTreeMap::new();
    let mut low: BTreeMap<u64, usize> = BTreeMap::new();
    let mut on: BTreeSet<u64> = BTreeSet::new();
    let mut stack: Vec<u64> = Vec::new();
    let mut found: Vec<Vec<u64>> = Vec::new();
    let empty = BTreeSet::new();
    for &root in &nodes {
        if index.contains_key(&root) {
            continue;
        }
        // (node, the successors left to visit)
        let mut work: Vec<(u64, Vec<u64>)> = Vec::new();
        let open = |n: u64,
                    index: &mut BTreeMap<u64, usize>,
                    low: &mut BTreeMap<u64, usize>,
                    on: &mut BTreeSet<u64>,
                    stack: &mut Vec<u64>| {
            let i = index.len();
            index.insert(n, i);
            low.insert(n, i);
            on.insert(n);
            stack.push(n);
        };
        open(root, &mut index, &mut low, &mut on, &mut stack);
        work.push((
            root,
            out.get(&root)
                .unwrap_or(&empty)
                .iter()
                .rev()
                .copied()
                .collect(),
        ));
        while let Some((n, rest)) = work.last_mut() {
            let n = *n;
            if let Some(m) = rest.pop() {
                if !index.contains_key(&m) {
                    open(m, &mut index, &mut low, &mut on, &mut stack);
                    work.push((
                        m,
                        out.get(&m)
                            .unwrap_or(&empty)
                            .iter()
                            .rev()
                            .copied()
                            .collect(),
                    ));
                } else if on.contains(&m) {
                    let l = low[&n].min(index[&m]);
                    low.insert(n, l);
                }
                continue;
            }
            work.pop();
            if let Some((p, _)) = work.last() {
                let l = low[p].min(low[&n]);
                low.insert(*p, l);
            }
            if low[&n] == index[&n] {
                let mut part = Vec::new();
                while let Some(m) = stack.pop() {
                    on.remove(&m);
                    part.push(m);
                    if m == n {
                        break;
                    }
                }
                let selfloop = out.get(&n).is_some_and(|s| s.contains(&n));
                if part.len() > 1 || selfloop {
                    part.sort_unstable();
                    found.push(part);
                }
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(a: &[u64]) -> BTreeSet<u64> {
        a.iter().copied().collect()
    }

    #[test]
    fn a_chain_ends_at_the_issue_that_names_no_blocker() {
        let g = analyse(&[(1, 2), (2, 3), (4, 3)], &set(&[]));
        assert!(g.cycles.is_empty());
        assert_eq!(g.ends[&1], vec![3]);
        assert_eq!(g.ends[&2], vec![3]);
        assert_eq!(g.ends[&4], vec![3]);
        assert!(!g.ends.contains_key(&3));
    }

    #[test]
    fn a_cycle_is_found_and_has_no_end_of_its_own() {
        let g = analyse(&[(1, 2), (2, 1), (5, 1), (3, 3)], &set(&[]));
        assert_eq!(g.cycles, vec![vec![1, 2], vec![3]]);
        assert_eq!(g.ends[&5], Vec::<u64>::new());
        assert_eq!(g.reach_cycle, set(&[1, 2, 3, 5]));
    }

    #[test]
    fn a_cycle_with_a_way_out_ends_there() {
        let g = analyse(&[(1, 2), (2, 1), (2, 9)], &set(&[]));
        assert_eq!(g.cycles, vec![vec![1, 2]]);
        assert_eq!(g.ends[&1], vec![9]);
    }

    #[test]
    fn a_link_is_an_end_at_the_issue_it_holds() {
        let g = analyse(&[(1, 2)], &set(&[2, 7]));
        assert_eq!(g.ends[&1], vec![2]);
        assert_eq!(g.ends[&2], vec![2]);
        assert_eq!(g.ends[&7], vec![7]);
    }

    /// The definitions, asked of every graph of up to four issues: an issue
    /// is in a cycle when it reaches itself by one edge or more, and an end of
    /// `x` is an issue `x` reaches by none or more that is linked, or that
    /// names nothing and is not `x`.
    #[test]
    fn every_graph_of_four_issues_agrees_with_the_definitions() {
        let nodes = [1u64, 2, 3, 4];
        let pairs: Vec<(u64, u64)> = nodes
            .iter()
            .flat_map(|a| nodes.iter().map(move |b| (*a, *b)))
            .collect();
        for mask in 0u32..(1 << pairs.len()) {
            let edges: Vec<(u64, u64)> = pairs
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, p)| *p)
                .collect();
            let linked = set(&[(mask % 5) as u64]);
            let g = analyse(&edges, &linked);
            let reach = |x: u64, least: usize| -> BTreeSet<u64> {
                let mut r = BTreeSet::new();
                let mut front: BTreeSet<u64> = if least == 0 {
                    set(&[x])
                } else {
                    BTreeSet::new()
                };
                if least == 1 {
                    front = edges.iter().filter(|e| e.0 == x).map(|e| e.1).collect();
                }
                while !front.is_empty() {
                    let next: BTreeSet<u64> = front
                        .iter()
                        .flat_map(|y| edges.iter().filter(move |e| e.0 == *y).map(|e| e.1))
                        .filter(|z| !r.contains(z) && !front.contains(z))
                        .collect();
                    r.extend(front);
                    front = next;
                }
                r
            };
            let cyclic: BTreeSet<u64> = nodes
                .iter()
                .copied()
                .filter(|x| reach(*x, 1).contains(x))
                .collect();
            assert_eq!(
                g.cycles.iter().flatten().copied().collect::<BTreeSet<_>>(),
                cyclic,
                "{edges:?}"
            );
            for c in &g.cycles {
                for a in c {
                    for b in c {
                        assert!(reach(*a, 0).contains(b), "{edges:?}");
                    }
                }
            }
            let held: BTreeSet<u64> = edges
                .iter()
                .map(|e| e.0)
                .chain(linked.iter().copied())
                .collect();
            assert_eq!(
                g.ends.keys().copied().collect::<BTreeSet<_>>(),
                held,
                "{edges:?}"
            );
            for x in &held {
                let r = reach(*x, 0);
                let ends: Vec<u64> = r
                    .iter()
                    .copied()
                    .filter(|y| {
                        linked.contains(y) || (*y != *x && !edges.iter().any(|e| e.0 == *y))
                    })
                    .collect();
                assert_eq!(g.ends[x], ends, "{edges:?} from {x}");
                assert_eq!(
                    g.reach_cycle.contains(x),
                    r.iter().any(|y| cyclic.contains(y)),
                    "{edges:?}"
                );
            }
        }
    }
}
