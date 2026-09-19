# Approved UI reference

The fixed inputs contain original fictional music metadata, newly written demo
lyrics and generated geometric cover art. There are no downloaded album covers
or commercial lyric excerpts. `provenance.json` hashes the fixture sources,
theme tokens and English catalog.

The static reference covers 240 frames: English and Korean, all 11 views, five
themes, 80×24 and 140×40, focus, queue, help, input, transparency and collapsed
panels. The animation reference covers 180 frames across both languages and ten
transitions. Every glyph, foreground/background, bold attribute and cell position
is compared exactly. Motion values use a 1e-6 tolerance; color and glyph checks
have no masks or tolerance.

```sh
python3 scripts/reference.py --check
python3 scripts/snapshots.py
python3 scripts/animations.py
```

The adapter calls the production `Scene.draw` / `draw_animated` entry points.
Production code does not read this directory. Demo assets are embedded from
`backend/Fixtures/demo.json` using `scripts/embed_demo.py`.

`--freeze` creates a missing reference and refuses to overwrite an existing one.
Changing an approved baseline requires explicit review of the intended visual
change; never replace expectations just to make a failing comparison pass.
