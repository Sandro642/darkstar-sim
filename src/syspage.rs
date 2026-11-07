//! Page SYSTEMES, d'après l'écran principal du Darkstar (photo du film) : traits fins sur fond noir.
//! - colonne gauche : état hydraulique, électrique, pile à combustible, oxygène, baie avionique ;
//! - vue en plan « STRUCTURE / THERMIQUE » au contour orange : baies et organes colorés selon leur
//!   état (vert, ambre, rouge), capteurs de température ;
//! - au centre : tableau des moteurs (turbines G / D, statoréacteur, scramjet), éventail du radar
//!   météo avec le faisceau, échelles verticales de centrage et d'incidence ;
//! - vue en plan « CARBURANT » remplie : réservoirs JP-7 (vert) et hydrogène (cyan) selon le niveau ;
//! - rangée d'icônes en bas : accès direct aux pages (comme les touches tactiles de l'écran).

use crate::aircraft::*;
use crate::gcs::Tab;
use crate::systems::Fail;
use crate::ui::*;
use macroquad::prelude::*;

const LINE: Color = Color::new(0.82, 0.85, 0.88, 1.0);
const FAINT: Color = Color::new(0.55, 0.58, 0.62, 0.55);
const ORANGE: Color = Color::new(1.0, 0.55, 0.2, 1.0);
const OKC: Color = Color::new(0.35, 0.95, 0.45, 1.0);

/// Silhouette du Darkstar en plan (x -1..1, y 0 nez .. 1 tuyères).
const OUTLINE: [(f32, f32); 21] = [
    (0.0, 0.0), (0.035, 0.10), (0.075, 0.24), (0.12, 0.40), (0.20, 0.55), (0.42, 0.70), (0.66, 0.84),
    (0.68, 0.90), (0.42, 0.91), (0.26, 0.96), (0.10, 0.99), (-0.10, 0.99), (-0.26, 0.96), (-0.42, 0.91),
    (-0.68, 0.90), (-0.66, 0.84), (-0.42, 0.70), (-0.20, 0.55), (-0.12, 0.40), (-0.075, 0.24), (-0.035, 0.10),
];

fn state_color(s: u8) -> Color {
    match s {
        0 => OKC,
        1 => AMBER,
        2 => RED,
        _ => FAINT,
    }
}

/// Repère de la vue en plan dans un rectangle.
pub fn plan(r: Rect) -> impl Fn(f32, f32) -> Vec2 {
    // Proportions de la cellule : envergure ~ 0,8 x longueur
    let pw = (r.w * 1.35).min((r.h - 12.0) * 1.15);
    let ph = pw * 0.87;
    let cx = r.x + r.w / 2.0;
    let y0 = r.y + (r.h - ph) / 2.0;
    move |x: f32, y: f32| vec2(cx + x * pw / 2.0, y0 + y * ph)
}

pub fn silhouette(r: Rect, edge: Color, fill: Color) -> Vec<Vec2> {
    let to = plan(r);
    let shape = smooth(&OUTLINE.iter().map(|&(x, y)| to(x, y)).collect::<Vec<_>>(), true, 2);
    let c = to(0.0, 0.62);
    if fill.a > 0.0 {
        for i in 0..shape.len() {
            draw_triangle(c, shape[i], shape[(i + 1) % shape.len()], fill);
        }
    }
    stroke(&shape, true, 1.6, edge);
    for sx in [-1.0f32, 1.0] {
        round_line(to(sx * 0.16, 0.74), to(sx * 0.27, 0.97), 2.5, edge);
    }
    shape
}

