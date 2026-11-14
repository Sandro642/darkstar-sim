//! Vue cockpit pilote (prototype) : vue fixe du pilote, monde réel à travers la verrière, structure
//! du cockpit en relief (montants de verrière, auvent, planche de bord, consoles latérales en
//! perspective, manche) et panneaux réels encastrés dans des lunettes : radio sur l'auvent, écran à
//! pages au centre, moteurs à gauche, élec à droite, manette / train sur la console gauche,
//! moteurs / APU / clim sur la console droite, scramjet sur le pupitre central.

use macroquad::models::{Mesh, Vertex, draw_mesh};
use macroquad::prelude::*;

/// Emplacements des éléments à l'écran.
pub struct Layout {
    /// Vue extérieure (au-dessus de l'auvent)
    pub outside: Rect,
    pub radio: Rect,
    pub screen: Rect,
    pub engines: Rect,
    pub elec: Rect,
    pub throttle: Rect,
    pub eng_apu: Rect,
    pub scram: Rect,
}

pub fn layout(sw: f32, sh: f32, top: f32) -> Layout {
    let h = sh - top;
    let glare_y = top + h * 0.47;
    let panel_y = glare_y + h * 0.035;
    let sw_ = sw * 0.40;
    let screen = Rect::new((sw - sw_) / 2.0, panel_y + 8.0, sw_, h * 0.36);
    let side_w = sw * 0.17;
    let engines = Rect::new(screen.x - side_w - 24.0, panel_y + 14.0, side_w, h * 0.21);
    let elec = Rect::new(screen.x + screen.w + 24.0, panel_y + 14.0, side_w, h * 0.21);
    let rw = sw * 0.34;
    let radio = Rect::new((sw - rw) / 2.0, glare_y - h * 0.135, rw, h * 0.115);
    // Consoles latérales (sous les panneaux latéraux)
    let cons_y = engines.y + engines.h + 26.0;
    let cons_h = sh - 14.0 - cons_y;
    let throttle = Rect::new(sw * 0.02, cons_y, sw * 0.20, cons_h);
    let eng_apu = Rect::new(sw * 0.78, cons_y, sw * 0.20, cons_h);
    // Pupitre central sous l'écran
    let sy = screen.y + screen.h + 18.0;
    let scram = Rect::new(sw / 2.0 - sw * 0.055, sy, sw * 0.11, sh - 12.0 - sy);
    Layout { outside: Rect::new(0.0, top, sw, glare_y - top + 6.0), radio, screen, engines, elec, throttle, eng_apu, scram }
}

/// Quadrilatère à couleurs par sommet (dégradés).
fn quad(p: [Vec2; 4], c: [Color; 4]) {
    let v: Vec<Vertex> = p.iter().zip(c.iter()).map(|(p, c)| Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, *c)).collect();
    draw_mesh(&Mesh { vertices: v, indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

fn grey(v: f32) -> Color {
    Color::new(v, v * 1.02, v * 1.06, 1.0)
}

/// Lunette d'un panneau encastré : cadre en relief, vis, ombre intérieure.
pub fn bezel(r: Rect) {
    let m = 10.0;
    // Ombre portée et cadre
    draw_rectangle(r.x - m + 4.0, r.y - m + 5.0, r.w + 2.0 * m, r.h + 2.0 * m, Color::new(0.0, 0.0, 0.0, 0.35));
    quad(
        [vec2(r.x - m, r.y - m), vec2(r.x + r.w + m, r.y - m), vec2(r.x + r.w + m, r.y + r.h + m), vec2(r.x - m, r.y + r.h + m)],
        [grey(0.30), grey(0.26), grey(0.14), grey(0.17)],
    );
    // Biseau intérieur
    draw_rectangle_lines(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0, 2.0, Color::new(0.04, 0.045, 0.05, 1.0));
    draw_line(r.x - m + 1.0, r.y - m + 1.0, r.x + r.w + m - 1.0, r.y - m + 1.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.18));
    for (x, y) in [(r.x - m * 0.55, r.y - m * 0.55), (r.x + r.w + m * 0.55, r.y - m * 0.55), (r.x - m * 0.55, r.y + r.h + m * 0.55), (r.x + r.w + m * 0.55, r.y + r.h + m * 0.55)] {
        draw_circle(x, y, 3.2, grey(0.42));
        draw_line(x - 2.2, y - 1.0, x + 2.2, y + 1.0, 1.2, grey(0.12));
    }
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 1.0));
}

