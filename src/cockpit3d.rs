//! Cockpit 3D du Darkstar, modélisé dans Blender (tools/cockpit_model.py → assets/cockpit.obj) :
//! arceaux tubulaires rivetés, pyramide centrale chanfreinée et ailes, lunettes d'instruments en
//! relief, capots rouges, appuis rembourrés et lattes, consoles à interrupteurs, quadrant de manette,
//! manche à soufflet annelé, poignée d'éjection. Occlusion ambiante cuite par Cycles dans les
//! couleurs de sommets, éclairage (fenêtres, reflets) précalculé, détail de surface par atlas de
//! textures procédurales. Rendu avec un tampon de profondeur effacé (occlusion exacte), maillages
//! fixes dans le repère avion (l'orientation est portée par la matrice de caméra).
//! Repère avion : œil à l'origine, x droite, y haut, z avant, en mètres.

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
    White,
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
        Mat::White => 5,
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
        Mat::White => [1.0 - grain * 0.3, 1.0 - grain * 0.3, 1.0 - grain * 0.3],
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
    let mats = [Mat::Metal, Mat::Panel, Mat::Ribs, Mat::Console, Mat::Floor, Mat::White, Mat::Teal, Mat::Red, Mat::Glass];
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

// ---------- Géométrie de référence (identique à tools/cockpit_model.py) ----------

/// UV dans la case du matériau ; coordonnées locales repliées en miroir dans [0,1].
fn uv_in(m: Mat, u: f32, v: f32) -> Vec2 {
    let f = |a: f32| {
        let r = a.rem_euclid(2.0);
        if r > 1.0 { 2.0 - r } else { r }
    };
    cell(m) + vec2(0.004 + f(u) * (CELL - 0.008), 0.004 + f(v) * (CELL - 0.008))
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

/// Interrupteurs gardés sur les ailes : (côté, position u, hauteur).
const GUARDS: [(f32, f32, f32); 3] = [(-1.0, 0.60, -0.37), (1.0, 0.60, -0.37), (1.0, 0.80, -0.64)];

// ---------- Chargement du modèle Blender ----------

const OBJ: &str = include_str!("../assets/cockpit.obj");
const MTL: &str = include_str!("../assets/cockpit.mtl");

/// Maillages prêts à dessiner : partie fixe et les deux leviers de manette (animés).
struct Model {
    fixed: Vec<Mesh>,
    levers: Vec<Mesh>,
}

/// Case d'atlas, teinte et caractère émissif d'un matériau Blender.
fn material(name: &str, kd: [f32; 3]) -> (Mat, [f32; 3], bool) {
    match name {
        "Metal" => (Mat::Metal, [0.50, 0.52, 0.55], false),
        "Panel" => (Mat::Panel, [0.60, 0.62, 0.65], false),
        "Ribs" => (Mat::Ribs, [0.50, 0.52, 0.54], false),
        "Console" => (Mat::Console, [0.9, 0.9, 0.9], false),
        "Floor" => (Mat::Floor, [0.8, 0.8, 0.8], false),
        "Black" => (Mat::Metal, [0.20, 0.21, 0.22], false),
        "Glass" => (Mat::Glass, [1.0, 1.0, 1.0], false),
        n if n.starts_with("E_") => (Mat::White, kd, true),
        _ => (Mat::White, kd, false),
    }
}

fn parse_mtl() -> Vec<(String, [f32; 3])> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for l in MTL.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first() {
            Some(&"newmtl") => cur = w.get(1).unwrap_or(&"").to_string(),
            Some(&"Kd") if w.len() >= 4 => {
                let f = |i: usize| w[i].parse::<f32>().unwrap_or(0.5);
                out.push((cur.clone(), [f(1), f(2), f(3)]));
            }
            _ => {}
        }
    }
    out
}

