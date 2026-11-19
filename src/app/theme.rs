// The look of the web version (app.css / stage.css / standalone.css): its colours, fonts, the "liquid glass"
// background, and its components – step cards, segmented pills, option tiles with hatched switches, slider rows,
// collapsible sections, drop zones and line icons – drawn with egui.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Response, Sense,
    Shadow, Shape, Stroke, StrokeKind, TextStyle, TextureHandle, Ui, Vec2,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------- Colours (app.css :root) ----------
pub const BG: Color32 = Color32::from_rgb(0x23, 0x0f, 0x1f);
pub const BG2: Color32 = Color32::from_rgb(0x34, 0x17, 0x2e);
pub const BG3: Color32 = Color32::from_rgb(0x4a, 0x23, 0x42);
pub const BASE: Color32 = Color32::from_rgb(0x1a, 0x0a, 0x17);
pub const TEXT: Color32 = Color32::from_rgb(0xfb, 0xef, 0xf4);
pub const TEXT2: Color32 = Color32::from_rgb(0xd6, 0xbc, 0xcd);
pub const FAINT: Color32 = Color32::from_rgb(0xa0, 0x7a, 0x96);
pub const PINK: Color32 = Color32::from_rgb(0xff, 0x3e, 0x8a);
pub const PINK2: Color32 = Color32::from_rgb(0xff, 0x6e, 0xa8);
pub const PINK_DEEP: Color32 = Color32::from_rgb(0x7a, 0x1d, 0x4a);
pub const SHINE: Color32 = Color32::from_rgb(0x8f, 0xe3, 0xff);
pub const VIOLET: Color32 = Color32::from_rgb(0xb4, 0x8c, 0xff);
pub const PAPER: Color32 = TEXT;
pub const INK: Color32 = BG;
pub const OK: Color32 = Color32::from_rgb(0x5e, 0xe3, 0x9a);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x5a, 0x5a);
pub const SECHEAD_HOVER: Color32 = Color32::from_rgb(0x5a, 0x2b, 0x50);

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}
/// The glass edge: rgba(255, 255, 255, .14)
pub fn glass_edge() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 36)
}

/// Liquid glass on (the switch at the top right) – read by every widget
static GLASS: AtomicBool = AtomicBool::new(true);
pub fn set_glass(on: bool) {
    GLASS.store(on, Ordering::Relaxed);
}
pub fn glass() -> bool {
    GLASS.load(Ordering::Relaxed)
}

// ---------- Fonts: the CSS font stacks, as far as this computer has them ----------

pub const DISPLAY: &str = "display";
pub const BOLD: &str = "bold";
pub const MEDIUM: &str = "medium";

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        if let Some(w) = std::env::var_os("WINDIR") {
            v.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(
                PathBuf::from(l)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Fonts"),
            );
        }
    } else if cfg!(target_os = "macos") {
        v.push("/System/Library/Fonts".into());
        v.push("/System/Library/Fonts/Supplemental".into());
        v.push("/Library/Fonts".into());
        if let Some(h) = dirs::home_dir() {
            v.push(h.join("Library/Fonts"));
        }
    } else {
        v.push("/usr/share/fonts".into());
        v.push("/usr/local/share/fonts".into());
        if let Some(h) = dirs::home_dir() {
            v.push(h.join(".local/share/fonts"));
            v.push(h.join(".fonts"));
        }
    }
    v
}

/// All font files below the font folders (a few levels deep): lower-case name → path
fn font_files() -> std::collections::HashMap<String, PathBuf> {
    let mut out = std::collections::HashMap::new();
    let mut stack: Vec<(PathBuf, usize)> = font_dirs().into_iter().map(|d| (d, 0)).collect();
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
            } else if let Some(n) = p.file_name() {
                let n = n.to_string_lossy().to_lowercase();
                if n.ends_with(".ttf") || n.ends_with(".otf") {
                    out.entry(n).or_insert(p);
                }
            }
        }
    }
    out
}

fn first_font(
    files: &std::collections::HashMap<String, PathBuf>,
    names: &[&str],
) -> Option<(String, Vec<u8>)> {
    for n in names {
        if let Some(p) = files.get(&n.to_lowercase()) {
            if let Ok(b) = std::fs::read(p) {
                if ttf_parser::Face::parse(&b, 0).is_ok() {
                    return Some((n.to_string(), b));
                }
            }
        }
    }
    None
}

