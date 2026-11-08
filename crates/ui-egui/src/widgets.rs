//! Ribbon widgets: large and small buttons, split buttons, groups, dropdowns, spinners, the colour
//! picker and galleries.

use deckcraft_color::{ColorScheme, Rgba, SchemeSlot};
use deckcraft_model::ColorRef;
use egui::{Align2, Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};

/// Height of the ribbon's content area.
pub const RIBBON_H: f32 = 66.0;

fn bg(ui: &Ui, r: &Response, selected: bool, t: &Tokens) -> Option<Color32> {
    if !r.sense.senses_click() && !r.sense.senses_drag() {
        return None;
    }
    if selected {
        Some(t.selected)
    } else if r.is_pointer_button_down_on() {
        Some(t.pressed)
    } else if r.hovered() && ui.is_enabled() {
        Some(t.hover)
    } else {
        None
    }
}

/// A large ribbon button: 32 px icon over a one- or two-line label.
pub fn big_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    big_button_ex(ui, icon, label, enabled, false, false).0
}

/// Width of the dropdown target at the right of a large split button. It is reserved outside the
/// whole label area, so no visible line of a one- or two-line label can open the menu.
const SPLIT_ARROW_W: f32 = 14.0;

/// Return the chevron-only sub-rectangle at the right edge of a split button.
fn split_arrow_rect(rect: Rect) -> Rect {
    Rect::from_min_max(pos2(rect.max.x - SPLIT_ARROW_W, rect.min.y + 38.0), rect.max)
}

/// Width of a large button: padding plus, for a split button, the reserved arrow strip.
fn big_button_width(text_w: f32, split: bool) -> f32 {
    (text_w + 10.0 + if split { SPLIT_ARROW_W } else { 0.0 }).max(40.0)
}

/// Large button; with `split`, the label row ends in a separate dropdown target (returns its
/// response). Like Office's split buttons, the icon and the label text run the main command; only
/// the chevron at the end of the label row opens the menu.
pub fn big_button_ex(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, split: bool, selected: bool) -> (Response, Option<Response>) {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(11.0);
    let lines: Vec<&str> = wrap_label(label);
    let text_w = lines.iter().map(|l| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), t.text).size().x)).fold(0.0f32, f32::max);
    let w = big_button_width(text_w, split);
    // The whole button runs the main command; the narrow strip at the end of the label row is the
    // dropdown. A split button therefore has two named targets, not a third unnamed row target.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, RIBBON_H - 2.0), if enabled { Sense::click() } else { Sense::hover() });
    // Named for screen readers and UI tests ("Ink to\nShape" reads "Ink to Shape").
    let spoken = label.replace('\n', " ");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &spoken));
    let painter = ui.painter_at(rect.expand(1.0));
    let arrow_rect = split.then(|| split_arrow_rect(rect));
    let main_resp = resp.clone();
    let arrow_resp = arrow_rect.map(|r| ui.interact(r, resp.id.with("arrow"), if enabled { Sense::click() } else { Sense::hover() }));
    if split && let Some(a) = &arrow_resp {
        a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{spoken} options")));
    }
    if let Some(c) = bg(ui, &main_resp, selected, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    if let (Some(ar), Some(a)) = (arrow_rect, &arrow_resp)
        && let Some(c) = bg(ui, a, false, &t)
    {
        painter.rect_filled(ar, CornerRadius::same(4), c);
    }
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.min.y + 19.0), vec2(30.0, 30.0));
    icons::paint(&painter, icon_rect, icon, t.text, !enabled);
    let col = if enabled { t.text } else { t.text_faint };
    let label_rect = if split { Rect::from_min_max(rect.min, pos2(rect.max.x - SPLIT_ARROW_W, rect.max.y)) } else { rect };
    let mut y = rect.min.y + 39.0;
    for l in &lines {
        let label_w = ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), col).size().x);
        let x = label_rect.center().x - label_w / 2.0;
        painter.text(pos2(x, y), Align2::LEFT_TOP, l, font.clone(), col);
        y += 12.5;
    }
    if split {
        // The chevron sits in the reserved strip beside the label's last line, never on its text.
        let chevron_y = rect.min.y + 39.0 + 12.5 * lines.len().saturating_sub(1) as f32 + 7.0;
        chevron(&painter, pos2(rect.max.x - 7.0, chevron_y), 3.0, col);
    }
    (if split { main_resp } else { resp }, arrow_resp)
}

fn wrap_label(label: &str) -> Vec<&str> {
    if let Some((a, b)) = label.split_once('\n') {
        return vec![a, b];
    }
    let words: Vec<&str> = label.split(' ').collect();
    if words.len() >= 2 && label.chars().count() > 8 {
        // Break near the middle.
        let mut best = 1;
        let mut best_diff = usize::MAX;
        for k in 1..words.len() {
            let a: usize = words.iter().take(k).map(|w| w.chars().count() + 1).sum();
            let b: usize = words.iter().skip(k).map(|w| w.chars().count() + 1).sum();
            let d = a.abs_diff(b);
            if d < best_diff {
                best_diff = d;
                best = k;
            }
        }
        let split_at: usize = words.iter().take(best).map(|w| w.len() + 1).sum::<usize>().saturating_sub(1);
        if let (Some(a), Some(b)) = (label.get(..split_at), label.get(split_at + 1..)) {
            return vec![a, b];
        }
    }
    vec![label]
}

pub fn chevron(p: &egui::Painter, c: egui::Pos2, s: f32, col: Color32) {
    p.line_segment([pos2(c.x - s, c.y - s * 0.5), pos2(c.x, c.y + s * 0.5)], Stroke::new(1.2, col));
    p.line_segment([pos2(c.x, c.y + s * 0.5), pos2(c.x + s, c.y - s * 0.5)], Stroke::new(1.2, col));
}

/// A small ribbon button: 16 px icon + label in one row (label may be empty).
pub fn small_button(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool) -> Response {
    small_button_ex(ui, icon, label, enabled, selected, false).0
}

/// Draw a small named button, optionally with a separately named options chevron, and return
/// both click targets.
pub fn small_button_ex(ui: &mut Ui, icon: Icon, label: &str, enabled: bool, selected: bool, split: bool) -> (Response, Option<Response>) {
    small_button_named(ui, icon, label, if label.is_empty() { "Options" } else { label }, enabled, selected, split)
}

