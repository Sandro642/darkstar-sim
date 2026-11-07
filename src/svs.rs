//! Vision synthétique (SVS) façon Darkstar : terrain réel (imagerie satellite sur relief), ciel,
//! pistes 3D, balisage.

use crate::aircraft::{Aircraft, EYE_H};
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::tiles::Tiles;
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;


pub const FOVY_DEG: f32 = 46.0;

/// Palette de la vision synthétique.
#[derive(Clone, Copy, PartialEq)]
pub enum SvsStyle {
    Darkstar,
    Realiste,
}

pub struct View {
    pub mat: Mat4,
    pub rect: Rect,
}

impl View {
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.mat * p.extend(1.0);
        if c.w < 0.5 {
            return None;
        }
        let (nx, ny) = (c.x / c.w, c.y / c.w);
        Some(vec2(self.rect.x + (nx * 0.5 + 0.5) * self.rect.w, self.rect.y + (0.5 - ny * 0.5) * self.rect.h))
    }
    pub fn project_dir(&self, d: Vec3) -> Option<Vec2> {
        self.project(d.normalize() * 100_000.0)
    }
}

struct SvsCam {
    mat: Mat4,
    vp: (i32, i32, i32, i32),
    depth: bool,
}

impl Camera for SvsCam {
    fn matrix(&self) -> Mat4 {
        self.mat
    }
    fn depth_enabled(&self) -> bool {
        self.depth
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        None
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        Some(self.vp)
    }
}

/// Direction monde (x=Est, y=Haut, z=Sud) d'un cap et d'une pente donnés (radians).
pub fn dir(hdg: f64, pitch: f64) -> Vec3 {
    vec3((hdg.sin() * pitch.cos()) as f32, pitch.sin() as f32, (-hdg.cos() * pitch.cos()) as f32)
}

pub fn basis(ac: &Aircraft) -> (Vec3, Vec3, Vec3) {
    let fwd = dir(ac.psi, ac.theta);
    let r0 = vec3(ac.psi.cos() as f32, 0.0, ac.psi.sin() as f32);
    let up0 = r0.cross(fwd).normalize();
    let (s, c) = (ac.phi.sin() as f32, ac.phi.cos() as f32);
    let up = up0 * c + r0 * s;
    let right = fwd.cross(up).normalize();
    (fwd, up, right)
}

