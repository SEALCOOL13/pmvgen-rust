// The song plays through the sound card (cpal); its position is the show's clock.
// Without a sound device the show still runs on a silent clock.

use crate::song::Pcm;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Shared {
    pcm: Arc<Pcm>,
    pos: AtomicU64, // frames played
    playing: AtomicBool,
    vol: AtomicU32,
    cb: Mutex<(Instant, u64)>, // when the last block was handed over, and the position then
}

pub struct Player {
    _stream: Option<cpal::Stream>,
    sh: Arc<Shared>,
    // silent clock (no sound device)
    silent: Option<Mutex<(Option<Instant>, f64)>>,
    pub device_error: Option<String>,
}

/// The sample rate the sound card wants (the song is decoded at it)
pub fn device_rate() -> u32 {
    cpal::default_host()
        .default_output_device()
        .and_then(|d| d.default_output_config().ok())
        .map(|c| c.sample_rate().0)
        .filter(|&r| (8000..=384_000).contains(&r))
        .unwrap_or(44100)
}

fn build<T: SizedSample + FromSample<f32>>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    sh: Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let ch = cfg.channels as usize;
    dev.build_output_stream(
        cfg,
        move |out: &mut [T], _| {
            let frames = out.len() / ch.max(1);
            let playing = sh.playing.load(Ordering::Relaxed);
            let vol = f32::from_bits(sh.vol.load(Ordering::Relaxed));
            let mut pos = sh.pos.load(Ordering::Relaxed) as usize;
            let data = &sh.pcm.data;
            let total = data.len() / 2;
            for f in 0..frames {
                let (l, r) = if playing && pos < total {
                    let v = (
                        data[2 * pos] as f32 / 32768.0 * vol,
                        data[2 * pos + 1] as f32 / 32768.0 * vol,
                    );
                    pos += 1;
                    v
                } else {
                    (0.0, 0.0)
                };
                for c in 0..ch {
                    let v = match (ch, c) {
                        (1, _) => (l + r) / 2.0,
                        (_, 0) => l,
                        (_, 1) => r,
                        _ => 0.0,
                    };
                    out[f * ch + c] = T::from_sample(v);
                }
            }
            if playing {
                sh.pos.store(pos as u64, Ordering::Relaxed);
                if let Ok(mut g) = sh.cb.lock() {
                    *g = (Instant::now(), pos as u64);
                }
            }
        },
        |e| eprintln!("[pmvgen] sound: {e}"),
        None,
    )
}

impl Player {
    pub fn new(pcm: Arc<Pcm>, volume: f32) -> Player {
        let sh = Arc::new(Shared {
            pcm,
            pos: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            vol: AtomicU32::new(volume.to_bits()),
            cb: Mutex::new((Instant::now(), 0)),
        });
        let host = cpal::default_host();
        let res = (|| -> Result<cpal::Stream, String> {
            let dev = host.default_output_device().ok_or("no sound device")?;
            let sup = dev.default_output_config().map_err(|e| e.to_string())?;
            let mut cfg: cpal::StreamConfig = sup.config();
            cfg.sample_rate = cpal::SampleRate(sh.pcm.rate);
            let s = match sup.sample_format() {
                cpal::SampleFormat::F32 => build::<f32>(&dev, &cfg, sh.clone()),
                cpal::SampleFormat::I16 => build::<i16>(&dev, &cfg, sh.clone()),
                cpal::SampleFormat::U16 => build::<u16>(&dev, &cfg, sh.clone()),
                cpal::SampleFormat::I32 => build::<i32>(&dev, &cfg, sh.clone()),
                cpal::SampleFormat::F64 => build::<f64>(&dev, &cfg, sh.clone()),
                f => return Err(format!("unsupported sample format {f:?}")),
            }
            .map_err(|e| e.to_string())?;
            s.play().map_err(|e| e.to_string())?;
            Ok(s)
        })();
        match res {
            Ok(s) => Player {
                _stream: Some(s),
                sh,
                silent: None,
                device_error: None,
            },
            Err(e) => Player {
                _stream: None,
                sh,
                silent: Some(Mutex::new((None, 0.0))),
                device_error: Some(e),
            },
        }
    }

    pub fn duration(&self) -> f64 {
        self.sh.pcm.frames() as f64 / self.sh.pcm.rate as f64
    }

    /// Song position in seconds (smoothed between the sound card's blocks)
    pub fn time(&self) -> f64 {
        if let Some(s) = &self.silent {
            let g = s.lock().unwrap();
            return match g.0 {
                Some(t0) => g.1 + t0.elapsed().as_secs_f64(),
                None => g.1,
            };
        }
        let rate = self.sh.pcm.rate as f64;
        if !self.sh.playing.load(Ordering::Relaxed) {
            return self.sh.pos.load(Ordering::Relaxed) as f64 / rate;
        }
        let (at, pos) = *self.sh.cb.lock().unwrap();
        // (the block handed over last is heard over the next ~blocks; never run past 50 ms ahead)
        pos as f64 / rate + at.elapsed().as_secs_f64().min(0.05)
    }

    pub fn playing(&self) -> bool {
        match &self.silent {
            Some(s) => s.lock().unwrap().0.is_some(),
            None => self.sh.playing.load(Ordering::Relaxed),
        }
    }

    pub fn play(&self) {
        if let Some(s) = &self.silent {
            let mut g = s.lock().unwrap();
            if g.0.is_none() {
                g.0 = Some(Instant::now());
            }
            return;
        }
        *self.sh.cb.lock().unwrap() = (Instant::now(), self.sh.pos.load(Ordering::Relaxed));
        self.sh.playing.store(true, Ordering::Relaxed);
    }
    pub fn pause(&self) {
        if let Some(s) = &self.silent {
            let mut g = s.lock().unwrap();
            if let Some(t0) = g.0.take() {
                g.1 += t0.elapsed().as_secs_f64();
            }
            return;
        }
        let t = self.time();
        self.sh.playing.store(false, Ordering::Relaxed);
        self.sh
            .pos
            .store((t * self.sh.pcm.rate as f64) as u64, Ordering::Relaxed);
    }
    pub fn ended(&self) -> bool {
        self.time() >= self.duration() - 0.01
    }
}
