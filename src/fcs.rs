//! Page FCS : loi de vol, protections, limites, manche et gouvernes.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;

/// Barre horizontale valeur / limites (min, max), avec repère de la valeur courante.
fn limit_bar(x: f32, y: f32, w: f32, label: &str, v: f64, lo: f64, hi: f64, vmin: f64, vmax: f64, unit: &str, limiting: bool) {
    txt(label, x, y, 12.0, DIM);
    let bx = x + 92.0;
    let bw = w - 92.0 - 70.0;
    let to = |val: f64| bx + (((val - vmin) / (vmax - vmin)).clamp(0.0, 1.0) as f32) * bw;
    draw_rectangle(bx, y - 9.0, bw, 10.0, Color::new(0.10, 0.12, 0.14, 1.0));
    // Zone autorisée
    let (a, b) = (to(lo), to(hi));
    draw_rectangle(a, y - 9.0, b - a, 10.0, Color::new(0.10, 0.35, 0.18, 1.0));
    draw_line(a, y - 12.0, a, y + 3.0, 2.0, AMBER);
    draw_line(b, y - 12.0, b, y + 3.0, 2.0, AMBER);
    let p = to(v);
    let c = if limiting { AMBER } else if v < lo || v > hi { RED } else { WHITE };
    draw_triangle(vec2(p, y - 10.0), vec2(p - 5.0, y - 17.0), vec2(p + 5.0, y - 17.0), c);
    draw_line(p, y - 10.0, p, y + 1.0, 2.0, c);
    txt_r(&format!("{v:+.1}{unit}"), x + w, y, 12.0, c);
}

fn status_row(x: f32, y: f32, w: f32, name: &str, detail: &str, state: (&str, Color)) {
    draw_circle(x + 6.0, y - 4.0, 5.0, state.1);
    txt(name, x + 18.0, y, 12.0, WHITE);
    txt(detail, x + 18.0 + w * 0.42, y, 12.0, DIM);
    txt_r(state.0, x + w, y, 12.0, state.1);
}

