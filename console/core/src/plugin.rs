//! Plugins, a feature still being built (docs/specs/2026-11-21-plugins.md):
//! programs from pinned git repositories. Here are the grammar of their
//! settings and the capabilities, which the settings' rules, the manifest's
//! reader and `tehas plugin` share. Nothing here reads a file or the network.

pub mod manifest;
#[cfg(test)]
mod schema;

/// The front of the grants settings, one per plugin:
/// `TEHAS_PLUGIN_GRANTS_<NAME>`, the name in upper case.
pub const GRANTS: &str = "TEHAS_PLUGIN_GRANTS_";
/// Plugins a factory may have.
pub const MAX: usize = 4;
/// The longest plugin name.
pub const NAME_MAX: usize = 12;
/// The longest action id.
pub const ACTION_MAX: usize = 16;

/// The words tehas-industry refuses as a part of a variable's name, since it
/// takes such a variable for a secret when it pushes an instance. A plugin's
/// name is one part of its grants setting's name.
pub const SECRET_WORDS: [&str; 12] = [
    "key",
    "keys",
    "token",
    "tokens",
    "secret",
    "secrets",
    "private",
    "privates",
    "password",
    "passwords",
    "passwd",
    "passwds",
];

/// Why a grant or an ask of a later tehas is refused.
const V2: &str = "is reserved for plugins v2";

/// One entry of TEHAS_PLUGINS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub name: String,
    pub url: String,
    pub commit: String,
}

/// A plugin's name: `a-z` and `0-9`, a letter first, 1 to 12 characters.
pub fn is_name(s: &str) -> bool {
    (1..=NAME_MAX).contains(&s.len())
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// An action's id: `a-z` and `-`, 1 to 16 characters.
pub fn is_action_id(s: &str) -> bool {
    (1..=ACTION_MAX).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

/// The grants setting of the plugin `name`.
pub fn grants_setting(name: &str) -> String {
    format!("{GRANTS}{}", name.to_ascii_uppercase())
}

/// An https address of a repository: a host, then a path of the ordinary
/// characters of a URL. No user, password, query or fragment.
fn is_url(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (name, port) = host.split_once(':').unwrap_or((host, ""));
    let host_ok = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && port.bytes().all(|b| b.is_ascii_digit())
        && (port.is_empty() == !host.contains(':'));
    host_ok
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._~/-".contains(&b))
}

/// TEHAS_PLUGINS: `<name>=<url>@<commit>` entries one space apart, at most
/// [`MAX`]. The empty value is no plugin. Err is why, as the load says it.
pub fn pins(value: &str) -> Result<Vec<Pin>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut pins: Vec<Pin> = Vec::new();
    for entry in value.split(' ') {
        let shape = entry
            .split_once('=')
            .and_then(|(name, rest)| rest.rsplit_once('@').map(|(u, c)| (name, u, c)));
        let Some((name, url, commit)) = shape else {
            return Err(
                "expected <name>=<url>@<commit> entries separated by single spaces".to_string(),
            );
        };
        if !is_name(name) {
            return Err(format!(
                "{name:?} is no plugin name: 1 to 12 characters of a-z and 0-9, a letter first"
            ));
        }
        if SECRET_WORDS.contains(&name) {
            return Err(format!(
                "{name} is a word tehas-industry takes for a secret in a setting's name, so it would refuse TEHAS_PLUGIN_GRANTS_{}: choose another name",
                name.to_ascii_uppercase()
            ));
        }
        if !is_url(url) {
            return Err(format!(
                "{name}: expected an https:// address with no user, password, query or fragment"
            ));
        }
        if !crate::grammar::commit_id(commit) {
            return Err(format!(
                "{name}: expected the whole commit, 40 lower-case hex digits"
            ));
        }
        if pins.iter().any(|p| p.name == name) {
            return Err(format!("{name} is named twice"));
        }
        pins.push(Pin {
            name: name.to_string(),
            url: url.to_string(),
            commit: commit.to_string(),
        });
    }
    if pins.len() > MAX {
        return Err(format!("at most {MAX} plugins"));
    }
    Ok(pins)
}

/// A capability of v1, as a grant or an ask names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cap {
    View,
    Product,
    Model,
    Action(String),
    HookAfterWork,
}

