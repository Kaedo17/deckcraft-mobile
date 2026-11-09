//! The built-in file browser: every file dialog on a host with no system file picker (Android —
//! rfd has no backend there; the web keeps its own flow). Open, Save As, Export, Insert picture,
//! Insert audio/video, Import theme, the transition sound and Reuse Slides are all served by it,
//! answering exactly as the system dialog would: the picked path goes to the same follow-up.
//!
//! A folder path field with Up one level and Home folder, the folder's subfolders and the files
//! of the chosen type, a file name field and the file types. It only lists folders the user
//! navigates to and never reads a file: the picked path goes to the caller's follow-up. Navigation
//! is clamped inside the host's app-private storage ([`Services::app_files_dir`]): *Up one level*
//! stops at the root's top, the home-folder button goes to the app's Documents folder, and a typed
//! pick outside the root is refused as not found. The desktop has no root and opens no browser —
//! its system dialogs answer, as before.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use crate::SlideApp;
use crate::icons;
use crate::theme::{Tokens, font};
use crate::widgets;

/// At most this many entries are listed; a bigger folder shows the first ones and says so.
const MAX_LISTED: usize = 5000;
/// At most this many folder entries are read, so a huge folder can't stall the window.
const MAX_SCANNED: usize = 100_000;
const ROW: f32 = 24.0;

/// The listing's view, as PowerPoint's file dialog View dropdown: a list of names, or a grid of
/// icons at three sizes. The icon is the entry's type glyph, drawn in code — the browser never
/// reads a file, so a thumbnail is not on offer here (see AGENTS.md §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickerView {
    List,
    Small,
    Medium,
    Large,
}

/// One cell of the listing: how many columns fit, the width each gets, the row's height, the
/// glyph's edge and the name's font size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cell {
    pub(crate) cols: usize,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) glyph: f32,
    pub(crate) label: f32,
}

impl PickerView {
    /// Every view, in the order the dropdown lists them.
    pub(crate) const ALL: [PickerView; 4] = [PickerView::List, PickerView::Small, PickerView::Medium, PickerView::Large];

    /// The dropdown's label, as PowerPoint's reads.
    pub(crate) fn label(self) -> &'static str {
        match self {
            PickerView::List => "List",
            PickerView::Small => "Small icons",
            PickerView::Medium => "Medium icons",
            PickerView::Large => "Large icons",
        }
    }

    /// The name persisted in `ui.json` (`pickerView`).
    pub(crate) fn name(self) -> &'static str {
        match self {
            PickerView::List => "list",
            PickerView::Small => "small",
            PickerView::Medium => "medium",
            PickerView::Large => "large",
        }
    }

    /// The persisted name as a view: anything else (a hand-edited or damaged `ui.json`) is List.
    pub(crate) fn from_name(s: &str) -> PickerView {
        match s {
            "small" => PickerView::Small,
            "medium" => PickerView::Medium,
            "large" => PickerView::Large,
            _ => PickerView::List,
        }
    }

    /// The cell for a listing `width` wide. A list is one column of 24-point rows; the icon views
    /// fit as many columns as their base cell width allows, at least one, and split the width
    /// evenly. A width that isn't usable gives one column, so a damaged layout can't divide by
    /// zero or ask for a million columns.
    pub(crate) fn cell(self, width: f32) -> Cell {
        let (base, height, glyph, label) = match self {
            PickerView::List => (width.max(ROW), ROW, 18.0, 13.0),
            PickerView::Small => (120.0, 48.0, 24.0, 11.0),
            PickerView::Medium => (144.0, 66.0, 40.0, 12.0),
            PickerView::Large => (190.0, 96.0, 64.0, 13.0),
        };
        let cols = if matches!(self, PickerView::List) {
            1
        } else if width.is_finite() && width >= base {
            (width / base) as usize
        } else {
            1
        };
        Cell { cols, width: width / cols as f32, height, glyph, label }
    }

    /// Rows the listing of `n` entries needs in this view.
    pub(crate) fn rows(self, n: usize, width: f32) -> usize {
        let cols = self.cell(width).cols.max(1);
        n.div_ceil(cols)
    }
}

