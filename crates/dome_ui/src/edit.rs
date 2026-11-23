use dome_math::Vec2;

use crate::clipboard::Clipboard;
use crate::grapheme::{self, Boundary, Class};
use crate::input::{Input, Key};

const BLINK_PERIOD: f32 = 1.06;
const BLINK_DUTY: f32 = 0.53;
const MULTI_CLICK_SECONDS: f32 = 0.35;
const MULTI_CLICK_SLOP: f32 = 4.0;

pub(crate) const COMMANDS: [char; 6] = ['a', 'c', 'v', 'x', 'y', 'z'];

pub(crate) fn command(character: char) -> bool {
    COMMANDS.contains(&character.to_ascii_lowercase())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Commit,
    Advance,
    Cancel,
}

#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    anchor: Boundary,
    caret: Boundary,
}

#[derive(Clone, Debug)]
pub struct Edit {
    key: u64,
    text: String,
    anchor: Boundary,
    caret: Boundary,
    active_at: f32,
    clicks: u32,
    last_click: Option<(f32, Vec2)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    multiline: bool,
}

impl Edit {
    pub fn new(key: u64, text: &str, time: f32) -> Self {
        let end = Boundary::end(text);
        Self {
            key,
            text: text.to_string(),
            anchor: end,
            caret: end,
            active_at: time,
            clicks: 0,
            last_click: None,
            undo: Vec::new(),
            redo: Vec::new(),
            multiline: false,
        }
    }

    pub fn multiline(key: u64, text: &str, time: f32) -> Self {
        Self {
            multiline: true,
            ..Self::new(key, text, time)
        }
    }

