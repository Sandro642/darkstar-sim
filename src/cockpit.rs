//! Vue cockpit, d'après l'intérieur du Darkstar : vue extérieure par les fenêtres latérales, pyramide
//! centrale portant l'écran principal, écran moteurs, interrupteurs gardés, jauges CG et H2, levier de
//! train, boîtier radio, consoles latérales, manche. HUD vert facultatif (le Darkstar n'en a pas).

use crate::aircraft::*;
use crate::geo::*;
use crate::svs::{dir, View};
use crate::ui::*;
use macroquad::prelude::*;

pub const HUD_GREEN: Color = Color::new(0.35, 1.0, 0.5, 0.95);
const INTERIOR: Color = Color::new(0.055, 0.058, 0.062, 1.0);
const INTERIOR_LIGHT: Color = Color::new(0.12, 0.125, 0.13, 1.0);
const PANEL_DARK: Color = Color::new(0.075, 0.078, 0.082, 1.0);

/// Champ vertical de la vue extérieure (degrés) et site par défaut du regard (vers la planche).
pub const FOVY: f32 = 72.0;
pub const PITCH0: f32 = -20.0;

/// Disposition de l'intérieur (comme le cockpit du Darkstar) : pyramide centrale portant l'écran
/// principal, fenêtres latérales, appuis et consoles latérales. `off` : décalage dû au regard.
pub struct Layout {
    pub v: Rect,
    pub off: Vec2,
    pub mfd: Rect,
    pub engines: Rect,
    pub radio: Rect,
    pub dsply: Rect,
    pub switches: [Rect; 3],
    pub cg: (Vec2, f32),
    pub h2: (Vec2, f32),
    pub gear: Rect,
    pub greens: Vec2,
    pub left_console: Rect,
    pub right_console: Rect,
}

pub fn layout(v: Rect, look: (f32, f32), mfd_aspect: f32) -> Layout {
    let off = vec2(-look.0 / (FOVY * v.w / v.h) * v.w, (look.1 - PITCH0) / FOVY * v.h);
    let p = |fx: f32, fy: f32| vec2(v.x + fx * v.w, v.y + fy * v.h) + off;
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let (a, b) = (p(x0, y0), p(x1, y1));
        Rect::new(a.x, a.y, b.x - a.x, b.y - a.y)
    };
    let mw = 0.30 * v.w;
    let mh = mw / mfd_aspect;
    let mtop = p(0.5, 0.29);
    let mfd = Rect::new(mtop.x - mw / 2.0, mtop.y, mw, mh);
    let rad = (v.h * 0.065).max(30.0);
    Layout {
        v,
        off,
        mfd,
        engines: rect(0.205, 0.50, 0.345, 0.62),
        radio: rect(0.525, 0.155, 0.60, 0.26),
        dsply: rect(0.40, 0.19, 0.515, 0.26),
        switches: [rect(0.355, 0.86, 0.445, 0.95), rect(0.455, 0.86, 0.545, 0.95), rect(0.555, 0.86, 0.645, 0.95)],
        cg: (p(0.71, 0.52), rad),
        h2: (p(0.71, 0.70), rad * 0.85),
        gear: rect(0.27, 0.66, 0.31, 0.84),
        greens: p(0.33, 0.75),
        left_console: rect(0.08, 0.78, 0.23, 1.0),
        right_console: rect(0.77, 0.78, 0.92, 1.0),
    }
}

fn poly(pts: &[Vec2], c: Color) {
    fill_shape(pts, c);
}