/// The glyph for an entry: a folder, or the file's type by its extension (any case). Every glyph
/// is one of DeckCraft's own icons (`crates/ui-egui/src/icons`), so no vendor artwork is involved.
pub(crate) fn type_glyph(dir: bool, path: &Path) -> icons::Icon {
    if dir {
        return icons::Icon::Folder;
    }
    let Some(ext) = path.extension().and_then(std::ffi::OsStr::to_str) else { return icons::Icon::File };
    let ext = ext.to_lowercase();
    match ext.as_str() {
        // Presentations, including the outline text a presentation can be made from.
        "deckcraft" | "slidecraft" | "pptx" | "ppt" | "potx" | "pot" | "ppsx" | "pps" | "txt" | "md" => icons::Icon::Slide,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "svg" => icons::Icon::Image,
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "wmv" | "avi" => icons::Icon::Video,
        "wav" | "mp3" | "m4a" | "m4b" | "aac" | "flac" | "ogg" | "oga" | "opus" | "aif" | "aiff" | "aifc" | "caf" | "wma" | "weba" | "mka" => {
            icons::Icon::Audio
        }
        "thmx" => icons::Icon::Themes,
        "ttf" | "otf" | "woff" | "woff2" => icons::Icon::Fonts,
        "csv" | "xlsx" | "xls" => icons::Icon::Table,
        _ => icons::Icon::File,
    }
}

/// The browser while it is open, the folder it showed last (where it opens next time), and the
/// answer waiting to be applied by the next logic pass.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) open: Option<Box<FileBrowser>>,
    pub(crate) last_dir: Option<PathBuf>,
    pub(crate) rx: Option<Receiver<Option<PathBuf>>>,
}

/// One subfolder or file in the listing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    /// The name as shown (a name that isn't valid UTF-8 is shown with replacement characters).
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) dir: bool,
}

/// A folder's subfolders (first) and files, by name.
#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) entries: Vec<Entry>,
    /// The folder holds more than was listed.
    pub(crate) truncated: bool,
}

/// Whether `path` has one of `exts` (case-insensitive); `*` takes any file.
fn matches(path: &Path, exts: &[String]) -> bool {
    exts.iter().any(|e| e == "*")
        || path.extension().and_then(std::ffi::OsStr::to_str).is_some_and(|e| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// The subfolders and the files with one of `exts` in `dir`, at most `cap` of them, folders first
/// and each by name. Hidden entries (a leading dot) are left out; entries that can't be read are
/// skipped.
pub(crate) fn list_dir(dir: &Path, exts: &[String], cap: usize) -> std::io::Result<Listing> {
    let mut listing = Listing::default();
    for (i, item) in std::fs::read_dir(dir)?.enumerate() {
        if i >= MAX_SCANNED {
            listing.truncated = true;
            break;
        }
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = item.path();
        // A symbolic link to a folder is a folder.
        let is_dir = match item.file_type() {
            Ok(t) if t.is_symlink() => std::fs::metadata(&path).is_ok_and(|m| m.is_dir()),
            Ok(t) => t.is_dir(),
            Err(_) => continue,
        };
        if !is_dir && !matches(&path, exts) {
            continue;
        }
        if listing.entries.len() >= cap {
            listing.truncated = true;
            break;
        }
        listing.entries.push(Entry { name, path, dir: is_dir });
    }
    listing.entries.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase()));
    Ok(listing)
}

/// The user's home folder (`None` on Android, where `HOME` is unset).
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// "Up one level" from `dir`: the parent while both `dir` and its parent stay inside `root`, else
/// `None` — at the root's top, and for a `dir` the root doesn't contain. `Path::starts_with`
/// compares path components, so a sibling whose name shares the root's text is not inside it.
pub(crate) fn up_from(root: &Path, dir: &Path) -> Option<PathBuf> {
    match dir.parent() {
        Some(parent) if dir.starts_with(root) && parent.starts_with(root) => Some(parent.to_path_buf()),
        _ => None,
    }
}

/// Whether `p` is a folder the browser may open: under the app-private root, or under the shared
/// root when the host offers one.
fn inside_root(p: &Path, app_root: &Path, shared_root: Option<&Path>) -> bool {
    p.starts_with(app_root) || shared_root.is_some_and(|s| p.starts_with(s))
}

