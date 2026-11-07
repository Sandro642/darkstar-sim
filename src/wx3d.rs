//! Météo 3D dans l'horizon (« horizon intelligent ») :
//! - flux de vent : particules advectées par le vent réel, traînées orientées selon le vent, longueur
//!   selon sa force, couleur selon le danger, épaisseur selon l'humidité (masse d'air humide) ;
//! - vecteurs de vent : rangées de flèches 3D devant l'avion, à son altitude, chiffrées au centre ;
//! - volumes de danger : cylindres filaires autour des cumulonimbus (sommet, distance) ;
//! - couches de turbulence en air clair : nappes quadrillées aux altitudes de fort cisaillement ;
//! - légende de l'échelle de danger.
//! Échelle : cyan (calme) → vert → jaune → orange → rouge → magenta (sévère).

use crate::aircraft::Aircraft;
use crate::geo::*;
use crate::svs::View;
use crate::terrain::EyeFrame;
use crate::ui::*;
use crate::weather::{CloudKind, Weather};
use macroquad::prelude::*;

/// Couleur de l'échelle de danger (0..1).
pub fn danger_color(d: f32, a: f32) -> Color {
    let stops: [(f32, [f32; 3]); 6] = [
        (0.0, [0.30, 0.85, 1.00]),
        (0.25, [0.35, 1.00, 0.55]),
        (0.45, [1.00, 0.90, 0.25]),
        (0.65, [1.00, 0.55, 0.15]),
        (0.80, [1.00, 0.20, 0.15]),
        (1.0, [1.00, 0.25, 0.90]),
    ];
    let d = d.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let ((d0, c0), (d1, c1)) = (w[0], w[1]);
        if d <= d1 {
            let t = (d - d0) / (d1 - d0);
            return Color::new(c0[0] + (c1[0] - c0[0]) * t, c0[1] + (c1[1] - c0[1]) * t, c0[2] + (c1[2] - c0[2]) * t, a);
        }
    }
    Color::new(1.0, 0.25, 0.9, a)
}

pub fn danger_label(d: f32) -> &'static str {
    if d > 0.8 {
        "SEVERE"
    } else if d > 0.6 {
        "FORT"
    } else if d > 0.4 {
        "MODERE"
    } else if d > 0.2 {
        "FAIBLE"
    } else {
        "CALME"
    }
}

fn seg(v: &View, a: Vec3, b: Vec3, w: f32, c: Color) {
    if let (Some(p), Some(q)) = (v.project(a), v.project(b)) {
        if (p - q).length() < v.rect.w * 1.5 {
            draw_line(p.x, p.y, q.x, q.y, w, c);
        }
    }
}

/// Point local (repère œil) à partir d'un point géographique décalé en est / nord / haut.
fn local(eye: &EyeFrame, lat: f64, lon: f64, alt: f64, de: f64, dn: f64, du: f64) -> Vec3 {
    let (la, lo) = from_enu(lat, lon, de, dn);
    eye.local(la, lo, alt + du)
}

