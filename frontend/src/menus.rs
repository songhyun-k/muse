use crate::{
    app::App,
    canvas::clock,
    generated::*,
    requests::Target,
    state::{Choice, Data, Dialog, EditAction, MenuRow, View},
};

impl MenuRow {
    fn new(label: impl Into<String>, choice: Choice) -> Self {
        Self {
            label: label.into(),
            detail: String::new(),
            checked: false,
            choice,
        }
    }
}

impl Dialog {
    pub fn cursor(&self) -> usize {
        match self {
            Self::Menu { cursor, .. } | Self::Lyrics { cursor } => *cursor,
        }
    }

    pub fn move_by(&mut self, delta: i32, count: usize) {
        let cursor = match self {
            Self::Menu { cursor, .. } | Self::Lyrics { cursor } => cursor,
        };
        *cursor =
            (*cursor as i64 + i64::from(delta)).clamp(0, count.saturating_sub(1) as i64) as usize;
    }

    pub fn rows(&self, data: &Data) -> Vec<MenuRow> {
        match self {
            Self::Menu { rows, .. } => rows.clone(),
            Self::Lyrics { .. } => data
                .matches
                .as_ref()
                .map(|result| {
                    result
                        .matches
                        .iter()
                        .map(|m| MenuRow {
                            label: m.title.clone(),
                            detail: format!(
                                "{} · {} · {}",
                                m.artist,
                                m.album,
                                m.duration.map_or_else(|| "--:--".into(), clock)
                            ),
                            checked: data
                                .lyrics
                                .as_ref()
                                .is_some_and(|l| l.match_id == Some(m.id)),
                            choice: Choice::Command(
                                Command::LyricsChoose(LyricsChooseParams {
                                    item: result.item.clone(),
                                    match_id: m.id,
                                }),
                                Target::Lyrics,
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl App {
    fn menu(&mut self, title: &str, rows: Vec<MenuRow>) {
        self.ui.dialog = Some(Dialog::Menu {
            title: title.into(),
            rows,
            cursor: 0,
        });
    }

    pub fn choose_dialog(&mut self) {
        let Some(dialog) = &self.ui.dialog else {
            return;
        };
        let rows = dialog.rows(&self.data);
        let Some(row) = rows.get(dialog.cursor()) else {
            return;
        };
        self.choose(row.choice.clone());
    }

    fn choose(&mut self, choice: Choice) {
        self.ui.dialog = None;
        match choice {
            Choice::Close => {}
            Choice::Language(language) => self.ui.language = language,
            Choice::Edit(action, buffer) => self.edit(action, buffer),
            Choice::Command(command, target) => {
                if matches!(command, Command::CollectionDelete(_)) {
                    self.go(View::Playlists);
                }
                self.send(command, target, 0);
            }
            Choice::Delete(id) => self.menu(
                self.ui.text("플레이리스트 삭제"),
                vec![
                    MenuRow::new(self.ui.text("돌아가기"), Choice::Close),
                    MenuRow::new(
                        self.ui.text("삭제"),
                        Choice::Command(
                            Command::CollectionDelete(CollectionDelete { id }),
                            Target::Mutation,
                        ),
                    ),
                ],
            ),
            Choice::Kind(kind) => {
                self.ui.search_kind = kind;
                if kind == Kind::Station {
                    self.ui.search_source = Source::Catalog;
                }
                self.go(View::Search);
            }
            Choice::Source(source) => {
                self.ui.search_source = source;
                if source == Source::Library && self.ui.search_kind == Kind::Station {
                    self.ui.search_kind = Kind::Song;
                }
                self.go(View::Search);
            }
            Choice::Order(order) => {
                self.ui.order = order;
                self.refresh(0);
            }
            Choice::PlaylistLibrary(library) => {
                self.ui.playlist_library = library;
                self.go(View::Playlists);
            }
            Choice::FullLyrics => self.go(View::Lyrics),
        }
    }

    pub fn playlist_picker(&mut self) {
        let Some(item) = self
            .selected()
            .filter(|i| i.r#ref.kind == Kind::Song)
            .cloned()
        else {
            return;
        };
        let mut rows = vec![MenuRow::new(
            self.ui.text("새 플레이리스트"),
            Choice::Edit(EditAction::Create(Some(item.r#ref.clone())), String::new()),
        )];
        if let Some(store) = &self.data.store {
            rows.extend(store.collections.iter().map(|c| {
                MenuRow::new(
                    &c.name,
                    Choice::Command(
                        Command::CollectionAdd(CollectionAdd {
                            id: c.id.clone(),
                            items: vec![item.r#ref.clone()],
                        }),
                        Target::Mutation,
                    ),
                )
            }));
        }
        self.menu(self.ui.text("플레이리스트에 추가"), rows);
    }

    pub fn language_menu(&mut self) {
        use crate::i18n::Language;
        let rows = [(Language::English, "English"), (Language::Korean, "한국어")]
            .into_iter()
            .map(|(language, name)| {
                let mut row = MenuRow::new(name, Choice::Language(language));
                row.checked = self.ui.language == language;
                row
            })
            .collect();
        self.menu("Language / 언어", rows);
    }

    pub fn collection_menu(&mut self) {
        let mut rows = vec![MenuRow::new(
            self.ui.text("새 플레이리스트"),
            Choice::Edit(EditAction::Create(None), String::new()),
        )];
        let item = self
            .focused_detail()
            .cloned()
            .or_else(|| self.selected().map(|i| i.r#ref.clone()));
        if let Some(item) = item {
            if let Some(collection) = self.data.store.as_ref().and_then(|s| {
                s.collections
                    .iter()
                    .find(|c| item.source == Source::Collection && c.id == item.id)
            }) {
                rows.extend([
                    MenuRow::new(
                        self.ui.text("이름 변경"),
                        Choice::Edit(
                            EditAction::Rename(collection.id.clone()),
                            collection.name.clone(),
                        ),
                    ),
                    MenuRow::new(
                        self.ui.text("설명"),
                        Choice::Edit(
                            EditAction::Describe(collection.id.clone()),
                            collection.description.clone(),
                        ),
                    ),
                    MenuRow::new(self.ui.text("삭제"), Choice::Delete(collection.id.clone())),
                ]);
            }
            if matches!(item.kind, Kind::Album | Kind::Playlist) {
                let name = self
                    .data
                    .detail
                    .as_ref()
                    .or_else(|| self.selected())
                    .map_or(self.ui.text("플레이리스트"), |i| i.title.as_str());
                rows.push(MenuRow::new(
                    self.ui.text("복사"),
                    Choice::Edit(
                        EditAction::Copy(item),
                        format!("{name} {}", self.ui.text("복사본")),
                    ),
                ));
            }
        }
        self.menu(self.ui.text("플레이리스트"), rows);
    }

    pub fn lyrics_menu(&mut self) {
        let Some(item) = self.data.current() else {
            return;
        };
        let mut rows = vec![
            MenuRow::new(
                self.ui.text("가사 찾기"),
                Choice::Edit(
                    EditAction::Lyrics(item.r#ref.clone()),
                    format!("{} {}", item.artist, item.title),
                ),
            ),
            MenuRow::new(self.ui.text("전체 화면"), Choice::FullLyrics),
        ];
        if let Some(lyrics) = self
            .data
            .lyrics
            .as_ref()
            .filter(|l| l.status == LyricsStatus::Synced)
        {
            rows.push(MenuRow::new(
                self.ui.text("시간 조절"),
                Choice::Edit(
                    EditAction::Offset(item.r#ref.clone()),
                    lyrics.offset.to_string(),
                ),
            ));
        }
        self.menu(self.ui.text("가사"), rows);
    }

    pub fn filter_menu(&mut self) {
        let mut rows = vec![];
        if self.ui.view == View::Search {
            for (kind, label) in [
                (Kind::Song, self.ui.text("노래")),
                (Kind::Album, self.ui.text("앨범")),
                (Kind::Artist, self.ui.text("아티스트")),
                (Kind::Playlist, self.ui.text("플레이리스트")),
                (Kind::Station, self.ui.text("스테이션")),
            ] {
                let mut row = MenuRow::new(label, Choice::Kind(kind));
                row.checked = self.ui.search_kind == kind;
                rows.push(row);
            }
            for (source, label) in [
                (Source::Catalog, "Apple Music"),
                (Source::Library, self.ui.text("보관함")),
            ] {
                let mut row = MenuRow::new(label, Choice::Source(source));
                row.checked = self.ui.search_source == source;
                rows.push(row);
            }
        } else if self.detail_ref.is_none()
            && !matches!(self.ui.view, View::Home | View::Lyrics | View::Recent)
        {
            for (order, label) in [
                (Order::Name, self.ui.text("이름순")),
                (Order::Artist, self.ui.text("아티스트순")),
                (Order::Recent, self.ui.text("최근순")),
            ] {
                if order == Order::Artist && matches!(self.ui.view, View::Artists | View::Playlists)
                {
                    continue;
                }
                let mut row = MenuRow::new(label, Choice::Order(order));
                row.checked = self.ui.order == order;
                rows.push(row);
            }
        }
        if self.ui.view == View::Playlists {
            for (library, label) in [
                (false, self.ui.text("플레이리스트")),
                (true, self.ui.text("보관함 플레이리스트")),
            ] {
                let mut row = MenuRow::new(label, Choice::PlaylistLibrary(library));
                row.checked = self.ui.playlist_library == library;
                rows.push(row);
            }
        }
        if !rows.is_empty() {
            self.menu(self.ui.text("보기 옵션"), rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Admission;
    #[test]
    fn delete_requires_a_second_choice_and_lyrics_does_not_browse_the_library() {
        let mut app = App::default();
        app.choose(Choice::Delete("id".into()));
        app.choose_dialog();
        assert_eq!(app.flush(|_| Ok(Admission::Accepted)).unwrap(), 0);
        app.go(View::Lyrics);
        assert!(!app.ui.right_open);
        assert_eq!(app.flush(|_| Ok(Admission::Accepted)).unwrap(), 0);
        app.choose(Choice::Kind(Kind::Station));
        app.choose(Choice::Source(Source::Library));
        assert_eq!(app.ui.search_kind, Kind::Song);
        assert_eq!(app.ui.search_source, Source::Library);
    }
}
