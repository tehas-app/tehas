//! Masks tokens, keys, passwords, the program's own secret settings, the
//! provider keys the host's models.json holds as literals, and private
//! network addresses.
//!
//! An agent can print its environment and the console file is served by the
//! dashboard, which may be on the internet. The mask is a net for accidents,
//! not a wall: an agent that means to can print a secret in a form no pattern
//! knows.

use regex::{Captures, Regex};
use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::{LazyLock, OnceLock};

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a pattern of the mask is not a regular expression")
}

// A token is a token whatever comes before it: a URL-encoded one follows a
// letter or a digit. A classic token has no underscore, which keeps the
// pattern off words of snake case.
static GITHUB_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(gh[pousr]_)[A-Za-z0-9]{16,}|(github_pat_)[A-Za-z0-9_]{16,}"));
static URL_PASSWORD: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)([a-z][a-z0-9+.-]*://[^:/\s@]*:)[^@\s/]+@"));
static SECRET_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"(?-u:\b)([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|PASSWD|KEY|CREDENTIAL)[A-Z0-9_]*=)\S+")
});
// The shape model providers give their keys: sk-, then a long run.
static PROVIDER_KEY: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)(sk-)[A-Za-z0-9][A-Za-z0-9_-]{19,}"));
// A name that ends in one of these words, in any case, then `=` or `:`, as in
// a shell, a YAML file or JSON (`"apiKey": "..."`). The value ends where a
// quote, a separator or white space does, so the rest of the line is kept.
static KEYED_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r#"(?i)((?:api[_-]?key|access[_-]?key|token|secret(?:[_-]?key)?|password|passwd)\\?["']?[ \t]*[:=][ \t]*\\?["']?)[^\s"'\\,;&}]+"#,
    )
});
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)(bearer[ \t]+)[A-Za-z0-9._~+/-]{16,}=*"));
// The private IPv4 ranges, the tailnet range and link-local, and the IPv6
// unique local range. The groups at either end take what would make the
// address part of something longer, see private_address.
static PRIVATE_IPV4: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"([0-9.])?(?:10\.[0-9]{1,3}|192\.168|172\.(?:1[6-9]|2[0-9]|3[01])|169\.254|100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}([0-9])?",
    )
});
static PRIVATE_IPV6: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)([0-9a-f:])?f[cd][0-9a-f]{2}(?::[0-9a-f]{0,4}){2,}"));
static TAILNET_NAME: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?-u:\b)[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net(?-u:\b)"));

/// A private network address: an agent can read the container's hosts file,
/// and a connection error names the address it failed to reach. An address
/// counts only when it does not continue a longer number, so a version such
/// as 1.10.2.3 is left alone: a match that took a neighbour into one of its
/// groups is written back as it was.
fn private_address(found: &Captures) -> String {
    if found.iter().skip(1).any(|group| group.is_some()) {
        found[0].to_string()
    } else {
        "[private address]".to_string()
    }
}

/// The format characters of Unicode (category Cf): invisible, and inside a
/// token they would hide it from the patterns.
pub fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

pub fn mask(s: &str) -> String {
    let s: String = s.chars().filter(|c| !is_format(*c)).collect();
    let s = match ENV_SECRETS.as_ref() {
        Some(values) => values.replace_all(&s, "***").into_owned(),
        None => s,
    };
    let s = GITHUB_TOKEN.replace_all(&s, "${1}${2}***");
    let s = URL_PASSWORD.replace_all(&s, "${1}***@");
    let s = SECRET_ASSIGNMENT.replace_all(&s, "${1}***");
    let s = KEYED_SECRET.replace_all(&s, "${1}***");
    let s = BEARER.replace_all(&s, "${1}***");
    let s = PROVIDER_KEY.replace_all(&s, "${1}***");
    let s = PRIVATE_IPV4.replace_all(&s, private_address);
    let s = PRIVATE_IPV6.replace_all(&s, private_address);
    let s = TAILNET_NAME.replace_all(&s, "[private name]");
    hide(PRIVATE.get().and_then(Option::as_ref), &s)
}

// The literal values the mask hides: the values this program was started with
// whose names say they are secret, and the provider keys the host's models.json
// holds as literals. Each is hidden as literal text wherever it appears,
// whatever surrounds it, so a value printed without its name is hidden too. A
// literal key is in no setting, so it is read from the file, once for the run.
static ENV_SECRETS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    secret_values(std::env::vars(), model_key_values(&read_models_file()))
});

