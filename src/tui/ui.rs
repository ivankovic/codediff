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
        Event as CrosstermEvent, EventStream,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend as Backend, layout::Rect};
use tokio::time::{Interval, interval};

use crate::tui::events::Event;

/// Owns the terminal and its raw-mode/alternate-screen lifecycle, plus the merged input/tick/
/// render event source.
///
/// Terminal input comes from `crossterm::event::EventStream`, an async epoll-driven stream, not
/// from a dedicated polling thread: see `next_event` below and the "Async event loop" entry in
/// `SPECS.md` for why a background thread/task was deliberately avoided.
pub struct UI {
    pub terminal: ratatui::Terminal<Backend<Stdout>>,

    pub tick_rate: f64,
    pub frame_rate: f64,

    pub mouse: bool,
    pub paste: bool,

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

    /// The current terminal size.
    pub fn size(&self) -> Result<Rect> {
        Ok(self.terminal.size()?)
    }

    /// Resize the terminal's internal buffers to match a new size (call after a resize event).
    pub fn resize(&mut self, area: Rect) -> Result<()> {
        self.terminal.resize(area)?;
        Ok(())
    }

    /// Wait for the next tick, render timer, or terminal input event, merging all three into a
    /// single source. Returns `None` only once the input stream itself has closed (e.g. stdin
    /// was closed), since that means there will never be another event.
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

    pub fn suspend(&mut self) -> Result<()> {
        self.exit()?;
        #[cfg(not(windows))]
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP)?;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.enter()?;
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
    fn drop(&mut self) {
        self.exit().unwrap();
    }
}

/// Map a raw crossterm event onto our own `Event`, dropping the kinds the app has no use for
/// (focus gained/lost, bracketed paste) rather than threading them through everywhere.
fn map_crossterm_event(event: CrosstermEvent) -> Option<Event> {
    match event {
        CrosstermEvent::Key(key) => Some(Event::Key(key)),
        CrosstermEvent::Mouse(mouse) => Some(Event::Mouse(mouse)),
        CrosstermEvent::Resize(w, h) => Some(Event::Resize(w, h)),
        _ => None,
    }
}
