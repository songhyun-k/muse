use crate::{
    app::App,
    art::ArtCache,
    effects::Effects,
    i18n::Language,
    motion::Motion,
    mouse::Mouse,
    preferences::Preferences,
    scene::Scene,
    scheduling::FrameClock,
    state::Ui,
    theme::{Palette, THEMES},
    transport::Channel,
};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    },
    execute,
    style::ResetColor,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io::{self, IsTerminal},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct Options {
    ui: Ui,
    fps: u32,
    limited: bool,
}

impl Options {
    fn parse(args: &[String], ui: Ui) -> Result<Self, String> {
        let mut ui = ui;
        for pair in args.windows(2).filter(|pair| pair[0] == "--language") {
            ui.language = Language::parse(&pair[1])?;
        }
        let mut options = Self {
            ui,
            fps: 60,
            limited: std::env::var("TERM_PROGRAM").as_deref() == Ok("Apple_Terminal"),
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--language" => {
                    options.ui.language =
                        Language::parse(args.next().ok_or("--language requires en or ko")?)?;
                }
                "--help" => {}
                "--theme" => {
                    options.ui.theme = args
                        .next()
                        .and_then(|name| THEMES.iter().position(|t| t == name))
                        .ok_or(options.ui.text(
                            "테마는 porcelain, graphite, linen, midnight, ink 중 하나입니다",
                        ))?;
                }
                "--transparent" | "--opaque" => options.ui.transparent = arg == "--transparent",
                "--hide-left" | "--show-left" => options.ui.left_open = arg == "--show-left",
                "--hide-right" | "--show-right" => options.ui.right_open = arg == "--show-right",
                "--plain-icons" | "--nerd-icons" => options.ui.plain_icons = arg == "--plain-icons",
                "--reduced-motion" | "--motion" => {
                    options.ui.reduced_motion = arg == "--reduced-motion"
                }
                "--fps" => {
                    options.fps = args
                        .next()
                        .and_then(|s| s.parse().ok())
                        .filter(|n| [15, 30, 60].contains(n))
                        .ok_or(options.ui.text("--fps는 15, 30, 60 중 하나입니다"))?;
                }
                "--demo" => {}
                "--256-color" | "--true-color" => options.limited = arg == "--256-color",
                _ => {
                    return Err(if options.ui.language == Language::Korean {
                        format!("알 수 없는 옵션: {arg}. --help로 사용법을 확인하세요")
                    } else {
                        format!("Unknown option: {arg}. See --help for usage")
                    });
                }
            }
        }
        Ok(options)
    }
}

// Declared before terminal setup: partial setup failures also restore the terminal.
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            ResetColor,
            Show,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

pub fn run(channel: Channel, args: &[String]) -> Result<i32, String> {
    let initial = Options::parse(
        args,
        Ui {
            language: Language::detect(),
            ..Ui::default()
        },
    )?;
    if args.iter().any(|a| a == "--help") {
        print!(
            "{}",
            match initial.ui.language {
                Language::Korean => include_str!("../assets/help.ko.txt"),
                Language::English => include_str!("../assets/help.en.txt"),
            }
        );
        return Ok(0);
    }
    let path = if args.iter().any(|a| a == "--demo") {
        None
    } else {
        let directory = if let Some(path) = std::env::var_os("MUSE_STATE_DIR") {
            let path = std::path::PathBuf::from(path);
            if !path.is_absolute() {
                return Err(initial
                    .ui
                    .text("MUSE_STATE_DIR에는 절대 경로를 지정해주세요")
                    .into());
            }
            Some(path)
        } else {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join("Library/Application Support/muse"))
        };
        directory.map(|directory| directory.join("ui.json"))
    };
    let preferences = Preferences::open(path);
    let options = Options::parse(args, preferences.ui())?;
    let language = options.ui.language;
    run_terminal(channel, options, preferences)
        .map_err(|message| language.message(&message).into_owned())
}

