//! Terrain 3D de la vision synthétique : tuiles satellite réelles drapées sur le relief réel,
//! niveau de détail par quadtree (fin près de l'avion, grossier vers l'horizon), géométrie sphérique.

use crate::aircraft::{Aircraft, EYE_H};
use crate::airports::{self, AIRPORTS, Airport};
use crate::geo::*;
use crate::tiles::{self, ElevPatch, Key, Layer, Tiles};
use crate::world::World;
use macroquad::prelude::*;

/// Facteur de détail : une tuile est subdivisée tant que sa taille dépasse distance / LOD_K.
const LOD_K: f64 = 1.7;
const MAX_TILES: usize = 230;
const Z_MIN: u8 = 3;
const Z_MAX: u8 = 16;

/// Repère local de l'œil : x = Est, y = Haut, z = Sud (comme le reste du SVS).
pub struct EyeFrame {
    o: [f64; 3],
    e: [f64; 3],
    n: [f64; 3],
    u: [f64; 3],
}

fn ecef(lat: f64, lon: f64, h: f64) -> [f64; 3] {
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    let r = R_EARTH + h;
    [r * la.cos() * lo.cos(), r * la.cos() * lo.sin(), r * la.sin()]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl EyeFrame {
    pub fn new(ac: &Aircraft) -> EyeFrame {
        let (la, lo) = (ac.lat.to_radians(), ac.lon.to_radians());
        EyeFrame {
            o: ecef(ac.lat, ac.lon, ac.alt + EYE_H),
            e: [-lo.sin(), lo.cos(), 0.0],
            n: [-la.sin() * lo.cos(), -la.sin() * lo.sin(), la.cos()],
            u: [la.cos() * lo.cos(), la.cos() * lo.sin(), la.sin()],
        }
    }
    pub fn local(&self, lat: f64, lon: f64, h: f64) -> Vec3 {
        let p = ecef(lat, lon, h);
        let d = [p[0] - self.o[0], p[1] - self.o[1], p[2] - self.o[2]];
        vec3(dot(d, self.e) as f32, dot(d, self.u) as f32, -dot(d, self.n) as f32)
    }
}

struct Ctx<'a> {
    ac: &'a Aircraft,
    eye: &'a EyeFrame,
    mat: Mat4,
    h: f64,
    reach: f64,
}

/// Le volume (points de contrôle à 0 et 4000 m) est-il entièrement hors du champ de vision ?
fn culled(c: &Ctx, z: u8, x: u32, y: u32) -> bool {
    if z < 6 {
        return false;
    }
    let mut out = [true; 5];
    for j in 0..=2 {
        for i in 0..=2 {
            let la = tiles::tile_lat(y as f64 + j as f64 * 0.5, z);
            let lo = tiles::tile_lon(x as f64 + i as f64 * 0.5, z);
            for hh in [0.0, 4_000.0] {
                let p = c.mat * c.eye.local(la, lo, hh).extend(1.0);
                let tests = [p.x + p.w < 0.0, p.w - p.x < 0.0, p.y + p.w < 0.0, p.w - p.y < 0.0, p.w < 0.0];
                for k in 0..5 {
                    out[k] &= tests[k];
                }
            }
        }
    }
    out.iter().any(|o| *o)
}

/// Tuile candidate : (z, x, y, distance au sol, rapport taille / distance) ou `None` si hors de portée.
fn node(c: &Ctx, z: u8, x: u32, y: u32) -> Option<(u8, u32, u32, f64, f64)> {
    let (lat_n, lat_s) = (tiles::tile_lat(y as f64, z), tiles::tile_lat(y as f64 + 1.0, z));
    let (lon_w, lon_e) = (tiles::tile_lon(x as f64, z), tiles::tile_lon(x as f64 + 1.0, z));
    let mid_lon = (lon_w + lon_e) / 2.0;
    let half = (lon_e - lon_w) / 2.0;
    let la = c.ac.lat.clamp(lat_s, lat_n);
    let lo = mid_lon + wrap180(c.ac.lon - mid_lon).clamp(-half, half);
    let gd = dist_brg(c.ac.lat, c.ac.lon, la, lo).0;
    if gd > c.reach || culled(c, z, x, y) {
        return None;
    }
    let size = (lat_n - lat_s) * 111_195.0;
    let d3 = (gd * gd + c.h * c.h).sqrt();
    Some((z, x, y, gd, size / d3.max(1.0)))
}

/// Quadtree en largeur : on subdivise toujours la tuile la plus grande vue de l'avion, dans la
/// limite du budget, pour que le détail aille d'abord au premier plan.
fn collect(c: &Ctx) -> Vec<(u8, u32, u32, f64)> {
    let n0 = 1u32 << Z_MIN;
    let mut list: Vec<(u8, u32, u32, f64, f64)> = Vec::new();
    for y in 0..n0 {
        for x in 0..n0 {
            list.extend(node(c, Z_MIN, x, y));
        }
    }
    loop {
        let best = list
            .iter()
            .enumerate()
            .filter(|(_, t)| t.0 < Z_MAX && t.4 * LOD_K > 1.0)
            .max_by(|a, b| a.1.4.total_cmp(&b.1.4))
            .map(|(i, _)| i);
        let Some(i) = best else { break };
        if list.len() + 3 > MAX_TILES {
            break;
        }
        let (z, x, y, _, _) = list.swap_remove(i);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            list.extend(node(c, z + 1, x * 2 + dx, y * 2 + dy));
        }
    }
    list.into_iter().map(|t| (t.0, t.1, t.2, t.3)).collect()
}

