//! The rules of a plugin's Dockerfile, read before its build
//! (docs/specs/2026-11-21-plugins-security.md, "The build"): every FROM by
//! `@sha256:` digest with no ARG in it, no `# syntax=` line, COPY --from and
//! RUN --mount=from= of a stage of the file only, no ADD of a URL or a git
//! address. It reads what BuildKit reads: parser directives and the escape
//! character, comments, line continuations and heredocs. What it cannot read
//! as BuildKit would is refused, so a reading that differs fails closed.

/// What a Dockerfile comes to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Read {
    /// The images its FROM lines name, each once, in order. `scratch` and an
    /// earlier stage are no image.
    pub bases: Vec<String>,
    /// Each rule it breaks, naming the line.
    pub refusals: Vec<String>,
}

impl Read {
    fn refuse(&mut self, line: usize, why: impl std::fmt::Display) {
        self.refusals.push(format!("line {line}: {why}"));
    }
}

/// The instructions whose lines may start heredocs, as BuildKit has them.
const HEREDOCS: [&str; 3] = ["run", "copy", "add"];

/// A parser directive, `# key=value`: the key in lower case and the value.
fn directive(line: &str) -> Option<(String, &str)> {
    let rest = line.trim_start().strip_prefix('#')?.trim_start();
    let (key, value) = rest.split_once('=')?;
    let key = key.trim_end();
    let mut chars = key.chars();
    let named = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric());
    named.then(|| (key.to_ascii_lowercase(), value.trim()))
}

/// Whether a line reads as the syntax directive, wherever it is: the
/// frontend it would name is code tehas never reviewed.
fn is_syntax(line: &str) -> bool {
    directive(line).is_some_and(|(key, _)| key == "syntax")
}

/// A line without its continuation: the text before the escape character
/// at its end, and whether it continues.
fn continued(line: &str, esc: char) -> (&str, bool) {
    let t = line.trim_end_matches([' ', '\t']);
    match t.strip_suffix(esc) {
        Some(body) => (body, true),
        None => (line, false),
    }
}

/// One word of a line: `raw` keeps its quotes, as BuildKit reads a heredoc's
/// word, and `plain` is the word the shell would make.
#[derive(Debug, PartialEq, Eq)]
struct Word {
    raw: String,
    plain: String,
}

/// The words of a line, split where whitespace is outside quotes. Err: a
/// quote that does not end.
fn words(line: &str, esc: char) -> Result<Vec<Word>, String> {
    let mut out = Vec::new();
    let mut raw = String::new();
    let mut plain = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    let mut open = false;
    while let Some(c) = chars.next() {
        match quote {
            Some('\'') if c == '\'' => {
                raw.push(c);
                quote = None;
            }
            Some('"') if c == '"' => {
                raw.push(c);
                quote = None;
            }
            Some('"') if c == esc => {
                raw.push(c);
                if let Some(n) = chars.next() {
                    raw.push(n);
                    plain.push(n);
                }
            }
            Some(_) => {
                raw.push(c);
                plain.push(c);
            }
            None if c.is_whitespace() => {
                if open {
                    out.push(Word {
                        raw: std::mem::take(&mut raw),
                        plain: std::mem::take(&mut plain),
                    });
                    open = false;
                }
                continue;
            }
            None if c == '\'' || c == '"' => {
                raw.push(c);
                quote = Some(c);
            }
            None if c == esc => {
                raw.push(c);
                if let Some(n) = chars.next() {
                    raw.push(n);
                    plain.push(n);
                }
            }
            None => {
                raw.push(c);
                plain.push(c);
            }
        }
        open = true;
    }
    if quote.is_some() {
        return Err("a quote that does not end".to_string());
    }
    if open {
        out.push(Word { raw, plain });
    }
    Ok(out)
}

/// A heredoc a word starts: its name and whether its body's leading tabs go.
/// None: the word starts none. Err: one this cannot read as BuildKit would.
fn heredoc(raw: &str) -> Result<Option<(String, bool)>, String> {
    let rest = raw.trim_start_matches(|c: char| c.is_ascii_digit());
    let Some(rest) = rest.strip_prefix("<<") else {
        return Ok(None);
    };
    let (rest, tabs) = match rest.strip_prefix('-') {
        Some(r) => (r, true),
        None => (rest, false),
    };
    if rest.starts_with('<') {
        return Ok(None);
    }
    let name = ['"', '\'']
        .iter()
        .find_map(|q| rest.strip_prefix(*q).and_then(|r| r.strip_suffix(*q)))
        .unwrap_or(rest);
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    if !plain {
        return Err(format!("a heredoc {raw} this reader cannot follow"));
    }
    Ok(Some((name.to_string(), tabs)))
}