impl Cap {
    /// The word of an ask: `model` without its tokens.
    pub fn ask(&self) -> String {
        match self {
            Cap::View => "view".to_string(),
            Cap::Product => "product".to_string(),
            Cap::Model => "model".to_string(),
            Cap::Action(id) => format!("action:{id}"),
            Cap::HookAfterWork => "hook:after:work".to_string(),
        }
    }
}

/// Whether a word is a capability of a later tehas.
fn reserved(word: &str) -> bool {
    ["gate:", "skill:", "tool:", "net:", "hook:"]
        .iter()
        .any(|front| word.starts_with(front))
        || matches!(word, "factory-read" | "factory-sessions")
}

/// The capability of one word. `granted` is whether it is a grant, which
/// names the model's tokens, or an ask, which does not. Ok holds the tokens
/// of a model grant.
fn cap(word: &str, granted: bool) -> Result<(Cap, Option<u64>), String> {
    let plain = match word {
        "view" => Some(Cap::View),
        "product" => Some(Cap::Product),
        "hook:after:work" => Some(Cap::HookAfterWork),
        _ => None,
    };
    if let Some(cap) = plain {
        return Ok((cap, None));
    }
    if word == "model" {
        return if granted {
            Err("model is granted with its tokens a run, as model:<tokens>".to_string())
        } else {
            Ok((Cap::Model, None))
        };
    }
    if let Some(tokens) = word.strip_prefix("model:") {
        if !granted {
            return Err(format!(
                "{word}: a manifest asks for model; the tokens are the grant's"
            ));
        }
        let n = tokens
            .parse::<u64>()
            .ok()
            .filter(|_| crate::settings::rules::is_count(tokens) && tokens != "0");
        return match n {
            Some(n) => Ok((Cap::Model, Some(n))),
            None => Err(format!(
                "{word}: the tokens are a whole number from 1 to 999999999"
            )),
        };
    }
    if let Some(id) = word.strip_prefix("action:") {
        return if is_action_id(id) {
            Ok((Cap::Action(id.to_string()), None))
        } else {
            Err(format!(
                "{word}: an action's id is 1 to 16 characters of a-z and -"
            ))
        };
    }
    if reserved(word) {
        return Err(format!("{word} {V2}"));
    }
    Err(format!("{word} is no capability of plugins v1"))
}

/// The capability an ask names, or why the word is refused.
pub fn ask(word: &str) -> Result<Cap, String> {
    cap(word, false).map(|(cap, _)| cap)
}

/// A grants line: the commit it is for and what it grants there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grants {
    pub commit: String,
    pub caps: Vec<Cap>,
    /// The tokens a run of `model:<tokens>`.
    pub tokens: Option<u64>,
    /// The words as the line gave them.
    pub words: Vec<String>,
}

/// `TEHAS_PLUGIN_GRANTS_<NAME>`: the commit, then the grants, one space
/// apart. Err is why, as the load says it.
pub fn grants(value: &str) -> Result<Grants, String> {
    let mut words = value.split(' ');
    let commit = words.next().unwrap_or_default();
    if !crate::grammar::commit_id(commit) {
        return Err(
            "expected the plugin's whole commit, 40 lower-case hex digits, then its grants separated by single spaces"
                .to_string(),
        );
    }
    let mut out = Grants {
        commit: commit.to_string(),
        caps: Vec::new(),
        tokens: None,
        words: Vec::new(),
    };
    for word in words {
        if word.is_empty() {
            return Err("expected grants separated by single spaces".to_string());
        }
        let (cap, tokens) = cap(word, true)?;
        if out.caps.contains(&cap) {
            return Err(format!("{} is granted twice", cap.ask()));
        }
        out.caps.push(cap);
        out.tokens = out.tokens.or(tokens);
        out.words.push(word.to_string());
    }
    Ok(out)
}

