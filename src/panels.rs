//! Panneaux de la station sol : EICAS propulsion, carte d'essais, domaine de vol, messages.

use crate::aircraft::*;
use crate::atmosphere::altitude_for_q;
use crate::geo::*;
use crate::mission::*;
use crate::ui::*;
use crate::ui::wrap_text;
use macroquad::prelude::*;

fn f_of(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Texte suivi d'un petit cercle « degré » (la police ne contient que l'ASCII).
fn deg_txt(s: &str, x: f32, y: f32, size: f32, c: Color, unit: &str) {
    let w = txt_w(s, size);
    txt(s, x, y, size, c);
    draw_circle_lines(x + w + size * 0.16, y - size * 0.66, size * 0.075, 1.2, c);
    txt(unit, x + w + size * 0.30, y, size, c);
}

/// Nacelle TBCC reproduite d'après l'écran moteurs du Darkstar : silhouette grise, turbine claire
/// (cône, GEN, RPM), conduit inférieur stato/scram (CIT), portes et repères orange entrée/tuyère.
fn nacelle(r: Rect, side: &str, ac: &Aircraft, i: usize, egt_off: f64, time: f64) {
    let e = &ac.eng;
    let running = e.side[i] == TurbState::Running;
    // Coordonnées de la photo (1200 x 250 par nacelle) -> écran
    let sx = r.w / 1200.0;
    let sy = r.h / 250.0;
    let p = |x: f32, y: f32| vec2(r.x + x * sx, r.y + y * sy);
    let fs = (r.h / 250.0 * 34.0).clamp(10.0, 22.0);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.135, 0.14, 0.145, 1.0));
    let label = Color::new(0.62, 0.64, 0.66, 1.0);
    txt("FWD", p(30.0, 0.0).x, p(0.0, 42.0).y, fs * 0.75, label);
    txt_r("AFT", p(1170.0, 0.0).x, p(0.0, 42.0).y, fs * 0.75, label);
    txt_c(side, r.x + r.w * 0.5, p(0.0, 42.0).y, fs * 0.6, label);

    // Contours arrondis (lissage de Chaikin) : formes fermées remplies, tracés ouverts
    let rad = (r.h * 0.07).max(3.0);
    let shape = |pts: &[(f32, f32)]| -> Vec<Vec2> { round_corners(&pts.iter().map(|&(x, y)| p(x, y)).collect::<Vec<_>>(), true, rad) };
    let poly = |pts: &[(f32, f32)], c: Color| fill_shape(&shape(pts), c);
    let outline = |pts: &[(f32, f32)], th: f32, c: Color| stroke(&smooth(&pts.iter().map(|&(x, y)| p(x, y)).collect::<Vec<_>>(), false, 2), false, th, c);
    let edge = Color::new(0.78, 0.80, 0.82, 1.0);
    let body = Color::new(0.25, 0.26, 0.27, 1.0);

    // Silhouette supérieure (veine turbine) : polygone convexe découpé en éventail
    let top = [(175.0, 105.0), (300.0, 88.0), (480.0, 62.0), (700.0, 50.0), (900.0, 64.0), (1010.0, 84.0), (1080.0, 108.0)];
    let mut sil: Vec<(f32, f32)> = top.to_vec();
    sil.extend_from_slice(&[(1040.0, 182.0), (800.0, 160.0), (420.0, 160.0), (210.0, 120.0)]);
    poly(&sil, body);
    outline(&top, 1.6, edge);
    // Rampe d'entrée vers le conduit inférieur
    poly(&[(205.0, 118.0), (420.0, 160.0), (420.0, 230.0), (360.0, 218.0)], Color::new(0.21, 0.22, 0.23, 1.0));

    // Conduit inférieur (stato / scram)
    let duct = [(372.0, 226.0), (410.0, 172.0), (1000.0, 162.0), (1052.0, 180.0), (1030.0, 205.0)];
    let lower = if e.scram == ScramState::Lit {
        Some((MAGENTA_S, "SCRAM", e.t_scram))
    } else if e.ram == RamState::Lit {
        Some((CYAN_S, "STATO", e.t_ram))
    } else {
        None
    };
    let duct_shape = shape(&duct);
    match lower {
        None => fill_shape(&duct_shape, Color::new(0.20, 0.205, 0.21, 1.0)),
        Some((c, name, thrust)) => {
            // Dégradé le long de la veine : entrée froide -> chambre de combustion incandescente -> tuyère
            let (xa, xb) = (p(410.0, 0.0).x, p(1050.0, 0.0).x);
            let heat = |t: f32| -> Color {
                let glow = (-((t - 0.68) / 0.18).powi(2)).exp();
                let base = 0.16 + 0.42 * t;
                Color::new(
                    (c.r * base + glow * (0.35 + c.r * 0.5)).min(1.0),
                    (c.g * base + glow * (0.30 + c.g * 0.45)).min(1.0),
                    (c.b * base + glow * (0.30 + c.b * 0.5)).min(1.0),
                    1.0,
                )
            };
            fill_shape_colored(&duct_shape, |q| heat(((q.x - xa) / (xb - xa)).clamp(0.0, 1.0)));
            // Halo de combustion
            let gc = p(820.0, 192.0);
            for k in 0..3 {
                draw_circle(gc.x, gc.y, (8.0 + k as f32 * 5.0) * sy, Color::new(1.0, 1.0, 1.0, 0.07));
            }
            // Particules d'écoulement sur trois filets, vitesse liée à la poussée
            let speed = 0.6 + (thrust / 200_000.0).clamp(0.0, 1.5);
            for (lane, yl) in [184.0f32, 194.0, 204.0].iter().enumerate() {
                for k in 0..9 {
                    let t = ((k as f64 / 9.0 + time * speed * 0.45 + lane as f64 * 0.13).fract()) as f32;
                    let x0 = 440.0 + t * 560.0;
                    let alpha = (t * 3.0).min(1.0) * (1.0 - t).powf(0.5) * 0.9;
                    round_line(p(x0, *yl), p(x0 + 34.0, *yl - 1.0), 1.6, Color::new(1.0, 1.0, 1.0, alpha * 0.7));
                }
            }
            // Jet : panache dégradé qui s'efface, losanges de choc pour le scram
            let fl = 0.9 + 0.1 * ((time * 21.0 + i as f64).sin() as f32);
            let n0 = p(1046.0, 176.0);
            let n1 = p(1046.0, 202.0);
            let tip = p(1046.0 + 150.0 * fl, 190.0);
            let vx = |q: Vec2, c: Color| macroquad::models::Vertex::new(q.x, q.y, 0.0, 0.0, 0.0, c);
            draw_mesh(&Mesh { vertices: vec![vx(n0, Color::new(c.r, c.g, c.b, 0.85)), vx(n1, Color::new(c.r, c.g, c.b, 0.85)), vx(tip, Color::new(c.r, c.g, c.b, 0.0))], indices: vec![0, 1, 2], texture: None });
            let core_tip = p(1046.0 + 80.0 * fl, 189.0);
            let (m0, m1) = (n0 + (n1 - n0) * 0.3, n0 + (n1 - n0) * 0.7);
            draw_mesh(&Mesh { vertices: vec![vx(m0, Color::new(1.0, 1.0, 1.0, 0.7)), vx(m1, Color::new(1.0, 1.0, 1.0, 0.7)), vx(core_tip, Color::new(1.0, 1.0, 1.0, 0.0))], indices: vec![0, 1, 2], texture: None });
            if name == "SCRAM" {
                for k in 0..3 {
                    let dc = p(1075.0 + k as f32 * 34.0 * fl, 189.5);
                    let (dw, dh) = (9.0 * sx * 2.0, 5.0 * sy * 2.0 * (1.0 - k as f32 * 0.2));
                    let pts = [vec2(dc.x - dw, dc.y), vec2(dc.x, dc.y - dh), vec2(dc.x + dw, dc.y), vec2(dc.x, dc.y + dh)];
                    stroke(&pts, true, 1.2, Color::new(1.0, 1.0, 1.0, 0.65 - k as f32 * 0.18));
                }
            }
            // Pastille mode / poussée
            let label = format!("{name}  {:.0} kN", thrust / 2000.0);
            let lw = txt_w(&label, fs * 0.55) + 14.0;
            let lc = p(905.0, 0.0).x;
            let br = Rect::new(lc - lw / 2.0, p(0.0, 178.0).y, lw, (24.0 * sy).max(12.0));
            rrect(br, br.h * 0.5, Color::new(0.04, 0.05, 0.06, 0.75));
            rrect_lines(br, br.h * 0.5, 1.2, c);
            txt_c(&label, lc, br.y + br.h * 0.5 + fs * 0.2, fs * 0.55, WHITE);
        }
    }
    stroke(&duct_shape, true, 1.6, edge);
    // CIT : température d'entrée compresseur (portes fermées = air de la turbine isolé)
    let cit_c = (ac.d.oat + 273.15) * (1.0 + 0.2 * ac.d.mach * ac.d.mach) - 273.15;
    let cit_show = ac.d.oat + (cit_c - ac.d.oat) * (1.0 - e.doors * 0.94);
    let cc = if cit_show > 430.0 { RED } else if cit_show > 400.0 { AMBER } else { WHITE };
    let cit_s = format!("{:.0}", f_of(cit_show));
    deg_txt(&cit_s, p(620.0, 0.0).x - txt_w(&cit_s, fs) * 0.6, p(0.0, 214.0).y, fs, cc, "F");

    // Turbine : cône, carter clair avec bagues, bride, tuyère
    let core_c = if running { Color::new(0.66, 0.67, 0.68, 1.0) } else { Color::new(0.42, 0.43, 0.44, 1.0) };
    let ring_c = Color::new(core_c.r * 0.78, core_c.g * 0.78, core_c.b * 0.78, 1.0);
    poly(&[(392.0, 105.0), (430.0, 92.0), (470.0, 84.0), (470.0, 126.0), (430.0, 118.0)], core_c);
    let c0 = p(462.0, 80.0);
    let c1 = p(812.0, 130.0);
    rrect(Rect::new(c0.x, c0.y, c1.x - c0.x, c1.y - c0.y), (c1.y - c0.y) * 0.22, core_c);
    for xr in [478.0, 525.0, 640.0, 700.0, 790.0] {
        round_line(p(xr, 83.0), p(xr, 127.0), 1.6, ring_c);
    }
    let f0 = p(812.0, 64.0);
    let f1 = p(826.0, 148.0);
    rrect(Rect::new(f0.x, f0.y, f1.x - f0.x, f1.y - f0.y), (f1.x - f0.x) * 0.45, core_c);
    poly(&[(826.0, 82.0), (905.0, 92.0), (905.0, 118.0), (826.0, 128.0)], ring_c);
    // Repères orange : tuyère turbine, entrée, sortie
    let orange = Color::new(0.93, 0.55, 0.16, 1.0);
    let orange_dim = Color::new(0.45, 0.30, 0.15, 1.0);
    let ab = e.ab_frac > 0.0 && running;
    for (a, b) in [((850.0, 92.0), (915.0, 92.0)), ((850.0, 118.0), (915.0, 118.0))] {
        round_line(p(a.0, a.1), p(b.0, b.1), 2.2, if running { orange } else { orange_dim });
    }
    let inlet_on = running || lower.is_some();
    for (a, b) in [((180.0, 158.0), (270.0, 142.0)), ((185.0, 182.0), (275.0, 166.0))] {
        round_line(p(a.0, a.1), p(b.0, b.1), 2.4, if inlet_on { orange } else { orange_dim });
    }
    let fl = if ab { 1.0 + 0.6 * e.ab_frac as f32 * (0.85 + 0.15 * (time * 23.0).sin() as f32) } else { 1.0 };
    for (a, b) in [((1062.0, 118.0), (1145.0, 130.0)), ((1058.0, 146.0), (1140.0, 160.0))] {
        let pa = p(a.0, a.1);
        let pb = p(a.0 + (b.0 - a.0) * fl, a.1 + (b.1 - a.1) * fl);
        round_line(pa, pb, 2.4, if running { orange } else { orange_dim });
    }
    // GEN
    let gen_fail = ac.fails.has(if i == 0 { crate::systems::Fail::GenL } else { crate::systems::Fail::GenR });
    let gen_on = running && e.n2s[i] > 0.55 && !gen_fail;
    let g0 = p(548.0, 88.0);
    let g1 = p(682.0, 122.0);
    let gr = Rect::new(g0.x, g0.y, g1.x - g0.x, g1.y - g0.y);
    let (gtxt, gcol, gbg) = if gen_fail {
        ("GEN FAIL", RED, Color::new(0.35, 0.06, 0.05, 1.0))
    } else if gen_on {
        ("GEN ON", orange, Color::new(0.38, 0.22, 0.12, 1.0))
    } else {
        ("GEN OFF", Color::new(0.55, 0.56, 0.57, 1.0), Color::new(0.30, 0.31, 0.32, 1.0))
    };
    rrect(gr, 3.0, gbg);
    rrect_lines(gr, 3.0, 1.5, gcol);
    txt_c(gtxt, gr.x + gr.w / 2.0, gr.y + gr.h * 0.5 + fs * 0.27, fs * 0.72, gcol);
    // RPM
    let rpm = e.n2s[i] * 104.0;
    let rc = if rpm > 106.0 { RED } else if rpm > 102.0 { AMBER } else { WHITE };
    txt_c("RPM", p(745.0, 0.0).x, p(0.0, 92.0).y, fs * 0.45, Color::new(0.25, 0.26, 0.27, 1.0));
    let rs = format!("{rpm:.0}%");
    txt_c(&rs, p(745.0, 0.0).x + 1.0, p(0.0, 125.0).y + 1.0, fs * 1.05, Color::new(0.2, 0.2, 0.2, 0.8));
    txt_c(&rs, p(745.0, 0.0).x, p(0.0, 125.0).y, fs * 1.05, rc);
    // EGT
    let egt_f = f_of(e.egt + egt_off);
    let ec = if e.egt > 1000.0 { RED } else if e.egt > 900.0 { AMBER } else { WHITE };
    deg_txt(&format!("{egt_f:.0}"), p(940.0, 0.0).x, p(0.0, 120.0).y, fs, ec, "F");
    // Portes
    let door_deg = e.doors * 90.0;
    let dc = if ac.fails.has(crate::systems::Fail::InletJam) { RED } else if e.doors > 0.01 && e.doors < 0.99 { AMBER } else { WHITE };
    txt("DOORS", p(78.0, 0.0).x, p(0.0, 152.0).y, fs * 0.42, label);
    deg_txt(&format!("{door_deg:02.0}"), p(82.0, 0.0).x, p(0.0, 196.0).y, fs * 1.05, dc, "");
}

