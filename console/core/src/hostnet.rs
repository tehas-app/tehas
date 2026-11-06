//! Whether a Docker network keeps the host out, for a container that runs
//! code nobody vetted: a preview's pull request (`console preview network`)
//! and a sandbox container with TEHAS_EGRESS=1 (#417). What does that is
//! checked, not a name: the network is internal (no way out), a bridge (the
//! option below means nothing to another driver), without IPv6 (the option is
//! about IPv4 only, and with IPv6 the host has an address of that kind on the
//! bridge), and made with gateway_mode_ipv4 "isolated" (the host has no IPv4
//! address on the bridge) by a Docker that knows that option. Docker before
//! 27.1 does not know it. What is known: the bridge driver of Docker 29.1.3
//! was seen to accept an option it does not know and to show it when asked.
//! That an older Docker takes this one and ignores it was not tried. The
//! version is checked so that it does not matter, and an old Docker is not
//! asked to make anything.
//!
//! `docker` answers each call with its standard output, or None on failure.

pub const OPTION: &str = "com.docker.network.bridge.gateway_mode_ipv4";

/// Docker's major and minor, from what it says of its server. Anything but
/// `<1-4 digits>.<1-4 digits>` and a suffix of `.+~-` and what follows is none.
pub fn version(said: &str) -> Option<(u32, u32)> {
    let digits = |text: &str| {
        let end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        (1..=4).contains(&end).then_some(end)
    };
    let (major, rest) = said.split_once('.')?;
    if digits(major)? != major.len() {
        return None;
    }
    let (minor, suffix) = rest.split_at(digits(rest)?);
    let plain = |c: char| c.is_ascii_alphanumeric() || ".+~-".contains(c);
    if !suffix.is_empty()
        && (!suffix.starts_with(['.', '+', '~', '-']) || !suffix.chars().all(plain))
    {
        return None;
    }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Why this Docker cannot keep the host out: older than 27.1, or no version
/// that can be read. `docker` answers with standard output, or None on failure.
pub fn docker_fault(docker: &mut impl FnMut(&[&str]) -> Option<String>) -> Option<String> {
    let said = docker(&["version", "--format", "{{.Server.Version}}"]).unwrap_or_default();
    let said = said.trim_end_matches('\n');
    match version(said) {
        None => Some("Docker's version could not be read".to_string()),
        Some(seen) if seen < (27, 1) => Some(format!("Docker {said} is older than 27.1")),
        Some(_) => None,
    }
}

/// The four words of Docker's answer, split as `read` does: on blanks in the
/// first line, what is left over staying in the last.
pub fn words(said: &str) -> [&str; 4] {
    let blank = [' ', '\t'];
    let mut rest = said.split('\n').next().unwrap_or("").trim_matches(blank);
    let mut found = [""; 4];
    for word in found.iter_mut().take(3) {
        let (first, after) = rest.split_once(blank).unwrap_or((rest, ""));
        *word = first;
        rest = after.trim_start_matches(blank);
    }
    found[3] = rest;
    found
}

/// The exit status and the reason for a network that is not as it should be.
/// It does not ask for Docker's version: a caller asks docker_fault first.
pub fn net_fault(
    docker: &mut impl FnMut(&[&str]) -> Option<String>,
    network: &str,
) -> Option<(u8, &'static str)> {
    let format = format!(
        "{{{{.Internal}}}} {{{{.Driver}}}} {{{{.EnableIPv6}}}} {{{{index .Options \"{OPTION}\"}}}}"
    );
    let Some(said) = docker(&["network", "inspect", "--format", &format, network]) else {
        return Some((2, "it could not be read"));
    };
    let [internal, driver, ipv6, mode] = words(&said);
    let why = if internal != "true" {
        "it is not internal"
    } else if driver != "bridge" {
        "its driver is not bridge"
    } else if ipv6 != "false" {
        "it has IPv6"
    } else if mode != "isolated" {
        "it is not isolated from the host"
    } else {
        return None;
    };
    Some((1, why))
}

pub fn net_create(docker: &mut impl FnMut(&[&str]) -> Option<String>, network: &str) -> bool {
    let option = format!("{OPTION}=isolated");
    let create = [
        "network",
        "create",
        "--internal",
        "--ipv6=false",
        "-o",
        &option,
        network,
    ];
    docker(&create).is_some()
}