/// What a manifest's asks come to against a plugin's grants and its pin.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Compared {
    /// The grants are for a commit other than the pin's, so they grant
    /// nothing.
    pub other_commit: bool,
    /// Asks of v1 that are not granted.
    pub missing: Vec<String>,
    /// Asks of a later tehas, each with why.
    pub reserved: Vec<String>,
    /// Asks that are no capability at all, each with why.
    pub unknown: Vec<String>,
    /// Grants the manifest does not ask for, which give nothing.
    pub unasked: Vec<String>,
}

impl Compared {
    /// Whether the plugin may run: every ask granted, nothing reserved.
    pub fn granted(&self) -> bool {
        !self.other_commit
            && self.missing.is_empty()
            && self.reserved.is_empty()
            && self.unknown.is_empty()
    }

    /// Each reason the plugin is refused, in the words `tehas plugin` says.
    pub fn refusals(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.other_commit {
            out.push("the grants are for another commit".to_string());
        }
        out.extend(self.reserved.iter().cloned());
        out.extend(self.unknown.iter().cloned());
        out.extend(
            self.missing
                .iter()
                .map(|a| format!("{a} is asked and not granted")),
        );
        out
    }
}

/// The asks of a manifest, as words, against a plugin's grants (None: no
/// grants line) at the commit `pin`.
pub fn compare(asks: &[String], grants: Option<&Grants>, pin: &str) -> Compared {
    let mut out = Compared::default();
    let given: &[Cap] = match grants {
        Some(g) if g.commit == pin => &g.caps,
        Some(_) => {
            out.other_commit = true;
            &[]
        }
        None => &[],
    };
    let mut asked = Vec::new();
    for word in asks {
        match ask(word) {
            Ok(cap) => {
                if !given.contains(&cap) {
                    out.missing.push(cap.ask());
                }
                asked.push(cap);
            }
            Err(why) if reserved(word) => out.reserved.push(why),
            Err(why) => out.unknown.push(why),
        }
    }
    if let Some(g) = grants.filter(|g| g.commit == pin) {
        for (cap, word) in g.caps.iter().zip(&g.words) {
            if !asked.contains(cap) {
                out.unasked.push(word.clone());
            }
        }
    }
    out
}

/// The names of the edge the plugins of `pins` take under `domain`: each
/// view's, then the sign-in's. `one_level` is TEHAS_EDGE_ONE_LEVEL.
pub fn edge_names(pins: &[Pin], domain: &str, one_level: bool) -> Vec<String> {
    let mut names: Vec<String> = pins
        .iter()
        .map(|p| crate::settings::edge_name(&format!("plugin-{}", p.name), domain, one_level))
        .collect();
    names.push(crate::settings::edge_name("plugins", domain, one_level));
    names
}

