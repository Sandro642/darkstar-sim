//! Cockpit 3D (repère avion, œil à l'origine : x droite, y haut, z avant, en mètres), d'après
//! l'intérieur du Darkstar : arceaux de verrière tubulaires rivetés, ciel de cabine épousant le haut
//! des fenêtres, pyramide centrale étroite à arêtes chanfreinées et ailes latérales, instruments
//! encastrés (lunettes en relief, cadran, rangée de boutons éclairés, capots rouges), appuis de
//! fenêtres rembourrés et nervurés, consoles latérales denses, manette bi-levier, manche cylindrique
//! à soufflet annelé.
//! Textures procédurales réunies dans un atlas unique ; éclairage par sommet depuis les fenêtres et
//! assombrissement des recoins ; triangles triés du plus lointain au plus proche, dessinés par lots.

use crate::aircraft::Aircraft;
use crate::svs::{self, Cam, SvsCam, View};
use macroquad::prelude::*;

// ---------- Atlas de textures ----------

#[derive(Clone, Copy, PartialEq)]
enum Mat {
    Metal,
    Panel,
    Ribs,
    Console,
    Floor,
    Boot,
    Teal,
    Red,
    Glass,
}

const ATLAS: u16 = 1024;
const CELL: f32 = 1.0 / 3.0;

fn cell(m: Mat) -> Vec2 {
    let i = match m {
        Mat::Metal => 0,
        Mat::Panel => 1,
        Mat::Ribs => 2,
        Mat::Console => 3,
        Mat::Floor => 4,
        Mat::Boot => 5,
        Mat::Teal => 6,
        Mat::Red => 7,
        Mat::Glass => 8,
    };
    vec2((i % 3) as f32 * CELL, (i / 3) as f32 * CELL)
}

fn hash(x: i32, y: i32, s: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ (s as u32).wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn noise(x: f32, y: f32, s: i32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy, s) + (hash(ix + 1, iy, s) - hash(ix, iy, s)) * sx;
    let b = hash(ix, iy + 1, s) + (hash(ix + 1, iy + 1, s) - hash(ix, iy + 1, s)) * sx;
    a + (b - a) * sy
}

/// Couleur d'un texel (coordonnées locales 0..1 dans la case du matériau).
fn texel(m: Mat, u: f32, v: f32) -> [f32; 3] {
    let (x, y) = (u * 340.0, v * 340.0);
    let grain = noise(x / 20.0, y / 20.0, 1) * 0.05 + noise(x / 5.0, y / 5.0, 2) * 0.025;
    match m {
        Mat::Metal => {
            let brush = noise(x / 120.0, y * 1.2, 3) * 0.04;
            let k = 0.30 + grain + brush;
            [k, k * 1.02, k * 1.05]
        }
        Mat::Panel => {
            // Platine vissée : joints, vis, sérigraphies claires éparses
            let (cx, cy) = (x % 85.0, y % 85.0);
            let joint = if cx < 1.5 || cy < 1.5 { -0.08 } else { 0.0 };
            let (dx, dy) = (cx - 7.0, cy - 7.0);
            let screw = if dx * dx + dy * dy < 12.0 { 0.16 - (dx + dy) * 0.015 } else { 0.0 };
            let legend = if hash((x / 18.0) as i32, (y / 9.0) as i32, 7) > 0.93 && (cy % 9.0) > 6.0 { 0.25 } else { 0.0 };
            let k = 0.24 + grain * 0.6 + joint + screw + legend;
            [k, k * 1.04, k * 1.02]
        }
        Mat::Ribs => {
            let ph = (x / 22.0 * std::f32::consts::TAU).sin();
            let k = 0.22 + ph * 0.06 + grain;
            [k, k * 1.02, k * 1.03]
        }
        Mat::Console => {
            // Console dense : interrupteurs à bascule, légendes vertes rétroéclairées
            let (cx, cy) = (x % 42.0, y % 34.0);
            let k = 0.18 + grain;
            if (14.0..28.0).contains(&cx) && (8.0..20.0).contains(&cy) {
                let lever = (cx - 21.0).abs() < 2.0 && cy < 14.0;
                if lever { [0.7, 0.7, 0.72] } else { [0.08, 0.08, 0.09] }
            } else if (6.0..36.0).contains(&cx) && (24.0..27.0).contains(&cy) && hash((x / 42.0) as i32, (y / 34.0) as i32, 9) > 0.3 {
                [0.45, 0.95, 0.35]
            } else {
                [k, k * 1.03, k * 1.02]
            }
        }
        Mat::Floor => {
            let (cx, cy) = (x % 14.0 - 7.0, y % 14.0 - 7.0);
            let k = 0.14 + grain + if cx * cx + cy * cy < 8.0 { 0.04 } else { 0.0 };
            [k, k, k * 1.02]
        }
        Mat::Boot => {
            if ((y / 26.0).floor() as i32) % 2 == 0 { [0.80, 0.62, 0.06] } else { [0.04, 0.04, 0.04] }
        }
        Mat::Teal => {
            let d = ((u - 0.5).abs().max((v - 0.5).abs()) * 2.0).powf(4.0);
            [0.30 + 0.25 * (1.0 - d), 0.85 * (1.0 - 0.4 * d), 0.75 * (1.0 - 0.4 * d)]
        }
        Mat::Red => {
            let k = 0.75 + grain;
            [k, 0.07, 0.05]
        }
        Mat::Glass => {
            let k = 0.03 + v * 0.03;
            [k, k * 1.1, k * 1.3]
        }
    }
}

