//! Drawing. The menu on the left with the logo and version on its top
//! border (stations, then Settings, About, Quit, like tubevibe), the chosen
//! page on the right, then a shaded status bar and the keys as boxed key
//! caps at the bottom.
//! Panels have titled borders, tables have column headers, and the selected
//! row is a reverse-video bar.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row as TableRow, Table, TableState, Wrap,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    App, Details, Editor, EditorTab, FeedState, Focus, GenresState, Loaded, MenuItem, Pane, Prompt, Rename, SearchMode,
    Session, Setting, SongAction, StationRef, StationSearch, View,
};
use crate::art::Cover;
use crate::config::{AlbumArt, BorderStyle, GoodVibes};
use crate::icons::{Icons, icons};
use crate::input::TextInput;
use crate::pandora::{Rating, Song};
use crate::theme::Palette;
use crate::update::{Progress, Replace};
use ratatui_image::{Resize, StatefulImage};

thread_local! {
    /// `[look] border_style` of the frame being drawn (set by [`draw`]), like tubevibe.
    static BORDERS: std::cell::Cell<BorderStyle> = const { std::cell::Cell::new(BorderStyle::Plain) };
}

/// A border of blanks: no lines, but the same room, so nothing moves.
const NO_BORDER: border::Set<'static> = border::Set {
    top_left: " ",
    top_right: " ",
    bottom_left: " ",
    bottom_right: " ",
    vertical_left: " ",
    vertical_right: " ",
    horizontal_top: " ",
    horizontal_bottom: " ",
};

/// The lines of `style`.
pub fn border_set(style: BorderStyle) -> border::Set<'static> {
    match style {
        BorderStyle::Plain => BorderType::Plain.to_border_set(),
        BorderStyle::Rounded => BorderType::Rounded.to_border_set(),
        BorderStyle::Thick => BorderType::Thick.to_border_set(),
        BorderStyle::Double => BorderType::Double.to_border_set(),
        BorderStyle::None => NO_BORDER,
    }
}

/// Every pane's frame, in the chosen border style.
fn framed<'a>() -> Block<'a> {
    Block::bordered().border_set(border_set(BORDERS.get()))
}

/// The menu's width: Menu Width, never more than half the screen.
pub fn menu_width(app: &App, screen: u16) -> u16 {
    app.config.ui.menu_width.min(screen / 2)
}

pub fn draw(frame: &mut Frame, app: &App) {
    BORDERS.set(app.config.ui.border_style);
    let p = app.palette;
    let area = frame.area();
    // Built-in themes paint their background; Omarchy's is the terminal's own.
    let base = if app.follows_omarchy() { Style::new().fg(p.fg) } else { Style::new().fg(p.fg).bg(p.background) };
    frame.render_widget(Block::new().style(base), area);
    if let Some(progress) = app.splash_progress() {
        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
        crate::splash::draw(frame, &p, progress, &version, "Pandora in your terminal");
        return;
    }
    let ic = icons(app.config.ui.icons);
    let [body, status, keys] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let side = menu_width(app, body.width);
    let [left, right] = Layout::horizontal([Constraint::Length(side), Constraint::Fill(1)]).areas(body);
    draw_menu(frame, app, ic, left);
    match app.view {
        View::Player => draw_player(frame, app, ic, right),
        View::Stations => draw_stations(frame, app, ic, right),
        View::Genres => draw_genres(frame, app, ic, right),
        View::Pinned => draw_pinned(frame, app, ic, right),
        View::Settings => draw_settings(frame, app, ic, right),
        View::About => draw_about(frame, app, ic, right),
        View::Notifications => draw_notes(frame, app, ic, right),
    }
    draw_status(frame, app, status);
    draw_keys(frame, &p, &key_caps(app), keys);
    if app.overlay_open() {
        dim(frame.buffer_mut(), p.background);
    }
    if app.help {
        draw_help(frame, app, body);
    }
    if app.system_open {
        draw_system(frame, app, body);
    }
    if let Some(menu) = &app.song_menu {
        draw_song_menu(frame, &p, body, menu);
    }
    if let Some(panel) = &app.panel {
        draw_panel(frame, &p, body, panel);
    }
    if let Some(d) = &app.details {
        draw_details(frame, &p, body, d);
    }
    if app.confirm_quit {
        draw_confirm_quit(frame, &p, area);
    }
    if let Some(station) = &app.confirm_delete {
        draw_confirm_delete(frame, &p, area, station);
    }
    if let Some(pin) = app.confirm_unpin.and_then(|at| app.pins.get(at)) {
        let question = format!("Unpin “{}” by {}?", clean(&pin.title), clean(&pin.artist));
        draw_box(frame, &p, area, &BoxText { title: " Unpin song ", ..BoxText::confirm(&question) });
    }
    if let Some(rename) = &app.rename {
        draw_rename(frame, &p, area, rename);
    }
    if let Some(search) = &app.add_station {
        draw_add_station(frame, &p, area, search);
    }
    if let Some(ed) = &app.editor {
        draw_editor(frame, &p, ic, area, ed);
    }
    if let Some(picker) = &app.theme_picker {
        draw_theme_picker(frame, app, area, picker);
    }
    if app.prompt == Some(Prompt::Password) {
        draw_password_box(frame, app, ic, area);
    }
    if let Some(notice) = &app.genre_notice {
        draw_genre_notice(frame, &p, ic, area, notice);
    }
    if let Some(mode) = app.confirm_good_vibes {
        draw_good_vibes_off(frame, &p, area, mode);
    }
}

/// How bright the screen behind a popup stays, and how much colour it keeps.
const DIM_BRIGHTNESS: f32 = 0.4;
const DIM_SATURATION: f32 = 0.5;

use ratatui::style::Color;

/// Behind a popup: every cell (the half-block cover too) faded toward the
/// theme background, to about 40% and greyer, so the popup stands out.
fn dim(buf: &mut ratatui::buffer::Buffer, background: Color) {
    for cell in buf.content.iter_mut() {
        cell.fg = dim_color(cell.fg, background);
        cell.bg = dim_color(cell.bg, background);
    }
}

/// One colour, dimmed. RGB is desaturated then blended toward the
/// background; named and indexed colours (no RGB to blend) become a dim gray.
fn dim_color(color: Color, background: Color) -> Color {
    let (br, bg, bb) = match background {
        Color::Rgb(r, g, b) => (f32::from(r), f32::from(g), f32::from(b)),
        _ => (0.0, 0.0, 0.0),
    };
    match color {
        Color::Reset => Color::Reset,
        c if c == background => c,
        Color::Rgb(r, g, b) => {
            let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
            let grey = 0.299 * r + 0.587 * g + 0.114 * b;
            let fade = |c: f32, base: f32| {
                let c = grey + (c - grey) * DIM_SATURATION;
                (base + (c - base) * DIM_BRIGHTNESS).round().clamp(0.0, 255.0) as u8
            };
            Color::Rgb(fade(r, br), fade(g, bg), fade(b, bb))
        }
        Color::Black => Color::Black,
        _ => Color::DarkGray,
    }
}

/// tubevibe's confirm box: " Confirm " on a red border; a blank line, the
/// question, the keys and a blank line, so it sits evenly in the frame.
fn draw_confirm_quit(frame: &mut Frame, p: &Palette, screen: Rect) {
    draw_box(frame, p, screen, &BoxText::confirm("Quit boxvibe?"));
}

/// `x` on a station: the same box, naming the station and what happens.
fn draw_confirm_delete(frame: &mut Frame, p: &Palette, screen: Rect, station: &StationRef) {
    let name = clean(&station.name);
    let question = if station.favorite {
        format!("Delete the station “{name}” (the one this favorite plays)? It removes it from your Pandora account.")
    } else {
        format!("Delete the station “{name}”? It removes it from your Pandora account.")
    };
    draw_box(frame, p, screen, &BoxText::confirm(&question));
}

/// `r` on a station: a box asking for the new name, in the same style.
fn draw_rename(frame: &mut Frame, p: &Palette, screen: Rect, rename: &Rename) {
    let name = clean(&rename.station.name);
    let question = if rename.station.favorite {
        format!("New name for the station “{name}” (the one this favorite plays):")
    } else {
        format!("New name for “{name}”:")
    };
    let text = BoxText {
        title: " Rename station ",
        border: p.cyan,
        width: 50,
        question,
        input: Some(rename.text.clone()),
        hint: "Enter = rename · Esc = cancel",
    };
    draw_box(frame, p, screen, &text);
}

/// What one of the small centred boxes says.
struct BoxText<'a> {
    title: &'a str,
    border: Color,
    width: u16,
    question: String,
    /// A line being typed, with its cursor.
    input: Option<TextInput>,
    hint: &'a str,
}

impl<'a> BoxText<'a> {
    /// The yes/no box: red border, "y = yes · any other key = no".
    fn confirm(question: &str) -> Self {
        Self {
            title: " Confirm ",
            border: Color::Reset,
            width: 40,
            question: question.to_owned(),
            input: None,
            hint: "y = yes · any other key = no",
        }
    }
}

/// The question wrapped to `width` cells, at most `max` lines; a question
/// longer than that ends in "…" on its last line.
fn wrap_question(question: &str, width: usize, max: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    for word in question.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.width() + 1 + word.width() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(truncate(word, width)),
        }
    }
    if lines.len() > max {
        lines.truncate(max.max(1));
        let last = lines.pop().unwrap_or_default();
        lines.push(truncate(&format!("{last} …"), width));
    }
    lines
}

fn draw_box(frame: &mut Frame, p: &Palette, screen: Rect, text: &BoxText) {
    let width = text.width.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    // Borders and the fixed lines (blank, hint, blank, the input) take rows; the question gets what's left.
    let fixed = 5 + u16::from(text.input.is_some());
    let room = usize::from(screen.height.saturating_sub(2).saturating_sub(fixed)).max(1);
    let question = wrap_question(&text.question, usize::from(width.saturating_sub(4)), room);
    let border = if text.border == Color::Reset { p.red } else { text.border };
    let mut lines = vec![Line::from("")];
    let question_rows = question.len() as u16;
    lines.extend(question.into_iter().map(Line::from));
    if let Some(input) = &text.input {
        let (shown, _) = input.window(usize::from(width.saturating_sub(7)), false, safe_char_width);
        lines.push(
            Line::from(vec![
                Span::styled(" › ", Style::new().fg(border)),
                Span::styled(shown, Style::new().fg(p.fg).bold()),
            ])
            .left_aligned(),
        );
    }
    lines.push(Line::from(Span::styled(text.hint, Style::new().fg(p.muted))));
    lines.push(Line::from(""));
    let area = centered(screen, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let block = framed().border_style(Style::new().fg(border)).title(text.title).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(block), area);
    if let Some(input) = &text.input {
        let (_, column) = input.window(usize::from(width.saturating_sub(7)), false, safe_char_width);
        let x = area.x + 1 + 3 + column as u16;
        let y = area.y + 1 + 1 + question_rows;
        frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(2)), y));
    }
}

/// `.`: what can be done with a song, its own key (if any) on the right.
fn draw_song_menu(frame: &mut Frame, p: &Palette, area: Rect, menu: &crate::app::SongMenu) {
    let width = 44.min(area.width.saturating_sub(2)).max(area.width.min(20));
    let inner_w = usize::from(width.saturating_sub(2));
    let mut lines = vec![Line::from(Span::styled(
        format!(" {}", fit(&format!("{} · {}", menu.song.title, menu.song.artist), inner_w.saturating_sub(1))),
        Style::new().fg(p.muted),
    ))];
    lines.push(Line::raw(""));
    for (i, action) in SongAction::ALL.iter().enumerate() {
        let key = action.key();
        let label = action.label();
        let gap = inner_w.saturating_sub(label.width() + key.width() + 3);
        let text = format!(" {label}{}{key}  ", " ".repeat(gap));
        let style = if i == menu.cursor { selection(p, true) } else { Style::new().fg(p.fg) };
        lines.push(Line::from(Span::styled(pad(&text, inner_w), style)));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(" Enter picks · Esc closes", Style::new().fg(p.muted))));
    let height = (lines.len() as u16 + 2).min(area.height);
    let box_area = centered(area, width, height);
    frame.render_widget(Clear, box_area);
    let block = panel(title("Song".into(), p), p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

/// The artist page (bio, similar artists) or a station's modes: intro text
/// on top, then a list with a cursor; long lists scroll with it.
fn draw_panel(frame: &mut Frame, p: &Palette, area: Rect, panel: &crate::app::Panel) {
    let width = 64.min(area.width.saturating_sub(2)).max(area.width.min(20));
    let inner_w = usize::from(width.saturating_sub(2));
    let text_w = inner_w.saturating_sub(2).max(1);
    let mut lines: Vec<Line> = Vec::new();
    let intro_style = if panel.loading { Style::new().fg(p.muted) } else { Style::new().fg(p.fg) };
    for part in wrap_question(&clean(&panel.intro), text_w, 8) {
        lines.push(Line::from(Span::styled(format!(" {part}"), intro_style)));
    }
    if !panel.link.is_empty() {
        lines.push(Line::from(Span::styled(format!(" {}", fit(&panel.link, text_w)), Style::new().fg(p.muted))));
    }
    if !panel.items.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(format!(" {}", panel.list_heading()), Style::new().fg(p.muted))));
    }
    // The list gets what is left of the screen; it scrolls to keep the cursor in view.
    let fixed = lines.len() + 2 + 2;
    let room = usize::from(area.height).saturating_sub(fixed + 2).max(1);
    let per_item = if panel.items.iter().any(|i| !i.note.is_empty()) { 2 } else { 1 };
    let shown = (room / per_item).max(1);
    let first = panel.cursor.saturating_sub(shown - 1);
    for (i, item) in panel.items.iter().enumerate().skip(first).take(shown) {
        let mark = if item.current { "● " } else { "  " };
        let style = if i == panel.cursor {
            selection(p, true)
        } else if item.locked {
            Style::new().fg(p.muted)
        } else {
            Style::new().fg(p.fg)
        };
        lines.push(Line::from(Span::styled(
            pad(&format!(" {mark}{}", fit(&item.label, text_w.saturating_sub(2))), inner_w),
            style,
        )));
        if !item.note.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("    {}", fit(&item.note, text_w.saturating_sub(3))),
                Style::new().fg(p.muted),
            )));
        }
    }
    lines.push(Line::raw(""));
    let hint = match panel.kind {
        crate::app::PanelKind::Artist { .. } if !panel.items.is_empty() => " Enter starts a station · Esc closes",
        crate::app::PanelKind::Modes { .. } if !panel.items.is_empty() => " Enter switches mode · Esc closes",
        _ => " Esc closes",
    };
    lines.push(Line::from(Span::styled(fit(hint, inner_w.saturating_sub(2)), Style::new().fg(p.muted))));
    let height = (lines.len() as u16 + 2).min(area.height);
    let box_area = centered(area, width, height);
    frame.render_widget(Clear, box_area);
    let heading = fit(&panel.title(), inner_w.saturating_sub(4));
    let block = panel_block(heading, p).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn panel_block<'a>(heading: String, p: &Palette) -> Block<'a> {
    panel(title(heading, p), p, true)
}

/// `v`: what Pandora tells about a song or station: label and value rows,
/// the values wrapped, nothing the API did not send.
fn draw_details(frame: &mut Frame, p: &Palette, area: Rect, d: &Details) {
    let rows = d.rows();
    let label_w = rows.iter().map(|(l, _)| l.width()).max().unwrap_or(0) + 2;
    let width = 72.min(area.width.saturating_sub(2)).max(area.width.min(20));
    let value_w = usize::from(width).saturating_sub(2 + 1 + label_w + 1).max(1);
    let mut lines: Vec<Line> = Vec::new();
    for (label, value) in &rows {
        for (i, part) in wrap_question(&clean(value), value_w, 4).into_iter().enumerate() {
            let head = if i == 0 { pad(label, label_w) } else { " ".repeat(label_w) };
            let value_style = if part.starts_with("loading") || part.starts_with("not available") {
                Style::new().fg(p.muted)
            } else {
                Style::new().fg(p.fg)
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {head}"), Style::new().fg(p.muted)),
                Span::styled(part, value_style),
            ]));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(" Any key closes", Style::new().fg(p.muted))));
    let height = (lines.len() as u16 + 2).min(area.height);
    let box_area = centered(area, width, height);
    frame.render_widget(Clear, box_area);
    let heading = format!("{} · {}", d.kind(), fit(&d.title(), usize::from(width).saturating_sub(16)));
    let block = panel(title(heading, p), p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

// --- Pieces ---

/// A panel with its title on the top border; `focused` brightens the
/// border. With no border to light, its title does (as in tubevibe).
fn panel<'a>(title: impl Into<Line<'a>>, p: &Palette, focused: bool) -> Block<'a> {
    let mut title = title.into();
    if focused && BORDERS.get() == BorderStyle::None {
        title = title.patch_style(Style::new().bg(p.highlight).add_modifier(Modifier::BOLD));
    }
    framed().border_style(Style::new().fg(if focused { p.fg } else { p.line })).title(title)
}

fn title<'a>(text: String, p: &Palette) -> Line<'a> {
    Line::from(Span::styled(format!(" {text} "), Style::new().fg(p.fg)))
}

/// The selected row: reverse video while its panel has the keys, a shade otherwise.
fn selection(p: &Palette, focused: bool) -> Style {
    if focused {
        Style::new().fg(p.background).bg(p.fg).add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(p.highlight)
    }
}

/// An icon padded to its worst-case width plus one spare cell when it may
/// draw wider than it measures (Nerd Font glyphs in foot), so it never
/// covers what follows.
fn icon_cell(icon: &str) -> String {
    let w = safe_width(icon).max(2);
    let w = if icon.chars().any(is_pua) { w + 1 } else { w };
    pad(icon, w)
}

/// One icon column: as wide as the widest of `icons` in `icon_cell` form,
/// counted the way the screen lays cells out (unicode-width: a Nerd glyph is
/// one cell, its padding is what gives it room). A row without an icon here
/// gets that many blanks, so the next column and the name start at the same
/// place on every row.
fn column(icons: &[&str]) -> usize {
    icons.iter().map(|i| icon_cell(i).width()).max().unwrap_or(1)
}

/// `icon` in a column of `width` cells; `None` is a blank column.
fn cell(icon: Option<&str>, width: usize) -> String {
    match icon {
        Some(i) => pad(&icon_cell(i), width),
        None => " ".repeat(width),
    }
}

fn clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// `text` padded with spaces to `cells` terminal cells (emoji take two).
fn pad(text: &str, cells: usize) -> String {
    let w = text.width();
    format!("{text}{}", " ".repeat(cells.saturating_sub(w)))
}

/// Private Use Area: Nerd Font icons, which foot (and others) may draw two
/// cells wide although they measure one.
fn is_pua(c: char) -> bool {
    matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

/// Pictographs and symbols that have emoji forms: terminals disagree on how
/// wide these are drawn (one cell or two), whatever unicode-width says.
fn is_pictographic(c: char) -> bool {
    matches!(
        c as u32,
        0x2300..=0x23FF
            | 0x24C2
            | 0x25A0..=0x25FF
            | 0x2600..=0x27BF
            | 0x2B00..=0x2BFF
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F000..=0x1FAFF
    )
}

/// Characters that take no cell, or join or restyle their neighbours:
/// variation selectors, joiners, soft hyphen, BOM, bidi controls.
fn is_zero_width(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD | 0x034F | 0x061C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F
            | 0xFE00..=0xFE0F | 0xFEFF | 0xE0000..=0xE007F
    ) || (c != ' ' && UnicodeWidthChar::width(c) == Some(0))
}

/// Song and station text made safe for a fixed-width cell: tabs, newlines
/// and other controls become spaces; zero-width characters go; symbols whose
/// drawn width terminals disagree on (one-cell pictographs that foot may draw
/// as two-cell emoji, Nerd Font glyphs) go too, since a glyph drawn wider
/// than measured shifts the rest of the row, the rating included.
/// Two-cell emoji (🎵) stay: every terminal agrees on those.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            out.push(' ');
        } else if !(is_zero_width(c) || is_pua(c) || is_pictographic(c) && UnicodeWidthChar::width(c) != Some(2)) {
            out.push(c);
        }
    }
    out
}

/// The cells `c` may take on any terminal: ambiguous-width, pictographic and
/// Private Use characters count as two so a cut never overflows.
fn safe_char_width(c: char) -> usize {
    if is_zero_width(c) || c.is_control() {
        return 0;
    }
    let w = UnicodeWidthChar::width(c).unwrap_or(0);
    let ambiguous = UnicodeWidthChar::width_cjk(c).unwrap_or(0) > w;
    if is_pua(c) || is_pictographic(c) || ambiguous || !c.is_ascii() && w == 0 { 2 } else { w }
}

/// The worst-case width of `text` on any terminal.
fn safe_width(text: &str) -> usize {
    text.chars().map(safe_char_width).sum()
}

/// Clean `text` and cut it to fit `cells` with one cell to spare, measuring
/// worst-case widths, ending in "…" when cut.
fn fit(text: &str, cells: usize) -> String {
    truncate(&clean(text), cells.saturating_sub(1))
}

/// `text` cut to `cells` cells (worst-case widths), ending in "…" when cut.
fn truncate(text: &str, cells: usize) -> String {
    if safe_width(text) <= cells {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = safe_char_width(c);
        if used + w + safe_char_width('…') > cells {
            break;
        }
        used += w;
        out.push(c);
    }
    if cells > 0 {
        out.push('…');
    }
    out
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn rating_word<'a>(rating: Rating, p: &Palette, ic: &Icons) -> Span<'a> {
    match rating {
        Rating::Up => Span::styled(format!("{} liked", ic.up), Style::new().fg(p.green).bold()),
        Rating::Down => Span::styled(format!("{} disliked", ic.down), Style::new().fg(p.red).bold()),
        Rating::None => Span::styled(format!("{} not rated", ic.unrated), Style::new().fg(p.muted)),
    }
}

// --- Status bar ---

/// What is going on, on a shaded bar above the keys (where tubevibe has it),
/// and "Logged In As: <email>" at its far right.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    let color = if app.status.starts_with('⚠') { p.yellow } else { p.fg };
    frame.render_widget(Block::new().bg(p.highlight), area);
    // The right side (sleep timer, look, account) gets at most half the bar,
    // and never less than the status.
    let room = (area.width / 2).saturating_sub(3) as usize;
    let account = account_label(app, room);
    let used = |extra: &[String]| -> usize {
        let parts = extra.iter().map(|s| safe_width(s) + 3).sum::<usize>();
        parts + account.as_ref().map_or(0, |a| safe_width(a))
    };
    // The look labels go first (from the last) when the bar is narrow, then the timer.
    let mut extra: Vec<String> = sleep_label(app).into_iter().chain(look_labels(app)).collect();
    while !extra.is_empty() && used(&extra) > room {
        extra.pop();
    }
    let extra_w = extra.iter().map(|s| safe_width(s) + 3).sum::<usize>();
    let want = match &account {
        Some(a) => extra_w + safe_width(a) + 3,
        None if extra.is_empty() => 0,
        None => extra_w,
    } as u16;
    let right_w = want.min(area.width / 2);
    let [left, right] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_w)]).areas(area);
    let status = fit(&format!(" {}", app.status), left.width.saturating_sub(1) as usize);
    frame.render_widget(Paragraph::new(Span::styled(status, Style::new().fg(color))), left);
    if right.width <= 2 {
        return;
    }
    let mut spans = Vec::new();
    for (i, label) in extra.iter().enumerate() {
        // The look labels share the account's colour; the sleep timer stays yellow.
        let color = if i == 0 && app.sleep.is_some() { p.yellow } else { p.blue };
        spans.push(Span::styled(label.clone(), Style::new().fg(color)));
        spans.push(Span::styled(" · ", Style::new().fg(p.muted)));
    }
    match &account {
        // tubevibe's colour for its account: the palette's `blue` (Gruvbox's aqua).
        Some(account) => {
            let room = (right.width as usize - 2).saturating_sub(extra_w);
            spans.push(Span::styled(fit(account, room), Style::new().fg(p.blue)));
        }
        None => {
            spans.pop();
        }
    }
    spans.push(Span::raw(" "));
    frame.render_widget(Paragraph::new(Line::from(spans)).alignment(Alignment::Right), right);
}

/// "☾ 14:59" (or "☾ end of song") while the sleep timer runs.
fn sleep_label(app: &App) -> Option<String> {
    match app.sleep? {
        crate::app::Sleep::Minutes(..) => app.sleep_left().map(|d| format!("☾ {}", crate::app::clock_text(d))),
        crate::app::Sleep::EndOfSong => Some("☾ end of song".into()),
    }
}

/// The look in a few words, in the order they are dropped last to first:
/// theme, album art, retro mode, tint.
fn look_labels(app: &App) -> Vec<String> {
    let ui = &app.config.ui;
    // Following Omarchy: "omarchy: Tokyo Night" (just "Omarchy" if its name is unknown).
    let theme = if ui.theme() == "omarchy" {
        app.omarchy_theme.as_ref().map_or_else(|| "Omarchy".to_owned(), |n| format!("omarchy: {n}"))
    } else {
        crate::theme::label(ui.theme())
    };
    vec![
        theme,
        format!("Art {}", ui.album_art.label()),
        ui.retro_mode.label().to_owned(),
        format!("Tint {}", ui.art_tint.label()),
    ]
}

/// "Logged In As: me@example.com" while logged in; nothing otherwise. When
/// the bar's right half (`room` columns) is too narrow for all of it, just the
/// email, so the part that matters is what stays.
fn account_label(app: &App, room: usize) -> Option<String> {
    let name = clean(app.config.pandora.username.trim());
    if app.session != Session::LoggedIn || name.is_empty() {
        return None;
    }
    let full = format!("Logged In As: {name}");
    Some(if safe_width(&full) <= room { full } else { name })
}

/// The logo on the menu's top border, like tubevibe's: the box icon (always
/// the Nerd Font one, whatever the icon set) and "box" in the theme's blue,
/// "vibe" in the text colour.
pub fn logo_title<'a>(p: &Palette) -> Line<'a> {
    logo_line_with(p, Span::styled(format!(" {} ", crate::icons::LOGO), Style::new().fg(p.blue).bold()), "")
}

/// tubevibe's Help icon: the little keyboard, the same in every icon set,
/// in the text colour with two spaces after it, like tubevibe's title.
const HELP_ICON: &str = "⌨";

fn help_title<'a>(p: &Palette) -> Line<'a> {
    logo_line_with(p, Span::styled(format!(" {HELP_ICON}  "), Style::new().fg(p.fg).bold()), HELP_TITLE_REST)
}

fn logo_line_with<'a>(p: &Palette, icon: Span<'a>, rest: &str) -> Line<'a> {
    Line::from(vec![
        icon,
        Span::styled("box", Style::new().fg(p.blue).bold()),
        Span::styled(format!("vibe{rest} "), Style::new().fg(p.fg).bold()),
    ])
}

/// The Help window's title: "⌨  boxvibe Help · Any Key Closes".
const HELP_TITLE_REST: &str = " Help · Any Key Closes";

// --- Menu ---

/// How far the menu's rules stay from each side of the frame (tubevibe's `SEPARATOR_INSET`).
const SEPARATOR_INSET: &str = "  ";

fn inset_rule<'a>(p: &Palette, width: u16) -> Paragraph<'a> {
    let w = usize::from(width).saturating_sub(2 * SEPARATOR_INSET.len());
    Paragraph::new(format!("{SEPARATOR_INSET}{}", "─".repeat(w))).fg(p.line)
}

