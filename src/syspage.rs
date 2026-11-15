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

/// Symbole avion : silhouette du Darkstar pleine, centrée en `c`, longueur `len` px, cap `hdg` (rad,
/// 0 = vers le haut de l'écran, sens horaire).
pub fn mini(c: Vec2, len: f32, hdg: f32, fill: Color, edge: Color) {
    let (sn, cs) = hdg.sin_cos();
    let pts: Vec<Vec2> = OUTLINE
        .iter()
        .map(|&(x, y)| {
            let (lx, ly) = (x * len * 0.5, (y - 0.55) * len);
            c + vec2(lx * cs - ly * sn, lx * sn + ly * cs)
        })
        .collect();
    let ctr = c + vec2(-(0.07 * len) * sn, 0.07 * len * cs);
    for i in 0..pts.len() {
        draw_triangle(ctr, pts[i], pts[(i + 1) % pts.len()], fill);
    }
    stroke(&pts, true, 1.2, edge);
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

/// Cadre de section titré ; renvoie la zone intérieure.
fn card(r: Rect, title: &str, c: Color) -> Rect {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.025, 0.03, 1.0));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.22, 0.24, 0.27, 1.0));
    draw_rectangle(r.x, r.y, 3.0, 16.0, c);
    txt(title, r.x + 9.0, r.y + 13.0, 10.0, c);
    Rect::new(r.x + 8.0, r.y + 20.0, r.w - 16.0, r.h - 26.0)
}

/// Anomalies en cours (synthèse en tête de page).
fn anomalies(ac: &Aircraft, th: &crate::thermal::ThermalSys) -> Vec<(&'static str, u8)> {
    let e = &ac.eng;
    let el = &ac.elec;
    let mut v = Vec::new();
    if ac.fails.has(Fail::Hyd) {
        v.push(("HYD", 2));
    }
    if !el.bus_ok {
        v.push(("BUS ESS", 2));
    } else if el.batt < 0.3 && el.gen_kw < 5.0 {
        v.push(("BATT", 1));
    }
    if !ac.on_ground && el.gen_kw < 5.0 {
        v.push(("GEN", 1));
    }
    if el.ebay_temp > 60.0 {
        v.push(("E BAY", 1));
    }
    for i in 0..2 {
        if e.side[i] == TurbState::Failed {
            v.push((["TURB G", "TURB D"][i], 2));
        }
    }
    if e.scram == ScramState::Unstart {
        v.push(("SCRAM", 2));
    }
    if ac.fails.has(Fail::InletJam) {
        v.push(("ENTREES", 2));
    }
    if th.sensors.iter().any(|s| !s.failed && !s.inhibited && s.value > s.caution) {
        v.push(("THERMIQUE", 1));
    }
    if !(21.0..=29.0).contains(&ac.cg()) {
        v.push(("CG", 1));
    }
    if ac.fuel_dump || ac.h2_dump {
        v.push(("VIDANGE", 1));
    }
    v
}

/// Vue en plan structure / thermique : organes colorés, train, capteurs.
fn structure(ac: &Aircraft, th: &crate::thermal::ThermalSys, area: Rect) -> Color {
    let e = &ac.eng;
    let el = &ac.elec;
    let hot = th.sensors.iter().filter(|s| !s.failed && !s.inhibited).map(|s| ((s.value - s.caution) / (s.limit - s.caution)).max(-1.0)).fold(-1.0f64, f64::max);
    let edge = if hot > 0.0 { RED } else if hot > -0.2 { AMBER } else { ORANGE };
    silhouette(area, edge, Color::new(0.0, 0.0, 0.0, 0.0));
    let to = plan(area);
    let st_turb = |i: usize| turb_state(e.side[i]).1;
    let ram_s = if e.ram_wall > crate::systems::RAM_WALL_CAUTION { 1 } else if e.ram == RamState::Lit { 0 } else { 3 };
    let scr_s = match e.scram {
        ScramState::Unstart => 2,
        ScramState::Lit if e.scram_wall > crate::systems::SCRAM_WALL_CAUTION => 1,
        ScramState::Lit => 0,
        _ => 3,
    };
    let parts: [(f32, f32, f32, f32, u8, &str); 7] = [
        (-0.05, 0.13, 0.05, 0.24, if el.bus_ok { 0 } else { 2 }, "AV"),
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
        if !l.is_empty() && rr.h > 12.0 {
            txt_c(l, rr.x + rr.w / 2.0, rr.y + rr.h / 2.0 + 4.0, 9.0, c);
        }
    }
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
    edge
}