pub fn draw_fcs(ac: &Aircraft, r: Rect) {
    panel(r, "FCS - COMMANDES DE VOL");
    let f = &ac.fcs;
    let ovr = ac.ovr_fbw;
    let law = if ovr {
        ("LOI DIRECTE (FBW OVERRIDE)", AMBER)
    } else if ac.test_mode && ac.test_low_speed {
        ("LOI ESSAI LOW SPEED", AMBER)
    } else if ac.test_mode {
        ("LOI ESSAI HIGH SPEED", GREEN_S)
    } else {
        ("LOI NORMALE", GREEN_S)
    };
    txt_r(law.0, r.x + r.w - 12.0, r.y + 19.0, 14.0, law.1);

    let x = r.x + 12.0;
    let lw = r.w * 0.56;
    let mut y = r.y + 42.0;
    let on = |active: bool| -> (&'static str, Color) {
        if ovr {
            ("INHIBEE", AMBER)
        } else if active {
            ("LIMITATION", AMBER)
        } else {
            ("ACTIVE", GREEN_S)
        }
    };
    status_row(x, y, lw, "INCIDENCE", &format!("{:+.1} / {:+.1} deg", f.alpha_min, f.alpha_max), on(f.alpha_limit_active));
    y += 17.0;
    status_row(x, y, lw, "FACTEUR DE CHARGE", &format!("max {:+.1} g", f.nz_max), on(false));
    y += 17.0;
    status_row(x, y, lw, "HAUTE VITESSE (q)", &format!("q prevue {:.0} kPa", f.q_pred / 1000.0), on(f.hs_active));
    y += 17.0;
    status_row(x, y, lw, "PIQUE MAXI", &format!("pente min {:+.1} deg", f.gamma_min.max(-90.0)), on(f.dive_active));
    y += 17.0;
    let inlet = if ac.eng.scram == ScramState::Lit {
        if ovr { ("INHIBEE", AMBER) } else { ("ACTIVE", GREEN_S) }
    } else {
        ("STBY", DIM)
    };
    status_row(x, y, lw, "ENTREE D'AIR SCRAM", if ac.ap.on { "AoA max 6.5 (AP)" } else { "AoA max 9.0" }, inlet);
    y += 17.0;
    status_row(x, y, lw, "INCLINAISON", "max 80 deg", ("ACTIVE", GREEN_S));
    y += 17.0;
    let ad = if ac.adc_degraded() { ("PITOT DEGRADE", AMBER) } else if ac.air_data_ovrd { ("INS / GPS", CYAN_S) } else { ("PITOT", GREEN_S) };
    status_row(x, y, lw, "DONNEES AIR", "source", ad);
    y += 24.0;

    // Jauges valeur / limites
    let a = ac.d.alpha.to_degrees();
    limit_bar(x, y, lw, "INCIDENCE", a, f.alpha_min, f.alpha_max, -10.0, 25.0, "", f.alpha_limit_active);
    y += 24.0;
    limit_bar(x, y, lw, "G", ac.d.nz, -1.0, f.nz_max, -2.5, 6.0, "g", false);
    y += 24.0;
    limit_bar(x, y, lw, "q", ac.d.q / 1000.0, 0.0, Q_LIMIT / 1000.0, 0.0, 130.0, "", f.hs_active);
    y += 24.0;
    limit_bar(x, y, lw, "PENTE", ac.gamma.to_degrees(), f.gamma_min.max(-40.0), 40.0, -40.0, 40.0, "", f.dive_active);
    y += 22.0;
    txt(&format!("TRIM LACET {:+.1}   ROULIS ORD {:+.1} deg/s   TANGAGE ORD {:+.1} deg/s", ac.yaw_trim * 10.0, f.roll_rate, f.pitch_rate), x, y, 12.0, DIM);
    // Enveloppe structurale : g de rupture selon la température de structure (masse actuelle et
    // masse maximale), g visé par l'auto recovery (90 %), point actuel
    let gh = (r.y + r.h - y - 34.0).min(150.0);
    if gh > 70.0 {
        envelope(ac, Rect::new(x, y + 14.0, lw, gh));
    }

    // Manche : ordre pilote (blanc) et ordre de la loi (vert)
    let rx = r.x + lw + 30.0;
    let rw = r.x + r.w - rx - 12.0;
    let bs = (rw * 0.45).min(110.0);
    let sb = Rect::new(rx, r.y + 34.0, bs, bs);
    rrect(sb, 6.0, Color::new(0.05, 0.06, 0.07, 1.0));
    rrect_lines(sb, 6.0, 1.0, PANEL_BORDER);
    let (cx, cy) = (sb.x + bs / 2.0, sb.y + bs / 2.0);
    draw_line(sb.x + 6.0, cy, sb.x + bs - 6.0, cy, 1.0, Color::new(1.0, 1.0, 1.0, 0.15));
    draw_line(cx, sb.y + 6.0, cx, sb.y + bs - 6.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.15));
    let half = bs / 2.0 - 8.0;
    let pilot = vec2(cx + f.roll_in as f32 * half, cy - f.pitch_in as f32 * half);
    let qn = (f.pitch_rate / 6.0).clamp(-1.0, 1.0) as f32;
    let pn = (f.roll_rate / 35.0).clamp(-1.0, 1.0) as f32;
    let law_p = vec2(cx + pn * half, cy - qn * half);
    draw_circle_lines(pilot.x, pilot.y, 7.0, 2.0, WHITE);
    draw_circle(law_p.x, law_p.y, 4.0, GREEN_S);
    txt_c("MANCHE", cx, sb.y + bs + 14.0, 11.0, DIM);
    txt("o pilote", sb.x + bs + 10.0, sb.y + 16.0, 11.0, WHITE);
    txt("* loi FCS", sb.x + bs + 10.0, sb.y + 32.0, 11.0, GREEN_S);

    // Gouvernes : vue en plan avec élevons et dérives
    let gy = sb.y + bs + 30.0;
    let gh = r.y + r.h - gy - 12.0;
    let gcx = rx + rw / 2.0;
    let gw = rw * 0.42;
    // Silhouette de la page SYSTEMES
    let pr = Rect::new(gcx - gw * 1.3, gy, gw * 2.6, gh * 0.82);
    let to = crate::syspage::plan(pr);
    crate::syspage::silhouette(pr, Color::new(0.5, 0.55, 0.6, 1.0), Color::new(0.10, 0.12, 0.14, 1.0));
    let surf_col = |d: f64| {
        let k = (d.abs() / 25.0).clamp(0.0, 1.0) as f32;
        Color::new(0.2 + 0.8 * k, 1.0 - 0.3 * k, 0.3, 1.0)
    };
    for (side, d) in [(-1.0f32, f.elevon_l), (1.0, f.elevon_r)] {
        let pe = to(side * 0.48, 0.905);
        let ex = pe.x;
        let ew = (to(0.25, 0.0).x - to(0.0, 0.0).x).abs();
        let ey = pe.y;
        // Le volet descend (positif) ou monte (négatif) : représenté par sa hauteur
        let hgt = (d as f32 / 30.0) * gh * 0.18;
        rrect(Rect::new(ex - ew / 2.0, if hgt < 0.0 { ey + hgt } else { ey }, ew, hgt.abs().max(3.0)), 2.0, surf_col(d));
        txt_c(&format!("{d:+.0}"), ex, ey + 24.0, 12.0, WHITE);
    }
    for side in [-1.0f32, 1.0] {
        let (a, b) = (to(side * 0.16, 0.74), to(side * 0.27, 0.97));
        let lean = f.rudder as f32 / 25.0 * gw * 0.12;
        round_line(b, a + vec2(lean, 0.0), 3.0, surf_col(f.rudder));
    }
    txt_c(&format!("DERIVES {:+.0}", f.rudder), gcx, gy + gh * 0.55, 12.0, WHITE);
    txt_c("ELEVONS G / D", gcx, gy + 12.0, 11.0, DIM);
}

