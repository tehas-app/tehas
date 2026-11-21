//! The words of `tehas plugin review` that are rules: what each ask lets a
//! plugin do (docs/specs/2026-11-21-plugins-security.md, "Capabilities"),
//! what changed between two manifests, whether the model's endpoint is on
//! this host, and the grants line to write.

use super::{ask, grants_setting, Cap, Grants};
use serde_json::Value;

/// One row of the capability table: the capability as a grant writes it,
/// and what it lets the plugin do, as a person weighs it.
pub struct Row {
    pub word: &'static str,
    pub does: &'static str,
}

/// The one capability table (plugin specification 1.0.0, the v3 part, "One
/// release, one manifest"). `tehas plugin review` prints from it, and the
/// manual's page `host/docs/plugins.md` holds it as [`markdown`] writes it.
/// A new capability is a new row here.
pub const TABLE: [Row; 10] = [
    Row { word: "view", does: "its /out is served behind the sign-in, and a panel in the dock: everyone the sign-in lets in sees it" },
    Row { word: "view:<id>", does: "one of its views, the part of its /out at that view's path, is served behind the sign-in and opens from the plugins menu: everyone the sign-in lets in sees it" },
    Row { word: "product", does: "reads all of the product's code, read only, in every run" },
    Row { word: "model:<tokens>", does: "chat completions through a socket, up to the tokens of the grant a run" },
    Row { word: "model:images", does: "images in its model requests, each carried in the request and never fetched: what it sends of the product's pages goes to the model" },
    Row { word: "action:<id>", does: "a button in the view: a signed-in person asks for a refresh" },
    Row { word: "hook:after:work", does: "a run on each pull request a cycle leaves; its summary is on the pull request for every reader of the repository" },
    Row { word: "skill:<id>", does: "its text from the pinned commit goes into the chats' instructions" },
    Row { word: "needs:<name>", does: "runs only while the named plugin is ready, and its chat gets the skills that plugin grants to a plugin's chat; it grants the other plugin nothing" },
    Row { word: "tool:<id>", does: "the chats it names may call it: each call is a run with the caller's arguments and the plugin's /state read only, and its answer goes to the model" },
];

/// The row of a capability.
pub fn row(cap: &Cap) -> &'static Row {
    let at = match cap {
        Cap::View => 0,
        Cap::ViewOf(_) => 1,
        Cap::Product => 2,
        Cap::Model => 3,
        Cap::ModelImages => 4,
        Cap::Action(_) => 5,
        Cap::HookAfterWork => 6,
        Cap::Skill(_) => 7,
        Cap::Needs(_) => 8,
        Cap::Tool(_) => 9,
    };
    &TABLE[at]
}

/// What a capability lets the plugin do, as a person weighs it.
pub fn does(cap: &Cap) -> &'static str {
    row(cap).does
}

/// The table as the manual holds it.
pub fn markdown() -> String {
    let mut out = String::from("| Capability | What it lets the plugin do |\n| --- | --- |\n");
    for r in &TABLE {
        let mut does = r.does.to_string();
        if let Some(first) = does.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        out.push_str(&format!("| `{}` | {does} |\n", r.word));
    }
    out
}

/// The warning of a view that shows what came of the code.
pub const VIEW_AND_PRODUCT: &str = "view and product together: the view shows what the plugin made of the code, and a page can still navigate away with data in its address";

/// The warnings of a set of asks.
pub fn warnings(asks: &[String]) -> Vec<String> {
    let has = |w: &str| asks.iter().any(|a| a == w);
    let mut out = Vec::new();
    if has("view") && has("product") {
        out.push(VIEW_AND_PRODUCT.to_string());
    }
    out
}

/// The host of an http or https address, in lower case, without its port or
/// the brackets of an IPv6 address. None: no such address.
pub fn url_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Whether `host` is this host: a loopback name or address, or one of
/// `names`, the host's own.
pub fn is_here(host: &str, names: &[String]) -> bool {
    let host = host.trim_end_matches('.');
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host.starts_with("127.")
        || names.iter().any(|n| n.eq_ignore_ascii_case(host))
}

/// The grants line that grants every ask of a manifest at `commit`. A model
/// grant keeps the tokens of `current`; with none it is `model:<tokens>`, a
/// number the person chooses.
pub fn grants_line(name: &str, commit: &str, asks: &[String], current: Option<&Grants>) -> String {
    let mut words = vec![commit.to_string()];
    for a in asks {
        match ask(a) {
            Ok(Cap::Model) => match current.and_then(|g| g.tokens) {
                Some(n) => words.push(format!("model:{n}")),
                None => words.push("model:<tokens>".to_string()),
            },
            Ok(cap) => words.push(cap.ask()),
            Err(_) => {}
        }
    }
    format!("{}=\"{}\"", grants_setting(name), words.join(" "))
}

