//! The three hour check. The five minute run removes what GitHub's list of open
//! pull requests no longer has. This does not trust that list or that run. It
//! starts from what is on the host, asks GitHub about each pull request by
//! itself, and removes what is merged, closed, not there, not ours or not
//! opened by one of ours. An answer that is not clear keeps everything: a sweep
//! never removes on a failed call.

use super::run::{
    docker_answers, docker_lines, docker_quiet, gh, is_number, same_repo, say, Preview, MAIN,
};
use super::verdict::{self, Pull};
use crate::edge;
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::{Command, Stdio};

const CONTAINERS: [&str; 6] = [
    "ps",
    "-a",
    "--filter",
    "label=tehas.preview",
    "--format",
    "{{.ID}} {{.Label \"tehas.preview\"}} {{.Label \"tehas.repo\"}} {{.Names}}",
];
const IMAGES: [&str; 6] = [
    "image",
    "ls",
    "--filter",
    "label=tehas.preview",
    "--format",
    "{{.ID}} {{.Repository}}:{{.Tag}}",
];

/// Only such a word is handed to "docker rm -f", which would take anything else
/// as a name or an option.
fn is_id(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A label that is this repository's pull request, or its default branch.
/// GitHub takes a repository's name in any capitals, and so does every
/// comparison here: the label was written from TEHAS_REPO as it was spelled
/// that day.
fn a_preview_here(number: &str, owner: &str, repo: &str) -> bool {
    (is_number(number) || number == MAIN) && same_repo(owner, repo)
}

/// A second container is one that has a name and none of its names is the
/// preview's. Docker gives several names with commas between them when a
/// legacy link points at a container, and a line with no name says nothing:
/// neither may take the serving container away.
fn a_second_container(name: &str, n: u64) -> bool {
    let serving = format!("tehas-preview-{n}");
    !name.is_empty() && !name.split(',').any(|part| part == serving)
}

/// An image tagged for a number or for main, of this repository or of none.
fn a_kept_image(tag: &str, owner: &str, repo: &str) -> bool {
    tag.strip_prefix("tehas-preview:")
        .is_some_and(|name| is_number(name) || name == MAIN)
        && (owner.is_empty() || same_repo(owner, repo))
}

/// The number an image line is tagged for, as a word of digits.
fn image_number(line: &str) -> Option<&str> {
    let (_, tag) = line.split_once(' ')?;
    tag.strip_prefix("tehas-preview:")
        .filter(|name| name.bytes().all(|b| b.is_ascii_digit()))
}

/// The id and the name of each container labelled with this number.
fn of_number<'a>(listing: &'a str, n: u64, repo: &str) -> Vec<(&'a str, &'a str)> {
    let n = n.to_string();
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (id, number, owner, name) = (
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next(),
            );
            (number == n && same_repo(owner, repo)).then_some((id, name.unwrap_or("")))
        })
        .collect()
}

fn repository_shown(answer: Option<&str>, repo: &str) -> bool {
    answer
        .and_then(|answer| serde_json::from_str::<Value>(answer).ok())
        .and_then(|answer| answer.get("full_name")?.as_str().map(str::to_string))
        .is_some_and(|name| name.eq_ignore_ascii_case(repo))
}

impl Preview {
    fn refs(&self) -> Vec<String> {
        if !self.bare.is_dir() {
            return Vec::new();
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.bare)
            .args(["for-each-ref", "--format=%(refname)", "refs/preview"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        out.map_or(Vec::new(), |out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|line| line.rsplit('/').next().unwrap_or(line).to_string())
                .collect()
        })
    }

    /// What is on the host under a number: directories, containers of this
    /// repository, images and fetched refs.
    fn numbers(&self) -> BTreeSet<u64> {
        let mut words = edge::numbers(&self.root);
        words.extend(self.container_numbers(&CONTAINERS));
        let images = docker_lines(&IMAGES);
        words.extend(images.lines().filter_map(image_number).map(str::to_string));
        words.extend(self.refs());
        words
            .iter()
            .filter(|word| is_number(word))
            .filter_map(|word| word.parse().ok())
            .collect()
    }