/// Intérieur : arceaux de verrière, pyramide centrale, appuis de fenêtres, consoles, manche.
pub fn interior(l: &Layout, throttle: f32) {
    let v = l.v;
    let p = |fx: f32, fy: f32| vec2(v.x + fx * v.w, v.y + fy * v.h) + l.off;
    let edge = Color::new(0.2, 0.21, 0.22, 1.0);
    // Arceau supérieur et montants descendant vers le sommet de la pyramide
    let top = [p(-0.5, -0.5), p(1.5, -0.5), p(1.5, 0.035), p(0.64, 0.05), p(0.5, 0.11), p(0.36, 0.05), p(-0.5, 0.035)];
    poly(&top, INTERIOR);
    for (a, b) in [(p(0.30, 0.0), p(0.43, 0.14)), (p(0.70, 0.0), p(0.57, 0.14))] {
        let n = vec2(-(b.y - a.y), b.x - a.x).normalize_or_zero() * 16.0;
        poly(&[a - n, b - n, b + n, a + n], INTERIOR);
        draw_line(a.x + n.x, a.y + n.y, b.x + n.x, b.y + n.y, 1.5, edge);
        // Rivets
        for k in 1..6 {
            let q = a + (b - a) * (k as f32 / 6.0);
            draw_circle(q.x, q.y, 2.0, INTERIOR_LIGHT);
        }
    }
    // Appuis de fenêtres (bas des vitres) et flancs
    let ls = [p(-0.5, 0.34), p(0.0, 0.34), p(0.18, 0.37), p(0.32, 0.42), p(0.28, 1.6), p(-0.5, 1.6)];
    let rs = [p(1.5, 0.34), p(1.0, 0.34), p(0.82, 0.37), p(0.68, 0.42), p(0.72, 1.6), p(1.5, 1.6)];
    poly(&ls, INTERIOR);
    poly(&rs, INTERIOR);
    stroke(&ls[1..4], false, 2.0, edge);
    stroke(&rs[1..4], false, 2.0, edge);
    // Nervures sur les appuis
    for k in 0..7 {
        let t = k as f32 / 7.0;
        let (a, b) = (p(0.02 + t * 0.16, 0.38 + t * 0.03), p(0.02 + t * 0.16 + 0.03, 0.40 + t * 0.03));
        draw_line(a.x, a.y, b.x, b.y, 2.0, INTERIOR_LIGHT);
        let (a, b) = (p(0.98 - t * 0.16, 0.38 + t * 0.03), p(0.98 - t * 0.16 - 0.03, 0.40 + t * 0.03));
        draw_line(a.x, a.y, b.x, b.y, 2.0, INTERIOR_LIGHT);
    }
    // Pyramide centrale (planche) : du sommet étroit vers une base large
    let pyr = [p(0.455, 0.10), p(0.545, 0.10), p(0.63, 0.30), p(0.70, 0.45), p(0.76, 1.6), p(0.24, 1.6), p(0.30, 0.45), p(0.37, 0.30)];
    // Dégradé : plus clair au sommet (éclairage de la verrière), sombre vers le bas
    let (yt, yb) = (pyr[0].y, p(0.0, 1.0).y);
    fill_shape_colored(&pyr, |q| {
        let t = ((q.y - yt) / (yb - yt)).clamp(0.0, 1.0);
        let k = 0.15 - 0.08 * t;
        Color::new(k, k * 1.03, k * 1.06, 1.0)
    });
    stroke(&[pyr[0], pyr[1], pyr[2], pyr[3]], false, 2.0, edge);
    stroke(&[pyr[0], pyr[7], pyr[6]], false, 2.0, edge);
    // Petit cadran rond au sommet
    let c = p(0.47, 0.17);
    let r = v.h * 0.028;
    draw_circle(c.x, c.y, r + 3.0, INTERIOR_LIGHT);
    draw_circle(c.x, c.y, r, BLACK);
    round_line(c, c + vec2(r * 0.6, -r * 0.5), 1.5, WHITE);
    // Consoles latérales
    for cr in [l.left_console, l.right_console] {
        rrect(cr, 10.0, PANEL_DARK);
        rrect_lines(cr, 10.0, 1.5, edge);
        for k in 0..4 {
            let y = cr.y + 20.0 + k as f32 * 26.0;
            for m in 0..3 {
                let b = Rect::new(cr.x + 14.0 + m as f32 * (cr.w - 28.0) / 3.0, y, (cr.w - 28.0) / 3.0 - 8.0, 14.0);
                rrect(b, 3.0, Color::new(0.16, 0.2, 0.15, 1.0));
                draw_rectangle(b.x + 3.0, b.y + 4.0, b.w - 6.0, 3.0, Color::new(0.5, 0.9, 0.4, 0.35));
            }
        }
    }
    // Manette des gaz sur la console gauche
    let lc = l.left_console;
    let slot = Rect::new(lc.x + lc.w * 0.75, lc.y + 18.0, 8.0, lc.h - 36.0);
    draw_rectangle(slot.x, slot.y, slot.w, slot.h, BLACK);
    let ty = slot.y + slot.h * (1.0 - throttle.clamp(0.0, 1.0));
    rrect(Rect::new(slot.x - 18.0, ty - 10.0, 44.0, 20.0), 5.0, Color::new(0.3, 0.31, 0.32, 1.0));
    // Manche au centre
    let base = p(0.5, 1.05);
    let grip = p(0.5, 0.80);
    round_line(base, grip, 14.0, Color::new(0.16, 0.165, 0.17, 1.0));
    rrect(Rect::new(grip.x - 16.0, grip.y - 30.0, 32.0, 42.0), 10.0, Color::new(0.2, 0.205, 0.21, 1.0));
    draw_circle(grip.x, grip.y - 22.0, 4.0, Color::new(0.6, 0.1, 0.08, 1.0));
    // Soufflet jaune et noir à la base
    for k in 0..6 {
        let y = base.y - 10.0 - k as f32 * 6.0;
        draw_line(base.x - 26.0 + k as f32, y, base.x + 26.0 - k as f32, y, 3.0, if k % 2 == 0 { Color::new(0.85, 0.7, 0.1, 1.0) } else { BLACK });
    }
}