/// Synoptique carburant : silhouette remplie, réservoirs JP-7 (vert) et H2 (cyan) selon le niveau.
pub fn fuel_tanks(ac: &Aircraft, pr: Rect) {
    silhouette(pr, OKC, Color::new(0.05, 0.16, 0.08, 1.0));
    let tp = plan(pr);
    let cap = FUEL_MAX / 4.0;
    let tank = |i: usize| ((ac.fuel - i as f64 * cap) / cap).clamp(0.0, 1.0) as f32;
    let h2f = (ac.h2 / H2_MAX) as f32;
    let blocks: [(f32, f32, f32, f32, f32, Color, &str); 6] = [
        (-0.05, 0.28, 0.05, 0.40, tank(0), OKC, "1"),
        (-0.09, 0.43, 0.09, 0.53, h2f, CYAN_S, "H2"),
        (-0.15, 0.56, 0.15, 0.66, tank(1), OKC, "2"),
        (-0.36, 0.70, -0.12, 0.82, tank(2), OKC, "3"),
        (0.12, 0.70, 0.36, 0.82, tank(2), OKC, "3"),
        (-0.14, 0.84, 0.14, 0.93, tank(3), OKC, "4"),
    ];
    for (x0, y0, x1, y1, f, c, l) in blocks {
        let (a, b) = (tp(x0, y0), tp(x1, y1));
        let rr = Rect::new(a.x, a.y, b.x - a.x, b.y - a.y);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.0, 0.0, 0.0, 0.6));
        let fh = rr.h * f;
        draw_rectangle(rr.x, rr.y + rr.h - fh, rr.w, fh, Color::new(c.r * 0.85, c.g * 0.85, c.b * 0.85, 0.95));
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, LINE);
        if rr.h > 9.0 {
            txt_c(l, rr.x + rr.w / 2.0, rr.y + rr.h / 2.0 + 4.0, 10.0, if f > 0.5 { BLACK } else { LINE });
        }
    }
}

/// Petite jauge horizontale libellée.
fn bar(x: f32, y: f32, w: f32, label: &str, val: &str, f: f32, c: Color) {
    txt(label, x, y, 11.0, LINE);
    txt_r(val, x + w, y, 11.0, c);
    draw_rectangle_lines(x, y + 4.0, w, 6.0, 1.0, FAINT);
    draw_rectangle(x + 1.0, y + 5.0, (w - 2.0) * f.clamp(0.0, 1.0), 4.0, c);
}

fn turb_state(s: TurbState) -> (&'static str, u8) {
    match s {
        TurbState::Off => ("OFF", 3),
        TurbState::Starting => ("DEM", 1),
        TurbState::Running => ("RUN", 0),
        TurbState::Failed => ("PANNE", 2),
    }
}

