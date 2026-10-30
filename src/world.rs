//! Carte du monde : traits de côte Natural Earth 1:50m, rasterisés en texture équirectangulaire.

use macroquad::prelude::*;

pub const TEX_W: usize = 4096;
pub const TEX_H: usize = 2048;

pub struct Ring {
    pub pts: Vec<(f32, f32)>, // (lon, lat)
    pub min: (f32, f32),
    pub max: (f32, f32),
}

pub struct World {
    pub rings: Vec<Ring>,
    pub rgba: Vec<u8>,
    pub texture: Texture2D,
}

fn hash(ix: i32, iy: i32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(374_761_393) ^ (iy as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy) + (hash(ix + 1, iy) - hash(ix, iy)) * sx;
    let b = hash(ix, iy + 1) + (hash(ix + 1, iy + 1) - hash(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn land_color(lat: f32, lon: f32) -> [f32; 3] {
    let al = lat.abs();
    let tropical = [0.22, 0.38, 0.16];
    let arid = [0.68, 0.57, 0.38];
    let temperate = [0.33, 0.42, 0.22];
    let tundra = [0.48, 0.47, 0.40];
    let ice = [0.90, 0.92, 0.95];
    let mut c = if al < 12.0 {
        tropical
    } else if al < 22.0 {
        lerp3(tropical, arid, (al - 12.0) / 10.0)
    } else if al < 33.0 {
        arid
    } else if al < 45.0 {
        lerp3(arid, temperate, (al - 33.0) / 12.0)
    } else if al < 58.0 {
        temperate
    } else if al < 68.0 {
        lerp3(temperate, tundra, (al - 58.0) / 10.0)
    } else {
        lerp3(tundra, ice, (al - 68.0) / 6.0)
    };
    if lat < -60.0 {
        c = ice;
    }
    let n = value_noise(lon * 0.6, lat * 0.6) * 0.6 + value_noise(lon * 3.0, lat * 3.0) * 0.4;
    let k = 0.82 + 0.36 * n;
    [c[0] * k, c[1] * k, c[2] * k]
}

impl World {
    pub fn load() -> World {
        let data = include_bytes!("../assets/land.bin");
        let rd = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let rf = |o: usize| f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let n_rings = rd(0) as usize;
        let mut off = 4;
        let mut rings = Vec::with_capacity(n_rings);
        for _ in 0..n_rings {
            let n = rd(off) as usize;
            off += 4;
            let mut pts = Vec::with_capacity(n);
            let (mut mn, mut mx) = ((999.0f32, 999.0f32), (-999.0f32, -999.0f32));
            for _ in 0..n {
                let (x, y) = (rf(off), rf(off + 4));
                off += 8;
                mn = (mn.0.min(x), mn.1.min(y));
                mx = (mx.0.max(x), mx.1.max(y));
                pts.push((x, y));
            }
            rings.push(Ring { pts, min: mn, max: mx });
        }

        // Rasterisation par balayage (règle pair-impair).
        let mut rows: Vec<Vec<f32>> = vec![Vec::new(); TEX_H];
        for r in &rings {
            for i in 0..r.pts.len() {
                let (x1, y1) = r.pts[i];
                let (x2, y2) = r.pts[(i + 1) % r.pts.len()];
                let py1 = (90.0 - y1) / 180.0 * TEX_H as f32 - 0.5;
                let py2 = (90.0 - y2) / 180.0 * TEX_H as f32 - 0.5;
                if py1 == py2 {
                    continue;
                }
                let (lo, hi) = (py1.min(py2), py1.max(py2));
                let start = lo.ceil().max(0.0) as usize;
                let end = (hi.ceil() as isize).min(TEX_H as isize);
                for row in start..end.max(0) as usize {
                    let t = (row as f32 - py1) / (py2 - py1);
                    let lon = x1 + (x2 - x1) * t;
                    rows[row].push((lon + 180.0) / 360.0 * TEX_W as f32);
                }
            }
        }
        let mut mask = vec![0u8; TEX_W * TEX_H];
        for (y, xs) in rows.iter_mut().enumerate() {
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in xs.chunks(2) {
                if pair.len() < 2 {
                    continue;
                }
                let a = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let b = ((pair[1] - 0.5).ceil() as usize).min(TEX_W);
                for x in a..b {
                    mask[y * TEX_W + x] = 255;
                }
            }
        }

        // Flou du masque pour les hauts-fonds côtiers.
        let blur = box_blur(&mask, 6);

        let mut rgba = vec![0u8; TEX_W * TEX_H * 4];
        for y in 0..TEX_H {
            let lat = 90.0 - (y as f32 + 0.5) * 180.0 / TEX_H as f32;
            for x in 0..TEX_W {
                let lon = (x as f32 + 0.5) * 360.0 / TEX_W as f32 - 180.0;
                let i = y * TEX_W + x;
                let c = if mask[i] > 0 {
                    land_color(lat, lon)
                } else {
                    let shallow = blur[i] as f32 / 255.0;
                    let deep = [0.04, 0.17, 0.38];
                    let ice_sea = if lat.abs() > 75.0 { [0.70, 0.78, 0.86] } else { deep };
                    let base = lerp3(deep, ice_sea, (lat.abs() - 75.0) / 8.0);
                    lerp3(base, [0.12, 0.38, 0.55], shallow * 1.6)
                };
                rgba[i * 4] = (c[0].clamp(0.0, 1.0) * 255.0) as u8;
                rgba[i * 4 + 1] = (c[1].clamp(0.0, 1.0) * 255.0) as u8;
                rgba[i * 4 + 2] = (c[2].clamp(0.0, 1.0) * 255.0) as u8;
                rgba[i * 4 + 3] = 255;
            }
        }
        let texture = Texture2D::from_rgba8(TEX_W as u16, TEX_H as u16, &rgba);
        texture.set_filter(FilterMode::Linear);
        World { rings, rgba, texture }
    }

    /// Couleur du sol (interpolation bilinéaire) à une position géographique.
    pub fn sample(&self, lat: f64, lon: f64) -> [f32; 3] {
        let fx = ((lon + 180.0) / 360.0 * TEX_W as f64 - 0.5).rem_euclid(TEX_W as f64);
        let fy = ((90.0 - lat) / 180.0 * TEX_H as f64 - 0.5).clamp(0.0, TEX_H as f64 - 1.001);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = ((fx - x0 as f64) as f32, (fy - y0 as f64) as f32);
        let x1 = (x0 + 1) % TEX_W;
        let y1 = (y0 + 1).min(TEX_H - 1);
        let px = |x: usize, y: usize, c: usize| self.rgba[(y * TEX_W + x) * 4 + c] as f32 / 255.0;
        let mut out = [0.0; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let a = px(x0, y0, c) + (px(x1, y0, c) - px(x0, y0, c)) * tx;
            let b = px(x0, y1, c) + (px(x1, y1, c) - px(x0, y1, c)) * tx;
            *o = a + (b - a) * ty;
        }
        out
    }
}

fn box_blur(src: &[u8], r: usize) -> Vec<u8> {
    let (w, h) = (TEX_W, TEX_H);
    let mut tmp = vec![0u8; w * h];
    let mut out = vec![0u8; w * h];
    let div = (2 * r + 1) as u32;
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let mut acc: u32 = 0;
        for k in 0..=2 * r {
            acc += row[(k + w - r) % w] as u32;
        }
        for x in 0..w {
            tmp[y * w + x] = (acc / div) as u8;
            acc += row[(x + r + 1) % w] as u32;
            acc -= row[(x + w - r) % w] as u32;
        }
    }
    for x in 0..w {
        let mut acc: u32 = 0;
        for k in 0..=2 * r {
            acc += tmp[(k.saturating_sub(r)).min(h - 1) * w + x] as u32;
        }
        for y in 0..h {
            out[y * w + x] = (acc / div) as u8;
            let add = (y + r + 1).min(h - 1);
            let sub = y.saturating_sub(r);
            acc += tmp[add * w + x] as u32;
            acc -= tmp[sub * w + x] as u32;
        }
    }
    out
}