// lib/models.sh names the file, and the dispatcher's reader of it builds the
// same path.
fn models_file_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".pi/agent/models.json")
}

// Once per run, with the mask: a missing or unreadable file holds nothing, and
// that is no error.
fn read_models_file() -> String {
    std::fs::read_to_string(models_file_path()).unwrap_or_default()
}

/// The apiKey and backupApiKey written as literals in the models.json text:
/// a value that starts with `$` names a variable, whose value the environment
/// already gives. Text is the argument, so the rule is tested without a file.
fn model_key_values(text: &str) -> Vec<String> {
    let Ok(file) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(providers) = file.get("providers").and_then(|p| p.as_object()) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for provider in providers.values() {
        for name in ["apiKey", "backupApiKey"] {
            if let Some(literal) = provider
                .get(name)
                .and_then(|v| v.as_str())
                .filter(|v| !v.starts_with('$'))
                .map(str::trim)
                .filter(|v| v.chars().count() >= SHORTEST_SECRET)
                // A value of another provider that is the same key is kept once.
                .filter(|literal| !values.iter().any(|kept| kept == literal))
            {
                values.push(literal.to_string());
            }
        }
    }
    values
}

// A shorter value would mask ordinary words.
const SHORTEST_SECRET: usize = 8;

// The words an environment setting's name must hold for its value to count as
// a secret. models.json gives the names of its keys itself, so the rule is
// only the environment's.
fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "KEY", "CREDENTIAL"]
        .iter()
        .any(|w| name.contains(w))
}

/// One pattern of the secret values: those of the environment whose names say
/// they are secret, and the literals of models.json, which name themselves.
fn secret_values(
    vars: impl Iterator<Item = (String, String)>,
    models: Vec<String>,
) -> Option<Regex> {
    let mut values: Vec<String> = vars
        .filter(|(name, _)| secret_name(name))
        .map(|(_, value)| value.trim().to_string())
        .filter(|v| v.chars().count() >= SHORTEST_SECRET)
        .collect();
    values.extend(models);
    if values.is_empty() {
        return None;
    }
    values.sort_by_key(|v| Reverse(v.len()));
    values.dedup();
    let alternation: Vec<String> = values.iter().map(|v| regex::escape(v)).collect();
    Regex::new(&alternation.join("|")).ok()
}

// Names the host knows and an agent can print: the host aliases, the model
// host, the bot's email. tehas work gives them with --private.
static PRIVATE: OnceLock<Option<Regex>> = OnceLock::new();

// A shorter name would mask ordinary words.
const SHORTEST_PRIVATE: usize = 4;

/// One pattern of the names, matched as text in any case, the longest first
/// so that a name is not masked in part. None when no name is kept.
fn private_names(names: &[String]) -> Result<Option<Regex>, String> {
    let own = "[private address] [private name] ***";
    let mut kept: Vec<&str> = names
        .iter()
        .map(|n| n.trim())
        .filter(|n| n.chars().count() >= SHORTEST_PRIVATE)
        // A name in the mask's own words would mask what the mask wrote.
        .filter(|n| !own.to_lowercase().contains(&n.to_lowercase()))
        .collect();
    if kept.is_empty() {
        return Ok(None);
    }
    kept.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let alternation: Vec<String> = kept.iter().map(|n| regex::escape(n)).collect();
    Regex::new(&format!("(?i){}", alternation.join("|")))
        .map(Some)
        .map_err(|e| format!("console: --private: {e}"))
}

/// Sets the names for the rest of the run. Once: the first call wins.
pub fn set_private(names: &[String]) -> Result<(), String> {
    let _ = PRIVATE.set(private_names(names)?);
    Ok(())
}