/// Paramètres détaillés : turbines G/D (N1, N2, EGT, débit, huile, vibrations, tuyère), statoréacteur,
/// scramjet et entrée d'air (température d'arrêt, rendement de prise d'air, rampe), performances.
/// Renvoie la hauteur utilisée.
fn engine_table(ac: &Aircraft, r: Rect) -> f32 {
    let e = &ac.eng;
    let d = &ac.d;
    let row = 15.0;
    let lab = Color::new(0.55, 0.6, 0.65, 1.0);
    let grid = Color::new(0.18, 0.2, 0.22, 1.0);
    rrect(r, 5.0, Color::new(0.035, 0.04, 0.045, 1.0));
    rrect_lines(r, 5.0, 1.0, grid);
    // --- Turbines ---
    let w1 = r.w * 0.46;
    let (c0, c1, c2) = (r.x + 8.0, r.x + w1 * 0.62, r.x + w1 - 6.0);
    let mut y = r.y + 14.0;
    txt("TURBINES", c0, y, 11.0, CYAN_S);
    txt_r("L", c1, y, 11.0, CYAN_S);
    txt_r("R", c2, y, 11.0, CYAN_S);
    y += row;
    let thrust_sum = (e.t_turb + e.t_ram).max(1.0);
    let ff_turb = e.fuel_flow * (e.t_turb / thrust_sum);
    let tside = (e.t_side[0] + e.t_side[1]).max(1.0);
    let run = |k: usize| e.side[k] == TurbState::Running;
    let n2 = |k: usize| e.n2s[k];
    let stag = (d.oat + 273.15) * (1.0 + 0.2 * d.mach * d.mach) - 273.15;
    type Cell = (String, Color);
    let col_val = |v: f64, warn: f64, lim: f64| if v > lim { RED } else if v > warn { AMBER } else { WHITE };
    let rows: Vec<(&str, [Cell; 2])> = vec![
        ("N1 %", std::array::from_fn(|k| (format!("{:.0}", n2(k).powf(1.6) * 100.0), WHITE))),
        ("N2 %", std::array::from_fn(|k| {
            let v = n2(k) * 104.0;
            (format!("{v:.0}"), col_val(v, 102.0, 106.0))
        })),
        ("EGT C", std::array::from_fn(|k| {
            let v = if run(k) || n2(k) > 0.05 { e.egt + if k == 0 { 8.0 } else { -6.0 } } else { d.oat.max(e.egt) };
            (format!("{v:.0}"), col_val(v, 900.0, 1000.0))
        })),
        ("FF PPH", std::array::from_fn(|k| (format!("{:.0}", ff_turb * (e.t_side[k] / tside) * 7_936.6), WHITE))),
        ("OIL PSI", std::array::from_fn(|k| {
            let v = if run(k) { 28.0 + 32.0 * n2(k) } else { 30.0 * n2(k) };
            (format!("{v:.0}"), if run(k) && v < 25.0 { AMBER } else { WHITE })
        })),
        ("OIL C", std::array::from_fn(|k| {
            let v = if run(k) { 60.0 + 55.0 * n2(k) + stag.max(0.0) * 0.08 } else { d.oat.max(15.0) };
            (format!("{v:.0}"), col_val(v, 140.0, 160.0))
        })),
        ("VIB IPS", std::array::from_fn(|k| {
            let v = if run(k) { 0.25 + 0.6 * n2(k) + (1.0 - e.health) * 1.5 } else { 0.0 };
            (format!("{v:.1}"), col_val(v, 1.5, 2.5))
        })),
        ("NOZ %", std::array::from_fn(|k| (format!("{:.0}", if run(k) { 25.0 + 45.0 * e.th_eff.min(1.0) + 30.0 * e.ab_frac } else { 0.0 }), WHITE))),
    ];
    for (l, cells) in &rows {
        txt(l, c0, y, 11.0, lab);
        txt_r(&cells[0].0, c1, y, 12.0, cells[0].1);
        txt_r(&cells[1].0, c2, y, 12.0, cells[1].1);
        y += row;
    }
    let h = y - r.y;
    draw_line(r.x + w1, r.y + 6.0, r.x + w1, r.y + h - 6.0, 1.0, grid);
    // --- Statoréacteur, scramjet, entrée d'air, performances ---
    let (k0, k1) = (r.x + w1 + 10.0, r.x + r.w - 8.0);
    let mut y = r.y + 14.0;
    let mut kv = |l: &str, v: String, c: Color| {
        txt(l, k0, y, 11.0, lab);
        txt_r(&v, k1, y, 12.0, c);
        y += row;
    };
    let ff_ram = e.fuel_flow * (e.t_ram / thrust_sum);
    kv("STATO  POUSSEE / DEBIT", format!("{:.0} kN  {:.2} kg/s", e.t_ram / 1000.0, ff_ram), if e.ram == RamState::Lit { CYAN_S } else { DIM });
    kv("STATO  PAROI", format!("{:.0} C", e.ram_wall), col_val(e.ram_wall, crate::systems::RAM_WALL_CAUTION, crate::systems::RAM_WALL_BURST));
    kv("SCRAM  POUSSEE / H2", format!("{:.0} kN  {:.2} kg/s", e.t_scram / 1000.0, e.h2_flow), if e.scram == ScramState::Lit { MAGENTA_S } else { DIM });
    kv("SCRAM  PAROI", format!("{:.0} C", e.scram_wall), col_val(e.scram_wall, crate::systems::SCRAM_WALL_CAUTION, crate::systems::SCRAM_WALL_BURST));
    let pr = if d.mach < 1.0 { 1.0 } else { (1.0 - 0.075 * (d.mach - 1.0).powf(1.35)).max(0.15) };
    let ramp = ((d.mach - 0.8) / 5.5).clamp(0.0, 1.0) * 100.0;
    kv("ENTREE  T ARRET / PR", format!("{stag:.0} C  {pr:.2}"), if stag > 900.0 { AMBER } else { WHITE });
    kv("ENTREE  RAMPE / PORTES", format!("{ramp:.0} %  {:.0} %", e.doors * 100.0), WHITE);
    let fl = e.fuel_flow + e.h2_flow;
    let isp = if fl > 0.01 { d.thrust / (fl * 9.81) } else { 0.0 };
    let tsfc = if d.thrust > 100.0 { fl * 1000.0 / (d.thrust / 1000.0) } else { 0.0 };
    kv("ISP / TSFC", format!("{isp:.0} s  {tsfc:.1} g/kN.s"), WHITE);
    kv("RESTANT  JP-7 / H2", format!("{:.1} t  {:.1} t", ac.fuel / 1000.0, ac.h2 / 1000.0), WHITE);
    let h = h.max(y - r.y);
    rrect_lines(Rect::new(r.x, r.y, r.w, h), 5.0, 1.0, grid);
    h
}