fn draw_menu(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Menu;
    let version = Line::from(Span::styled(format!(" {} ", env!("CARGO_PKG_VERSION")), Style::new().fg(p.muted)))
        .alignment(Alignment::Right);
    let block = panel(logo_title(&p), &p, focused).title(version);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let now_h = if app.has_now_playing() { 2 } else { 0 };
    let pinned = app.config.ui.pinned_in_menu;
    let [now_area, stations_row, favs_area, _, rule, app_area] = Layout::vertical([
        Constraint::Length(now_h),
        Constraint::Length(2 + u16::from(pinned)),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(4),
    ])
    .areas(inner);
    let selected = app.menu_selected();
    // Without the keys, the entry of the page shown stays shaded.
    let shown = |item: MenuItem| {
        matches!(
            (item, app.view),
            (MenuItem::Settings, View::Settings)
                | (MenuItem::About, View::About)
                | (MenuItem::Notifications, View::Notifications)
                | (MenuItem::NowPlaying, View::Player)
                | (MenuItem::Stations, View::Stations)
                | (MenuItem::Genres, View::Genres)
                | (MenuItem::Pinned, View::Pinned)
        )
    };
    let row_style = |item: MenuItem| {
        if focused && item == selected {
            Some(selection(&p, true))
        } else if !focused && shown(item) {
            Some(selection(&p, false))
        } else {
            None
        }
    };
    if now_h > 0 {
        // NOW PLAYING on top, back to the player; a rule under it, inset like tubevibe's.
        let [entry, line] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(now_area);
        let mut style = Style::new().fg(p.green).bold();
        if let Some(sel) = row_style(MenuItem::NowPlaying) {
            style = style.patch(sel);
        }
        let text = format!(" {} NOW PLAYING", pad(ic.play, 2));
        frame.render_widget(Paragraph::new(Line::from(pad(&text, entry.width as usize)).style(style)), entry);
        frame.render_widget(inset_rule(&p, line.width), line);
    }
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let mut lines = Vec::new();
    let mut top = vec![
        (MenuItem::Stations, ic.station, format!("Stations{count}")),
        (MenuItem::Genres, ic.genre, "Genres".into()),
    ];
    if pinned {
        let n = if app.pins.is_empty() { String::new() } else { format!(" ({})", app.pins.len()) };
        top.push((MenuItem::Pinned, ic.pin, format!("Pinned{n}")));
    }
    for (item, icon, label) in top {
        let mut style = Style::new().fg(p.fg);
        if let Some(sel) = row_style(item) {
            style = style.patch(sel);
        }
        let text = format!(" {} {label}", pad(icon, 2));
        lines.push(Line::from(pad(&text, stations_row.width as usize)).style(style));
    }
    frame.render_widget(Paragraph::new(lines), stations_row);
    // FAVORITES, like tubevibe's tabs: only once a station is starred.
    let favorites = &app.config.ui.favorites;
    if app.config.ui.favorites_in_menu && !favorites.is_empty() && favs_area.height > 1 {
        let [gap, head, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)]).areas(favs_area);
        frame.render_widget(inset_rule(&p, gap.width), gap);
        frame.render_widget(
            Paragraph::new(Span::styled(format!(" FAVORITES ({})", favorites.len()), Style::new().fg(p.orange).bold())),
            head,
        );
        let c_fav = column(&[ic.play, ic.favorite]);
        let c_mark = column(&[ic.genre_station, ic.own_station]);
        let items: Vec<ListItem> = favorites
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let playing = app.current_station.as_deref() == Some(f.id.as_str());
                let mut style = if playing { Style::new().fg(p.green).bold() } else { Style::new().fg(p.fg) };
                if let Some(sel) = row_style(MenuItem::Favorite(i)) {
                    style = style.patch(sel);
                }
                let icon = if playing { ic.play } else { ic.favorite };
                // The favorite's station may not be loaded yet: a blank mark until it is.
                let mark = app.stations.iter().find(|s| s.id == f.id).and_then(|s| station_mark(s, ic));
                let head = format!(" {} {} ", cell(Some(icon), c_fav), cell(mark, c_mark));
                let room = (list_area.width as usize).saturating_sub(safe_width(&head));
                let line = format!("{head}{}", truncate(&clean(&f.name), room));
                ListItem::new(Line::from(pad(&line, list_area.width as usize))).style(style)
            })
            .collect();
        let at = match selected {
            MenuItem::Favorite(i) => Some(i),
            _ => None,
        };
        let mut state = ListState::default().with_selected(at);
        frame.render_stateful_widget(List::new(items), list_area, &mut state);
    }
    frame.render_widget(inset_rule(&p, rule.width), rule);
    let entries = [
        (MenuItem::Settings, ic.settings, "Settings"),
        (MenuItem::Notifications, ic.bell, "Notifications"),
        (MenuItem::About, ic.about, "About"),
        (MenuItem::Quit, ic.quit, "Quit"),
    ];
    let mut lines = Vec::new();
    for (item, icon, label) in entries {
        let mut style = Style::new().fg(p.fg);
        if let Some(sel) = row_style(item) {
            style = style.patch(sel);
        }
        let mut text = format!(" {} {label}", pad(icon, 2));
        if item == MenuItem::About && app.update.is_some() {
            text.push_str(&format!("  {} new", ic.update));
        }
        lines.push(Line::from(pad(&text, app_area.width as usize)).style(style));
    }
    frame.render_widget(Paragraph::new(lines), app_area);
}

// --- Notifications ---

/// "just now", "4 min ago", "2 h ago".
fn ago(secs: u64) -> String {
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

/// What the bottom bar said, newest first, with how long ago.
fn draw_notes(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let block = panel(title(format!("{} Notifications", ic.bell), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.notes.is_empty() {
        let text = " Nothing yet. What the bottom bar says lands here, newest first.";
        frame.render_widget(Paragraph::new(text).fg(p.muted).wrap(Wrap { trim: false }), inner);
        return;
    }
    let when_w = 12;
    let text_w = usize::from(inner.width).saturating_sub(when_w + 3);
    let items: Vec<ListItem> = app
        .notes
        .iter()
        .map(|(at, text)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {}", pad(&ago(at.elapsed().as_secs()), when_w)), Style::new().fg(p.muted)),
                Span::styled(fit(text, text_w), Style::new().fg(p.fg)),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.notes_cursor.min(app.notes.len() - 1)));
    let list = List::new(items).highlight_style(selection(&p, focused));
    frame.render_stateful_widget(list, inner, &mut state);
}

// --- Pinned ---

/// The songs Shift+I pinned, newest first: title, artist · album, the day.
fn draw_pinned(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let count = if app.pins.is_empty() { String::new() } else { format!(" ({})", app.pins.len()) };
    let block = panel(title(format!("{} Pinned{count}", ic.pin), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.pins.is_empty() {
        let text = " Nothing pinned yet. Shift+I pins the selected or playing song: its title, artist, album, cover and Pandora page (never the audio).";
        frame.render_widget(Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: false }).fg(p.muted), inner);
        return;
    }
    let width = inner.width as usize;
    let items: Vec<ListItem> = app
        .pins
        .iter()
        .map(|pin| {
            let head = format!(" {} ", icon_cell(ic.pin));
            let date = format!("  {} ", pin.pinned);
            let by = if pin.album.is_empty() {
                clean(&pin.artist)
            } else {
                format!("{} · {}", clean(&pin.artist), clean(&pin.album))
            };
            let room = width.saturating_sub(safe_width(&head) + safe_width(&date));
            let name = truncate(&clean(&pin.title), room);
            let rest = truncate(&format!("  {by}"), room.saturating_sub(safe_width(&name)));
            let gap = room.saturating_sub(safe_width(&name) + safe_width(&rest));
            ListItem::new(Line::from(vec![
                Span::styled(format!("{head}{name}"), Style::new().fg(p.fg).bold()),
                Span::styled(rest, Style::new().fg(p.muted)),
                Span::raw(" ".repeat(gap)),
                Span::styled(date, Style::new().fg(p.muted)),
            ]))
        })
        .collect();
    let list = List::new(items).highlight_style(selection(&p, focused));
    let mut state = ListState::default().with_selected(Some(app.pinned_cursor.min(app.pins.len() - 1)));
    frame.render_stateful_widget(list, inner, &mut state);
}

// --- Stations ---

/// The icon after a station's star: the boxvibe logo for one of Pandora's
/// genre stations (Pandora won't let you rename it), a person for one you
/// made. Shuffle is neither. Only Stations and Favorites show it, never the
/// Genres page.
fn station_mark(s: &crate::pandora::Station, ic: &Icons) -> Option<&'static str> {
    if s.is_shuffle {
        None
    } else if s.is_locked() {
        Some(ic.genre_station)
    } else {
        Some(ic.own_station)
    }
}

fn draw_stations(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let block = panel(title(format!("{} Stations{count}", ic.station), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.stations.is_empty() && app.session != Session::LoggedIn {
        let text = match &app.session {
            Session::SigningIn => format!(" {} Signing in…", ic.loading),
            _ => " Log in from Settings".into(),
        };
        frame.render_widget(Paragraph::new(text).fg(p.muted), inner);
        return;
    }
    // Fixed columns, so every row's icons and name line up whatever it has:
    // [playing] [kind: radio / shuffle / shared] [star] [genre logo / person] name.
    // A row without a star or a mark keeps its blank cells.
    let (c_play, c_kind) = (ic.play.width().max(1), column(&[ic.station, ic.shuffle, ic.shared]));
    let (c_star, c_mark) = (column(&[ic.favorite]), column(&[ic.genre_station, ic.own_station]));
    // "Add station…" is the list's first row, above every station; its + sits in the kind column.
    let add_row =
        ListItem::new(Line::from(format!(" {} {} Add station…", " ".repeat(c_play), cell(Some("+"), c_kind))))
            .style(Style::new().fg(p.orange).bold());
    let items: Vec<ListItem> = std::iter::once(add_row)
        .chain(app.stations.iter().map(|s| {
            let playing = app.current_station.as_deref() == Some(s.id.as_str());
            let icon = if s.is_shuffle {
                ic.shuffle
            } else if s.is_shared {
                ic.shared
            } else {
                ic.station
            };
            let star = app.is_favorite(&s.id).then_some(ic.favorite);
            let style = if playing { Style::new().fg(p.green).bold() } else { Style::new().fg(p.fg) };
            let mark = playing.then_some(ic.play);
            // Right after the star: the logo for a genre station, a person for your own.
            let head = format!(
                " {} {} {} {} ",
                pad(mark.unwrap_or(" "), c_play),
                cell(Some(icon), c_kind),
                cell(star, c_star),
                cell(station_mark(s, ic), c_mark),
            );
            let room = (inner.width as usize).saturating_sub(safe_width(&head));
            let name = fit(&s.name, room);
            ListItem::new(Line::from(format!("{head}{name}"))).style(style)
        }))
        .collect();
    let list = List::new(items).highlight_style(selection(&p, focused));
    let at = if app.on_add_row { 0 } else { app.stations_index() + 1 };
    let mut state = ListState::default().with_selected(Some(at));
    frame.render_stateful_widget(list, inner, &mut state);
}

// --- Genres ---

fn draw_genres(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let block = panel(title(format!("{} Genres", ic.genre), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let message = match &app.genres {
        GenresState::Idle if app.session != Session::LoggedIn => {
            Some(" Log in from Settings to browse genres".to_owned())
        }
        GenresState::Idle | GenresState::Loading => Some(format!(" {} Loading genres…", ic.loading)),
        GenresState::Failed(why) => Some(format!(" {} {}  (Enter tries again)", ic.warn, clean(why))),
        GenresState::Loaded(categories) if categories.is_empty() => Some(" Pandora listed no genres".to_owned()),
        GenresState::Loaded(_) => None,
    };
    if let Some(text) = message {
        frame.render_widget(Paragraph::new(text).fg(p.muted), inner);
        return;
    }
    let GenresState::Loaded(categories) = &app.genres else { return };
    let width = inner.width as usize;
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = 0;
    let mut at = 0;
    for category in categories {
        items.push(ListItem::new(Line::from(Span::styled(
            format!(" {}", clean(&category.name).to_uppercase()),
            Style::new().fg(p.orange).bold(),
        ))));
        for genre in &category.genres {
            if at == app.genres_index() {
                selected_row = items.len();
            }
            at += 1;
            let head = format!("   {} ", icon_cell(ic.genre));
            let name = clean(&genre.name);
            let mut line = vec![Span::styled(format!("{head}{name}"), Style::new().fg(p.fg))];
            if !genre.description.is_empty() {
                let used = safe_width(&head) + safe_width(&name) + 3;
                let room = width.saturating_sub(used);
                if room > 8 {
                    line.push(Span::styled(
                        format!("  {}", fit(&clean(&genre.description), room - 2)),
                        Style::new().fg(p.muted),
                    ));
                }
            }
            items.push(ListItem::new(Line::from(line)));
        }
    }
    let list = List::new(items).highlight_style(selection(&p, focused));
    let mut state = ListState::default().with_selected(Some(selected_row));
    frame.render_stateful_widget(list, inner, &mut state);
}

/// Why a genre station can't be changed, and what the two station icons
/// mean, with a "Don't show this again" box. (The same message is always in
/// the status bar too.)
fn draw_genre_notice(frame: &mut Frame, p: &Palette, ic: &Icons, screen: Rect, notice: &crate::app::GenreNotice) {
    let width = 70.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    let inner_w = usize::from(width.saturating_sub(2 + 4));
    let mut lines = vec![Line::raw("")];
    let head = format!("“{}” can't be {}.", clean(&notice.station), notice.what);
    for part in wrap_question(&head, inner_w, 2) {
        lines.push(Line::from(Span::styled(format!("  {part}"), Style::new().fg(p.fg).bold())).left_aligned());
    }
    lines.push(Line::raw(""));
    let why = "Genre stations are made by Pandora, and Pandora doesn't let boxvibe (or anyone) change them.";
    for part in wrap_question(why, inner_w, 3) {
        lines.push(Line::from(Span::styled(format!("  {part}"), Style::new().fg(p.fg))).left_aligned());
    }
    lines.push(Line::raw(""));
    let meanings = [
        (ic.genre_station, p.accent, "Genre station: made by Pandora. Not renamed or edited; e shows your thumbs."),
        (ic.own_station, p.green, "Your own station: you made it. You can rename, edit and delete it."),
    ];
    for (glyph, color, text) in meanings {
        let mark = format!("{} ", icon_cell(glyph));
        let indent = safe_width(&mark);
        for (j, part) in wrap_question(text, inner_w.saturating_sub(indent), 3).into_iter().enumerate() {
            let head = if j == 0 { mark.clone() } else { " ".repeat(indent) };
            lines.push(Line::from(vec![
                Span::styled(format!("  {head}"), Style::new().fg(color).bold()),
                Span::styled(part, Style::new().fg(p.fg)),
            ]));
        }
    }
    lines.push(Line::raw(""));
    let (tick, style) =
        if notice.dont_show { ("[x]", Style::new().fg(p.green).bold()) } else { ("[ ]", Style::new().fg(p.fg).bold()) };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(tick, style),
        Span::styled(" Don't show this again", Style::new().fg(p.fg).bold()),
    ]));
    let note = "It comes back any time in Settings: BOXVIBE, Genre station edit popup.";
    for part in wrap_question(note, inner_w.saturating_sub(6), 2) {
        lines.push(Line::from(Span::styled(format!("      {part}"), Style::new().fg(p.muted))).left_aligned());
    }
    lines.push(Line::raw(""));
    lines.push(
        Line::from(Span::styled("Space = tick the box · Enter = OK · Esc = close", Style::new().fg(p.muted)))
            .centered(),
    );
    lines.push(Line::raw(""));
    // A short terminal: the blank spacer rows go first.
    if lines.len() as u16 + 2 > screen.height {
        lines.retain(|l| !l.spans.iter().all(|s| s.content.trim().is_empty()));
    }
    let area = centered(screen, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let block = framed()
        .border_style(Style::new().fg(p.accent))
        .title(Span::styled(" Genre station ", Style::new().fg(p.accent).bold()))
        .style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// Said in the password popup (both variants), the README and the CHANGELOG.
pub const PASSWORD_DISCLAIMER: &str = "Use at your own risk: boxvibe is not made by Pandora. By logging in with boxvibe you accept that neither boxvibe nor its creator is responsible if Pandora limits or bans your account.";

/// Said in the popup that asks before Good Vibes Only is turned off, the
/// README and the CHANGELOG.
pub const GOOD_VIBES_MUTED_DISCLAIMER: &str = "Muting Pandora's ads is a gray area under Pandora's terms of service. Use at your own risk: neither boxvibe nor its creator is responsible if Pandora limits or bans your account.";
pub const GOOD_VIBES_DISCLAIMER: &str = "Skipping Pandora's ads could violate Pandora's terms of service. Use at your own risk: neither boxvibe nor its creator is responsible if Pandora limits or bans your account.";

/// What the password popup says, by what is known about the keychain.
/// The first line is the reassurance, the rest the detail; `warn` marks the
/// no-keychain case.
pub struct PasswordNote {
    pub warn: bool,
    pub lines: Vec<String>,
    pub hint: &'static str,
}

pub fn password_note(state: crate::app::KeychainState) -> PasswordNote {
    use crate::app::KeychainState;
    match state {
        KeychainState::Unavailable => PasswordNote {
            warn: true,
            lines: vec![
                "No system keychain was found on this computer.".into(),
                "Your password will be kept for this session only: boxvibe asks for it again the next time it starts.".into(),
                "It is never written to config.toml or to any other file.".into(),
            ],
            hint: "Enter = log in · Esc = cancel",
        },
        KeychainState::Unknown => PasswordNote {
            warn: false,
            lines: vec![
                "boxvibe saves your password in your system keychain (the Secret Service: GNOME Keyring, KDE Wallet or KeePassXC).".into(),
                "If there is no keychain it is kept for this session only.".into(),
                "It is never written to config.toml or to any other file.".into(),
            ],
            hint: "Enter = log in and save · Esc = cancel",
        },
        KeychainState::Saved | KeychainState::NotSaved => PasswordNote {
            warn: false,
            lines: vec![
                "Your password is saved in your system keychain (the Secret Service: GNOME Keyring, KDE Wallet or KeePassXC).".into(),
                "It is never written to config.toml or to any other file, and boxvibe never keeps it in a file of its own.".into(),
            ],
            hint: "Enter = log in and save · Esc = cancel",
        },
    }
}

/// The password popup (a modal): who it is for, where the password goes,
/// the masked input and the keys.
fn draw_password_box(frame: &mut Frame, app: &App, ic: &Icons, screen: Rect) {
    let p = app.palette;
    let note = password_note(app.keychain_state);
    let width = 66.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    let inner_w = usize::from(width.saturating_sub(2 + 4));
    let border = if note.warn { p.orange } else { p.cyan };
    let who = clean(app.config.pandora.username.trim());
    let who = if who.is_empty() { "your Pandora account".to_owned() } else { who };
    let mut lines = vec![Line::raw("")];
    for part in wrap_question(&format!("Password for {who}"), inner_w, 2) {
        lines.push(Line::from(Span::styled(format!("  {part}"), Style::new().fg(p.fg).bold())));
    }
    lines.push(Line::raw(""));
    for (i, text) in note.lines.iter().enumerate() {
        let (mark, style) = match (note.warn, i) {
            (true, 0) => (format!("{} ", ic.warn), Style::new().fg(p.orange).bold()),
            (false, 0) => (format!("{} ", ic.ok), Style::new().fg(p.green)),
            _ => ("  ".to_owned(), Style::new().fg(p.fg)),
        };
        let indent = safe_width(&mark);
        for (j, part) in wrap_question(text, inner_w.saturating_sub(indent), 4).into_iter().enumerate() {
            let head = if j == 0 { mark.clone() } else { " ".repeat(indent) };
            lines.push(Line::from(Span::styled(format!("  {head}{part}"), style)).left_aligned());
        }
    }
    lines.push(Line::raw(""));
    // The risk notice, set apart in yellow (every variant).
    for part in wrap_question(PASSWORD_DISCLAIMER, inner_w, 5) {
        lines.push(Line::from(Span::styled(format!("  {part}"), Style::new().fg(p.yellow))).left_aligned());
    }
    lines.push(Line::raw(""));
    let mut input_row = lines.len() as u16;
    let (shown, column) = app.query.window(inner_w.saturating_sub(3), true, safe_char_width);
    lines.push(
        Line::from(vec![
            Span::styled("   › ", Style::new().fg(border)),
            Span::styled(shown, Style::new().fg(p.fg).bold()),
        ])
        .left_aligned(),
    );
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(note.hint, Style::new().fg(p.muted))).centered());
    lines.push(Line::raw(""));
    // A short terminal: the blank spacer rows go first so the input stays on screen.
    if lines.len() as u16 + 2 > screen.height {
        let blank = |l: &Line| l.spans.iter().all(|s| s.content.trim().is_empty());
        input_row -= lines[..usize::from(input_row)].iter().filter(|l| blank(l)).count() as u16;
        lines.retain(|l| !blank(l));
        // Still too tall (a tiny terminal): the key hint goes, then the top lines.
        let excess = (lines.len() as u16 + 2).saturating_sub(screen.height);
        if excess > 0 {
            lines.truncate(usize::from(input_row) + 1);
            let drop = usize::from((lines.len() as u16 + 2).saturating_sub(screen.height));
            lines.drain(..drop);
            input_row -= drop as u16;
        }
    }
    let area = centered(screen, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let block = framed()
        .border_style(Style::new().fg(border))
        .title(Span::styled(" Pandora password ", Style::new().fg(border).bold()))
        .style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), area);
    let x = area.x + 1 + 5 + column as u16;
    frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(2)), area.y + 1 + input_row));
}

/// Settings: turning Good Vibes Only off. Styled like the password popup,
/// with the same yellow risk notice; Enter turns it off, Esc keeps it on.
fn draw_good_vibes_off(frame: &mut Frame, p: &Palette, screen: Rect, mode: GoodVibes) {
    let muted = mode == GoodVibes::Muted;
    let width = 66.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    let inner_w = usize::from(width.saturating_sub(2 + 4));
    let text_lines = |text: &str, color: Color| -> Vec<Line<'static>> {
        wrap_question(text, inner_w, 12)
            .into_iter()
            .map(|part| Line::from(Span::styled(format!("  {part}"), Style::new().fg(color))).left_aligned())
            .collect()
    };
    let intro = text_lines(
        if muted {
            "Muted still plays Pandora's audio ads and reports them like the website does, at volume 0. Your volume comes back when the ad ends."
        } else {
            "Good Vibes Only plays Pandora's audio ads, like the website does. Turning it off skips them."
        },
        p.fg,
    );
    // The risk notice, set apart in yellow like the one in the password popup; never shortened.
    let risk = text_lines(if muted { GOOD_VIBES_MUTED_DISCLAIMER } else { GOOD_VIBES_DISCLAIMER }, p.yellow);
    let (long, short) = if muted {
        ("Enter = mute them anyway · Esc = keep it on", "Enter = mute · Esc = keep on")
    } else {
        ("Enter = turn it off anyway · Esc = keep it as it was", "Enter = turn off · Esc = keep")
    };
    let hint = if long.width() <= inner_w { long } else { short };
    let hint = Line::from(Span::styled(hint, Style::new().fg(p.muted))).centered();
    let build = |with_intro: bool, spaced: bool| {
        let gap = || spaced.then(|| Line::raw(""));
        let mut lines: Vec<Line> = gap().into_iter().collect();
        if with_intro {
            lines.extend(intro.clone());
            lines.extend(gap());
        }
        lines.extend(risk.clone());
        lines.extend(gap());
        lines.push(hint.clone());
        lines.extend(gap());
        lines
    };
    // A short terminal: the blank spacer rows go first, then the first paragraph.
    let mut lines = build(true, true);
    for (with_intro, spaced) in [(true, false), (false, false)] {
        if lines.len() as u16 + 2 <= screen.height {
            break;
        }
        lines = build(with_intro, spaced);
    }
    // Still too tall (a tiny terminal): the key line goes (the bar below has the keys),
    // and last of all the end of the notice.
    if lines.len() as u16 + 2 > screen.height {
        lines.pop();
        lines.truncate(usize::from(screen.height.saturating_sub(2)));
    }
    let area = centered(screen, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let block = framed()
        .border_style(Style::new().fg(p.yellow))
        .title(Span::styled(
            if muted { " Mute Pandora's ads? " } else { " Turn off Good Vibes Only? " },
            Style::new().fg(p.yellow).bold(),
        ))
        .style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// `a`: the add-station box: the search text and, once Enter searched, the
/// artists and songs Pandora found.
fn draw_add_station(frame: &mut Frame, p: &Palette, screen: Rect, search: &StationSearch) {
    let look = AddLook {
        title: " Add station ",
        prompt: "Type an artist or a song",
        suggested: false,
        results_hint: "Enter = make the station · j/k = move · / = edit · Esc = cancel",
    };
    draw_search_box(frame, p, screen, search, &look);
}

/// What differs between the add-station box and the editor's add-a-seed box.
struct AddLook<'a> {
    title: &'a str,
    /// The note under the text before anything was asked.
    prompt: &'a str,
    /// The rows are Pandora's suggestions, not search results.
    suggested: bool,
    results_hint: &'a str,
}

/// What the add boxes say when a search fails: a plain sentence for the
/// raw answers of the site ("Service method not found (HTTP 404)"), the
/// short messages of our own (offline, login expired) kept as they are.
pub fn search_error_text(error: &str) -> String {
    let raw = ["HTTP", "not found", "Service method", "{", "errorCode", "NOT_FOUND"].iter().any(|k| error.contains(k));
    if raw { "Couldn't search Pandora right now".to_owned() } else { format!("Search failed: {}", clean(error)) }
}

/// The search box both of them share: the text, Pandora's answers below.
fn draw_search_box(frame: &mut Frame, p: &Palette, screen: Rect, search: &StationSearch, look: &AddLook) {
    let width = 64.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    let inner_w = usize::from(width.saturating_sub(2));
    let rows_room = usize::from(screen.height.saturating_sub(10)).clamp(1, 8);
    let border = p.cyan;
    let typing = search.mode == SearchMode::Typing;
    let (shown, column) = search.text.window(inner_w.saturating_sub(5), false, safe_char_width);
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" › ", Style::new().fg(border)),
            Span::styled(shown, Style::new().fg(p.fg).bold()),
        ])
        .left_aligned(),
    ];
    let note = if search.waiting {
        "Searching…".to_owned()
    } else if let Some(error) = &search.error {
        search_error_text(error)
    } else if search.asked.is_some() && search.hits.is_empty() {
        "No artists or songs found".to_owned()
    } else if search.asked.is_none() && look.suggested {
        "Suggested for this station (or type to search)".to_owned()
    } else if search.asked.is_none() {
        look.prompt.to_owned()
    } else {
        String::new()
    };
    lines.push(Line::from(Span::styled(format!(" {note}"), Style::new().fg(p.muted))).left_aligned());
    // The results scroll to keep the selected one in view.
    let len = search.hits.len();
    let start =
        search.cursor.min(len.saturating_sub(1)).saturating_sub(rows_room - 1).min(len.saturating_sub(rows_room));
    for (i, hit) in search.hits.iter().enumerate().skip(start).take(rows_room) {
        let tag = pad(hit.kind.label(), 7);
        let by = if hit.by.is_empty() { String::new() } else { format!(" · {}", clean(&hit.by)) };
        let text = fit(&format!(" {tag}{}{by}", clean(&hit.name)), inner_w);
        let style =
            if i == search.cursor.min(len.saturating_sub(1)) { selection(p, !typing) } else { Style::new().fg(p.fg) };
        lines.push(Line::from(Span::styled(pad(&text, inner_w), style)).left_aligned());
    }
    lines.push(Line::from(""));
    let hint = if typing {
        match (len > 0, look.suggested && search.text.text().trim().is_empty()) {
            (true, true) => "Enter = add the suggestion · ↓ = pick another · Esc = cancel",
            (true, false) => "Enter = search · ↓ = results · Esc = cancel",
            (false, _) => "Enter = search · Esc = cancel",
        }
    } else {
        look.results_hint
    };
    lines.push(Line::from(Span::styled(hint, Style::new().fg(p.muted))));
    lines.push(Line::from(""));
    let area = centered(screen, width, (lines.len() as u16 + 2).min(screen.height));
    frame.render_widget(Clear, area);
    let block = framed().border_style(Style::new().fg(border)).title(look.title).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(block), area);
    if typing {
        let x = area.x + 1 + 3 + column as u16;
        frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(2)), area.y + 2));
    }
}