fn atlas() -> Texture2D {
    let n = ATLAS;
    let mut img = Image::gen_image_color(n, n, WHITE);
    let mats = [Mat::Metal, Mat::Panel, Mat::Ribs, Mat::Console, Mat::Floor, Mat::Boot, Mat::Teal, Mat::Red, Mat::Glass];
    let cs = (n as f32 * CELL) as u32;
    for m in mats {
        let o = cell(m) * n as f32;
        for j in 0..cs {
            for i in 0..cs {
                let c = texel(m, i as f32 / cs as f32, j as f32 / cs as f32);
                img.set_pixel((o.x as u32 + i).min(n as u32 - 1), (o.y as u32 + j).min(n as u32 - 1), Color::new(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), 1.0));
            }
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
}

// ---------- Construction du maillage ----------

struct Tri {
    p: [Vec3; 3],
    n: [Vec3; 3],
    uv: [Vec2; 3],
    tint: [f32; 3],
}

struct Builder {
    t: Vec<Tri>,
}

/// UV dans la case du matériau ; coordonnées locales repliées en miroir dans [0,1].
fn uv_in(m: Mat, u: f32, v: f32) -> Vec2 {
    let f = |a: f32| {
        let r = a.rem_euclid(2.0);
        if r > 1.0 { 2.0 - r } else { r }
    };
    cell(m) + vec2(0.004 + f(u) * (CELL - 0.008), 0.004 + f(v) * (CELL - 0.008))
}

impl Builder {
    fn tri(&mut self, p: [Vec3; 3], n: [Vec3; 3], uv: [Vec2; 3], tint: [f32; 3]) {
        self.t.push(Tri { p, n, uv, tint });
    }

    /// Quadrilatère plan subdivisé (éclairage et assombrissement par sommet).
    fn quad(&mut self, p: [Vec3; 4], m: Mat, scale: f32, tint: [f32; 3], div: usize) {
        let n = (p[1] - p[0]).cross(p[3] - p[0]).normalize_or_zero();
        let (l1, l2) = ((p[1] - p[0]).length() / scale, (p[3] - p[0]).length() / scale);
        let at = |a: f32, b: f32| {
            let top = p[0] + (p[1] - p[0]) * a;
            let bot = p[3] + (p[2] - p[3]) * a;
            top + (bot - top) * b
        };
        let d = div.max(1);
        for j in 0..d {
            for i in 0..d {
                let (a0, a1, b0, b1) = (i as f32 / d as f32, (i + 1) as f32 / d as f32, j as f32 / d as f32, (j + 1) as f32 / d as f32);
                let q = [at(a0, b0), at(a1, b0), at(a1, b1), at(a0, b1)];
                let uv = [uv_in(m, a0 * l1, b0 * l2), uv_in(m, a1 * l1, b0 * l2), uv_in(m, a1 * l1, b1 * l2), uv_in(m, a0 * l1, b1 * l2)];
                self.tri([q[0], q[1], q[2]], [n; 3], [uv[0], uv[1], uv[2]], tint);
                self.tri([q[0], q[2], q[3]], [n; 3], [uv[0], uv[2], uv[3]], tint);
            }
        }
    }

    fn boxq(&mut self, a: Vec3, b: Vec3, m: Mat, tint: [f32; 3]) {
        let c = |x, y, z| vec3(x, y, z);
        let (x0, y0, z0, x1, y1, z1) = (a.x, a.y, a.z, b.x, b.y, b.z);
        let s = 0.1;
        self.quad([c(x0, y1, z0), c(x1, y1, z0), c(x1, y1, z1), c(x0, y1, z1)], m, s, tint, 1);
        self.quad([c(x0, y0, z0), c(x1, y0, z0), c(x1, y1, z0), c(x0, y1, z0)], m, s, tint, 1);
        self.quad([c(x0, y0, z0), c(x0, y0, z1), c(x0, y1, z1), c(x0, y1, z0)], m, s, tint, 1);
        self.quad([c(x1, y0, z0), c(x1, y0, z1), c(x1, y1, z1), c(x1, y1, z0)], m, s, tint, 1);
        self.quad([c(x0, y0, z1), c(x1, y0, z1), c(x1, y1, z1), c(x0, y1, z1)], m, s, tint, 1);
    }

    /// Tube à normales lissées le long d'un chemin (arceaux, manche, rembourrages).
    fn tube(&mut self, path: &[Vec3], r: f32, sides: usize, m: Mat, tint: [f32; 3]) {
        let ring = |k: usize| -> Vec<(Vec3, Vec3)> {
            let t = if k + 1 < path.len() { path[k + 1] - path[k] } else { path[k] - path[k - 1] }.normalize_or_zero();
            let up = if t.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
            let a = t.cross(up).normalize();
            let b = a.cross(t).normalize();
            (0..=sides)
                .map(|s| {
                    let th = s as f32 / sides as f32 * std::f32::consts::TAU;
                    let n = a * th.cos() + b * th.sin();
                    (path[k] + n * r, n)
                })
                .collect()
        };
        let rings: Vec<_> = (0..path.len()).map(ring).collect();
        let len = 2.0 * std::f32::consts::PI * r / 0.1;
        for k in 0..path.len() - 1 {
            let along0 = k as f32 / (path.len() - 1) as f32 * 4.0;
            let along1 = (k + 1) as f32 / (path.len() - 1) as f32 * 4.0;
            for s in 0..sides {
                let (p00, n00) = rings[k][s];
                let (p01, n01) = rings[k][s + 1];
                let (p10, n10) = rings[k + 1][s];
                let (p11, n11) = rings[k + 1][s + 1];
                let (u0, u1) = (s as f32 / sides as f32 * len, (s + 1) as f32 / sides as f32 * len);
                let uv = [uv_in(m, u0, along0), uv_in(m, u1, along0), uv_in(m, u1, along1), uv_in(m, u0, along1)];
                self.tri([p00, p01, p11], [n00, n01, n11], [uv[0], uv[1], uv[2]], tint);
                self.tri([p00, p11, p10], [n00, n11, n10], [uv[0], uv[2], uv[3]], tint);
            }
        }
    }

    /// Disque (cadran, rivet) orienté selon la normale.
    fn disc(&mut self, c: Vec3, n: Vec3, r: f32, m: Mat, tint: [f32; 3]) {
        let n = n.normalize();
        let a = n.cross(Vec3::Y).normalize_or(Vec3::X);
        let b = a.cross(n);
        let seg = 14;
        let uv = uv_in(m, 0.5, 0.5);
        for k in 0..seg {
            let (t0, t1) = (k as f32 / seg as f32 * std::f32::consts::TAU, (k + 1) as f32 / seg as f32 * std::f32::consts::TAU);
            let (p0, p1) = (c + (a * t0.cos() + b * t0.sin()) * r, c + (a * t1.cos() + b * t1.sin()) * r);
            self.tri([c, p0, p1], [n; 3], [uv; 3], tint);
        }
    }
}

/// Courbe de Catmull-Rom passant par les points de contrôle.
fn spline(ctrl: &[Vec3], per: usize) -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in 0..ctrl.len() - 1 {
        let p0 = ctrl[i.saturating_sub(1)];
        let (p1, p2) = (ctrl[i], ctrl[i + 1]);
        let p3 = ctrl[(i + 2).min(ctrl.len() - 1)];
        for k in 0..per {
            let t = k as f32 / per as f32;
            let (t2, t3) = (t * t, t * t * t);
            out.push(0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3));
        }
    }
    out.push(*ctrl.last().unwrap());
    out
}

