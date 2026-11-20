//! Poste pilote (version embarquée) : panneaux physiques autour de l'écran.
//! - RADIO (haut) : COMM1 / COMM2 / NAV1 / NAV2 (active / attente, permutation, réglage MHz et
//!   kHz), transpondeur (code octal, STBY / ON / ALT, IDENT) ;
//! - MOTEURS (gauche) : trois boutons de page — 1 moteurs simple (nacelles), 2 données moteur,
//!   3 pourcentage manette 0-100 % ;
//! - MANETTE (bas gauche) : manette des gaz, levier de train, JETTISON protégé ;
//! - SCRAM (bas milieu) : bouton scramjet ;

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;

/// Action d'un panneau du poste pilote.
pub enum Act {
    Cmd(String),
    Key(KeyCode),
    Throttle(f64),
}

const FACE: Color = Color::new(0.07, 0.08, 0.09, 1.0);
const LBL: Color = Color::new(0.62, 0.66, 0.7, 1.0);

/// Bouton-poussoir à légende (voyant en haut).
fn push(r: Rect, title: &str, state: &str, lit: Option<Color>) -> bool {
    rrect(r, 4.0, if hovered(r) { Color::new(0.14, 0.15, 0.18, 1.0) } else { Color::new(0.10, 0.11, 0.13, 1.0) });
    rrect_lines(r, 4.0, 1.0, Color::new(0.28, 0.31, 0.35, 1.0));
    draw_rectangle(r.x + 5.0, r.y + 4.0, r.w - 10.0, 3.0, lit.unwrap_or(Color::new(0.22, 0.24, 0.27, 1.0)));
    txt_c(title, r.x + r.w / 2.0, r.y + r.h * 0.5 + 1.0, 10.0, WHITE);
    if !state.is_empty() {
        txt_c(state, r.x + r.w / 2.0, r.y + r.h * 0.5 + 13.0, 9.0, lit.unwrap_or(LBL));
    }
    clicked(r)
}

// ---------- RADIO ----------

#[derive(Clone, Debug)]
pub struct Radio {
    /// (active, attente) en MHz : COMM1, COMM2, NAV1, NAV2
    pub ch: [(f64, f64); 4],
    pub xpdr: [u8; 4],
    /// 0 STBY, 1 ON, 2 ALT
    pub mode: u8,
    pub ident_t: f64,
}

impl Radio {
    pub fn new() -> Radio {
        Radio { ch: [(124.850, 121.500), (118.700, 126.300), (113.900, 110.300), (113.500, 116.800)], xpdr: [1, 2, 0, 0], mode: 2, ident_t: -100.0 }
    }

    pub fn squawk(&self) -> String {
        self.xpdr.iter().map(|d| char::from(b'0' + d)).collect()
    }
}

fn freq_step(k: usize, mhz: bool, up: bool, f: f64) -> f64 {
    let nav = k >= 2;
    let (lo, hi) = if nav { (108.0, 117.95) } else { (118.0, 136.975) };
    let step = if mhz { 1.0 } else if nav { 0.05 } else { 0.025 };
    let mut v = f + if up { step } else { -step };
    // Bouclage dans la bande
    if v > hi + 1e-6 {
        v = lo + (v - hi - step).max(0.0);
    }
    if v < lo - 1e-6 {
        v = hi - (lo - v - step).max(0.0);
    }
    (v * 1000.0).round() / 1000.0
}