fn rgba(c: [f32; 3]) -> [u8; 4] {
    [(c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8, 255]
}

/// Couleur de repli (hors ligne) façon carte Natural Earth.
fn fallback_color(world: &World, lat: f64, lon: f64, dark: bool) -> [f32; 3] {
    let c = world.sample(lat, lon);
    if !dark {
        return c;
    }
    let water = c[2] > c[0] + 0.08 && c[2] > c[1];
    let lum = (c[0] + c[1] + c[2]) / 3.0;
    if water {
        [0.10 + 0.12 * lum, 0.24 + 0.18 * lum, 0.55 + 0.17 * lum]
    } else {
        [0.16 + 0.34 * lum, 0.48 + 0.28 * lum, 0.10 + 0.18 * lum]
    }
}

/// Dessine le terrain (caméra 3D avec test de profondeur déjà active). Renvoie le nombre de tuiles.
pub fn draw_terrain(world: &World, tiles: &mut Tiles, ac: &Aircraft, mat: Mat4, dark: bool) -> usize {
    let eye = EyeFrame::new(ac);
    let h = (ac.alt + EYE_H - ac.d.elev).max(1.0);
    // Horizon géométrique + reliefs lointains (jusqu'à ~4000 m) visibles au-delà
    let reach = ((2.0 * R_EARTH * (ac.alt + EYE_H).max(1.0)).sqrt() + 225_000.0).min(2_500_000.0);
    let c = Ctx { ac, eye: &eye, mat, h, reach };
    let mut list = collect(&c);
    // Du plus lointain au plus proche (le test de profondeur fait le reste)
    list.sort_by(|a, b| b.3.total_cmp(&a.3));
    let near_apts: Vec<&Airport> = AIRPORTS.iter().filter(|a| dist_brg(ac.lat, ac.lon, a.lat, a.lon).0 < reach).collect();
    let tint = if dark { [0.92, 0.96, 0.92] } else { [1.0, 1.0, 1.0] };
    let taws = !ac.on_ground;
    for (z, x, y, gd) in &list {
        let (z, x, y) = (*z, *x, *y);
        let ze = z.saturating_sub(3).clamp(4, tiles::ELEV_Z);
        let prio = z as f32 * 0.25 + (*gd / reach) as f32 * 4.0;
        let (clat, clon) = (tiles::tile_lat(y as f64 + 0.5, z), tiles::tile_lon(x as f64 + 0.5, z));
        tiles.request_elev(clat, clon, ze, prio - 1.0);
        let patch = ElevPatch::find(z, x, y, ze);
        let g: usize = if z < 8 { 24 } else { 16 };
        let apts: Vec<&Airport> = near_apts
            .iter()
            .copied()
            .filter(|a| a.lat <= tiles::tile_lat(y as f64, z) + 0.15 && a.lat >= tiles::tile_lat(y as f64 + 1.0, z) - 0.15)
            .collect();
        let tex = tiles.get(Key { layer: Layer::Sat, z: z.min(Layer::Sat.max_z()), x: x >> z.saturating_sub(Layer::Sat.max_z()), y: y >> z.saturating_sub(Layer::Sat.max_z()) }, prio);
        let (texture, uv) = match &tex {
            Some(t) => (Some(t.tex.clone()), t.uv),
            None => (None, Rect::new(0.0, 0.0, 1.0, 1.0)),
        };
        let mut verts = Vec::with_capacity((g + 1) * (g + 1));
        for j in 0..=g {
            let fy = y as f64 + j as f64 / g as f64;
            let la = tiles::tile_lat(fy, z);
            for i in 0..=g {
                let fx = x as f64 + i as f64 / g as f64;
                let lo = tiles::tile_lon(fx, z);
                let raw = patch.as_ref().map(|p| p.sample_xy(fx, fy, z)).unwrap_or(0.0);
                let mut e = raw;
                if !apts.is_empty() {
                    e = airports::flatten_airports(la, lo, e, apts.iter().copied());
                }
                let p = eye.local(la, lo, e);
                let mut col = if texture.is_some() { tint } else { fallback_color(world, la, lo, dark) };
                // Alerte relief (TAWS) : relief à moins de 500 ft sous l'avion en ambre, au-dessus en rouge
                // (inhibée au sol et sur les plateformes aplanies des aérodromes)
                if taws && (e - raw).abs() < 0.5 && e > 1.0 {
                    let dh = e - ac.alt;
                    let k = ((dh + 150.0) / 120.0).clamp(0.0, 1.0) as f32;
                    if dh > -150.0 {
                        let warn = if dh > 0.0 { [1.0, 0.30, 0.25] } else { [1.0, 0.72, 0.25] };
                        col = [col[0] * (1.0 - k) + warn[0] * k, col[1] * (1.0 - k) + warn[1] * k, col[2] * (1.0 - k) + warn[2] * k];
                    }
                }
                let (u, v) = (uv.x + uv.w * i as f32 / g as f32, uv.y + uv.h * j as f32 / g as f32);
                verts.push(Vertex { position: p, uv: vec2(u, v), color: rgba(col), normal: Vec4::ZERO });
            }
        }
        let mut idx: Vec<u16> = Vec::with_capacity(g * g * 6);
        for j in 0..g {
            for i in 0..g {
                let a = (j * (g + 1) + i) as u16;
                let b = a + (g + 1) as u16;
                idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        draw_mesh(&Mesh { vertices: verts, indices: idx, texture });
    }
    // Relief sous l'avion toujours demandé (modèle de vol)
    for (dy, dx) in [(0.0, 0.0), (0.05, 0.0), (-0.05, 0.0), (0.0, 0.05), (0.0, -0.05)] {
        tiles.request_elev(ac.lat + dy, ac.lon + dx, tiles::ELEV_Z, -10.0);
    }
    list.len()
}
