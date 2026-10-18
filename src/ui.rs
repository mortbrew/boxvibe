//! Drawing. A header line (name, version, what is going on), the menu on
//! the left (stations, then Settings, About, Quit, like tubevibe), the
//! chosen page on the right, and the keys as boxed key caps at the bottom.
//! Panels have titled borders, tables have column headers, and the selected
//! row is a reverse-video bar.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row as TableRow, Table, TableState, Wrap,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, MenuItem, Prompt, Row, Session, Setting, View};
use crate::icons::{Icons, icons};
use crate::pandora::{Rating, Song};
use crate::theme::Palette;
use crate::update::Progress;

pub fn draw(frame: &mut Frame, app: &App) {
    let p = app.palette;
    let area = frame.area();
    // Built-in themes paint their background; Omarchy's is the terminal's own.
    let base = if app.follows_omarchy() { Style::new().fg(p.fg) } else { Style::new().fg(p.fg).bg(p.background) };
    frame.render_widget(Block::new().style(base), area);
    let ic = icons(app.config.ui.icons);
    let [header, body, keys] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(area);
    draw_header(frame, app, header);
    let side = (body.width / 4).clamp(24, 32);
    let [left, right] = Layout::horizontal([Constraint::Length(side), Constraint::Fill(1)]).spacing(1).areas(body);
    draw_menu(frame, app, ic, left);
    match app.view {
        View::Player => draw_player(frame, app, ic, right),
        View::Settings => draw_settings(frame, app, ic, right),
        View::About => draw_about(frame, app, ic, right),
    }
    draw_keys(frame, &p, &key_caps(app), keys);
    if app.help {
        draw_help(frame, app, ic, body);
    }
}

// --- Pieces ---

/// A panel with its title on the top border; `focused` brightens the border.
fn panel<'a>(title: impl Into<Line<'a>>, p: &Palette, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(if focused { p.fg } else { p.line }))
        .title(title)
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

// --- Header ---

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let p = app.palette;
    let status_color = if app.status.starts_with('⚠') { p.yellow } else { p.fg };
    let left = Line::from(vec![
        Span::styled(" box", Style::new().fg(p.fg).bold()),
        Span::styled("vibe", Style::new().fg(p.orange).bold()),
        Span::styled(format!(" {}  ", env!("CARGO_PKG_VERSION")), Style::new().fg(p.muted)),
        Span::styled(app.status.clone(), Style::new().fg(status_color)),
    ]);
    let right = match &app.session {
        Session::LoggedIn if app.demo => Span::styled("demo ", Style::new().fg(p.muted)),
        Session::LoggedIn => Span::styled(format!("{} ", app.config.pandora.username), Style::new().fg(p.muted)),
        Session::SigningIn => Span::styled("signing in… ", Style::new().fg(p.yellow)),
        _ => Span::styled("not logged in ", Style::new().fg(p.muted)),
    };
    let right_w = right.width() as u16;
    let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_w)]).areas(area);
    frame.render_widget(Paragraph::new(left), l);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), r);
}

// --- Menu ---

fn draw_menu(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Menu;
    let count = if app.stations.is_empty() { String::new() } else { format!(" ({})", app.stations.len()) };
    let block = panel(title(format!("Stations{count}"), &p), &p, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [list_area, _, rule, app_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(3)])
            .areas(inner);
    let selected = app.menu_selected();
    // Without the keys, the entry of the page shown stays shaded.
    let shown = |item: MenuItem| match (item, app.view) {
        (MenuItem::Settings, View::Settings) | (MenuItem::About, View::About) => true,
        (MenuItem::Station(i), View::Player) => {
            app.stations.get(i).is_some_and(|s| app.current_station.as_deref() == Some(s.id.as_str()))
        }
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
    if app.stations.is_empty() {
        let text = match &app.session {
            Session::SigningIn => format!(" {} Signing in…", ic.loading),
            Session::LoggedIn => format!(" {} Loading stations…", ic.loading),
            _ => " Log in from Settings".into(),
        };
        frame.render_widget(Paragraph::new(text).fg(p.muted), list_area);
    } else {
        let items: Vec<ListItem> = app
            .stations
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let playing = app.current_station.as_deref() == Some(s.id.as_str());
                let icon = if s.is_shuffle {
                    ic.shuffle
                } else if s.is_shared {
                    ic.shared
                } else {
                    ic.station
                };
                let mut style = if playing { Style::new().fg(p.green).bold() } else { Style::new().fg(p.fg) };
                if let Some(sel) = row_style(MenuItem::Station(i)) {
                    style = style.patch(sel);
                }
                let mark = if playing { ic.play } else { " " };
                ListItem::new(Line::from(format!(" {} {} {}", pad(mark, 1), pad(icon, 2), s.name))).style(style)
            })
            .collect();
        let at = match selected {
            MenuItem::Station(i) => Some(i),
            _ => app.stations.iter().position(|s| app.current_station.as_deref() == Some(s.id.as_str())),
        };
        let mut state = ListState::default().with_selected(at);
        frame.render_stateful_widget(List::new(items), list_area, &mut state);
    }
    frame.render_widget(Paragraph::new("─".repeat(rule.width as usize)).fg(p.line), rule);
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
        let mut text = format!("   {} {label}", pad(icon, 2));
        if item == MenuItem::About && app.update.is_some() {
            text.push_str(&format!("  {} new", ic.update));
        }
        lines.push(Line::from(pad(&text, app_area.width as usize)).style(style));
    }
    frame.render_widget(Paragraph::new(lines), app_area);
}