/// Panneau radio : renvoie un message éventuel. `ils` : fréquence ILS calée par le FMS (NAV1).
/// Une colonne par voie (COMM1, COMM2, NAV1, NAV2 : active verte, attente blanche, permutation,
/// réglage MHz / kHz), puis le transpondeur (mode, IDENT, code octal).
pub fn radio(rd: &mut Radio, r: Rect, ils: Option<f64>, time: f64) -> Option<String> {
    let mut out = None;
    rrect(r, 8.0, FACE);
    rrect_lines(r, 8.0, 2.0, Color::new(0.35, 0.7, 0.4, 0.8));
    if let Some(f) = ils {
        rd.ch[2].0 = f;
    }
    let names = ["COMM1", "COMM2", "NAV1", "NAV2"];
    let pad = 6.0;
    let n = 5.0;
    let cw = (r.w - (n + 1.0) * pad) / n;
    let ch = r.h - 2.0 * pad;
    let green = Color::new(0.35, 1.0, 0.45, 1.0);
    let fs = (ch * 0.2).clamp(11.0, 20.0);
    for k in 0..4 {
        let c = Rect::new(r.x + pad + k as f32 * (cw + pad), r.y + pad, cw, ch);
        rrect(c, 4.0, Color::new(0.02, 0.03, 0.03, 1.0));
        rrect_lines(c, 4.0, 1.0, Color::new(0.2, 0.3, 0.22, 1.0));
        txt_c(names[k], c.x + c.w / 2.0, c.y + 12.0, 10.0, WHITE);
        let (act, stby) = rd.ch[k];
        // Active
        let ay = c.y + 16.0;
        let ab = Rect::new(c.x + 4.0, ay, c.w - 8.0, fs + 8.0);
        rrect(ab, 3.0, Color::new(0.05, 0.12, 0.06, 1.0));
        txt_c(&format!("{act:.3}"), ab.x + ab.w / 2.0, ab.y + fs + 2.0, fs, green);
        // Permutation
        let swb = Rect::new(c.x + c.w / 2.0 - 14.0, ab.y + ab.h + 2.0, 28.0, 14.0);
        rrect(swb, 2.0, if hovered(swb) { Color::new(0.25, 0.7, 0.35, 1.0) } else { Color::new(0.15, 0.5, 0.22, 1.0) });
        txt_c("<->", swb.x + swb.w / 2.0, swb.y + 11.0, 9.0, BLACK);
        if clicked(swb) {
            rd.ch[k] = (stby, act);
            out = Some(format!("{} : {:.3} ACTIVE", names[k], stby));
        }
        // Attente et réglage
        let sy = swb.y + swb.h + 2.0;
        txt_c(&format!("{stby:.3}"), c.x + c.w / 2.0, sy + fs * 0.85, fs * 0.85, WHITE);
        let by = (sy + fs + 2.0).min(c.y + c.h - 16.0);
        let bw = (c.w - 8.0 - 3.0 * 3.0) / 4.0;
        for (j, (mhz, up, l)) in [(true, false, "MHz-"), (true, true, "MHz+"), (false, false, "kHz-"), (false, true, "kHz+")].iter().enumerate() {
            let b = Rect::new(c.x + 4.0 + j as f32 * (bw + 3.0), by, bw, 14.0);
            rrect(b, 2.0, if hovered(b) { Color::new(0.2, 0.25, 0.22, 1.0) } else { Color::new(0.1, 0.13, 0.11, 1.0) });
            txt_c(l, b.x + b.w / 2.0, b.y + 10.0, 8.0, if *mhz { WHITE } else { CYAN_S });
            if clicked(b) {
                rd.ch[k].1 = freq_step(k, *mhz, *up, rd.ch[k].1);
            }
        }
    }
    // Transpondeur
    let c = Rect::new(r.x + pad + 4.0 * (cw + pad), r.y + pad, cw, ch);
    rrect(c, 4.0, Color::new(0.02, 0.03, 0.03, 1.0));
    rrect_lines(c, 4.0, 1.0, Color::new(0.2, 0.3, 0.22, 1.0));
    txt_c("XPDR", c.x + c.w / 2.0, c.y + 12.0, 10.0, WHITE);
    let dw = (c.w - 8.0 - 3.0 * 2.0) / 4.0;
    for i in 0..4 {
        let d = Rect::new(c.x + 4.0 + i as f32 * (dw + 2.0), c.y + 16.0, dw, fs + 8.0);
        rrect(d, 3.0, Color::new(0.05, 0.12, 0.06, 1.0));
        txt_c(&rd.xpdr[i].to_string(), d.x + d.w / 2.0, d.y + fs + 2.0, fs, green);
        if clicked(d) {
            rd.xpdr[i] = (rd.xpdr[i] + 1) % 8;
            out = Some(format!("SQUAWK {}", rd.squawk()));
        }
    }
    let modes = ["STNDBY", "ON", "ALT"];
    let half = (c.w - 10.0) / 2.0;
    let by = c.y + 16.0 + fs + 12.0;
    let bh = (c.y + c.h - 4.0 - by).clamp(18.0, 34.0);
    if push(Rect::new(c.x + 4.0, by, half, bh), modes[rd.mode as usize], "", Some(if rd.mode == 0 { AMBER } else { GREEN_S })) {
        rd.mode = (rd.mode + 1) % 3;
        out = Some(format!("TRANSPONDEUR {}", modes[rd.mode as usize]));
    }
    let ident = time - rd.ident_t < 18.0;
    if push(Rect::new(c.x + 6.0 + half, by, half, bh), "IDENT", "", ident.then_some(CYAN_S)) {
        rd.ident_t = time;
        out = Some(format!("IDENT — SQUAWK {}", rd.squawk()));
    }
    out
}

