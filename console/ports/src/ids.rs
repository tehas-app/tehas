//! What names a thing: keys, numbers, references, opaque tokens, `Rev`,
//! `Item`, `WebLink`, `GitRemote`, `At`.

use std::fmt;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

/// A text that a checked type refused when it was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotValid {
    /// The type that refused it.
    pub what: &'static str,
}

impl fmt::Display for NotValid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a valid {}", self.what)
    }
}

impl std::error::Error for NotValid {}

/// A text newtype that holds whatever it is given: stored and compared only.
macro_rules! opaque {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps the text as it is.
            pub fn new(text: impl Into<String>) -> Self {
                Self(text.into())
            }

            /// The text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
pub(crate) use opaque;

/// A text newtype checked when made, and again when read back.
macro_rules! checked {
    ($(#[$doc:meta])* $name:ident, $what:literal, $ok:path) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// The text, if it keeps the rule.
            pub fn new(text: impl Into<String>) -> Result<Self, $crate::NotValid> {
                let text = text.into();
                if $ok(&text) {
                    Ok(Self(text))
                } else {
                    Err($crate::NotValid { what: $what })
                }
            }

            /// The text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::NotValid;
            fn try_from(text: String) -> Result<Self, Self::Error> {
                Self::new(text)
            }
        }

        impl From<$name> for String {
            fn from(v: $name) -> String {
                v.0
            }
        }
    };
}

pub(crate) use checked;

/// A number newtype that is never zero.
macro_rules! number {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// The number, or None for 0.
            pub fn new(n: u64) -> Option<Self> {
                NonZeroU64::new(n).map(Self)
            }

            /// The number.
            pub fn get(self) -> u64 {
                self.0.get()
            }
        }

        impl From<NonZeroU64> for $name {
            fn from(n: NonZeroU64) -> Self {
                Self(n)
            }
        }
    };
}

/// A ticket on the tickets supplier. On a code host's own tracker its
/// number; on a separate tracker its key, kept whole and never split.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TicketKey {
    /// The ticket's number.
    Number(NonZeroU64),
    /// The ticket's key.
    Key(KeyText),
}

checked!(
    /// A ticket's key. Checked when made: 1 to 128 of ASCII letters, digits,
    /// `.`, `_`, `:`, `/` and `-`, the rule of `key(n)` in the vendored
    /// table. Never parsed.
    KeyText,
    "ticket key",
    key_text_ok
);

fn key_text_ok(s: &str) -> bool {
    (1..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-'))
}

number!(
    /// A blueprint's number on the code host. Never assumed equal to a
    /// ticket's.
    BlueprintNo
);

number!(
    /// A change's number on the code host. Never assumed in the same number
    /// space as a blueprint's.
    ChangeNo
);

/// What a number names where tickets, blueprints and changes share one
/// number space (a code host's own tracker). A caller asks; it never guesses.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Item {
    /// The number is a ticket's.
    Ticket(TicketKey),
    /// The number is a change's.
    Change(ChangeNo),
}

/// An item in another project or on another site: named, never followed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ref<T> {
    /// An item of this project.
    Here(T),
    /// An item of another project, by its names.
    Elsewhere {
        /// The other project.
        project: ProjectName,
        /// The item there.
        item: ItemName,
    },
}

opaque!(
    /// A project as its site names it, for display. A caller never parses it.
    ProjectName
);
opaque!(
    /// An item as its site names it, for display. A caller never parses it.
    ItemName
);

opaque!(
    /// A comment's id, made by the adapter and read back. Stored and
    /// compared only.
    CommentId
);
opaque!(
    /// A release target's id, made by the adapter. Stored and compared only.
    TargetId
);
opaque!(
    /// A release's id, made by the adapter. Stored and compared only.
    ReleaseId
);
opaque!(
    /// An attachment's id, made by the adapter. Stored and compared only.
    AttachmentId
);
opaque!(
    /// What an item looked like when read; a conditional write names it.
    Revision
);
opaque!(
    /// Equal tokens mean nothing changed.
    ChangeToken
);
opaque!(
    /// Where a delta read stopped.
    Cursor
);

checked!(
    /// A commit. Checked when made: hex, 7 to 64 characters.
    CommitId,
    "commit id",
    commit_ok
);