/// The day of a Pandora timestamp (`2026-10-24T…` → `2026-10-24`); nothing
/// when it is written some other way.
fn day_of(stamp: &str) -> &str {
    let day = stamp.get(..10).unwrap_or_default();
    let ok = day.bytes().enumerate().all(|(i, b)| if matches!(i, 4 | 7) { b == b'-' } else { b.is_ascii_digit() });
    if day.len() == 10 && ok { day } else { "" }
}

/// The two dim lines at the top of the editor's Seeds tab.
const EDITOR_SEEDS_INTRO: [&str; 2] = [
    "The station plays music like these artists, songs and genres.",
    "a adds one to steer it · x removes one to stop leaning that way",
];
/// The thumbs tabs' column heads: the song and artist, and the day it was thumbed.
const EDITOR_THUMBS_SONG: &str = "Song · Artist";
const EDITOR_THUMBS_DATE: &str = "Thumbed on";
/// List-area rows (see `draw_editor`) from which the Seeds intro / Thumbs heads show.
const EDITOR_INTRO_FROM: usize = 8;
const EDITOR_HEADS_FROM: usize = 5;

/// `e`: the station editor, a popup with the Seeds, Thumbs Up and Thumbs Down tabs.
fn draw_editor(frame: &mut Frame, p: &Palette, ic: &Icons, screen: Rect, ed: &Editor) {
    let width = 76.min(screen.width.saturating_sub(2)).max(screen.width.min(24));
    let inner_w = usize::from(width.saturating_sub(2));
    let rows_room = usize::from(screen.height.saturating_sub(12)).clamp(3, 14);
    let tab = |label: String, on: bool| {
        let style = if on { selection(p, true) } else { Style::new().fg(p.muted) };
        Span::styled(format!(" {label} "), style)
    };
    let seeds_label = match &ed.seeds {
        Loaded::Ready(seeds) => format!("Seeds ({})", seeds.len()),
        _ => "Seeds".to_owned(),
    };
    let count = |positive: bool| ed.count(positive).map_or(String::new(), |n| format!(" {n}"));
    // Three tabs: Seeds, then the thumbs up and the thumbs down lists, each with its
    // icon and count. A narrow popup shortens the two thumbs titles to the icon and count.
    let thumbs_titles = |long: bool| {
        let name = |up: bool| match (long, up) {
            (true, true) => " Thumbs Up",
            (true, false) => " Thumbs Down",
            (false, _) => "",
        };
        (format!("{}{}{}", ic.up, name(true), count(true)), format!("{}{}{}", ic.down, name(false), count(false)))
    };
    let mut titles = thumbs_titles(true);
    if safe_width(&seeds_label) + safe_width(&titles.0) + safe_width(&titles.1) + 12 > inner_w {
        titles = thumbs_titles(false);
    }
    // A genre station has no Seeds tab: Pandora keeps those.
    let mut tabs = vec![Span::raw(" ")];
    if !ed.thumbs_only {
        tabs.extend([tab(seeds_label, ed.tab == EditorTab::Seeds), Span::raw("  ")]);
    }
    tabs.extend([
        tab(titles.0, ed.tab == EditorTab::ThumbsUp),
        Span::raw("  "),
        tab(titles.1, ed.tab == EditorTab::ThumbsDown),
    ]);
    let mut lines = vec![Line::from(""), Line::from(tabs).left_aligned(), Line::from("")];
    let row = |text: String, selected: bool| {
        let style = if selected { selection(p, true) } else { Style::new().fg(p.fg) };
        Line::from(Span::styled(pad(&fit(&text, inner_w), inner_w), style)).left_aligned()
    };
    let note = |text: String| {
        Line::from(Span::styled(format!(" {}", fit(&text, inner_w - 1)), Style::new().fg(p.muted))).left_aligned()
    };
    // The visible window of a list, scrolled to keep the cursor in view.
    let window = |len: usize, cursor: usize, room: usize| {
        let at = cursor.min(len.saturating_sub(1));
        let start = at.saturating_sub(room - 1).min(len.saturating_sub(room));
        (start, at)
    };
    let mut footer = String::new();
    let hint;
    if ed.tab == EditorTab::Seeds {
        hint = "a = add · x = remove · Tab = Thumbs Up · Esc = close";
        // What a seed is, in two dim lines; a short screen goes without.
        let intro = rows_room >= EDITOR_INTRO_FROM;
        if intro {
            lines.push(note(EDITOR_SEEDS_INTRO[0].to_owned()));
            lines.push(note(EDITOR_SEEDS_INTRO[1].to_owned()));
        }
        let room = rows_room - if intro { 2 } else { 0 };
        match &ed.seeds {
            Loaded::Loading => lines.push(note(format!("{} Loading the seeds…", ic.loading))),
            Loaded::Failed(e) => lines.push(note(format!("Couldn't load the seeds: {}", clean(e)))),
            Loaded::Ready(seeds) if seeds.is_empty() => lines.push(note("This station has no seeds".to_owned())),
            Loaded::Ready(seeds) => {
                let (start, at) = window(seeds.len(), ed.seed_cursor, room);
                for (i, seed) in seeds.iter().enumerate().skip(start).take(room) {
                    lines.push(row(format!(" {}{}", pad(seed.kind, 8), clean(&seed.name)), i == at));
                }
                footer = format!("{} of {}", at + 1, seeds.len());
            }
        }
    } else {
        let positive = ed.tab == EditorTab::ThumbsUp;
        hint = if positive {
            "x = take back · Tab = Thumbs Down · Esc = close"
        } else {
            if ed.thumbs_only {
                "x = take back · Tab = Thumbs Up · Esc = close"
            } else {
                "x = take back · Tab = Seeds · Esc = close"
            }
        };
        let list = ed.list(positive);
        let arrow = if positive { ic.up } else { ic.down };
        lines.push(note(format!(
            "{arrow} {}",
            if positive { "Songs you gave a thumbs up" } else { "Songs you gave a thumbs down" }
        )));
        // Column heads over the rows: the song on the left, the day you thumbed it on the right.
        let heads = rows_room >= EDITOR_HEADS_FROM;
        if heads {
            let left = format!(" {} {}", " ".repeat(arrow.width()), EDITOR_THUMBS_SONG);
            let right = EDITOR_THUMBS_DATE;
            // The dates end two cells short of the right border (see the rows below).
            let gap = inner_w.saturating_sub(left.width() + right.width() + 3);
            lines.push(note(format!("{left}{}{right}", " ".repeat(gap))));
        }
        let room = rows_room - 1 - usize::from(heads);
        match &list.state {
            FeedState::Idle | FeedState::Loading if list.items.is_empty() => {
                lines.push(note(format!("{} Loading…", ic.loading)));
            }
            FeedState::Failed(e) if list.items.is_empty() => {
                lines.push(note(format!("Couldn't load the thumbs: {}", clean(e))));
            }
            _ if list.items.is_empty() => lines.push(note("No thumbs here yet".to_owned())),
            state => {
                let (start, at) = window(list.items.len(), list.cursor, room);
                for (i, f) in list.items.iter().enumerate().skip(start).take(room) {
                    let by = if f.artist.is_empty() { String::new() } else { format!(" · {}", clean(&f.artist)) };
                    let date = day_of(&f.date);
                    let text = format!(" {} {}{by}", if f.positive { ic.up } else { ic.down }, clean(&f.title));
                    let text = if date.is_empty() {
                        text
                    } else {
                        format!(
                            "{} {date}",
                            pad(
                                &fit(&text, inner_w.saturating_sub(date.width() + 3)),
                                inner_w.saturating_sub(date.width() + 3)
                            )
                        )
                    };
                    lines.push(row(text, i == at));
                }
                let more = if matches!(state, FeedState::Loading) {
                    format!(" · {} loading more", ic.loading)
                } else {
                    String::new()
                };
                footer = format!("{} of {}{more}", at + 1, list.total.max(list.items.len()));
            }
        }
    }
    // The list area keeps a fixed height, so the popup doesn't jump.
    let used = lines.len() - 3;
    for _ in used..rows_room.max(1) {
        lines.push(Line::from(""));
    }
    lines.push(note(footer));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(hint, Style::new().fg(p.muted))));
    lines.push(Line::from(""));
    let area = centered(screen, width, (lines.len() as u16 + 2).min(screen.height));
    frame.render_widget(Clear, area);
    let heading = format!("Edit station · {}", fit(&clean(&ed.station.name), usize::from(width).saturating_sub(24)));
    let block = panel(title(heading, p), p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(block), area);
    if let Some(add) = &ed.add {
        let look = AddLook {
            title: " Add a seed ",
            prompt: "Type an artist or a song",
            suggested: add.suggested || add.suggesting || !add.suggestions.is_empty(),
            results_hint: "Enter = add it as a seed · j/k = move · / = edit · Esc = cancel",
        };
        draw_search_box(frame, p, screen, &add.search, &look);
    }
    if let Some(confirm) = &ed.confirm {
        draw_box(frame, p, screen, &BoxText::confirm(&confirm.question(&clean(&ed.station.name))));
    }
}

/// The key bar under the theme picker.
fn theme_picker_key_caps(picker: &crate::app::ThemePicker) -> Vec<(&'static str, &'static str)> {
    use crate::app::ThemeTab;
    if picker.confirm_delete.is_some() {
        return vec![("y", "Yes, Delete"), ("n Esc", "No")];
    }
    match picker.tab {
        ThemeTab::Search => vec![
            ("Type", "Fuzzy Search"),
            ("↑↓", "Move"),
            ("Enter", "Download & Use"),
            ("Tab", "Installed"),
            ("Esc", "Close"),
        ],
        ThemeTab::Installed => {
            vec![("j/k ↑↓", "Move"), ("Enter", "Use"), ("x Del", "Delete"), ("Tab", "Search"), ("Esc", "Close")]
        }
    }
}

/// The theme hints at the bottom of the picker (and the key bar's twin).
pub const THEME_SEARCH_HINT: &str = "type search · ↑↓ move · Enter use · Tab Installed · Esc close";
pub const THEME_INSTALLED_HINT: &str = "↑↓ move · Enter use · x delete · Tab Search · Esc close";

/// Settings → Theme Select: Search and Installed tabs, styled like the
/// station editor.
fn draw_theme_picker(frame: &mut Frame, app: &App, screen: Rect, picker: &crate::app::ThemePicker) {
    use crate::app::ThemeTab;
    let p = &app.palette;
    let width = 76.min(screen.width.saturating_sub(2)).max(screen.width.min(24));
    let inner_w = usize::from(width.saturating_sub(2));
    let rows_room = usize::from(screen.height.saturating_sub(14)).clamp(3, 14);
    let tab = |label: String, on: bool| {
        let style = if on { selection(p, true) } else { Style::new().fg(p.muted) };
        Span::styled(format!(" {label} "), style)
    };
    let installed = app.installed_themes();
    let search_label = if picker.loading { "Search".to_owned() } else { format!("Search ({})", picker.hits.len()) };
    let tabs = vec![
        Span::raw(" "),
        tab(search_label, picker.tab == ThemeTab::Search),
        Span::raw("  "),
        tab(format!("Installed ({})", installed.len()), picker.tab == ThemeTab::Installed),
    ];
    let mut lines = vec![Line::from(""), Line::from(tabs).left_aligned(), Line::from("")];
    let row = |text: String, selected: bool, dim: bool| {
        let style = if selected {
            selection(p, true)
        } else if dim {
            Style::new().fg(p.muted)
        } else {
            Style::new().fg(p.fg)
        };
        Line::from(Span::styled(pad(&fit(&text, inner_w), inner_w), style)).left_aligned()
    };
    let note = |text: String| {
        Line::from(Span::styled(format!(" {}", fit(&text, inner_w - 1)), Style::new().fg(p.muted))).left_aligned()
    };
    let window = |len: usize, cursor: usize, room: usize| {
        let at = cursor.min(len.saturating_sub(1));
        let start = at.saturating_sub(room - 1).min(len.saturating_sub(room));
        (start, at)
    };
    // Right-aligned tag after a name ("in use", "downloading…"), never cut:
    // the name gives way, and a margin keeps the tag off the border.
    let tagged = |name: &str, tag: &str| {
        if tag.is_empty() {
            return format!(" {}", fit(name, inner_w.saturating_sub(2)));
        }
        // Worst-case widths ("·", "…"), the same ones `fit` cuts by.
        let room = inner_w.saturating_sub(safe_width(tag) + 4);
        let name = fit(name, room);
        format!(" {name}{} {tag} ", " ".repeat(room.saturating_sub(safe_width(&name))))
    };
    let current = app.config.ui.theme();
    let mut footer = String::new();
    let hint;
    if picker.tab == ThemeTab::Search {
        hint = THEME_SEARCH_HINT;
        lines.push(
            Line::from(vec![
                Span::styled(" › ", Style::new().fg(p.accent).bold()),
                Span::styled(fit(&picker.query, inner_w.saturating_sub(5)), Style::new().fg(p.fg)),
                Span::styled("▏", Style::new().fg(p.accent)),
            ])
            .left_aligned(),
        );
        let mut room = rows_room - 1;
        // The built-in themes list at once; GitHub's join them when they come.
        if picker.loading {
            lines.push(note("Asking GitHub for iTerm2-Color-Schemes…".into()));
            room = room.saturating_sub(1).max(1);
        } else if let Some(e) = &picker.error {
            lines.push(note(format!("No theme list: {}", clean(e))));
            room = room.saturating_sub(1).max(1);
        }
        if picker.hits.is_empty() {
            if !picker.loading && picker.error.is_none() {
                lines.push(note("No theme matches".into()));
            }
        } else {
            let (start, at) = window(picker.hits.len(), picker.search_cursor, room);
            for (i, idx) in picker.hits.iter().enumerate().skip(start).take(room) {
                let entry = &picker.catalog[*idx];
                let tag = if entry.slug() == current {
                    "in use"
                } else if picker.downloading.as_deref() == Some(entry.name.as_str()) {
                    "downloading…"
                } else {
                    ""
                };
                let name = format!("{} · {}", clean(&entry.name), entry.source.label());
                lines.push(row(tagged(&name, tag), i == at, false));
            }
            footer = format!("{} of {}", at + 1, picker.hits.len());
        }
    } else {
        hint = THEME_INSTALLED_HINT;
        lines.push(note("The themes you downloaded (built-in ones are in Search)".into()));
        let room = rows_room - 1;
        if installed.is_empty() {
            lines.push(note("None yet: Tab to Search and Enter on one".into()));
        } else {
            let (start, at) = window(installed.len(), picker.installed_cursor, room);
            for (i, name) in installed.iter().enumerate().skip(start).take(room) {
                let tag = if name == current { "in use" } else { "" };
                lines.push(row(tagged(&crate::theme::label(name), tag), i == at, false));
            }
            footer = format!("{} of {}", at + 1, installed.len());
        }
    }
    let used = lines.len() - 3;
    for _ in used..rows_room.max(1) {
        lines.push(Line::from(""));
    }
    lines.push(note(footer));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(hint, Style::new().fg(p.muted))));
    lines.push(Line::from(""));
    let area = centered(screen, width, (lines.len() as u16 + 2).min(screen.height));
    frame.render_widget(Clear, area);
    let block = panel(title("Theme Select".into(), p), p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(block), area);
    if let Some(name) = &picker.confirm_delete {
        let question = format!("Delete the theme “{}”? Its file in ~/.config/boxvibe/themes/ goes too.", clean(name));
        draw_box(frame, p, screen, &BoxText { title: " Delete theme ", ..BoxText::confirm(&question) });
    }
}

// --- Player ---

/// Rows Up Next and History each show at most (History keeps 10 songs).
pub const SONG_ROWS: u16 = 10;
/// One song pane: borders, the header and [`SONG_ROWS`] rows.
pub const SONG_PANE: u16 = SONG_ROWS + 3;
/// Now Playing never gets fewer rows than this while the song panes have room to give.
pub const NOW_PLAYING_MIN: u16 = 11;

/// Now Playing's rows: whatever the song panes (one row of them, or two
/// stacked) leave; on a short screen it keeps [`NOW_PLAYING_MIN`] and the
/// song panes shrink and scroll instead.
pub fn now_playing_height(height: u16, stacked: bool) -> u16 {
    let songs = if stacked { 2 * SONG_PANE } else { SONG_PANE };
    height.saturating_sub(songs).max(NOW_PLAYING_MIN).min(height)
}

fn draw_player(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let stacked = stack_songs(area.width);
    let card_h = now_playing_height(area.height, stacked);
    let [card, songs] = Layout::vertical([Constraint::Length(card_h), Constraint::Fill(1)]).areas(area);
    draw_now_playing(frame, app, ic, card);
    // Up Next bottom-left, History bottom-right, side by side; stacked
    // (Up Next on top) when the page is too narrow for two readable tables.
    let [next, history] = if stacked {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(songs)
    } else {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).spacing(1).areas(songs)
    };
    draw_up_next(frame, app, ic, next);
    draw_history(frame, app, ic, history);
}

/// Below this many columns Up Next and History stack instead of sharing a row.
pub const STACK_BELOW: u16 = 90;

pub fn stack_songs(width: u16) -> bool {
    width < STACK_BELOW
}

fn draw_now_playing(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let station = app.station_name().unwrap_or("no station");
    let focused = app.in_songs() && app.pane == Pane::NowPlaying;
    let block = panel(title(format!("Now Playing · {station}"), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // A blank row above and below the card's content, and three columns at each side
    // (fewer on a narrow card, so the text keeps its room).
    let side = if inner.width >= 40 { NOW_PLAYING_SIDE_PAD } else { NOW_PLAYING_PAD };
    let inner = Rect {
        x: inner.x + side,
        y: inner.y + NOW_PLAYING_PAD,
        width: inner.width.saturating_sub(2 * side),
        height: inner.height.saturating_sub(2 * NOW_PLAYING_PAD),
    };
    if let Some(ad) = app.ad_view() {
        draw_ad(frame, app, ic, &ad, inner);
        return;
    }
    let Some(song) = &app.now else {
        let text = match (&app.session, &app.current_station) {
            (Session::LoggedIn, Some(_)) => format!("{} Getting songs from Pandora…", ic.loading),
            (Session::LoggedIn, None) => "Open Stations on the left, pick one and press Enter".into(),
            (Session::SigningIn, _) => format!("{} Signing in…", ic.loading),
            _ => "Not logged in: open Settings to log in".into(),
        };
        frame.render_widget(
            Paragraph::new(text).fg(p.muted).alignment(Alignment::Center),
            centered(inner, inner.width, 1),
        );
        return;
    };
    // The art fills the padded height, two cells wide per row so it looks
    // square; song info, the time line and the volume sit to its right and
    // end as far from the right border as the art is from the left one.
    let art_w = (inner.height * 2).min(inner.width / 2);
    let art = Rect { width: art_w, ..inner };
    // The picture sits against the left padding; the info starts a few
    // cells after wherever the picture really ended.
    let drawn = draw_art(frame, app, song, art);
    let info_x = (drawn.right() + 3).min(inner.right());
    let info = Rect { x: info_x, width: inner.right() - info_x, ..inner };
    let label = |text: &str| Span::styled(format!("{text:<9}"), Style::new().fg(p.muted));
    let state = if app.buffering.is_some() {
        Span::styled(format!("{} buffering…", ic.loading), Style::new().fg(p.yellow))
    } else if app.audio.is_paused() {
        Span::styled(format!("{} paused", ic.pause), Style::new().fg(p.yellow).bold())
    } else {
        Span::styled(format!("{} playing", ic.play), Style::new().fg(p.green))
    };
    let w = info.width as usize;
    let lines = vec![
        Line::from(Span::styled(fit(&song.title, w), Style::new().fg(p.fg).bold())),
        Line::raw(""),
        Line::from(vec![
            label("artist"),
            Span::styled(fit(&song.artist, w.saturating_sub(9)), Style::new().fg(p.cyan).bold()),
        ]),
        Line::from(vec![label("album"), Span::styled(fit(&song.album, w.saturating_sub(9)), Style::new().fg(p.fg))]),
        Line::from(vec![label("rating"), rating_word(song.rating, &p, ic)]),
        Line::from(vec![label("state"), state]),
    ];
    // One block (info, time, volume, buttons) centred top to bottom.
    let buttons_h = if info.height >= STACK_WITH_BUTTONS { 4 } else { 0 };
    let stack = centered(info, info.width, (9 + buttons_h).min(info.height));
    let [text, _, progress, volume, buttons] = Layout::vertical([
        Constraint::Length(6),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(buttons_h),
    ])
    .areas(stack);
    let buttons = Rect { y: buttons.y + 1, height: buttons.height.saturating_sub(1), ..buttons };
    frame.render_widget(Paragraph::new(lines), text);
    draw_progress(frame, app, song, progress);
    draw_volume(frame, app, ic, volume);
    if buttons_h > 0 {
        draw_buttons(frame, app, ic, song, buttons);
    }
}

/// Now Playing while one of Pandora's audio ads plays (Good Vibes Only): no
/// cover, no skip or thumbs, a countdown instead of the song's time.
fn draw_ad(frame: &mut Frame, app: &App, ic: &Icons, ad: &crate::app::AdView, area: Rect) {
    let p = app.palette;
    app.art.hide();
    let w = area.width as usize;
    let state = if ad.loading {
        Span::styled(format!("{} getting the ad…", ic.loading), Style::new().fg(p.yellow))
    } else if ad.paused {
        Span::styled(format!("{} paused", ic.pause), Style::new().fg(p.yellow).bold())
    } else {
        Span::styled(format!("{} playing", ic.play), Style::new().fg(p.green))
    };
    let lines = vec![
        Line::from(Span::styled("Advertisement", Style::new().fg(p.yellow).bold())).alignment(Alignment::Center),
        Line::from(Span::styled(
            fit(&format!("Ad {} of {} · your music comes right after", ad.number, ad.count), w),
            Style::new().fg(p.muted),
        ))
        .alignment(Alignment::Center),
        Line::raw(""),
        Line::from(state).alignment(Alignment::Center),
    ];
    let stack = centered(area, area.width.min(60), 6.min(area.height));
    let [text, _, progress, volume] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(0), Constraint::Length(1), Constraint::Length(1)])
            .areas(stack);
    frame.render_widget(Paragraph::new(lines), text);
    progress_bar(frame, &p, ad.elapsed.as_secs(), u64::from(ad.length), true, progress);
    draw_volume(frame, app, ic, volume);
}

/// Rows Now Playing needs before the buttons join the stack.
pub const STACK_WITH_BUTTONS: u16 = 11;

/// Blank rows between Now Playing's border and what it holds, top and bottom (and the side pad on a narrow card).
pub const NOW_PLAYING_PAD: u16 = 1;
/// Blank columns left and right of the Now Playing content.
pub const NOW_PLAYING_SIDE_PAD: u16 = 3;

/// Draws the cover, or a message that there is none, and returns where it went.
fn draw_art(frame: &mut Frame, app: &App, song: &Song, area: Rect) -> Rect {
    let p = app.palette;
    // No frame: the cover (or the message) fills the box itself.
    let inner = area;
    app.art.last_area.set(inner);
    if inner.width == 0 || inner.height == 0 {
        return inner;
    }
    let look = crate::art::Look::of(&app.config.ui, p);
    if let Some(mut cover) = app.art.protocol(&song.cover_art_url, look) {
        // Against the left edge of the box (the pane's padding), centred
        // top to bottom: the picture keeps its shape.
        let fit = cover.size_for(Resize::Fit(None), inner.as_size());
        let x = inner.x;
        let y = inner.y + inner.height.saturating_sub(fit.height) / 2;
        let at = Rect::new(x, y, fit.width.min(inner.width), fit.height.min(inner.height));
        frame.render_stateful_widget(StatefulImage::default(), at, &mut *cover);
        drop(cover);
        app.art.check_encoding(&song.cover_art_url);
        app.art.last_cover.set(at);
        return at;
    }
    app.art.hide();
    // No cover to draw: say so, in the middle of the space the cover would fill.
    let loading = app.config.ui.album_art == AlbumArt::Picture
        && !song.cover_art_url.is_empty()
        && matches!(app.art.status(&song.cover_art_url), Cover::Loading);
    // Off by choice (Shift+C) says so; a song with no cover, or one that failed, says there is none.
    let message = if app.config.ui.album_art == AlbumArt::Off {
        "Cover art off"
    } else if loading {
        "Loading cover art…"
    } else {
        "No cover art"
    };
    let line = Rect { y: inner.y + inner.height / 2, height: 1, ..inner };
    frame.render_widget(
        Paragraph::new(Span::styled(message, Style::new().fg(p.muted).add_modifier(Modifier::DIM)))
            .alignment(Alignment::Center),
        line,
    );
    inner
}

/// The four CarPlay buttons (the key bar and help show their keys).
fn draw_buttons(frame: &mut Frame, app: &App, ic: &Icons, song: &Song, area: Rect) {
    let p = app.palette;
    let play = if app.audio.is_paused() || !app.audio.is_active() { ic.play } else { ic.pause };
    let buttons = [
        (ic.down, if song.rating == Rating::Down { p.red } else { p.line }),
        (play, p.fg),
        (ic.skip, p.line),
        (ic.up, if song.rating == Rating::Up { p.green } else { p.line }),
    ];
    let w: u16 = 9;
    let cells = Layout::horizontal([Constraint::Length(w); 4]).spacing(2).split(area);
    for ((icon, color), cell) in buttons.iter().zip(cells.iter()) {
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(*color));
        frame.render_widget(
            Paragraph::new(Span::styled(*icon, Style::new().fg(*color).bold()))
                .alignment(Alignment::Center)
                .block(block),
            *cell,
        );
    }
}

fn draw_progress(frame: &mut Frame, app: &App, song: &Song, area: Rect) {
    let p = app.palette;
    progress_bar(frame, &p, app.audio.position().as_secs(), u64::from(song.length), false, area);
}