/// The roots an Android browser walks (the host's app-private storage, and shared storage when
/// the host offers it): navigation is clamped inside them. The desktop has none —
/// `FileBrowser::roots` is `None` and every folder the user types is fair game.
pub(crate) struct Roots {
    /// The app-private root (`Services::app_files_dir`).
    pub(crate) app: PathBuf,
    /// Shared internal storage (`Services::shared_storage_root`), entered only while Android's
    /// "All files access" grant holds; `None` when the host offers no shared tree.
    pub(crate) shared: Option<PathBuf>,
}

/// The app storage the browser starts in: its `Documents` folder when it exists, else the root.
pub(crate) fn app_start(roots: &Roots) -> PathBuf {
    let documents = roots.app.join("Documents");
    if documents.is_dir() { documents } else { roots.app.clone() }
}

/// What the user typed in a path field, as a path: relative to `dir`, absolute, or under the home
/// folder (`~/…`). `None` when nothing was typed.
fn resolve(dir: &Path, typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    if typed == "~" {
        return home();
    }
    if let Some(rest) = typed.strip_prefix("~/") {
        return home().map(|h| h.join(rest));
    }
    Some(dir.join(typed))
}

/// What OK (or Enter, or a double click) did.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// The browser stays open (it moved to a folder, or has something to say).
    Stay,
    /// The file to open or save to.
    Pick(PathBuf),
}

/// A message under the listing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Note {
    None,
    NotFound(String),
    /// The file exists: the next OK replaces it.
    Replace(PathBuf),
    /// Shared storage needs Android's "All files access" before it can be browsed; the ask
    /// opened the system page where the user toggles it.
    Grant,
}

/// What a pick asks for, when the built-in browser serves it (a host with no system file dialog):
/// the kind, the title the caller shows and the file types.
pub(crate) enum BrowserRequest {
    /// Pick a file to open or import.
    Open { title: String, exts: Vec<String> },
    /// Pick where to save; `name` pre-fills the file name field.
    Save { title: String, name: String, exts: Vec<String> },
}

impl BrowserRequest {
    fn title(&self) -> &str {
        match self {
            BrowserRequest::Open { title, .. } | BrowserRequest::Save { title, .. } => title,
        }
    }

    /// OK's label, as the system dialog's would read.
    fn ok_label(&self) -> &'static str {
        match self {
            BrowserRequest::Open { .. } => "Open",
            BrowserRequest::Save { .. } => "Save",
        }
    }

    /// The type filters as (label, extensions): Open one combined list, Save one per type plus
    /// All files.
    fn filters(&self) -> Vec<(String, Vec<String>)> {
        match self {
            BrowserRequest::Open { exts, .. } => vec![("Supported files".into(), exts.clone())],
            BrowserRequest::Save { exts, .. } => {
                let mut filters: Vec<(String, Vec<String>)> = exts.iter().map(|e| (format!("*.{e}"), vec![e.clone()])).collect();
                filters.push(("All files".into(), vec!["*".into()]));
                filters
            }
        }
    }
}

/// The open browser: what it asks for, where its answer goes, the folder it shows and its fields.
pub(crate) struct FileBrowser {
    request: BrowserRequest,
    /// Where the answer goes when it closes (the next logic pass reads it).
    send: Sender<Option<PathBuf>>,
    /// The roots navigation is clamped to (Android); `None` on the desktop, as before.
    pub(crate) roots: Option<Roots>,
    pub(crate) dir: PathBuf,
    /// The folder field (edited by the user, applied on Enter).
    location: String,
    /// The file name field.
    pub(crate) name: String,
    /// The file types, as (label, extensions).
    pub(crate) filters: Vec<(String, Vec<String>)>,
    filter: usize,
    pub(crate) listing: Result<Listing, String>,
    /// The highlighted entry.
    pub(crate) selected: Option<PathBuf>,
    pub(crate) note: Note,
    /// The listing's view, remembered in `ui.json` as `pickerView`.
    pub(crate) view: PickerView,
    /// Whether the person chose a view in this browser: the remembered view is then not applied
    /// over their choice, and the choice is written back to `ui.json`.
    pub(crate) view_dirty: bool,
    focus_name: bool,
}

