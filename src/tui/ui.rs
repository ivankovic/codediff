/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::{
    io::{Stdout, stdout},
    ops::{Deref, DerefMut},
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as CrosstermEvent, EventStream, KeyEventKind,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{Frame, backend::CrosstermBackend as Backend, layout::Rect};
use tokio::time::{Interval, interval};

use crate::tui::{color_depth::ColorDepth, events::Event, theme::OverlayPalette};

/// Owns the terminal and its raw-mode/alternate-screen lifecycle, plus the merged input/tick/
/// render event source. Input is an async, epoll-driven `EventStream`, not a thread polling
/// crossterm: an earlier version polled with a zero timeout in a loop, which kept a CPU core busy
/// and made the whole TUI feel slow.
pub struct UI {
    pub terminal: ratatui::Terminal<Backend<Stdout>>,

    pub tick_rate: f64,
    pub frame_rate: f64,

    pub mouse: bool,
    pub paste: bool,

    color_depth: ColorDepth,

    tick_interval: Interval,
    render_interval: Interval,
    crossterm_events: EventStream,
}

impl UI {
    pub fn new() -> Result<Self> {
        let tick_rate = 4.0;
        let frame_rate = 60.0;
        Ok(Self {
            terminal: ratatui::Terminal::new(Backend::new(stdout()))?,

            tick_rate,
            frame_rate,

            mouse: false,
            paste: false,

            color_depth: ColorDepth::detect(),

            tick_interval: interval(Duration::from_secs_f64(1.0 / tick_rate)),
            render_interval: interval(Duration::from_secs_f64(1.0 / frame_rate)),
            crossterm_events: EventStream::new(),
        })
    }

    pub fn tick_rate(mut self, tick_rate: f64) -> Self {
        self.tick_rate = tick_rate;
        self.tick_interval = interval(Duration::from_secs_f64(1.0 / tick_rate));
        self
    }

    pub fn frame_rate(mut self, frame_rate: f64) -> Self {
        self.frame_rate = frame_rate;
        self.render_interval = interval(Duration::from_secs_f64(1.0 / frame_rate));
        self
    }

    /// The whole terminal as a `Rect` at the origin, the shape `Component::init` and `resize` take.
    pub fn size(&self) -> Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    /// `Terminal::draw`, with the finished frame fitted to the terminal's color depth. `palette`
    /// is the overlay on screen (see `ColorDepth::fit`). Shadows the `Deref` to `Terminal`, so no
    /// caller can draw around the fitting.
    pub fn draw(
        &mut self,
        palette: &OverlayPalette,
        render: impl FnOnce(&mut Frame),
    ) -> std::io::Result<()> {
        let color_depth = self.color_depth;
        self.terminal.draw(|frame| {
            render(frame);
            color_depth.fit(frame.buffer_mut(), palette);
        })?;
        Ok(())
    }

    /// Call after a resize event.
    pub fn resize(&mut self, area: Rect) -> Result<()> {
        self.terminal.resize(area)?;
        Ok(())
    }

    /// The next tick, render or input event. `None` only once the input stream has closed.
    pub async fn next_event(&mut self) -> Option<Event> {
        loop {
            let event = tokio::select! {
                _ = self.tick_interval.tick() => Some(Event::Tick),
                _ = self.render_interval.tick() => Some(Event::Render),
                maybe_event = self.crossterm_events.next() => match maybe_event {
                    Some(Ok(event)) => map_crossterm_event(event),
                    Some(Err(_)) => None,
                    None => return None,
                },
            };
            if let Some(event) = event {
                return Some(event);
            }
        }
    }

    pub fn enter(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;
        if self.mouse {
            crossterm::execute!(stdout(), EnableMouseCapture)?;
        }
        if self.paste {
            crossterm::execute!(stdout(), EnableBracketedPaste)?;
        }
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        if crossterm::terminal::is_raw_mode_enabled()? {
            self.flush()?;
            if self.paste {
                crossterm::execute!(stdout(), DisableBracketedPaste)?;
            }
            if self.mouse {
                crossterm::execute!(stdout(), DisableMouseCapture)?;
            }
            crossterm::execute!(stdout(), LeaveAlternateScreen, cursor::Show)?;
            crossterm::terminal::disable_raw_mode()?;
        }
        Ok(())
    }
}

impl Deref for UI {
    type Target = ratatui::Terminal<Backend<Stdout>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl DerefMut for UI {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for UI {
    /// Best effort: a failure here has nowhere to go, and a panic inside `drop` during unwinding
    /// aborts the process instead of letting the panic hook report the original problem.
    fn drop(&mut self) {
        let _ = self.exit();
    }
}

/// Puts the terminal back the way the shell expects it, from any thread and without a `UI` in
/// hand: raw mode off, mouse capture off, main screen, cursor shown. Every step is attempted even
/// if an earlier one fails, and it is a no-op when raw mode is already off.
pub fn restore_terminal() {
    if !crossterm::terminal::is_raw_mode_enabled().unwrap_or(true) {
        return;
    }
    let _ = crossterm::execute!(
        stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
    let _ = crossterm::terminal::disable_raw_mode();
}

/// Installs the panic hook the TUI runs under. Without it, a panic's message is printed into the
/// alternate screen and vanishes when the terminal is restored, and the user is left with an exit
/// code and no explanation. The hook restores the terminal first, then prints the panic and where
/// to report it.
///
/// A panic on the diff-computation thread is the exception: the app catches it and shows it in the
/// viewer's error banner, so the hook leaves the terminal alone and prints nothing there - text
/// written to stderr underneath the live TUI stays on screen as garbage.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() == Some(crate::tui::app::DIFF_THREAD_NAME) {
            return;
        }
        restore_terminal();
        default_hook(info);
        eprintln!(
            "\ncodediff {} crashed. Please report this, with the lines above and the two files \
             being diffed if you can share them, at {}",
            env!("CARGO_PKG_VERSION"),
            crate::tui::ISSUE_TRACKER_URL
        );
    }));
}