// ---------- MOTEURS (3 pages) ----------

/// Panneau moteurs : page 1 nacelles, 2 données moteur, 3 manette %. Boutons verts en bas.
pub fn engines(ac: &Aircraft, mon: &crate::health::ChamberMon, r: Rect, page: &mut u8, time: f64) {
    rrect(r, 8.0, FACE);
    let bh = 22.0;
    let scr = Rect::new(r.x + 6.0, r.y + 6.0, r.w - 12.0, r.h - bh - 18.0);
    draw_rectangle(scr.x, scr.y, scr.w, scr.h, BLACK);
    match *page {
        1 => crate::tbcc::draw_both(scr, ac, time),
        2 => crate::panels::draw_engines(ac, mon, scr, time),
        3 => throttle_page(ac, scr),
        _ => {
            txt("FWD", scr.x + 6.0, scr.y + 12.0, 9.0, LBL);
            txt_r("AFT", scr.x + scr.w - 6.0, scr.y + 12.0, 9.0, LBL);
            crate::panels::draw_nacelles(ac, Rect::new(scr.x + 4.0, scr.y + 16.0, scr.w - 8.0, scr.h - 20.0), time);
        }
    }
    // Un seul bouton : page suivante (nom de la page, repères de position)
    let names = ["NACELLES", "VUE EN COUPE TBCC", "DONNEES MOTEUR", "MANETTE"];
    let b = Rect::new(r.x + 6.0, r.y + r.h - bh - 6.0, r.w - 12.0, bh);
    let hov = hovered(b);
    rrect(b, 4.0, if hov { Color::new(0.2, 0.21, 0.235, 1.0) } else { Color::new(0.14, 0.145, 0.16, 1.0) });
    rrect_lines(b, 4.0, 1.0, Color::new(0.24, 0.25, 0.28, 1.0));
    let cur = (*page as usize).min(3);
    txt(names[cur], b.x + 10.0, b.y + b.h / 2.0 + 4.0, 11.0, WHITE);
    for k in 0..4 {
        let c = vec2(b.x + b.w - 54.0 + k as f32 * 10.0, b.y + b.h / 2.0);
        if k == cur {
            draw_circle(c.x, c.y, 3.0, ACCENT);
        } else {
            draw_circle_lines(c.x, c.y, 3.0, 1.0, LBL);
        }
    }
    let (ax, ay) = (b.x + b.w - 12.0, b.y + b.h / 2.0);
    draw_line(ax - 4.0, ay - 4.0, ax, ay, 1.5, WHITE);
    draw_line(ax, ay, ax - 4.0, ay + 4.0, 1.5, WHITE);
    if clicked(b) {
        *page = ((cur + 1) % 4) as u8;
    }
}

