//! Carte de navigation mondiale (projection équirectangulaire).

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::traffic::{Tcas, TrafficSys};
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;

pub struct MapView {
    pub zoom: f32, // pixels par degré
    pub follow: bool,
    pub center: (f64, f64), // (lat, lon)
    pub trail: Vec<(f64, f64)>,
    trail_t: f64,
    drag: Option<Vec2>,
}

impl MapView {
    pub fn new() -> Self {
        MapView { zoom: 60.0, follow: true, center: (0.0, 0.0), trail: Vec::new(), trail_t: 0.0, drag: None }
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
                self.zoom = (self.zoom * f).clamp(r.w / 360.0, 3000.0);
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
                    let cl = self.center.0.to_radians().cos().max(0.2) as f32;
                    let _ = cl;
                    self.center.1 = wrap180(self.center.1 - (dlt.x / self.zoom) as f64);
                    self.center.0 = (self.center.0 + (dlt.y / self.zoom) as f64).clamp(-85.0, 85.0);
                    self.drag = Some(vec2(mx, my));
                }
            } else {
                self.drag = None;
            }
        }
    }

    pub fn draw(&mut self, world: &World, ac: &Aircraft, traffic: &TrafficSys, r: Rect, dest: usize, time: f64) {
        if self.follow {
            self.center = (ac.lat, ac.lon);
        }
        let z = self.zoom;
        let (clat, clon) = self.center;
        let cx = r.x + r.w / 2.0;
        let cy = r.y + r.h / 2.0;
        let to_s = |lat: f64, lon: f64| -> Vec2 {
            vec2(cx + wrap180(lon - clon) as f32 * z, cy - (lat - clat) as f32 * z)
        };

        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.06, 0.12, 1.0));
        scissor(Some(r));
        // Texture monde (3 copies pour l'antiméridien)
        for k in [-1.0f32, 0.0, 1.0] {
            let x0 = cx + ((-180.0 - clon) as f32 + k * 360.0) * z;
            let y0 = cy - (90.0 - clat) as f32 * z;
            if x0 > r.x + r.w || x0 + 360.0 * z < r.x {
                continue;
            }
            draw_texture_ex(&world.texture, x0, y0, Color::new(0.75, 0.78, 0.82, 1.0), DrawTextureParams { dest_size: Some(vec2(360.0 * z, 180.0 * z)), ..Default::default() });
        }
        // Traits de côte nets aux zooms élevés
        if z > 14.0 {
            let half_w = (r.w / 2.0 / z) as f64 + 1.0;
            let half_h = (r.h / 2.0 / z) as f64 + 1.0;
            let col = Color::new(0.85, 0.9, 0.8, 0.55);
            for ring in &world.rings {
                if (ring.max.1 as f64) < clat - half_h || (ring.min.1 as f64) > clat + half_h {
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
        let mut lat = -90.0;
        while lat <= 90.0 {
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

        // Trafic
        for t in &traffic.list {
            let p = to_s(t.lat, t.lon);
            if !r.contains(p) {
                continue;
            }
            let c = match t.tcas {
                Tcas::Ra => RED,
                Tcas::Ta => AMBER,
                Tcas::Proximate => WHITE,
                Tcas::None => Color::new(0.75, 0.8, 0.85, 0.85),
            };
            let hr = (t.hdg as f32).to_radians();
            let f = vec2(hr.sin(), -hr.cos());
            let s = vec2(-f.y, f.x);
            draw_triangle(p + f * 6.0, p - f * 4.0 + s * 4.0, p - f * 4.0 - s * 4.0, c);
            if z > 40.0 {
                txt(&format!("FL{:03.0}", t.alt / FT / 100.0), p.x + 6.0, p.y + 12.0, 12.0, c);
                if z > 120.0 {
                    txt(&format!("{} {}", t.callsign, t.actype), p.x + 6.0, p.y - 4.0, 12.0, c);
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
        draw_triangle(p + f * 14.0, p - f * 8.0 + s * 9.0, p - f * 4.0, col);
        draw_triangle(p + f * 14.0, p - f * 8.0 - s * 9.0, p - f * 4.0, col);
        let _ = time;

        // Légende
        let km_per_px = 111.195 / z as f64;
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
    }
}
