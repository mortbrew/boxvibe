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
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, MenuItem, Pane, Prompt, Session, Setting, View};
use crate::config::BorderStyle;
use crate::icons::{Icons, icons};
use crate::pandora::{Rating, Song};
use crate::theme::Palette;
use crate::update::Progress;

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
        draw_help(frame, app, ic, body);
    }
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

fn clock(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// `text` padded with spaces to `cells` terminal cells (emoji take two).
fn pad(text: &str, cells: usize) -> String {
    let w = text.width();
    format!("{text}{}", " ".repeat(cells.saturating_sub(w)))
}

/// Song text made safe for a table cell: tabs and newlines become spaces,
/// and emoji variation selectors and joiners go, since terminals draw those
/// wider than they measure and push the rest of the row (the rating) out.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{FE0E}' | '\u{FE0F}' | '\u{200D}'))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// `text` cut to `cells` terminal cells, ending in "…" when cut.
fn truncate(text: &str, cells: usize) -> String {
    if text.width() <= cells {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) + 1 > cells {
            break;
        }
        out.push(c);
    }
    out.push('…');
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

/// What is going on, on a shaded bar above the keys (where tubevibe has it).
fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    let color = if app.status.starts_with('⚠') { p.yellow } else { p.fg };
    frame.render_widget(Block::new().bg(p.highlight), area);
    frame.render_widget(Paragraph::new(Span::styled(format!(" {}", app.status), Style::new().fg(color))), area);
}

/// The logo on the menu's top border, like tubevibe's: the box icon and
/// "box" in the theme's blue, "vibe" in the text colour.
pub fn logo_title<'a>(p: &Palette, ic: &Icons) -> Line<'a> {
    logo_line(p, ic, "")
}

/// The one place the logo is drawn: the menu's title and the Help window's
/// title share it, with `rest` (bold, text colour) after "boxvibe".
fn logo_line<'a>(p: &Palette, ic: &Icons, rest: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {} ", ic.logo), Style::new().fg(p.blue).bold()),
        Span::styled("box", Style::new().fg(p.blue).bold()),
        Span::styled(format!("vibe{rest} "), Style::new().fg(p.fg).bold()),
    ])
}

/// The Help window's title: "<icon> boxvibe Help · Any Key Closes".
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
            let line = format!(" {} {} {} {}", pad(mark, 1), pad(icon, 2), pad(star, 2), s.name);
            ListItem::new(Line::from(pad(&line, inner.width as usize))).style(style)
        })
        .collect();
    let list = List::new(items).highlight_style(selection(&p, focused));
    let mut state = ListState::default().with_selected(Some(app.stations_index()));
    frame.render_stateful_widget(list, inner, &mut state);
}

// --- Player ---

/// Now Playing's rows: its setting, leaving Up Next and History room on a short screen.
pub fn now_playing_height(app: &App, height: u16) -> u16 {
    app.config.ui.now_playing_size.min(height.saturating_sub(9).max(8))
}

fn draw_player(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let card_h = now_playing_height(app, area.height);
    let [card, songs] = Layout::vertical([Constraint::Length(card_h), Constraint::Fill(1)]).areas(area);
    draw_now_playing(frame, app, ic, card);
    // Up Next bottom-left, History bottom-right, side by side; stacked
    // (Up Next on top) when the page is too narrow for two readable tables.
    let [next, history] = if stack_songs(songs.width) {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(songs)
    } else {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).spacing(1).areas(songs)
    };
    draw_up_next(frame, app, next);
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
    let block = panel(title(format!("Now Playing · {station}"), &p), &p, false);
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
    draw_art(frame, &p, ic, song, art);
    let label = |text: &str| Span::styled(format!("{text:<9}"), Style::new().fg(p.muted));
    let state = if app.buffering.is_some() {
        Span::styled(format!("{} buffering…", ic.loading), Style::new().fg(p.yellow))
    } else if app.audio.is_paused() {
        Span::styled(format!("{} paused", ic.pause), Style::new().fg(p.yellow).bold())
    } else {
        Span::styled(format!("{} playing", ic.play), Style::new().fg(p.green))
    };
    let lines = vec![
        Line::from(Span::styled(song.title.clone(), Style::new().fg(p.fg).bold())),
        Line::raw(""),
        Line::from(vec![label("artist"), Span::styled(song.artist.clone(), Style::new().fg(p.cyan).bold())]),
        Line::from(vec![label("album"), Span::styled(song.album.clone(), Style::new().fg(p.fg))]),
        Line::from(vec![label("rating"), rating_word(song.rating, &p, ic)]),
        Line::from(vec![label("state"), state]),
    ];
    let buttons_h = if info.height >= 13 { 3 } else { 0 };
    let [_, text, _, progress, volume, _, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(6),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(buttons_h),
    ])
    .areas(info);
    frame.render_widget(Paragraph::new(lines), text);
    draw_progress(frame, app, song, progress);
    draw_volume(frame, app, ic, volume);
    if buttons_h > 0 {
        draw_buttons(frame, app, ic, song, buttons);
    }
}