/// Tableau propulsion : turbines G / D, stato, scram.
fn propulsion(ac: &Aircraft, r: Rect) {
    let e = &ac.eng;
    let cols = ["TURB G", "TURB D", "STATO", "SCRAM"];
    let lab_w = 52.0;
    let col_w = (r.w - lab_w) / 4.0;
    let tx = r.x + lab_w;
    let hs = if col_w < 52.0 { 9.0 } else { 10.0 };
    for (i, c) in cols.iter().enumerate() {
        txt_c(c, tx + col_w * (i as f32 + 0.5), r.y + 10.0, hs, LINE);
    }
    draw_line(r.x, r.y + 15.0, r.x + r.w, r.y + 15.0, 1.0, FAINT);
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
        ScramState::Unstart => ("DESAM.", 2),
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
    let rh = ((r.h - 18.0) / 4.0).min(17.0);
    for (k, (lbl, vals)) in rows.iter().enumerate() {
        let yy = r.y + 18.0 + (k as f32 + 0.75) * rh;
        txt(lbl, r.x, yy, 10.0, FAINT);
        for (i, (v, s)) in vals.iter().enumerate() {
            let c = if *s == 3 { FAINT } else if *s == 0 { LINE } else { state_color(*s) };
            txt_c(v, tx + col_w * (i as f32 + 0.5), yy, 11.0, c);
        }
    }
}

/// Jauge horizontale avec plage normale (vert) et repère de valeur.
fn hgauge(x: f32, y: f32, w: f32, lbl: &str, v: f32, lo: f32, hi: f32, okl: f32, okh: f32) {
    let ok = (okl..=okh).contains(&v);
    let c = if ok { OKC } else { AMBER };
    txt(lbl, x, y, 10.0, LINE);
    txt_r(&format!("{v:.1}"), x + w, y, 11.0, c);
    let by = y + 6.0;
    let fx = |val: f32| x + (val - lo) / (hi - lo) * w;
    draw_line(x, by + 4.0, x + w, by + 4.0, 1.0, FAINT);
    draw_rectangle(fx(okl), by, fx(okh) - fx(okl), 8.0, Color::new(0.35, 0.95, 0.45, 0.3));
    let vx = fx(v.clamp(lo, hi));
    draw_triangle(vec2(vx, by + 9.0), vec2(vx - 5.0, by + 16.0), vec2(vx + 5.0, by + 16.0), c);
    draw_line(vx, by - 1.0, vx, by + 9.0, 2.0, c);
}

