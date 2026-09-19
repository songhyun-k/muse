# Credits and third-party material

muse is an independent Apple Music terminal client. Apple Music, MusicKit and macOS
are Apple trademarks; this project is not affiliated with or endorsed by Apple.

[Yatoro](https://github.com/jayadamsmorgan/Yatoro) informed the native MusicKit
integration and the choice to build a standalone macOS terminal client. Its MIT
notice is preserved below. muse's Swift service, JSON contract and Ratatui interface
are maintained in this repository.

## Yatoro notice

MIT License

Copyright (c) 2024 Herman Berdnikov

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Other components

Ratatui, Crossterm and the remaining Rust packages keep their own licenses.
Binary archives include their license texts in `THIRD_PARTY_LICENSES.txt`;
`cargo metadata --locked --manifest-path frontend/Cargo.toml` lists their declared
licenses. Apple frameworks are supplied by macOS, not redistributed here. Nerd Font
glyphs require a separately installed font; no font files are bundled.

Album artwork, catalog metadata and lyrics returned by online providers belong
to their respective rightsholders and are not relicensed by this project's MIT
license. README previews use synthetic metadata and generated artwork.