impl FileBrowser {
    pub(crate) fn new(request: BrowserRequest, start: PathBuf, roots: Option<Roots>, send: Sender<Option<PathBuf>>) -> Self {
        let name = match &request {
            BrowserRequest::Save { name, .. } => name.clone(),
            _ => String::new(),
        };
        let filters = request.filters();
        let mut b = FileBrowser {
            request,
            send,
            roots,
            dir: start.clone(),
            location: String::new(),
            name,
            filters,
            filter: 0,
            listing: Ok(Listing::default()),
            selected: None,
            note: Note::None,
            view: PickerView::List,
            view_dirty: false,
            focus_name: true,
        };
        b.navigate(start);
        b
    }

    fn exts(&self) -> &[String] {
        self.filters.get(self.filter).map_or(&[], |(_, e)| e.as_slice())
    }

    /// Show the folder `dir` (an unreadable folder shows why, and Up still works). On Android a
    /// folder outside the browser's root lands on the root's top instead.
    pub(crate) fn navigate(&mut self, dir: PathBuf) {
        let dir = self.clamped(dir);
        self.listing = list_dir(&dir, self.exts(), MAX_LISTED).map_err(|e| e.to_string());
        self.location = dir.to_string_lossy().into_owned();
        self.dir = dir;
        self.selected = None;
        self.note = Note::None;
    }

    /// The folder actually shown for `dir`: itself, or — when the browser is clamped and `dir`
    /// is outside its roots (a typed path, a jump) — the top of the root the browser is in.
    fn clamped(&self, dir: PathBuf) -> PathBuf {
        let Some(roots) = &self.roots else { return dir };
        if self.inside_root(&dir) {
            return dir;
        }
        match roots.shared.as_deref() {
            Some(shared) if self.dir.starts_with(shared) => shared.to_path_buf(),
            _ => roots.app.clone(),
        }
    }

    /// Whether `p` is a folder this browser may open (under either of its roots).
    pub(crate) fn inside_root(&self, p: &Path) -> bool {
        self.roots.as_ref().is_some_and(|r| inside_root(p, &r.app, r.shared.as_deref()))
    }

    /// The root the browser is currently in: shared storage while it browses there, else the
    /// app-private root; `None` on a host with no roots (the desktop).
    fn current_root(&self) -> Option<&Path> {
        let roots = self.roots.as_ref()?;
        match roots.shared.as_deref() {
            Some(shared) if self.dir.starts_with(shared) => Some(shared),
            _ => Some(roots.app.as_path()),
        }
    }

    /// Where "Up one level" goes: the parent, clamped to the root the browser is in on Android;
    /// `None` when there is nowhere up to go (the root's top, or no parent on the desktop).
    pub(crate) fn up_target(&self) -> Option<PathBuf> {
        match self.current_root() {
            Some(root) => up_from(root, &self.dir),
            None => self.dir.parent().map(Path::to_path_buf),
        }
    }

    fn up(&mut self) {
        if let Some(parent) = self.up_target() {
            self.navigate(parent);
        }
    }

    /// Where the app-files switcher goes: the app's Documents when it exists, else the app root.
    fn app_start_target(&self) -> Option<PathBuf> {
        self.roots.as_ref().map(app_start)
    }

    /// Where the home-folder button goes: the app's Documents on Android even while shared
    /// storage is being browsed, the user's home on the desktop (where no browser opens — for
    /// `resolve`'s `~`).
    fn home_target(&self) -> Option<PathBuf> {
        self.app_start_target().or_else(home)
    }

    /// The "Shared storage" tap: enter the shared root's top while the host says the grant holds;
    /// otherwise stay where the browser is, with a note explaining the ask — Android never grants
    /// "All files access" silently. No shared root at all: nothing to enter, grant or not.
    pub(crate) fn open_shared(&mut self, granted: bool) -> bool {
        let Some(shared) = self.roots.as_ref().and_then(|r| r.shared.clone()) else { return false };
        if !granted {
            self.note = Note::Grant;
            return false;
        }
        self.navigate(shared);
        true
    }