/// The stages so far: each one's name, if it has one, in lower case.
#[derive(Default)]
struct Stages(Vec<Option<String>>);

impl Stages {
    /// Whether `from` names a stage before the current one, by name or index.
    fn earlier(&self, from: &str) -> bool {
        let before = &self.0[..self.0.len().saturating_sub(1)];
        let from = from.to_ascii_lowercase();
        if !from.is_empty() && from.bytes().all(|b| b.is_ascii_digit()) {
            return from.parse::<usize>().is_ok_and(|i| i < before.len());
        }
        before.iter().flatten().any(|n| *n == from)
    }

    /// Whether `name` is any stage so far, for FROM.
    fn named(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.0.iter().flatten().any(|n| *n == name)
    }
}

/// An image named by digest: a lower-case name, a tag if any, then
/// `@sha256:` and 64 hex digits.
fn by_digest(image: &str) -> bool {
    let Some((name, digest)) = image.split_once("@sha256:") else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/:-".contains(&b))
        && digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A source of ADD that BuildKit would fetch: a URL or a git address.
fn fetched(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    lower.contains("://")
        || lower.starts_with("git@")
        || lower.starts_with("github.com/")
        || lower.ends_with(".git")
        || lower.contains(".git#")
}

/// The flags at the front of an instruction's words, as `(name, value)`,
/// then the rest. A flag without `=` has no value.
fn flags(words: &[Word]) -> (Vec<(String, Option<String>)>, &[Word]) {
    let n = words
        .iter()
        .take_while(|w| w.plain.starts_with("--") && w.plain != "--")
        .count();
    let list = words[..n]
        .iter()
        .map(|w| match w.plain[2..].split_once('=') {
            Some((k, v)) => (k.to_ascii_lowercase(), Some(v.to_string())),
            None => (w.plain[2..].to_ascii_lowercase(), None),
        })
        .collect();
    (list, &words[n..])
}

/// The arguments after the flags: a JSON list of strings when the text is
/// one, the words otherwise.
fn args(rest: &[Word]) -> Vec<String> {
    let text = rest.iter().map(|w| w.raw.as_str()).collect::<Vec<_>>();
    let text = text.join(" ");
    if text.starts_with('[') {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&text) {
            return list;
        }
    }
    rest.iter().map(|w| w.plain.clone()).collect()
}

