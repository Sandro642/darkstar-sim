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
fn nacelle(r: Rect, side: &str, ac: &Aircraft, egt_off: f64, time: f64) {
    let e = &ac.eng;
    let running = e.turb == TurbState::Running;
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
    let gen_on = running && e.n2 > 0.55;
    let bx = x + w * 0.36;
    draw_rectangle(bx, cy - 9.0, 58.0, 18.0, Color::new(0.20, 0.12, 0.05, 1.0));
    draw_rectangle_lines(bx, cy - 9.0, 58.0, 18.0, 1.0, if gen_on { AMBER } else { DIM });
    txt_c(if gen_on { "GEN ON" } else { "GEN OFF" }, bx + 29.0, cy + 5.0, 12.0, if gen_on { AMBER } else { DIM });
    txt("RPM", bx + 64.0, cy - 2.0, 10.0, DIM);
    let rpm = e.n2 * 104.0;
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

pub fn draw_eicas(ac: &Aircraft, r: Rect, master_warn: bool, master_caut: bool, time: f64) {
    panel(r, "EICAS - PROPULSION DARKSTAR");
    let e = &ac.eng;

    // Voyants maîtres
    let blink = (time * 2.5).fract() < 0.6;
    let mw = Rect::new(r.x + r.w - 196.0, r.y + 2.0, 92.0, 18.0);
    let mc = Rect::new(r.x + r.w - 100.0, r.y + 2.0, 92.0, 18.0);
    draw_rectangle(mw.x, mw.y, mw.w, mw.h, if master_warn && blink { RED } else { Color::new(0.25, 0.0, 0.0, 1.0) });
    txt_c("WARNING", mw.x + mw.w / 2.0, mw.y + 14.0, 13.0, if master_warn { WHITE } else { DIM });
    draw_rectangle(mc.x, mc.y, mc.w, mc.h, if master_caut { AMBER } else { Color::new(0.25, 0.18, 0.0, 1.0) });
    txt_c("CAUTION", mc.x + mc.w / 2.0, mc.y + 14.0, 13.0, if master_caut { BLACK } else { DIM });

    let left_w = r.w * 0.60;
    let x = r.x + 8.0;
    let y = r.y + 26.0;

    // Mode et poussée
    let m = ac.mode_name();
    let mcol = match m {
        "SCRAMJET" => MAGENTA_S,
        "RAMJET" | "DUAL TURB+RAM" => CYAN_S,
        "TURBINE" => GREEN_S,
        _ => AMBER,
    };
    txt(&format!("MODE {m}"), x, y + 12.0, 16.0, mcol);
    txt_r(&format!("T {:.0} kN  D {:.0} kN", ac.d.thrust / 1000.0, ac.d.drag / 1000.0), x + left_w - 8.0, y + 12.0, 13.0, WHITE);

    // Deux nacelles
    let nh = ((r.h - 150.0) / 2.0).clamp(70.0, 110.0);
    let tstate = match e.turb {
        TurbState::Off => "OFF",
        TurbState::Starting => "START",
        TurbState::Running => "RUN",
        TurbState::Failed => "FAIL",
    };
    let nr1 = Rect::new(x, y + 20.0, left_w - 8.0, nh);
    let nr2 = Rect::new(x, y + 24.0 + nh, left_w - 8.0, nh);
    draw_rectangle(nr1.x, nr1.y, nr1.w, nr1.h, Color::new(0.06, 0.07, 0.09, 1.0));
    draw_rectangle(nr2.x, nr2.y, nr2.w, nr2.h, Color::new(0.06, 0.07, 0.09, 1.0));
    nacelle(nr1, &format!("ENG L  {tstate}"), ac, 8.0, time);
    nacelle(nr2, &format!("ENG R  {tstate}"), ac, -6.0, time + 0.37);

    // Statoréacteur / scramjet
    let sy = nr2.y + nr2.h + 16.0;
    let rs = match e.ram {
        RamState::Off => ("OFF", DIM),
        RamState::Armed => ("ARME", AMBER),
        RamState::Lit => ("ALLUME", CYAN_S),
    };
    let ss = match e.scram {
        ScramState::Off => ("OFF", DIM),
        ScramState::Armed => ("ENABLE", AMBER),
        ScramState::Lit => ("ALLUME", MAGENTA_S),
        ScramState::Unstart => ("UNSTART", RED),
    };
    txt("STATO", x, sy, 13.0, DIM);
    txt(rs.0, x + 52.0, sy, 13.0, rs.1);
    txt(&format!("{:.0} kN", e.t_ram / 1000.0), x + 120.0, sy, 13.0, WHITE);
    txt("SCRAM", x, sy + 17.0, 13.0, DIM);
    txt(ss.0, x + 52.0, sy + 17.0, 13.0, ss.1);
    txt(&format!("{:.0} kN", e.t_scram / 1000.0), x + 120.0, sy + 17.0, 13.0, WHITE);
    let a = ac.d.alpha.to_degrees();
    let ok = |b: bool| if b { GREEN_S } else { AMBER };
    let cx2 = x + 190.0;
    txt(&format!("M{:.2}", ac.d.mach), cx2, sy + 17.0, 13.0, ok(ac.d.mach >= 4.8));
    txt(&format!("q{:.0}", ac.d.q / 1000.0), cx2 + 58.0, sy + 17.0, 13.0, ok((25_000.0..=90_000.0).contains(&ac.d.q)));
    txt(&format!("a{a:+.1}"), cx2 + 100.0, sy + 17.0, 13.0, ok((0.0..=7.0).contains(&a)));
    vbar(cx2 + 150.0, sy + 4.0, 40.0 * 0.0 + 8.0, 16.0, e.scram_margin as f32, ok(e.scram_margin > 0.3));
    let ram_ok = ac.ram_ignition_check().is_ok();
    txt(if ram_ok { "IGN OK" } else { "" }, cx2, sy, 13.0, GREEN_S);

    // Page carburant (droite)
    let fx = r.x + left_w + 4.0;
    let fw = r.w - left_w - 12.0;
    let fy = y + 18.0;
    draw_rectangle(fx, fy, fw, r.h - (fy - r.y) - 8.0, Color::new(0.03, 0.05, 0.04, 1.0));
    draw_rectangle_lines(fx, fy, fw, r.h - (fy - r.y) - 8.0, 1.0, Color::new(0.2, 0.5, 0.25, 1.0));
    let gr = (fw * 0.16).min(24.0);
    let pph = e.fuel_flow * 3600.0 * 2.2046 / 1000.0;
    txt("FUEL FLOW", fx + 6.0, fy + 14.0, 11.0, WHITE);
    dial(fx + 8.0 + gr, fy + 26.0 + gr, gr, pph as f32, 0.0, 300.0, 260.0, 290.0, "", "");
    txt(&format!("PPH {pph:03.0}"), fx + 6.0, fy + 38.0 + gr * 2.0, 12.0, WHITE);
    txt("x1000", fx + 6.0, fy + 50.0 + gr * 2.0, 10.0, DIM);
    if e.h2_flow > 0.0 {
        txt(&format!("H2 {:.1} kg/s", e.h2_flow), fx + 50.0, fy + 50.0 + gr * 2.0, 11.0, CYAN_S);
    }
    let fp = ac.fuel_psi();
    txt_r("FUEL PRES", fx + fw - 6.0, fy + 14.0, 11.0, WHITE);
    dial(fx + fw - 10.0 - gr * 2.2, fy + 26.0 + gr, gr, fp as f32, 0.0, 120.0, 105.0, 115.0, "", "");
    txt_r(&format!("PSI {fp:.0}"), fx + fw - 6.0, fy + 38.0 + gr * 2.0, 12.0, if fp < 40.0 { AMBER } else { WHITE });

    // Silhouette réservoirs : JP-7 (vert) et H2 (cyan)
    let sy0 = fy + 64.0 + gr * 2.0;
    let sh = (r.y + r.h - 70.0 - sy0).max(60.0);
    let scx = fx + fw * 0.5;
    let sw2 = fw * 0.30;
    let outline = [(0.0, 0.0), (0.18, 0.45), (1.0, 0.95), (0.25, 1.0), (-0.25, 1.0), (-1.0, 0.95), (-0.18, 0.45)];
    for i in 0..outline.len() {
        let (a0, b0) = (outline[i], outline[(i + 1) % outline.len()]);
        draw_line(scx + a0.0 * sw2, sy0 + a0.1 * sh, scx + b0.0 * sw2, sy0 + b0.1 * sh, 1.5, GREEN_S);
    }
    let jp = (ac.fuel / FUEL_MAX) as f32;
    let h2f = (ac.h2 / H2_MAX) as f32;
    let tanks = [(0.30, jp, GREEN_S), (0.50, h2f, CYAN_S), (0.70, jp, GREEN_S), (0.86, jp, GREEN_S)];
    for (ty, f, c) in tanks {
        let tw = sw2 * (0.25 + ty * 0.5);
        let ty = sy0 + ty * sh;
        draw_rectangle_lines(scx - tw / 2.0, ty - 7.0, tw, 14.0, 1.0, c);
        draw_rectangle(scx - tw / 2.0 + 1.0, ty - 6.0, (tw - 2.0) * f, 12.0, Color::new(c.r, c.g, c.b, 0.75));
    }
    let cg = ac.cg();
    txt(&format!("CG {cg:.0}%"), fx + 6.0, sy0 + sh * 0.55, 14.0, if (21.0..=29.0).contains(&cg) { GREEN_S } else { AMBER });
    txt(&format!("JP7 {:.1}t", ac.fuel / 1000.0), fx + 6.0, sy0 + sh * 0.55 + 16.0, 12.0, GREEN_S);
    let h2p = ac.h2_psi();
    txt_r(&format!("H2 {:.1}t", ac.h2 / 1000.0), fx + fw - 6.0, sy0 + sh * 0.55, 12.0, CYAN_S);
    txt_r(&format!("{h2p:.0} psi"), fx + fw - 6.0, sy0 + sh * 0.55 + 16.0, 12.0, if h2p < 40.0 { AMBER } else { CYAN_S });
    if ac.h2_dump {
        draw_rectangle(fx + 6.0, sy0 - 18.0, fw - 12.0, 16.0, Color::new(0.5, 0.0, 0.0, 1.0));
        txt_c("EMER H2 DUMP", fx + fw / 2.0, sy0 - 6.0, 12.0, WHITE);
    }

    // Ligne basse : manette, train, freins, peau
    let by = r.y + r.h - 40.0;
    txt(&format!("THR {:.0}%{}", ac.throttle * 100.0, if e.ab_frac > 0.0 { " PC" } else { "" }), fx + 6.0, by + 4.0, 13.0, if ac.throttle > 0.8 { AMBER } else { WHITE });
    let endu = if e.fuel_flow > 0.01 { ac.fuel / e.fuel_flow } else { 0.0 };
    txt(&format!("ENDUR {}", fmt_time(endu)), fx + 6.0, by + 20.0, 12.0, WHITE);
    let skin = ac.skin_temp - 273.15;
    let sk_c = if skin > SKIN_LIMIT_C { RED } else if skin > SKIN_LIMIT_C - 100.0 { AMBER } else { WHITE };
    txt(&format!("PEAU {skin:.0}°C"), x, by + 4.0, 13.0, sk_c);
    let gear = if ac.gear_pos > 0.99 { ("TRAIN DOWN", GREEN_S) } else if ac.gear_pos < 0.01 { ("TRAIN UP", DIM) } else { ("TRAIN TRANSIT", AMBER) };
    txt(gear.0, x, by + 20.0, 13.0, gear.1);
    txt(if ac.brakes { "FREINS" } else { "" }, x + 120.0, by + 20.0, 13.0, AMBER);
    txt(if ac.speedbrake { "AEROFREIN" } else { "" }, x + 180.0, by + 20.0, 13.0, AMBER);
    txt(&format!("MASSE {:.1}t", ac.mass() / 1000.0), x + 120.0, by + 4.0, 13.0, WHITE);
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
    draw_rectangle(x, y, w, h, Color::new(0.0, 0.02, 0.05, 0.93));
    draw_rectangle_lines(x, y, w, h, 2.0, CYAN_S);
    let lines = [
        ("STATION SOL DARKSTAR — PILOTAGE PAR INSTRUCTIONS", CYAN_S),
        ("", WHITE),
        ("ENTRÉE ouvre la console d'instructions :", WHITE),
        ("  CAP 270   ALT 90000 / FL350   MACH 6   KEAS 300   THR 80", GREEN_S),
        ("  DIRECT KEDW   ATTERRIR KEDW (autoland)   DECOLLAGE 15000", GREEN_S),
        ("  TURB START/STOP  INLET OPEN/CLOSE  RAM ARM/IGN  SCRAM ARM/IGN", CYAN_S),
        ("  TRAIN UP/DOWN   FREINS ON/OFF   AP OFF   MANUEL   WARP 8   AIDE", GREEN_S),
        ("", WHITE),
        ("Raccourcis : T turbines  I entrées  Y stato  U scram  G train", WHITE),
        ("  B freins  S aérofreins  P AP  O A/THR  N NAV  L ILS  D/K dest.", WHITE),
        ("  AP actif : flèches = consignes alt/cap (MAJ x10), R/F = vitesse", DIM),
        ("X  reprise en main manuelle (flèches = manche, R/F = manette)", AMBER),
        ("", WHITE),
        ("V carte plein écran  C recentrer  H palette SVS  E temps x1..x32", WHITE),
        ("ESPACE acquitter alarmes  J son on/off  MAJ+ENTRÉE nouvel essai  TAB aide", WHITE),
        ("", WHITE),
        ("Suivre la carte d'essais (bas centre). Le domaine de vol", AMBER),
        ("(bas droite) montre couloirs moteurs, lignes de q, limite thermique.", AMBER),
        ("Liaison : LOS 45 ms près d'Edwards, SATCOM 380 ms au-delà.", AMBER),
    ];
    let mut yy = y + 30.0;
    for (s, c) in lines {
        txt(s, x + 20.0, yy, if c == CYAN_S && yy < y + 40.0 { 20.0 } else { 15.0 }, c);
        yy += 21.0;
    }
}
