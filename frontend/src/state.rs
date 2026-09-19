use crate::{
    generated::*,
    requests::{Response, Target},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    Search,
    Home,
    Recent,
    Artists,
    Albums,
    #[default]
    Songs,
    Playlists,
    Favorites,
    History,
    Detail,
    Lyrics,
}

pub const NAV: [View; 9] = [
    View::Search,
    View::Home,
    View::Recent,
    View::Artists,
    View::Albums,
    View::Songs,
    View::Playlists,
    View::Favorites,
    View::History,
];

impl View {
    pub fn label(self) -> &'static str {
        match self {
            Self::Search => "검색",
            Self::Home => "홈",
            Self::Recent => "최근 추가",
            Self::Artists => "아티스트",
            Self::Albums => "앨범",
            Self::Songs => "노래",
            Self::Playlists => "플레이리스트",
            Self::Favorites => "즐겨찾기",
            Self::History => "재생 이력",
            Self::Detail => "상세",
            Self::Lyrics => "가사",
        }
    }
    pub fn cards(self) -> bool {
        matches!(
            self,
            Self::Home | Self::Recent | Self::Artists | Self::Albums
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    Nav,
    #[default]
    Main,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Lyrics,
    Queue,
}

#[derive(Clone, Debug)]
pub enum EditAction {
    Search,
    Create(Option<ItemRef>),
    Rename(String),
    Describe(String),
    Copy(ItemRef),
    Lyrics(ItemRef),
    Offset(ItemRef),
}

impl EditAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Search => "검색",
            Self::Create(_) => "플레이리스트 만들기",
            Self::Rename(_) => "이름 변경",
            Self::Describe(_) => "설명",
            Self::Copy(_) => "플레이리스트 복사",
            Self::Lyrics(_) => "가사 찾기",
            Self::Offset(_) => "가사 시간 (초)",
        }
    }

    pub fn limit(&self) -> usize {
        match self {
            Self::Describe(_) => 2000,
            Self::Search | Self::Lyrics(_) => 200,
            Self::Offset(_) => 12,
            _ => 100,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Editor {
    pub action: EditAction,
    pub buffer: String,
    pub(crate) cursor: usize,
}

#[derive(Clone)]
pub enum Choice {
    Close,
    Language(crate::i18n::Language),
    Edit(EditAction, String),
    Command(Command, Target),
    Delete(String),
    Kind(Kind),
    Source(Source),
    Order(Order),
    PlaylistLibrary(bool),
    FullLyrics,
}

#[derive(Clone)]
pub struct MenuRow {
    pub label: String,
    pub detail: String,
    pub checked: bool,
    pub choice: Choice,
}

#[derive(Clone)]
pub enum Dialog {
    Menu {
        title: String,
        rows: Vec<MenuRow>,
        cursor: usize,
    },
    Lyrics {
        cursor: usize,
    },
}

#[derive(Clone)]
pub struct Ui {
    pub language: crate::i18n::Language,
    pub pulse: Option<(String, f64)>,
    pub favorite_pulse: Option<(ItemRef, f64)>,
    pub view: View,
    pub back: View,
    pub focus: Focus,
    pub panel: Panel,
    pub cursors: [usize; 2],
    pub nav_cursor: usize,
    pub left_open: bool,
    pub right_open: bool,
    pub preferred: Focus,
    pub theme: usize,
    pub transparent: bool,
    pub plain_icons: bool,
    pub reduced_motion: bool,
    pub tour: Option<f64>,
    pub tour_step: u64,
    pub query: String,
    pub search_kind: Kind,
    pub search_source: Source,
    pub order: Order,
    pub editor: Option<Editor>,
    pub dialog: Option<Dialog>,
    pub playlist_library: bool,
    pub help: bool,
    pub lyric_manual: Option<f64>,
    pub toast: Option<(String, f64)>,
    pub feedback: Option<String>,
    pub hover: Option<(i32, i32)>,
    pub hover_since: f64,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            language: crate::i18n::Language::Korean,
            pulse: None,
            favorite_pulse: None,
            view: View::Songs,
            back: View::Albums,
            focus: Focus::Main,
            panel: Panel::Lyrics,
            cursors: [0, 0],
            nav_cursor: 5,
            left_open: true,
            right_open: true,
            preferred: Focus::Nav,
            theme: 0,
            transparent: false,
            plain_icons: false,
            reduced_motion: false,
            tour: None,
            tour_step: 0,
            query: String::new(),
            search_kind: Kind::Song,
            search_source: Source::Catalog,
            order: Order::Recent,
            editor: None,
            dialog: None,
            playlist_library: false,
            help: false,
            lyric_manual: None,
            toast: None,
            feedback: None,
            hover: None,
            hover_since: 0.0,
        }
    }
}

