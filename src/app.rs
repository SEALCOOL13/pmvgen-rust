// The window: pick songs and clip folders, set the style, then play the show live or export a video.

use crate::export::{self, Progress};
use crate::library::{self, Kind};
use crate::plan::LAYOUTS;
use crate::player::{self, Player};
use crate::settings::{self, Settings, PRESETS};
use crate::show::{self, Built};
use crate::song::{self, Song};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PINK: Color32 = Color32::from_rgb(255, 62, 138);
const BG: Color32 = Color32::from_rgb(18, 9, 16);
const CARD: Color32 = Color32::from_rgb(35, 15, 31);
const DIM: Color32 = Color32::from_rgb(190, 160, 180);

// ---------- helpers ----------

/// "1:23", "1:02:03", "83" → seconds; "" → 0; None = not a time
fn parse_time(v: &str) -> Option<f64> {
    let v = v.trim().replace(',', ".");
    if v.is_empty() {
        return Some(0.0);
    }
    let mut acc = 0.0;
    for p in v.split(':') {
        let n: f64 = p.trim().parse().ok()?;
        if !n.is_finite() || n < 0.0 {
            return None;
        }
        acc = acc * 60.0 + n;
    }
    Some(acc)
}
fn fmt_time(sec: f64) -> String {
    let sec = sec.max(0.0).round() as i64;
    let (h, m, x) = (sec / 3600, (sec % 3600) / 60, sec % 60);
    if h > 0 {
        format!("{h}:{m:02}:{x:02}")
    } else {
        format!("{m}:{x:02}")
    }
}

fn icon() -> egui::IconData {
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
    let mut pm = Pixmap::new(64, 64).unwrap();
    let mut p = Paint {
        anti_alias: true,
        ..Default::default()
    };
    p.set_color_rgba8(35, 15, 31, 255);
    if let Some(r) = Rect::from_xywh(0.0, 0.0, 64.0, 64.0) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(16.0, 26.0);
    pb.cubic_to(16.0, 26.0, 8.0, 21.1, 5.8, 16.1);
    pb.cubic_to(4.2, 12.5, 6.4, 8.5, 10.3, 8.5);
    pb.cubic_to(12.6, 8.5, 14.4, 9.9, 16.0, 11.9);
    pb.cubic_to(17.5, 9.9, 19.3, 8.5, 21.6, 8.5);
    pb.cubic_to(25.5, 8.5, 27.8, 12.5, 26.2, 16.1);
    pb.cubic_to(24.0, 21.1, 16.0, 26.0, 16.0, 26.0);
    pb.close();
    p.set_color_rgba8(255, 62, 138, 255);
    if let Some(path) = pb.finish() {
        pm.fill_path(
            &path,
            &p,
            FillRule::Winding,
            Transform::from_scale(2.0, 2.0),
            None,
        );
    }
    egui::IconData {
        rgba: pm.data().to_vec(),
        width: 64,
        height: 64,
    }
}

