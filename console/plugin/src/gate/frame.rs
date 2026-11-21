//! `/_frame/<path>`: a file a plugin kept, shown inside its own view
//! (docs/specs/2026-11-21-plugins-v3.md, "Changes to v1", the web part). A
//! kept file is the plugin's data, not its code, so its policy is fixed, not
//! worked out from its bytes: `sandbox` without `allow-scripts`, so nothing in
//! it runs whatever it holds, no network, and only the plugin's own view may
//! frame it.

use super::http::{Body, Response};
use super::{catalog, PAGE_MAX};
use std::fs;
use std::path::Path;

/// The policy of a framed file, framed only by `own`, the plugin's origin.
pub fn policy(own: &str) -> String {
    format!(
        "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; \
         frame-ancestors {own}; base-uri 'none'; form-action 'none'; sandbox allow-same-origin"
    )
}

/// The type a kept file is served as, by its name: a page or an image, and
/// nothing a browser would run as script.
pub fn kind(name: &str) -> Option<&'static str> {
    let ext = name
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// The kept file at `segs` of the plugin `name` under `home`, framed only by
/// `own`. 404 for a link on the way, a folder, a file of another type or one
/// over [`PAGE_MAX`].
pub fn serve(home: &Path, name: &str, segs: &[String], own: &str) -> Response {
    let mut at = catalog::dir(home, name).join("keep");
    for s in segs {
        at.push(s);
        if fs::symlink_metadata(&at).map_or(true, |m| m.file_type().is_symlink()) {
            return Response::empty(404);
        }
    }
    let Ok(meta) = fs::symlink_metadata(&at) else {
        return Response::empty(404);
    };
    let file = segs.last().map_or("", String::as_str);
    let Some(kind) = kind(file) else {
        return Response::empty(404);
    };
    if !meta.is_file() || meta.len() > PAGE_MAX {
        return Response::empty(404);
    }
    let Ok(f) = fs::File::open(&at) else {
        return Response::empty(404);
    };
    let mut r = Response::empty(200);
    r.set("Content-Type", kind);
    r.set("Content-Security-Policy", &policy(own));
    r.body = Body::File(f, meta.len());
    r
}