/// --f-text: Bahnschrift, "DIN Alternate", "Segoe UI", …, system-ui; --f-display: "Bahnschrift Condensed",
/// Bahnschrift, "Arial Narrow", "Segoe UI" – the same order here, then the usual Linux UI fonts, then egui's own
pub fn install_fonts(ctx: &egui::Context) {
    let files = font_files();
    let text = first_font(
        &files,
        &[
            "bahnschrift.ttf",
            "DIN Alternate Bold.ttf",
            "segoeui.ttf",
            "NotoSans-Regular.ttf",
            "Ubuntu-R.ttf",
            "DejaVuSans.ttf",
            "LiberationSans-Regular.ttf",
            "Cantarell-Regular.otf",
        ],
    );
    let medium = first_font(
        &files,
        &[
            "bahnschrift.ttf",
            "DIN Alternate Bold.ttf",
            "seguisb.ttf",
            "NotoSans-Medium.ttf",
            "Ubuntu-M.ttf",
            "NotoSans-SemiBold.ttf",
            "DejaVuSans.ttf",
            "LiberationSans-Regular.ttf",
        ],
    );
    let bold = first_font(
        &files,
        &[
            "DIN Alternate Bold.ttf",
            "segoeuib.ttf",
            "NotoSans-Bold.ttf",
            "Ubuntu-B.ttf",
            "DejaVuSans-Bold.ttf",
            "LiberationSans-Bold.ttf",
            "bahnschrift.ttf",
        ],
    );
    let display = first_font(
        &files,
        &[
            "ARIALNB.TTF",
            "Arial Narrow Bold.ttf",
            "LiberationSansNarrow-Bold.ttf",
            "NotoSans-CondensedBold.ttf",
            "RobotoCondensed-Bold.ttf",
            "DejaVuSansCondensed-Bold.ttf",
            "UbuntuCondensed-Regular.ttf",
        ],
    );
    let mut defs = egui::FontDefinitions::default();
    let mut add = |key: &str, data: Option<(String, Vec<u8>)>, fallback: &'static [u8]| {
        let bytes = data.map(|d| d.1).unwrap_or_else(|| fallback.to_vec());
        defs.font_data.insert(
            key.to_string(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
    };
    static ANTON: &[u8] = include_bytes!("../../assets/Anton-Regular.ttf");
    let have_text = text.is_some();
    add("pmv-text", text, ANTON);
    add("pmv-medium", medium, ANTON);
    add("pmv-bold", bold, ANTON);
    add("pmv-display", display, ANTON);
    let base: Vec<String> = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let with = |first: &str| -> Vec<String> {
        let mut v = vec![first.to_string()];
        v.extend(base.iter().cloned());
        v
    };
    if have_text {
        defs.families
            .insert(FontFamily::Proportional, with("pmv-text"));
    }
    defs.families
        .insert(FontFamily::Name(MEDIUM.into()), with("pmv-medium"));
    defs.families
        .insert(FontFamily::Name(BOLD.into()), with("pmv-bold"));
    defs.families
        .insert(FontFamily::Name(DISPLAY.into()), with("pmv-display"));
    ctx.set_fonts(defs);
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}
pub fn text(size: f32) -> FontId {
    FontId::proportional(size)
}

/// egui's widgets in the web version's colours
pub fn install_style(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = BG2;
        v.window_stroke = Stroke::new(1.0_f32, BG3);
        v.window_corner_radius = CornerRadius::same(10);
        v.extreme_bg_color = BASE;
        v.faint_bg_color = BG2;
        v.code_bg_color = BASE;
        v.hyperlink_color = PINK;
        v.selection.bg_fill = alpha(PINK, 0.55);
        v.selection.stroke = Stroke::new(1.0_f32, PINK);
        v.text_cursor.stroke = Stroke::new(2.0_f32, PINK);
        let w = &mut v.widgets;
        let r = CornerRadius::same(4);
        w.noninteractive.bg_fill = BG2;
        w.noninteractive.weak_bg_fill = BG2;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BG3);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT2);
        w.noninteractive.corner_radius = r;
        // (bg_fill is also the rail of sliders – it has to show on the page)
        w.inactive.bg_fill = BG3;
        w.inactive.weak_bg_fill = BG2;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, BG3);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = r;
        w.hovered.bg_fill = BG3;
        w.hovered.weak_bg_fill = BG3;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, FAINT);
        w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.hovered.corner_radius = r;
        w.active.bg_fill = PINK;
        w.active.weak_bg_fill = PINK;
        w.active.bg_stroke = Stroke::new(1.0_f32, PINK);
        w.active.fg_stroke = Stroke::new(1.0_f32, INK);
        w.active.corner_radius = r;
        w.open.bg_fill = BG3;
        w.open.weak_bg_fill = BG3;
        w.open.bg_stroke = Stroke::new(1.0_f32, PINK);
        w.open.corner_radius = r;
        v.popup_shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(110),
        };
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.combo_height = 320.0;
        s.text_styles = [
            (TextStyle::Heading, display(24.0)),
            (TextStyle::Body, text(15.0)),
            (TextStyle::Button, medium(14.0)),
            (TextStyle::Small, text(12.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
    });
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
}

// ---------- The page background ----------

