// Clips for the shots: a pool of prepared clips (a good spot found in each video – best moments, clean
// cuts, variety), the pick for each shot shortly before it comes on (shape, match cut, calm/busy), and
// one ffmpeg decoder per shot that delivers its frames at the field's size.
// Ports prepareFrom(), nextSource(), takeMedia() and friends from the browser version.

use crate::ff;
use crate::library::{shape_ok, Item, Kind};
use crate::plan::{Plan, Shot};
use crate::settings::Settings;
use crate::smart::{self, Sig};
use rand::Rng;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

pub const SNAP: f64 = 1.16; // "Fit": a clip this close to the field's shape (ratio of the two aspect ratios) fills it

/// Show clock shared between the player / exporter and the decoders (f64 seconds as bits)
#[derive(Default)]
pub struct Clock(AtomicU64);
impl Clock {
    pub fn set(&self, t: f64) {
        self.0.store(t.to_bits(), Ordering::Relaxed)
    }
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

// 3 min → 15 s, 10 min → 24 s, longer → 30 s; short clips a tenth; plus "Skip the start" (never more than 40 %)
fn intro_end(dur: f64, skip_head: f64) -> f64 {
    if dur > 10.0 {
        (30f64.min((dur * 0.04).max(15f64.min(dur * 0.1)))).max(skip_head.min(dur * 0.4))
    } else {
        0.0
    }
}
fn tail_x(dur: f64, skip_tail: f64) -> f64 {
    if dur > 10.0 {
        skip_tail.min(dur * 0.4)
    } else {
        0.0
    }
}

#[derive(Clone)]
pub struct Ready {
    pub item: Item,
    pub start: f64,
    pub end_at: Option<f64>,
    pub sig: Option<Sig>,
    pub motion: Option<f64>,
    pub kb: f64, // image drift direction
}

/// Frames of one shot
pub struct Frames {
    pub fw: u32,
    pub fh: u32,
    pub fps: f64,
    pub still: Option<Arc<Pixmap>>,
    slots: Mutex<Vec<Option<Arc<Pixmap>>>>,
    decoded: AtomicUsize,
    pub done: AtomicBool,
    pub failed: Mutex<Option<String>>,
    cancel: Arc<AtomicBool>,
    cond: Condvar,
    gate: Mutex<()>,
}
impl Frames {
    fn new(fw: u32, fh: u32, fps: f64, n: usize) -> Arc<Frames> {
        Arc::new(Frames {
            fw,
            fh,
            fps,
            still: None,
            slots: Mutex::new(vec![None; n]),
            decoded: AtomicUsize::new(0),
            done: AtomicBool::new(false),
            failed: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
            cond: Condvar::new(),
            gate: Mutex::new(()),
        })
    }
    pub fn decoded(&self) -> usize {
        self.decoded.load(Ordering::SeqCst)
    }
    /// Frame `idx` (or, while it isn't there, the newest one before it). wait: block until it is decoded.
    pub fn get(&self, idx: usize, wait: Option<Duration>) -> Option<Arc<Pixmap>> {
        if let Some(s) = &self.still {
            return Some(s.clone());
        }
        if let Some(limit) = wait {
            let t0 = Instant::now();
            let mut g = self.gate.lock().unwrap();
            while self.decoded() <= idx && !self.done.load(Ordering::SeqCst) {
                let left = limit.saturating_sub(t0.elapsed());
                if left.is_zero() {
                    break;
                }
                g = self
                    .cond
                    .wait_timeout(g, left.min(Duration::from_millis(50)))
                    .unwrap()
                    .0;
            }
        }
        let slots = self.slots.lock().unwrap();
        let have = self.decoded().min(slots.len());
        if have == 0 {
            return None;
        }
        let mut i = idx.min(have - 1);
        loop {
            if let Some(f) = &slots[i] {
                return Some(f.clone());
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        // (dropped below: the first one still kept after it)
        slots[idx.min(have - 1)..have]
            .iter()
            .flatten()
            .next()
            .cloned()
    }
    fn drop_below(&self, idx: usize) {
        let mut s = self.slots.lock().unwrap();
        let n = idx.min(s.len());
        for x in s.iter_mut().take(n) {
            *x = None;
        }
    }
}
impl Drop for Frames {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

pub struct LiveSig {
    pub focus: (f64, f64),
    pub target: (f64, f64),
    pub lum: Option<f64>,
    pub measured_t: f64,
}

/// A clip in a shot
pub struct Assigned {
    pub shot: usize,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
    pub w: u32,
    pub h: u32,
    pub start: f64,
    pub end_at: Option<f64>,
    pub sig: Option<Sig>,
    pub kb: f64,
    pub frames: Arc<Frames>,
    pub live: Mutex<LiveSig>,
    pub smart: bool, // (a quick pick without analysis)
}

impl Assigned {
    /// Video frame index at show time t
    pub fn index_at(&self, plan: &Plan, shot: &Shot, t: f64) -> usize {
        let mut c = plan.shot_clock(shot, t);
        if let Some(end) = self.end_at {
            let period = end - self.start;
            if period > 0.1 && self.start + c >= end {
                c %= period;
            }
        }
        (c.max(0.0) * self.frames.fps).floor() as usize
    }
}

pub fn fit_for(fit: &str, field_w: f64, field_h: f64, mw: f64, mh: f64) -> &'static str {
    let r = (field_w / field_h / (mw / mh)).max(mw / mh / (field_w / field_h));
    if fit == "contain" && r <= SNAP {
        "cover"
    } else if fit == "contain" {
        "contain"
    } else {
        "cover"
    }
}

/// The size a clip is decoded at: as it is drawn into its field (fill or fit)
fn decode_size(fit: &str, fw: f64, fh: f64, mw: f64, mh: f64) -> (u32, u32) {
    let f = fit_for(fit, fw, fh, mw, mh);
    let r = if f == "contain" {
        (fw / mw).min(fh / mh)
    } else {
        (fw / mw).max(fh / mh)
    };
    (
        ((mw * r).round() as u32).max(2),
        ((mh * r).round() as u32).max(2),
    )
}

fn to_pixmap(w: u32, h: u32, rgba: Vec<u8>) -> Option<Pixmap> {
    // (opaque pictures are the same premultiplied; see-through ones are premultiplied here)
    let mut data = rgba;
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a < 255 {
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        }
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

// ---------- Tiny frames for the analysis ----------

/// Tiny RGB frames of a video at `times` seconds after `t0` (one ffmpeg run)
fn tiny_frames(path: &std::path::Path, w: u32, h: u32, t0: f64, times: &[f64]) -> Vec<Sig> {
    let (tw, th) = smart::tiny(w, h);
    let last = times.iter().cloned().fold(0.0, f64::max);
    let Ok(mut c) = ff::ffmpeg() else {
        return Vec::new();
    };
    c.arg("-ss")
        .arg(format!("{:.3}", t0.max(0.0)))
        .arg("-i")
        .arg(path);
    c.arg("-t").arg(format!("{:.3}", last + 0.12));
    c.args(["-an", "-sn", "-dn", "-threads", "2", "-vf"])
        .arg(format!("fps=20,scale={tw}:{th}:flags=area"));
    c.args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"]);
    let Ok(out) = c.stdout(Stdio::piped()).stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    let fsz = (tw * th * 3) as usize;
    let frames: Vec<&[u8]> = out.stdout.chunks_exact(fsz).collect();
    if frames.is_empty() {
        return Vec::new();
    }
    times
        .iter()
        .map(|t| {
            let i = ((t * 20.0).round() as usize).min(frames.len() - 1);
            smart::measure(frames[i], tw as usize, th as usize, 3)
        })
        .collect()
}

fn image_sig(path: &std::path::Path, w: u32, h: u32) -> Option<Sig> {
    let (tw, th) = smart::tiny(w, h);
    let px = ff::decode_rgba(path, None, tw, th, true).ok()?;
    Some(smart::measure(&px, tw as usize, th as usize, 4))
}

// ---------- The pool ----------

struct Pool {
    sources: Vec<Item>,
    src_idx: usize,
    ready: Vec<Ready>,
    preparing: usize,
    prep_keys: Vec<usize>,
    recent: VecDeque<usize>,
    spots: HashMap<usize, Vec<f64>>,
    bad: HashMap<usize, String>,
    shown: std::collections::HashSet<usize>,
    zoom_n: usize,
}

pub struct Failure {
    pub name: String,
    pub why: String,
}

pub struct Media {
    pub s: Settings,
    bpm: f64,
    song_dur: f64,
    items: Vec<Item>,
    pub images: Vec<Item>,
    pool: Mutex<Pool>,
    pool_cond: Condvar,
    pub clock: Arc<Clock>,
    pub assigned: Mutex<HashMap<usize, Arc<Assigned>>>,
    next_assign: AtomicUsize,
    pub failures: Mutex<Vec<Failure>>,
    pub export: bool,
    pic_cache: Mutex<HashMap<(usize, u32, u32), Arc<Pixmap>>>,
    stop: AtomicBool,
    max_prep: usize,
    pub lookahead: f64,
}

impl Media {
    pub fn new(
        s: &Settings,
        items: Vec<Item>,
        images: Vec<Item>,
        bpm: f64,
        song_dur: f64,
        export: bool,
    ) -> Arc<Media> {
        let mut rng = rand::thread_rng();
        let mut sources = items.clone();
        for i in (1..sources.len()).rev() {
            let j = rng.gen_range(0..=i);
            sources.swap(i, j);
        }
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Arc::new(Media {
            s: s.clone(),
            bpm,
            song_dur,
            items,
            images,
            pool: Mutex::new(Pool {
                sources,
                src_idx: 0,
                ready: Vec::new(),
                preparing: 0,
                prep_keys: Vec::new(),
                recent: VecDeque::new(),
                spots: HashMap::new(),
                bad: HashMap::new(),
                shown: Default::default(),
                zoom_n: 0,
            }),
            pool_cond: Condvar::new(),
            clock: Arc::new(Clock::default()),
            assigned: Mutex::new(HashMap::new()),
            next_assign: AtomicUsize::new(0),
            failures: Mutex::new(Vec::new()),
            export,
            pic_cache: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
            // At most 3 at once in the browser (each decodes and seeks its video); a desktop with more cores can do more
            max_prep: if export {
                (cores / 2).clamp(3, 8)
            } else {
                (cores / 4).clamp(3, 6)
            },
            lookahead: if export { 2.0 } else { 3.0 },
        })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.assigned.lock().unwrap().clear();
        self.pool_cond.notify_all();
    }

    pub fn shown_count(&self) -> usize {
        self.pool.lock().unwrap().shown.len()
    }
    pub fn ready_count(&self) -> (usize, usize) {
        let p = self.pool.lock().unwrap();
        (p.ready.len(), p.preparing)
    }

    fn note_fail(&self, it: &Item, why: &str) {
        let mut f = self.failures.lock().unwrap();
        if !f.iter().any(|x| x.name == it.name) {
            f.push(Failure {
                name: it.name.clone(),
                why: why.to_string(),
            });
        }
    }

    // Variety: the same clip not within the last 24, and not while a copy is ready or being prepared
    fn is_recent(p: &Pool, id: usize) -> bool {
        p.recent.contains(&id)
            || p.ready.iter().any(|r| r.item.id == id)
            || p.prep_keys.contains(&id)
    }

    fn next_source(&self, p: &mut Pool) -> Option<Item> {
        if p.sources.is_empty() {
            return None;
        }
        let n = p.sources.len();
        for tries in 0..=n * 2 {
            if p.src_idx >= p.sources.len() {
                // a new round, shuffled again
                let mut rng = rand::thread_rng();
                for i in (1..p.sources.len()).rev() {
                    let j = rng.gen_range(0..=i);
                    p.sources.swap(i, j);
                }
                p.src_idx = 0;
            }
            let s = p.sources[p.src_idx].clone();
            p.src_idx += 1;
            if p.bad.contains_key(&s.id) && tries < n {
                continue;
            }
            if p.bad.len() >= n {
                return None;
            }
            if !self.s.variety || tries >= n || !Self::is_recent(p, s.id) {
                if p.bad.contains_key(&s.id) {
                    continue;
                }
                return Some(s);
            }
        }
        p.sources
            .iter()
            .find(|s| !p.bad.contains_key(&s.id))
            .cloned()
    }

    /// Keep the pool topped up (in the background)
    pub fn fill_pool(self: &Arc<Self>) {
        let cap = if self.s.match_cut { 8 } else { 6 };
        loop {
            let item = {
                let mut p = self.pool.lock().unwrap();
                if self.stop.load(Ordering::SeqCst)
                    || p.preparing >= self.max_prep
                    || p.ready.len() + p.preparing >= cap
                {
                    return;
                }
                let Some(it) = self.next_source(&mut p) else {
                    return;
                };
                p.preparing += 1;
                p.prep_keys.push(it.id);
                it
            };
            let me = self.clone();
            std::thread::spawn(move || {
                let res = me.prepare(&item);
                let fail = {
                    let mut p = me.pool.lock().unwrap();
                    p.preparing -= 1;
                    if let Some(i) = p.prep_keys.iter().position(|&k| k == item.id) {
                        p.prep_keys.remove(i);
                    }
                    match res {
                        Ok(r) => {
                            if !me.stop.load(Ordering::SeqCst) {
                                p.ready.push(r);
                            }
                            None
                        }
                        Err((why, skip)) => {
                            if !skip {
                                p.bad.insert(item.id, why.clone());
                                Some(why)
                            } else {
                                None
                            }
                        }
                    }
                };
                if let Some(why) = fail {
                    me.note_fail(&item, &why);
                }
                me.pool_cond.notify_all();
                if !me.stop.load(Ordering::SeqCst) {
                    me.fill_pool();
                }
            });
        }
    }

    fn seq_window(&self, dur: f64) -> Option<(f64, f64)> {
        if !self.s.sequenced || dur <= 10.0 || self.song_dur <= 0.0 {
            return None;
        }
        let prog = (self.clock.get() / self.song_dur).clamp(0.0, 1.0);
        let half = (dur * 0.12).max(6.0);
        let lo = (prog * dur - half).min(dur - 6.0).max(0.0);
        Some((lo, (dur - 3.0).min(lo.max(prog * dur + half))))
    }

    /// Prepare a clip: find a good spot. Err((why, skip)) – skip = doesn't fit the filters (not an error)
    fn prepare(&self, s: &Item) -> Result<Ready, (String, bool)> {
        let mut it = s.clone();
        if it.w == 0 || it.dur <= 0.0 && it.kind == Kind::Video {
            match crate::library::probe_item(&it.path, it.kind) {
                Ok((w, h, d)) => {
                    it.w = w;
                    it.h = h;
                    it.dur = d;
                }
                Err(e) => return Err((e, false)),
            }
        }
        if !shape_ok(it.w, it.h, &self.s.shape) || (it.kind == Kind::Video && it.dur <= 4.0) {
            return Err(("doesn't fit the filters".into(), true));
        }
        let mut rng = rand::thread_rng();
        if it.kind == Kind::Image {
            let sig = image_sig(&it.path, it.w, it.h);
            if sig.is_none() {
                return Err(("this picture can't be read".into(), false));
            }
            return Ok(Ready {
                item: it,
                start: 0.0,
                end_at: None,
                sig,
                motion: None,
                kb: if rng.gen::<f64>() < 0.5 { 1.0 } else { -1.0 },
            });
        }
        let dur = it.dur;
        let seq = self.seq_window(dur);
        let head = intro_end(dur, self.s.skip_head);
        let tail = tail_x(dur, self.s.skip_tail);
        let rand_t = |rng: &mut rand::rngs::ThreadRng| -> f64 {
            if let Some((lo, hi)) = seq {
                let a = lo.max(head.min(hi));
                a + rng.gen::<f64>() * (hi - lo.max(head)).max(0.0)
            } else if dur > 10.0 {
                head + rng.gen::<f64>() * ((dur * 0.92 - 4.0).min(dur - 4.0 - tail) - head).max(0.0)
            } else {
                0.0
            }
        };
        let (used, ready_n) = {
            let p = self.pool.lock().unwrap();
            (
                p.spots.get(&it.id).cloned().unwrap_or_default(),
                p.ready.len(),
            )
        };
        let r = (dur * 0.03).max(6.0);
        let fresh = |t: f64| !used.iter().any(|u| (u - t).abs() < r);
        let fresh_rand = |rng: &mut rand::rngs::ThreadRng| {
            let mut t = rand_t(rng);
            for _ in 0..4 {
                if fresh(t) {
                    break;
                }
                t = rand_t(rng);
            }
            t
        };
        let mut start = fresh_rand(&mut rng);
        let mut info: Option<Sig> = None;
        let mut mo: Option<f64> = None;
        if self.s.best_spots && dur > 10.0 {
            // Several random spots, two frames each for motion; big videos and a low supply look at fewer
            let big = (it.w as u64 * it.h as u64) > 2560 * 1440;
            let urgent = (ready_n < 2 && !self.export) || big;
            let n = if urgent { 2 } else { 4 };
            let clean = self.s.clean_cuts && !urgent;
            let span = if clean {
                (if self.bpm > 0.0 {
                    4.0 * 60.0 / self.bpm
                } else {
                    2.5
                })
                .clamp(1.5, 4.0)
            } else {
                0.0
            };
            let mut cands: Vec<(f64, bool)> =
                (0..n).map(|_| (fresh_rand(&mut rng), false)).collect();
            if clean {
                for _ in 0..4 {
                    cands.push((fresh_rand(&mut rng), true));
                }
            }
            let mut best = f64::NEG_INFINITY;
            let mut any_clean = false;
            for (c, extra) in cands {
                if self.stop.load(Ordering::SeqCst) {
                    break;
                }
                if extra && any_clean {
                    break;
                }
                let t0 = if clean {
                    c.min((dur - span - 0.5).max(0.0))
                } else {
                    c
                };
                let times: Vec<f64> = if clean {
                    vec![0.0, 0.35, span * 0.45, span * 0.8]
                } else {
                    vec![0.0, 0.35]
                };
                let sigs = tiny_frames(&it.path, it.w, it.h, t0, &times);
                if sigs.len() < 2 {
                    continue;
                }
                let (a, b) = (&sigs[0], &sigs[1]);
                let mut cut = clean && smart::is_cut(a, b);
                if clean && !cut {
                    // a few more looks along the stretch
                    let mut prev = b;
                    for x in &sigs[2..] {
                        if smart::is_cut(prev, x) {
                            cut = true;
                            break;
                        }
                        prev = x;
                    }
                }
                if clean && !cut {
                    any_clean = true;
                }
                // motion counts, but a "motion" that is a cut isn't one
                let sc = if cut {
                    smart::spot_score(a, a, 0.0) - 5.0
                } else {
                    smart::spot_score(a, b, 0.0)
                };
                if sc > best {
                    best = sc;
                    start = t0;
                    info = Some(a.clone());
                    mo = Some(if cut { 0.0 } else { smart::motion(a, b) });
                }
            }
        }
        {
            let mut p = self.pool.lock().unwrap();
            let l = p.spots.entry(it.id).or_default();
            l.push(start);
            if l.len() > 8 {
                l.remove(0);
            }
        }
        if info.is_none() {
            let sigs = tiny_frames(&it.path, it.w, it.h, start, &[0.0]);
            if sigs.is_empty() {
                return Err(("no picture – ffmpeg can't decode this video".into(), false));
            }
            info = sigs.into_iter().next();
        }
        let end_at = if dur - 0.4 > start + 0.5 {
            Some(dur - 0.4)
        } else {
            None
        };
        Ok(Ready {
            item: it,
            start,
            end_at,
            sig: info,
            motion: mo,
            kb: 0.0,
        })
    }

    /// A quick clip without analysis (the pool ran dry)
    fn quick(&self) -> Option<Ready> {
        let it = {
            let mut p = self.pool.lock().unwrap();
            self.next_source(&mut p)?
        };
        let mut it = it;
        if it.w == 0 {
            if let Ok((w, h, d)) = crate::library::probe_item(&it.path, it.kind) {
                it.w = w;
                it.h = h;
                it.dur = d;
            } else {
                return None;
            }
        }
        let mut rng = rand::thread_rng();
        let dur = it.dur;
        let head = intro_end(dur, self.s.skip_head);
        let start = if it.kind == Kind::Video && dur > 10.0 {
            head + rng.gen::<f64>()
                * ((dur * 0.92 - 4.0).min(dur - 4.0 - tail_x(dur, self.s.skip_tail)) - head)
                    .max(0.0)
        } else {
            0.0
        };
        let end_at = if it.kind == Kind::Video && dur - 0.4 > start + 0.5 {
            Some(dur - 0.4)
        } else {
            None
        };
        Some(Ready {
            item: it,
            start,
            end_at,
            sig: None,
            motion: None,
            kb: if rng.gen::<f64>() < 0.5 { 1.0 } else { -1.0 },
        })
    }

    /// The best of the ready clips for a field: shape fits, match cut, calm/busy, variety
    fn take(
        &self,
        aspect: f64,
        out: Option<&Assigned>,
        strict: bool,
        layout: &str,
        energy: f64,
    ) -> Option<Ready> {
        let mut p = self.pool.lock().unwrap();
        if p.ready.is_empty() {
            return None;
        }
        let out_sig = if self.s.match_cut {
            out.and_then(|o| o.sig.clone())
        } else {
            None
        };
        let mut cand: Vec<usize> = (0..p.ready.len()).collect();
        let want = self
            .s
            .layout_shape
            .get(layout)
            .map(|s| s.as_str())
            .unwrap_or("all");
        if want == "landscape" || want == "portrait" {
            let fit: Vec<usize> = cand
                .iter()
                .copied()
                .filter(|&i| {
                    let r = &p.ready[i].item;
                    if want == "portrait" {
                        r.h > r.w
                    } else {
                        r.w >= r.h
                    }
                })
                .collect();
            if !fit.is_empty() {
                cand = fit;
            } else if strict {
                return None;
            }
        }
        // Copies of one clip count once
        let mut seen = std::collections::HashSet::new();
        cand.retain(|&i| seen.insert(p.ready[i].item.id));
        cand.truncate(if self.s.match_cut { 8 } else { 5 });
        // Best moments: calm music gets the calmer clips, loud parts the ones where the most is going on
        let mut rank: HashMap<usize, f64> = HashMap::new();
        if self.s.best_spots {
            let mut known: Vec<(usize, f64)> = cand
                .iter()
                .filter_map(|&i| p.ready[i].motion.map(|m| (i, (m / 0.3).min(1.0))))
                .collect();
            known.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            if known.len() >= 2 {
                let n = known.len() - 1;
                for (k, (i, _)) in known.iter().enumerate() {
                    rank.insert(*i, k as f64 / n as f64);
                }
            }
        }
        let mut best = cand[0];
        let mut score = f64::INFINITY;
        for &i in &cand {
            let m = &p.ready[i];
            let mut sc = ((m.item.w as f64 / m.item.h.max(1) as f64) / aspect)
                .ln()
                .abs();
            if let Some(os) = &out_sig {
                sc += 1.5 * smart::match_dist(Some(os), m.sig.as_ref());
            }
            if !rank.is_empty() {
                sc += 1.2 * (rank.get(&i).copied().unwrap_or(0.5) - energy).abs();
            }
            if self.s.variety && p.recent.contains(&m.item.id) {
                sc += 3.0;
            }
            if sc < score {
                score = sc;
                best = i;
            }
        }
        let m = p.ready.remove(best);
        p.shown.insert(m.item.id);
        p.recent.push_back(m.item.id);
        while p.recent.len() > 24 {
            p.recent.pop_front();
        }
        Some(m)
    }

    /// Assign clips to the shots that come on soon, start their decoders, let go of the ones that are over.
    /// Called often (live: a timer thread; export: before each frame).
    pub fn tick(self: &Arc<Self>, plan: &Plan, now: f64) {
        self.clock.set(now);
        self.fill_pool();
        loop {
            let i = self.next_assign.load(Ordering::SeqCst);
            if i >= plan.shots.len()
                || plan.shots[i].t_on > now + self.lookahead
                || self.stop.load(Ordering::SeqCst)
            {
                break;
            }
            self.assign(plan, i);
            self.next_assign.store(i + 1, Ordering::SeqCst);
        }
        // let go of what is over (kept a moment: a frame may still be drawn)
        let mut a = self.assigned.lock().unwrap();
        a.retain(|&id, _| plan.shots[id].until > now - 0.5);
    }

    fn assign(self: &Arc<Self>, plan: &Plan, i: usize) {
        let shot = &plan.shots[i];
        let layout = plan.phases[shot.phase].layout;
        let out = shot
            .prev
            .and_then(|p| self.assigned.lock().unwrap().get(&p).cloned());
        let mut r = self.take(
            shot.aspect,
            out.as_deref(),
            shot.strict,
            layout,
            shot.energy,
        );
        if r.is_none() && self.export {
            // offline: wait for the pool rather than take a quick one
            let t0 = Instant::now();
            while r.is_none()
                && t0.elapsed() < Duration::from_secs(40)
                && !self.stop.load(Ordering::SeqCst)
            {
                self.fill_pool();
                {
                    let p = self.pool.lock().unwrap();
                    let (p, _) = self
                        .pool_cond
                        .wait_timeout(p, Duration::from_millis(100))
                        .unwrap();
                    if p.preparing == 0 && p.ready.is_empty() && p.bad.len() >= p.sources.len() {
                        break;
                    }
                }
                r = self.take(
                    shot.aspect,
                    out.as_deref(),
                    shot.strict,
                    layout,
                    shot.energy,
                );
            }
        }
        let smart = r.is_some();
        let r = match r.or_else(|| {
            // nothing ready: a field rather keeps its clip (strict), else a quick one
            if shot.strict {
                if let Some(o) = &out {
                    return Some(Ready {
                        item: self.items.iter().find(|x| x.path == o.path).cloned()?,
                        start: o.start,
                        end_at: o.end_at,
                        sig: o.sig.clone(),
                        motion: None,
                        kb: o.kb,
                    });
                }
            }
            self.quick()
        }) {
            Some(r) => r,
            None => return,
        };
        let a = self.start_shot(plan, shot, r, smart);
        self.assigned.lock().unwrap().insert(i, a);
    }

    fn start_shot(
        self: &Arc<Self>,
        plan: &Plan,
        shot: &Shot,
        r: Ready,
        smart: bool,
    ) -> Arc<Assigned> {
        let (mw, mh) = (r.item.w.max(1) as f64, r.item.h.max(1) as f64);
        let (fw, fh) = decode_size(
            &self.s.fit,
            shot.field_w as f64,
            shot.field_h as f64,
            mw,
            mh,
        );
        let focus = r.sig.as_ref().map(|s| s.focus).unwrap_or((0.5, 0.5));
        let live = Mutex::new(LiveSig {
            focus,
            target: focus,
            lum: r.sig.as_ref().map(|s| s.lum),
            measured_t: shot.t_on,
        });
        let mut zoom_n = 0;
        {
            let mut p = self.pool.lock().unwrap();
            p.zoom_n += 1;
            zoom_n = zoom_n.max(p.zoom_n);
        }
        let _ = zoom_n;
        if r.item.kind == Kind::Image {
            let key = (r.item.id, fw, fh);
            let pix = self.pic_cache.lock().unwrap().get(&key).cloned();
            let frames = match pix {
                Some(p) => {
                    let f = Frames::new(fw, fh, 1.0, 0);
                    let mut f = Arc::try_unwrap(f).ok().unwrap();
                    f.still = Some(p);
                    f.done.store(true, Ordering::SeqCst);
                    Arc::new(f)
                }
                None => {
                    let f = Frames::new(fw, fh, 1.0, 1);
                    let me = self.clone();
                    let item = r.item.clone();
                    let fc = f.clone();
                    std::thread::spawn(move || {
                        match ff::decode_rgba(&item.path, None, fw, fh, true)
                            .ok()
                            .and_then(|v| to_pixmap(fw, fh, v))
                        {
                            Some(p) => {
                                let p = Arc::new(p);
                                me.pic_cache.lock().unwrap().insert(key, p.clone());
                                fc.slots.lock().unwrap()[0] = Some(p);
                                fc.decoded.store(1, Ordering::SeqCst);
                            }
                            None => {
                                *fc.failed.lock().unwrap() =
                                    Some("this picture can't be read".into());
                                me.note_fail(&item, "this picture can't be read");
                            }
                        }
                        fc.done.store(true, Ordering::SeqCst);
                        let _g = fc.gate.lock().unwrap();
                        fc.cond.notify_all();
                    });
                    f
                }
            };
            return Arc::new(Assigned {
                shot: shot.id,
                kind: Kind::Image,
                name: r.item.name.clone(),
                path: r.item.path.clone(),
                w: r.item.w,
                h: r.item.h,
                start: 0.0,
                end_at: None,
                sig: r.sig,
                kb: r.kb,
                frames,
                live,
                smart,
            });
        }
        // Video: how far into the clip this shot goes (speed ramps, stutter, the loop back near its end)
        let fps = if self.export {
            self.s.export_fps.max(24.0)
        } else {
            30.0
        };
        let probe = Assigned {
            shot: shot.id,
            kind: Kind::Video,
            name: r.item.name.clone(),
            path: r.item.path.clone(),
            w: r.item.w,
            h: r.item.h,
            start: r.start,
            end_at: r.end_at,
            sig: r.sig.clone(),
            kb: 0.0,
            frames: Frames::new(fw, fh, fps, 0),
            live: Mutex::new(LiveSig {
                focus,
                target: focus,
                lum: None,
                measured_t: 0.0,
            }),
            smart,
        };
        let mut max_idx = 0usize;
        let end = shot.until.min(plan.duration + 1.0);
        let mut t = shot.t_on;
        while t <= end + 1e-9 {
            max_idx = max_idx.max(probe.index_at(plan, shot, t));
            t += 1.0 / 60.0;
        }
        let n = max_idx + 3;
        let frames = Frames::new(fw, fh, fps, n);
        let a = Arc::new(Assigned {
            frames: frames.clone(),
            live,
            ..probe
        });
        let me = self.clone();
        let path = r.item.path.clone();
        let item = r.item.clone();
        let shot_c = shot.clone();
        let plan_rates = PlanView::from(plan, shot);
        let a2 = Arc::downgrade(&a);
        std::thread::spawn(move || {
            let hw = me.s.hwaccel;
            let mut got = me.decode(&frames, &path, r.start, n, hw, &shot_c, &plan_rates, &a2);
            if got == 0 && hw && !frames.cancel.load(Ordering::SeqCst) {
                got = me.decode(&frames, &path, r.start, n, false, &shot_c, &plan_rates, &a2);
            }
            if got == 0 && !frames.cancel.load(Ordering::SeqCst) {
                let why = frames
                    .failed
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "no picture – ffmpeg can't decode this video".into());
                me.note_fail(&item, &why);
                me.pool.lock().unwrap().bad.insert(item.id, why);
            }
            frames.done.store(true, Ordering::SeqCst);
            let _g = frames.gate.lock().unwrap();
            frames.cond.notify_all();
        });
        a
    }

    /// Run one ffmpeg for a shot; returns how many frames came
    #[allow(clippy::too_many_arguments)]
    fn decode(
        &self,
        f: &Arc<Frames>,
        path: &std::path::Path,
        start: f64,
        n: usize,
        hw: bool,
        shot: &Shot,
        view: &PlanView,
        me_a: &std::sync::Weak<Assigned>,
    ) -> usize {
        let Ok(mut c) = ff::ffmpeg() else { return 0 };
        if hw {
            c.args(["-hwaccel", "auto"]);
        }
        c.arg("-ss")
            .arg(format!("{:.3}", start.max(0.0)))
            .arg("-i")
            .arg(path);
        c.arg("-t").arg(format!("{:.3}", n as f64 / f.fps + 0.2));
        let flags = if self.export { "bicubic" } else { "bilinear" };
        c.args(["-an", "-sn", "-dn", "-vf"]).arg(format!(
            "fps={},scale={}:{}:flags={flags},format=rgba",
            f.fps, f.fw, f.fh
        ));
        c.args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
        let mut child = match c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(ch) => ch,
            Err(e) => {
                *f.failed.lock().unwrap() = Some(format!("ffmpeg didn't start: {e}"));
                return 0;
            }
        };
        let mut out = child.stdout.take().unwrap();
        let mut errp = child.stderr.take().unwrap();
        let err_t = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = errp.read_to_string(&mut s);
            s
        });
        let size = (f.fw * f.fh * 4) as usize;
        let mut got = 0;
        let mut last_drop = Instant::now();
        while got < n {
            if f.cancel.load(Ordering::SeqCst) || self.stop.load(Ordering::SeqCst) {
                break;
            }
            // Don't run too far ahead (memory): at most ~2.5 s of video ahead of where the show is
            loop {
                if f.cancel.load(Ordering::SeqCst) || self.stop.load(Ordering::SeqCst) {
                    break;
                }
                let now = self.clock.get();
                let ahead_s = got as f64 / f.fps;
                let allowed = ((now - shot.t_on + self.lookahead + 0.5).max(0.0)) * 2.0 + 1.5;
                if ahead_s <= allowed {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            let mut buf = vec![0u8; size];
            match ff::read_full(&mut out, &mut buf) {
                Ok(true) => {}
                _ => break,
            }
            let Some(pm) = Pixmap::from_vec(buf, tiny_skia::IntSize::from_wh(f.fw, f.fh).unwrap())
            else {
                break;
            };
            {
                let mut s = f.slots.lock().unwrap();
                if got < s.len() {
                    s[got] = Some(Arc::new(pm));
                }
            }
            got += 1;
            f.decoded.store(got, Ordering::SeqCst);
            {
                let _g = f.gate.lock().unwrap();
                f.cond.notify_all();
            }
            // Frames no longer needed by this shot are let go
            if last_drop.elapsed() > Duration::from_millis(250) {
                last_drop = Instant::now();
                if let Some(a) = me_a.upgrade() {
                    let now = self.clock.get().max(shot.t_on);
                    let mut lo = usize::MAX;
                    let mut t = now;
                    while t <= shot.until {
                        lo = lo.min(view.index(&a, t));
                        t += 1.0 / 30.0;
                    }
                    // (a decoder that fell behind keeps its newest frame – better late than black)
                    if lo != usize::MAX && lo > 2 {
                        f.drop_below((lo - 2).min(got.saturating_sub(1)));
                    }
                } else {
                    break; // nobody needs this shot any more
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        let err = err_t.join().unwrap_or_default();
        if got == 0 && !err.trim().is_empty() {
            *f.failed.lock().unwrap() = Some(err.lines().last().unwrap_or("").to_string());
        }
        got
    }

    /// A picture for the image flash / overlay, at the frame's size (cached)
    pub fn picture(&self, idx: usize, w: u32, h: u32, cover_ok: bool) -> Option<Arc<Pixmap>> {
        let it = self.images.get(idx)?;
        if it.w == 0 {
            return None;
        }
        let (mw, mh) = (it.w as f64, it.h as f64);
        let fit = if cover_ok {
            fit_for("contain", w as f64, h as f64, mw, mh)
        } else {
            "contain"
        };
        let r = if fit == "contain" {
            (w as f64 / mw).min(h as f64 / mh)
        } else {
            (w as f64 / mw).max(h as f64 / mh)
        };
        let (fw, fh) = (
            ((mw * r).round() as u32).max(2),
            ((mh * r).round() as u32).max(2),
        );
        let key = (usize::MAX - it.id, fw, fh);
        if let Some(p) = self.pic_cache.lock().unwrap().get(&key) {
            return Some(p.clone());
        }
        let p = Arc::new(to_pixmap(
            fw,
            fh,
            ff::decode_rgba(&it.path, None, fw, fh, true).ok()?,
        )?);
        let mut c = self.pic_cache.lock().unwrap();
        if c.len() > 64 {
            c.clear();
        }
        c.insert(key, p.clone());
        Some(p)
    }

    /// Decode the pictures the plan will flash ahead of time (export / start of a show)
    pub fn warm_pictures(self: &Arc<Self>, plan: &Plan, w: u32, h: u32) {
        let mut ids: Vec<(usize, bool)> = Vec::new();
        for e in &plan.fx {
            match e.fx {
                crate::plan::Fx::PicFlash { pic, .. } => ids.push((pic, true)),
                crate::plan::Fx::PicOver { pic, .. } => ids.push((pic, false)),
                _ => {}
            }
        }
        ids.sort();
        ids.dedup();
        let me = self.clone();
        std::thread::spawn(move || {
            for (i, flash) in ids {
                if me.stop.load(Ordering::SeqCst) {
                    break;
                }
                let _ = me.picture(i, w, h, flash);
            }
        });
    }
}

/// What a decoder needs of the plan to know which frames its shot still needs
pub struct PlanView {
    plan_rates: Vec<(f64, f64)>,
    cum: Vec<f64>,
    stutters: Vec<f64>,
    shot: Shot,
}
impl PlanView {
    fn from(plan: &Plan, shot: &Shot) -> PlanView {
        let lo = shot.t_on - 1.0;
        let hi = shot.until + 1.0;
        // (the rate before the shot matters too: keep everything up to its end – it's small)
        let mut cum = Vec::new();
        let rates: Vec<(f64, f64)> = plan.rates.iter().copied().filter(|r| r.0 <= hi).collect();
        for r in &rates {
            cum.push(plan.rate_integral(r.0));
        }
        PlanView {
            plan_rates: rates,
            cum,
            stutters: plan
                .stutters
                .iter()
                .copied()
                .filter(|&s| s >= lo && s <= hi)
                .collect(),
            shot: shot.clone(),
        }
    }
    fn integral(&self, t: f64) -> f64 {
        let i = self.plan_rates.partition_point(|r| r.0 <= t);
        if i == 0 {
            t
        } else {
            self.cum[i - 1] + (t - self.plan_rates[i - 1].0) * self.plan_rates[i - 1].1
        }
    }
    fn index(&self, a: &Assigned, t: f64) -> usize {
        let shot = &self.shot;
        let t = t.max(shot.t_on);
        let lim = t.min(shot.t_off);
        let i = self.stutters.partition_point(|&s| s <= lim);
        let (mut bt, mut base) = (shot.t_on, 0.0);
        if i > 0 && self.stutters[i - 1] > shot.t_on {
            bt = self.stutters[i - 1];
            base = (bt - shot.t_on) * 0.25;
        }
        let mut c = base + self.integral(t) - self.integral(bt);
        if let Some(end) = a.end_at {
            let period = end - a.start;
            if period > 0.1 && a.start + c >= end {
                c %= period;
            }
        }
        (c.max(0.0) * a.frames.fps).floor() as usize
    }
}
