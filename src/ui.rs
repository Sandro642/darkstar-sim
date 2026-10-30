//! Petits utilitaires de dessin pour l'avionique.

use macroquad::prelude::*;

pub const AMBER: Color = Color::new(1.0, 0.72, 0.0, 1.0);
pub const GREEN_S: Color = Color::new(0.2, 1.0, 0.35, 1.0);
pub const CYAN_S: Color = Color::new(0.2, 0.9, 1.0, 1.0);
pub const MAGENTA_S: Color = Color::new(1.0, 0.35, 1.0, 1.0);
pub const DIM: Color = Color::new(0.6, 0.65, 0.7, 1.0);
pub const PANEL_BG: Color = Color::new(0.058, 0.07, 0.088, 1.0);

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

pub const BG: Color = Color::new(0.035, 0.043, 0.055, 1.0);
pub const PANEL_BORDER: Color = Color::new(0.16, 0.19, 0.23, 1.0);
pub const ACCENT: Color = Color::new(0.30, 0.78, 1.0, 1.0);

/// Rectangle à coins arrondis (plein).
pub fn rrect(r: Rect, rad: f32, c: Color) {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0);
    draw_rectangle(r.x + rad, r.y, r.w - 2.0 * rad, r.h, c);
    draw_rectangle(r.x, r.y + rad, rad, r.h - 2.0 * rad, c);
    draw_rectangle(r.x + r.w - rad, r.y + rad, rad, r.h - 2.0 * rad, c);
    for (x, y) in [(r.x + rad, r.y + rad), (r.x + r.w - rad, r.y + rad), (r.x + rad, r.y + r.h - rad), (r.x + r.w - rad, r.y + r.h - rad)] {
        draw_circle(x, y, rad, c);
    }
}

/// Contour à coins arrondis.
pub fn rrect_lines(r: Rect, rad: f32, th: f32, c: Color) {
    let rad = rad.min(r.w / 2.0).min(r.h / 2.0);
    let pi = std::f32::consts::PI;
    draw_line(r.x + rad, r.y, r.x + r.w - rad, r.y, th, c);
    draw_line(r.x + rad, r.y + r.h, r.x + r.w - rad, r.y + r.h, th, c);
    draw_line(r.x, r.y + rad, r.x, r.y + r.h - rad, th, c);
    draw_line(r.x + r.w, r.y + rad, r.x + r.w, r.y + r.h - rad, th, c);
    arc(r.x + rad, r.y + rad, rad, pi, 1.5 * pi, th, c);
    arc(r.x + r.w - rad, r.y + rad, rad, 1.5 * pi, 2.0 * pi, th, c);
    arc(r.x + r.w - rad, r.y + r.h - rad, rad, 0.0, 0.5 * pi, th, c);
    arc(r.x + rad, r.y + r.h - rad, rad, 0.5 * pi, pi, th, c);
}

/// Panneau moderne : fond sombre arrondi, titre en petites capitales avec liseré d'accent.
pub fn panel(r: Rect, title: &str) {
    rrect(r, 8.0, PANEL_BG);
    rrect_lines(r, 8.0, 1.0, PANEL_BORDER);
    if !title.is_empty() {
        draw_rectangle(r.x + 10.0, r.y + 8.0, 3.0, 13.0, ACCENT);
        txt(title, r.x + 18.0, r.y + 19.0, 15.0, Color::new(0.78, 0.84, 0.9, 1.0));
    }
}

/// Bouton : vrai si cliqué cette image.
pub fn clicked(r: Rect) -> bool {
    is_mouse_button_pressed(MouseButton::Left) && r.contains(mouse_position().into())
}

pub fn hovered(r: Rect) -> bool {
    r.contains(mouse_position().into())
}

/// Jauge en arc façon écran Darkstar (vert, extrémité rouge, aiguille blanche).
pub fn arc_gauge(cx: f32, cy: f32, r: f32, frac: f32, red_from: f32) {
    let a0 = std::f32::consts::PI * 0.80;
    let span = std::f32::consts::PI * 1.40;
    arc(cx, cy, r, a0, a0 + span * red_from, 3.0, GREEN_S);
    arc(cx, cy, r, a0 + span * red_from, a0 + span, 3.0, RED);
    let a = a0 + span * frac.clamp(0.0, 1.0);
    draw_line(cx, cy, cx + (r - 2.0) * a.cos(), cy + (r - 2.0) * a.sin(), 2.0, WHITE);
    draw_circle(cx, cy, 2.5, WHITE);
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
