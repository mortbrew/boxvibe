//! boxvibe: Pandora in the terminal.

mod ads;
mod app;
mod art;
mod audio;
mod backend;
mod config;
mod demo;
mod deps;
mod desktop;
mod icons;
mod input;
mod keychain;
mod pandora;
mod snapshot;
mod splash;
mod theme;
mod ui;
mod update;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::app::App;
use crate::config::{Config, IconSet};

#[derive(Parser)]
#[command(version, about = "Pandora in the terminal: stations, thumbs, themes and icon sets")]
struct Args {
    /// Made-up stations and songs: no Pandora account, no network, no sound.
    #[arg(long)]
    demo: bool,
    /// Another config file instead of ~/.config/boxvibe/config.toml.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Theme for this run only (omarchy, tokyo-night, gruvbox, catppuccin, ...).
    #[arg(long)]
    theme: Option<String>,
    /// Icon set for this run only: emoji, nerd or unicode.
    #[arg(long)]
    icons: Option<String>,
    /// Draw one demo frame to an HTML file and exit (for screenshots).
    #[arg(long, hide = true, value_name = "FILE")]
    snapshot: Option<PathBuf>,
    /// Snapshot screen: main, settings, password, password-nokeychain, genre-edit-popup, genre-edit-popup-ticked, good-vibes-off, about, help, system, seeds, seeds-add, seeds-remove, thumbs, thumbs-delete or ad.
    #[arg(long, hide = true, default_value = "main")]
    screen: String,
    #[arg(long, hide = true, default_value_t = 120)]
    cols: u16,
    #[arg(long, hide = true, default_value_t = 36)]
    rows: u16,
}

fn main() -> anyhow::Result<()> {
    // Where this binary is now: after Update Now replaces it, Linux names
    // the running one "… (deleted)", so Restart Now uses this path.
    let exe = std::env::current_exe().ok();
    let args = Args::parse();
    let path = args.config.clone().or_else(Config::default_path);
    let (mut config, warnings) = match &path {
        // Never fails: what was wrong is a line in the status bar.
        Some(p) => Config::load(p),
        None => (Config::default(), Vec::new()),
    };
    if let Some(theme) = &args.theme {
        config.ui.theme = theme.clone();
    }
    if let Some(icons) = &args.icons {
        config.ui.icons = IconSet::parse(icons).context("--icons takes emoji, nerd or unicode")?;
    }
    if let Some(out) = &args.snapshot {
        return snapshot::write(config, &args.screen, args.cols, args.rows, out);
    }
    let demo = args.demo;
    // Open the sound device before the screen switches, so ALSA's warnings can't scribble on it.
    let mut app = App::new(config, path, &warnings, demo);
    let mut terminal = ratatui::init();
    // Once, before any events are read: what pictures this terminal shows.
    app.art.set_graphics(art::detect());
    app.begin_splash();
    // What the system lacks (a Nerd Font...) is looked at in the background and listed when it is known.
    app.start_system_check();
    let result = run(&mut terminal, &mut app);
    let restart = app.restart && result.is_ok();
    // Stop the sound and the backend before the new version takes over.
    drop(app);
    ratatui::restore();
    if restart && let Some(exe) = exe {
        // Like Update Now, never for a build from source.
        if update::is_dev_build(&exe.display().to_string()) {
            eprintln!("boxvibe: built from source, not restarting");
        } else {
            let error = restart_into(&exe);
            eprintln!("boxvibe: could not restart {}: {error}", exe.display());
        }
    }
    result
}

/// Starts `exe` with the same arguments in place of this process (same
/// terminal, same pid). Returns only on failure.
fn restart_into(exe: &std::path::Path) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    std::process::Command::new(exe).args(&args).exec()
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    let mut last = Instant::now();
    while !app.quit {
        app.tick();
        if app.edit_config {
            app.edit_config = false;
            edit_config(terminal, app)?;
        }
        app.art.set_overlay(app.overlay_open());
        if app.art.needs_clear.take() {
            terminal.clear()?;
        }
        terminal.draw(|frame| ui::draw(frame, app))?;
        // Faster while the boot screen moves, 10 fps otherwise.
        let frame_time = Duration::from_millis(if app.splash.is_some() { 40 } else { 100 });
        let wait = frame_time.saturating_sub(last.elapsed());
        if event::poll(wait)? {
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                _ => {}
            }
        }
        if last.elapsed() >= frame_time {
            last = Instant::now();
        }
    }
    Ok(())
}

/// Settings → Edit config.toml: the terminal goes to `$VISUAL` / `$EDITOR`
/// (else nano, else vi) and the file is read again when it closes.
fn edit_config(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> anyhow::Result<()> {
    let Some(path) = app.config_path.clone() else { return Ok(()) };
    if !path.exists() {
        // Something to edit, private from the start.
        config::set_values(&path, &[("look", "theme", app.config.ui.theme.as_str().into())])?;
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
        .filter(|e| !e.trim().is_empty())
        .unwrap_or_else(|| if which("nano") { "nano".into() } else { "vi".into() });
    ratatui::restore();
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(program).args(parts).arg(&path).status();
    *terminal = ratatui::init();
    terminal.clear()?;
    match status {
        Ok(_) => app.reload_config(),
        Err(e) => app.status = format!("⚠ Can't run {editor}: {e}"),
    }
    Ok(())
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}
