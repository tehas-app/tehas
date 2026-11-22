//! `tehas index` and the sandbox image the index was built with. The record
//! names the image that ran. An index written by another image, or by one
//! that recorded nothing, is removed and built from nothing; the same image
//! leaves what it wrote for the indexer to update. What the directory given
//! read-write held when the run started (`index.at.run`) tells them apart.
//! Each test is a case of the shell file it replaced, by the same name.

mod index;

use index::*;

#[test]
fn the_record_names_the_sandbox_image_that_built_the_index() {
    let h = Host::new("names-image");
    h.indexed();
    h.index_holds(&[("logs/", ""), ("other-tree.db", "old")]);
    h.moved("recorded");
    let r = h.index(&[]);
    rc(&r, 0);
    assert_eq!(h.field("indexer_image"), IMAGE_ID);
    lacks(&r.out, "from scratch");
    assert_eq!(h.noted("index.at.run"), "logs other-tree.db ");
}

/// A host indexed by the default image, then again by the other one over an
/// index directory with what that image did not write.
fn other_image(name: &str) -> (Host, Run) {
    let h = Host::new(name);
    h.indexed();
    h.index_holds(&[
        ("logs/", ""),
        ("_config.db", "cfg"),
        ("other-tree.db", "old"),
        ("widget.db.stage.abc123.tmp.945328.0x200002bc000", "left"),
    ]);
    h.set_env(&[&format!("IMAGE_ID={OTHER_ID}")]);
    let r = h.index(&[]);
    rc(&r, 0);
    (h, r)
}

#[test]
fn an_index_built_by_another_sandbox_image_is_built_from_scratch() {
    let (h, r) = other_image("other-image");
    has(
        &r.out,
        &format!("the index was built by sandbox image {IMAGE_ID}, this run uses {OTHER_ID}: 2 file(s) it wrote are set aside and the index is built from scratch"),
    );
    assert_eq!(h.noted("index.at.run"), "_config.db logs ");
    assert_eq!(h.index_names(), "_config.db logs widget.db ");
    assert_eq!(h.field("indexer_image"), OTHER_ID);
    assert_eq!(h.tmp_count(), 0);
}

#[test]
fn the_same_sandbox_image_updates_the_index_it_built() {
    let (h, _) = other_image("same-image");
    h.reset();
    write(h.indexdir().join("other-tree.db"), "keep\n");
    h.moved("updated");
    let r = h.index(&[]);
    rc(&r, 0);
    lacks(&r.out, "sandbox image");
    assert_eq!(
        h.noted("index.at.run"),
        "_config.db logs other-tree.db widget.db "
    );
    assert_eq!(h.field("indexer_image"), OTHER_ID);
}

#[test]
fn before_a_cycle_a_moved_branch_is_built_from_scratch_when_the_image_changed() {
    let (h, _) = other_image("moved-image");
    h.reset();
    h.index_holds(&[("widget.db", "old")]);
    h.set_env(&[&format!("IMAGE_ID={IMAGE_ID}")]);
    h.moved("moved");
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    has(
        &r.out,
        &format!("the index was built by sandbox image {OTHER_ID}, this run uses {IMAGE_ID}"),
    );
    assert_eq!(h.noted("index.at.run"), "");
    assert_eq!(h.field("indexer_image"), IMAGE_ID);
}

#[test]
fn an_index_with_no_recorded_image_is_built_from_scratch() {
    let h = Host::new("unrecorded");
    h.indexed();
    h.index_holds(&[("widget.db", "old")]);
    h.record_image(None);
    h.moved("unrecorded");
    let r = h.index(&[]);
    rc(&r, 0);
    has(
        &r.out,
        &format!("the index was built by sandbox image unknown, this run uses {IMAGE_ID}"),
    );
    assert_eq!(h.noted("index.at.run"), "");
    assert_eq!(h.field("indexer_image"), IMAGE_ID);
}

