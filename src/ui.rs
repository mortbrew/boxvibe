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
    App, BrowseRow, BrowseState, Details, Editor, EditorTab, FeedState, Focus, Loaded, MenuItem, Pane, Prompt, Rename,
    SearchMode, Session, Setting, SongAction, StationSearch, View,
};
use crate::art::Cover;
use crate::config::{AlbumArt, BorderStyle, GoodVibes};
use crate::icons::{Icons, icons};
use crate::input::TextInput;
use crate::pandora::{Rating, Song, station_ids_match};
use crate::theme::Palette;
use crate::update::{Progress, Replace};
use ratatui_image::{Resize, StatefulImage};
use vibekit::dialog::Section;

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

/// Blank columns between the menu and the page, so the two frames do not touch.
pub const PANE_GAP: u16 = 1;

pub fn draw(frame: &mut Frame, app: &mut App) {
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
    let [left, _, right] =
        Layout::horizontal([Constraint::Length(side), Constraint::Length(PANE_GAP), Constraint::Fill(1)]).areas(body);
    draw_menu(frame, app, ic, left);
    match app.view {
        View::Player => draw_player(frame, app, ic, right),
        View::Stations => draw_stations(frame, app, ic, right),
        View::Browse => draw_browse(frame, app, ic, right),
        View::Pinned => draw_pinned(frame, app, ic, right),
        View::Recent => draw_recent(frame, app, ic, right),
        View::Settings => draw_settings(frame, app, ic, right),
        View::About => draw_about(frame, app, ic, right),
        View::Notifications => draw_notes(frame, app, ic, right),
    }
    draw_status(frame, app, status);
    draw_keys(frame, &p, &key_caps(app), keys);
    if app.overlay_open() {
        dim(frame.buffer_mut(), p.background);
    }
    if let Some(menu) = &app.song_menu {
        draw_song_menu(frame, &p, body, menu);
    }
    if let Some(panel) = &app.panel {
        draw_panel(frame, &p, body, panel);
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
    // Questions (incl. editor / theme-delete), Help, details, System Check — on top of the popups above.
    app.dialogs.draw_dialogs(frame, &p);
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
pub(crate) fn draw_details(frame: &mut Frame, p: &Palette, area: Rect, d: &Details) {
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
pub(crate) fn clean(text: &str) -> String {
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
    // The right side (sleep timer, look, stream, account) gets at most half
    // the bar, and never less than the status.
    let room = (area.width / 2).saturating_sub(3) as usize;
    let account = account_label(app, room);
    let mut sleep = sleep_label(app);
    let mut looks = look_labels(app);
    let mut stream = stream_label(app);
    // What goes when the bar is narrow, first to last: the look labels (from
    // the last), then the stream, then the timer. The account never goes.
    let used = |sleep: &Option<String>, looks: &[String], stream: &Option<String>| -> usize {
        let parts = sleep.iter().chain(looks).chain(stream).map(|s| safe_width(s) + 3).sum::<usize>();
        parts + account.as_ref().map_or(0, |a| safe_width(a))
    };
    while used(&sleep, &looks, &stream) > room {
        if looks.pop().is_some() {
            continue;
        }
        if stream.take().is_some() {
            continue;
        }
        if sleep.take().is_some() {
            continue;
        }
        break;
    }
    let extra: Vec<String> = sleep.into_iter().chain(looks).chain(stream).collect();
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
    for label in &extra {
        // The look and stream labels share the account's colour; the sleep
        // timer stays yellow.
        let color = if label.starts_with('☾') { p.yellow } else { p.blue };
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

/// What the song now playing is really streamed as: "AAC 64 kbps", "MP3 192
/// kbps". Pandora does not always send what was asked for (a free plan gets
/// HE-AAC whatever `high` says), so this is read from the song, and it changes
/// with every song that arrives in a different format.
fn stream_label(app: &App) -> Option<String> {
    let (format, kbps) = app.stream?;
    let what = match format {
        crate::pandora::AudioFormat::Mp3 => "MP3",
        crate::pandora::AudioFormat::Aac => "AAC",
        crate::pandora::AudioFormat::Unknown => return None,
    };
    Some(if kbps > 0 { format!("{what} {kbps} kbps") } else { what.to_owned() })
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

fn logo_line_with<'a>(p: &Palette, icon: Span<'a>, rest: &str) -> Line<'a> {
    Line::from(vec![
        icon,
        Span::styled("box", Style::new().fg(p.blue).bold()),
        Span::styled(format!("vibe{rest} "), Style::new().fg(p.fg).bold()),
    ])
}

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
    let section_pins = app.browse_menu_pins().len() as u16;
    let pinned = app.config.ui.pinned_in_menu;
    // MY COLLECTION groups the personal rows -- My Stations, the optional
    // Pinned songs, and Recently Played once it has something to show -- ruled
    // and headed like FAVORITES. The pinned Browse sections follow under
    // their own "BROWSE (n)" heading.
    let recent_n = u16::from(app.recent_row());
    let personal_n = 1 + u16::from(pinned) + recent_n;
    // Hidden while it has nothing of its own, the way FAVORITES hides.
    let collection_head = !app.stations.is_empty() || (pinned && !app.pins.is_empty()) || app.recent_row();
    // The only rule above MY COLLECTION is the one under NOW PLAYING, so it
    // stays hidden until something is playing.
    let collection_n = personal_n + u16::from(collection_head);
    let browse_n = if section_pins == 0 { 0 } else { section_pins + 2 };
    let [now_area, collection_area, browse_area, favs_area, _, rule, app_area] = Layout::vertical([
        Constraint::Length(now_h),
        Constraint::Length(collection_n),
        Constraint::Length(browse_n),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(4),
    ])
    .areas(inner);
    let selected = app.menu_selected();
    // Without the keys, the entry of the page shown stays shaded.
    // Browse pins share View::Browse; match browse_filter so only the active pin shades.
    let shown = |item: MenuItem| match (item, app.view) {
        (MenuItem::Settings, View::Settings)
        | (MenuItem::About, View::About)
        | (MenuItem::Notifications, View::Notifications)
        | (MenuItem::NowPlaying, View::Player)
        | (MenuItem::Stations, View::Stations)
        | (MenuItem::Pinned, View::Pinned)
        | (MenuItem::RecentlyPlayed, View::Recent) => true,
        (MenuItem::BrowsePin(i), View::Browse) => app
            .browse_filter
            .as_ref()
            .is_some_and(|want| app.browse_menu_pins().get(i).is_some_and(|(_, title)| title == want)),
        _ => false,
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
    // The heading and the rows it covers; without a heading the rows take the
    // whole area.
    let (rows_area, head_area) = if collection_head {
        let [head, rows] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(personal_n)]).areas(collection_area);
        (rows, Some(head))
    } else {
        (collection_area, None)
    };
    if let Some(head) = head_area {
        frame.render_widget(Paragraph::new(Span::styled(" MY COLLECTION", Style::new().fg(p.orange).bold())), head);
    }
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let mut lines = Vec::new();
    let mut top = vec![(MenuItem::Stations, ic.station, format!("My Stations{count}"))];
    if app.recent_row() {
        top.push((MenuItem::RecentlyPlayed, ic.recent, format!("Recently Played ({})", app.recent.len())));
    }
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
        lines.push(Line::from(pad(&text, rows_area.width as usize)).style(style));
    }
    frame.render_widget(Paragraph::new(lines), rows_area);
    // The pinned Browse sections, under a heading that matches FAVORITES: a
    // rule, "BROWSE (n)", then one row each.
    if section_pins > 0 {
        let [gap, head, rows] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(section_pins)])
                .areas(browse_area);
        frame.render_widget(inset_rule(&p, gap.width), gap);
        frame.render_widget(
            Paragraph::new(Span::styled(format!(" BROWSE ({section_pins})"), Style::new().fg(p.orange).bold())),
            head,
        );
        let mut lines = Vec::new();
        for (i, (_, title)) in app.browse_menu_pins().into_iter().enumerate() {
            let icon = if title == "Browse Genres" { ic.genre } else { ic.browse };
            let mut style = Style::new().fg(p.fg);
            if let Some(sel) = row_style(MenuItem::BrowsePin(i)) {
                style = style.patch(sel);
            }
            let text = format!(" {} {title}", pad(icon, 2));
            lines.push(Line::from(pad(&text, rows.width as usize)).style(style));
        }
        frame.render_widget(Paragraph::new(lines), rows);
    }
    // FAVORITES, like tubevibe's tabs: only once a station is starred. While
    // the stations load the rows would have a blank where the Pandora /
    // your-own mark goes, so one loading row stands in for them.
    let favorites = &app.config.ui.favorites;
    if app.favorites_in_menu() && favs_area.height > 1 {
        let [gap, head, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)]).areas(favs_area);
        frame.render_widget(inset_rule(&p, gap.width), gap);
        frame.render_widget(
            Paragraph::new(Span::styled(format!(" FAVORITES ({})", favorites.len()), Style::new().fg(p.orange).bold())),
            head,
        );
        if app.favorites_loading() {
            let text = format!(" {} Loading favorites…", ic.loading);
            frame.render_widget(
                Paragraph::new(Line::from(pad(&text, list_area.width as usize))).style(Style::new().fg(p.muted)),
                list_area,
            );
        } else {
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
    let local_only = " Pins stay on this device only (not in your Pandora account).";
    if app.pins.is_empty() {
        let text = format!(
            " Nothing pinned yet. Shift+I pins the selected or playing song: its title, artist, album, cover and Pandora page (never the audio).{local_only}"
        );
        frame.render_widget(Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: false }).fg(p.muted), inner);
        return;
    }
    let [note_area, list_area] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(local_only.trim()).wrap(ratatui::widgets::Wrap { trim: false }).fg(p.muted),
        note_area,
    );
    let inner = list_area;
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