/// The page background as a picture (rendered once per window size): the base colour, in glass mode three soft
/// colour glows, and the halftone dots fading out towards the top right (body::before)
pub fn background_image(w: u32, h: u32, glass: bool) -> egui::ColorImage {
    use tiny_skia::{
        Color, FillRule, GradientStop, Paint, PathBuilder, Pixmap, Point, RadialGradient,
        SpreadMode, Transform,
    };
    let (w, h) = (w.clamp(16, 8192), h.clamp(16, 8192));
    let Some(mut pm) = Pixmap::new(w, h) else {
        return egui::ColorImage::new([1, 1], BG);
    };
    pm.fill(Color::from_rgba8(BG.r(), BG.g(), BG.b(), 255));
    let (wf, hf) = (w as f32, h as f32);
    let rect = tiny_skia::Rect::from_xywh(0.0, 0.0, wf, hf);
    let glow = |pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: [u8; 3], a: f32| {
        let Some(sh) = RadialGradient::new(
            Point::from_xy(cx, cy),
            Point::from_xy(cx, cy),
            r,
            vec![
                GradientStop::new(0.0, Color::from_rgba8(c[0], c[1], c[2], (a * 255.0) as u8)),
                GradientStop::new(0.7, Color::from_rgba8(c[0], c[1], c[2], 0)),
                GradientStop::new(1.0, Color::from_rgba8(c[0], c[1], c[2], 0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return;
        };
        let p = Paint {
            shader: sh,
            anti_alias: true,
            ..Default::default()
        };
        if let Some(r) = rect {
            pm.fill_rect(r, &p, Transform::identity(), None);
        }
    };
    if glass {
        // radial-gradient(40vw at 12% 18%, pink 34%) · (45vw at 88% 12%, violet 26%) · (50vw at 70% 92%, pink 22% + shine)
        glow(
            &mut pm,
            wf * 0.12,
            hf * 0.18,
            wf * 0.40,
            [0xff, 0x3e, 0x8a],
            0.34,
        );
        glow(
            &mut pm,
            wf * 0.88,
            hf * 0.12,
            wf * 0.45,
            [0xb4, 0x8c, 0xff],
            0.26,
        );
        glow(
            &mut pm,
            wf * 0.70,
            hf * 0.92,
            wf * 0.50,
            [0xe6, 0x5c, 0xa0],
            0.30,
        );
    }
    // halftone: dots every 7 px (radius ~1.4), pink at 22 %, masked by an ellipse at the top right
    let dots_a = if glass { 0.35 } else { 1.0 };
    let mut pb = PathBuilder::new();
    let (ex, ey) = (wf * 0.7, hf * 0.6);
    let mut y = 3.5f32;
    while y < hf {
        let mut x = 3.5f32;
        while x < wf {
            let dx = (x - wf) / ex;
            let dy = y / ey;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 0.7 {
                let m = 1.0 - d / 0.7;
                let r = 1.4 * m.sqrt().max(0.25);
                pb.push_circle(x, y, r);
            }
            x += 7.0;
        }
        y += 7.0;
    }
    if let Some(path) = pb.finish() {
        let mut p = Paint::default();
        p.set_color_rgba8(0xff, 0x3e, 0x8a, (0.22 * dots_a * 255.0) as u8);
        p.anti_alias = true;
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
    egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pm.data())
}

/// Keeps the background picture in step with the window size and the glass switch
#[derive(Default)]
pub struct Background {
    tex: Option<TextureHandle>,
    key: (u32, u32, bool),
}
impl Background {
    pub fn paint(&mut self, ctx: &egui::Context, painter: &egui::Painter, rect: Rect) {
        let ppp = ctx.pixels_per_point();
        // (a quarter of the pixels is plenty for soft glows; dots stay crisp enough)
        let scale = (ppp * 0.5).max(0.5);
        let (w, h) = (
            (rect.width() * scale).round().max(16.0) as u32,
            (rect.height() * scale).round().max(16.0) as u32,
        );
        let key = (w / 32, h / 32, glass());
        if self.tex.is_none() || key != self.key {
            let img = background_image(w, h, glass());
            let opts = egui::TextureOptions::LINEAR;
            match &mut self.tex {
                Some(t) => t.set(img, opts),
                None => self.tex = Some(ctx.load_texture("pmv-background", img, opts)),
            }
            self.key = key;
        }
        if let Some(t) = &self.tex {
            painter.image(
                t.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

// ---------- Painting helpers ----------

/// The pink hatching (repeating-linear-gradient(-60deg, pink 0 2px, transparent 2px 5px)) inside a rect
pub fn hatch(painter: &egui::Painter, rect: Rect, color: Color32) {
    let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let step = 5.0;
    let slope = (60f32).to_radians().tan(); // lines at -60°
    let h = rect.height();
    let mut x = rect.left() - h / slope - step;
    while x < rect.right() + step {
        let a = Pos2::new(x, rect.bottom());
        let b = Pos2::new(x + h / slope, rect.top());
        p.line_segment([a, b], Stroke::new(2.0_f32, color));
        x += step;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Music,
    Folder,
    Film,
    Bolt,
    Sliders,
    Plus,
    Download,
    Upload,
    Eye,
    Check,
    Edit,
    Play,
    Pause,
    Chevron,
    Heart,
    Close,
    Pin,
    Save,
    Info,
    Expand,
    Prev,
    Next,
    Undo,
    Dice,
}

/// A line icon (24×24 design, 2 px strokes) in `rect`
pub fn icon(painter: &egui::Painter, rect: Rect, which: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - Vec2::splat(12.0 * s);
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let st = Stroke::new((2.0 * s).max(1.2), color);
    let line = |pts: &[(f32, f32)]| {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::line(v, st));
    };
    let closed = |pts: &[(f32, f32)]| {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::closed_line(v, st));
    };
    let filled = |pts: &[(f32, f32)]| {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| p(x, y)).collect();
        painter.add(Shape::convex_polygon(v, color, Stroke::NONE));
    };
    match which {
        Icon::Music => {
            line(&[(9.0, 18.0), (9.0, 5.0), (20.0, 3.0), (20.0, 16.0)]);
            painter.circle_stroke(p(6.5, 18.0), 2.6 * s, st);
            painter.circle_stroke(p(17.5, 16.0), 2.6 * s, st);
        }
        Icon::Folder => closed(&[
            (3.0, 6.0),
            (9.0, 6.0),
            (11.0, 8.0),
            (21.0, 8.0),
            (21.0, 19.0),
            (3.0, 19.0),
        ]),
        Icon::Film => {
            painter.rect_stroke(
                Rect::from_min_max(p(3.0, 4.0), p(21.0, 20.0)),
                CornerRadius::same((2.0 * s) as u8),
                st,
                StrokeKind::Middle,
            );
            line(&[(8.0, 4.0), (8.0, 20.0)]);
            line(&[(16.0, 4.0), (16.0, 20.0)]);
            line(&[(3.0, 12.0), (21.0, 12.0)]);
        }
        Icon::Bolt => filled(&[
            (13.0, 2.0),
            (4.0, 14.0),
            (11.0, 14.0),
            (10.0, 22.0),
            (20.0, 9.0),
            (13.0, 9.0),
        ]),
        Icon::Sliders => {
            line(&[(4.0, 7.0), (20.0, 7.0)]);
            line(&[(4.0, 17.0), (20.0, 17.0)]);
            painter.circle_filled(p(9.0, 7.0), 2.4 * s, color);
            painter.circle_filled(p(15.0, 17.0), 2.4 * s, color);
        }
        Icon::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Download => {
            line(&[(12.0, 4.0), (12.0, 15.0)]);
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            line(&[(5.0, 20.0), (19.0, 20.0)]);
        }
        Icon::Upload => {
            line(&[(12.0, 15.0), (12.0, 4.0)]);
            line(&[(7.0, 9.0), (12.0, 4.0), (17.0, 9.0)]);
            line(&[(5.0, 20.0), (19.0, 20.0)]);
        }
        Icon::Eye => {
            line(&[
                (2.0, 12.0),
                (7.0, 6.5),
                (12.0, 5.0),
                (17.0, 6.5),
                (22.0, 12.0),
                (17.0, 17.5),
                (12.0, 19.0),
                (7.0, 17.5),
                (2.0, 12.0),
            ]);
            painter.circle_stroke(p(12.0, 12.0), 3.0 * s, st);
        }
        Icon::Check => line(&[(5.0, 12.5), (9.5, 17.0), (19.0, 7.5)]),
        Icon::Edit => {
            closed(&[
                (4.0, 20.0),
                (5.0, 15.0),
                (16.0, 4.0),
                (20.0, 8.0),
                (9.0, 19.0),
            ]);
            line(&[(13.5, 6.5), (17.5, 10.5)]);
        }
        Icon::Play => filled(&[(7.0, 4.0), (20.0, 12.0), (7.0, 20.0)]),
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(p(6.0, 4.0), p(10.0, 20.0)), 1.0, color);
            painter.rect_filled(Rect::from_min_max(p(14.0, 4.0), p(18.0, 20.0)), 1.0, color);
        }
        Icon::Chevron => line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Icon::Heart => {
            // M16 26s-8-4.9-10.2-9.9C4.2 12.5 6.4 8.5 10.3 8.5c2.3 0 4.1 1.4 5.7 3.4 1.5-2 3.3-3.4 5.6-3.4 3.9 0 6.2 4 4.6 7.6C24 21.1 16 26 16 26z (32 grid)
            let k = 24.0 / 32.0;
            let pts: Vec<Pos2> = [
                (16.0, 26.0),
                (10.5, 22.0),
                (5.8, 16.1),
                (5.0, 12.0),
                (7.0, 9.4),
                (10.3, 8.5),
                (13.6, 9.6),
                (16.0, 11.9),
                (18.3, 9.6),
                (21.6, 8.5),
                (25.0, 9.4),
                (26.9, 12.0),
                (26.2, 16.1),
                (21.5, 22.0),
            ]
            .iter()
            .map(|&(x, y)| p(x * k, y * k))
            .collect();
            // (a heart isn't convex: two halves)
            let mid = p(16.0 * k, 26.0 * k);
            let left: Vec<Pos2> = pts[..8].to_vec();
            let mut right: Vec<Pos2> = pts[7..].to_vec();
            right.push(mid);
            painter.add(Shape::convex_polygon(left, color, Stroke::NONE));
            painter.add(Shape::convex_polygon(right, color, Stroke::NONE));
            painter.add(Shape::convex_polygon(
                vec![p(5.8 * k, 16.1 * k), pts[7], p(26.2 * k, 16.1 * k), mid],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::Pin => {
            closed(&[
                (9.0, 3.0),
                (15.0, 3.0),
                (14.0, 9.0),
                (18.0, 13.0),
                (6.0, 13.0),
                (10.0, 9.0),
            ]);
            line(&[(12.0, 13.0), (12.0, 21.0)]);
        }
        Icon::Save => {
            closed(&[
                (4.0, 4.0),
                (17.0, 4.0),
                (20.0, 7.0),
                (20.0, 20.0),
                (4.0, 20.0),
            ]);
            line(&[(8.0, 4.0), (8.0, 9.0), (15.0, 9.0), (15.0, 4.0)]);
            painter.rect_stroke(
                Rect::from_min_max(p(7.0, 13.0), p(17.0, 20.0)),
                0.0,
                st,
                StrokeKind::Middle,
            );
        }
        Icon::Info => {
            painter.circle_stroke(p(12.0, 12.0), 9.0 * s, st);
            line(&[(12.0, 11.0), (12.0, 17.0)]);
            painter.circle_filled(p(12.0, 7.5), 1.3 * s, color);
        }
        Icon::Expand => {
            line(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)]);
            line(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)]);
            line(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)]);
            line(&[(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)]);
        }
        Icon::Prev => {
            filled(&[(18.0, 5.0), (8.0, 12.0), (18.0, 19.0)]);
            line(&[(6.0, 5.0), (6.0, 19.0)]);
        }
        Icon::Next => {
            filled(&[(6.0, 5.0), (16.0, 12.0), (6.0, 19.0)]);
            line(&[(18.0, 5.0), (18.0, 19.0)]);
        }
        Icon::Undo => {
            line(&[(9.0, 5.0), (4.0, 10.0), (9.0, 15.0)]);
            line(&[
                (4.0, 10.0),
                (15.0, 10.0),
                (19.0, 13.0),
                (19.0, 17.0),
                (16.0, 20.0),
                (11.0, 20.0),
            ]);
        }
        Icon::Dice => {
            painter.rect_stroke(
                Rect::from_min_max(p(4.0, 4.0), p(20.0, 20.0)),
                CornerRadius::same((3.0 * s) as u8),
                st,
                StrokeKind::Middle,
            );
            for (x, y) in [
                (8.5, 8.5),
                (15.5, 15.5),
                (12.0, 12.0),
                (15.5, 8.5),
                (8.5, 15.5),
            ] {
                painter.circle_filled(p(x, y), 1.4 * s, color);
            }
        }
    }
}