#[test]
fn a_host_that_has_never_been_indexed_says_nothing_about_images() {
    let h = Host::new("never");
    let _ = std::fs::remove_dir_all(h.indexdir());
    assert!(!h.marker().exists());
    h.moved("fresh");
    let r = h.index(&[]);
    rc(&r, 0);
    lacks(&r.out, "sandbox image");
    assert_eq!(h.noted("index.at.run"), "");
    assert_eq!(h.field("indexer_image"), IMAGE_ID);
}

#[test]
fn what_the_indexer_wrote_for_itself_is_not_what_a_rebuild_removes() {
    let h = Host::new("own-files");
    h.indexed();
    h.index_holds(&[("logs/", ""), ("_config.db", "cfg")]);
    h.set_env(&[&format!("IMAGE_ID={OTHER_ID}")]);
    h.moved("nothing");
    let r = h.index(&[]);
    rc(&r, 0);
    lacks(&r.out, "sandbox image");
    assert_eq!(h.noted("index.at.run"), "_config.db logs ");
    assert_eq!(h.index_names(), "_config.db logs widget.db ");
}

/// A host whose record names the other image, with the calls forgotten and
/// the runs given the default image again.
fn recorded_other(name: &str) -> Host {
    let h = Host::new(name);
    h.set_env(&[&format!("IMAGE_ID={OTHER_ID}")]);
    h.indexed();
    h.set_env(&[]);
    assert_eq!(h.field("indexer_image"), OTHER_ID);
    h
}

// The record names the other image and the stub answers with the default
// one, so this run rebuilds. It is the dispatcher's own case: the rebuild is
// what the run that fails has to put back, because the cycles get whatever is
// in the index directory until a run succeeds.
#[test]
fn a_rebuild_that_fails_before_a_cycle_leaves_the_cycles_the_index_they_had() {
    let h = recorded_other("rebuild-fails-before");
    h.index_holds(&[
        ("logs/", ""),
        ("_config.db", "cfg"),
        ("other-tree.db", "old"),
    ]);
    let kept = h.record();
    h.moved("broken");
    h.set_env(&[&format!("IMAGE_ID={IMAGE_ID}"), "RUN_RC=1"]);
    let r = h.index(&["--before-cycle"]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("the index was built by sandbox image {OTHER_ID}, this run uses {IMAGE_ID}: 1 file(s) it wrote are set aside and the index is built from scratch"),
    );
    assert_eq!(h.noted("index.at.run"), "_config.db logs ");
    assert_eq!(h.failed_at(), h.tip());
    assert_eq!(h.record(), kept);
    assert_eq!(h.index_names(), "_config.db logs other-tree.db ");
    assert_eq!(h.index_file("other-tree.db"), "old");
    assert_eq!(h.tmp_count(), 0);
    let r = h.index(&["--before-cycle"]);
    rc(&r, 1);
    has(&r.out, "not tried again before the branch moves");
    assert_eq!(h.index_names(), "_config.db logs other-tree.db ");
}