/// The time line: elapsed, the bar, then the length (`countdown`: the time left).
fn progress_bar(frame: &mut Frame, p: &Palette, position: u64, length: u64, countdown: bool, area: Rect) {
    let position = if length > 0 { position.min(length) } else { position };
    let left = format!("{} ", clock(position));
    let right = match (length > 0, countdown) {
        (true, true) => format!(" -{}", clock(length - position)),
        (true, false) => format!(" {}", clock(length)),
        (false, _) => " --:--".into(),
    };
    let bar = (area.width as usize).saturating_sub(left.len() + right.len());
    let ratio = if length > 0 { position as f64 / length as f64 } else { 0.0 };
    let done = ((bar as f64 * ratio) as usize).min(bar.saturating_sub(1));
    let line = Line::from(vec![
        Span::styled(left, Style::new().fg(p.yellow).bold()),
        Span::styled("━".repeat(done), Style::new().fg(p.yellow)),
        Span::styled("●", Style::new().fg(p.fg).bold()),
        Span::styled("─".repeat(bar.saturating_sub(done + 1)), Style::new().fg(p.line)),
        Span::styled(right, Style::new().fg(p.fg)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_volume(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let volume = app.config.player.volume;
    let steps = usize::from(volume.div_ceil(10));
    let line = Line::from(vec![
        Span::styled(format!("{} ", pad(if volume == 0 { ic.muted } else { ic.volume }, 2)), Style::new().fg(p.muted)),
        Span::styled("▮".repeat(steps), Style::new().fg(p.fg)),
        Span::styled("▯".repeat(10 - steps), Style::new().fg(p.line)),
        Span::styled(format!(" {volume}%"), Style::new().fg(p.muted)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// A whyd-style songs table: column headers, cut text ending in "…", the
/// selected row as a bar while the pane has the keys. The last column (time
/// or rating) is its own table in its own rect, so however a terminal draws
/// the song text it can never cover that column.
#[allow(clippy::too_many_arguments)]
fn song_table<'a>(
    frame: &mut Frame,
    app: &App,
    block: Block<'a>,
    header: &[&'static str],
    widths: &[Constraint],
    rows: Vec<Vec<(String, Style)>>,
    selected: usize,
    focused: bool,
    area: Rect,
) {
    let p = app.palette;
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // One blank cell on each side, equal left and right: the text and the
    // highlight bar touch neither border.
    let inner = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
    let (last_w, text_widths) = match widths.split_last() {
        Some((Constraint::Length(w), rest)) => (*w, rest),
        _ => (0, widths),
    };
    let [text_area, _, last_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(2), Constraint::Length(last_w)]).areas(inner);
    let cols = Layout::horizontal(text_widths.to_vec()).spacing(2).split(text_area);
    let n = text_widths.len();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for cells in rows {
        let mut texts = Vec::new();
        for (i, (text, style)) in cells.into_iter().enumerate() {
            if i < n {
                texts.push(Cell::from(Span::styled(fit(&text, cols[i].width as usize), style)));
            } else {
                right.push(TableRow::new([Cell::from(Span::styled(text, style))]));
            }
        }
        left.push(TableRow::new(texts));
    }
    let head = Style::new().fg(p.muted).add_modifier(Modifier::BOLD);
    let bar = if focused { selection(&p, true) } else { Style::new() };
    let gap = Rect { x: text_area.x + text_area.width, width: 2, ..inner };
    let sel = focused.then_some(selected);
    let table = Table::new(left, text_widths.to_vec())
        .header(TableRow::new(header[..n].to_vec()).style(head))
        .column_spacing(2)
        .row_highlight_style(bar);
    frame.render_stateful_widget(table, text_area, &mut TableState::default().with_selected(sel));
    // The 2-cell gap: highlighted too, so the bar runs unbroken.
    let gap_rows: Vec<TableRow> = (0..right.len().max(1)).map(|_| TableRow::new([""])).collect();
    let gap_table = Table::new(gap_rows, [Constraint::Length(2)]).header(TableRow::new([""])).row_highlight_style(bar);
    frame.render_stateful_widget(gap_table, gap, &mut TableState::default().with_selected(sel));
    if last_w > 0 {
        let table = Table::new(right, [Constraint::Length(last_w)])
            .header(TableRow::new(header[n..].to_vec()).style(head))
            .row_highlight_style(bar);
        frame.render_stateful_widget(table, last_area, &mut TableState::default().with_selected(sel));
    }
}

/// History's RATED cell: liked, disliked, or a dash.
fn rated_cell(rating: Rating, p: &Palette, ic: &Icons) -> (String, Style) {
    match rating {
        Rating::Up => (format!("{}liked", icon_cell(ic.up)), Style::new().fg(p.green)),
        Rating::Down => (format!("{}disliked", icon_cell(ic.down)), Style::new().fg(p.red)),
        Rating::None => ("—".to_owned(), Style::new().fg(p.muted)),
    }
}

fn draw_up_next(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.in_songs() && app.pane == Pane::UpNext;
    let block = panel(title(format!("Up Next ({})", app.queue.len()), &p), &p, focused);
    if app.queue.is_empty() {
        let text = if app.now.is_some() { " Asking Pandora for more…" } else { " The next songs show here" };
        frame.render_widget(Paragraph::new(text).fg(p.muted).block(block), area);
        return;
    }
    let rows = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let time = if song.length > 0 { clock(u64::from(song.length)) } else { String::new() };
            vec![
                (format!("{}", i + 1), Style::new().fg(p.muted)),
                (song.title.clone(), Style::new().fg(p.fg)),
                (song.artist.clone(), Style::new().fg(p.cyan)),
                (format!("{time:>5}"), Style::new().fg(p.muted)),
                rated_cell(song.rating, &p, ic),
            ]
        })
        .collect();
    let widths = [
        Constraint::Length(2),
        Constraint::Fill(3),
        Constraint::Fill(2),
        Constraint::Length(6),
        Constraint::Length(12),
    ];
    let header = ["#", "TITLE", "ARTIST", " TIME", "RATED"];
    song_table(frame, app, block, &header, &widths, rows, app.next_index(), focused, area);
}

fn draw_history(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.in_songs() && app.pane == Pane::History;
    let block = panel(title(format!("History ({})", app.history.len()), &p), &p, focused);
    if app.history.is_empty() {
        frame.render_widget(Paragraph::new(" Songs you played show here").fg(p.muted).block(block), area);
        return;
    }
    let rows = app
        .history
        .iter()
        .map(|song| {
            let rated = rated_cell(song.rating, &p, ic);
            vec![(song.title.clone(), Style::new().fg(p.fg)), (song.artist.clone(), Style::new().fg(p.cyan)), rated]
        })
        .collect();
    let widths = [Constraint::Fill(3), Constraint::Fill(2), Constraint::Length(12)];
    song_table(frame, app, block, &["TITLE", "ARTIST", "RATED"], &widths, rows, app.history_index(), focused, area);
}

// --- Settings ---

fn draw_settings(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content && app.prompt.is_none();
    let block = panel(title(format!("{} Settings", ic.settings), &p), &p, app.focus == Focus::Content);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
    let prompt_h = if app.prompt == Some(Prompt::Email) { 4 } else { 0 };
    let [prompt_area, list_area, about] =
        Layout::vertical([Constraint::Length(prompt_h), Constraint::Fill(1), Constraint::Length(3)]).areas(inner);
    // The email is typed in a box above the list; the password gets a popup.
    if let Some(prompt @ Prompt::Email) = app.prompt {
        draw_prompt(frame, app, prompt, Rect { height: 3, ..prompt_area });
    }
    let label_w = Setting::ALL.iter().map(|(_, s)| s.label().width()).max().unwrap_or(0) + 3;
    let mut items = Vec::new();
    let mut selected_item = 0;
    let mut group = "";
    for (i, (g, setting)) in Setting::ALL.iter().enumerate() {
        if *g != group {
            if !group.is_empty() {
                items.push(ListItem::new(""));
            }
            group = g;
            items.push(ListItem::new(Span::styled(format!(" {g}"), Style::new().fg(p.orange).bold())));
        }
        if i == app.settings_cursor {
            selected_item = items.len();
        }
        let value = app.setting_value(*setting);
        let value_style = match (setting, &app.session) {
            (Setting::LogIn, Session::LoggedIn) => Style::new().fg(p.green).bold(),
            (Setting::LogIn, Session::Failed(_)) => Style::new().fg(p.red).bold(),
            (Setting::Password, _) if value.starts_with("no keychain") => Style::new().fg(p.orange),
            (Setting::LogIn | Setting::Email | Setting::Password, _)
                if value == "not set" || value == "not logged in" || value == "not saved" =>
            {
                Style::new().fg(p.muted)
            }
            (Setting::EditFile, _) => Style::new().fg(p.cyan),
            (Setting::ThemeSelect, _) if app.theme_select_greyed() => Style::new().fg(p.muted),
            _ => Style::new().fg(p.yellow).bold(),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("   {} ", pad(ic.setting[i], 2))),
            Span::styled(
                pad(setting.label(), label_w),
                Style::new().fg(if *setting == Setting::ThemeSelect && app.theme_select_greyed() {
                    p.muted
                } else {
                    p.fg
                }),
            ),
            Span::styled(value, value_style),
        ])));
    }
    let list = List::new(items).highlight_style(if focused || app.prompt.is_some() {
        selection(&p, focused)
    } else {
        Style::new()
    });
    let mut state = ListState::default().with_selected(Some(selected_item));
    frame.render_stateful_widget(list, list_area, &mut state);
    let note = match (&app.session, app.setting()) {
        (Session::Failed(e), Setting::Email | Setting::Password | Setting::LogIn) => {
            Line::from(Span::styled(format!("{} Login failed: {e}", ic.warn), Style::new().fg(p.red)))
        }
        (_, setting) => Line::from(Span::styled(setting.about(), Style::new().fg(p.muted))),
    };
    frame.render_widget(Paragraph::new(vec![Line::raw(""), note]).wrap(Wrap { trim: true }), about);
}

fn draw_prompt(frame: &mut Frame, app: &App, prompt: Prompt, area: Rect) {
    let p = app.palette;
    let (label, mask) = match prompt {
        Prompt::Email => ("Pandora email", false),
        Prompt::Password => ("Password · Enter logs in", true),
    };
    let (shown, column) = app.query.window(usize::from(area.width.saturating_sub(3)), mask, safe_char_width);
    let block = framed()
        .border_style(Style::new().fg(p.cyan))
        .title(Span::styled(format!(" {label} "), Style::new().fg(p.cyan).bold()));
    frame.render_widget(Paragraph::new(shown).fg(p.fg).block(block), area);
    let x = area.x + 1 + column as u16;
    frame.set_cursor_position(Position::new(x.min(area.right().saturating_sub(2)), area.y + 1));
}

// --- About ---

fn draw_about(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let block = panel(title(format!("{} About", ic.about), &p), &p, app.focus == Focus::Content);
    let label = |text: &str| Span::styled(format!("   {text:<10}"), Style::new().fg(p.muted));
    let link = |text: String| Span::styled(text, Style::new().fg(p.cyan));
    let config = app.setting_value(Setting::EditFile);
    let mut lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("   {} ", crate::icons::LOGO), Style::new().fg(p.blue).bold()),
            Span::styled("box", Style::new().fg(p.blue).bold()),
            Span::styled("vibe", Style::new().fg(p.fg).bold()),
            Span::styled(format!("  {}", env!("CARGO_PKG_VERSION")), Style::new().fg(p.muted)),
        ]),
        Line::from(Span::styled("   Pandora in the terminal", Style::new().fg(p.fg))),
        Line::raw(""),
        Line::from(vec![label("github"), link(env!("CARGO_PKG_REPOSITORY").into())]),
        Line::from(vec![label("bugs"), link(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues").into())]),
        Line::from(vec![
            label("based on"),
            Span::styled("pandora.com's web API · tubevibe (the look) · Music Assistant", Style::new().fg(p.fg)),
        ]),
        Line::from(vec![label("license"), Span::styled("MIT / Apache-2.0", Style::new().fg(p.fg))]),
        Line::from(vec![label("config"), Span::styled(config, Style::new().fg(p.fg))]),
        Line::raw(""),
        Line::from(Span::styled("   UPDATES", Style::new().fg(p.orange).bold())),
    ];
    match &app.update {
        Some(update) => {
            lines.push(Line::from(vec![
                label("new"),
                Span::styled(
                    format!("{} boxvibe {} is out", ic.update, update.version),
                    Style::new().fg(p.green).bold(),
                ),
            ]));
            lines.push(Line::raw(""));
            // The Update Now button: focused with Tab, pressed with Enter; once
            // the new version is in, it is Restart Now. A package manager owns
            // this copy, so there is no download button: it says to update there.
            if update.replace != Replace::Package {
                let ready = app.update_button_ready();
                let focused = app.focus == Focus::Content;
                let style = match (ready, focused) {
                    (true, true) => Style::new().fg(p.background).bg(p.green).bold(),
                    (true, false) => Style::new().fg(p.green).bg(p.highlight).bold(),
                    (false, _) => Style::new().fg(p.muted).bg(p.highlight),
                };
                let label = if app.restart_ready() { "Restart Now" } else { "Update Now" };
                let (left, right) = if ready && focused { (" ▶ ", " ◀") } else { ("   ", "  ") };
                lines.push(Line::from(vec![
                    Span::styled(left, Style::new().fg(p.green).bold()),
                    Span::styled(format!("  {} {label}  ", ic.update), style),
                    Span::styled(right, Style::new().fg(p.green).bold()),
                ]));
                if ready && !focused {
                    lines
                        .push(Line::from(Span::styled("   (Tab to Select, Enter to Press)", Style::new().fg(p.muted))));
                }
            }
            match &app.self_update {
                Progress::Downloading(percent) => lines.push(Line::from(Span::styled(
                    format!("   {} Downloading… {percent}%", ic.loading),
                    Style::new().fg(p.yellow).bold(),
                ))),
                Progress::Done(version) => lines.push(Line::from(Span::styled(
                    format!("   {} boxvibe {version} Installed · Restart Now Opens It", ic.ok),
                    Style::new().fg(p.green).bold(),
                ))),
                Progress::Failed(error) => lines.push(Line::from(Span::styled(
                    format!("   {} Could Not Update: {error}", ic.warn),
                    Style::new().fg(p.red),
                ))),
                Progress::Idle => {}
            }
            if let Some(why) = update.replace.why_not() {
                lines.push(Line::from(Span::styled(format!("   {why}"), Style::new().fg(p.muted))));
            }
            if update.replace.why_not().is_some() || matches!(app.self_update, Progress::Failed(_)) {
                lines.push(Line::raw(""));
                if update.replace != Replace::Package {
                    lines.push(Line::from(Span::styled("   Update with:", Style::new().fg(p.fg))));
                }
                lines.push(Line::from(Span::styled(format!("   {}", update.how), Style::new().fg(p.cyan))));
            }
        }
        None if app.up_to_date => lines.push(Line::from(vec![
            label("status"),
            Span::styled(format!("{} You're on the latest version", ic.ok), Style::new().fg(p.green).bold()),
        ])),
        None if app.config.ui.check_updates && !app.demo => lines.push(Line::from(vec![
            label("status"),
            Span::styled("Asks GitHub for new versions each time boxvibe opens", Style::new().fg(p.muted)),
        ])),
        None => lines.push(Line::from(vec![
            label("status"),
            Span::styled("Not looking for new versions (Settings turns it on)", Style::new().fg(p.muted)),
        ])),
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
}

// --- Keys ---

/// The keys that work right now: (keys, what they do). The last two (help, quit) always fit.
pub fn key_caps(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.splash.is_some() {
        return Vec::new();
    }
    if app.system_open {
        return vec![("Enter", "Check Again"), ("Any Other Key", "Close")];
    }
    if app.rename.is_some() {
        return vec![("Enter", "Rename"), ("Esc", "Cancel"), ("← →", "Move"), ("Ctrl+u", "Clear"), ("Ctrl+c", "Quit")];
    }
    if app.confirm_delete.is_some() {
        return vec![("y", "Yes, Delete"), ("Other Key", "No")];
    }
    if app.confirm_unpin.is_some() {
        return vec![("y", "Yes, Unpin"), ("Other Key", "No")];
    }
    if let Some(mode) = app.confirm_good_vibes {
        let yes = if mode == GoodVibes::Muted { "Mute Anyway" } else { "Turn Off Anyway" };
        return vec![("Enter", yes), ("Esc", "Keep As Is"), ("Ctrl+c", "Quit")];
    }
    if app.genre_notice.is_some() {
        return vec![("Space", "Tick Box"), ("Enter", "OK"), ("Esc", "Close"), ("Ctrl+c", "Quit")];
    }
    if let Some(panel) = &app.panel {
        let enter = match panel.kind {
            crate::app::PanelKind::Artist { .. } => "Start Station",
            crate::app::PanelKind::Modes { .. } => "Switch Mode",
        };
        let mut keys = vec![];
        if !panel.items.is_empty() {
            keys.extend([("j/k ↑↓", "Move"), ("Enter", enter)]);
        }
        keys.push(("Esc", "Close"));
        return keys;
    }
    if let Some(picker) = &app.theme_picker {
        return theme_picker_key_caps(picker);
    }
    if let Some(ed) = &app.editor {
        return editor_key_caps(ed);
    }
    if let Some(search) = &app.add_station {
        return match search.mode {
            SearchMode::Typing if search.hits.is_empty() => {
                vec![("Enter", "Search"), ("Esc", "Cancel"), ("← →", "Move"), ("Ctrl+u", "Clear"), ("Ctrl+c", "Quit")]
            }
            SearchMode::Typing => vec![
                ("Enter", "Search"),
                ("↓", "Results"),
                ("Esc", "Cancel"),
                ("← →", "Move"),
                ("Ctrl+u", "Clear"),
                ("Ctrl+c", "Quit"),
            ],
            SearchMode::Results => {
                vec![
                    ("Enter", "Make Station"),
                    ("j/k ↑↓", "Move"),
                    ("/", "Edit"),
                    ("Esc", "Cancel"),
                    ("Ctrl+c", "Quit"),
                ]
            }
        };
    }
    if app.prompt.is_some() {
        return vec![("Enter", "Save"), ("Esc", "Cancel"), ("← →", "Move"), ("Ctrl+u", "Clear"), ("Ctrl+c", "Quit")];
    }
    if app.confirm_quit {
        return vec![("y", "Yes, Quit"), ("Other Key", "No")];
    }
    if app.details.is_some() {
        return vec![("Any Key", "Close")];
    }
    if app.help {
        return vec![("Esc", "Close"), ("?", "Close")];
    }
    let pause = if app.audio.is_paused() { "Play" } else { "Pause" };
    // A key is hinted only where it does something: what each one acts on
    // (a station row, a song row, the playing song) has to be there.
    let in_songs = app.in_songs();
    let row_here = match app.pane {
        Pane::NowPlaying => false,
        Pane::UpNext => !app.queue.is_empty(),
        Pane::History => !app.history.is_empty(),
    };
    let mut keys = Vec::new();
    // Rows to move over: not on Now Playing (one song) or About (text).
    let nothing_to_move = app.view == View::About
        || app.view == View::Notifications && app.focus == Focus::Content && app.notes.len() < 2
        || app.view == View::Pinned && app.focus == Focus::Content && app.pins.len() < 2
        || in_songs && app.pane == Pane::NowPlaying
        || app.view == View::Genres && app.focus == Focus::Content && app.genre_rows().is_empty();
    if !nothing_to_move {
        keys.push(("j/k ↑↓", "Move"));
    }
    // r, x, v: on a station (a list row, or a favorite in the menu); Pandora
    // keeps some stations (Shuffle) from being renamed or deleted.
    let station_keys = |keys: &mut Vec<(&'static str, &'static str)>, favorite: bool| {
        let Some((station, _)) = app.station_in_reach() else { return };
        keys.push(("Enter", "Play"));
        keys.push(("f", if favorite { "Unfavorite" } else { "Favorite" }));
        if station.allow_rename {
            keys.push(("r", "Rename"));
        }
        if station.allow_delete {
            keys.push(("x", "Delete"));
        }
        if station.allow_edit_seeds {
            keys.push(("e", "Edit"));
        } else if station.is_locked() && !station.is_shuffle {
            // A genre station: e shows (and takes back) your thumbs on it.
            keys.push(("e", "Thumbs"));
        }
        if !station.is_shuffle {
            keys.push(("m", "Mode"));
        }
        keys.push(("v", "Details"));
    };
    // Now Playing (its pane, or its menu entry): r and e on the playing station, never x.
    let playing_keys = |keys: &mut Vec<(&'static str, &'static str)>| {
        let Some(station) = app.playing_station_in_reach() else { return };
        if station.allow_rename {
            keys.push(("r", "Rename"));
        }
        if station.allow_edit_seeds {
            keys.push(("e", "Edit"));
        } else if station.is_locked() && !station.is_shuffle {
            keys.push(("e", "Thumbs"));
        }
        // With a song playing, Station Mode is in the `.` menu (m still works).
        if !station.is_shuffle && app.now.is_none() {
            keys.push(("m", "Mode"));
        }
    };
    // Keys the `.` song menu also offers (U/D, l/b, I, v on a song, m, B, Z)
    // are left off the bar once `.` works; they still work and Help lists them.
    let song_menu_here = app.now.is_some() || in_songs && row_here;
    match (app.focus, app.view) {
        (Focus::Menu, _) if matches!(app.menu_selected(), MenuItem::Favorite(_)) => {
            if app.station_in_reach().is_some() {
                station_keys(&mut keys, true);
            } else {
                // The favorite's station is not loaded (yet): only f, to remove it.
                keys.push(("f", "Unfavorite"));
            }
        }
        (Focus::Menu, View::Player) => {
            keys.push(("Enter", "Open"));
            playing_keys(&mut keys);
            keys.push(("Tab", "Now Playing"));
        }
        (Focus::Content, View::Player) if app.pane == Pane::NowPlaying => {
            keys.push(("Tab", "Up Next"));
            playing_keys(&mut keys);
            keys.push(("Esc", "Menu"));
        }
        (Focus::Menu, _) => {
            keys.push(("Enter", "Open"));
            playing_keys(&mut keys);
            keys.push(("Tab", "Page"));
        }
        (Focus::Content, View::Player) => {
            keys.push(("Tab", if app.pane == Pane::UpNext { "History" } else { "Menu" }));
            keys.push(("Esc", "Menu"));
        }
        (Focus::Content, View::Settings) => keys.extend([("Enter", "Change"), ("Esc", "Menu")]),
        (Focus::Content, View::Stations) => {
            if app.on_add_row {
                keys.push(("Enter", "Add Station"));
            } else {
                station_keys(&mut keys, false);
                keys.push(("a", "Add Station"));
                keys.push(("X", "Shuffle"));
            }
            keys.push(("Esc", "Menu"));
        }
        (Focus::Content, View::Genres) => {
            // Enter adds the genre's station, or asks again when the list failed.
            match &app.genres {
                GenresState::Loaded(_) if !app.genre_rows().is_empty() => keys.push(("Enter", "Add & Play")),
                GenresState::Failed(_) => keys.push(("Enter", "Try Again")),
                _ => {}
            }
            keys.push(("Esc", "Menu"));
        }
        (Focus::Content, View::Pinned) => {
            if let Some(pin) = app.pins.get(app.pinned_cursor) {
                if !pin.link.is_empty() {
                    keys.push(("Enter", "Open Page"));
                }
                keys.push(("x", "Unpin"));
            }
            keys.push(("Esc", "Menu"));
        }
        (Focus::Content, View::Notifications) => keys.push(("Esc", "Menu")),
        (Focus::Content, View::About) => {
            if app.update_button_ready() {
                keys.push(("Enter", if app.restart_ready() { "Restart Now" } else { "Update Now" }));
            }
            keys.push(("Esc", "Menu"));
        }
    }
    if app.view != View::Player && app.has_now_playing() {
        keys.push(("P", "Now Playing"));
    }
    // Pause and skip act on the playing song: only once there is one.
    if app.now.is_some() {
        keys.extend([("space", pause), ("N", "Skip")]);
    }
    if song_menu_here {
        keys.push((".", "Song Menu"));
    }
    keys.extend([("⇧←→", "Volume"), ("?", "Help"), ("Q", "Quit")]);
    keys
}

/// The key bar while the station editor is open: its own keys only.
fn editor_key_caps(ed: &Editor) -> Vec<(&'static str, &'static str)> {
    if ed.confirm.is_some() {
        return vec![("y", "Yes"), ("Other Key", "No")];
    }
    if let Some(add) = &ed.add {
        let mut keys = match add.search.mode {
            SearchMode::Typing if add.search.hits.is_empty() => vec![("Enter", "Search")],
            SearchMode::Typing if add.suggested && add.search.text.text().trim().is_empty() => {
                vec![("Enter", "Add Suggestion"), ("↓", "Pick")]
            }
            SearchMode::Typing => vec![("Enter", "Search"), ("↓", "Results")],
            SearchMode::Results => vec![("Enter", "Add Seed"), ("j/k ↑↓", "Move"), ("/", "Edit")],
        };
        keys.extend([("Esc", "Cancel"), ("Ctrl+c", "Quit")]);
        return keys;
    }
    // Tab goes round Seeds → Thumbs Up → Thumbs Down; the bar names where it goes next.
    let next = match ed.tab {
        EditorTab::Seeds => "Thumbs Up",
        EditorTab::ThumbsUp => "Thumbs Down",
        EditorTab::ThumbsDown if ed.thumbs_only => "Thumbs Up",
        EditorTab::ThumbsDown => "Seeds",
    };
    let mut keys = vec![("j/k ↑↓", "Move"), ("Tab", next)];
    match ed.tab {
        EditorTab::Seeds => keys.extend([("a", "Add Seed"), ("x", "Remove")]),
        EditorTab::ThumbsUp | EditorTab::ThumbsDown => keys.push(("x", "Take Back")),
    }
    keys.push(("m", "Mode"));
    keys.push(("Esc", "Close"));
    keys
}

/// Each key as a boxed (reverse-video) key cap, then what it does.
fn draw_keys(frame: &mut Frame, p: &Palette, keys: &[(&str, &str)], area: Rect) {
    let cap = |(key, what): &(&str, &str)| {
        [
            Span::styled(format!(" {key} "), Style::new().fg(p.background).bg(p.fg).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {what}  "), Style::new().fg(p.fg)),
        ]
    };
    let width = |spans: &[Span]| spans.iter().map(Span::width).sum::<usize>();
    // On a narrow screen the least useful hints go first, in this order; the rest
    // (Rename, Edit and the like) stay as long as they can. The last two (Help, Quit) always stay.
    const DROP_FIRST: [&str; 14] =
        ["j/k ↑↓", "⇧←→", "U/D", "N", "space", "l/b", "Esc", "v", "P", "a", "Tab", "x", "f", "Enter"];
    let split = keys.len().saturating_sub(2);
    let mut shown: Vec<&(&str, &str)> = keys.iter().collect();
    let cost = |shown: &[&(&str, &str)]| 2 + shown.iter().map(|k| width(&cap(k))).sum::<usize>();
    for gone in DROP_FIRST {
        if cost(&shown) <= area.width as usize {
            break;
        }
        // Only keys before the last two can go.
        let pinned: Vec<&(&str, &str)> = keys[split..].iter().collect();
        shown.retain(|k| k.0 != gone || pinned.iter().any(|q| std::ptr::eq(*q, *k)));
    }
    let (rest, last) = shown.split_at(shown.len().saturating_sub(2));
    let last: Vec<Span> = last.iter().copied().flat_map(cap).collect();
    let mut spans = vec![Span::raw(" ")];
    let room = (area.width as usize).saturating_sub(width(&last) + 1);
    for k in rest {
        let next = cap(k);
        if width(&spans) + width(&next) > room {
            break;
        }
        spans.extend(next);
    }
    spans.extend(last);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Help's sections: (title, [(keys, what they do)]). A test holds this to
/// the key table, so every key is listed and nothing listed is unbound.
pub const HELP: [(&str, &[(&str, &str)]); 4] = [
    (
        "PLAYING (any pane)",
        &[
            ("space p", "play / pause"),
            ("N Z", "skip to the next song · tired of this song (a month off)"),
            ("U D", "thumbs up / down the playing song (down skips it) · the same thumb again takes it back"),
            ("Shift ← →", "volume down / up (saved)"),
            ("I", "pin the selected or playing song (menu: Pinned; Enter opens its Pandora page, x unpins)"),
            ("B", "sleep timer: 15, 30, 60, 90 min, end of this song, off (fades out, then pauses)"),
            (
                ".",
                "song menu: station from artist / song, tired, share, why, pin, thumbs, artist bio & similar artists…",
            ),
            (
                "m X",
                "station mode (Deep Cuts, Discovery, Crowd Faves…; also in e) · X plays Shuffle: your stations mixed",
            ),
        ],
    ),
    (
        "THE PANE YOU ARE IN",
        &[
            (
                "l b",
                "like / bad the selected row (Now Playing: its song) · Up Next b waits for its turn, b again cancels",
            ),
            ("f", "star / unstar the selected station"),
            ("r x", "rename / delete the selected station (x asks first; on Now Playing r and e use the playing one)"),
            ("a", "add a station: search artists and songs (Stations page)"),
            ("v", "details: a song, or a station's seeds and thumbs"),
            (
                "e",
                "edit the station: Tab / Shift+Tab switch Seeds / Thumbs Up / Thumbs Down, Esc closes (genre: thumbs only)",
            ),
            ("", "Seeds: the artists, songs and genres it plays music like; a adds one, x removes one (one stays)"),
            ("", "Thumbs Up / Down: songs you rated and the day you thumbed each, one list each; x takes one back"),
            ("j k ↑ ↓", "move in the list"),
            ("Enter", "play the station · open Settings / About"),
        ],
    ),
    (
        "GOING PLACES",
        &[
            ("P", "Now Playing: the pane gets the keys"),
            ("S", "Stations: every station, Enter plays one"),
            ("G", "Genres: Pandora's genre stations, Enter adds and plays one"),
            ("O A", "Settings / About · menu Notifications (between them): what the bottom bar said"),
            ("F", "jump to FAVORITES in the menu"),
            ("Tab Esc", "menu, Now Playing, Up Next, History (Shift+Tab: back) · Esc: back to the menu"),
        ],
    ),
    (
        "LOOK AND BOXVIBE",
        &[
            ("C R H", "album art picture / off · retro HD, 64-bit … 2-bit · tint off / duotone / palette (saved)"),
            ("{ }", "menu narrower / wider (saved)"),
            ("?", "this help"),
            ("Q Ctrl+c", "quit (asks first) / quit now"),
        ],
    ),
];

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    // Every key pill is padded to the widest one, so the descriptions line up.
    let key_w = HELP.iter().flat_map(|(_, keys)| keys.iter()).map(|(k, _)| k.width()).max().unwrap_or(0);
    let mut lines = Vec::new();
    for (name, keys) in HELP {
        lines.push(Line::from(Span::styled(format!(" {name}"), Style::new().fg(p.orange).bold())));
        for (key, what) in keys {
            // A row with no key goes on explaining the one above it.
            let pill = if key.is_empty() {
                Span::raw("  ")
            } else {
                Span::styled(format!(" {key} "), Style::new().fg(p.background).bg(p.fg).bold())
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                pill,
                Span::raw(" ".repeat(if key.is_empty() { key_w + 2 } else { key_w - key.width() + 2 })),
                Span::styled((*what).to_owned(), Style::new().fg(p.fg)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    // The station icons, in the glyphs the icon set in use draws.
    let ic = icons(app.config.ui.icons);
    lines.push(Line::from(Span::styled(" STATION ICONS (right after the star)", Style::new().fg(p.orange).bold())));
    let station_icons = [
        (ic.genre_station, "genre station: made by Pandora, so it can't be renamed or edited (it can be deleted)"),
        (ic.own_station, "a station you made: you can rename, edit and delete it"),
    ];
    for (glyph, what) in station_icons {
        // Only the glyph and one cell either side wear the pill colour, so the
        // glyph sits in the middle of it; the room the glyph may need (and the
        // gap to the text) is plain background, never highlighted.
        let pill = format!(" {glyph} ");
        let gap = (key_w + 4).saturating_sub(safe_width(&pill));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(pill, Style::new().fg(p.background).bg(p.fg).bold()),
            Span::raw(" ".repeat(gap)),
            Span::styled(what.to_owned(), Style::new().fg(p.fg)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(HELP_FOOTER, Style::new().fg(p.muted))));
    // A short terminal: the blank rows between the sections go first.
    if lines.len() as u16 + 2 > area.height {
        lines.retain(|l| l.width() > 0);
    }
    // As wide as the longest line (and the title), plus the borders and one
    // blank cell; only a terminal too narrow for that wraps the lines.
    let title_w = help_title(&p).width() + 2;
    let text_w = lines.iter().map(Line::width).max().unwrap_or(0).max(title_w) as u16;
    let width = (text_w + 3).min(area.width);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    let rows: usize = lines.iter().map(|l| l.width().max(1).div_ceil(inner_w)).sum();
    let box_area = centered(area, width, (rows as u16 + 2).min(area.height));
    frame.render_widget(Clear, box_area);
    let block = panel(help_title(&p), &p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), box_area);
}

/// What boxvibe uses from the system and whether it is there, like tubevibe's
/// System Check: a mark, the name, what it is for, and under a missing one the
/// package. Plain characters only: it has to read without the font it may lack.
fn draw_system(frame: &mut Frame, app: &App, area: Rect) {
    use crate::deps::Presence;
    let p = app.palette;
    let text = Style::new().fg(p.fg);
    let muted = Style::new().fg(p.muted);
    let mut lines = vec![Line::raw("")];
    let Some(checks) = &app.system else {
        lines.push(Line::from(Span::styled(" Checking…", Style::new().fg(p.orange))));
        return draw_system_box(frame, &p, lines, area);
    };
    let name_w = checks.iter().map(|c| c.dep.name().len()).max().unwrap_or(0) + 2;
    let indent = " ".repeat(name_w + 4);
    for check in checks {
        let (mark, color, note) = match (check.presence, check.needed) {
            (Presence::Present, _) => ("✔", p.green, ""),
            (Presence::Unknown, _) if check.dep == crate::deps::Dep::Keychain => {
                ("?", p.muted, "can't tell: the keychain didn't answer in time")
            }
            (Presence::Unknown, _) => ("?", p.muted, "can't tell: fc-list (fontconfig) isn't there"),
            (Presence::Missing, true) => ("✘", p.red, "missing · needed"),
            (Presence::Missing, false) => ("·", p.muted, "not installed · optional"),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {mark} "), Style::new().fg(color).bold()),
            Span::styled(format!("{:<name_w$}", check.dep.name()), text.bold()),
            Span::styled(check.dep.why().to_owned(), text),
        ]));
        if check.presence == Presence::Present {
            continue;
        }
        lines.push(Line::from(vec![Span::raw(indent.clone()), Span::styled(note, Style::new().fg(color))]));
        if check.presence == Presence::Missing {
            lines.push(Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled(format!("package: {}", check.dep.package()), Style::new().fg(p.accent)),
            ]));
        }
    }
    let font_missing = checks.iter().any(|c| c.dep == crate::deps::Dep::NerdFont && c.presence == Presence::Missing);
    if font_missing {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  boxvibe only sees fonts installed on this computer, not the one your terminal uses. If your terminal already uses a Nerd Font, ignore this.",
            muted,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Enter checks again · any other key closes", muted)));
    draw_system_box(frame, &p, lines, area);
}

fn draw_system_box(frame: &mut Frame, p: &Palette, lines: Vec<Line<'static>>, area: Rect) {
    let width = 100.min(area.width);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    let rows: usize = lines.iter().map(|l| l.width().max(1).div_ceil(inner_w)).sum();
    let box_area = centered(area, width, (rows as u16 + 2).min(area.height));
    frame.render_widget(Clear, box_area);
    let block = panel(" System Check ", p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), box_area);
}

const HELP_FOOTER: &str = " Themes (fuzzy-search 900 more), account, boot screen and more: Settings (Shift+O)";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn the_theme_picker_never_cuts_its_tags_or_hints() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.themes_dir = Some(crate::config::temp_dir("picker-draw"));
        app.config.ui.set_theme("gruvbox");
        app.view = crate::app::View::Settings;
        app.focus = crate::app::Focus::Content;
        app.settings_cursor = crate::app::Setting::at(crate::app::Setting::ThemeSelect);
        app.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Enter));
        for cols in [80, 100, 160] {
            let (lines, _) = screen(&app, cols, 32);
            let all = lines.join("\n");
            assert!(all.contains(THEME_SEARCH_HINT), "{cols}: search hint cut\n{all}");
            assert!(all.contains("in use "), "{cols}: tag cut\n{all}");
            assert!(!all.contains("installed"), "{cols}\n{all}");
        }
        app.on_key(ratatui::crossterm::event::KeyEvent::from(ratatui::crossterm::event::KeyCode::Tab));
        for cols in [80, 160] {
            let all = screen(&app, cols, 32).0.join("\n");
            assert!(all.contains(THEME_INSTALLED_HINT), "{cols}: installed hint cut\n{all}");
        }
    }

    fn screen(app: &App, cols: u16, rows: u16) -> (Vec<String>, ratatui::buffer::Buffer) {
        let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let lines =
            (0..rows).map(|y| (0..cols).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>()).collect();
        (lines, buffer)
    }

    fn demo() -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        // The tests look for emoji (the default is the Nerd Font set).
        app.config.ui.icons = crate::config::IconSet::Emoji;
        app.demo_warm_up(3, std::time::Duration::ZERO);
        app
    }

    #[test]
    fn the_ad_card_says_advertisement_with_a_countdown_and_no_rating_line() {
        let mut app = demo();
        for _ in 0..3 {
            app.on_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('N'),
                crossterm::event::KeyModifiers::SHIFT,
            ));
            app.settle(std::time::Duration::from_millis(300));
        }
        assert!(app.ad_view().is_some());
        let text = screen(&app, 110, 34).0.join("\n");
        assert!(text.contains("Advertisement") && text.contains("Ad 1 of 2"), "{text}");
        assert!(text.contains("playing") && text.contains("-0:06"), "{text}");
        assert!(text.contains("your music comes right after"), "{text}");
    }

    /// The Now Playing art box as (top, bottom, left, right), each one
    /// cell outside the area the art may use (the old frame's position).
    fn art_box(app: &App) -> (usize, usize, usize, usize) {
        let a = app.art.last_area.get();
        assert!(a.width > 0 && a.height > 0, "art box drawn");
        (a.y as usize - 1, (a.y + a.height) as usize, a.x as usize - 1, (a.x + a.width) as usize)
    }

    /// Where the terminal's cursor is after drawing `app`, and the screen text.
    fn cursor_of(app: &App, cols: u16, rows: u16) -> (Position, Vec<String>) {
        let mut terminal = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let cursor = terminal.get_cursor_position().unwrap();
        let buffer = terminal.backend().buffer().clone();
        let lines =
            (0..rows).map(|y| (0..cols).map(|x| buffer[(x, y)].symbol().to_owned()).collect::<String>()).collect();
        (cursor, lines)
    }

    /// A demo app with the genre station popup open (`r` on Today's Hits).
    fn genre_popup_demo(set: crate::config::IconSet) -> App {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.config.ui.icons = set;
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        app.stations_cursor = app.stations.iter().position(|s| s.name == "Today's Hits").unwrap();
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert!(app.genre_notice.is_some());
        app
    }

    /// The cell column where `name` starts on the row that has it, at or after `from`.
    fn name_column(buffer: &ratatui::buffer::Buffer, name: &str, from: u16, to: u16) -> Option<u16> {
        (0..buffer.area.height).find_map(|y| {
            (from..to).find_map(|x| {
                let rest: String = (x..to).map(|i| buffer[(i, y)].symbol()).collect();
                rest.trim_start_matches(' ').starts_with(name).then_some(())?;
                // The first cell of the name itself, not the blanks before it.
                (rest.starts_with(name) && buffer[(x, y)].symbol() == &name[..1]).then_some(x)
            })
        })
    }

    /// A demo app with the station editor open on M83 Radio, on the Seeds tab.
    fn editor_demo() -> App {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        app.stations_cursor = app.stations.iter().position(|s| s.name == "M83 Radio").unwrap();
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(300));
        assert!(app.editor.is_some());
        app
    }

    /// A demo app on Now Playing, playing the station called `name`, with the
    /// keys on the Now Playing pane (`menu` false) or the menu on NOW PLAYING.
    #[test]
    fn the_notifications_page_lists_messages_under_its_menu_entry() {
        let mut app = demo();
        app.status = "Thumb taken back: Song".into();
        app.note_status();
        app.view = View::Notifications;
        app.focus = Focus::Content;
        let (lines, _) = screen(&app, 120, 30);
        let text = lines.join("\n");
        assert!(text.contains("Notifications"), "{text}");
        assert!(text.contains("just now"), "{text}");
        assert!(text.contains("Thumb taken back: Song"), "{text}");
        let settings = lines.iter().position(|l| l.contains("Settings")).unwrap();
        let about = lines.iter().position(|l| l.contains("About")).unwrap();
        assert!(lines[settings + 1].contains("Notifications") && about == settings + 2);
    }

    fn playing_demo(name: &str, menu: bool) -> App {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        app.current_station = app.stations.iter().find(|s| s.name == name).map(|s| s.id.clone());
        assert!(app.current_station.is_some(), "{name}");
        app.on_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT));
        if menu {
            app.focus = crate::app::Focus::Menu;
            app.menu_cursor = 0;
            assert_eq!(app.menu_selected(), MenuItem::NowPlaying);
        }
        app
    }

    fn press(app: &mut App, key: char) {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        app.on_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
    }

    #[test]
    fn r_and_e_work_on_now_playing_for_your_own_station_and_x_does_not() {
        for menu in [false, true] {
            let mut app = playing_demo("M83 Radio", menu);
            let keys: Vec<&str> = key_caps(&app).iter().map(|(k, _)| *k).collect();
            assert!(keys.contains(&"r") && keys.contains(&"e") && !keys.contains(&"x"), "menu={menu}: {keys:?}");
            press(&mut app, 'r');
            let rename = app.rename.take().expect("the rename box opens");
            assert_eq!(rename.station.name, "M83 Radio");
            press(&mut app, 'e');
            app.settle(std::time::Duration::from_millis(300));
            let ed = app.editor.take().expect("the editor opens");
            assert_eq!(ed.station.name, "M83 Radio");
            // Delete stays refused here, with its old message.
            press(&mut app, 'x');
            assert!(app.confirm_delete.is_none() && app.genre_notice.is_none());
            assert!(app.status.starts_with("x deletes works on a station"), "{}", app.status);
        }
    }

    #[test]
    fn closing_the_editor_or_the_rename_box_leaves_the_keys_on_now_playing() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = playing_demo("M83 Radio", false);
        for key in ['r', 'e'] {
            press(&mut app, key);
            app.settle(std::time::Duration::from_millis(300));
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            assert!(app.rename.is_none() && app.editor.is_none(), "{key}: closed");
            assert_eq!((app.view, app.focus, app.pane), (View::Player, crate::app::Focus::Content, Pane::NowPlaying));
        }
    }

    #[test]
    fn renaming_the_playing_station_changes_its_title_in_now_playing() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = playing_demo("M83 Radio", false);
        assert!(screen(&app, 120, 36).0.join("\n").contains("M83 Radio"));
        press(&mut app, 'r');
        app.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for c in "Night Drive".chars() {
            press(&mut app, c);
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(500));
        assert_eq!(app.station_name(), Some("Night Drive"));
        let text = screen(&app, 120, 36).0.join("\n");
        assert!(text.contains("Night Drive") && !text.contains("M83 Radio"), "{text}");
        assert_eq!((app.view, app.focus, app.pane), (View::Player, crate::app::Focus::Content, Pane::NowPlaying));
    }

    #[test]
    fn on_now_playing_a_genre_station_gets_the_popup_and_shuffle_just_the_message() {
        for menu in [false, true] {
            {
                let key = 'r';
                let mut app = playing_demo("Today's Hits", menu);
                let keys: Vec<&str> = key_caps(&app).iter().map(|(k, _)| *k).collect();
                assert!(!keys.contains(&"r"), "not hinted for a genre station: {keys:?}");
                assert!(keys.contains(&"e"), "e shows a genre station's thumbs: {keys:?}");
                press(&mut app, key);
                assert!(app.rename.is_none() && app.editor.is_none());
                let notice = app.genre_notice.as_ref().expect("the genre popup");
                assert_eq!(notice.station, "Today's Hits");
                assert!(app.status.contains("Pandora doesn't let you"), "{}", app.status);
                // "Don't show this again" is honoured here as well.
                let mut app = playing_demo("Today's Hits", menu);
                app.config.ui.genre_edit_popup = false;
                press(&mut app, key);
                assert!(app.genre_notice.is_none() && app.status.contains("Pandora doesn't let you"));

                let mut app = playing_demo("Shuffle", menu);
                press(&mut app, key);
                assert!(app.rename.is_none() && app.editor.is_none() && app.genre_notice.is_none());
                assert!(app.status.contains("Pandora doesn't let you"), "{}", app.status);
            }
        }
    }

    /// The key bar line of a drawn screen: the last row with text.
    fn key_bar_line(app: &App, cols: u16) -> String {
        let lines = screen(app, cols, 36).0;
        lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap().trim().to_owned()
    }

    #[test]
    fn the_key_bar_has_no_menu_size_hint_and_shows_e_where_the_editor_opens() {
        use crate::config::Favorite;
        for cols in [110u16, 50] {
            // The playing station, from the Now Playing pane and from the menu's NOW PLAYING entry.
            for menu in [false, true] {
                let app = playing_demo("M83 Radio", menu);
                let bar = key_bar_line(&app, cols);
                assert!(!bar.contains("Menu Size") && !bar.contains("{/}"), "{cols}: {bar}");
                assert!(bar.contains("r  Rename") && bar.contains("e  Edit"), "{cols} menu={menu}: {bar}");
                assert!(bar.contains("?  Help") && bar.contains("Q  Quit"), "{cols}: {bar}");
            }
            // A favorite in the menu.
            let mut app = playing_demo("M83 Radio", true);
            let id = app.stations.iter().find(|s| s.name == "Lo-Fi Beats").unwrap().id.clone();
            app.config.ui.favorites = vec![Favorite { id, name: "Lo-Fi Beats".into() }];
            app.menu_cursor = app.menu_items().iter().position(|m| matches!(m, MenuItem::Favorite(_))).unwrap();
            let bar = key_bar_line(&app, cols);
            assert!(!bar.contains("Menu Size") && bar.contains("?  Help") && bar.contains("Q  Quit"), "{cols}: {bar}");
            assert!(bar.contains("r  Rename") && bar.contains("e  Edit"), "{cols} favorite: {bar}");
            // Never for a station Pandora won't let you change, nor on the plain Stations entry.
            let genre = playing_demo("Today's Hits", false);
            assert!(!key_bar_line(&genre, cols).contains("Edit"), "genre station");
            // The Stations page: a station row has r and e too.
            let mut app = playing_demo("M83 Radio", true);
            app.view = View::Stations;
            app.focus = crate::app::Focus::Content;
            app.stations_cursor = 2;
            let bar = key_bar_line(&app, cols);
            assert!(bar.contains("r  Rename") && bar.contains("e  Edit") && !bar.contains("Menu Size"), "{bar}");
            app.focus = crate::app::Focus::Menu;
            app.menu_cursor = app.menu_items().iter().position(|m| *m == MenuItem::Stations).unwrap();
            let bar = key_bar_line(&app, cols);
            assert!(!bar.contains("Edit") && !bar.contains("Menu Size"), "{cols} Stations entry: {bar}");
        }
    }

    #[test]
    fn r_and_e_still_do_nothing_on_up_next_and_history() {
        let mut app = playing_demo("M83 Radio", false);
        for pane in [Pane::UpNext, Pane::History] {
            app.pane = pane;
            for key in ['r', 'e'] {
                press(&mut app, key);
                assert!(app.rename.is_none() && app.editor.is_none(), "{pane:?} {key}");
            }
        }
    }

    #[test]
    fn the_seeds_tab_says_what_seeds_are_and_a_short_screen_goes_without() {
        let app = editor_demo();
        let tall = screen(&app, 120, 40).0.join("\n");
        for line in EDITOR_SEEDS_INTRO {
            assert!(tall.contains(line), "intro line {line:?}\n{tall}");
        }
        assert!(tall.contains("Seeds (") && tall.contains("1 of "), "the list is still there\n{tall}");
        // The two lines are dim and never longer than the popup is wide.
        assert!(EDITOR_SEEDS_INTRO.iter().all(|l| l.width() <= 74), "fits the 76-cell popup");
        let short = screen(&app, 120, 18).0.join("\n");
        assert!(!short.contains("plays music like") && short.contains("Seeds ("), "dropped when short\n{short}");
    }

    #[test]
    fn the_thumbs_tab_has_column_heads_and_dates_that_line_up_under_thumbed_on() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = editor_demo();
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(300));
        let lines = screen(&app, 120, 40).0;
        let text = lines.join("\n");
        assert!(text.contains("Songs you gave a thumbs up") && !text.contains("press t"), "{text}");
        let head = lines.iter().find(|l| l.contains(EDITOR_THUMBS_SONG)).expect("the header row");
        assert!(head.contains(EDITOR_THUMBS_DATE), "{head}");
        // The header's last cell is the same column as every date's last cell.
        let end_of = |l: &str, word: &str| {
            let at = l.find(word).unwrap() + word.len();
            l[..at].chars().count()
        };
        let want = end_of(head, EDITOR_THUMBS_DATE);
        let dates: Vec<&String> = lines.iter().filter(|l| l.contains("2026-10-")).collect();
        assert!(dates.len() >= 3, "{text}");
        for l in dates {
            let at = l.find("2026-10-").unwrap();
            assert_eq!(l[..at].chars().count() + 10, want, "date ends under the header: {l}");
        }
        // The thumbs-down side says so, and the header is dropped on a short screen.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(300));
        let down = screen(&app, 120, 40).0.join("\n");
        assert!(down.contains("Songs you gave a thumbs down") && !down.contains("press t"), "{down}");
        assert!(down.contains(EDITOR_THUMBS_SONG) && down.contains(EDITOR_THUMBS_DATE), "{down}");
        let short = screen(&app, 120, 16).0.join("\n");
        assert!(!short.contains(EDITOR_THUMBS_DATE) && short.contains("Songs you gave"), "{short}");
    }

    /// The editor's tab row and its key bar, as drawn.
    fn editor_tab_row(app: &App, cols: u16, rows: u16) -> String {
        let lines = screen(app, cols, rows).0;
        let at = lines.iter().position(|l| l.contains("Edit station")).expect("the editor");
        lines[at + 2].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn now_playing_keeps_three_blank_columns_inside_each_side() {
        let app = playing_demo("Today's Hits", false);
        let lines = screen(&app, 160, 40).0;
        let top = lines.iter().position(|l| l.contains("Now Playing ·")).expect("the card");
        let chars: Vec<char> = lines[top].chars().collect();
        let title_at = lines[top].find("Now Playing").map(|b| lines[top][..b].chars().count()).unwrap();
        let left = (0..title_at).rev().find(|&x| matches!(chars[x], '╭' | '┌' | '┏')).expect("left corner");
        let right = (title_at..chars.len()).find(|&x| matches!(chars[x], '╮' | '┐' | '┓')).expect("right corner");
        let mut rows = 0;
        for line in &lines[top + 1..] {
            let c: Vec<char> = line.chars().collect();
            if matches!(c[left], '╰' | '└' | '┗') {
                break;
            }
            rows += 1;
            assert!(c[left + 1..=left + 3].iter().all(|&ch| ch == ' '), "left pad: {line}");
            assert!(c[right - 3..right].iter().all(|&ch| ch == ' '), "right pad: {line}");
        }
        assert!(rows > 3, "the card has rows");
        assert!(lines.join("\n").contains(app.now.as_ref().unwrap().title.as_str()), "the song still shows");
    }

    #[test]
    fn a_genre_station_editor_shows_only_the_two_thumbs_tabs() {
        let mut app = editor_demo();
        let ed = app.editor.as_mut().unwrap();
        ed.thumbs_only = true;
        ed.tab = EditorTab::ThumbsDown;
        let row = editor_tab_row(&app, 120, 40);
        assert!(!row.contains("Seeds") && row.contains("Thumbs Up") && row.contains("Thumbs Down"), "{row}");
        let bar = key_bar_line(&app, 120);
        assert!(bar.contains("Tab  Thumbs Up") && !bar.contains("Add Seed"), "{bar}");
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("x = take back · Tab = Thumbs Up"), "{text}");
    }

    #[test]
    fn the_editor_has_three_tabs_with_the_thumb_icons_and_counts_in_their_titles() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let press = |app: &mut App, code| {
            app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            app.settle(std::time::Duration::from_millis(300));
        };
        let mut app = editor_demo();
        let icons = crate::icons::icons(app.config.ui.icons);
        let (up, down) = (icons.up, icons.down);
        // Wide: names and counts (the counts are in before a tab is opened).
        let ed = app.editor.as_ref().unwrap();
        let (seeds, ups, downs) = (ed.seed_list().len(), ed.count(true).unwrap(), ed.count(false).unwrap());
        let row = editor_tab_row(&app, 120, 40);
        assert!(row.contains(&format!("Seeds ({seeds}) {up} Thumbs Up {ups} {down} Thumbs Down {downs}")), "{row}");
        // Narrow: just the icon and the count, still three tabs.
        let row = editor_tab_row(&app, 50, 30);
        assert!(
            row.contains(&format!("{up} {ups}")) && row.contains(&format!("{down} {downs}")) && !row.contains("Thumbs"),
            "{row}"
        );
        // The hints name the tab Tab goes to, and `t` is gone from them.
        let bar = key_bar_line(&app, 120);
        assert!(bar.contains("Tab  Thumbs Up") && bar.contains("a  Add Seed"), "{bar}");
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("Tab = Thumbs Up") && !text.contains("t = up/down"), "{text}");
        press(&mut app, KeyCode::Tab);
        let bar = key_bar_line(&app, 120);
        assert!(bar.contains("Tab  Thumbs Down") && bar.contains("x  Take Back"), "{bar}");
        assert!(!bar.contains("Show Down") && !bar.contains("t  "), "{bar}");
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("x = take back · Tab = Thumbs Down") && !text.contains("t = up/down"), "{text}");
        press(&mut app, KeyCode::Tab);
        let bar = key_bar_line(&app, 120);
        assert!(bar.contains("Tab  Seeds") && bar.contains("x  Take Back"), "{bar}");
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("x = take back · Tab = Seeds"), "{text}");
        // Once a list is loaded its total is the count in the title.
        let row = editor_tab_row(&app, 120, 40);
        assert!(row.contains(&format!("{down} Thumbs Down {downs}")), "{row}");
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.editor.as_ref().unwrap().tab, crate::app::EditorTab::Seeds);
    }

    #[test]
    fn a_thumb_without_a_date_shows_a_blank_not_junk() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = editor_demo();
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.settle(std::time::Duration::from_millis(300));
        let ed = app.editor.as_mut().unwrap();
        let list = &mut ed.up;
        list.items[0].date = String::new();
        list.items[1].date = "not a date".to_owned();
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(!text.contains("not a date") && !text.contains("None"), "{text}");
    }

    #[test]
    fn help_explains_seeds_and_thumbs_under_the_editor() {
        for (cols, rows) in [(255u16, 70u16), (160, 40)] {
            let mut app = demo();
            app.help = true;
            let text = screen(&app, cols, rows).0.join("\n");
            assert!(text.contains("Seeds: the artists, songs and genres it plays music like"), "{cols}x{rows}\n{text}");
            assert!(
                text.contains("Thumbs Up / Down: songs you rated and the day you thumbed each"),
                "{cols}x{rows}\n{text}"
            );
            assert!(!text.contains("t switches"), "{cols}x{rows}\n{text}");
        }
    }

    #[test]
    fn every_station_row_starts_its_name_in_the_same_column() {
        use crate::config::{Favorite, IconSet};
        for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
            let mut app = demo();
            app.config.ui.icons = set;
            app.view = View::Stations;
            app.focus = crate::app::Focus::Content;
            // Starred and not, genre, yours, shared, Shuffle and the one playing.
            for name in ["M83 Radio", "Today's Hits", "Shuffle", "Synthwave Radio"] {
                let st = app.stations.iter().find(|s| s.name == name).unwrap();
                app.config.ui.favorites.push(Favorite { id: st.id.clone(), name: st.name.clone() });
            }
            app.current_station = app.stations.iter().find(|s| s.name == "Lo-Fi Beats").map(|s| s.id.clone());
            for cursor in [0, 3] {
                app.stations_cursor = cursor; // the highlighted row lines up too
                let buffer = screen(&app, 120, 34).1;
                let offsets: Vec<(&str, Option<u16>)> = [
                    "Shuffle",          // starred, no kind icon
                    "Thumbprint Radio", // yours, no star
                    "M83 Radio",        // yours, starred
                    "Lo-Fi Beats",      // yours, playing
                    "Classic Rock Radio",
                    "Synthwave Radio", // shared, starred
                    "Today's Hits",    // genre, starred
                ]
                .iter()
                .map(|n| (*n, name_column(&buffer, n, 31, 120)))
                .collect();
                let first = offsets[0].1.expect("Shuffle is listed");
                for (name, at) in &offsets {
                    assert_eq!(*at, Some(first), "{set:?} (cursor {cursor}): {name} is off: {offsets:?}");
                }
            }
            // The logo and person sit in one column: same x on every row that has one.
            let ic = icons(set);
            let buffer = screen(&app, 120, 34).1;
            let x_of = |glyph: &str, name: &str| {
                (0..buffer.area.height).find_map(|y| {
                    let line: String = (31..120).map(|i| buffer[(i, y)].symbol()).collect();
                    line.contains(name).then(|| (31..120).find(|&i| buffer[(i, y)].symbol() == glyph))?
                })
            };
            let person = [x_of(ic.own_station, "M83 Radio"), x_of(ic.own_station, "Thumbprint Radio")];
            assert!(person[0].is_some() && person[0] == person[1], "{set:?}: person icons {person:?}");
            let genre = x_of(ic.genre_station, "Today's Hits");
            assert!(genre.is_some() && genre == person[0], "{set:?}: logo {genre:?} vs person {person:?}");
        }
    }

    #[test]
    fn favorites_in_the_menu_line_up_and_the_add_row_sits_over_the_radio_icons() {
        use crate::config::{Favorite, IconSet};
        for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
            let mut app = demo();
            app.config.ui.icons = set;
            app.view = View::Stations;
            // A genre station, one of yours, Shuffle, and one not loaded (no mark yet).
            for name in ["Today's Hits", "M83 Radio", "Shuffle"] {
                let st = app.stations.iter().find(|s| s.name == name).unwrap();
                app.config.ui.favorites.push(Favorite { id: st.id.clone(), name: st.name.clone() });
            }
            app.config.ui.favorites.push(Favorite { id: "gone".into(), name: "Quiet Storm".into() });
            let buffer = screen(&app, 120, 34).1;
            let at: Vec<_> = ["Today's Hits", "M83 Radio", "Shuffle", "Quiet Storm"]
                .iter()
                .map(|n| (*n, name_column(&buffer, n, 1, 29)))
                .collect();
            assert!(at[0].1.is_some() && at.iter().all(|(_, x)| *x == at[0].1), "{set:?}: {at:?}");
        }
    }

    #[test]
    fn a_narrow_stations_list_cuts_the_names_and_keeps_the_icons() {
        use crate::config::IconSet;
        for set in [IconSet::Nerd, IconSet::Unicode, IconSet::Emoji] {
            let mut app = demo();
            app.config.ui.icons = set;
            app.view = View::Stations;
            let ic = icons(set);
            let (lines, _) = screen(&app, 56, 34);
            let row = lines.iter().find(|l| l.contains(ic.genre_station)).expect("the genre row");
            assert!(row.contains(ic.station), "{set:?}: {row}");
            let rows: Vec<&String> = lines.iter().filter(|l| l.contains(ic.own_station)).collect();
            assert!(!rows.is_empty() && rows.iter().any(|r| r.contains('…')), "{set:?}: names are cut: {rows:?}");
        }
    }

    #[test]
    fn the_genre_popup_explains_the_problem_and_both_icons() {
        use crate::config::IconSet;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
            let ic = icons(set);
            let mut app = genre_popup_demo(set);
            let (lines, _) = screen(&app, 110, 34);
            let text = lines.join("\n");
            for want in [
                "Genre station ─",
                "“Today's Hits” can't be renamed.",
                "made by Pandora",
                "Genre station: made by Pandora",
                "Your own station: you made it",
                "[ ] Don't show this again",
                "Space = tick the box · Enter = OK · Esc = close",
                "edit popup.",
            ] {
                assert!(text.contains(want), "{set:?}: {want:?} in:\n{text}");
            }
            // Each meaning has its glyph in front of it.
            let row = |what: &str| lines.iter().find(|l| l.contains(what)).unwrap().clone();
            let (g, o) = (row("Genre station: made"), row("Your own station: you"));
            assert!(
                g.contains(ic.genre_station) && g[..g.find("Genre station: made").unwrap()].contains(ic.genre_station)
            );
            assert!(o.contains(ic.own_station) && o[..o.find("Your own station").unwrap()].contains(ic.own_station));
            // The bar at the bottom still has the error, and its keys.
            assert!(lines[32].contains("Pandora doesn't let you rename “Today's Hits”"), "{}", lines[32]);
            assert!(
                lines[33].contains("Space") && lines[33].contains("Enter") && lines[33].contains("Esc"),
                "{}",
                lines[33]
            );
            // The box ticks with Space.
            app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
            let text = screen(&app, 110, 34).0.join("\n");
            assert!(text.contains("[x] Don't show this again") && !text.contains("[ ] Don't"), "{text}");
        }
        // A small terminal keeps the box, the checkbox and the keys.
        let app = genre_popup_demo(IconSet::Unicode);
        for (cols, rows) in [(60u16, 24u16), (60, 18), (50, 16)] {
            let lines = screen(&app, cols, rows).0;
            let text = lines.join("\n");
            assert!(text.contains("Genre station ─") && text.contains("[ ] Don't show"), "{cols}x{rows}:\n{text}");
            // The hint line is the first to go on a very short screen; the key bar always has the keys.
            assert!(rows < 18 || text.contains("Esc = close"), "{cols}x{rows}:\n{text}");
            assert!(lines[rows as usize - 1].contains("Esc"), "{cols}x{rows}:\n{text}");
        }
    }

    #[test]
    fn help_explains_the_station_icons_in_the_active_set() {
        use crate::config::IconSet;
        for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
            let ic = icons(set);
            let mut app = demo();
            app.config.ui.icons = set;
            app.help = true;
            for (cols, rows) in [(160u16, 40u16), (255, 70)] {
                let lines = screen(&app, cols, rows).0;
                let all = lines.join("\n");
                assert!(all.contains("STATION ICONS"), "{set:?} {cols}x{rows}:\n{all}");
                let row = |what: &str| lines.iter().find(|l| l.contains(what)).cloned().unwrap_or_default();
                let g = row("genre station: made by Pandora");
                let o = row("a station you made");
                assert!(g.contains(ic.genre_station), "{set:?} {cols}x{rows}: {g}");
                assert!(g.contains("can't be renamed or edited") && g.contains("can be deleted"), "{g}");
                assert!(
                    o.contains(ic.own_station) && o.contains("rename, edit and delete"),
                    "{set:?} {cols}x{rows}: {o}"
                );
                assert!(all.contains(HELP_FOOTER.trim()), "{cols}x{rows}: the footer is still there");
            }
        }
    }

    #[test]
    fn help_icons_sit_in_the_middle_of_their_highlight_with_no_extra_colour_on_the_right() {
        use crate::config::IconSet;
        use unicode_width::UnicodeWidthStr;
        for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
            let ic = icons(set);
            let mut app = demo();
            app.config.ui.icons = set;
            app.help = true;
            let fg = app.palette.fg;
            for (cols, rows) in [(160u16, 40u16), (255, 70)] {
                let (lines, buffer) = screen(&app, cols, rows);
                for (glyph, what) in
                    [(ic.genre_station, "genre station: made by Pandora"), (ic.own_station, "a station you made")]
                {
                    let y = lines.iter().position(|l| l.contains(what)).unwrap() as u16;
                    let lit: Vec<u16> = (0..cols).filter(|&x| buffer[(x, y)].bg == fg).collect();
                    let (first, last) = (lit[0], *lit.last().unwrap());
                    let w = glyph.width().max(1) as u16;
                    let ctx = format!("{set:?} {cols}x{rows} {what}");
                    assert_eq!(last - first + 1, w + 2, "{ctx}: one cell either side of the glyph, nothing more");
                    // (The second cell of a wide glyph carries no colour of its own.)
                    assert!(lit.len() as u16 + 1 >= w + 2, "{ctx}: the highlight has no gaps");
                    assert_eq!(buffer[(first, y)].symbol(), " ", "{ctx}");
                    assert_eq!(buffer[(first + 1, y)].symbol(), glyph, "{ctx}: the glyph is first inside the block");
                    assert_eq!(buffer[(last, y)].symbol(), " ", "{ctx}");
                }
            }
        }
    }

    #[test]
    fn the_settings_page_lists_the_genre_popup_row() {
        let mut app = demo();
        app.view = View::Settings;
        app.focus = crate::app::Focus::Content;
        app.settings_cursor = crate::app::Setting::at(crate::app::Setting::GenrePopup);
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("Genre station edit popup"), "{text}");
        let line = text.lines().find(|l| l.contains("Genre station edit popup")).unwrap();
        assert!(line.contains(" on"), "{line}");
    }

    #[test]
    fn the_password_is_asked_in_a_popup_that_says_where_it_goes() {
        use crate::app::KeychainState;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.config.pandora.username = "you@example.com".into();
        app.keychain_state = KeychainState::NotSaved;
        app.ask_password_for_snapshot("s3cret");
        assert!(app.overlay_open(), "a popup: the page behind is dimmed and its keys are off");
        let (cursor, lines) = cursor_of(&app, 110, 34);
        let text = lines.join("\n");
        for want in [
            "Pandora password",
            "Password for you@example.com",
            "system keychain",
            "Secret Service",
            "never written to config.toml",
            "Enter = log in and save",
            "Esc = cancel",
        ] {
            assert!(text.contains(want), "{want:?} in:\n{text}");
        }
        assert!(!text.contains("session only"), "a keychain is there: {text}");
        assert!(!text.contains("s3cret"), "masked");
        let row = lines.iter().position(|l| l.contains("› ••••••")).expect("six dots");
        assert_eq!(usize::from(cursor.y), row, "the cursor is on the input row");
        assert!(lines[row].chars().nth(usize::from(cursor.x) - 1) == Some('•'), "right after the dots: {}", lines[row]);
        // Without a keychain it says the password lasts this session only.
        app.keychain_state = KeychainState::Unavailable;
        let text = screen(&app, 110, 34).0.join("\n");
        assert!(text.contains("No system keychain was found"), "{text}");
        assert!(text.contains("session only"), "{text}");
        assert!(text.contains("never written to config.toml"), "{text}");
        assert!(text.contains("Enter = log in ·") && !text.contains("and save"), "{text}");
        // Keychain not asked yet: it says what it will try.
        app.keychain_state = KeychainState::Unknown;
        let text = screen(&app, 110, 34).0.join("\n");
        assert!(text.contains("If there is no keychain it is kept for this session only"), "{text}");
        // The risk notice is in every variant (checked on one line: it wraps).
        for state in [KeychainState::NotSaved, KeychainState::Unavailable, KeychainState::Unknown] {
            app.keychain_state = state;
            let (_, lines) = cursor_of(&app, 110, 34);
            let flat: String =
                lines.iter().map(|l| l.chars().skip(22).take(66).collect::<String>()).collect::<Vec<_>>().join(" ");
            let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
            let flat: String = flat.replace("│ │", "").replace("│", "");
            let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(flat.contains(PASSWORD_DISCLAIMER), "{state:?}: {flat}");
        }
        // Small terminals still fit all of it, with the input and its cursor on screen.
        for (cols, rows) in [(60u16, 20u16), (60, 24), (70, 18), (50, 16)] {
            let (cursor, lines) = cursor_of(&app, cols, rows);
            let text = lines.join("\n");
            assert!(text.contains("Pandora password"), "{cols}x{rows}:\n{text}");
            assert!(rows < 18 || text.contains("Esc = cancel"), "{cols}x{rows}:\n{text}");
            assert!(text.contains("responsible"), "{cols}x{rows}:\n{text}");
            assert!(text.contains("› "), "{cols}x{rows}: the input row:\n{text}");
            assert!(lines[usize::from(cursor.y)].contains("› "), "{cols}x{rows}: cursor on the input row");
        }
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.prompt.is_none() && !app.overlay_open(), "Esc closes it");
        let text = screen(&app, 110, 34).0.join("\n");
        assert!(!text.contains("Pandora password"), "{text}");
        app.open_prompt(Prompt::Email);
        assert!(app.prompt == Some(Prompt::Email) && !app.overlay_open(), "the email box is not a popup");
        let text = screen(&app, 110, 34).0.join("\n");
        assert!(!text.contains("Pandora password"), "the email stays in its own box: {text}");
    }

    /// The words inside the "Turn off Good Vibes Only?" popup, joined by single spaces.
    fn good_vibes_popup_words(app: &App, cols: u16, rows: u16) -> Option<String> {
        let lines = screen(app, cols, rows).0;
        let width = usize::from(66.min(cols.saturating_sub(4)).max(cols.min(8)));
        let top = lines.iter().position(|l| l.contains("Turn off Good Vibes"))?;
        let left = lines[top].chars().take_while(|c| *c != 'T').count().saturating_sub(2);
        let words: Vec<String> = lines[top + 1..]
            .iter()
            .map(|l| l.chars().skip(left + 1).take(width - 2).collect::<String>())
            .take_while(|l| !l.contains('─'))
            .collect();
        Some(words.join(" ").split_whitespace().collect::<Vec<_>>().join(" "))
    }

    #[test]
    fn turning_good_vibes_only_off_asks_first_in_a_risk_popup_that_fits_small_terminals() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.view = View::Settings;
        app.focus = crate::app::Focus::Content;
        app.settings_cursor = crate::app::Setting::at(crate::app::Setting::GoodVibesOnly);
        // The row and its description use the new name.
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("Good Vibes Only") && !text.contains("Good Guy"), "{text}");
        assert!(good_vibes_popup_words(&app, 120, 40).is_none(), "no popup yet");
        // On → Muted asks first, with its own gray-area notice.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.confirm_good_vibes, Some(GoodVibes::Muted));
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("Mute Pandora's ads?") && text.contains("Enter = mute them anyway"), "{text}");
        assert!(text.contains("gray area") && text.contains("Mute Anyway"), "{text}");
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.config.player.good_vibes_only, GoodVibes::Muted);
        // Muted → Off asks too.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.confirm_good_vibes == Some(GoodVibes::Off) && app.overlay_open());
        assert_eq!(app.config.player.good_vibes_only, GoodVibes::Muted);
        let text = screen(&app, 120, 40).0.join("\n");
        assert!(text.contains("Turn off Good Vibes Only?") && text.contains("Enter = turn it off anyway"), "{text}");
        assert!(text.contains("Esc = keep it as it was") && !text.contains("http"), "{text}");
        assert!(text.contains("Turn Off Anyway") && text.contains("Keep As Is"), "the key bar: {text}");
        // The whole risk notice is there, at any size that has room for it.
        for (cols, rows) in [(120u16, 40u16), (80, 24), (60, 20), (50, 16), (40, 16), (40, 14)] {
            let words = good_vibes_popup_words(&app, cols, rows).unwrap_or_default();
            assert!(words.contains(GOOD_VIBES_DISCLAIMER), "{cols}x{rows}: {words}");
            assert!(words.contains("Esc = keep"), "{cols}x{rows}: {words}");
        }
        // A tiny terminal still draws a popup inside the screen, from the start of the notice.
        for (cols, rows) in [(30u16, 8u16), (24, 6)] {
            let (lines, _) = screen(&app, cols, rows);
            assert!(lines.iter().any(|l| l.contains("Turn off")), "{cols}x{rows}");
            assert!(lines.iter().any(|l| l.contains("Skipping")), "{cols}x{rows}: {lines:?}");
        }
        // Esc leaves it on, closes the popup; Enter in a second round turns it off.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.confirm_good_vibes.is_none() && app.config.player.good_vibes_only == GoodVibes::Muted);
        assert!(!screen(&app, 120, 40).0.join("\n").contains("Turn off Good Vibes Only?"));
    }

    #[test]
    fn the_rename_box_edits_in_the_middle_and_draws_the_cursor_there() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let press = |app: &mut App, code| app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        press(&mut app, KeyCode::Char('r'));
        let name = app.rename.as_ref().unwrap().text.text().to_owned();
        let (end, lines) = cursor_of(&app, 120, 36);
        let row = lines.iter().position(|l| l.contains(" › ")).expect("the input row");
        assert_eq!(usize::from(end.y), row);
        // Home puts the cursor on the first letter; typing goes in before it.
        press(&mut app, KeyCode::Home);
        let (home, _) = cursor_of(&app, 120, 36);
        assert_eq!(end.x - home.x, name.width() as u16, "the cursor moved to the start");
        press(&mut app, KeyCode::Char('>'));
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Delete);
        let (moved, lines) = cursor_of(&app, 120, 36);
        let shown = lines[row].split(" › ").nth(1).unwrap().trim_end_matches(['│', ' ']).to_owned();
        let mut chars = name.chars();
        let first = chars.next().unwrap();
        chars.next();
        let expected = format!(">{first}{}", chars.collect::<String>());
        assert!(shown.starts_with(&expected), "{shown:?} vs {expected:?}");
        assert_eq!(moved.x - home.x, 2, "after the typed > and one step right");
    }

    #[test]
    fn the_password_prompt_masks_and_keeps_the_cursor_where_it_is() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.view = View::Settings;
        app.open_prompt(Prompt::Password);
        for c in "secret".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        let (end, lines) = cursor_of(&app, 100, 30);
        assert!(lines.iter().any(|l| l.contains("••••••")), "masked");
        assert!(!lines.iter().any(|l| l.contains("secret")));
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        let (back, _) = cursor_of(&app, 100, 30);
        assert_eq!(end.x - back.x, 2);
        assert_eq!(end.y, back.y);
    }

    #[test]
    fn the_time_line_ends_as_far_from_the_right_border_as_the_cover_starts_from_the_left() {
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        for (cols, rows) in [(200u16, 50u16), (160, 40), (120, 36)] {
            let (lines, _) = screen(&app, cols, rows);
            let grid: Vec<Vec<char>> = lines.iter().map(|l| l.chars().collect()).collect();
            let y = grid.iter().position(|r| r.contains(&'●')).expect("time line");
            let row = &grid[y];
            let cover_x = grid.iter().filter_map(|r| r.iter().position(|c| matches!(c, '▀' | '▄'))).min().unwrap();
            let left = (0..cover_x).rev().find(|&x| "│┃║".contains(row[x])).expect("left border");
            let dot = row.iter().position(|&c| c == '●').unwrap();
            let right = (dot..row.len()).find(|&x| "│┃║".contains(row[x])).expect("right border");
            let end = (dot..right).rev().find(|&x| row[x] != ' ').unwrap();
            assert!(row[end].is_ascii_digit(), "{cols}x{rows}: ends with the total time: {}", lines[y]);
            assert_eq!(right - end, cover_x - left, "{cols}x{rows}: symmetrical\n{}", lines.join("\n"));
        }
    }

    #[test]
    #[allow(deprecated)]
    fn the_half_block_stand_in_fills_the_pictures_rectangle_and_nothing_moves() {
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        let mut sixel = ratatui_image::picker::Picker::from_fontsize(ratatui_image::FontSize::new(8, 17));
        sixel.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
        app.art.set_graphics(Some(sixel));
        for (cols, rows) in [(255u16, 70u16), (160, 40)] {
            let mut seen = Vec::new();
            for help in [false, true, false] {
                app.help = help;
                app.art.set_overlay(help);
                let (lines, buf) = screen(&app, cols, rows);
                let cover = app.art.last_cover.get();
                // The time line's total, at its right end (the start may be under Help).
                let time = lines
                    .iter()
                    .position(|l| {
                        l.contains("──")
                            && l.trim_end_matches([' ', '│']).ends_with(|c: char| c.is_ascii_digit())
                            && l.contains(":")
                    })
                    .map(|y| (y, lines[y].trim_end_matches([' ', '│']).chars().count()));
                assert!(time.is_some(), "{cols}x{rows}: time line\n{}", lines.join("\n"));
                seen.push((cover, time));
                // At 160 columns Help covers the whole cover; at 255 it shows beside it.
                if help && cols > 200 {
                    let inside: String = cover.positions().map(|at| buf[at].symbol().to_owned()).collect();
                    assert!(inside.contains('▀') || inside.contains('▄'), "{cols}x{rows}: half blocks under Help");
                }
            }
            assert!(seen.windows(2).all(|w| w[0] == w[1]), "{cols}x{rows}: same cover and info with Help: {seen:?}");
        }
    }

    #[test]
    #[allow(deprecated)]
    fn a_song_change_under_help_fetches_and_shows_the_new_cover() {
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        let mut sixel = ratatui_image::picker::Picker::from_fontsize(ratatui_image::FontSize::new(8, 17));
        sixel.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
        app.art.set_graphics(Some(sixel));
        let frame = |app: &mut App| {
            app.tick();
            app.art.set_overlay(app.overlay_open());
            screen(app, 255, 70);
        };
        for _ in 0..10 {
            frame(&mut app);
            app.settle(std::time::Duration::from_millis(50));
        }
        let old = app.now.as_ref().unwrap().cover_art_url.clone();
        assert_eq!(app.art.shown_cover(), Some((old.clone(), false)), "the picture");
        app.help = true;
        frame(&mut app);
        assert_eq!(app.art.shown_cover(), Some((old.clone(), true)), "half blocks under Help");
        app.demo_warm_up(1, std::time::Duration::ZERO);
        let new = app.now.as_ref().unwrap().cover_art_url.clone();
        assert_ne!(new, old);
        for _ in 0..20 {
            frame(&mut app);
            app.settle(std::time::Duration::from_millis(50));
        }
        assert!(app.help, "Help still open");
        assert_eq!(app.art.shown_cover(), Some((new.clone(), true)), "the new cover, in half blocks");
        app.help = false;
        frame(&mut app);
        assert_eq!(app.art.shown_cover(), Some((new, false)), "the new picture once Help closes");
    }

    #[test]
    fn settings_has_no_column_headers_and_starts_with_its_first_section() {
        let mut app = demo();
        app.view = View::Settings;
        let (lines, _) = screen(&app, 160, 40);
        let all = lines.join("\n");
        assert!(!all.contains("SETTING") && !all.contains("VALUE"), "{all}");
        let top = lines.iter().position(|l| l.contains("Settings")).unwrap();
        let first = Setting::ALL[0].0;
        assert!(lines[top + 1].contains(first), "{first} right under the border\n{all}");
    }

    #[test]
    fn colours_behind_a_popup_fade_toward_the_background() {
        let bg = Color::Rgb(20, 20, 30);
        let Color::Rgb(r, g, b) = dim_color(Color::Rgb(250, 200, 0), bg) else { panic!() };
        assert!(r < 130 && g < 110 && b > 20, "darker, toward the background: {r} {g} {b}");
        assert!(r.abs_diff(b) < 70, "less saturated: {r} vs {b}");
        assert_eq!(dim_color(bg, bg), bg, "the background stays");
        assert_eq!(dim_color(Color::Yellow, bg), Color::DarkGray);
        assert_eq!(dim_color(Color::Indexed(208), bg), Color::DarkGray);
        assert_eq!(dim_color(Color::Reset, bg), Color::Reset);
    }

    #[test]
    fn a_popup_dims_everything_behind_it_but_not_itself() {
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        app.art.set_overlay(true);
        let (_, before) = screen(&app, 255, 70);
        app.help = true;
        let (lines, after) = screen(&app, 255, 70);
        let top = lines.iter().position(|l| l.contains("boxvibe Help")).unwrap() as u16;
        let left = lines[top as usize].chars().position(|c| c == '┌' || c == '╭').unwrap_or(0) as u16;
        let brightness = |c: Color| match c {
            Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            _ => 0,
        };
        // Outside the popup: nothing brighter than before, and lots darker
        // (the half-block cover included).
        let mut darker = 0;
        for y in 0..70 {
            for x in 0..255 {
                let (a, b) = (&before[(x, y)], &after[(x, y)]);
                if y >= top && x >= left && x < left + 10 {
                    continue;
                }
                if a.symbol() == b.symbol() && brightness(b.fg) + brightness(b.bg) < brightness(a.fg) + brightness(a.bg)
                {
                    darker += 1;
                }
            }
        }
        assert!(darker > 500, "only {darker} cells dimmed");
        let blocks = (0..70).flat_map(|y| (0..255).map(move |x| (x, y))).filter(|&p| after[p].symbol() == "▀");
        assert!(blocks.clone().count() > 0);
        assert!(
            blocks.into_iter().all(|p| !matches!(after[p].fg, Color::Rgb(r, g, b) if r > 140 || g > 140 || b > 140))
        );
        // The popup's own text keeps its colour.
        let title = lines[top as usize].find("boxvibe Help").unwrap();
        let x = lines[top as usize][..title].chars().count() as u16;
        assert!(brightness(after[(x, top)].fg) > 400, "the popup stays bright: {:?}", after[(x, top)]);
    }

    #[test]
    fn help_lists_every_bound_key_and_nothing_unbound() {
        use crate::app::{KEYMAP, command_for};
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let tokens: Vec<&str> =
            HELP.iter().flat_map(|(_, keys)| keys.iter()).flat_map(|(k, _)| k.split_whitespace()).collect();
        for b in KEYMAP {
            let want = match b.code {
                KeyCode::Char(' ') => "space".to_owned(),
                KeyCode::Char(c) => c.to_string(),
                KeyCode::Up => "↑".into(),
                KeyCode::Down => "↓".into(),
                KeyCode::Left => "←".into(),
                KeyCode::Right => "→".into(),
                KeyCode::BackTab => "Tab".into(),
                other => format!("{other:?}"),
            };
            assert!(tokens.contains(&want.as_str()), "Help does not list {:?} ({want})", b.code);
        }
        for t in &tokens {
            let mut chars = t.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { continue };
            if matches!(c, '↑' | '↓' | '←' | '→') {
                continue;
            }
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            assert!(command_for(key).is_some(), "Help lists {t:?} but nothing is bound to it");
        }
    }

    #[test]
    fn help_fits_its_longest_line_and_its_keys_line_up() {
        let mut app = demo();
        app.help = true;
        for (cols, rows) in [(255u16, 70u16), (160, 40)] {
            let (lines, _) = screen(&app, cols, rows);
            let all = lines.join("\n");
            for whole in [
                "play the station · open Settings / About",
                "Now Playing: the pane gets the keys",
                HELP_FOOTER.trim(),
                "⌨  boxvibe Help · Any Key Closes",
            ] {
                assert!(all.contains(whole), "{cols}x{rows}: {whole:?} is cut off\n{all}");
            }
            // Every description starts the same distance in from Help's left border.
            let start = |what: &str| {
                let l = lines.iter().find(|l| l.contains(what)).unwrap();
                let at = l.find(what).unwrap();
                l[l[..at].rfind('│').unwrap()..at].chars().count()
            };
            let a = start("volume down / up");
            assert_eq!(a, start("like / bad the selected row"), "{cols}x{rows}\n{all}");
            assert_eq!(a, start("skip to the next song"), "{cols}x{rows}\n{all}");
        }
    }

    #[test]
    fn the_pinned_page_lists_the_pins_and_the_menu_has_its_entry() {
        let mut app = demo();
        let (lines, _) = screen(&app, 140, 40);
        assert!(lines.iter().any(|l| l.contains("Pinned")), "the menu entry, shown by default");
        // I still pins, but the bar leaves it to the `.` menu.
        assert!(!key_caps(&app).iter().any(|(k, _)| *k == "I"));
        assert!(key_caps(&app).iter().any(|(k, v)| *k == "." && *v == "Song Menu"));
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('I'),
            crossterm::event::KeyModifiers::SHIFT,
        ));
        let title = app.pins[0].title.clone();
        app.view = View::Pinned;
        app.focus = Focus::Content;
        let (lines, _) = screen(&app, 140, 40);
        let all = lines.join("\n");
        assert!(all.contains("Pinned (1)") && all.contains(&title) && all.contains(&app.pins[0].pinned), "{all}");
        assert!(key_caps(&app).iter().any(|(k, v)| *k == "x" && *v == "Unpin"));
        app.config.ui.pinned_in_menu = false;
        app.view = View::Player;
        app.status.clear();
        let (lines, _) = screen(&app, 140, 40);
        assert!(!lines.iter().any(|l| l.contains("Pinned")), "hidden");
    }

    #[test]
    fn the_key_bar_leaves_out_theme_and_icons_and_t_does_nothing() {
        let mut app = demo();
        let caps = key_caps(&app);
        assert!(!caps.iter().any(|(k, _)| matches!(*k, "t" | "i" | "T")), "{caps:?}");
        let theme = app.config.ui.theme().to_owned();
        app.on_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('t')));
        assert_eq!(app.config.ui.theme(), theme, "t no longer switches the theme");
    }

    /// Every place the keys can be: (focus, view, pane), plus the menu on a favorite.
    fn contexts(app: &mut App) -> Vec<(String, crate::app::Focus, View, Pane, bool)> {
        use crate::app::Focus;
        let mut all = Vec::new();
        for view in [View::Player, View::Stations, View::Genres, View::Settings, View::About] {
            for (focus, favorite) in [(Focus::Menu, false), (Focus::Menu, true), (Focus::Content, false)] {
                let panes = if view == View::Player && focus == Focus::Content {
                    vec![Pane::NowPlaying, Pane::UpNext, Pane::History]
                } else {
                    vec![Pane::UpNext]
                };
                for pane in panes {
                    all.push((format!("{view:?}/{focus:?}/{pane:?}/favorite={favorite}"), focus, view, pane, favorite));
                }
            }
        }
        let _ = app;
        all
    }

    /// Puts the app in one of [`contexts`] with a favorite in the menu.
    fn enter(app: &mut App, focus: crate::app::Focus, view: View, pane: Pane, favorite: bool) {
        app.rename = None;
        app.confirm_delete = None;
        app.details = None;
        app.add_station = None;
        app.editor = None;
        app.on_add_row = false;
        app.view = view;
        app.focus = focus;
        app.pane = pane;
        app.menu_cursor = 0;
        if favorite {
            let at = app.menu_items().iter().position(|m| matches!(m, MenuItem::Favorite(_))).expect("a favorite");
            app.menu_cursor = at;
        }
    }

    #[test]
    fn a_pane_key_is_hinted_exactly_where_it_does_something() {
        use crate::app::Focus;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(400));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        // The favorite is a station that allows renaming and deleting.
        let station = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).cloned().expect("a station");
        app.config.ui.favorites = vec![crate::config::Favorite { id: station.id.clone(), name: station.name }];
        assert!(!app.queue.is_empty() && !app.history.is_empty() && app.now.is_some(), "the demo has songs");
        let hinted = |app: &App, key: &str| {
            key_caps(app).iter().any(|(k, _)| k.split('/').any(|part| part == key) && *k != "j/k ↑↓")
        };
        for (name, focus, view, pane, favorite) in contexts(&mut app) {
            if favorite && !matches!(view, View::Player | View::Stations | View::Genres | View::Settings | View::About)
            {
                continue;
            }
            // r, x and v: what the key does is what the bar says.
            for key in ['r', 'x', 'v', 'a', 'e'] {
                enter(&mut app, focus, view, pane, favorite);
                let shown = hinted(&app, &key.to_string());
                app.on_key(KeyEvent::from(KeyCode::Char(key)));
                let did = match key {
                    'r' => app.rename.is_some(),
                    'x' => app.confirm_delete.is_some(),
                    'a' => app.add_station.is_some(),
                    'e' => app.editor.is_some(),
                    _ => app.details.is_some(),
                };
                // v on a song is in the `.` menu: it works, unhinted.
                let song_v = key == 'v' && view == View::Player && focus == Focus::Content;
                if song_v {
                    assert!(!shown, "v in {name} is left to the . menu");
                } else {
                    assert_eq!(shown, did, "{key} in {name}: hinted {shown}, acts {did}");
                }
            }
            // l/b, U/D, I, B: in the `.` menu, so never on the bar; `.` is, with a song.
            enter(&mut app, focus, view, pane, favorite);
            for k in ["l", "b", "U", "D", "I", "B", "Z"] {
                assert!(!hinted(&app, k), "{k} in {name}");
            }
            assert!(hinted(&app, "."), ". in {name}");
            // f: stars a station row, or unstars the favorite the menu is on.
            let stars = (view == View::Stations && focus == Focus::Content) || (focus == Focus::Menu && favorite);
            assert_eq!(hinted(&app, "f"), stars, "f in {name}");
            // Enter changes a setting only on the Settings page.
            let change = key_caps(&app).iter().any(|(_, what)| *what == "Change");
            assert_eq!(change, view == View::Settings && focus == Focus::Content, "Change in {name}");
        }
    }

    #[test]
    fn station_keys_leave_out_what_pandora_does_not_allow_and_what_is_not_loaded() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(400));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        let keys = |app: &App| key_caps(app).iter().map(|(k, _)| *k).collect::<Vec<_>>();
        while app.stations_index() > 0 {
            app.on_key(KeyEvent::from(KeyCode::Char('k')));
        }
        for (i, s) in app.stations.clone().iter().enumerate() {
            while app.stations_index() < i {
                app.on_key(KeyEvent::from(KeyCode::Char('j')));
            }
            let caps = keys(&app);
            assert_eq!(caps.contains(&"r"), s.allow_rename, "r on {}", s.name);
            assert_eq!(caps.contains(&"x"), s.allow_delete, "x on {}", s.name);
            let thumbs = s.is_locked() && !s.is_shuffle;
            assert_eq!(caps.contains(&"e"), s.allow_edit_seeds || thumbs, "e on {}", s.name);
        }
        // No stations yet: nothing to play, favorite, rename, delete or show.
        app.stations.clear();
        let caps = keys(&app);
        assert!(!caps.iter().any(|k| matches!(*k, "r" | "x" | "v" | "f" | "e")), "{caps:?}");
    }

    #[test]
    fn pause_skip_and_the_song_menu_are_hinted_only_with_a_song_to_act_on() {
        let mut app = App::new(Config::default(), None, &[], true);
        let has = |app: &App, key: &str| key_caps(app).iter().any(|(k, _)| *k == key);
        // Nothing playing yet.
        assert!(app.now.is_none());
        for key in ["space", "N", "."] {
            assert!(!has(&app, key), "{key} with nothing playing");
        }
        app.settle(std::time::Duration::from_millis(400));
        app.demo_warm_up(3, std::time::Duration::ZERO);
        app.settle(std::time::Duration::from_millis(400));
        assert!(app.now.is_some());
        for key in ["space", "N", "."] {
            assert!(has(&app, key), "{key} while playing");
        }
        // Thumbs, pin, sleep and mode are in the `.` menu, not the bar.
        for key in ["U/D", "I", "B", "l/b", "m"] {
            assert!(!has(&app, key), "{key} is left to the . menu");
        }
    }

    #[test]
    fn about_and_now_playing_have_no_rows_to_move_over() {
        use crate::app::Focus;
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(400));
        let moves = |app: &App| key_caps(app).iter().any(|(k, _)| *k == "j/k ↑↓");
        enter(&mut app, Focus::Content, View::About, Pane::UpNext, false);
        assert!(!moves(&app), "About");
        enter(&mut app, Focus::Content, View::Player, Pane::NowPlaying, false);
        assert!(!moves(&app), "Now Playing");
        enter(&mut app, Focus::Content, View::Player, Pane::UpNext, false);
        assert!(moves(&app), "Up Next");
        enter(&mut app, Focus::Content, View::Settings, Pane::UpNext, false);
        assert!(moves(&app), "Settings");
    }

    #[test]
    fn the_cover_draws_in_half_blocks_inside_the_art_box() {
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(300));
        let song = app.now.clone().expect("a song");
        assert_eq!(app.art.status(&song.cover_art_url), crate::art::Cover::None, "demo cover arrived");
        assert!(app.art.protocol(&song.cover_art_url, crate::art::Look::of(&app.config.ui, app.palette)).is_some());
        let (lines, _) = screen(&app, 120, 36);
        let (top, bottom, left, right) = art_box(&app);
        assert!(bottom > top + 2);
        let inside: String = lines[top + 1..bottom]
            .iter()
            .map(|l| l.chars().skip(left + 1).take(right - left - 1).collect::<String>())
            .collect();
        assert!(inside.contains('▀') || inside.contains('▄'), "halfblocks picture:\n{}", lines.join("\n"));
        assert!(!inside.contains("no cover"));
        // No frame around it, and nothing spilled past the box.
        assert!(!lines[top + 1..bottom].iter().any(|l| l.contains('╭') && l.find('╭') < l.find('▀')));
        for line in &lines[top + 1..bottom] {
            let chars: Vec<char> = line.chars().collect();
            assert!(!['▀', '▄'].contains(&chars[left]) && !['▀', '▄'].contains(&chars[right]), "{line}");
        }
    }

    /// What the art box holds, row by row (no border: it is the area itself).
    fn art_rows(app: &App, lines: &[String]) -> Vec<String> {
        let a = app.art.last_area.get();
        lines[a.y as usize..(a.y + a.height) as usize]
            .iter()
            .map(|l| l.chars().skip(a.x as usize).take(a.width as usize).collect())
            .collect()
    }

    /// The art box shows just `message`, centred, and no picture or bars.
    fn assert_only_message(app: &App, message: &str) {
        let (lines, _) = screen(app, 120, 36);
        let rows = art_rows(app, &lines);
        let shown: Vec<(usize, &String)> = rows.iter().enumerate().filter(|(_, r)| !r.trim().is_empty()).collect();
        let [(row, text)] = shown[..] else { panic!("one line of text in the art box:\n{}", rows.join("\n")) };
        assert_eq!(text.trim(), message);
        assert_eq!(row, rows.len() / 2, "in the middle, top to bottom");
        let width = app.art.last_area.get().width as usize;
        let left = text.chars().take_while(|c| *c == ' ').count();
        let right = width - left - message.width();
        assert!(left.abs_diff(right) <= 1, "centred: {left} left, {right} right");
        for glyph in ['▀', '▄', '█', '▁', '▂', '▃', '▅', '▆', '▇', '▔'] {
            assert!(!rows.iter().any(|r| r.contains(glyph)), "no picture or bars ({glyph}):\n{}", rows.join("\n"));
        }
    }

    #[test]
    fn with_album_art_off_the_space_says_cover_art_off() {
        let mut app = demo();
        app.config.ui.album_art = crate::config::AlbumArt::Off;
        assert_only_message(&app, "Cover art off");
        // Playing music changes nothing: there is no visualizer to fall back on.
        assert_only_message(&app, "Cover art off");
        // Off by choice wins over a song that has no cover anyway.
        if let Some(song) = app.now.as_mut() {
            song.cover_art_url.clear();
        }
        assert_only_message(&app, "Cover art off");
        // Turning it back on says "No cover art" for that song.
        app.config.ui.album_art = crate::config::AlbumArt::Picture;
        assert_only_message(&app, "No cover art");
    }

    #[test]
    fn shift_c_flips_between_the_picture_and_cover_art_off() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.config.ui.album_art = crate::config::AlbumArt::Picture;
        app.on_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));
        assert_eq!(app.config.ui.album_art, crate::config::AlbumArt::Off);
        assert_only_message(&app, "Cover art off");
        app.on_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));
        assert_eq!(app.config.ui.album_art, crate::config::AlbumArt::Picture);
        assert!(!screen(&app, 120, 36).0.join("\n").contains("Cover art off"));
    }

    #[test]
    fn without_a_cover_the_space_says_so_and_not_a_visualizer() {
        let mut app = demo();
        if let Some(song) = app.now.as_mut() {
            song.cover_art_url.clear();
        }
        assert_only_message(&app, "No cover art");
    }

    #[test]
    fn a_cover_on_its_way_says_loading_and_a_lost_one_says_no_cover() {
        let mut app = demo();
        app.art = crate::art::Art::default();
        let url = app.now.as_ref().map(|s| s.cover_art_url.clone()).unwrap();
        assert!(app.art.want(&url), "the download starts");
        assert_only_message(&app, "Loading cover art…");
        // Art off while one downloads: nothing is coming, and the space says art is off.
        app.config.ui.album_art = crate::config::AlbumArt::Off;
        assert_only_message(&app, "Cover art off");
        // A cover that has been given up on (every try failed).
        app.config.ui.album_art = crate::config::AlbumArt::Picture;
        let mut gone = crate::art::Art::default();
        for _ in 0..=crate::art::RETRY_WAITS.len() {
            gone.arrived(url.clone(), None);
        }
        app.art = gone;
        assert_only_message(&app, "No cover art");
    }

    #[test]
    fn confirm_has_a_blank_line_above_and_below() {
        let mut app = demo();
        app.confirm_quit = true;
        let (lines, _) = screen(&app, 120, 36);
        let top = lines.iter().position(|l| l.contains(" Confirm ")).expect("confirm box");
        let chars = |y: usize| lines[y].chars().collect::<Vec<char>>();
        let left = lines[top].chars().position(|c| c == '┌').unwrap();
        let bottom = (top + 1..lines.len()).find(|&y| chars(y)[left] == '└').expect("bottom");
        let right = (left + 1..chars(top).len()).find(|&x| chars(top)[x] == '┐').unwrap();
        let inner: Vec<String> =
            (top + 1..bottom).map(|y| chars(y)[left + 1..right].iter().collect::<String>().trim().to_owned()).collect();
        assert_eq!(inner, ["", "Quit boxvibe?", "y = yes · any other key = no", ""], "{inner:?}");
    }

    #[test]
    fn a_long_question_wraps_and_cuts_to_fit() {
        assert_eq!(wrap_question("Quit boxvibe?", 36, 3), ["Quit boxvibe?"]);
        let w = wrap_question("one two three four five six seven", 9, 2);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[1].ends_with('…') && w.iter().all(|l| l.width() <= 9), "{w:?}");
    }

    #[test]
    fn shift_q_shows_tubevibes_confirm_box() {
        let mut app = demo();
        app.confirm_quit = true;
        let (lines, _) = screen(&app, 120, 36);
        let all = lines.join("\n");
        assert!(all.contains(" Confirm "), "{all}");
        assert!(all.contains("Quit boxvibe?"), "{all}");
        assert!(all.contains("y = yes · any other key = no"), "{all}");
        assert_eq!(key_caps(&app), vec![("y", "Yes, Quit"), ("Other Key", "No")]);
    }

    #[test]
    fn the_logo_and_version_sit_on_the_menus_top_border() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.status = "hello status".into();
        let (lines, buffer) = screen(&app, 120, 36);
        let top = &lines[0];
        assert!(top.starts_with(&format!("┌ {} boxvibe ─", crate::icons::LOGO)), "{top}");
        let menu_end = top.find('┐').unwrap();
        let version = format!(" {} ┐", env!("CARGO_PKG_VERSION"));
        assert!(top[..menu_end + '┐'.len_utf8()].ends_with(&version), "{top}");
        assert!(top.contains("Now Playing"), "no header row: Now Playing's border is the first row: {top}");
        assert!(!lines.iter().any(|l| l.contains("demo@") || l.contains("not logged in ")));
        assert!(lines[34].starts_with(" hello status"), "status bar above the keys: {}", lines[34]);
        // "box" and its icon blue, "vibe" the text colour.
        let p = app.palette;
        let x = top.chars().position(|c| c == 'b').unwrap() as u16;
        assert_eq!(buffer[(2, 0)].fg, p.blue);
        assert_eq!((buffer[(x, 0)].fg, buffer[(x + 3, 0)].fg), (p.blue, p.fg));
    }

    #[test]
    fn help_title_uses_tubevibes_keyboard_and_the_logo_name() {
        for set in [crate::config::IconSet::Unicode, crate::config::IconSet::Emoji, crate::config::IconSet::Nerd] {
            let mut app = demo();
            app.config.ui.icons = set;
            app.help = true;
            let (lines, buffer) = screen(&app, 120, 40);
            let (y, row) = lines.iter().enumerate().find(|(_, l)| l.contains("Help · Any Key Closes")).unwrap();
            assert!(row.contains("⌨  boxvibe Help · Any Key Closes"), "{row}");
            let (p, y) = (app.palette, y as u16);
            let x = row.chars().position(|c| c == '⌨').unwrap() as u16;
            assert_eq!(buffer[(x, y)].fg, p.fg);
            assert_eq!((buffer[(x + 3, y)].fg, buffer[(x + 6, y)].fg), (p.blue, p.fg));
            assert!(buffer[(x + 6, y)].modifier.contains(ratatui::style::Modifier::BOLD));
        }
    }

    #[test]
    fn every_border_style_draws_and_the_menu_width_holds() {
        let mut app = demo();
        for (style, corner) in [
            (BorderStyle::Plain, '┌'),
            (BorderStyle::Rounded, '╭'),
            (BorderStyle::Thick, '┏'),
            (BorderStyle::Double, '╔'),
            (BorderStyle::None, ' '),
        ] {
            app.config.ui.border_style = style;
            let (lines, _) = screen(&app, 120, 36);
            assert_eq!(lines[0].chars().next(), Some(corner), "{style:?}: {}", lines[0]);
            assert!(lines[0].contains("boxvibe") && lines[0].contains("Now Playing"), "titles stay: {}", lines[0]);
        }
        app.config.ui.border_style = BorderStyle::Plain;
        for (width, cols, menu) in [(20, 120, 20), (40, 120, 40), (40, 60, 30)] {
            app.config.ui.menu_width = width;
            let (lines, _) = screen(&app, cols, 30);
            let end = lines[0].chars().position(|c| c == '┐').unwrap();
            assert_eq!(end + 1, menu, "menu_width {width} at {cols} columns");
        }
    }

    #[test]
    fn now_playing_heads_the_menu_with_an_inset_rule() {
        let mut app = demo();
        app.view = View::Settings;
        app.focus = crate::app::Focus::Content;
        let (lines, _) = screen(&app, 120, 36);
        assert!(lines[1].starts_with("│ ▶  NOW PLAYING"), "{}", lines[1]);
        let menu: String = lines[2].chars().take(30).collect();
        assert!(menu.starts_with("│  ──") && menu.ends_with("──  │"), "inset rule: {menu:?}");
        assert!(lines[3].contains("Stations ("), "{}", lines[3]);
        assert!(lines[35].contains(" P  Now Playing"), "key bar: {}", lines[35]);
    }

    #[test]
    fn song_panes_fit_ten_rows_and_now_playing_takes_the_rest() {
        let mut app = demo();
        // Side by side: Up Next and History are SONG_PANE (13) tall, Now Playing the rest.
        for (rows, want) in [(36, 21), (50, 35), (20, 11)] {
            assert_eq!(now_playing_height(rows - 2, false), want, "{rows} rows");
            let (lines, _) = screen(&app, 120, rows);
            let up = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
            assert_eq!(up, usize::from(want), "Up Next right under Now Playing");
        }
        // Stacked: two song panes of 13 each.
        assert_eq!(now_playing_height(48, true), 22);
        assert_eq!(now_playing_height(30, true), NOW_PLAYING_MIN, "short: Now Playing keeps its minimum");
        assert_eq!(now_playing_height(5, true), 5, "never more than the screen");
        // A full History (10 songs) fits exactly: last row right above the bottom border.
        for i in 0..12 {
            let mut s = app.now.clone().unwrap();
            s.title = format!("Old song {i}");
            app.history.insert(0, s);
        }
        app.history.truncate(10);
        let (lines, _) = screen(&app, 120, 36);
        let hist = lines.iter().position(|l| l.contains("History (")).unwrap();
        assert!(lines[hist + 11].contains("Old song"), "10th row: {}", lines[hist + 11]);
        assert!(lines[hist + 12].contains('└') || lines[hist + 12].contains('╰'), "{}", lines[hist + 12]);
    }

    #[test]
    fn stations_page_and_favorites_section() {
        let mut app = demo();
        let (lines, _) = screen(&app, 120, 36);
        assert!(!lines.iter().any(|l| l.contains("FAVORITES")), "hidden while empty");
        let fav = app.stations[1].clone();
        app.config.ui.favorites.push(crate::config::Favorite { id: fav.id.clone(), name: fav.name.clone() });
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        let (lines, _) = screen(&app, 120, 36);
        assert!(lines.iter().any(|l| l.starts_with("│ FAVORITES (1)")), "the menu's section");
        let side = |l: &String, menu: bool| -> String {
            if menu { l.chars().take(30).collect() } else { l.chars().skip(30).collect() }
        };
        assert!(lines.iter().any(|l| side(l, true).contains(&fav.name)), "in the menu");
        assert!(
            lines.iter().any(|l| side(l, false).contains("⭐") && l.contains(&fav.name)),
            "starred on the page: {lines:#?}"
        );
        assert!(lines[0].contains("Stations ("), "the page: {}", lines[0]);
        assert!(lines[35].contains(" f  Favorite"), "key bar: {}", lines[35]);
    }

    #[test]
    fn a_station_icon_sits_between_the_star_and_the_name() {
        use crate::config::IconSet;
        let mut app = demo();
        app.view = View::Stations;
        app.focus = crate::app::Focus::Content;
        let genre = app.stations.iter().find(|s| s.name == "Today's Hits").cloned().unwrap();
        assert!(genre.is_locked() && genre.allow_delete, "a genre station");
        let made = app.stations.iter().find(|s| s.allow_rename && s.allow_delete).cloned().unwrap();
        app.config.ui.favorites.push(crate::config::Favorite { id: made.id.clone(), name: made.name.clone() });
        assert!(!made.is_locked());
        let shuffle = app.stations.iter().find(|s| s.is_shuffle).cloned().unwrap();
        // The logo for a genre station, a person for your own, in each icon set.
        let sets = [
            (IconSet::Emoji, "\u{f1b2}", "👤"),
            (IconSet::Nerd, "\u{f1b2}", "\u{f007}"),
            (IconSet::Unicode, "\u{f1b2}", "☺"),
        ];
        for (set, logo, person) in sets {
            let ic = icons(set);
            assert_eq!((ic.genre_station, ic.own_station), (logo, person));
            assert_eq!(ic.genre_station, crate::icons::LOGO, "the same icon as the logo");
            app.config.ui.icons = set;
            let (lines, _) = screen(&app, 120, 36);
            let row = |name: &str| {
                let page = lines.iter().map(|l| l.chars().skip(30).collect::<String>());
                page.into_iter().find(|l| l.contains(name)).unwrap()
            };
            let (g, m) = (row(&genre.name), row(&made.name));
            let right_before = |row: &str, icon: &str, name: &str| {
                let (at, name) = (row.find(icon).unwrap(), row.find(name).unwrap());
                at < name && row[at + icon.len()..name].trim().is_empty()
            };
            assert!(right_before(&g, logo, &genre.name), "{set:?}: {g}");
            assert!(!g.contains(person), "{set:?}: a genre station has no person: {g}");
            assert!(right_before(&m, person, &made.name), "{set:?}: {m}");
            assert!(!m.contains(logo), "{set:?}: your own station has no logo: {m}");
            // After the star, before the name.
            let (star, at, name) = (m.find(ic.favorite).unwrap(), m.find(person).unwrap(), m.find(&made.name).unwrap());
            assert!(star < at && at < name, "{set:?}: star, icon, name: {m}");
            // Shuffle is neither.
            let s = row(&shuffle.name);
            assert!(!s.contains(logo) && !s.contains(person), "{set:?}: {s}");
        }
        // Favorites in the menu too, once the station is loaded.
        app.config.ui.icons = IconSet::Unicode;
        app.config.ui.favorites.push(crate::config::Favorite { id: genre.id.clone(), name: genre.name.clone() });
        let (lines, _) = screen(&app, 120, 36);
        let menu: Vec<String> = lines.iter().map(|l| l.chars().take(30).collect()).collect();
        assert!(menu.iter().any(|l| l.contains("\u{f1b2}") && l.contains("Today's Hits")), "{menu:#?}");
        // A narrow list cuts the name, never the icon.
        let (lines, _) = screen(&app, 52, 36);
        let row = lines.iter().rfind(|l| l.contains("Today")).unwrap();
        assert!(row.find("\u{f1b2}").unwrap() < row.find("Today").unwrap(), "{row}");
    }

    #[test]
    fn the_genres_page_shows_neither_station_icon() {
        use crate::config::IconSet;
        let mut app = demo();
        app.view = View::Genres;
        app.focus = crate::app::Focus::Content;
        app.settle(std::time::Duration::from_millis(300));
        for set in [IconSet::Emoji, IconSet::Nerd, IconSet::Unicode] {
            app.config.ui.icons = set;
            let ic = icons(set);
            let (lines, _) = screen(&app, 120, 36);
            let page: String =
                lines.iter().map(|l| l.chars().skip(30).collect::<String>()).collect::<Vec<_>>().join("\n");
            assert!(page.contains("Genres"), "{page}");
            assert!(!page.contains(ic.genre_station) && !page.contains(ic.own_station), "{set:?}: {page}");
        }
    }

    #[test]
    fn the_menu_and_the_page_touch() {
        let app = demo();
        let (lines, _) = screen(&app, 120, 36);
        let top: Vec<char> = lines[0].chars().collect();
        let end = top.iter().position(|c| *c == '┐').unwrap();
        assert_eq!(top[end + 1], '┌', "no blank column between the frames, like tubevibe: {}", lines[0]);
    }

    #[test]
    fn up_next_and_history_sit_side_by_side() {
        let app = demo();
        let (lines, _) = screen(&app, 120, 36);
        let row = lines.iter().find(|l| l.contains("Up Next (")).expect("Up Next pane");
        let (next, history) = (row.find("Up Next").unwrap(), row.find("History (").expect("on the same row"));
        assert!(next < history, "{row}");
    }

    #[test]
    fn menu_text_is_left_justified_like_tubevibe() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        let (lines, _) = screen(&app, 120, 36);
        for word in ["NOW PLAYING", "Stations", "Settings", "About", "Quit"] {
            let line = lines.iter().find(|l| l.chars().take(30).collect::<String>().contains(word)).expect(word);
            let menu: Vec<char> = line.chars().take(30).collect();
            assert_eq!(menu[1], ' ', "{word}: one space after the border: {line}");
            assert_ne!(menu[2], ' ', "{word}: the icon at column 2: {line}");
        }
    }

    #[test]
    fn up_next_and_history_stack_when_narrow() {
        let app = demo();
        assert!(stack_songs(STACK_BELOW - 1) && !stack_songs(STACK_BELOW));
        let (lines, _) = screen(&app, 100, 40);
        let next = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
        let history = lines.iter().position(|l| l.contains("History (")).expect("History");
        assert!(history > next, "History under Up Next on a narrow window");
        assert!(!lines[next].contains("History ("), "{}", lines[next]);
    }

    #[test]
    fn long_song_text_is_cut_and_the_rating_stays_visible() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        let long = "A Very Long Song Title 🎵🎶 That Goes On ❤️ And On\tWith\nControl Characters Forever".to_owned();
        for song in app.history.iter_mut().chain(app.queue.iter_mut()) {
            song.title = long.clone();
            song.artist = "An Artist Whose Name 名前 Is Also Far Too Long For Any Pane".into();
        }
        app.history[0].rating = Rating::Up;
        for cols in [80u16, 100, 120, 160] {
            let (lines, _) = screen(&app, cols, 40);
            let rated = lines.iter().any(|l| l.contains("liked"));
            assert!(rated, "{cols} cols: the rating shows: {lines:#?}");
            assert!(lines.iter().any(|l| l.contains('…')), "{cols} cols: cut with an ellipsis");
            let row = lines.iter().find(|l| l.contains("liked")).unwrap();
            let end = row.trim_end().chars().last();
            assert!(matches!(end, Some('│')), "{cols} cols: the pane's border closes the row: {row}");
            assert!(!row.contains('\u{FE0F}') && !row.contains('\t'), "{cols} cols: cleaned: {row:?}");
        }
    }

    #[test]
    fn the_account_sits_at_the_right_of_the_status_bar() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.config.pandora.username = "toby@example.com".into();
        app.status = "hello status".into();
        app.session = Session::LoggedIn;
        let (lines, buffer) = screen(&app, 120, 36);
        let bar = &lines[34];
        assert!(bar.starts_with(" hello status"), "{bar}");
        assert!(bar.trim_end().ends_with("Logged In As: toby@example.com"), "{bar}");
        assert!(bar.ends_with("Logged In As: toby@example.com "), "one space before the edge: {bar:?}");
        let x = (bar.chars().count() - "Logged In As: toby@example.com ".len()) as u16;
        assert_eq!(buffer[(x, 34)].fg, app.palette.blue, "tubevibe's account colour");
        // Narrow, with a long status: both cut, never overlapping.
        app.status = "a very long status message that goes on and on and on".into();
        let (lines, _) = screen(&app, 40, 36);
        let bar = &lines[34];
        assert!(bar.contains('…') && bar.contains('@'), "{bar}");
        assert!(bar.trim_end().ends_with("toby@example.com"), "narrow: just the email, whole: {bar}");
        assert!(!bar.contains("Logged In As"), "{bar}");
        assert!(bar.chars().count() == 40);
        let status_end = bar.find("on").unwrap();
        assert!(status_end < bar.find('@').unwrap());
        // Logged out: nothing on the right.
        app.session = Session::LoggedOut;
        let (lines, _) = screen(&app, 120, 36);
        assert!(!lines[34].contains("toby@"), "{}", lines[34]);
    }

    #[test]
    fn the_status_bar_shows_the_look_and_the_sleep_timer_left_of_the_account_and_drops_the_look_first() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.config.pandora.username = "toby@example.com".into();
        app.config.ui.set_theme("gruvbox");
        app.config.ui.album_art = AlbumArt::Picture;
        app.config.ui.retro_mode = crate::config::RetroMode::Bit8;
        app.config.ui.art_tint = crate::config::ArtTint::Duotone;
        app.session = Session::LoggedIn;
        app.status = "hi".into();
        let bar = screen(&app, 200, 36).0[34].clone();
        assert!(
            bar.trim_end().ends_with("Gruvbox · Art Picture · 8-bit · Tint Duotone · Logged In As: toby@example.com"),
            "{bar}"
        );
        // The look is in the account's colour; the separators stay muted.
        let (lines, buffer) = screen(&app, 200, 36);
        let col = |s: &str| lines[34].chars().take(lines[34].find(s).unwrap()).count() as u16;
        for label in ["Gruvbox", "Art Picture", "8-bit", "Tint Duotone", "Logged In As"] {
            assert_eq!(buffer[(col(label), 34)].fg, app.palette.blue, "{label}");
        }
        assert_eq!(buffer[(col("Gruvbox") + 8, 34)].fg, app.palette.muted);
        app.sleep = Some(crate::app::Sleep::Minutes(15, std::time::Instant::now()));
        let bar = screen(&app, 200, 36).0[34].clone();
        assert!(bar.contains("☾ 15:00 · Gruvbox") || bar.contains("☾ 14:59 · Gruvbox"), "{bar}");
        // Narrower: the tint goes first, then the rest of the look; the timer stays.
        let bar = screen(&app, 150, 36).0[34].clone();
        assert!(bar.contains("☾ 1") && bar.contains("Logged In As") && !bar.contains("Tint"), "{bar}");
        let bar = screen(&app, 96, 36).0[34].clone();
        assert!(bar.contains("☾ 1") && !bar.contains("Gruvbox") && bar.contains("toby@example.com"), "{bar}");
        app.sleep = Some(crate::app::Sleep::EndOfSong);
        assert!(screen(&app, 200, 36).0[34].contains("☾ end of song"));
    }

    #[test]
    fn following_omarchy_the_status_bar_names_its_theme_in_the_account_colour() {
        let mut app = demo();
        app.config.pandora.username = "toby@example.com".into();
        app.session = Session::LoggedIn;
        app.config.ui.set_theme("omarchy");
        app.omarchy_theme = Some("Tokyo Night".into());
        let (lines, buffer) = screen(&app, 200, 36);
        let bar = &lines[34];
        assert!(bar.contains("omarchy: Tokyo Night · Art"), "{bar}");
        let x = bar.chars().take(bar.find("omarchy: Tokyo").unwrap()).count() as u16;
        assert_eq!((buffer[(x, 34)].fg, buffer[(x + 12, 34)].fg), (app.palette.blue, app.palette.blue));
        // Its name not known yet: just "Omarchy"; a picked theme: its own name.
        app.omarchy_theme = None;
        assert!(screen(&app, 200, 36).0[34].contains("Omarchy · Art"));
        app.config.ui.set_theme("gruvbox");
        app.omarchy_theme = Some("Tokyo Night".into());
        let bar = screen(&app, 200, 36).0[34].clone();
        assert!(bar.contains("Gruvbox · Art") && !bar.contains("omarchy:"), "{bar}");
    }

    #[test]
    fn up_next_shows_each_songs_thumbs_and_updates_after_l() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.focus = crate::app::Focus::Content;
        app.view = View::Player;
        app.pane = Pane::UpNext;
        let up_next = |app: &App| {
            let (lines, _) = screen(app, 160, 40);
            let y0 = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
            let x0 = lines[y0].chars().position(|c| c == 'U').unwrap();
            let rows: Vec<String> =
                lines[y0 + 1..y0 + 4].iter().map(|l| l.chars().skip(x0).take(70).collect()).collect();
            rows
        };
        let before = up_next(&app);
        assert!(before[0].contains("TIME") && before[0].contains("RATED"), "{before:?}");
        assert!(before[1].contains('—') && before[1].contains(':'), "not rated, time kept: {before:?}");
        app.on_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('l')));
        assert_eq!(app.queue[app.next_index()].rating, Rating::Up);
        let after = up_next(&app);
        assert!(after.iter().any(|r| r.contains("liked")), "{after:?}");
    }

    #[test]
    fn up_next_keeps_a_blank_cell_before_its_right_border() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.focus = crate::app::Focus::Content;
        app.view = View::Player;
        app.pane = Pane::UpNext;
        // Side by side (wide) and stacked (narrow).
        for cols in [80u16, 100, 120, 140, 160, 200] {
            let (lines, buffer) = screen(&app, cols, 40);
            let y0 = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
            let row: Vec<char> = lines[y0].chars().collect();
            let start = lines[y0].chars().position(|c| c == 'U').unwrap();
            let right = (start..row.len()).find(|&x| row[x] == '┐').expect("right corner") as u16;
            let mut y = y0 as u16 + 1;
            let mut checked = 0;
            while buffer[(right, y)].symbol() != "┘" {
                let cell = &buffer[(right - 1, y)];
                assert_eq!(cell.symbol(), " ", "{cols} cols row {y}: {}", lines[y as usize]);
                assert_eq!(cell.bg, buffer[(right, y)].bg, "{cols} cols row {y}: highlight touches the border");
                y += 1;
                checked += 1;
            }
            assert!(checked >= 3, "{cols} cols: rows checked");
        }
    }

    #[test]
    fn clean_and_truncate_keep_cells_honest() {
        assert_eq!(clean("a\tb\nc ❤️ d 🎵"), "a b c  d 🎵");
        assert_eq!(truncate("abcdef", 4), "ab…");
        assert_eq!(truncate("名前名前", 6), "名前…");
        assert!(truncate("名前名前", 5).width() <= 5);
    }

    #[test]
    fn now_playing_art_fills_the_pane_with_info_time_and_volume_beside_it() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        let (lines, _) = screen(&app, 120, 36);
        let rows = usize::from(now_playing_height(34, false));
        let menu = |l: &String| l.chars().skip(30).collect::<String>();
        let art = app.art.last_area.get();
        assert_eq!(
            (art.y, art.y + art.height),
            (2, rows as u16 - 2),
            "art fills the pane's height inside one row of padding"
        );
        assert!(!lines[..rows].iter().any(|l| l.contains('╰') && menu(l).chars().nth(2) == Some('╰')), "no frame");
        let title = app.now.as_ref().unwrap().title.clone();
        let time = lines.iter().position(|l| l.contains('●')).expect("time line");
        let volume = lines.iter().position(|l| l.contains('%') && l.contains('▮')).expect("volume");
        assert!(time < rows && volume == time + 1, "both inside Now Playing, beside the art");
        let picture_right = lines[..rows]
            .iter()
            .filter_map(|l| l.chars().enumerate().filter(|(_, c)| *c == '▀' || *c == '▄').map(|(i, _)| i).last())
            .max()
            .expect("picture");
        assert!(lines[time].chars().position(|c| c == '●').unwrap() > picture_right, "beside the picture");
        // The stack is centred: as many blank rows above it as below.
        let first = lines.iter().position(|l| menu(l).contains(&title)).unwrap();
        let last = lines[..rows].iter().rposition(|l| l.contains('╰')).expect("buttons");
        let (above, below) = (first - 1, rows - 2 - last);
        assert!(above.abs_diff(below) <= 1, "centred: {above} above, {below} below");
        for key in ["-/d", "+/u", "space"] {
            assert!(!lines[..rows].iter().any(|l| l.contains(key)), "no {key} under the buttons");
        }
    }

    /// Every row of Up Next and History ends in its own column: the rating
    /// cell starts with the expected icon, and no text crosses a border.
    #[test]
    fn song_text_never_reaches_the_rating_column_at_any_width() {
        let titles = [
            "Ünïcödé Àccénts Évérywhere Ça Va Très Bien Merci Beaucoup",
            "東京の夜 名前 世界 音楽 歌 東京の夜 名前 世界 音楽 歌",
            "❤️ Love ☀ Sun ★ Star ♪ Note 🎵🎶 👍👎 📻 \u{f164} Emoji ‍👩‍👩‍👧 Family\u{200B}Zero\tTab\nLine",
        ];
        for set in [crate::config::IconSet::Emoji, crate::config::IconSet::Nerd, crate::config::IconSet::Unicode] {
            for (k, title) in titles.iter().enumerate() {
                let mut app = demo();
                app.config.ui.icons = set;
                let ic = crate::icons::icons(set);
                for song in app.history.iter_mut().chain(app.queue.iter_mut()) {
                    song.title = (*title).into();
                    song.artist = titles[(k + 1) % 3].into();
                    song.rating = Rating::Up;
                }
                for cols in [60u16, 80, 89, 90, 100, 120, 160, 200] {
                    let (_, buf) = screen(&app, cols, 44);
                    let mut found = 0;
                    for y in 0..44 {
                        let row: Vec<&str> = (0..cols).map(|x| buf[(x, y)].symbol()).collect();
                        for (x, sym) in row.iter().enumerate() {
                            if *sym != "liked"
                                && sym.starts_with('l')
                                && row[x..].iter().take(5).copied().collect::<String>() == "liked"
                            {
                                found += 1;
                                let icon_start = (0..x).rev().find(|&j| !row[j].is_empty() && row[j] != " ").unwrap();
                                assert_eq!(row[icon_start], ic.up, "{cols} cols {set:?}: {}", row.concat());
                                // Two cells of gap before the rating column hold only spaces.
                                let border = row[x + 5..].iter().position(|s| *s == "│").expect("closing border");
                                assert!(row[x + 5..x + 5 + border].iter().all(|s| *s == " " || s.is_empty()));
                            }
                        }
                        // No song text ever paints a border cell.
                        let line = row.concat();
                        for bad in ["东", "東", "Ü", "Love", "Star"] {
                            if let Some(pos) = line.find(bad) {
                                let before = &line[..pos];
                                assert!(
                                    before.matches('│').count() % 2 == 1,
                                    "{cols} cols: text outside a pane: {line}"
                                );
                            }
                        }
                    }
                    assert!(found > 0, "{cols} cols {set:?}: a rating shows");
                }
            }
        }
    }

    #[test]
    fn fit_is_conservative_about_risky_widths() {
        for (text, cells) in [("❤ ★ ☀ \u{f164} 🎵 名前 é", 6usize), ("abcdef", 4), ("名前名前", 5), ("🎵🎵🎵", 4)]
        {
            let out = fit(text, cells);
            assert!(safe_width(&out) < cells || cells == 0, "{text:?} -> {out:?}");
            assert!(out.width() < cells, "{text:?} -> {out:?}");
        }
        assert_eq!(clean("a\u{200B}b\u{FE0F}c\u{200D}d\te\u{f164}f❤g"), "abcd efg");
        assert_eq!(safe_char_width('★'), 2);
        assert_eq!(safe_char_width('\u{f164}'), 2);
        assert_eq!(safe_char_width('a'), 1);
        assert_eq!(icon_cell("\u{f164}").width(), 3);
        assert_eq!(icon_cell("👍").width(), 2);
    }

    fn all_text(lines: &[String]) -> String {
        lines.join("\n")
    }

    #[test]
    fn a_failed_search_in_the_seed_box_says_so_in_plain_words() {
        use crate::backend::Reply;
        use ratatui::crossterm::event::{KeyCode, KeyEvent};
        assert_eq!(
            search_error_text("Service method not found (HTTP 404, NOT_FOUND)"),
            "Couldn't search Pandora right now"
        );
        assert_eq!(search_error_text("offline"), "Search failed: offline");
        let mut app = editor_demo();
        app.on_key(KeyEvent::from(KeyCode::Char('a')));
        // Pandora's raw answer to the old suggestions path, then a search that failed the same way.
        app.on_reply(Reply::Failed { what: "suggestions", message: "Service method not found (HTTP 404)".into() });
        let text = all_text(&screen(&app, 120, 40).0);
        assert!(
            !text.contains("Service method") && !text.contains("no suggestions") && !text.contains("Search failed"),
            "{text}"
        );
        app.on_reply(Reply::Failed { what: "search", message: "Service method not found (HTTP 404)".into() });
        let text = all_text(&screen(&app, 120, 40).0);
        assert!(text.contains("Couldn't search Pandora right now") && !text.contains("Service method"), "{text}");
    }

    #[test]
    fn the_add_station_box_shows_the_text_the_results_and_their_kind() {
        use crate::app::Focus;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(400));
        app.on_key(KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert_eq!(app.focus, Focus::Content);
        let (lines, _) = screen(&app, 120, 36);
        assert!(lines.iter().any(|l| l.contains("Add station…")), "the first row of the list");
        let caps = key_caps(&app);
        assert!(caps.iter().any(|(k, w)| *k == "a" && *w == "Add Station"), "{caps:?}");
        app.on_key(KeyEvent::from(KeyCode::Char('a')));
        for c in "m83".chars() {
            app.on_key(KeyEvent::from(KeyCode::Char(c)));
        }
        let caps = key_caps(&app);
        assert!(caps.iter().any(|(k, _)| *k == "Enter") && caps.iter().any(|(k, _)| *k == "Esc"), "{caps:?}");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        app.settle(std::time::Duration::from_millis(300));
        let (lines, _) = screen(&app, 120, 36);
        let text = all_text(&lines);
        assert!(text.contains("Add station") && text.contains("m83"), "{text}");
        assert!(text.contains("M83") && text.contains("Midnight City"), "artist and song: {text}");
        assert!(text.contains("Artist") && text.contains("Song"), "with their kind: {text}");
        let caps = key_caps(&app);
        assert!(caps.iter().any(|(k, _)| k.contains("j/k")), "results can be moved: {caps:?}");
        assert!(!caps.iter().any(|(k, _)| *k == "a" || *k == "f"), "other keys are not live: {caps:?}");
        // Narrow and short windows do not panic.
        for (c, r) in [(40, 10), (20, 6), (120, 8)] {
            let _ = screen(&app, c, r);
        }
    }

    #[test]
    fn the_genres_page_lists_categories_and_stations_and_the_menu_has_it() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = demo();
        app.settle(std::time::Duration::from_millis(400));
        app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
        app.settle(std::time::Duration::from_millis(300));
        let (lines, _) = screen(&app, 120, 36);
        let text = all_text(&lines);
        for word in ["Genres", "POP", "ELECTRONIC", "ROCK", "Today's Hits", "Synthwave", "Classic Rock"] {
            assert!(text.contains(word), "{word}: {text}");
        }
        assert!(text.contains("Neon, night drives"), "descriptions: {text}");
        let genre_icon = crate::icons::icons(app.config.ui.icons).genre;
        let menu: Vec<String> = lines.iter().map(|l| l.chars().take(30).collect()).collect();
        assert!(menu.iter().any(|l| l.contains("Genres") && l.contains(genre_icon)), "in the menu with its icon");
        let caps = key_caps(&app);
        assert!(caps.iter().any(|(k, w)| *k == "Enter" && *w == "Add & Play"), "{caps:?}");
        assert!(caps.iter().any(|(k, _)| k.contains("j/k")), "{caps:?}");
        // Before it has loaded there is nothing to move through.
        app.genres = crate::app::GenresState::Loading;
        let (lines, _) = screen(&app, 120, 36);
        assert!(all_text(&lines).contains("Loading"), "{}", all_text(&lines));
        assert!(!key_caps(&app).iter().any(|(k, _)| k.contains("j/k")));
        app.genres = crate::app::GenresState::Failed("offline".into());
        assert!(all_text(&screen(&app, 120, 36).0).contains("offline"));
        assert!(key_caps(&app).iter().any(|(_, w)| *w == "Try Again"));
        let _ = screen(&app, 30, 8);
    }

    #[test]
    fn every_icon_set_has_a_genre_and_an_animations_icon() {
        use crate::config::IconSet;
        for set in [IconSet::Nerd, IconSet::Emoji, IconSet::Unicode] {
            let icons = crate::icons::icons(set);
            assert!(!icons.genre.is_empty(), "{set:?}");
            assert!(!icons.setting[crate::app::Setting::at(crate::app::Setting::Animations)].is_empty(), "{set:?}");
        }
    }

    #[test]
    fn the_system_check_lists_what_is_missing_and_that_is_all() {
        use crate::deps::{Facts, checks};
        let mut app = App::new(Config::default(), None, &[], true);
        app.system_checked(&Facts {
            families: Some("DejaVu Sans\n".into()),
            glyph_fonts: None,
            notify_send: false,
            keychain: Some(false),
        });
        assert!(app.system_open);
        let all = screen(&app, 110, 30).0.join("\n");
        for want in [
            "System Check",
            "✘ Nerd Font symbols",
            "missing · needed",
            "package: ttf-nerd-fonts-symbols (Arch) or any Nerd Font",
            "notify-send",
            "not installed · optional",
            "package: libnotify (Debian: libnotify-bin)",
            "Secret Service keychain",
            "package: a Secret Service keychain, e.g. gnome-keyring or KeePassXC",
            "only sees fonts installed on this computer",
            "Enter checks again · any other key closes",
        ] {
            assert!(all.contains(want), "{want:?} is missing from:\n{all}");
        }
        // Nothing to answer, nothing to install from here.
        for gone in ["Install", "y/n", "pacman", "Don't ask"] {
            assert!(!all.contains(gone), "{gone:?} should not be there:\n{all}");
        }
        assert_eq!(key_caps(&app), [("Enter", "Check Again"), ("Any Other Key", "Close")]);
        // The app is behind it, as with Help.
        assert!(all.contains("STATIONS") || all.contains("NOW PLAYING") || all.contains("Stations"), "{all}");
        // What is there gets a tick and no package line.
        app.system = Some(checks(
            true,
            &Facts {
                families: Some("X Nerd Font\n".into()),
                glyph_fonts: None,
                notify_send: true,
                keychain: Some(true),
            },
        ));
        let all = screen(&app, 110, 30).0.join("\n");
        assert!(all.contains("✔ Nerd Font symbols") && !all.contains("package:"), "{all}");
        // While it looks again.
        app.system = None;
        assert!(screen(&app, 110, 30).0.join("\n").contains("Checking…"));
    }

    #[test]
    fn the_system_check_is_plain_text_and_fits_a_small_terminal() {
        use crate::deps::Facts;
        let mut app = App::new(Config::default(), None, &[], true);
        // The app behind it may use the emoji set; the list itself is plain text.
        app.config.ui.icons = crate::config::IconSet::Emoji;
        app.system_checked(&Facts {
            families: Some("DejaVu Sans\n".into()),
            glyph_fonts: None,
            notify_send: false,
            keychain: Some(false),
        });
        // No glyph from the private use area is added by the list: it reads without a Nerd Font.
        let private =
            |all: &str| all.chars().filter(|c| ('\u{e000}'..='\u{f8ff}').contains(c) || *c > '\u{f0000}').count();
        for (cols, rows) in [(110, 30), (60, 24), (40, 14)] {
            app.system_open = false;
            let without = private(&screen(&app, cols, rows).0.join(""));
            app.system_open = true;
            let with = private(&screen(&app, cols, rows).0.join(""));
            assert!(with <= without, "{cols}x{rows}");
        }
    }
}

