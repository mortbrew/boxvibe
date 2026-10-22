//! `--snapshot FILE`: draws one demo frame with ratatui's TestBackend and
//! writes it as HTML (each cell coloured as in a terminal), which a headless
//! browser turns into a PNG (see `tools/screenshots.sh`).

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, View};
use crate::config::Config;
use crate::theme::Palette;
use crate::ui;

pub fn write(config: Config, screen: &str, cols: u16, rows: u16, out: &Path) -> anyhow::Result<()> {
    // Demo mode never writes the config; the path is only shown in Settings and About.
    let mut app = App::new(config, Config::default_path(), &[], true);
    app.demo_warm_up(3, Duration::from_secs(102));
    match screen {
        "help" => app.help = true,
        "settings" => {
            app.view = View::Settings;
            app.focus = Focus::Content;
            // On the look settings, to show the border and notification options.
            app.settings_cursor =
                crate::app::Setting::ALL.iter().position(|(_, s)| *s == crate::app::Setting::BorderStyle).unwrap_or(0);
            app.status = "Settings are saved to config.toml as you change them".into();
        }
        "about" | "about-restart" => {
            app.view = View::About;
            app.focus = Focus::Content;
            // What About shows once GitHub has a newer release (made up here).
            app.update = Some(crate::update::Update {
                version: "0.2.0".into(),
                how: String::new(),
                replace: crate::update::Replace::Ready(String::new()),
            });
            if screen == "about-restart" {
                // Update Now finished: the button is Restart Now.
                app.self_update = crate::update::Progress::Done("0.2.0".into());
                app.status = "✔ boxvibe 0.2.0 installed · Restart Now in About opens it".into();
            }
            if screen == "about" {
                app.status =
                    format!("{} boxvibe 0.2.0 available · see About", crate::icons::icons(app.config.ui.icons).update);
            }
        }
        _ => {}
    }
    let mut terminal = Terminal::new(TestBackend::new(cols, rows))?;
    terminal.draw(|frame| ui::draw(frame, &app))?;
    let html = to_html(terminal.backend().buffer(), &app.palette);
    std::fs::write(out, html)?;
    Ok(())
}

fn css(color: Color, fallback: Color) -> String {
    match if color == Color::Reset { fallback } else { color } {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000".into(),
        Color::White => "#fff".into(),
        _ => match fallback {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => "#ccc".into(),
        },
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_html(buffer: &Buffer, palette: &Palette) -> String {
    let area = buffer.area;
    let mut body = String::new();
    for y in 0..area.height {
        body.push_str("<div class=r>");
        let mut skip = 0;
        for x in 0..area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            let width = symbol.width().max(1);
            skip = width - 1;
            let mut style = format!(
                "color:{};background:{};width:{}ch",
                css(cell.fg, palette.fg),
                css(cell.bg, palette.background),
                width
            );
            if cell.modifier.contains(Modifier::BOLD) {
                style.push_str(";font-weight:bold");
            }
            if cell.modifier.contains(Modifier::ITALIC) {
                style.push_str(";font-style:italic");
            }
            let _ = write!(body, "<span style=\"{style}\">{}</span>", escape(symbol));
        }
        body.push_str("</div>");
    }
    format!(
        "<!doctype html><meta charset=utf-8><style>\
         html,body{{margin:0;background:{bg}}}\
         .t{{padding:10px;font:16px/1.17 'DejaVu Sans Mono','Noto Color Emoji',monospace;white-space:pre}}\
         .r{{height:1.17em;display:flex}}\
         .r span{{display:inline-block;overflow:visible;text-align:left}}\
         </style><div class=t>{body}</div>",
        bg = css(palette.background, Color::Rgb(0, 0, 0)),
    )
}