fn run_terminal(
    mut channel: Channel,
    options: Options,
    mut preferences: Preferences,
) -> Result<i32, String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("대화형 터미널에서 실행해주세요. 사용법: muse --help".into());
    }
    let io_error = |error: io::Error| error.to_string();
    let _restore = Restore;
    crate::terminal_lifecycle::enter_raw_mode().map_err(io_error)?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        Hide,
        EnableBracketedPaste,
        EnableMouseCapture
    )
    .map_err(io_error)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout())).map_err(io_error)?;
    terminal.clear().map_err(io_error)?;
    let mut app = App::default();
    app.ui = options.ui;
    app.ui.toast = preferences.warning.take().map(|message| (message, 0.0));
    let mut art = ArtCache::default();
    let started = Instant::now();
    let mut clock = FrameClock::new(options.fps);
    let mut dirty = true;
    let mut hits = Vec::new();
    let mut mouse = Mouse::default();
    let mut tooltip_shown = false;
    let mut motion = None;
    let mut effects = Effects::default();
    let mut visible_layout = None;
    app.start();
    loop {
        let now = started.elapsed().as_secs_f64();
        let unix = unix_time();
        // Bound each drain so a busy provider cannot starve terminal input.
        for _ in 0..32 {
            let Some(event) = channel.receive()? else {
                break;
            };
            dirty |= app.receive(event);
        }
        if channel.closed {
            return Err("음악 서비스 연결이 종료되었습니다".into());
        }
        app.dispatch_due(now);
        dirty |= app.tour(now);
        app.flush(|request| channel.send(request))?;
        if let Some(message) = app.ui.feedback.take() {
            app.ui.toast = Some((message, now));
            dirty = true;
        }
        if let Err(error) = preferences.save(&app.ui) {
            app.ui.toast = Some((error, now));
            dirty = true;
        }
        if app
            .ui
            .toast
            .as_ref()
            .is_some_and(|(_, since)| now - since >= 2.4)
        {
            app.ui.toast = None;
            dirty = true;
        }
        let size = terminal.size().map_err(io_error)?;
        let layout = app.ui.fit(size.width.into(), size.height.into());
        if app.ui.hover.is_some() && !tooltip_shown && now - app.ui.hover_since >= 0.55 {
            tooltip_shown = true;
            dirty = true;
        }
        let playing = app.data.player.as_ref().is_some_and(|p| p.playing);
        let dimensions = (size.width, size.height);
        let motion =
            motion.get_or_insert_with(|| Motion::new(&app.ui, &app.data, now, unix, dimensions));
        let moving = motion.advance(&app.ui, &app.data, now, unix, dimensions);
        effects.observe(&app.ui, &app.data, now);
        let toast = app.ui.toast.as_ref().is_some_and(|(_, since)| {
            let life = now - since;
            life < 0.18 || (2.0..2.4).contains(&life)
        });
        let animating = moving
            || effects.active(now, app.ui.reduced_motion)
            || (toast && !app.ui.reduced_motion);
        if clock.due(now, dirty, animating, playing) {
            let palette = Palette::new(app.ui.theme, app.ui.transparent);
            let mut artwork = Vec::new();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    let mut scene = Scene::new(
                        &app.ui,
                        &app.data,
                        &motion.visual,
                        &palette,
                        &mut art,
                        area.width,
                        area.height,
                    );
                    scene.draw_animated(&mut effects);
                    if options.limited {
                        for cell in &mut scene.canvas.buffer.content {
                            cell.fg = crate::theme::ansi256(cell.fg);
                            cell.bg = crate::theme::ansi256(cell.bg);
                        }
                    }
                    visible_layout = Some(scene.layout);
                    *frame.buffer_mut() = scene.canvas.buffer;
                    hits = scene.hits;
                    artwork = scene.artwork;
                })
                .map_err(io_error)?;
            app.prefetch_artwork(artwork);
            dirty = false;
            clock.drawn(now, animating || effects.active(now, app.ui.reduced_motion));
        }
        // ponytail: the native mailbox has no OS wait handle; bounded polling
        // sleeps at idle (10 wakes/sec). Add a wake fd only if measured idle cost warrants it.
        let timeout = clock.wait(started.elapsed().as_secs_f64(), dirty, animating, playing);
        if event::poll(timeout).map_err(io_error)? {
            let now = started.elapsed().as_secs_f64();
            let unix = unix_time();
            match event::read().map_err(io_error)? {
                Event::Key(event) => {
                    if let Some(key) = key_name(event) {
                        mouse.reset();
                        app.ui.hover = None;
                        if !app.key(&key, layout, now, unix) {
                            return Ok(0);
                        }
                        dirty = true;
                    }
                }
                Event::Paste(text) => {
                    app.paste(&text, now);
                    dirty = true;
                }
                Event::Mouse(event) => {
                    let previous = app.ui.hover;
                    if !mouse.handle(
                        &mut app,
                        event,
                        visible_layout.unwrap_or(layout),
                        &hits,
                        now,
                        unix,
                    ) {
                        return Ok(0);
                    }
                    if app.ui.hover != previous {
                        tooltip_shown = false;
                    }
                    dirty = true;
                }
                Event::Resize(_, _) => {
                    mouse.reset();
                    app.ui.hover = None;
                    hits.clear();
                    visible_layout = None;
                    dirty = true;
                }
                _ => {}
            }
            effects.observe(&app.ui, &app.data, now);
        }
    }
}

