//! Menu « panneau console » : ENGINE/APU et FLT CNTRL, reproduits d'après le Darkstar.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;

const PLATE: Color = Color::new(0.11, 0.13, 0.12, 1.0);
const PLATE_EDGE: Color = Color::new(0.20, 0.23, 0.21, 1.0);
const LEGEND: Color = Color::new(0.62, 0.95, 0.60, 1.0);
const SCREW: Color = Color::new(0.06, 0.07, 0.065, 1.0);

/// Actions demandées depuis le panneau.
pub enum MenuAction {
    EngSwitch(usize, EngSw),
    FadecReset(usize),
    Start,
    Stop,
    AlertStart(bool),
    AirDataOvrd(bool),
    Bit,
    SystemReset,
    YawTrim(f64),
    Close,
}

pub struct BitState {
    pub running: bool,
    pub fail: Option<bool>,
}

fn screw(x: f32, y: f32) {
    draw_circle(x, y, 7.0, SCREW);
    draw_circle_lines(x, y, 7.0, 1.0, Color::new(0.25, 0.27, 0.26, 1.0));
    draw_line(x - 4.0, y - 2.0, x + 4.0, y + 2.0, 1.5, Color::new(0.25, 0.27, 0.26, 1.0));
}

fn plate(r: Rect, title: &str) {
    rrect(r, 6.0, PLATE);
    rrect_lines(r, 6.0, 1.5, PLATE_EDGE);
    for (x, y) in [(r.x + 14.0, r.y + 14.0), (r.x + r.w - 14.0, r.y + 14.0), (r.x + 14.0, r.y + r.h - 14.0), (r.x + r.w - 14.0, r.y + r.h - 14.0)] {
        screw(x, y);
    }
    // Titre vertical, comme gravé sur la platine
    let n = title.chars().count() as f32;
    let step = 15.0;
    let y0 = r.y + r.h / 2.0 - n * step / 2.0;
    for (i, ch) in title.chars().enumerate() {
        txt_c(&ch.to_string(), r.x + 26.0, y0 + i as f32 * step + 10.0, 15.0, LEGEND);
    }
}

/// Interrupteur à levier ; `pos` : 0 = haut, 1 = milieu, 2 = bas. Renvoie la nouvelle position si cliqué.
fn toggle(cx: f32, cy: f32, pos: usize, positions: usize) -> Option<usize> {
    draw_circle(cx, cy, 13.0, Color::new(0.05, 0.05, 0.05, 1.0));
    draw_circle_lines(cx, cy, 13.0, 1.5, Color::new(0.45, 0.47, 0.46, 1.0));
    draw_circle(cx, cy, 6.0, Color::new(0.35, 0.36, 0.36, 1.0));
    let dy = match (positions, pos) {
        (3, 0) | (2, 0) => -22.0,
        (3, 1) => 0.0,
        _ => 22.0,
    };
    let tip = vec2(cx + if dy == 0.0 { 0.0 } else { 6.0 }, cy + dy);
    draw_line(cx, cy, tip.x, tip.y, 6.0, Color::new(0.78, 0.80, 0.82, 1.0));
    draw_circle(tip.x, tip.y, 5.0, Color::new(0.88, 0.9, 0.92, 1.0));
    let up = Rect::new(cx - 18.0, cy - 34.0, 36.0, 30.0);
    let dn = Rect::new(cx - 18.0, cy + 4.0, 36.0, 30.0);
    if clicked(up) && pos > 0 {
        return Some(pos - 1);
    }
    if clicked(dn) && pos + 1 < positions {
        return Some(pos + 1);
    }
    None
}