/// A small button whose accessible name is independent of its visible label. Split arrows are
/// named after their own command, not a shared "More options", so assistive technology can tell
/// Bullets, Numbering and the shape-format controls apart.
pub fn small_button_named(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    name: &str,
    enabled: bool,
    selected: bool,
    split: bool,
) -> (Response, Option<Response>) {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(12.0);
    let tw = if label.is_empty() { 0.0 } else { ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), t.text).size().x) + 5.0 };
    let arrow_w = if split { 12.0 } else { 0.0 };
    let w = 24.0 + tw + arrow_w;
    // As in `big_button_ex`, a split row is two targets and must not add a third named node.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), if enabled && !split { Sense::click() } else { Sense::hover() });
    if !name.is_empty() && !split {
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    }
    let painter = ui.painter_at(rect);
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - arrow_w, rect.max.y));
    let main_resp =
        if split { ui.interact(main_rect, resp.id.with("m"), if enabled { Sense::click() } else { Sense::hover() }) } else { resp.clone() };
    let arrow_resp = split.then(|| {
        ui.interact(
            Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max),
            resp.id.with("a"),
            if enabled { Sense::click() } else { Sense::hover() },
        )
    });
    if split {
        main_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
        if let Some(a) = &arrow_resp {
            a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{name} options")));
        }
    }
    if let Some(c) = bg(ui, &main_resp, selected, &t) {
        painter.rect_filled(main_rect, CornerRadius::same(4), c);
    }
    if let Some(a) = &arrow_resp
        && let Some(c) = bg(ui, a, false, &t)
    {
        painter.rect_filled(Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max), CornerRadius::same(4), c);
    }
    icons::paint(&painter, Rect::from_center_size(pos2(rect.min.x + 12.0, rect.center().y), vec2(17.0, 17.0)), icon, t.text, !enabled);
    if !label.is_empty() {
        painter.text(pos2(rect.min.x + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, if enabled { t.text } else { t.text_faint });
    }
    if split {
        chevron(&painter, pos2(rect.max.x - 6.0, rect.center().y), 2.8, t.text_dim);
    }
    (if split { main_resp } else { resp }, arrow_resp)
}

/// Open one menu from either half of a split control that has no separate main action.
///
/// Both halves keep their own names for assistive technology, but the popup has one stable id
/// and one anchor spanning the complete painted control. Clicking the icon/label or the chevron
/// therefore opens and closes the same menu, and neither named target is a dead end.
pub fn split_menu(main: &Response, arrow: &Response, add: impl FnOnce(&mut Ui)) {
    split_popup(main, arrow).show(|ui| add(ui));
}

/// The shared popup behind [`split_menu`], for callers that adjust its width or close behavior.
pub fn split_popup<'a>(main: &'a Response, arrow: &'a Response) -> egui::Popup<'a> {
    // Pointer clicks read Context input, so resolve them before taking the data lock below.
    let main_clicked = main.clicked();
    let arrow_clicked = arrow.clicked();
    let toggle = (main_clicked || arrow_clicked).then_some(egui::SetOpenCommand::Toggle);
    let id = split_popup_id(main);
    // The popup's geometry and identity are shared, but keyboard entry belongs to the half that
    // opened it. Remember that origin while focus moves into the menu (or the Eyedropper runs).
    let anchor = main.ctx.data_mut(|data| {
        let key = id.with("anchor");
        if arrow_clicked {
            data.insert_temp(key, arrow.id);
        } else if main_clicked {
            data.insert_temp(key, main.id);
        }
        data.get_temp::<egui::Id>(key).unwrap_or(main.id)
    });
    egui::Popup::menu(main).id(id).anchor(main.rect.union(arrow.rect)).anchor_widget(anchor).open_memory(toggle)
}

/// A dropdown-only split whose menu contains a colour grid, retaining the Eyedropper hand-back.
pub fn split_color_popup<'a>(main: &'a Response, arrow: &'a Response) -> egui::Popup<'a> {
    let id = split_popup_id(main);
    let mut popup = split_popup(main, arrow).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    if crate::eyedropper::result_waits_for(&main.ctx, id) {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    if popup.is_open() {
        crate::eyedropper::note_open_menu(&main.ctx, id);
    }
    popup
}

/// Derive the popup id from the main button response so a split menu follows that button.
fn split_popup_id(main: &Response) -> egui::Id {
    main.id.with("split-menu")
}

/// A menu choice ticked in the icon column (Align to Slide…), its text lined up with the
/// [`small_button`] items around it.
pub fn check_item(ui: &mut Ui, label: &str, enabled: bool, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let font = theme::font(12.0);
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), t.text).size().x) + 5.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0 + tw, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, on, label));
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &resp, false, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    if on {
        icons::paint(&painter, Rect::from_center_size(pos2(rect.min.x + 12.0, rect.center().y), vec2(17.0, 17.0)), Icon::Check, t.text, !enabled);
    }
    painter.text(pos2(rect.min.x + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, if enabled { t.text } else { t.text_faint });
    resp
}

/// A square icon toggle (B, I, U, alignment…).
pub fn icon_toggle(ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, on: bool) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &resp, on, &t) {
        painter.rect_filled(rect, CornerRadius::same(4), c);
    }
    icons::paint(&painter, rect.shrink(3.0), icon, t.text, !enabled);
    // Named for screen readers (and tests) by its tooltip.
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    resp.on_hover_text(tip)
}

