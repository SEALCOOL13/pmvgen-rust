// Text as filled / stroked outlines (Anton, a condensed display face – the web version asks for
// Bahnschrift Condensed / Impact). Glyph outlines from ttf-parser become tiny-skia paths.

use std::sync::OnceLock;
use tiny_skia::{Path, PathBuilder};

static FONT_DATA: &[u8] = include_bytes!("../assets/Anton-Regular.ttf");
static FACE: OnceLock<Option<ttf_parser::Face<'static>>> = OnceLock::new();

fn face() -> Option<&'static ttf_parser::Face<'static>> {
    FACE.get_or_init(|| ttf_parser::Face::parse(FONT_DATA, 0).ok())
        .as_ref()
}

struct Builder<'a> {
    pb: &'a mut PathBuilder,
    x: f32,
    s: f32,
}
impl ttf_parser::OutlineBuilder for Builder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.x + x * self.s, -y * self.s);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.x + x * self.s, -y * self.s);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(
            self.x + x1 * self.s,
            -y1 * self.s,
            self.x + x * self.s,
            -y * self.s,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.x + x1 * self.s,
            -y1 * self.s,
            self.x + x2 * self.s,
            -y2 * self.s,
            self.x + x * self.s,
            -y * self.s,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// Width of a text at a font size (px)
pub fn measure(text: &str, size: f32) -> f32 {
    let Some(f) = face() else {
        return text.chars().count() as f32 * size * 0.45;
    };
    let s = size / f.units_per_em() as f32;
    text.chars()
        .map(|c| {
            f.glyph_index(c)
                .or_else(|| f.glyph_index('?'))
                .and_then(|g| f.glyph_hor_advance(g))
                .unwrap_or(0) as f32
                * s
        })
        .sum()
}

/// The outline of a text centred on (0, 0) (canvas textAlign center, textBaseline middle)
pub fn path(text: &str, size: f32) -> Option<Path> {
    let f = face()?;
    let s = size / f.units_per_em() as f32;
    let w = measure(text, size);
    let mut pb = PathBuilder::new();
    let mut x = -w / 2.0;
    for c in text.chars() {
        let Some(g) = f.glyph_index(c).or_else(|| f.glyph_index('?')) else {
            continue;
        };
        let mut b = Builder { pb: &mut pb, x, s };
        f.outline_glyph(g, &mut b);
        x += f.glyph_hor_advance(g).unwrap_or(0) as f32 * s;
    }
    let p = pb.finish()?;
    // "middle": the middle of the em box – between ascender and descender
    let mid = (f.ascender() as f32 + f.descender() as f32) / 2.0 * s;
    p.transform(tiny_skia::Transform::from_translate(0.0, mid))
}