/// Drops the event kinds the app has no use for: focus, bracketed paste, and key releases -
/// Windows reports a release for every press (and a repeat while held), which would run every
/// keybinding twice; the app acts on presses and repeats only.
fn map_crossterm_event(event: CrosstermEvent) -> Option<Event> {
    match event {
        CrosstermEvent::Key(key) if key.kind == KeyEventKind::Release => None,
        CrosstermEvent::Key(key) => Some(Event::Key(key)),
        CrosstermEvent::Mouse(mouse) => Some(Event::Mouse(mouse)),
        CrosstermEvent::Resize(w, h) => Some(Event::Resize(w, h)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn maps_key_event() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(
            map_crossterm_event(CrosstermEvent::Key(key)),
            Some(Event::Key(key))
        );
    }

    /// Windows reports a release for every press; acting on both would run each binding twice.
    #[test]
    fn drops_key_release_events_but_keeps_presses_and_repeats() {
        let press = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let mut repeat = press;
        repeat.kind = KeyEventKind::Repeat;
        let mut release = press;
        release.kind = KeyEventKind::Release;

        assert_eq!(
            map_crossterm_event(CrosstermEvent::Key(press)),
            Some(Event::Key(press))
        );
        assert_eq!(
            map_crossterm_event(CrosstermEvent::Key(repeat)),
            Some(Event::Key(repeat))
        );
        assert_eq!(map_crossterm_event(CrosstermEvent::Key(release)), None);
    }

    /// Outside raw mode there is nothing to restore, and a test process has no terminal to write
    /// escape sequences to.
    #[test]
    fn restore_terminal_is_a_no_op_outside_raw_mode() {
        restore_terminal();
    }

    #[test]
    fn maps_resize_event() {
        assert_eq!(
            map_crossterm_event(CrosstermEvent::Resize(80, 24)),
            Some(Event::Resize(80, 24))
        );
    }

    /// Focus and bracketed-paste events are deliberately dropped: the app has no use for them.
    #[test]
    fn drops_focus_and_paste_events() {
        assert_eq!(map_crossterm_event(CrosstermEvent::FocusGained), None);
        assert_eq!(map_crossterm_event(CrosstermEvent::FocusLost), None);
        assert_eq!(
            map_crossterm_event(CrosstermEvent::Paste("hi".to_string())),
            None
        );
    }

    #[test]
    fn maps_mouse_event() {
        let mouse = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(
            map_crossterm_event(CrosstermEvent::Mouse(mouse)),
            Some(Event::Mouse(mouse))
        );
    }
}
