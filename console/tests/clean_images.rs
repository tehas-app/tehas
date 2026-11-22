//! `tehasbot clean`, steps 4 and 5: the dated tags of the sandbox image past
//! the newest two builds, and the old `sha-` tags of the base's repository.
//! Every removal is one name the cleaner knows; nothing prunes images.

#[path = "clean/mod.rs"]
mod common;

use common::*;

// Step 4: six dated tags, none held. The newest two builds stay whatever
// their age; the ones before them go whatever theirs.

#[test]
fn a_dated_tag_before_the_newest_two_builds_no_container_holds_is_removed() {
    let h = Host::new("dated");
    h.old_tags();
    h.size("sha256:1", 3_000_000_000);
    let r = h.clean(&[]);
    rc(&r, 0);
    let d = h.docker();
    has(
        &d,
        &format!("image ls --no-trunc --format {{{{.Tag}}}} {{{{.ID}}}} {IMAGE}"),
    );
    has(&d, &format!("rmi {IMAGE}:261009-1000"));
    lacks(&d, &format!("rmi {IMAGE}:261014-1000"));
    lacks(&d, &format!("rmi {IMAGE}:261013-1000"));
    has(&d, &format!("rmi {IMAGE}:261012-1000"));
    has(
        &r.out,
        &format!("removed {IMAGE}:261009-1000: a sandbox image older than the newest 2 builds, 3000000000 bytes"),
    );
    lacks(&r.out, &format!("removed {IMAGE}:261014-1000"));
    // Only one image reports a size, and the total adds up what docker says.
    has(&r.out, "freed 3000000000 bytes");
    // The sizes of the four removals are one question.
    assert_eq!(h.docker_count("image inspect "), 1);
    has(
        &d,
        "image inspect --format {{.Id}} {{.Size}} {{.Created}} sha256:4 sha256:3 sha256:2 sha256:1",
    );
}

#[test]
fn the_newest_two_builds_stay_whatever_their_age() {
    let h = Host::new("newest");
    h.old_tags();
    h.created("sha256:6", &old_time());
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), &format!("rmi {IMAGE}:261014-1000"));
    has(
        &r.out,
        &format!("kept {IMAGE}:261014-1000: one of the newest 2 builds"),
    );
    has(
        &r.out,
        &format!("kept {IMAGE}:261013-1000: one of the newest 2 builds"),
    );
}

#[test]
fn a_tag_a_container_was_made_from_stays_an_old_one_though_it_is() {
    let h = Host::new("held-tag");
    h.old_tags();
    h.containers(&["c1 sha256:1"]);
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), &format!("rmi {IMAGE}:261009-1000"));
    has(
        &r.out,
        &format!("kept {IMAGE}:261009-1000: a container or a uid tag uses it"),
    );
}

#[test]
fn a_tag_a_uid_tag_points_at_stays() {
    let h = Host::new("uid-tag");
    h.old_tags();
    h.fx_line("images", "uid4242 sha256:1");
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), &format!("rmi {IMAGE}:261009-1000"));
    has(
        &r.out,
        &format!("kept {IMAGE}:261009-1000: a container or a uid tag uses it"),
    );
}

#[test]
fn an_image_younger_than_the_age_goes_all_the_same_once_two_builds_stand_after_it() {
    let h = Host::new("young-dated");
    h.old_tags();
    h.created("sha256:1", &young_time());
    h.rm_fx("created-sha256:2");
    let r = h.clean(&[]);
    rc(&r, 0);
    has(&h.docker(), &format!("rmi {IMAGE}:261009-1000"));
    has(&h.docker(), &format!("rmi {IMAGE}:261010-1000"));
    lacks(&r.out, "docker did not date its image");
}

#[test]
fn the_instances_age_does_not_hold_the_old_builds_back() {
    let h = Host::new("age-dated");
    h.old_tags();
    let year = docker_time(365 * 86400);
    for n in 1..=6 {
        h.created(&format!("sha256:{n}"), &year);
    }
    h.minimal(&["TEHAS_CLEAN_AFTER=96"]);
    rc(&h.clean(&[]), 0);
    assert_eq!(h.docker_count("rmi "), 4);
    lacks(&h.docker(), &format!("rmi {IMAGE}:261014-1000"));
}