impl Ui {
    pub fn queue_focus(&self) -> bool {
        self.focus == Focus::Right && self.panel == Panel::Queue
    }

    pub fn lyrics_focus(&self) -> bool {
        (self.focus == Focus::Right && self.panel == Panel::Lyrics)
            || (self.focus == Focus::Main && self.view == View::Lyrics)
    }

    pub fn cursor(&self) -> usize {
        self.cursors[usize::from(self.queue_focus())]
    }

    pub fn go(&mut self, view: View) {
        self.view = view;
        if view == View::Lyrics {
            self.right_open = false;
        }
        self.focus = Focus::Main;
        self.cursors[0] = 0;
        self.nav_cursor = NAV
            .iter()
            .position(|v| *v == view)
            .unwrap_or_else(|| NAV.iter().position(|v| *v == self.back).unwrap_or(4));
    }
}

pub fn item_key(reference: &ItemRef) -> String {
    format!(
        "{:?}:{:?}:{}",
        reference.source, reference.kind, reference.id
    )
}

#[derive(Default)]
pub struct Data {
    pub session: Option<SessionState>,
    pub player: Option<PlayerState>,
    pub store: Option<StoreState>,
    pub volume: Option<VolumeState>,
    pub items: Vec<Item>,
    pub detail: Option<Item>,
    pub next_offset: Option<u64>,
    pub queue: Option<QueuePage>,
    pub lyrics: Option<Lyrics>,
    pub matches: Option<LyricsMatches>,
    pub artwork: BTreeMap<String, Artwork>,
    pub loading: BTreeSet<Target>,
    pub errors: BTreeMap<Target, Failure>,
    error_order: Vec<Target>,
    offsets: BTreeMap<Target, u64>,
    sequences: BTreeMap<&'static str, u64>,
}

impl Data {
    pub fn notification_error(&self) -> Option<(&Target, &Failure)> {
        self.error_order
            .iter()
            .rev()
            .filter_map(|target| self.errors.get_key_value(target))
            .chain(self.errors.iter())
            .find(|(target, _)| {
                target.is_control() || (**target == Target::Lyrics && self.lyrics.is_some())
            })
    }

    pub fn cancel(&mut self, target: &Target) {
        self.loading.remove(target);
        self.errors.remove(target);
        self.offsets.remove(target);
    }

    pub fn begin(&mut self, target: Target, offset: u64) {
        self.errors.remove(&target);
        if target == Target::Main && offset == 0 {
            self.items.clear();
            self.detail = None;
            self.next_offset = None;
        }
        self.offsets.insert(target.clone(), offset);
        self.loading.insert(target);
    }

    fn newer(&mut self, stream: &'static str, sequence: u64) -> bool {
        let last = self.sequences.entry(stream).or_default();
        if sequence <= *last {
            return false;
        }
        *last = sequence;
        true
    }