/// Icon with a colour bar under it and a dropdown arrow (Font Color, Highlight, Shape Fill…).
pub fn color_split(ui: &mut Ui, icon: Icon, tip: &str, color: Color32, enabled: bool) -> (Response, Response) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 22.0), Sense::hover());
    let main = ui.interact(Rect::from_min_size(rect.min, vec2(23.0, 22.0)), resp.id.with("m"), if enabled { Sense::click() } else { Sense::hover() });
    main.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    let arrow = ui.interact(
        Rect::from_min_max(pos2(rect.min.x + 23.0, rect.min.y), rect.max),
        resp.id.with("a"),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    arrow.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{tip} options")));
    let painter = ui.painter_at(rect);
    if let Some(c) = bg(ui, &main, false, &t) {
        painter.rect_filled(main.rect, CornerRadius::same(4), c);
    }
    if let Some(c) = bg(ui, &arrow, false, &t) {
        painter.rect_filled(arrow.rect, CornerRadius::same(4), c);
    }
    icons::paint(&painter, Rect::from_min_size(rect.min + vec2(3.0, 1.0), vec2(17.0, 15.0)), icon, t.text, !enabled);
    painter.rect_filled(
        Rect::from_min_size(rect.min + vec2(4.0, 17.0), vec2(15.0, 3.5)),
        CornerRadius::ZERO,
        if enabled { color } else { t.text_faint },
    );
    chevron(&painter, pos2(rect.max.x - 5.5, rect.center().y), 2.6, t.text_dim);
    (main.on_hover_text(tip), arrow)
}

/// A ribbon group: contents, then a thin vertical separator.
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
        add(ui)
    });
    separator(ui);
    r.inner
}

pub fn separator(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(13.0, RIBBON_H - 6.0), Sense::hover());
    ui.painter().line_segment([pos2(rect.center().x, rect.min.y + 6.0), pos2(rect.center().x, rect.max.y - 4.0)], Stroke::new(1.0, t.separator));
}

/// Two or three rows of small controls stacked in one ribbon column.
pub fn rows<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        ui.add_space(0.0);
        add(ui)
    })
    .inner
}

/// A text dropdown field (font name, size): shows `current`, opens a popup with `add_items`.
pub fn dropdown(ui: &mut Ui, id: &str, current: &str, width: f32, enabled: bool, add_items: impl FnOnce(&mut Ui)) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(4),
        t.field_bg,
        Stroke::new(1.0, if resp.hovered() { t.text_faint } else { t.border }),
        egui::StrokeKind::Inside,
    );
    let clip = rect.shrink2(vec2(6.0, 0.0));
    painter.with_clip_rect(Rect::from_min_max(clip.min, pos2(clip.max.x - 12.0, clip.max.y))).text(
        pos2(rect.min.x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        current,
        theme::font(12.0),
        if enabled { t.text } else { t.text_faint },
    );
    chevron(&painter, pos2(rect.max.x - 9.0, rect.center().y), 3.0, t.text_dim);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, if current.is_empty() { id } else { current }));
    let _ = id;
    egui::Popup::menu(&resp).width(width.max(140.0)).show(|ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, add_items);
    });
    resp
}

/// The drop-down for a `color_grid`: a menu that stays open when its hex field is clicked
/// (egui menus close on any click inside). Picking a colour closes it (`color_grid` calls
/// `ui.close()`); other buttons in it must close it themselves.
pub fn color_popup(button: &Response) -> egui::Popup<'_> {
    let mut popup = egui::Popup::menu(button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    // A colour the Eyedropper took for this menu: open it so `color_grid` hands the colour back.
    if crate::eyedropper::result_waits_for(&button.ctx, button.id) {
        popup = popup.open_memory(egui::SetOpenCommand::Bool(true));
    }
    if popup.is_open() {
        crate::eyedropper::note_open_menu(&button.ctx, button.id);
    }
    popup
}

/// The colour grid popup body: theme colours with tints/shades, standard colours, extra entries.
/// Returns the picked colour (`Some(None)` = "No Fill"/"Automatic" when `none_label` is given).
pub fn color_grid(ui: &mut Ui, scheme: &ColorScheme, none_label: Option<&str>) -> Option<Option<ColorRef>> {
    // The Eyedropper's colour, handed back as if its swatch had been clicked.
    if let Some(c) = crate::eyedropper::take_result(ui.ctx()).and_then(|hex| Rgba::from_hex(&hex)) {
        ui.close();
        return Some(Some(ColorRef::rgb(c)));
    }
    let t = Tokens::get(ui.ctx());
    let mut out = None;
    ui.set_min_width(220.0);
    ui.label(egui::RichText::new("Theme Colors").font(theme::bold(12.0)));
    let slots = [
        SchemeSlot::Lt1,
        SchemeSlot::Dk1,
        SchemeSlot::Lt2,
        SchemeSlot::Dk2,
        SchemeSlot::Accent1,
        SchemeSlot::Accent2,
        SchemeSlot::Accent3,
        SchemeSlot::Accent4,
        SchemeSlot::Accent5,
        SchemeSlot::Accent6,
    ];
    let cell = 18.0;
    let sw = |ui: &mut Ui, c: Rgba, tip: &str| -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
        ui.painter().rect(
            r.shrink(1.0),
            CornerRadius::same(2),
            theme::to_color32(c),
            Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { t.accent } else { t.border }),
            egui::StrokeKind::Inside,
        );
        resp.on_hover_text(tip).clicked()
    };
    egui::Grid::new("theme_colors").spacing(vec2(3.0, 1.0)).show(ui, |ui| {
        for slot in slots {
            let alias = match slot {
                SchemeSlot::Lt1 => SchemeSlot::Bg1,
                SchemeSlot::Dk1 => SchemeSlot::Tx1,
                SchemeSlot::Lt2 => SchemeSlot::Bg2,
                SchemeSlot::Dk2 => SchemeSlot::Tx2,
                s => s,
            };
            if sw(ui, scheme.get(slot), slot.label()) {
                out = Some(Some(ColorRef::scheme(alias)));
            }
        }
        ui.end_row();
        for row in 0..5 {
            for slot in slots {
                let base = scheme.get(slot);
                let vars = deckcraft_color::theme_variants(base);
                let Some((mods, label)) = vars.get(row) else { continue };
                let c = deckcraft_color::apply(base, mods);
                if sw(ui, c, label) {
                    let alias = match slot {
                        SchemeSlot::Lt1 => SchemeSlot::Bg1,
                        SchemeSlot::Dk1 => SchemeSlot::Tx1,
                        SchemeSlot::Lt2 => SchemeSlot::Bg2,
                        SchemeSlot::Dk2 => SchemeSlot::Tx2,
                        s => s,
                    };
                    let mut r = ColorRef::scheme(alias);
                    r.mods = mods.clone();
                    out = Some(Some(r));
                }
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Standard Colors").font(theme::bold(12.0)));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (c, name) in deckcraft_color::STANDARD_COLORS {
            if sw(ui, c, name) {
                out = Some(Some(ColorRef::rgb(c)));
            }
        }
    });
    ui.separator();
    if let Some(n) = none_label
        && ui.button(n).clicked()
    {
        out = Some(None);
    }
    // "More Colors…": a hex field.
    let id = ui.scope_id().with("hex");
    let mut hex: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.label("More Colors… #");
        let r = ui.add(egui::TextEdit::singleline(&mut hex).desired_width(70.0).hint_text("RRGGBB"));
        if r.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Some(c) = Rgba::from_hex(&hex)
        {
            out = Some(Some(ColorRef::rgb(c)));
        }
    });
    ui.data_mut(|d| d.insert_temp(id, hex));
    if ui.button("Eyedropper").on_hover_text("Click the slide to take the colour under the pointer").clicked() {
        crate::eyedropper::request(ui.ctx());
        ui.close();
    }
    if out.is_some() {
        ui.close();
    }
    out
}