pub fn draw_engines(ac: &Aircraft, mon: &crate::health::ChamberMon, r: Rect, time: f64) {
    panel(r, "MOTEURS - TBCC DARKSTAR");
    let e = &ac.eng;
    let x = r.x + 10.0;
    let w = r.w - 20.0;
    let m = ac.mode_name();
    let mcol = match m {
        "SCRAMJET" => MAGENTA_S,
        "RAMJET" | "DUAL TURB+RAM" => CYAN_S,
        "TURBINE" => GREEN_S,
        _ => AMBER,
    };
    txt_r(&format!("MODE {m}"), r.x + r.w - 12.0, r.y + 19.0, 15.0, mcol);

    // Nacelles gauche / droite
    let top = r.y + 30.0;
    // Proportions de l'écran moteurs du cockpit (nacelle ~4.8 fois plus large que haute)
    let nh = (w * 0.66 / 4.6).min((r.h - 120.0) / 2.0).max(60.0);
    for (i, (side, off)) in [("ENG L", 8.0), ("ENG R", -6.0)].iter().enumerate() {
        let tstate = match e.side[i] {
            TurbState::Off if e.fadec_fault[i] => "FADEC FAULT",
            TurbState::Off if e.sw[i] == EngSw::Motor => "MOTOR",
            TurbState::Off => "OFF",
            TurbState::Starting => "START",
            TurbState::Running => "RUN",
            TurbState::Failed => "FAIL",
        };
        let nr = Rect::new(x, top + i as f32 * (nh + 6.0), w * 0.66, nh);
        nacelle(nr, &format!("{side}  {tstate}"), ac, i, *off, time + i as f64 * 0.37);
    }

    // Colonne droite : poussée, stato, scram
    let cx = x + w * 0.69;
    let mut y = top + 14.0;
    let line = |y: &mut f32, l: &str, v: String, c: Color| {
        txt(l, cx, *y, 13.0, DIM);
        txt_r(&v, x + w, *y, 14.0, c);
        *y += 19.0;
    };
    line(&mut y, "POUSSEE", format!("{:.0} kN", ac.d.thrust / 1000.0), WHITE);
    line(&mut y, "TRAINEE", format!("{:.0} kN", ac.d.drag / 1000.0), WHITE);
    let rs = match e.ram {
        RamState::Off => ("OFF", DIM),
        RamState::Armed => ("ARME", AMBER),
        RamState::Lit => ("ALLUME", CYAN_S),
    };
    line(&mut y, "STATO", format!("{} {:.0}kN", rs.0, e.t_ram / 1000.0), rs.1);
    let ss = match e.scram {
        ScramState::Off => ("OFF", DIM),
        ScramState::Armed => ("ENABLE", AMBER),
        ScramState::Lit => ("ALLUME", MAGENTA_S),
        ScramState::Unstart => ("UNSTART", RED),
    };
    line(&mut y, "SCRAM", format!("{} {:.0}kN", ss.0, e.t_scram / 1000.0), ss.1);
    let ok = |b: bool| if b { GREEN_S } else { AMBER };
    let a = ac.d.alpha.to_degrees();
    line(&mut y, "MACH >= 4.8", format!("{:.2}", ac.d.mach), ok(ac.d.mach >= 4.8));
    line(&mut y, "q 25-90 kPa", format!("{:.0}", ac.d.q / 1000.0), ok((25_000.0..=90_000.0).contains(&ac.d.q)));
    line(&mut y, "AoA 0-7", format!("{a:+.1}"), ok((0.0..=7.0).contains(&a)));
    txt("MARGE INLET", cx, y, 13.0, DIM);
    draw_rectangle_lines(cx, y + 6.0, x + w - cx, 9.0, 1.0, DIM);
    draw_rectangle(cx + 1.0, y + 7.0, (x + w - cx - 2.0) * e.scram_margin as f32, 7.0, ok(e.scram_margin > 0.3));

    // Ligne basse
    let by = r.y + r.h - 28.0;
    txt(&format!("CHAMBRES {}", mon.state.label()), x, by, 13.0, mon.state.color());
    // Tableau des paramètres moteur (style page ENG d'avion de chasse), puis courbes de surveillance
    let ttop = top + 2.0 * (nh + 6.0) + 6.0;
    let th = engine_table(ac, Rect::new(x, ttop, w, 150.0));
    let gtop = ttop + th + 8.0;
    if by - 18.0 - gtop > 90.0 {
        mon.draw(Rect::new(x, gtop, w, by - 18.0 - gtop));
    }
    let thr = format!("MANETTE {:.0}%{}", ac.throttle * 100.0, if e.ab_frac > 0.0 { " PC" } else { "" });
    txt(&thr, x + 130.0, by, 13.0, if ac.throttle > 1.0 { RED } else if ac.throttle > 0.8 { AMBER } else { WHITE });
    if ac.ovr_engine {
        txt("ENG OVRD", x + 280.0, by, 13.0, AMBER);
    } else if ac.fadec_limit < 0.995 {
        txt(&format!("FADEC {:.0}%", ac.fadec_limit * 100.0), x + 280.0, by, 13.0, AMBER);
    }
    let endu = if e.fuel_flow > 0.01 { ac.fuel / e.fuel_flow } else { 0.0 };
    txt(&format!("ENDUR {}", fmt_time(endu)), x, by + 17.0, 12.0, WHITE);
    // Train : trois voyants (nez, gauche, droite)
    txt("TRAIN", x + 130.0, by + 17.0, 12.0, DIM);
    for (i, l) in ac.gear_legs().iter().enumerate() {
        let c = if *l >= 0.999 { GREEN_S } else if *l > 0.001 { RED } else { Color::new(0.2, 0.21, 0.22, 1.0) };
        draw_circle(x + 182.0 + i as f32 * 14.0, by + 13.0, 5.0, c);
    }
    txt(if ac.brakes { "FREINS" } else { "" }, x + 240.0, by + 17.0, 12.0, AMBER);
    txt(if ac.speedbrake { "AEROFREIN" } else { "" }, x + 300.0, by + 17.0, 12.0, AMBER);
    txt_r(&format!("MASSE {:.1}t", ac.mass() / 1000.0), x + w, by + 17.0, 12.0, WHITE);
}