// Face avant de la pyramide : (hauteur, profondeur, demi-largeur)
const PYR: [(f32, f32, f32); 4] = [(0.07, 0.80, 0.035), (-0.20, 0.66, 0.15), (-0.66, 0.53, 0.27), (-1.0, 0.47, 0.33)];

fn pyr_at(y: f32) -> (f32, f32) {
    for w in PYR.windows(2) {
        if y <= w[0].0 && y >= w[1].0 {
            let t = (w[0].0 - y) / (w[0].0 - w[1].0);
            return (w[0].1 + (w[1].1 - w[0].1) * t, w[0].2 + (w[1].2 - w[0].2) * t);
        }
    }
    (PYR[3].1, PYR[3].2)
}

/// Point sur la face avant de la pyramide, légèrement en avant (instruments).
pub fn on_panel(x: f32, y: f32) -> Vec3 {
    vec3(x, y, pyr_at(y).0 - 0.006)
}

/// Point sur une aile latérale (`s` = -1 gauche, +1 droite ; u = 0 au bord de la pyramide, 1 à l'extérieur).
fn on_wing(s: f32, u: f32, y: f32) -> Vec3 {
    let (z, hw) = pyr_at(y);
    vec3(s * (hw + u * 0.20), y - u * 0.04, z + u * 0.04 - 0.006)
}