    pub fn key(&self) -> u64 {
        self.key
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> Boundary {
        self.caret
    }

    pub fn anchor(&self) -> Boundary {
        self.anchor
    }

    pub fn length(&self) -> usize {
        self.text.len()
    }

    pub fn selection(&self) -> Option<(Boundary, Boundary)> {
        (self.caret != self.anchor)
            .then(|| (self.caret.min(self.anchor), self.caret.max(self.anchor)))
    }

    pub fn caret_visible(&self, time: f32) -> bool {
        self.blink_phase(time) < BLINK_DUTY
    }

    pub fn blink_remaining(&self, time: f32) -> f32 {
        let phase = self.blink_phase(time);
        if phase < BLINK_DUTY {
            BLINK_DUTY - phase
        } else {
            BLINK_PERIOD - phase
        }
    }

    fn blink_phase(&self, time: f32) -> f32 {
        (time - self.active_at).rem_euclid(BLINK_PERIOD)
    }

    pub fn composed(&self, preedit: &str) -> String {
        if preedit.is_empty() {
            return self.text.clone();
        }
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let mut composed = String::with_capacity(self.text.len() + preedit.len());
        composed.push_str(&self.text[..start.byte()]);
        composed.push_str(preedit);
        composed.push_str(&self.text[end.byte()..]);
        composed
    }

    pub fn place(&mut self, caret: Boundary, extend: bool, time: f32) {
        self.caret = caret;
        if !extend {
            self.anchor = caret;
        }
        self.active_at = time;
    }

    pub fn select(&mut self, range: (Boundary, Boundary), time: f32) {
        self.anchor = range.0;
        self.caret = range.1;
        self.active_at = time;
    }

    pub fn select_all(&mut self, time: f32) {
        self.anchor = Boundary::start();
        self.caret = Boundary::end(&self.text);
        self.active_at = time;
    }

    pub fn click(&mut self, point: Vec2, time: f32) -> u32 {
        let count = match self.last_click {
            Some((last, at))
                if time - last <= MULTI_CLICK_SECONDS
                    && (point - at).length() <= MULTI_CLICK_SLOP =>
            {
                self.clicks % 3 + 1
            }
            _ => 1,
        };
        self.last_click = Some((time, point));
        self.clicks = count;
        count
    }

    pub fn word(&self, caret: Boundary) -> (Boundary, Boundary) {
        let clusters = grapheme::clusters(&self.text).collect::<Vec<_>>();
        if clusters.is_empty() {
            return (caret, caret);
        }
        let index = clusters
            .partition_point(|cluster| cluster.start < caret)
            .min(clusters.len() - 1);
        let class = clusters[index].class();
        if class == Class::Space {
            return (caret, caret);
        }
        let mut start = index;
        while start > 0 && clusters[start - 1].class() == class {
            start -= 1;
        }
        let mut end = index + 1;
        while end < clusters.len() && clusters[end].class() == class {
            end += 1;
        }
        (clusters[start].start, clusters[end - 1].end)
    }

    fn previous_word(&self, caret: Boundary) -> Boundary {
        let clusters = grapheme::clusters(&self.text).collect::<Vec<_>>();
        let mut index = clusters
            .iter()
            .take_while(|cluster| cluster.end <= caret)
            .count();
        while index > 0 && clusters[index - 1].class() == Class::Space {
            index -= 1;
        }
        if index == 0 {
            return Boundary::start();
        }
        let class = clusters[index - 1].class();
        while index > 0 && clusters[index - 1].class() == class {
            index -= 1;
        }
        clusters
            .get(index)
            .map_or_else(Boundary::start, |cluster| cluster.start)
    }

    fn next_word(&self, caret: Boundary) -> Boundary {
        let clusters = grapheme::clusters(&self.text).collect::<Vec<_>>();
        let mut index = clusters.partition_point(|cluster| cluster.start < caret);
        while index < clusters.len() && clusters[index].class() == Class::Space {
            index += 1;
        }
        if index == clusters.len() {
            return Boundary::end(&self.text);
        }
        let class = clusters[index].class();
        while index < clusters.len() && clusters[index].class() == class {
            index += 1;
        }
        clusters
            .get(index)
            .map_or_else(|| Boundary::end(&self.text), |cluster| cluster.start)
    }

    fn line_bounds(&self, line: usize) -> (usize, usize) {
        let mut start = 0usize;
        let mut current = 0usize;
        for (offset, character) in self.text.char_indices() {
            if character != '\n' {
                continue;
            }
            if current == line {
                return (start, offset);
            }
            current += 1;
            start = offset + 1;
        }
        (start, self.text.len())
    }

    fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    fn line_span(&self, line: usize) -> (Boundary, Boundary) {
        let (start, end) = self.line_bounds(line);
        let end = Boundary::floor(&self.text, end);
        (Boundary::ceil(&self.text, start).min(end), end)
    }

    fn caret_line(&self) -> (usize, usize) {
        let caret = self.caret.byte();
        let mut line = 0usize;
        let mut start = 0usize;
        for (offset, character) in self.text.char_indices() {
            if offset >= caret {
                break;
            }
            if character == '\n' {
                line += 1;
                start = offset + 1;
            }
        }
        let start = Boundary::ceil(&self.text, start).byte();
        (line, grapheme::clusters(&self.text[start..caret]).count())
    }

    fn vertical(&mut self, delta: i32, extend: bool, time: f32) {
        let (line, column) = self.caret_line();
        let target = (line as i32 + delta).clamp(0, self.line_count() as i32 - 1) as usize;
        let (start, end) = self.line_span(target);
        let caret = if column == 0 {
            start
        } else {
            grapheme::clusters(&self.text)
                .filter(|cluster| cluster.start >= start && cluster.end <= end)
                .nth(column - 1)
                .map_or(end, |cluster| cluster.end)
        };
        self.place(caret, extend, time);
    }

    fn home(&mut self, extend: bool, time: f32) {
        let caret = if self.multiline {
            self.line_span(self.caret_line().0).0
        } else {
            Boundary::start()
        };
        self.place(caret, extend, time);
    }

    fn end(&mut self, extend: bool, time: f32) {
        let caret = if self.multiline {
            self.line_span(self.caret_line().0).1
        } else {
            Boundary::end(&self.text)
        };
        self.place(caret, extend, time);
    }

    fn remember(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 128 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.anchor = snapshot.anchor;
        self.caret = snapshot.caret;
    }

    fn undo(&mut self, time: f32) {
        let Some(snapshot) = self.undo.pop() else {
            return;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        self.active_at = time;
    }

    fn redo(&mut self, time: f32) {
        let Some(snapshot) = self.redo.pop() else {
            return;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        self.active_at = time;
    }

    pub fn insert(&mut self, text: &str, time: f32) {
        let text = if self.multiline {
            text.replace(['\r', '\t'], " ")
        } else {
            text.replace(['\n', '\r', '\t'], " ")
        };
        if text.is_empty() {
            return;
        }
        self.replace(&text, time);
    }

    pub fn poll(
        &mut self,
        input: &Input,
        clipboard: &mut dyn Clipboard,
        time: f32,
    ) -> Option<Outcome> {
        for text in input.texts() {
            self.insert(text, time);
        }
        let modifiers = input.modifiers();
        for event in input.keys() {
            if !event.pressed {
                continue;
            }
            let Some(key) = event.key else {
                continue;
            };
            if (modifiers.control || modifiers.logo)
                && let Key::Character(character) = key
            {
                if command(character) {
                    match character.to_ascii_lowercase() {
                        'a' => self.select_all(time),
                        'c' => self.copy(clipboard),
                        'x' => self.cut(clipboard, time),
                        'v' => self.paste(clipboard, time),
                        'z' if modifiers.shift => self.redo(time),
                        'z' => self.undo(time),
                        'y' => self.redo(time),
                        _ => {}
                    }
                }
                continue;
            }
            match key {
                Key::Character(_) | Key::Function(_) => {}
                Key::Backspace if modifiers.control || modifiers.logo => {
                    self.erase_word_before(time);
                }
                Key::Backspace => self.erase_before(time),
                Key::Delete if modifiers.control || modifiers.logo => self.erase_word_after(time),
                Key::Delete => self.erase_after(time),
                Key::Left if modifiers.control || modifiers.logo => {
                    let target = self.previous_word(self.caret);
                    self.place(target, modifiers.shift, time);
                }
                Key::Left => self.step(-1, modifiers.shift, time),
                Key::Right if modifiers.control || modifiers.logo => {
                    let target = self.next_word(self.caret);
                    self.place(target, modifiers.shift, time);
                }
                Key::Right => self.step(1, modifiers.shift, time),
                Key::Home => self.home(modifiers.shift, time),
                Key::End => self.end(modifiers.shift, time),
                Key::Up if self.multiline => self.vertical(-1, modifiers.shift, time),
                Key::Down if self.multiline => self.vertical(1, modifiers.shift, time),
                Key::Tab => return Some(Outcome::Advance),
                Key::Enter if self.multiline => self.insert("\n", time),
                Key::Enter => return Some(Outcome::Commit),
                Key::Escape => return Some(Outcome::Cancel),
                Key::Space | Key::Up | Key::Down | Key::PageUp | Key::PageDown => {}
            }
        }
        None
    }

    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection()?;
        Some(self.text[start.byte()..end.byte()].to_string())
    }

    fn copy(&self, clipboard: &mut dyn Clipboard) {
        let Some(text) = self.selected_text() else {
            return;
        };
        clipboard.write(&text);
    }

    fn cut(&mut self, clipboard: &mut dyn Clipboard, time: f32) {
        let Some(text) = self.selected_text() else {
            return;
        };
        clipboard.write(&text);
        self.replace("", time);
    }

    fn paste(&mut self, clipboard: &mut dyn Clipboard, time: f32) {
        if let Some(text) = clipboard.read() {
            self.insert(&text, time);
        }
    }

    fn replace(&mut self, text: &str, time: f32) {
        self.remember();
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        self.text.replace_range(start.byte()..end.byte(), text);
        self.caret = Boundary::ceil(&self.text, start.byte() + text.len());
        self.anchor = self.caret;
        self.active_at = time;
    }

    fn erase_before(&mut self, time: f32) {
        if self.selection().is_some() {
            self.replace("", time);
            return;
        }
        let Some(start) = self.caret.previous(&self.text) else {
            return;
        };
        self.remember();
        self.text.replace_range(start.byte()..self.caret.byte(), "");
        self.caret = Boundary::floor(&self.text, start.byte());
        self.anchor = self.caret;
        self.active_at = time;
    }

    fn erase_after(&mut self, time: f32) {
        if self.selection().is_some() {
            self.replace("", time);
            return;
        }
        let Some(end) = self.caret.next(&self.text) else {
            return;
        };
        self.remember();
        self.text.replace_range(self.caret.byte()..end.byte(), "");
        self.caret = Boundary::floor(&self.text, self.caret.byte());
        self.anchor = self.caret;
        self.active_at = time;
    }

    fn erase_word_before(&mut self, time: f32) {
        if self.selection().is_some() {
            self.replace("", time);
            return;
        }
        let start = self.previous_word(self.caret);
        if start < self.caret {
            self.remember();
            self.text.replace_range(start.byte()..self.caret.byte(), "");
            self.caret = Boundary::floor(&self.text, start.byte());
            self.anchor = self.caret;
            self.active_at = time;
        }
    }

    fn erase_word_after(&mut self, time: f32) {
        if self.selection().is_some() {
            self.replace("", time);
            return;
        }
        let end = self.next_word(self.caret);
        if self.caret < end {
            self.remember();
            self.text.replace_range(self.caret.byte()..end.byte(), "");
            self.caret = Boundary::floor(&self.text, self.caret.byte());
            self.anchor = self.caret;
            self.active_at = time;
        }
    }

    fn step(&mut self, delta: i32, extend: bool, time: f32) {
        let from = if extend {
            self.caret
        } else if let Some((start, end)) = self.selection() {
            if delta < 0 { start } else { end }
        } else {
            self.caret
        };
        let caret = if delta < 0 {
            from.previous(&self.text)
        } else {
            from.next(&self.text)
        };
        self.place(caret.unwrap_or(from), extend, time);
    }
}