fn commit_ok(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

checked!(
    /// A branch. Checked when made: git's rules for a ref name.
    Branch,
    "branch",
    ref_name_ok
);
checked!(
    /// A tag. Checked when made: git's rules for a ref name.
    TagName,
    "tag",
    ref_name_ok
);

/// Git's rules for a ref name, and no leading `-`, as git asks of a branch.
fn ref_name_ok(s: &str) -> bool {
    let bad_byte = |b: u8| {
        b < 0x20 || b == 0x7f || matches!(b, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
    };
    !s.is_empty()
        && s != "@"
        && !s.starts_with(['/', '-'])
        && !s.ends_with(['/', '.'])
        && !s.contains("..")
        && !s.contains("//")
        && !s.contains("@{")
        && !s.bytes().any(bad_byte)
        && s.split('/')
            .all(|part| !part.starts_with('.') && !part.ends_with(".lock"))
}

checked!(
    /// A file of the product, relative, no `..`, no leading `/`.
    RepoPath,
    "product path",
    repo_path_ok
);

fn repo_path_ok(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('/') && s.split('/').all(|part| part != "..")
}

checked!(
    /// A name a project gives its own mark, or a site gives a type of
    /// ticket. Checked when made: ASCII letters, digits, `-`, `_`, `:`,
    /// `.`, `/` and spaces, 1 to 50. A write of one outside the rule cannot be made. Reading a
    /// site's list of marks never fails on a name: [`MarkName::read_list`]
    /// leaves a name that breaks the rule out of the list and returns it, for
    /// the adapter to report through its notices, as it reports a variant it
    /// does not know.
    MarkName,
    "mark name",
    mark_name_ok
);

/// A site's list of mark names, read: the names that keep the rule, in the
/// list's order, and the names left out, each once, for a notice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkNames {
    /// The names that keep the rule, in the order the site gave them.
    pub kept: Vec<MarkName>,
    /// The names left out because they break the rule, as the site gave them.
    pub left_out: Vec<String>,
}

impl MarkName {
    /// A site's list of names, read without failing: each name that keeps
    /// the rule is kept, and each that breaks it is left out and returned.
    pub fn read_list<S: Into<String>>(names: impl IntoIterator<Item = S>) -> MarkNames {
        let mut read = MarkNames::default();
        for name in names {
            let name = name.into();
            match MarkName::new(name.clone()) {
                Ok(kept) => read.kept.push(kept),
                Err(_) if !read.left_out.contains(&name) => read.left_out.push(name),
                Err(_) => {}
            }
        }
        read
    }
}

fn mark_name_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 50
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.' | b'/' | b' ')
        })
}

checked!(
    /// A file of a release, as the release names it. Checked when made: not
    /// empty, no `/` or `\`, not `.` or `..`, no control character.
    FileName,
    "release file name",
    file_name_ok
);

fn file_name_ok(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.chars().any(|c| c == '/' || c == '\\' || c.is_control())
}

/// Where to read a file of the product.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rev {
    /// At a commit.
    Commit(CommitId),
    /// At a branch's head.
    Branch(Branch),
    /// At a tag.
    Tag(TagName),
}

checked!(
    /// A link a person opens. Returned for display, and taken in one place
    /// only: a commit status's link, which the site shows and never
    /// follows. Checked when made: `https`, no user or password part, no
    /// space or control character, at most 2,048 characters. The adapter
    /// passes it through as text and never calls it.
    WebLink,
    "web link",
    web_link_ok
);

fn web_link_ok(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    s.chars().count() <= 2048
        && !host.is_empty()
        && !host.contains('@')
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

checked!(
    /// A git remote, `https` or `ssh`, returned for git alone, never taken by
    /// a port. Checked when made: `https://`, `ssh://`, or the short ssh form
    /// `user@host:path`.
    GitRemote,
    "git remote",
    git_remote_ok
);

fn git_remote_ok(s: &str) -> bool {
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    if let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("ssh://"))
    {
        return !rest.is_empty();
    }
    // The short form: user@host:path, with no scheme.
    match s.split_once(':') {
        Some((user_host, path)) => {
            !path.is_empty()
                && !user_host.contains('/')
                && user_host
                    .split_once('@')
                    .is_some_and(|(user, host)| !user.is_empty() && !host.is_empty())
        }
        None => false,
    }
}

/// Seconds since the epoch, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct At(i64);

impl At {
    /// The time, `secs` seconds after the epoch.
    pub fn new(secs: i64) -> Self {
        Self(secs)
    }