fn draw_art(frame: &mut Frame, p: &Palette, ic: &Icons, song: &Song, area: Rect) {
    // No cover art in v1 (song.cover_art_url is kept for later): a card in the theme's colours.
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(p.line));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Block::new().bg(p.highlight), inner);
    if inner.height >= 3 {
        let mid = inner.y + inner.height / 2;
        let line = |y: u16| Rect { y, height: 1, ..inner };
        frame.render_widget(
            Paragraph::new(Span::styled(ic.title, Style::new().fg(p.orange).bold())).alignment(Alignment::Center),
            line(mid.saturating_sub(1)),
        );
        let initials: String = song.album.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect();
        frame.render_widget(
            Paragraph::new(Span::styled(initials.to_uppercase(), Style::new().fg(p.muted).bold()))
                .alignment(Alignment::Center),
            line(mid + 1),
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
/// selected row as a bar while the pane has the keys.
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
    let cols = Layout::horizontal(widths.to_vec()).spacing(2).split(inner);
    let body: Vec<TableRow> = rows
        .into_iter()
        .map(|cells| {
            TableRow::new(cells.into_iter().enumerate().map(|(i, (text, style))| {
                let text = truncate(&clean(&text), cols[i].width as usize);
                Cell::from(Span::styled(text, style))
            }))
        })
        .collect();
    let table = Table::new(body, widths.to_vec())
        .header(TableRow::new(header.to_vec()).style(Style::new().fg(p.muted).add_modifier(Modifier::BOLD)))
        .column_spacing(2)
        .block(block)
        .row_highlight_style(if focused { selection(&p, true) } else { Style::new() });
    let mut state = TableState::default().with_selected(focused.then_some(selected));
    frame.render_stateful_widget(table, area, &mut state);
}

fn draw_up_next(frame: &mut Frame, app: &App, area: Rect) {
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
            ]
        })
        .collect();
    let widths = [Constraint::Length(2), Constraint::Fill(3), Constraint::Fill(2), Constraint::Length(5)];
    song_table(frame, app, block, &["#", "TITLE", "ARTIST", " TIME"], &widths, rows, app.next_index(), focused, area);
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
            let rated = match song.rating {
                Rating::Up => (format!("{} liked", ic.up), Style::new().fg(p.green)),
                Rating::Down => (format!("{} disliked", ic.down), Style::new().fg(p.red)),
                Rating::None => ("—".to_owned(), Style::new().fg(p.muted)),
            };
            vec![(song.title.clone(), Style::new().fg(p.fg)), (song.artist.clone(), Style::new().fg(p.cyan)), rated]
        })
        .collect();
    let widths = [Constraint::Fill(3), Constraint::Fill(2), Constraint::Length(11)];
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
    if app.help {
        return vec![("Esc", "Close"), ("?", "Close")];
    }
    let pause = if app.audio.is_paused() { "Play" } else { "Pause" };
    let mut keys = vec![("j/k ↑↓", "Move")];
    match (app.focus, app.view) {
        (Focus::Menu, _) if matches!(app.menu_selected(), MenuItem::Favorite(_)) => {
            keys.extend([("Enter", "Play"), ("f", "Unfavorite")])
        }
        (Focus::Menu, View::Player) => keys.extend([("Enter", "Open"), ("Tab", "Up Next")]),
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
    keys.extend([
        ("space", pause),
        ("n", "Skip"),
        ("+/u", "Like"),
        ("-/d", "Dislike"),
        ("⇧←→", "Volume"),
        ("t", "Theme"),
        ("{/}", "Menu Size"),
        ("i", "Icons"),
        ("?", "Help"),
        ("q", "Quit"),
    ]);
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

fn draw_help(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let sections: [(&str, &[(&str, &str)]); 3] = [
        (
            "PLAYING",
            &[
                ("space p", "play / pause"),
                ("n", "skip to the next song"),
                ("+ u =", "like (thumbs up)"),
                ("- d", "dislike (thumbs down, skips the song)"),
                ("z", "tired of this song (a month off)"),
                ("Shift ← →", "volume down / up"),
            ],
        ),
        (
            "MOVING AROUND",
            &[
                ("j k ↑ ↓", "move in the list · g G first / last"),
                ("Enter", "play the station · open Stations / Settings / About"),
                ("N", "Now Playing (also the menu's top entry)"),
                ("s", "Stations: every station, Enter plays one"),
                ("f F", "favorite / unfavorite: the menu's FAVORITES"),
                ("Tab", "menu → Up Next → History (rate past songs there)"),
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
                ("q Ctrl+c", "quit"),
            ],
        ),
    ];
    let mut lines = Vec::new();
    for (name, keys) in sections {
        lines.push(Line::from(Span::styled(format!(" {name}"), Style::new().fg(p.orange).bold())));
        for (key, what) in keys {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!(" {key} "), Style::new().fg(p.background).bg(p.fg).bold()),
                Span::raw(" ".repeat(10usize.saturating_sub(key.width() + 2) + 1)),
                Span::styled((*what).to_owned(), Style::new().fg(p.fg)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        " Settings and account: the Settings entry in the menu",
        Style::new().fg(p.muted),
    )));
    let box_area = centered(area, 64, lines.len() as u16 + 2);
    frame.render_widget(Clear, box_area);
    let block = panel(logo_line(&p, ic, HELP_TITLE_REST), &p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

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
    fn help_title_uses_the_menus_logo() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        app.help = true;
        let (lines, buffer) = screen(&app, 120, 40);
        let (y, row) = lines.iter().enumerate().find(|(_, l)| l.contains("Help · Any Key Closes")).unwrap();
        assert!(row.contains("▣ boxvibe Help · Any Key Closes"), "{row}");
        let (p, y) = (app.palette, y as u16);
        let x = row.chars().position(|c| c == '▣').unwrap() as u16;
        assert_eq!(buffer[(x, y)].fg, p.blue);
        assert_eq!((buffer[(x + 2, y)].fg, buffer[(x + 5, y)].fg), (p.blue, p.fg));
        assert!(buffer[(x + 5, y)].modifier.contains(ratatui::style::Modifier::BOLD));
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
    fn now_playing_size_sets_the_pane_height() {
        let mut app = demo();
        for (size, rows, want) in [(17, 36, 17), (9, 36, 9), (25, 36, 25), (25, 20, 9)] {
            app.config.ui.now_playing_size = size;
            assert_eq!(now_playing_height(&app, rows - 2), want, "{size} on {rows} rows");
            let (lines, _) = screen(&app, 120, rows);
            let up = lines.iter().position(|l| l.contains("Up Next (")).expect("Up Next");
            assert_eq!(up, usize::from(want), "Up Next right under Now Playing");
        }
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
    fn clean_and_truncate_keep_cells_honest() {
        assert_eq!(clean("a\tb\nc ❤️ d"), "a b c ❤ d");
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("名前名前", 5), "名前…");
        assert!(truncate("名前名前", 5).width() <= 5);
    }

    #[test]
    fn now_playing_art_fills_the_pane_with_info_time_and_volume_beside_it() {
        let mut app = demo();
        app.config.ui.icons = crate::config::IconSet::Unicode;
        let (lines, _) = screen(&app, 120, 36);
        let rows = usize::from(now_playing_height(&app, 34));
        let menu = |l: &String| l.chars().skip(30).collect::<String>();
        let art_col = |l: &String| menu(l).chars().nth(2).unwrap_or(' ');
        assert_eq!(art_col(&lines[1]), '╭', "art starts on the pane's first row: {}", lines[1]);
        assert_eq!(art_col(&lines[rows - 2]), '╰', "and ends on its last: {}", lines[rows - 2]);
        let title = app.now.as_ref().unwrap().title.clone();
        assert!(menu(&lines[2]).contains(&title), "song info one line down: {}", lines[2]);
        let time = lines.iter().position(|l| l.contains('●')).expect("time line");
        let volume = lines.iter().position(|l| l.contains('%') && l.contains('▮')).expect("volume");
        assert!(time < rows && volume == time + 1, "both inside Now Playing, beside the art");
        assert!(menu(&lines[time]).chars().nth(2) == Some('│'), "beside the art: {}", lines[time]);
        for key in ["-/d", "+/u", "space"] {
            assert!(!lines[..rows].iter().any(|l| l.contains(key)), "no {key} under the buttons");
        }
    }
}