#[test]
fn a_docker_that_lists_no_tags_removes_nothing() {
    let h = Host::new("no-tags");
    h.old_tags();
    h.set_env(&["IMAGE_LS_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("WARN: clean: docker gave no list of its {IMAGE} tags"),
    );
    assert_eq!(h.docker_count("rmi "), 0);
}

#[test]
fn a_docker_that_names_no_container_removes_nothing() {
    let h = Host::new("no-ps");
    h.old_tags();
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "PS_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        "WARN: clean: docker said nothing about the containers on the host",
    );
    assert_eq!(h.docker_count("rmi "), 0);
}

#[test]
fn a_docker_that_names_nothing_about_its_containers_removes_nothing() {
    // A container must be there, or the inspect is never asked.
    let h = Host::new("no-inspect");
    h.old_tags();
    h.containers(&["c1 sha256:1"]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "INSPECT_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        "WARN: clean: docker said nothing about the containers on the host",
    );
    assert_eq!(h.docker_count("rmi "), 0);
}

#[test]
fn a_tag_that_cannot_be_removed_is_a_warning_and_the_others_still_go() {
    let h = Host::new("rmi-fails");
    h.old_tags();
    h.set_env(&[&format!("RMI_FAILS={IMAGE}:261010-1000")]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("WARN: clean: could not remove {IMAGE}:261010-1000"),
    );
    has(&h.docker(), &format!("rmi {IMAGE}:261009-1000"));
    has(&h.docker(), &format!("rmi {IMAGE}:261011-1000"));
}

#[test]
fn a_builds_product_tag_is_a_dated_tag_like_any_other() {
    let h = Host::new("product");
    h.six_tags();
    h.fx_line("images", "261014-1000-product sha256:7");
    h.all_old();
    h.created("sha256:7", &old_time());
    h.no_containers();
    rc(&h.clean(&[]), 0);
    // A date and its -product are one build.
    lacks(&h.docker(), &format!("rmi {IMAGE}:261014-1000-product"));
    lacks(&h.docker(), &format!("rmi {IMAGE}:261013-1000"));
    has(&h.docker(), &format!("rmi {IMAGE}:261012-1000"));
}

// Step 5: the base.

#[test]
fn a_base_no_image_on_the_host_has_is_no_removal_and_no_sweep_of_the_untagged() {
    let h = Host::new("no-base");
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "image prune");
    lacks(&h.docker(), "rmi sha256:");
    has(
        &r.out,
        &format!("kept the base: no image of {BASE_REPO} is on the host"),
    );
}

#[test]
fn the_old_sha_tags_of_the_followed_repository_go_and_the_current_one_stays() {
    let h = Host::new("bases");
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    h.size(&sha_old1(), 4_000_000_000);
    h.size(&sha_old2(), 1_000_000_000);
    // The base followed now is young, and so is the running build's image.
    h.created(&sha_now(), &young_time());
    h.created("sha256:6", &young_time());
    h.minimal(&["TEHAS_CLEAN_AFTER=6"]);
    let r = h.clean(&[]);
    rc(&r, 0);
    let d = h.docker();
    has(
        &d,
        &format!(
            "image ls --no-trunc --format {{{{.Repository}}}} {{{{.Tag}}}} {{{{.ID}}}} {BASE_REPO}"
        ),
    );
    has(
        &d,
        &format!(
            "image ls --no-trunc --format {{{{.Repository}}}} {{{{.Tag}}}} {{{{.ID}}}} {IMAGE}"
        ),
    );
    has(&d, &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
    has(&d, &format!("rmi {BASE_REPO}:sha-cccccccccccc"));
    lacks(&d, &format!("rmi {BASE_REPO}:sha-aaaaaaaaaaaa"));
    lacks(&d, "image prune");
    has(
        &r.out,
        &format!(
            "removed {BASE_REPO}:sha-bbbbbbbbbbbb: a base image of that age, 4000000000 bytes"
        ),
    );
    has(
        &r.out,
        &format!(
            "removed {BASE_REPO}:sha-cccccccccccc: a base image of that age, 1000000000 bytes"
        ),
    );
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-aaaaaaaaaaaa: docker did not date its image past 6 hours"),
    );
    has(&r.out, "freed 5000000000 bytes");
    // Each removal is one tag of the followed repository.
    assert_eq!(h.docker_count("rmi "), 2);
    // The three bases are one question.
    assert_eq!(h.docker_count("image inspect "), 1);
    has(
        &d,
        &format!(
            "image inspect --format {{{{.Id}}}} {{{{.Size}}}} {{{{.Created}}}} {} {} {}",
            sha_old1(),
            sha_old2(),
            sha_now()
        ),
    );
}