// --- Player ---

fn draw_player(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let card_h = if area.height >= 30 { 17 } else { area.height.saturating_sub(9).clamp(8, 17) };
    let [card, songs] = Layout::vertical([Constraint::Length(card_h), Constraint::Fill(1)]).areas(area);
    draw_now_playing(frame, app, ic, card);
    draw_songs(frame, app, ic, songs);
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
            (Session::LoggedIn, None) => "Pick a station on the left and press Enter".into(),
            (Session::SigningIn, _) => format!("{} Signing in…", ic.loading),
            _ => "Not logged in: open Settings to log in".into(),
        };
        frame.render_widget(
            Paragraph::new(text).fg(p.muted).alignment(Alignment::Center),
            centered(inner, inner.width, 1),
        );
        return;
    };
    let [card, _, progress, volume] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
            .areas(inner);
    // The art square: two cells wide per row, so it looks square.
    let art_h = card.height.min(12);
    let art_w = (art_h * 2 + 2).min(card.width / 3);
    let [art, _, info] =
        Layout::horizontal([Constraint::Length(art_w), Constraint::Length(3), Constraint::Fill(1)]).areas(card);
    draw_art(frame, &p, ic, song, Rect { height: art_h, ..art });
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
    let buttons_h = if card.height >= 11 { 4 } else { 0 };
    let [text, _, buttons] =
        Layout::vertical([Constraint::Length(6), Constraint::Fill(1), Constraint::Length(buttons_h)]).areas(info);
    frame.render_widget(Paragraph::new(lines), text);
    if buttons_h > 0 {
        draw_buttons(frame, app, ic, song, buttons);
    }
    draw_progress(frame, app, song, progress);
    draw_volume(frame, app, ic, volume);
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