fn hud_txt(s: &str, x: f32, y: f32, size: f32) {
    txt(s, x + 1.0, y + 1.0, size, Color::new(0.0, 0.08, 0.0, 0.5));
    txt(s, x, y, size, HUD_GREEN);
}

/// HUD conforme : ligne d'horizon et échelle d'assiette, vecteur vitesse, vitesse, altitude, cap.
pub fn hud(ac: &Aircraft, v: &View, r: Rect, fma: &str, warn: Option<&str>, time: f64) {
    scissor(Some(r));
    let hr = vec3(ac.psi.cos() as f32, 0.0, ac.psi.sin() as f32);
    let big = 100_000.0f32;
    let at = |pitch_deg: f64, k: f32| v.project(dir(ac.psi, pitch_deg.to_radians()) * big + hr * big * k);
    // Horizon (large, avec trou central) et échelle tous les 5°
    for p in (-30i32..=30).step_by(5) {
        let (k0, k1) = if p == 0 { (0.06, 0.6) } else { (0.035, 0.11) };
        for side in [-1.0f32, 1.0] {
            if let (Some(a), Some(b)) = (at(p as f64, side * k0), at(p as f64, side * k1)) {
                if p < 0 {
                    // Barres négatives en pointillés
                    for kk in 0..3 {
                        let t0 = kk as f32 / 3.0;
                        let t1 = t0 + 0.2;
                        draw_line(a.x + (b.x - a.x) * t0, a.y + (b.y - a.y) * t0, a.x + (b.x - a.x) * t1, a.y + (b.y - a.y) * t1, 1.5, HUD_GREEN);
                    }
                } else {
                    draw_line(a.x, a.y, b.x, b.y, 1.5, HUD_GREEN);
                }
                if p != 0 {
                    hud_txt(&format!("{}", p.abs()), b.x + side * 6.0 - if side < 0.0 { 16.0 } else { 0.0 }, b.y + 4.0, 12.0);
                }
            }
        }
    }
    // Vecteur vitesse (route sol)
    if let Some(f) = v.project_dir(dir(ac.track_deg().to_radians(), ac.gamma)) {
        draw_circle_lines(f.x, f.y, 8.0, 1.8, HUD_GREEN);
        draw_line(f.x - 22.0, f.y, f.x - 8.0, f.y, 1.8, HUD_GREEN);
        draw_line(f.x + 8.0, f.y, f.x + 22.0, f.y, 1.8, HUD_GREEN);
        draw_line(f.x, f.y - 8.0, f.x, f.y - 16.0, 1.8, HUD_GREEN);
    }
    // Repère de nez (croix « W »)
    if let Some(n) = v.project_dir(dir(ac.psi, ac.theta)) {
        let pts = [n + vec2(-18.0, 0.0), n + vec2(-9.0, 0.0), n + vec2(-4.5, 7.0), n + vec2(0.0, 0.0), n + vec2(4.5, 7.0), n + vec2(9.0, 0.0), n + vec2(18.0, 0.0)];
        stroke(&pts, false, 1.5, HUD_GREEN);
    }
    scissor(None);
    // Vitesse (gauche), altitude (droite), cap (haut)
    let cy = r.y + r.h * 0.5;
    let bx = r.x + 10.0;
    draw_rectangle_lines(bx, cy - 14.0, 70.0, 26.0, 1.5, HUD_GREEN);
    hud_txt(&format!("{:.0}", ac.d.keas), bx + 8.0, cy + 6.0, 18.0);
    hud_txt(&format!("M {:.2}", ac.d.mach), bx, cy + 32.0, 13.0);
    hud_txt(&format!("G {:.1}", ac.d.nz), bx, cy + 50.0, 13.0);
    hud_txt(&format!("a {:.1}", ac.d.alpha.to_degrees()), bx, cy + 68.0, 13.0);
    let ax = r.x + r.w - 92.0;
    draw_rectangle_lines(ax, cy - 14.0, 84.0, 26.0, 1.5, HUD_GREEN);
    hud_txt(&format!("{:.0}", ac.alt / FT), ax + 8.0, cy + 6.0, 18.0);
    hud_txt(&format!("{:+.0} VS", ac.d.vs / FT * 60.0), ax, cy + 32.0, 13.0);
    if ac.d.agl / FT < 2_500.0 && !ac.on_ground {
        hud_txt(&format!("R {:.0}", ac.d.agl / FT), ax, cy + 50.0, 13.0);
    }
    let hdg = ac.hdg_deg();
    let hx = r.x + r.w / 2.0;
    let hy = r.y + 6.0;
    let ppd = r.w / 50.0;
    for k in -6..=6 {
        let h = (hdg / 5.0).round() as i32 * 5 + k * 5;
        let x = hx + (h as f32 - hdg as f32) * ppd;
        if (x - hx).abs() > r.w * 0.42 {
            continue;
        }
        draw_line(x, hy, x, hy + if h % 10 == 0 { 9.0 } else { 5.0 }, 1.5, HUD_GREEN);
        if h % 10 == 0 {
            hud_txt(&format!("{:02}", h.rem_euclid(360) / 10), x - 7.0, hy + 22.0, 12.0);
        }
    }
    draw_triangle(vec2(hx, hy + 26.0), vec2(hx - 5.0, hy + 34.0), vec2(hx + 5.0, hy + 34.0), HUD_GREEN);
    hud_txt(fma, r.x + 10.0, r.y + r.h - 8.0, 12.0);
    if let Some(w) = warn {
        if (time * 2.5).fract() < 0.6 {
            let c = Color::new(1.0, 0.3, 0.25, 1.0);
            txt_c(w, hx, cy - 60.0, 18.0, c);
        }
    }
}