/// Page 3 : pourcentage manette 0-100 %, poussée.
fn throttle_page(ac: &Aircraft, r: Rect) {
    let pct = (ac.throttle * 100.0) as f32;
    txt_c("MANETTE", r.x + r.w / 2.0, r.y + 18.0, 12.0, LBL);
    txt_c(&format!("{pct:.0} %"), r.x + r.w / 2.0, r.y + r.h * 0.45, (r.h * 0.22).clamp(18.0, 44.0), if pct > 100.0 { AMBER } else { GREEN_S });
    let b = Rect::new(r.x + 14.0, r.y + r.h * 0.62, r.w - 28.0, 16.0);
    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.12, 0.13, 0.15, 1.0));
    draw_rectangle(b.x, b.y, b.w * (pct / 100.0).clamp(0.0, 1.0), b.h, if pct > 80.0 { AMBER } else { GREEN_S });
    for t in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let x = b.x + b.w * t;
        draw_line(x, b.y + b.h, x, b.y + b.h + 5.0, 1.0, LBL);
        txt_c(&format!("{:.0}", t * 100.0), x, b.y + b.h + 16.0, 9.0, LBL);
    }
    txt_c(&format!("POUSSEE {:.0} kN   PC {:.0} %", ac.d.thrust / 1000.0, ac.eng.ab_frac * 100.0), r.x + r.w / 2.0, r.y + r.h - 10.0, 11.0, WHITE);
}

// ---------- MANETTE / TRAIN / JETTISON ----------

