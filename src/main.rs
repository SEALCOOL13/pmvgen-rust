// PMV Generator – native rewrite of pmvgen.github.io (2.38.10) in Rust.
// Pick a song and a folder of clips – on every beat it cuts to the next one.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod bars;
mod beats;
mod compositor;
#[cfg(test)]
mod difftest;
mod export;
mod ff;
mod funscript;
mod library;
mod media;
mod plan;
mod player;
mod raster;
mod render;
mod settings;
mod show;
mod smart;
mod song;
mod text;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const HELP: &str = "PMV Generator 2.38.10 (native)

  pmvgen [FILES / FOLDERS]    open the app (songs and clip folders given are added)
  pmvgen --export --song FILE --clips DIR [--clips DIR …] --out FILE.mp4
         [--settings FILE.pmvgen.json] [--from SEC] [--to SEC]
                              render a video without the window
  pmvgen --analyze FILE       print tempo and beats of a song (JSON)
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // no options: the app (paths given are opened in it – songs and clip folders / files)
    if args.iter().all(|a| !a.starts_with("--") && a != "-h") {
        let paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
        if let Err(e) = app::run(paths) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let code = match cli(&args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn cli(args: &[String]) -> anyhow::Result<()> {
    let mut song = None;
    let mut clips: Vec<PathBuf> = Vec::new();
    let mut out = None;
    let mut sfile = None;
    let mut from = 0.0;
    let mut to = None;
    let mut mode = "";
    let mut i = 0;
    let val = |i: usize| {
        args.get(i + 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("{} needs a value", args[i]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(());
            }
            "--export" => mode = "export",
            "--analyze" => {
                mode = "analyze";
                song = Some(PathBuf::from(val(i)?));
                i += 1;
            }
            "--song" => {
                song = Some(PathBuf::from(val(i)?));
                i += 1;
            }
            "--clips" => {
                clips.push(PathBuf::from(val(i)?));
                i += 1;
            }
            "--out" => {
                out = Some(PathBuf::from(val(i)?));
                i += 1;
            }
            "--settings" => {
                sfile = Some(PathBuf::from(val(i)?));
                i += 1;
            }
            "--from" => {
                from = val(i)?.parse()?;
                i += 1;
            }
            "--to" => {
                to = Some(val(i)?.parse()?);
                i += 1;
            }
            a => anyhow::bail!("unknown option {a}\n\n{HELP}"),
        }
        i += 1;
    }
    ff::available()?;
    let cancel = AtomicBool::new(false);
    let song_path = song.ok_or_else(|| anyhow::anyhow!("--song is missing"))?;
    eprintln!("Reading {} …", song_path.display());
    let s0 = song::load(&song_path, from, to, 44100, &|_| {}, &cancel)?;
    if mode == "analyze" {
        let j = serde_json::json!({
            "bpm": s0.a.bpm, "duration": s0.duration(), "beats": s0.a.beats, "energy": s0.a.energy,
            "downbeats": s0.bars.as_ref().map(|b| b.downbeat.iter().enumerate().filter(|(_, d)| **d).map(|(k, _)| k).collect::<Vec<_>>()),
        });
        println!("{}", serde_json::to_string_pretty(&j)?);
        return Ok(());
    }
    if mode != "export" {
        anyhow::bail!("nothing to do\n\n{HELP}");
    }
    let mut s = settings::Settings::default();
    if let Some(f) = sfile {
        let (_, o) = settings::import_file(&std::fs::read(&f)?)?;
        s = s.with_preset(&o);
    }
    let lib: library::Shared = Default::default();
    for c in &clips {
        if c.is_dir() {
            library::add_folder(&lib, c);
        } else {
            library::add_files(&lib, std::slice::from_ref(c));
        }
    }
    // probe everything first (the size decides the shape filters)
    let busy = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    library::probe_all(&lib, &busy);
    while busy.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let (items, images) = {
        let l = lib.lock().unwrap();
        for it in l.items.iter().filter(|i| i.bad.is_some()) {
            eprintln!(
                "skipped {}: {}",
                it.name,
                it.bad.clone().unwrap_or_default()
            );
        }
        (l.matching(&s), l.images())
    };
    eprintln!(
        "{} BPM, {:.1} s, {} beats; {} clips",
        s0.a.bpm.round(),
        s0.duration(),
        s0.a.beats.len(),
        items.len()
    );
    let out = out.ok_or_else(|| anyhow::anyhow!("--out is missing"))?;
    let prog = Arc::new(Mutex::new(export::Progress::default()));
    let cancel = Arc::new(AtomicBool::new(false));
    let p2 = prog.clone();
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let printer = std::thread::spawn(move || {
        let mut last = String::new();
        while !d2.load(std::sync::atomic::Ordering::SeqCst) {
            let p = p2.lock().unwrap().clone();
            let line = format!("{:5.1} %  {}", p.frac * 100.0, p.msg);
            if line != last {
                eprintln!("{line}");
                last = line;
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    });
    let t0 = std::time::Instant::now();
    let r = export::run(&s0, items, images, &s, &out, &prog, &cancel);
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = printer.join();
    let path = r?;
    for f in prog.lock().unwrap().fails.iter() {
        eprintln!("clip left out – {f}");
    }
    eprintln!(
        "Wrote {} in {:.1} s",
        path.display(),
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}