/// Layout pictogram (layoutIcon of the web version: 24×15)
pub fn layout_icon(painter: &egui::Painter, rect: Rect, id: &str, color: Color32) {
    let s = (rect.width() / 24.0).min(rect.height() / 15.0);
    let o = rect.center() - Vec2::new(12.0 * s, 7.5 * s);
    let r = |x: f32, y: f32, w: f32, h: f32, on: bool| {
        let rr = Rect::from_min_size(Pos2::new(o.x + x * s, o.y + y * s), Vec2::new(w * s, h * s));
        if on {
            painter.rect_filled(rr, s, color);
        } else {
            painter.rect_stroke(rr, s, Stroke::new(1.2 * s, color), StrokeKind::Inside);
        }
    };
    match id {
        "kaleido" => {
            r(1.0, 1.0, 10.5, 6.0, true);
            r(12.5, 1.0, 10.5, 6.0, true);
            r(1.0, 8.0, 10.5, 6.0, true);
            r(12.5, 8.0, 10.5, 6.0, true);
        }
        "duo" => {
            r(1.0, 1.0, 10.5, 13.0, true);
            r(12.5, 1.0, 10.5, 13.0, false);
        }
        "trim" => {
            r(1.0, 1.0, 6.7, 13.0, true);
            r(8.7, 1.0, 6.6, 13.0, false);
            r(16.3, 1.0, 6.7, 13.0, true);
        }
        "tri" => {
            r(1.0, 1.0, 6.7, 13.0, true);
            r(8.7, 1.0, 6.6, 13.0, false);
            r(16.3, 1.0, 6.7, 13.0, false);
        }
        "quad" => {
            r(1.0, 1.0, 10.5, 6.0, true);
            r(12.5, 1.0, 10.5, 6.0, false);
            r(1.0, 8.0, 10.5, 6.0, false);
            r(12.5, 8.0, 10.5, 6.0, true);
        }
        _ => r(1.0, 1.0, 22.0, 13.0, true),
    }
}

// ---------- Components ----------

/// A card (.kb-card) – with glass: see-through, light edge, rounder
pub fn card_frame() -> egui::Frame {
    if glass() {
        egui::Frame::new()
            .fill(alpha(BG2, 0.55))
            .stroke(Stroke::new(1.0_f32, glass_edge()))
            .corner_radius(CornerRadius::same(18))
            .inner_margin(Margin::symmetric(22, 20))
            .shadow(Shadow {
                offset: [0, 12],
                blur: 34,
                spread: 0,
                color: Color32::from_black_alpha(82),
            })
    } else {
        egui::Frame::new()
            .fill(BG2)
            .stroke(Stroke::new(1.0_f32, BG3))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(22, 20))
    }
}

/// The "glass shine" over a card: a light diagonal sheen at its top left
fn shine(painter: &egui::Painter, rect: Rect, radius: u8) {
    if !glass() {
        return;
    }
    // (inset top highlight, inside the rounded corners)
    let r = radius as f32;
    painter.line_segment(
        [
            Pos2::new(rect.left() + r, rect.top() + 1.0),
            Pos2::new(rect.right() - r, rect.top() + 1.0),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(46)),
    );
}

/// The numbered step badge (.kb-pmvg-no)
pub fn step_badge(ui: &mut Ui, n: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect.translate(Vec2::splat(2.0)), 4.0, PAPER);
    p.rect_filled(rect, 4.0, PINK);
    p.text(rect.center(), Align2::CENTER_CENTER, n, display(18.0), INK);
}