fn hide(names: Option<&Regex>, s: &str) -> String {
    match names {
        Some(names) => names.replace_all(s, "[private name]").into_owned(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{mask, model_key_values, secret_values};
    use std::path::{Path, PathBuf};

    // Built here, not written out: scripts/check-generic.sh reads this file too.
    fn ip(parts: [u8; 4]) -> String {
        parts.map(|p| p.to_string()).join(".")
    }

    #[test]
    fn masks_a_fine_grained_github_token() {
        let out = mask("token is github_pat_ABC123DEF456GHI789JOK012 and done");
        assert_eq!(out, "token is github_pat_*** and done");
    }

    #[test]
    fn masks_classic_github_tokens() {
        for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
            assert_eq!(
                mask(&format!("x={prefix}ABCDEFGHIJKLMNOPQRSTUVWXYZ")),
                format!("x={prefix}***")
            );
        }
    }

    #[test]
    fn does_not_mask_a_short_or_incomplete_github_token_prefix() {
        // Fewer than 16 secret characters: the prefix is not a real token.
        assert_eq!(mask("github_pat_short123"), "github_pat_short123");
    }

    #[test]
    fn masks_the_password_in_a_postgres_url() {
        assert_eq!(
            mask("DB=postgres://tehas:supersecretvalue@db:5432/prod"),
            "DB=postgres://tehas:***@db:5432/prod"
        );
        assert_eq!(
            mask("DB=postgresql://user:hunter2@host:5432/db"),
            "DB=postgresql://user:***@host:5432/db"
        );
    }

    #[test]
    fn masks_key_value_secrets_for_token_like_names() {
        assert_eq!(mask("FOO_TOKEN=abcdef123456"), "FOO_TOKEN=***");
        assert_eq!(
            mask("DATABASE_PASSWORD=hunter2 x"),
            "DATABASE_PASSWORD=*** x"
        );
        assert_eq!(mask("GH_TOKEN=ghp_whatever"), "GH_TOKEN=***");
        assert_eq!(mask("CF_API_KEY=s3cr3t"), "CF_API_KEY=***");
    }

    #[test]
    fn a_secret_ends_at_white_space_of_any_kind() {
        assert_eq!(mask("A_TOKEN=abc\tnext"), "A_TOKEN=***\tnext");
        assert_eq!(mask("A_TOKEN=abc\u{a0}next"), "A_TOKEN=***\u{a0}next");
        assert_eq!(mask("A_SECRET=päivää and"), "A_SECRET=*** and");
    }

    #[test]
    fn masks_a_token_glued_to_a_letter_or_a_digit() {
        let token = format!("ghp_{}", "A".repeat(36));
        assert_eq!(
            mask(&format!("GITHUB_TOKEN%3D{token}&x=1")),
            "GITHUB_TOKEN%3Dghp_***&x=1"
        );
        assert_eq!(mask(&format!("x9{token}")), "x9ghp_***");
    }

    #[test]
    fn a_format_character_does_not_hide_a_token() {
        let token = format!("ghp_{}\u{200b}{}", "A".repeat(10), "B".repeat(26));
        assert_eq!(mask(&format!("is {token} ok")), "is ghp_*** ok");
        assert_eq!(mask("gh\u{2060}p_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "ghp_***");
        assert_eq!(mask("a\u{feff}b"), "ab");
    }

    #[test]
    fn masks_a_key_in_json() {
        assert_eq!(
            mask(r#"{"apiKey": "sk-abc123", "baseUrl": "http://models/v1"}"#),
            r#"{"apiKey": "***", "baseUrl": "http://models/v1"}"#
        );
        assert_eq!(mask(r#"{"api_key":"sk-abc"}"#), r#"{"api_key":"***"}"#);
    }

    #[test]
    fn masks_a_bearer_token() {
        assert_eq!(
            mask("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.e30.abc next"),
            "Authorization: Bearer *** next"
        );
        assert_eq!(
            mask("authorization: bearer abcdefghijklmnopqrstuv=="),
            "authorization: bearer ***"
        );
    }

    #[test]
    fn masks_the_password_in_any_url() {
        assert_eq!(mask("mysql://u:p4ss@db:3306/x"), "mysql://u:***@db:3306/x");
        assert_eq!(mask("redis://:p4ss@cache:6379"), "redis://:***@cache:6379");
        assert_eq!(
            mask("https://bot:hunter2@git.example/r.git"),
            "https://bot:***@git.example/r.git"
        );
    }

    #[test]
    fn masks_a_secret_named_in_lower_case() {
        assert_eq!(mask("token=abc123 rest"), "token=*** rest");
        assert_eq!(mask("password: hunter2"), "password: ***");
        assert_eq!(mask("--client-secret=xyz"), "--client-secret=***");
        assert_eq!(mask("?access_token=abc&page=2"), "?access_token=***&page=2");
    }

    #[test]
    fn masks_any_upper_case_name_with_key_in_it() {
        assert_eq!(
            mask("TEHAS_MODEL_KEY_MAIN=abcdef0123456789 x"),
            "TEHAS_MODEL_KEY_MAIN=*** x"
        );
        assert_eq!(mask("SIGNING_KEY=s3cr3t"), "SIGNING_KEY=***");
        assert_eq!(mask("A_CREDENTIAL=xyz"), "A_CREDENTIAL=***");
    }

    #[test]
    fn masks_a_provider_shaped_key() {
        let key = format!("sk-{}", "a1B2".repeat(8));
        assert_eq!(mask(&format!("key is {key} ok")), "key is sk-*** ok");
        assert_eq!(mask(&format!("sk-ant-{}", "x".repeat(30))), "sk-***");
        assert_eq!(mask("a task-oriented run"), "a task-oriented run");
        assert_eq!(mask("sk-short"), "sk-short");
    }

    #[test]
    fn hides_secret_values_of_the_environment_as_text() {
        let vars = vec![
            (
                "TEHAS_MODEL_KEY_MAIN".to_string(),
                "Zq9xLm2Pw8Rt".to_string(),
            ),
            ("SOME_TOKEN".to_string(), "short".to_string()),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ];
        let values = secret_values(vars.into_iter(), Vec::new()).expect("one value is kept");
        let out = values.replace_all("got Zq9xLm2Pw8Rt back, PATH=/usr/bin:/bin", "***");
        assert_eq!(out, "got *** back, PATH=/usr/bin:/bin");
        assert!(values.find("short").is_none());
        assert!(secret_values(std::iter::empty(), Vec::new()).is_none());
    }

    // models.json of the host that runs this test would join the mask, so a
    // rule of it is tested on the text of a file and not on mask() itself,
    // whose pattern a process builds once and cannot build twice. The reading
    // of the host's file is the two steps below, tested as one would set HOME
    // and the file of a host.
    fn models_with(providers: &str) -> String {
        format!("{{\"providers\": {providers}}}")
    }

    // The keys this mask hides, built here and not written out: a literal of
    // one would name the very words it must leave alone.
    fn key(n: usize) -> String {
        "wY3tKp9mRx7Lq2Zb8Nd4Fc5Vg1"
            .chars()
            .cycle()
            .take(n)
            .collect()
    }

    // The reading of the host's file, as a host has it: HOME, the file under
    // it, and the text the mask is given. The pattern itself is a LazyLock,
    // so a test cannot point HOME at two hosts in one process.
    fn models_file_of(home: &Path) -> String {
        std::fs::read_to_string(home.join(".pi/agent/models.json")).unwrap_or_default()
    }

    fn home_of(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tehas-core-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".pi/agent")).unwrap();
        d
    }

    #[test]
    fn a_literal_key_of_the_host_file_is_a_value_the_mask_hides() {
        let home = home_of("host");
        let (main, backup, kept) = (key(13), key(14), key(15));
        std::fs::write(
            home.join(".pi/agent/models.json"),
            models_with(&format!(
                "{{\"main\": {{\"apiKey\": \"{main}\", \"backupApiKey\": \"{backup}\"}}, \"fleet\": {{\"apiKey\": \"{kept}\"}}}}"
            )),
        )
        .unwrap();
        let values = model_key_values(&models_file_of(&home));
        let pattern = secret_values(std::iter::empty(), values).expect("the keys are kept");
        assert_eq!(
            pattern.replace_all(&format!("printed: {main} {backup} {kept}"), "***"),
            "printed: *** *** ***"
        );
        // No file at all is no error, and hides nothing beyond the environment.
        let _ = std::fs::remove_dir_all(&home);
        assert!(model_key_values(&models_file_of(&home)).is_empty());
    }

    #[test]
    fn text_that_is_no_file_adds_nothing() {
        // The reading of the host's file takes what it holds and nothing else:
        // no file at all is empty text, and that is no error.
        assert!(model_key_values("").is_empty());
        assert!(model_key_values("   ").is_empty());
    }

    #[test]
    fn a_literal_key_of_models_json_is_kept() {
        // A reference and a short value join nothing, whatever provider they
        // are in; the other cases here cover them.
        let (main, backup, spare) = (key(12), key(13), key(14));
        let text = models_with(&format!(
            r#"{{"main": {{"apiKey": "{main}", "backupApiKey": "{backup}"}},
                 "other": {{"apiKey": "{spare}", "backupApiKey": "$TEHAS_MODEL_KEY_FLEET"}},
                 "spare": {{"apiKey": "unused"}}}}"#
        ));
        let values = model_key_values(&text);
        assert_eq!(values, [main.clone(), backup.clone(), spare.clone()]);
        let values = secret_values(std::iter::empty(), values).expect("a value is kept");
        assert_eq!(
            values.replace_all(&format!("got {main} and {backup} and {spare} back"), "***"),
            "got *** and *** and *** back"
        );
    }

    #[test]
    fn a_reference_a_short_value_and_a_file_that_is_not_json_add_nothing() {
        // A value that starts with $ names a variable: its value is in the
        // environment, which the mask already reads.
        let text = models_with(r#"{"main": {"apiKey": "$TEHAS_MODEL_KEY_MAIN"}}"#);
        assert!(model_key_values(&text).is_empty());
        // A shorter value would mask ordinary words: 7 characters are kept
        // out, as for the environment.
        let text = models_with(&format!(
            "{{\"main\": {{\"apiKey\": \"{}\", \"backupApiKey\": \"{}\"}}}}",
            key(7),
            key(8)
        ));
        assert_eq!(model_key_values(&text), [key(8)]);
        // The same key written for two providers is kept once, and one that
        // only differs in white space counts as the same value.
        let text = models_with(&format!(
            "{{\"a\": {{\"apiKey\": \"{same}\"}}, \"b\": {{\"backupApiKey\": \" {same} \"}}}}",
            same = key(9)
        ));
        assert_eq!(model_key_values(&text), [key(9)]);
        assert!(model_key_values("not json at all").is_empty());
        // Providers are not an object, and there is no providers at all.
        assert!(model_key_values(r#"{"providers": []}"#).is_empty());
        assert!(model_key_values(r#"{"models": []}"#).is_empty());
        // A key that is not a string, and a provider that has no keys.
        let text = models_with(r#"{"main": {"apiKey": 7}, "other": {}}"#);
        assert!(model_key_values(&text).is_empty());
    }

    #[test]
    fn the_longer_of_two_keys_of_models_json_is_masked_whole() {
        let short = key(8);
        let long = key(10);
        let text = models_with(&format!(
            r#"{{"a": {{"apiKey": "{short}"}}, "b": {{"backupApiKey": "{long}"}}}}"#
        ));
        let values = secret_values(std::iter::empty(), model_key_values(&text))
            .expect("values are kept");
        assert_eq!(values.replace_all(&long, "***"), "***");
    }

    #[test]
    fn leaves_prose_and_counts_alone() {
        for s in [
            "max_tokens: 4096",
            r#"{"totalTokens": 120, "input_tokens": 7}"#,
            "the bearer of bad news",
            "laughs_at_everything_and_nothing",
            "a token is a word, a secret is kept",
            "the password field is empty",
            "http://host:8080/path@v1",
        ] {
            assert_eq!(mask(s), s);
        }
    }

    #[test]
    fn leaves_ordinary_text_untouched() {
        let plain = "no secrets here, just a postgres://user@host:5432/db url";
        assert_eq!(mask(plain), plain);
    }

    #[test]
    fn masks_private_network_addresses() {
        assert_eq!(
            mask(&format!("models {} ok", ip([10, 1, 2, 3]))),
            "models [private address] ok"
        );
        assert_eq!(
            mask(&format!("{}:8086", ip([192, 168, 1, 10]))),
            "[private address]:8086"
        );
        assert_eq!(
            mask(&format!("at {}.", ip([172, 16, 0, 5]))),
            "at [private address]."
        );
        assert_eq!(mask(&ip([172, 31, 255, 1])), "[private address]");
        assert_eq!(
            mask(&format!("host_{}", ip([100, 100, 2, 3]))),
            "host_[private address]"
        );
        assert_eq!(mask(&ip([169, 254, 1, 1])), "[private address]");
        assert_eq!(
            mask(&format!(
                "{}:8086",
                ["node", "tail1234", "ts", "net"].join(".")
            )),
            "[private name]:8086"
        );
    }

    #[test]
    fn masks_unique_local_ipv6_addresses() {
        let address = ["fd7a", "115c", "a1e0", "", "1"].join(":");
        assert_eq!(
            mask(&format!("at [{address}]:80")),
            "at [[private address]]:80"
        );
        assert_eq!(mask(&address.to_uppercase()), "[private address]");
        // Not the end of a longer run of hexadecimal, and not a time.
        for s in [
            format!("0{address}"),
            format!("a:{address}"),
            ["fd12", "30"].join(":"),
        ] {
            assert_eq!(mask(&s), s);
        }
    }

    #[test]
    fn an_address_beside_another_or_inside_a_number_is_judged_alone() {
        let (a, b) = (ip([10, 0, 0, 1]), ip([10, 0, 0, 2]));
        assert_eq!(
            mask(&format!("{a} {b}")),
            "[private address] [private address]"
        );
        assert_eq!(
            mask(&format!("{a}:{b}")),
            "[private address]:[private address]"
        );
        for s in [format!("1.{a}"), format!("5{a}"), format!(".{a}")] {
            assert_eq!(mask(&s), s);
        }
        assert_eq!(mask(&format!("{a}.{b}")), format!("[private address].{b}"));
        assert_eq!(mask(&format!("ä{a}ä")), "ä[private address]ä");
    }

    #[test]
    fn masks_every_address_on_a_line() {
        let line = format!(
            "{} then {},{}",
            ip([10, 0, 0, 1]),
            ip([10, 0, 0, 2]),
            ip([192, 168, 0, 3])
        );
        assert_eq!(
            mask(&line),
            "[private address] then [private address],[private address]"
        );
    }

    #[test]
    fn leaves_public_and_documentation_addresses_and_versions_alone() {
        for s in [
            "192.0.2.7",
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "version 1.10.2.3",
            "210.1.2.3",
            "127.0.0.1",
        ] {
            assert_eq!(mask(s), s);
        }
        // An address that runs on into more digits is a longer number.
        let long = format!("{}456", ip([10, 1, 2, 3]));
        assert_eq!(mask(&long), long);
    }
}

#[cfg(test)]
mod private_tests {
    use super::{hide, private_names};

    fn names(given: &[&str]) -> Vec<String> {
        given.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_name_the_host_knows_is_masked_wherever_it_is() {
        let known = private_names(&names(&["models.lan", "bot@example.com"])).unwrap();
        assert_eq!(
            hide(
                known.as_ref(),
                "curl http://models.lan:8086/v1 by bot@example.com."
            ),
            "curl http://[private name]:8086/v1 by [private name]."
        );
    }

    #[test]
    fn a_name_is_masked_in_any_case_and_as_text_not_a_pattern() {
        let known = private_names(&names(&["a.b-c"])).unwrap();
        assert_eq!(hide(known.as_ref(), "A.B-C axb-c"), "[private name] axb-c");
    }

    #[test]
    fn the_longer_of_two_names_is_masked_whole() {
        let known = private_names(&names(&["gpu1", "gpu1.lan"])).unwrap();
        assert_eq!(hide(known.as_ref(), "at gpu1.lan"), "at [private name]");
    }

    #[test]
    fn short_and_empty_names_are_left_out() {
        assert!(private_names(&names(&["", "abc", "  "])).unwrap().is_none());
        let known = private_names(&names(&["abc", "host1"])).unwrap();
        assert_eq!(hide(known.as_ref(), "abc host1"), "abc [private name]");
    }

    #[test]
    fn a_name_in_the_mask_s_own_words_is_left_out() {
        let known = private_names(&names(&["private", "name"])).unwrap();
        assert!(known.is_none());
    }

    #[test]
    fn without_names_nothing_changes() {
        assert_eq!(hide(None, "models.lan"), "models.lan");
    }
}