#[test]
fn a_base_a_kept_sandbox_tag_was_built_on_stays_and_one_a_container_holds_stays() {
    let h = Host::new("base-kept");
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2(), &sha_now()]);
    // Two releases, each on its own base, and a container of the third. A
    // second container whose image is no base proves the inspect was asked.
    h.kept_release("0.10.1", &sha_old1());
    h.kept_release("1.0.0", &sha_old2());
    h.containers(&[&format!("c9 {}", sha_now()), "c1 sha256:6"]);
    h.created("sha256:6", &young_time());
    h.created(&sha_now(), &young_time());
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "rmi");
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-bbbbbbbbbbbb: a kept sandbox tag was built on it"),
    );
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-cccccccccccc: a kept sandbox tag was built on it"),
    );
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-aaaaaaaaaaaa: a container was made from it"),
    );

    // Both proofs gone, and the same two bases go.
    h.reset();
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2(), &sha_now()]);
    h.no_containers();
    rc(&h.clean(&[]), 0);
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-cccccccccccc"));

    // Two kept releases read out of one listing, one line each: both ids
    // are proofs.
    h.reset();
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2(), &sha_now()]);
    h.kept_release("0.10.1", &sha_old1());
    h.kept_release("1.0.0", &sha_old2());
    h.no_containers();
    h.created(&sha_now(), &young_time());
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "rmi");
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-bbbbbbbbbbbb: a kept sandbox tag was built on it"),
    );
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-cccccccccccc: a kept sandbox tag was built on it"),
    );

    // One release pruned: its base is the only one this step may take.
    h.reset();
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2(), &sha_now()]);
    h.kept_release("1.0.0", &sha_old2());
    h.no_containers();
    h.created(&sha_now(), &young_time());
    let r = h.clean(&[]);
    rc(&r, 0);
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
    lacks(&h.docker(), &format!("rmi {BASE_REPO}:sha-cccccccccccc"));
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-cccccccccccc: a kept sandbox tag was built on it"),
    );
}

#[test]
fn a_sha_tag_of_another_repository_is_never_named_to_docker() {
    // The listing is asked of the pinned repository, so the line of another
    // account answered for it is never seen.
    let h = Host::new("other-repo");
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    let other = id_of('e');
    h.fx_line(
        "base-images",
        &format!("ghcr.io/other-owner/pi-rukas sha-eeeeeeeeeeee {other}"),
    );
    h.created(&other, &old_time());
    h.no_containers();
    rc(&h.clean(&[]), 0);
    lacks(&h.docker(), "other-owner");
    lacks(
        &h.docker(),
        "rmi ghcr.io/other-owner/pi-rukas:sha-eeeeeeeeeeee",
    );
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
}

#[test]
fn no_step_prunes_the_images_every_removal_is_a_name_the_cleaner_knows() {
    let h = Host::new("no-prune");
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    h.no_containers();
    rc(&h.clean(&[]), 0);
    assert_eq!(count(&h.docker(), "image prune"), 0);
    assert_eq!(h.docker_count("rmi "), 2);
}

