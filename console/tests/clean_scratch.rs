//! `tehasbot clean`: the host's scratch (step 3), the build output of a
//! manual session's clone (step 7), and the dry run.

#[path = "clean/mod.rs"]
mod common;

use common::*;
use std::os::unix::fs::symlink;

#[test]
fn the_host_scratch_of_that_age_goes_and_nothing_else_under_tmp() {
    let h = Host::new("scratch");
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    write(h.tmp("/node-compile-cache/blob"), "old\n");
    write(h.tmp("/other/record.json"), "keep\n");
    write(h.tmp("/tehas-test.OLD/x"), "old\n");
    age(&[
        h.tmp("/dl/tool.tar.gz"),
        h.tmp("/node-compile-cache/blob"),
        h.tmp("/other/record.json"),
        h.tmp("/tehas-test.OLD"),
        h.tmp("/tehas-test.OLD/x"),
    ]);
    rc(&h.clean(&[]), 0);
    gone(h.tmp("/dl/tool.tar.gz"));
    gone(h.tmp("/node-compile-cache/blob"));
    gone(h.tmp("/tehas-test.OLD"));
    there(h.tmp("/other/record.json"));
}

#[test]
fn an_analytics_facts_file_of_that_age_goes_a_young_one_and_other_names_stay() {
    // A call's facts file outlives it only when the caller was killed.
    let h = Host::new("facts");
    write(h.tmp("/analytics-facts.OLD"), "old\n");
    write(h.tmp("/analytics-facts.NEW"), "new\n");
    write(h.tmp("/approval.OLD"), "old\n");
    mkdir(h.tmp("/analytics-facts.DIR"));
    symlink(h.t("/outside-facts"), h.tmp("/analytics-facts.LINK")).unwrap();
    write(h.t("/outside-facts"), "stay\n");
    age(&[
        h.tmp("/analytics-facts.OLD"),
        h.tmp("/approval.OLD"),
        h.tmp("/analytics-facts.DIR"),
        h.t("/outside-facts"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.tmp("/analytics-facts.OLD"));
    there(h.tmp("/analytics-facts.NEW"));
    there(h.tmp("/approval.OLD"));
    dir(h.tmp("/analytics-facts.DIR"));
    link(h.tmp("/analytics-facts.LINK"));
    there(h.t("/outside-facts"));
    has(
        &r.out,
        &format!("removed {}: host scratch", h.tmp("/analytics-facts.OLD")),
    );
}

#[test]
fn a_health_report_of_that_age_goes_a_young_one_a_directory_and_a_link_stay() {
    // The report survives only when the poll's bound killed the publish.
    let h = Host::new("health");
    write(h.tmp("/health.OLD"), "old\n");
    write(h.tmp("/health.NEW"), "new\n");
    mkdir(h.tmp("/health.DIR"));
    symlink(h.t("/outside-report"), h.tmp("/health.LINK")).unwrap();
    write(h.t("/outside-report"), "stay\n");
    age(&[
        h.tmp("/health.OLD"),
        h.tmp("/health.DIR"),
        h.t("/outside-report"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(h.tmp("/health.OLD"));
    there(h.tmp("/health.NEW"));
    dir(h.tmp("/health.DIR"));
    link(h.tmp("/health.LINK"));
    there(h.t("/outside-report"));
    has(
        &r.out,
        &format!("removed {}: host scratch", h.tmp("/health.OLD")),
    );
}

#[test]
fn young_host_scratch_stays() {
    let h = Host::new("scratch-young");
    write(h.tmp("/dl/tool.tar.gz"), "new\n");
    write(h.tmp("/dl/old.tar.gz"), "old\n");
    age(&[h.tmp("/dl/old.tar.gz")]);
    rc(&h.clean(&[]), 0);
    there(h.tmp("/dl/tool.tar.gz"));
    gone(h.tmp("/dl/old.tar.gz"));
}

#[test]
fn a_staged_sandbox_recipe_of_that_age_goes_a_young_one_and_other_names_of_the_directory_stay() {
    // Only the two names the build writes are taken: its copy of the recipe
    // and that copy's temporary of six characters.
    let h = Host::new("recipe");
    let sb = h.th("/sandbox");
    for f in [
        "sandbox-uid.Dockerfile",
        "sandbox-uid.Dockerfile.abcdef",
        "sandbox-uid.Dockerfile.toolong",
        "sandbox-uid.Dockerfilex",
    ] {
        write(format!("{sb}/{f}"), "old\n");
    }
    mkdir(format!("{sb}/sandbox-uid.Dockerfile.DIR"));
    symlink(
        h.t("/outside-recipe"),
        format!("{sb}/sandbox-uid.Dockerfile.link0"),
    )
    .unwrap();
    write(h.t("/outside-recipe"), "stay\n");
    write(format!("{sb}/Dockerfile"), "keep\n");
    write(format!("{sb}/build.sh"), "keep\n");
    age(&[
        format!("{sb}/sandbox-uid.Dockerfile"),
        format!("{sb}/sandbox-uid.Dockerfile.abcdef"),
        format!("{sb}/sandbox-uid.Dockerfile.toolong"),
        format!("{sb}/sandbox-uid.Dockerfilex"),
        format!("{sb}/sandbox-uid.Dockerfile.DIR"),
        format!("{sb}/Dockerfile"),
        format!("{sb}/build.sh"),
        h.t("/outside-recipe"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{sb}/sandbox-uid.Dockerfile"));
    gone(format!("{sb}/sandbox-uid.Dockerfile.abcdef"));
    there(format!("{sb}/sandbox-uid.Dockerfile.toolong"));
    there(format!("{sb}/sandbox-uid.Dockerfilex"));
    dir(format!("{sb}/sandbox-uid.Dockerfile.DIR"));
    link(format!("{sb}/sandbox-uid.Dockerfile.link0"));
    there(h.t("/outside-recipe"));
    assert_eq!(
        std::fs::read_to_string(format!("{sb}/Dockerfile")).unwrap(),
        "keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(format!("{sb}/build.sh")).unwrap(),
        "keep\n"
    );
    has(
        &r.out,
        &format!("removed {sb}/sandbox-uid.Dockerfile: host scratch"),
    );
}

#[test]
fn a_young_staged_sandbox_recipe_stays_as_a_young_file_of_tmp_does() {
    let h = Host::new("recipe-young");
    let sb = h.th("/sandbox");
    write(format!("{sb}/sandbox-uid.Dockerfile"), "new\n");
    write(format!("{sb}/sandbox-uid.Dockerfile.abcdef"), "old\n");
    age(&[format!("{sb}/sandbox-uid.Dockerfile.abcdef")]);
    rc(&h.clean(&[]), 0);
    there(format!("{sb}/sandbox-uid.Dockerfile"));
    gone(format!("{sb}/sandbox-uid.Dockerfile.abcdef"));
}

// Step 7: a manual session's clone under dev.

#[test]
fn a_sessions_kept_build_output_is_not_walked_for_build_output_names() {
    // The walk of a workspace never enters dev, and a directory there is no
    // build output by its name alone.
    let h = Host::new("dev-walk");
    h.workspace(7);
    let devs = h.th("/dev");
    write(
        format!("{devs}/alice.state/next_modules/debug/console"),
        "big\n",
    );
    write(format!("{devs}/alice.state/memory/learned"), "m\n");
    age(&[
        devs.clone(),
        format!("{devs}/alice.state"),
        format!("{devs}/alice.state/next_modules"),
        format!("{devs}/alice.state/next_modules/debug"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{devs}/alice.state/next_modules/debug/console"));
    there(format!("{devs}/alice.state/memory/learned"));
    lacks(&r.out, &format!("removed {devs}/alice.state"));
    lacks(&r.out, &format!("would remove {devs}/alice.state"));
}

#[test]
fn low_on_disk_a_clones_build_output_goes_and_the_rest_of_its_state_stays() {
    let h = Host::new("dev-low");
    h.workspace(7);
    let devs = h.th("/dev");
    h.clone_state("alice");
    // A clone with no state beside it, and one whose state is a link: no
    // build output, and what a link points at is not walked.
    mkdir(format!("{devs}/bob"));
    write(h.t("/outside/build/lib"), "stay\n");
    symlink(h.t("/outside"), format!("{devs}/carol.state")).unwrap();
    // No container is named for a session of these clones.
    h.fx("containers", "");
    h.df_free(1);
    let r = h.clean(&[]);
    rc(&r, 0);
    gone(format!("{devs}/alice.state/target"));
    for k in ["memory/learned", "sessions/one.jsonl", "cache/index.db"] {
        there(format!("{devs}/alice.state/{k}"));
    }
    there(format!("{devs}/alice.author"));
    there(format!("{devs}/bob"));
    link(format!("{devs}/carol.state"));
    there(h.t("/outside/build/lib"));
    has(
        &r.out,
        &format!("removed {devs}/alice.state/target: the build output of clone alice"),
    );
    has(&r.out, "freed");
    assert_eq!(count(&h.log(), "the build output of clone alice"), 1);
}

#[test]
fn above_the_minimum_free_a_clone_keeps_its_build_of_any_age_and_the_size_is_logged() {
    let h = Host::new("dev-room");
    h.workspace(7);
    let devs = h.th("/dev");
    h.clone_state("alice");
    h.fx("containers", "");
    h.df_reset();
    for words in [&[][..], &["--dry-run"][..]] {
        let r = h.clean(words);
        rc(&r, 0);
        there(format!("{devs}/alice.state/target/debug/console"));
        has(
            &r.out,
            "kept the build output of clone alice: the disk is not low, ",
        );
        lacks(&r.out, &format!("would remove {devs}/alice.state/target"));
    }
}

#[test]
fn low_on_disk_a_young_build_output_stays() {
    let h = Host::new("dev-young");
    h.workspace(7);
    let devs = h.th("/dev");
    mkdir(format!("{devs}/alice.state/target/debug"));
    h.fx("containers", "");
    h.df_free(1);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{devs}/alice.state/target"));
    has(
        &r.out,
        "kept the build output of clone alice: its target was written within 24 hours, ",
    );
}

#[test]
fn a_clone_whose_session_is_open_keeps_its_build_output_and_the_line_says_why() {
    let h = Host::new("dev-open");
    h.workspace(7);
    let devs = h.th("/dev");
    write(format!("{devs}/alice.state/target/debug/console"), "big\n");
    age(&[
        devs.clone(),
        format!("{devs}/alice.state"),
        format!("{devs}/alice.state/target"),
        format!("{devs}/alice.state/target/debug"),
    ]);
    let _held = hold(h.dev_lock("alice"));
    h.fx("containers", "");
    h.df_free(1);
    let r = h.clean(&[]);
    rc(&r, 0);
    there(format!("{devs}/alice.state/target/debug/console"));
    has(
        &r.out,
        "kept the build output of clone alice: its session is open",
    );
}

#[test]
fn the_containers_of_the_host_are_listed_once_and_a_docker_that_will_not_answer_keeps_every_build()
{
    let h = Host::new("dev-ps");
    h.workspace(7);
    let devs = h.th("/dev");
    for one in ["alice", "bob"] {
        write(format!("{devs}/{one}.state/target/debug/console"), "big\n");
    }
    age(&[
        devs.clone(),
        format!("{devs}/alice.state"),
        format!("{devs}/alice.state/target"),
        format!("{devs}/alice.state/target/debug"),
        format!("{devs}/bob.state"),
        format!("{devs}/bob.state/target"),
        format!("{devs}/bob.state/target/debug"),
    ]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "PS_RC=1"]);
    h.df_free(1);
    let r = h.clean(&[]);
    there(format!("{devs}/alice.state/target"));
    there(format!("{devs}/bob.state/target"));
    has(
        &r.out,
        "docker gave no answer for the session of clone alice",
    );
    has(&r.out, "docker gave no answer for the session of clone bob");
    let asked = h
        .docker()
        .lines()
        .filter(|l| *l == "ps --all --format {{.Names}}")
        .count();
    assert_eq!(asked, 1, "{}", h.docker());
}

#[test]
fn the_dry_run_names_a_clones_build_output_with_its_bytes_and_removes_none_of_it() {
    let h = Host::new("dev-dry");
    h.workspace(7);
    let devs = h.th("/dev");
    write(
        format!("{devs}/alice.state/target/debug/console"),
        &"x".repeat(4096),
    );
    age(&[
        devs.clone(),
        format!("{devs}/alice.state"),
        format!("{devs}/alice.state/target"),
        format!("{devs}/alice.state/target/debug"),
    ]);
    h.fx("containers", "");
    h.df_free(1);
    let r = h.clean(&["--dry-run"]);
    rc(&r, 0);
    there(format!("{devs}/alice.state/target/debug/console"));
    has(
        &r.out,
        &format!("would remove {devs}/alice.state/target: the build output of clone alice, "),
    );
    has(&r.out, "clean: would free ");
    lacks(&r.out, &format!("clean: removed {devs}/alice.state/target"));
}

#[test]
fn dry_run_removes_nothing_and_logs_what_it_would_remove() {
    let h = Host::new("dry");
    h.workspace(7);
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    age(&[h.tmp("/dl/tool.tar.gz")]);
    h.six_tags();
    h.all_old();
    h.no_containers();
    let base1 = id_of('1');
    h.fx(
        "base-images",
        &format!("{BASE_REPO} sha-aaaaaaaaaaaa {base1}\n"),
    );
    h.old_base(&[&base1]);
    let r = h.clean(&["--dry-run"]);
    there(h.ws("-7/target"));
    there(h.ws("-7/node_modules"));
    there(h.ws("-7.state/cache"));
    there(h.tmp("/dl/tool.tar.gz"));
    let w = h.wd();
    has(
        &r.out,
        &format!("would remove {w}-7/target: build output of #7"),
    );
    has(
        &r.out,
        &format!("would remove {}: host scratch", h.tmp("/dl/tool.tar.gz")),
    );
    has(
        &r.out,
        &format!(
            "would remove {IMAGE}:261009-1000: a sandbox image older than the newest 2 builds"
        ),
    );
    has(
        &r.out,
        &format!("would remove {BASE_IMAGE}: a base image of that age"),
    );
    lacks(&r.out, "image prune");
    lacks(&r.out, &format!("rmi {IMAGE}"));
    // The figure is of the file system's paths, and says the docker and store
    // steps are not in it.
    has(
        &r.out,
        &format!("would remove {w}-7/target: build output of #7, untouched for 24 hours, "),
    );
    has(&r.out, "clean: would free ");
    lacks(&r.out, "freed 0 bytes");
    lacks(&r.out, "clean: removed");
    lacks(&h.docker(), "builder prune");
    has(
        &r.out,
        "would prune docker's build cache of 24 hours and older, whose size is not known beforehand",
    );
}
