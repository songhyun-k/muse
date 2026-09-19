use crate::{
    app::App,
    canvas::{cells, clean, cut},
    generated::*,
    requests::Target,
    state::{EditAction, Editor, View},
};
use unicode_segmentation::UnicodeSegmentation;

impl Editor {
    pub fn new(action: EditAction, buffer: String) -> Self {
        let mut editor = Self {
            action,
            buffer: String::new(),
            cursor: 0,
        };
        editor.insert(&buffer);
        editor
    }

    pub fn insert(&mut self, text: &str) {
        let mut remaining = self
            .action
            .limit()
            .saturating_sub(self.buffer.chars().count());
        let text = clean(&text.replace(['\r', '\n', '\t'], " "));
        let text: String = text
            .graphemes(true)
            .take_while(|g| {
                let size = g.chars().count();
                if size > remaining {
                    return false;
                }
                remaining -= size;
                true
            })
            .collect();
        self.buffer.insert_str(self.cursor, &text);
        let desired = self.cursor + text.len();
        self.cursor = self
            .buffer
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([self.buffer.len()])
            .find(|i| *i >= desired)
            .unwrap_or(self.buffer.len());
    }

    pub fn backspace(&mut self) {
        if let Some((index, _)) = self.buffer[..self.cursor]
            .grapheme_indices(true)
            .next_back()
        {
            self.buffer.replace_range(index..self.cursor, "");
            self.cursor = index;
        }
    }

    pub fn delete(&mut self) {
        if let Some(g) = self.buffer[self.cursor..].graphemes(true).next() {
            self.buffer
                .replace_range(self.cursor..self.cursor + g.len(), "");
        }
    }

    pub fn move_cursor(&mut self, right: bool) {
        if right {
            self.cursor += self.buffer[self.cursor..]
                .graphemes(true)
                .next()
                .map_or(0, str::len);
        } else {
            self.cursor = self.buffer[..self.cursor]
                .grapheme_indices(true)
                .next_back()
                .map_or(0, |(i, _)| i);
        }
    }

    pub fn edge(&mut self, end: bool) {
        self.cursor = if end { self.buffer.len() } else { 0 };
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
    }

    pub fn visible(&self, width: i32) -> String {
        if width <= 0 {
            return String::new();
        }
        let before = &self.buffer[..self.cursor];
        let clipped = cells(before) > width - 1;
        let prefix = if clipped && width > 1 { "‹" } else { "" };
        let mut remaining = (width - 1 - cells(prefix)).max(0);
        let mut left: Vec<_> = before
            .graphemes(true)
            .rev()
            .take_while(|g| {
                remaining -= cells(g);
                remaining >= 0
            })
            .collect();
        left.reverse();
        let left = left.concat();
        let right = cut(
            &self.buffer[self.cursor..],
            width - cells(prefix) - cells(&left) - 1,
        );
        format!("{prefix}{left}▏{right}")
    }
}

impl App {
    pub fn edit(&mut self, action: EditAction, buffer: String) {
        self.ui.editor = Some(Editor::new(action, buffer));
    }