fn load() -> Model {
    let mtl = parse_mtl();
    let mut pos: Vec<(Vec3, f32)> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut nrm: Vec<Vec3> = Vec::new();
    // Lumières (repère avion) : les deux fenêtres et le haut ; l'œil est à l'origine
    let lights = [(vec3(-0.8, 0.5, 0.3).normalize(), 0.60f32), (vec3(0.8, 0.5, 0.3).normalize(), 0.60), (vec3(0.0, 1.0, 0.2).normalize(), 0.35)];
    let mut fixed: Vec<Vertex> = Vec::new();
    let mut levers: Vec<Vertex> = Vec::new();
    let mut mat = (Mat::Metal, [0.5f32, 0.5, 0.5], false);
    let mut lever = false;
    for l in OBJ.lines() {
        let mut it = l.split_whitespace();
        match it.next() {
            Some("v") => {
                let f: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                if f.len() >= 3 {
                    // Blender (X droite, Y avant, Z haut) -> avion (x droite, y haut, z avant)
                    pos.push((vec3(f[0], f[2], f[1]), *f.get(3).unwrap_or(&1.0)));
                }
            }
            Some("vt") => {
                let f: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                if f.len() >= 2 {
                    uvs.push(vec2(f[0], f[1]));
                }
            }
            Some("vn") => {
                let f: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
                if f.len() >= 3 {
                    nrm.push(vec3(f[0], f[2], f[1]));
                }
            }
            Some("o") => lever = it.next().is_some_and(|n| n.starts_with("throttle")),
            Some("usemtl") => {
                let n = it.next().unwrap_or("");
                let kd = mtl.iter().find(|(m, _)| m == n).map(|(_, k)| *k).unwrap_or([0.5; 3]);
                mat = material(n, kd);
            }
            Some("f") => {
                let corners: Vec<(usize, Option<usize>, Option<usize>)> = it
                    .map(|c| {
                        let mut p = c.split('/');
                        let i = |s: Option<&str>| s.and_then(|x| x.parse::<usize>().ok()).map(|x| x - 1);
                        (i(p.next()).unwrap_or(0), i(p.next()), i(p.next()))
                    })
                    .collect();
                for k in 1..corners.len().saturating_sub(1) {
                    for &(vi, ti, ni) in [corners[0], corners[k], corners[k + 1]].iter() {
                        let (p, ao) = pos[vi];
                        let n0 = ni.map(|i| nrm[i]).unwrap_or(Vec3::Y);
                        let uv = ti.map(|i| uvs[i]).unwrap_or(Vec2::ZERO);
                        let (cell_m, tint, emissive) = mat;
                        let color = if emissive {
                            [tint[0], tint[1], tint[2]]
                        } else {
                            let n = if n0.dot(p) > 0.0 { -n0 } else { n0 };
                            let diffuse: f32 = lights.iter().map(|(l, w)| n.dot(*l).max(0.0) * w).sum();
                            let view = (-p).normalize_or_zero();
                            let spec: f32 = lights.iter().map(|(l, w)| n.dot((*l + view).normalize_or_zero()).max(0.0).powf(28.0) * w * 0.8).sum();
                            let occl = 0.25 + 0.75 * ao;
                            let sh = (0.18 + diffuse) * occl;
                            let hl = spec * occl;
                            [tint[0] * sh + hl, tint[1] * sh + hl, tint[2] * sh + hl]
                        };
                        let v = Vertex {
                            position: p,
                            uv: uv_in(cell_m, uv.x * 3.0, uv.y * 3.0),
                            color: [(color[0].min(1.0) * 255.0) as u8, (color[1].min(1.0) * 255.0) as u8, (color[2].min(1.0) * 255.0) as u8, 255],
                            normal: Vec4::ZERO,
                        };
                        if lever { levers.push(v) } else { fixed.push(v) }
                    }
                }
            }
            _ => {}
        }
    }
    let tex = ATL.with(|a| a.borrow_mut().get_or_insert_with(atlas).clone());
    let chunks = |v: &[Vertex]| -> Vec<Mesh> {
        v.chunks(3_600)
            .map(|c| Mesh { vertices: c.to_vec(), indices: (0..c.len() as u16).collect(), texture: Some(tex.clone()) })
            .collect()
    };
    Model { fixed: chunks(&fixed), levers: chunks(&levers) }
}

thread_local! {
    static ATL: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
    static MODEL: std::cell::RefCell<Option<Model>> = const { std::cell::RefCell::new(None) };
}

/// Efface le tampon de profondeur (la vue extérieure déjà dessinée reste à l'écran).
fn clear_depth() {
    unsafe {
        let mut gl = get_internal_gl();
        gl.flush();
        gl.quad_context.begin_default_pass(macroquad::miniquad::PassAction::Clear { color: None, depth: Some(1.0), stencil: None });
        gl.quad_context.end_render_pass();
    }
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
    // Matrice modèle : repère avion -> repère monde local
    let model = Mat4::from_cols(right0.extend(0.0), up0.extend(0.0), fwd0.extend(0.0), Vec4::W);
    let vp = svs::viewport(rect);
    set_camera(&SvsCam { mat: mat * model, vp, depth: true });
    clear_depth();
    MODEL.with(|m| {
        let mut m = m.borrow_mut();
        let model_data = m.get_or_insert_with(load);
        for mesh in &model_data.fixed {
            draw_mesh(mesh);
        }
        // Leviers de manette : translation le long du quadrant selon la poussée
        let shift = Mat4::from_translation(vec3(0.0, 0.0, throttle.clamp(0.0, 1.0) * 0.40));
        set_camera(&SvsCam { mat: mat * model * shift, vp, depth: true });
        for mesh in &model_data.levers {
            draw_mesh(mesh);
        }
    });
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
        engines: wing(-1.0, 0.10, -0.43, 0.95, -0.56),
        radio: panel(0.026, -0.06, 0.111, -0.14),
        dsply: panel(-0.115, -0.15, -0.025, -0.205),
        switches: GUARDS.map(|(s, u, y)| wing(s, u - 0.33, y + 0.05, u + 0.18, y - 0.05)),
        cg: dial(on_wing(1.0, 0.32, -0.50), 0.05),
        h2: dial(on_wing(1.0, 0.30, -0.66), 0.04),
        gear: wing(-1.0, 0.12, -0.63, 0.28, -0.78),
        greens: view.project(to_w(on_wing(-1.0, 0.62, -0.72))).filter(|_| facing),
    }
}
