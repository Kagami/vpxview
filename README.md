# vpxview

Simple viewer of VPx (only VP9 currently) frame internals.

## Requirements

* Rust 1.0.0-beta.3+
* libvpx 1.4.0+
* OpenGL-capable system

## Build

```bash
git clone https://github.com/Kagami/vpxview
cd vpxview
cargo build --release
```

## Usage

```bash
ffmpeg -i file.webm -c copy file.ivf
./target/release/vpxview file.ivf
```

* Use LEFT and RIGHT arrow keys to switch between the frames
* Press Q or ESC to quit

## License

vpxview - VPx viewer

Written in 2015 by Kagami Hiiragi <kagami@genshiken.org>

To the extent possible under law, the author(s) have dedicated all copyright and related and neighboring rights to this software to the public domain worldwide. This software is distributed without any warranty.

You should have received a copy of the CC0 Public Domain Dedication along with this software. If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
