//! Page ÉLEC / ENV : écran BATT / FUEL CELL et panneau de température (sélecteurs rétroéclairés).

use crate::aircraft::*;
use crate::systems::{self, Fail};
use crate::ui::*;
use macroquad::prelude::*;

const BACKLIT: Color = Color::new(0.72, 0.95, 0.30, 1.0);

pub enum EnvAction {
    TempKnob(f64),
    FaceHeat(u8),
    SelToggle,
    AutoToggle,
}

fn f_of(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

/// Échelle verticale graduée avec curseur triangulaire rouge et remplissage vert.
fn vscale(x: f32, y: f32, h: f32, v: f64, vmax: f64, ticks: &[f64], label: &str, right: bool) {
    txt_c(label, x + 8.0, y - 6.0, 11.0, WHITE);
    draw_rectangle(x, y, 16.0, h, Color::new(0.02, 0.02, 0.02, 1.0));
    draw_rectangle_lines(x, y, 16.0, h, 1.0, WHITE);
    let fy = |t: f64| y + h - (t / vmax).clamp(0.0, 1.0) as f32 * h;
    let level = fy(v);
    draw_rectangle(x + 2.0, level, 12.0, y + h - level, Color::new(0.25, 0.75, 0.55, 1.0));
    for &t in ticks {
        let ty = fy(t);
        if right {
            draw_line(x + 16.0, ty, x + 22.0, ty, 1.0, WHITE);
            txt(&format!("{t:.0}"), x + 24.0, ty + 4.0, 10.0, WHITE);
        } else {
            draw_line(x - 6.0, ty, x, ty, 1.0, WHITE);
            txt_r(&format!("{t:.0}"), x - 8.0, ty + 4.0, 10.0, WHITE);
        }
    }
    let tri_x = if right { x + 18.0 } else { x - 2.0 };
    let dir = if right { 1.0 } else { -1.0 };
    draw_triangle(vec2(tri_x, level), vec2(tri_x + dir * 9.0, level - 5.0), vec2(tri_x + dir * 9.0, level + 5.0), RED);
}

fn knob(cx: f32, cy: f32, r: f32, angle_deg: f32) {
    draw_circle(cx, cy, r, Color::new(0.33, 0.34, 0.35, 1.0));
    draw_circle(cx, cy, r * 0.8, Color::new(0.45, 0.46, 0.47, 1.0));
    let a = angle_deg.to_radians();
    draw_line(cx, cy, cx + r * 0.95 * a.cos(), cy + r * 0.95 * a.sin(), 4.0, Color::new(0.9, 0.9, 0.9, 1.0));
}

pub fn draw_elec(ac: &Aircraft, r: Rect, time: f64) -> Option<EnvAction> {
    let mut out = None;
    panel(r, "ELEC / ENV - BATTERIE, PILE A COMBUSTIBLE, TEMPERATURE");
    let el = &ac.elec;

    // --- Écran LCD BATT / FUEL CELL ---
    let lcd_w = (r.w * 0.52).min(330.0);
    let lcd = Rect::new(r.x + 14.0, r.y + 34.0, lcd_w, (r.h * 0.48).min(230.0));
    rrect(lcd, 6.0, BLACK);
    rrect_lines(lcd, 6.0, 2.0, Color::new(0.25, 0.26, 0.27, 1.0));
    let top = lcd.y + 26.0;
    let sh = lcd.h - 70.0;
    let (temp_v, temp_lbl) = if el.sel_ebay { (f_of(el.ebay_temp), "TEMP") } else { (f_of(el.ckpt_temp), "TEMP") };
    vscale(lcd.x + 34.0, top, sh, temp_v, 300.0, &[0.0, 60.0, 120.0, 180.0, 240.0, 300.0], temp_lbl, false);
    // Batterie
    let bx = lcd.x + 72.0;
    txt_c("BATT", bx + 22.0, top - 6.0, 11.0, WHITE);
    let charging = el.gen_kw + el.fc_kw >= systems::ELEC_LOAD_KW && el.batt < 0.999;
    let discharging = el.gen_kw + el.fc_kw < systems::ELEC_LOAD_KW;
    draw_triangle(vec2(bx + 22.0, top + 2.0), vec2(bx + 16.0, top + 10.0), vec2(bx + 28.0, top + 10.0), if charging { GREEN_S } else { WHITE });
    let cells = 6;
    let ch = (sh - 30.0) / cells as f32;
    for i in 0..cells {
        let cy = top + 14.0 + i as f32 * ch;
        let filled = el.batt * cells as f64 > (cells - 1 - i) as f64;
        draw_rectangle(bx + 4.0, cy, 36.0, ch - 4.0, if filled { WHITE } else { Color::new(0.2, 0.2, 0.2, 1.0) });
    }
    let by = top + 14.0 + cells as f32 * ch;
    draw_triangle(vec2(bx + 22.0, by + 10.0), vec2(bx + 16.0, by + 2.0), vec2(bx + 28.0, by + 2.0), if discharging { AMBER } else { WHITE });
    txt_c(&format!("{:.0}%", el.batt * 100.0), bx + 22.0, lcd.y + lcd.h - 14.0, 13.0, if el.batt < 0.2 { RED } else { WHITE });
    // Flèche pile → batterie / bus
    let fx = bx + 58.0;
    draw_line(fx + 26.0, top + sh * 0.45, bx + 46.0, top + sh * 0.45, 2.0, WHITE);
    draw_triangle(vec2(bx + 44.0, top + sh * 0.45), vec2(bx + 52.0, top + sh * 0.45 - 5.0), vec2(bx + 52.0, top + sh * 0.45 + 5.0), WHITE);
    // Pile à combustible : plaques
    txt_c("FUEL CELL", fx + 58.0, top - 6.0, 11.0, WHITE);
    let flow = if el.fc_on { (time * 3.0).fract() as f32 } else { 0.0 };
    for i in 0..6 {
        let px = fx + 30.0 + i as f32 * 13.0;
        let skew = 10.0;
        let pts = [vec2(px, top + 16.0 + skew), vec2(px + 9.0, top + 16.0), vec2(px + 9.0, top + sh - 10.0 - skew), vec2(px, top + sh - 10.0)];
        let c = if el.fc_on { Color::new(0.9, 0.95, 0.9, 1.0) } else { Color::new(0.35, 0.36, 0.36, 1.0) };
        draw_triangle(pts[0], pts[1], pts[2], c);
        draw_triangle(pts[0], pts[2], pts[3], c);
    }
    // Alimentations H2 et O2
    let lx = fx + 112.0;
    let ac_col = Color::new(1.0, 0.75, 0.1, 1.0);
    let h2c = if el.fc_on && ac.h2 > 0.0 { ac_col } else { Color::new(0.3, 0.25, 0.1, 1.0) };
    let o2c = if el.fc_on && el.o2 > 0.0 { ac_col } else { Color::new(0.3, 0.25, 0.1, 1.0) };
    draw_line(lx, top + sh * 0.25, lx + 14.0, top + sh * 0.25, 2.0, h2c);
    draw_line(lx + 14.0, top + sh * 0.25, lx + 14.0, top + sh * 0.1, 2.0, h2c);
    draw_line(lx, top + sh * 0.65, lx + 14.0, top + sh * 0.65, 2.0, o2c);
    draw_line(lx + 14.0, top + sh * 0.65, lx + 14.0, top + sh * 0.8, 2.0, o2c);
    draw_triangle(vec2(lx - 4.0, top + sh * 0.25), vec2(lx + 3.0, top + sh * 0.25 - 4.0), vec2(lx + 3.0, top + sh * 0.25 + 4.0), h2c);
    draw_triangle(vec2(lx - 4.0, top + sh * 0.65), vec2(lx + 3.0, top + sh * 0.65 - 4.0), vec2(lx + 3.0, top + sh * 0.65 + 4.0), o2c);
    txt("H2", lx + 18.0, top + sh * 0.1 + 4.0, 11.0, WHITE);
    txt("O2", lx + 18.0, top + sh * 0.8 + 4.0, 11.0, WHITE);
    if el.fc_on {
        let yy = top + sh * 0.25 + (flow - 0.5) * 6.0;
        draw_circle(lx + 7.0, yy, 1.5, ac_col);
    }
    let sts = Rect::new(fx + 22.0, lcd.y + lcd.h - 28.0, 84.0, 18.0);
    draw_rectangle(sts.x, sts.y, sts.w, sts.h, Color::new(0.25, 0.75, 0.6, 1.0));
    txt_c(&format!("STS {:.0}%", el.fc_status()), sts.x + sts.w / 2.0, sts.y + 14.0, 12.0, BLACK);
    // Température pile
    vscale(lcd.x + lcd.w - 54.0, top, sh, f_of(el.fc_temp), 400.0, &[0.0, 50.0, 100.0, 150.0, 200.0, 250.0, 300.0, 350.0, 400.0], "TEMP", true);

    // --- Bilan électrique ---
    let ix = lcd.x + lcd.w + 16.0;
    let mut iy = lcd.y + 14.0;
    let line = |y: &mut f32, l: &str, v: String, c: Color| {
        txt(l, ix, *y, 12.0, DIM);
        txt_r(&v, r.x + r.w - 14.0, *y, 12.0, c);
        *y += 18.0;
    };
    let gen_state = |i: usize| {
        let f = if i == 0 { Fail::GenL } else { Fail::GenR };
        if ac.fails.has(f) {
            ("PANNE".to_string(), RED)
        } else if ac.eng.side[i] == TurbState::Running && ac.eng.n2s[i] > 0.55 {
            (format!("{:.0} kW", systems::GEN_KW), GREEN_S)
        } else {
            ("HORS LIGNE".to_string(), DIM)
        }
    };
    let (g0, c0) = gen_state(0);
    let (g1, c1) = gen_state(1);
    line(&mut iy, "GEN L", g0, c0);
    line(&mut iy, "GEN R", g1, c1);
    line(&mut iy, "PILE H2/O2", if el.fc_on { format!("{:.0} kW", el.fc_kw) } else { "VEILLE".into() }, if el.fc_on { CYAN_S } else { DIM });
    line(&mut iy, "CHARGE BUS", format!("{:.0} kW", systems::ELEC_LOAD_KW), WHITE);
    line(&mut iy, "BATTERIE", format!("{:.0} %{}", el.batt * 100.0, if discharging { " DECHARGE" } else { "" }), if el.batt < 0.2 { RED } else if discharging { AMBER } else { GREEN_S });
    line(&mut iy, "OXYGENE PILE", format!("{:.0} %", el.o2 * 100.0), if el.o2 < 0.2 { AMBER } else { WHITE });
    line(&mut iy, "BUS ESSENTIEL", if el.bus_ok { "ALIMENTE".into() } else { "PERDU".into() }, if el.bus_ok { GREEN_S } else { RED });
    let endu = if discharging { el.batt * 1800.0 / ((systems::ELEC_LOAD_KW - el.gen_kw - el.fc_kw) / systems::ELEC_LOAD_KW).max(0.01) } else { 0.0 };
    if discharging {
        line(&mut iy, "AUTONOMIE BATT", crate::mission::fmt_time(endu), AMBER);
    }

    // --- Panneau température (rétroéclairé) ---
    let pp = Rect::new(r.x + 14.0, lcd.y + lcd.h + 14.0, r.w - 28.0, r.y + r.h - (lcd.y + lcd.h + 14.0) - 12.0);
    rrect(pp, 8.0, Color::new(0.06, 0.065, 0.06, 1.0));
    rrect_lines(pp, 8.0, 1.5, Color::new(0.2, 0.22, 0.2, 1.0));
    let kr = (pp.h * 0.2).clamp(14.0, 26.0);
    let cy = pp.y + pp.h * 0.45;
    // TEMP COLD / WARM
    let k1 = vec2(pp.x + pp.w * 0.17, cy);
    txt_c("TEMP", k1.x - 10.0, cy - kr - 18.0, 13.0, BACKLIT);
    arc(k1.x, k1.y, kr + 10.0, (200f32).to_radians(), (340f32).to_radians() + std::f32::consts::TAU * 0.0, 2.0, BACKLIT);
    txt("COLD", k1.x - kr - 48.0, cy + 4.0, 11.0, BACKLIT);
    txt("WARM", k1.x + kr + 8.0, cy + kr + 6.0, 11.0, BACKLIT);
    knob(k1.x, k1.y, kr, 200.0 + 140.0 * el.temp_knob as f32);
    let k1r = Rect::new(k1.x - kr - 6.0, k1.y - kr - 6.0, 2.0 * kr + 12.0, 2.0 * kr + 12.0);
    if clicked(k1r) {
        let d = if mouse_position().0 < k1.x { -0.1 } else { 0.1 };
        out = Some(EnvAction::TempKnob((el.temp_knob + d).clamp(0.0, 1.0)));
    }
    // FACEHEAT OFF / 1..7
    let k2 = vec2(pp.x + pp.w * 0.50, cy);
    for i in 0..8 {
        let a = (150.0 + i as f32 * 34.0).to_radians();
        draw_line(k2.x + kr * 1.1 * a.cos(), k2.y + kr * 1.1 * a.sin(), k2.x + (kr + 14.0) * a.cos(), k2.y + (kr + 14.0) * a.sin(), 1.5, BACKLIT);
        let lbl = if i == 0 { "OFF".to_string() } else { format!("{i}") };
        txt_c(&lbl, k2.x + (kr + 24.0) * a.cos(), k2.y + (kr + 24.0) * a.sin() + 4.0, 11.0, BACKLIT);
    }
    txt_c("FACEHEAT", k2.x, cy + kr + 40.0, 11.0, BACKLIT);
    knob(k2.x, k2.y, kr, 150.0 + el.faceheat as f32 * 34.0);
    if clicked(Rect::new(k2.x - kr - 6.0, k2.y - kr - 6.0, 2.0 * kr + 12.0, 2.0 * kr + 12.0)) {
        out = Some(EnvAction::FaceHeat((el.faceheat + 1) % 8));
    }
    // TEMP IND SELECTOR CKPT / E BAY
    let k3 = vec2(pp.x + pp.w * 0.82, cy);
    txt_c("CKPT", k3.x, cy - kr - 10.0, 12.0, BACKLIT);
    txt_c("E BAY", k3.x, cy + kr + 20.0, 12.0, BACKLIT);
    arc(k3.x, k3.y, kr + 8.0, (-90f32).to_radians(), (90f32).to_radians(), 2.0, BACKLIT);
    knob(k3.x, k3.y, kr, if el.sel_ebay { 40.0 } else { -40.0 });
    txt_c("TEMP IND SELECTOR", k3.x, pp.y + pp.h - 10.0, 11.0, BACKLIT);
    if clicked(Rect::new(k3.x - kr - 6.0, k3.y - kr - 6.0, 2.0 * kr + 12.0, 2.0 * kr + 12.0)) {
        out = Some(EnvAction::SelToggle);
    }
    // HOLD AUTO TEMP / MAN
    let sw = vec2(pp.x + pp.w * 0.17 - 30.0, pp.y + pp.h - 18.0);
    draw_circle(sw.x, sw.y, 6.0, Color::new(0.1, 0.1, 0.1, 1.0));
    draw_circle(sw.x + if el.auto_temp { 4.0 } else { -4.0 }, sw.y - 2.0, 3.5, Color::new(0.8, 0.8, 0.82, 1.0));
    txt("MAN", sw.x - 36.0, sw.y + 4.0, 11.0, if el.auto_temp { DIM } else { BACKLIT });
    txt("HOLD AUTO TEMP", sw.x + 14.0, sw.y + 4.0, 11.0, if el.auto_temp { BACKLIT } else { DIM });
    if clicked(Rect::new(sw.x - 40.0, sw.y - 10.0, 150.0, 20.0)) {
        out = Some(EnvAction::AutoToggle);
    }
    txt(&format!("{} {:.0} C", if el.sel_ebay { "E BAY" } else { "CKPT" }, if el.sel_ebay { el.ebay_temp } else { el.ckpt_temp }), pp.x + pp.w * 0.5 - 30.0, pp.y + 16.0, 12.0, WHITE);
    out
}