fn open_folder(p: &Path) {
    #[cfg(windows)]
    {
        let _ = crate::ff::command(Path::new("explorer"))
            .arg(format!("/select,{}", p.display()))
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let dir = p.parent().unwrap_or(p);
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

// ---------- state ----------

enum Load {
    Loading(f32),
    Ready(Box<Song>),
    Failed(String),
}

struct SongEntry {
    path: PathBuf,
    from: String,
    to: String,
    state: Arc<Mutex<Load>>,
    cancel: Arc<AtomicBool>,
}

struct Live {
    built: Built,
    player: Player,
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    started: Instant,
    playing: bool,
    tex: Option<egui::TextureHandle>,
    last_move: Instant,
    bar_hidden: bool,
    info: bool,
    order: Vec<usize>,
    order_pos: usize,
    msg: Option<(String, Instant)>,
    fps: (Instant, u32, f32),
}

struct ExportJob {
    prog: Arc<Mutex<Progress>>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<PathBuf, String>>>>,
    started: Instant,
}

enum Screen {
    Setup,
    Live(Box<Live>),
    Export(ExportJob),
}

pub struct App {
    s: Settings,
    stored: settings::Stored,
    saved_json: String,
    save_check: Instant,
    lib: library::Shared,
    probing: Arc<AtomicUsize>,
    songs: Vec<SongEntry>,
    sel: usize,
    screen: Screen,
    toast: Option<(String, Instant, bool)>,
    preset_name: String,
    device_rate: u32,
    ff_err: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> App {
        let mut v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = Color32::from_rgb(12, 5, 11);
        v.selection.bg_fill = PINK.linear_multiply(0.6);
        v.hyperlink_color = PINK;
        v.widgets.hovered.bg_stroke.color = PINK;
        cc.egui_ctx.set_visuals(v);
        let stored = settings::load_stored();
        let s = settings::settings_from_stored(&stored);
        let lib: library::Shared = Default::default();
        let probing = Arc::new(AtomicUsize::new(0));
        // the folders from last time are added again (in the background)
        let folders = stored.folders.clone();
        if !folders.is_empty() {
            let lib2 = lib.clone();
            let pr = probing.clone();
            std::thread::spawn(move || {
                for f in folders {
                    let p = PathBuf::from(&f);
                    if p.is_dir() {
                        library::add_folder(&lib2, &p);
                    }
                }
                library::probe_all(&lib2, &pr);
            });
        }
        let ff_err = crate::ff::available().err().map(|e| e.to_string());
        App {
            saved_json: serde_json::to_string(&s).unwrap_or_default(),
            s,
            stored,
            save_check: Instant::now(),
            lib,
            probing,
            songs: Vec::new(),
            sel: 0,
            screen: Screen::Setup,
            toast: None,
            preset_name: String::new(),
            device_rate: player::device_rate(),
            ff_err,
        }
    }

    fn toast(&mut self, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), Instant::now(), ok));
    }

    fn persist(&mut self) {
        let j = serde_json::to_string(&self.s).unwrap_or_default();
        let folders: Vec<String> = self
            .lib
            .lock()
            .unwrap()
            .roots
            .iter()
            .filter(|r| r.is_dir())
            .map(|r| r.to_string_lossy().to_string())
            .collect();
        if j != self.saved_json || folders != self.stored.folders {
            self.stored.settings = serde_json::to_value(&self.s).ok();
            self.stored.folders = folders;
            if settings::save_stored(&self.stored).is_ok() {
                self.saved_json = j;
            }
        }
    }

    // ---------- songs ----------

    fn add_song(&mut self, path: PathBuf) {
        if self.songs.iter().any(|s| s.path == path) {
            return;
        }
        let e = SongEntry {
            path,
            from: String::new(),
            to: String::new(),
            state: Arc::new(Mutex::new(Load::Loading(0.0))),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.start_load(&e, 0.0, None);
        self.songs.push(e);
        if self.songs.len() == 1 {
            self.sel = 0;
        }
    }
    fn start_load(&self, e: &SongEntry, from: f64, to: Option<f64>) {
        let path = e.path.clone();
        let st = e.state.clone();
        let cancel = e.cancel.clone();
        let rate = self.device_rate;
        *st.lock().unwrap() = Load::Loading(0.0);
        std::thread::spawn(move || {
            let st2 = st.clone();
            let r = song::load(
                &path,
                from,
                to,
                rate,
                &|p| *st2.lock().unwrap() = Load::Loading(p),
                &cancel,
            );
            *st.lock().unwrap() = match r {
                Ok(s) => Load::Ready(Box::new(s)),
                Err(e) => Load::Failed(e.to_string()),
            };
        });
    }
    fn song_ready(&self, i: usize) -> Option<Song> {
        match &*self.songs.get(i)?.state.lock().unwrap() {
            Load::Ready(s) => Some((**s).clone()),
            _ => None,
        }
    }
    fn with_song(&mut self, f: impl FnOnce(&mut Song)) {
        if let Some(e) = self.songs.get(self.sel) {
            if let Load::Ready(s) = &mut *e.state.lock().unwrap() {
                f(s);
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        self.open_paths(dropped);
    }

    /// Songs become songs, folders and video / image files become clips
    fn open_paths(&mut self, dropped: Vec<PathBuf>) {
        if dropped.is_empty() {
            return;
        }
        let mut clips = Vec::new();
        for p in dropped {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let audio = matches!(
                ext.as_str(),
                "mp3"
                    | "m4a"
                    | "aac"
                    | "wav"
                    | "ogg"
                    | "oga"
                    | "opus"
                    | "flac"
                    | "wma"
                    | "aiff"
                    | "aif"
            );
            if p.is_dir() {
                let n = library::add_folder(&self.lib, &p);
                self.toast(format!("{n} clips added"), true);
            } else if audio {
                self.add_song(p);
            } else if library::kind_of(&p).is_some() {
                clips.push(p);
            }
        }
        if !clips.is_empty() {
            let n = library::add_files(&self.lib, &clips);
            self.toast(format!("{n} clips added"), true);
        }
        library::probe_all(&self.lib, &self.probing);
    }

    // ---------- start / export ----------

    fn clip_lists(&self) -> (Vec<library::Item>, Vec<library::Item>) {
        let l = self.lib.lock().unwrap();
        (l.matching(&self.s), l.images())
    }

    fn start_show(&mut self, song_idx: usize, order: Vec<usize>, order_pos: usize) {
        let Some(song) = self.song_ready(song_idx) else {
            self.toast("The song isn't ready yet", false);
            return;
        };
        let (items, images) = self.clip_lists();
        match show::build(&song, items, images, &self.s, self.s.show_quality, false) {
            Ok(built) => {
                let player = Player::new(song.pcm.clone(), (self.s.song_vol / 100.0) as f32);
                let stop = Arc::new(AtomicBool::new(false));
                let ready = Arc::new(AtomicBool::new(false));
                show::run_scheduler(built.plan.clone(), built.media.clone(), stop.clone());
                {
                    let (plan, media, ready) =
                        (built.plan.clone(), built.media.clone(), ready.clone());
                    std::thread::spawn(move || {
                        show::preroll(&plan, &media, Instant::now() + Duration::from_secs(20));
                        ready.store(true, Ordering::SeqCst);
                    });
                }
                let msg = player.device_error.as_ref().map(|e| {
                    (
                        format!("No sound ({e}) – the show runs silently"),
                        Instant::now(),
                    )
                });
                self.screen = Screen::Live(Box::new(Live {
                    built,
                    player,
                    stop,
                    ready,
                    started: Instant::now(),
                    playing: false,
                    tex: None,
                    last_move: Instant::now(),
                    bar_hidden: false,
                    info: false,
                    order,
                    order_pos,
                    msg,
                    fps: (Instant::now(), 0, 0.0),
                }));
            }
            Err(e) => self.toast(e.to_string(), false),
        }
    }

    fn go(&mut self) {
        let n = self.songs.len();
        if n == 0 {
            return;
        }
        let mut order: Vec<usize> = (0..n).collect();
        if self.s.shuffle {
            use rand::seq::SliceRandom;
            order.shuffle(&mut rand::thread_rng());
            // the selected song first
            if let Some(p) = order.iter().position(|&i| i == self.sel) {
                order.swap(0, p);
            }
        } else {
            order.rotate_left(self.sel.min(n - 1));
        }
        let first = order[0];
        self.start_show(first, order, 0);
    }

    fn stop_show(&mut self, ctx: &egui::Context) {
        if let Screen::Live(l) = &self.screen {
            l.stop.store(true, Ordering::SeqCst);
            l.built.media.stop();
            l.player.pause();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        self.screen = Screen::Setup;
    }

    fn export(&mut self) {
        let Some(song) = self.song_ready(self.sel) else {
            self.toast("Pick a song (and wait until it is read) first", false);
            return;
        };
        let title = if self.s.title.trim().is_empty() {
            song.name.clone()
        } else {
            self.s.title.trim().to_string()
        };
        let mut dlg = rfd::FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
            .set_file_name(format!("{}.mp4", export::file_name(&title)));
        if let Some(d) = &self.stored.last_dir {
            dlg = dlg.set_directory(d);
        }
        let Some(mut out) = dlg.save_file() else {
            return;
        };
        if out.extension().is_none() {
            out.set_extension("mp4");
        }
        self.stored.last_dir = out.parent().map(|p| p.to_string_lossy().to_string());
        let (items, images) = self.clip_lists();
        let s = self.s.clone();
        let prog = Arc::new(Mutex::new(Progress::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        {
            let (prog, cancel, result) = (prog.clone(), cancel.clone(), result.clone());
            std::thread::spawn(move || {
                let r = export::run(&song, items, images, &s, &out, &prog, &cancel)
                    .map_err(|e| e.to_string());
                *result.lock().unwrap() = Some(r);
            });
        }
        self.screen = Screen::Export(ExportJob {
            prog,
            cancel,
            result,
            started: Instant::now(),
        });
    }

    // ---------- the setup page ----------

    fn card(ui: &mut egui::Ui, title: &str, sub: &str, body: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(10.0)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title)
                            .size(18.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.label(RichText::new(sub).color(DIM));
                });
                ui.add_space(6.0);
                body(ui);
            });
        ui.add_space(10.0);
    }

    fn music_card(&mut self, ui: &mut egui::Ui) {
        let mut add: Vec<PathBuf> = Vec::new();
        let mut remove = None;
        Self::card(
            ui,
            "1  Music",
            "a song (or several) – tempo and beats are found here",
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("➕ Add songs …").clicked() {
                        if let Some(f) = rfd::FileDialog::new()
                            .add_filter(
                                "Music",
                                &[
                                    "mp3", "m4a", "aac", "wav", "ogg", "opus", "flac", "wma",
                                    "aiff",
                                ],
                            )
                            .add_filter(
                                "Video (its music)",
                                &["mp4", "m4v", "mkv", "webm", "mov", "avi"],
                            )
                            .pick_files()
                        {
                            add.extend(f);
                        }
                    }
                    if ui.button("📁 Add a folder of songs …").clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            if let Ok(rd) = std::fs::read_dir(&d) {
                                let mut v: Vec<PathBuf> = rd
                                    .flatten()
                                    .map(|e| e.path())
                                    .filter(|p| {
                                        p.is_file()
                                            && song::is_song_file(p)
                                            && library::kind_of(p) != Some(Kind::Video)
                                    })
                                    .collect();
                                v.sort();
                                add.extend(v);
                            }
                        }
                    }
                    ui.checkbox(&mut self.s.shuffle, "Shuffle")
                        .on_hover_text("Several songs: play them in a random order");
                });
                if self.songs.is_empty() {
                    ui.label(RichText::new("Drop songs onto this window, or add them above (MP3, M4A, WAV, OGG, FLAC – or a video, then its music is used).").color(DIM));
                }
                for (i, e) in self.songs.iter().enumerate() {
                    let st = e.state.lock().unwrap();
                    let status = match &*st {
                        Load::Loading(p) => format!("reading … {:.0} %", p * 100.0),
                        Load::Ready(s) => format!(
                            "{:.0} BPM · {} · {} beats",
                            s.a.bpm,
                            fmt_time(s.duration()),
                            s.a.beats.len()
                        ),
                        Load::Failed(e) => format!("⚠ {e}"),
                    };
                    drop(st);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.sel == i, song::song_name(&e.path))
                            .clicked()
                        {
                            self.sel = i;
                        }
                        ui.label(RichText::new(status).color(DIM));
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
            },
        );
        if let Some(i) = remove {
            self.songs[i].cancel.store(true, Ordering::SeqCst);
            self.songs.remove(i);
            if self.sel >= self.songs.len() {
                self.sel = self.songs.len().saturating_sub(1);
            }
        }
        for p in add {
            self.add_song(p);
        }
    }

    fn song_tools(&mut self, ui: &mut egui::Ui) {
        let Some(song) = self.song_ready(self.sel) else {
            return;
        };
        Self::card(ui, &song.name, "the beat grid", |ui| {
            draw_wave(ui, &song);
            ui.horizontal_wrapped(|ui| {
                let mut act: Option<u8> = None;
                if ui
                    .button("½ tempo")
                    .on_hover_text("The cuts come twice as fast as they should")
                    .clicked()
                {
                    act = Some(0);
                }
                if ui
                    .button("2× tempo")
                    .on_hover_text("The cuts come only on every other beat")
                    .clicked()
                {
                    act = Some(1);
                }
                if ui
                    .button("◀ Earlier")
                    .on_hover_text("The cuts come too late: 20 ms earlier")
                    .clicked()
                {
                    act = Some(2);
                }
                if ui
                    .button("Later ▶")
                    .on_hover_text("The cuts come too early: 20 ms later")
                    .clicked()
                {
                    act = Some(3);
                }
                if (song.tempo != 1.0 || song.shift_ms != 0.0) && ui.button("Reset").clicked() {
                    act = Some(4);
                }
                ui.label(
                    RichText::new(format!(
                        "{:.1} BPM{}",
                        song.a.bpm,
                        if song.shift_ms != 0.0 {
                            format!(" · {:+.0} ms", song.shift_ms)
                        } else {
                            String::new()
                        }
                    ))
                    .color(DIM),
                );
                if let Some(a) = act {
                    self.with_song(|s| match a {
                        0 => s.rescale(0.5),
                        1 => s.rescale(2.0),
                        2 => s.nudge(-20.0),
                        3 => s.nudge(20.0),
                        _ => s.reset_grid(),
                    });
                }
            });
            ui.horizontal(|ui| {
                let e = &mut self.songs[self.sel];
                ui.label("Only use");
                ui.add(
                    egui::TextEdit::singleline(&mut e.from)
                        .desired_width(60.0)
                        .hint_text("0:00"),
                );
                ui.label("to");
                ui.add(
                    egui::TextEdit::singleline(&mut e.to)
                        .desired_width(60.0)
                        .hint_text(fmt_time(song.duration() + song.from)),
                );
                let cut = ui
                    .button("✂ Cut")
                    .on_hover_text("Take just a part of the song (e.g. without the intro)")
                    .clicked();
                let whole = song.from > 0.0 || song.to.is_some();
                let again = whole && ui.button("Whole song again").clicked();
                if cut || again {
                    let (f, t) = if again {
                        (Some(0.0), Some(0.0))
                    } else {
                        (parse_time(&e.from), parse_time(&e.to))
                    };
                    match (f, t) {
                        (Some(f), Some(t)) => {
                            let to = if t > 0.0 { Some(t) } else { None };
                            if to.is_some_and(|t| t <= f + 2.0) {
                                self.toast = Some((
                                    "The end has to come after the start".into(),
                                    Instant::now(),
                                    false,
                                ));
                            } else {
                                if again {
                                    e.from.clear();
                                    e.to.clear();
                                }
                                let (from, to) = if again { (0.0, None) } else { (f, to) };
                                let e2 = SongEntry {
                                    path: e.path.clone(),
                                    from: e.from.clone(),
                                    to: e.to.clone(),
                                    state: e.state.clone(),
                                    cancel: e.cancel.clone(),
                                };
                                self.start_load(&e2, from, to);
                            }
                        }
                        _ => {
                            self.toast =
                                Some(("Times like 1:23 or 83".into(), Instant::now(), false))
                        }
                    }
                }
            });
        });
    }

    fn clips_card(&mut self, ui: &mut egui::Ui) {
        let (v, im, pending) = self.lib.lock().unwrap().count(&self.s);
        let mut add_dir = None;
        let mut add_files = None;
        let mut remove = None;
        let s = &mut self.s;
        let lib = self.lib.clone();
        Self::card(
            ui,
            "2  Clips",
            "folders with your videos and images (subfolders included)",
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("📁 Add a folder …").clicked() {
                        add_dir = rfd::FileDialog::new().pick_folder();
                    }
                    if ui.button("➕ Add files …").clicked() {
                        add_files = rfd::FileDialog::new()
                            .add_filter(
                                "Clips",
                                &[
                                    "mp4", "m4v", "webm", "mkv", "mov", "avi", "wmv", "jpg",
                                    "jpeg", "png", "webp", "gif", "avif", "bmp",
                                ],
                            )
                            .pick_files();
                    }
                    ui.label(
                        RichText::new(format!(
                            "{v} videos · {im} images match{}",
                            if pending > 0 {
                                format!(" · reading {pending} …")
                            } else {
                                String::new()
                            }
                        ))
                        .color(DIM),
                    );
                });
                {
                    let mut l = lib.lock().unwrap();
                    let roots = l.roots.clone();
                    for r in roots {
                        let n = l.items.iter().filter(|i| i.root == r).count();
                        let bad = l
                            .items
                            .iter()
                            .filter(|i| i.root == r && i.bad.is_some())
                            .count();
                        let mut on = !l.off.contains(&r);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut on, r.to_string_lossy()).changed() {
                                if on {
                                    l.off.remove(&r);
                                } else {
                                    l.off.insert(r.clone());
                                }
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{n} files{}",
                                    if bad > 0 {
                                        format!(", {bad} unreadable")
                                    } else {
                                        String::new()
                                    }
                                ))
                                .color(DIM),
                            );
                            if ui.small_button("✕").clicked() {
                                remove = Some(r.clone());
                            }
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("What");
                    combo(
                        ui,
                        "src",
                        &mut s.source,
                        &[("scene", "Videos"), ("image", "Images"), ("both", "Both")],
                    );
                    ui.label("Clip shape");
                    combo(
                        ui,
                        "shape",
                        &mut s.shape,
                        &[
                            ("all", "All"),
                            ("portrait", "Portrait only"),
                            ("landscape", "Landscape only"),
                        ],
                    );
                });
                egui::CollapsingHeader::new("Clip selection").default_open(false).show(ui, |ui| {
                sw(ui, &mut s.best_spots, "Best moments", "Looks at several spots of each video and takes the one with the most going on");
                sw(ui, &mut s.clean_cuts, "Clean cuts", "A clip starts where its scene runs on for the next few seconds – no hidden scene change inside the clip");
                sw(ui, &mut s.smart_crop, "Smart crop", "Cropping follows what matters in the picture");
                sw(ui, &mut s.match_cut, "Match cuts", "The next clip looks like the one leaving (color, brightness, where the subject is)");
                sw(ui, &mut s.variety, "Variety", "The same clip not shortly after itself");
                sw(ui, &mut s.sequenced, "Follow the scenes' timeline", "How far the song is = where in each video the clip comes from");
                ui.add(egui::Slider::new(&mut s.skip_head, 0.0..=300.0).step_by(5.0).suffix(" s").text("Skip the start of each video"));
                ui.add(egui::Slider::new(&mut s.skip_tail, 0.0..=300.0).step_by(5.0).suffix(" s").text("Skip the end of each video"));
            });
            },
        );
        if let Some(d) = add_dir {
            let lib = self.lib.clone();
            let pr = self.probing.clone();
            std::thread::spawn(move || {
                library::add_folder(&lib, &d);
                library::probe_all(&lib, &pr);
            });
        }
        if let Some(f) = add_files {
            library::add_files(&self.lib, &f);
            library::probe_all(&self.lib, &self.probing);
        }
        if let Some(r) = remove {
            self.lib.lock().unwrap().remove_root(&r);
        }
        // anything added while the probing ran
        if pending > 0 && self.probing.load(Ordering::SeqCst) == 0 {
            library::probe_all(&self.lib, &self.probing);
        }
    }

    fn style_card(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.s;
        Self::card(ui, "3  Style", "how it cuts and looks", |ui| {
            ui.horizontal(|ui| {
                ui.label("Mood");
                for p in PRESETS.iter() {
                    if ui.button(p.name).clicked() {
                        s.apply_mood(p);
                    }
                }
            });
            section(ui, "Cutting", "When to cut and which split screens", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Cut");
                    combo(
                        ui,
                        "cut",
                        &mut s.cut,
                        &[
                            ("auto", "Automatic"),
                            ("1", "Every beat"),
                            ("2", "Every 2 beats"),
                            ("4", "Every 4 beats"),
                        ],
                    );
                    if s.cut == "auto" {
                        ui.label("Pace");
                        combo(
                            ui,
                            "pace",
                            &mut s.pace,
                            &[("slow", "Slow"), ("normal", "Normal"), ("fast", "Fast")],
                        );
                    }
                });
                ui.label(RichText::new("Layouts (and the clip shape each one takes)").color(DIM));
                egui::Grid::new("layouts")
                    .num_columns(3)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for l in LAYOUTS.iter() {
                            ui.checkbox(s.layouts.get_mut(l.id), l.name)
                                .on_hover_text(l.hint);
                            let mut v = s
                                .layout_shape
                                .get(l.id)
                                .cloned()
                                .unwrap_or_else(|| "all".into());
                            let before = v.clone();
                            combo(
                                ui,
                                &format!("ls{}", l.id),
                                &mut v,
                                &[
                                    ("all", "Any shape"),
                                    ("portrait", "Portrait"),
                                    ("landscape", "Landscape"),
                                ],
                            );
                            if v != before {
                                if v == "all" {
                                    s.layout_shape.remove(l.id);
                                } else {
                                    s.layout_shape.insert(l.id.to_string(), v);
                                }
                            }
                            ui.end_row();
                        }
                    });
                if !settings::LAYOUT_IDS.iter().any(|id| s.layouts.get(id)) {
                    s.layouts.full = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Split");
                    combo(
                        ui,
                        "split",
                        &mut s.split,
                        &[("cols", "Side by side"), ("rows", "Stacked")],
                    );
                });
                sw(ui, &mut s.bars, "Bars and phrases", "Cuts sit on the bar's strong beats, split screens change at the start of a phrase");
                sw(
                    ui,
                    &mut s.reveal,
                    "Reveal opening",
                    "The clip sits small in the middle and grows until the first drop",
                );
                sw(
                    ui,
                    &mut s.scroll,
                    "Scrolling sides",
                    "In 3-way layouts the middle clip stays, the sides scroll like a feed",
                );
                ui.add(
                    egui::Slider::new(&mut s.cut_lead, 0.0..=150.0)
                        .step_by(5.0)
                        .suffix(" ms")
                        .text("Cut ahead of the beat"),
                );
                ui.add(
                    egui::Slider::new(&mut s.min_beats, 0.0..=16.0)
                        .step_by(1.0)
                        .text("Shortest clip (beats, 0 = off)"),
                );
                ui.add(
                    egui::Slider::new(&mut s.max_beats, 0.0..=32.0)
                        .step_by(1.0)
                        .text("Longest clip (beats, 0 = off)"),
                );
            });
            section(
                ui,
                "Effects",
                "What happens on cuts, beats and drops",
                |ui| {
                    for (g, sub, keys) in [
                        (
                            "On cuts",
                            "When a new clip comes in",
                            &["whip", "zoomin", "flash"][..],
                        ),
                        (
                            "On the beat",
                            "Run through the whole song",
                            &["zoom", "shake", "speed", "stutter", "strobe"][..],
                        ),
                        (
                            "On drops",
                            "The big moments",
                            &["rgb", "glitch", "tunnel", "invert", "echo", "text"][..],
                        ),
                    ] {
                        ui.label(RichText::new(format!("{g} – {sub}")).color(DIM));
                        ui.horizontal_wrapped(|ui| {
                            for k in keys {
                                let (name, desc) = fx_text(k);
                                ui.checkbox(s.fx.get_mut(k), name).on_hover_text(desc);
                            }
                        });
                    }
                    if s.fx.zoom {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Slider::new(&mut s.pulse_amt, 0.0..=150.0)
                                    .suffix(" %")
                                    .text("Zoom pulse"),
                            );
                            combo(
                                ui,
                                "pon",
                                &mut s.pulse_on,
                                &[
                                    ("beat", "every beat"),
                                    ("2", "every 2nd beat"),
                                    ("bar", "each bar"),
                                ],
                            );
                        });
                    }
                    if s.fx.text {
                        ui.horizontal(|ui| {
                            ui.label("Words");
                            ui.add(
                                egui::TextEdit::singleline(&mut s.words)
                                    .hint_text("comma separated")
                                    .desired_width(300.0),
                            );
                        });
                    }
                },
            );
            section(
                ui,
                "Pictures",
                "Images that flash between the clips or lie over them",
                |ui| {
                    sw(
                        ui,
                        &mut s.pic_flash,
                        "Image flash",
                        "Still images from your library flash between the clips",
                    );
                    if s.pic_flash {
                        ui.horizontal(|ui| {
                            combo(
                                ui,
                                "pfr",
                                &mut s.pic_flash_rate,
                                &[
                                    ("auto", "Auto (follows the song)"),
                                    ("1", "1 per beat"),
                                    ("2", "2 per beat"),
                                    ("4", "4 per beat"),
                                    ("8", "8 per beat"),
                                ],
                            );
                            combo(
                                ui,
                                "pfm",
                                &mut s.pic_flash_mode,
                                &[
                                    ("single", "One picture"),
                                    ("burst", "A burst"),
                                    ("both", "Both"),
                                ],
                            );
                        });
                        ui.add(
                            egui::Slider::new(&mut s.pic_flash_len, 1.0..=8.0)
                                .step_by(1.0)
                                .text("Burst length (beats)"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.pic_flash_pct, 0.0..=100.0)
                                .suffix(" %")
                                .text("Beats that flash"),
                        );
                    }
                    sw(
                        ui,
                        &mut s.pic_overlay,
                        "Image overlay",
                        "A picture lies see-through over the playing clips",
                    );
                    if s.pic_overlay {
                        ui.add(
                            egui::Slider::new(&mut s.pic_overlay_amt, 10.0..=90.0)
                                .suffix(" %")
                                .text("How solid"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.pic_overlay_len, 1.0..=16.0)
                                .step_by(1.0)
                                .text("For beats"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.pic_overlay_pct, 0.0..=100.0)
                                .suffix(" %")
                                .text("Bars that get one"),
                        );
                    }
                },
            );
            section(
                ui,
                "Color & look",
                "Color mood, brightness and film effects",
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Look");
                        combo(
                            ui,
                            "look",
                            &mut s.look,
                            &[
                                ("none", "Original"),
                                ("warm", "Warm"),
                                ("pink", "Pink"),
                                ("cold", "Cold"),
                                ("vivid", "Vivid"),
                                ("bw", "Black & white"),
                                ("noir", "Noir"),
                                ("custom", "Own color"),
                            ],
                        );
                        if s.look == "custom" {
                            let mut c = crate::compositor::hex_rgb(&s.look_color);
                            if ui.color_edit_button_srgb(&mut c).changed() {
                                s.look_color = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
                            }
                        }
                    });
                    if s.look != "none" {
                        ui.add(
                            egui::Slider::new(&mut s.look_amt, 0.0..=100.0)
                                .suffix(" %")
                                .text("Strength"),
                        );
                    }
                    ui.add(
                        egui::Slider::new(&mut s.bright, 0.0..=100.0)
                            .suffix(" %")
                            .text("Brighter"),
                    );
                    sw(
                        ui,
                        &mut s.look_even,
                        "Even out brightness",
                        "Every clip towards a medium brightness",
                    );
                    ui.horizontal_wrapped(|ui| {
                        for k in ["hue", "vhs", "kenburns", "lines"] {
                            let (name, desc) = fx_text(k);
                            ui.checkbox(s.fx.get_mut(k), name).on_hover_text(desc);
                        }
                    });
                    sw(
                        ui,
                        &mut s.smooth,
                        "Smooth scaling",
                        "Clips scaled smoothly, less pixelated",
                    );
                },
            );
            section(
                ui,
                "Picture & frame",
                "Format, fit, seams and edges",
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Format");
                        combo(
                            ui,
                            "fmt",
                            &mut s.format,
                            &[("16:9", "16:9"), ("9:16", "9:16 (portrait)")],
                        );
                        ui.label("Clips");
                        combo(
                            ui,
                            "fit",
                            &mut s.fit,
                            &[
                                ("contain", "Fit (whole clip, blurred border)"),
                                ("cover", "Fill (crop)"),
                            ],
                        );
                    });
                    sw(
                        ui,
                        &mut s.soft,
                        "Soften the seams",
                        "In a split screen the line between the clips is soft",
                    );
                    if s.soft {
                        ui.add(
                            egui::Slider::new(&mut s.soft_amt, 0.0..=100.0)
                                .suffix(" %")
                                .text("How wide"),
                        );
                    } else {
                        ui.add(
                            egui::Slider::new(&mut s.div_w, 0.0..=12.0)
                                .step_by(1.0)
                                .suffix(" px")
                                .text("Dividers"),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Rim");
                        combo(
                            ui,
                            "edge",
                            &mut s.edge,
                            &[
                                ("off", "Off"),
                                ("blur", "Blur"),
                                ("motion", "Motion"),
                                ("lens", "Lens"),
                            ],
                        );
                        if s.edge != "off" {
                            ui.add(egui::Slider::new(&mut s.edge_amt, 0.0..=100.0).suffix(" %"));
                        }
                    });
                },
            );
            section(ui, "Sound", "Song volume", |ui| {
                ui.add(
                    egui::Slider::new(&mut s.song_vol, 0.0..=150.0)
                        .suffix(" %")
                        .text("Song"),
                );
            });
            section(
                ui,
                "Funscript",
                "A script for The Handy that follows the song",
                |ui| {
                    sw(
                        ui,
                        &mut s.fs_on,
                        "Save a funscript with the export",
                        "Written next to the video (same name, .funscript)",
                    );
                    ui.horizontal(|ui| {
                        ui.label("Pace");
                        combo(
                            ui,
                            "fsp",
                            &mut s.fs_pace,
                            &[
                                ("auto", "Auto"),
                                ("slow", "Slow"),
                                ("normal", "Normal"),
                                ("fast", "Fast"),
                            ],
                        );
                        ui.label("Size");
                        combo(
                            ui,
                            "fss",
                            &mut s.fs_size,
                            &[
                                ("auto", "Auto"),
                                ("small", "Small"),
                                ("medium", "Medium"),
                                ("large", "Large"),
                                ("full", "Full"),
                            ],
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Where");
                        combo(
                            ui,
                            "fsw",
                            &mut s.fs_where,
                            &[("low", "Low"), ("mid", "Middle"), ("high", "High")],
                        );
                        ui.label("Style");
                        combo(
                            ui,
                            "fsst",
                            &mut s.fs_style,
                            &[("sharp", "Sharp"), ("smooth", "Smooth")],
                        );
                    });
                    sw(
                        ui,
                        &mut s.fs_rests,
                        "Rests",
                        "Calm parts get slow, gentle strokes",
                    );
                    sw(
                        ui,
                        &mut s.fs_accent,
                        "Accents",
                        "A bigger stroke on the first beat of a bar and on drops",
                    );
                    ui.add(
                        egui::Slider::new(&mut s.fs_speed, 100.0..=600.0)
                            .step_by(10.0)
                            .text("Top speed"),
                    );
                },
            );
            section(ui, "Output", "Intro, outro, quality", |ui| {
                sw(ui, &mut s.intro, "Intro", "The title slams in at the start");
                sw(ui, &mut s.outro, "Outro", "Title and details at the end");
                ui.horizontal(|ui| {
                    ui.label("Title");
                    ui.add(
                        egui::TextEdit::singleline(&mut s.title)
                            .hint_text("empty = the song's name")
                            .desired_width(260.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Export");
                    combo_f(
                        ui,
                        "q",
                        &mut s.quality,
                        &[(720.0, "720p"), (1080.0, "1080p")],
                    );
                    combo_f(
                        ui,
                        "fps",
                        &mut s.export_fps,
                        &[(30.0, "30 fps"), (60.0, "60 fps")],
                    );
                    ui.add(
                        egui::Slider::new(&mut s.export_crf, 14.0..=30.0)
                            .step_by(1.0)
                            .text("CRF (lower = better)"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Live show picture");
                    combo_f(
                        ui,
                        "sq",
                        &mut s.show_quality,
                        &[
                            (360.0, "360p (fastest)"),
                            (540.0, "540p"),
                            (720.0, "720p"),
                            (1080.0, "1080p"),
                        ],
                    );
                });
                sw(
                    ui,
                    &mut s.hwaccel,
                    "Hardware video decoding",
                    "Lets the graphics card decode the clips (falls back to the CPU by itself)",
                );
            });
        });
    }

    fn presets_card(&mut self, ui: &mut egui::Ui) {
        let mut msg: Option<(String, bool)> = None;
        let names: Vec<String> = self.stored.presets.keys().cloned().collect();
        let stored = &mut self.stored;
        let s = &mut self.s;
        let name = &mut self.preset_name;
        Self::card(
            ui,
            "My settings",
            "clips, cutting, effects, look and sound under a name",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("mypreset")
                        .selected_text(if names.is_empty() {
                            "Nothing saved yet"
                        } else {
                            "Load saved settings …"
                        })
                        .show_ui(ui, |ui| {
                            for n in &names {
                                if ui.selectable_label(false, n).clicked() {
                                    if let Some(o) = stored.presets.get(n) {
                                        *s = s.with_preset(o);
                                        *name = n.clone();
                                        msg = Some((format!("Loaded: {n}"), true));
                                    }
                                }
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("name")
                            .desired_width(160.0),
                    );
                    if ui.button("Save").clicked() && !name.trim().is_empty() {
                        stored.presets.insert(name.trim().to_string(), s.snapshot());
                        msg = Some((format!("Saved: {}", name.trim()), true));
                    }
                    if stored.presets.contains_key(name.trim()) && ui.button("Delete").clicked() {
                        stored.presets.remove(name.trim());
                        msg = Some(("Deleted".into(), true));
                    }
                    if ui
                        .button("Export …")
                        .on_hover_text("As a file – also for the web version and the Stash plugin")
                        .clicked()
                    {
                        let n = if name.trim().is_empty() {
                            "PMV settings".to_string()
                        } else {
                            name.trim().to_string()
                        };
                        let safe: String = n
                            .chars()
                            .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
                            .collect();
                        if let Some(p) = rfd::FileDialog::new()
                            .set_file_name(format!("{safe}.pmvgen.json"))
                            .save_file()
                        {
                            let j = settings::export_file(&n, s);
                            msg = Some(
                                match std::fs::write(
                                    &p,
                                    serde_json::to_vec_pretty(&j).unwrap_or_default(),
                                ) {
                                    Ok(_) => ("Exported".into(), true),
                                    Err(e) => (format!("Export failed: {e}"), false),
                                },
                            );
                        }
                    }
                    if ui.button("Import …").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Settings", &["json"])
                            .pick_file()
                        {
                            msg = Some(
                                match std::fs::read(&p)
                                    .map_err(anyhow::Error::from)
                                    .and_then(|b| settings::import_file(&b))
                                {
                                    Ok((n, o)) => {
                                        *s = s.with_preset(&o);
                                        *name = n.clone();
                                        (format!("Imported: {n}"), true)
                                    }
                                    Err(e) => (format!("Import failed: {e}"), false),
                                },
                            );
                        }
                    }
                });
            },
        );
        if let Some((m, ok)) = msg {
            let _ = settings::save_stored(&self.stored);
            self.toast(m, ok);
        }
    }

    fn setup_ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::new().fill(CARD).inner_margin(10.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("♥").color(PINK).size(20.0));
                    ui.label(
                        RichText::new("PMV Generator")
                            .size(20.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.label(RichText::new("Runs locally – nothing is uploaded").color(DIM));
                });
            });
        let ready = self.song_ready(self.sel).is_some();
        let (v, im, _) = self.lib.lock().unwrap().count(&self.s);
        let clips = v + im > 0;
        let mut go = false;
        let mut exp = false;
        egui::TopBottomPanel::bottom("go").frame(egui::Frame::new().fill(CARD).inner_margin(12.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = if !ready { "Pick a song first" } else if !clips { "Add clips first" } else { "▶  Start the show" };
                let b = egui::Button::new(RichText::new(label).size(18.0).strong().color(Color32::WHITE)).fill(PINK).min_size(Vec2::new(220.0, 40.0));
                if ui.add_enabled(ready && clips && self.ff_err.is_none(), b).clicked() {
                    go = true;
                }
                let b2 = egui::Button::new(RichText::new("⬇  Export video …").size(16.0)).min_size(Vec2::new(180.0, 40.0));
                if ui.add_enabled(ready && clips && self.ff_err.is_none(), b2).on_hover_text("Renders the PMV to an MP4 file (no frames dropped)").clicked() {
                    exp = true;
                }
                ui.label(RichText::new("Space pause · F fullscreen · Esc stop · H hide bar · I clip info · N/P next/previous song").color(DIM));
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(980.0);
                    if let Some(e) = &self.ff_err {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(90, 20, 40))
                            .corner_radius(8.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(format!("⚠ {e}")).color(Color32::WHITE));
                            });
                        ui.add_space(8.0);
                    }
                    self.music_card(ui);
                    self.song_tools(ui);
                    self.clips_card(ui);
                    self.style_card(ui);
                    self.presets_card(ui);
                });
        });
        if go {
            self.go();
        }
        if exp {
            self.export();
        }
    }

    // ---------- the show ----------

    fn live_ui(&mut self, ctx: &egui::Context) {
        let Screen::Live(l) = &mut self.screen else {
            return;
        };
        let mut stop = false;
        let mut next: Option<i32> = None;
        // keys
        let (space, f, esc, h, i, n, p, moved) = ctx.input(|inp| {
            (
                inp.key_pressed(egui::Key::Space),
                inp.key_pressed(egui::Key::F),
                inp.key_pressed(egui::Key::Escape),
                inp.key_pressed(egui::Key::H),
                inp.key_pressed(egui::Key::I),
                inp.key_pressed(egui::Key::N),
                inp.key_pressed(egui::Key::P),
                inp.pointer.delta() != Vec2::ZERO || inp.pointer.any_click(),
            )
        });
        if moved {
            l.last_move = Instant::now();
        }
        let ready = l.ready.load(Ordering::SeqCst);
        if ready
            && !l.playing
            && l.started.elapsed() > Duration::from_millis(50)
            && !l.player.playing()
            && l.player.time() == 0.0
        {
            l.player.play();
            l.playing = true;
        }
        if space && ready {
            if l.player.playing() {
                l.player.pause();
            } else {
                l.player.play();
            }
        }
        if f {
            let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
        }
        if h {
            l.bar_hidden = !l.bar_hidden;
        }
        if i {
            l.info = !l.info;
        }
        if n {
            next = Some(1);
        }
        if p {
            next = Some(-1);
        }
        if esc {
            stop = true;
        }
        let t = if ready { l.player.time() } else { 0.0 };
        l.built.media.clock.set(t);
        // the picture
        let frame = l.built.renderer.frame(t);
        let img = egui::ColorImage::from_rgba_premultiplied(
            [frame.width() as usize, frame.height() as usize],
            frame.data(),
        );
        match &mut l.tex {
            Some(tx) => tx.set(img, egui::TextureOptions::LINEAR),
            None => l.tex = Some(ctx.load_texture("show", img, egui::TextureOptions::LINEAR)),
        }
        // frames per second (for the info)
        l.fps.1 += 1;
        if l.fps.0.elapsed() > Duration::from_secs(1) {
            l.fps.2 = l.fps.1 as f32 / l.fps.0.elapsed().as_secs_f32();
            l.fps = (Instant::now(), 0, l.fps.2);
        }
        let show_bar = !l.bar_hidden
            && (l.last_move.elapsed() < Duration::from_millis(2500)
                || !ready
                || !l.player.playing());
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::BLACK))
            .show(ctx, |ui| {
                let avail = ui.available_rect_before_wrap();
                if let Some(tx) = &l.tex {
                    let sz = tx.size_vec2();
                    let k = (avail.width() / sz.x).min(avail.height() / sz.y);
                    let r = egui::Rect::from_center_size(avail.center(), sz * k);
                    ui.painter().image(
                        tx.id(),
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                if !ready {
                    ui.painter().text(
                        avail.center(),
                        egui::Align2::CENTER_CENTER,
                        "Getting the first clips ready …",
                        egui::FontId::proportional(22.0),
                        Color32::WHITE,
                    );
                }
                if let Some((m, at)) = &l.msg {
                    if at.elapsed() < Duration::from_secs(6) {
                        ui.painter().text(
                            avail.center_top() + Vec2::new(0.0, 30.0),
                            egui::Align2::CENTER_TOP,
                            m,
                            egui::FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                    }
                }
                if l.info {
                    let mut lines = vec![format!(
                        "{:.0} fps · {}×{} · {}",
                        l.fps.2,
                        l.built.plan.w,
                        l.built.plan.h,
                        if ready { "playing" } else { "starting" }
                    )];
                    for (g, name, st) in l.built.renderer.info(t) {
                        lines.push(format!("Field {g}: {name} – {st}"));
                    }
                    let fails = l.built.media.failures.lock().unwrap();
                    if !fails.is_empty() {
                        lines.push("Left out:".into());
                        for f in fails.iter().rev().take(8) {
                            lines.push(format!("  {} – {}", f.name, f.why));
                        }
                    }
                    let text = lines.join("\n");
                    let pos = avail.left_top() + Vec2::new(14.0, 14.0);
                    let galley = ui.painter().layout_no_wrap(
                        text,
                        egui::FontId::monospace(13.0),
                        Color32::WHITE,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            pos - Vec2::splat(6.0),
                            galley.size() + Vec2::splat(12.0),
                        ),
                        6.0,
                        Color32::from_black_alpha(190),
                    );
                    ui.painter().galley(pos, galley, Color32::WHITE);
                }
                if !show_bar {
                    ctx.set_cursor_icon(egui::CursorIcon::None);
                }
            });
        if show_bar {
            egui::TopBottomPanel::bottom("bar")
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(200))
                        .inner_margin(8.0),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let playing = l.player.playing();
                        if ui.button(if playing { "⏸" } else { "▶" }).clicked() && ready {
                            if playing {
                                l.player.pause();
                            } else {
                                l.player.play();
                            }
                        }
                        if ui.button("⏹ Stop").clicked() {
                            stop = true;
                        }
                        if l.order.len() > 1 {
                            if ui.button("⏮").clicked() {
                                next = Some(-1);
                            }
                            if ui.button("⏭").clicked() {
                                next = Some(1);
                            }
                        }
                        if ui.button("⛶").on_hover_text("Fullscreen (F)").clicked() {
                            let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fs));
                        }
                        let dur = l.player.duration();
                        let plan = &l.built.plan;
                        let ph = plan.phases[plan.phase_at(t)].layout;
                        ui.label(format!(
                            "{} / {} · {} cuts · {}",
                            fmt_time(t),
                            fmt_time(dur),
                            plan.cuts_before(t),
                            crate::plan::layout(ph).name
                        ));
                        let w = ui.available_width() - 10.0;
                        let (r, _) = ui
                            .allocate_exact_size(Vec2::new(w.max(20.0), 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 3.0, Color32::from_gray(60));
                        let mut r2 = r;
                        r2.set_width(r.width() * (t / dur.max(0.01)).clamp(0.0, 1.0) as f32);
                        ui.painter().rect_filled(r2, 3.0, PINK);
                    });
                });
        }
        let ended = ready && l.player.ended();
        let order_len = l.order.len();
        let pos = l.order_pos as i32;
        let order = l.order.clone();
        if ended && next.is_none() && order_len > 1 {
            next = Some(1);
        }
        ctx.request_repaint();
        if stop || (ended && order_len <= 1) {
            self.stop_show(ctx);
            return;
        }
        if let Some(d) = next {
            let np = (pos + d).rem_euclid(order_len as i32) as usize;
            let idx = order[np];
            if self.song_ready(idx).is_some() {
                let fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                if let Screen::Live(l) = &self.screen {
                    l.stop.store(true, Ordering::SeqCst);
                    l.built.media.stop();
                    l.player.pause();
                }
                self.start_show(idx, order, np);
                if fs {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                }
            } else if let Screen::Live(l) = &mut self.screen {
                l.msg = Some(("The next song isn't ready yet".into(), Instant::now()));
            }
        }
    }

    fn export_ui(&mut self, ctx: &egui::Context) {
        let Screen::Export(job) = &self.screen else {
            return;
        };
        let p = job.prog.lock().unwrap().clone();
        let res = job.result.lock().unwrap().clone();
        let mut back = false;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(640.0);
                match &res {
                    None => {
                        ui.label(RichText::new("Exporting the video").size(24.0).strong());
                        ui.add_space(12.0);
                        ui.add(
                            egui::ProgressBar::new(p.frac)
                                .show_percentage()
                                .animate(true),
                        );
                        let el = job.started.elapsed().as_secs_f64();
                        ui.label(
                            RichText::new(format!(
                                "{} · {}{}",
                                p.msg,
                                fmt_time(el),
                                p.eta
                                    .map(|e| format!(" · about {} left", fmt_time(e)))
                                    .unwrap_or_default()
                            ))
                            .color(DIM),
                        );
                        ui.add_space(12.0);
                        if ui.button("Cancel").clicked() {
                            job.cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    Some(Ok(path)) => {
                        ui.label(RichText::new("✓ Done").size(24.0).strong().color(PINK));
                        ui.label(path.to_string_lossy());
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button("Show in folder").clicked() {
                                open_folder(path);
                            }
                            if ui.button("Back").clicked() {
                                back = true;
                            }
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(
                            RichText::new("The export didn't finish")
                                .size(22.0)
                                .strong(),
                        );
                        ui.label(e);
                        if ui.button("Back").clicked() {
                            back = true;
                        }
                    }
                }
                if !p.fails.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Clips left out (they couldn't be read):").color(DIM));
                    for f in p.fails.iter().take(12) {
                        ui.label(RichText::new(f).small().color(DIM));
                    }
                }
            });
        });
        ctx.request_repaint_after(Duration::from_millis(200));
        if back {
            self.screen = Screen::Setup;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::Setup => {
                self.handle_drops(ctx);
                self.setup_ui(ctx);
                // (songs being read and clips being measured update the page)
                ctx.request_repaint_after(Duration::from_millis(300));
            }
            Screen::Live(_) => self.live_ui(ctx),
            Screen::Export(_) => self.export_ui(ctx),
        }
        if let Some((m, at, ok)) = &self.toast {
            if at.elapsed() < Duration::from_secs(4) {
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 60.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(if *ok {
                                Color32::from_rgb(40, 90, 60)
                            } else {
                                Color32::from_rgb(120, 30, 50)
                            })
                            .corner_radius(8.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(m).color(Color32::WHITE));
                            });
                    });
            } else {
                self.toast = None;
            }
        }
        if self.save_check.elapsed() > Duration::from_secs(1) {
            self.save_check = Instant::now();
            self.persist();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Screen::Live(l) = &self.screen {
            l.stop.store(true, Ordering::SeqCst);
            l.built.media.stop();
        }
        if let Screen::Export(j) = &self.screen {
            j.cancel.store(true, Ordering::SeqCst);
        }
        self.persist();
    }
}

