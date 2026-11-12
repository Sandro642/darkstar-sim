//! Bannière d'approche sur l'horizon (ILS / autoland) : profil vertical en 2D transparent.
//! - bandeau des phases (IAF, LOC, G/S, FLARE, ROLLOUT, ARRÊT ou, en manuel, LOC, G/S, MINIMUMS,
//!   TOUCHER, ROULAGE), phase courante en surbrillance ;
//! - « fil » du plan de descente à 3° jusqu'au seuil, arrondi, piste ;
//! - repères 1000 ft (stabilisé), minimums 200 ft, arrondi 50 ft, zone de toucher ;
//! - avion abstrait orienté selon sa pente, traînée de sa trajectoire, écart au plan de descente.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::autoflight::AlPhase;
use crate::geo::*;
use crate::ui::*;
use macroquad::prelude::*;

/// Horizon : cadres à angles nets (présentation d'afficheur de bord).
fn rrect(r: Rect, _rad: f32, c: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, c);
}

#[allow(dead_code)]
fn rrect_lines(r: Rect, _rad: f32, th: f32, c: Color) {
    draw_rectangle_lines(r.x, r.y, r.w, r.h, th, c);
}
use std::cell::RefCell;

thread_local! {
    /// Traînée : (distance avant le seuil m, hauteur m, instant).
    static TRAIL: RefCell<(Vec<(f64, f64)>, f64)> = const { RefCell::new((Vec::new(), -1.0)) };
}

const GS: f64 = 3.0;

/// Hauteur de la bannière (pour décaler les autres symboles).
pub const BANNER_H: f32 = 74.0;

/// Bannière affichée : approche ILS dans les 15 NM ou autoland engagé.
pub fn visible(ac: &Aircraft) -> bool {
    if ac.crashed.is_some() {
        return false;
    }
    match ac.approach.or_else(|| ac.autoland.as_ref().map(|a| (a.apt, a.thr_lat, a.thr_lon, a.hdg, a.elev))) {
        Some((ai, tla, tlo, thdg, _)) => {
            let along = along_before(tla, tlo, thdg, ac.lat, ac.lon);
            along < 40.0 * NM && (ac.autoland.is_some() || (along < 15.0 * NM && along > -AIRPORTS[ai].len - 200.0 && (!ac.on_ground || ac.tas > 3.0)))
        }
        None => false,
    }
}