fn unix_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn key_name(event: KeyEvent) -> Option<String> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    if event
        .modifiers
        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    if control {
        return match event.code {
            KeyCode::Char('c') => Some("ctrl-c"),
            KeyCode::Char('a') => Some("home"),
            KeyCode::Char('e') => Some("end"),
            KeyCode::Char('u') => Some("clear"),
            KeyCode::Char('l') => Some("full_lyrics"),
            _ => None,
        }
        .map(str::to_owned);
    }
    Some(
        match event.code {
            KeyCode::Char(ch) => return Some(ch.to_string()),
            KeyCode::Enter => "enter",
            KeyCode::Esc => "esc",
            KeyCode::Backspace => "backspace",
            KeyCode::Delete => "delete",
            KeyCode::Tab if event.modifiers.contains(KeyModifiers::SHIFT) => "backtab",
            KeyCode::Tab => "tab",
            KeyCode::BackTab => "backtab",
            KeyCode::Left => "left",
            KeyCode::Right => "right",
            KeyCode::Up => "up",
            KeyCode::Down => "down",
            KeyCode::Home => "home",
            KeyCode::End => "end",
            KeyCode::PageUp => "pageup",
            KeyCode::PageDown => "pagedown",
            _ => return None,
        }
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_playback_keys_replace_the_whole_container_in_compact_layout() {
        use crate::{
            generated::{Command, ItemRef, Kind, Placement, PlayParams, Source},
            state::{EditAction, Focus, View},
            transport::Admission,
        };

        for (source, kind) in [
            (Source::Catalog, Kind::Album),
            (Source::Library, Kind::Playlist),
            (Source::Collection, Kind::Playlist),
        ] {
            for (character, shuffle) in [('P', false), ('S', true)] {
                let mut app = App::default();
                let detail = ItemRef {
                    id: "detail".into(),
                    source,
                    kind,
                };
                app.ui.view = if source == Source::Collection {
                    View::Playlists
                } else {
                    View::Detail
                };
                app.detail_ref = Some(detail.clone());
                app.data.items = vec![
                    serde_json::from_value(serde_json::json!({
                        "ref":{"id":"selected-song","source":"catalog","kind":"song"},
                        "title":"Song","artist":"Artist","album":"Album"
                    }))
                    .unwrap(),
                ];
                app.data.next_offset = Some(1);
                let layout = app.ui.fit(80, 24);
                let key =
                    key_name(KeyEvent::new(KeyCode::Char(character), KeyModifiers::SHIFT)).unwrap();
                assert!(app.key(&key, layout, 1.0, 1.0));
                assert_eq!(
                    app.flush(|request| {
                        assert_eq!(
                            request.command,
                            Command::Play(PlayParams {
                                items: vec![detail.clone()],
                                start_index: 0,
                                placement: Placement::Replace,
                                shuffle: Some(shuffle),
                            })
                        );
                        Ok(Admission::Accepted)
                    })
                    .unwrap(),
                    1
                );
                for focus in [Focus::Nav, Focus::Right] {
                    app.ui.focus = focus;
                    app.key(&key, layout, 1.0, 1.0);
                }
                app.ui.focus = Focus::Main;
                app.edit(EditAction::Create(None), String::new());
                app.key(&key, layout, 1.0, 1.0);
                assert_eq!(app.ui.editor.as_ref().unwrap().buffer, key);
                assert_eq!(
                    app.flush(|_| panic!("unfocused detail playback")).unwrap(),
                    0
                );
            }
        }
    }

    #[test]
    fn cli_and_terminal_keys_retain_intent() {
        let args = [
            "--theme",
            "midnight",
            "--transparent",
            "--fps",
            "30",
            "--hide-right",
        ]
        .map(String::from);
        let options = Options::parse(&args, Ui::default()).unwrap();
        assert_eq!((options.ui.theme, options.fps), (3, 30));
        assert!(options.ui.transparent && !options.ui.right_open);
        for args in [vec!["--theme"], vec!["--fps", "0"], vec!["--unknown"]] {
            assert!(
                Options::parse(
                    &args.into_iter().map(String::from).collect::<Vec<_>>(),
                    Ui::default()
                )
                .is_err()
            );
        }
        assert_eq!(
            key_name(KeyEvent::new(KeyCode::Char('한'), KeyModifiers::NONE)).as_deref(),
            Some("한")
        );
        assert_eq!(
            key_name(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)).as_deref(),
            Some("full_lyrics")
        );
        assert_eq!(
            key_name(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)).as_deref(),
            Some("backtab")
        );
        assert!(
            key_name(KeyEvent::new_with_kind(
                KeyCode::Char('q'),
                KeyModifiers::NONE,
                KeyEventKind::Release
            ))
            .is_none()
        );
    }
}