// ---------- small widgets ----------

fn sw(ui: &mut egui::Ui, v: &mut bool, name: &str, desc: &str) {
    ui.checkbox(v, name).on_hover_text(desc);
}

fn section(ui: &mut egui::Ui, name: &str, sub: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(name).strong())
        .id_salt(name)
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new(sub).small().color(DIM));
            body(ui);
        });
}

fn combo(ui: &mut egui::Ui, id: &str, v: &mut String, opts: &[(&str, &str)]) {
    let cur = opts
        .iter()
        .find(|(x, _)| *x == v.as_str())
        .map(|(_, l)| *l)
        .unwrap_or("?");
    egui::ComboBox::from_id_salt(id)
        .selected_text(cur)
        .show_ui(ui, |ui| {
            for (x, l) in opts {
                if ui.selectable_label(v.as_str() == *x, *l).clicked() {
                    *v = x.to_string();
                }
            }
        });
}
fn combo_f(ui: &mut egui::Ui, id: &str, v: &mut f64, opts: &[(f64, &str)]) {
    let cur = opts
        .iter()
        .find(|(x, _)| *x == *v)
        .map(|(_, l)| *l)
        .unwrap_or("?");
    egui::ComboBox::from_id_salt(id)
        .selected_text(cur)
        .show_ui(ui, |ui| {
            for (x, l) in opts {
                ui.selectable_value(v, *x, *l);
            }
        });
}

