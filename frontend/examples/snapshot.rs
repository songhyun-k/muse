//! Test-only adapter: frozen contract events + presentation inputs -> actual Ratatui cells.
use music_frontend::{
    art::ArtCache,
    effects::Effects,
    generated::*,
    geometry::Visual,
    motion::Motion,
    requests::{Response, Target},
    scene::Scene,
    state::{Data, EditAction, Editor, Focus, NAV, Panel, Ui, View, item_key},
    theme::Palette,
};
use ratatui::style::{Color, Modifier};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufWriter, Write};

#[derive(Deserialize)]
struct Input {
    cover: Vec<u8>,
    cases: Vec<Case>,
    #[serde(default)]
    animated: bool,
    #[serde(default)]
    benchmark: usize,
}

#[derive(Deserialize)]
struct Case {
    #[serde(default)]
    group: String,
    name: String,
    width: u16,
    height: u16,
    presentation: Value,
    events: Vec<Event>,
    albums: Vec<Item>,
    tracks: Vec<Item>,
}

#[derive(Default)]
struct Replay {
    group: String,
    motion: Option<Motion>,
    effects: Effects,
    art: ArtCache,
}

fn view(label: &str) -> View {
    NAV.into_iter()
        .chain([View::Detail, View::Lyrics])
        .find(|v| v.label() == label)
        .unwrap()
}

fn focus(value: &Value) -> Focus {
    match value.as_str().unwrap() {
        "nav" | "left" => Focus::Nav,
        "right" => Focus::Right,
        _ => Focus::Main,
    }
}

fn color(value: Color) -> Value {
    match value {
        Color::Reset => Value::Null,
        Color::Rgb(r, g, b) => json!([r, g, b]),
        _ => panic!("unexpected color"),
    }
}

