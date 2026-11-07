//! boxvibe's modal dialogs on a `vibekit::dialog::DialogStack`: the quit,
//! unpin, delete, editor and theme-delete questions (`Confirm`), Help, a
//! station's or song's details, and the System Check list.
//!
//! The rules (vibekit STYLE.md, Global keys): Esc closes the top dialog,
//! `?` also closes Help, confirms answer only on `y`/`n`/Enter/Esc (a
//! second `Q` on the quit question), other keys do nothing, and lowercase
//! `q` never closes anything.

use super::*;
use ratatui::Frame;
use ratatui::layout::Rect;
use vibekit::dialog::{Confirm, Dialog, Help, Response};

/// A `Confirm` that carries what it is about.
pub struct Ask<T> {
    pub confirm: Confirm,
    pub about: T,
}

/// The quit question.
pub struct Quit;
/// The unpin question: which pin.
#[derive(Clone, Copy)]
pub struct Unpin(pub usize);

/// Theme Select: which saved theme to delete.
pub struct ThemeDelete(pub String);

impl<T: 'static> Dialog for Ask<T> {
    fn key(&mut self, key: KeyEvent) -> Response {
        self.confirm.key(key)
    }
    fn draw(&self, frame: &mut Frame, screen: Rect, palette: &Palette) {
        self.confirm.draw(frame, screen, palette);
    }
}

/// `v`: a song's or station's details (filled in as Pandora answers).
pub struct DetailsBox(pub Details);

impl Dialog for DetailsBox {
    fn key(&mut self, _: KeyEvent) -> Response {
        Response::Keep
    }
    fn draw(&self, frame: &mut Frame, screen: Rect, palette: &Palette) {
        crate::ui::draw_details(frame, palette, body(screen), &self.0);
    }
}

/// The System Check list. Enter looks again (the list stays open).
pub struct SystemBox {
    pub checks: Option<Vec<crate::deps::Check>>,
    pub look_again: bool,
}

impl Dialog for SystemBox {
    fn key(&mut self, key: KeyEvent) -> Response {
        if key.code == KeyCode::Enter {
            self.look_again = true;
        }
        Response::Keep
    }
    fn draw(&self, frame: &mut Frame, screen: Rect, palette: &Palette) {
        crate::ui::draw_system(frame, palette, self.checks.as_deref(), body(screen));
    }
}

/// The screen above the status and key bars, where the big popups go.
fn body(screen: Rect) -> Rect {
    Rect { height: screen.height.saturating_sub(2), ..screen }
}

impl App {
    fn push_dialog(&mut self, dialog: impl Dialog) {
        self.dialogs.push(dialog, Instant::now());
    }

    /// `Q`: the quit question (a second `Q` or `y` quits).
    pub fn ask_quit(&mut self) {
        if !self.dialogs.has::<Ask<Quit>>() {
            self.push_dialog(Ask { confirm: Confirm::quit("boxvibe"), about: Quit });
        }
    }

    pub fn quit_asked(&self) -> bool {
        self.dialogs.has::<Ask<Quit>>()
    }

    pub(super) fn ask_unpin(&mut self, at: usize) {
        let question = self
            .pins
            .get(at)
            .map(|pin| format!("Unpin “{}” by {}?", crate::ui::clean(&pin.title), crate::ui::clean(&pin.artist)))
            .unwrap_or_else(|| "Unpin this song?".into());
        let confirm = Confirm::new("unpin", question).title("Unpin song").labels("Unpin", "Keep");
        self.push_dialog(Ask { confirm, about: Unpin(at) });
    }

    /// Which pin the unpin question is about.
    pub fn confirm_unpin(&self) -> Option<usize> {
        self.dialogs.find::<Ask<Unpin>>().map(|a| a.about.0)
    }

    pub(super) fn ask_delete_station(&mut self, station: StationRef) {
        let name = crate::ui::clean(&station.name);
        let question = if station.favorite {
            format!(
                "Delete the station “{name}” (the one this favorite plays)? It removes it from your Pandora account."
            )
        } else {
            format!("Delete the station “{name}”? It removes it from your Pandora account.")
        };
        let confirm = Confirm::new("delete", question).title("Delete station").labels("Delete", "Keep");
        self.push_dialog(Ask { confirm, about: station });
    }

    /// The station the delete question is about.
    pub fn confirm_delete(&self) -> Option<&StationRef> {
        self.dialogs.find::<Ask<StationRef>>().map(|a| &a.about)
    }

    /// `?`: Help (`?` or Esc closes it).
    pub fn open_help(&mut self) {
        if !self.dialogs.has::<Help>() {
            let help =
                Help { title: "boxvibe Help".into(), ..Help::new(crate::ui::help_sections(self.config.ui.icons)) };
            self.push_dialog(help);
        }
    }