pub fn draw(wx: &Weather, ac: &Aircraft, v: &View, time: f64) {
    if !wx.show3d || ac.crashed.is_some() {
        return;
    }
    let eye = EyeFrame::new(ac);
    let r = wx.part_r;
    scissor(Some(v.rect));

    // --- Flux de vent : traînées ---
    for p in &wx.parts {
        let a = eye.local(p.lat, p.lon, p.alt);
        let dist = a.length() as f64;
        if dist < 30.0 {
            continue;
        }
        let fade = (1.0 - dist / (r * 1.1)).clamp(0.0, 1.0) as f32;
        if fade < 0.02 {
            continue;
        }
        let spd = p.u.hypot(p.v);
        let len = r * 0.03 * (spd / 25.0).clamp(0.5, 3.0);
        let k = len / spd.max(0.5);
        let b = local(&eye, p.lat, p.lon, p.alt, p.u * k, p.v * k, 0.0);
        let col = danger_color(p.danger, (0.35 + 0.6 * fade.powf(0.7)) * (0.7 + 0.3 * p.danger.max(0.3)));
        let w = 1.0 + 1.6 * p.rh + p.danger * 1.2;
        seg(v, a, b, w, col);
        if let Some(q) = v.project(b) {
            draw_circle(q.x, q.y, w * 0.9, col);
        }
    }

    // --- Vecteurs de vent : 3 rangées x 5 flèches (vent à l'altitude de l'avion), dessinées un peu
    // en contrebas pour être vues en perspective plutôt que par la tranche ---
    if !ac.on_ground {
        let hdg = ac.track_deg();
        let drop = -(r * 0.05).max(150.0).min((ac.alt - ac.d.elev) * 0.5);
        for (row, f) in [0.35f64, 0.6, 0.9].iter().enumerate() {
            for col in [-2i32, -1, 1, 2] {
                let (a1, o1) = destination(ac.lat, ac.lon, hdg, r * f);
                let (la, lo) = destination(a1, o1, wrap360(hdg + 90.0), col as f64 * r * 0.22 * f);
                let (u, w, dir, kt) = wx.wind_at(la, lo, ac.alt);
                let (dg, _, _) = wx.field_danger(la, lo, ac.alt);
                let spd = u.hypot(w).max(0.1);
                let (ue, un) = (u / spd, w / spd);
                let half = r * 0.045 * (spd / 30.0).clamp(0.35, 2.2);
                let tail = local(&eye, la, lo, ac.alt, -ue * half, -un * half, drop);
                let tip = local(&eye, la, lo, ac.alt, ue * half, un * half, drop);
                let c = danger_color(dg, 0.85 - row as f32 * 0.15);
                seg(v, tail, tip, 2.5, c);
                // Pointe : deux branches dans le plan horizontal
                let hl = half * 0.45;
                for s in [-1.0f64, 1.0] {
                    let (be, bn) = (-ue * 0.9 + s * -un * 0.45, -un * 0.9 + s * ue * 0.45);
                    let wing = local(&eye, la, lo, ac.alt, ue * half + be * hl, un * half + bn * hl, drop);
                    seg(v, tip, wing, 2.5, c);
                }
                if col == 1 && row == 2 {
                    if let Some(q) = v.project(local(&eye, la, lo, ac.alt, 0.0, 0.0, drop)) {
                        let lbl = format!("{:03.0}/{:.0}", dir, kt);
                        let tw = txt_w(&lbl, 11.0) + 8.0;
                        rrect(Rect::new(q.x - tw / 2.0, q.y + 8.0, tw, 15.0), 4.0, Color::new(0.0, 0.0, 0.0, 0.45));
                        txt_c(&lbl, q.x, q.y + 19.0, 11.0, c);
                    }
                }
            }
        }
    }

    // --- Volumes de danger : cumulonimbus ---
    let pulse = 0.75 + 0.25 * ((time * 3.0).sin() as f32);
    // Les 4 orages les plus proches devant ; étiquettes sans chevauchement
    let mut cbs: Vec<(f64, &crate::weather::Cloud)> = wx
        .clouds_near(ac.lat, ac.lon, 180_000.0)
        .filter(|c| c.kind == CloudKind::Cb)
        .filter_map(|c| {
            let (d, brg) = dist_brg(ac.lat, ac.lon, c.lat, c.lon);
            (wrap180(brg - ac.hdg_deg()).abs() < 80.0 || d < c.radius * 2.0).then_some((d, c))
        })
        .collect();
    cbs.sort_by(|a, b| a.0.total_cmp(&b.0));
    cbs.truncate(4);
    let mut labels: Vec<Rect> = Vec::new();
    for (d, cl) in cbs {
        let rad = cl.radius * 1.3;
        let c = danger_color(1.0, 0.75 * pulse);
        let n = 24;
        let ring = |alt: f64, k: usize| {
            let a = k as f64 / n as f64 * std::f64::consts::TAU;
            local(&eye, cl.lat, cl.lon, alt, rad * a.sin(), rad * a.cos(), 0.0)
        };
        for alt in [cl.base, (cl.base + cl.top) / 2.0, cl.top] {
            for k in 0..n {
                seg(v, ring(alt, k), ring(alt, k + 1), if alt == cl.top { 2.0 } else { 1.2 }, c);
            }
        }
        for k in (0..n).step_by(6) {
            seg(v, ring(cl.base, k), ring(cl.top, k), 1.2, c);
        }
        if let Some(q) = v.project(eye.local(cl.lat, cl.lon, cl.top + 800.0)) {
            let lbl = format!("CB FL{:.0}  {:.0} NM", cl.top / FT / 100.0, d / NM);
            let tw = txt_w(&lbl, 12.0) + 10.0;
            let lr = Rect::new(q.x - tw / 2.0, q.y - 16.0, tw, 17.0);
            if !labels.iter().any(|o| o.overlaps(&lr)) {
                rrect(lr, 4.0, Color::new(0.0, 0.0, 0.0, 0.55));
                txt_c(&lbl, q.x, q.y - 3.0, 12.0, danger_color(1.0, 1.0));
                labels.push(lr);
            }
        }
    }

    // --- Couches de turbulence en air clair devant l'avion ---
    if !ac.on_ground {
        let hdg = ac.track_deg();
        let (ca, co) = destination(ac.lat, ac.lon, hdg, r * 0.55);
        let mut layers: Vec<(f64, f32)> = Vec::new();
        for k in -6i32..=6 {
            let alt = ac.alt + k as f64 * 500.0;
            if alt < ac.d.elev + 300.0 {
                continue;
            }
            let (dg, cause, _) = wx.field_danger(ca, co, alt);
            if cause == "CAT" && dg > 0.35 {
                layers.push((alt, dg));
            }
        }
        layers.sort_by(|a, b| b.1.total_cmp(&a.1));
        layers.truncate(2);
        let (fe, fn_) = (hdg.to_radians().sin(), hdg.to_radians().cos());
        let (le, ln) = (fn_, -fe);
        let (hw, hd) = (r * 0.3, r * 0.25);
        for (alt, dg) in layers {
            let c = danger_color(dg, 0.55);
            let p = |a: f64, b: f64| local(&eye, ca, co, alt, fe * a + le * b, fn_ * a + ln * b, 0.0);
            for k in 0..=4 {
                let t = -1.0 + k as f64 * 0.5;
                seg(v, p(t * hd, -hw), p(t * hd, hw), 1.2, c);
                seg(v, p(-hd, t * hw), p(hd, t * hw), 1.2, c);
            }
            if let Some(q) = v.project(p(-hd, hw)) {
                txt(&format!("CAT FL{:.0} {}", alt / FT / 100.0, danger_label(dg)), q.x + 4.0, q.y - 4.0, 12.0, danger_color(dg, 1.0));
            }
        }
    }
    scissor(None);
}

/// Légende compacte de l'échelle de danger (bloc météo de l'horizon).
pub fn legend(x: f32, y: f32, w: f32, a: f32) {
    let n = 24;
    let sw = w / n as f32;
    for k in 0..n {
        draw_rectangle(x + k as f32 * sw, y, sw + 0.5, 5.0, danger_color(k as f32 / (n - 1) as f32, 0.9 * a));
    }
    txt("CALME", x, y + 16.0, 10.0, Color::new(0.8, 0.85, 0.9, a));
    txt_r("SEVERE", x + w, y + 16.0, 10.0, Color::new(0.8, 0.85, 0.9, a));
}
