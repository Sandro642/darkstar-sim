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

/// Nacelle moteur façon écran Darkstar : portes, GEN, RPM, EGT et température d'entrée (°F).
fn nacelle(r: Rect, side: &str, ac: &Aircraft, i: usize, egt_off: f64, time: f64) {
    let e = &ac.eng;
    let running = e.side[i] == TurbState::Running;
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    let cy = y + h * 0.55;
    txt("FWD", x + 4.0, y + 12.0, 12.0, DIM);
    txt_r("AFT", x + w - 4.0, y + 12.0, 12.0, DIM);
    txt(side, x + w * 0.45, y + 12.0, 12.0, DIM);
    // Corps de nacelle
    let body = [
        vec2(x + w * 0.10, cy - h * 0.10),
        vec2(x + w * 0.22, cy - h * 0.26),
        vec2(x + w * 0.80, cy - h * 0.22),
        vec2(x + w * 0.90, cy - h * 0.10),
        vec2(x + w * 0.90, cy + h * 0.10),
        vec2(x + w * 0.80, cy + h * 0.24),
        vec2(x + w * 0.22, cy + h * 0.28),
        vec2(x + w * 0.10, cy + h * 0.12),
    ];
    let col = Color::new(0.62, 0.66, 0.70, 1.0);
    for i in 0..body.len() {
        let (p, q) = (body[i], body[(i + 1) % body.len()]);
        draw_line(p.x, p.y, q.x, q.y, 1.5, col);
    }
    // Arbre moteur
    draw_rectangle(x + w * 0.28, cy - 5.0, w * 0.46, 10.0, Color::new(0.30, 0.32, 0.35, 1.0));
    // Portes d'entrée d'air (angle)
    let door_deg = e.doors * 90.0;
    let pivot = vec2(x + w * 0.12, cy - h * 0.12);
    let a = (door_deg as f32).to_radians();
    let tip = pivot + vec2(a.cos(), a.sin()) * (h * 0.24);
    draw_line(pivot.x, pivot.y, tip.x, tip.y, 2.5, if e.doors > 0.01 && e.doors < 0.99 { AMBER } else { col });
    txt("DOORS", x + 4.0, cy + h * 0.30, 11.0, DIM);
    txt(&format!("{:02.0}°", door_deg), x + 8.0, cy + h * 0.30 + 14.0, 15.0, WHITE);
    // GEN + RPM
    let gen_on = running && e.n2s[i] > 0.55;
    let bx = x + w * 0.36;
    draw_rectangle(bx, cy - 9.0, 58.0, 18.0, Color::new(0.20, 0.12, 0.05, 1.0));
    draw_rectangle_lines(bx, cy - 9.0, 58.0, 18.0, 1.0, if gen_on { AMBER } else { DIM });
    txt_c(if gen_on { "GEN ON" } else { "GEN OFF" }, bx + 29.0, cy + 5.0, 12.0, if gen_on { AMBER } else { DIM });
    txt("RPM", bx + 64.0, cy - 2.0, 10.0, DIM);
    let rpm = e.n2s[i] * 104.0;
    let rc = if rpm > 106.0 { RED } else if rpm > 102.0 { AMBER } else { WHITE };
    txt(&format!("{rpm:.0}%"), bx + 64.0, cy + 11.0, 15.0, rc);
    // Flamme de post-combustion
    if e.ab_frac > 0.0 && running {
        let fl = 0.85 + 0.15 * ((time * 23.0).sin() as f32);
        for k in -2..=2 {
            let yy = cy + k as f32 * 4.0;
            let len = (18.0 + 14.0 * e.ab_frac as f32) * fl * (1.0 - (k as f32).abs() * 0.15);
            draw_line(x + w * 0.90, yy, x + w * 0.90 + len, yy + k as f32 * 2.0, 2.0, Color::new(1.0, 0.55, 0.1, 0.9));
        }
    }
    // EGT et température d'entrée (°F, comme sur le Darkstar)
    let egt_f = f_of(e.egt + egt_off);
    let ec = if e.egt > 1000.0 { RED } else if e.egt > 900.0 { AMBER } else { WHITE };
    txt_r(&format!("{egt_f:.0}°F"), x + w - 4.0, cy - h * 0.30, 15.0, ec);
    let cit_c = (ac.d.oat + 273.15) * (1.0 + 0.2 * ac.d.mach * ac.d.mach) - 273.15;
    let cit_show = ac.d.oat + (cit_c - ac.d.oat) * (1.0 - e.doors * 0.94);
    let cc = if cit_show > 430.0 { RED } else if cit_show > 400.0 { AMBER } else { WHITE };
    txt_c(&format!("{:.0}°F", f_of(cit_show)), x + w * 0.50, cy + h * 0.30 + 14.0, 15.0, cc);
    txt_c("CIT", x + w * 0.50, cy + h * 0.30, 10.0, DIM);
}

