use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap, sync::LazyLock};

static ENGLISH: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/en.json")).expect("valid English catalog")
});

/// Presentation-only preference. Song titles, lyrics and user text are never translated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "en")]
    English,
}

impl Language {
    pub fn detect() -> Self {
        let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok())
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        Self::from_locale(&locale)
    }

    pub fn from_locale(locale: &str) -> Self {
        if locale.split(['_', '-', '.']).next() == Some("ko") {
            Self::Korean
        } else {
            Self::English
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "ko" => Ok(Self::Korean),
            "en" => Ok(Self::English),
            _ => Err("--language must be en or ko"),
        }
    }

    pub fn text(self, source: &str) -> &str {
        if self == Self::English {
            ENGLISH.get(source).map_or(source, String::as_str)
        } else {
            source
        }
    }

    pub fn unit(self, count: usize, songs: bool) -> &'static str {
        match (self, songs, count) {
            (Self::Korean, true, _) => "곡",
            (Self::Korean, false, _) => "개",
            (Self::English, true, 1) => " song",
            (Self::English, true, _) => " songs",
            (Self::English, false, 1) => " item",
            (Self::English, false, _) => " items",
        }
    }

    /// Translate known application errors, preserving OS details after the colon.
    pub fn message(self, source: &str) -> Cow<'_, str> {
        let translated = self.text(source);
        if translated != source || self == Self::Korean {
            return Cow::Borrowed(translated);
        }
        if let Some((prefix, detail)) = source.split_once(": ")
            && self.text(prefix) != prefix
        {
            return Cow::Owned(format!("{}: {}", self.text(prefix), self.message(detail)));
        }
        Cow::Borrowed(source)
    }
}

impl crate::state::Ui {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        self.language.text(source)
    }

    pub fn message<'a>(&self, source: &'a str) -> Cow<'a, str> {
        self.language.message(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_views_and_feedback_do_not_translate_music_metadata() {
        use crate::{
            art::ArtCache,
            geometry::Visual,
            scene::Scene,
            state::{Data, Ui},
            theme::Palette,
        };
        let render = |ui: &Ui, data: &Data, width, height| {
            let visual = Visual::settled(ui, data, 1000.0);
            let palette = Palette::new(ui.theme, ui.transparent);
            let mut cache = ArtCache::default();
            let mut scene = Scene::new(ui, data, &visual, &palette, &mut cache, width, height);
            scene.draw();
            scene
                .canvas
                .buffer
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        let mut ui = Ui {
            language: Language::English,
            ..Ui::default()
        };
        let mut data = Data::default();
        data.items.push(
            serde_json::from_value(serde_json::json!({
                "ref":{"source":"library","kind":"song","id":"metadata"},
                "title":"보관함", "artist":"가사", "album":"노래"
            }))
            .unwrap(),
        );
        ui.toast = Some(("음악 앱에 로그인해주세요".into(), 0.0));
        let screen = render(&ui, &data, 140, 40);
        assert!(screen.contains("보관함") && screen.contains("가사"));
        assert!(screen.contains("Sign in to the Music app"));
        assert!(!screen.contains("음악 앱에 로그인해주세요"));
    }

    #[test]
    fn locales_translate_application_text_and_preserve_unknown_content() {
        assert_eq!(Language::from_locale("ko_KR.UTF-8"), Language::Korean);
        assert_eq!(Language::from_locale("ko-KR"), Language::Korean);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::English);
        assert_eq!(Language::from_locale("C"), Language::English);
        assert!(Language::parse("fr").is_err());
        assert_eq!(Language::English.text("보관함"), "Library");
        assert_eq!(Language::Korean.text("보관함"), "보관함");
        assert_eq!(Language::English.text("낯선 제목"), "낯선 제목");
        assert_eq!(
            Language::English.message("설정을 저장하지 못했습니다: disk full"),
            "Could not save settings: disk full"
        );
        assert!(ENGLISH.values().all(|value| !value.trim().is_empty()));
    }
}