#[test]
fn a_base_younger_than_the_age_is_kept_and_docker_is_asked_to_remove_nothing() {
    let h = Host::new("base-young");
    let base1 = id_of('1');
    h.fx(
        "base-images",
        &format!("{BASE_REPO} sha-aaaaaaaaaaaa {base1}\n"),
    );
    h.created(&base1, &young_time());
    h.minimal(&["TEHAS_CLEAN_AFTER=6"]);
    let r = h.clean(&[]);
    rc(&r, 0);
    has(
        &h.docker(),
        &format!("image inspect --format {{{{.Id}}}} {{{{.Size}}}} {{{{.Created}}}} {base1}"),
    );
    lacks(&h.docker(), "rmi");
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-aaaaaaaaaaaa: docker did not date its image past 6 hours"),
    );

    // A base docker will not date is kept the same way.
    h.reset();
    h.fx(
        "base-images",
        &format!("{BASE_REPO} sha-aaaaaaaaaaaa {base1}\n"),
    );
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "rmi");
    has(
        &r.out,
        &format!("kept {BASE_REPO}:sha-aaaaaaaaaaaa: docker did not date its image past 24 hours"),
    );

    // Two bases, one young: only the old one is offered.
    h.reset();
    h.base_tags();
    h.old_base(&[&sha_old1()]);
    h.created(&sha_old2(), &young_time());
    rc(&h.clean(&[]), 0);
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
    lacks(&h.docker(), &format!("rmi {BASE_REPO}:sha-cccccccccccc"));
}

#[test]
fn a_tag_of_any_other_shape_is_no_base_and_is_never_named_to_docker() {
    let h = Host::new("base-shapes");
    let (base1, base2, base9) = (id_of('1'), id_of('2'), id_of('9'));
    h.fx(
        "base-images",
        &format!(
            "{BASE_REPO} preview-123 {base1}\n{BASE_REPO} uid1000 {base2}\n{BASE_REPO} latest {base9}\n{BASE_REPO} sha-zzzzzzzzzzzz {}\n",
            sha_old1()
        ),
    );
    h.old_base(&[&base1, &base2, &base9, &sha_old1()]);
    h.no_containers();
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "rmi");
    for tag in ["preview-123", "uid1000", "latest", "sha-zzzzzzzzzzzz"] {
        has(
            &r.out,
            &format!("kept {BASE_REPO}:{tag}: not a base tag of sha- and twelve characters"),
        );
    }
}

#[test]
fn a_docker_that_says_nothing_about_the_containers_keeps_the_base_and_is_a_warning() {
    let h = Host::new("base-ps");
    h.base_tags();
    h.old_base(&[&sha_old1()]);
    h.containers(&["c1 sha256:6"]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "PS_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    lacks(&h.docker(), "rmi");
    has(
        &r.out,
        "WARN: clean: docker said nothing about the containers on the host",
    );
}

#[test]
fn a_docker_that_lists_no_base_tags_removes_nothing_and_is_not_a_base_that_is_absent() {
    let h = Host::new("base-ls");
    h.base_tags();
    h.old_base(&[&sha_old1()]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "BASE_LS_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("WARN: clean: docker gave no list of its base tags of {BASE_REPO}"),
    );
    lacks(&r.out, &format!("no image of {BASE_REPO} is on the host"));
    lacks(&h.docker(), "image prune");
}

#[test]
fn a_docker_that_lists_the_bases_but_not_the_sandbox_image_removes_nothing() {
    // Without the sandbox image's tags there is no proof of what a kept
    // release stands on.
    let h = Host::new("held-ls");
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    h.no_containers();
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "HELD_LS_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("WARN: clean: docker gave no list of the tags of {IMAGE}"),
    );
    lacks(&h.docker(), &format!("rmi {BASE_REPO}"));
    lacks(&r.out, &format!("no image of {BASE_REPO} is on the host"));
}

#[test]
fn a_host_that_follows_no_commit_names_no_base_and_asks_docker_nothing_of_it() {
    let h = Host::new("no-pins");
    h.no_versions();
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(
        &h.docker(),
        &format!(
            "image ls --no-trunc --format {{{{.Repository}}}} {{{{.Tag}}}} {{{{.ID}}}} {BASE_REPO}"
        ),
    );
    lacks(&h.docker(), "image prune");
    has(
        &r.out,
        "WARN: clean: kept the base: the host's pins name no base to prune",
    );
}

