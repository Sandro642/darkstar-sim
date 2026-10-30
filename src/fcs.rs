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
    let law = if ovr { ("LOI DIRECTE (FBW OVERRIDE)", AMBER) } else { ("LOI NORMALE", GREEN_S) };
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
    let pts = [(0.0, 0.0), (0.12, 0.45), (1.0, 0.85), (1.0, 0.95), (-1.0, 0.95), (-1.0, 0.85), (-0.12, 0.45)];
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        draw_line(gcx + a.0 * gw, gy + a.1 * gh, gcx + b.0 * gw, gy + b.1 * gh, 1.5, Color::new(0.5, 0.55, 0.6, 1.0));
    }
    let surf_col = |d: f64| {
        let k = (d.abs() / 25.0).clamp(0.0, 1.0) as f32;
        Color::new(0.2 + 0.8 * k, 1.0 - 0.3 * k, 0.3, 1.0)
    };
    for (side, d) in [(-1.0f32, f.elevon_l), (1.0, f.elevon_r)] {
        let ex = gcx + side * gw * 0.55;
        let ew = gw * 0.42;
        let ey = gy + 0.95 * gh;
        // Le volet descend (positif) ou monte (négatif) : représenté par sa hauteur
        let hgt = (d as f32 / 30.0) * gh * 0.18;
        draw_rectangle(ex - ew / 2.0, if hgt < 0.0 { ey + hgt } else { ey }, ew, hgt.abs().max(2.0), surf_col(d));
        txt_c(&format!("{d:+.0}"), ex, ey + gh * 0.18 + 14.0, 12.0, WHITE);
    }
    for side in [-1.0f32, 1.0] {
        let rxp = gcx + side * gw * 0.18;
        let top = gy + gh * 0.62;
        let bot = gy + gh * 0.95;
        let lean = f.rudder as f32 / 25.0 * gw * 0.12;
        draw_line(rxp, bot, rxp + lean, top, 3.0, surf_col(f.rudder));
    }
    txt_c(&format!("DERIVES {:+.0}", f.rudder), gcx, gy + gh * 0.55, 12.0, WHITE);
    txt_c("ELEVONS G / D", gcx, gy + 12.0, 11.0, DIM);
}