const DARK: [f32; 3] = [0.42, 0.44, 0.46];
const BLACKISH: [f32; 3] = [0.26, 0.27, 0.29];

fn build() -> Vec<Tri> {
    let mut b = Builder { t: Vec::new() };
    let v = vec3;
    for s in [-1.0f32, 1.0] {
        // Ciel de cabine épousant le haut des fenêtres
        let edge = spline(&[v(s * 0.05, 0.10, 0.82), v(s * 0.45, 0.30, 0.65), v(s * 1.05, 0.34, 0.25), v(s * 1.4, 0.30, -0.4)], 6);
        for w in edge.windows(2) {
            let (a, c) = (w[0], w[1]);
            b.quad([a, c, v(c.x * 0.6, 0.95, c.z - 0.25), v(a.x * 0.6, 0.95, a.z - 0.25)], Mat::Metal, 0.4, BLACKISH, 2);
        }
        b.tube(&edge, 0.025, 8, Mat::Metal, DARK);
        // Arceau tubulaire riveté vers le sommet de la pyramide
        let bow = spline(&[v(s * 0.62, 0.70, -0.10), v(s * 0.50, 0.50, 0.35), v(s * 0.24, 0.22, 0.66), v(s * 0.04, 0.08, 0.80)], 8);
        b.tube(&bow, 0.034, 10, Mat::Metal, DARK);
        for k in (2..bow.len() - 2).step_by(3) {
            let n = -bow[k];
            b.disc(bow[k] + n.normalize() * 0.035, n, 0.006, Mat::Metal, [0.8, 0.82, 0.85]);
        }
    }
    // Pyramide : face avant en trois pans, arêtes chanfreinées, flancs vers le nez
    for w in PYR.windows(2) {
        let ((y0, z0, h0), (y1, z1, h1)) = (w[0], w[1]);
        b.quad([v(-h0, y0, z0), v(h0, y0, z0), v(h1, y1, z1), v(-h1, y1, z1)], Mat::Panel, 0.17, [0.62, 0.64, 0.66], 3);
        for s in [-1.0f32, 1.0] {
            let (e0, e1) = (v(s * h0, y0, z0), v(s * h1, y1, z1));
            let off = v(s * 0.018, 0.0, 0.03);
            b.quad([e0, e1, e1 + off, e0 + off], Mat::Metal, 0.2, [0.55, 0.57, 0.6], 1);
            b.quad([e0 + off, e1 + off, v(s * (h1 + 0.15), y1, z1 + 0.45), v(s * (h0 + 0.10), y0, z0 + 0.45)], Mat::Metal, 0.4, BLACKISH, 2);
        }
    }
    // Ailes latérales (écran moteurs à gauche, cadrans à droite)
    for s in [-1.0f32, 1.0] {
        let q = [on_wing(s, 0.0, -0.30), on_wing(s, 1.0, -0.30), on_wing(s, 1.0, -0.68), on_wing(s, 0.0, -0.68)].map(|p| p + v(0.0, 0.0, 0.006));
        b.quad(q, Mat::Panel, 0.17, [0.55, 0.57, 0.6], 3);
        b.tube(&[on_wing(s, 1.0, -0.30), on_wing(s, 1.0, -0.68)], 0.012, 6, Mat::Metal, DARK);
    }
    // Lunettes en relief : écran principal et boîtier radio ; cadran sommital
    let bez = |b: &mut Builder, x0: f32, y0: f32, x1: f32, y1: f32, depth: f32| {
        let (p00, p10, p11, p01) = (on_panel(x0, y0), on_panel(x1, y0), on_panel(x1, y1), on_panel(x0, y1));
        let n = (p10 - p00).cross(p01 - p00).normalize() * depth;
        let n = if n.z > 0.0 { -n } else { n };
        for (a, c) in [(p00, p10), (p10, p11), (p11, p01), (p01, p00)] {
            b.quad([a, c, c + n, a + n], Mat::Metal, 0.1, [0.5, 0.52, 0.55], 1);
        }
        b.quad([p00 + n * 0.2, p10 + n * 0.2, p11 + n * 0.2, p01 + n * 0.2], Mat::Glass, 0.3, [1.0, 1.0, 1.0], 1);
    };
    bez(&mut b, -0.165, -0.22, 0.165, -0.50, 0.012);
    bez(&mut b, 0.022, -0.055, 0.115, -0.145, 0.010);
    let gc = on_panel(-0.035, 0.0);
    b.disc(gc + v(0.0, 0.0, -0.004), v(0.0, 0.2, -1.0), 0.028, Mat::Metal, [0.6, 0.62, 0.65]);
    b.disc(gc + v(0.0, 0.0, -0.008), v(0.0, 0.2, -1.0), 0.022, Mat::Glass, [1.0, 1.0, 1.0]);
    // Rangée de boutons éclairés sous l'écran
    for k in 0..6 {
        let p = on_panel(-0.10 + k as f32 * 0.04, -0.53);
        b.boxq(p + v(-0.012, -0.012, -0.012), p + v(0.012, 0.012, 0.0), Mat::Teal, [1.0, 1.0, 1.0]);
    }
    // Capots rouges des interrupteurs gardés
    for x in [-0.10f32, 0.0, 0.10] {
        let p = on_panel(x, -0.60);
        b.boxq(p + v(-0.022, -0.03, -0.028), p + v(0.022, 0.03, 0.0), Mat::Red, [1.0, 1.0, 1.0]);
    }
    for s in [-1.0f32, 1.0] {
        // Appui de fenêtre rembourré et paroi nervurée
        let rail = spline(&[v(s * 1.0, -0.26, -0.3), v(s * 0.72, -0.24, 0.25), v(s * 0.40, -0.20, 0.62)], 8);
        b.tube(&rail, 0.055, 10, Mat::Metal, BLACKISH);
        for w in rail.windows(2) {
            let (a, c) = (w[0] + v(-s * 0.05, -0.04, 0.0), w[1] + v(-s * 0.05, -0.04, 0.0));
            b.quad([a, c, c + v(0.0, -0.8, 0.0), a + v(0.0, -0.8, 0.0)], Mat::Ribs, 0.3, [0.5, 0.52, 0.54], 2);
        }
        for k in (1..rail.len() - 1).step_by(2) {
            let p = rail[k] + v(-s * 0.06, -0.06, 0.0);
            b.boxq(p + v(-0.012, -0.72, -0.012), p + v(0.012, 0.0, 0.012), Mat::Metal, DARK);
        }
        // Console latérale inclinée, dense en interrupteurs
        let (xi, xo) = (s * 0.42, s * 0.78);
        b.quad([v(xo, -0.66, -0.25), v(xi, -0.60, -0.25), v(xi, -0.60, 0.48), v(xo, -0.66, 0.48)], Mat::Console, 0.12, [0.9, 0.9, 0.9], 2);
        b.quad([v(xi, -0.60, -0.25), v(xi, -1.0, -0.25), v(xi, -1.0, 0.48), v(xi, -0.60, 0.48)], Mat::Metal, 0.3, BLACKISH, 1);
        b.quad([v(xo, -0.66, 0.48), v(xi, -0.60, 0.48), v(xi, -1.0, 0.48), v(xo, -1.0, 0.48)], Mat::Metal, 0.3, BLACKISH, 1);
    }
    // Plancher
    b.quad([v(-0.9, -1.0, -0.3), v(0.9, -1.0, -0.3), v(0.9, -1.0, 0.9), v(-0.9, -1.0, 0.9)], Mat::Floor, 0.3, [0.8, 0.8, 0.8], 3);
    // Manche : soufflet annelé, fût, poignée galbée, bouton rouge
    b.tube(&[v(0.0, -1.0, 0.36), v(0.0, -0.90, 0.37)], 0.07, 12, Mat::Boot, [1.0, 1.0, 1.0]);
    b.tube(&[v(0.0, -0.90, 0.37), v(0.0, -0.60, 0.385)], 0.016, 8, Mat::Metal, DARK);
    b.tube(&[v(0.0, -0.60, 0.385), v(0.0, -0.50, 0.40), v(0.0, -0.46, 0.43)], 0.032, 10, Mat::Metal, [0.36, 0.37, 0.39]);
    b.disc(v(0.0, -0.44, 0.44), v(0.0, 1.0, -0.4), 0.008, Mat::Red, [1.0, 1.0, 1.0]);
    b.t
}