/// The four CarPlay buttons, each with its keys under it.
fn draw_buttons(frame: &mut Frame, app: &App, ic: &Icons, song: &Song, area: Rect) {
    let p = app.palette;
    let play = if app.audio.is_paused() || !app.audio.is_active() { ic.play } else { ic.pause };
    let buttons = [
        (ic.down, "-/d", if song.rating == Rating::Down { p.red } else { p.line }),
        (play, "space", p.fg),
        (ic.skip, "n", p.line),
        (ic.up, "+/u", if song.rating == Rating::Up { p.green } else { p.line }),
    ];
    let w: u16 = 9;
    let cells = Layout::horizontal([Constraint::Length(w); 4]).spacing(2).split(area);
    for ((icon, key, color), cell) in buttons.iter().zip(cells.iter()) {
        let [b, label] = Layout::vertical([Constraint::Length(3), Constraint::Length(1)]).areas(*cell);
        let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(*color));
        frame.render_widget(
            Paragraph::new(Span::styled(*icon, Style::new().fg(*color).bold()))
                .alignment(Alignment::Center)
                .block(block),
            b,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(*key, Style::new().fg(p.muted))).alignment(Alignment::Center),
            label,
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

fn draw_songs(frame: &mut Frame, app: &App, ic: &Icons, area: Rect) {
    let p = app.palette;
    let focused = app.focus == Focus::Content;
    let rows = app.song_rows();
    let played = rows.iter().filter(|r| matches!(r, Row::Played(_))).count();
    let next = rows.iter().filter(|r| matches!(r, Row::Next(_))).count();
    let block = panel(title(format!("Songs: {played} played, {next} up next"), &p), &p, focused);
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(" Songs you play show here").fg(p.muted).block(block), area);
        return;
    }
    let header = TableRow::new(["WHEN", "TITLE", "ARTIST", "ALBUM", "TIME", "RATED"])
        .style(Style::new().fg(p.muted).add_modifier(Modifier::BOLD));
    let widths = [
        Constraint::Length(7),
        Constraint::Fill(3),
        Constraint::Fill(2),
        Constraint::Fill(3),
        Constraint::Length(5),
        Constraint::Length(11),
    ];
    // The columns' real widths, to end cut text with "…".
    let inner = block.inner(area);
    let cols = Layout::horizontal(widths).spacing(2).split(inner);
    let fit = |text: &str, col: usize| truncate(text, cols[col].width as usize);
    let selected = app.song_index();
    let body: Vec<TableRow> = rows
        .iter()
        .map(|row| {
            let song = app.row_song(*row).expect("a row has its song");
            let (when, style) = match row {
                Row::Played(n) => (format!("{n} ago"), Style::new().fg(p.fg)),
                Row::Now => (format!("{} now", ic.play), Style::new().fg(p.green).bold()),
                Row::Next(n) => (format!("next {n}"), Style::new().fg(p.muted)),
            };
            let artist_style = match row {
                Row::Played(_) => Style::new().fg(p.cyan),
                _ => style,
            };
            let rated = match (row, song.rating) {
                (Row::Next(_), _) => Span::raw(""),
                (_, Rating::Up) => Span::styled(format!("{} liked", ic.up), Style::new().fg(p.green)),
                (_, Rating::Down) => Span::styled(format!("{} disliked", ic.down), Style::new().fg(p.red)),
                (_, Rating::None) => Span::styled("—", Style::new().fg(p.muted)),
            };
            let time = if song.length > 0 { clock(u64::from(song.length)) } else { String::new() };
            TableRow::new(vec![
                Cell::from(when),
                Cell::from(fit(&song.title, 1)),
                Cell::from(Span::styled(fit(&song.artist, 2), artist_style)),
                Cell::from(fit(&song.album, 3)),
                Cell::from(Line::from(time).alignment(Alignment::Right)),
                Cell::from(rated),
            ])
            .style(style)
        })
        .collect();
    let table = Table::new(body, widths)
        .header(header)
        .column_spacing(2)
        .block(block)
        .row_highlight_style(if focused { selection(&p, true) } else { Style::new() });
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, area, &mut state);
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
    let block = Block::bordered()
        .border_type(BorderType::Plain)
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
            Span::styled(format!("   {} ", ic.logo), Style::new().fg(p.orange)),
            Span::styled("box", Style::new().fg(p.fg).bold()),
            Span::styled("vibe", Style::new().fg(p.orange).bold()),
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
            let ready = app.update_button_ready();
            let focused = app.focus == Focus::Content;
            let style = match (ready, focused) {
                (true, true) => Style::new().fg(p.background).bg(p.green).bold(),
                (true, false) => Style::new().fg(p.green).bg(p.highlight).bold(),
                (false, _) => Style::new().fg(p.muted).bg(p.highlight),
            };
            let mut button = vec![Span::raw("   "), Span::styled(format!("  {} Update Now  ", ic.update), style)];
            if ready {
                let how = if focused { "  Enter installs it" } else { "  Tab, then Enter" };
                button.push(Span::styled(how, Style::new().fg(p.muted)));
            }
            lines.push(Line::from(button));
            match &app.self_update {
                Progress::Downloading(percent) => lines.push(Line::from(Span::styled(
                    format!("   {} Downloading… {percent}%", ic.loading),
                    Style::new().fg(p.yellow).bold(),
                ))),
                Progress::Done(version) => lines.push(Line::from(Span::styled(
                    format!("   {} boxvibe {version} installed · restart boxvibe to use it", ic.ok),
                    Style::new().fg(p.green).bold(),
                ))),
                Progress::Failed(error) => lines.push(Line::from(Span::styled(
                    format!("   {} Could not update: {error}", ic.warn),
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
        (Focus::Menu, _) => keys.extend([("Enter", "Open"), ("Tab", "Songs")]),
        (Focus::Content, View::Player) => keys.extend([("Tab/Esc", "Stations")]),
        (Focus::Content, View::Settings) => keys.extend([("Enter", "Change"), ("Esc", "Menu")]),
        (Focus::Content, View::About) => {
            if app.update_button_ready() {
                keys = vec![("Enter", "Update Now")];
            } else {
                keys.clear();
            }
            keys.push(("Esc", "Menu"));
        }
    }
    keys.extend([
        ("space", pause),
        ("n", "Skip"),
        ("+/u", "Like"),
        ("-/d", "Dislike"),
        ("←→", "Volume"),
        ("t", "Theme"),
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
                ("← → [ ]", "volume"),
            ],
        ),
        (
            "MOVING AROUND",
            &[
                ("j k ↑ ↓", "move in the list · g G first / last"),
                ("Enter", "play the station · open Settings / About"),
                ("s", "back to the stations"),
                ("Tab", "menu ⇄ page (the songs table: rate past songs)"),
                ("Esc", "back to the menu"),
            ],
        ),
        (
            "BOXVIBE",
            &[
                ("t", "next theme"),
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
    let block = panel(title(format!("{} Keys", ic.logo), &p), &p, true).style(Style::new().bg(p.background));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}