#[test]
fn a_rebuild_that_fails_by_hand_puts_the_index_back_where_the_next_run_finds_it() {
    let h = recorded_other("rebuild-fails-by-hand");
    h.index_holds(&[
        ("logs/", ""),
        ("_config.db", "cfg"),
        ("other-tree.db", "old"),
    ]);
    h.set_env(&[&format!("IMAGE_ID={IMAGE_ID}"), "RUN_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(&r.out, "1 file(s) it wrote are set aside");
    assert_eq!(h.index_names(), "_config.db logs other-tree.db ");
    h.set_env(&[&format!("IMAGE_ID={IMAGE_ID}")]);
    let r = h.index(&[]);
    rc(&r, 0);
    assert_eq!(h.noted("index.at.run"), "_config.db logs ");
    assert_eq!(h.index_names(), "_config.db logs widget.db ");
    assert_eq!(h.field("indexer_image"), IMAGE_ID);
    assert_eq!(h.tmp_count(), 0);
}

#[test]
fn a_signal_during_a_rebuild_puts_the_index_back_and_removes_the_clone() {
    // The shell file's record named the image of the run; a record of the
    // other image is the rebuild the name says. Both end the same way.
    for recorded in [IMAGE_ID, OTHER_ID] {
        for (name, status) in [("TERM", 143), ("INT", 130), ("HUP", 129)] {
            let h = Host::new(&format!("signal-{name}"));
            h.indexed();
            h.record_image(Some(recorded));
            h.index_holds(&[("other-tree.db", "old")]);
            h.set_env(&[
                &format!("IMAGE_ID={IMAGE_ID}"),
                &format!("RUN_SIGNAL={name}"),
            ]);
            let r = h.index(&[]);
            rc(&r, status);
            assert_eq!(h.index_names(), "other-tree.db ", "{name} {recorded}");
            assert_eq!(h.tmp_count(), 0, "{name} {recorded}");
            // The client outlives the run and holds the lock until it ends.
            let lock = std::fs::File::open(h.index_lock()).unwrap();
            lock.lock().unwrap();
        }
    }
}

// The set-aside goes back only for a run that wrote no index. An indexer that
// wrote its database and then answered in a shape this command does not
// accept is the case: what it wrote is the index the host keeps, and putting
// the other image's files back over it would lose it and mix two images' data.
#[test]
fn a_rebuild_whose_indexer_wrote_a_database_keeps_it_and_drops_what_it_set_aside() {
    let h = Host::new("wrote-database");
    h.indexed();
    h.index_holds(&[
        ("logs/", ""),
        ("_config.db", "cfg"),
        ("other-tree.db", "old"),
    ]);
    h.record_image(Some(OTHER_ID));
    let kept = h.record();
    let tip = h.tip();
    h.set_env(&[r#"RUN_OUT={"error":"no such path"}"#]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!("the index was built by sandbox image {OTHER_ID}, this run uses {IMAGE_ID}: 1 file(s) it wrote are set aside and the index is built from scratch"),
    );
    has(
        &r.out,
        "this run wrote index data, what the other sandbox image wrote was dropped",
    );
    assert_eq!(h.index_names(), "_config.db logs widget.db ");
    assert_eq!(h.index_file("widget.db"), "index");
    assert_eq!(h.failed_at(), tip);
    assert_eq!(h.record(), kept);
    assert_eq!(h.tmp_count(), 0);
    // The database the rejected run wrote is what the cycles are given, and
    // before a cycle that is what the record says is the branch's tip, so
    // nothing runs.
    let r = h.index(&["--before-cycle"]);
    rc(&r, 0);
    has(&r.out, "already indexed");
    assert_eq!(h.index_file("widget.db"), "index");
}

// A run that was killed never ran its trap, so what it set aside waits in
// $TEHAS_HOME/tmp and the index directory holds nothing a cycle can search.
#[test]
fn what_a_killed_run_left_set_aside_goes_back_where_the_next_run_can_use_it() {
    let h = recorded_other("killed-back");
    h.index_holds(&[("logs/", "")]);
    h.orphan("other-tree.db", "old");
    let kept = h.record();
    h.set_env(&["RUN_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!(
            "a run that was killed left index data set aside: 1 file(s) put back in {}/state/index, 0 removed",
            h.th.display()
        ),
    );
    has(&r.out, "1 file(s) it wrote are set aside");
    assert_eq!(h.index_names(), "logs other-tree.db ");
    assert_eq!(h.index_file("other-tree.db"), "old");
    assert_eq!(h.record(), kept);
    assert_eq!(h.tmp_count(), 0);
}

#[test]
fn what_a_killed_run_left_set_aside_is_removed_when_the_index_holds_a_database() {
    let h = recorded_other("killed-removed");
    h.index_holds(&[("logs/", ""), ("widget.db", "keep")]);
    age(h.indexdir().join("widget.db"), 120);
    h.orphan("other-tree.db", "old");
    h.set_env(&["RUN_RC=1"]);
    let r = h.index(&[]);
    rc(&r, 1);
    has(
        &r.out,
        &format!(
            "a run that was killed left index data set aside: 0 file(s) put back in {}/state/index, 1 removed",
            h.th.display()
        ),
    );
    assert_eq!(h.index_names(), "logs widget.db ");
    assert_eq!(h.index_file("widget.db"), "keep");
    assert_eq!(h.tmp_count(), 0);
}
