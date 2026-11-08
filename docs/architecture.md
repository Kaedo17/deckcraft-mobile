# DeckCraft architecture

> **Last reviewed:** 2026-10-14 · **Last updated:** 2026-11-08 · **Change:** minor (Android touch ergonomics: the canvas and the sorter's rows arbitrate a finger press so a drag pans instead of selecting or reordering, scroll bodies scroll by finger drag, scroll bars take a 24-point fingertip band, interface size reaches 50%, and Android's Back answers Escape instead of backgrounding the app; previous: minor, Android input is completed — the pinned winit is vendored with the Android mouse delta (wheel, right/middle buttons, hover, one click per press) and the app-side modifier stamp gives chords and the wheel their real modifiers; previous: minor, Android's built-in file browser serves every file dialog over app-private storage, and shared storage behind the "All files access" grant; launcher icon mipmaps; previous: trivial, Android entry crate `deckcraft-android` registered; settings, the log file and AutoRecover resolve to the app-private storage; previous: minor, the `deckcraft` app crate became library + shim binary with the eframe features split by target; previous: minor, native chart payloads stored in bounded ZIP members; previous: minor, community PR wave 2026-10-14: 320 engine commands, 41 dialogs, the `metafile` crate, 1,641 tests; duplicated L2/L5/L6 rows and headers folded; previous: minor, transition frames) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

How DeckCraft is built today. The binding rules (layering, everything is a command, thin UI, never
break wasm, never crash) are in [AGENTS.md](../AGENTS.md) §3–§5; this document describes what
exists. 202k lines of Rust in 24 crates, 3 apps and `xtask`; 1,641 `#[test]` functions
(`cargo xtask stats` counts them per crate).

## Layers

`cargo xtask layers` enforces that a crate depends only on lower layers (the table in
`xtask/src/layers.rs`, which is authoritative; AGENTS.md §4 gives the simplified picture), and that
nothing below L6 uses egui, eframe, winit or rfd.

| Layer | Crates | What they do |
|---|---|---|
| L0 | `geom`, `color`, `metafile`; standalone `bitstream`, `matroska`, `ogg`, `opus` | EMU/pt units, transforms, ~180 preset shape generators re-derived from ECMA-376, adjust handles, hit testing; RGB/HSL and DrawingML colour transforms; MKV/WebM and Ogg demuxers, the Opus decoder; EMF/WMF read into a neutral vector model (`metafile`) |
| L1 | `model`, `fonts`; `h264`, `hevc`, `vp9`, `av1`, `isobmff` | Presentation, masters, layouts, slides, shapes, text, themes, tables, charts, transitions, animations, placeholder inheritance (`resolve`); font database (craft-fonts + system) with metric-compatible substitutes for Office fonts (Carlito for Calibri, Caladea for Cambria, Liberation for Arial/Times); video decoders and the MP4/MOV demuxer |
| L2 | `text`, `anim`, `format`, `media` | Shaping (HarfRust), UAX #9 bidi, UAX #14 line breaking, autofit, caret hit testing; per-shape animation state over time, the show state machine, transition frame layouts, Morph matching; `.deckcraft` (zip + JSON + media) and outline text; media probing, decoding, mixer and clock |
| L3 | `render`, `pptx` | `vello_cpu` raster of slides, masters and layouts (fills, outlines, effects, pictures, tables, charts, Morph frames); PPTX import/export from ECMA-376 |
| L4 | `pdf` | App-owned export layout and rendering through the shared `craft-pdf` writer |
| L5 | `engine` | Sessions, documents, selection, undo history on shared slide snapshots, the command registry (320 commands in `cmd/*`), pointer tools, keys, AutoRecover, the panic guard (`guard.rs`) |
| L6 | `ui-egui`, `mcp` | The PowerPoint-style interface (ribbon with 10 tabs + contextual tabs, canvas, thumbnails, panes, 41 dialogs, show and presenter windows, the JSON control channel, and a built-in file browser serving file dialogs on hosts with no system one (Android)); the MCP server (JSON-RPC 2.0 over stdio), headless or connected to a running app |
| apps | `deckcraft` (app library + shim binary: eframe/wgpu, cpal audio, logging; Android entry calls `run_android` and resolves app-private storage and offers shared storage to the browser through the grant hooks), `deckcraft-android` (Android NativeActivity entry: a cdylib whose `android_main` opens the shared app, carrying the launcher-icon mipmaps, the shared-storage permissions, and — the workspace's named `unsafe` exception, confined to one module — the JNI grant check and ask in `android_access`), `deckcraft-cli` (run, render, convert, mcp, app), `deckcraft-web` (trunk; WebGPU with WebGL2 fallback), `xtask` | Exempt from layering |

The media codecs and containers are clean-room pure Rust, 59k lines, shared in spirit with
FilmCraft.

## Data model

A `Presentation` holds a theme set, slide masters with their layouts, slides, sections, custom
shows, comments, media and document properties. Shapes are a tree (`ShapeKind`: preset or custom
geometry, picture, table, chart, group, connector, ink, media, SmartArt-as-group, and `Opaque` for
anything read from PPTX that DeckCraft doesn't model: its XML is kept and written back). Text
bodies hold paragraphs of runs with DrawingML properties; formatting resolves slide → layout →
master → theme at render time. Transitions live on slides; animations are the flat ordered list the
Animation Pane shows, rebuilt into PowerPoint's timing tree on save.

## Commands and frontends

Every user-visible action is a `CommandSpec` (id, label, ribbon/menu place, shortcut, params doc,
`enabled`, `run`) in `crates/engine/src/cmd/*`. The UI, `deckcraft-cli run`, the JSON control
channel (`deckcraft --control PORT`, [control-protocol.md](control-protocol.md)) and MCP
([mcp.md](mcp.md)) all dispatch through `Session::execute`, so they behave identically.
Programmatic calls never open dialogs. Pointer gestures go through `Session::pointer`, keys through
`Session::key`.

## Rendering and the show

The renderer draws a slide to an RGBA pixmap on the CPU; the UI uploads it as an egui texture.
Transitions are layers of textured quads over the old and new slide images, or over their
backgrounds and contents rendered apart (Dynamic Content), from `anim::transition_frame`: 3-D
effects project cards through a perspective camera and `anim::layer_mesh` subdivides them so
textures stay straight; soft edges are per-corner opacities. The show renders the slides once per
transition and only composites cached textures each frame; `render::composite_transition` is the
CPU twin used by `deckcraft-cli render-transition` and the tests. The show window and the presenter view are egui viewports; animation state per shape
comes from `anim::timeline` each frame. Animations ▸ Preview plays that same state on the editor
canvas for the current slide, then draws the slide at rest. PDF export renders each slide at print resolution and
overlays an invisible real-text layer with embedded font subsets.

## File I/O

- **PPTX** (`crates/pptx`): lenient reader (unknown parts skipped, malformed parts logged and
  ignored, sizes and depths bounded, zip bombs capped), writer that PowerPoint opens without
  repair. Unknown elements round-trip through `Opaque` and `raw_ext` fields.
- **Native** (`crates/format`): zip with `mimetype`, `presentation.json`, `media/<id>` and
  `chart-parts/<index>`. Chart relationships and payload references stay in the manifest; binary
  chart parts use bounded ZIP reads and share the media byte/item budgets. Earlier inline chart
  bytes still load and migrate on save; `.slidecraft` files from before the rename still open.
- Formats and their gaps: [file-format-parity.md](file-format-parity.md).

## Quality gates

`cargo xtask ci`: rustfmt check, clippy `-D warnings`, tests, layering, asset attribution
(`cargo xtask assets`), wasm32 check. CI runs it on Ubuntu for every PR and push to `main`;
macOS is built and tested by the release workflow only. Releases (pushes to `release`) build signed
macOS, Windows, Linux, FreeBSD and web artifacts.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-08 | minor | Android touch ergonomics: `canvas::touch` arbitrates a finger press on the canvas — a move before 0.55 s pans the slide stack and the engine's marquee stands down for the rest of the press, a press the engine owns (shape, caret, handle: `ToolState::marquee_press()` says which) is never arbitrated, a still hold past it grows a selection as a desktop drag does, and a still hold opens the canvas menu through egui's `long_touched()`. `widgets::touch_row` does the same for the sorter's section bars and thumbnails (early move scrolls, hold-then-move reorders, a move under an open menu never picks the row up), panning one area once per pass and latching the verdict for the press. `widgets::panel_scroll` forces drag-to-scroll on every scroll body (panels, dialog bodies, galleries, the file browser, presenter and notes panes) and `widgets::touch_scroll_target` widens scroll bars to 24 points once a touch screen is seen; Preferences ▸ Interface size now runs from 50% to 175%. The vendored winit answers Android's `KEYCODE_BACK` as Escape and consumes it, so Back dismisses a dialog or menu while Home still backgrounds the activity. Mouse presses are never arbitrated, and drag commits read `interact_pos`, which survives the frame a finger lifts. |
| 2026-11-08 | minor | Android mouse and keyboard: the pinned `storytold/winit` rev `a517b3846` is vendored at `third_party/winit` (`[patch.crates-io]` path; every other target compiles the pinned revision byte-for-byte) with the Android backend's mouse input filled in — hover, wheel, right/middle/back/forward buttons, the primary button deduped so one click is one click, and the changed button derived from the button-state diff (some devices' libandroid omits the API 33 `AMotionEvent_getActionButton`; a reference to it fails the whole library to load). winit's Android backend never emits `ModifiersChanged`, so `SlideApp::raw_input_hook` stamps each event's modifiers from the tracked key events and ends the frame with the real state: Ctrl chords, Ctrl+wheel zoom and Ctrl+click multi-select work. The dedupe's unit tests run on every host through `crates/ui-egui/tests/android_primary_button.rs` (the patched package is not a workspace member); the stamp has 7 unit tests in `app::modifier_stamp_tests`. |
| 2026-11-08 | minor | Shared storage for Android's file dialogs: `Services` gains the shared root (`/storage/emulated/0`) plus a re-asked grant answer and an ask hook, and the browser's switcher enters it only while Android's "All files access" holds — otherwise it opens the settings page that grants it (Android 11+) or the runtime permission dialog (Android 9); Android 10 targets scoped storage, so nothing is offered there. The entry crate's `android_access` holds the raw JNI handles — AGENTS.md §3's named `unsafe` exception, confined to that module, plain closures out. The manifest declares the permissions (declaring grants nothing), and the launcher icon ships as five mipmap densities rendered by `cargo xtask mipmaps`. |
| 2026-11-08 | minor | Built-in file browser (`ui-egui` `file_browser`): Open, Save As, image/media/theme imports and exports answer it on hosts without a system dialog (Android); listing, type filters, typed paths, folder navigation, a replace-ask, and clamping inside the host's roots. On desktop and web the system pickers (`rfd`) stay, unchanged. |
| 2026-11-08 | trivial | Android entry crate `deckcraft-android` added (NativeActivity `android_main` → `deckcraft::run_android`; APK identity, sdk 28/36, landscape, exhaustive configChanges). The app library resolves Android's app-private storage once at launch: settings folder, log file and AutoRecover copies hang off the activity's internal data path (`android_storage`) |
| 2026-11-08 | minor | `deckcraft` became library + shim binary: window, startup, hooks and tests in `lib.rs`, `main.rs` calls `deckcraft::run()`; eframe features split by target (desktop accesskit/clipboard/links/wayland/x11, Android android-native-activity); rfd, arboard and image drop off the Android graph |
| 2026-10-18 | minor | Native chart payloads use referenced ZIP members and share media byte/item limits; old inline sidecars remain readable. Synthetic tests cover multi-megabyte layout, all shape owners, shared budgets, missing/corrupt members and invalid references. Scores and milestone scope unchanged |
| 2026-10-14 | minor | Community PR wave folded: one L2/L5/L6 row each; command registry 320 (`cmd!` registrations in `cmd/*`), 41 dialogs (titled in `dialogs.rs`), 202k lines, 24 crates (`metafile` added at L0), 1,641 tests. The per-PR rows below give the counts each PR saw on its own branch |
| 2026-10-14 | minor | Size and Position dialog (`shape.sizePosition`): position, size, rotation, scale and lock aspect in one undo step. 227 commands, 28 dialogs |
| 2026-10-14 | trivial | Command registry: 228 (`comment.next`, `comment.prev`) |
| 2026-10-14 | trivial | Photo Album dialog and `insert.photoAlbum` (227 commands, 28 dialogs) |
| 2026-10-14 | trivial | Edit Photo Album (`insert.photoAlbumEdit`) reopens the Photo Album dialog and re-flows those slides (228 commands, 28 dialogs) |
| 2026-10-14 | minor | Create New Theme Colors (`design.colors` with no params); command count stays 226 |
| 2026-10-14 | minor | Create New Theme Fonts (`design.fonts` with no params); command count stays 226 |
| 2026-10-14 | minor | Insert › Date and Time dialog (`insert.dateTime` with no keys). 28 dialogs; command count stays 226 |
| 2026-10-14 | minor | Command registry is 227 (`design.effects`). Dialogs stay 27. The test-count line stays 504 |
| 2026-10-14 | minor | Command registry counted at 236, including `master.preserve` and `master.placeholders`. Dialogs stay 27. The test-count line stays 504. The menu measurement and the d556ff9 snapshot still count 226 |
| 2026-10-13 | trivial | Animation preview draws the current slide's timeline on the editor canvas |
| 2026-10-14 | minor | Paragraph and Tabs dialogs; command registry 229; 29 dialogs |
| 2026-10-14 | minor | Font dialog; command registry 227; 28 dialogs |
| 2026-10-13 | minor | Transitions: perspective meshes, background/content parts, CPU compositor |
| 2026-10-10 | major | Created from the code on main (d556ff9) and AGENTS.md §4 |

The PDF writer dependency enables only `write`. Krilla handles serialization and simple-text
shaping for notes and footers; slide layout, rasterization, font selection, text mapping,
links and outlines remain app-owned. PDF embedding and all Hayro production dependencies
are disabled. Native builds enable parallel compression on the same pinned Krilla backend;
WASM keeps it disabled. Generated-PDF assertions use a std-only parser as a development dependency.
