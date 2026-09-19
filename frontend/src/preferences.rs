use crate::{
    i18n::Language,
    state::{Focus, Panel, Ui},
    theme::THEMES,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Settings {
    version: u8,
    language: Option<Language>,
    theme: String,
    transparent: bool,
    plain_icons: bool,
    reduced_motion: bool,
    left_open: bool,
    right_open: bool,
    queue: bool,
    prefer_right: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let mut settings = Self::from(&Ui::default());
        settings.language = None;
        settings
    }
}

impl From<&Ui> for Settings {
    fn from(ui: &Ui) -> Self {
        Self {
            version: 1,
            language: Some(ui.language),
            theme: THEMES[ui.theme % THEMES.len()].into(),
            transparent: ui.transparent,
            plain_icons: ui.plain_icons,
            reduced_motion: ui.reduced_motion,
            left_open: ui.left_open,
            right_open: ui.right_open,
            queue: ui.panel == Panel::Queue,
            prefer_right: ui.preferred == Focus::Right,
        }
    }
}

pub struct Preferences {
    path: Option<PathBuf>,
    saved: Settings,
    pub warning: Option<String>,
}

impl Preferences {
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut result = Self {
            path,
            saved: Settings::default(),
            warning: None,
        };
        if let Some(path) = &result.path {
            match read(path) {
                Ok(settings) => result.saved = settings,
                Err(error) => {
                    // Never overwrite an unreadable, newer or corrupt preferences file.
                    result.warning =
                        Some(format!("설정을 읽을 수 없어 기본값을 사용합니다: {error}"));
                    result.path = None;
                }
            }
        }
        result
    }

    pub fn ui(&self) -> Ui {
        let s = &self.saved;
        Ui {
            language: s.language.unwrap_or_else(Language::detect),
            theme: THEMES.iter().position(|t| *t == s.theme).unwrap_or(0),
            transparent: s.transparent,
            plain_icons: s.plain_icons,
            reduced_motion: s.reduced_motion,
            left_open: s.left_open,
            right_open: s.right_open,
            panel: if s.queue { Panel::Queue } else { Panel::Lyrics },
            preferred: if s.prefer_right {
                Focus::Right
            } else {
                Focus::Nav
            },
            ..Ui::default()
        }
    }

    pub fn save(&mut self, ui: &Ui) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let settings = Settings::from(ui);
        if settings == self.saved {
            return Ok(());
        }
        if let Err(error) = write(path, &settings) {
            self.path = None;
            return Err(format!("설정을 저장하지 못했습니다: {error}"));
        }
        self.saved = settings;
        Ok(())
    }
}

fn read(path: &Path) -> Result<Settings, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(8193)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err("설정 파일이 너무 큽니다".into());
    }
    let settings: Settings = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if settings.version != 1 || !THEMES.contains(&settings.theme.as_str()) {
        return Err("지원하지 않는 설정입니다".into());
    }
    Ok(settings)
}

fn write(path: &Path, settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    let directory = path.parent().ok_or("설정 경로가 없습니다")?;
    fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".ui-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec_pretty(settings)?)?;
        file.sync_all()?;
        // ponytail: UI preferences use atomic last-writer-wins across concurrent sessions.
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_without_domain_state_and_preserve_corrupt_files() {
        let directory = std::env::temp_dir().join(format!("muse-prefs-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ui.json");
        let mut prefs = Preferences::open(Some(path.clone()));
        let mut ui = Ui {
            language: Language::English,
            theme: 3,
            transparent: true,
            right_open: false,
            ..Ui::default()
        };
        prefs.save(&ui).unwrap();
        let restored = Preferences::open(Some(path.clone())).ui();
        assert_eq!(restored.theme, 3);
        assert_eq!(restored.language, Language::English);
        assert!(restored.transparent && !restored.right_open);
        let bytes = fs::read(&path).unwrap();
        let mut old: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        old.as_object_mut().unwrap().remove("language");
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(Preferences::open(Some(path.clone())).warning.is_none());
        assert!(!String::from_utf8_lossy(&bytes).contains("query"));
        fs::write(&path, b"broken").unwrap();
        let mut invalid = Preferences::open(Some(path.clone()));
        assert!(invalid.warning.is_some());
        ui.theme = 2;
        invalid.save(&ui).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"broken");
        fs::write(&path, &bytes).unwrap();
        fs::write(
            directory.join(format!(".ui-{}.tmp", std::process::id())),
            b"occupied",
        )
        .unwrap();
        assert!(prefs.save(&ui).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir_all(directory).unwrap();
    }
}