    pub fn help_open(&self) -> bool {
        self.dialogs.has::<Help>()
    }

    /// Opens the details popup (in place of one already open).
    pub(super) fn open_details(&mut self, details: Details) {
        self.dialogs.remove::<DetailsBox>();
        self.push_dialog(DetailsBox(details));
    }

    pub fn details(&self) -> Option<&Details> {
        self.dialogs.find::<DetailsBox>().map(|d| &d.0)
    }

    pub(super) fn details_mut(&mut self) -> Option<&mut Details> {
        self.dialogs.find_mut::<DetailsBox>().map(|d| &mut d.0)
    }

    pub fn open_system(&mut self) {
        if !self.dialogs.has::<SystemBox>() {
            let checks = self.system.clone();
            self.push_dialog(SystemBox { checks, look_again: false });
        }
    }

    pub fn system_open(&self) -> bool {
        self.dialogs.has::<SystemBox>()
    }

    /// The list shows the latest look (`None`: looking).
    pub(crate) fn system_changed(&mut self) {
        let checks = self.system.clone();
        if let Some(open) = self.dialogs.find_mut::<SystemBox>() {
            open.checks = checks;
        }
    }

    /// Theme Select's delete question.
    pub(super) fn ask_delete_theme(&mut self, name: String) {
        let question =
            format!("Delete the theme “{}”? Its file in ~/.config/boxvibe/themes/ goes too.", crate::ui::clean(&name));
        let confirm = Confirm::new("theme-delete", question).title("Delete theme").labels("Delete", "Keep");
        self.push_dialog(Ask { confirm, about: ThemeDelete(name) });
    }

    pub fn theme_delete_asked(&self) -> Option<&str> {
        self.dialogs.find::<Ask<ThemeDelete>>().map(|a| a.about.0.as_str())
    }

    /// Station editor: remove a seed or take back a thumb.
    pub(super) fn ask_editor(&mut self, about: EditorConfirm) {
        let station = self.editor.as_ref().map(|e| e.station.name.clone()).unwrap_or_default();
        let question = about.question(&station);
        let (id, title, yes) = match &about {
            EditorConfirm::RemoveSeed(_) => ("remove-seed", "Remove seed", "Remove"),
            EditorConfirm::DeleteThumb(_) => ("delete-thumb", "Take back thumb", "Take Back"),
        };
        let confirm = Confirm::new(id, question).title(title).labels(yes, "Keep");
        self.push_dialog(Ask { confirm, about });
    }

    pub fn editor_confirm(&self) -> Option<&EditorConfirm> {
        self.dialogs.find::<Ask<EditorConfirm>>().map(|a| &a.about)
    }

    /// A key for the open dialogs; false if none is open (the key is the app's).
    pub(super) fn dialog_key(&mut self, key: KeyEvent) -> bool {
        // What the top question is about: the stack drops it as it answers.
        let unpin = self.dialogs.top::<Ask<Unpin>>().map(|a| a.about);
        let delete = self.dialogs.top::<Ask<StationRef>>().map(|a| a.about.clone());
        let theme = self.dialogs.top::<Ask<ThemeDelete>>().map(|a| a.about.0.clone());
        let editor = self.dialogs.top::<Ask<EditorConfirm>>().map(|a| a.about.clone());
        let Some(response) = self.dialogs.key(key) else { return false };
        let yes = matches!(response, Response::Answer { yes: true, .. });
        let closed = response != Response::Keep;
        if let Some(Unpin(at)) = unpin.filter(|_| closed) {
            if yes && at < self.pins.len() {
                let pin = self.pins.remove(at);
                self.pinned_cursor = self.pinned_cursor.min(self.pins.len().saturating_sub(1));
                self.save_pins();
                self.status = format!("Unpinned “{}”", pin.title);
            } else {
                self.status = "Kept the pin".into();
            }
        }
        if let Some(station) = delete.filter(|_| closed) {
            if yes {
                self.status = format!("Deleting “{}” from your Pandora account…", station.name);
                self.backend.send(Request::RemoveStation { station_id: station.id });
            } else {
                self.status = "Kept the station".into();
            }
        }
        if let Some(name) = theme.filter(|_| closed) {
            if yes {
                self.delete_theme(&name);
            } else {
                self.status = "Kept the theme".into();
            }
        }
        if let Some(confirm) = editor.filter(|_| closed) {
            if yes {
                self.editor_confirmed(confirm);
            } else {
                self.status = "Kept it".into();
            }
        }
        if let Response::Answer { id, yes: true } = &response
            && id == "quit"
        {
            self.quit = true;
        }
        if let Some(open) = self.dialogs.top_mut::<SystemBox>()
            && std::mem::take(&mut open.look_again)
        {
            open.checks = None;
            self.system = None;
            self.start_system_check();
        }
        true
    }
}