/// Plaque d'équipement vissée.
pub fn plate(r: Rect) {
    rrect(r, 8.0, PANEL_DARK);
    rrect_lines(r, 8.0, 1.2, Color::new(0.2, 0.21, 0.22, 1.0));
    for (x, y) in [(r.x + 8.0, r.y + 8.0), (r.x + r.w - 8.0, r.y + 8.0), (r.x + 8.0, r.y + r.h - 8.0), (r.x + r.w - 8.0, r.y + r.h - 8.0)] {
        draw_circle(x, y, 4.0, Color::new(0.16, 0.17, 0.17, 1.0));
        draw_line(x - 2.5, y - 1.0, x + 2.5, y + 1.0, 1.0, Color::new(0.3, 0.31, 0.32, 1.0));
    }
}

/// Cadran rond avec aiguille (valeur normalisée 0..1 sur 270°) et graduations.
pub fn dial(c: Vec2, r: f32, f: f32, label: &str, ticks: &[&str]) {
    draw_circle(c.x, c.y, r + 4.0, Color::new(0.2, 0.2, 0.21, 1.0));
    draw_circle(c.x, c.y, r, Color::new(0.04, 0.04, 0.045, 1.0));
    let a0 = 135f32.to_radians();
    let span = 270f32.to_radians();
    for (i, t) in ticks.iter().enumerate() {
        let a = a0 + span * i as f32 / (ticks.len() - 1).max(1) as f32;
        let d = vec2(a.cos(), a.sin());
        draw_line(c.x + d.x * r * 0.82, c.y + d.y * r * 0.82, c.x + d.x * r * 0.96, c.y + d.y * r * 0.96, 2.0, WHITE);
        txt_c(t, c.x + d.x * r * 0.64, c.y + d.y * r * 0.64 + 4.0, 10.0, WHITE);
    }
    let a = a0 + span * f.clamp(0.0, 1.0);
    let d = vec2(a.cos(), a.sin());
    round_line(c - d * r * 0.15, c + d * r * 0.85, 2.5, WHITE);
    draw_circle(c.x, c.y, 4.0, Color::new(0.5, 0.5, 0.5, 1.0));
    txt_c(label, c.x, c.y + r * 0.45, 12.0, WHITE);
}