pub fn draw_card(ac: &Aircraft, mi: &Mission, r: Rect) {
    panel(r, &format!("CARTE D'ESSAIS — {}", crate::link::tail()));
    let x = r.x + 12.0;
    let mut y = r.y + 40.0;
    txt_r(&format!("T+ {}", fmt_time(mi.elapsed)), r.x + r.w - 10.0, r.y + 16.0, 14.0, WHITE);
    if mi.done {
        txt("CAMPAGNE TERMINÉE", x, y, 22.0, GREEN_S);
        y += 28.0;
        txt(&format!("Mach max {:.2}   Alt max {:.0} ft", ac.max_mach, ac.max_alt / FT), x, y, 16.0, WHITE);
        y += 22.0;
        for (i, s) in STEPS.iter().enumerate() {
            if let Some(t) = mi.completed_at.get(i) {
                txt(&format!("{}  {}", fmt_time(*t), s.title), x, y, 13.0, DIM);
                y += 16.0;
            }
        }
        return;
    }
    let st = &STEPS[mi.idx];
    txt(st.title, x, y, 20.0, CYAN_S);
    y += 24.0;
    for line in wrap_text(st.text, 15.0, r.w - 24.0) {
        txt(&line, x, y, 15.0, WHITE);
        y += 19.0;
    }
    y += 6.0;
    for c in mi.checks(ac) {
        let (mark, col) = if c.ok { ("[OK]", GREEN_S) } else { ("[  ]", AMBER) };
        txt(mark, x, y, 15.0, col);
        txt(&c.label, x + 44.0, y, 15.0, col);
        y += 19.0;
    }
    if let Some(adv) = mi.advisory(ac) {
        y += 2.0;
        for line in wrap_text(&adv, 14.0, r.w - 24.0) {
            txt(&line, x, y, 14.0, AMBER);
            y += 17.0;
        }
        y += 4.0;
    }
    if st.hold > 0.0 {
        let f = (mi.timer / st.hold) as f32;
        draw_rectangle_lines(x, y - 4.0, r.w - 24.0, 10.0, 1.0, DIM);
        draw_rectangle(x + 1.0, y - 3.0, (r.w - 26.0) * f.min(1.0), 8.0, GREEN_S);
        txt(&format!("TENUE {:.0}/{:.0} s", mi.timer, st.hold), x, y + 20.0, 13.0, DIM);
        y += 28.0;
    }
    // Liste des points
    y += 8.0;
    let rows = ((r.y + r.h - y - 6.0) / 15.0).max(0.0) as usize;
    let start = mi.idx.saturating_sub(rows / 2).min(STEPS.len().saturating_sub(rows));
    for (i, s) in STEPS.iter().enumerate().skip(start).take(rows) {
        let col = if i < mi.idx { GREEN_S } else if i == mi.idx { WHITE } else { DIM };
        let mark = if i < mi.idx { "+" } else if i == mi.idx { ">" } else { " " };
        txt(&format!("{mark} {}", s.title), x, y, 13.0, col);
        y += 15.0;
    }
}