/// The README's screenshots: `cargo test --release screenshots -- --ignored`
/// writes each screen (demo mode, no account) as cells to
/// `target/screenshots/*.json`; `scripts/screenshots.py` draws them to PNG.
#[cfg(test)]
mod screenshots {
    use super::*;
    use crate::config::{AlbumArt, ArtTint, Config, RetroMode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::Color;

    const COLS: u16 = 160;
    const ROWS: u16 = 40;

    fn app() -> App {
        let mut app = App::new(Config::default(), None, &[], true);
        app.config.ui.set_theme("tokyo-night");
        app.apply_theme();
        app.demo_warm_up(3, std::time::Duration::ZERO);
        app.settle(std::time::Duration::from_millis(400));
        let station = app.playing_station_in_reach().map(|s| s.name).unwrap_or_default();
        app.status = format!("Playing {station}");
        cover(&mut app);
        app
    }

    /// A made-up synthwave cover with detail, so the art styles show their difference.
    fn cover(app: &mut App) {
        use image::{DynamicImage, Rgb, RgbImage};
        let Some(song) = app.now.clone() else { return };
        let n = 256u32;
        let img = RgbImage::from_fn(n, n, |x, y| {
            let (fx, fy) = (x as f32 / n as f32, y as f32 / n as f32);
            let horizon = 0.62;
            if fy < horizon {
                // Sky: deep purple to orange, a striped sun, a mountain ridge.
                let t = fy / horizon;
                let mut c = [40.0 + 215.0 * t, 20.0 + 70.0 * t, 90.0 - 30.0 * t];
                let (dx, dy) = (fx - 0.5, fy - 0.42);
                if (dx * dx + dy * dy).sqrt() < 0.2 && !(fy > 0.42 && ((fy * 60.0) as u32).is_multiple_of(3)) {
                    c = [255.0, 200.0 - 160.0 * (fy / 0.62), 80.0 + 60.0 * (1.0 - fy)];
                }
                let ridge = horizon - 0.08 - 0.07 * ((fx * 9.0).sin() * 0.6 + (fx * 23.0).cos() * 0.4).abs();
                if fy > ridge {
                    c = [30.0, 15.0, 55.0];
                }
                if fy <= ridge && (x * 7 + y * 13) % 97 == 0 && fy < 0.3 {
                    c = [255.0, 255.0, 255.0];
                }
                Rgb(c.map(|v| v.clamp(0.0, 255.0) as u8))
            } else {
                // A neon grid floor.
                let d = (fy - horizon) / (1.0 - horizon);
                let gx = ((fx - 0.5) / (d + 0.05) * 4.0).fract().abs();
                let gy = (1.0 / (d + 0.08)).fract();
                if gx < 0.08 || gy < 0.12 {
                    Rgb([255, 60, 200])
                } else {
                    Rgb([(25.0 + 30.0 * d) as u8, 10, (50.0 + 40.0 * d) as u8])
                }
            }
        });
        app.art.arrived(song.cover_art_url.clone(), Some(DynamicImage::ImageRgb8(img)));
    }

    fn key(app: &mut App, code: KeyCode) {
        let mods = match code {
            KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
            _ => KeyModifiers::NONE,
        };
        app.on_key(KeyEvent::new(code, mods));
        app.settle(std::time::Duration::from_millis(300));
    }

    fn hex(c: Color, reset: Color) -> String {
        match c {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            Color::Reset => hex(reset, Color::Rgb(0, 0, 0)),
            other => format!("{other:?}"),
        }
    }

    fn shot(app: &App, name: &str) {
        let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        let p = app.palette;
        let cells: Vec<serde_json::Value> = buffer
            .content()
            .iter()
            .map(|c| {
                let bold = c.modifier.contains(ratatui::style::Modifier::BOLD);
                let rev = c.modifier.contains(ratatui::style::Modifier::REVERSED);
                let (fg, bg) = (hex(c.fg, p.fg), hex(c.bg, p.background));
                let (fg, bg) = if rev { (bg, fg) } else { (fg, bg) };
                serde_json::json!([c.symbol(), fg, bg, bold])
            })
            .collect();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/screenshots");
        std::fs::create_dir_all(&dir).unwrap();
        let doc = serde_json::json!({ "cols": COLS, "rows": ROWS, "cells": cells });
        std::fs::write(dir.join(format!("{name}.json")), doc.to_string()).unwrap();
    }

    #[test]
    #[ignore = "writes the README screenshots"]
    fn screenshots() {
        shot(&app(), "now-playing");

        let styles = [
            ("hd", RetroMode::Hd, ArtTint::Off),
            ("64bit", RetroMode::Bit64, ArtTint::Off),
            ("32bit", RetroMode::Bit32, ArtTint::Off),
            ("16bit", RetroMode::Bit16, ArtTint::Off),
            ("8bit", RetroMode::Bit8, ArtTint::Off),
            ("4bit", RetroMode::Bit4, ArtTint::Off),
            ("2bit", RetroMode::Bit2, ArtTint::Off),
            ("duotone", RetroMode::Hd, ArtTint::Duotone),
            ("palette", RetroMode::Bit16, ArtTint::Palette),
        ];
        for (name, retro, tint) in styles {
            let mut a = app();
            a.config.ui.album_art = AlbumArt::Picture;
            a.config.ui.retro_mode = retro;
            a.config.ui.art_tint = tint;
            shot(&a, &format!("art-{name}"));
        }
        let mut a = app();
        a.config.ui.album_art = AlbumArt::Off;
        shot(&a, "art-off");

        // The station editor: seeds, then thumbs up.
        let mut a = app();
        a.view = View::Stations;
        a.focus = crate::app::Focus::Content;
        a.stations_cursor = a.stations.iter().position(|s| s.name == "M83 Radio").unwrap();
        key(&mut a, KeyCode::Char('e'));
        shot(&a, "editor-seeds");
        key(&mut a, KeyCode::Tab);
        shot(&a, "editor-thumbs");

        // Creating a station from a search.
        let mut a = app();
        key(&mut a, KeyCode::Char('S'));
        key(&mut a, KeyCode::Char('a'));
        for c in "m83".chars() {
            a.on_key(KeyEvent::from(KeyCode::Char(c)));
        }
        key(&mut a, KeyCode::Enter);
        shot(&a, "create-station");

        // Favorites: a few starred stations in the menu and on the Stations page.
        let mut a = app();
        for s in a.stations.iter().skip(1).take(3).cloned().collect::<Vec<_>>() {
            a.config.ui.favorites.push(crate::config::Favorite { id: s.id, name: s.name });
        }
        a.view = View::Stations;
        a.focus = crate::app::Focus::Content;
        shot(&a, "favorites");

        let mut a = app();
        a.view = View::Settings;
        a.focus = crate::app::Focus::Content;
        shot(&a, "settings");
    }
}
