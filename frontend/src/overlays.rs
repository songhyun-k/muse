use crate::{
    canvas::cells,
    generated::ErrorCode,
    geometry::{Area, ease},
    icons::Icon,
    requests::Target,
    scene::{Action, Scene},
    state::{Panel, View},
};

impl Scene<'_> {
    pub fn draw(&mut self) {
        self.compose(None);
    }

    pub fn draw_animated(&mut self, effects: &mut crate::effects::Effects) {
        self.compose(Some(effects));
    }

    fn compose(&mut self, effects: Option<&mut crate::effects::Effects>) {
        if !self.chrome() {
            return;
        }
        let failure = self
            .data
            .errors
            .get(&Target::Main)
            .or_else(|| self.data.errors.get(&Target::Bootstrap));
        if let Some(error) = failure.filter(|_| self.data.items.is_empty()) {
            let area = self.heading();
            self.canvas.label(
                area.x + 1,
                area.y + 1,
                &self.ui.message(&error.message),
                self.canvas.palette["text.secondary"],
                false,
                area.w - 2,
            );
            if error.code == ErrorCode::NotAuthorized {
                self.button(
                    area.x + 1,
                    area.y + 3,
                    self.ui.text("접근 허용"),
                    "authorize",
                );
            } else if error.retryable {
                self.button(area.x + 1, area.y + 3, self.ui.text("다시 시도"), "retry");
            }
        } else if self.data.loading.contains(&Target::Main) && self.data.items.is_empty() {
            let area = self.heading();
            self.canvas.text(
                area.x + 1,
                area.y + 1,
                self.ui.text("불러오는 중"),
                self.canvas.palette["text.secondary"],
            );
        } else if self.ui.view.cards() {
            self.cards_view();
            if self.data.items.is_empty() {
                let area = self.layout.main;
                self.canvas.label(
                    area.x + 1,
                    area.y + 3,
                    self.ui.text("표시할 항목이 없습니다"),
                    self.canvas.palette["text.secondary"],
                    false,
                    area.w - 2,
                );
            }
        } else if self.ui.view == View::Lyrics
            && (!self.ui.right_open || self.layout.side.is_none())
        {
            let area = self.heading();
            self.lyrics(area);
        } else if matches!(self.ui.view, View::Detail | View::Lyrics)
            || (self.ui.view == View::Playlists && self.data.detail.is_some())
        {
            self.detail_view();
        } else {
            self.song_view();
        }
        if self.ui.panel == Panel::Queue {
            self.queue_panel();
        } else {
            self.lyrics_panel();
        }
        self.player();
        self.overlay();
        self.dialog();
        if let Some(effects) = effects {
            effects.apply(self.ui, self.data, self.visual.now, &mut self.canvas);
            self.hits
                .retain(|hit| effects.current_content_visible(hit.area, self.visual.now));
        }
        self.tooltip();
    }

    fn overlay(&mut self) {
        let p = self.canvas.palette;
        let width = self.canvas.width();
        let height = self.canvas.height();
        if self.ui.help {
            let settings_hint = self.ui.text(", / I       설정 / 언어");
            let lines = [
                self.ui.text("[ / ]       왼쪽 / 오른쪽 패널 접기"),
                self.ui.text("Tab / S-Tab 영역 이동"),
                self.ui.text("↑/↓ j/k     선택 · 가사 스크롤"),
                self.ui.text("Enter       열기 · 재생 · 가사 따라가기"),
                self.ui.text("Space       재생 / 일시정지"),
                self.ui.text("P / S       상세 전체 재생 / 셔플"),
                self.ui.text("←/→         화면 이동"),
                self.ui.text("/ / o / p   검색 / 상세 / 플레이리스트"),
                self.ui.text("l / Q       가사 / 재생 큐"),
                self.ui.text("f / a / A   즐겨찾기 / 목록 추가 / 큐 추가"),
                self.ui.text("d / J/K / R 제거 / 순서 / 이름 변경"),
                self.ui.text("n/b · H/L   곡 / 재생 위치 이동"),
                self.ui.text("+/- m s r   음량 · 음소거 · 셔플 · 반복"),
                self.ui.text("1–5 / T     테마 선택 / 다음 테마"),
                self.ui.text("v           터미널 배경 사용"),
                self.ui.text("t / z       둘러보기 / 움직임 줄이기"),
                settings_hint,
                self.ui.text("Esc / q     뒤로 / 종료"),
            ];
            let bw = 60.min(width - 6);
            let bh = (lines.len() as i32 + 4).min(height - 2);
            let x = (width - bw) / 2;
            let y = (height - bh) / 2;
            self.canvas.fill(x, y, bw, bh, p["overlay.background"]);
            self.canvas.rule(x, y, bw, p["overlay.accent"]);
            self.canvas.label(
                x + 3,
                y + 1,
                self.ui.text("단축키"),
                p["overlay.accent"],
                true,
                bw - 6,
            );
            for (i, line) in lines.iter().take((bh - 4) as usize).enumerate() {
                self.canvas.label(
                    x + 3,
                    y + 3 + i as i32,
                    line,
                    p["overlay.text"],
                    false,
                    bw - 6,
                );
                if *line == settings_hint {
                    self.hit(
                        Area::new(x + 3, y + 3 + i as i32, bw - 6, 1),
                        Action::Key(","),
                    );
                }
            }
        } else if let Some(editor) = &self.ui.editor {
            let prefix = format!("{}  /  ", self.ui.text(editor.action.label()));
            let text = format!("{prefix}{}", editor.visible(width - 28 - cells(&prefix)));
            self.canvas
                .fill(1, height - 6, width - 2, 1, p["control.background"]);
            self.canvas
                .label(2, height - 6, &text, p["accent"], false, width - 28);
            self.canvas.text(
                width - 23,
                height - 6,
                self.ui.text("Enter 확인  Esc 닫기"),
                p["text.secondary"],
            );
            if let Some((message, time)) = &self.ui.toast
                && self.visual.now - time < 2.4
            {
                self.canvas.label(
                    2,
                    height - 7,
                    &self.ui.message(message),
                    p["accent"],
                    false,
                    width - 4,
                );
            }
        } else if let Some((message, time)) = &self.ui.toast {
            let life = self.visual.now - time;
            if life < 2.4 {
                let alpha = if self.ui.reduced_motion {
                    1.0
                } else {
                    ease(life / 0.18).min(((2.4 - life) / 0.4).clamp(0.0, 1.0))
                };
                self.toast(message, alpha, self.icon(Icon::Check));
            }
        } else if let Some((_, error)) = self.data.notification_error() {
            self.toast(&error.message, 1.0, "!");
        }
    }

    fn toast(&mut self, text: &str, alpha: f64, icon: &str) {
        let text = self.ui.message(text);
        let p = self.canvas.palette;
        let bw = (cells(&text) + 4).min(self.canvas.width() - 6);
        let x = (self.canvas.width() - bw) / 2;
        self.capsule(
            Area::new(x - 1, self.canvas.height() - 6, bw + 3, 1),
            p["control.background"],
            p["background"].mix(p["accent"], alpha),
            &format!(" {icon} {text}"),
            false,
        );
    }

    fn tooltip(&mut self) {
        let Some((hx, hy)) = self.ui.hover else {
            return;
        };
        if self.visual.now - self.ui.hover_since < 0.55
            || self.ui.help
            || self.ui.editor.is_some()
            || self.ui.dialog.is_some()
        {
            return;
        }
        let key = self.hits.iter().rev().find_map(|hit| match hit.action {
            Action::Key(key) if self.hovered(hit.area) => Some(key),
            _ => None,
        });
        let text = match key {
            Some("[") => self.ui.text("탐색 접기 / 펼치기"),
            Some("]") => self.ui.text("가사·큐 접기 / 펼치기"),
            Some("l") => self.ui.text("가사"),
            Some("Q") => self.ui.text("재생 큐"),
            Some("m") => self.ui.text("음소거"),
            Some("T") => "",
            Some("v") => self.ui.text("터미널 배경"),
            Some(",") => self.ui.text("설정"),
            Some("/") => self.ui.text("검색"),
            Some(" ") => self.ui.text("재생 / 일시정지"),
            Some("s") => self.ui.text("셔플"),
            Some("r") => self.ui.text("반복"),
            Some("n") => self.ui.text("다음 곡"),
            Some("b") => self.ui.text("이전 곡"),
            _ => return,
        };
        let text = if key == Some("T") {
            format!(
                "{} · {}",
                self.canvas.palette.name,
                self.ui.text("다음 테마")
            )
        } else {
            text.into()
        };
        let w = cells(&text) + 4;
        let x = (hx - w / 2).clamp(1, self.canvas.width() - w - 1);
        let y = if hy < self.canvas.height() - 7 {
            hy + 2
        } else {
            hy - 2
        };
        let p = self.canvas.palette;
        self.capsule(
            Area::new(x, y, w, 1),
            p["overlay.background"],
            p["overlay.text"],
            &format!(" {text}"),
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        art::ArtCache,
        geometry::Visual,
        state::{Data, Dialog, Ui},
        theme::Palette,
    };
    #[test]
    fn failed_lyric_changes_keep_previous_lyrics_and_render_the_failure() {
        use crate::{app::App, generated::*, transport::Admission};
        let mut app = App::default();
        let item: Item = serde_json::from_value(serde_json::json!({
            "ref":{"id":"song","source":"catalog","kind":"song"},
            "title":"song","artist":"a","album":"a"
        }))
        .unwrap();
        app.data.player = Some(
            serde_json::from_value(serde_json::json!({
                "current":item,"playing":false,"position":0,"queueCount":0,
                "queueRevision":1,"updatedAt":0,"repeatMode":"off","shuffle":false,"canSeek":false
            }))
            .unwrap(),
        );
        let previous = Lyrics {
            item: item.r#ref.clone(),
            match_id: Some(1),
            status: LyricsStatus::Synced,
            lines: vec![LyricLine {
                seconds: Some(0.0),
                text: "이전 가사".into(),
            }],
            offset: 0.0,
        };
        app.data.lyrics = Some(previous.clone());
        for (index, code) in [ErrorCode::Network, ErrorCode::Storage]
            .into_iter()
            .enumerate()
        {
            if index == 0 {
                app.data.matches = Some(LyricsMatches {
                    item: item.r#ref.clone(),
                    matches: vec![LyricMatch {
                        id: 2,
                        title: item.title.clone(),
                        artist: item.artist.clone(),
                        album: item.album.clone(),
                        duration: None,
                        synced: true,
                    }],
                });
                app.ui.dialog = Some(Dialog::Lyrics { cursor: 0 });
                app.choose_dialog();
            } else {
                app.edit(
                    crate::state::EditAction::Offset(item.r#ref.clone()),
                    "2".into(),
                );
                app.commit_edit().unwrap();
            }
            let mut id = 0;
            app.flush(|r| {
                if index == 0 {
                    assert!(matches!(&r.command, Command::LyricsChoose(p)
                        if p.item == item.r#ref && p.match_id == 2));
                }
                id = r.id;
                Ok(Admission::Accepted)
            })
            .unwrap();
            app.receive(Event {
                version: 1,
                id: Some(id),
                sequence: index as u64 + 1,
                event: Notice::Failure(Failure {
                    code,
                    message: "변경을 저장하지 못했습니다".into(),
                    retryable: true,
                }),
            });
            assert_eq!(app.data.lyrics, Some(previous.clone()));
            let visual = Visual::settled(&app.ui, &app.data, 0.0);
            let palette = Palette::new(0, false);
            let mut cache = ArtCache::default();
            let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut cache, 140, 40);
            scene.draw();
            let text: String = scene
                .canvas
                .buffer
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(text.contains("변경을 저장하지 못했습니다"));
            assert!(text.contains("이전 가사"));
            app.key("esc", scene.layout, 1.0, 1.0);
            assert!(app.data.notification_error().is_none());
        }
    }

    #[test]
    fn loading_has_its_own_visible_state() {
        let ui = Ui::default();
        let mut data = Data::default();
        data.begin(Target::Main, 0);
        let visual = Visual::settled(&ui, &data, 0.0);
        let palette = Palette::new(0, false);
        let mut cache = ArtCache::default();
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut cache, 140, 40);
        scene.draw();
        let text: String = scene
            .canvas
            .buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("불러오는 중"));
        assert!(!text.contains("표시할 곡이 없습니다"));
    }
}
