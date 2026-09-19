use crate::{
    canvas::{clean, cut},
    generated::LyricsStatus,
    geometry::Area,
    icons::Icon,
    requests::Target,
    scene::{Action, Scene},
    theme::Ink,
};

impl Scene<'_> {
    pub fn lyrics(&mut self, area: Area) {
        let Area { x, y, w, h } = area;
        let p = self.canvas.palette;
        let Some(lyrics) = &self.data.lyrics else {
            let text = if self.data.current().is_none() {
                self.ui.text("재생 중인 곡이 없습니다")
            } else if self.data.errors.contains_key(&Target::Lyrics) {
                self.ui.text("가사를 불러오지 못했어요")
            } else {
                self.ui.text("가사를 불러오는 중")
            };
            self.lyrics_caption(area, text, self.data.current().is_some());
            return;
        };
        match lyrics.status {
            LyricsStatus::Missing => {
                self.lyrics_caption(area, self.ui.text("가사를 찾지 못했어요"), true);
                return;
            }
            LyricsStatus::Instrumental => {
                self.lyrics_caption(area, self.ui.text("연주곡"), false);
                return;
            }
            _ => {}
        }
        let active = self.data.active_lyric(self.visual.playback_position);
        for (i, line) in lyrics.lines.iter().enumerate() {
            let yy = (f64::from(y) + (i as f64 - self.visual.lyric_scroll) * 3.0).round_ties_even()
                as i32;
            if yy < y || yy >= y + h {
                continue;
            }
            let distance = active.map_or(0, |n| n.abs_diff(i));
            let mut color = p[if distance < 3 {
                "lyrics.inactive"
            } else {
                "lyrics.distant"
            }];
            let strength = if active.is_some() {
                (-(i as f64 - self.visual.lyric_emphasis).powi(2) * 3.0).exp()
            } else {
                0.0
            };
            if matches!((color, p["lyrics.active"]), (Ink::Rgb(..), Ink::Rgb(..))) {
                color = color.mix(p["lyrics.active"], strength);
            } else if strength > 0.5 {
                color = p["lyrics.active"];
            }
            let line = clean(&line.text);
            let (text, rest) = split_line(&line, w - 2);
            let bold = active == Some(i);
            self.canvas.label(x + 1, yy, text, color, bold, w - 2);
            if bold {
                self.canvas.text(x, yy, "▎", p["accent"]);
            }
            if !rest.is_empty() && yy + 1 < y + h {
                self.canvas.label(x + 1, yy + 1, rest, color, bold, w - 2);
            }
        }
    }

    fn lyrics_caption(&mut self, area: Area, text: &str, search: bool) {
        let p = self.canvas.palette;
        self.canvas.label(
            area.x + 1,
            area.y,
            text,
            p["text.secondary"],
            false,
            area.w - 2,
        );
        if search && area.h >= 3 {
            self.canvas.label(
                area.x + 1,
                area.y + 2,
                self.ui.text("M 가사 찾기"),
                p["accent"],
                false,
                area.w - 2,
            );
            self.hit(
                Area::new(area.x + 1, area.y + 2, area.w - 2, 1),
                Action::Key("M"),
            );
        }
    }

    pub fn lyrics_panel(&mut self) {
        let Some(Area { x, y, w, h }) = self.layout.side else {
            return;
        };
        let p = self.canvas.palette;
        self.canvas.label(
            x + 3,
            y,
            self.data.current().map_or("", |i| &i.title),
            p["text.secondary"],
            false,
            w - 7,
        );
        self.spectrum(x + 3, y + 2, 12.min(w - 7));
        self.lyrics(Area::new(x + 2, y + 5, w - 5, h - 6));
        if self.ui.lyric_manual.is_some() {
            self.canvas.label(
                x + 3,
                y + h - 1,
                &format!(
                    "{} Enter {}",
                    self.icon(Icon::Play),
                    self.ui.text("따라가기")
                ),
                p["text.secondary"],
                false,
                w - 6,
            );
            self.hit(Area::new(x + 3, y + h - 1, w - 6, 1), Action::Key("enter"));
        }
    }
}

fn split_line(line: &str, width: i32) -> (&str, &str) {
    let mut end = cut(line, width).len();
    if end < line.len() && !line[end..].starts_with(char::is_whitespace) {
        end = line[..end]
            .rfind(char::is_whitespace)
            .filter(|n| *n > 0)
            .unwrap_or(end);
    }
    (line[..end].trim_end(), line[end..].trim_start())
}

#[cfg(test)]
mod tests {
    #[test]
    fn wrapped_lyrics_keep_words_and_graphemes_intact() {
        assert_eq!(
            super::split_line("We leave the window open", 22),
            ("We leave the window", "open")
        );
        assert_eq!(super::split_line("하나 둘 셋", 5), ("하나", "둘 셋"));
        assert_eq!(super::split_line("👨‍👩‍👧‍👦 together", 2), ("👨‍👩‍👧‍👦", "together"));
    }
    use crate::{
        generated::{LyricLine, Lyrics, LyricsStatus},
        state::Data,
    };
    #[test]
    fn timed_lyrics_respect_offsets_without_fabricating_plain_timestamps() {
        let mut data = Data::default();
        let item = serde_json::from_str(r#"{"id":"a","source":"library","kind":"song"}"#).unwrap();
        data.lyrics = Some(Lyrics {
            item,
            match_id: None,
            status: LyricsStatus::Synced,
            offset: 2.0,
            lines: vec![
                LyricLine {
                    seconds: Some(5.0),
                    text: "첫 줄".into(),
                },
                LyricLine {
                    seconds: Some(10.0),
                    text: "다음 줄".into(),
                },
            ],
        });
        assert_eq!(data.active_lyric(6.9), None);
        assert_eq!(data.active_lyric(7.0), Some(0));
        assert_eq!(data.active_lyric(12.0), Some(1));
        data.lyrics.as_mut().unwrap().status = LyricsStatus::Plain;
        assert_eq!(data.active_lyric(100.0), None);
    }
}