/// Bouton-poussoir à deux légendes éclairables. Renvoie vrai si pressé.
fn pushbutton(r: Rect, top: &str, top_lit: Option<Color>, bot: &str, bot_lit: Option<Color>) -> bool {
    rrect(r, 4.0, Color::new(0.03, 0.035, 0.035, 1.0));
    rrect_lines(r, 4.0, 1.5, Color::new(0.28, 0.30, 0.29, 1.0));
    let dim = Color::new(0.35, 0.38, 0.36, 1.0);
    txt_c(top, r.x + r.w / 2.0, r.y + r.h * 0.40, 12.0, top_lit.unwrap_or(dim));
    txt_c(bot, r.x + r.w / 2.0, r.y + r.h * 0.82, 12.0, bot_lit.unwrap_or(dim));
    let pressed = hovered(r) && is_mouse_button_down(MouseButton::Left);
    if pressed {
        draw_rectangle(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, Color::new(1.0, 1.0, 1.0, 0.06));
    }
    clicked(r)
}

/// Bouton carré lumineux (FADEC RESET). Renvoie vrai si pressé.
fn lit_square(r: Rect, lit: bool, time: f64) -> bool {
    let glow = if lit && (time * 2.0).fract() < 0.7 { Color::new(1.0, 0.68, 0.15, 1.0) } else { Color::new(0.42, 0.52, 0.48, 1.0) };
    draw_rectangle(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0, Color::new(0.04, 0.04, 0.04, 1.0));
    draw_rectangle(r.x, r.y, r.w, r.h, glow);
    draw_rectangle(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h * 0.35, Color::new(1.0, 1.0, 1.0, 0.12));
    clicked(r)
}