fn fx_text(k: &str) -> (&'static str, &'static str) {
    match k {
        "flash" => ("Flash", "Bright flash on cuts and drops"),
        "zoom" => ("Zoom pulse", "The picture pumps on every beat"),
        "whip" => (
            "Transitions",
            "New clips whip into the field with motion blur",
        ),
        "zoomin" => (
            "Zoom-in entry",
            "New clips zoom into the picture fast – always zooming in, alternating strong and soft",
        ),
        "speed" => (
            "Speed ramps",
            "Slow motion in calm parts, faster when it gets loud, a burst on drops",
        ),
        "voice" => ("Clip audio", "Not in this version"),
        "rgb" => (
            "RGB split",
            "Red and cyan tear apart – really hard on drops",
        ),
        "echo" => ("Echo", "The last frame lingers as a ghost (loud parts)"),
        "tunnel" => (
            "Tunnel",
            "On drops the picture sits inside itself three times",
        ),
        "invert" => ("Negative", "Short negative on drops"),
        "glitch" => ("Glitch", "Picture stripes slip on drops"),
        "stutter" => (
            "Stutter",
            "Clips jump back on half beats – a classic PMV effect",
        ),
        "shake" => ("Shake", "Shakes in loud parts"),
        "hue" => ("Color rush", "Colors rotate to the beat"),
        "vhs" => ("VHS", "Noise, scanlines and dark corners"),
        "strobe" => (
            "Strobe",
            "White flashes on half beats – careful if you are sensitive to light",
        ),
        "text" => ("Text", "Your words slam into the picture on drops"),
        "kenburns" => (
            "Image drift",
            "Still images zoom and glide slowly (Ken Burns)",
        ),
        "lines" => (
            "Glowing dividers",
            "The lines between fields flash pink to the beat",
        ),
        _ => ("?", ""),
    }
}

