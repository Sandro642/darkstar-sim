//! Carte de navigation mondiale (projection Mercator) sur tuiles réelles : plan CARTO / OpenStreetMap
//! ou imagerie satellite Esri, avec repli sur la carte Natural Earth hors ligne.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::tiles::{self, Key, Layer, Tiles};
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;

pub struct MapView {
    pub zoom: f32, // pixels par degré
    pub follow: bool,
    pub center: (f64, f64), // (lat, lon)
    pub trail: Vec<(f64, f64)>,
    /// Fond satellite (sinon plan).
    pub sat: bool,
    trail_t: f64,
    drag: Option<Vec2>,
}

impl MapView {
    pub fn new() -> Self {
        MapView { zoom: 60.0, follow: true, center: (0.0, 0.0), trail: Vec::new(), sat: false, trail_t: 0.0, drag: None }
    }

    pub fn record(&mut self, ac: &Aircraft, dt: f64) {
        self.trail_t -= dt;
        if self.trail_t <= 0.0 {
            self.trail_t = 2.0;
            self.trail.push((ac.lat, ac.lon));
            if self.trail.len() > 4000 {
                self.trail.remove(0);
            }
        }
    }

    pub fn input(&mut self, r: Rect) {
        let (mx, my) = mouse_position();
        let inside = r.contains(vec2(mx, my));
        if inside {
            let (_, wy) = mouse_wheel();
            if wy != 0.0 {
                let f = if wy > 0.0 { 1.15 } else { 1.0 / 1.15 };
                self.zoom = (self.zoom * f).clamp(r.w / 360.0, 60_000.0);
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                self.drag = Some(vec2(mx, my));
            }
        }
        if let Some(p) = self.drag {
            if is_mouse_button_down(MouseButton::Left) {
                let dlt = vec2(mx, my) - p;
                if dlt.length() > 2.0 {
                    self.follow = false;
                    self.center.1 = wrap180(self.center.1 - (dlt.x / self.zoom) as f64);
                    self.center.0 = merc_inv(merc(self.center.0) + (dlt.y / self.zoom) as f64).clamp(-85.0, 85.0);
                    self.drag = Some(vec2(mx, my));
                }
            } else {
                self.drag = None;
            }
        }
    }