/// Graphe de l'enveloppe structurale : n de rupture en fonction de la température de structure.
fn envelope(ac: &Aircraft, r: Rect) {
    let (nb, nn) = ac.n_break();
    let nrec = ac.n_recovery();
    let skin = ac.skin_temp - 273.15;
    txt("ENVELOPPE STRUCTURALE", r.x, r.y + 10.0, 11.0, WHITE);
    txt_r(&format!("RUPTURE {nb:+.1} / {nn:+.1} g   RECOVERY {nrec:.1} g"), r.x + r.w, r.y + 10.0, 11.0, AMBER);
    let g = Rect::new(r.x + 30.0, r.y + 18.0, r.w - 34.0, r.h - 34.0);
    draw_rectangle(g.x, g.y, g.w, g.h, Color::new(0.05, 0.06, 0.07, 1.0));
    draw_rectangle_lines(g.x, g.y, g.w, g.h, 1.0, PANEL_BORDER);
    let (t0, t1, n0, n1) = (0.0f64, 1400.0f64, 0.0f64, 8.8f64);
    let px = |t: f64| g.x + ((t - t0) / (t1 - t0)) as f32 * g.w;
    let py = |n: f64| g.y + g.h - ((n - n0) / (n1 - n0)) as f32 * g.h;
    for n in [2.0, 4.0, 6.0] {
        draw_line(g.x, py(n), g.x + g.w, py(n), 1.0, Color::new(1.0, 1.0, 1.0, 0.08));
        txt_r(&format!("{n:.0}"), g.x - 4.0, py(n) + 4.0, 9.0, DIM);
    }
    for t in [500.0, 1000.0] {
        draw_line(px(t), g.y, px(t), g.y + g.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.08));
        txt_c(&format!("{t:.0}C"), px(t), g.y + g.h + 11.0, 9.0, DIM);
    }
    let curve = |m: f64, k: f64, c: Color, w: f32| {
        let mut prev: Option<Vec2> = None;
        for i in 0..=28 {
            let t = t0 + (t1 - t0) * i as f64 / 28.0;
            let p = vec2(px(t), py(n_break_at(m, t).0 * k));
            if let Some(q) = prev {
                draw_line(q.x, q.y, p.x, p.y, w, c);
            }
            prev = Some(p);
        }
    };
    // Masse maximale (pointillé gris), masse actuelle : rupture (rouge) et cible recovery (ambre)
    curve(M_MAX, 1.0, Color::new(0.6, 0.6, 0.65, 0.5), 1.0);
    curve(ac.mass(), 1.0, RED, 2.0);
    curve(ac.mass(), RECOVERY_MARGIN, AMBER, 1.5);
    // Point actuel (température de peau, facteur de charge)
    let p = vec2(px(skin.clamp(t0, t1)), py(ac.d.nz.clamp(n0, n1)));
    let c = if ac.d.nz > nrec { RED } else { WHITE };
    draw_circle(p.x, p.y, 4.0, c);
    draw_circle_lines(p.x, p.y, 6.0, 1.0, c);
    txt(&format!("{:.0} t", ac.mass() / 1000.0), g.x + 4.0, g.y + 11.0, 9.0, RED);
    txt("rupture", g.x + 40.0, g.y + 11.0, 9.0, RED);
    txt("recovery 90 %", g.x + 90.0, g.y + 11.0, 9.0, AMBER);
    txt("masse max", g.x + 180.0, g.y + 11.0, 9.0, Color::new(0.6, 0.6, 0.65, 0.8));
}
