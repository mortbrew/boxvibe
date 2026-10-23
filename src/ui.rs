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

use crate::app::{App, Focus, MenuItem, Pane, Prompt, Session, Setting, View};
use crate::art::Cover;
use crate::config::{AlbumArt, BorderStyle};
use crate::icons::{Icons, icons};
use crate::pandora::{Rating, Song};
use crate::theme::Palette;
use crate::update::Progress;
use ratatui_image::{Resize, StatefulImage};

thread_local! {
    /// `[ui] border_style` of the frame being drawn (set by [`draw`]), like tubevibe.
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
    let ic = icons(app.config.ui.icons);
    let [body, status, keys] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let side = menu_width(app, body.width);
    let [left, right] = Layout::horizontal([Constraint::Length(side), Constraint::Fill(1)]).areas(body);
    draw_menu(frame, app, ic, left);
    match app.view {
        View::Player => draw_player(frame, app, ic, right),
        View::Stations => draw_stations(frame, app, ic, right),
        View::Settings => draw_settings(frame, app, ic, right),
        View::About => draw_about(frame, app, ic, right),
    }
    draw_status(frame, app, status);
    draw_keys(frame, &p, &key_caps(app), keys);
    if app.help {
        draw_help(frame, app, body);
    }
    if app.confirm_quit {
        draw_confirm_quit(frame, &p, area);
    }
}

/// tubevibe's confirm box: " Confirm " on a red border; a blank line, the
/// question, the keys and a blank line, so it sits evenly in the frame.
fn draw_confirm_quit(frame: &mut Frame, p: &Palette, screen: Rect) {
    draw_confirm(frame, p, screen, "Quit boxvibe?");
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

fn draw_confirm(frame: &mut Frame, p: &Palette, screen: Rect, question: &str) {
    let width = 40.min(screen.width.saturating_sub(4)).max(screen.width.min(8));
    // Borders and the three fixed lines take 5 rows; the question gets what's left.
    let room = usize::from(screen.height.saturating_sub(2).saturating_sub(5)).max(1);
    let question = wrap_question(question, usize::from(width.saturating_sub(4)), room);
    let mut lines = vec![Line::from("")];
    lines.extend(question.into_iter().map(Line::from));
    lines.push(Line::from(Span::styled("y = yes · any other key = no", Style::new().fg(p.muted))));
    lines.push(Line::from(""));
    let area = centered(screen, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    let block = framed().border_style(Style::new().fg(p.red)).title(" Confirm ").style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center).block(block), area);
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
/// and the logged-in account at its far right.
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    let color = if app.status.starts_with('⚠') { p.yellow } else { p.fg };
    frame.render_widget(Block::new().bg(p.highlight), area);
    let account = account_label(app);
    // The account gets at most half the bar, and never less than the status.
    let want = account.as_ref().map_or(0, |a| safe_width(a) as u16 + 3);
    let right_w = want.min(area.width / 2);
    let [left, right] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_w)]).areas(area);
    let status = fit(&format!(" {}", app.status), left.width.saturating_sub(1) as usize);
    frame.render_widget(Paragraph::new(Span::styled(status, Style::new().fg(color))), left);
    if let Some(account) = account
        && right.width > 2
    {
        // tubevibe's colour for its account: the palette's `blue` (Gruvbox's aqua).
        let text = format!("{} ", fit(&account, right.width as usize - 2));
        frame.render_widget(Paragraph::new(text).fg(p.blue).alignment(Alignment::Right), right);
    }
}

/// "👤 me@example.com" while logged in; nothing otherwise.
fn account_label(app: &App) -> Option<String> {
    let name = app.config.pandora.username.trim();
    (app.session == Session::LoggedIn && !name.is_empty())
        .then(|| format!("{}{}", icon_cell(icons(app.config.ui.icons).setting[0]), clean(name)))
}

/// The logo on the menu's top border, like tubevibe's: the box icon and
/// "box" in the theme's blue, "vibe" in the text colour.
pub fn logo_title<'a>(p: &Palette, ic: &Icons) -> Line<'a> {
    logo_line(p, ic, "")
}

