//! Cockpit 3D texturé (repère avion, œil à l'origine : x droite, y haut, z avant, en mètres).
//! Géométrie : arceaux de verrière, plafond, pyramide centrale, appuis de fenêtres nervurés,
//! consoles latérales, plancher, manche et manette. Textures procédurales (métal brossé, platine
//! vissée, parois nervurées, plancher antidérapant, consoles à boutons éclairés), éclairage par face.
//! Rendu avec sa propre caméra (plans très proches), faces triées de la plus lointaine à la plus
//! proche par-dessus la vue extérieure. Les instruments sont posés sur leurs emplacements 3D projetés.

use crate::aircraft::Aircraft;
use crate::svs::{self, Cam, SvsCam, View};
use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum Mat {
    Metal,
    Panel,
    Ribs,
    Floor,
    Console,
    Boot,
}

struct Quad {
    p: [Vec3; 4],
    uv: [Vec2; 4],
    mat: Mat,
    tint: [f32; 3],
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

// ---------- Textures procédurales ----------

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

fn make_tex(mat: Mat) -> Texture2D {
    let n = 256u16;
    let mut img = Image::gen_image_color(n, n, WHITE);
    for y in 0..n as i32 {
        for x in 0..n as i32 {
            let (fx, fy) = (x as f32, y as f32);
            let grain = noise(fx / 24.0, fy / 24.0, 1) * 0.06 + noise(fx / 6.0, fy / 6.0, 2) * 0.03;
            let v: [f32; 3] = match mat {
                Mat::Metal => {
                    // Métal brossé sombre avec rayures fines
                    let brush = noise(fx / 90.0, fy * 1.6, 3) * 0.05;
                    let scratch = if hash(x / 3, y, 4) > 0.997 { 0.08 } else { 0.0 };
                    let k = 0.36 + grain + brush + scratch;
                    [k, k * 1.02, k * 1.05]
                }
                Mat::Panel => {
                    // Platine : bords biseautés, vis aux coins d'une grille de 4x4 cases
                    let (cx, cy) = (x % 64, y % 64);
                    let bevel = if cx < 2 || cy < 2 { 0.12 } else if cx > 61 || cy > 61 { -0.10 } else { 0.0 };
                    let (dx, dy) = ((cx - 8) as f32, (cy - 8) as f32);
                    let screw = if dx * dx + dy * dy < 9.0 { 0.18 - (dx + dy) * 0.02 } else { 0.0 };
                    let k = 0.30 + grain * 0.6 + bevel + screw;
                    [k, k * 1.03, k * 1.04]
                }
                Mat::Ribs => {
                    // Paroi nervurée (ombrage de nervures verticales)
                    let ph = (fx / 32.0 * std::f32::consts::TAU).sin();
                    let k = 0.28 + ph * 0.07 + grain;
                    [k, k * 1.02, k * 1.04]
                }
                Mat::Floor => {
                    // Antidérapant : pastilles en relief
                    let (cx, cy) = ((x % 16) as f32 - 8.0, (y % 16) as f32 - 8.0);
                    let bump = if cx * cx + cy * cy < 10.0 { 0.06 } else { 0.0 };
                    let k = 0.22 + grain + bump;
                    [k, k, k * 1.02]
                }
                Mat::Console => {
                    // Console : boutons à liseré vert éclairé, sérigraphies
                    let (cx, cy) = (x % 64, y % 48);
                    let btn = (8..56).contains(&cx) && (10..34).contains(&cy);
                    let lit = btn && (14..18).contains(&cy);
                    let k = 0.24 + grain;
                    if lit {
                        [0.45, 0.95, 0.40]
                    } else if btn {
                        [k * 0.7, k * 0.75, k * 0.7]
                    } else {
                        [k, k * 1.02, k * 1.03]
                    }
                }
                Mat::Boot => {
                    // Soufflet de manche : rayures jaunes et noires
                    if ((fx + fy) / 22.0).floor() as i32 % 2 == 0 { [0.85, 0.68, 0.08] } else { [0.05, 0.05, 0.05] }
                }
            };
            img.set_pixel(x as u32, y as u32, Color::new(v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0), 1.0));
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
}

thread_local! {
    static TEX: std::cell::RefCell<Vec<(Mat, Texture2D)>> = const { std::cell::RefCell::new(Vec::new()) };
    static GEOM: std::cell::RefCell<Option<Vec<Quad>>> = const { std::cell::RefCell::new(None) };
}

fn tex(mat: Mat) -> Texture2D {
    TEX.with(|t| {
        let mut t = t.borrow_mut();
        if let Some((_, tx)) = t.iter().find(|(m, _)| *m == mat) {
            return tx.clone();
        }
        let tx = make_tex(mat);
        t.push((mat, tx.clone()));
        tx
    })
}

// ---------- Géométrie ----------

/// Profondeur de la face avant de la pyramide en fonction de la hauteur (deux pans).
fn panel_z(y: f32) -> f32 {
    if y > -0.28 {
        0.82 + (y - 0.02) * 0.667
    } else {
        0.62 + (y + 0.28) * 0.2687
    }
}

/// Point posé sur la face avant de la pyramide (légèrement devant pour les instruments).
pub fn on_panel(x: f32, y: f32) -> Vec3 {
    vec3(x, y, panel_z(y) - 0.004)
}

fn quad(out: &mut Vec<Quad>, p: [Vec3; 4], mat: Mat, scale: f32, tint: [f32; 3]) {
    // UV : projection planaire le long des deux arêtes, 1 répétition par `scale` mètres
    let (e1, e2) = (p[1] - p[0], p[3] - p[0]);
    let (l1, l2) = (e1.length() / scale, e2.length() / scale);
    let uv = [vec2(0.0, 0.0), vec2(l1, 0.0), vec2(l1, l2), vec2(0.0, l2)];
    out.push(Quad { p, uv, mat, tint });
}

fn boxq(out: &mut Vec<Quad>, a: Vec3, b: Vec3, mat: Mat, tint: [f32; 3]) {
    let (x0, y0, z0, x1, y1, z1) = (a.x, a.y, a.z, b.x, b.y, b.z);
    let c = |x, y, z| vec3(x, y, z);
    let s = 0.3;
    quad(out, [c(x0, y1, z0), c(x1, y1, z0), c(x1, y1, z1), c(x0, y1, z1)], mat, s, tint);
    quad(out, [c(x0, y0, z0), c(x1, y0, z0), c(x1, y1, z0), c(x0, y1, z0)], mat, s, tint);
    quad(out, [c(x0, y0, z1), c(x1, y0, z1), c(x1, y1, z1), c(x0, y1, z1)], mat, s, tint);
    quad(out, [c(x0, y0, z0), c(x0, y0, z1), c(x0, y1, z1), c(x0, y1, z0)], mat, s, tint);
    quad(out, [c(x1, y0, z0), c(x1, y0, z1), c(x1, y1, z1), c(x1, y1, z0)], mat, s, tint);
}

fn build() -> Vec<Quad> {
    let mut q = Vec::new();
    let w = [1.0, 1.0, 1.0];
    let v = vec3;
    // Plafond au-dessus des fenêtres et coin central entre les arceaux
    quad(&mut q, [v(-2.5, 0.36, 1.25), v(2.5, 0.36, 1.25), v(2.5, 0.9, -0.2), v(-2.5, 0.9, -0.2)], Mat::Metal, 0.6, [0.32, 0.33, 0.35]);
    quad(&mut q, [v(-0.09, 0.03, 0.84), v(0.09, 0.03, 0.84), v(0.32, 0.34, 1.25), v(-0.32, 0.34, 1.25)], Mat::Metal, 0.4, [0.38, 0.39, 0.41]);
    // Arceaux de verrière (vers le sommet de la pyramide), avec épaisseur
    for s in [-1.0f32, 1.0] {
        let (a, b) = (v(s * 0.62, 0.5, 0.25), v(s * 0.08, 0.04, 0.84));
        let lat = v(s * 0.06, 0.0, 0.0);
        let dep = v(0.0, 0.0, 0.05);
        quad(&mut q, [a, b, b + lat, a + lat], Mat::Metal, 0.3, [0.7, 0.72, 0.74]);
        quad(&mut q, [a + lat, b + lat, b + lat + dep, a + lat + dep], Mat::Metal, 0.3, [0.5, 0.52, 0.54]);
        quad(&mut q, [a, b, b + dep, a + dep], Mat::Metal, 0.3, [0.5, 0.52, 0.54]);
    }
    // Appuis de fenêtres (rebord) et parois nervurées en dessous
    for s in [-1.0f32, 1.0] {
        let (s0, s1) = (v(s * 0.85, -0.30, -0.1), v(s * 0.30, -0.20, 0.80));
        let inn = v(-s * 0.06, -0.02, 0.0);
        quad(&mut q, [s0, s1, s1 + inn, s0 + inn], Mat::Metal, 0.4, [0.6, 0.62, 0.65]);
        let down = v(0.0, -0.9, 0.0);
        quad(&mut q, [s0 + inn, s1 + inn, s1 + inn + down, s0 + inn + down], Mat::Ribs, 0.5, [0.55, 0.56, 0.58]);
        // Nervures en relief
        for k in 0..7 {
            let t = k as f32 / 7.0;
            let p0 = s0 + inn + (s1 - s0) * t;
            boxq(&mut q, p0 + v(-0.01, -0.85, 0.0), p0 + v(0.01 - s * 0.02, -0.02, 0.025), Mat::Metal, [0.42, 0.43, 0.45]);
        }
    }
    // Pyramide centrale : face avant en deux pans, flancs, chapeau
    let (a, b) = (v(-0.05, 0.02, 0.82), v(0.05, 0.02, 0.82));
    let (c, d) = (v(0.26, -0.28, 0.62), v(-0.26, -0.28, 0.62));
    let (e, f) = (v(0.42, -0.95, 0.44), v(-0.42, -0.95, 0.44));
    quad(&mut q, [a, b, c, d], Mat::Panel, 0.25, w);
    quad(&mut q, [d, c, e, f], Mat::Panel, 0.25, w);
    for s in [-1.0f32, 1.0] {
        let (ta, td, tf) = (if s < 0.0 { a } else { b }, if s < 0.0 { d } else { c }, if s < 0.0 { f } else { e });
        let g = v(s * 0.11, 0.02, 1.05);
        let h = v(s * 0.42, -0.30, 0.90);
        let i = v(s * 0.52, -0.95, 0.75);
        quad(&mut q, [ta, g, h, td], Mat::Metal, 0.4, [0.62, 0.64, 0.66]);
        quad(&mut q, [td, h, i, tf], Mat::Metal, 0.4, [0.55, 0.57, 0.59]);
    }
    quad(&mut q, [a, b, v(0.11, 0.02, 1.05), v(-0.11, 0.02, 1.05)], Mat::Metal, 0.3, [0.5, 0.52, 0.55]);
    // Plancher
    quad(&mut q, [v(-0.9, -1.0, -0.3), v(0.9, -1.0, -0.3), v(0.9, -1.0, 0.95), v(-0.9, -1.0, 0.95)], Mat::Floor, 0.4, w);
    // Consoles latérales (dessus à boutons éclairés)
    for s in [-1.0f32, 1.0] {
        let (x0, x1) = if s < 0.0 { (-0.80, -0.46) } else { (0.46, 0.80) };
        boxq(&mut q, v(x0, -1.0, -0.15), v(x1, -0.74, 0.42), Mat::Metal, [0.36, 0.37, 0.39]);
        quad(&mut q, [v(x0, -0.738, -0.15), v(x1, -0.738, -0.15), v(x1, -0.738, 0.42), v(x0, -0.738, 0.42)], Mat::Console, 0.09, [0.8, 0.8, 0.8]);
    }
    // Manche : soufflet, fût, poignée
    boxq(&mut q, v(-0.07, -1.0, 0.33), v(0.07, -0.90, 0.47), Mat::Boot, w);
    boxq(&mut q, v(-0.018, -0.90, 0.385), v(0.018, -0.58, 0.415), Mat::Metal, [0.35, 0.36, 0.37]);
    boxq(&mut q, v(-0.04, -0.58, 0.37), v(0.04, -0.44, 0.45), Mat::Metal, [0.42, 0.43, 0.44]);
    q
}

// ---------- Rendu ----------

/// Dessine le cockpit par-dessus la vue extérieure et renvoie les emplacements des instruments.
pub fn draw(ac: &Aircraft, rect: Rect, cam: Cam, throttle: f32) -> Spots {
    let (fwd0, up0, _) = svs::basis(ac);
    let right0 = fwd0.cross(up0).normalize();
    let to_w = |p: Vec3| right0 * p.x + up0 * p.y + fwd0 * p.z;
    let (fwd, up) = svs::look_basis(ac, cam);
    let proj = Mat4::perspective_rh_gl(cam.fovy.to_radians(), rect.w / rect.h, 0.03, 60.0);
    let mat = proj * Mat4::look_at_rh(Vec3::ZERO, fwd, up);
    set_camera(&SvsCam { mat, vp: svs::viewport(rect), depth: false });
    // Lumière : du dessus et de l'avant (verrière), repère avion
    let light = vec3(0.15, 0.9, 0.4).normalize();
    let mut faces: Vec<(f32, [Vec3; 4], [Vec2; 4], Mat, [u8; 4])> = Vec::new();
    GEOM.with(|g| {
        let mut g = g.borrow_mut();
        let geom = g.get_or_insert_with(build);
        let mut extra = Vec::new();
        // Manette des gaz (console gauche), selon la position réelle
        let tz = -0.05 + throttle.clamp(0.0, 1.0) * 0.45;
        boxq(&mut extra, vec3(-0.66, -0.74, tz - 0.03), vec3(-0.60, -0.62, tz + 0.03), Mat::Metal, [0.5, 0.5, 0.52]);
        for qd in geom.iter().chain(extra.iter()) {
            let c = (qd.p[0] + qd.p[1] + qd.p[2] + qd.p[3]) * 0.25;
            let mut n = (qd.p[1] - qd.p[0]).cross(qd.p[3] - qd.p[0]).normalize_or_zero();
            if n.dot(c) > 0.0 {
                n = -n;
            }
            let shade = 0.28 + 0.52 * n.dot(light).max(0.0);
            let col = [
                (qd.tint[0] * shade * 255.0).min(255.0) as u8,
                (qd.tint[1] * shade * 255.0).min(255.0) as u8,
                (qd.tint[2] * shade * 255.0).min(255.0) as u8,
                255,
            ];
            faces.push((c.length(), qd.p.map(to_w), qd.uv, qd.mat, col));
        }
    });
    faces.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, p, uv, m, col) in &faces {
        let vtx: Vec<Vertex> = (0..4).map(|k| Vertex { position: p[k], uv: uv[k], color: *col, normal: Vec4::ZERO }).collect();
        draw_mesh(&Mesh { vertices: vtx, indices: vec![0, 1, 2, 0, 2, 3], texture: Some(tex(*m)) });
    }
    set_default_camera();
    // Emplacements des instruments projetés (seulement si l'on regarde vers la planche)
    let view = View { mat, rect };
    let facing = cam.yaw.abs() < 40.0;
    let proj_rect = |x0: f32, y0: f32, x1: f32, y1: f32| -> Option<Rect> {
        if !facing {
            return None;
        }
        let pts: Vec<Vec2> = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].iter().filter_map(|&(x, y)| view.project(to_w(on_panel(x, y)))).collect();
        if pts.len() < 4 {
            return None;
        }
        let (mut a, mut b) = (pts[0], pts[0]);
        for p in &pts {
            a = a.min(*p);
            b = b.max(*p);
        }
        Some(Rect::new(a.x, a.y, b.x - a.x, b.y - a.y))
    };
    let proj_dial = |x: f32, y: f32, r: f32| -> Option<(Vec2, f32)> {
        if !facing {
            return None;
        }
        let c = view.project(to_w(on_panel(x, y)))?;
        let e = view.project(to_w(on_panel(x + r, y)))?;
        Some((c, (e - c).length()))
    };
    // Panneau sous les instruments : cadre sombre de l'écran central
    Spots {
        mfd: proj_rect(-0.19, -0.23, 0.19, -0.525),
        engines: proj_rect(-0.365, -0.40, -0.205, -0.50),
        radio: proj_rect(0.03, -0.02, 0.135, -0.13),
        dsply: proj_rect(-0.14, -0.06, -0.02, -0.13),
        switches: [proj_rect(-0.21, -0.64, -0.08, -0.73), proj_rect(-0.065, -0.64, 0.065, -0.73), proj_rect(0.08, -0.64, 0.21, -0.73)],
        cg: proj_dial(0.275, -0.43, 0.055),
        h2: proj_dial(0.275, -0.56, 0.045),
        gear: proj_rect(-0.355, -0.55, -0.295, -0.72),
        greens: view.project(to_w(on_panel(-0.33, -0.80))).filter(|_| facing),
    }
}
