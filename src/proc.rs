//! Starting a program boxvibe does not wait for, without leaving a zombie.
//!
//! A child that is spawned and never waited on stays in the process table as
//! a zombie until boxvibe exits. boxvibe runs for hours and starts one of
//! these per song change (`notify-send`) or per opened page (`xdg-open`), so
//! they would pile up. [`detached`] reaps each one on its own thread instead.

use std::process::Command;

/// Runs `command` in the background and reaps it when it exits.
pub fn detached(mut command: Command) -> std::io::Result<()> {
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests;