/// The one place the logo is drawn: the menu's title and the Help window's
/// title share it, with `rest` (bold, text colour) after "boxvibe".
fn logo_line<'a>(p: &Palette, ic: &Icons, rest: &str) -> Line<'a> {
    logo_line_with(p, Span::styled(format!(" {} ", ic.logo), Style::new().fg(p.blue).bold()), rest)
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
    let block = panel(logo_title(&p, ic), &p, focused).title(version);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let now_h = if app.has_now_playing() { 2 } else { 0 };
    let [now_area, stations_row, favs_area, _, rule, app_area] = Layout::vertical([
        Constraint::Length(now_h),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .areas(inner);
    let selected = app.menu_selected();
    // Without the keys, the entry of the page shown stays shaded.
    let shown = |item: MenuItem| {
        matches!(
            (item, app.view),
            (MenuItem::Settings, View::Settings)
                | (MenuItem::About, View::About)
                | (MenuItem::NowPlaying, View::Player)
                | (MenuItem::Stations, View::Stations)
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
    let mut style = Style::new().fg(p.fg);
    if let Some(sel) = row_style(MenuItem::Stations) {
        style = style.patch(sel);
    }
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let text = format!(" {} Stations{count}", pad(ic.station, 2));
    frame.render_widget(Paragraph::new(Line::from(pad(&text, stations_row.width as usize)).style(style)), stations_row);
    // FAVORITES, like tubevibe's tabs: only once a station is starred.
    let favorites = &app.config.ui.favorites;
    if !favorites.is_empty() && favs_area.height > 1 {
        let [gap, head, list_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Fill(1)]).areas(favs_area);
        frame.render_widget(inset_rule(&p, gap.width), gap);
        frame.render_widget(
            Paragraph::new(Span::styled(format!(" FAVORITES ({})", favorites.len()), Style::new().fg(p.orange).bold())),
            head,
        );
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
                let line = format!(" {} {}", pad(icon, 2), f.name);
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

// --- Stations ---

fn draw_stations(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let block = panel(title(format!("{} Stations{count}", ic.station), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.stations.is_empty() {
        let text = match &app.session {
            Session::SigningIn => format!(" {} Signing in…", ic.loading),
            Session::LoggedIn => format!(" {} Loading stations…", ic.loading),
            _ => " Log in from Settings".into(),
        };
        frame.render_widget(Paragraph::new(text).fg(p.muted), inner);
        return;
    }
    let items: Vec<ListItem> = app
        .stations
        .iter()
        .map(|s| {
            let playing = app.current_station.as_deref() == Some(s.id.as_str());
            let icon = if s.is_shuffle {
                ic.shuffle
            } else if s.is_shared {
                ic.shared
            } else {
                ic.station
            };
            let star = if app.is_favorite(&s.id) { ic.favorite } else { " " };
            let style = if playing { Style::new().fg(p.green).bold() } else { Style::new().fg(p.fg) };
            let mark = if playing { ic.play } else { " " };
            let head = format!(" {} {} {} ", pad(mark, 1), icon_cell(icon), icon_cell(star));
            let name = fit(&s.name, (inner.width as usize).saturating_sub(safe_width(&head)));
            ListItem::new(Line::from(format!("{head}{name}"))).style(style)
        })
        .collect();
    let list = List::new(items).highlight_style(selection(&p, focused));
    let mut state = ListState::default().with_selected(Some(app.stations_index()));
    frame.render_stateful_widget(list, inner, &mut state);
}

// --- Player ---

/// Rows Up Next and History each show at most (History keeps 10 songs).
pub const SONG_ROWS: u16 = 10;
/// One song pane: borders, the header and [`SONG_ROWS`] rows.
pub const SONG_PANE: u16 = SONG_ROWS + 3;
/// Now Playing never gets fewer rows than this while the song panes have room to give.
pub const NOW_PLAYING_MIN: u16 = 9;

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
    let inner = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
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
    // The art fills the pane's height, two cells wide per row so it looks
    // square; song info, the time line and the volume sit to its right.
    let art_w = (inner.height * 2 + 2).min(inner.width / 2);
    let [art, _, info] =
        Layout::horizontal([Constraint::Length(art_w), Constraint::Length(3), Constraint::Fill(1)]).areas(inner);
    // The time line ends as far in from the right border as the cover
    // starts from the left one, so the pane is symmetrical; the whole stack
    // (title, time, volume, buttons) keeps to that width.
    let drawn = draw_art(frame, app, song, art);
    let gap = drawn.x.saturating_sub(area.x);
    let width = (area.right().saturating_sub(gap)).saturating_sub(info.x);
    let info = if width >= MIN_INFO_WIDTH { Rect { width: width.min(info.width), ..info } } else { info };
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

/// Rows Now Playing needs before the buttons join the stack.
pub const STACK_WITH_BUTTONS: u16 = 13;

/// The info stack is not narrowed below this to mirror the cover's gap.
const MIN_INFO_WIDTH: u16 = 30;

/// Draws the cover (or the visualizer) and returns where it went.
fn draw_art(frame: &mut Frame, app: &App, song: &Song, area: Rect) -> Rect {
    let p = app.palette;
    // No frame: the cover (or the visualizer) fills the box itself.
    let inner = area;
    app.art.last_area.set(inner);
    if inner.width == 0 || inner.height == 0 {
        return inner;
    }
    let look = crate::art::Look::of(&app.config.ui, p);
    let mode = look.mode;
    if let Some(mut cover) = app.art.protocol(&song.cover_art_url, look) {
        // Centred in the box: the picture keeps its shape.
        let fit = cover.size_for(Resize::Fit(None), inner.as_size());
        let x = inner.x + inner.width.saturating_sub(fit.width) / 2;
        let y = inner.y + inner.height.saturating_sub(fit.height) / 2;
        let at = Rect::new(x, y, fit.width.min(inner.width), fit.height.min(inner.height));
        frame.render_stateful_widget(StatefulImage::default(), at, &mut *cover);
        drop(cover);
        app.art.check_encoding(&song.cover_art_url);
        app.art.last_cover.set(at);
        return at;
    }
    app.art.hide();
    let ic = icons(app.config.ui.icons);
    let caption = match (mode, app.art.status(&song.cover_art_url)) {
        (AlbumArt::Off, _) => String::new(),
        (_, Cover::Loading) if !song.cover_art_url.is_empty() => format!("{} loading cover…", ic.title),
        _ => format!("{} no cover", ic.title),
    };
    draw_visualizer(frame, &p, &app.art.bars, &caption, inner);
    inner
}

/// Spectrum bars from the bottom of `area` up, green → yellow → orange, with
/// a peak cap over each; `caption` dim on the top row.
fn draw_visualizer(frame: &mut Frame, p: &Palette, bars: &crate::visualizer::Bars, caption: &str, area: Rect) {
    const EIGHTHS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let top = u16::from(!caption.is_empty());
    let h = area.height.saturating_sub(top);
    let count = usize::from(area.width.div_ceil(2));
    if h == 0 || count == 0 {
        return;
    }
    let buf = frame.buffer_mut();
    let level = |i: usize, of: &[f32]| -> f32 {
        if of.is_empty() {
            return 0.0;
        }
        // Spread the bands over the bars that fit.
        of[(i * of.len() / count).min(of.len() - 1)]
    };
    for i in 0..count {
        let x = area.x + (i as u16) * 2;
        let eighths = (level(i, &bars.levels) * f32::from(h) * 8.0).round() as u16;
        let peak_row = (level(i, &bars.peaks) * f32::from(h)).ceil() as u16;
        for row in 0..h {
            let y = area.y + area.height - 1 - row;
            let fill = eighths.saturating_sub(row * 8).min(8);
            let frac = f32::from(row) / f32::from(h);
            let color = if frac < 0.5 {
                p.green
            } else if frac < 0.8 {
                p.yellow
            } else {
                p.orange
            };
            if fill > 0 {
                buf[(x, y)].set_symbol(EIGHTHS[usize::from(fill)]).set_fg(color);
            } else if peak_row > 0 && row + 1 == peak_row && peak_row * 8 > eighths {
                buf[(x, y)].set_symbol("▔").set_fg(p.orange);
            }
        }
    }
    if top == 1 {
        let line = Rect { height: 1, ..area };
        frame.render_widget(
            Paragraph::new(Span::styled(caption, Style::new().fg(p.muted).add_modifier(Modifier::DIM)))
                .alignment(Alignment::Center),
            line,
        );
    }
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
    let length = u64::from(song.length);
    let position = app.audio.position().as_secs();
    let position = if length > 0 { position.min(length) } else { position };
    let left = format!("{} ", clock(position));
    let right = format!(" {}", if length > 0 { clock(length) } else { "--:--".into() });
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
    // Keep one blank cell before the right border so the last column and
    // the highlight bar never touch it (matches History's breathing room).
    let inner = Rect { width: inner.width.saturating_sub(1), ..inner };
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
    let prompt_h = if app.prompt.is_some() { 4 } else { 0 };
    let [prompt_area, head, list_area, about] = Layout::vertical([
        Constraint::Length(prompt_h),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(inner);
    if let Some(prompt) = app.prompt {
        draw_prompt(frame, app, prompt, Rect { height: 3, ..prompt_area });
    }
    let label_w = Setting::ALL.iter().map(|(_, s)| s.label().width()).max().unwrap_or(0) + 3;
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("      {}VALUE", pad("SETTING", label_w)),
            Style::new().fg(p.muted).add_modifier(Modifier::BOLD),
        )),
        head,
    );
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
            (Setting::LogIn | Setting::Email | Setting::Password, _)
                if value == "not set" || value == "not logged in" =>
            {
                Style::new().fg(p.muted)
            }
            (Setting::EditFile, _) => Style::new().fg(p.cyan),
            _ => Style::new().fg(p.yellow).bold(),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("   {} ", pad(ic.setting[i], 2))),
            Span::styled(pad(setting.label(), label_w), Style::new().fg(p.fg)),
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
    let (label, shown) = match prompt {
        Prompt::Email => ("Pandora email", app.query.clone()),
        Prompt::Password => ("Password · Enter logs in", "•".repeat(app.query.chars().count())),
    };
    let block = framed()
        .border_style(Style::new().fg(p.cyan))
        .title(Span::styled(format!(" {label} "), Style::new().fg(p.cyan).bold()));
    frame.render_widget(Paragraph::new(shown.clone()).fg(p.fg).block(block), area);
    let x = area.x + 1 + shown.width() as u16;
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
            Span::styled(format!("   {} ", ic.logo), Style::new().fg(p.blue).bold()),
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
            // the new version is in, it is Restart Now.
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
                lines.push(Line::from(Span::styled("   (Tab to Select, Enter to Press)", Style::new().fg(p.muted))));
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
                lines.push(Line::from(Span::styled("   Update with:", Style::new().fg(p.fg))));
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
    if app.prompt.is_some() {
        return vec![("Enter", "Save"), ("Esc", "Cancel"), ("Ctrl+u", "Clear"), ("Ctrl+c", "Quit")];
    }
    if app.confirm_quit {
        return vec![("y", "Yes, Quit"), ("Other Key", "No")];
    }
    if app.help {
        return vec![("Esc", "Close"), ("?", "Close")];
    }
    let pause = if app.audio.is_paused() { "Play" } else { "Pause" };
    let mut keys = vec![("j/k ↑↓", "Move")];
    match (app.focus, app.view) {
        (Focus::Menu, _) if matches!(app.menu_selected(), MenuItem::Favorite(_)) => {
            keys.extend([("Enter", "Play"), ("f", "Unfavorite")])
        }
        (Focus::Menu, View::Player) => keys.extend([("Enter", "Open"), ("Tab", "Now Playing")]),
        (Focus::Content, View::Player) if app.pane == Pane::NowPlaying => {
            keys.extend([("Tab", "Up Next"), ("Esc", "Menu")])
        }
        (Focus::Menu, _) => keys.extend([("Enter", "Open"), ("Tab", "Page")]),
        (Focus::Content, View::Player) if app.pane == Pane::UpNext => {
            keys.extend([("Tab", "History"), ("Esc", "Menu")])
        }
        (Focus::Content, View::Player) => keys.extend([("Tab", "Menu"), ("Esc", "Menu")]),
        (Focus::Content, View::Settings) => keys.extend([("Enter", "Change"), ("Esc", "Menu")]),
        (Focus::Content, View::Stations) => keys.extend([("Enter", "Play"), ("f", "Favorite"), ("Esc", "Menu")]),
        (Focus::Content, View::About) => {
            if app.update_button_ready() {
                keys = vec![("Enter", if app.restart_ready() { "Restart Now" } else { "Update Now" })];
            } else {
                keys.clear();
            }
            keys.push(("Esc", "Menu"));
        }
    }
    if app.view != View::Player && app.has_now_playing() {
        keys.push(("N", "Now Playing"));
    }
    keys.extend([("space", pause), ("n", "Skip"), ("u/d", "Like/Dislike")]);
    // U / D rate the highlighted row: shown when Up Next or History has the keys.
    if app.focus == Focus::Content && app.view == View::Player && matches!(app.pane, Pane::UpNext | Pane::History) {
        keys.push(("U/D", "Rate Row"));
    }
    keys.extend([("⇧←→", "Volume"), ("{/}", "Menu Size"), ("?", "Help"), ("Q", "Quit")]);
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
    let split = keys.len().saturating_sub(2);
    let (rest, last) = keys.split_at(split);
    let last: Vec<Span> = last.iter().flat_map(cap).collect();
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

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    let sections: [(&str, &[(&str, &str)]); 3] = [
        (
            "PLAYING",
            &[
                ("space p", "play / pause"),
                ("n", "skip to the next song"),
                ("+ u =", "like the playing song (thumbs up)"),
                ("- d", "dislike the playing song (skips it)"),
                ("U D", "like / dislike the Up Next or History row"),
                ("z", "tired of this song (a month off)"),
                ("Shift ← →", "volume down / up"),
            ],
        ),
        (
            "MOVING AROUND",
            &[
                ("j k ↑ ↓", "move in the list · g G first / last"),
                ("Enter", "play the station · open Stations / Settings / About"),
                ("N", "Now Playing pane gets the keys (menu on NOW PLAYING)"),
                ("s S", "Stations: every station, Enter plays one"),
                ("T A", "Settings / About"),
                ("f", "favorite / unfavorite: the menu's FAVORITES"),
                ("F", "to FAVORITES in the menu"),
                ("Tab", "menu → Now Playing → Up Next → History"),
                ("Esc", "back to the menu"),
            ],
        ),
        (
            "BOXVIBE",
            &[
                ("t", "next theme"),
                ("{ }", "menu narrower / wider (saved)"),
                ("i", "emoji / Nerd Font / Unicode icons"),
                ("?", "this help"),
                ("Q Ctrl+c", "quit (asks first) / quit now"),
            ],
        ),
    ];
    // Every key pill is padded to the widest one, so the descriptions line up.
    let key_w = sections.iter().flat_map(|(_, keys)| keys.iter()).map(|(k, _)| k.width()).max().unwrap_or(0);
    let mut lines = Vec::new();
    for (name, keys) in sections {
        lines.push(Line::from(Span::styled(format!(" {name}"), Style::new().fg(p.orange).bold())));
        for (key, what) in keys {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!(" {key} "), Style::new().fg(p.background).bg(p.fg).bold()),
                Span::raw(" ".repeat(key_w - key.width() + 2)),
                Span::styled((*what).to_owned(), Style::new().fg(p.fg)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(HELP_FOOTER, Style::new().fg(p.muted))));
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

const HELP_FOOTER: &str = " Settings and account, album art, retro mode and tint: the Settings entry in the menu";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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
        app.demo_warm_up(3, std::time::Duration::ZERO);
        app
    }

    /// The Now Playing art box as (top, bottom, left, right), each one
    /// cell outside the area the art may use (the old frame's position).
    fn art_box(app: &App) -> (usize, usize, usize, usize) {
        let a = app.art.last_area.get();
        assert!(a.width > 0 && a.height > 0, "art box drawn");
        (a.y as usize - 1, (a.y + a.height) as usize, a.x as usize - 1, (a.x + a.width) as usize)
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
    fn help_fits_its_longest_line_and_its_keys_line_up() {
        let mut app = demo();
        app.help = true;
        for (cols, rows) in [(255u16, 70u16), (160, 40)] {
            let (lines, _) = screen(&app, cols, rows);
            let all = lines.join("\n");
            for whole in [
                "play the station · open Stations / Settings / About",
                "Now Playing pane gets the keys (menu on NOW PLAYING)",
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
            assert_eq!(a, start("like the playing song"), "{cols}x{rows}\n{all}");
            assert_eq!(a, start("skip to the next song"), "{cols}x{rows}\n{all}");
        }
    }

    #[test]
    fn the_key_bar_leaves_out_theme_and_icons_but_the_keys_still_work() {
        let mut app = demo();
        let caps = key_caps(&app);
        assert!(!caps.iter().any(|(k, _)| *k == "t" || *k == "i"), "{caps:?}");
        let theme = app.config.ui.theme.clone();
        app.on_key(crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('t')));
        assert_ne!(app.config.ui.theme, theme);
    }

    #[test]
    fn rate_row_is_only_hinted_when_up_next_or_history_has_the_keys() {
        let mut app = demo();
        app.view = View::Player;
        let hinted = |app: &App| key_caps(app).iter().any(|(k, _)| *k == "U/D");
        app.focus = crate::app::Focus::Menu;
        assert!(!hinted(&app), "menu");
        app.focus = crate::app::Focus::Content;
        app.pane = Pane::NowPlaying;
        assert!(!hinted(&app), "Now Playing");
        for pane in [Pane::UpNext, Pane::History] {
            app.pane = pane;
            assert!(hinted(&app), "{pane:?}");
        }
        app.view = View::Settings;
        assert!(!hinted(&app), "Settings");
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

    #[test]
    fn with_album_art_off_the_box_shows_the_visualizer() {
        let mut app = demo();
        app.config.ui.album_art = crate::config::AlbumArt::Off;
        app.art.bars.update(&[0.9; 48]);
        let (lines, _) = screen(&app, 120, 36);
        let (top, bottom, left, right) = art_box(&app);
        let inside: String = lines[top + 1..bottom]
            .iter()
            .map(|l| l.chars().skip(left + 1).take(right - left - 1).collect::<String>())
            .collect();
        assert!(inside.contains('█'), "bars:\n{}", lines.join("\n"));
        assert!(!inside.contains('▀'), "no picture when Off");
    }

    #[test]
    fn without_a_cover_the_visualizer_says_so() {
        let mut app = demo();
        if let Some(song) = app.now.as_mut() {
            song.cover_art_url.clear();
        }
        let (lines, _) = screen(&app, 120, 36);
        assert!(lines.join("\n").contains("no cover"), "{}", lines.join("\n"));
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
        assert!(top.starts_with("┌ ▣ boxvibe ─"), "{top}");
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
        assert!(lines[35].contains(" N  Now Playing"), "key bar: {}", lines[35]);
    }

    #[test]
    fn song_panes_fit_ten_rows_and_now_playing_takes_the_rest() {
        let mut app = demo();
        // Side by side: Up Next and History are SONG_PANE (13) tall, Now Playing the rest.
        for (rows, want) in [(36, 21), (50, 35), (20, 9)] {
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
        assert!(bar.trim_end().ends_with("toby@example.com"), "{bar}");
        assert!(bar.ends_with("toby@example.com "), "one space before the edge: {bar:?}");
        let x = (bar.chars().count() - "toby@example.com ".len()) as u16;
        assert_eq!(buffer[(x, 34)].fg, app.palette.blue, "tubevibe's account colour");
        // Narrow, with a long status: both cut, never overlapping.
        app.status = "a very long status message that goes on and on and on".into();
        let (lines, _) = screen(&app, 40, 36);
        let bar = &lines[34];
        assert!(bar.contains('…') && bar.contains('@'), "{bar}");
        assert!(bar.chars().count() == 40);
        let status_end = bar.find("on").unwrap();
        assert!(status_end < bar.find('@').unwrap());
        // Logged out: nothing on the right.
        app.session = Session::LoggedOut;
        let (lines, _) = screen(&app, 120, 36);
        assert!(!lines[34].contains("toby@"), "{}", lines[34]);
    }

    #[test]
    fn up_next_shows_each_songs_thumbs_and_updates_after_shift_u() {
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
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('U'),
            crossterm::event::KeyModifiers::SHIFT,
        ));
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
        assert_eq!((art.y, art.y + art.height), (1, rows as u16 - 1), "art fills the pane's height");
        assert!(!lines[..rows].iter().any(|l| l.contains('╰') && menu(l).chars().nth(2) == Some('╰')), "no frame");
        let title = app.now.as_ref().unwrap().title.clone();
        let time = lines.iter().position(|l| l.contains('●')).expect("time line");
        let volume = lines.iter().position(|l| l.contains('%') && l.contains('▮')).expect("volume");
        assert!(time < rows && volume == time + 1, "both inside Now Playing, beside the art");
        assert!(lines[time].chars().position(|c| c == '●').unwrap() > usize::from(art.x + art.width), "beside the art");
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
}
#[cfg(test)]
mod scratch {
    use super::*;
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    #[test]
    fn dump() {
        let mut app = App::new(Config::default(), None, &[], true);
        app.demo_warm_up(3, std::time::Duration::ZERO);
        app.help = true;
        for (c, r) in [(255u16, 70u16), (160, 40), (80, 30)] {
            let mut t = Terminal::new(TestBackend::new(c, r)).unwrap();
            t.draw(|f| draw(f, &app)).unwrap();
            let b = t.backend().buffer().clone();
            println!("=== {c}x{r}");
            for y in 0..r {
                let s: String = (0..c).map(|x| b[(x, y)].symbol().to_string()).collect();
                println!("{s}");
            }
        }
    }
}
