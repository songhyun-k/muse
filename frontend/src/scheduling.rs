use std::time::Duration;

pub struct FrameClock {
    interval: f64,
    drawn: f64,
    settling: bool,
}

impl FrameClock {
    pub fn new(fps: u32) -> Self {
        Self {
            interval: 1.0 / f64::from(fps),
            drawn: -1.0,
            settling: false,
        }
    }

    fn deadline(&self, dirty: bool, moving: bool, playing: bool) -> Option<f64> {
        if dirty || moving || self.settling {
            Some(self.drawn + self.interval)
        } else if playing {
            Some(self.drawn + 1.0)
        } else {
            None
        }
    }

    pub fn due(&self, now: f64, dirty: bool, moving: bool, playing: bool) -> bool {
        self.deadline(dirty, moving, playing)
            .is_some_and(|t| now >= t)
    }

    pub fn drawn(&mut self, now: f64, moving: bool) {
        self.drawn = now;
        // Draw once more after animation ends, including its exact final frame.
        self.settling = moving;
    }

    pub fn wait(&self, now: f64, dirty: bool, moving: bool, playing: bool) -> Duration {
        Duration::from_secs_f64(
            self.deadline(dirty, moving, playing)
                .map_or(0.1, |t| (t - now).clamp(0.0, 0.1)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_are_bounded_and_stop_only_after_the_final_frame() {
        let mut clock = FrameClock::new(60);
        assert!(clock.due(0.0, true, false, false));
        clock.drawn(0.0, true);
        assert!(!clock.due(0.001, true, true, false));
        assert!(clock.due(0.017, false, false, false));
        clock.drawn(0.017, false);
        assert!(!clock.due(2.0, false, false, false));
        assert_eq!(
            clock.wait(2.0, false, false, false),
            Duration::from_millis(100)
        );
        assert!(!clock.due(0.5, false, false, true));
        assert!(clock.due(1.1, false, false, true));
        let mut frames = 0;
        let mut clock = FrameClock::new(30);
        for tick in 0..1000 {
            let now = f64::from(tick) / 1000.0;
            if clock.due(now, true, true, false) {
                frames += 1;
                clock.drawn(now, true);
            }
        }
        assert!(frames <= 30);
    }
}