thread_local! {
    static ATL: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
    static GEOM: std::cell::RefCell<Option<Vec<Tri>>> = const { std::cell::RefCell::new(None) };
}

/// Emplacements projetés des instruments (pixels écran), absents si hors du regard.
#[derive(Default)]
pub struct Spots {
    pub mfd: Option<Rect>,
    pub engines: Option<Rect>,
    pub radio: Option<Rect>,
    pub dsply: Option<Rect>,
    pub switches: [Option<Rect>; 3],
    pub cg: Option<(Vec2, f32)>,
    pub h2: Option<(Vec2, f32)>,
    pub gear: Option<Rect>,
    pub greens: Option<Vec2>,
}

/// Dessine le cockpit par-dessus la vue extérieure et renvoie les emplacements des instruments.
pub fn draw(ac: &Aircraft, rect: Rect, cam: Cam, throttle: f32) -> Spots {
    let (fwd0, up0, _) = svs::basis(ac);
    let right0 = fwd0.cross(up0).normalize();
    let to_w = |p: Vec3| right0 * p.x + up0 * p.y + fwd0 * p.z;
    let (fwd, up) = svs::look_basis(ac, cam);
    let proj = Mat4::perspective_rh_gl(cam.fovy.to_radians(), rect.w / rect.h, 0.03, 60.0);
    let mat = proj * Mat4::look_at_rh(Vec3::ZERO, fwd, up);
    set_camera(&SvsCam { mat, vp: svs::viewport(rect), depth: false });
    let tex = ATL.with(|a| a.borrow_mut().get_or_insert_with(atlas).clone());
    // Éclairage (repère avion) : les deux fenêtres et le haut, ambiance faible
    let lights = [(vec3(-0.8, 0.5, 0.3).normalize(), 0.55f32), (vec3(0.8, 0.5, 0.3).normalize(), 0.55), (vec3(0.0, 1.0, 0.2).normalize(), 0.35)];
    let mut tris: Vec<(f32, [Vertex; 3])> = Vec::new();
    GEOM.with(|g| {
        let mut g = g.borrow_mut();
        let geom = g.get_or_insert_with(build);
        // Manette bi-levier sur la console gauche (suit la poussée)
        let mut extra = Builder { t: Vec::new() };
        let tz = -0.15 + throttle.clamp(0.0, 1.0) * 0.45;
        for dx in [-0.035f32, 0.035] {
            extra.tube(&[vec3(-0.60 + dx, -0.62, tz - 0.05), vec3(-0.60 + dx, -0.52, tz)], 0.012, 6, Mat::Metal, DARK);
            extra.boxq(vec3(-0.60 + dx - 0.022, -0.53, tz - 0.02), vec3(-0.60 + dx + 0.022, -0.49, tz + 0.03), Mat::Metal, [0.55, 0.56, 0.58]);
        }
        for t in geom.iter().chain(extra.t.iter()) {
            let c = (t.p[0] + t.p[1] + t.p[2]) / 3.0;
            let mut vs = [Vertex { position: Vec3::ZERO, uv: Vec2::ZERO, color: [0; 4], normal: Vec4::ZERO }; 3];
            for k in 0..3 {
                let mut n = t.n[k];
                if n.dot(t.p[k]) > 0.0 {
                    n = -n;
                }
                let diffuse: f32 = lights.iter().map(|(l, w)| n.dot(*l).max(0.0) * w).sum();
                // Recoins plus sombres : bas de la cabine et loin de l'œil
                let occl = (1.0 + t.p[k].y * 0.35).clamp(0.55, 1.0) * (1.0 - (t.p[k].length() - 0.6).max(0.0) * 0.12).clamp(0.6, 1.0);
                let shade = (0.16 + diffuse) * occl;
                vs[k] = Vertex {
                    position: to_w(t.p[k]),
                    uv: t.uv[k],
                    color: [(t.tint[0] * shade * 255.0).min(255.0) as u8, (t.tint[1] * shade * 255.0).min(255.0) as u8, (t.tint[2] * shade * 255.0).min(255.0) as u8, 255],
                    normal: Vec4::ZERO,
                };
            }
            tris.push((c.length(), vs));
        }
    });
    tris.sort_by(|a, b| b.0.total_cmp(&a.0));
    for chunk in tris.chunks(1_200) {
        let mut vertices = Vec::with_capacity(chunk.len() * 3);
        for (_, t) in chunk {
            vertices.extend_from_slice(t);
        }
        let indices: Vec<u16> = (0..vertices.len() as u16).collect();
        draw_mesh(&Mesh { vertices, indices, texture: Some(tex.clone()) });
    }
    set_default_camera();
    // Emplacements projetés des instruments
    let view = View { mat, rect };
    let facing = cam.yaw.abs() < 40.0;
    let bounds = |pts: [Vec3; 4]| -> Option<Rect> {
        if !facing {
            return None;
        }
        let pp: Vec<Vec2> = pts.iter().filter_map(|p| view.project(to_w(*p))).collect();
        if pp.len() < 4 {
            return None;
        }
        let (mut a, mut b) = (pp[0], pp[0]);
        for p in &pp {
            a = a.min(*p);
            b = b.max(*p);
        }
        Some(Rect::new(a.x, a.y, b.x - a.x, b.y - a.y))
    };
    let panel = |x0: f32, y0: f32, x1: f32, y1: f32| bounds([on_panel(x0, y0), on_panel(x1, y0), on_panel(x1, y1), on_panel(x0, y1)]);
    let wing = |s: f32, u0: f32, y0: f32, u1: f32, y1: f32| bounds([on_wing(s, u0, y0), on_wing(s, u1, y0), on_wing(s, u1, y1), on_wing(s, u0, y1)]);
    let dial = |p: Vec3, r: f32| -> Option<(Vec2, f32)> {
        if !facing {
            return None;
        }
        let c = view.project(to_w(p))?;
        let e = view.project(to_w(p + vec3(r, 0.0, 0.0)))?;
        Some((c, (e - c).length()))
    };
    Spots {
        mfd: panel(-0.16, -0.225, 0.16, -0.495),
        engines: wing(-1.0, 0.12, -0.34, 0.92, -0.45),
        radio: panel(0.026, -0.06, 0.111, -0.14),
        dsply: panel(-0.115, -0.15, -0.025, -0.205),
        switches: [panel(-0.125, -0.57, -0.075, -0.63), panel(-0.025, -0.57, 0.025, -0.63), panel(0.075, -0.57, 0.125, -0.63)],
        cg: dial(on_wing(1.0, 0.45, -0.38), 0.05),
        h2: dial(on_wing(1.0, 0.45, -0.53), 0.042),
        gear: wing(-1.0, 0.25, -0.50, 0.42, -0.66),
        greens: view.project(to_w(on_wing(-1.0, 0.70, -0.58))).filter(|_| facing),
    }
}