    /// Seconds since the epoch.
    pub fn secs(self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_text_is_1_to_128_of_the_tables_key_set() {
        assert!(KeyText::new("ABC-12").is_ok());
        assert!(KeyText::new("123").is_ok());
        assert!(KeyText::new("org/repo:12").is_ok());
        for p in [".", "_", ":", "/", "-"] {
            assert!(KeyText::new(format!("a{p}b")).is_ok(), "{p}");
            assert!(KeyText::new(p).is_ok(), "{p}");
        }
        assert!(KeyText::new("a".repeat(128)).is_ok());
        assert_eq!(
            KeyText::new("a".repeat(129)),
            Err(NotValid { what: "ticket key" })
        );
        for bad in ["", "AB 12", "AB#12", "a>b", "-->", "a,b", "\u{e4}", "a\tb"] {
            assert!(KeyText::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn numbers_are_never_zero() {
        assert_eq!(BlueprintNo::new(0), None);
        assert_eq!(BlueprintNo::new(7).map(BlueprintNo::get), Some(7));
        assert_eq!(ChangeNo::new(0), None);
    }

    #[test]
    fn a_commit_is_7_to_64_hex() {
        assert!(CommitId::new("abc1234").is_ok());
        assert!(CommitId::new("ABC1234").is_ok());
        assert!(CommitId::new("f".repeat(64)).is_ok());
        assert!(CommitId::new("abc123").is_err());
        assert!(CommitId::new("f".repeat(65)).is_err());
        assert!(CommitId::new("abc123g").is_err());
    }

    #[test]
    fn a_ref_name_keeps_gits_rules() {
        for good in ["main", "fix/typo", "v2.0.0", "a-b_c", "t4.1a"] {
            assert!(Branch::new(good).is_ok(), "{good}");
            assert!(TagName::new(good).is_ok(), "{good}");
        }
        for bad in [
            "", "@", "/main", "main/", "main.", "a..b", "a//b", "a@{b", "a b", "a~b", "a^b", "a:b",
            "a?b", "a*b", "a[b", "a\\b", "a\tb", ".hidden", "a/.b", "a.lock", "a/b.lock", "-x",
        ] {
            assert!(Branch::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_product_path_is_relative_with_no_dot_dot() {
        assert!(RepoPath::new("src/lib.rs").is_ok());
        assert!(RepoPath::new("a..b/c").is_ok());
        assert!(RepoPath::new("").is_err());
        assert!(RepoPath::new("/etc/passwd").is_err());
        assert!(RepoPath::new("a/../b").is_err());
        assert!(RepoPath::new("..").is_err());
    }

    #[test]
    fn a_mark_name_is_up_to_50_of_its_set() {
        assert!(MarkName::new("area: docs").is_ok());
        assert!(MarkName::new("v1.2-later").is_ok());
        assert!(MarkName::new("a".repeat(50)).is_ok());
        assert!(MarkName::new("a".repeat(51)).is_err());
        assert!(MarkName::new("").is_err());
        for bad in ["a#b", "a,b", "a\\b", "a\tb", "\u{e4}", "a>b"] {
            assert!(MarkName::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_mark_name_may_hold_underscores_and_slashes() {
        for good in ["dependabot_style", "area/ui", "kind/bug", "a_b/c"] {
            assert!(MarkName::new(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn reading_a_list_of_mark_names_never_fails_and_leaves_out_a_bad_name() {
        let read = MarkName::read_list([
            "area: docs",
            "a#b",
            "v1.2-later",
            "",
            "a#b",
            &"a".repeat(51),
        ]);
        let kept: Vec<&str> = read.kept.iter().map(MarkName::as_str).collect();
        assert_eq!(kept, ["area: docs", "v1.2-later"]);
        assert_eq!(
            read.left_out,
            ["a#b".to_string(), String::new(), "a".repeat(51)]
        );
        assert_eq!(
            MarkName::read_list(Vec::<String>::new()),
            MarkNames::default()
        );
    }

    #[test]
    fn a_release_file_name_is_one_name() {
        assert!(FileName::new("tehas-2.0.0.tar.gz").is_ok());
        for bad in ["", ".", "..", "a/b", "a\\b", "a\nb"] {
            assert!(FileName::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_web_link_is_https_with_no_user_and_at_most_2048() {
        assert!(WebLink::new("https://example.test/a?b=c#d").is_ok());
        let long = format!("https://example.test/{}", "a".repeat(2048 - 21));
        assert_eq!(long.len(), 2048);
        assert!(WebLink::new(long.clone()).is_ok());
        assert!(WebLink::new(format!("{long}a")).is_err());
        for bad in [
            "http://example.test/",
            "https://",
            "https:///path",
            "https://user@example.test/",
            "https://user:pass@example.test/",
            "https://example.test/a b",
            "ftp://example.test/",
            "example.test",
        ] {
            assert!(WebLink::new(bad).is_err(), "{bad:?}");
        }
        assert!(WebLink::new("https://example.test/a@b").is_ok());
    }

    #[test]
    fn a_git_remote_is_https_or_ssh() {
        for good in [
            "https://example.test/o/p.git",
            "ssh://git@example.test/o/p.git",
            "git@example.test:o/p.git",
        ] {
            assert!(GitRemote::new(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "http://example.test/o/p",
            "file:///o/p",
            "example.test:o/p",
            "/o/p",
        ] {
            assert!(GitRemote::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn opaque_ids_keep_their_text() {
        assert_eq!(Revision::new("r1").as_str(), "r1");
        assert_eq!(At::new(-5).secs(), -5);
    }
}
