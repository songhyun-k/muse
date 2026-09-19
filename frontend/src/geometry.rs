use crate::state::{Data, Focus, NAV, Ui};

pub fn ease(value: f64) -> f64 {
    1.0 - (1.0 - value.clamp(0.0, 1.0)).powi(3)
}

/// Signed coordinates allow full-width content to slide beyond the terminal edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x,
            y,
            w: w.max(0),
            h: h.max(0),
        }
    }

    pub fn contains(self, point: (i32, i32)) -> bool {
        point.0 >= self.x
            && point.0 < self.x + self.w
            && point.1 >= self.y
            && point.1 < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub main: Area,
    pub nav: Option<Area>,
    pub side: Option<Area>,
    pub player: Area,
}

impl Layout {
    pub fn new(width: i32, height: i32, ui: &Ui) -> Self {
        let mut left = if ui.left_open { 21 } else { 0 };
        let mut right = if ui.right_open {
            if width >= 130 { 29 } else { 25 }
        } else {
            0
        };
        if width - left - right - 6 < 40 {
            if ui.preferred == Focus::Right && right > 0 {
                left = 0;
            } else {
                right = 0;
            }
        }
        Self::with_widths(width, height, [left, right], false)
    }

    pub fn animated(width: i32, height: i32, sizes: [f64; 2]) -> Self {
        Self::with_widths(
            width,
            height,
            sizes.map(|n| n.round_ties_even() as i32),
            true,
        )
    }

    fn with_widths(width: i32, height: i32, sizes: [i32; 2], sliding: bool) -> Self {
        let [left, right] = sizes;
        let full_left = if sliding { 21 } else { left };
        let full_right = if sliding {
            if width >= 130 { 29 } else { 25 }
        } else {
            right
        };
        Self {
            main: Area::new(left + 3, 4, width - left - right - 6, height - 9),
            nav: (left > 0).then(|| Area::new(left - full_left, 4, full_left, height - 9)),
            side: (right > 0).then(|| Area::new(width - right, 4, full_right, height - 9)),
            player: Area::new(0, height - 5, width, 5),
        }
    }

    pub fn panel_widths(self) -> [f64; 2] {
        [
            self.nav.map_or(0, |r| r.w) as f64,
            self.side.map_or(0, |r| r.w) as f64,
        ]
    }
}

impl Ui {
    pub fn fit(&mut self, width: i32, height: i32) -> Layout {
        let layout = Layout::new(width, height, self);
        if (self.focus == Focus::Nav && layout.nav.is_none())
            || (self.focus == Focus::Right && layout.side.is_none())
        {
            self.focus = Focus::Main;
        }
        layout
    }

    pub fn toggle_panel(&mut self, pane: Focus, layout: Layout) {
        let visible = match pane {
            Focus::Nav => {
                self.left_open = layout.nav.is_none();
                layout.nav.is_some()
            }
            Focus::Right => {
                self.right_open = layout.side.is_none();
                layout.side.is_some()
            }
            Focus::Main => return,
        };
        self.preferred = pane;
        if !visible {
            self.focus = pane;
        } else if self.focus == pane {
            self.focus = Focus::Main;
        }
    }

    pub fn cycle_focus(&mut self, layout: Layout, backwards: bool) {
        let panes: Vec<_> = [
            (Focus::Nav, layout.nav.is_some()),
            (Focus::Main, true),
            (Focus::Right, layout.side.is_some()),
        ]
        .into_iter()
        .filter_map(|(pane, visible)| visible.then_some(pane))
        .collect();
        let index = panes.iter().position(|p| *p == self.focus).unwrap_or(0);
        self.focus = panes[(index + if backwards { panes.len() - 1 } else { 1 }) % panes.len()];
    }
}

/// Interpolated presentation values. They never modify authoritative service data.
#[derive(Clone, Debug)]
pub struct Visual {
    pub now: f64,
    pub art_time: f64,
    pub energy: f64,
    pub position: f64,
    pub playback_position: f64,
    pub track_phase: f64,
    pub volume: f64,
    pub nav_position: f64,
    pub selections: [f64; 2],
    pub lyric_scroll: f64,
    pub lyric_emphasis: f64,
    pub panel_widths: Option<[f64; 2]>,
    pub previous_track: Option<crate::generated::Item>,
    pub cover_blend: f64,
}

impl Visual {
    pub fn settled(ui: &Ui, data: &Data, unix_time: f64) -> Self {
        let position = data.position(unix_time);
        let lyric = data.active_lyric(position).map_or(-1.0, |n| n as f64);
        let active_view = if NAV.contains(&ui.view) {
            ui.view
        } else {
            ui.back
        };
        let nav = if ui.focus == Focus::Nav {
            ui.nav_cursor
        } else {
            NAV.iter().position(|v| *v == active_view).unwrap_or(4)
        };
        Self {
            now: 1.0,
            art_time: 0.0,
            energy: if data.player.as_ref().is_some_and(|p| p.playing) {
                1.0
            } else {
                0.0
            },
            position,
            playback_position: position,
            track_phase: data
                .current()
                .and_then(|current| data.items.iter().position(|i| i.r#ref == current.r#ref))
                .unwrap_or(0) as f64,
            volume: data
                .volume
                .as_ref()
                .filter(|v| v.muted != Some(true))
                .and_then(|v| v.level)
                .unwrap_or(0.0)
                * 100.0,
            nav_position: nav as f64,
            selections: ui.cursors.map(|n| n as f64),
            lyric_scroll: ui.lyric_manual.unwrap_or((lyric - 1.0).max(0.0)),
            lyric_emphasis: lyric,
            panel_widths: None,
            previous_track: None,
            cover_blend: 1.0,
        }
    }
}

impl Data {
    pub fn active_lyric(&self, position: f64) -> Option<usize> {
        let lyrics = self.lyrics.as_ref()?;
        if lyrics.status != crate::generated::LyricsStatus::Synced {
            return None;
        }
        lyrics
            .lines
            .iter()
            .take_while(|line| line.seconds.is_some_and(|t| t <= position - lyrics.offset))
            .count()
            .checked_sub(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resizing_preserves_panel_intent_and_independent_cursors() {
        let mut ui = Ui {
            cursors: [7, 3],
            ..Ui::default()
        };
        let small = ui.fit(80, 24);
        assert!(small.nav.is_some() && small.side.is_none());
        ui.toggle_panel(Focus::Right, small);
        let small = ui.fit(80, 24);
        assert!(small.nav.is_none() && small.side.is_some());
        assert!(ui.left_open && ui.right_open);
        let wide = ui.fit(140, 40);
        assert!(wide.nav.is_some() && wide.side.is_some());
        ui.cycle_focus(wide, true);
        assert_eq!(ui.focus, Focus::Main);
        assert_eq!(ui.cursors, [7, 3]);
    }
}
