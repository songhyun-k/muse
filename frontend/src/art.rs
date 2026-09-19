use crate::{
    canvas::Canvas,
    generated::{Artwork, Item, Kind, Source},
    geometry::Area,
    state::item_key,
    theme::{Ink, Palette},
};
use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

type Pixels = Vec<(Ink, Ink)>;
type CacheKey = (String, i32, i32, String, u64);

#[derive(Default)]
pub struct ArtCache {
    seeds: BTreeMap<(String, String), usize>,
    resized: BTreeMap<CacheKey, Pixels>,
}

impl ArtCache {
    pub fn seed(&mut self, item: &Item) -> usize {
        if item.r#ref.source == Source::Collection && item.r#ref.kind == Kind::Playlist {
            return 2;
        }
        // Placeholder colors describe the album, independently of catalog/library IDs.
        let key = if item.album.is_empty() {
            (item_key(&item.r#ref), String::new())
        } else {
            (item.artist.clone(), item.album.clone())
        };
        let next = self.seeds.len();
        // ponytail: remember 4096 album placeholders, then use a stable fallback hash.
        if next >= 4096 && !self.seeds.contains_key(&key) {
            return fingerprint(&key) as usize % 6;
        }
        *self.seeds.entry(key).or_insert(next)
    }

    pub fn paint(
        &mut self,
        canvas: &mut Canvas<'_>,
        area: Area,
        item: &Item,
        image: Option<&Artwork>,
    ) {
        if area.w < 2 || area.h < 1 || area.w > 96 || area.h > 48 {
            return;
        }
        let seed = self.seed(item);
        let image = image.filter(|a| {
            a.width > 0
                && a.height > 0
                && a.width <= 96
                && a.height <= 96
                && a.rgb.len() as u64 == a.width * a.height * 3
        });
        let hash = image.map_or(seed as u64, |a| fingerprint(&(a.width, a.height, &a.rgb)));
        let key = (
            item_key(&item.r#ref),
            area.w,
            area.h,
            canvas.palette.name.clone(),
            hash,
        );
        if !self.resized.contains_key(&key) {
            // ponytail: 192 previews; replace ordered eviction only after measured cache churn.
            if self.resized.len() >= 192 {
                self.resized.pop_first();
            }
            let pixels = (0..area.h)
                .flat_map(|y| (0..area.w).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let pixel = |yy| match image {
                        Some(image) => sample(image, x, yy, area.w, area.h * 2),
                        None => placeholder(
                            (f64::from(x) + 0.5) / f64::from(area.w) * 2.0 - 1.0,
                            (f64::from(yy) + 0.5) / f64::from(area.h * 2) * 2.0 - 1.0,
                            seed,
                            canvas.palette,
                        ),
                    };
                    (pixel(y * 2), pixel(y * 2 + 1))
                })
                .collect();
            self.resized.insert(key.clone(), pixels);
        }
        for (i, &(top, bottom)) in self.resized[&key].iter().enumerate() {
            canvas.pixel(
                area.x + i as i32 % area.w,
                area.y + i as i32 / area.w,
                top,
                bottom,
            );
        }
    }
}

fn fingerprint(value: &impl Hash) -> u64 {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}

fn sample(image: &Artwork, x: i32, y: i32, width: i32, height: i32) -> Ink {
    let sw = image.width as usize;
    let sh = image.height as usize;
    let xa = x as usize * sw / width as usize;
    let xb = ((x as usize + 1) * sw / width as usize).max(xa + 1).min(sw);
    let ya = y as usize * sh / height as usize;
    let yb = ((y as usize + 1) * sh / height as usize)
        .max(ya + 1)
        .min(sh);
    let mut sum = [0_u64; 3];
    let mut count = 0;
    for yy in ya.min(sh - 1)..yb {
        for xx in xa.min(sw - 1)..xb {
            for (channel, total) in sum.iter_mut().enumerate() {
                *total += u64::from(image.rgb[(yy * sw + xx) * 3 + channel]);
            }
            count += 1;
        }
    }
    let [r, g, b] = sum.map(|n| (n / count) as u8);
    Ink::Rgb(r, g, b)
}

fn placeholder(x: f64, y: f64, seed: usize, p: &Palette) -> Ink {
    let bg = p["art.background"];
    let a = p["art.primary"];
    let b = p["art.secondary"];
    let fg = p["art.foreground"];
    let phase = seed as f64;
    match (seed + 1) % 5 {
        0 => {
            let radius = x.hypot(y);
            let mut color = bg.mix(b, (-2.7 * radius * radius).exp() * 0.24);
            let r = (x + 0.03).hypot(y + 0.02);
            if r < 0.76 {
                let z = (1.0 - (r / 0.76).powi(2)).max(0.0).sqrt();
                let light = (0.35 - x * 0.4 - y * 0.35 + z * 0.3).clamp(0.0, 1.0);
                let band = 0.5 + 0.5 * (y * 8.0 + x * 3.0 + phase * 0.5).sin();
                color = bg.mix(a.mix(b, band), light * 0.92);
                color = color.mix(
                    fg,
                    (-((x + 0.31).powi(2) + (y + 0.38).powi(2)) * 35.0).exp() * 0.32,
                );
            }
            color
        }
        1 => {
            let mut color = bg;
            for i in 0..3 {
                let i = f64::from(i);
                let ridge = (x * 2.4 + phase * 0.35 + i * 0.7).sin() * 0.35 + (i - 1.0) * 0.27;
                let glow = (-((y - ridge) / (0.17 + i * 0.03)).powi(2)).exp() * (0.56 - i * 0.09);
                color = color.mix(a.mix(b, i / 2.0), glow);
            }
            color.mix(
                fg,
                (-((x + 0.44).powi(2) + (y + 0.2).powi(2)) * 24.0).exp() * 0.18,
            )
        }
        2 => {
            let r = (x + 0.18).hypot(y - 0.08);
            let mut color = bg.mix(b, (0.75 - r).clamp(0.0, 1.0) * 0.08);
            if r > 0.27 && r < 0.85 {
                let grooves = 0.86 + 0.14 * (r * 95.0).cos();
                color = bg.mix(a, (0.48 + 0.25 * (1.0 - x)) * grooves);
            }
            if (x - 0.5).hypot(y + 0.48) < 0.18 {
                color = a.mix(fg, 0.12);
            }
            color
        }
        3 => {
            let r = x.hypot(y + 0.04);
            let corona = (-(r - 0.62).abs() * 18.0).exp()
                * (0.7 + 0.3 * (y.atan2(x) * 3.0 + phase * 0.5).sin());
            let color = if r < 0.55 {
                bg
            } else {
                bg.mix(a, corona * 0.95)
            };
            color.mix(b, (-((y - 0.86) * 8.0).powi(2)).exp() * 0.12)
        }
        _ => {
            let mut color = bg;
            for i in 0..2 {
                let curve = (x * 2.8 - phase * 0.28 + f64::from(i) * 2.0).sin() * 0.45;
                let distance = y - curve;
                if distance.abs() < 0.24 {
                    let shine = (distance / 0.24 * std::f64::consts::PI / 2.0).cos().powi(2);
                    color = color.mix(
                        a.mix(
                            b,
                            if i == 0 {
                                (x + 1.0) / 2.0
                            } else {
                                (1.0 - x) / 2.0
                            },
                        ),
                        shine * 0.85,
                    );
                    color = color.mix(fg, (-((distance + 0.06) * 30.0).powi(2)).exp() * 0.4);
                }
            }
            color
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::ItemRef;
    #[test]
    fn art_preserves_pixel_orientation_and_rejects_invalid_dimensions() {
        let item: Item = serde_json::from_value(serde_json::json!({
            "ref":{"source":"catalog","kind":"song","id":"one"},
            "title":"one","artist":"artist","album":"album"
        }))
        .unwrap();
        let mut image = Artwork {
            item: ItemRef {
                id: "one".into(),
                source: Source::Catalog,
                kind: Kind::Song,
            },
            width: 2,
            height: 2,
            rgb: vec![255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255],
        };
        let palette = Palette::new(0, true);
        let mut cache = ArtCache::default();
        let mut library_item = item.clone();
        library_item.r#ref.source = Source::Library;
        assert_eq!(cache.seed(&item), cache.seed(&library_item));
        let mut canvas = Canvas::new(4, 2, &palette);
        let area = Area::new(0, 0, 2, 1);
        cache.paint(&mut canvas, area, &item, Some(&image));
        assert_eq!(canvas.buffer[(0, 0)].fg, Ink::Rgb(255, 0, 0).color());
        assert_eq!(canvas.buffer[(0, 0)].bg, Ink::Rgb(0, 0, 255).color());
        image.rgb.fill(120);
        cache.paint(&mut canvas, area, &item, Some(&image));
        assert_eq!(canvas.buffer[(0, 0)].fg, Ink::Rgb(120, 120, 120).color());
        image.width = u64::MAX;
        cache.paint(&mut canvas, area, &item, Some(&image));
        assert_eq!(canvas.buffer[(0, 0)].symbol(), "▀");
        assert!(cache.resized.len() <= 192);
    }
}