thread_local! {
    static JETT_ARM: std::cell::Cell<f64> = const { std::cell::Cell::new(-100.0) };
    static THR_DRAG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn throttle(ac: &Aircraft, r: Rect) -> Option<Act> {
    let mut out = None;
    rrect(r, 8.0, FACE);
    txt("MANETTE", r.x + 10.0, r.y + 16.0, 10.0, LBL);
    // Rail de la manette (0 en bas, 100 % en haut ; au-delà avec ENG OVRD)
    let maxv = ac.thr_max();
    let rail = Rect::new(r.x + 22.0, r.y + 28.0, 26.0, r.h - 44.0);
    draw_rectangle(rail.x, rail.y, rail.w, rail.h, Color::new(0.02, 0.02, 0.03, 1.0));
    draw_rectangle_lines(rail.x, rail.y, rail.w, rail.h, 1.0, Color::new(0.3, 0.32, 0.35, 1.0));
    let to_y = |v: f64| rail.y + rail.h - (v / maxv).clamp(0.0, 1.0) as f32 * rail.h;
    for (v, l) in [(0.0, "IDLE"), (0.8, "MIL"), (1.0, "MAX")] {
        if v <= maxv {
            let y = to_y(v);
            draw_line(rail.x + rail.w, y, rail.x + rail.w + 6.0, y, 1.0, LBL);
            txt(l, rail.x + rail.w + 8.0, y + 4.0, 9.0, LBL);
        }
    }
    let hy = to_y(ac.throttle);
    let handle = Rect::new(rail.x - 8.0, hy - 9.0, rail.w + 16.0, 18.0);
    rrect(handle, 4.0, Color::new(0.75, 0.77, 0.8, 1.0));
    txt_c(&format!("{:.0}", ac.throttle * 100.0), handle.x + handle.w / 2.0, handle.y + 13.0, 10.0, BLACK);
    let (mx, my) = mouse_position();
    let grab = Rect::new(rail.x - 10.0, rail.y - 10.0, rail.w + 20.0, rail.h + 20.0);
    if is_mouse_button_pressed(MouseButton::Left) && hovered(grab) {
        THR_DRAG.with(|c| c.set(true));
    }
    if !is_mouse_button_down(MouseButton::Left) {
        THR_DRAG.with(|c| c.set(false));
    }
    if THR_DRAG.with(|c| c.get()) {
        let _ = mx;
        let v = ((rail.y + rail.h - my) / rail.h).clamp(0.0, 1.0) as f64 * maxv;
        out = Some(Act::Throttle((v * 100.0).round() / 100.0));
    }
    // Levier de train (roue) et JETTISON protégé
    let cx = r.x + r.w * 0.68;
    let gw = (r.w * 0.5).min(90.0);
    let gear = Rect::new(cx - gw / 2.0, r.y + 28.0, gw, 54.0);
    let (gl, gc) = if ac.gear_pos > 0.01 && ac.gear_pos < 0.99 { ("TRANSIT", AMBER) } else if ac.gear_down { ("DOWN", GREEN_S) } else { ("UP", LBL) };
    rrect(gear, 6.0, Color::new(0.10, 0.11, 0.13, 1.0));
    rrect_lines(gear, 6.0, 1.5, gc);
    draw_circle(gear.x + gear.w / 2.0, gear.y + 22.0, 12.0, Color::new(0.85, 0.86, 0.88, 1.0));
    draw_circle(gear.x + gear.w / 2.0, gear.y + 22.0, 5.0, Color::new(0.3, 0.32, 0.35, 1.0));
    txt_c(&format!("TRAIN {gl}"), gear.x + gear.w / 2.0, gear.y + 48.0, 9.0, gc);
    if clicked(gear) {
        out = Some(Act::Key(KeyCode::G));
    }
    let now = get_time();
    let armed = now - JETT_ARM.with(|c| c.get()) < 5.0;
    let jb = Rect::new(cx - gw / 2.0, gear.y + gear.h + 10.0, gw, 40.0);
    // Capot rayé jaune / noir
    draw_rectangle(jb.x - 3.0, jb.y - 3.0, jb.w + 6.0, jb.h + 6.0, Color::new(0.9, 0.75, 0.1, 1.0));
    let mut x = jb.x - 3.0;
    while x < jb.x + jb.w + 3.0 {
        draw_line(x, jb.y - 3.0, (x + 8.0).min(jb.x + jb.w + 3.0), jb.y + 5.0, 3.0, BLACK);
        x += 12.0;
    }
    if push(jb, "JETTISON", if armed { "CONFIRMER" } else { "2 APPUIS" }, armed.then_some(RED)) {
        if armed {
            JETT_ARM.with(|c| c.set(-100.0));
            out = Some(Act::Cmd("JETTISON".into()));
        } else {
            JETT_ARM.with(|c| c.set(now));
        }
    }
    out
}

// ---------- SCRAM ----------

pub fn scram(ac: &Aircraft, r: Rect, time: f64) -> Option<Act> {
    rrect(r, 8.0, FACE);
    let e = &ac.eng;
    let (st, c) = match e.scram {
        ScramState::Lit => ("LIT", Color::new(1.0, 0.35, 0.95, 1.0)),
        ScramState::Armed => ("ARM", CYAN_S),
        ScramState::Unstart => ("UNSTART", AMBER),
        _ => ("OFF", LBL),
    };
    let rad = (r.w.min(r.h - 30.0) * 0.36).max(18.0);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0 - 6.0);
    let blink = e.scram == ScramState::Unstart && (time * 3.0).fract() < 0.5;
    draw_circle(cx, cy, rad + 4.0, Color::new(0.2, 0.22, 0.25, 1.0));
    draw_circle(cx, cy, rad, if e.scram == ScramState::Lit { Color::new(0.45, 0.1, 0.42, 1.0) } else { Color::new(0.08, 0.08, 0.1, 1.0) });
    draw_circle_lines(cx, cy, rad, 2.0, if blink { WHITE } else { c });
    txt_c("SCRAM", cx, cy + 4.0, 14.0, WHITE);
    txt_c(st, cx, r.y + r.h - 10.0, 11.0, c);
    let ok = ac.d.mach >= 4.8;
    txt_c(if ok { "MACH OK" } else { "MACH < 4.8" }, cx, r.y + 14.0, 9.0, if ok { GREEN_S } else { LBL });
    if clicked(Rect::new(cx - rad, cy - rad, 2.0 * rad, 2.0 * rad)) {
        return Some(Act::Key(KeyCode::U));
    }
    None
}

// ---------- BATTERIE ----------