    /// OK: a highlighted folder is opened; otherwise the typed name is the pick. Save adds the
    /// chosen type's extension to a name without one and asks once more before replacing a file;
    /// Open wants a file that exists.
    pub(crate) fn accept(&mut self) -> Step {
        if let Some(dir) = self.selected.take().filter(|p| p.is_dir()) {
            self.navigate(dir);
            return Step::Stay;
        }
        let Some(mut path) = resolve(&self.dir, &self.name) else { return Step::Stay };
        // Android: a typed pick outside both roots (…/, an absolute path) is not one this dialog
        // may take.
        if self.roots.as_ref().is_some_and(|r| !inside_root(&path, &r.app, r.shared.as_deref())) {
            self.note = Note::NotFound(self.name.trim().to_string());
            return Step::Stay;
        }
        if path.is_dir() {
            self.name.clear();
            self.navigate(path);
            return Step::Stay;
        }
        if matches!(self.request, BrowserRequest::Open { .. }) {
            if path.is_file() {
                return Step::Pick(path);
            }
            self.note = Note::NotFound(self.name.trim().to_string());
            return Step::Stay;
        }
        if path.extension().is_none()
            && let Some(ext) = self.exts().first().filter(|e| e.as_str() != "*")
        {
            path.set_extension(ext);
        }
        if path.exists() && self.note != Note::Replace(path.clone()) {
            self.note = Note::Replace(path);
            return Step::Stay;
        }
        Step::Pick(path)
    }

    /// A click on an entry: highlight it, and a file's name goes into the file name field.
    pub(crate) fn select(&mut self, e: &Entry) {
        self.selected = Some(e.path.clone());
        if !e.dir {
            self.name = e.name.clone();
        }
        self.note = Note::None;
    }
}

/// Open the browser at `start` for `request`, its navigation clamped to `roots`; the answer (or
/// `None`: cancelled) arrives on the receiver, which the next logic pass drains.
pub(crate) fn open_browser(request: BrowserRequest, start: PathBuf, roots: Option<Roots>) -> (Receiver<Option<PathBuf>>, Box<FileBrowser>) {
    let (send, recv) = channel();
    (recv, Box::new(FileBrowser::new(request, start, roots, send)))
}

/// Close the browser with this answer (`None`: cancelled); the pending follow-up takes it.
pub(crate) fn close(app: &mut SlideApp, picked: Option<PathBuf>) {
    let Some(b) = app.file_browser.open.take() else { return };
    app.file_browser.last_dir = Some(b.dir.clone());
    let _ = b.send.send(picked);
}

/// The browser closed without a pick (Escape, the Cancel button or the window's X).
pub(crate) fn cancel(app: &mut SlideApp) {
    close(app, None);
}

/// Draw the browser, if it is open (called from [`crate::dialogs`] with the other dialogs). It
/// sits above the app dialogs: its Escape closes it first.
pub(crate) fn show(app: &mut SlideApp, ctx: &egui::Context) {
    let Some(mut b) = app.file_browser.open.take() else { return };
    let t = Tokens::get(ctx);
    let title = b.request.title().to_string();
    let mut done: Option<Option<PathBuf>> = None;
    let mut open = true;
    // The view remembered in `ui.json`, unless the person already chose one in this browser.
    if !b.view_dirty {
        b.view = PickerView::from_name(&app.ui.picker_view);
    }
    egui::Window::new(title)
        .id(egui::Id::unique(("builtin_file_browser",)))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .order(egui::Order::Foreground)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(620.0);
            done = body(ui, &mut b, &t, &app.services);
        });
    if !open {
        done = Some(None);
    }
    let chosen = b.view_dirty.then(|| b.view.name().to_string());
    app.file_browser.open = Some(b);
    if let Some(name) = chosen {
        // The next dialog opens in the view this one was browsed in.
        app.ui.picker_view = name;
    }
    if let Some(answer) = done {
        close(app, answer);
    }
}