pub fn draw_engines(ac: &Aircraft, r: Rect, time: f64) {
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
    let nh = ((r.h - 140.0) / 2.0).clamp(60.0, 110.0);
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
        rrect(nr, 6.0, Color::new(0.035, 0.04, 0.05, 1.0));
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
    let hc = if e.combustor_failed { RED } else if e.health < 0.3 { AMBER } else { GREEN_S };
    txt(&format!("CHAMBRES {:.0}%", e.health * 100.0), x, by, 13.0, hc);
    let thr = format!("MANETTE {:.0}%{}", ac.throttle * 100.0, if e.ab_frac > 0.0 { " PC" } else { "" });
    txt(&thr, x + 130.0, by, 13.0, if ac.throttle > 1.0 { RED } else if ac.throttle > 0.8 { AMBER } else { WHITE });
    if ac.ovr_engine {
        txt("ENG OVRD", x + 280.0, by, 13.0, AMBER);
    } else if ac.fadec_limit < 0.995 {
        txt(&format!("FADEC {:.0}%", ac.fadec_limit * 100.0), x + 280.0, by, 13.0, AMBER);
    }
    let endu = if e.fuel_flow > 0.01 { ac.fuel / e.fuel_flow } else { 0.0 };
    txt(&format!("ENDUR {}", fmt_time(endu)), x, by + 17.0, 12.0, WHITE);
    let gear = if ac.gear_pos > 0.99 { ("TRAIN DOWN", GREEN_S) } else if ac.gear_pos < 0.01 { ("TRAIN UP", DIM) } else { ("TRAIN TRANSIT", AMBER) };
    txt(gear.0, x + 130.0, by + 17.0, 12.0, gear.1);
    txt(if ac.brakes { "FREINS" } else { "" }, x + 240.0, by + 17.0, 12.0, AMBER);
    txt(if ac.speedbrake { "AEROFREIN" } else { "" }, x + 300.0, by + 17.0, 12.0, AMBER);
    txt_r(&format!("MASSE {:.1}t", ac.mass() / 1000.0), x + w, by + 17.0, 12.0, WHITE);
}

pub fn draw_card(ac: &Aircraft, mi: &Mission, r: Rect) {
    panel(r, "CARTE D'ESSAIS — FTV-01");
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
        ("ENTREE ouvre la console d'instructions :", WHITE),
        ("  CAP 270  ALT 90000 / FL350  ASSIETTE 10  MACH 6  KEAS 300  THR 80", GREEN_S),
        ("  DIRECT KEDW   ATTERRIR KEDW (autoland)   DECOLLAGE 15000", GREEN_S),
        ("  TURB START/STOP  INLET OPEN/CLOSE  RAM ARM/IGN  SCRAM ENABLE/IGN", CYAN_S),
        ("  OVERRIDE MOTEUR ON  OVERRIDE FBW ON  OVERRIDE OFF  H2/FUEL DUMP", AMBER),
        ("  HSI 80  SON OFF  WARP 8  MENU  AIDE", GREEN_S),
        ("", WHITE),
        ("Raccourcis : T turbines  I entrees  Y stato  U scram  G train", WHITE),
        ("  B freins  S aerofreins  P AP  O A/THR  N NAV  L ILS  D/K dest.", WHITE),
        ("  AP actif : fleches = consignes alt (ou assiette)/cap", DIM),
        ("  R/F = manette (coupe l'A/THR) ; vitesse auto : MACH 2.7 / KEAS 300", DIM),
        ("X  reprise en main manuelle (fleches = manche, R/F = manette)", AMBER),
        ("4-9 onglets MOTEURS/THERMIQUE/ESSAIS/DOMAINE/OVERRIDE/CONSOLE", WHITE),
        ("CONSOLE (9 ou bouton MENU du PFD) : ENGINE/APU + FLT CNTRL", WHITE),
        ("V carte plein ecran  C recentrer  H palette SVS  E temps (ou clic TEMPS)", WHITE),
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