pub fn draw(ac: &Aircraft, th: &crate::thermal::ThermalSys, wxr: &crate::wxr::Wxr, r: Rect, time: f64) -> Option<Tab> {
    panel(r, "SYSTEMES — SYNOPTIQUE");
    let inner = Rect::new(r.x + 8.0, r.y + 26.0, r.w - 16.0, r.h - 34.0);
    draw_rectangle(inner.x, inner.y, inner.w, inner.h, Color::new(0.0, 0.0, 0.0, 0.92));
    draw_rectangle_lines(inner.x, inner.y, inner.w, inner.h, 1.0, FAINT);
    let icons_h = 58.0;
    let body = Rect::new(inner.x + 8.0, inner.y + 8.0, inner.w - 16.0, inner.h - icons_h - 16.0);
    let e = &ac.eng;

    // ---- Colonne gauche : servitudes ----
    let lw = (body.w * 0.15).max(96.0);
    let mut y = body.y + 14.0;
    let hyd_ok = !ac.fails.has(Fail::Hyd);
    let el = &ac.elec;
    let items: [(&str, String, f32, Color); 7] = [
        ("HYD 1", if hyd_ok { "3000".into() } else { "0".into() }, if hyd_ok { 1.0 } else { 0.0 }, if hyd_ok { OKC } else { RED }),
        ("HYD 2", if hyd_ok { "2980".into() } else { "0".into() }, if hyd_ok { 0.99 } else { 0.0 }, if hyd_ok { OKC } else { RED }),
        ("GEN kW", format!("{:.0}", el.gen_kw), (el.gen_kw / 60.0) as f32, if el.gen_kw > 5.0 { OKC } else { AMBER }),
        ("BATT", format!("{:.0}%", el.batt * 100.0), el.batt as f32, if el.batt > 0.3 { OKC } else { AMBER }),
        ("PILE H2", format!("{:.0}C", el.fc_temp), if el.fc_on { (el.fc_temp / 120.0) as f32 } else { 0.0 }, if el.fc_on { CYAN_S } else { FAINT }),
        ("O2 PILE", format!("{:.0}%", el.o2 * 100.0), el.o2 as f32, if el.o2 > 0.2 { OKC } else { AMBER }),
        ("E BAY", format!("{:.0}C", el.ebay_temp), (el.ebay_temp / 80.0) as f32, if el.ebay_temp < 60.0 { OKC } else { AMBER }),
    ];
    for (l, v, f, c) in items.iter() {
        bar(body.x, y, lw - 10.0, l, v, *f, *c);
        y += 30.0;
    }
    let bus = if el.bus_ok { ("BUS ESS  OK", OKC) } else { ("BUS ESS  OFF", RED) };
    txt(bus.0, body.x, y + 4.0, 11.0, bus.1);

    // ---- Vue structure / thermique (contour orange) ----
    let colw = (body.w - lw) * 0.30;
    let left = Rect::new(body.x + lw, body.y, colw, body.h - 16.0);
    let hot = th.sensors.iter().filter(|s| !s.failed && !s.inhibited).map(|s| ((s.value - s.caution) / (s.limit - s.caution)).max(-1.0)).fold(-1.0f64, f64::max);
    let edge = if hot > 0.0 { RED } else if hot > -0.2 { AMBER } else { ORANGE };
    silhouette(left, edge, Color::new(0.0, 0.0, 0.0, 0.0));
    let to = plan(left);
    // Organes : avionique, entrées d'air, turbines, combustion stato / scram, train
    let st_turb = |i: usize| turb_state(e.side[i]).1;
    let ram_s = if e.ram_wall > crate::systems::RAM_WALL_CAUTION { 1 } else if e.ram == RamState::Lit { 0 } else { 3 };
    let scr_s = match e.scram {
        ScramState::Unstart => 2,
        ScramState::Lit if e.scram_wall > crate::systems::SCRAM_WALL_CAUTION => 1,
        ScramState::Lit => 0,
        _ => 3,
    };
    let parts: [(f32, f32, f32, f32, u8, &str); 7] = [
        (-0.05, 0.13, 0.05, 0.24, if el.bus_ok { 0 } else { 2 }, "AVIO"),
        (-0.14, 0.38, -0.08, 0.62, if ac.fails.has(Fail::InletJam) { 2 } else { 0 }, ""),
        (0.08, 0.38, 0.14, 0.62, if ac.fails.has(Fail::InletJam) { 2 } else { 0 }, ""),
        (-0.20, 0.74, -0.08, 0.94, st_turb(0), "G"),
        (0.08, 0.74, 0.20, 0.94, st_turb(1), "D"),
        (-0.06, 0.64, 0.00, 0.86, scr_s, ""),
        (0.00, 0.64, 0.06, 0.86, ram_s, ""),
    ];
    for (x0, y0, x1, y1, s, l) in parts {
        let (a, b) = (to(x0, y0), to(x1, y1));
        let rr = Rect::new(a.x, a.y, b.x - a.x, b.y - a.y);
        let c = state_color(s);
        rrect(rr, 2.0, Color::new(c.r, c.g, c.b, 0.18));
        rrect_lines(rr, 2.0, 1.2, c);
        if !l.is_empty() {
            txt_c(l, rr.x + rr.w / 2.0, rr.y + rr.h / 2.0 + 4.0, 10.0, c);
        }
    }
    // Train (trois points)
    for (x, yy) in [(0.0, 0.30), (-0.14, 0.66), (0.14, 0.66)] {
        let p = to(x, yy);
        let g = if ac.gear_pos > 0.99 { OKC } else if ac.gear_pos > 0.01 { RED } else { FAINT };
        draw_circle_lines(p.x, p.y, 3.0, 1.2, g);
    }
    for s in th.sensors.iter().filter(|s| !s.failed && !s.inhibited) {
        let p = to(s.pos.0, s.pos.1);
        let k = ((s.value - s.caution) / (s.limit - s.caution)) as f32;
        let c = if k > 0.0 { RED } else if k > -0.15 { AMBER } else { Color::new(1.0, 0.75, 0.4, 0.8) };
        draw_circle(p.x, p.y, 2.0, c);
    }
    txt_c("STRUCTURE / THERMIQUE", left.x + left.w / 2.0, left.y + left.h + 12.0, 11.0, edge);
    let skin = ac.skin_temp - 273.15;
    txt(&format!("PEAU {skin:.0}C"), left.x, left.y + 12.0, 11.0, edge);

    // ---- Centre : tableau moteurs, éventail radar, échelles ----
    let cw = (body.w - lw) * 0.40;
    let mid = Rect::new(left.x + left.w, body.y, cw, body.h);
    let cols = ["TURB G", "TURB D", "STATO", "SCRAM"];
    let col_w = (mid.w - 50.0) / 4.0;
    let tx = mid.x + 50.0;
    for (i, c) in cols.iter().enumerate() {
        txt_c(c, tx + col_w * (i as f32 + 0.5), mid.y + 12.0, 10.0, LINE);
    }
    draw_line(mid.x + 4.0, mid.y + 17.0, mid.x + mid.w - 4.0, mid.y + 17.0, 1.0, FAINT);
    let (s0, c0) = turb_state(e.side[0]);
    let (s1, c1) = turb_state(e.side[1]);
    let ram_t = match e.ram {
        RamState::Off => ("OFF", 3),
        RamState::Armed => ("ARME", 1),
        RamState::Lit => ("ALLUME", 0),
    };
    let scr_t = match e.scram {
        ScramState::Off => ("OFF", 3),
        ScramState::Armed => ("ARME", 1),
        ScramState::Lit => ("ALLUME", 0),
        ScramState::Unstart => ("DESAMORC", 2),
    };
    let rows: [(&str, [(String, u8); 4]); 4] = [
        ("ETAT", [(s0.into(), c0), (s1.into(), c1), (ram_t.0.into(), ram_t.1), (scr_t.0.into(), scr_t.1)]),
        ("N2 %", [(format!("{:.0}", e.n2s[0] * 100.0), 0), (format!("{:.0}", e.n2s[1] * 100.0), 0), ("--".into(), 3), ("--".into(), 3)]),
        ("kN", [(format!("{:.0}", e.t_turb / 2000.0), 0), (format!("{:.0}", e.t_turb / 2000.0), 0), (format!("{:.0}", e.t_ram / 1000.0), 0), (format!("{:.0}", e.t_scram / 1000.0), 0)]),
        (
            "TEMP C",
            [
                (format!("{:.0}", e.egt), (e.egt > 900.0) as u8),
                (format!("{:.0}", e.egt), (e.egt > 900.0) as u8),
                (format!("{:.0}", e.ram_wall), (e.ram_wall > crate::systems::RAM_WALL_CAUTION) as u8),
                (format!("{:.0}", e.scram_wall), (e.scram_wall > crate::systems::SCRAM_WALL_CAUTION) as u8),
            ],
        ),
    ];
    for (k, (lbl, vals)) in rows.iter().enumerate() {
        let yy = mid.y + 32.0 + k as f32 * 15.0;
        txt(lbl, mid.x + 4.0, yy, 10.0, FAINT);
        for (i, (v, s)) in vals.iter().enumerate() {
            let c = if *s == 3 { FAINT } else if *s == 0 { LINE } else { state_color(*s) };
            txt_c(v, tx + col_w * (i as f32 + 0.5), yy, 11.0, c);
        }
    }
    // Éventail radar météo (± 60°, vu de l'avion en bas)
    let fan_top = mid.y + 100.0;
    let fan_r = (mid.w * 0.42).min(body.y + body.h * 0.70 - fan_top);
    let fc = vec2(mid.x + mid.w / 2.0, fan_top + fan_r);
    let pt = |az: f32, f: f32| {
        let a = (az - 90.0).to_radians();
        vec2(fc.x + fan_r * f * a.cos(), fc.y + fan_r * f * a.sin())
    };
    wxr.draw_nd(fc, fan_r, time);
    for k in 1..=4 {
        let f = k as f32 / 4.0;
        let pts: Vec<Vec2> = (0..=24).map(|i| pt(-60.0 + i as f32 * 5.0, f)).collect();
        stroke(&pts, false, 1.0, FAINT);
    }
    for az in [-60.0f32, -40.0, -20.0, 0.0, 20.0, 40.0, 60.0] {
        let p = pt(az, 1.0);
        draw_line(fc.x, fc.y, p.x, p.y, 1.0, FAINT);
    }
    draw_triangle(fc + vec2(0.0, -8.0), fc + vec2(-6.0, 6.0), fc + vec2(6.0, 6.0), LINE);
    txt_c(&format!("WXR  {}", wxr.tilt_label()), fc.x, fc.y + 18.0, 10.0, if wxr.on { OKC } else { AMBER });
    // Échelles verticales : centrage et incidence
    let sy0 = fc.y + 30.0;
    let sy1 = (body.y + body.h - 20.0).min(sy0 + 200.0);
    if sy1 - sy0 > 40.0 {
        let cg = ac.cg() as f32;
        let alpha = ac.d.alpha.to_degrees() as f32;
        let amax = ac.fcs.alpha_max as f32;
        for (i, (lbl, v, lo, hi, okl, okh)) in [("CG %", cg, 15.0f32, 35.0f32, 21.0f32, 29.0f32), ("AoA", alpha, -5.0, amax.max(5.0) + 2.0, -2.0, amax * 0.8)].iter().enumerate() {
            let x = mid.x + mid.w * (0.32 + 0.36 * i as f32);
            draw_line(x, sy0, x, sy1, 1.2, LINE);
            let fy = |val: f32| sy1 - (val - lo) / (hi - lo) * (sy1 - sy0);
            for k in 0..=4 {
                let yy = sy0 + k as f32 * (sy1 - sy0) / 4.0;
                draw_line(x - 5.0, yy, x, yy, 1.0, FAINT);
            }
            draw_rectangle(x + 2.0, fy(*okh), 4.0, fy(*okl) - fy(*okh), Color::new(0.35, 0.95, 0.45, 0.35));
            let c = if (*okl..=*okh).contains(v) { OKC } else { AMBER };
            let yv = fy(v.clamp(*lo, *hi));
            draw_triangle(vec2(x - 2.0, yv), vec2(x - 10.0, yv - 5.0), vec2(x - 10.0, yv + 5.0), c);
            txt_c(lbl, x, sy1 + 12.0, 10.0, LINE);
            txt(&format!("{v:.1}"), x + 9.0, yv + 4.0, 10.0, c);
        }
    }

    // ---- Vue carburant (silhouette remplie) ----
    let right = Rect::new(mid.x + mid.w, body.y, body.x + body.w - (mid.x + mid.w), body.h - 16.0);
    let pr = Rect::new(right.x, right.y, right.w * 0.68, right.h);
    fuel_tanks(ac, pr);
    txt_c("CARBURANT", pr.x + pr.w / 2.0, pr.y + pr.h + 12.0, 11.0, OKC);
    // Colonne de quantités
    let qx = pr.x + pr.w + 4.0;
    let qw = right.x + right.w - qx;
    let mut qy = right.y + 16.0;
    for (l, v, c) in [
        ("JP-7", format!("{:.1} t", ac.fuel / 1000.0), OKC),
        ("H2", format!("{:.1} t", ac.h2 / 1000.0), CYAN_S),
        ("DEBIT", format!("{:.1} kg/s", e.fuel_flow), LINE),
        ("H2 DEB", format!("{:.2}", e.h2_flow), CYAN_S),
        ("CG", format!("{:.1}%", ac.cg()), if (21.0..=29.0).contains(&ac.cg()) { OKC } else { AMBER }),
        ("PRESS", format!("{:.0} psi", ac.fuel_psi()), if ac.fuel_psi() > 40.0 { OKC } else { AMBER }),
    ] {
        txt(l, qx, qy, 10.0, FAINT);
        txt_r(&v, qx + qw, qy + 13.0, 11.0, c);
        qy += 32.0;
    }
    if ac.fuel_dump || ac.h2_dump {
        if (time * 2.0).fract() < 0.6 {
            txt("VIDANGE", qx, qy + 6.0, 12.0, AMBER);
        }
    }

    // ---- Rangée d'icônes de pages ----
    let icons: [(Tab, &str); 8] = [
        (Tab::Moteurs, "MOT"),
        (Tab::Alarmes, "ALM"),
        (Tab::Fcs, "FCS"),
        (Tab::Limites, "LIM"),
        (Tab::Elec, "ELEC"),
        (Tab::Thermique, "THERM"),
        (Tab::Recon, "RECON"),
        (Tab::Armement, "ARM"),
    ];
    let iy = inner.y + inner.h - icons_h - 4.0;
    let iw = (inner.w - 16.0) / icons.len() as f32;
    let mut out = None;
    for (i, (tab, lbl)) in icons.iter().enumerate() {
        let b = Rect::new(inner.x + 8.0 + i as f32 * iw + 2.0, iy, iw - 4.0, icons_h);
        let hov = hovered(b);
        draw_rectangle(b.x, b.y, b.w, b.h, if hov { Color::new(0.08, 0.1, 0.12, 1.0) } else { BLACK });
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.2, if hov { ACCENT } else { LINE });
        icon(*tab, vec2(b.x + b.w / 2.0, b.y + b.h * 0.42), b.h * 0.26);
        txt_c(lbl, b.x + b.w / 2.0, b.y + b.h - 6.0, 9.0, FAINT);
        if clicked(b) {
            out = Some(*tab);
        }
    }
    out
}