/// Read a Dockerfile.
pub fn read(bytes: &[u8]) -> Read {
    let mut out = Read::default();
    let Ok(text) = std::str::from_utf8(bytes) else {
        out.refusals.push("the Dockerfile is not UTF-8".to_string());
        return out;
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    for (i, l) in lines.iter().enumerate() {
        if is_syntax(l) {
            out.refuse(
                i + 1,
                "a # syntax= line, which would build with a frontend of its own",
            );
        }
    }

    let mut esc = '\\';
    let mut i = 0;
    while let Some((key, value)) = lines.get(i).and_then(|l| directive(l)) {
        if key == "escape" {
            match value {
                "\\" => esc = '\\',
                "`" => esc = '`',
                _ => out.refuse(i + 1, format!("the escape character {value:?}")),
            }
        }
        i += 1;
    }

    let mut stages = Stages::default();
    while i < lines.len() {
        let first = i + 1;
        let t = lines[i].trim_start();
        i += 1;
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let mut joined = String::new();
        let (body, mut more) = continued(t, esc);
        joined.push_str(body);
        while more {
            let Some(next) = lines.get(i) else { break };
            i += 1;
            let nt = next.trim_start();
            if nt.is_empty() || nt.starts_with('#') {
                continue;
            }
            let (body, m) = continued(next, esc);
            joined.push_str(body);
            more = m;
        }
        let ws = match words(&joined, esc) {
            Ok(ws) => ws,
            Err(why) => {
                out.refuse(first, why);
                continue;
            }
        };
        let name = ws[0].plain.to_ascii_lowercase();
        if HEREDOCS.contains(&name.as_str()) {
            for w in &ws[1..] {
                match heredoc(&w.raw) {
                    Ok(None) => {}
                    Ok(Some((end, tabs))) => {
                        let ends = |l: &str| {
                            if tabs {
                                l.trim_start_matches('\t') == end
                            } else {
                                l == end
                            }
                        };
                        match lines[i..].iter().position(|l| ends(l)) {
                            Some(n) => i += n + 1,
                            None => {
                                out.refuse(first, format!("the heredoc {end} does not end"));
                                i = lines.len();
                            }
                        }
                    }
                    Err(why) => {
                        out.refuse(first, why);
                        i = lines.len();
                        break;
                    }
                }
            }
        }
        instruction(&ws, first, &mut stages, &mut out);
    }
    out
}

/// The rules of one instruction, given its words.
fn instruction(ws: &[Word], line: usize, stages: &mut Stages, out: &mut Read) {
    let name = ws[0].plain.to_ascii_lowercase();
    let rest = &ws[1..];
    match name.as_str() {
        "onbuild" if !rest.is_empty() => instruction(rest, line, stages, out),
        "from" => from(rest, line, stages, out),
        "copy" => {
            let (fl, _) = flags(rest);
            for (key, value) in fl {
                if key == "from" {
                    stage_from(value.as_deref(), "COPY --from", line, stages, out);
                }
            }
        }
        "run" => {
            let (fl, _) = flags(rest);
            for (key, value) in fl {
                if key != "mount" {
                    continue;
                }
                let Some(value) = value else {
                    out.refuse(line, "RUN --mount without its options");
                    continue;
                };
                for field in value.split(',') {
                    let (k, v) = field.split_once('=').unwrap_or((field, ""));
                    if k.trim().eq_ignore_ascii_case("from") {
                        stage_from(Some(v.trim()), "RUN --mount=from", line, stages, out);
                    }
                }
            }
        }
        "add" => {
            let (_, rest) = flags(rest);
            let a = args(rest);
            for src in a.iter().take(a.len().saturating_sub(1)) {
                if src.contains('$') {
                    out.refuse(line, format!("ADD {src}: an ARG in a source"));
                } else if fetched(src) {
                    out.refuse(line, format!("ADD {src}: a URL or a git address"));
                }
            }
        }
        _ => {}
    }
}

/// `FROM [--platform=<p>] <image> [AS <name>]`.
fn from(rest: &[Word], line: usize, stages: &mut Stages, out: &mut Read) {
    let (_, rest) = flags(rest);
    let (image, alias) = match rest {
        [image] => (image.plain.as_str(), None),
        [image, a, alias] if a.plain.eq_ignore_ascii_case("as") => {
            (image.plain.as_str(), Some(alias.plain.to_ascii_lowercase()))
        }
        _ => {
            out.refuse(line, "expected FROM [--platform=<p>] <image> [AS <name>]");
            stages.0.push(None);
            return;
        }
    };
    let stage = image.eq_ignore_ascii_case("scratch") || stages.named(image);
    if image.contains('$') {
        out.refuse(line, format!("FROM {image}: an ARG in the image"));
    } else if !stage && !by_digest(image) {
        out.refuse(line, format!("FROM {image}: not named by @sha256: digest"));
    } else if !stage && !out.bases.iter().any(|b| b == image) {
        out.bases.push(image.to_string());
    }
    stages.0.push(alias);
}

/// A `--from` that must name an earlier stage of the file.
fn stage_from(value: Option<&str>, what: &str, line: usize, stages: &Stages, out: &mut Read) {
    match value {
        None | Some("") => out.refuse(line, format!("{what} without a stage")),
        Some(v) if v.contains('$') => out.refuse(line, format!("{what}={v}: an ARG in a stage")),
        Some(v) if !stages.earlier(v) => out.refuse(
            line,
            format!("{what}={v}: not an earlier stage of this file"),
        ),
        Some(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn base() -> String {
        format!("node:22-bookworm@sha256:{D}")
    }

    fn of(text: &str) -> Read {
        read(text.as_bytes())
    }

    fn refused(text: &str) -> Vec<String> {
        of(text).refusals
    }

    #[test]
    fn a_dockerfile_of_digests_and_stages_passes_and_names_its_bases() {
        let text = format!(
            "# check=error=true\n\
             FROM --platform=$BUILDPLATFORM {b} AS build\n\
             RUN --mount=type=cache,target=/c npm ci\n\
             from alpine@sha256:{D} as Second\n\
             COPY --from=build /app /app\n\
             COPY --from=0 /a /b\n\
             FROM build\n\
             RUN --mount=type=bind,from=second,source=/x,target=/y true\n\
             FROM scratch\n\
             ADD ./local.tar /\n\
             COPY --from=Build /app /\n",
            b = base()
        );
        let got = of(&text);
        assert_eq!(got.refusals, Vec::<String>::new());
        assert_eq!(got.bases, vec![base(), format!("alpine@sha256:{D}")]);
    }

    #[test]
    fn a_from_needs_a_digest() {
        assert_eq!(
            refused("FROM node:22\n"),
            ["line 1: FROM node:22: not named by @sha256: digest"]
        );
        assert_eq!(refused("FROM node@sha256:abc\n").len(), 1);
        assert_eq!(refused(&format!("FROM Node@sha256:{D}\n")).len(), 1);
        assert_eq!(
            refused(&format!("FROM node@sha256:{}\n", D.to_uppercase())).len(),
            1
        );
        // A stage named later is no earlier stage: it is an image.
        assert_eq!(refused("FROM later\nFROM scratch AS later\n").len(), 1);
    }

    #[test]
    fn a_from_with_an_arg_is_refused() {
        let text = format!("ARG V={D}\nFROM node@sha256:$V\nFROM ${{IMG:-x@sha256:{D}}}\n");
        let got = refused(&text);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].starts_with("line 2: ") && got[0].ends_with("an ARG in the image"));
        assert!(got[1].starts_with("line 3: "));
    }

    #[test]
    fn a_from_of_another_shape_is_refused() {
        let got = refused(&format!("FROM {} AS a b\n", base()));
        assert_eq!(
            got,
            ["line 1: expected FROM [--platform=<p>] <image> [AS <name>]"]
        );
    }

    #[test]
    fn a_syntax_line_is_refused_wherever_it_is() {
        let b = base();
        assert_eq!(
            refused(&format!("# syntax=docker/dockerfile:1\nFROM {b}\n")),
            ["line 1: a # syntax= line, which would build with a frontend of its own"]
        );
        assert_eq!(refused(&format!("#SYNTAX = x\nFROM {b}\n")).len(), 1);
        assert_eq!(refused(&format!("FROM {b}\n\n# syntax=x\n")).len(), 1);
        assert_eq!(refused(&format!("# a comment\nFROM {b}\n")).len(), 0);
    }

    #[test]
    fn copy_from_names_only_an_earlier_stage() {
        let b = base();
        let got = refused(&format!(
            "FROM {b} AS a\nCOPY --from=a /x /y\nCOPY --from=nginx:latest /x /y\n\
             COPY --from=1 /x /y\nCOPY --from=$S /x /y\nCOPY --from /x /y\n\
             copy --chown=1:1 --FROM=evil /x /y\n"
        ));
        assert_eq!(
            got,
            [
                "line 2: COPY --from=a: not an earlier stage of this file",
                "line 3: COPY --from=nginx:latest: not an earlier stage of this file",
                "line 4: COPY --from=1: not an earlier stage of this file",
                "line 5: COPY --from=$S: an ARG in a stage",
                "line 6: COPY --from without a stage",
                "line 7: COPY --from=evil: not an earlier stage of this file",
            ]
        );
    }

    #[test]
    fn a_stage_by_index_is_one_before_the_current() {
        let b = base();
        let text = format!("FROM {b}\nFROM {b}\nCOPY --from=0 /x /y\nCOPY --from=1 /x /y\n");
        assert_eq!(
            refused(&text),
            ["line 4: COPY --from=1: not an earlier stage of this file"]
        );
    }

    #[test]
    fn run_mount_from_names_only_an_earlier_stage() {
        let b = base();
        let got = refused(&format!(
            "FROM {b} AS a\nFROM {b}\n\
             RUN --mount=type=bind,from=a,target=/a true\n\
             RUN --mount=type=bind,FROM=busybox,target=/b true\n\
             RUN --network=none --mount=type=cache,target=/c,from=other true\n\
             run --mount true\n"
        ));
        assert_eq!(
            got,
            [
                "line 4: RUN --mount=from=busybox: not an earlier stage of this file",
                "line 5: RUN --mount=from=other: not an earlier stage of this file",
                "line 6: RUN --mount without its options",
            ]
        );
    }

    #[test]
    fn add_of_a_url_or_a_git_address_is_refused() {
        let b = base();
        let got = refused(&format!(
            "FROM {b}\nADD https://example.com/a.tgz /a\n\
             ADD --keep-git-dir=true git@github.com:o/r.git /r\n\
             ADD github.com/o/r /r\nADD [\"HTTP://x/y\", \"/y\"]\n\
             ADD ./r.git /r\nADD $SRC /s\nADD ./ok.tar /\n"
        ));
        assert_eq!(got.len(), 6, "{got:?}");
        for (n, line) in got.iter().enumerate() {
            assert!(line.starts_with(&format!("line {}: ADD ", n + 2)), "{line}");
        }
        assert!(got[5].ends_with("an ARG in a source"));
    }

    #[test]
    fn a_continuation_joins_lines_past_comments_and_blanks() {
        let b = base();
        let text = format!("FROM {b} AS a\nCOPY \\\n  # a comment\n\n  --from=\\\nnginx /x /y\n");
        assert_eq!(
            refused(&text),
            ["line 2: COPY --from=nginx: not an earlier stage of this file"]
        );
        // The instruction is FROM, continued.
        assert_eq!(refused("FROM \\\n node:22\n").len(), 1);
    }

    #[test]
    fn the_escape_directive_changes_the_continuation() {
        let b = base();
        let text = format!("# escape=`\nFROM {b}\nCOPY `\n --from=nginx /x /y\n");
        assert_eq!(
            refused(&text),
            ["line 3: COPY --from=nginx: not an earlier stage of this file"]
        );
        assert_eq!(refused(&format!("# escape=x\nFROM {b}\n")).len(), 1);
        // A directive after a blank line is a comment.
        let late = format!("\n# escape=`\nFROM {b}\nCOPY `\n --from=nginx /x /y\n");
        assert_eq!(refused(&late).len(), 0);
    }

    #[test]
    fn a_heredocs_body_is_not_read_as_instructions() {
        let b = base();
        let text = format!(
            "FROM {b}\nRUN <<EOF bash\nFROM evil\nCOPY --from=evil / /\nEOF\n\
             COPY <<-\"A\" <<'B' /x\nADD https://x/y /z\n\t\tA\nFROM evil\nB\n\
             RUN echo \"<<NOT\" a\nFROM nginx\n"
        );
        assert_eq!(
            refused(&text),
            ["line 12: FROM nginx: not named by @sha256: digest"]
        );
    }

    #[test]
    fn a_heredoc_that_does_not_end_or_cannot_be_read_is_refused() {
        let b = base();
        assert_eq!(
            refused(&format!("FROM {b}\nRUN <<EOF\necho\n")),
            ["line 2: the heredoc EOF does not end"]
        );
        assert_eq!(refused(&format!("FROM {b}\nRUN <<$X\n$X\n")).len(), 1);
        assert_eq!(refused(&format!("FROM {b}\nRUN <<\n")).len(), 1);
        // A here-string is no heredoc.
        assert_eq!(refused(&format!("FROM {b}\nRUN cat <<<x\n")).len(), 0);
    }

    #[test]
    fn onbuild_is_held_to_the_same_rules() {
        let b = base();
        assert_eq!(
            refused(&format!("FROM {b}\nONBUILD COPY --from=nginx / /\n")).len(),
            1
        );
    }

    #[test]
    fn crlf_a_bom_and_bytes_that_are_not_utf8() {
        let text = format!("\u{feff}FROM {}\r\nFROM node:22\r\n", base());
        assert_eq!(refused(&text).len(), 1);
        assert_eq!(
            read(b"FROM \xff\n").refusals,
            ["the Dockerfile is not UTF-8"]
        );
    }

    #[test]
    fn a_quote_that_does_not_end_is_refused() {
        assert_eq!(
            refused(&format!("FROM {}\nCOPY \"a /b\n", base())),
            ["line 2: a quote that does not end"]
        );
    }
}
