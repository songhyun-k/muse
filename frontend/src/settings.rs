use crate::{
    app::App,
    i18n::Language,
    state::{Choice, Dialog, Focus, MenuRow, Panel, Ui},
    theme::THEMES,
};

#[derive(Clone, Copy)]
pub enum Setting {
    Theme,
    Language,
    Background,
    Icons,
    Motion,
    Navigation,
    SidePanel,
    SideContent,
}

impl Setting {
    pub const ALL: [Self; 8] = [
        Self::Theme,
        Self::Language,
        Self::Background,
        Self::Icons,
        Self::Motion,
        Self::Navigation,
        Self::SidePanel,
        Self::SideContent,
    ];

    pub fn row(self, ui: &Ui) -> MenuRow {
        let toggle = |on| if on { "켜짐" } else { "꺼짐" };
        let (label, value) = match self {
            Self::Theme => ("테마", THEMES[ui.theme % THEMES.len()]),
            Self::Language => (
                "언어",
                match ui.language {
                    Language::Korean => "한국어",
                    Language::English => "English",
                },
            ),
            Self::Background => (
                "배경",
                if ui.transparent {
                    "터미널"
                } else {
                    "테마"
                },
            ),
            Self::Icons => (
                "아이콘",
                if ui.plain_icons {
                    "기본"
                } else {
                    "Nerd Font"
                },
            ),
            Self::Motion => ("움직임 줄이기", toggle(ui.reduced_motion)),
            Self::Navigation => ("탐색 패널", toggle(ui.left_open)),
            Self::SidePanel => ("가사·큐 패널", toggle(ui.right_open)),
            Self::SideContent => (
                "패널 내용",
                if ui.panel == Panel::Lyrics {
                    "가사"
                } else {
                    "재생 큐"
                },
            ),
        };
        MenuRow {
            label: ui.text(label).into(),
            detail: ui.text(value).into(),
            checked: false,
            choice: Choice::Setting(self),
        }
    }

    pub fn apply(self, app: &mut App, direction: i32) {
        let ui = &mut app.ui;
        match self {
            Self::Theme => {
                ui.theme = (ui.theme as i32 + direction).rem_euclid(THEMES.len() as i32) as usize;
            }
            Self::Language => {
                ui.language = match ui.language {
                    Language::Korean => Language::English,
                    Language::English => Language::Korean,
                };
            }
            Self::Background => ui.transparent = !ui.transparent,
            Self::Icons => ui.plain_icons = !ui.plain_icons,
            Self::Motion => ui.reduced_motion = !ui.reduced_motion,
            Self::Navigation => {
                ui.left_open = !ui.left_open;
                if ui.left_open {
                    ui.preferred = Focus::Nav;
                }
            }
            Self::SidePanel => {
                ui.right_open = !ui.right_open;
                if ui.right_open {
                    ui.preferred = Focus::Right;
                    app.leave_full_lyrics();
                }
            }
            Self::SideContent => {
                ui.panel = if ui.panel == Panel::Lyrics {
                    Panel::Queue
                } else {
                    Panel::Lyrics
                };
                ui.lyric_manual = None;
            }
        }
        app.related();
    }
}

impl App {
    pub fn settings_menu(&mut self) {
        self.ui.help = false;
        self.ui.dialog = Some(Dialog::Settings { cursor: 0 });
    }

    pub fn adjust_setting(&mut self, direction: i32) {
        if let Some(Dialog::Settings { cursor }) = self.ui.dialog
            && let Some(setting) = Setting::ALL.get(cursor)
        {
            setting.apply(self, direction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        art::ArtCache,
        geometry::Visual,
        mouse::Mouse,
        preferences::Preferences,
        scene::{Action, Scene},
        theme::Palette,
    };
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn settings_apply_live_stay_modal_and_persist_in_both_languages() {
        let mut app = App::default();
        let layout = app.ui.fit(140, 40);
        app.key(",", layout, 1.0, 1.0);
        app.key("left", layout, 1.0, 1.0);
        assert_eq!(app.ui.theme, THEMES.len() - 1);
        app.key("enter", layout, 1.0, 1.0);
        assert_eq!(app.ui.theme, 0);
        for key in [
            "tab", "right", "down", " ", "down", "enter", "down", "enter",
        ] {
            app.key(key, layout, 1.0, 1.0);
        }
        assert_eq!(app.ui.language, Language::English);
        assert!(app.ui.transparent && app.ui.plain_icons && app.ui.reduced_motion);
        assert!(matches!(
            app.ui.dialog,
            Some(Dialog::Settings { cursor: 4 })
        ));
        for language in [Language::English, Language::Korean] {
            for theme in 0..THEMES.len() {
                for transparent in [false, true] {
                    for (width, height) in [(80, 24), (140, 40)] {
                        let mut ui = app.ui.clone();
                        ui.language = language;
                        ui.theme = theme;
                        ui.transparent = transparent;
                        let visual = Visual::settled(&ui, &app.data, 0.0);
                        let palette = Palette::new(theme, transparent);
                        let mut art = ArtCache::default();
                        let mut scene =
                            Scene::new(&ui, &app.data, &visual, &palette, &mut art, width, height);
                        scene.draw();
                        let text: String = scene
                            .canvas
                            .buffer
                            .content
                            .iter()
                            .map(|c| c.symbol())
                            .collect();
                        assert!(
                            text.contains(ui.text("설정")) && text.contains(ui.text("패널 내용"))
                        );
                        if language == Language::English {
                            assert!(!text.chars().any(|c| ('가'..='힣').contains(&c)));
                        }
                        assert_eq!(
                            scene
                                .hits
                                .iter()
                                .filter(|h| matches!(h.action, Action::Dialog(_)))
                                .count(),
                            8
                        );
                        assert!(
                            scene.hits.iter().all(|h| matches!(
                                h.action,
                                Action::Dialog(_) | Action::Key("esc")
                            ))
                        );
                        assert!(
                            scene
                                .hits
                                .iter()
                                .all(|h| h.area.y + h.area.h <= i32::from(height))
                        );
                    }
                }
            }
        }
        let visual = Visual::settled(&app.ui, &app.data, 0.0);
        let palette = Palette::new(app.ui.theme, app.ui.transparent);
        let mut art = ArtCache::default();
        let mut scene = Scene::new(&app.ui, &app.data, &visual, &palette, &mut art, 80, 24);
        scene.draw();
        let hits = scene.hits;
        let mut mouse = Mouse::default();
        for action in [Action::Dialog(5), Action::Key("esc")] {
            let area = hits
                .iter()
                .find(|h| match (&h.action, &action) {
                    (Action::Dialog(a), Action::Dialog(b)) => a == b,
                    (Action::Key(a), Action::Key(b)) => a == b,
                    _ => false,
                })
                .unwrap()
                .area;
            mouse.handle(
                &mut app,
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: area.x as u16,
                    row: area.y as u16,
                    modifiers: KeyModifiers::NONE,
                },
                layout,
                &hits,
                1.0,
                1.0,
            );
        }
        assert!(!app.ui.left_open && app.ui.dialog.is_none());
        let directory = std::env::temp_dir().join(format!("muse-settings-{}", std::process::id()));
        let path = directory.join("ui.json");
        Preferences::open(Some(path.clone())).save(&app.ui).unwrap();
        let restored = Preferences::open(Some(path)).ui();
        assert_eq!(restored.language, Language::English);
        assert!(restored.transparent && restored.plain_icons && restored.reduced_motion);
        assert!(!restored.left_open);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