// --- Recently played ---

fn draw_recent(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let count = if app.recent.is_empty() { String::new() } else { format!(" ({})", app.recent.len()) };
    let block = panel(title(format!("{} Recently Played{count}", ic.recent), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.recent.is_empty() {
        let text = if app.recent_state.loading() {
            format!(" {} Loading…", ic.loading)
        } else {
            " Nothing played recently yet".to_owned()
        };
        frame.render_widget(Paragraph::new(text).fg(p.muted), inner);
        return;
    }
    let width = inner.width as usize;
    let items: Vec<ListItem> = app
        .recent
        .iter()
        .map(|source| {
            let playing = app.current_station.as_deref().is_some_and(|id| station_ids_match(id, &source.id));
            let icon = if playing {
                ic.play
            } else if source.kind == "ST" {
                ic.station
            } else {
                ic.recent
            };
            let style = if playing { Style::new().fg(p.green).bold() } else { Style::new().fg(p.fg) };
            let head = format!(" {} ", pad(icon, 2));
            let room = width.saturating_sub(safe_width(&head));
            let name = truncate(&clean(&app.recent_title(source)), room);
            ListItem::new(Line::from(format!("{head}{name}"))).style(style)
        })
        .collect();
    let list = List::new(items).highlight_style(selection(&p, focused));
    let mut state = ListState::default().with_selected(Some(app.recent_cursor.min(app.recent.len() - 1)));
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
    let block = panel(title(format!("{} My Stations{count}", ic.station), &p), &p, focused);
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

fn draw_browse(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = &app.palette;
    let focused = app.focus == Focus::Content;
    let heading = match &app.browse_filter {
        Some(section) => format!("{} {section}", if section == "Browse Genres" { ic.genre } else { ic.browse }),
        None => format!("{} Browse", ic.browse),
    };
    let block = panel(title(heading, p), p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let message = match &app.browse {
        BrowseState::Idle if app.session != Session::LoggedIn => {
            Some(" Log in (Settings, Shift+O) to browse".to_owned())
        }
        BrowseState::Idle | BrowseState::Loading => Some(format!(" {} Loading Browse…", ic.loading)),
        BrowseState::Failed(why) => Some(format!(" {} {}  (Enter tries again)", ic.warn, clean(why))),
        BrowseState::Loaded(sections)
            if sections.is_empty() || app.browse_rows().iter().all(|r| matches!(r, BrowseRow::Header(_))) =>
        {
            Some(" Nothing to show (every section is off in Settings, or Pandora sent none)".to_owned())
        }
        BrowseState::Loaded(_) => None,
    };
    if let Some(text) = message {
        frame.render_widget(Paragraph::new(text).fg(p.muted), inner);
        return;
    }
    let rows = app.browse_rows();
    let width = inner.width as usize;
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row = 0;
    for (i, row) in rows.iter().enumerate() {
        if i == app.browse_index() {
            selected_row = items.len();
        }
        match row {
            BrowseRow::Header(title) => {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!(" {}", clean(title).to_uppercase()),
                    Style::new().fg(p.orange).bold(),
                ))));
            }
            BrowseRow::Item(item) => {
                let mark = if item.create_id().is_some() { "▶" } else { "·" };
                let head = format!("   {mark} ");
                let name = clean(&item.name);
                let mut line = vec![Span::styled(format!("{head}{name}"), Style::new().fg(p.fg))];
                if !item.detail.is_empty() {
                    let used = safe_width(&head) + safe_width(&name) + 3;
                    let room = width.saturating_sub(used);
                    if room > 8 {
                        line.push(Span::styled(
                            format!("  {}", fit(&clean(&item.detail), room - 2)),
                            Style::new().fg(p.muted),
                        ));
                    }
                }
                items.push(ListItem::new(Line::from(line)));
            }
        }
    }
    let list = List::new(items).highlight_style(selection(p, focused));
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
        results_hint: ADD_RESULTS_HINT,
    };
    draw_search_box(frame, p, screen, search, &look);
}

