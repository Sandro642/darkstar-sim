//! Petits utilitaires de dessin pour l'avionique.

use macroquad::prelude::*;

pub const AMBER: Color = Color::new(1.0, 0.72, 0.0, 1.0);
pub const GREEN_S: Color = Color::new(0.2, 1.0, 0.35, 1.0);
pub const CYAN_S: Color = Color::new(0.2, 0.9, 1.0, 1.0);
pub const MAGENTA_S: Color = Color::new(1.0, 0.35, 1.0, 1.0);
pub const DIM: Color = Color::new(0.6, 0.65, 0.7, 1.0);
pub const PANEL_BG: Color = Color::new(0.035, 0.045, 0.06, 1.0);

/// Taille unique de rastérisation : macroquad re-téléverse tout l'atlas de glyphes à chaque
/// nouvelle combinaison caractère/taille, ce qui sature la mémoire GPU (GL sur Metal).
/// Avec une seule taille, l'atlas est rempli une fois au démarrage puis ne change plus.
const RASTER: u16 = 18;

pub fn load_font() {
    let ascii: String = (32u8..127).map(|c| c as char).collect();
    draw_text_ex(&ascii, -5000.0, -5000.0, TextParams { font_size: RASTER, ..Default::default() });
}

/// Convertit en ASCII (style afficheur avionique) : la police intégrée ne couvre que l'ASCII.
fn ascii(s: &str) -> std::borrow::Cow<'_, str> {
    if s.is_ascii() {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.chars()
            .filter_map(|c| match c {
                'é' | 'è' | 'ê' | 'ë' => Some('e'),
                'É' | 'È' | 'Ê' => Some('E'),
                'à' | 'â' => Some('a'),
                'À' | 'Â' => Some('A'),
                'ç' => Some('c'),
                'Ç' => Some('C'),
                'î' | 'ï' => Some('i'),
                'Î' | 'Ï' => Some('I'),
                'ô' => Some('o'),
                'Ô' => Some('O'),
                'ù' | 'û' => Some('u'),
                'Û' | 'Ù' => Some('U'),
                '—' | '–' => Some('-'),
                '°' => None,
                c if c.is_ascii() => Some(c),
                _ => Some('?'),
            })
            .collect(),
    )
}

pub fn txt(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_text_ex(&ascii(s), x, y, TextParams { font_size: RASTER, font_scale: size / RASTER as f32, color: c, ..Default::default() });
}

pub fn txt_w(s: &str, size: f32) -> f32 {
    measure_text(&ascii(s), None, RASTER, size / RASTER as f32).width
}

pub fn txt_c(s: &str, x: f32, y: f32, size: f32, c: Color) {
    txt(s, x - txt_w(s, size) / 2.0, y, size, c);
}

pub fn txt_r(s: &str, x: f32, y: f32, size: f32, c: Color) {
    txt(s, x - txt_w(s, size), y, size, c);
}

/// Découpe (scissor) en coordonnées logiques.
pub fn scissor(r: Option<Rect>) {
    let dpi = screen_dpi_scale();
    let mut gl = unsafe { get_internal_gl() };
    gl.flush();
    gl.quad_gl.scissor(r.map(|r| ((r.x * dpi) as i32, (r.y * dpi) as i32, (r.w * dpi) as i32, (r.h * dpi) as i32)));
}

pub fn panel(r: Rect, title: &str) {
    draw_rectangle(r.x, r.y, r.w, r.h, PANEL_BG);
    draw_rectangle_lines(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0, 1.0, Color::new(0.25, 0.3, 0.36, 1.0));
    if !title.is_empty() {
        draw_rectangle(r.x + 1.0, r.y + 1.0, r.w - 2.0, 20.0, Color::new(0.09, 0.11, 0.14, 1.0));
        txt(title, r.x + 8.0, r.y + 16.0, 16.0, DIM);
    }
}

pub fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, th: f32, c: Color) {
    let n = ((a1 - a0).abs() / 0.08).ceil().max(2.0) as i32;
    let mut prev = vec2(cx + r * a0.cos(), cy + r * a0.sin());
    for i in 1..=n {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        let p = vec2(cx + r * a.cos(), cy + r * a.sin());
        draw_line(prev.x, prev.y, p.x, p.y, th, c);
        prev = p;
    }
}

/// Jauge circulaire style EICAS (210° d'ouverture).
pub fn dial(cx: f32, cy: f32, r: f32, v: f32, vmin: f32, vmax: f32, caution: f32, warn: f32, label: &str, value: &str) {
    let a0 = std::f32::consts::PI * 0.75;
    let span = std::f32::consts::PI * 1.25;
    let frac = |x: f32| ((x - vmin) / (vmax - vmin)).clamp(0.0, 1.0);
    arc(cx, cy, r, a0, a0 + span * frac(caution), 2.0, WHITE);
    arc(cx, cy, r, a0 + span * frac(caution), a0 + span * frac(warn), 2.5, AMBER);
    arc(cx, cy, r, a0 + span * frac(warn), a0 + span, 2.5, RED);
    let a = a0 + span * frac(v);
    let col = if v >= warn { RED } else if v >= caution { AMBER } else { WHITE };
    // secteur rempli
    let n = 24;
    for i in 0..n {
        let t0 = a0 + (a - a0) * i as f32 / n as f32;
        let t1 = a0 + (a - a0) * (i + 1) as f32 / n as f32;
        draw_triangle(
            vec2(cx, cy),
            vec2(cx + r * 0.95 * t0.cos(), cy + r * 0.95 * t0.sin()),
            vec2(cx + r * 0.95 * t1.cos(), cy + r * 0.95 * t1.sin()),
            Color::new(col.r * 0.35, col.g * 0.35, col.b * 0.35, 0.6),
        );
    }
    draw_line(cx, cy, cx + r * a.cos(), cy + r * a.sin(), 2.5, col);
    draw_rectangle_lines(cx + 2.0, cy - 14.0, r + 6.0, 18.0, 1.0, DIM);
    txt_r(value, cx + r + 6.0, cy, 16.0, col);
    txt_c(label, cx, cy + r * 0.75 + 4.0, 14.0, DIM);
}

/// Barre verticale de progression.
pub fn vbar(x: f32, y: f32, w: f32, h: f32, frac: f32, c: Color) {
    draw_rectangle_lines(x, y, w, h, 1.0, DIM);
    let f = frac.clamp(0.0, 1.0);
    draw_rectangle(x + 1.0, y + h - h * f, w - 2.0, h * f, c);
}

pub fn wrap_text(s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        let trial = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if txt_w(&trial, size) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = trial;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}