fn asks_of(m: &Value) -> Vec<String> {
    m.get("asks")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|w| w.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// What changed from the manifest `old` to `new`, both as JSON: the asks
/// added and dropped, then each other field that changed, by name.
pub fn changes(old: &Value, new: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let (was, now) = (asks_of(old), asks_of(new));
    for a in now.iter().filter(|a| !was.contains(a)) {
        out.push(format!("asks for {a}, which the pin does not"));
    }
    for a in was.iter().filter(|a| !now.contains(a)) {
        out.push(format!("no longer asks for {a}"));
    }
    let empty = serde_json::Map::new();
    let (o, n) = (
        old.as_object().unwrap_or(&empty),
        new.as_object().unwrap_or(&empty),
    );
    let mut keys: Vec<&String> = o.keys().chain(n.keys()).filter(|k| *k != "asks").collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        match (o.get(k), n.get(k)) {
            (Some(a), Some(b)) if a != b => out.push(format!("{k} changed")),
            (Some(_), None) => out.push(format!("{k} is gone")),
            (None, Some(_)) => out.push(format!("{k} is new")),
            _ => {}
        }
    }
    out
}

/// What changed in the base images from the pin's to these.
pub fn base_changes(old: &[String], new: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for b in new.iter().filter(|b| !old.contains(b)) {
        out.push(format!("a new base image: {b}"));
    }
    for b in old.iter().filter(|b| !new.contains(b)) {
        out.push(format!("no longer built on {b}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const C: &str = "0123456789abcdef0123456789abcdef01234567";

    fn words(w: &[&str]) -> Vec<String> {
        w.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_capability_has_one_row_of_the_table_and_every_row_one_capability() {
        let caps = [
            Cap::View,
            Cap::ViewOf("map".into()),
            Cap::Product,
            Cap::Model,
            Cap::ModelImages,
            Cap::Action("refresh".into()),
            Cap::HookAfterWork,
            Cap::Skill("grill".into()),
            Cap::Needs("specs".into()),
            Cap::Tool("find-doc".into()),
        ];
        let words: Vec<&str> = caps.iter().map(|c| row(c).word).collect();
        assert_eq!(words, TABLE.iter().map(|r| r.word).collect::<Vec<_>>());
        for cap in &caps {
            assert!(!does(cap).is_empty());
            // The row's word is the grant's, with a placeholder for its value.
            let front = row(cap).word.split('<').next().unwrap();
            assert!(
                cap.ask().starts_with(front.trim_end_matches(':')),
                "{}",
                cap.ask()
            );
        }
    }

    #[test]
    fn the_manual_holds_the_table_as_review_prints_it() {
        let page = include_str!("../../../../host/docs/plugins.md");
        assert!(
            page.contains(&markdown()),
            "host/docs/plugins.md: replace its capability table with\n{}",
            markdown()
        );
    }

    #[test]
    fn a_view_of_the_product_is_warned_of() {
        assert_eq!(warnings(&words(&["view", "product"])), [VIEW_AND_PRODUCT]);
        assert!(warnings(&words(&["view", "model"])).is_empty());
        assert!(warnings(&words(&["product"])).is_empty());
    }

    #[test]
    fn the_host_of_an_address_is_read_without_its_port_or_user() {
        let h = |u: &str| url_host(u);
        assert_eq!(h("http://127.0.0.1:8080/v1"), Some("127.0.0.1".into()));
        assert_eq!(h("https://LLM.example/v1"), Some("llm.example".into()));
        assert_eq!(h("http://[::1]:11434/v1"), Some("::1".into()));
        assert_eq!(h("http://u:p@gpu-box:80"), Some("gpu-box".into()));
        assert_eq!(h("ftp://x"), None);
        assert_eq!(h("http:///v1"), None);
    }

    #[test]
    fn loopback_and_the_hosts_own_names_are_here() {
        let names = words(&["tehas-tehas"]);
        for here in [
            "localhost",
            "127.0.0.1",
            "127.1.2.3",
            "::1",
            "tehas-tehas",
            "TEHAS-TEHAS.",
        ] {
            assert!(is_here(here, &names), "{here}");
        }
        for away in ["llm.example", "203.0.113.5", "128.0.0.1", "tehas"] {
            assert!(!is_here(away, &names), "{away}");
        }
    }

    #[test]
    fn the_grants_line_grants_each_ask_and_keeps_the_tokens_given() {
        let asks = words(&["view", "product", "model", "action:refresh"]);
        assert_eq!(
            grants_line("floorplan", C, &asks, None),
            format!(
                "TEHAS_PLUGIN_GRANTS_FLOORPLAN=\"{C} view product model:<tokens> action:refresh\""
            )
        );
        let g = super::super::grants(&format!("{} model:2000000", "f".repeat(40))).unwrap();
        assert_eq!(
            grants_line("floorplan", C, &words(&["model"]), Some(&g)),
            format!("TEHAS_PLUGIN_GRANTS_FLOORPLAN=\"{C} model:2000000\"")
        );
    }

    #[test]
    fn a_change_of_the_manifest_names_the_asks_and_the_fields() {
        let old =
            json!({"plugin": 1, "label": "Floorplan", "asks": ["view", "product"], "timeout": 60});
        let new = json!({"plugin": 1, "label": "Map", "asks": ["product", "model"], "hooks": []});
        assert_eq!(
            changes(&old, &new),
            [
                "asks for model, which the pin does not",
                "no longer asks for view",
                "hooks is new",
                "label changed",
                "timeout is gone",
            ]
        );
        assert!(changes(&old, &old).is_empty());
        assert_eq!(
            base_changes(&words(&["a@sha256:1"]), &words(&["b@sha256:2"])),
            [
                "a new base image: b@sha256:2",
                "no longer built on a@sha256:1"
            ]
        );
    }
}