/// Pictogrammes en traits fins.
fn icon(t: Tab, c: Vec2, s: f32) {
    let w = 1.4;
    match t {
        Tab::Moteurs => {
            rrect_lines(Rect::new(c.x - s, c.y - s * 0.45, s * 2.0, s * 0.9), s * 0.4, w, LINE);
            draw_line(c.x - s * 0.4, c.y - s * 0.45, c.x - s * 0.4, c.y + s * 0.45, w, LINE);
            draw_line(c.x + s * 0.3, c.y - s * 0.45, c.x + s * 0.3, c.y + s * 0.45, w, LINE);
        }
        Tab::Alarmes => {
            stroke(&[c + vec2(0.0, -s), c + vec2(s, s * 0.8), c + vec2(-s, s * 0.8)], true, w, LINE);
            draw_line(c.x, c.y - s * 0.35, c.x, c.y + s * 0.3, w, LINE);
            draw_circle(c.x, c.y + s * 0.55, 1.5, LINE);
        }
        Tab::Fcs => {
            stroke(&[c + vec2(0.0, -s), c + vec2(s, s * 0.7), c + vec2(-s, s * 0.7)], true, w, LINE);
            draw_line(c.x - s * 0.8, c.y + s * 0.9, c.x - s * 0.2, c.y + s * 0.9, 2.0, CYAN_S);
            draw_line(c.x + s * 0.2, c.y + s * 0.9, c.x + s * 0.8, c.y + s * 0.9, 2.0, CYAN_S);
        }
        Tab::Limites => {
            let pts: Vec<Vec2> = (0..=12).map(|i| {
                let a = (200.0 + i as f32 * 140.0 / 12.0).to_radians();
                c + vec2(a.cos(), a.sin()) * s
            }).collect();
            stroke(&pts, false, w, LINE);
            draw_line(c.x, c.y, c.x + s * 0.6, c.y - s * 0.6, w, AMBER);
        }
        Tab::Elec => {
            stroke(&[c + vec2(s * 0.3, -s), c + vec2(-s * 0.4, s * 0.1), c + vec2(s * 0.2, s * 0.1), c + vec2(-s * 0.3, s)], false, w + 0.4, LINE);
        }
        Tab::Thermique => {
            rrect_lines(Rect::new(c.x - s * 0.22, c.y - s, s * 0.44, s * 1.5), s * 0.2, w, LINE);
            draw_circle_lines(c.x, c.y + s * 0.7, s * 0.35, w, LINE);
            draw_circle(c.x, c.y + s * 0.7, s * 0.2, RED);
        }
        Tab::Recon => {
            draw_circle_lines(c.x, c.y, s, w, LINE);
            draw_circle_lines(c.x, c.y, s * 0.45, w, LINE);
            for k in 0..6 {
                let a = (k as f32 * 60.0).to_radians();
                draw_line(c.x + a.cos() * s * 0.45, c.y + a.sin() * s * 0.45, c.x + (a + 0.6).cos() * s, c.y + (a + 0.6).sin() * s, 1.0, LINE);
            }
        }
        _ => {
            draw_circle_lines(c.x, c.y, s * 0.8, w, LINE);
            draw_line(c.x - s, c.y, c.x + s, c.y, w, LINE);
            draw_line(c.x, c.y - s, c.x, c.y + s, w, LINE);
        }
    }
}
