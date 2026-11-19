// All settings (same names and meaning as the web version's, so "My settings" files move between the two)
// plus a few of this version's own. Saved in the user's config folder.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn lb<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Bool(b) => b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => matches!(s.as_str(), "true" | "1" | "on" | "yes"),
        _ => false,
    })
}
fn ln<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        Value::Bool(b) => b as i32 as f64,
        _ => 0.0,
    })
}
fn ls<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 {
                format!("{}", f as i64)
            } else {
                f.to_string()
            }
        }
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    })
}
fn lmap<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let mut out = BTreeMap::new();
    if let Value::Object(m) = Value::deserialize(d)? {
        for (k, v) in m {
            if let Value::String(s) = v {
                out.insert(k, s);
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Layouts {
    #[serde(deserialize_with = "lb")]
    pub full: bool,
    #[serde(deserialize_with = "lb")]
    pub kaleido: bool,
    #[serde(deserialize_with = "lb")]
    pub duo: bool,
    #[serde(deserialize_with = "lb")]
    pub trim: bool,
    #[serde(deserialize_with = "lb")]
    pub tri: bool,
    #[serde(deserialize_with = "lb")]
    pub quad: bool,
}
impl Default for Layouts {
    fn default() -> Self {
        Layouts {
            full: true,
            kaleido: true,
            duo: true,
            trim: true,
            tri: true,
            quad: true,
        }
    }
}
impl Layouts {
    pub fn get(&self, id: &str) -> bool {
        match id {
            "full" => self.full,
            "kaleido" => self.kaleido,
            "duo" => self.duo,
            "trim" => self.trim,
            "tri" => self.tri,
            "quad" => self.quad,
            _ => false,
        }
    }
    pub fn get_mut(&mut self, id: &str) -> &mut bool {
        match id {
            "full" => &mut self.full,
            "kaleido" => &mut self.kaleido,
            "duo" => &mut self.duo,
            "trim" => &mut self.trim,
            "tri" => &mut self.tri,
            _ => &mut self.quad,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Fx {
    #[serde(deserialize_with = "lb")]
    pub flash: bool,
    #[serde(deserialize_with = "lb")]
    pub zoom: bool,
    #[serde(deserialize_with = "lb")]
    pub shake: bool,
    #[serde(deserialize_with = "lb")]
    pub glitch: bool,
    #[serde(deserialize_with = "lb")]
    pub stutter: bool,
    #[serde(deserialize_with = "lb")]
    pub hue: bool,
    #[serde(deserialize_with = "lb")]
    pub rgb: bool,
    #[serde(deserialize_with = "lb")]
    pub echo: bool,
    #[serde(deserialize_with = "lb")]
    pub tunnel: bool,
    #[serde(deserialize_with = "lb")]
    pub invert: bool,
    #[serde(deserialize_with = "lb")]
    pub whip: bool,
    #[serde(deserialize_with = "lb")]
    pub zoomin: bool,
    #[serde(deserialize_with = "lb")]
    pub speed: bool,
    #[serde(deserialize_with = "lb")]
    pub voice: bool,
    #[serde(deserialize_with = "lb")]
    pub vhs: bool,
    #[serde(deserialize_with = "lb")]
    pub strobe: bool,
    #[serde(deserialize_with = "lb")]
    pub text: bool,
    #[serde(deserialize_with = "lb")]
    pub kenburns: bool,
    #[serde(deserialize_with = "lb")]
    pub lines: bool,
}
impl Default for Fx {
    fn default() -> Self {
        Fx {
            flash: true,
            zoom: true,
            shake: false,
            glitch: false,
            stutter: false,
            hue: false,
            rgb: true,
            echo: false,
            tunnel: false,
            invert: false,
            whip: false,
            zoomin: true,
            speed: false,
            voice: true,
            vhs: false,
            strobe: false,
            text: false,
            kenburns: true,
            lines: true,
        }
    }
}
pub const FX_KEYS: [&str; 19] = [
    "flash", "zoom", "shake", "glitch", "stutter", "hue", "rgb", "echo", "tunnel", "invert",
    "whip", "zoomin", "speed", "voice", "vhs", "strobe", "text", "kenburns", "lines",
];
impl Fx {
    pub fn get_mut(&mut self, k: &str) -> &mut bool {
        match k {
            "flash" => &mut self.flash,
            "zoom" => &mut self.zoom,
            "shake" => &mut self.shake,
            "glitch" => &mut self.glitch,
            "stutter" => &mut self.stutter,
            "hue" => &mut self.hue,
            "rgb" => &mut self.rgb,
            "echo" => &mut self.echo,
            "tunnel" => &mut self.tunnel,
            "invert" => &mut self.invert,
            "whip" => &mut self.whip,
            "zoomin" => &mut self.zoomin,
            "speed" => &mut self.speed,
            "voice" => &mut self.voice,
            "vhs" => &mut self.vhs,
            "strobe" => &mut self.strobe,
            "text" => &mut self.text,
            "kenburns" => &mut self.kenburns,
            _ => &mut self.lines,
        }
    }
    pub fn set_all(&mut self, on: &[&str]) {
        for k in FX_KEYS {
            *self.get_mut(k) = on.contains(&k);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Funscript
    #[serde(deserialize_with = "lb")]
    pub fs_on: bool,
    #[serde(deserialize_with = "ls")]
    pub fs_pace: String,
    #[serde(deserialize_with = "ls")]
    pub fs_size: String,
    #[serde(deserialize_with = "ls")]
    pub fs_where: String,
    #[serde(deserialize_with = "ls")]
    pub fs_style: String,
    #[serde(deserialize_with = "lb")]
    pub fs_rests: bool,
    #[serde(deserialize_with = "lb")]
    pub fs_accent: bool,
    #[serde(deserialize_with = "ln")]
    pub fs_speed: f64,
    // Music
    #[serde(deserialize_with = "lb")]
    pub shuffle: bool,
    // Clips
    #[serde(deserialize_with = "ls")]
    pub source: String,
    #[serde(deserialize_with = "ls")]
    pub shape: String,
    #[serde(deserialize_with = "lmap")]
    pub layout_shape: BTreeMap<String, String>,
    #[serde(deserialize_with = "lb")]
    pub best_spots: bool,
    #[serde(deserialize_with = "lb")]
    pub clean_cuts: bool,
    #[serde(deserialize_with = "lb")]
    pub smart_crop: bool,
    #[serde(deserialize_with = "lb")]
    pub variety: bool,
    #[serde(deserialize_with = "lb")]
    pub sequenced: bool,
    #[serde(deserialize_with = "lb")]
    pub match_cut: bool,
    #[serde(deserialize_with = "ln")]
    pub skip_head: f64,
    #[serde(deserialize_with = "ln")]
    pub skip_tail: f64,
    // Cutting
    #[serde(deserialize_with = "ls")]
    pub cut: String,
    #[serde(deserialize_with = "lb")]
    pub bars: bool,
    #[serde(deserialize_with = "lb")]
    pub reveal: bool,
    #[serde(deserialize_with = "lb")]
    pub scroll: bool,
    pub layouts: Layouts,
    #[serde(deserialize_with = "ln")]
    pub cut_lead: f64,
    #[serde(deserialize_with = "ln")]
    pub min_beats: f64,
    #[serde(deserialize_with = "ln")]
    pub max_beats: f64,
    #[serde(deserialize_with = "ls")]
    pub pace: String,
    // Effects
    pub fx: Fx,
    #[serde(deserialize_with = "ls")]
    pub words: String,
    #[serde(deserialize_with = "ln")]
    pub pulse_amt: f64,
    #[serde(deserialize_with = "ls")]
    pub pulse_on: String,
    // Pictures
    #[serde(deserialize_with = "lb")]
    pub pic_flash: bool,
    #[serde(deserialize_with = "ls")]
    pub pic_flash_rate: String,
    #[serde(deserialize_with = "ls")]
    pub pic_flash_mode: String,
    #[serde(deserialize_with = "ln")]
    pub pic_flash_len: f64,
    #[serde(deserialize_with = "ln")]
    pub pic_flash_pct: f64,
    #[serde(deserialize_with = "lb")]
    pub pic_overlay: bool,
    #[serde(deserialize_with = "ln")]
    pub pic_overlay_amt: f64,
    #[serde(deserialize_with = "ln")]
    pub pic_overlay_len: f64,
    #[serde(deserialize_with = "ln")]
    pub pic_overlay_pct: f64,
    // Look
    #[serde(deserialize_with = "ls")]
    pub look: String,
    #[serde(deserialize_with = "ln")]
    pub look_amt: f64,
    #[serde(deserialize_with = "ls")]
    pub look_color: String,
    #[serde(deserialize_with = "ln")]
    pub bright: f64,
    #[serde(deserialize_with = "lb")]
    pub look_even: bool,
    #[serde(deserialize_with = "lb")]
    pub smooth: bool,
    // Frame
    #[serde(deserialize_with = "lb")]
    pub soft: bool,
    #[serde(deserialize_with = "ln")]
    pub soft_amt: f64,
    #[serde(deserialize_with = "ln")]
    pub div_w: f64,
    #[serde(deserialize_with = "ls")]
    pub edge: String,
    #[serde(deserialize_with = "ln")]
    pub edge_amt: f64,
    #[serde(deserialize_with = "ls")]
    pub format: String,
    #[serde(deserialize_with = "ls")]
    pub split: String,
    #[serde(deserialize_with = "ls")]
    pub fit: String,
    // Sound
    #[serde(deserialize_with = "ln")]
    pub song_vol: f64,
    #[serde(deserialize_with = "ln")]
    pub clip_vol: f64,
    #[serde(deserialize_with = "ls")]
    pub voice_mode: String,
    // Output
    #[serde(deserialize_with = "lb")]
    pub intro: bool,
    #[serde(deserialize_with = "lb")]
    pub outro: bool,
    #[serde(deserialize_with = "lb")]
    pub intro_each: bool,
    #[serde(deserialize_with = "ls")]
    pub title: String,
    #[serde(deserialize_with = "ln")]
    pub quality: f64,
    // This version's own
    #[serde(deserialize_with = "ln")]
    pub export_fps: f64,
    #[serde(deserialize_with = "ln")]
    pub show_quality: f64,
    #[serde(deserialize_with = "lb")]
    pub hwaccel: bool,
    #[serde(deserialize_with = "ln")]
    pub export_crf: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fs_on: false,
            fs_pace: "auto".into(),
            fs_size: "auto".into(),
            fs_where: "mid".into(),
            fs_style: "sharp".into(),
            fs_rests: true,
            fs_accent: true,
            fs_speed: 300.0,
            shuffle: false,
            source: "scene".into(),
            shape: "all".into(),
            layout_shape: BTreeMap::new(),
            best_spots: true,
            clean_cuts: true,
            smart_crop: true,
            variety: true,
            sequenced: false,
            match_cut: true,
            skip_head: 0.0,
            skip_tail: 0.0,
            cut: "auto".into(),
            bars: true,
            reveal: false,
            scroll: false,
            layouts: Layouts::default(),
            cut_lead: 0.0,
            min_beats: 0.0,
            max_beats: 0.0,
            pace: "normal".into(),
            fx: Fx::default(),
            words: String::new(),
            pulse_amt: 100.0,
            pulse_on: "beat".into(),
            pic_flash: false,
            pic_flash_rate: "auto".into(),
            pic_flash_mode: "both".into(),
            pic_flash_len: 2.0,
            pic_flash_pct: 20.0,
            pic_overlay: false,
            pic_overlay_amt: 35.0,
            pic_overlay_len: 4.0,
            pic_overlay_pct: 30.0,
            look: "none".into(),
            look_amt: 100.0,
            look_color: "#ff4d94".into(),
            bright: 0.0,
            look_even: true,
            smooth: true,
            soft: false,
            soft_amt: 50.0,
            div_w: 2.0,
            edge: "off".into(),
            edge_amt: 50.0,
            format: "16:9".into(),
            split: "cols".into(),
            fit: "contain".into(),
            song_vol: 100.0,
            clip_vol: 50.0,
            voice_mode: "drops".into(),
            intro: false,
            outro: false,
            intro_each: false,
            title: String::new(),
            quality: 720.0,
            export_fps: 30.0,
            show_quality: 720.0,
            hwaccel: true,
            export_crf: 20.0,
        }
    }
}

pub const LAYOUT_IDS: [&str; 6] = ["full", "kaleido", "duo", "trim", "tri", "quad"];

// Keys that are not part of "My settings" (web version's list + this version's own machine settings)
const NOT_IN_PRESET: [&str; 6] = [
    "title",
    "shuffle",
    "showQuality",
    "hwaccel",
    "exportFps",
    "exportCrf",
];

pub struct Preset {
    pub name: &'static str,
    pub cut: &'static str,
    pub layouts: &'static [&'static str],
    pub fx: &'static [&'static str],
}
pub const PRESETS: [Preset; 4] = [
    Preset {
        name: "PMV classic",
        cut: "auto",
        layouts: &["full", "trim", "tri"],
        fx: &[
            "flash", "zoom", "whip", "zoomin", "speed", "voice", "rgb", "glitch", "stutter",
            "shake", "kenburns", "lines",
        ],
    },
    Preset {
        name: "Maximal",
        cut: "1",
        layouts: &["full", "kaleido", "duo", "trim", "tri", "quad"],
        fx: &[
            "flash", "zoom", "whip", "zoomin", "speed", "voice", "rgb", "echo", "tunnel", "invert",
            "glitch", "stutter", "shake", "hue", "text", "kenburns", "lines",
        ],
    },
    Preset {
        name: "Hypno",
        cut: "2",
        layouts: &["full", "kaleido", "trim"],
        fx: &["zoom", "speed", "echo", "tunnel", "hue", "vhs", "kenburns"],
    },
    Preset {
        name: "Clean",
        cut: "auto",
        layouts: &["full", "duo", "trim"],
        fx: &["zoom", "whip", "kenburns"],
    },
];

impl Settings {
    pub fn apply_mood(&mut self, p: &Preset) {
        self.cut = p.cut.to_string();
        for id in LAYOUT_IDS {
            *self.layouts.get_mut(id) = p.layouts.contains(&id);
        }
        self.fx.set_all(p.fx);
    }

    /// Fix values this version doesn't know (older, edited or plugin files) → the default
    pub fn sanitize(&mut self) {
        let d = Settings::default();
        let pick = |v: &mut String, ok: &[&str], def: &str| {
            if !ok.contains(&v.as_str()) {
                *v = def.to_string();
            }
        };
        pick(&mut self.cut, &["auto", "1", "2", "4"], &d.cut);
        pick(
            &mut self.look,
            &[
                "none", "warm", "pink", "cold", "vivid", "bw", "noir", "custom",
            ],
            &d.look,
        );
        pick(&mut self.source, &["scene", "image", "both"], &d.source);
        pick(&mut self.shape, &["all", "portrait", "landscape"], &d.shape);
        pick(&mut self.pace, &["slow", "normal", "fast"], &d.pace);
        pick(&mut self.pulse_on, &["beat", "2", "bar"], &d.pulse_on);
        pick(
            &mut self.pic_flash_rate,
            &["auto", "1", "2", "4", "8"],
            &d.pic_flash_rate,
        );
        pick(
            &mut self.pic_flash_mode,
            &["single", "burst", "both"],
            &d.pic_flash_mode,
        );
        pick(&mut self.edge, &["off", "blur", "motion", "lens"], &d.edge);
        // ("window" – match the window – is the web version's own; here 16:9)
        pick(&mut self.format, &["16:9", "9:16"], &d.format);
        pick(&mut self.split, &["cols", "rows"], &d.split);
        pick(&mut self.fit, &["contain", "cover"], &d.fit);
        pick(&mut self.voice_mode, &["drops", "always"], &d.voice_mode);
        pick(
            &mut self.fs_pace,
            &["auto", "slow", "normal", "fast"],
            &d.fs_pace,
        );
        pick(
            &mut self.fs_size,
            &["auto", "small", "medium", "large", "full"],
            &d.fs_size,
        );
        pick(&mut self.fs_where, &["low", "mid", "high"], &d.fs_where);
        pick(&mut self.fs_style, &["sharp", "smooth"], &d.fs_style);
        if self.quality != 1080.0 {
            self.quality = 720.0;
        }
        if ![360.0, 540.0, 720.0, 1080.0].contains(&self.show_quality) {
            self.show_quality = 720.0;
        }
        if self.export_fps != 60.0 {
            self.export_fps = 30.0;
        }
        self.export_crf = self.export_crf.clamp(12.0, 35.0);
        if !LAYOUT_IDS.iter().any(|id| self.layouts.get(id)) {
            self.layouts = Layouts::default();
        }
        self.layout_shape.retain(|k, v| {
            LAYOUT_IDS.contains(&k.as_str()) && (v == "portrait" || v == "landscape")
        });
    }

    pub fn snapshot(&self) -> Value {
        let mut v = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Value::Object(m) = &mut v {
            for k in NOT_IN_PRESET {
                m.remove(k);
            }
        }
        v
    }

    /// A saved / imported set over the defaults; title and machine settings stay as they are
    pub fn with_preset(&self, o: &Value) -> Settings {
        let mut base = serde_json::to_value(Settings::default()).unwrap();
        if let (Value::Object(b), Value::Object(src)) = (&mut base, o) {
            for (k, v) in src {
                if k == "fx" || k == "layouts" {
                    // merge, so missing keys keep their defaults
                    if let (Some(Value::Object(bm)), Value::Object(sm)) = (b.get_mut(k), v) {
                        for (kk, vv) in sm {
                            bm.insert(kk.clone(), vv.clone());
                        }
                    }
                } else {
                    b.insert(k.clone(), v.clone());
                }
            }
            let cur = serde_json::to_value(self).unwrap();
            for k in NOT_IN_PRESET {
                if let Some(x) = cur.get(k) {
                    b.insert(k.to_string(), x.clone());
                }
            }
        }
        let mut s: Settings = serde_json::from_value(base).unwrap_or_else(|_| self.clone());
        s.sanitize();
        s
    }
}

// ---------- On disk ----------

pub fn config_dir() -> PathBuf {
    let d = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pmvgen");
    let _ = std::fs::create_dir_all(&d);
    d
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Stored {
    pub settings: Option<Value>,
    pub presets: BTreeMap<String, Value>,
    pub folders: Vec<String>,
    pub last_dir: Option<String>,
}

pub fn load_stored() -> Stored {
    std::fs::read(config_dir().join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn save_stored(st: &Stored) -> anyhow::Result<()> {
    let p = config_dir().join("settings.json");
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(st)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}
pub fn settings_from_stored(st: &Stored) -> Settings {
    let mut s = st
        .settings
        .as_ref()
        .and_then(|v| serde_json::from_value::<Settings>(v.clone()).ok())
        .unwrap_or_default();
    s.sanitize();
    s
}

/// The web version's export file: { pmvGenerator: 1, name, settings }
pub fn export_file(name: &str, s: &Settings) -> Value {
    serde_json::json!({ "pmvGenerator": 1, "name": name, "settings": s.snapshot() })
}
pub fn import_file(bytes: &[u8]) -> anyhow::Result<(String, Value)> {
    let d: Value = serde_json::from_slice(bytes)?;
    let o = d
        .get("settings")
        .filter(|x| x.is_object())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("This isn't a PMV Generator settings file"))?;
    let name = d
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("Imported")
        .to_string();
    Ok((name, o))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lenient_import() {
        let j = serde_json::json!({"cut": 2, "quality": "1080", "fx": {"shake": 1}, "layouts": {"quad": false}, "picFlashRate": 4, "bogus": [1]});
        let s = Settings::default().with_preset(&j);
        assert_eq!(s.cut, "2");
        assert_eq!(s.quality, 1080.0);
        assert!(s.fx.shake && s.fx.flash);
        assert!(!s.layouts.quad && s.layouts.full);
        assert_eq!(s.pic_flash_rate, "4");
    }
}
