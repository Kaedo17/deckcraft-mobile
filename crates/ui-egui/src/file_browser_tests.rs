#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The built-in file browser: listing and clamps (pure logic), and the routing that serves a
//! file request over the host's app-private storage when the host has no system dialog.

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::channel;

use serde_json::json;

use super::file_browser::{BrowserRequest, Entry, FileBrowser, Note, Roots, Step, cancel, close, list_dir, up_from};
use crate::{Services, SlideApp};
use deckcraft_engine::Session;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("deckcraft-ui-browser-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn exts(e: &[&str]) -> Vec<String> {
    e.iter().map(|x| x.to_string()).collect()
}

fn open_request() -> BrowserRequest {
    BrowserRequest::Open { title: "Open presentation".into(), exts: exts(&["deckcraft"]) }
}

/// Folders come first, then the files of the chosen type (any case), each by name; hidden
/// entries and other types are left out, and a big folder is cut off and says so.
#[test]
fn listing_filters_sorts_and_caps() {
    let dir = scratch("list");
    for f in ["b.deckcraft", "A.DECKCRAFT", "notes.txt", ".hidden.deckcraft", "c.deck"] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    std::fs::create_dir(dir.join("zeta")).unwrap();
    std::fs::create_dir(dir.join(".git")).unwrap();
    let l = list_dir(&dir, &exts(&["deckcraft"]), 100).unwrap();
    let names: Vec<&str> = l.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["zeta", "A.DECKCRAFT", "b.deckcraft"], "folders first, then the chosen type (any case) by name; .deck is another type here");
    assert!(!l.truncated);
    assert_eq!(list_dir(&dir, &exts(&["*"]), 100).unwrap().entries.len(), 5, "the files plus the folder");
    let capped = list_dir(&dir, &exts(&["deckcraft"]), 2).unwrap();
    assert_eq!(capped.entries.len(), 2);
    assert!(capped.truncated);
    assert!(list_dir(&dir.join("missing"), &exts(&["*"]), 10).is_err(), "an unreadable folder is an error, not a panic");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Save in the browser: a typed folder is opened, a bare name gets the chosen type's