/// The first name of `names` that is there twice.
pub fn twice(names: &[String]) -> Option<&str> {
    names
        .iter()
        .enumerate()
        .find(|(i, n)| names[..*i].contains(n))
        .map(|(_, n)| n.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";
    const D: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn floorplan() -> String {
        format!("floorplan=https://github.com/tehas-app/tehas-plugin-floorplan@{C}")
    }

    #[test]
    fn a_name_is_a_letter_then_letters_and_digits_up_to_twelve() {
        for good in ["a", "floorplan", "a1", "abcdefghijkl"] {
            assert!(is_name(good), "{good}");
        }
        for bad in [
            "",
            "1a",
            "Floorplan",
            "floor-plan",
            "abcdefghijklm",
            "a_b",
            "ä",
        ] {
            assert!(!is_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_pins_are_name_url_and_whole_commit() {
        assert_eq!(pins(""), Ok(vec![]));
        let got = pins(&floorplan()).unwrap();
        assert_eq!(
            got,
            [Pin {
                name: "floorplan".into(),
                url: "https://github.com/tehas-app/tehas-plugin-floorplan".into(),
                commit: C.into(),
            }]
        );
        let two = format!("{} b=https://git.example:8443/x/y.git@{D}", floorplan());
        assert_eq!(pins(&two).unwrap().len(), 2);
    }

    #[test]
    fn a_pin_that_breaks_a_rule_is_refused_with_why() {
        let shape = "expected <name>=<url>@<commit> entries separated by single spaces";
        for bad in [
            "floorplan".to_string(),
            format!("a=https://h.example/x {C}"),
            format!("{}  b=https://h.example/x@{C}", floorplan()),
            format!("{} ", floorplan()),
        ] {
            assert!(pins(&bad).is_err(), "{bad}");
        }
        assert_eq!(pins("floorplan").unwrap_err(), shape);
        let name = "\"Floor\" is no plugin name: 1 to 12 characters of a-z and 0-9, a letter first";
        assert_eq!(
            pins(&format!("Floor=https://h.example/x@{C}")).unwrap_err(),
            name
        );
        let url = "a: expected an https:// address with no user, password, query or fragment";
        for u in [
            "http://h.example/x",
            "https://user@h.example/x",
            "https://user:pw@h.example/x",
            "https://h.example/x?ref=main",
            "https://h.example/x#top",
            "https:///x",
            "https://h.example:/x",
            "https://h.example/x\"",
            "ssh://h.example/x",
            "file:///x",
        ] {
            assert_eq!(pins(&format!("a={u}@{C}")).unwrap_err(), url, "{u}");
        }
        let commit = "a: expected the whole commit, 40 lower-case hex digits";
        for c in ["0123456", &C.to_uppercase(), &format!("{C}0"), "main"] {
            assert_eq!(
                pins(&format!("a=https://h.example/x@{c}")).unwrap_err(),
                commit
            );
        }
        assert_eq!(
            pins(&format!("{} {}", floorplan(), floorplan())).unwrap_err(),
            "floorplan is named twice"
        );
        for word in SECRET_WORDS {
            assert_eq!(
                pins(&format!("{word}=https://h.example/x@{C}")).unwrap_err(),
                format!("{word} is a word tehas-industry takes for a secret in a setting's name, so it would refuse TEHAS_PLUGIN_GRANTS_{}: choose another name", word.to_ascii_uppercase())
            );
        }
        assert!(pins(&format!("keyring=https://h.example/x@{C}")).is_ok());
        let five: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|n| format!("{n}=https://h.example/{n}@{C}"))
            .collect();
        assert_eq!(pins(&five[..4].join(" ")).unwrap().len(), 4);
        assert_eq!(pins(&five.join(" ")).unwrap_err(), "at most 4 plugins");
    }

    #[test]
    fn a_grants_line_is_a_commit_then_the_grants_of_v1() {
        let g = grants(&format!(
            "{C} view product model:2000000 action:refresh hook:after:work"
        ))
        .unwrap();
        assert_eq!(g.commit, C);
        assert_eq!(
            g.caps,
            [
                Cap::View,
                Cap::Product,
                Cap::Model,
                Cap::Action("refresh".into()),
                Cap::HookAfterWork
            ]
        );
        assert_eq!(g.tokens, Some(2_000_000));
        assert_eq!(grants(C).unwrap().caps, []);
    }

    #[test]
    fn a_grants_line_that_breaks_a_rule_is_refused_with_why() {
        let commit = "expected the plugin's whole commit, 40 lower-case hex digits, then its grants separated by single spaces";
        for bad in [
            "",
            "view",
            "0123456 view",
            &format!("{} view", C.to_uppercase()),
        ] {
            assert_eq!(grants(bad).unwrap_err(), commit, "{bad:?}");
        }
        let cases = [
            (
                "view  product",
                "expected grants separated by single spaces",
            ),
            ("view ", "expected grants separated by single spaces"),
            ("view view", "view is granted twice"),
            ("model:1 model:2", "model is granted twice"),
            (
                "model",
                "model is granted with its tokens a run, as model:<tokens>",
            ),
            (
                "model:0",
                "model:0: the tokens are a whole number from 1 to 999999999",
            ),
            (
                "model:01",
                "model:01: the tokens are a whole number from 1 to 999999999",
            ),
            (
                "model:1000000000",
                "model:1000000000: the tokens are a whole number from 1 to 999999999",
            ),
            (
                "action:Refresh",
                "action:Refresh: an action's id is 1 to 16 characters of a-z and -",
            ),
            (
                "action:",
                "action:: an action's id is 1 to 16 characters of a-z and -",
            ),
            ("secrets", "secrets is no capability of plugins v1"),
            ("View", "View is no capability of plugins v1"),
        ];
        for (line, why) in cases {
            assert_eq!(grants(&format!("{C} {line}")).unwrap_err(), why, "{line}");
        }
    }

    #[test]
    fn a_grant_of_a_later_tehas_is_refused_naming_the_word() {
        for word in [
            "gate:verify",
            "hook:before:work",
            "hook:after:merge",
            "skill:map",
            "tool:read",
            "net:api.example.org",
            "factory-read",
            "factory-sessions",
        ] {
            assert_eq!(
                grants(&format!("{C} view {word}")).unwrap_err(),
                format!("{word} is reserved for plugins v2")
            );
        }
    }

    fn words(asks: &[&str]) -> Vec<String> {
        asks.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn every_ask_granted_at_the_pin_runs() {
        let g = grants(&format!(
            "{C} view product model:2000000 action:refresh hook:after:work"
        ))
        .unwrap();
        let asks = words(&[
            "view",
            "product",
            "model",
            "action:refresh",
            "hook:after:work",
        ]);
        let got = compare(&asks, Some(&g), C);
        assert!(got.granted(), "{got:?}");
        assert_eq!(got, Compared::default());
    }

    #[test]
    fn an_ask_not_granted_is_missing_and_a_grant_not_asked_gives_nothing() {
        let g = grants(&format!("{C} view model:10")).unwrap();
        let got = compare(&words(&["view", "product", "action:refresh"]), Some(&g), C);
        assert_eq!(got.missing, ["product", "action:refresh"]);
        assert_eq!(got.unasked, ["model:10"]);
        assert!(!got.granted());
        assert_eq!(
            got.refusals(),
            [
                "product is asked and not granted",
                "action:refresh is asked and not granted"
            ]
        );
        let none = compare(&words(&["view"]), None, C);
        assert_eq!(none.missing, ["view"]);
        assert!(compare(&[], None, C).granted());
    }

    #[test]
    fn grants_for_another_commit_grant_nothing() {
        let g = grants(&format!("{D} view")).unwrap();
        let got = compare(&words(&["view"]), Some(&g), C);
        assert!(got.other_commit);
        assert_eq!(got.missing, ["view"]);
        assert!(got.unasked.is_empty());
        assert_eq!(got.refusals()[0], "the grants are for another commit");
        assert!(!compare(&[], Some(&g), C).granted());
    }

    #[test]
    fn an_ask_of_a_later_tehas_is_reserved_and_an_unknown_one_said() {
        let got = compare(&words(&["gate:verify", "factory-read", "nope"]), None, C);
        assert_eq!(
            got.reserved,
            [
                "gate:verify is reserved for plugins v2",
                "factory-read is reserved for plugins v2"
            ]
        );
        assert_eq!(got.unknown, ["nope is no capability of plugins v1"]);
        assert!(!got.granted());
    }

    #[test]
    fn an_ask_names_the_model_without_its_tokens() {
        assert_eq!(ask("model"), Ok(Cap::Model));
        assert_eq!(
            ask("model:10").unwrap_err(),
            "model:10: a manifest asks for model; the tokens are the grant's"
        );
    }

    #[test]
    fn the_edge_names_are_one_each_and_the_sign_ins() {
        let p = pins(&floorplan()).unwrap();
        assert_eq!(
            edge_names(&p, "example.org", false),
            ["plugin-floorplan.example.org", "plugins.example.org"]
        );
        assert_eq!(
            edge_names(&p, "demo.example.org", true),
            [
                "plugin-floorplan-demo.example.org",
                "plugins-demo.example.org"
            ]
        );
        assert_eq!(twice(&edge_names(&p, "demo.example.org", true)), None);
        let same = ["a".to_string(), "b".to_string(), "a".to_string()];
        assert_eq!(twice(&same), Some("a"));
    }

    #[test]
    fn the_grants_setting_names_the_plugin_in_upper_case() {
        assert_eq!(
            grants_setting("floorplan2"),
            "TEHAS_PLUGIN_GRANTS_FLOORPLAN2"
        );
    }
}