    pub fn commit_edit(&mut self) -> Result<(), String> {
        let Some(editor) = self.ui.editor.clone() else {
            return Ok(());
        };
        let value = editor.buffer.trim();
        if value.chars().count() > editor.action.limit() {
            return Err("입력할 수 있는 길이를 초과했습니다".into());
        }
        if value.is_empty()
            && !matches!(editor.action, EditAction::Search | EditAction::Describe(_))
        {
            return Err("내용을 입력해주세요".into());
        }
        if let EditAction::Lyrics(item) | EditAction::Offset(item) = &editor.action
            && self.data.current().is_none_or(|i| &i.r#ref != item)
        {
            return Err("재생 중인 곡이 변경되었습니다".into());
        }
        let creating = matches!(editor.action, EditAction::Create(_) | EditAction::Copy(_));
        if creating && self.data.loading.contains(&Target::Create) {
            return Err("플레이리스트를 만드는 중입니다".into());
        }
        let (command, target) = match &editor.action {
            EditAction::Search => {
                self.ui.query = value.into();
                self.go(View::Search);
                self.ui.editor = None;
                return Ok(());
            }
            EditAction::Create(item) => (
                Command::CollectionCreate(CollectionCreate {
                    name: value.into(),
                    description: String::new(),
                    items: item.as_ref().map(|item| vec![item.clone()]),
                }),
                Target::Create,
            ),
            EditAction::Copy(item) => (
                Command::CollectionCopy(CollectionCopy {
                    item: item.clone(),
                    name: value.into(),
                }),
                Target::Create,
            ),
            EditAction::Rename(id) => (
                Command::CollectionUpdate(CollectionUpdate {
                    id: id.clone(),
                    name: Some(value.into()),
                    description: None,
                }),
                Target::Mutation,
            ),
            EditAction::Describe(id) => (
                Command::CollectionUpdate(CollectionUpdate {
                    id: id.clone(),
                    name: None,
                    description: Some(value.into()),
                }),
                Target::Mutation,
            ),
            EditAction::Lyrics(item) => (
                Command::LyricsSearch(LyricsSearchParams {
                    item: item.clone(),
                    query: value.into(),
                }),
                Target::LyricMatches,
            ),
            EditAction::Offset(item) => {
                let seconds: f64 = value.parse().map_err(|_| "초 단위 숫자를 입력해주세요")?;
                if !seconds.is_finite() || !(-30.0..=30.0).contains(&seconds) {
                    return Err("-30~30초 사이로 입력해주세요".into());
                }
                (
                    Command::LyricsOffset(LyricsOffsetParams {
                        item: item.clone(),
                        seconds,
                    }),
                    Target::Lyrics,
                )
            }
        };
        let matches = target == Target::LyricMatches;
        if !self.send(command, target, 0) {
            return Err("잠시 후 다시 시도해주세요".into());
        }
        if creating {
            self.create_revision = Some(self.navigation_revision);
        }
        if matches {
            self.data.matches = None;
            self.ui.dialog = Some(crate::state::Dialog::Lyrics { cursor: 0 });
        }
        self.ui.editor = None;
        Ok(())
    }

    pub fn favorite_selected(&mut self, now: f64) {
        if let Some(item) = self
            .selected()
            .filter(|i| i.r#ref.kind == Kind::Song)
            .cloned()
        {
            self.favorite_at(item.r#ref, now);
        }
    }

    pub fn favorite_at(&mut self, item: ItemRef, now: f64) {
        self.ui.favorite_pulse = Some((item.clone(), now));
        self.toggle_favorite(item);
    }

    pub fn toggle_favorite(&mut self, item: ItemRef) {
        if item.kind != Kind::Song {
            return;
        }
        let key = crate::state::item_key(&item);
        let enabled = !self.wanted_favorites.get(&key).copied().unwrap_or_else(|| {
            self.data
                .store
                .as_ref()
                .is_some_and(|store| store.favorites.contains(&item))
        });
        if self.send(
            Command::Favorite(FavoriteParams { item, enabled }),
            Target::Favorite(key.clone()),
            0,
        ) {
            self.wanted_favorites.insert(key, enabled);
        } else {
            self.wanted_favorites.remove(&key);
        }
    }

    pub fn remove_collection_song(&mut self) {
        if !self.list_ready(&Target::Main) || self.data.items.get(self.ui.cursors[0]).is_none() {
            return;
        }
        if let Some(item) = self
            .focused_detail()
            .filter(|r| r.source == Source::Collection)
        {
            self.send(
                Command::CollectionRemove(CollectionRemove {
                    id: item.id.clone(),
                    index: self.ui.cursors[0] as u64,
                }),
                Target::MainEdit,
                0,
            );
        }
    }

    pub fn move_collection_song(&mut self, down: bool) {
        if !self.list_ready(&Target::Main) || self.data.items.get(self.ui.cursors[0]).is_none() {
            return;
        }
        let Some(item) = self
            .focused_detail()
            .filter(|r| r.source == Source::Collection)
        else {
            return;
        };
        let from = self.ui.cursors[0];
        let to = if down {
            from + 1
        } else {
            from.saturating_sub(1)
        };
        if to >= self.data.items.len() {
            self.more();
            return;
        }
        if to != from
            && self.send(
                Command::CollectionMove(CollectionMove {
                    id: item.id.clone(),
                    from: from as u64,
                    to: to as u64,
                }),
                Target::MainEdit,
                0,
            )
        {
            self.ui.cursors[0] = to;
        }
    }

    pub fn choose_lyrics(&mut self, id: u64) {
        if let Some(item) = self.data.current() {
            self.send(
                Command::LyricsChoose(LyricsChooseParams {
                    item: item.r#ref.clone(),
                    match_id: id,
                }),
                Target::Lyrics,
                0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Admission;
    #[test]
    fn cursor_edits_whole_graphemes_and_stays_visible_in_narrow_fields() {
        let mut editor = Editor::new(EditAction::Search, "가나👨‍👩‍👧‍👦다".into());
        editor.move_cursor(false);
        editor.backspace();
        assert_eq!(editor.buffer, "가나다");
        editor.insert("X");
        assert_eq!(editor.buffer, "가나X다");
        editor.delete();
        assert_eq!(editor.buffer, "가나X");
        editor.edge(false);
        editor.insert("첫\n줄 ");
        assert_eq!(editor.buffer, "첫 줄 가나X");
        editor.edge(true);
        for width in 1..10 {
            let visible = editor.visible(width);
            assert!(cells(&visible) <= width && visible.contains('▏'));
        }
        editor.clear();
        assert_eq!(editor.visible(10), "▏");
    }
    #[test]
    fn editing_one_field_never_resubmits_a_stale_other_field() {
        let mut app = App::default();
        app.edit(
            EditAction::Describe("playlist".into()),
            "New description".into(),
        );
        app.commit_edit().unwrap();
        assert_eq!(app.flush(|request| {
            assert!(matches!(&request.command, Command::CollectionUpdate(p)
                if p.id == "playlist" && p.name.is_none() && p.description.as_deref() == Some("New description")));
            Ok(Admission::Accepted)
        }).unwrap(), 1);
    }
    #[test]
    fn repeated_favorite_keys_keep_both_intentions() {
        let mut app = App::default();
        let item = ItemRef {
            id: "song".into(),
            source: Source::Catalog,
            kind: Kind::Song,
        };
        app.toggle_favorite(item.clone());
        app.toggle_favorite(item.clone());
        let mut id = 0;
        assert_eq!(
            app.flush(|r| {
                assert!(matches!(&r.command, Command::Favorite(p) if !p.enabled));
                id = r.id;
                Ok(Admission::Accepted)
            })
            .unwrap(),
            1
        );
        assert!(app.data.store.is_none());
        assert!(app.ui.feedback.is_none());
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 1,
            event: Notice::Store(StoreState {
                collections: vec![],
                favorites: vec![],
                history_count: 0,
            }),
        });
        assert_eq!(app.ui.feedback.take().as_deref(), Some("즐겨찾기에서 제거"));
        app.toggle_favorite(item);
        app.flush(|r| {
            id = r.id;
            Ok(Admission::Accepted)
        })
        .unwrap();
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 2,
            event: Notice::Failure(Failure {
                code: ErrorCode::Storage,
                message: "저장 실패".into(),
                retryable: true,
            }),
        });
        assert!(app.ui.feedback.is_none());
        assert!(app.data.store.as_ref().unwrap().favorites.is_empty());
    }
    #[test]
    fn editor_limits_and_grapheme_deletion_preserve_input() {
        let mut editor = Editor::new(EditAction::Search, "가사👨‍👩‍👧‍👦".into());
        editor.backspace();
        assert_eq!(editor.buffer, "가사");
        editor.insert(&"가".repeat(300));
        assert_eq!(editor.buffer.chars().count(), 200);
        let mut app = App::default();
        app.edit(EditAction::Create(None), " ".into());
        assert!(app.commit_edit().is_err());
        assert!(app.ui.editor.is_some());
        assert_eq!(app.flush(|_| Ok(Admission::Accepted)).unwrap(), 0);
    }

    #[test]
    fn creation_includes_initial_items_and_opens_the_returned_identity() {
        let mut app = App::default();
        let song = ItemRef {
            id: "song".into(),
            source: Source::Catalog,
            kind: Kind::Song,
        };
        app.edit(EditAction::Create(Some(song.clone())), "Night".into());
        app.commit_edit().unwrap();
        let mut id = 0;
        app.flush(|r| {
            id = r.id;
            assert!(matches!(&r.command, Command::CollectionCreate(p) if p.items.as_ref() == Some(&vec![song.clone()])));
            Ok(Admission::Accepted)
        })
        .unwrap();
        let created = Collection {
            id: "exact-created-id".into(),
            name: "Night".into(),
            description: String::new(),
            count: 1,
        };
        app.receive(Event {
            version: 1,
            id: Some(id),
            sequence: 1,
            event: Notice::CollectionCreated(CreatedCollection {
                collection: created.clone(),
                store: StoreState {
                    collections: vec![created],
                    favorites: vec![],
                    history_count: 0,
                },
            }),
        });
        app.flush(|r| {
            assert!(matches!(&r.command, Command::Detail(p) if p.item.id == "exact-created-id"));
            Ok(Admission::Accepted)
        })
        .unwrap();
        assert_eq!(app.detail_ref.unwrap().id, "exact-created-id");
    }
}