/// A card with a step number and a title
pub fn card<R>(ui: &mut Ui, step: Option<&str>, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let resp = card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 12.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if let Some(n) = step {
                step_badge(ui, n);
            }
            ui.label(egui::RichText::new(title).font(display(26.0)).color(TEXT));
        });
        body(ui)
    });
    shine(ui.painter(), resp.response.rect, 18);
    resp.inner
}

/// Field label (.kb-lab-t) with an optional small note
pub fn lab(ui: &mut Ui, title: &str, small: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(egui::RichText::new(title).font(medium(13.5)).color(TEXT2));
        if !small.is_empty() {
            ui.label(
                egui::RichText::new(format!("– {small}"))
                    .font(text(12.0))
                    .color(FAINT),
            );
        }
    });
}

/// A hint (.kb-hint)
pub fn hint(ui: &mut Ui, t: &str) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(t).font(text(13.0)).color(FAINT)).wrap())
}

/// A tip box (.kb-pmvg-tip): pink line on the left, a pink tint
pub fn tip(ui: &mut Ui, t: &str) {
    let resp = egui::Frame::new()
        .fill(alpha(PINK, 0.08))
        .corner_radius(3)
        .inner_margin(Margin {
            left: 12,
            right: 10,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(t).font(text(13.0)).color(TEXT2)).wrap());
        });
    let r = resp.response.rect;
    ui.painter().rect_filled(
        Rect::from_min_size(r.min, Vec2::new(3.0, r.height())),
        0.0,
        PINK,
    );
}

/// A group's head (.kb-pmvg-grouphead): hatch mark, title, note
pub fn group_head(ui: &mut Ui, title: &str, small: &str) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(12.0, 8.0), Sense::hover());
        hatch(ui.painter(), r, PINK);
        ui.label(egui::RichText::new(title).font(display(19.0)).color(TEXT));
        if !small.is_empty() {
            ui.label(egui::RichText::new(small).font(text(12.5)).color(FAINT));
        }
    });
}

/// Segmented pills (.kb-seg). Returns true when changed.
pub fn seg<T: PartialEq + Clone>(ui: &mut Ui, value: &mut T, opts: &[(T, &str, &str)]) -> bool {
    let mut changed = false;
    let g = glass();
    let frame = egui::Frame::new()
        .fill(if g { alpha(BG3, 0.38) } else { BASE })
        .stroke(Stroke::new(1.0_f32, if g { glass_edge() } else { BG3 }))
        .corner_radius(if g { 255 } else { 4 })
        .inner_margin(Margin::same(2));
    frame.show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
            for (v, label, tooltip) in opts {
                let on = *value == *v;
                let galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    medium(13.0),
                    if on { INK } else { TEXT2 },
                );
                let size = Vec2::new(galley.size().x + 24.0, 30.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let hovered = resp.hovered();
                let p = ui.painter();
                let cr = if g { 255 } else { 2 };
                if on {
                    p.rect_filled(rect, cr, PINK);
                } else if hovered {
                    p.rect_filled(rect, cr, BG2);
                }
                let color = if on {
                    INK
                } else if hovered {
                    TEXT
                } else {
                    TEXT2
                };
                p.galley(rect.center() - galley.size() / 2.0, galley, color);
                let resp = if tooltip.is_empty() {
                    resp
                } else {
                    resp.on_hover_text(*tooltip)
                };
                if resp.clicked() && !on {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    });
    changed
}

/// The switch (.kb-switch): 38×20, hatched pink when on
pub fn switch_paint(painter: &egui::Painter, rect: Rect, on: bool, hovered: bool) {
    if on {
        painter.rect_filled(rect, 4.0, PINK_DEEP);
        hatch(painter, rect.shrink(1.0), PINK);
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, PINK), StrokeKind::Inside);
    } else {
        painter.rect_filled(rect, 4.0, BG3);
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, if hovered { FAINT } else { BG3 }),
            StrokeKind::Inside,
        );
    }
    let knob = 14.0;
    let x = if on {
        rect.right() - 3.0 - knob
    } else {
        rect.left() + 3.0
    };
    let k = Rect::from_min_size(Pos2::new(x, rect.top() + 3.0), Vec2::splat(knob));
    painter.rect_filled(k, 2.0, if on { PAPER } else { FAINT });
}

pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(38.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    switch_paint(ui.painter(), rect, *on, resp.hovered());
    resp
}

