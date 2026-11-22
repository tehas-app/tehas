//! The store's build against a stub curl and a stub attach: the limits of an
//! item, a download tried again, the names it keeps, and a zip attached or
//! named by address. The unpacking itself is attach's and tested there; the
//! stub answers with what the zip's bytes say. Each case was a case of the
//! Protolab's shell test, which built pages through the store.
//!
//! One test runs every case in turn, after it made the stubs and set PATH.
//! curl hands out `<net>/<address, every other character _>`: a
//! file `<key>.rc` is an exit status to fail with, `<key>.moved` an address
//! a redirect sends it on to, `<key>.type` the content type it prints and
//! `<key>.unbounded` a download that ignores its cap. Every address asked is
//! written to `<net>/asked`, so each case uses addresses of its own.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tehas_core::issue_feed::{Refs, Zip};
use tehas_core::testdir::TestDir;
use tehas_items::store::{build, Store, LIMIT};

const F: &str = "https://github.com/user-attachments/files";
const X: &str = "https://files.example.org/p";

const CURL: &str = r#"#!/bin/sh
net=$(dirname "$0")/net
out="" url="" max=0 flags=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out=$2; shift ;;
    --max-filesize) max=$2; shift ;;
    -w|-H|--proto|--proto-redir|--max-redirs|--max-time) shift ;;
    -*) flags="$flags $1" ;;
    *) url=$1 ;;
  esac
  shift
done
printf '%s%s\n' "$url" "$flags" >>"$net/asked"
key=$(printf %s "$url" | tr -c 'A-Za-z0-9' _)
[ -f "$net/$key.rc" ] && exit "$(cat "$net/$key.rc")"
if [ -f "$net/$key.moved" ]; then
  : >"$out"
  printf 'text/html\n302\n%s' "$(cat "$net/$key.moved")"
  exit 0
fi
[ -f "$net/$key" ] || exit 22
[ -f "$net/$key.unbounded" ] || [ "$(stat -c %s "$net/$key")" -le "$max" ] || exit 63
cp "$net/$key" "$out"
printf '%s\n200\n' "$(cat "$net/$key.type" 2>/dev/null)"
"#;

/// attach unpack <zip> <folder> --max-bytes <n> --max-files <n>: the zip's
/// bytes are its answer, or, when they start BAD, a zip it cannot read. What
/// it was asked goes to attach.args in the work area, three above the folder.
const ATTACH: &str = r#"#!/bin/sh
folder=$3
printf '%s %s\n' "$5" "$7" >>"${folder%/*/*/*}/attach.args"
case "$(head -c 3 "$2")" in
  BAD) echo "attach: unreadable: not a zip"; exit 1 ;;
esac
mkdir -p "$folder"
cat "$2"
"#;

/// The directory of the stubs, which [`the_store`] makes and holds.
static STUBS: OnceLock<PathBuf> = OnceLock::new();

fn stubs() -> &'static Path {
    STUBS.get().expect("the stubs are made")
}

/// The stubs, with PATH set to find curl there. PATH is the process's, and
/// the stubs' directory goes when the one test ends.
fn made() -> TestDir {
    let dir = TestDir::new("items-stubs");
    fs::create_dir_all(dir.join("net")).unwrap();
    for (name, body) in [("curl", CURL), ("attach", ATTACH)] {
        fs::write(dir.join(name), body).unwrap();
        fs::set_permissions(
            dir.join(name),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
    }
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{path}", dir.display()));
    STUBS.set(dir.to_path_buf()).unwrap();
    dir
}

fn key(address: &str) -> PathBuf {
    let k: String = address
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                b as char
            } else {
                '_'
            }
        })
        .collect();
    stubs().join("net").join(k)
}

/// Something the network hands out at `address`.
fn net(address: &str, bytes: &[u8]) {
    fs::write(key(address), bytes).unwrap();
}

/// A file of `size` bytes at `address`, sparse.
fn net_sized(address: &str, size: u64) {
    fs::File::create(key(address))
        .unwrap()
        .set_len(size)
        .unwrap();
}

fn mark(address: &str, what: &str, text: &str) {
    fs::write(key(address).with_extension(what), text).unwrap();
}

fn unmark(address: &str, what: &str) {
    fs::remove_file(key(address).with_extension(what)).unwrap();
}

