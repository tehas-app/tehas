//! The one way into a command that runs under the instance's settings.
//!
//!   console run <libexec dir> <euid> <bash> tehas|tehasbot <command> [arguments]
//!
//! bin/tehas and bin/tehasbot call it, and the few paths a test or a shim still
//! names. `<euid>` and `<bash>` are the shell's, as for `console settings load`:
//! reading the uid here would need `unsafe`. Each run goes in this order:
//!
//!   1. `check` on the words: a bad argument is 64 and a help text 0 whatever the
//!      settings hold, and neither loads them. A command with no check passes.
//!   2. tehas_load on this process's environment, as a shell's load leaves it, so
//!      the verb and whatever it starts have the same names.
//!   3. what the command cannot run without, `commands::required`.
//!   4. the verb, in this process.
//!
//! So a bad setting is 78 before GitHub, Docker or git and before any write
//! (AGENTS.md, rule 2).
//!
//! What runs here hands no descriptor to a child, so nothing here needs a
//! close-on-exec cleared. libexec/index does, which is why it stays bash.
//!
//! Exit: the command's; 2 an argument that is not UTF-8; 64 usage; 78 the
//! settings are missing or malformed.

use crate::commands::{self, ANSWERED};
use crate::settings;

/// Steps 2 to 4. Err is the status of a refusal.
fn command(uid: &str, bash: &str, words: &[String]) -> Result<u8, u8> {
    let vars = settings::load_here(uid, bash)?;
    settings::require(commands::required(words), &vars)?;
    Ok(match words.first().map(String::as_str) {
        Some("tehas") => commands::tehas(&words[1..]),
        _ => commands::tehasbot(&words[1..]),
    })
}

/// `text` is whether every argument the program was given is UTF-8. `check` is
/// asked either way: it refuses such a word with the usage of its command.
pub fn main(given: &[String], text: bool) -> u8 {
    let [_libexec, uid, bash, words @ ..] = given else {
        eprintln!(
            "usage: console run <libexec dir> <euid> <bash> tehas|tehasbot <command> [arguments]"
        );
        return 64;
    };
    match commands::check(words) {
        0 => {}
        ANSWERED => return 0,
        rc => return rc,
    }
    if !text {
        eprintln!("console: an argument is not UTF-8");
        return 2;
    }
    command(uid, bash, words).unwrap_or_else(|rc| rc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_few_words_are_a_usage_error() {
        assert_eq!(main(&["/l".to_string()], true), 64);
    }
}
