//! Dependencies between plugins (plugin specification 1.0.0, the v3 part,
//! "Dependencies"): which plugins may run, and in which order the update
//! builds them. A pure function of what the settings and the manifests said.
//!
//! A plugin names another in its manifest's `needs` with the skills it wants
//! from it, and its grant line holds `needs:<name>`. The rules:
//!
//! - The named plugin is in TEHAS_PLUGINS and ready, or the dependant is
//!   refused with "needs `<name>`, which is <state>".
//! - One level: a plugin that is needed needs nothing, or its dependants are
//!   refused.
//! - A cycle of needs refuses every plugin in it, naming the cycle.
//! - Each skill named is one the dependency grants to a plugin's chat.
//! - Dependencies are built first; otherwise the order of TEHAS_PLUGINS holds.

use std::collections::BTreeSet;

/// What a plugin needs from another: its name and the skills it wants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    pub name: String,
    pub skills: Vec<String>,
}

/// A plugin's state before its needs are weighed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Its pin, manifest, grants and build passed.
    Ready,
    /// Refused for a reason of its own.
    Refused,
    /// In TEHAS_PLUGINS but not set up yet.
    Off,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Refused => "refused",
            State::Off => "off",
        }
    }
}

/// One plugin as the resolution reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub state: State,
    /// The manifest's `needs`.
    pub needs: Vec<Need>,
    /// The names its grant line grants as `needs:<name>`.
    pub granted: Vec<String>,
    /// The skills its grant line grants to a plugin's chat.
    pub offers: Vec<String>,
}

/// Every plugin in build order, each with the reason its needs refuse it.
pub fn resolve(plugins: &[Plugin]) -> Vec<(String, Option<String>)> {
    let find = |name: &str| plugins.iter().find(|p| p.name == name);
    let in_cycle = cycles(plugins);
    let why = |p: &Plugin| -> Option<String> {
        if let Some(cycle) = in_cycle.iter().find(|c| c.contains(&p.name)) {
            return Some(format!("its needs form a cycle: {}", cycle.join(", ")));
        }
        for need in &p.needs {
            let n = &need.name;
            if !p.granted.contains(n) {
                return Some(format!("needs `{n}`, and needs:{n} is not granted"));
            }
            let Some(dep) = find(n) else {
                return Some(format!("needs `{n}`, which is not in TEHAS_PLUGINS"));
            };
            if dep.state != State::Ready {
                return Some(format!("needs `{n}`, which is {}", dep.state.word()));
            }
            if !dep.needs.is_empty() {
                return Some(format!(
                    "needs `{n}`, which needs other plugins: one level only"
                ));
            }
            if let Some(s) = need.skills.iter().find(|s| !dep.offers.contains(s)) {
                return Some(format!(
                    "needs the skill `{s}` of `{n}`, which it does not grant to a plugin's chat"
                ));
            }
        }
        None
    };
    // Dependencies first, each once, then the rest; both in settings order.
    let needed: BTreeSet<&str> = plugins
        .iter()
        .flat_map(|p| p.needs.iter().map(|n| n.name.as_str()))
        .collect();
    let first = plugins.iter().filter(|p| needed.contains(p.name.as_str()));
    let rest = plugins.iter().filter(|p| !needed.contains(p.name.as_str()));
    first
        .chain(rest)
        .map(|p| (p.name.clone(), why(p)))
        .collect()
}

/// The cycles of needs, each as the names in it, in the order first met.
fn cycles(plugins: &[Plugin]) -> Vec<Vec<String>> {
    let mut found: Vec<Vec<String>> = Vec::new();
    for start in plugins {
        let mut path = vec![start.name.clone()];
        walk(plugins, &mut path, &mut found);
    }
    found
}