/// Diagramme Mach / altitude avec couloirs moteurs et lignes de q constant.
pub fn draw_envelope(ac: &Aircraft, r: Rect, history: &[(f32, f32)]) {
    panel(r, "DOMAINE DE VOL  M / ALT");
    let pr = Rect::new(r.x + 46.0, r.y + 28.0, r.w - 58.0, r.h - 52.0);
    let (mmax, amax) = (12.0f32, 130_000.0f32);
    let to = |m: f32, a: f32| vec2(pr.x + m / mmax * pr.w, pr.y + pr.h - a / amax * pr.h);
    draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::new(0.0, 0.0, 0.0, 0.4));
    // Couloirs moteurs
    let bands = [(0.0, 3.2, "TURB", GREEN_S), (2.5, 6.8, "RAM", CYAN_S), (4.8, 12.0, "SCRAM", MAGENTA_S)];
    for (i, (m0, m1, n, c)) in bands.iter().enumerate() {
        let a = to(*m0, 0.0);
        let b = to(*m1, 0.0);
        let yb = pr.y + pr.h - 6.0 - i as f32 * 7.0;
        draw_rectangle(a.x, yb, b.x - a.x, 5.0, Color::new(c.r, c.g, c.b, 0.6));
        txt(n, a.x + 2.0, yb - 1.0, 11.0, *c);
    }
    // Grille
    for m in 0..=12 {
        let p = to(m as f32, 0.0);
        draw_line(p.x, pr.y, p.x, pr.y + pr.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.07));
        if m % 2 == 0 {
            txt_c(&format!("{m}"), p.x, pr.y + pr.h + 14.0, 12.0, DIM);
        }
    }
    for k in 0..=6 {
        let a = k as f32 * 20_000.0;
        let p = to(0.0, a);
        draw_line(pr.x, p.y, pr.x + pr.w, p.y, 1.0, Color::new(1.0, 1.0, 1.0, 0.07));
        txt_r(&format!("{}k", k * 20), pr.x - 4.0, p.y + 4.0, 12.0, DIM);
    }
    // Lignes de q constant
    for (q, c) in [(25_000.0, Color::new(0.4, 0.8, 1.0, 0.7)), (50_000.0, Color::new(0.6, 0.9, 0.6, 0.5)), (90_000.0, Color::new(1.0, 0.6, 0.2, 0.7))] {
        let mut prev: Option<Vec2> = None;
        let mut m = 0.4f32;
        while m <= 12.0 {
            let a = (altitude_for_q(q, m as f64) / FT) as f32;
            let p = to(m, a.clamp(0.0, amax));
            if let Some(pp) = prev {
                draw_line(pp.x, pp.y, p.x, p.y, 1.2, c);
            }
            prev = Some(p);
            m += 0.2;
        }
        let lm = 11.0;
        let la = (altitude_for_q(q, lm as f64) / FT) as f32;
        let p = to(lm, la);
        txt(&format!("{:.0}kPa", q / 1000.0), p.x - 30.0, p.y - 3.0, 11.0, c);
    }
    // Limite thermique
    let mut prev: Option<Vec2> = None;
    let mut m = 4.0f32;
    while m <= 12.0 {
        // altitude minimale où la peau reste sous la limite
        let mut a = 0.0f64;
        while a < 40_000.0 {
            let v = m as f64 * crate::atmosphere::isa(a).a;
            if skin_equilibrium(a, v) - 273.15 < SKIN_LIMIT_C {
                break;
            }
            a += 250.0;
        }
        let p = to(m, (a / FT) as f32);
        if let Some(pp) = prev {
            draw_line(pp.x, pp.y, p.x, p.y, 1.5, RED);
        }
        prev = Some(p);
        m += 0.25;
    }
    // Historique et position
    for w in history.windows(2) {
        let (a, b) = (to(w[0].0, w[0].1), to(w[1].0, w[1].1));
        draw_line(a.x, a.y, b.x, b.y, 1.5, Color::new(1.0, 0.85, 0.2, 0.6));
    }
    let p = to((ac.d.mach as f32).min(mmax), ((ac.alt / FT) as f32).min(amax));
    draw_circle(p.x, p.y, 4.5, Color::new(1.0, 0.85, 0.1, 1.0));
    txt("MACH", pr.x + pr.w - 34.0, pr.y + pr.h + 14.0, 11.0, DIM);
}

