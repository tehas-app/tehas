//! A plugin as a call or a chat takes it: pinned, granted at the pin, ready
//! as `tehas update` left it, its image there when it has a build, and its
//! manifest read again from the commit and still the one update read.

use super::exec::Exec;
use super::prepare;
use std::path::Path;
use tehas_core::plugin::manifest::Manifest;
use tehas_core::plugin::state::Word;
use tehas_core::plugin::{self, Grants, Pin};

pub(crate) struct Ready {
    pub pin: Pin,
    /// The grants, at the pin.
    pub grants: Grants,
    /// The image, by id; None for a plugin with no build.
    pub image: Option<String>,
    pub manifest: Manifest,
}

/// The plugin `name`, ready, or why not.
pub(crate) fn ready(
    x: &mut dyn Exec,
    home: &Path,
    get: &dyn Fn(&str) -> String,
    name: &str,
) -> Result<Ready, String> {
    let pin = plugin::pins(&get("TEHAS_PLUGINS"))
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.name == name)
        .ok_or("TEHAS_PLUGINS names no such plugin")?;
    let grants = plugin::grants(&get(&plugin::grants_setting(name)))
        .ok()
        .filter(|g| g.commit == pin.commit)
        .ok_or("the grants are not for the pin")?;
    let state = prepare::read_state(home, name)
        .filter(|s| s.commit == pin.commit)
        .ok_or("it is unbuilt at its pin: run tehas update")?;
    if state.state != Word::Ready {
        let word = state.state.word();
        return Err(match &state.refusal {
            Some(why) => format!("it is {word}: {why}"),
            None => format!("it is {word}"),
        });
    }
    let r = prepare::read(x, home, name, &pin.commit)?;
    if r.manifest_sha256_differs(state.manifest_sha256.as_deref()) {
        return Err("the manifest is not the one tehas update read: run tehas update".into());
    }
    if !r.refusals.is_empty() {
        return Err(r.refusals.join("; "));
    }
    let manifest = r.manifest.ok_or("the manifest could not be read")?;
    if manifest.build.is_some() && state.image.is_none() {
        return Err("it has a build and no image: run tehas update".into());
    }
    Ok(Ready {
        pin,
        grants,
        image: state.image,
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::super::prepare::tests::{dockerfile, good, manifest, C, ID, URL};
    use super::*;
    use tehas_core::plugin::state::State;

    fn settings(grants: &str) -> impl Fn(&str) -> String {
        let grants = grants.to_string();
        move |k: &str| match k {
            "TEHAS_PLUGINS" => format!("floorplan={URL}@{C}"),
            "TEHAS_PLUGIN_GRANTS_FLOORPLAN" => grants.clone(),
            _ => String::new(),
        }
    }

    #[test]
    fn a_plugin_is_ready_only_pinned_granted_built_and_unchanged() {
        let m = manifest(r#""product""#);
        let d = dockerfile();
        let (home, mut x) = good("ready", &[("tehas-plugin.json", &m), ("Dockerfile", &d)]);
        let get = settings(&format!("{C} product"));
        let why = |x: &mut dyn Exec, get: &dyn Fn(&str) -> String| {
            ready(x, &home, get, "floorplan").err().unwrap_or_default()
        };
        assert_eq!(
            why(&mut x, &get),
            "it is unbuilt at its pin: run tehas update"
        );
        let mut s = State::new(Word::Ready, C);
        s.manifest_sha256 = Some(prepare::sha256(m.as_bytes()));
        prepare::write_state(&home, "floorplan", &s).unwrap();
        assert_eq!(
            why(&mut x, &get),
            "it has a build and no image: run tehas update"
        );
        s.image = Some(ID.into());
        prepare::write_state(&home, "floorplan", &s).unwrap();
        let r = ready(&mut x, &home, &get, "floorplan").unwrap();
        assert_eq!((r.pin.commit.as_str(), r.image.as_deref()), (C, Some(ID)));
        assert_eq!(r.manifest.label, "Floorplan");
        assert_eq!(
            why(&mut x, &settings(&format!("{} product", "f".repeat(40)))),
            "the grants are not for the pin"
        );
        assert_eq!(
            ready(&mut x, &home, &get, "other").err().as_deref(),
            Some("TEHAS_PLUGINS names no such plugin")
        );
        s.manifest_sha256 = Some("0".repeat(64));
        prepare::write_state(&home, "floorplan", &s).unwrap();
        assert!(why(&mut x, &get).starts_with("the manifest is not the one"));
        let held = State::refused(C, "the disk");
        prepare::write_state(&home, "floorplan", &held).unwrap();
        assert_eq!(why(&mut x, &get), "it is refused: the disk");
    }
}
