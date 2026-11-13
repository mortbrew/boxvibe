//! Login, logout, and the desktop keychain. Split out of `app.rs`; these are
//! still `App` methods.

use super::*;

impl App {
    /// First thing after a start: log in with what is saved, or ask.
    pub(super) fn start_account(&mut self) {
        if self.config.has_login() {
            // A password an older boxvibe left in config.toml moves to the keychain
            // (once Pandora accepts it; see `Reply::LoggedIn`).
            self.pending_password.read_from_file();
            self.log_in();
        } else if !self.config.pandora.username.trim().is_empty() {
            // The usual start: the password is in the keychain.
            let user = self.config.pandora.username.trim().to_owned();
            self.backend.keychain(&self.keychain, &user, KeychainJob::Load);
            if self.status.is_empty() {
                self.status = "Getting your password from the keychain…".into();
            }
        } else {
            // First run: Settings, on the account, typing the email.
            self.view = View::Settings;
            self.focus = Focus::Content;
            self.settings_cursor = Setting::at(Setting::Email);
            self.open_prompt(Prompt::Email);
            self.status = "Welcome to boxvibe: log in with your Pandora account".into();
        }
    }

    pub(super) fn log_in(&mut self) {
        let (username, password) = if self.demo {
            ("demo".to_owned(), String::new())
        } else {
            (self.config.pandora.username.trim().to_owned(), self.config.pandora.password.expose().to_owned())
        };
        if !self.demo && username.is_empty() {
            self.status = "⚠ Type your Pandora email first".into();
            self.settings_cursor = Setting::at(Setting::Email);
            return;
        }
        if !self.demo && password.is_empty() {
            // Nothing in the keychain (or no keychain): ask, like a login form.
            self.ask_password();
            return;
        }
        self.session = Session::SigningIn;
        self.refresh_menu_cache();
        if !self.demo {
            // No account name here: `status` is kept for the Notifications page
            // and is the one string that could reach a log, so it stays free of
            // anything personal. Settings shows which account is signing in.
            self.status = "Signing in…".into();
        }
        self.backend.send(Request::Login { username, password });
    }

    /// Opens the password box (Settings, on Log in).
    pub(super) fn ask_password(&mut self) {
        self.view = View::Settings;
        self.focus = Focus::Content;
        self.open_prompt(Prompt::Password);
        self.settings_cursor = Setting::at(Setting::LogIn);
        // The account is on the Settings row above, so it is not repeated here.
        self.status = "Type your password".into();
    }

    /// The password popup with `typed` already in it (the snapshot screens).
    pub(crate) fn ask_password_for_snapshot(&mut self, typed: &str) {
        self.ask_password();
        for c in typed.chars() {
            self.query.key(
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::Char(c),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
                MAX_PROMPT,
            );
        }
    }

    /// What the keychain answered (it always runs on its own thread).
    pub(super) fn on_keychain(&mut self, reply: KeychainReply) {
        match reply {
            KeychainReply::Loaded(Ok(Some(password))) => {
                if self.config.pandora.password.is_empty() {
                    self.config.pandora.password = Secret::new(password);
                    self.keychain_state = KeychainState::Saved;
                    self.log_in();
                }
            }
            KeychainReply::Loaded(Ok(None)) => {
                self.keychain_state = KeychainState::NotSaved;
                if self.config.pandora.password.is_empty() {
                    self.ask_password();
                }
            }
            KeychainReply::Loaded(Err(_)) => {
                self.keychain_state = KeychainState::Unavailable;
                if self.config.pandora.password.is_empty() {
                    self.ask_password();
                }
                self.status = NO_KEYCHAIN.into();
            }
            KeychainReply::Saved(Ok(())) => {
                self.keychain_state = KeychainState::Saved;
                if std::mem::take(&mut self.pending_password).in_file() {
                    // Only now, with the keychain holding it, does the file let go of it.
                    self.status =
                        match self.config_path.as_deref().filter(|_| !self.demo).map(config::forget_plaintext_password)
                        {
                            Some(Err(e)) => format!("⚠ {e}"),
                            Some(Ok(true)) => "Your password moved from config.toml to the keychain".into(),
                            _ => String::new(),
                        };
                }
            }
            KeychainReply::Saved(Err(_)) => {
                // The password stays in memory for this session, and nowhere on disk.
                // (One an older boxvibe left in config.toml is left there: it isn't lost.)
                self.keychain_state = KeychainState::Unavailable;
                self.pending_password.done();
                self.status = NO_KEYCHAIN.into();
            }
            KeychainReply::Forgotten { result: Ok(()), .. } => {
                if self.keychain_state == KeychainState::Saved {
                    self.keychain_state = KeychainState::NotSaved;
                }
            }
            KeychainReply::Forgotten { result: Err(_), loud } => {
                self.keychain_state = KeychainState::Unavailable;
                if loud {
                    self.status = format!("{NO_KEYCHAIN}: nothing saved to forget");
                }
            }
        }
    }

