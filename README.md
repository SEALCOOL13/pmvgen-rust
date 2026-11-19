# PMV Generator – native (Rust)

A native desktop rewrite of [pmvgen.github.io](https://github.com/pmvgen/pmvgen.github.io) (web version 2.38.10).
Pick a song and a folder of clips – on every beat it cuts to the next one: split-screen layouts, effects
on cuts, beats and drops, speed ramps, a live fullscreen show, and an MP4 export.

Everything runs locally. Nothing is uploaded.

## Run it

**Windows:** unzip, double-click `pmvgen.exe`. ffmpeg is included in the `ffmpeg` folder.
(Using the zip without ffmpeg: install it with `winget install Gyan.FFmpeg`, or put `ffmpeg.exe` and
`ffprobe.exe` next to `pmvgen.exe`.)

**Linux** (built on Ubuntu 24.04 / Mint 22; also starts on glibc 2.35 / Mint 21):

```
sudo apt install ffmpeg
tar xzf pmvgen-linux-x86_64.tar.gz && ./pmvgen/pmvgen
```

You can also pass songs and clip folders directly: `pmvgen song.mp3 ~/Videos/clips` (or drop them onto the window).

## Using it

1. **Music** – add songs (MP3, M4A, WAV, OGG, FLAC, … or a video, then its music is used). Tempo and beats are
   found right away. **½ tempo / 2× tempo** if the detection is off by an octave, **Earlier / Later** move the
   cuts by 20 ms, **Only use … Cut** takes a part of the song. Several songs play one after another
   (optionally shuffled). DJ mixes over 45 minutes get their tempo per 2-minute stretch, like the web version.
2. **Clips** – add folders (subfolders included) or files: MP4, WebM, MOV, MKV, AVI, … and JPG, PNG, WebP,
   GIF, AVIF, BMP. Folders are remembered between runs. Filters: videos / images / both, portrait / landscape,
   best moments, clean cuts, smart crop, match cuts, variety, follow the scenes' timeline, skip start / end.
3. **Style** – moods (PMV classic, Maximal, Hypno, Clean) and the same sections as the web version: cutting
   and layouts (with a clip shape per layout), bars and phrases, reveal opening, scrolling sides, all
   19 effects, image flash / overlay, color looks, soft seams, rim effects, intro / outro, funscript.
4. **Start the show** – fullscreen live show. Keys: **Space** pause · **F** fullscreen · **Esc** stop ·
   **H** hide the bar · **I** clip info (what is on screen, fps, clips that were left out) · **N / P** next /
   previous song.
5. **Export video** – renders the show frame by frame to an MP4 (H.264 + AAC, 720p or 1080p, 30 or 60 fps).
   Nothing is dropped however slow the computer is. With **Funscript** on, a `.funscript` is written next to it.

**My settings**: save / load named settings; **Export / Import** uses the web version's `.pmvgen.json` format,
so settings move between this, the web version and the Stash plugin.

Settings live in `%APPDATA%\pmvgen\settings.json` (Windows) or `~/.config/pmvgen/settings.json` (Linux).

### Command line

```
pmvgen --export --song song.mp3 --clips DIR [--clips DIR …] --out out.mp4 [--settings my.pmvgen.json] [--from S] [--to S]
pmvgen --analyze song.mp3        # tempo, beats, energy, downbeats as JSON
```

(On Windows the program is a window app, so command-line output only shows when redirected, e.g. `2> log.txt`.)

## What is the same as the web version

The parts that decide what happens were ported line by line and **checked against the original JavaScript**:
the original modules run unmodified in Node (with a Web Audio implementation), and their results are
replayed against the Rust code (`tests/diff`). They all match: tempo, every beat time, energy, pace,
bars and phrases, ½× / 2× tempo, shifting, funscripts, layouts and the picture analysis, including the
long-mix path.

The direction (`onBeat`: when to cut, which layout, drops, effects, speed ramps, stutter, strobe, pictures)
and clip selection (best moments, clean cuts, match cuts, variety, sequencing, per-layout shapes) follow the
original's logic. The drawing (layouts, fit / fill with blurred border, zoom pulse, all effects, looks,
seams, rim, intro / outro) is a port of `pmvfx.js`.

## What is different

- **Recording → Export.** The web version records the live show in real time (WebM). This version renders
  an MP4 offline instead: every frame is there, even with 4K clips on a slow machine.
- **Not in this version:** clip audio (mixing in the clips' own sound), Plex, *PMV as template*, live control
  of The Handy (the funscript file is there), *Change the moment* / pinned moments, mouse control,
  *Clip speed follows the music* and *Motion on the beat* (both experimental in the web version), the
  Chinese translation, the *Match window* format.
- Shifting the beats (Earlier / Later) keeps each beat's energy with its beat (the web version can misalign
  them when the first beat drops out).

## Building

```
cargo build --release                         # Linux (needs libgtk-3-dev, libasound2-dev)
python3 tools_weaken_glibc.py target/release/pmvgen   # optional: lets the binary start on glibc < 2.39
```

Windows (cross-compiled from Linux with MinGW): `cargo build --release --target x86_64-pc-windows-gnu`.

Tests: `cargo test --release`; the comparison with the original: see `tests/diff/README.md`.

## Licenses

MIT (like the original – see `ORIGINAL-LICENSE.txt`). Font: Anton (SIL Open Font License, `Anton-OFL.txt`).
The bundled ffmpeg (Windows zip) is a GPL build from BtbN/FFmpeg-Builds – see `ffmpeg/LICENSE.txt`.