/// The song's waveform with its beats (bars pink)
fn draw_wave(ui: &mut egui::Ui, song: &Song) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 70.0), egui::Sense::hover());
    let p = ui.painter_at(r);
    p.rect_filled(r, 6.0, Color32::from_rgb(20, 8, 18));
    let peaks = &song.a.peaks;
    let n = peaks.len().max(1);
    let max = peaks.iter().cloned().fold(0.0001f32, f32::max);
    for (i, v) in peaks.iter().enumerate() {
        let x = r.left() + r.width() * i as f32 / n as f32;
        let h = (v / max) * (r.height() * 0.45);
        p.line_segment(
            [
                egui::pos2(x, r.center().y - h),
                egui::pos2(x, r.center().y + h),
            ],
            egui::Stroke::new(1.0_f32, Color32::from_rgb(110, 58, 99)),
        );
    }
    let dur = song.duration().max(0.01);
    for (k, b) in song.a.beats.iter().enumerate() {
        let x = r.left() + r.width() * (*b / dur) as f32;
        let (bar, phrase) = match &song.bars {
            Some(bb) => (
                bb.downbeat.get(k).copied().unwrap_or(false),
                bb.phrase.get(k).copied().unwrap_or(false),
            ),
            None => (k % 4 == 0, k % 16 == 0),
        };
        let (c, hh) = if phrase {
            (PINK, 1.0)
        } else if bar {
            (PINK.linear_multiply(0.6), 0.6)
        } else {
            (Color32::from_white_alpha(40), 0.3)
        };
        p.line_segment(
            [
                egui::pos2(x, r.bottom()),
                egui::pos2(x, r.bottom() - r.height() * hh),
            ],
            egui::Stroke::new(1.0_f32, c),
        );
    }
}

pub fn run(open: Vec<PathBuf>) -> anyhow::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PMV Generator")
            .with_inner_size([1100.0, 860.0])
            .with_min_inner_size([640.0, 480.0])
            .with_icon(icon())
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "PMV Generator",
        opts,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            app.open_paths(open);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