pub fn draw_messages(log: &[(f64, Level, String)], r: Rect) {
    panel(r, "MESSAGES / LIAISON GCS");
    let mut y = r.y + r.h - 8.0;
    for (t, l, s) in log.iter().rev() {
        if y < r.y + 34.0 {
            break;
        }
        let c = match l {
            Level::Info => Color::new(0.75, 0.85, 0.9, 1.0),
            Level::Caution => AMBER,
            Level::Warning => RED,
        };
        let lines = wrap_text(&format!("{} {}", fmt_time(*t), s), 13.0, r.w - 16.0);
        for line in lines.iter().rev() {
            if y < r.y + 34.0 {
                break;
            }
            txt(line, r.x + 8.0, y, 13.0, c);
            y -= 15.0;
        }
    }
}

pub fn draw_help(r: Rect) {
    let w = 640.0f32.min(r.w - 20.0);
    let h = 560.0f32.min(r.h - 20.0);
    let x = r.x + (r.w - w) / 2.0;
    let y = r.y + (r.h - h) / 2.0;
    rrect(Rect::new(x, y, w, h), 12.0, Color::new(0.03, 0.04, 0.06, 0.96));
    rrect_lines(Rect::new(x, y, w, h), 12.0, 1.5, ACCENT);
    let lines = [
        ("STATION SOL DARKSTAR - PILOTAGE PAR INSTRUCTIONS", CYAN_S),
        ("", WHITE),
        ("ENTREE ouvre la console d'instructions (onglet COMMANDES : les memes fonctions en boutons) :", WHITE),
        ("  CAP 270  ALT 90000 / FL350  ASSIETTE 10  MACH 6  KEAS 300  THR 80", GREEN_S),
        ("  DEST KEDW  DIRECT KEDW  ATTERRIR KEDW (autoland)  DECOLLAGE 15000", GREEN_S),
        ("  DESCENTE 50 : descente hypersonique a q constant (anti-surpression)", GREEN_S),
        ("  TURB START/STOP  INLET OPEN/CLOSE  RAM ARM/IGN  SCRAM ENABLE/IGN", CYAN_S),
        ("  OVERRIDE MOTEUR ON  OVERRIDE FBW ON  OVERRIDE OFF  H2/FUEL DUMP", AMBER),
        ("  HSI 80  SON OFF  WARP 8  MENU  CAPTEUR 101 OFF  FIN  QUITTER  AIDE", GREEN_S),
        ("  PANNE ENGL/GENL/POMPE/ADC/FCS/HYD/REFROID/PORTES/TRAIN/FUITEH2 [OFF]", RED),
        ("  PANNES OFF (tout reparer)  TRAIN SECOURS (sortie de secours)", RED),
        ("", WHITE),
        ("Raccourcis : T turbines  I entrees  Y stato  U scram  G train", WHITE),
        ("  B freins  S aerofreins  P AP  O A/THR  N NAV  L ILS", WHITE),
        ("  AP actif : fleches = consignes alt (ou assiette)/cap", DIM),
        ("  R/F = manette (coupe l'A/THR) ; vitesse auto : MACH 2.7 / KEAS 300", DIM),
        ("X  reprise en main manuelle (fleches = manche, R/F = manette)", AMBER),
        ("4-9, 0 pages MOTEURS/ALARMES/FCS/LIMITES/ELEC-ENV/THERMIQUE/ESSAIS", WHITE),
        ("6 boutons de l'ecran Darkstar = 6 premieres pages ; CONSOLE : bouton MENU", WHITE),
        ("V carte plein ecran  C recentrer  E temps (ou clic TEMPS)", WHITE),
        ("Carte : bouton PLAN / SAT (tuiles reelles) ; SVS : satellite + relief reel", WHITE),
        ("ESPACE acquitter  J son  MAJ+ENTREE nouvel essai  TAB aide", WHITE),
        ("", WHITE),
        ("Ecran Darkstar : clic sur DUMP (vidange), fleches de portee HSI.", DIM),
        ("Onglet OVERRIDE : interrupteurs gardes. Onglet CONSOLE : moteurs.", DIM),
        ("ENGINE OVERRIDE debride les moteurs (120 %) au prix de leur usure.", AMBER),
    ];
    txt_r(&format!("v{}", crate::updater::VERSION), x + w - 16.0, y + 24.0, 14.0, DIM);
    let mut yy = y + 30.0;
    for (s, c) in lines {
        txt(s, x + 20.0, yy, if c == CYAN_S && yy < y + 40.0 { 20.0 } else { 15.0 }, c);
        yy += 21.0;
    }
}
