// Building a show from a song, the clips and the settings – shared by the live show and the export.

use crate::library::{Item, Kind};
use crate::media::Media;
use crate::plan::{self, Plan};
use crate::render::Renderer;
use crate::settings::Settings;
use crate::song::Song;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Picture size for a quality (short side) and format
pub fn size_for(s: &Settings, short: f64) -> (i32, i32) {
    let short = short.round() as i32;
    let long = ((short as f64) * 16.0 / 9.0).round() as i32;
    let even = |n: i32| (n / 2) * 2;
    if s.format == "9:16" {
        (even(short), even(long))
    } else {
        (even(long), even(short))
    }
}

pub struct Built {
    pub plan: Arc<Plan>,
    pub media: Arc<Media>,
    pub renderer: Renderer,
}

pub fn build(
    song: &Song,
    items: Vec<Item>,
    images: Vec<Item>,
    s: &Settings,
    short: f64,
    export: bool,
) -> anyhow::Result<Built> {
    if items.is_empty() {
        anyhow::bail!("No clips match the filters – add a folder with videos (or images) first");
    }
    let (w, h) = size_for(s, short);
    // Per-layout clip shape: a layout opens only if the library has enough clips of that shape
    let shapes: Vec<(u32, u32)> = items.iter().map(|i| (i.w, i.h)).collect();
    let layout_ok = |id: &str| -> bool {
        let rule = s.layout_shape.get(id).map(|x| x.as_str()).unwrap_or("all");
        if rule != "portrait" && rule != "landscape" {
            return true;
        }
        let n = shapes
            .iter()
            .filter(|(w, h)| *w > 0 && if rule == "portrait" { h > w } else { w >= h })
            .count();
        n >= plan::layout(id).groups
    };
    let imgs_ok: Vec<Item> = images
        .into_iter()
        .filter(|i| i.kind == Kind::Image && i.w > 0)
        .collect();
    let duration = song.duration();
    let p = plan::make(&plan::Input {
        beats: &song.a.beats,
        energy: &song.a.energy,
        duration,
        bars: if s.bars { song.bars.as_deref() } else { None },
        s,
        w,
        h,
        n_images: imgs_ok.len(),
        layout_ok: &layout_ok,
        seed: rand::random(),
    });
    let plan = Arc::new(p);
    let media = Media::new(s, items, imgs_ok, song.a.bpm, duration, export);
    let title = if s.title.trim().is_empty() {
        song.name.clone()
    } else {
        s.title.trim().to_string()
    };
    let renderer = Renderer::new(plan.clone(), media.clone(), title, song.a.bpm, export);
    media.warm_pictures(&plan, w as u32, h as u32);
    Ok(Built {
        plan,
        media,
        renderer,
    })
}

/// Keeps the clips coming for a live show (a timer thread)
pub fn run_scheduler(plan: Arc<Plan>, media: Arc<Media>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let now = media.clock.get();
            media.tick(&plan, now);
            std::thread::sleep(Duration::from_millis(20));
        }
        media.stop();
    });
}

/// Before the song starts: wait until the first clips are ready (at most 20 s, like the browser version)
pub fn preroll(plan: &Plan, media: &Arc<Media>, deadline: Instant) -> bool {
    media.clock.set(0.0);
    media.tick(plan, 0.0);
    let (_, cur, _) = plan.shots_at(0.0);
    let all_ready = || {
        let a = media.assigned.lock().unwrap();
        cur.iter().all(|id| {
            id.and_then(|i| a.get(&i))
                .map(|x| x.frames.decoded() > 0 || x.frames.done.load(Ordering::SeqCst))
                .unwrap_or(false)
        })
    };
    while Instant::now() < deadline {
        media.tick(plan, 0.0);
        let (ready, _) = media.ready_count();
        if all_ready() && ready >= 2 {
            return true;
        }
        if all_ready() && Instant::now() + Duration::from_secs(14) > deadline {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    all_ready()
}