/// An option tile with a switch (.kb-pmvg-opt) – the whole tile toggles. Returns true when changed.
pub fn opt(ui: &mut Ui, on: &mut bool, title: &str, desc: &str) -> bool {
    let width = ui.available_width();
    let g = glass();
    let pad = Vec2::new(12.0, 8.0);
    let text_w = (width - pad.x * 2.0 - 38.0 - 12.0).max(60.0);
    let t_gal = ui
        .painter()
        .layout(title.to_string(), bold(14.0), TEXT, text_w);
    let d_gal = (!desc.is_empty()).then(|| {
        ui.painter()
            .layout(desc.to_string(), text(12.5), FAINT, text_w)
    });
    let h =
        (t_gal.size().y + d_gal.as_ref().map(|g| g.size().y + 2.0).unwrap_or(0.0) + pad.y * 2.0)
            .max(52.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    let cr = if g { 12 } else { 5 };
    p.rect_filled(rect, cr, if g { alpha(BASE, 0.5) } else { BASE });
    if *on {
        // linear-gradient(90deg, rgba(pink, .09 / .16 glass), transparent 70%)
        let mut mesh = egui::Mesh::default();
        let a = alpha(PINK, if g { 0.16 } else { 0.09 });
        let r2 = Rect::from_min_max(
            rect.min,
            Pos2::new(rect.left() + rect.width() * 0.7, rect.bottom()),
        );
        mesh.colored_vertex(r2.left_top(), a);
        mesh.colored_vertex(r2.right_top(), Color32::TRANSPARENT);
        mesh.colored_vertex(r2.right_bottom(), Color32::TRANSPARENT);
        mesh.colored_vertex(r2.left_bottom(), a);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.with_clip_rect(rect.shrink(1.0)).add(Shape::mesh(mesh));
    }
    let border = if *on {
        if g {
            alpha(PINK, 0.6)
        } else {
            PINK_DEEP
        }
    } else if hovered {
        FAINT
    } else if g {
        glass_edge()
    } else {
        BG3
    };
    p.rect_stroke(rect, cr, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    let content_h = t_gal.size().y + d_gal.as_ref().map(|g| g.size().y + 2.0).unwrap_or(0.0);
    let mut y = rect.center().y - content_h / 2.0;
    let tx = rect.left() + pad.x;
    let th = t_gal.size().y;
    p.galley(Pos2::new(tx, y), t_gal, TEXT);
    y += th + 2.0;
    if let Some(dg) = d_gal {
        p.galley(Pos2::new(tx, y), dg, FAINT);
    }
    let sw = Rect::from_min_size(
        Pos2::new(rect.right() - pad.x - 38.0, rect.center().y - 10.0),
        Vec2::new(38.0, 20.0),
    );
    switch_paint(p, sw, *on, hovered);
    if resp.clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// Option tiles in a responsive grid (repeat(auto-fill, minmax(270px, 1fr)), 6 px gaps)
pub fn opts_grid(ui: &mut Ui, n: usize, mut each: impl FnMut(&mut Ui, usize)) {
    if n == 0 {
        return;
    }
    let w = ui.available_width();
    let cols = ((w + 6.0) / (270.0 + 6.0)).floor().clamp(1.0, n as f32) as usize;
    let cell = (w - 6.0 * (cols as f32 - 1.0)) / cols as f32;
    let rows = n.div_ceil(cols);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
        for r in 0..rows {
            ui.horizontal_top(|ui| {
                for c in 0..cols {
                    let i = r * cols + c;
                    if i >= n {
                        break;
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(cell, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(cell);
                            each(ui, i)
                        },
                    );
                }
            });
        }
    });
}

/// A slider row (.kb-pmvg-range): icon + label | slider | value. Returns true when changed.
#[allow(clippy::too_many_arguments)]
pub fn range(
    ui: &mut Ui,
    ic: Icon,
    label: &str,
    value: &mut f64,
    min: f64,
    max: f64,
    step: f64,
    fmt: &dyn Fn(f64) -> String,
) -> bool {
    let width = ui.available_width();
    let g = glass();
    let label_w = (width * 0.3).clamp(90.0, 170.0);
    let val_w = 64.0;
    let label_gal = ui
        .painter()
        .layout(label.to_string(), bold(14.0), TEXT, label_w - 26.0);
    let h = (label_gal.size().y + 16.0).max(52.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
    let p = ui.painter().clone();
    let cr = if g { 12 } else { 5 };
    p.rect_filled(rect, cr, if g { alpha(BASE, 0.5) } else { BASE });
    p.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0_f32, if g { glass_edge() } else { BG3 }),
        StrokeKind::Inside,
    );
    let ir = Rect::from_center_size(
        Pos2::new(rect.left() + 14.0 + 8.5, rect.center().y),
        Vec2::splat(17.0),
    );
    icon(&p, ir, ic, PINK);
    p.galley(
        Pos2::new(ir.right() + 8.0, rect.center().y - label_gal.size().y / 2.0),
        label_gal,
        TEXT,
    );
    // the track
    let tr = Rect::from_min_max(
        Pos2::new(rect.left() + 14.0 + label_w + 8.0, rect.center().y - 10.0),
        Pos2::new(rect.right() - 14.0 - val_w - 8.0, rect.center().y + 10.0),
    );
    let id = ui.id().with(("range", label));
    let resp = ui.interact(
        tr.expand2(Vec2::new(10.0, 6.0)),
        id,
        Sense::click_and_drag(),
    );
    let mut changed = false;
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on() {
            let f = ((pos.x - tr.left()) / tr.width()).clamp(0.0, 1.0) as f64;
            let mut v = min + f * (max - min);
            if step > 0.0 {
                v = (v / step).round() * step;
            }
            v = v.clamp(min, max);
            if (v - *value).abs() > 1e-9 {
                *value = v;
                changed = true;
            }
        }
    }
    // keyboard: arrows when hovered
    if resp.hovered() {
        let (l, r) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        let st = if step > 0.0 {
            step
        } else {
            (max - min) / 100.0
        };
        if l || r {
            let v = (*value + if r { st } else { -st }).clamp(min, max);
            if v != *value {
                *value = v;
                changed = true;
            }
        }
    }
    let f = if max > min {
        ((*value - min) / (max - min)).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let line = Rect::from_center_size(tr.center(), Vec2::new(tr.width(), 4.0));
    p.rect_filled(line, 2.0, BG3);
    p.rect_filled(
        Rect::from_min_size(line.min, Vec2::new(line.width() * f, line.height())),
        2.0,
        PINK,
    );
    let knob = Pos2::new(line.left() + line.width() * f, line.center().y);
    p.circle_filled(knob, 9.0, PAPER);
    p.circle_stroke(knob, 7.5, Stroke::new(3.0_f32, PINK));
    if resp.hovered() || resp.dragged() {
        p.circle_stroke(knob, 12.0, Stroke::new(1.5_f32, alpha(SHINE, 0.6)));
    }
    p.text(
        Pos2::new(rect.right() - 14.0, rect.center().y),
        Align2::RIGHT_CENTER,
        fmt(*value),
        medium(14.0),
        PINK,
    );
    changed
}

/// Two slider rows side by side when there is room (.kb-pmvg-sound)
pub fn pair(ui: &mut Ui, a: impl FnOnce(&mut Ui), b: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    if w < 560.0 {
        a(ui);
        b(ui);
    } else {
        ui.columns(2, |c| {
            a(&mut c[0]);
            b(&mut c[1]);
        });
    }
}