fn lerpc(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgba(c: [f32; 3]) -> [u8; 4] {
    [(c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8, 255]
}

fn vtx(p: Vec3, c: [u8; 4]) -> Vertex {
    Vertex { position: p, uv: Vec2::ZERO, color: c, normal: Vec4::ZERO }
}

/// Dessine un maillage indexé arbitraire en lots compatibles avec la capacité
/// d'un appel de dessin (macroquad alloue un tampon GPU par appel).
pub fn draw_mesh_chunked(vertices: &[Vertex], indices: &[u16]) {
    const MAX_TRI: usize = 1_300; // <= 3900 sommets et indices par lot
    let mut remap = vec![u16::MAX; vertices.len()];
    for tris in indices.chunks(MAX_TRI * 3) {
        let mut v: Vec<Vertex> = Vec::with_capacity(tris.len());
        let mut idx: Vec<u16> = Vec::with_capacity(tris.len());
        for &i in tris {
            let i = i as usize;
            if remap[i] == u16::MAX {
                remap[i] = v.len() as u16;
                v.push(vertices[i]);
            }
            idx.push(remap[i]);
        }
        for &i in tris {
            remap[i as usize] = u16::MAX;
        }
        draw_mesh(&Mesh { vertices: v, indices: idx, texture: None });
    }
}

/// Constructeur de maillages colorés (quads), vidé automatiquement avant 65k sommets.
pub struct MeshBuilder {
    v: Vec<Vertex>,
    i: Vec<u16>,
}

impl MeshBuilder {
    pub fn new() -> Self {
        MeshBuilder { v: Vec::with_capacity(8192), i: Vec::with_capacity(16384) }
    }
    pub fn quad(&mut self, p: [Vec3; 4], c: [u8; 4]) {
        if self.v.len() > 3_000 {
            self.flush();
        }
        let b = self.v.len() as u16;
        for q in p {
            self.v.push(vtx(q, c));
        }
        self.i.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    pub fn flush(&mut self) {
        if self.i.is_empty() {
            return;
        }
        draw_mesh_chunked(&self.v, &self.i);
        self.v.clear();
        self.i.clear();
    }
}

pub struct SkyColors {
    pub haze: [f32; 3],
    pub mid: [f32; 3],
    pub zenith: [f32; 3],
}

pub fn sky_colors(alt: f64, style: SvsStyle) -> SkyColors {
    let a = alt as f32;
    if style == SvsStyle::Darkstar {
        // Ciel ardoise uniforme du PFD Darkstar, qui s'assombrit vers l'espace
        let k = (a / 40_000.0).clamp(0.0, 1.0);
        return SkyColors {
            haze: lerpc([0.44, 0.52, 0.66], [0.30, 0.38, 0.58], k),
            mid: lerpc([0.33, 0.40, 0.55], [0.10, 0.13, 0.26], k),
            zenith: lerpc([0.29, 0.35, 0.50], [0.02, 0.03, 0.08], k),
        };
    }
    SkyColors {
        haze: lerpc([0.80, 0.87, 0.95], [0.55, 0.72, 0.98], a / 30_000.0),
        mid: lerpc([0.47, 0.66, 0.92], [0.08, 0.16, 0.42], a / 25_000.0),
        zenith: lerpc([0.24, 0.44, 0.84], [0.0, 0.0, 0.03], a / 35_000.0),
    }
}

/// Repère local d'un aérodrome par rapport à l'œil (ENU plat + courbure).
struct AptFrame {
    ce: f64,
    cn: f64,
    u: (f64, f64),
    c: (f64, f64),
    dz: f64,
}

impl AptFrame {
    fn p(&self, along: f64, cross: f64, z: f64) -> Vec3 {
        let e = self.ce + along * self.u.0 + cross * self.c.0;
        let n = self.cn + along * self.u.1 + cross * self.c.1;
        let d = (e * e + n * n).sqrt();
        // Légère surélévation croissante avec la distance : pas de scintillement avec le relief aplani
        let lift = 0.25 + d * 0.0012;
        vec3(e as f32, (self.dz + z + lift - curvature_drop(d)) as f32, -n as f32)
    }
    /// Point d'un marquage peint : au-dessus de l'enrobé (test de profondeur), écart croissant avec la distance.
    fn pm(&self, along: f64, cross: f64) -> Vec3 {
        let e = self.ce + along * self.u.0 + cross * self.c.0;
        let n = self.cn + along * self.u.1 + cross * self.c.1;
        let d = (e * e + n * n).sqrt();
        self.p(along, cross, 0.08 + d * 0.0006)
    }
}

/// Surface de piste découpée en carreaux de 150 m au plus : la surélévation anti-scintillement
/// (fonction de la distance) reste correcte partout au lieu d'être interpolée sur 4 km.
#[allow(clippy::too_many_arguments)]
fn rw_quad(mb: &mut MeshBuilder, f: &AptFrame, a0: f64, a1: f64, c0: f64, c1: f64, mark: bool, col: [u8; 4]) {
    let na = ((a1 - a0).abs() / 150.0).ceil().max(1.0) as usize;
    let nc = ((c1 - c0).abs() / 150.0).ceil().max(1.0) as usize;
    let pt = |a: f64, c: f64| if mark { f.pm(a, c) } else { f.p(a, c, 0.0) };
    for i in 0..na {
        let (x0, x1) = (a0 + (a1 - a0) * i as f64 / na as f64, a0 + (a1 - a0) * (i + 1) as f64 / na as f64);
        for j in 0..nc {
            let (y0, y1) = (c0 + (c1 - c0) * j as f64 / nc as f64, c0 + (c1 - c0) * (j + 1) as f64 / nc as f64);
            mb.quad([pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)], col);
        }
    }
}

/// Segments de chiffres 7-segments : (x0, y0, x1, y1) dans une cellule 1 x 2.
fn seg_rects(d: u32) -> Vec<(f64, f64, f64, f64)> {
    let t = 0.22;
    let segs = [
        (0.0, 2.0 - t, 1.0, 2.0),   // a haut
        (1.0 - t, 1.0, 1.0, 2.0),   // b haut droit
        (1.0 - t, 0.0, 1.0, 1.0),   // c bas droit
        (0.0, 0.0, 1.0, t),         // d bas
        (0.0, 0.0, t, 1.0),         // e bas gauche
        (0.0, 1.0, t, 2.0),         // f haut gauche
        (0.0, 1.0 - t / 2.0, 1.0, 1.0 + t / 2.0), // g milieu
    ];
    let on: &[usize] = match d {
        0 => &[0, 1, 2, 3, 4, 5],
        1 => &[1, 2],
        2 => &[0, 1, 6, 4, 3],
        3 => &[0, 1, 6, 2, 3],
        4 => &[5, 6, 1, 2],
        5 => &[0, 5, 6, 2, 3],
        6 => &[0, 5, 6, 4, 2, 3],
        7 => &[0, 1, 2],
        8 => &[0, 1, 2, 3, 4, 5, 6],
        _ => &[0, 1, 2, 3, 5, 6],
    };
    on.iter().map(|&i| segs[i]).collect()
}

pub struct Lights {
    pub pts: Vec<(Vec3, Color, f32)>,
}

/// Caméra de la vue : champ vertical et orientation de la tête (vue cockpit), en degrés.
#[derive(Clone, Copy)]
pub struct Cam {
    pub fovy: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Cam {
    fn default() -> Self {
        Cam { fovy: FOVY_DEG, yaw: 0.0, pitch: 0.0 }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_svs(world: &World, tiles: &mut Tiles, wx: &crate::weather::Weather, rf: &crate::refuel::Refuel, ac: &Aircraft, rect: Rect, time: f64, style: SvsStyle, cam: Cam) -> View {
    let dark = style == SvsStyle::Darkstar;
    let dpi = screen_dpi_scale();
    let (fwd0, up0, _) = basis(ac);
    // Regard du pilote : lacet autour de l'axe vertical avion, puis site
    let (sy, cy) = (-cam.yaw.to_radians()).sin_cos();
    let fwd1 = (fwd0 * cy + up0.cross(fwd0) * sy + up0 * up0.dot(fwd0) * (1.0 - cy)).normalize();
    let right1 = fwd1.cross(up0).normalize();
    let (sp, cp) = cam.pitch.to_radians().sin_cos();
    let fwd = (fwd1 * cp + up0 * sp).normalize();
    let up = right1.cross(fwd).normalize();
    let fovy = cam.fovy;
    // Plan proche adapté à la hauteur : précision du tampon de profondeur pour le relief
    let mut near = ((ac.alt - ac.d.elev) * 0.02).clamp(1.5, 1_500.0) as f32;
    // Ravitailleur tout proche : plan proche réduit pour ne pas le découper
    if let Some(t) = &rf.tanker {
        let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let d3 = d.hypot(t.alt - ac.alt);
        if d3 < 3_000.0 {
            near = near.min((d3 * 0.3).max(1.0) as f32);
        }
    }
    let proj = Mat4::perspective_rh_gl(fovy.to_radians(), rect.w / rect.h, near, 6.0e6);
    let view = Mat4::look_at_rh(Vec3::ZERO, fwd, up);
    let mat = proj * view;
    let vp = (
        (rect.x * dpi) as i32,
        ((screen_height() - rect.y - rect.h) * dpi) as i32,
        (rect.w * dpi) as i32,
        (rect.h * dpi) as i32,
    );
    set_camera(&SvsCam { mat, vp, depth: false });

    let elev = ac.d.elev;
    let h = (ac.alt - elev + EYE_H).max(1.0);
    let rg = R_EARTH + elev;
    let dip = (rg / (rg + h)).acos();
    let sky = sky_colors(ac.alt, style);
    let ground_haze = if dark { [0.52, 0.70, 0.45] } else { lerpc(sky.haze, [0.62, 0.66, 0.70], 0.3) };

    // --- Ciel (dôme) ---
    {
        let band = 1.0 / (1.0 + ac.alt as f32 / 8000.0);
        let dipd = dip.to_degrees() as f32;
        let rings: [(f32, [f32; 3]); 8] = [
            (-89.0, ground_haze),
            (-dipd - 0.3, ground_haze),
            (-dipd + 0.2, sky.haze),
            (-dipd + 3.0 * band, lerpc(sky.haze, sky.mid, 0.5)),
            (-dipd + 12.0 * band, sky.mid),
            (35.0, lerpc(sky.mid, sky.zenith, 0.6)),
            (65.0, sky.zenith),
            (89.9, sky.zenith),
        ];
        let seg = 48;
        let rd = 1.0e6f32;
        let mut v = Vec::new();
        let mut idx: Vec<u16> = Vec::new();
        for (ri, (el, col)) in rings.iter().enumerate() {
            let el = el.to_radians();
            for s in 0..=seg {
                let az = s as f32 / seg as f32 * std::f32::consts::TAU;
                let p = vec3(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos()) * rd;
                v.push(vtx(p, rgba(*col)));
                if ri > 0 && s < seg {
                    let a = ((ri - 1) * (seg + 1) + s) as u16;
                    let b = (ri * (seg + 1) + s) as u16;
                    idx.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
                }
            }
        }
        draw_mesh_chunked(&v, &idx);
    }

    // --- Sol : calotte sphérique jusqu'à l'horizon, peinte du lointain vers le proche ---
    {
        let d_h = rg * dip;
        let n_az = 96usize;
        let n_r = 64usize;
        let d0 = (h * 0.3).clamp(8.0, d_h * 0.5);
        let vis = 180_000.0 + ac.alt * 4.0;
        let mut verts = Vec::with_capacity(n_az * (n_r + 1) + 1);
        let mut dists = Vec::with_capacity(n_r + 1);
        dists.push(0.0);
        for i in 0..n_r {
            dists.push(d0 * (d_h / d0).powf(i as f64 / (n_r - 1) as f64));
        }
        for (ri, &d) in dists.iter().enumerate() {
            let th = d / rg;
            let horiz = rg * th.sin();
            let vert = rg * th.cos() - rg - h;
            let count = if ri == 0 { 1 } else { n_az };
            for s in 0..count {
                let az = s as f64 / n_az as f64 * 360.0;
                let (la, lo) = destination(ac.lat, ac.lon, az, d);
                let mut c = world.sample(la, lo);
                if dark {
                    // Terre verte / mer bleue, modulée par la luminance de la texture
                    let water = c[2] > c[0] + 0.08 && c[2] > c[1];
                    let lum = (c[0] + c[1] + c[2]) / 3.0;
                    c = if water {
                        lerpc([0.10, 0.24, 0.55], [0.22, 0.42, 0.72], lum * 1.5)
                    } else if lum > 0.8 {
                        [0.85, 0.92, 0.88]
                    } else {
                        lerpc([0.16, 0.48, 0.10], [0.50, 0.76, 0.28], (lum - 0.2) * 1.8)
                    };
                }
                // Brume atmosphérique
                let fog_k = if dark { 0.55 } else { 0.85 };
                let fog = ((d / vis).min(1.0) * fog_k + (d / d_h).powi(4) * 0.15) as f32;
                c = lerpc(c, ground_haze, fog);
                let azr = az.to_radians();
                let p = vec3((azr.sin() * horiz) as f32, vert as f32, (-azr.cos() * horiz) as f32);
                verts.push(vtx(p, rgba(c)));
            }
        }
        let mut idx: Vec<u16> = Vec::with_capacity(n_az * n_r * 6);
        let ring_start = |ri: usize| if ri == 0 { 0 } else { 1 + (ri - 1) * n_az };
        for ri in (1..=n_r).rev() {
            for s in 0..n_az {
                let s1 = (s + 1) % n_az;
                let o0 = ring_start(ri) + s;
                let o1 = ring_start(ri) + s1;
                if ri == 1 {
                    idx.extend_from_slice(&[0, o0 as u16, o1 as u16]);
                } else {
                    let i0 = ring_start(ri - 1) + s;
                    let i1 = ring_start(ri - 1) + s1;
                    idx.extend_from_slice(&[i0 as u16, o0 as u16, o1 as u16, i0 as u16, o1 as u16, i1 as u16]);
                }
            }
        }
        draw_mesh_chunked(&verts, &idx);
    }

    // --- Terrain réel (test de profondeur) ---
    set_camera(&SvsCam { mat, vp, depth: true });
    crate::terrain::draw_terrain(world, tiles, ac, mat, dark);

    // --- Aérodromes ---
    let mut lights = Lights { pts: Vec::new() };
    let mut outlines: Vec<[Vec3; 4]> = Vec::new();
    let mut mb = MeshBuilder::new();
    for apt in AIRPORTS {
        let (ce, cn) = enu_offset(ac.lat, ac.lon, apt.lat, apt.lon);
        let dist = (ce * ce + cn * cn).sqrt();
        if dist > 150_000.0 {
            continue;
        }
        let hr = apt.hdg.to_radians();
        let f = AptFrame {
            ce,
            cn,
            u: (hr.sin(), hr.cos()),
            c: (hr.cos(), -hr.sin()),
            dz: apt.elev - (ac.alt + EYE_H),
        };
        let (l2, w2) = (apt.len / 2.0, apt.width / 2.0);
        outlines.push([f.p(-l2, -w2, 0.0), f.p(l2, -w2, 0.0), f.p(l2, w2, 0.0), f.p(-l2, w2, 0.0)]);
        let white = [235, 235, 235, 255];
        let concrete = [150, 148, 140, 255];
        // Taxiway, aire de trafic, bretelles
        rw_quad(&mut mb, &f, -l2, l2, w2 + 160.0, w2 + 185.0, false, [70, 70, 72, 255]);
        let mut a = -l2 + 100.0;
        while a < l2 {
            rw_quad(&mut mb, &f, a - 12.0, a + 12.0, w2, w2 + 170.0, false, [70, 70, 72, 255]);
            a += 900.0;
        }
        rw_quad(&mut mb, &f, -600.0, 600.0, w2 + 185.0, w2 + 380.0, false, concrete);
        // Piste + prolongements
        let asphalt = if dark { [118, 118, 124, 255] } else { [52, 52, 55, 255] };
        rw_quad(&mut mb, &f, -l2 - 60.0, l2 + 60.0, -w2, w2, false, asphalt);
        rw_quad(&mut mb, &f, -l2 + 60.0, l2 - 60.0, -w2 + 1.5, -w2 + 2.5, true, white);
        rw_quad(&mut mb, &f, -l2 + 60.0, l2 - 60.0, w2 - 2.5, w2 - 1.5, true, white);
        // Axe
        let mut a = -l2 + 90.0;
        while a + 36.0 < l2 - 90.0 {
            rw_quad(&mut mb, &f, a, a + 36.0, -0.9, 0.9, true, white);
            a += 66.0;
        }
        for s in [-1.0f64, 1.0] {
            let thr = s * l2;
            let inw = -s;
            let num_hdg = if s < 0.0 { apt.hdg } else { wrap360(apt.hdg + 180.0) };
            let at = |x: f64| thr + inw * x;
            let qa = |x0: f64, x1: f64, c0: f64, c1: f64| {
                let (a0, a1) = (at(x0), at(x1));
                let (c0, c1) = if inw > 0.0 { (c0, c1) } else { (-c1, -c0) };
                (a0.min(a1), a0.max(a1), c0, c1)
            };
            { let (a0, a1, c0, c1) = qa(0.0, 3.0, -w2, w2); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
            // Bandes de seuil ("piano")
            let n = ((apt.width - 6.0) / 3.6) as i32;
            for k in 0..n {
                let c0 = -w2 + 3.0 + k as f64 * 3.6;
                if c0.abs() < 3.0 || (c0 + 1.8).abs() < 3.0 {
                    continue;
                }
                { let (a0, a1, c0, c1) = qa(6.0, 36.0, c0, c0 + 1.8); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
            }
            // Numéro de piste
            let num = crate::airports::Airport::runway_number(num_hdg);
            let digits: Vec<u32> = if num >= 10 { vec![num / 10, num % 10] } else { vec![0, num] };
            let (dw, dh, gap) = (9.0, 18.0, 4.0);
            let total = digits.len() as f64 * dw + gap;
            for (k, dg) in digits.iter().enumerate() {
                let x0 = -total / 2.0 + k as f64 * (dw + gap);
                for (sx0, sy0, sx1, sy1) in seg_rects(*dg) {
                    { let (a0, a1, c0, c1) = qa(48.0 + sy0 * dh / 2.0, 48.0 + sy1 * dh / 2.0, x0 + sx0 * dw, x0 + sx1 * dw); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
                }
            }
            // Point d'aboutissement et zone de toucher
            { let (a0, a1, c0, c1) = qa(300.0, 345.0, 7.0, 15.0); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
            { let (a0, a1, c0, c1) = qa(300.0, 345.0, -15.0, -7.0); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
            for (dx, nb) in [(150.0, 3), (450.0, 2), (600.0, 2), (750.0, 1), (900.0, 1)] {
                for k in 0..nb {
                    let c0 = 8.0 + k as f64 * 3.0;
                    { let (a0, a1, c0, c1) = qa(dx, dx + 22.5, c0, c0 + 1.8); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
                    { let (a0, a1, c0, c1) = qa(dx, dx + 22.5, -c0 - 1.8, -c0); rw_quad(&mut mb, &f, a0, a1, c0, c1, true, white); }
                }
            }
            // Feux : rampe d'approche, seuil, PAPI
            if dist < 40_000.0 {
                let mut x = 30.0;
                while x <= 900.0 {
                    for k in -2..=2 {
                        lights.pts.push((f.p(at(-x), k as f64 * 1.2 * inw, 0.5), Color::new(1.0, 0.98, 0.85, 1.0), 1.6));
                    }
                    x += 30.0;
                }
                for k in -6..=6 {
                    lights.pts.push((f.p(at(-300.0), k as f64 * 2.5, 0.5), Color::new(1.0, 0.98, 0.85, 1.0), 1.6));
                }
                // Flash séquentiel ("rabbit")
                let ph = ((time * 2.0).fract() * 30.0) as i32;
                let xr = 900.0 - ph as f64 * 30.0;
                if xr > 300.0 {
                    lights.pts.push((f.p(at(-xr), 0.0, 1.0), WHITE, 4.0));
                }
                let mut c = -w2;
                while c <= w2 {
                    lights.pts.push((f.p(at(0.0), c, 0.5), Color::new(0.2, 1.0, 0.3, 1.0), 1.8));
                    lights.pts.push((f.p(at(-1.0), c, 0.5), Color::new(1.0, 0.1, 0.1, 1.0), 1.2));
                    c += 4.5;
                }
                // PAPI (à gauche) : seuils 3.5 / 3.17 / 2.83 / 2.5°
                let papi_pos = f.p(at(300.0), 0.0, 0.0);
                let horiz = (papi_pos.x * papi_pos.x + papi_pos.z * papi_pos.z).sqrt().max(1.0);
                let ang = ((-papi_pos.y) / horiz).atan().to_degrees();
                for (k, set) in [3.5f32, 3.17, 2.83, 2.5].iter().enumerate() {
                    let col = if ang > *set { Color::new(1.0, 1.0, 0.95, 1.0) } else { Color::new(1.0, 0.1, 0.1, 1.0) };
                    let cr = -(w2 + 15.0 + (3 - k) as f64 * 9.0);
                    lights.pts.push((f.p(at(300.0), cr * inw, 1.0), col, 2.6));
                }
            }
        }
        // Feux de bord de piste
        if dist < 25_000.0 {
            let mut a = -l2;
            while a <= l2 {
                for c in [-w2 - 1.0, w2 + 1.0] {
                    lights.pts.push((f.p(a, c, 0.5), Color::new(1.0, 0.95, 0.7, 0.9), 1.3));
                }
                a += 60.0;
            }
        }
        // Hangars (cubes, faces arrière éliminées)
        if dist < 30_000.0 {
            let hangars = [(-350.0, 100.0), (-200.0, 80.0), (250.0, 120.0), (420.0, 60.0)];
            for (a0, wdt) in hangars {
                let (c0, c1) = (w2 + 395.0, w2 + 470.0);
                let (a1, hz) = (a0 + wdt, 28.0);
                let corners = [f.p(a0, c0, 0.0), f.p(a1, c0, 0.0), f.p(a1, c1, 0.0), f.p(a0, c1, 0.0)];
                let tops = [f.p(a0, c0, hz), f.p(a1, c0, hz), f.p(a1, c1, hz), f.p(a0, c1, hz)];
                let center = (corners[0] + corners[2]) * 0.5;
                for k in 0..4 {
                    let k1 = (k + 1) % 4;
                    let mid = (corners[k] + corners[k1]) * 0.5;
                    let normal = (mid - center).normalize_or_zero();
                    if normal.dot(-mid) > 0.0 {
                        let shade = 120 + (k as u8) * 25;
                        mb.quad([corners[k], corners[k1], tops[k1], tops[k]], [shade, shade, (shade as f32 * 1.05) as u8, 255]);
                    }
                }
                if tops[0].y < 0.0 {
                    mb.quad(tops, [190, 192, 196, 255]);
                }
            }
        }
    }
    // Ravitailleur et perche
    crate::refuel::draw_tanker(rf, ac, &mut mb);
    mb.flush();

    // --- Nuages volumétriques (masqués par le relief) ---
    let right = fwd.cross(up).normalize();
    let imc = crate::weather::draw_clouds(wx, ac, right, up, time);

    // --- Perspective aérienne : voile de brume vers l'horizon, par-dessus le relief ---
    set_camera(&SvsCam { mat, vp, depth: false });
    {
        let dipd = dip.to_degrees() as f32;
        let band = 1.0 / (1.0 + ac.alt as f32 / 8000.0);
        let hz = rgba(ground_haze);
        let a_max = if dark { 0.55 } else { 0.75 };
        let rings: [(f32, f32); 5] = [(-dipd - 25.0 * band - 2.0, 0.0), (-dipd - 6.0 * band - 0.5, 0.25), (-dipd - 0.4, 0.8), (-dipd + 0.3, 1.0), (-dipd + 2.5 * band + 0.3, 0.0)];
        let seg = 48;
        let rd = 1.0e6f32;
        let mut v = Vec::new();
        let mut idx: Vec<u16> = Vec::new();
        for (ri, (el, al)) in rings.iter().enumerate() {
            let el = el.to_radians();
            let c = [hz[0], hz[1], hz[2], (al * a_max * 255.0) as u8];
            for k in 0..=seg {
                let az = k as f32 / seg as f32 * std::f32::consts::TAU;
                v.push(vtx(vec3(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos()) * rd, c));
                if ri > 0 && k < seg {
                    let a = ((ri - 1) * (seg + 1) + k) as u16;
                    let b = (ri * (seg + 1) + k) as u16;
                    idx.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
                }
            }
        }
        draw_mesh_chunked(&v, &idx);
    }

    set_default_camera();
    let v = View { mat, rect };
    // Dans le nuage : voile léger (la vision synthétique montre le relief même sans visibilité)
    if imc > 0.01 {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.88, 0.9, 0.92, (imc * 0.35).min(0.35)));
    }

    // --- Superpositions 2D dans la vue : contours de piste, feux ---
    scissor(Some(rect));
    for o in &outlines {
        let p: Vec<Option<Vec2>> = o.iter().map(|&p| v.project(p)).collect();
        if p.iter().all(|x| x.is_some()) {
            for k in 0..4 {
                let (a, b) = (p[k].unwrap(), p[(k + 1) % 4].unwrap());
                draw_line(a.x, a.y, b.x, b.y, 1.5, Color::new(1.0, 1.0, 1.0, 0.65));
            }
        }
    }
    for (p, c, s) in &lights.pts {
        if let Some(sp) = v.project(*p) {
            let d = p.length();
            let r = (s * (600.0 / d.max(1.0)).sqrt()).clamp(0.8, 4.0);
            draw_circle(sp.x, sp.y, r * 1.8, Color::new(c.r, c.g, c.b, 0.18));
            draw_circle(sp.x, sp.y, r, *c);
        }
    }
    scissor(None);
    v
}