/// Bouton batterie principale (comme le bouton scramjet) : ON / OFF, charge.
pub fn battery(ac: &Aircraft, r: Rect) -> Option<Act> {
    rrect(r, 8.0, FACE);
    let el = &ac.elec;
    let on = el.batt_sw;
    let c = if !on { LBL } else if el.batt < 0.25 { AMBER } else { GREEN_S };
    let rad = (r.w.min(r.h - 30.0) * 0.36).max(18.0);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0 - 6.0);
    draw_circle(cx, cy, rad + 4.0, Color::new(0.2, 0.22, 0.25, 1.0));
    draw_circle(cx, cy, rad, if on { Color::new(0.08, 0.3, 0.12, 1.0) } else { Color::new(0.08, 0.08, 0.1, 1.0) });
    draw_circle_lines(cx, cy, rad, 2.0, c);
    txt_c("BATT", cx, cy + 4.0, 14.0, WHITE);
    txt_c(if on { "ON" } else { "OFF" }, cx, r.y + r.h - 10.0, 11.0, c);
    txt_c(&format!("CHARGE {:.0} %", el.batt * 100.0), cx, r.y + 14.0, 9.0, c);
    if clicked(Rect::new(cx - rad, cy - rad, 2.0 * rad, 2.0 * rad)) {
        return Some(Act::Cmd(if on { "BATT OFF".into() } else { "BATT ON".into() }));
    }
    None
}

// ---------- MASTER WARNING / MASTER CAUTION ----------

/// Boutons MASTER WARNING / MASTER CAUTION reproduits sur le modèle réel, côte à côte (ou
/// l'un sous l'autre) dans `r` ; vrai si l'un est appuyé (acquittement).
pub fn master(r: Rect, warn: bool, caut: bool, time: f64) -> bool {
    rrect(r, 8.0, FACE);
    master_pair(Rect::new(r.x + 8.0, r.y + 8.0, r.w - 16.0, r.h - 16.0), warn, caut, time)
}

/// Les deux boutons dans `r`, sans fond (pour l'auvent de l'écran de la version normale).
pub fn master_pair(r: Rect, warn: bool, caut: bool, time: f64) -> bool {
    let blink = (time * 2.5).fract() < 0.6;
    let vertical = r.h > r.w * 0.9;
    let gap = 8.0;
    let (cw, chh) = if vertical { (r.w, (r.h - gap) / 2.0) } else { ((r.w - gap) / 2.0, r.h) };
    let aspect = 1.95;
    let (hw, hh) = if cw / chh > aspect { (chh * aspect, chh) } else { (cw, cw / aspect) };
    let mut pressed = false;
    for k in 0..2 {
        let cell = if vertical { Rect::new(r.x, r.y + k as f32 * (chh + gap), cw, chh) } else { Rect::new(r.x + k as f32 * (cw + gap), r.y, cw, chh) };
        let hb = Rect::new(cell.x + (cell.w - hw) / 2.0, cell.y + (cell.h - hh) / 2.0, hw, hh);
        let lit = if k == 0 { warn && blink } else { caut };
        if master_button(hb, k == 0, lit) {
            pressed = true;
        }
    }
    pressed
}