/// A style section (.kb-pmvg-sec): header bar with title, summary, subtitle and chevron; body when open
pub fn section(
    ui: &mut Ui,
    title: &str,
    summary: &str,
    sub: &str,
    open: &mut bool,
    body: impl FnOnce(&mut Ui),
) {
    let g = glass();
    let frame = egui::Frame::new()
        .fill(if g { alpha(BASE, 0.45) } else { BASE })
        .stroke(Stroke::new(1.0_f32, if g { glass_edge() } else { BG3 }))
        .corner_radius(if g { 14 } else { 6 });
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 58.0), Sense::click());
        let p = ui.painter();
        let top_r = if g {
            CornerRadius {
                nw: 14,
                ne: 14,
                sw: if *open { 0 } else { 14 },
                se: if *open { 0 } else { 14 },
            }
        } else {
            CornerRadius::same(0)
        };
        let bg = if resp.hovered() {
            SECHEAD_HOVER
        } else if g {
            alpha(BG3, 0.45)
        } else {
            BG3
        };
        p.rect_filled(rect, top_r, bg);
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())),
            0.0,
            if *open { PINK } else { FAINT },
        );
        p.text(
            Pos2::new(rect.left() + 16.0, rect.top() + 12.0),
            Align2::LEFT_TOP,
            title,
            display(19.0),
            TEXT,
        );
        p.text(
            Pos2::new(rect.left() + 16.0, rect.top() + 36.0),
            Align2::LEFT_TOP,
            summary,
            medium(12.5),
            PINK2,
        );
        let chev = Rect::from_center_size(
            Pos2::new(rect.right() - 22.0, rect.center().y),
            Vec2::splat(16.0),
        );
        if w > 560.0 {
            p.text(
                Pos2::new(chev.left() - 12.0, rect.center().y),
                Align2::RIGHT_CENTER,
                sub,
                text(13.0),
                TEXT2,
            );
        }
        // the chevron points up when open
        let cp = egui::Painter::clone(p);
        if *open {
            let flip = Rect::from_center_size(chev.center(), chev.size());
            let s = flip.width() / 24.0;
            let o = flip.center() - Vec2::splat(12.0 * s);
            let pts = vec![
                Pos2::new(o.x + 6.0 * s, o.y + 15.0 * s),
                Pos2::new(o.x + 12.0 * s, o.y + 9.0 * s),
                Pos2::new(o.x + 18.0 * s, o.y + 15.0 * s),
            ];
            cp.add(Shape::line(pts, Stroke::new(2.2 * s, PINK)));
        } else {
            icon(&cp, chev, Icon::Chevron, FAINT);
        }
        if resp.clicked() {
            *open = !*open;
        }
        if *open {
            egui::Frame::new()
                .inner_margin(Margin {
                    left: 16,
                    right: 16,
                    top: 14,
                    bottom: 16,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 10.0;
                    body(ui)
                });
        }
    });
}

/// Button kinds (.kb-btn, .is-primary, .is-ghost)
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Normal,
    Primary,
    Ghost,
}

/// A button with an optional icon. disabled: drawn faint and not clickable.
pub fn btn(ui: &mut Ui, kind: Btn, ic: Option<Icon>, label: &str, enabled: bool) -> Response {
    let g = glass();
    let font = if kind == Btn::Primary {
        bold(15.0)
    } else {
        medium(14.0)
    };
    let fg = match kind {
        Btn::Primary => INK,
        Btn::Ghost => TEXT2,
        Btn::Normal => TEXT,
    };
    let gal = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let icon_w = if ic.is_some() { 22.0 } else { 0.0 };
    let h = if kind == Btn::Primary { 42.0 } else { 36.0 };
    let w = gal.size().x + icon_w + 28.0;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    paint_btn(ui, rect, &resp, kind, ic, gal, enabled, g);
    resp
}

/// A button as wide as the space (the Go card's start button)
pub fn wide_btn(ui: &mut Ui, kind: Btn, ic: Option<Icon>, label: &str, enabled: bool) -> Response {
    let g = glass();
    let font = if kind == Btn::Primary {
        bold(16.0)
    } else {
        medium(14.5)
    };
    let fg = match kind {
        Btn::Primary => INK,
        Btn::Ghost => TEXT2,
        Btn::Normal => TEXT,
    };
    let gal = ui.painter().layout_no_wrap(label.to_string(), font, fg);
    let w = ui.available_width() - 4.0;
    let h = if kind == Btn::Primary { 48.0 } else { 40.0 };
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), sense);
    paint_btn(ui, rect, &resp, kind, ic, gal, enabled, g);
    resp
}

#[allow(clippy::too_many_arguments)]
fn paint_btn(
    ui: &Ui,
    rect: Rect,
    resp: &Response,
    kind: Btn,
    ic: Option<Icon>,
    gal: std::sync::Arc<egui::Galley>,
    enabled: bool,
    g: bool,
) {
    let p = ui.painter();
    let hovered = enabled && resp.hovered();
    let cr: u8 = if g { 255 } else { 4 };
    let lift = if hovered && kind == Btn::Primary && !g {
        Vec2::splat(-1.0)
    } else {
        Vec2::ZERO
    };
    let r = rect.translate(lift);
    let dim = |c: Color32| if enabled { c } else { alpha(c, 0.45) };
    match kind {
        Btn::Primary => {
            if g {
                p.add(Shape::rect_filled(
                    r.translate(Vec2::new(0.0, 6.0)).expand(2.0),
                    cr,
                    alpha(PINK, if enabled { 0.22 } else { 0.08 }),
                ));
            } else {
                let off = if hovered { 4.0 } else { 3.0 };
                p.rect_filled(r.translate(Vec2::splat(off)), cr, dim(PAPER));
            }
            p.rect_filled(r, cr, dim(if hovered { PINK2 } else { PINK }));
            if g {
                p.line_segment(
                    [
                        Pos2::new(r.left() + 16.0, r.top() + 1.0),
                        Pos2::new(r.right() - 16.0, r.top() + 1.0),
                    ],
                    Stroke::new(1.0_f32, Color32::from_white_alpha(110)),
                );
            }
        }
        Btn::Ghost => {
            if hovered {
                p.rect_filled(r, cr, BG2);
            }
        }
        Btn::Normal => {
            let fill = if hovered {
                BG3
            } else if g {
                alpha(BG3, 0.38)
            } else {
                BG2
            };
            p.rect_filled(r, cr, fill);
            p.rect_stroke(
                r,
                cr,
                Stroke::new(
                    1.0_f32,
                    if hovered {
                        FAINT
                    } else if g {
                        glass_edge()
                    } else {
                        BG3
                    },
                ),
                StrokeKind::Inside,
            );
        }
    }
    let fg = match kind {
        Btn::Primary => INK,
        Btn::Ghost if hovered => TEXT,
        Btn::Ghost => TEXT2,
        Btn::Normal => TEXT,
    };
    let fg = dim(fg);
    let icon_w = if ic.is_some() { 22.0 } else { 0.0 };
    let total = gal.size().x + icon_w;
    let x0 = r.center().x - total / 2.0;
    if let Some(i) = ic {
        let ir = Rect::from_center_size(Pos2::new(x0 + 8.0, r.center().y), Vec2::splat(16.0));
        icon(p, ir, i, fg);
    }
    p.galley(
        Pos2::new(x0 + icon_w, r.center().y - gal.size().y / 2.0),
        gal,
        fg,
    );
}

