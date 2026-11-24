//! `project_of`: the project a git remote of GitHub names, with no request,
//! and None for every other remote.

use tehas_github::project_of;

#[test]
fn each_form_of_a_github_remote_names_its_project() {
    for remote in [
        "https://github.com/acme/widget",
        "https://github.com/acme/widget.git",
        "https://github.com/acme/widget/",
        "https://github.com/acme/widget.git/",
        "HTTPS://GitHub.com/acme/widget.GIT",
        "git@github.com:acme/widget.git",
        "git@github.com:acme/widget",
        "ssh://git@github.com/acme/widget.git",
        "  https://github.com/acme/widget\n",
    ] {
        assert_eq!(
            project_of(remote).as_deref(),
            Some("acme/widget"),
            "{remote}"
        );
    }
    assert_eq!(
        project_of("https://github.com/up-owner/pi-rukas.git").as_deref(),
        Some("up-owner/pi-rukas")
    );
    assert_eq!(
        project_of("git@github.com:Acme/my.lib_2").as_deref(),
        Some("Acme/my.lib_2")
    );
}

#[test]
fn any_other_remote_names_none() {
    for remote in [
        "",
        "../origin.git",
        "/srv/git/widget.git",
        "file:///srv/git/widget.git",
        "https://gitlab.com/acme/widget.git",
        "https://github.com.evil.example/acme/widget",
        "https://github.com:8443/acme/widget",
        "http://github.com/acme/widget",
        "https://x-access-token:secret@github.com/acme/widget.git",
        "https://github.com/acme",
        "https://github.com/acme/widget/extra",
        "https://github.com/acme/..",
        "https://github.com/-acme/widget",
        "https://github.com/acme/widget.git.git",
        "git@github.com:acme/widget/extra.git",
        "ssh://git@github.com:22/acme/widget.git",
        "git@gitlab.com:acme/widget.git",
    ] {
        assert_eq!(project_of(remote), None, "{remote}");
    }
}