/// Interrupteur gardé à capot rouge ; vrai si cliqué.
pub fn guarded(r: Rect, label: &str, on: bool, time: f64) -> bool {
    plate(r);
    let gw = r.w * 0.34;
    let g = Rect::new(r.x + r.w - gw - 12.0, r.y + 12.0, gw, r.h - 24.0);
    // Capot relevé (rouge) quand l'interrupteur est actif
    let red = Color::new(0.78, 0.08, 0.06, 1.0);
    if on {
        rrect(Rect::new(g.x, g.y - g.h * 0.55, g.w, g.h * 0.55), 4.0, red);
        draw_circle(g.x + g.w / 2.0, g.y + g.h * 0.55, 6.0, Color::new(0.85, 0.85, 0.87, 1.0));
        if (time * 2.0).fract() < 0.6 {
            draw_circle(g.x + g.w / 2.0, g.y + g.h * 0.85, 4.0, AMBER);
        }
    } else {
        rrect(g, 5.0, red);
        draw_rectangle(g.x + 3.0, g.y + 3.0, g.w - 6.0, g.h * 0.2, Color::new(1.0, 1.0, 1.0, 0.18));
    }
    let lines: Vec<&str> = label.split(' ').collect();
    for (i, l) in lines.iter().enumerate() {
        txt(l, r.x + 14.0, r.y + 24.0 + i as f32 * 18.0, 15.0, Color::new(0.75, 1.0, 0.45, 1.0));
    }
    clicked(r)
}