/// A chip (.kb-chip): toggles; with a layout pictogram if given
pub fn chip(ui: &mut Ui, label: &str, on: bool, layout: Option<&str>) -> Response {
    let g = glass();
    let gal = ui.painter().layout_no_wrap(
        label.to_string(),
        medium(13.0),
        if on { INK } else { TEXT2 },
    );
    let lw = if layout.is_some() { 26.0 } else { 0.0 };
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(gal.size().x + 22.0 + lw, 28.0), Sense::click());
    let p = ui.painter();
    // .kb-chip: violet tint with a violet bar on the left; glass: a capsule of tinted glass; on: pink
    let cr: u8 = if g { 255 } else { 3 };
    let hovered = resp.hovered();
    let violet = |a: f32| Color32::from_rgba_unmultiplied(180, 140, 255, (a * 255.0) as u8);
    let fill = if on {
        PINK
    } else if g {
        if hovered {
            alpha(BG3, 0.7)
        } else {
            alpha(BG3, 0.38)
        }
    } else if hovered {
        violet(0.28)
    } else {
        violet(0.16)
    };
    p.rect_filled(rect, cr, fill);
    if !on {
        if g {
            p.rect_stroke(
                rect,
                cr,
                Stroke::new(1.0_f32, glass_edge()),
                StrokeKind::Inside,
            );
        } else {
            p.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                CornerRadius {
                    nw: 3,
                    sw: 3,
                    ne: 0,
                    se: 0,
                },
                VIOLET,
            );
        }
    }
    let fg = if on { INK } else { TEXT };
    let mut x = rect.left() + 11.0;
    if let Some(id) = layout {
        layout_icon(
            p,
            Rect::from_min_size(Pos2::new(x, rect.center().y - 7.5), Vec2::new(22.0, 15.0)),
            id,
            fg,
        );
        x += lw;
    }
    p.galley(Pos2::new(x, rect.center().y - gal.size().y / 2.0), gal, fg);
    resp
}

/// A text field (.kb-field)
pub fn field(ui: &mut Ui, value: &mut String, placeholder: &str, width: f32) -> Response {
    let width = width.min(ui.available_width()).max(60.0);
    let g = glass();
    let frame = egui::Frame::new()
        .fill(if g { alpha(BASE, 0.45) } else { BASE })
        .stroke(Stroke::new(1.0_f32, if g { glass_edge() } else { BG3 }))
        .corner_radius(if g { 10 } else { 4 })
        .inner_margin(Margin::symmetric(10, 7));
    frame
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text(egui::RichText::new(placeholder).color(FAINT))
                    .font(text(14.0))
                    .text_color(TEXT)
                    .frame(false)
                    .desired_width(width - 22.0),
            )
        })
        .inner
}

/// A drop zone (.kb-pmvg-drop): dashed border, halftone, icon, title and note. `over`: files are dragged over
pub fn drop_zone(
    ui: &mut Ui,
    ic: Icon,
    title: &str,
    small: &str,
    over: bool,
    busy: bool,
) -> Response {
    let w = ui.available_width();
    let small_gal = ui
        .painter()
        .layout(small.to_string(), text(13.0), FAINT, w - 40.0);
    let h = (150.0f32).max(90.0 + small_gal.size().y);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter();
    let g = glass();
    let cr = if g { 14.0 } else { 6.0 };
    p.rect_filled(rect, cr, if g { alpha(BASE, 0.4) } else { BASE });
    // halftone (cheap: a few hundred dots)
    {
        let pc = p.with_clip_rect(rect.shrink(2.0));
        let mut y = rect.top() + 3.5;
        let col = alpha(PINK, 0.22);
        while y < rect.bottom() {
            let mut x = rect.left() + 3.5;
            while x < rect.right() {
                pc.circle_filled(Pos2::new(x, y), 1.1, col);
                x += 7.0;
            }
            y += 7.0;
        }
    }
    let hot = over || resp.hovered() || busy;
    let border = if hot {
        PINK
    } else if g {
        glass_edge()
    } else {
        BG3
    };
    if busy {
        p.rect_stroke(rect, cr, Stroke::new(2.0_f32, border), StrokeKind::Inside);
    } else {
        let r = rect.shrink(1.0);
        let pts = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        for w2 in pts.windows(2) {
            p.extend(Shape::dashed_line(
                &[w2[0], w2[1]],
                Stroke::new(2.0_f32, border),
                6.0,
                5.0,
            ));
        }
    }
    let t = ui.input(|i| i.time) as f32;
    let pulse = if busy {
        0.75 + 0.25 * (t * 6.0).sin()
    } else {
        1.0
    };
    let cy = rect.center().y - (small_gal.size().y + 30.0) / 2.0;
    icon(
        p,
        Rect::from_center_size(Pos2::new(rect.center().x, cy - 6.0), Vec2::splat(34.0)),
        ic,
        alpha(PINK, pulse),
    );
    p.text(
        Pos2::new(rect.center().x, cy + 26.0),
        Align2::CENTER_CENTER,
        title,
        display(22.0),
        TEXT,
    );
    p.galley(
        Pos2::new(rect.center().x - small_gal.size().x / 2.0, cy + 42.0),
        small_gal,
        FAINT,
    );
    if busy {
        ui.ctx().request_repaint();
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small "remove" button (an ×, drawn – fonts don't all have the glyph)
pub fn x_btn(ui: &mut Ui, tip: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, BG3);
    }
    icon(
        ui.painter(),
        r.shrink(4.0),
        Icon::Close,
        if resp.hovered() { TEXT } else { FAINT },
    );
    resp.on_hover_text(tip)
}

/// A select (ComboBox) in the field style
pub fn select<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    opts: &[(T, String)],
    width: f32,
) -> bool {
    let mut changed = false;
    let cur = opts
        .iter()
        .find(|o| o.0 == *value)
        .map(|o| o.1.clone())
        .unwrap_or_default();
    egui::ComboBox::from_id_salt(id)
        .selected_text(egui::RichText::new(cur).font(text(14.0)).color(TEXT))
        .width(width)
        .show_ui(ui, |ui| {
            for (v, l) in opts {
                if ui
                    .selectable_label(*value == *v, egui::RichText::new(l).font(text(14.0)))
                    .clicked()
                    && *value != *v
                {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    changed
}

/// A progress bar (.kb-job-bar)
pub fn progress_bar(ui: &mut Ui, f: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, BG3);
    p.rect_filled(
        Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width() * f.clamp(0.0, 1.0), rect.height()),
        ),
        3.0,
        PINK,
    );
}

/// The summary list of the Go card (.kb-pmvg-sum): small caps label, value
pub fn sum_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add_sized(
            Vec2::new(78.0, 18.0),
            egui::Label::new(
                egui::RichText::new(label.to_uppercase())
                    .font(medium(11.5))
                    .color(FAINT),
            ),
        );
        ui.add(egui::Label::new(egui::RichText::new(value).font(text(13.5)).color(TEXT2)).wrap());
    });
}

/// Does a path look like one of ours (for drops)?
pub fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}
