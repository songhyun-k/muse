use crate::{
    geometry::{Layout, Visual},
    state::{Data, Ui, View},
};

/// One presentation clock. Service records are borrowed, never advanced or mutated.
pub struct Motion {
    pub visual: Visual,
    last: f64,
    size: (u16, u16),
    entry: Option<String>,
    view: View,
    current: Option<crate::generated::Item>,
    track_time: f64,
}

fn approach(value: &mut f64, target: f64, rate: f64, dt: f64, epsilon: f64) -> bool {
    *value += (target - *value) * (1.0 - (-dt * rate).exp());
    if (*value - target).abs() < epsilon {
        *value = target;
    }
    *value != target
}

impl Motion {
    pub fn new(ui: &Ui, data: &Data, now: f64, unix: f64, size: (u16, u16)) -> Self {
        let mut visual = Visual::settled(ui, data, unix);
        visual.now = now;
        visual.panel_widths = Some(if now < 0.35 && !ui.reduced_motion {
            [0.0; 2]
        } else {
            Layout::new(size.0.into(), size.1.into(), ui).panel_widths()
        });
        Self {
            visual,
            last: now,
            size,
            entry: data
                .player
                .as_ref()
                .and_then(|p| p.current_entry_id.clone()),
            view: ui.view,
            current: data.current().cloned(),
            track_time: -100.0,
        }
    }

    pub fn advance(&mut self, ui: &Ui, data: &Data, now: f64, unix: f64, size: (u16, u16)) -> bool {
        let dt = (now - self.last).clamp(0.0, 0.25);
        self.last = now;
        let target = Visual::settled(ui, data, unix);
        let panels = Layout::new(size.0.into(), size.1.into(), ui).panel_widths();
        let v = &mut self.visual;
        let entry = data
            .player
            .as_ref()
            .and_then(|p| p.current_entry_id.clone());
        if entry != self.entry {
            v.position = target.position;
            self.entry = entry;
            v.previous_track = self.current.take();
            self.current = data.current().cloned();
            self.track_time = now;
        }
        if self.view != ui.view {
            v.selections[0] = target.selections[0];
            self.view = ui.view;
        }
        if ui.reduced_motion {
            let art_time = v.art_time;
            *v = target;
            v.now = now;
            v.art_time = art_time;
            v.panel_widths = Some(panels);
            self.size = size;
            return false;
        }
        if self.size != size {
            v.panel_widths = Some(panels);
            self.size = size;
        }
        let mut moving = false;
        for (value, target, rate) in [
            (&mut v.energy, target.energy, 5.0),
            (&mut v.position, target.position, 22.0),
            (&mut v.volume, target.volume, 16.0),
            (&mut v.nav_position, target.nav_position, 16.0),
            (&mut v.lyric_scroll, target.lyric_scroll, 7.0),
            (&mut v.lyric_emphasis, target.lyric_emphasis, 8.0),
        ] {
            moving |= approach(value, target, rate, dt, 0.000001);
        }
        for (value, target) in v.selections.iter_mut().zip(target.selections) {
            moving |= approach(value, target, 16.0, dt, 0.000001);
        }
        for (value, target) in v.panel_widths.as_mut().unwrap().iter_mut().zip(panels) {
            moving |= approach(value, target, 13.0, dt, 0.035);
        }
        v.now = now;
        v.playback_position = target.playback_position;
        v.track_phase = target.track_phase;
        v.cover_blend = crate::geometry::ease((now - self.track_time) / 0.38);
        let playing = data.player.as_ref().is_some_and(|p| p.playing);
        if playing {
            v.art_time += dt;
        }
        moving
            || playing
            || v.cover_blend < 1.0
            || ui
                .pulse
                .as_ref()
                .is_some_and(|(_, since)| now - since < 0.35)
            || ui
                .favorite_pulse
                .as_ref()
                .is_some_and(|(_, since)| now - since < 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_entries_fades_only_the_cover_and_snaps_the_new_playback_clock() {
        let mut ui = Ui::default();
        let mut data = Data::default();
        data.player = serde_json::from_value(serde_json::json!({
            "current":{"ref":{"id":"a","source":"library","kind":"song"},"title":"a","artist":"a","album":"a","duration":100},
            "currentEntryId":"first","playing":false,"position":70,"queueCount":0,"queueRevision":1,
            "updatedAt":1000,"repeatMode":"off","shuffle":false,"canSeek":true
        })).unwrap();
        let mut motion = Motion::new(&ui, &data, 1.0, 1000.0, (140, 40));
        let player = data.player.as_mut().unwrap();
        player.current_entry_id = Some("second".into());
        player.current.as_mut().unwrap().r#ref.id = "b".into();
        player.position = 2.0;
        assert!(motion.advance(&ui, &data, 1.1, 1000.0, (140, 40)));
        assert_eq!(motion.visual.position, 2.0);
        assert_eq!(motion.visual.previous_track.as_ref().unwrap().r#ref.id, "a");
        assert_eq!(motion.visual.cover_blend, 0.0);
        motion.advance(&ui, &data, 1.29, 1000.0, (140, 40));
        assert!((motion.visual.cover_blend - 0.875).abs() < 1e-10);
        ui.reduced_motion = true;
        motion.advance(&ui, &data, 1.3, 1000.0, (140, 40));
        assert_eq!(motion.visual.cover_blend, 1.0);
        assert!(motion.visual.previous_track.is_none());
        assert_eq!(data.player.unwrap().position, 2.0);
    }

    #[test]
    fn deterministic_smoothing_resizes_and_reduced_motion_settle_without_domain_writes() {
        let mut ui = Ui::default();
        let data = Data::default();
        let mut motion = Motion::new(&ui, &data, 0.0, 1000.0, (140, 40));
        assert_eq!(motion.visual.panel_widths, Some([0.0, 0.0]));
        assert!(motion.advance(&ui, &data, 0.1, 1000.1, (140, 40)));
        assert!(
            (motion.visual.panel_widths.unwrap()[0] - 21.0 * (1.0 - (-1.3_f64).exp())).abs()
                < 1e-10
        );
        ui.cursors[0] = 4;
        motion.advance(&ui, &data, 0.2, 1000.2, (140, 40));
        assert!((motion.visual.selections[0] - 4.0 * (1.0 - (-1.6_f64).exp())).abs() < 1e-10);
        for tick in 3..100 {
            motion.advance(&ui, &data, f64::from(tick) / 10.0, 1000.0, (140, 40));
        }
        assert!(!motion.advance(&ui, &data, 10.0, 1000.0, (140, 40)));
        assert_eq!(motion.visual.panel_widths, Some([21.0, 29.0]));
        motion.advance(&ui, &data, 10.1, 1000.0, (80, 24));
        assert_eq!(motion.visual.panel_widths, Some([21.0, 0.0]));
        ui.reduced_motion = true;
        ui.cursors[0] = 8;
        ui.left_open = false;
        assert!(!motion.advance(&ui, &data, 10.2, 1000.0, (140, 40)));
        assert_eq!(motion.visual.selections, [8.0, 0.0]);
        assert_eq!(motion.visual.panel_widths, Some([0.0, 29.0]));
        assert!(data.player.is_none());
    }
}