/// Structure derrière les panneaux : auvent, planche de bord, consoles latérales, pupitre.
pub fn structure(l: &Layout, sw: f32, sh: f32, top: f32) {
    let h = sh - top;
    let glare_y = l.outside.y + l.outside.h - 6.0;
    // Planche de bord (gris anthracite, dégradé vers le bas)
    quad([vec2(0.0, glare_y), vec2(sw, glare_y), vec2(sw, sh), vec2(0.0, sh)], [grey(0.20), grey(0.20), grey(0.07), grey(0.07)]);
    // Consoles latérales en perspective (fuyant vers le centre bas)
    for s in [-1.0f32, 1.0] {
        let x_out = if s < 0.0 { 0.0 } else { sw };
        let x_in = sw / 2.0 + s * sw * 0.30;
        let y0 = top + h * 0.70;
        let pts = [vec2(x_out, y0 - h * 0.05), vec2(x_in, y0 + h * 0.03), vec2(x_in + s * sw * 0.02, sh), vec2(x_out, sh)];
        quad(pts, [grey(0.16), grey(0.13), grey(0.05), grey(0.06)]);
        // Arête éclairée
        draw_line(pts[0].x, pts[0].y, pts[1].x, pts[1].y, 2.0, Color::new(1.0, 1.0, 1.0, 0.12));
    }
    // Pupitre central (entre les jambes)
    let pc = l.scram;
    quad(
        [vec2(pc.x - 30.0, pc.y - 6.0), vec2(pc.x + pc.w + 30.0, pc.y - 6.0), vec2(pc.x + pc.w + 60.0, sh), vec2(pc.x - 60.0, sh)],
        [grey(0.17), grey(0.17), grey(0.06), grey(0.06)],
    );
    // Auvent (glareshield) : bourrelet sombre avec reflet sur l'arête
    let gy = glare_y;
    let bulge = h * 0.05;
    let n = 40;
    let pt = |t: f32| vec2(t * sw, gy - bulge * (1.0 - (2.0 * t - 1.0).powi(2)));
    for i in 0..n {
        let (a, b) = (pt(i as f32 / n as f32), pt((i + 1) as f32 / n as f32));
        quad([a, b, vec2(b.x, gy + h * 0.035), vec2(a.x, gy + h * 0.035)], [grey(0.10), grey(0.10), grey(0.03), grey(0.03)]);
        draw_line(a.x, a.y, b.x, b.y, 2.5, Color::new(1.0, 1.0, 1.0, 0.16));
    }
}

/// Verrière par-dessus la vue extérieure : montants (arceau central, montants latéraux), joint,
/// léger reflet.
pub fn canopy(l: &Layout, sw: f32) {
    let o = l.outside;
    let frame = Color::new(0.06, 0.065, 0.07, 1.0);
    let edge = Color::new(0.35, 0.37, 0.40, 1.0);
    // Montants latéraux en perspective
    for s in [-1.0f32, 1.0] {
        let x_bot = sw / 2.0 + s * sw * 0.47;
        let x_top = sw / 2.0 + s * sw * 0.30;
        let w_bot = sw * 0.035;
        let w_top = sw * 0.018;
        let p = [vec2(x_top - w_top, o.y), vec2(x_top + w_top, o.y), vec2(x_bot + w_bot, o.y + o.h), vec2(x_bot - w_bot, o.y + o.h)];
        quad(p, [frame, frame, Color::new(0.1, 0.105, 0.11, 1.0), Color::new(0.1, 0.105, 0.11, 1.0)]);
        draw_line(p[0].x, p[0].y, p[3].x, p[3].y, 1.5, edge);
        draw_line(p[1].x, p[1].y, p[2].x, p[2].y, 1.5, edge);
    }
    // Arceau du haut (bord de la verrière)
    quad([vec2(0.0, o.y), vec2(sw, o.y), vec2(sw, o.y + 16.0), vec2(0.0, o.y + 16.0)], [frame, frame, Color::new(0.06, 0.065, 0.07, 0.0), Color::new(0.06, 0.065, 0.07, 0.0)]);
    // Coins sombres (vignettage du poste)
    for s in [-1.0f32, 1.0] {
        let x = if s < 0.0 { 0.0 } else { sw };
        quad([vec2(x, o.y), vec2(x + s * sw * 0.10, o.y), vec2(x + s * sw * 0.02, o.y + o.h), vec2(x, o.y + o.h)], [frame, Color::new(0.06, 0.065, 0.07, 0.0), Color::new(0.06, 0.065, 0.07, 0.0), frame]);
    }
    // Reflet diagonal sur le plexiglas
    quad(
        [vec2(sw * 0.58, o.y), vec2(sw * 0.66, o.y), vec2(sw * 0.50, o.y + o.h * 0.8), vec2(sw * 0.44, o.y + o.h * 0.8)],
        [Color::new(1.0, 1.0, 1.0, 0.05), Color::new(1.0, 1.0, 1.0, 0.0), Color::new(1.0, 1.0, 1.0, 0.0), Color::new(1.0, 1.0, 1.0, 0.03)],
    );
}

/// Manche (au centre bas, devant le pupitre) : poignée en relief, gâchette, chapeau.
pub fn stick(sw: f32, sh: f32, pitch: f32, roll: f32) {
    let base = vec2(sw * 0.735 + roll * 10.0, sh + 10.0);
    let top = vec2(base.x + roll * 16.0, sh - sh * 0.085 + pitch * 10.0);
    let w = sw * 0.012;
    quad([vec2(base.x - w * 1.4, base.y), vec2(base.x + w * 1.4, base.y), vec2(top.x + w, top.y), vec2(top.x - w, top.y)], [grey(0.05), grey(0.12), grey(0.22), grey(0.10)]);
    // Poignée
    let g = Rect::new(top.x - w * 2.0, top.y - sh * 0.05, w * 4.0, sh * 0.06);
    quad([vec2(g.x, g.y), vec2(g.x + g.w, g.y), vec2(g.x + g.w, g.y + g.h), vec2(g.x, g.y + g.h)], [grey(0.18), grey(0.28), grey(0.12), grey(0.06)]);
    draw_circle(g.x + g.w * 0.5, g.y + 6.0, w * 0.7, Color::new(0.5, 0.08, 0.06, 1.0));
    draw_rectangle(g.x - 3.0, g.y + g.h * 0.45, 6.0, g.h * 0.2, grey(0.25));
}