    /// Deletes the saved password: from the keychain, and from config.toml if an
    /// older boxvibe left it there. `loud`: the user asked, so say it.
    pub(super) fn forget_saved_password(&mut self, loud: bool) {
        if self.demo {
            if loud {
                self.status = "The demo has no password".into();
            }
            return;
        }
        let user = self.config.pandora.username.trim().to_owned();
        if !user.is_empty() {
            self.backend.keychain(&self.keychain, &user, KeychainJob::Forget { loud });
        }
        let from_file = self.config_path.as_deref().map(config::forget_plaintext_password);
        self.pending_password.done();
        if let Some(Err(e)) = from_file {
            self.status = format!("⚠ {e}");
        } else if loud {
            self.status = "Forgot the saved password: you'll be asked for it next start".into();
        }
        if self.keychain_state == KeychainState::Saved {
            self.keychain_state = KeychainState::NotSaved;
        }
    }

    pub(super) fn log_out(&mut self) {
        self.audio.stop();
        self.ad = AdPhase::Off;
        self.now = None;
        self.stream = None;
        // The next account's plan is not known until it logs in, so Audio
        // quality must not stay greyed out from this one.
        self.hifi = None;
        self.queue.clear();
        self.history.clear();
        self.stations.clear();
        // A dislike held on an Up Next row belongs to the account that held it.
        self.held_dislikes.clear();
        self.bump_audio_generation();
        self.buffering = None;
        self.audio_cache.clear();
        self.forget_recent();
        self.current_station = None;
        self.menu_cursor = 0;
        self.view = View::Settings;
        self.session = Session::LoggedOut;
        self.refresh_menu_cache();
        self.forget_saved_password(false);
        self.config.pandora.password.clear();
        self.pending_password.done();
        self.status = if self.demo || self.keychain_state == KeychainState::Unavailable {
            "Logged out".into()
        } else {
            "Logged out: the saved password was removed from your keychain".into()
        };
    }

    pub(super) fn reply_logged_in(&mut self, hifi: bool) {
        self.hifi = Some(hifi);
        let first = self.session != Session::LoggedIn;
        self.session = Session::LoggedIn;
        if first {
            self.refresh_menu_cache();
        }
        if !self.demo {
            let email = self.config.pandora.username.clone();
            self.save(&[("account", "email_or_username", email.as_str().into())]);
            // The password goes to the keychain, now that Pandora has taken it.
            if self.pending_password.needs_saving() {
                self.pending_password.sent();
                let password = self.config.pandora.password.expose().to_owned();
                self.backend.keychain(&self.keychain, &email, KeychainJob::Save(password));
            }
            // The status bar's right side says who is logged in.
            self.status =
                if self.keychain_state == KeychainState::Unavailable { NO_KEYCHAIN.into() } else { String::new() };
        }
        if first && self.view == View::Settings && self.stations.is_empty() {
            // Straight to the music after logging in from Settings.
            self.view = View::Player;
            self.focus = Focus::Menu;
        }
        self.backend.send(Request::Stations);
    }

    pub(super) fn reply_login_failed(&mut self, message: String, wrong_password: bool) {
        if wrong_password {
            // A wrong password is not kept: not in memory, not in the keychain.
            self.config.pandora.password.clear();
            self.pending_password.done();
            self.forget_saved_password(false);
        }
        self.session = Session::Failed(message.clone());
        self.status = format!("⚠ Login failed: {message}");
        self.view = View::Settings;
        self.focus = Focus::Content;
        self.settings_cursor = Setting::at(Setting::LogIn);
    }
}