    /// Containers that carry the label and are no pull request of this
    /// repository.
    fn strays(&self, removed: &mut u64) {
        for line in docker_lines(&CONTAINERS).lines() {
            let [id, number, owner, _] = edge::words(line);
            if id.is_empty() || a_preview_here(number, owner, &self.repo) {
                continue;
            }
            if !is_id(id) {
                say(format_args!("an id that is not one, skipped"));
                continue;
            }
            if docker_quiet(&["rm", "-f", "-v", id]) {
                let repo = &self.repo;
                say(format_args!(
                    "container {id}: swept, it belongs to no pull request of {repo}"
                ));
                *removed += 1;
            }
        }
    }

    /// The default branch's image is no pull request's and is never swept. One
    /// that is not tagged for a number or for main is what a rebuild left, and
    /// one of another repository is not ours. Docker refuses to remove an image
    /// a container runs, which is the wanted answer.
    fn stray_images(&self, removed: &mut u64) {
        let blank = [' ', '\t'];
        for line in docker_lines(&IMAGES).lines() {
            let line = line.trim_matches(blank);
            let (id, tag) = line.split_once(blank).unwrap_or((line, ""));
            if id.is_empty() {
                continue;
            }
            let tag = tag.trim_start_matches(blank);
            let label = "{{index .Config.Labels \"tehas.repo\"}}";
            let inspect = ["image", "inspect", "--format", label, id];
            let owner = docker_lines(&inspect);
            if a_kept_image(tag, owner.trim_end_matches('\n'), &self.repo) {
                continue;
            }
            if docker_quiet(&["image", "rm", id]) {
                say(format_args!("image {id}: swept, it belongs to no preview"));
                *removed += 1;
            }
        }
    }

    /// A second container of a pull request that stays goes by id.
    fn seconds(&self, n: u64, listing: &str, removed: &mut u64) {
        for (id, name) in of_number(listing, n, &self.repo) {
            if !a_second_container(name, n) {
                continue;
            }
            if !is_id(id) {
                say(format_args!("an id that is not one, skipped"));
                continue;
            }
            if docker_quiet(&["rm", "-f", "-v", id]) {
                say(format_args!(
                    "container {id}: swept, a second container of #{n}"
                ));
                *removed += 1;
            }
        }
    }

    /// Every container of the number, by id: a purge removes the name
    /// tehas-preview-<n> only, and a labelled container of any other name would
    /// stay for ever and be swept again at every check. False when one is left.
    fn every_container_gone(&self, n: u64, listing: &str) -> bool {
        let mut gone = true;
        for (id, _) in of_number(listing, n, &self.repo) {
            if !is_id(id) {
                say(format_args!("an id that is not one, skipped"));
                gone = false;
            } else if !docker_quiet(&["rm", "-f", "-v", id]) {
                gone = false;
            }
        }
        gone
    }

