use crate::theme::{Ink, Palette};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Metadata is text, never terminal instructions (CSI, OSC, DCS or C1 controls).
pub fn clean(text: &str) -> String {
    let mut result = String::new();
    let mut escape = 0;
    for ch in text.nfc() {
        match (escape, ch) {
            (0, '\u{1b}') => escape = 1,
            (0, '\u{9b}') => escape = 2,
            (0, '\u{9d}' | '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}') => escape = 3,
            (1, '[') => escape = 2,
            (1, ']' | 'P' | 'X' | '^' | '_') => escape = 3,
            (1, _) => escape = 0,
            (2, '@'..='~') => escape = 0,
            (3, '\u{7}' | '\u{9c}') | (4, '\\') => escape = 0,
            (3, '\u{1b}') => escape = 4,
            (4, _) => escape = 3,
            (0, _)
                if !ch.is_control()
                    && !matches!(ch,
                '\u{ad}' | '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}' |
                '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}') =>
            {
                result.push(ch)
            }
            _ => {}
        }
    }
    result
}

pub fn cells(text: &str) -> i32 {
    clean(text).width() as i32
}

pub fn cut(text: &str, width: i32) -> String {
    let mut remaining = width.max(0);
    clean(text)
        .graphemes(true)
        .take_while(|ch| {
            remaining -= ch.width() as i32;
            remaining >= 0
        })
        .collect()
}

pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

pub struct Canvas<'a> {
    pub buffer: Buffer,
    pub palette: &'a Palette,
}

impl<'a> Canvas<'a> {
    pub fn new(width: u16, height: u16, palette: &'a Palette) -> Self {
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        buffer.set_style(
            buffer.area,
            palette["text.primary"]
                .style()
                .bg(palette["background"].color()),
        );
        Self { buffer, palette }
    }

    pub fn width(&self) -> i32 {
        i32::from(self.buffer.area.width)
    }
    pub fn height(&self) -> i32 {
        i32::from(self.buffer.area.height)
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, ink: Ink) {
        self.label(x, y, text, ink, false, self.width());
    }

    pub fn label(&mut self, x: i32, y: i32, text: &str, ink: Ink, bold: bool, width: i32) {
        let style = if bold {
            ink.style().add_modifier(Modifier::BOLD)
        } else {
            ink.style()
        };
        self.put(x, y, text, style, width);
    }

    pub fn pixel(&mut self, x: i32, y: i32, foreground: Ink, background: Ink) {
        self.put(x, y, "▀", foreground.style().bg(background.color()), 1);
    }

    fn blank(&mut self, x: i32, y: i32, style: Style) {
        if let Some(cell) = self.buffer.cell_mut((x as u16, y as u16)) {
            cell.set_symbol(" ");
            cell.modifier = Modifier::empty();
            cell.set_style(Style { bg: None, ..style }.remove_modifier(Modifier::BOLD));
        }
    }

    fn put(&mut self, mut x: i32, y: i32, text: &str, style: Style, width: i32) {
        if y < 0 || y >= self.height() {
            return;
        }
        let end = self.width().min(x.saturating_add(width.max(0)));
        for ch in clean(text).graphemes(true) {
            let size = ch.width() as i32;
            if size == 0 {
                continue;
            }
            if x + size > end {
                break;
            }
            if x >= 0 {
                for xx in x..x + size {
                    let mut start = xx;
                    while start > 0 && self.buffer[(start as u16, y as u16)].symbol().is_empty() {
                        start -= 1;
                    }
                    let old_width = self.buffer[(start as u16, y as u16)].symbol().width() as i32;
                    if old_width > 1 {
                        for col in start..(start + old_width).min(self.width()) {
                            self.blank(col, y, style);
                        }
                    }
                }
                let bg = style.bg.unwrap_or(self.buffer[(x as u16, y as u16)].bg);
                for dx in 0..size {
                    let cell = &mut self.buffer[((x + dx) as u16, y as u16)];
                    cell.set_symbol(if dx == 0 { ch } else { "" });
                    cell.modifier = Modifier::empty();
                    cell.set_style(style.bg(bg));
                }
            }
            x += size;
        }
    }

    pub fn fill(&mut self, x: i32, y: i32, width: i32, height: i32, color: Ink) {
        let style = self.palette["text.primary"].style().bg(color.color());
        let text = " ".repeat(width.clamp(0, self.width()) as usize);
        for yy in y.max(0)..(y + height).min(self.height()) {
            self.put(x, yy, &text, style, width);
        }
    }

    pub fn rule(&mut self, x: i32, y: i32, width: i32, color: Ink) {
        self.text(
            x,
            y,
            &"─".repeat(width.clamp(0, self.width()) as usize),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_metadata_and_partial_wide_overwrites_are_safe() {
        assert_eq!(
            clean("가\u{1b}[31m사\u{1b}]52;c;secret\u{7}\u{202e}\n"),
            "가사"
        );
        assert_eq!(cells("가사 ABC"), 8);
        assert_eq!(cut("한글 ABC", 5), "한글 ");
        assert_eq!(cut("e\u{301}", 1), "é");
        assert_eq!(clean("कि 👨‍👩‍👧‍👦"), "कि 👨‍👩‍👧‍👦");
        assert_eq!(cut("👨‍👩‍👧‍👦abc", 2), "👨‍👩‍👧‍👦");
        let palette = Palette::new(0, true);
        let mut canvas = Canvas::new(8, 2, &palette);
        canvas.text(0, 0, "한글", Ink::Default);
        canvas.text(1, 0, "x", Ink::Default);
        let row: Vec<_> = canvas
            .buffer
            .content
            .iter()
            .take(4)
            .map(|c| c.symbol())
            .collect();
        assert_eq!(row, [" ", "x", "글", ""]);
        canvas.text(-1, 1, "한a\u{f001}", Ink::Default);
        assert_eq!(canvas.buffer[(1, 1)].symbol(), "a");
        assert_eq!(canvas.buffer[(2, 1)].symbol(), "\u{f001}");
        assert!(
            canvas
                .buffer
                .content
                .iter()
                .all(|c| c.bg == ratatui::style::Color::Reset)
        );
        canvas.text(7, 0, "한", Ink::Default);
        assert_eq!(canvas.buffer[(7, 0)].symbol(), " ");
    }
}
