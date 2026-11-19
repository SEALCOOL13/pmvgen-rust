// Export: the show rendered frame by frame (no dropped frames, whatever the computer) and encoded by
// ffmpeg together with the song – an MP4 that every player can open and seek.

use crate::ff;
use crate::library::Item;
use crate::settings::Settings;
use crate::show;
use crate::song::Song;
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Default)]
pub struct Progress {
    pub frac: f32,
    pub msg: String,
    pub eta: Option<f64>,
    pub fails: Vec<String>,
}

pub fn run(
    song: &Song,
    items: Vec<Item>,
    images: Vec<Item>,
    s: &Settings,
    out: &Path,
    prog: &Arc<Mutex<Progress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<PathBuf> {
    ff::available()?;
    let set = |f: f32, m: &str, eta: Option<f64>| {
        let mut p = prog.lock().unwrap();
        p.frac = f;
        p.msg = m.to_string();
        p.eta = eta;
    };
    set(0.0, "Preparing clips …", None);
    let mut built = show::build(song, items, images, s, s.quality, true)?;
    let plan = built.plan.clone();
    let media = built.media.clone();
    let (w, h) = (plan.w, plan.h);
    let fps = s.export_fps.max(1.0);
    let total = (plan.duration * fps).ceil() as usize;

    let enc = ff::video_encoder();
    let mut c = ff::ffmpeg()?;
    c.args(["-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg(format!("{w}x{h}"));
    c.arg("-r").arg(format!("{fps}")).args(["-i", "pipe:0"]);
    if song.from > 0.0 {
        c.arg("-ss").arg(format!("{:.3}", song.from));
    }
    c.arg("-t").arg(format!("{:.3}", plan.duration));
    c.arg("-i").arg(&song.path);
    c.args(["-map", "0:v:0", "-map", "1:a:0", "-c:v", enc]);
    match enc {
        "libx264" => {
            c.args(["-preset", "veryfast", "-crf"])
                .arg(format!("{}", s.export_crf.round() as i64));
        }
        "mpeg4" => {
            c.args(["-q:v", "3"]);
        }
        _ => {
            c.args(["-b:v", if s.quality >= 1080.0 { "12M" } else { "6M" }]);
        }
    }
    c.args(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]);
    let vol = s.song_vol / 100.0;
    if (vol - 1.0).abs() > 0.001 {
        c.arg("-af").arg(format!("volume={vol:.3}"));
    }
    c.args(["-shortest", "-movflags", "+faststart"]).arg(out);
    c.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = c.spawn().context("starting ffmpeg")?;
    let mut stdin = child.stdin.take().unwrap();
    let mut errp = child.stderr.take().unwrap();
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = errp.read_to_string(&mut s);
        s
    });

    let t0 = Instant::now();
    let prof = std::env::var_os("PMVGEN_PROF").is_some();
    let (mut tt, mut tf, mut tw) = (0.0, 0.0, 0.0);
    let mut write_err = None;
    for i in 0..total {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let t = i as f64 / fps;
        let a = Instant::now();
        media.tick(&plan, t);
        let b = Instant::now();
        let frame = built.renderer.frame(t);
        let c2 = Instant::now();
        if let Err(e) = stdin.write_all(frame.data()) {
            write_err = Some(e);
            break;
        }
        if prof {
            tt += (b - a).as_secs_f64();
            tf += (c2 - b).as_secs_f64();
            tw += c2.elapsed().as_secs_f64();
        }
        if i % 10 == 0 {
            let f = (i + 1) as f32 / total as f32;
            let el = t0.elapsed().as_secs_f64();
            let eta = if i > 30 {
                Some(el / f as f64 - el)
            } else {
                None
            };
            set(
                f,
                &format!(
                    "Rendering {} / {} s",
                    t.round() as i64,
                    plan.duration.round() as i64
                ),
                eta,
            );
            let fails: Vec<String> = media
                .failures
                .lock()
                .unwrap()
                .iter()
                .map(|f| format!("{}: {}", f.name, f.why))
                .collect();
            prog.lock().unwrap().fails = fails;
        }
    }
    drop(stdin);
    if prof {
        eprintln!(
            "[prof] tick {tt:.1} s, frames {tf:.1} s, write {tw:.1} s, draw {:.1} s",
            built.renderer.draw_time
        );
    }
    media.stop();
    let status = child.wait()?;
    let err = err_t.join().unwrap_or_default();
    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(out);
        bail!("Cancelled");
    }
    if !status.success() || write_err.is_some() {
        let last = err
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string();
        bail!(
            "The video couldn't be written{}",
            if last.is_empty() {
                String::new()
            } else {
                format!(": {last}")
            }
        );
    }
    // Funscript next to the video
    if s.fs_on {
        if let Ok(fs) = crate::funscript::build(
            &song.a.beats,
            &song.a.energy,
            song.a.bpm,
            plan.duration,
            song.bars.as_deref(),
            s,
            &song.name,
        ) {
            let _ = std::fs::write(
                out.with_extension("funscript"),
                serde_json::to_vec(&fs).unwrap_or_default(),
            );
        }
    }
    set(1.0, "Done", None);
    Ok(out.to_path_buf())
}

pub fn file_name(title: &str) -> String {
    let s: String = format!("PMV - {title}")
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || (c as u32) < 32 {
                '_'
            } else {
                c
            }
        })
        .collect();
    s.chars().take(100).collect()
}