/// extension, and an existing file is replaced only on the second OK.
#[test]
fn save_adds_the_extension_and_asks_before_replacing() {
    let dir = scratch("save");
    std::fs::create_dir(dir.join("drafts")).unwrap();
    std::fs::write(dir.join("drafts").join("old.deckcraft"), b"x").unwrap();
    let (send, _rx) = channel();
    let mut b = FileBrowser::new(
        BrowserRequest::Save { title: "Save As".into(), name: "Presentation.deckcraft".into(), exts: exts(&["deckcraft", "pptx"]) },
        dir.clone(),
        None,
        send,
    );
    b.name = "drafts".into();
    assert_eq!(b.accept(), Step::Stay);
    assert_eq!(b.dir, dir.join("drafts"));
    b.name = "novel".into();
    assert_eq!(b.accept(), Step::Pick(dir.join("drafts").join("novel.deckcraft")));
    b.name = "old".into();
    assert_eq!(b.accept(), Step::Stay, "the first OK asks before replacing");
    assert_eq!(b.accept(), Step::Pick(dir.join("drafts").join("old.deckcraft")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// "Up one level" stays inside the shown root: the parent while it is still under the root,
/// and none at the root's top.
#[test]
fn up_from_stops_at_the_root() {
    let root = Path::new("/data/user/0/ai.storyteller.deckcraft/files/DeckCraft");
    assert_eq!(up_from(root, &root.join("Documents").join("drafts")), Some(root.join("Documents")));
    assert_eq!(up_from(root, &root.join("Documents")), Some(root.to_path_buf()));
    assert_eq!(up_from(root, root), None, "the root's top has no Up");
    // a root that isn't a parent of dir (defensive): no Up rather than an escape
    assert_eq!(up_from(root, Path::new("/storage/emulated/0")), None);
}

/// A folder this browser may open is under the root; a path that merely starts with the same
/// text but is a sibling directory is not inside it.
#[test]
fn inside_root_accepts_only_the_root() {
    let root = Path::new("/data/user/0/ai.storyteller.deckcraft/files/DeckCraft");
    let (send, _rx) = channel();
    let b = FileBrowser::new(open_request(), root.join("Documents"), Some(Roots { app: root.to_path_buf() }), send);
    assert!(b.inside_root(&root.join("Documents")), "a folder under the root");
    assert!(b.inside_root(root), "the root itself is a folder the browser may show");
    assert!(!b.inside_root(Path::new("/data/user/0/ai.storyteller.deckcraft/other")));
    assert!(!b.inside_root(Path::new("/data/user/0/ai.storyteller.deckcraft/files/DeckCraft-x")), "a sibling that shares a prefix");
    assert!(!b.inside_root(Path::new("/etc")));
}

/// Android (no system dialog, the host's app-private storage recorded): Save As serves the
/// request in the built-in browser over that storage instead of answering none — the browser
/// is clamped to the host's root, starts in its Documents, and its answer saves the file.
#[test]
fn app_files_dir_serves_save_as_in_the_browser() {
    let base = scratch("android-route");
    std::fs::create_dir_all(base.join("Documents")).unwrap();
    let mut app = SlideApp::new(Session::new(), Services { app_files_dir: Some(base.clone()), ..Default::default() });
    app.session.execute("file.new", &json!({})).unwrap();
    app.save_as_dialog("deckcraft");
    {
        let b = app.file_browser.open.as_ref().expect("the built-in browser serves the request");
        assert_eq!(b.roots.as_ref().map(|r| r.app.clone()), Some(base.clone()), "clamped to the host's root");
        assert_eq!(b.dir, base.join("Documents"), "starts in the app's Documents");
        assert_eq!(b.name, "Presentation1.deckcraft", "the save name pre-fills the field (the new document's title)");
    }
    assert!(app.session.active().is_some_and(|d| d.path.is_none()), "the pick is pending: nothing saved yet");
    let path = base.join("Documents").join("novel.deckcraft");
    {
        let b = app.file_browser.open.as_mut().unwrap();
        b.name = "novel".into();
        assert_eq!(b.accept(), Step::Pick(path.clone()), "the typed name is picked");
    }
    close(&mut app, Some(path));
    app.poll_file_browser();
    let saved = base.join("Documents").join("novel.deckcraft");
    assert!(saved.is_file(), "the answer saves into the browser's Documents");
    assert_eq!(app.session.active().and_then(|d| d.path.clone()).unwrap_or_default(), saved.to_string_lossy().as_ref());
    assert_eq!(app.ui.recent.first().map(String::as_str), Some(saved.to_string_lossy().as_ref()), "the saved file leads the recent list");
    let _ = std::fs::remove_dir_all(&base);
}

/// No app-private storage (a bare host): today's behaviour — no browser opens and the request
/// answers none.
#[test]
fn no_app_files_dir_answers_none_without_a_browser() {
    let mut app = SlideApp::new(Session::new(), Services::default());
    app.session.execute("file.new", &json!({})).unwrap();
    app.save_as_dialog("deckcraft");
    assert!(app.file_browser.open.is_none(), "no browser without a host's root");
    app.poll_file_browser();
    assert!(app.pending_follow.is_none(), "nothing was pending");
    assert!(app.session.active().is_some_and(|d| d.path.is_none()), "nothing was saved");
}

/// Cancel (Escape, the button or the window's X) closes the browser and answers none:
/// nothing is saved.
#[test]
fn cancelling_the_browser_answers_none() {
    let base = scratch("cancel");
    std::fs::create_dir_all(base.join("Documents")).unwrap();
    let mut app = SlideApp::new(Session::new(), Services { app_files_dir: Some(base.clone()), ..Default::default() });
    app.session.execute("file.new", &json!({})).unwrap();
    app.save_as_dialog("deckcraft");
    assert!(app.file_browser.open.is_some());
    cancel(&mut app);
    assert!(app.file_browser.open.is_none());
    app.poll_file_browser();
    assert!(app.pending_follow.is_none(), "the cancelled request is gone");
    assert!(app.session.active().is_some_and(|d| d.path.is_none()), "nothing was saved");
    let _ = std::fs::remove_dir_all(&base);
}

/// Navigation is clamped inside the host's root: a typed pick outside is refused as not
/// found, a folder jump outside lands on the root's top, and the root's top has no Up.
#[test]
fn a_pick_outside_the_root_is_refused_and_clamped() {
    let base = scratch("clamp");
    std::fs::create_dir_all(base.join("Documents")).unwrap();
    let (send, _rx) = channel();
    let mut b = FileBrowser::new(open_request(), base.join("Documents"), Some(Roots { app: base.clone() }), send);
    b.name = "/etc/passwd".into();
    assert_eq!(b.accept(), Step::Stay);
    assert_eq!(b.note, Note::NotFound("/etc/passwd".into()), "outside the root: not found");
    b.navigate(base.parent().unwrap().to_path_buf());
    assert_eq!(b.dir, base, "a folder outside the root lands on the root's top");
    assert_eq!(b.up_target(), None, "the root's top has no Up");
    let _ = std::fs::remove_dir_all(&base);
}

/// A highlighted entry takes its name into the file name field; folders stay highlighted only.
#[test]
fn selecting_an_entry_names_a_file() {
    let dir = scratch("select");
    std::fs::write(dir.join("talk.deckcraft"), b"x").unwrap();
    std::fs::create_dir(dir.join("slides")).unwrap();
    let (send, _rx) = channel();
    let mut b = FileBrowser::new(open_request(), dir.clone(), None, send);
    b.select(&Entry { name: "talk.deckcraft".into(), path: dir.join("talk.deckcraft"), dir: false });
    assert_eq!(b.name, "talk.deckcraft", "a file's name goes into the name field");
    b.select(&Entry { name: "slides".into(), path: dir.join("slides"), dir: true });
    assert_eq!(b.name, "talk.deckcraft", "a folder keeps the typed name");
    let _ = std::fs::remove_dir_all(&dir);
}