    pub fn apply(&mut self, response: Response) {
        let Response {
            target,
            sequence,
            notice,
        } = response;
        let offset = target
            .as_ref()
            .and_then(|t| self.offsets.remove(t))
            .unwrap_or(0);
        if let Some(target) = &target {
            self.loading.remove(target);
        }
        match notice {
            Notice::Snapshot(value) => {
                for notice in [
                    Notice::Session(value.session),
                    Notice::Player(value.player),
                    Notice::Store(value.store),
                    Notice::Volume(value.volume),
                ] {
                    self.apply(Response {
                        target: None,
                        sequence,
                        notice,
                    });
                }
            }
            Notice::Session(value) if self.newer("session", sequence) => self.session = Some(value),
            Notice::Store(value) if self.newer("store", sequence) => self.store = Some(value),
            Notice::CollectionCreated(value) => self.apply(Response {
                target: None,
                sequence,
                notice: Notice::Store(value.store),
            }),
            Notice::Volume(value) if self.newer("volume", sequence) => self.volume = Some(value),
            Notice::Player(value) if self.newer("player", sequence) => {
                if self
                    .player
                    .as_ref()
                    .and_then(|p| p.current.as_ref())
                    .map(|i| &i.r#ref)
                    != value.current.as_ref().map(|i| &i.r#ref)
                {
                    self.lyrics = None;
                    self.matches = None;
                }
                if self
                    .queue
                    .as_ref()
                    .is_some_and(|q| q.revision != value.queue_revision)
                {
                    self.queue = None;
                }
                self.player = Some(value);
            }
            Notice::Page(value) if target == Some(Target::Main) => {
                if offset == 0 {
                    self.items.clear();
                }
                self.items.extend(value.items);
                self.next_offset = value.next_offset;
            }
            Notice::Detail(value) if target == Some(Target::Main) => {
                if offset == 0 {
                    self.items.clear();
                }
                self.detail = Some(value.item);
                self.items.extend(value.children);
                self.next_offset = value.next_offset;
            }
            Notice::Queue(mut value) if target == Some(Target::Queue) => {
                if self
                    .player
                    .as_ref()
                    .is_some_and(|p| p.queue_revision != value.revision)
                {
                    return;
                }
                if offset > 0 {
                    let Some(previous) =
                        self.queue.as_mut().filter(|q| q.revision == value.revision)
                    else {
                        return;
                    };
                    value
                        .entries
                        .splice(0..0, std::mem::take(&mut previous.entries));
                }
                self.queue = Some(value);
            }
            Notice::Lyrics(value) if self.current().is_some_and(|i| i.r#ref == value.item) => {
                if self.newer("lyrics", sequence) {
                    self.lyrics = Some(value);
                }
            }
            Notice::LyricsMatches(value)
                if self.current().is_some_and(|i| i.r#ref == value.item) =>
            {
                self.matches = Some(value);
            }
            Notice::Artwork(value) => {
                // ponytail: 64 cached images; use LRU only if scrolling makes misses noticeable.
                if self.artwork.len() >= 64 {
                    self.artwork.pop_first();
                }
                self.artwork.insert(item_key(&value.item), value);
            }
            Notice::Failure(value) => {
                let target = target.unwrap_or(Target::Mutation);
                self.error_order
                    .retain(|key| key != &target && self.errors.contains_key(key));
                self.error_order.push(target.clone());
                self.errors.insert(target, value);
            }
            _ => {}
        }
    }

    pub fn row_key(&self, index: usize, queue: bool) -> Option<String> {
        if queue {
            self.queue
                .as_ref()?
                .entries
                .get(index)
                .map(|entry| entry.id.clone())
        } else {
            self.items.get(index).map(|item| item_key(&item.r#ref))
        }
    }

    pub fn current(&self) -> Option<&Item> {
        self.player.as_ref()?.current.as_ref()
    }

    pub fn position(&self, unix_time: f64) -> f64 {
        let Some(player) = &self.player else {
            return 0.0;
        };
        let elapsed = if player.playing {
            (unix_time - player.updated_at).max(0.0)
        } else {
            0.0
        };
        (player.position + elapsed).min(self.current().and_then(|i| i.duration).unwrap_or(f64::MAX))
    }

    pub fn favorite(&self, item: &Item) -> bool {
        self.store
            .as_ref()
            .is_some_and(|s| s.favorites.contains(&item.r#ref))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(level: f64) -> VolumeState {
        VolumeState {
            device: "test".into(),
            level: Some(level),
            muted: Some(false),
            can_set_volume: true,
            can_mute: true,
        }
    }

    #[test]
    fn notification_errors_follow_recency_and_skip_nonvisible_or_cleared_failures() {
        let mut data = Data {
            lyrics: Some(Lyrics {
                item: ItemRef {
                    id: "song".into(),
                    source: Source::Catalog,
                    kind: Kind::Song,
                },
                match_id: None,
                status: LyricsStatus::Plain,
                lines: vec![],
                offset: 0.0,
            }),
            ..Data::default()
        };
        for (target, message) in [
            (Target::Lyrics, "old lyrics"),
            (Target::Mode, "mode"),
            (Target::Mutation, "new playback"),
            (Target::Artwork("cover".into()), "hidden cover"),
        ] {
            data.apply(Response {
                target: Some(target),
                sequence: 1,
                notice: Notice::Failure(Failure {
                    code: ErrorCode::Network,
                    message: message.into(),
                    retryable: true,
                }),
            });
        }
        assert_eq!(data.notification_error().unwrap().1.message, "new playback");
        data.cancel(&Target::Mutation);
        assert_eq!(data.notification_error().unwrap().1.message, "mode");
        data.begin(Target::Mode, 0);
        assert_eq!(data.notification_error().unwrap().1.message, "old lyrics");
        data.lyrics = None;
        assert!(data.notification_error().is_none());
        data.errors.clear();
        data.apply(Response {
            target: Some(Target::Volume),
            sequence: 0,
            notice: Notice::Failure(Failure {
                code: ErrorCode::Busy,
                message: "local failure".into(),
                retryable: true,
            }),
        });
        assert_eq!(
            data.notification_error().unwrap().1.message,
            "local failure"
        );
        assert_eq!(data.error_order, [Target::Volume]);
    }

    #[test]
    fn stale_queue_pages_preserve_the_confirmed_entries() {
        let first = QueuePage {
            entries: vec![QueueEntry {
                id: "a".into(),
                item: None,
            }],
            next_offset: Some(1),
            revision: 2,
            total: 2,
        };
        let mut data = Data {
            queue: Some(first.clone()),
            ..Data::default()
        };
        let mut second = QueuePage {
            entries: vec![QueueEntry {
                id: "b".into(),
                item: None,
            }],
            next_offset: None,
            revision: 1,
            total: 2,
        };
        data.begin(Target::Queue, 1);
        data.apply(Response {
            target: Some(Target::Queue),
            sequence: 1,
            notice: Notice::Queue(second.clone()),
        });
        assert_eq!(data.queue, Some(first));
        second.revision = 2;
        data.begin(Target::Queue, 1);
        data.apply(Response {
            target: Some(Target::Queue),
            sequence: 2,
            notice: Notice::Queue(second),
        });
        assert_eq!(
            data.queue
                .unwrap()
                .entries
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn streams_merge_independently_and_failed_reads_are_not_empty_successes() {
        let mut data = Data::default();
        data.apply(Response {
            target: None,
            sequence: 9,
            notice: Notice::Volume(volume(0.9)),
        });
        data.apply(Response {
            target: None,
            sequence: 7,
            notice: Notice::Volume(volume(0.7)),
        });
        data.apply(Response {
            target: None,
            sequence: 2,
            notice: Notice::Session(SessionState {
                authorization: Authorization::Authorized,
                can_play_catalog: Some(true),
            }),
        });
        assert_eq!(data.volume.unwrap().level, Some(0.9));
        assert!(data.session.is_some());
        data.volume = None;
        data.begin(Target::Main, 0);
        data.apply(Response {
            target: Some(Target::Main),
            sequence: 3,
            notice: Notice::Failure(Failure {
                code: ErrorCode::Network,
                message: "offline".into(),
                retryable: true,
            }),
        });
        assert!(data.loading.is_empty());
        assert!(data.errors.contains_key(&Target::Main));
        let mut ui = Ui {
            cursors: [7, 3],
            focus: Focus::Right,
            panel: Panel::Queue,
            ..Ui::default()
        };
        assert_eq!(ui.cursor(), 3);
        ui.focus = Focus::Main;
        assert_eq!(ui.cursor(), 7);
        ui.go(View::Albums);
        assert_eq!(ui.cursors, [0, 3]);
    }
}