pub fn draw(ac: &Aircraft, th: &crate::thermal::ThermalSys, wxr: &crate::wxr::Wxr, r: Rect, time: f64) -> Option<Tab> {
    panel(r, "SYSTEMES — SYNOPTIQUE");
    let inner = Rect::new(r.x + 8.0, r.y + 26.0, r.w - 16.0, r.h - 34.0);
    draw_rectangle(inner.x, inner.y, inner.w, inner.h, Color::new(0.0, 0.0, 0.0, 0.92));
    let e = &ac.eng;
    let el = &ac.elec;
    let gap = 6.0;
    // ---- Synthèse ----
    let an = anomalies(ac, th);
    let sb = Rect::new(inner.x + 4.0, inner.y + 4.0, inner.w - 8.0, 22.0);
    let worst = an.iter().map(|a| a.1).max().unwrap_or(0);
    let sc = state_color(worst);
    draw_rectangle(sb.x, sb.y, sb.w, sb.h, Color::new(sc.r * 0.15, sc.g * 0.15, sc.b * 0.15, 1.0));
    draw_rectangle_lines(sb.x, sb.y, sb.w, sb.h, 1.0, sc);
    let msg = if an.is_empty() { "SYSTEMES NOMINAUX".to_string() } else { format!("{} ANOMALIE{} : {}", an.len(), if an.len() > 1 { "S" } else { "" }, an.iter().map(|a| a.0).collect::<Vec<_>>().join("  ")) };
    txt_c(&msg, sb.x + sb.w / 2.0, sb.y + 15.0, 11.0, sc);
    let icons_h = (inner.h * 0.08).clamp(38.0, 56.0);
    let body = Rect::new(inner.x + 4.0, sb.y + sb.h + gap, inner.w - 8.0, inner.y + inner.h - icons_h - 8.0 - (sb.y + sb.h + gap));
    let wide = body.w > body.h * 1.25;
    // Disposition des sections
    let (r_serv, r_str, r_prop, r_fuel, r_wx) = if wide {
        let c1 = body.w * 0.18;
        let c2 = body.w * 0.26;
        let c4 = body.w * 0.26;
        let c3 = body.w - c1 - c2 - c4 - 3.0 * gap;
        let x2 = body.x + c1 + gap;
        let x3 = x2 + c2 + gap;
        let x4 = x3 + c3 + gap;
        let ph = 100.0f32.min(body.h * 0.3);
        (
            Rect::new(body.x, body.y, c1, body.h),
            Rect::new(x2, body.y, c2, body.h),
            Rect::new(x3, body.y, c3, ph),
            Rect::new(x4, body.y, c4, body.h),
            Rect::new(x3, body.y + ph + gap, c3, body.h - ph - gap),
        )
    } else {
        let h1 = body.h * 0.38;
        let ph = 96.0;
        let h3 = body.h - h1 - ph - 2.0 * gap;
        let half = (body.w - gap) / 2.0;
        let y2 = body.y + h1 + gap;
        let y3 = y2 + ph + gap;
        (
            Rect::new(body.x, body.y, half, h1),
            Rect::new(body.x + half + gap, body.y, half, h1),
            Rect::new(body.x, y2, body.w, ph),
            Rect::new(body.x, y3, half, h3),
            Rect::new(body.x + half + gap, y3, half, h3),
        )
    };
    // ---- Servitudes ----
    let a = card(r_serv, "SERVITUDES", LINE);
    let hyd_ok = !ac.fails.has(Fail::Hyd);
    let items: [(&str, String, f32, Color); 7] = [
        ("HYD 1", if hyd_ok { "3000".into() } else { "0".into() }, if hyd_ok { 1.0 } else { 0.0 }, if hyd_ok { OKC } else { RED }),
        ("HYD 2", if hyd_ok { "2980".into() } else { "0".into() }, if hyd_ok { 0.99 } else { 0.0 }, if hyd_ok { OKC } else { RED }),
        ("GEN kW", format!("{:.0}", el.gen_kw), (el.gen_kw / 60.0) as f32, if el.gen_kw > 5.0 { OKC } else { AMBER }),
        ("BATT", format!("{:.0}%", el.batt * 100.0), el.batt as f32, if el.batt > 0.3 { OKC } else { AMBER }),
        ("PILE H2", format!("{:.0}C", el.fc_temp), if el.fc_on { (el.fc_temp / 120.0) as f32 } else { 0.0 }, if el.fc_on { CYAN_S } else { FAINT }),
        ("O2 PILE", format!("{:.0}%", el.o2 * 100.0), el.o2 as f32, if el.o2 > 0.2 { OKC } else { AMBER }),
        ("E BAY", format!("{:.0}C", el.ebay_temp), (el.ebay_temp / 80.0) as f32, if el.ebay_temp < 60.0 { OKC } else { AMBER }),
    ];
    let cols = if a.w > 260.0 { 2 } else { 1 };
    let bw = (a.w - (cols as f32 - 1.0) * 12.0) / cols as f32;
    let rows = items.len().div_ceil(cols);
    let step = ((a.h - 6.0) / rows as f32).clamp(16.0, 30.0);
    for (k, (l, v, f, c)) in items.iter().enumerate() {
        bar(a.x + (k % cols) as f32 * (bw + 12.0), a.y + 10.0 + (k / cols) as f32 * step, bw, l, v, *f, *c);
    }
    let bus = if el.bus_ok { ("BUS ESS OK", OKC) } else { ("BUS ESS OFF", RED) };
    txt_r(bus.0, a.x + a.w, a.y - 7.0, 10.0, bus.1);
    // ---- Structure / thermique ----
    let skin = ac.skin_temp - 273.15;
    let a = card(r_str, "STRUCTURE / THERMIQUE", ORANGE);
    let edge = structure(ac, th, Rect::new(a.x, a.y + 12.0, a.w, a.h - 12.0));
    txt_r(&format!("PEAU {skin:.0} C"), a.x + a.w, a.y - 7.0, 10.0, edge);
    // ---- Propulsion ----
    let a = card(r_prop, "PROPULSION", LINE);
    propulsion(ac, a);
    // ---- Carburant ----
    let a = card(r_fuel, "CARBURANT", OKC);
    let qw = (a.w * 0.36).clamp(70.0, 110.0);
    fuel_tanks(ac, Rect::new(a.x, a.y, a.w - qw - 4.0, a.h));
    let qx = a.x + a.w - qw;
    let qitems = [
        ("JP-7", format!("{:.1} t", ac.fuel / 1000.0), OKC),
        ("H2", format!("{:.1} t", ac.h2 / 1000.0), CYAN_S),
        ("DEBIT", format!("{:.1} kg/s", e.fuel_flow), LINE),
        ("H2 DEB", format!("{:.2}", e.h2_flow), CYAN_S),
        ("PRESS", format!("{:.0} psi", ac.fuel_psi()), if ac.fuel_psi() > 40.0 { OKC } else { AMBER }),
    ];
    let qs = ((a.h - 10.0) / qitems.len() as f32).clamp(22.0, 32.0);
    let mut qy = a.y + 8.0;
    for (l, v, c) in qitems {
        txt(l, qx, qy, 9.0, FAINT);
        txt_r(&v, qx + qw, qy + 12.0, 11.0, c);
        qy += qs;
    }
    if (ac.fuel_dump || ac.h2_dump) && (time * 2.0).fract() < 0.6 {
        txt("VIDANGE", qx, qy + 4.0, 11.0, AMBER);
    }
    // ---- Radar météo + centrage / incidence ----
    let a = card(r_wx, "RADAR METEO / CENTRAGE", LINE);
    let gh = 64.0;
    let fan_r = (a.w * 0.45).min(a.h - gh - 26.0).max(20.0);
    let fc = vec2(a.x + a.w / 2.0, a.y + 4.0 + fan_r);
    let pt = |az: f32, f: f32| {
        let an = (az - 90.0).to_radians();
        vec2(fc.x + fan_r * f * an.cos(), fc.y + fan_r * f * an.sin())
    };
    wxr.draw_nd(fc, fan_r, time);
    for k in 1..=4 {
        let f = k as f32 / 4.0;
        let pts: Vec<Vec2> = (0..=24).map(|i| pt(-60.0 + i as f32 * 5.0, f)).collect();
        stroke(&pts, false, 1.0, FAINT);
    }
    for az in [-60.0f32, -30.0, 0.0, 30.0, 60.0] {
        let p = pt(az, 1.0);
        draw_line(fc.x, fc.y, p.x, p.y, 1.0, FAINT);
    }
    draw_triangle(fc + vec2(0.0, -7.0), fc + vec2(-5.0, 5.0), fc + vec2(5.0, 5.0), LINE);
    txt_c(&format!("WXR  {}", wxr.tilt_label()), fc.x, fc.y + 16.0, 10.0, if wxr.on { OKC } else { AMBER });
    let gy = a.y + a.h - gh + 8.0;
    let amax = ac.fcs.alpha_max as f32;
    hgauge(a.x, gy, a.w, "CENTRAGE CG %", ac.cg() as f32, 15.0, 35.0, 21.0, 29.0);
    hgauge(a.x, gy + 32.0, a.w, "INCIDENCE AoA deg", ac.d.alpha.to_degrees() as f32, -5.0, amax.max(5.0) + 2.0, -2.0, amax * 0.8);

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
    let iw = (inner.w - 8.0) / icons.len() as f32;
    let mut out = None;
    for (i, (tab, lbl)) in icons.iter().enumerate() {
        let b = Rect::new(inner.x + 4.0 + i as f32 * iw + 2.0, iy, iw - 4.0, icons_h);
        let hov = hovered(b);
        draw_rectangle(b.x, b.y, b.w, b.h, if hov { Color::new(0.08, 0.1, 0.12, 1.0) } else { Color::new(0.02, 0.025, 0.03, 1.0) });
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, if hov { ACCENT } else { Color::new(0.3, 0.32, 0.35, 1.0) });
        icon(*tab, vec2(b.x + b.w / 2.0, b.y + b.h * 0.42), b.h * 0.24);
        txt_c(lbl, b.x + b.w / 2.0, b.y + b.h - 5.0, 9.0, FAINT);
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