/// The browser's contents; `Some` once it closes (`Some(None)`: cancelled).
fn body(ui: &mut Ui, b: &mut FileBrowser, t: &Tokens, services: &crate::Services) -> Option<Option<PathBuf>> {
    let mut location_done = false;
    ui.horizontal(|ui| {
        if widgets::small_button(ui, icons::Icon::ChevronUp, "Up one level", true, false).clicked() {
            b.up();
        }
        if widgets::small_button(ui, icons::Icon::Home, "Home folder", true, false).clicked()
            && let Some(h) = b.home_target()
        {
            b.navigate(h);
        }
        let r = ui.add(egui::TextEdit::singleline(&mut b.location).desired_width(f32::INFINITY));
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            location_done = true;
            let typed = b.location.clone();
            if let Some(dir) = resolve(&b.dir, &typed) {
                b.navigate(dir);
            }
        }
    });
    ui.add_space(4.0);
    // Root switcher, drawn only where the host offers shared storage (Android): the browser walks
    // the app-private tree or shared storage, one root at a time. "Shared storage" without the
    // grant opens Android's settings page instead of navigating — the grant can never be taken
    // silently, and the tap re-checks it, so coming back from the page enters it at once.
    if let Some(shared) = b.roots.as_ref().and_then(|r| r.shared.clone()) {
        let in_shared = b.dir.starts_with(&shared);
        ui.horizontal(|ui| {
            if ui.selectable_label(!in_shared, "App files").clicked()
                && in_shared
                && let Some(start) = b.app_start_target()
            {
                b.navigate(start);
            }
            let granted = services.shared_storage_granted.as_ref().is_some_and(|f| f());
            if ui.selectable_label(in_shared, "Shared storage").clicked()
                && !in_shared
                && !b.open_shared(granted)
                && let Some(ask) = &services.shared_storage_grant
            {
                ask();
            }
        });
        ui.add_space(4.0);
    }
    let mut pick = false;
    let mut go: Option<PathBuf> = None;
    let mut clicked: Option<Entry> = None;
    egui::Frame::NONE.fill(t.text_field).stroke(egui::Stroke::new(1.0, t.border)).inner_margin(2.0).show(ui, |ui| {
        let entries: &[Entry] = b.listing.as_ref().map(|l| l.entries.as_slice()).unwrap_or(&[]);
        // An unreadable folder has its own message below; the hint is for a listing that came back empty.
        let listed = b.listing.is_ok();
        // The view decides the layout: one column of rows, or a grid of icon cells that fit the width.
        let cell = b.view.cell(ui.available_width());
        let cols = cell.cols.max(1);
        let rows = b.view.rows(entries.len(), ui.available_width());
        crate::widgets::panel_scroll(egui::ScrollArea::vertical())
            .id_salt("builtin_file_browser_list")
            .max_height(300.0)
            .min_scrolled_height(300.0)
            .auto_shrink([false, false])
            .show_rows(ui, cell.height, rows, |ui, range| {
                if listed && entries.is_empty() {
                    // An empty folder (or a type filter that matches nothing) says so instead of
                    // showing a blank box.
                    ui.add_space(140.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("No items to show.").small().color(t.text_dim));
                    });
                }
                // The entries this row shows: a list row is one entry, a grid row up to `cols`.
                let from = range.start.saturating_mul(cols).min(entries.len());
                let to = range.end.saturating_mul(cols).min(entries.len());
                let shown = entries.get(from..to).unwrap_or(&[]);
                let (row, _row_resp) = ui.allocate_exact_size(vec2(ui.available_width(), cell.height), Sense::hover());
                for (i, e) in shown.iter().enumerate() {
                    // A cell of its own width; a list row takes the whole width.
                    let x0 = row.min.x + i as f32 * cell.width;
                    let r = Rect::from_min_max(pos2(x0, row.min.y), pos2(x0 + cell.width, row.max.y));
                    let resp = ui.interact(r, ui.scope_id().with(("builtin_file_browser_entry", &e.path)), Sense::click());
                    if b.selected.as_ref() == Some(&e.path) {
                        ui.painter().rect_filled(r, 2.0, t.selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, 2.0, t.hover);
                    }
                    let glyph = type_glyph(e.dir, &e.path);
                    if cols == 1 {
                        icons::paint(
                            ui.painter(),
                            Rect::from_center_size(pos2(r.min.x + 14.0, r.center().y), vec2(cell.glyph, cell.glyph)),
                            glyph,
                            t.text_dim,
                            false,
                        );
                        ui.painter().with_clip_rect(r).text(
                            pos2(r.min.x + 30.0, r.center().y),
                            Align2::LEFT_CENTER,
                            &e.name,
                            font(cell.label),
                            t.text,
                        );
                    } else {
                        // The glyph above, its name below, both centred in the cell: the icon is the
                        // entry's type, never a picture of the file's contents.
                        let name_band = cell.label + 8.0;
                        let gy = row.min.y + cell.height - name_band - cell.glyph * 0.5;
                        icons::paint(
                            ui.painter(),
                            Rect::from_center_size(pos2(r.center().x, gy), vec2(cell.glyph, cell.glyph)),
                            glyph,
                            t.text_dim,
                            false,
                        );
                        ui.painter().with_clip_rect(r).text(
                            pos2(r.center().x, r.max.y - 2.0),
                            Align2::CENTER_BOTTOM,
                            &e.name,
                            font(cell.label),
                            t.text,
                        );
                    }
                    if resp.double_clicked() {
                        if e.dir {
                            go = Some(e.path.clone());
                        } else {
                            clicked = Some(e.clone());
                            pick = true;
                        }
                    } else if resp.clicked() {
                        clicked = Some(e.clone());
                    }
                }
            });
    });
    if let Some(e) = clicked {
        b.select(&e);
    }
    if let Some(dir) = go {
        b.navigate(dir);
    }
    let message = match (&b.listing, &b.note) {
        (_, Note::NotFound(name)) => Some((format!("{name} wasn't found."), t.accent)),
        (_, Note::Replace(p)) => {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Some((format!("{name} already exists. Choose Save again to replace it."), t.accent))
        }
        (_, Note::Grant) => Some((
            "To browse shared storage, allow DeckCraft all-files access in Android's settings, then choose Shared storage again.".to_string(),
            t.accent,
        )),
        (Err(e), _) => Some((format!("This folder can't be read: {e}"), t.accent)),
        (Ok(l), _) if l.truncated => Some((format!("Showing the first {} items.", l.entries.len()), t.text_dim)),
        _ => None,
    };
    if let Some((text, color)) = message {
        ui.label(egui::RichText::new(text).color(color));
    }
    ui.add_space(6.0);
    let mut refilter = false;
    let mut view_changed = false;
    ui.horizontal(|ui| {
        ui.label("File name");
        // The name field takes what the label and the two dropdowns leave.
        let width = (ui.available_width() - 320.0).max(120.0);
        let r = ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(width));
        if b.focus_name {
            r.request_focus();
            b.focus_name = false;
        }
        if r.changed() {
            b.selected = None;
            b.note = Note::None;
        }
        let before = b.filter;
        let current = b.filters.get(b.filter).map(|(l, _)| l.clone()).unwrap_or_default();
        craft_ui::ComboBox::from_id_salt("builtin_file_browser_type").selected_text(current).width(150.0).show_ui(ui, |ui| {
            for (i, (l, _)) in b.filters.iter().enumerate() {
                ui.selectable_value(&mut b.filter, i, l.as_str());
            }
        });
        refilter = b.filter != before;
        // PowerPoint's View dropdown: the same entries, as a list or as icons of three sizes. The
        // choice is remembered in `ui.json`, so the next dialog opens the way this one was browsed.
        let before_view = b.view;
        craft_ui::ComboBox::from_id_salt("builtin_file_browser_view").selected_text(b.view.label()).width(120.0).show_ui(ui, |ui| {
            for v in PickerView::ALL {
                ui.selectable_value(&mut b.view, v, v.label());
            }
        });
        view_changed = b.view != before_view;
    });
    if refilter {
        b.navigate(b.dir.clone());
    }
    if view_changed {
        b.view_dirty = true;
    }
    ui.add_space(8.0);
    let mut cancel = false;
    // Enter answers OK when the location field didn't just take it, as the app dialogs do.
    let enter = !location_done && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui
            .add(egui::Button::new(egui::RichText::new(b.request.ok_label()).color(t.accent_text)).fill(t.accent).min_size(vec2(72.0, 24.0)))
            .clicked()
        {
            pick = true;
        }
        if ui.add(egui::Button::new("Cancel").min_size(vec2(72.0, 24.0))).clicked() {
            cancel = true;
        }
    });
    if cancel {
        return Some(None);
    }
    if (pick || enter)
        && let Step::Pick(path) = b.accept()
    {
        return Some(Some(path));
    }
    None
}