/// Every address asked that starts with `prefix`, in order.
fn asked(prefix: &str) -> Vec<String> {
    fs::read_to_string(stubs().join("net/asked"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

struct Case {
    store: Store,
    _home: TestDir,
    _work: TestDir,
}

/// A store whose home has the stub attach, or none.
fn case(name: &str, zip_hosts: &str, with_attach: bool) -> Case {
    let home = TestDir::new(&format!("items-home-{name}"));
    let work = TestDir::new(&format!("items-work-{name}"));
    fs::create_dir_all(home.join("libexec")).unwrap();
    if with_attach {
        std::os::unix::fs::symlink(stubs().join("attach"), home.join("libexec/attach")).unwrap();
    }
    Case {
        store: Store {
            home: home.to_path_buf(),
            work: work.to_path_buf(),
            token: String::new(),
            zip_hosts: zip_hosts.to_string(),
        },
        _home: home,
        _work: work,
    }
}

fn refs(fetch: &[&str], zips: Vec<Result<Zip, String>>) -> Refs {
    Refs {
        fetch: fetch.iter().map(|l| l.to_string()).collect(),
        zips,
        outside: 0,
    }
}

fn zip(link: &str, sha256: &str) -> Result<Zip, String> {
    Ok(Zip {
        link: link.to_string(),
        sha256: sha256.to_string(),
    })
}

/// What an item's .meta.json says.
struct Meta {
    json: Value,
}

impl Meta {
    fn names(&self, key: &str) -> Vec<String> {
        self.json[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap().to_string())
            .collect()
    }

    fn reason(&self, name: &str) -> String {
        self.json["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .map(|s| s["reason"].as_str().unwrap().to_string())
            .unwrap_or_else(|| panic!("{name} is not left out: {}", self.json))
    }

    fn skipped(&self) -> Vec<String> {
        self.json["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["reason"].as_str().unwrap().to_string())
            .collect()
    }

    fn bytes(&self) -> i64 {
        self.json["bytes"].as_i64().unwrap()
    }
}

/// Build item `n` and give its meta and the line for the log. The item
/// carries no time of its own: the store copies it as it copies the rest.
fn built(case: &Case, n: u64, found: &Refs) -> (Meta, String) {
    let item = json!({
        "number": n,
        "title": "Item",
        "url": format!("https://example.org/item/{n}"),
        "author": "alice",
    });
    let page = |_: &Value, files: &[Value], _: &[Value], _: i64| format!("{} files", files.len());
    let (dir, line) = build(&case.store, n, &item, found, "stamp", &page).expect("built");
    let meta = fs::read_to_string(dir.join(".meta.json")).unwrap();
    let json = serde_json::from_str(&meta).unwrap();
    (Meta { json }, line)
}

fn stamped(case: &Case, n: u64) -> bool {
    case.store.work.join(n.to_string()).join(".stamp").exists()
}

fn attach_args(case: &Case) -> Vec<String> {
    fs::read_to_string(case.store.work.join("attach.args"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn ten_megabytes_an_item_what_does_not_fit_is_left_out_what_still_fits_is_taken() {
    let c = case("big", "", true);
    let (a, b, d) = (
        format!("{F}/51/part1.html"),
        format!("{F}/52/part2.html"),
        format!("{F}/53/part3.html"),
    );
    net_sized(&a, 6 << 20);
    net_sized(&b, 6 << 20);
    net_sized(&d, 1 << 20);
    let (meta, line) = built(&c, 9, &refs(&[&a, &b, &d], Vec::new()));
    assert_eq!(meta.names("files"), ["part1.html", "part3.html"]);
    assert_eq!(
        meta.reason("part2.html"),
        "Over the 10 MB limit of a ticket."
    );
    assert_eq!(line, format!("#9: 2 files, {} bytes, 1 left out", 7 << 20));
    assert!(meta.bytes() <= LIMIT);
}

fn a_download_that_answers_more_than_it_promised_is_still_held_to_the_limit() {
    let c = case("unbounded", "", true);
    let (first, late) = (format!("{F}/61/first.html"), format!("{F}/62/late.html"));
    net_sized(&first, 9 << 20);
    net_sized(&late, 2 << 20);
    mark(&late, "unbounded", "");
    let (meta, _) = built(&c, 10, &refs(&[&first, &late], Vec::new()));
    assert_eq!(meta.names("files"), ["first.html"]);
    assert_eq!(
        meta.reason("late.html"),
        "Over the 10 MB limit of a ticket."
    );
}

fn a_name_that_would_leave_its_folder_is_cleaned_and_two_files_may_share_a_name() {
    let c = case("names", "", true);
    let evil = format!("{F}/71/..%2F..%2Fevil.html");
    let (one, two) = (format!("{F}/72/same.html"), format!("{F}/73/same.html"));
    net(&evil, b"<p>evil</p>");
    net(&one, b"<p>one</p>");
    net(&two, b"<p>two</p>");
    let (meta, _) = built(&c, 11, &refs(&[&evil, &one, &two], Vec::new()));
    assert_eq!(
        meta.names("files"),
        ["2F..-2Fevil.html", "same.html", "2-same.html"]
    );
    let files = c.store.work.join("11/files");
    assert_eq!(
        fs::read_to_string(files.join("2-same.html")).unwrap(),
        "<p>two</p>"
    );
    assert!(!c.store.work.join("evil.html").exists());
}

fn a_download_that_fails_for_now_is_tried_again_on_the_next_run() {
    let c = case("retry", "", true);
    let slow = format!("{F}/81/slow.html");
    net(&slow, b"<p>slow</p>");
    mark(&slow, "rc", "28");
    let (meta, _) = built(&c, 12, &refs(&[&slow], Vec::new()));
    assert_eq!(
        meta.reason("slow.html"),
        "Could not be fetched. It is tried again."
    );
    assert!(!stamped(&c, 12));
    unmark(&slow, "rc");
    let (meta, _) = built(&c, 12, &refs(&[&slow], Vec::new()));
    assert_eq!(meta.names("files"), ["slow.html"]);
    assert!(stamped(&c, 12));
}

fn an_items_meta_keeps_its_keys_in_the_order_it_always_had() {
    let c = case("meta", "", true);
    let (a, tar) = (format!("{F}/70/a.html"), format!("{F}/71/b.tar"));
    net(&a, b"<p>a</p>");
    built(&c, 66, &refs(&[&a, &tar], Vec::new()));
    let text = fs::read_to_string(c.store.work.join("66/.meta.json")).unwrap();
    // The item's time, which it does not carry here, is the one null.
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[5].ends_with("\": null,"), "{text}");
    lines
        .iter()
        .filter(|l| l.contains("null"))
        .for_each(|l| assert_eq!(*l, lines[5]));
    let rest: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 5)
        .map(|(_, l)| *l)
        .collect();
    assert_eq!(
        rest.join("\n"),
        r#"{
  "number": 66,
  "title": "Item",
  "url": "https://example.org/item/66",
  "author": "alice",
  "files": [
    {
      "name": "a.html",
      "kind": "app",
      "bytes": 8
    }
  ],
  "skipped": [
    {
      "name": "b.tar",
      "reason": "Not a type the host serves."
    }
  ],
  "outside": 0,
  "bytes": 8
}"#
    );
    assert!(text.ends_with("}\n"));
    // A type the host does not serve is never asked for.
    assert!(asked(&tar).is_empty());
}

fn a_pasted_image_is_named_by_the_type_it_answers_with() {
    let c = case("pasted", "", true);
    let shot = "https://github.com/user-attachments/assets/11111111-2222-3333-4444-000000000013";
    net(shot, b"\x89PNG");
    mark(shot, "type", "image/png");
    let (meta, _) = built(&c, 13, &refs(&[shot], Vec::new()));
    assert_eq!(meta.names("files"), ["image-1.png"]);
}

fn forty_files_an_item_the_one_after_is_left_out() {
    let c = case("count", "", true);
    let links: Vec<String> = (0..41).map(|i| format!("{F}/14{i}/f{i}.txt")).collect();
    for link in &links {
        net(link, b"x");
    }
    let found: Vec<&str> = links.iter().map(String::as_str).collect();
    let (meta, _) = built(&c, 14, &refs(&found, Vec::new()));
    assert_eq!(meta.names("files").len(), 40);
    assert_eq!(meta.reason("f40.txt"), "More than 40 files.");
}

fn entries(base: &str) -> String {
    json!({"api": 1, "ok": true, "verb": "unpack", "entries": [
        {"name": "index.html", "kind": "app", "bytes": 20},
        {"name": "app.js", "kind": "part", "bytes": 10},
        {"skip": "notes.pdf", "reason": "Not a type the host serves."},
        {"skip": format!("f{base}.txt"), "reason": "More than 3 files."},
    ]})
    .to_string()
}

fn an_attached_zip_is_unpacked_into_a_folder_with_what_is_left_of_the_limits() {
    let c = case("zip", "", true);
    let (first, good) = (format!("{F}/15/first.html"), format!("{F}/16/good.zip"));
    net(&first, b"<p>first</p>");
    net(&good, entries("45").as_bytes());
    let (meta, line) = built(&c, 15, &refs(&[&first, &good], Vec::new()));
    assert_eq!(
        meta.names("files"),
        ["first.html", "good/index.html", "good/app.js"]
    );
    assert_eq!(meta.reason("good/notes.pdf"), "Not a type the host serves.");
    // Its own count of files is said as the item's.
    assert_eq!(meta.reason("good/f45.txt"), "More than 40 files.");
    assert_eq!(line, "#15: 3 files, 42 bytes, 2 left out");
    // The zip has what the first file left of the 10 MB and the 40 files.
    assert_eq!(attach_args(&c), [format!("{} 39", LIMIT - 12)]);
    assert!(!c.store.work.join("15/.part").exists());
}

fn a_zip_the_host_cannot_read_or_cannot_unpack_is_left_out_with_its_reason() {
    let c = case("badzip", "", true);
    let fake = format!("{F}/17/fake.zip");
    net(&fake, b"BAD not a zip");
    let (meta, _) = built(&c, 17, &refs(&[&fake], Vec::new()));
    assert_eq!(meta.reason("fake.zip"), "Not a zip the host can read.");
    assert!(!c.store.work.join("17/files/fake").exists());

    let bare = case("noattach", "", false);
    let good = format!("{F}/18/good.zip");
    net(&good, entries("1").as_bytes());
    let (meta, _) = built(&bare, 18, &refs(&[&good], Vec::new()));
    assert_eq!(meta.reason("good.zip"), "This host cannot unpack a zip.");
}

fn sha256(bytes: &[u8]) -> String {
    let dir = TestDir::new("items-sum");
    fs::write(dir.join("f"), bytes).unwrap();
    attach::fetch::sha256_of(&dir.join("f")).unwrap()
}

fn a_zip_by_address_from_an_allowed_host_with_its_sha256_is_unpacked_as_an_attached_zip() {
    let c = case("byaddress", "files.example.org", true);
    let (first, good) = (format!("{F}/19/first.html"), format!("{X}/19/good.zip"));
    let bytes = entries("1");
    net(&first, b"<p>first</p>");
    net(&good, bytes.as_bytes());
    let (meta, _) = built(
        &c,
        19,
        &refs(&[&first], vec![zip(&good, &sha256(bytes.as_bytes()))]),
    );
    assert_eq!(
        meta.names("files"),
        ["first.html", "good/index.html", "good/app.js"]
    );
    // After the attached files: what is left of the 40 and the 10 MB.
    assert_eq!(attach_args(&c), [format!("{} 39", LIMIT - 12)]);
    // No redirect is left to curl.
    assert_eq!(asked(&good), [format!("{good} -sSf --path-as-is")]);
}

fn a_zip_by_address_of_another_sha256_is_refused_and_nothing_of_it_kept() {
    let c = case("othersum", "files.example.org", true);
    let good = format!("{X}/20/good.zip");
    net(&good, entries("1").as_bytes());
    let (meta, line) = built(&c, 20, &refs(&[], vec![zip(&good, &"0".repeat(64))]));
    assert_eq!(meta.reason("good.zip"), "Not the bytes its sha256 names.");
    assert_eq!(line, "#20: 0 files, 0 bytes, 1 left out");
    assert!(attach_args(&c).is_empty());
    assert!(!c.store.work.join("20/.part").exists());
}

fn a_zip_by_address_from_a_host_off_the_list_is_never_asked() {
    let sum = "a".repeat(64);
    for (n, hosts) in [(21, "files.example.org"), (22, "")] {
        let c = case("offlist", hosts, true);
        let other = format!("https://other.example.org/{n}/good.zip");
        let plain = format!("http://files.example.org/p/{n}/good.zip");
        let listed = format!("{X}/{n}/listed.zip");
        let zips = if hosts.is_empty() {
            vec![zip(&listed, &sum)]
        } else {
            vec![zip(&other, &sum), zip(&plain, &sum)]
        };
        let (meta, _) = built(&c, n, &refs(&[], zips));
        let skipped = meta.skipped();
        assert_eq!(skipped.len(), if hosts.is_empty() { 1 } else { 2 });
        for reason in skipped {
            assert_eq!(reason, "Not from a host the instance allows.");
        }
        assert!(asked(&other).is_empty() && asked(&plain).is_empty() && asked(&listed).is_empty());
    }
}

fn a_zip_by_address_follows_a_redirect_on_the_listed_hosts_and_nowhere_else() {
    let c = case("moved", "files.example.org", true);
    let bytes = entries("1");
    let sum = sha256(bytes.as_bytes());
    let (good, there) = (format!("{X}/23/good.zip"), format!("{X}/23/moved/good.zip"));
    let (away, evil) = (
        format!("{X}/23/away.zip"),
        "https://evil.example/23/good.zip".to_string(),
    );
    mark(&good, "moved", &there);
    net(&there, bytes.as_bytes());
    mark(&away, "moved", &evil);
    net(&evil, bytes.as_bytes());
    let (meta, _) = built(&c, 23, &refs(&[], vec![zip(&good, &sum), zip(&away, &sum)]));
    assert_eq!(meta.names("files"), ["good/index.html", "good/app.js"]);
    assert_eq!(
        meta.reason("away.zip"),
        "Sent on to an address the instance does not allow."
    );
    assert_eq!(asked(&there).len(), 1);
    assert!(asked(&evil).is_empty());
}

fn a_zip_by_address_that_fails_for_now_is_tried_again_on_the_next_run() {
    let c = case("ziplater", "files.example.org", true);
    let bytes = entries("1");
    let good = format!("{X}/24/good.zip");
    net(&good, bytes.as_bytes());
    mark(&good, "rc", "7");
    let found = refs(&[], vec![zip(&good, &sha256(bytes.as_bytes()))]);
    let (meta, _) = built(&c, 24, &found);
    assert_eq!(
        meta.reason("good.zip"),
        "Could not be fetched. It is tried again."
    );
    assert!(!stamped(&c, 24));
    unmark(&good, "rc");
    let (meta, _) = built(&c, 24, &found);
    assert_eq!(meta.names("files"), ["good/index.html", "good/app.js"]);
    assert!(stamped(&c, 24));
}

fn a_zip_line_of_another_form_is_left_out_and_said() {
    let c = case("notamark", "files.example.org", true);
    let (meta, _) = built(
        &c,
        25,
        &refs(
            &[],
            vec![Err(format!("{X}/25/good.zip")), Err(String::new())],
        ),
    );
    let not_a_mark = "Not a zip and its sha256: the line is Zip:, the address and sha256: with 64 lower-case hex digits.";
    assert_eq!(meta.reason("good.zip"), not_a_mark);
    assert_eq!(meta.reason("Zip:"), not_a_mark);
    assert!(asked(&format!("{X}/25/")).is_empty());
}

#[test]
fn the_store() {
    let _stubs = made();
    let cases: [fn(); 15] = [
        ten_megabytes_an_item_what_does_not_fit_is_left_out_what_still_fits_is_taken,
        a_download_that_answers_more_than_it_promised_is_still_held_to_the_limit,
        a_name_that_would_leave_its_folder_is_cleaned_and_two_files_may_share_a_name,
        a_download_that_fails_for_now_is_tried_again_on_the_next_run,
        an_items_meta_keeps_its_keys_in_the_order_it_always_had,
        a_pasted_image_is_named_by_the_type_it_answers_with,
        forty_files_an_item_the_one_after_is_left_out,
        an_attached_zip_is_unpacked_into_a_folder_with_what_is_left_of_the_limits,
        a_zip_the_host_cannot_read_or_cannot_unpack_is_left_out_with_its_reason,
        a_zip_by_address_from_an_allowed_host_with_its_sha256_is_unpacked_as_an_attached_zip,
        a_zip_by_address_of_another_sha256_is_refused_and_nothing_of_it_kept,
        a_zip_by_address_from_a_host_off_the_list_is_never_asked,
        a_zip_by_address_follows_a_redirect_on_the_listed_hosts_and_nowhere_else,
        a_zip_by_address_that_fails_for_now_is_tried_again_on_the_next_run,
        a_zip_line_of_another_form_is_left_out_and_said,
    ];
    for case in cases {
        case();
    }
}