pub fn draw_menu(ac: &Aircraft, bit: &BitState, r: Rect, time: f64) -> Option<MenuAction> {
    let mut out = None;
    let e = &ac.eng;
    // Fond et cadre
    rrect(r, 10.0, Color::new(0.02, 0.025, 0.025, 0.97));
    rrect_lines(r, 10.0, 1.5, Color::new(0.3, 0.33, 0.32, 1.0));
    txt("MENU - CONSOLE MOTEURS / COMMANDES DE VOL", r.x + 16.0, r.y + 22.0, 15.0, WHITE);
    let close = Rect::new(r.x + r.w - 34.0, r.y + 8.0, 24.0, 22.0);
    rrect(close, 4.0, Color::new(0.15, 0.16, 0.17, 1.0));
    txt_c("X", close.x + 12.0, close.y + 16.0, 15.0, WHITE);
    if clicked(close) {
        out = Some(MenuAction::Close);
    }

    // --- Platine ENGINE/APU ---
    let p1 = Rect::new(r.x + 12.0, r.y + 36.0, r.w - 24.0, (r.h - 60.0) * 0.56);
    plate(p1, "ENGINE/APU");
    let x0 = p1.x + 50.0;
    let w = p1.w - 70.0;
    let col = |f: f32| x0 + w * f;
    let y_sw = p1.y + 56.0;
    // Interrupteurs L ENG / R ENG (ON / OFF / MOTOR)
    txt_c("L ENG", col(0.18), p1.y + 26.0, 13.0, LEGEND);
    txt_c("R ENG", col(0.82), p1.y + 26.0, 13.0, LEGEND);
    for (i, f) in [(0usize, 0.18f32), (1, 0.82)] {
        let pos = match e.sw[i] {
            EngSw::On => 0,
            EngSw::Off => 1,
            EngSw::Motor => 2,
        };
        if let Some(np) = toggle(col(f), y_sw, pos, 3) {
            let sw = [EngSw::On, EngSw::Off, EngSw::Motor][np];
            out = Some(MenuAction::EngSwitch(i, sw));
        }
        let (st, c) = match e.side[i] {
            TurbState::Running => (format!("RUN {:.0}%", e.n2s[i] * 104.0), LEGEND),
            TurbState::Starting => (format!("START {:.0}%", e.n2s[i] * 104.0), AMBER),
            TurbState::Failed => ("FAIL".into(), RED),
            TurbState::Off if e.n2s[i] > 0.05 => (format!("N2 {:.0}%", e.n2s[i] * 104.0), DIM),
            TurbState::Off => ("OFF".into(), DIM),
        };
        txt_c(&st, col(f), y_sw + 46.0, 12.0, c);
    }
    txt_c("ON", col(0.5), y_sw - 18.0, 12.0, LEGEND);
    txt_c("OFF", col(0.5), y_sw + 4.0, 12.0, LEGEND);
    txt_c("MOTOR", col(0.5), y_sw + 26.0, 12.0, LEGEND);

    // FADEC RESET (un bouton lumineux par moteur)
    let y_fr = p1.y + p1.h * 0.47;
    let sq = 40.0;
    for (i, f) in [(0usize, 0.24f32), (1, 0.76)] {
        if lit_square(Rect::new(col(f) - sq / 2.0, y_fr, sq, sq), e.fadec_fault[i], time) {
            out = Some(MenuAction::FadecReset(i));
        }
    }
    txt_c("FADEC", col(0.5), y_fr + 16.0, 13.0, LEGEND);
    txt_c("RESET", col(0.5), y_fr + 32.0, 13.0, LEGEND);

    // START / STOP / ALERT START ENABLE
    let y_ss = p1.y + p1.h * 0.76;
    let bw = 62.0;
    let bh = 44.0;
    txt_c("START", col(0.18), y_ss - 8.0, 13.0, LEGEND);
    txt_c("STOP", col(0.50), y_ss - 8.0, 13.0, LEGEND);
    let starting = e.side.contains(&TurbState::Starting);
    let running = e.side.contains(&TurbState::Running);
    let ready = !starting && !running && (0..2).any(|i| e.sw[i] == EngSw::On && e.side[i] == TurbState::Off && !e.fadec_fault[i]);
    let white = Color::new(0.95, 0.95, 0.9, 1.0);
    if pushbutton(Rect::new(col(0.18) - bw / 2.0, y_ss, bw, bh), "STBY", starting.then_some(white), "RDY", ready.then_some(GREEN_S)) {
        out = Some(MenuAction::Start);
    }
    let all_off = e.side.iter().all(|s| matches!(s, TurbState::Off | TurbState::Failed));
    if pushbutton(Rect::new(col(0.50) - bw / 2.0, y_ss, bw, bh), "RUN", running.then_some(GREEN_S), "OFF", all_off.then_some(AMBER)) {
        out = Some(MenuAction::Stop);
    }
    txt_c("ALERT", col(0.83), y_ss - 40.0, 13.0, LEGEND);
    txt_c("START", col(0.83), y_ss - 25.0, 13.0, LEGEND);
    txt_c("ENABLE", col(0.83), y_ss - 10.0, 13.0, LEGEND);
    if let Some(np) = toggle(col(0.83), y_ss + 24.0, if e.alert_start { 0 } else { 1 }, 2) {
        out = Some(MenuAction::AlertStart(np == 0));
    }
    txt_c(if e.alert_start { "ON" } else { "OFF" }, col(0.83), y_ss + 64.0, 13.0, LEGEND);

    // --- Platine FLT CNTRL ---
    let p2 = Rect::new(r.x + 12.0, p1.y + p1.h + 10.0, r.w - 24.0, r.y + r.h - 12.0 - (p1.y + p1.h + 10.0));
    plate(p2, "FLT CNTRL");
    let x0 = p2.x + 50.0;
    let w = p2.w - 70.0;
    let col = |f: f32| x0 + w * f;
    let y_top = p2.y + 22.0;
    // AIR DATA OVRD : voyant ambre + bouton
    txt_c("AIR DATA", col(0.18), y_top + 6.0, 13.0, LEGEND);
    txt_c("OVRD", col(0.18), y_top + 21.0, 13.0, LEGEND);
    let lamp = vec2(col(0.18), y_top + 44.0);
    let on = ac.air_data_ovrd;
    draw_circle(lamp.x, lamp.y, 9.0, Color::new(0.04, 0.04, 0.04, 1.0));
    draw_circle(lamp.x, lamp.y, 6.0, if on { Color::new(1.0, 0.6, 0.15, 1.0) } else { Color::new(0.3, 0.2, 0.1, 1.0) });
    if on {
        draw_circle(lamp.x, lamp.y, 12.0, Color::new(1.0, 0.6, 0.15, 0.2));
    }
    if clicked(Rect::new(lamp.x - 30.0, y_top - 8.0, 60.0, 64.0)) {
        out = Some(MenuAction::AirDataOvrd(!on));
    }
    txt_c(if ac.adc_degraded() { "ADC DEGR" } else if on { "INS" } else { "PITOT" }, col(0.18), y_top + 70.0, 11.0, if ac.adc_degraded() { AMBER } else { DIM });
    // INITIATE BIT
    txt_c("INITIATE", col(0.50), y_top + 6.0, 13.0, LEGEND);
    txt_c("BIT", col(0.50), y_top + 21.0, 13.0, LEGEND);
    let blink = (time * 3.0).fract() < 0.6;
    if pushbutton(
        Rect::new(col(0.50) - bw / 2.0, y_top + 30.0, bw, bh),
        "ON",
        (bit.running && blink).then_some(white),
        "FAIL",
        (bit.fail == Some(true)).then_some(AMBER),
    ) {
        out = Some(MenuAction::Bit);
    }
    if bit.fail == Some(false) && !bit.running {
        txt_c("BIT OK", col(0.50), y_top + 92.0, 11.0, GREEN_S);
    }
    // SYSTEM RESET
    txt_c("SYSTEM", col(0.82), y_top + 6.0, 13.0, LEGEND);
    txt_c("RESET", col(0.82), y_top + 21.0, 13.0, LEGEND);
    if pushbutton(Rect::new(col(0.82) - bw / 2.0, y_top + 30.0, bw, bh), "", None, "RESET", None) {
        out = Some(MenuAction::SystemReset);
    }
    // YAW TRIM (bouton rotatif) : clic à gauche / à droite, ou molette
    let kc = vec2(col(0.45), p2.y + p2.h - 52.0);
    txt_c("YAW TRIM", kc.x, kc.y - 40.0, 13.0, LEGEND);
    txt_r("LEFT", kc.x - 40.0, kc.y + 24.0, 12.0, LEGEND);
    txt("RIGHT", kc.x + 40.0, kc.y + 24.0, 12.0, LEGEND);
    draw_circle(kc.x, kc.y, 30.0, Color::new(0.30, 0.31, 0.32, 1.0));
    draw_circle(kc.x, kc.y, 24.0, Color::new(0.42, 0.43, 0.44, 1.0));
    let a = (-90.0 + ac.yaw_trim as f32 * 120.0).to_radians();
    draw_line(kc.x + 6.0 * a.cos(), kc.y + 6.0 * a.sin(), kc.x + 26.0 * a.cos(), kc.y + 26.0 * a.sin(), 4.0, WHITE);
    for k in -4..=4 {
        let t = (-90.0 + k as f32 * 30.0).to_radians();
        draw_line(kc.x + 34.0 * t.cos(), kc.y + 34.0 * t.sin(), kc.x + 39.0 * t.cos(), kc.y + 39.0 * t.sin(), 1.5, LEGEND);
    }
    txt_c(&format!("{:+.1}", ac.yaw_trim * 10.0), kc.x, kc.y + 48.0, 12.0, WHITE);
    let knob = Rect::new(kc.x - 40.0, kc.y - 40.0, 80.0, 80.0);
    if clicked(knob) {
        let dir = if mouse_position().0 < kc.x { -0.1 } else { 0.1 };
        out = Some(MenuAction::YawTrim((ac.yaw_trim + dir).clamp(-1.0, 1.0)));
    }
    if hovered(knob) {
        let (_, wy) = mouse_wheel();
        if wy != 0.0 {
            out = Some(MenuAction::YawTrim((ac.yaw_trim + wy.signum() as f64 * 0.05).clamp(-1.0, 1.0)));
        }
    }
    // Asymétrie de poussée (aide au trim)
    let asym = (e.t_side[0] - e.t_side[1]) / 1000.0;
    if asym.abs() > 5.0 {
        txt_c(&format!("ASYM POUSSEE {asym:+.0} kN -> TRIM {}", if asym > 0.0 { "LEFT" } else { "RIGHT" }), col(0.82), p2.y + p2.h - 30.0, 11.0, AMBER);
    }
    out
}