/// The hints under the add boxes' results; they fit the 62 cells inside the box.
pub const ADD_RESULTS_HINT: &str = "Enter make station · j/k move · / edit · Esc cancel";
pub const SEED_RESULTS_HINT: &str = "Enter add seed · j/k move · / edit · Esc cancel";

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
            (true, true) => "Enter add suggestion · ↓ pick another · Esc cancel",
            (true, false) => "Enter search · ↓ results · Esc cancel",
            (false, _) => "Enter search · Esc cancel",
        }
    } else {
        look.results_hint
    };
    // Short like the theme picker's, and cut with "…" rather than mid-word on a tiny screen.
    lines.push(Line::from(Span::styled(fit(hint, inner_w), Style::new().fg(p.muted))));
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
            results_hint: SEED_RESULTS_HINT,
        };
        draw_search_box(frame, p, screen, &add.search, &look);
    }
}

/// Settings → Theme Select, drawn by vibekit inside boxvibe's panel.
fn draw_theme_picker(frame: &mut Frame, app: &App, screen: Rect, picker: &crate::app::ThemePicker) {
    let p = &app.palette;
    let installed = app.installed_themes();
    let block = panel(title("Theme Select".into(), p), p, true).style(Style::new().bg(p.background));
    let view = vibekit::picker::View { palette: p, installed: &installed, current: app.config.ui.theme(), block };
    vibekit::picker::draw(frame, screen, picker, view);
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

fn draw_player(frame: &mut Frame, app: &mut App, ic: &Icons, area: Rect) {
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
/// It is the page's width, which is [`PANE_GAP`] narrower than the terminal
/// leaves it, so the same terminal keeps the layout it had before the gap.
pub const STACK_BELOW: u16 = 89;

pub fn stack_songs(width: u16) -> bool {
    width < STACK_BELOW
}

fn draw_now_playing(frame: &mut Frame, app: &mut App, ic: &Icons, area: Rect) {
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
    // The look is read before the song is borrowed: `draw_art` needs
    // `&mut app.art`, and a borrow of `app.now` would cover the whole struct.
    let look = crate::art::Look::of(&app.config.ui, p);
    let Some(song) = &app.now else {
        let text = match (&app.session, &app.current_station) {
            (Session::LoggedIn, Some(_)) => format!("{} Getting songs from Pandora…", ic.loading),
            (Session::LoggedIn, None) => "Open My Stations on the left, pick one and press Enter".into(),
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
    let drawn = draw_art(frame, &mut app.art, look, song, art);
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
fn draw_ad(frame: &mut Frame, app: &mut App, ic: &Icons, ad: &crate::app::AdView, area: Rect) {
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
fn draw_art(frame: &mut Frame, art: &mut crate::art::Art, look: crate::art::Look, song: &Song, area: Rect) -> Rect {
    let p = look.palette;
    // No frame: the cover (or the message) fills the box itself.
    let inner = area;
    art.last_area = inner;
    if inner.width == 0 || inner.height == 0 {
        return inner;
    }
    if let Some(cover) = art.protocol(&song.cover_art_url, look) {
        // Against the left edge of the box (the pane's padding), centred
        // top to bottom: the picture keeps its shape.
        let fit = cover.size_for(Resize::Fit(None), inner.as_size());
        let x = inner.x;
        let y = inner.y + inner.height.saturating_sub(fit.height) / 2;
        let at = Rect::new(x, y, fit.width.min(inner.width), fit.height.min(inner.height));
        frame.render_stateful_widget(StatefulImage::default(), at, cover);
        art.check_encoding(&song.cover_art_url);
        art.last_cover = at;
        return at;
    }
    art.hide();
    // No cover to draw: say so, in the middle of the space the cover would fill.
    let loading = look.mode == AlbumArt::Picture
        && !song.cover_art_url.is_empty()
        && matches!(art.status(&song.cover_art_url), Cover::Loading);
    // Off by choice (Shift+C) says so; a song with no cover, or one that failed, says there is none.
    let message = if look.mode == AlbumArt::Off {
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
            (Setting::Quality, _) if app.quality_greyed() => Style::new().fg(p.muted),
            _ => Style::new().fg(p.yellow).bold(),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("   {} ", pad(ic.setting[i], 2))),
            Span::styled(
                pad(setting.label(), label_w),
                Style::new().fg(if app.setting_greyed(*setting) { p.muted } else { p.fg }),
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
    popup_key_caps(app).unwrap_or_else(|| page_key_caps(app))
}

/// The key bar while a popup owns the screen: its own keys only. `None` when
/// no popup is open and the page's keys apply.
fn popup_key_caps(app: &App) -> Option<Vec<(&'static str, &'static str)>> {
    if app.system_open() {
        return Some(vec![("Enter", "Check Again"), ("Esc", "Close")]);
    }
    if app.rename.is_some() {
        return Some(vec![
            ("Enter", "Rename"),
            ("Esc", "Cancel"),
            ("← →", "Move"),
            ("Ctrl+u", "Clear"),
            ("Ctrl+c", "Quit"),
        ]);
    }
    if app.confirm_delete().is_some() {
        return Some(vec![("y", "Yes, Delete"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if app.confirm_unpin().is_some() {
        return Some(vec![("y", "Yes, Unpin"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if app.confirm_forget_recent().is_some() {
        return Some(vec![("y", "Yes, Remove"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if app.theme_delete_asked().is_some() {
        return Some(vec![("y", "Yes, Delete"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if app.editor_confirm().is_some() {
        return Some(vec![("y", "Yes"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if let Some(mode) = app.confirm_good_vibes {
        let yes = if mode == GoodVibes::Muted { "Mute Anyway" } else { "Turn Off Anyway" };
        return Some(vec![("Enter", yes), ("Esc", "Keep As Is"), ("Ctrl+c", "Quit")]);
    }
    if app.genre_notice.is_some() {
        return Some(vec![("Space", "Tick Box"), ("Enter", "OK"), ("Esc", "Close"), ("Ctrl+c", "Quit")]);
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
        return Some(keys);
    }
    if let Some(picker) = &app.theme_picker {
        return Some(picker.key_caps());
    }
    if let Some(ed) = &app.editor {
        return Some(editor_key_caps(ed));
    }
    if let Some(search) = &app.add_station {
        return Some(match search.mode {
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
        });
    }
    if app.prompt.is_some() {
        return Some(vec![
            ("Enter", "Save"),
            ("Esc", "Cancel"),
            ("← →", "Move"),
            ("Ctrl+u", "Clear"),
            ("Ctrl+c", "Quit"),
        ]);
    }
    if app.quit_asked() {
        return Some(vec![("Q y", "Yes, Quit"), ("n Esc", "No"), ("← → Enter", "Choose")]);
    }
    if app.details().is_some() {
        return Some(vec![("Esc", "Close")]);
    }
    if app.help_open() {
        return Some(vec![("j/k ↑↓", "Scroll"), ("? Esc", "Close")]);
    }
    None
}

/// The key bar for the page on screen: what the focused pane offers.
fn page_key_caps(app: &App) -> Vec<(&'static str, &'static str)> {
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
        || app.view == View::Recent && app.focus == Focus::Content && app.recent.len() < 2
        || in_songs && app.pane == Pane::NowPlaying
        || app.view == View::Browse && app.focus == Focus::Content && app.browse_rows().is_empty();
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
        (Focus::Content, View::Browse) => {
            match &app.browse {
                BrowseState::Loaded(_) if app.browse_rows().iter().any(|r| matches!(r, BrowseRow::Item(_))) => {
                    let open = app.browse_rows().get(app.browse_index()).is_some_and(|row| match row {
                        BrowseRow::Item(item) => item.create_id().is_none() && item.link.is_some(),
                        BrowseRow::Header(_) => false,
                    });
                    keys.push(("Enter", if open { "Open" } else { "Start Station" }));
                }
                BrowseState::Failed(_) => keys.push(("Enter", "Try Again")),
                _ => {}
            }
            keys.push(("Esc", if app.browse_can_go_back() { "Back" } else { "Menu" }));
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
        (Focus::Content, View::Recent) => {
            if !app.recent.is_empty() {
                keys.push(("x", "Remove"));
            }
            if app.recent.get(app.recent_cursor.min(app.recent.len().saturating_sub(1))).is_some_and(|s| s.kind == "ST")
            {
                keys.push(("Enter", "Play"));
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
    let cost = |shown: &[&(&str, &str)]| shown.iter().map(|k| width(&cap(k))).sum::<usize>();
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
    let mut spans: Vec<Span> = Vec::new();
    let room = (area.width as usize).saturating_sub(width(&last));
    for k in rest {
        let next = cap(k);
        if width(&spans) + width(&next) > room {
            break;
        }
        spans.extend(next);
    }
    spans.extend(last);
    // The gap after the last label is not part of the bar: without it the
    // centring below would leave two more columns on the right than the left.
    if let Some(end) = spans.last_mut() {
        *end = Span::styled(end.content.trim_end().to_owned(), end.style);
    }
    // Centred: as much blank space on the left as on the right.
    let pad = (area.width as usize).saturating_sub(width(&spans)) / 2;
    let mut line = vec![Span::raw(" ".repeat(pad))];
    line.extend(spans);
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

/// Help's sections: (title, [(keys, what they do)]). A test holds this to
/// the key table, so every key is listed and nothing listed is unbound.
pub const HELP: [(&str, &[(&str, &str)]); 4] = [
    (
        "PLAYING (any pane)",
        &[
            ("space", "play / pause (from anywhere)"),
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
            ("a", "add a station: search artists and songs (My Stations page)"),
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
            ("S", "My Stations: every station, Enter plays one"),
            (
                "G",
                "Genres: Pandora's genre stations, Enter adds and plays one (global; End, not G, jumps to the end of a list)",
            ),
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
            ("Q Ctrl+c", "quit (asks first; Q again confirms) / quit now"),
        ],
    ),
];

/// Help, for vibekit's `Help`: every key in `HELP`, the station icons in the
/// glyphs of the icon set in use, and where the rest lives.
pub fn help_sections(icon_set: crate::config::IconSet) -> Vec<Section> {
    let mut sections: Vec<Section> = HELP.iter().map(|(name, keys)| Section::new(*name, keys)).collect();
    let ic = icons(icon_set);
    sections.push(Section::new(
        "STATION ICONS (right after the star)",
        &[
            (ic.genre_station, "genre station: made by Pandora, so it can't be renamed or edited (it can be deleted)"),
            (ic.own_station, "a station you made: you can rename, edit and delete it"),
        ],
    ));
    sections.push(Section::new("MORE", &[("Shift+O", HELP_FOOTER.trim())]));
    sections
}

/// What boxvibe uses from the system and whether it is there, like tubevibe's
/// System Check: a mark, the name, what it is for, and under a missing one the
/// package. Plain characters only: it has to read without the font it may lack.
pub(crate) fn draw_system(frame: &mut Frame, p: &Palette, checks: Option<&[crate::deps::Check]>, area: Rect) {
    use crate::deps::Presence;
    let p = *p;
    let text = Style::new().fg(p.fg);
    let muted = Style::new().fg(p.muted);
    let mut lines = vec![Line::raw("")];
    let Some(checks) = checks else {
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
    lines.push(Line::from(Span::styled("  Enter checks again · Esc closes", muted)));
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
mod tests;

/// The README's screenshots: `cargo test --release screenshots -- --ignored`
/// draws each screen (demo mode, no account) and rasterizes it to
/// `docs/screenshots/*.png`; see `crate::screenshots`.
#[cfg(test)]
mod screenshots {
    use super::*;
    use crate::config::{AlbumArt, ArtTint, Config, RetroMode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
        let Some(song) = &app.now else { return };
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

    fn shot(app: &mut App, name: &str) {
        let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
        crate::screenshots::write(terminal.backend().buffer(), &app.palette, &dir.join(format!("{name}.png"))).unwrap();
    }

    #[test]
    #[ignore = "writes the README screenshots"]
    fn screenshots() {
        shot(&mut app(), "now-playing");

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
            shot(&mut a, &format!("art-{name}"));
        }
        let mut a = app();
        a.config.ui.album_art = AlbumArt::Off;
        shot(&mut a, "art-off");

        // The station editor: seeds, then thumbs up.
        let mut a = app();
        a.view = View::Stations;
        a.focus = crate::app::Focus::Content;
        a.stations_cursor = a.stations.iter().position(|s| s.name == "M83 Radio").unwrap();
        key(&mut a, KeyCode::Char('e'));
        shot(&mut a, "editor-seeds");
        key(&mut a, KeyCode::Tab);
        shot(&mut a, "editor-thumbs");

        // Creating a station from a search.
        let mut a = app();
        key(&mut a, KeyCode::Char('S'));
        key(&mut a, KeyCode::Char('a'));
        for c in "m83".chars() {
            a.on_key(KeyEvent::from(KeyCode::Char(c)));
        }
        key(&mut a, KeyCode::Enter);
        shot(&mut a, "create-station");

        // Favorites: a few starred stations in the menu and on the Stations page.
        let mut a = app();
        for s in a.stations.iter().skip(1).take(3).cloned().collect::<Vec<_>>() {
            a.config.ui.favorites.push(crate::config::Favorite { id: s.id, name: s.name });
        }
        a.view = View::Stations;
        a.focus = crate::app::Focus::Content;
        shot(&mut a, "favorites");

        let mut a = app();
        a.view = View::Settings;
        a.focus = crate::app::Focus::Content;
        shot(&mut a, "settings");
    }
}