/// A row of gallery tiles drawn by `paint_tile`; returns the clicked index.
pub fn gallery(
    ui: &mut Ui,
    n: usize,
    tile: Vec2,
    selected: Option<usize>,
    mut paint_tile: impl FnMut(&egui::Painter, Rect, usize),
    mut tip: impl FnMut(usize) -> String,
) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    let mut clicked = None;
    for i in 0..n {
        let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
        let painter = ui.painter_at(rect.expand(2.0));
        let sel = selected == Some(i);
        if sel || resp.hovered() {
            painter.rect_filled(rect.expand(1.0), CornerRadius::same(4), if sel { t.selected } else { t.hover });
        }
        paint_tile(&painter, rect.shrink(3.0), i);
        if sel {
            painter.rect_stroke(rect.expand(1.0), CornerRadius::same(4), Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
        if resp.on_hover_text(tip(i)).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// A number field with up/down steppers, named by the visible label of its row. Returns the new
/// value when committed.
pub fn spinner(ui: &mut Ui, id: &str, label: &str, value: f64, step: f64, suffix: &str, width: f32) -> Option<f64> {
    let t = Tokens::get(ui.ctx());
    let key = ui.scope_id().with(id);
    let mut text: String = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| fmt_num(value, suffix));
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).font(theme::font(12.0))).accessible_name(format!("{label} value"));
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, text.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(key));
        }
        if r.lost_focus() {
            let num: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
            if let Ok(v) = num.parse::<f64>() {
                out = Some(v);
            }
        }
        let (rect, _) = ui.allocate_exact_size(vec2(12.0, 22.0), Sense::hover());
        let up = ui.interact(Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y)), key.with("up"), Sense::click());
        up.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Increase {label}")));
        let down = ui.interact(Rect::from_min_max(pos2(rect.min.x, rect.center().y), rect.max), key.with("dn"), Sense::click());
        down.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Decrease {label}")));
        let p = ui.painter();
        let c = rect.center();
        p.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x, c.y - 6.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x, c.y - 6.0), pos2(c.x + 3.0, c.y - 3.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x - 3.0, c.y + 3.0), pos2(c.x, c.y + 6.0)], Stroke::new(1.0, t.text_dim));
        p.line_segment([pos2(c.x, c.y + 6.0), pos2(c.x + 3.0, c.y + 3.0)], Stroke::new(1.0, t.text_dim));
        if up.clicked() {
            out = Some(value + step);
        }
        if down.clicked() {
            out = Some(value - step);
        }
    });
    out
}

pub fn fmt_num(v: f64, suffix: &str) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    format!("{s}{suffix}")
}

/// The text a kept-open text box shows: the typed draft while the stored value is still what the
/// draft started from, otherwise the stored value (another document, a reorder or an undo changed it).
pub fn text_draft(stored: Option<(String, String)>, current: &str) -> String {
    match stored {
        Some((base, buf)) if base == current => buf,
        _ => current.to_string(),
    }
}

/// Section header inside panes.
pub fn section(ui: &mut Ui, title: &str, open: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let p = ui.painter();
    let c = pos2(rect.min.x + 8.0, rect.center().y);
    if open {
        chevron(p, c, 3.5, t.text);
    } else {
        p.line_segment([pos2(c.x - 2.0, c.y - 4.0), pos2(c.x + 2.0, c.y)], Stroke::new(1.2, t.text));
        p.line_segment([pos2(c.x + 2.0, c.y), pos2(c.x - 2.0, c.y + 4.0)], Stroke::new(1.2, t.text));
    }
    p.text(pos2(rect.min.x + 18.0, rect.center().y), Align2::LEFT_CENTER, title, theme::bold(13.0), t.text);
    if resp.clicked() { !open } else { open }
}

