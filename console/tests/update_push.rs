//! The proof a real `tehas update` makes that the app's cycle token can push:
//! a dry-run push from the product checkout with the token tehasbot makes,
//! and a factory that stays drained when it cannot. tehasbot and the push
//! itself are stubs; every other git call runs for real.

mod common;

use common::{git, has, lacks, which, Host};
use std::fs;
use std::path::PathBuf;
use tehas_core::testing::write_exe;

const DRAIN: &str = r#"{"v":1,"cycles":0,"plan":false,"until":null,"reason":"draining"}"#;
const REFUSED: &str = "remote: Write access to repository not granted.
fatal: unable to access 'https://github.com/acme/widget.git/': The requested URL returned error: 403";

struct Push {
    h: Host,
    product: PathBuf,
    limit: PathBuf,
}

impl Push {
    /// A host with the product checkout and tehasbot, drained or running.
    fn new(drained: bool) -> Push {
        let mut h = Host::new("push");
        // git, real but for the proof's push: that one records the token it
        // was given and is refused while $T/push-refused holds what the remote
        // says.
        h.stub(
            "git",
            &format!(
                r#"case "$*" in
  *'push --dry-run origin HEAD:refs/heads/tehas-preflight')
    printf '%s\n' "${{GH_TOKEN:-}}" >"$T/push-token"
    if [ -e "$T/push-refused" ]; then cat "$T/push-refused" >&2; exit 128; fi
    echo 'To https://github.com/acme/widget.git'; exit 0 ;;
esac
exec '{}' "$@""#,
                which("git")
            ),
        );
        h.instance();
        h.platform(0, "");
        let bin = h.home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        write_exe(
            bin.join("tehasbot"),
            format!(
                r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>'{t}/calls/tehasbot'
case "$*" in
  'github-token --login') echo 'acme-factory[bot]' ;;
  'github-token --cycle')
    if [ -e '{t}/no-token' ]; then cat '{t}/no-token' >&2; exit 1; fi
    echo ghs_cycle ;;
  *) exit 64 ;;
esac
"#,
                t = h.t().display()
            ),
        )
        .unwrap();
        let product = h.home.join("repos/widget");
        let p = product.display().to_string();
        git(None, &["init", "-q", "--initial-branch=main", &p]);
        git(
            Some(&product),
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "first",
            ],
        );
        let origin = h.join("product-origin").display().to_string();
        git(Some(&product), &["remote", "add", "origin", &origin]);
        let limit = h.home.join("state/limit.json");
        if drained {
            fs::create_dir_all(limit.parent().unwrap()).unwrap();
            fs::write(&limit, format!("{DRAIN}\n")).unwrap();
        }
        Push { h, product, limit }
    }

    fn refuse(&self) {
        fs::write(self.h.join("push-refused"), format!("{REFUSED}\n")).unwrap();
    }

    fn pushed(&self) -> bool {
        self.h.join("push-token").exists()
    }
}

#[test]
fn an_update_proves_the_cycle_token_pushes_then_says_the_drained_factory_is_ready_to_clear() {
    let p = Push::new(true);
    let r = p.h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    has(
        &r.out,
        "Cycle token:  OK (acme-factory[bot] can push to origin)",
    );
    assert_eq!(
        r.last(),
        "the factory is drained: tehas limit clear starts the factory, and this version has run no cycle yet"
    );
    assert_eq!(
        fs::read_to_string(p.h.join("push-token")).unwrap(),
        "ghs_cycle\n"
    );
    assert_eq!(
        p.h.calls("tehasbot"),
        "github-token --login\ngithub-token --cycle"
    );
    has(
        &p.h.calls("git"),
        &format!("-C {} -c credential.helper= -c", p.product.display()),
    );
    lacks(&p.h.calls("git"), "ghs_cycle");
}

#[test]
fn a_cycle_token_the_remote_refuses_fails_the_update_names_the_account_and_gits_words_and_the_factory_stays_drained(
) {
    let p = Push::new(true);
    p.refuse();
    let r = p.h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "Cycle token:  CANNOT PUSH (acme-factory[bot] was refused a dry-run push to origin)",
    );
    has(
        &r.out,
        "    remote: Write access to repository not granted.",
    );
    has(&r.out, "The requested URL returned error: 403");
    has(&r.out, "Install the app on acme/widget");
    assert_eq!(
        r.last(),
        "the factory stays drained: every cycle would stop at the sandbox's push check; fix the app, run tehas update --check, then tehas limit clear"
    );
    lacks(&r.out, "the factory is drained:");
    assert_eq!(fs::read_to_string(&p.limit).unwrap(), format!("{DRAIN}\n"));
    // What install.sh installed stays installed: the proof comes after it.
    assert_eq!(p.h.calls("install"), "install ");
}

#[test]
fn a_refused_cycle_token_on_a_running_factory_fails_the_update_and_drains_nothing() {
    let p = Push::new(false);
    p.refuse();
    let r = p.h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(&r.out, "Cycle token:  CANNOT PUSH");
    has(
        &r.out,
        "the factory is not drained: each start stops at the sandbox's push check until the token can push; tehas drain holds it",
    );
    assert!(!p.limit.exists());
}

#[test]
fn no_cycle_token_from_the_app_fails_the_update_with_tehasbots_words_and_pushes_nothing() {
    let p = Push::new(true);
    fs::write(
        p.h.join("no-token"),
        "tehas: GitHub's answer for the app's token held no token and expiry\n",
    )
    .unwrap();
    let r = p.h.update(&[]);
    assert_eq!(r.rc, 1, "{}", r.out);
    has(
        &r.out,
        "Cycle token:  CANNOT PUSH (the app made no cycle token)",
    );
    has(
        &r.out,
        "    tehas: GitHub's answer for the app's token held no token and expiry",
    );
    assert!(!p.pushed());
    has(&r.out, "the factory stays drained:");
}

#[test]
fn a_host_with_no_product_checkout_proves_nothing_and_asks_tehasbot_nothing() {
    let p = Push::new(true);
    fs::remove_dir_all(&p.product).unwrap();
    let r = p.h.update(&[]);
    assert_eq!(r.rc, 0, "{}", r.out);
    lacks(&r.out, "Cycle token:");
    assert_eq!(p.h.calls("tehasbot"), "");
}

#[test]
fn check_and_pending_push_nothing_themselves_the_checks_report_is_install_shs() {
    let p = Push::new(false);
    p.refuse();
    let r = p.h.update(&["--check"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    let r = p.h.update(&["--pending"]);
    assert_eq!(r.rc, 0, "{}", r.out);
    assert!(!p.pushed());
}