    pub(super) fn sweep(&mut self) -> u8 {
        if !docker_answers() {
            say(format_args!("Docker does not answer; nothing was swept"));
            return 0;
        }
        let mut removed = 0;
        self.strays(&mut removed);
        self.stray_images(&mut removed);
        let mut numbers = self.numbers();

        // A 404 means "no such pull request" only when GitHub shows the
        // repository at all: a token that lost access gets a 404 for every pull
        // request, and that must not remove every preview.
        let repo = self.repo.clone();
        if !repository_shown(gh(&format!("repos/{repo}")).as_deref(), &repo) {
            say(format_args!(
                "GitHub does not show {repo}; nothing was swept by its word"
            ));
            numbers.clear();
        }

        for n in numbers {
            let verdict = verdict::ask(n, &repo);
            let listing = docker_lines(&CONTAINERS);
            let why = match verdict {
                Pull::Keep => {
                    self.seconds(n, &listing, &mut removed);
                    continue;
                }
                Pull::Unclear => {
                    say(format_args!("#{n}: no answer from GitHub, kept"));
                    continue;
                }
                Pull::Closed => "the pull request is closed".to_string(),
                Pull::Gone => "there is no such pull request".to_string(),
                Pull::Outside => format!("it was not opened by someone of {repo}"),
                Pull::Elsewhere => format!("its branch is not in {repo}"),
            };
            // Swept is said and counted when it is true. With a container
            // still there the rest stays too, so the next sweep finds the
            // number again.
            if !self.every_container_gone(n, &listing) {
                say(format_args!(
                    "#{n}: a container could not be removed, tried again at the next sweep"
                ));
                continue;
            }
            self.purge(&n.to_string());
            removed += 1;
            say(format_args!("#{n}: swept, {why}"));
        }

        if !self.write_sweep(removed) {
            return 1;
        }
        self.measure();
        u8::from(!self.write_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "acme/widget";

    #[test]
    fn an_id_is_lowercase_hex_and_nothing_else() {
        for id in ["abc123", "0", "c0de01"] {
            assert!(is_id(id), "{id}");
        }
        for id in ["", "-f", "ABC", "abcg", "ab c", "tehas-preview-12"] {
            assert!(!is_id(id), "{id}");
        }
    }

    #[test]
    fn a_label_is_a_preview_of_this_repository_or_a_stray() {
        assert!(a_preview_here("12", REPO, REPO));
        assert!(a_preview_here("main", "Acme/WIDGET", REPO));
        for (number, owner) in [
            ("12", "other/thing"),
            ("12", "acme/widget-two"),
            ("12", ""),
            ("", REPO),
            ("012", REPO),
            ("1234567890", REPO),
            ("nightly", REPO),
        ] {
            assert!(!a_preview_here(number, owner, REPO), "{number} {owner}");
        }
    }

    #[test]
    fn a_second_container_has_a_name_and_none_is_the_previews() {
        for name in [
            "tehas-preview-64",
            "tehas-preview-64,proxy/app",
            "proxy/a,tehas-preview-64,proxy/b",
            "",
        ] {
            assert!(!a_second_container(name, 64), "{name}");
        }
        for name in ["extra", "tehas-preview-640", "x,old-tehas-preview-64"] {
            assert!(a_second_container(name, 64), "{name}");
        }
    }

    #[test]
    fn an_image_is_kept_by_its_tag_and_its_label() {
        assert!(a_kept_image("tehas-preview:12", "", REPO));
        assert!(a_kept_image("tehas-preview:main", "ACME/widget", REPO));
        for (tag, owner) in [
            ("<none>:<none>", ""),
            ("tehas-preview:012", ""),
            ("tehas-preview:latest", ""),
            ("tehas-preview:12", "other/thing"),
            ("other:12", ""),
            ("tehas-preview:12 x", ""),
        ] {
            assert!(!a_kept_image(tag, owner, REPO), "{tag} {owner}");
        }
    }

    #[test]
    fn an_image_line_gives_digits_only() {
        assert_eq!(image_number("sha256:a tehas-preview:12"), Some("12"));
        assert_eq!(image_number("sha256:a tehas-preview:main"), None);
        assert_eq!(image_number("sha256:a <none>:<none>"), None);
        assert_eq!(image_number("tehas-preview:12"), None);
    }

    #[test]
    fn a_listing_of_a_number_is_this_repositorys_containers_of_it() {
        let listing = "ab12 12 Acme/Widget tehas-preview-12\n\
                       c0de 12 acme/widget\n\
                       dead 12 other/thing x\n\
                       beef 120 acme/widget y\n";
        assert_eq!(
            of_number(listing, 12, REPO),
            [("ab12", "tehas-preview-12"), ("c0de", "")]
        );
    }

    #[test]
    fn a_repository_is_shown_only_when_the_answer_names_it() {
        assert!(repository_shown(
            Some("{\"full_name\":\"Acme/Widget\"}"),
            REPO
        ));
        for answer in [
            None,
            Some(""),
            Some("[]"),
            Some("\"acme/widget\""),
            Some("{}"),
            Some("{\"full_name\":null}"),
            Some("{\"full_name\":\"other/thing\"}"),
            Some("{\"full_name\":\"acme/widget-two\"}"),
        ] {
            assert!(!repository_shown(answer, REPO), "{answer:?}");
        }
    }
}