#[test]
fn a_malformed_repository_line_names_no_base_and_is_a_warning() {
    let h = Host::new("bad-pins");
    h.versions(&format!(
        "PI_RUKAS_REPO=https://example.org/Up-Owner/pi-rukas.git\nPI_RUKAS_FOLLOW=commit:{}\n",
        "a".repeat(40)
    ));
    let r = h.clean(&[]);
    rc(&r, 0);
    lacks(&h.docker(), "image ls --no-trunc --format {{.Repository}}");
    has(
        &r.out,
        "WARN: clean: kept the base: the host's pins name no base to prune",
    );
}

#[test]
fn a_base_id_docker_names_that_is_not_an_id_removes_nothing_and_is_a_warning() {
    let h = Host::new("not-an-id");
    h.fx(
        "base-images",
        &format!(
            "{BASE_REPO} sha-bbbbbbbbbbbb {}\n{BASE_REPO} sha-cccccccccccc not-an-id\n",
            sha_old1()
        ),
    );
    h.old_base(&[&sha_old1()]);
    let r = h.clean(&[]);
    rc(&r, 1);
    lacks(&h.docker(), "rmi ");
    has(
        &r.out,
        &format!(
            "WARN: clean: docker named not-an-id as an image of {BASE_REPO}, which is not an id"
        ),
    );
}

#[test]
fn a_base_pinned_by_its_digest_lists_the_tags_of_its_repository_all_the_same() {
    let h = Host::new("digest");
    h.versions(&format!(
        "PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git\nPI_RUKAS_IMAGE_DIGEST={}\n",
        id_of('f')
    ));
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    h.no_containers();
    h.clean(&[]);
    let d = h.docker();
    has(
        &d,
        &format!(
            "image ls --no-trunc --format {{{{.Repository}}}} {{{{.Tag}}}} {{{{.ID}}}} {BASE_REPO}"
        ),
    );
    has(
        &d,
        &format!(
            "image ls --no-trunc --format {{{{.Repository}}}} {{{{.Tag}}}} {{{{.ID}}}} {IMAGE}"
        ),
    );
    has(&d, &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
    has(&d, &format!("rmi {BASE_REPO}:sha-cccccccccccc"));
}

#[test]
fn a_base_that_cannot_be_removed_is_a_warning_naming_dockers_reason_the_exit_is_1_and_the_other_steps_ran(
) {
    let h = Host::new("base-refused");
    h.workspace(7);
    write(h.tmp("/dl/tool.tar.gz"), "old\n");
    age(&[h.tmp("/dl/tool.tar.gz")]);
    h.base_tags();
    h.old_base(&[&sha_old1(), &sha_old2()]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "PRUNE_RC=1"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!(
            "WARN: clean: could not remove {BASE_REPO}:sha-bbbbbbbbbbbb: docker: prune refused"
        ),
    );
    gone(h.ws("-7/target"));
    gone(h.tmp("/dl/tool.tar.gz"));
    // One base docker will not take does not hold the other back.
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-cccccccccccc"));

    h.set_env(&[
        &format!("BASE_NAME={BASE_REPO}"),
        &format!("RMI_FAILS={BASE_REPO}:sha-cccccccccccc"),
    ]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("WARN: clean: could not remove {BASE_REPO}:sha-cccccccccccc"),
    );
    has(&h.docker(), &format!("rmi {BASE_REPO}:sha-bbbbbbbbbbbb"));
}

#[test]
fn a_base_removal_that_does_not_finish_in_time_is_a_warning_naming_its_name() {
    let h = Host::new("base-timeout");
    h.base_tags();
    h.old_base(&[&sha_old1()]);
    h.set_env(&[&format!("BASE_NAME={BASE_REPO}"), "PRUNE_RC=124"]);
    let r = h.clean(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!(
            "WARN: clean: could not remove {BASE_REPO}:sha-bbbbbbbbbbbb: docker: prune refused"
        ),
    );
}
