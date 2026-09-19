use ratatui::style::{Color, Modifier, Style};
use std::{collections::BTreeMap, ops::Index, sync::LazyLock};

pub const THEMES: [&str; 5] = ["porcelain", "graphite", "linen", "midnight", "ink"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Rgb(u8, u8, u8),
    Default,
    Dim,
}

impl Ink {
    fn parse(value: &str) -> Self {
        match value {
            "terminal" => Self::Default,
            "terminal-dim" => Self::Dim,
            _ => {
                assert!(value.len() == 7 && value.starts_with('#') && value.is_ascii());
                let channel = |i| u8::from_str_radix(&value[i..i + 2], 16).expect("theme RGB");
                Self::Rgb(channel(1), channel(3), channel(5))
            }
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Rgb(r, g, b) => Color::Rgb(r, g, b),
            _ => Color::Reset,
        }
    }

    pub fn style(self) -> Style {
        let style = Style::default().fg(self.color());
        if self == Self::Dim {
            style.add_modifier(Modifier::DIM)
        } else {
            style
        }
    }

    pub fn mix(self, other: Self, amount: f64) -> Self {
        let t = amount.clamp(0.0, 1.0);
        if t == 0.0 {
            return self;
        }
        if let (Self::Rgb(r, g, b), Self::Rgb(x, y, z)) = (self, other) {
            let channel = |a: u8, b: u8| {
                (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round_ties_even() as u8
            };
            Self::Rgb(channel(r, x), channel(g, y), channel(b, z))
        } else {
            // The terminal's wallpaper/color is unknown; never blend it as black.
            other
        }
    }
}

#[derive(Clone)]
pub struct Palette {
    pub name: String,
    colors: BTreeMap<String, Ink>,
}

impl Palette {
    pub fn new(index: usize, transparent: bool) -> Self {
        static SOURCE: LazyLock<serde_json::Value> = LazyLock::new(|| {
            serde_json::from_str(include_str!("../assets/themes.json")).expect("embedded themes")
        });
        let source = &SOURCE["themes"][THEMES[index % THEMES.len()]];
        let mut colors: BTreeMap<String, Ink> = source["colors"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), Ink::parse(value.as_str().unwrap())))
            .collect();
        if transparent {
            for (key, value) in SOURCE["transparent"].as_object().unwrap() {
                assert!(colors.contains_key(key));
                colors.insert(key.clone(), Ink::parse(value.as_str().unwrap()));
            }
        }
        Self {
            name: source["name"].as_str().unwrap().into(),
            colors,
        }
    }
}

impl Index<&str> for Palette {
    type Output = Ink;
    fn index(&self, token: &str) -> &Self::Output {
        &self.colors[token]
    }
}

/// The prototype's nearest cube-or-gray mapping; terminal-owned colors stay inherited.
pub fn ansi256(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    let levels = [0_i32, 95, 135, 175, 215, 255];
    let channels = [r, g, b].map(i32::from);
    let indices =
        channels.map(|channel| (0..6).min_by_key(|&i| (levels[i] - channel).abs()).unwrap());
    let gray = ((f64::from(r) + f64::from(g) + f64::from(b)) / 3.0 - 8.0) / 10.0;
    let gray = gray.clamp(0.0, 23.0).round_ties_even() as i32;
    let gray_distance: i32 = channels
        .iter()
        .map(|channel| (channel - (8 + gray * 10)).pow(2))
        .sum();
    let cube_distance: i32 = channels
        .iter()
        .zip(indices)
        .map(|(channel, i)| (channel - levels[i]).pow(2))
        .sum();
    Color::Indexed(if gray_distance < cube_distance {
        (232 + gray) as u8
    } else {
        (16 + 36 * indices[0] + 6 * indices[1] + indices[2]) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn themes_share_tokens_and_keep_terminal_colors_unknown() {
        let original = Palette::new(0, false);
        for index in 0..5 {
            let palette = Palette::new(index, false);
            assert!(original.colors.keys().eq(palette.colors.keys()));
            let transparent = Palette::new(index, true);
            assert_eq!(transparent["background"].color(), Color::Reset);
            assert_eq!(transparent["text.secondary"], Ink::Dim);
        }
        assert_eq!(
            Ink::Rgb(0, 0, 0).mix(Ink::Rgb(1, 3, 5), 0.5),
            Ink::Rgb(0, 2, 2)
        );
        assert_eq!(Ink::Rgb(1, 2, 3).mix(Ink::Default, 0.5), Ink::Default);
        assert_eq!(ansi256(Color::Rgb(255, 0, 0)), Color::Indexed(196));
        assert_eq!(ansi256(Color::Rgb(128, 128, 128)), Color::Indexed(244));
        assert_eq!(ansi256(Color::Rgb(95, 135, 175)), Color::Indexed(67));
        assert_eq!(ansi256(Color::Reset), Color::Reset);
    }
}