/// A button with an optional label and a painted dropdown chevron (the UI font has no ▾).
pub fn drop_button(ui: &mut Ui, label: &str, size: egui::Vec2, enabled: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let r = ui
        .add_enabled(enabled, egui::Button::new(if label.is_empty() { String::new() } else { format!("{label}   ") }).min_size(size))
        .on_hover_text(if label.is_empty() { "Options" } else { label });
    let col = if enabled { t.text_dim } else { t.text_faint };
    chevron(ui.painter(), egui::pos2(r.rect.max.x - 8.0, r.rect.center().y), 3.0, col);
    r
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::cell::RefCell;

    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    /// Office's large split button: the icon and the visible label text run the main command;
    /// only the chevron at the end of the label row opens the options menu. The old geometry made
    /// the whole label row the arrow target, so clicking "Paste" opened its menu instead of
    /// pasting.
    #[test]
    fn a_large_split_buttons_label_runs_the_command_and_its_chevron_opens_the_options() {
        use std::cell::Cell;
        use std::rc::Rc;

        let main_clicks = Rc::new(Cell::new(0u32));
        let arrow_clicks = Rc::new(Cell::new(0u32));
        let (main_counter, arrow_counter) = (main_clicks.clone(), arrow_clicks.clone());
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(200.0, 100.0)).build_ui(move |ui| {
            theme::install_fonts(ui.ctx());
            let (main, arrow) = big_button_ex(ui, Icon::Paste, "Paste", true, true, false);
            if main.clicked() {
                main_counter.set(main_counter.get() + 1);
            }
            if arrow.is_some_and(|a| a.clicked()) {
                arrow_counter.set(arrow_counter.get() + 1);
            }
        });
        harness.run();
        let main_rect = harness.get_by_role_and_label(Role::Button, "Paste").rect();
        let arrow_rect = harness.get_by_role_and_label(Role::Button, "Paste options").rect();
        let label = egui::pos2(main_rect.center().x, main_rect.min.y + 45.0);
        assert!(main_rect.contains(label) && !arrow_rect.contains(label), "the visible label is outside the chevron strip");

        /// Nested large-split-button test helper: move and press/release the primary pointer at a
        /// position.
        fn click(harness: &mut egui_kittest::Harness<'_, ()>, pos: egui::Pos2) {
            harness.input_mut().events.push(egui::Event::PointerMoved(pos));
            harness.run();
            for pressed in [true, false] {
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run();
            }
        }
        click(&mut harness, label);
        assert_eq!(main_clicks.get(), 1, "clicking the label runs the main command");
        assert_eq!(arrow_clicks.get(), 0, "clicking the label does not open the menu");

        click(&mut harness, arrow_rect.center());
        assert_eq!(main_clicks.get(), 1, "clicking the chevron does not run the main command");
        assert_eq!(arrow_clicks.get(), 1, "clicking the chevron opens the options");
    }

    /// A two-line split label reserves the arrow strip outside *all* visible text. The click
    /// lands on the last line's widest glyph pixels (the old overlap) and runs the main command;
    /// only the named arrow opens the menu.
    #[test]
    fn a_two_line_split_labels_last_text_pixels_run_the_command() {
        use std::cell::Cell;
        use std::rc::Rc;

        let main_clicks = Rc::new(Cell::new(0u32));
        let arrow_clicks = Rc::new(Cell::new(0u32));
        let last_line = Rc::new(RefCell::new(None));
        let (main_counter, arrow_counter) = (main_clicks.clone(), arrow_clicks.clone());
        let (last_line_out, last_line_cell) = (last_line.clone(), last_line.clone());
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(220.0, 100.0)).build_ui(move |ui| {
            theme::install_fonts(ui.ctx());
            let label = "Send\nBackward";
            let (main, arrow) = big_button_ex(ui, Icon::SendBackward, label, true, true, false);
            let font = theme::font(11.0);
            let col = Tokens::get(ui.ctx()).text;
            let lines = wrap_label(label);
            let rect = main.rect;
            let label_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - SPLIT_ARROW_W, rect.max.y));
            let last = lines.last().copied().unwrap_or_default();
            let last_w = ui.fonts_mut(|f| f.layout_no_wrap(last.to_string(), font.clone(), col).size().x);
            let y = rect.min.y + 39.0 + 12.5 * lines.len().saturating_sub(1) as f32;
            let x = label_rect.center().x - last_w / 2.0;
            *last_line_cell.borrow_mut() = Some(Rect::from_min_size(pos2(x, y), vec2(last_w, 12.0)));
            if main.clicked() {
                main_counter.set(main_counter.get() + 1);
            }
            if arrow.is_some_and(|a| a.clicked()) {
                arrow_counter.set(arrow_counter.get() + 1);
            }
        });
        harness.run();
        let arrow_rect = harness.get_by_role_and_label(Role::Button, "Send Backward options").rect();
        let text = last_line_out.borrow().expect("last line laid out");
        assert!(text.max.x < arrow_rect.min.x, "the reserved strip is outside all visible text: {text:?} vs {arrow_rect:?}");
        let label_pixels = pos2(text.max.x - 1.0, text.center().y);
        assert!(!arrow_rect.contains(label_pixels));

        /// Nested two-line-split-button test helper: move and press/release the primary pointer at a
        /// position.
        fn click(harness: &mut egui_kittest::Harness<'_, ()>, pos: egui::Pos2) {
            harness.input_mut().events.push(egui::Event::PointerMoved(pos));
            harness.run();
            for pressed in [true, false] {
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run();
            }
        }
        click(&mut harness, label_pixels);
        assert_eq!(main_clicks.get(), 1, "the last line's real text pixels run the main command");
        assert_eq!(arrow_clicks.get(), 0, "the last line's text does not open the menu");

        click(&mut harness, arrow_rect.center());
        assert_eq!(main_clicks.get(), 1, "the arrow does not run the main command");
        assert_eq!(arrow_clicks.get(), 1, "the arrow opens the options");
    }

    /// Small split arrows carry their own command name; bullets and numbering must not both be
    /// announced as a generic "More options" target.
    #[test]
    fn small_split_arrows_are_named_for_their_own_commands() {
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 100.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let _ = small_button_named(ui, Icon::Bullets, "", "Bullets", true, false, true);
            let _ = small_button_named(ui, Icon::Numbering, "", "Numbering", true, false, true);
        });
        harness.run();
        for name in ["Bullets", "Bullets options", "Numbering", "Numbering options"] {
            assert!(harness.get_by_role_and_label(Role::Button, name).rect().width() > 0.0, "{name} is a named target");
        }
        assert!(harness.query_by_role_and_label(Role::Button, "More options").is_none(), "no shared arrow name remains");
    }

    /// The custom ribbon dropdown is a real combo box in the accessibility tree, named by its
    /// current value (0.37's strict unnamed-input check would otherwise reject it).
    #[test]
    fn the_dropdown_has_a_combo_box_role_and_a_name() {
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 200.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            dropdown(ui, "font-size", "18 pt", 120.0, true, |ui| {
                ui.label("12 pt");
                ui.label("18 pt");
            });
        });
        harness.run();
        let combo = harness.get_by_role_and_label(Role::ComboBox, "18 pt");
        assert!(combo.rect().width() > 100.0);
    }

    /// #32: clicking the "More Colors" hex field closed the colour menu before anything could be
    /// typed. It stays open now; Enter picks the colour and closes it.
    #[test]
    fn the_hex_field_keeps_the_color_menu_open() {
        let picked = RefCell::new(None);
        let scheme = deckcraft_model::Theme::default().colors;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 500.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let r = ui.button("Pick");
            color_popup(&r).show(|ui| {
                if let Some(c) = color_grid(ui, &scheme, None) {
                    *picked.borrow_mut() = Some(c);
                }
            });
        });
        harness.get_by_label("Pick").click();
        harness.run();
        harness.get_by_role(Role::TextInput).click();
        harness.run();
        assert!(harness.query_by_role(Role::TextInput).is_some(), "the menu is still open after clicking the hex field");
        harness.get_by_role(Role::TextInput).type_text("00FF00");
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(*picked.borrow(), Some(Some(ColorRef::rgb(Rgba::from_hex("00FF00").unwrap()))));
        assert!(harness.query_by_role(Role::TextInput).is_none(), "picking a colour closes the menu");
    }

    /// Choosing Eyedropper closes the menu and asks for it; the colour it takes comes back through
    /// the same menu, as if its swatch had been clicked.
    #[test]
    fn the_eyedroppers_colour_comes_back_through_the_menu_that_asked() {
        let picked = RefCell::new(None);
        let scheme = deckcraft_model::Theme::default().colors;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(400.0, 500.0)).build_ui(|ui| {
            theme::install_fonts(ui.ctx());
            let r = ui.button("Pick");
            color_popup(&r).show(|ui| {
                if let Some(c) = color_grid(ui, &scheme, None) {
                    *picked.borrow_mut() = Some(c);
                }
            });
        });
        harness.get_by_label("Pick").click();
        harness.run();
        harness.get_by_label("Eyedropper").click();
        harness.run();
        assert!(harness.query_by_label("Eyedropper").is_none(), "choosing it closes the menu");
        assert!(picked.borrow().is_none(), "nothing is picked yet");
        // The app starts the Eyedropper from this request; here the colour is taken at once.
        assert!(harness.ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("deckcraft.eyedropper.request"))).unwrap_or(false));
        let menu = crate::eyedropper::open_menu(&harness.ctx).expect("the open menu noted its button");
        crate::eyedropper::store_result(&harness.ctx, menu, "00FF00");
        harness.run();
        assert_eq!(*picked.borrow(), Some(Some(ColorRef::rgb(Rgba::from_hex("00FF00").unwrap()))));
        assert!(harness.query_by_label("Eyedropper").is_none(), "the menu closes again");
        harness.run();
        assert_eq!(picked.borrow().iter().count(), 1, "the colour is handed back once");
    }

    /// #413: a draft is kept while the stored text is unchanged ...
    #[test]
    fn text_draft_keeps_typing_while_the_stored_text_is_unchanged() {
        let stored = Some(("old".to_string(), "old plus typing".to_string()));
        assert_eq!(text_draft(stored, "old"), "old plus typing");
    }

    /// ... and dropped when the stored text changes underneath it.
    #[test]
    fn text_draft_drops_a_stale_draft_when_the_stored_text_changes() {
        let stored = Some(("old".to_string(), "old plus typing".to_string()));
        assert_eq!(text_draft(stored, "undone"), "undone");
        assert_eq!(text_draft(None, "fresh"), "fresh");
    }
}