fn walk(plugins: &[Plugin], path: &mut Vec<String>, found: &mut Vec<Vec<String>>) {
    let Some(here) = plugins.iter().find(|p| Some(&p.name) == path.last()) else {
        return;
    };
    for need in &here.needs {
        if let Some(at) = path.iter().position(|n| *n == need.name) {
            let cycle: Vec<String> = path[at..].to_vec();
            let seen = found
                .iter()
                .any(|c| c.len() == cycle.len() && cycle.iter().all(|n| c.contains(n)));
            if !seen {
                found.push(cycle);
            }
        } else if path.len() < plugins.len() {
            path.push(need.name.clone());
            walk(plugins, path, found);
            path.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, needs: &[(&str, &[&str])], offers: &[&str]) -> Plugin {
        Plugin {
            name: name.into(),
            state: State::Ready,
            needs: needs
                .iter()
                .map(|(n, s)| Need {
                    name: (*n).into(),
                    skills: s.iter().map(|x| (*x).into()).collect(),
                })
                .collect(),
            granted: needs.iter().map(|(n, _)| (*n).into()).collect(),
            offers: offers.iter().map(|x| (*x).into()).collect(),
        }
    }

    fn refused(r: &[(String, Option<String>)], name: &str) -> String {
        r.iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, w)| w.clone())
            .unwrap_or_else(|| panic!("{name} is not refused: {r:?}"))
    }

    #[test]
    fn dependencies_build_first_and_the_rest_keep_their_order() {
        let r = resolve(&[
            p("floorplan", &[], &[]),
            p("pageticket", &[("specs", &["grill", "spec"])], &[]),
            p("specs", &[], &["grill", "spec", "brief"]),
        ]);
        assert_eq!(
            r,
            [
                ("specs".to_string(), None),
                ("floorplan".to_string(), None),
                ("pageticket".to_string(), None),
            ]
        );
    }

    #[test]
    fn a_need_is_granted_set_up_ready_and_one_level_deep() {
        let mut ungranted = p("pageticket", &[("specs", &[])], &[]);
        ungranted.granted.clear();
        let r = resolve(&[ungranted, p("specs", &[], &[])]);
        assert!(refused(&r, "pageticket").contains("needs:specs is not granted"));

        let r = resolve(&[p("pageticket", &[("specs", &[])], &[])]);
        assert!(refused(&r, "pageticket").contains("not in TEHAS_PLUGINS"));

        for (state, word) in [(State::Refused, "refused"), (State::Off, "off")] {
            let mut dep = p("specs", &[], &[]);
            dep.state = state;
            let r = resolve(&[p("pageticket", &[("specs", &[])], &[]), dep]);
            assert_eq!(
                refused(&r, "pageticket"),
                format!("needs `specs`, which is {word}")
            );
        }

        let r = resolve(&[
            p("a", &[("b", &[])], &[]),
            p("b", &[("c", &[])], &[]),
            p("c", &[], &[]),
        ]);
        assert!(refused(&r, "a").contains("one level only"));
        assert_eq!(r.iter().find(|(n, _)| n == "b").unwrap().1, None);
    }

    #[test]
    fn a_skill_named_is_one_the_dependency_offers() {
        let r = resolve(&[
            p("pageticket", &[("specs", &["grill", "slice"])], &[]),
            p("specs", &[], &["grill"]),
        ]);
        assert!(refused(&r, "pageticket").contains("the skill `slice` of `specs`"));
    }

    #[test]
    fn a_cycle_refuses_every_plugin_in_it_and_no_other() {
        let r = resolve(&[
            p("a", &[("b", &[])], &[]),
            p("b", &[("a", &[])], &[]),
            p("c", &[("c", &[])], &[]),
            p("d", &[], &[]),
        ]);
        assert_eq!(refused(&r, "a"), "its needs form a cycle: a, b");
        assert_eq!(refused(&r, "b"), "its needs form a cycle: a, b");
        assert_eq!(refused(&r, "c"), "its needs form a cycle: c");
        assert_eq!(r.iter().find(|(n, _)| n == "d").unwrap().1, None);
        assert_eq!(r.len(), 4);
    }
}