/// Un bouton (boîtier `hb`) : boîtier noir mat, deux vis cruciformes, poussoir en relief gris
/// foncé, légende gravée « MASTER / CAUTION / PRESS TO RESET » qui s'éclaire (ambre, ou rouge
/// pour WARNING) ; s'enfonce à l'appui. Vrai si cliqué.
pub fn master_button(hb: Rect, warning: bool, lit: bool) -> bool {
    let (hw, hh) = (hb.w, hb.h);
    let (l2, lamp) = if warning { ("WARNING", Color::new(1.0, 0.22, 0.16, 1.0)) } else { ("CAUTION", Color::new(1.0, 0.72, 0.16, 1.0)) };
    {
        // --- Boîtier ---
        rrect(Rect::new(hb.x + 1.5, hb.y + 2.5, hb.w, hb.h), 3.0, Color::new(0.0, 0.0, 0.0, 0.55));
        rrect(hb, 3.0, Color::new(0.075, 0.075, 0.08, 1.0));
        draw_line(hb.x + 3.0, hb.y + 1.0, hb.x + hb.w - 3.0, hb.y + 1.0, 1.0, Color::new(0.2, 0.2, 0.21, 1.0));
        // --- Vis cruciformes ---
        let sr = (hh * 0.1).max(3.0);
        for sx in [hb.x + hw * 0.075, hb.x + hw * 0.925] {
            let sy = hb.y + hh * 0.52;
            draw_circle(sx, sy, sr, Color::new(0.03, 0.03, 0.035, 1.0));
            draw_circle(sx, sy, sr * 0.82, Color::new(0.11, 0.11, 0.12, 1.0));
            draw_circle(sx - sr * 0.2, sy - sr * 0.25, sr * 0.4, Color::new(0.17, 0.17, 0.18, 1.0));
            let c = sr * 0.55;
            draw_line(sx - c, sy, sx + c, sy, (sr * 0.28).max(1.0), Color::new(0.02, 0.02, 0.02, 1.0));
            draw_line(sx, sy - c, sx, sy + c, (sr * 0.28).max(1.0), Color::new(0.02, 0.02, 0.02, 1.0));
        }
        // --- Poussoir en relief ---
        let btn = Rect::new(hb.x + hw * 0.16, hb.y + hh * 0.22, hw * 0.68, hh * 0.76);
        let down = hovered(btn) && is_mouse_button_down(MouseButton::Left);
        let cap = (hh * 0.07).max(2.0) * if down { 0.35 } else { 1.0 };
        let dy = if down { cap * 1.5 } else { 0.0 };
        // Épaisseur visible au-dessus de la face (dessus éclairé) et ombre portée
        draw_rectangle(btn.x + 2.0, btn.y + 3.0, btn.w, btn.h, Color::new(0.0, 0.0, 0.0, 0.45));
        draw_rectangle(btn.x, btn.y + dy, btn.w, cap, Color::new(0.21, 0.215, 0.225, 1.0));
        let face = Rect::new(btn.x, btn.y + cap + dy, btn.w, btn.h - cap);
        draw_rectangle(face.x, face.y, face.w, face.h, Color::new(0.135, 0.14, 0.145, 1.0));
        draw_rectangle_lines(face.x, face.y, face.w, face.h, 1.0, Color::new(0.06, 0.06, 0.065, 1.0));
        // Plaque de légende légèrement en retrait
        let lp = Rect::new(face.x + face.w * 0.03, face.y + face.h * 0.06, face.w * 0.94, face.h * 0.88);
        draw_rectangle(lp.x, lp.y, lp.w, lp.h, if lit { Color::new(lamp.r * 0.18, lamp.g * 0.18, lamp.b * 0.18, 1.0) } else { Color::new(0.115, 0.12, 0.125, 1.0) });
        draw_rectangle_lines(lp.x, lp.y, lp.w, lp.h, 1.0, Color::new(0.075, 0.075, 0.08, 1.0));
        // --- Légende gravée ---
        let big = (lp.h * 0.27).clamp(7.0, 30.0);
        let small = (lp.h * 0.16).clamp(6.0, 18.0);
        let engraved = Color::new(0.68, 0.69, 0.7, 1.0);
        let tc = if lit { lamp } else { engraved };
        let cx = lp.x + lp.w / 2.0;
        let y1 = lp.y + lp.h * 0.36;
        let y2 = lp.y + lp.h * 0.68;
        let y3 = lp.y + lp.h * 0.92;
        // Gravure : liseré sombre décalé (creux), puis le texte ; halo quand la lampe éclaire
        let lines: Vec<(&str, f32, f32)> = if small >= 7.0 { vec![("MASTER", y1, big), (l2, y2, big), ("PRESS TO RESET", y3, small)] } else { vec![("MASTER", lp.y + lp.h * 0.42, big * 1.15), (l2, lp.y + lp.h * 0.82, big * 1.15)] };
        for (t, y, sz) in lines {
            if lit {
                txt_c(t, cx + 0.8, y + 0.8, sz, Color::new(lamp.r, lamp.g, lamp.b, 0.35));
                txt_c(t, cx - 0.8, y - 0.8, sz, Color::new(lamp.r, lamp.g, lamp.b, 0.35));
            } else {
                txt_c(t, cx + 0.7, y + 0.7, sz, Color::new(0.03, 0.03, 0.03, 0.9));
            }
            txt_c(t, cx, y, sz, tc);
        }
        // Légende fine illisible en petit : omise
        let _ = small;
        clicked(btn) || clicked(hb)
    }
}
