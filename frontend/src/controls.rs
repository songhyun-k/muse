use crate::{
    app::App,
    generated::*,
    requests::Target,
    state::{Focus, View},
};

impl App {
    pub(crate) fn focused_detail(&self) -> Option<&ItemRef> {
        self.detail_ref.as_ref().filter(|_| {
            self.ui.focus == Focus::Main && matches!(self.ui.view, View::Detail | View::Playlists)
        })
    }

    pub fn selected(&self) -> Option<&Item> {
        if self.ui.focus == Focus::Nav {
            None
        } else if self.ui.queue_focus() {
            if !self.selection_ready(&Target::Queue) {
                return None;
            }
            self.data
                .queue
                .as_ref()?
                .entries
                .get(self.ui.cursors[1])?
                .item
                .as_ref()
        } else if self.ui.lyrics_focus() {
            self.data.current()
        } else if self.selection_ready(&Target::Main) {
            self.data.items.get(self.ui.cursors[0])
        } else {
            None
        }
    }

    pub fn activate(&mut self) {
        if self.ui.queue_focus() {
            if self.data.errors.contains_key(&Target::Queue) {
                self.queue_page(0);
                return;
            }
            if !self.selection_ready(&Target::Queue) {
                return;
            }
            if let Some(entry) = self
                .data
                .queue
                .as_ref()
                .and_then(|q| q.entries.get(self.ui.cursors[1]))
            {
                self.send(
                    Command::QueueJump(QueueJump {
                        entry_id: entry.id.clone(),
                    }),
                    Target::Mutation,
                    0,
                );
            }
        } else if self.ui.lyrics_focus() {
            self.ui.lyric_manual = None;
        } else if let Some(item) = self.selected().cloned() {
            if matches!(item.r#ref.kind, Kind::Song | Kind::Station) {
                self.play_selected(Placement::Replace);
            } else {
                self.open(item.r#ref);
            }
        }
    }

    pub fn play_selected(&mut self, placement: Placement) {
        if let Some(item) = self.selected().cloned() {
            self.send(
                Command::Play(PlayParams {
                    items: vec![item.r#ref],
                    start_index: 0,
                    placement,
                    shuffle: None,
                }),
                Target::Mutation,
                0,
            );
        }
    }

    pub fn play_detail(&mut self, shuffle: bool) {
        if let Some(item) = self.focused_detail().cloned() {
            self.send(
                Command::Play(PlayParams {
                    items: vec![item],
                    start_index: 0,
                    placement: Placement::Replace,
                    shuffle: Some(shuffle),
                }),
                Target::Mutation,
                0,
            );
        }
    }

    pub fn control(&mut self, action: Control) {
        self.send(
            Command::Control(ControlParams { action }),
            Target::Mutation,
            0,
        );
    }

    pub fn toggle_shuffle(&mut self) {
        if let Some(mut mode) = self.mode_intent() {
            mode.shuffle = !mode.shuffle;
            self.set_mode(mode);
        }
    }

    pub fn cycle_repeat(&mut self) {
        if let Some(mut mode) = self.mode_intent() {
            mode.repeat_mode = match mode.repeat_mode {
                RepeatMode::Off => RepeatMode::One,
                RepeatMode::One => RepeatMode::All,
                RepeatMode::All => RepeatMode::Off,
            };
            self.set_mode(mode);
        }
    }

    fn mode_intent(&self) -> Option<ModeParams> {
        self.wanted_mode.clone().or_else(|| {
            self.data
                .player
                .as_ref()
                .filter(|p| p.current.is_some())
                .map(|p| ModeParams {
                    shuffle: p.shuffle,
                    repeat_mode: p.repeat_mode,
                })
        })
    }

    fn set_mode(&mut self, mode: ModeParams) {
        self.wanted_mode = self
            .send(Command::Mode(mode.clone()), Target::Mode, 0)
            .then_some(mode);
    }

    pub fn seek(&mut self, seconds: f64) {
        let duration = self.data.current().and_then(|i| i.duration);
        if !seconds.is_finite() || !self.data.player.as_ref().is_some_and(|p| p.can_seek) {
            return;
        }
        let entry = self
            .data
            .player
            .as_ref()
            .and_then(|p| p.current_entry_id.clone());
        if let (Some(duration), Some(entry_id)) = (duration, entry) {
            let seconds = seconds.clamp(0.0, duration.max(0.0));
            self.wanted_position = self
                .send(
                    Command::Seek(SeekParams { seconds, entry_id }),
                    Target::Seek,
                    0,
                )
                .then_some(seconds);
        }
    }

    pub fn seek_by(&mut self, seconds: f64, unix_time: f64) {
        self.seek(
            self.wanted_position
                .unwrap_or_else(|| self.data.position(unix_time))
                + seconds,
        );
    }

    pub fn volume_by(&mut self, amount: f64) {
        let level = self
            .wanted_volume
            .as_ref()
            .and_then(|v| v.level)
            .or_else(|| self.data.volume.as_ref().and_then(|v| v.level));
        if let Some(level) = level {
            self.volume_to(level + amount);
        }
    }

    pub fn volume_to(&mut self, level: f64) {
        if !level.is_finite() || !self.data.volume.as_ref().is_some_and(|v| v.can_set_volume) {
            return;
        }
        let muted = self
            .data
            .volume
            .as_ref()
            .filter(|v| v.can_mute)
            .map(|_| false);
        self.set_volume(VolumeParams {
            level: Some(level.clamp(0.0, 1.0)),
            muted,
        });
    }

    pub fn toggle_mute(&mut self) {
        let Some(volume) = self.data.volume.as_ref().filter(|v| v.can_mute) else {
            return;
        };
        let wanted = self.wanted_volume.clone().unwrap_or(VolumeParams {
            level: None,
            muted: volume.muted,
        });
        self.set_volume(VolumeParams {
            level: wanted.level,
            muted: Some(!wanted.muted.unwrap_or(false)),
        });
    }

    fn set_volume(&mut self, volume: VolumeParams) {
        self.wanted_volume = self
            .send(Command::Volume(volume.clone()), Target::Volume, 0)
            .then_some(volume);
    }

    pub fn remove_queue(&mut self) {
        if !self.list_ready(&Target::Queue) {
            return;
        }
        if let Some(entry) = self
            .data
            .queue
            .as_ref()
            .and_then(|q| q.entries.get(self.ui.cursors[1]))
        {
            self.send(
                Command::QueueRemove(QueueRemove {
                    entry_id: entry.id.clone(),
                }),
                Target::QueueEdit,
                0,
            );
        }
    }

    pub fn move_queue(&mut self, down: bool) {
        if !self.list_ready(&Target::Queue) {
            return;
        }
        let Some(queue) = &self.data.queue else {
            return;
        };
        let index = self.ui.cursors[1];
        let Some(entry) = queue.entries.get(index) else {
            return;
        };
        if !down && index == 0 {
            return;
        }
        let before = if down { index + 2 } else { index - 1 };
        if down && before >= queue.entries.len() {
            if let Some(offset) = queue.next_offset {
                self.queue_page(offset);
                return;
            }
            if index + 1 >= queue.entries.len() {
                return;
            }
        }
        let params = QueueMove {
            entry_id: entry.id.clone(),
            before_entry_id: queue.entries.get(before).map(|e| e.id.clone()),
        };
        if self.send(Command::QueueMove(params), Target::QueueEdit, 0) {
            self.ui.cursors[1] = if down { index + 1 } else { index - 1 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Admission;
    #[test]
    fn playback_round_trips_do_not_swallow_stable_item_actions() {
        for queue in [false, true] {
            for mode in [false, true] {
                let mut app = App::default();
                let item: Item = serde_json::from_value(serde_json::json!({
                    "ref":{"id":"song","source":"catalog","kind":"song"},
                    "title":"song","artist":"a","album":"a"
                }))
                .unwrap();
                app.data.items = vec![item.clone()];
                app.data.queue = Some(QueuePage {
                    entries: vec![QueueEntry {
                        id: "entry".into(),
                        item: Some(item.clone()),
                    }],
                    next_offset: None,
                    revision: 1,
                    total: 1,
                });
                if queue {
                    app.ui.focus = Focus::Right;
                    app.ui.panel = crate::state::Panel::Queue;
                }
                if mode {
                    app.send(
                        Command::Mode(ModeParams {
                            shuffle: true,
                            repeat_mode: RepeatMode::Off,
                        }),
                        Target::Mode,
                        0,
                    );
                } else {
                    app.control(Control::Toggle);
                }
                app.flush(|_| Ok(Admission::Accepted)).unwrap();
                assert!(!app.list_ready(if queue { &Target::Queue } else { &Target::Main }));
                assert_eq!(app.selected().unwrap().r#ref, item.r#ref);
                app.activate();
                app.favorite_selected(0.0);
                app.playlist_picker();
                assert!(
                    matches!(&app.ui.dialog.as_ref().unwrap().rows(&app.ui, &app.data)[0].choice,
                    crate::state::Choice::Edit(crate::state::EditAction::Create(Some(reference)), _) if *reference == item.r#ref)
                );
                let mut commands = vec![];
                app.flush(|r| {
                    commands.push(r.command.clone());
                    Ok(Admission::Accepted)
                })
                .unwrap();
                assert_eq!(commands.len(), 2);
                assert!(matches!(&commands[1], Command::Favorite(p) if p.item == item.r#ref));
                if queue {
                    assert!(matches!(&commands[0], Command::QueueJump(p) if p.entry_id == "entry"));
                } else {
                    assert!(matches!(&commands[0], Command::Play(p) if p.items == [item.r#ref]));
                }
            }
        }
    }

    #[test]
    fn shuffled_detail_is_one_intention_and_old_entry_seeks_are_discarded() {
        let mut app = App::default();
        app.detail_ref = Some(ItemRef {
            id: "album".into(),
            source: Source::Catalog,
            kind: Kind::Album,
        });
        app.ui.view = View::Detail;
        app.play_detail(true);
        assert_eq!(
            app.flush(|request| {
                assert!(matches!(&request.command, Command::Play(p) if p.shuffle == Some(true)));
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
        let mut player: PlayerState = serde_json::from_value(serde_json::json!({
            "current":{"ref":{"id":"song","source":"library","kind":"song"},"title":"song","artist":"a","album":"a","duration":100},
            "currentEntryId":"entry-a","playing":false,"position":0,"queueCount":0,"queueRevision":1,
            "updatedAt":1000,"repeatMode":"off","shuffle":false,"canSeek":true
        })).unwrap();
        app.receive(Event {
            version: 1,
            id: None,
            sequence: 1,
            event: Notice::Player(player.clone()),
        });
        app.seek(20.0);
        assert_eq!(app.wanted_position, Some(20.0));
        player.current_entry_id = Some("entry-b".into());
        app.receive(Event {
            version: 1,
            id: None,
            sequence: 2,
            event: Notice::Player(player),
        });
        assert_eq!(app.wanted_position, None);
        app.flush(|request| {
            assert!(!matches!(request.command, Command::Seek(_)));
            Ok(Admission::Accepted)
        })
        .unwrap();
    }
    #[test]
    fn relative_volume_keeps_intent_while_display_stays_confirmed() {
        let mut app = App::default();
        app.data.volume = Some(VolumeState {
            device: "test".into(),
            level: Some(0.5),
            muted: Some(false),
            can_set_volume: true,
            can_mute: true,
        });
        app.volume_by(0.05);
        let mut first = 0;
        app.flush(|r| {
            first = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.volume_by(0.05);
        let mut second = 0;
        app.flush(|r| {
            assert!(
                matches!(&r.command, Command::Volume(p) if (p.level.unwrap() - 0.6).abs() < 0.0001)
            );
            second = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(app.data.volume.as_ref().unwrap().level, Some(0.5));
        let mut volume = app.data.volume.clone().unwrap();
        volume.level = Some(0.55);
        assert!(!app.receive(Event {
            version: 1,
            id: Some(first),
            sequence: 1,
            event: Notice::Volume(volume.clone())
        }));
        assert!(app.wanted_volume.is_some());
        volume.level = Some(0.6);
        app.receive(Event {
            version: 1,
            id: Some(second),
            sequence: 2,
            event: Notice::Volume(volume),
        });
        assert!(app.wanted_volume.is_none());
        assert_eq!(app.data.volume.unwrap().level, Some(0.6));
    }
}