    pub fn draw(&mut self, world: &World, tiles: &mut Tiles, wx: &crate::weather::Weather, rf: &crate::refuel::Refuel, cb: &crate::combat::Combat, recon: &crate::recon::Recon, ac: &Aircraft, r: Rect, dest: usize, time: f64) {
        if self.follow {
            // Mercator : au-delà de ±85° la carte n'existe pas ; centre limité, avion décalé vers le bord
            self.center = (ac.lat.clamp(-80.0, 80.0), ac.lon);
        }
        let z = self.zoom;
        let (clat, clon) = self.center;
        let cx = r.x + r.w / 2.0;
        let cy = r.y + r.h / 2.0;
        let cm = merc(clat);
        let to_s = |lat: f64, lon: f64| -> Vec2 {
            vec2(cx + wrap180(lon - clon) as f32 * z, cy - (merc(lat) - cm) as f32 * z)
        };

        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.06, 0.12, 1.0));
        scissor(Some(r));
        // Repli hors ligne : texture Natural Earth (équirectangulaire) découpée en bandes Mercator
        for k in [-1.0f32, 0.0, 1.0] {
            let x0 = cx + ((-180.0 - clon) as f32 + k * 360.0) * z;
            if x0 > r.x + r.w || x0 + 360.0 * z < r.x {
                continue;
            }
            let (tw, th) = (world.texture.width(), world.texture.height());
            let mut la = -84.0;
            while la < 84.0 {
                let (y_top, y_bot) = (cy - (merc(la + 4.0) - cm) as f32 * z, cy - (merc(la) - cm) as f32 * z);
                if y_bot >= r.y && y_top <= r.y + r.h {
                    let sy = (90.0 - (la + 4.0)) as f32 / 180.0 * th;
                    draw_texture_ex(
                        &world.texture,
                        x0,
                        y_top,
                        Color::new(0.75, 0.78, 0.82, 1.0),
                        DrawTextureParams { dest_size: Some(vec2(360.0 * z, y_bot - y_top)), source: Some(Rect::new(0.0, sy, tw, 4.0 / 180.0 * th)), ..Default::default() },
                    );
                }
                la += 4.0;
            }
        }
        // Tuiles réelles
        let layer = if self.sat { Layer::Sat } else { Layer::Plan };
        let dpi = screen_dpi_scale();
        let zt = ((z * dpi * 360.0 / 256.0).log2().round().max(0.0) as u8).min(layer.max_z());
        let n = tiles::n_tiles(zt);
        let lon_l = clon - (r.w / 2.0 / z) as f64;
        let lon_r = clon + (r.w / 2.0 / z) as f64;
        let lat_t = merc_inv(cm + (r.h / 2.0 / z) as f64).min(85.05);
        let lat_b = merc_inv(cm - (r.h / 2.0 / z) as f64).max(-85.05);
        let (x0, y0) = tiles::tile_xy(lat_t, lon_l, zt);
        let (x1, y1) = tiles::tile_xy(lat_b, lon_r, zt);
        let mut missing = 0;
        for (li, layer) in [layer, Layer::Labels].into_iter().enumerate() {
            let tint = if li == 0 && self.sat { Color::new(0.80, 0.82, 0.86, 1.0) } else { WHITE };
            for ty in (y0.floor() as i64).max(0)..=(y1.floor() as i64).min(n as i64 - 1) {
                for txu in (x0.floor() as i64)..=(x1.floor() as i64) {
                    let tx = txu.rem_euclid(n as i64) as u32;
                    // Position non repliée (txu peut sortir de 0..n quand la vue chevauche ±180°)
                    let lo_a = tiles::tile_lon(txu as f64, zt);
                    let p0 = vec2(cx + (lo_a - clon) as f32 * z, cy - (merc(tiles::tile_lat(ty as f64, zt)) - cm) as f32 * z);
                    let p1 = vec2(p0.x + (360.0 / n) as f32 * z, cy - (merc(tiles::tile_lat(ty as f64 + 1.0, zt)) - cm) as f32 * z);
                    let mid = (p0 + p1) * 0.5 - vec2(cx, cy);
                    let prio = zt as f32 + mid.length() / r.w + li as f32 * 0.5;
                    match tiles.get(Key { layer, z: zt, x: tx, y: ty as u32 }, prio) {
                        Some(t) => {
                            if !t.exact && li == 0 {
                                missing += 1;
                            }
                            // Les noms ne sont dessinés qu'à leur propre zoom (pas d'agrandissement flou)
                            if li == 1 && !t.exact {
                                continue;
                            }
                            let (tw, th) = (t.tex.width(), t.tex.height());
                            draw_texture_ex(
                                t.tex,
                                p0.x.floor(),
                                p0.y.floor(),
                                tint,
                                DrawTextureParams {
                                    dest_size: Some(vec2((p1.x - p0.x).ceil() + 1.0, (p1.y - p0.y).ceil() + 1.0)),
                                    source: Some(Rect::new(t.uv.x * tw, t.uv.y * th, t.uv.w * tw, t.uv.h * th)),
                                    ..Default::default()
                                },
                            );
                        }
                        None => {
                            if li == 0 {
                                missing += 1
                            }
                        }
                    }
                }
            }
        }
        // Calottes polaires (au-delà de la projection Mercator) : banquise unie et repère
        for (lim, sgn) in [(85.05f64, 1.0f32), (-85.05, -1.0)] {
            let y = cy - (merc(lim) - cm) as f32 * z;
            let (y0, y1) = if sgn > 0.0 { (r.y, y) } else { (y, r.y + r.h) };
            if y1 > y0 {
                draw_rectangle(r.x, y0, r.w, y1 - y0, Color::new(0.72, 0.78, 0.84, 1.0));
                draw_line(r.x, y, r.x + r.w, y, 1.0, Color::new(0.4, 0.5, 0.6, 1.0));
                if y1 - y0 > 14.0 {
                    txt_c(if sgn > 0.0 { "CALOTTE POLAIRE NORD (> 85 N)" } else { "CALOTTE POLAIRE SUD (> 85 S)" }, r.x + r.w / 2.0, (y0 + y1) / 2.0 + 4.0, 11.0, Color::new(0.2, 0.25, 0.3, 1.0));
                }
            }
        }
        // Traits de côte Natural Earth tant que les tuiles manquent
        if z > 14.0 && missing > 0 {
            let half_w = (r.w / 2.0 / z) as f64 + 1.0;
            let half_h = (r.h / 2.0 / z) as f64 + 1.0;
            let col = Color::new(0.85, 0.9, 0.8, 0.55);
            for ring in &world.rings {
                if (ring.max.1 as f64) < clat - half_h * 2.0 || (ring.min.1 as f64) > clat + half_h * 2.0 {
                    continue;
                }
                let rc = (ring.min.0 + ring.max.0) as f64 / 2.0;
                let rh = (ring.max.0 - ring.min.0) as f64 / 2.0;
                if wrap180(rc - clon).abs() - rh > half_w {
                    continue;
                }
                let mut prev: Option<Vec2> = None;
                for &(lo, la) in &ring.pts {
                    let p = to_s(la as f64, lo as f64);
                    if let Some(q) = prev {
                        if (p.x - q.x).abs() < r.w {
                            draw_line(q.x, q.y, p.x, p.y, 1.0, col);
                        }
                    }
                    prev = Some(p);
                }
            }
        }
        // Graticule
        let step = if z < 8.0 { 30.0 } else if z < 40.0 { 10.0 } else if z < 200.0 { 5.0 } else { 1.0 };
        let gcol = Color::new(0.5, 0.6, 0.7, 0.18);
        let mut lat = -80.0;
        while lat <= 80.0 {
            let y = to_s(lat, clon).y;
            draw_line(r.x, y, r.x + r.w, y, 1.0, gcol);
            lat += step;
        }
        let mut lon = -180.0;
        while lon < 180.0 {
            let x = to_s(clat, lon).x;
            draw_line(x, r.y, x, r.y + r.h, 1.0, gcol);
            lon += step;
        }

        // Zones d'essais (R-2508 approximatif autour d'Edwards / China Lake)
        let r2508 = [(36.2, -118.6), (36.3, -116.2), (35.0, -115.8), (34.6, -116.6), (34.5, -118.3), (35.3, -118.8)];
        let zc = Color::new(1.0, 0.5, 0.2, 0.6);
        for i in 0..r2508.len() {
            let a = to_s(r2508[i].0, r2508[i].1);
            let b = to_s(r2508[(i + 1) % r2508.len()].0, r2508[(i + 1) % r2508.len()].1);
            draw_line(a.x, a.y, b.x, b.y, 1.5, zc);
        }
        if z > 30.0 {
            let p = to_s(35.6, -117.3);
            txt("R-2508", p.x, p.y, 13.0, zc);
        }

        // Route orthodromique vers la destination
        let apt = &AIRPORTS[dest];
        let (dist, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
        let n = 64;
        let mut prev = to_s(ac.lat, ac.lon);
        for i in 1..=n {
            let (la, lo) = destination(ac.lat, ac.lon, brg, dist * i as f64 / n as f64);
            let p = to_s(la, lo);
            if (p.x - prev.x).abs() < r.w * 0.5 {
                draw_line(prev.x, prev.y, p.x, p.y, 2.0, MAGENTA_S);
            }
            prev = p;
        }

        // Aéroports
        for (i, a) in AIRPORTS.iter().enumerate() {
            let p = to_s(a.lat, a.lon);
            if !r.contains(p) {
                continue;
            }
            let c = if i == dest { MAGENTA_S } else { CYAN_S };
            draw_circle_lines(p.x, p.y, 5.0, 1.5, c);
            if z > 150.0 {
                let (la1, lo1) = a.point(-a.len / 2.0, 0.0);
                let (la2, lo2) = a.point(a.len / 2.0, 0.0);
                let (p1, p2) = (to_s(la1, lo1), to_s(la2, lo2));
                draw_line(p1.x, p1.y, p2.x, p2.y, 3.0, WHITE);
            }
            if z > 12.0 || i == dest {
                txt(a.icao, p.x + 7.0, p.y - 5.0, 14.0, c);
            }
        }

        // Reconnaissance : objectifs, route, émetteurs et menaces, empreintes des capteurs
        crate::weather::draw_map(wx, ac, &to_s, z, r);
        crate::recon::draw_map(recon, ac, &to_s, z, r);
        crate::refuel::draw_map(rf, &to_s, r);
        crate::combat::draw_map(cb, &recon.emitters, &to_s);

        // Trace
        let mut prev: Option<Vec2> = None;
        for &(la, lo) in &self.trail {
            let p = to_s(la, lo);
            if let Some(q) = prev {
                if (p.x - q.x).abs() < r.w * 0.5 {
                    draw_line(q.x, q.y, p.x, p.y, 1.5, Color::new(1.0, 0.85, 0.2, 0.5));
                }
            }
            prev = Some(p);
        }

        // Rayon d'action approximatif (carburant / débit actuel)
        if ac.eng.fuel_flow > 0.1 && !ac.on_ground {
            let range = ac.fuel / ac.eng.fuel_flow * ac.tas;
            if range < 20_000_000.0 {
                let mut prev: Option<Vec2> = None;
                for k in 0..=72 {
                    let (la, lo) = destination(ac.lat, ac.lon, k as f64 * 5.0, range);
                    let p = to_s(la, lo);
                    if let Some(q) = prev {
                        if k % 2 == 0 && (p.x - q.x).abs() < r.w * 0.5 {
                            draw_line(q.x, q.y, p.x, p.y, 1.0, Color::new(0.3, 1.0, 0.4, 0.6));
                        }
                    }
                    prev = Some(p);
                }
            }
        }


        // Avion
        let p = to_s(ac.lat, ac.lon);
        let hr = ac.psi as f32;
        let f = vec2(hr.sin(), -hr.cos());
        let s = vec2(-f.y, f.x);
        draw_line(p.x, p.y, p.x + f.x * 2000.0, p.y + f.y * 2000.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.25));
        let col = Color::new(1.0, 0.85, 0.1, 1.0);
        let _ = s;
        crate::syspage::mini(p, 28.0, hr, col, Color::new(0.3, 0.22, 0.0, 1.0));
        let _ = time;

        // Légende
        let km_per_px = 111.195 * clat.to_radians().cos() / z as f64;
        let bar_km = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0]
            .iter()
            .copied()
            .find(|k| k / km_per_px > 60.0)
            .unwrap_or(5000.0);
        let bw = (bar_km / km_per_px) as f32;
        let (bx, by) = (r.x + 12.0, r.y + r.h - 14.0);
        draw_line(bx, by, bx + bw, by, 2.0, WHITE);
        txt(&format!("{bar_km:.0} km"), bx, by - 6.0, 13.0, WHITE);
        txt(if self.follow { "SUIVI AVION" } else { "LIBRE (C: recentrer)" }, r.x + r.w - 170.0, r.y + r.h - 10.0, 13.0, DIM);
        scissor(None);
        draw_rectangle_lines(r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0, 1.0, Color::new(0.25, 0.3, 0.36, 1.0));
        draw_rectangle(r.x + 1.0, r.y + 1.0, 290.0, 20.0, Color::new(0.0, 0.0, 0.0, 0.6));
        txt("ND / CARTE MONDE  (molette: zoom)", r.x + 8.0, r.y + 16.0, 14.0, DIM);
        // Sélecteur PLAN / SAT et crédits des tuiles
        let b = Rect::new(r.x + r.w - 118.0, r.y + 4.0, 112.0, 20.0);
        rrect(b, 5.0, Color::new(0.0, 0.0, 0.0, 0.7));
        for (i, (lbl, on)) in [("PLAN", !self.sat), ("SAT", self.sat)].iter().enumerate() {
            let c = Rect::new(b.x + 2.0 + i as f32 * 55.0, b.y + 2.0, 53.0, 16.0);
            if *on {
                rrect(c, 4.0, Color::new(0.10, 0.30, 0.42, 1.0));
            }
            txt_c(lbl, c.x + c.w / 2.0, c.y + 12.0, 12.0, if *on { WHITE } else { DIM });
        }
        if clicked(b) {
            self.sat = !self.sat;
        }
        let credit = if tiles.offline() { "HORS LIGNE - carte Natural Earth" } else { layer.credit() };
        let cw = credit.len() as f32 * 6.2 + 10.0;
        draw_rectangle(r.x + r.w - cw - 2.0, r.y + r.h - 34.0, cw, 15.0, Color::new(0.0, 0.0, 0.0, 0.55));
        txt_r(credit, r.x + r.w - 6.0, r.y + r.h - 23.0, 10.0, DIM);
    }
}

/// Ordonnée Mercator exprimée en « degrés » (même échelle que la longitude).
pub fn merc(lat: f64) -> f64 {
    let l = lat.clamp(-85.05, 85.05).to_radians();
    (std::f64::consts::FRAC_PI_4 + l / 2.0).tan().ln().to_degrees()
}

pub fn merc_inv(m: f64) -> f64 {
    (2.0 * m.to_radians().exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees()
}