/// Touch arbitration for draggable rows in scrollable lists (the slide sorter and friends): a
/// finger press on a row can mean scroll (vertical pan), grab (hold, then drag to reorder),
/// hold (the row's long-press menu) or tap (the row's click). The arbiter owns the press while
/// the finger is down and pans the enclosing scroll area while the press is a scroll, so the
/// row only has to skip its own drag and click while the finger scrolls. Copied from
/// FigCraft's `widgets::touch_row` (same port series, sibling copying as house rules allow).
mod touch_arbiter {
    /// The distance a finger may wobble before the press is no longer a click (egui's
    /// `max_click_dist` is 6.0 too; a constant keeps the arbiter self-contained).
    pub(super) const MOVED: f32 = 6.0;
    /// The hold a finger keeps before a move becomes a drag instead of a scroll. Below egui's
    /// own long-press timeout (0.8 s), so holding and then dragging beats the context menu.
    pub(super) const HOLD: f64 = 0.55;
}
use touch_arbiter::{HOLD, MOVED};

/// What a finger's press turned out to mean. Latched for the rest of the press, so a scroll
/// never turns back into a grab because the finger later travelled past the hold.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TouchKind {
    /// Not settled yet: still a candidate for a click, a scroll, the menu and a drag.
    Undecided,
    /// The finger moved before the hold expired: the enclosing list is scrolling.
    Scroll,
    /// The hold expired with the finger still: egui's long-press is on the press. A move before
    /// it opens the row's menu turns the hold into the row's drag, a move after it does not.
    Held,
    /// The hold expired and the finger then moved: the row's drag.
    Grab,
}

/// Which scroll area a finger press started in and where: temp data, gone after the lift. The
/// area id is the gesture's identity (and gates the pan: one finger pans one area).
#[derive(Clone, Copy, Debug, PartialEq)]
struct TouchPress {
    /// Identity of the enclosing scroll area (its `id_salt`).
    area: egui::Id,
    /// The row the finger went down on: only that row may act on the press (every other row of
    /// the area asks too, and gets no verdict).
    row: egui::Id,
    /// Where the finger went down.
    origin: egui::Pos2,
    /// What the finger meant.
    kind: TouchKind,
    /// Whether egui's long-press opened the row's context menu during this press: under an open
    /// menu a finger moving on must not pick the row up.
    menu: bool,
}

fn touch_press_id() -> egui::Id {
    egui::Id::unique("deckcraft-touch-row-press")
}

/// The (pass, area) that already spent this pass's pan.
fn touch_pan_id() -> egui::Id {
    egui::Id::unique("deckcraft-touch-row-pan")
}

/// Whether this pass has already spent the finger's pan on `area`. Every row of a scroll area
/// consults [`touch_row`], and the scroll delta *accumulates* — one pan per pass, not one per
/// row.
fn panned_this_pass(ctx: &egui::Context, pass: u64, area: egui::Id) -> bool {
    if ctx.data(|d| d.get_temp::<(u64, egui::Id)>(touch_pan_id())) == Some((pass, area)) {
        return true;
    }
    ctx.data_mut(|d| d.insert_temp(touch_pan_id(), (pass, area)));
    false
}

