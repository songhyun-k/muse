//! Render README artwork from the production UI and explicit synthetic demo data.
use music_frontend::{
    art::ArtCache,
    generated::*,
    geometry::Visual,
    i18n::Language,
    scene::Scene,
    state::{Data, Ui, item_key},
    theme::Palette,
};
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};
use std::io::{self, Write};

fn color(value: Color) -> Value {
    match value {
        Color::Rgb(r, g, b) => json!([r, g, b]),
        _ => Value::Null,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: Value = serde_json::from_reader(io::stdin().lock())?;
    let mut data = Data::default();
    data.items = serde_json::from_value(corpus["tracks"].clone())?;
    data.player = Some(serde_json::from_value(corpus["player"].clone())?);
    data.player.as_mut().unwrap().position = 36.0;
    data.volume = Some(serde_json::from_value(corpus["volume"].clone())?);
    data.session = Some(SessionState {
        authorization: Authorization::Authorized,
        can_play_catalog: Some(true),
    });
    let seed = &corpus["seed"];
    let collections: Vec<_> = seed["collections"].as_array().unwrap().iter().map(|c| json!({
        "id":c["id"],"name":c["name"],"description":c["description"],"count":c["items"].as_array().unwrap().len()
    })).collect();
    let favorites: Vec<_> = seed["favorites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["ref"].clone())
        .collect();
    data.store = Some(serde_json::from_value(json!({
        "collections":collections,"favorites":favorites,"historyCount":seed["history"].as_array().unwrap().len()
    }))?);
    let lines: Vec<_> = corpus["lyrics"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, line)| json!({"seconds":i as f64 * 18.0,"text":line}))
        .collect();
    data.lyrics = Some(serde_json::from_value(json!({
        "item":data.items[0].r#ref,"status":"synced","offset":0,"lines":lines
    }))?);
    data.artwork.insert(
        item_key(&data.items[0].r#ref),
        Artwork {
            item: data.items[0].r#ref.clone(),
            width: 96,
            height: 96,
            rgb: serde_json::from_value(corpus["cover"].clone())?,
        },
    );
    let mut output = io::BufWriter::new(io::stdout().lock());
    for (name, theme, language) in [
        ("overview-dark", 1, Language::English),
        ("overview-light", 0, Language::English),
        ("overview-ko", 0, Language::Korean),
    ] {
        let mut ui = Ui {
            theme,
            language,
            plain_icons: false,
            ..Ui::default()
        };
        ui.fit(140, 40);
        let mut visual = Visual::settled(&ui, &data, 1000.0);
        visual.lyric_scroll = 1.0;
        visual.lyric_emphasis = 2.0;
        let palette = Palette::new(theme, false);
        let mut art = ArtCache::default();
        let mut scene = Scene::new(&ui, &data, &visual, &palette, &mut art, 140, 40);
        scene.draw();
        let cells: Vec<Vec<_>> = scene
            .canvas
            .buffer
            .content
            .chunks(140)
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
                            cell.modifier.contains(Modifier::BOLD)
                        ])
                    })
                    .collect()
            })
            .collect();
        serde_json::to_writer(&mut output, &json!({"name":name,"cells":cells}))?;
        output.write_all(b"\n")?;
    }
    Ok(())
}
