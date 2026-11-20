//! Panneaux de la station sol : EICAS propulsion, carte d'essais, domaine de vol, messages.

use crate::aircraft::*;
use crate::atmosphere::altitude_for_q;
use crate::geo::*;
use crate::mission::*;
use crate::ui::*;
use crate::ui::wrap_text;
use macroquad::prelude::*;

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

/// Page moteurs simple (poste pilote) : les deux nacelles empilées, pleine largeur.
pub fn draw_nacelles(ac: &Aircraft, r: Rect, time: f64) {
    let e = &ac.eng;
    let gap = 8.0;
    let nh = ((r.h - gap) / 2.0).min(r.w / 3.9);
    let y0 = r.y + (r.h - (2.0 * nh + gap)) / 2.0;
    for (i, (side, off)) in [("ENG L", 8.0), ("ENG R", -6.0)].iter().enumerate() {
        let tstate = match e.side[i] {
            TurbState::Off if e.fadec_fault[i] => "FADEC FAULT",
            TurbState::Off if e.sw[i] == EngSw::Motor => "MOTOR",
            TurbState::Off => "OFF",
            TurbState::Starting => "START",
            TurbState::Running => "RUN",
            TurbState::Failed => "FAIL",
        };
        let _ = tstate;
        crate::tbcc::draw_film(Rect::new(r.x, y0 + i as f32 * (nh + gap), r.w, nh), side, ac, i, *off, time + i as f64 * 0.37);
    }
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
    let nh = (w * 0.66 / 4.0).min((r.h - 120.0) / 2.0).max(60.0);
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
        let _ = tstate;
        crate::tbcc::draw_film(nr, side, ac, i, *off, time + i as f64 * 0.37);
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
        ("  B freins  K aerofreins  P AP  O A/THR  N NAV  L ILS", WHITE),
        ("  AP actif : fleches = consignes alt (ou assiette)/cap", DIM),
        ("  R/F = manette (coupe l'A/THR) ; vitesse auto : MACH 2.7 / KEAS 300", DIM),
        ("X  reprise en main manuelle (fleches = manche, R/F = manette)", AMBER),
        ("Manche : Z/S tangage  Q/D roulis  A/E palonnier (lacet, roue avant au sol)", WHITE),
        ("  (QWERTY : W/S, A/D, Q/E) ; fleches = tangage / roulis aussi", DIM),
        ("4-9, 0 pages MOTEURS/ALARMES/FCS/LIMITES/ELEC-ENV/THERMIQUE/ESSAIS", WHITE),
        ("6 boutons de l'ecran Darkstar = 6 premieres pages ; CONSOLE : bouton MENU", WHITE),
        ("V carte plein ecran  C recentrer  H temps (ou clic TEMPS)", WHITE),
        ("Carte : bouton PLAN / SAT (tuiles reelles) ; SVS : satellite + relief reel", WHITE),
        ("MASTER WARN / CAUT (clic) acquitter  J son  MAJ+ENTREE nouvel essai  TAB aide", WHITE),
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