pub fn draw(ac: &Aircraft, r: Rect, time: f64) {
    if !visible(ac) {
        TRAIL.with(|t| t.borrow_mut().0.clear());
        return;
    }
    let Some((ai, tla, tlo, thdg, tel)) = ac.approach.or_else(|| ac.autoland.as_ref().map(|a| (a.apt, a.thr_lat, a.thr_lon, a.hdg, a.elev))) else {
        TRAIL.with(|t| t.borrow_mut().0.clear());
        return;
    };
    if ac.crashed.is_some() {
        return;
    }
    let apt = &AIRPORTS[ai];
    let along = along_before(tla, tlo, thdg, ac.lat, ac.lon);
    let h = (ac.alt - tel).max(0.0);
    let active = ac.autoland.is_some() || (along < 15.0 * NM && along > -apt.len - 200.0 && (!ac.on_ground || ac.tas > 3.0));
    if !active || along > 40.0 * NM {
        return;
    }
    // Traînée échantillonnée toutes les 0,5 s
    TRAIL.with(|t| {
        let mut t = t.borrow_mut();
        if time - t.1 > 0.5 {
            t.1 = time;
            t.0.push((along, h));
            if t.0.len() > 90 {
                t.0.remove(0);
            }
        }
    });

    let w = (r.w * 0.52).min(460.0);
    let bh = BANNER_H;
    let b = Rect::new(r.x + (r.w - w) / 2.0, r.y + r.h - bh - 20.0, w, bh);
    rrect(b, 6.0, Color::new(0.0, 0.02, 0.04, 0.28));
    rrect_lines(b, 6.0, 1.0, Color::new(0.4, 0.85, 1.0, 0.18));

    // --- Bandeau des phases ---
    let (labels, cur): (Vec<&str>, usize) = match &ac.autoland {
        Some(al) => {
            let i = match al.phase {
                AlPhase::ToIaf => 0,
                AlPhase::Approach if al.gs_captured => 2,
                AlPhase::Approach => 1,
                AlPhase::Flare => 3,
                AlPhase::Rollout if ac.ground_speed() < 3.0 => 5,
                AlPhase::Rollout => 4,
            };
            (vec!["IAF", "LOC", "G/S", "FLARE", "ROLLOUT", "ARRET"], i)
        }
        None => {
            let i = if ac.on_ground {
                if ac.ground_speed() < 3.0 { 4 } else { 3 }
            } else if h / FT < 200.0 {
                2
            } else if ac.ap.vert == VertMode::Gs {
                1
            } else {
                0
            };
            (vec!["LOC", "G/S", "MINIMUMS", "TOUCHER", "ROULAGE"], i)
        }
    };
    let title = if ac.autoland.is_some() { "AUTOLAND" } else { "ILS" };
    txt(&format!("{title} {:02}", crate::airports::Airport::runway_number(thdg)), b.x + 8.0, b.y + 12.0, 9.0, Color::new(0.5, 0.85, 1.0, 0.75));
    let cx0 = b.x + 78.0;
    let cw = (b.w - 86.0) / labels.len() as f32;
    let pulse = 0.6 + 0.4 * ((time * 4.0).sin() as f32 * 0.5 + 0.5);
    for (k, l) in labels.iter().enumerate() {
        let x = cx0 + k as f32 * cw + cw / 2.0;
        let fg = if k == cur {
            Color::new(0.6, 1.0, 1.0, 0.95)
        } else if k < cur {
            Color::new(0.4, 0.9, 0.5, 0.55)
        } else {
            Color::new(0.6, 0.65, 0.7, 0.4)
        };
        txt_c(l, x, b.y + 12.0, 9.0, fg);
        if k == cur {
            let tw = txt_w(l, 9.0);
            draw_line(x - tw / 2.0, b.y + 15.0, x + tw / 2.0, b.y + 15.0, 1.0, Color::new(0.5, 0.95, 1.0, pulse * 0.8));
        }
    }

    // --- Profil vertical ---
    let p = Rect::new(b.x + 10.0, b.y + 20.0, b.w - 20.0, b.h - 30.0);
    let dmax = (along.max(0.0) * 1.15 + 1.0 * NM).clamp(4.0 * NM, 15.0 * NM);
    let span = dmax + apt.len + 300.0;
    let hmax = ((dmax * GS.to_radians().tan()) * 1.25).max(h * 1.15).max(300.0);
    let px = |a: f64| p.x + ((dmax - a) / span) as f32 * p.w;
    let py = |hh: f64| p.y + p.h - (hh / hmax).clamp(-0.05, 1.0) as f32 * p.h;
    let faint = Color::new(0.6, 0.65, 0.7, 0.25);
    // Graduations de hauteur et de distance
    for ft in [500.0, 1000.0, 2000.0, 3000.0, 5000.0] {
        let hh = ft * FT;
        if hh < hmax {
            let y = py(hh);
            draw_line(p.x, y, p.x + p.w, y, 1.0, Color::new(0.6, 0.65, 0.7, 0.07));
        }
    }
    let mut nm = 1.0;
    while nm * NM < dmax {
        let x = px(nm * NM);
        draw_line(x, p.y + p.h, x, p.y + p.h + 4.0, 1.0, faint);

        nm += 1.0;
    }
    // Sol et piste
    let gy = py(0.0);
    draw_line(p.x, gy, p.x + p.w, gy, 1.0, Color::new(0.7, 0.6, 0.35, 0.35));
    let (rx0, rx1) = (px(0.0), px(-apt.len));
    draw_rectangle(rx0, gy - 1.5, rx1 - rx0, 3.0, Color::new(0.85, 0.88, 0.9, 0.55));
    for k in 0..6 {
        let x = rx0 + (rx1 - rx0) * (0.1 + k as f32 * 0.15);
        draw_line(x, gy, x + 4.0, gy, 1.0, BLACK);
    }
    // Fil du plan de descente (avec halo) puis arrondi jusqu'au toucher
    let wire = Color::new(1.0, 0.35, 0.9, 0.6);
    let gs_h = |a: f64| a.max(0.0) * GS.to_radians().tan() + 15.0;
    let a_flare = (50.0 * FT - 15.0) / GS.to_radians().tan();
    let (x0, y0) = (px(dmax), py(gs_h(dmax)));
    let (x1, y1) = (px(a_flare), py(gs_h(a_flare)));
    draw_line(x0, y0, x1, y1, 1.0, wire);
    let mut prev = vec2(x1, y1);
    for k in 1..=12 {
        let t = k as f64 / 12.0;
        let a = a_flare + (-300.0 - a_flare) * t;
        let hh = 50.0 * FT * (1.0 - t).powi(2);
        let q = vec2(px(a), py(hh));
        draw_line(prev.x, prev.y, q.x, q.y, 1.0, Color::new(1.0, 0.35, 0.9, 0.45));
        prev = q;
    }
    // Repères
    for (ft, lbl) in [(1000.0, "STAB"), (200.0, "MINI"), (50.0, "FLARE")] {
        let a = (ft * FT - 15.0) / GS.to_radians().tan();
        if a < dmax {
            let (x, y) = (px(a), py(ft * FT));
            draw_line(x, y - 3.0, x, y + 3.0, 1.0, Color::new(1.0, 0.85, 0.3, 0.55));
            let _ = lbl;
        }
    }
    let tdz = px(-300.0);
    draw_line(tdz, gy - 3.0, tdz, gy + 3.0, 1.0, Color::new(1.0, 0.85, 0.3, 0.55));
    // Traînée
    TRAIL.with(|t| {
        let t = t.borrow();
        let n = t.0.len();
        for (k, &(a, hh)) in t.0.iter().enumerate() {
            if a > dmax {
                continue;
            }
            let f = k as f32 / n.max(1) as f32;
            draw_circle(px(a), py(hh), 1.1, Color::new(0.4, 0.95, 1.0, 0.08 + 0.3 * f));
        }
    });
    // Avion abstrait : chevron orienté selon la pente, écart au fil
    if along <= dmax {
        let (x, y) = (px(along), py(h));
        let dev = if along > 0.0 { ((ac.alt - tel) / (along + 300.0).max(50.0)).atan().to_degrees() - GS } else { 0.0 };
        let on_gs = along > a_flare && !ac.on_ground;
        if on_gs {
            let wy = py(gs_h(along));
            let c = if dev.abs() < 0.35 { Color::new(0.4, 1.0, 0.5, 0.8) } else { AMBER };
            let mut yy = y.min(wy);
            while yy < y.max(wy) {
                draw_line(x, yy, x, (yy + 3.0).min(y.max(wy)), 1.0, c);
                yy += 6.0;
            }
        }
        let ang = -(ac.gamma as f32) * (p.h / hmax as f32) / (p.w / span as f32);
        let ang = ang.clamp(-0.8, 0.8);
        let d = vec2(ang.cos(), ang.sin());
        let n = vec2(-d.y, d.x);
        let tip = vec2(x, y) + d * 6.0;
        let col = Color::new(0.45, 0.95, 1.0, 0.9);
        draw_triangle(tip, vec2(x, y) - d * 4.0 + n * 3.5, vec2(x, y) - d * 2.0, col);
        draw_triangle(tip, vec2(x, y) - d * 2.0, vec2(x, y) - d * 4.0 - n * 3.5, Color::new(0.25, 0.7, 0.85, 0.9));
        // Valeurs
        let lbl = if ac.on_ground {
            format!("{:.0} kt", ac.ground_speed() / KT)
        } else {
            format!("{:.1} NM  {:.0} ft", along.max(0.0) / NM, h / FT)
        };
        let tx = (x + 9.0).min(b.x + b.w - txt_w(&lbl, 9.0) - 6.0);
        txt(&lbl, tx, y - 6.0, 9.0, Color::new(0.6, 0.9, 1.0, 0.75));
        if on_gs {
            let c = if dev.abs() < 0.35 { GREEN_S } else { AMBER };
            txt_r(&format!("G/S {dev:+.2}"), b.x + b.w - 8.0, b.y + b.h - 3.0, 9.0, Color::new(c.r, c.g, c.b, 0.75));
        }
    }

}