/// What the arbiter says this frame about a row's press (see the `touch_arbiter` module).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TouchRow {
    /// No arbitrated press this frame (a mouse press, or none).
    None,
    /// A finger press neither held long enough nor moved: keep waiting.
    Pressing,
    /// The finger moved before the hold expired: the row must not grab; the list is scrolling.
    Scroll,
    /// The hold expired with the finger still: the row must not grab; egui's own long-press
    /// opens the row's context menu.
    Hold,
    /// The finger held and then moved: the row grabs now and keeps its drag while the finger is
    /// down.
    Grab,
}

/// A draggable row consults this every frame so a finger can scroll instead of picking it up.
///
/// `row` is the row's own response (only the row the press began on may act on it), `area` an
/// identity of the enclosing scroll area (its `id_salt`). While the press is a scroll the
/// helper pans that area with the pointer delta; the row only needs to skip starting its drag
/// and its click. A mouse press is never arbitrated, so desktop behaviour is untouched.
pub fn touch_row(ui: &Ui, row: &Response, area: egui::Id) -> TouchRow {
    let ctx = ui.ctx().clone();
    // All reads up front (no nested `data`/`input` calls — those lock for write):
    let (touch, pressed, down, origin, press_start, pos, delta, time) = ctx.input(|i| {
        (
            i.any_touches(),
            i.pointer.primary_pressed(),
            i.pointer.any_down(),
            i.pointer.press_origin(),
            i.pointer.press_start_time(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
            i.pointer.delta(),
            i.time,
        )
    });
    let pass = ctx.cumulative_pass_nr();
    let press = ctx.data(|d| d.get_temp::<Option<TouchPress>>(touch_press_id())).flatten();
    let press = match press {
        Some(p) => Some(p),
        None if touch && pressed && origin.is_some_and(|o| row.rect.contains(o)) => {
            let p = TouchPress { area, row: row.id, origin: origin.unwrap_or_else(|| row.rect.center()), kind: TouchKind::Undecided, menu: false };
            ctx.data_mut(|d| d.insert_temp(touch_press_id(), Some(p)));
            Some(p)
        }
        None => None,
    };
    let Some(press) = press else { return TouchRow::None };

    if !down {
        // The finger lifted this frame (egui keeps interact_pos through a touch release, and
        // the click answers from the row's own response): the arbitration is over.
        ctx.data_mut(|d| d.remove::<Option<TouchPress>>(touch_press_id()));
        return TouchRow::None;
    }
    if !touch {
        // A mouse pressed while a press was arbitrated (or the touch state lost): this frame
        // adds no meaning.
        return TouchRow::None;
    }
    let Some(pos) = pos else {
        // A `PointerGone` while held (a touch release is such a frame): keep waiting.
        return TouchRow::Pressing;
    };
    // Settle the press once: the finger decides what it meant by moving, and only *when* it
    // moved against the hold says which — early is a scroll, after the hold is the row's drag.
    // A finger that never moves leaves the press to egui, whose long-press opens the row's
    // context menu; a move after that menu opened must not pick the row up under it.
    let held = press_start.is_some_and(|t| time - t > HOLD);
    let moved = press.origin.distance(pos) > MOVED;
    let menu = row.long_touched() || press.menu;
    let kind = match press.kind {
        TouchKind::Undecided if moved => {
            if held {
                TouchKind::Grab
            } else {
                TouchKind::Scroll
            }
        }
        TouchKind::Undecided if held => TouchKind::Held,
        TouchKind::Held if moved && !menu => TouchKind::Grab,
        settled => settled,
    };
    if kind != press.kind || menu != press.menu {
        let settled = TouchPress { kind, menu, ..press };
        ctx.data_mut(|d| d.insert_temp(touch_press_id(), Some(settled)));
    }
    if kind == TouchKind::Scroll {
        // A scrolling finger: hand this pass's pointer delta to the enclosing scroll area —
        // instantly, the drag *is* the scroll. The content follows the finger: a finger moving
        // up (negative delta.y) scrolls down, which the area's inverted accounting produces
        // from the same sign. Only the area the press began in pans, and only once per pass.
        if press.area == area && delta != egui::Vec2::ZERO && !panned_this_pass(&ctx, pass, area) {
            ui.scroll_with_delta(delta);
        }
    }
    if press.row != row.id {
        // Another row of the area: it adds no verdict. A hold-then-drag must grab only the row
        // the finger went down on, never every visible row it passes. The pan above still runs
        // here, so the area keeps following the finger even if the pressed row scrolled away.
        return TouchRow::None;
    }
    match kind {
        TouchKind::Scroll => TouchRow::Scroll,
        TouchKind::Grab => TouchRow::Grab,
        TouchKind::Held => TouchRow::Hold,
        TouchKind::Undecided => TouchRow::Pressing,
    }
}

/// Widen the scroll bars' hit band to a fingertip's 24 points on a touch screen. Called on the
/// `Ui` whose scroll bars draw, once per pass: the band is what a finger aims at, and egui's
/// own touch detection (the first `Event::Touch`) says when to widen it. Mouse hosts keep the
/// theme's band.
pub fn touch_scroll_target(ui: &mut Ui) {
    if ui.input(|i| i.has_touch_screen()) && ui.spacing().scroll.bar_width < 24.0 {
        ui.spacing_mut().scroll.bar_width = 24.0;
    }
}

/// Force drag-to-scroll on a scrollable body: a finger dragged inside it pans it. egui's
/// default (`DragScroll::OnTouch`) already pans once touch is detected, but the force removes
/// the detection dependency, so panels, dialogs and the file browser scroll the same way on
/// every host — and the test harness exercises it like a finger does.
pub fn panel_scroll(sa: egui::ScrollArea) -> egui::ScrollArea {
    sa.scroll_source(egui::containers::scroll_area::ScrollSource { drag: egui::containers::scroll_area::DragScroll::Always, ..Default::default() })
}

#[cfg(test)]
mod touch_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{TouchRow, panel_scroll, touch_row, touch_scroll_target};

    /// A scrollable area whose rows consult `touch_row`, fed the events an Android backend
    /// sends (an `Event::Touch` plus the emulated pointer events, a `PointerGone` when the
    /// finger lifts). `answers` stacks what row 7 was told each frame; `offset_y` the area's
    /// offset after each frame.
    struct Bench {
        answers: Vec<TouchRow>,
        offset_y: Vec<f32>,
        row7: egui::Pos2,
        /// The row gap below row 7: a drag there is a drag of the body itself, not of a widget,
        /// which is what drag-to-scroll answers to.
        gap: egui::Pos2,
        always: bool,
    }

    fn frame(b: &mut Bench, ctx: &egui::Context, events: Vec<egui::Event>, at: f64) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
        let area = egui::Id::unique("deckcraft-bench-scroll");
        let warm = events.is_empty();
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(at), events, ..Default::default() };
        let mut offset = 0.0;
        let always = b.always;
        let mut run = ctx.run_ui(input, |ui| {
            let sa = egui::ScrollArea::vertical().id_salt(area).auto_shrink([false, false]);
            let sa = if always { panel_scroll(sa) } else { sa };
            let out = sa.show(ui, |ui| {
                for i in 0..20 {
                    let r = ui.add_sized([ui.available_width(), 18.0], egui::Button::new(format!("row {i}")));
                    let answer = touch_row(ui, &r, area);
                    if i == 7 {
                        b.answers.push(answer);
                        if warm {
                            b.row7 = r.rect.center();
                            b.gap = egui::pos2(r.rect.center().x, r.rect.max.y + 4.0);
                        }
                    }
                }
            });
            offset = out.state.clamped_offset().y;
        });
        run.textures_delta.clear();
        b.offset_y.push(offset);
    }

    fn bench() -> (egui::Context, Bench) {
        (
            egui::Context::default(),
            Bench { answers: Vec::new(), offset_y: Vec::new(), row7: egui::Pos2::ZERO, gap: egui::pos2(150.0, 0.0), always: false },
        )
    }

    fn touch(phase: egui::TouchPhase, pos: egui::Pos2) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos, force: Some(0.5) }
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    /// A finger on a row that drags up before the hold expires: the row sees `Scroll`, never
    /// `Hold`/`Grab`, and the area's offset actually moves (the arbiter pans it).
    #[test]
    fn an_early_finger_move_scrolls_and_does_not_grab() {
        let (ctx, mut b) = bench();
        // A warm-up pass, as the app has run many frames before a finger lands: egui hit-tests
        // the press against the previous pass's widgets, and with none it has no candidate.
        frame(&mut b, &ctx, vec![], -0.1);
        let at = b.row7;
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Start, at), egui::Event::PointerMoved(at), button(at, true)], 0.0);
        assert_eq!(b.answers.last(), Some(&TouchRow::Pressing), "a wait at first: {:?}", b.answers);
        for k in 1..=5 {
            let p = egui::pos2(at.x, at.y - 15.0 * k as f32);
            frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)], 0.1 * k as f64);
        }
        frame(&mut b, &ctx, vec![], 0.6);
        assert!(b.answers.contains(&TouchRow::Scroll), "an early move scrolls: {:?}", b.answers);
        assert!(!b.answers.iter().any(|a| matches!(a, TouchRow::Hold | TouchRow::Grab)), "no grab: {:?}", b.answers);
        assert!(*b.offset_y.last().unwrap() > 20.0, "the area really panned: {:?}", b.offset_y);
    }

    /// A finger held still on a row is told `Hold` and never grabs: egui's own long-press opens
    /// the row's menu. Moving *after* the hold turns it into `Grab`, and the offset stays put.
    #[test]
    fn a_still_hold_is_left_to_the_long_press_and_only_a_move_after_it_grabs() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1);
        let at = b.row7;
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Start, at), egui::Event::PointerMoved(at), button(at, true)], 0.0);
        for k in 1..=6 {
            frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, at), egui::Event::PointerMoved(at)], 0.1 * k as f64);
        }
        assert!(b.answers.contains(&TouchRow::Hold), "a still hold is the menu's: {:?}", b.answers);
        assert!(!b.answers.iter().any(|a| matches!(a, TouchRow::Grab | TouchRow::Scroll)), "nothing else: {:?}", b.answers);
        assert_eq!(*b.offset_y.last().unwrap(), 0.0, "the area never panned: {:?}", b.offset_y);
        for k in 1..=3 {
            let p = egui::pos2(at.x, at.y - 8.0 * k as f32);
            frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)], 0.7 + 0.1 * k as f64);
        }
        assert_eq!(b.answers.last(), Some(&TouchRow::Grab), "a move after the hold grabs: {:?}", b.answers);
        assert_eq!(*b.offset_y.last().unwrap(), 0.0, "a grabbed drag never pans the list: {:?}", b.offset_y);
    }

    /// A mouse press is never arbitrated: the row sees `None` every frame, so its ordinary
    /// `drag_started` answers desktop behaviour untouched.
    #[test]
    fn a_mouse_press_is_never_arbitrated() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1);
        let at = b.row7;
        frame(&mut b, &ctx, vec![egui::Event::PointerMoved(at), button(at, true)], 0.0);
        for k in 1..=5 {
            let p = egui::pos2(at.x, at.y - 15.0 * k as f32);
            frame(&mut b, &ctx, vec![egui::Event::PointerMoved(p)], 0.1 * k as f32 as f64);
        }
        assert!(b.answers.iter().all(|a| *a == TouchRow::None), "a mouse press is none of the arbiter's business: {:?}", b.answers);
    }

    /// On a touch screen the scroll bar's hit band widens to 24 points; on a mouse host it
    /// stays whatever the theme set. Read through the `Ui` whose scroll bars draw with it.
    #[test]
    fn a_touch_screen_widens_the_scroll_bar_hit_band() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
        let mut widened = 0.0;
        let mut run = egui::Context::default().run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![touch(egui::TouchPhase::Start, egui::pos2(100.0, 100.0))],
                ..Default::default()
            },
            |ui| {
                touch_scroll_target(ui);
                widened = ui.spacing().scroll.bar_width;
            },
        );
        run.textures_delta.clear();
        assert_eq!(widened, 24.0, "a fingertip band: {widened}");
        let mut kept = -1.0;
        let mut run = egui::Context::default().run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            touch_scroll_target(ui);
            kept = ui.spacing().scroll.bar_width;
        });
        run.textures_delta.clear();
        assert_ne!(kept, 24.0, "a mouse host keeps its band: {kept}");
    }
}