/// Levier de train (tirer vers le bas = sortie) ; vrai si cliqué.
pub fn gear_lever(r: Rect, down: bool) -> bool {
    plate(r);
    let slot = Rect::new(r.x + r.w / 2.0 - 4.0, r.y + 18.0, 8.0, r.h - 36.0);
    draw_rectangle(slot.x, slot.y, slot.w, slot.h, BLACK);
    let ky = if down { slot.y + slot.h - 10.0 } else { slot.y + 10.0 };
    draw_line(slot.x + 4.0, slot.y + slot.h / 2.0, slot.x + 4.0, ky, 5.0, Color::new(0.7, 0.7, 0.72, 1.0));
    draw_circle(slot.x + 4.0, ky, 11.0, Color::new(0.92, 0.92, 0.94, 1.0));
    draw_circle_lines(slot.x + 4.0, ky, 11.0, 1.5, Color::new(0.5, 0.5, 0.52, 1.0));
    txt_c("UP", r.x + r.w / 2.0, r.y + 14.0, 10.0, WHITE);
    txt_c("DN", r.x + r.w / 2.0, r.y + r.h - 4.0, 10.0, WHITE);
    clicked(r)
}

/// Boîtier radio (NAV1 / NAV2 / transpondeur / ADF) sur l'auvent.
pub fn radio(r: Rect, squawk: &str) {
    rrect(r, 6.0, Color::new(0.02, 0.03, 0.05, 1.0));
    rrect_lines(r, 6.0, 1.5, Color::new(0.25, 0.27, 0.3, 1.0));
    let c = Color::new(0.35, 0.8, 1.0, 1.0);
    let half = r.w / 2.0;
    txt_c("NAV1", r.x + half / 2.0, r.y + 14.0, 11.0, WHITE);
    txt_c("NAV2", r.x + half * 1.5, r.y + 14.0, 11.0, WHITE);
    txt_c("110.50", r.x + half / 2.0, r.y + 30.0, 13.0, c);
    txt_c("110.50", r.x + half * 1.5, r.y + 30.0, 13.0, c);
    txt_c("113.90", r.x + half / 2.0, r.y + 44.0, 12.0, WHITE);
    txt_c("113.90", r.x + half * 1.5, r.y + 44.0, 12.0, WHITE);
    txt_c("XPDR", r.x + half / 2.0, r.y + 60.0, 11.0, WHITE);
    txt_c("ADF", r.x + half * 1.5, r.y + 60.0, 11.0, WHITE);
    let sq_c = if squawk == "7700" { RED } else { GREEN_S };
    txt_c(squawk, r.x + half / 2.0, r.y + 76.0, 14.0, sq_c);
    txt_c("890.0", r.x + half * 1.5, r.y + 76.0, 13.0, c);
}

/// Boutons DSPLY (luminosité jour/nuit des écrans), décoratifs.
pub fn dsply(r: Rect) {
    plate(r);
    txt("D", r.x + 8.0, r.y + 22.0, 13.0, Color::new(0.75, 1.0, 0.45, 1.0));
    txt("S", r.x + 8.0, r.y + 36.0, 13.0, Color::new(0.75, 1.0, 0.45, 1.0));
    txt("P", r.x + 8.0, r.y + 50.0, 13.0, Color::new(0.75, 1.0, 0.45, 1.0));
    for (i, l) in ["LH", "", "RH", ""].iter().enumerate() {
        let c = vec2(r.x + 40.0 + i as f32 * (r.w - 50.0) / 4.0, r.y + r.h * 0.58);
        draw_circle(c.x, c.y, 13.0, Color::new(0.45, 0.46, 0.47, 1.0));
        draw_circle(c.x, c.y, 9.0, Color::new(0.6, 0.61, 0.62, 1.0));
        draw_line(c.x, c.y, c.x + 4.0, c.y - 9.0, 2.0, WHITE);
        if !l.is_empty() {
            txt_c(l, c.x + (r.w - 50.0) / 8.0, r.y + 16.0, 11.0, Color::new(0.75, 1.0, 0.45, 1.0));
        }
    }
}