fn render(
    case: Case,
    cover: &[u8],
    replay: &mut Replay,
    animated: bool,
    benchmark: usize,
) -> Value {
    if !animated || replay.group != case.group {
        *replay = Replay {
            group: case.group.clone(),
            ..Replay::default()
        };
    }
    let p = &case.presentation;
    let number = |key: &str| p[key].as_f64().unwrap();
    let flag = |key: &str| p[key].as_bool().unwrap();
    let pair = |key: &str| {
        [
            p[key]["main"].as_f64().unwrap(),
            p[key]["right"].as_f64().unwrap(),
        ]
    };
    let mut ui = Ui {
        language: music_frontend::i18n::Language::parse(p["language"].as_str().unwrap_or("ko"))
            .unwrap(),
        view: view(p["page"].as_str().unwrap()),
        back: view(p["return_page"].as_str().unwrap()),
        focus: focus(&p["focus"]),
        preferred: focus(&p["preferred_panel"]),
        panel: if p["panel"] == "lyrics" {
            Panel::Lyrics
        } else {
            Panel::Queue
        },
        cursors: pair("cursors").map(|n| n as usize),
        nav_cursor: number("nav_cursor") as usize,
        left_open: flag("left_open"),
        right_open: flag("right_open"),
        theme: number("style") as usize,
        transparent: flag("transparent"),
        plain_icons: flag("plain_icons"),
        reduced_motion: !flag("motion"),
        query: p["query"].as_str().unwrap().into(),
        help: flag("help"),
        lyric_manual: p["lyric_manual"].as_f64(),
        hover: p["hover"]
            .as_array()
            .map(|a| (a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32)),
        hover_since: number("hover_time"),
        pulse: p["pulse_key"]
            .as_str()
            .map(|key| (key.into(), number("pulse_time"))),
        favorite_pulse: p["favorite_pulse"][0]
            .as_i64()
            .filter(|i| *i >= 0)
            .map(|i| {
                (
                    case.tracks[i as usize].r#ref.clone(),
                    p["favorite_pulse"][1].as_f64().unwrap(),
                )
            }),
        toast: p["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| (s.into(), number("message_time"))),
        editor: p["editing"].as_str().map(|label| {
            Editor::new(
                match label {
                    "검색" => EditAction::Search,
                    _ => panic!("unmapped reference editor"),
                },
                p["buffer"].as_str().unwrap().into(),
            )
        }),
        ..Ui::default()
    };
    ui.fit(case.width.into(), case.height.into());
    let mut data = Data::default();
    for event in case.events {
        let target = match &event.event {
            Notice::Page(_) | Notice::Detail(_) => Some(Target::Main),
            Notice::Queue(_) => Some(Target::Queue),
            Notice::Lyrics(_) => Some(Target::Lyrics),
            _ => None,
        };
        data.apply(Response {
            target,
            sequence: event.sequence,
            notice: event.event,
        });
    }
    for item in [&case.tracks[0], &case.albums[0]] {
        data.artwork.insert(
            item_key(&item.r#ref),
            Artwork {
                item: item.r#ref.clone(),
                width: 96,
                height: 96,
                rgb: cover.to_vec(),
            },
        );
    }
    let mut visual = Visual {
        previous_track: None,
        cover_blend: 1.0,
        now: number("now"),
        art_time: number("art_time"),
        energy: number("energy"),
        position: number("position_draw"),
        playback_position: number("position"),
        track_phase: number("current"),
        volume: number("volume_draw"),
        nav_position: number("nav_position"),
        selections: pair("selections"),
        lyric_scroll: number("lyric_scroll"),
        lyric_emphasis: number("lyric_emphasis"),
        panel_widths: p["panel_sizes"]
            .as_array()
            .map(|a| [a[0].as_f64().unwrap(), a[1].as_f64().unwrap()]),
    };
    let initial = replay.motion.is_none();
    if animated {
        if initial {
            let mut motion = Motion::new(&ui, &data, visual.now, 1000.0, (case.width, case.height));
            motion.visual = visual.clone();
            replay.motion = Some(motion);
        } else {
            replay
                .effects
                .observe(&ui, &data, number("transition_time"));
            let motion = replay.motion.as_mut().unwrap();
            motion.advance(&ui, &data, visual.now, 1000.0, (case.width, case.height));
            visual = motion.visual.clone();
        }
    }
    let palette = Palette::new(ui.theme, ui.transparent);
    for album in &case.albums {
        replay.art.seed(album);
    }
    let mut times = Vec::new();
    let mut buffer = None;
    let before = replay.effects.clone();
    for sample in 0..benchmark.max(1) {
        let mut measured = before.clone();
        let effects = if sample == 0 {
            &mut replay.effects
        } else {
            &mut measured
        };
        let began = std::time::Instant::now();
        let mut scene = Scene::new(
            &ui,
            &data,
            &visual,
            &palette,
            &mut replay.art,
            case.width,
            case.height,
        );
        if animated && !initial {
            scene.draw_animated(effects);
        } else {
            scene.draw();
            if animated {
                let settled = Ui {
                    reduced_motion: true,
                    ..ui.clone()
                };
                effects.apply(&settled, &data, visual.now, &mut scene.canvas);
            }
        }
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        if sample == 0 {
            buffer = Some(scene.canvas.buffer);
        }
    }
    let rows: Vec<Vec<_>> = buffer
        .unwrap()
        .content
        .chunks(usize::from(case.width))
        .map(|row| {
            row.iter()
                .map(|cell| {
                    json!([
                        cell.symbol(),
                        if cell.modifier.contains(Modifier::DIM) {
                            json!("dim")
                        } else {
                            color(cell.fg)
                        },
                        color(cell.bg),
                        cell.modifier.contains(Modifier::BOLD),
                    ])
                })
                .collect()
        })
        .collect();
    json!({"name":case.name,"cells":rows,"renderMs":times,
        "motion":[visual.position,visual.volume,visual.nav_position,visual.lyric_scroll,visual.lyric_emphasis,
            visual.energy,visual.art_time,visual.selections[0],visual.selections[1]],"panels":visual.panel_widths})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let mut output = BufWriter::new(io::stdout().lock());
    let mut replay = Replay::default();
    for case in input.cases {
        serde_json::to_writer(
            &mut output,
            &render(
                case,
                &input.cover,
                &mut replay,
                input.animated,
                input.benchmark,
            ),
        )?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
