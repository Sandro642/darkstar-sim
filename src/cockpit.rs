//! Poste pilote (version embarquée) : panneaux physiques autour de l'écran.
//! - RADIO (haut) : COMM1 / COMM2 / NAV1 / NAV2 (active / attente, permutation, réglage MHz et
//!   kHz), transpondeur (code octal, STBY / ON / ALT, IDENT) ;
//! - MOTEURS (gauche) : trois boutons de page — 1 moteurs simple (nacelles), 2 données moteur,
//!   3 pourcentage manette 0-100 % ;
//! - MANETTE (bas gauche) : manette des gaz, levier de train, JETTISON protégé ;
//! - SCRAM (bas milieu) : bouton scramjet ;
//! - MOTEURS / APU / CLIM (bas droite) : interrupteurs moteurs, START / STOP, APU, climatisation.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;

/// Action d'un panneau du poste pilote.
pub enum Act {
    Cmd(String),
    Key(KeyCode),
    Throttle(f64),
    Menu(crate::menu::MenuAction),
    Env(crate::elec::EnvAction),
}

const FACE: Color = Color::new(0.07, 0.08, 0.09, 1.0);
const LBL: Color = Color::new(0.62, 0.66, 0.7, 1.0);

/// Bouton carré vert du poste (allumé = vert vif).
fn sq_button(r: Rect, lit: bool) -> bool {
    let c = if lit { Color::new(0.35, 1.0, 0.45, 1.0) } else { Color::new(0.18, 0.42, 0.22, 1.0) };
    rrect(r, 3.0, c);
    draw_rectangle(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h * 0.35, Color::new(1.0, 1.0, 1.0, if lit { 0.3 } else { 0.08 }));
    clicked(r)
}

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
        1 => crate::panels::draw_engines(ac, mon, scr, time),
        2 => throttle_page(ac, scr),
        _ => {
            txt("FWD", scr.x + 6.0, scr.y + 12.0, 9.0, LBL);
            txt_r("AFT", scr.x + scr.w - 6.0, scr.y + 12.0, 9.0, LBL);
            crate::panels::draw_nacelles(ac, Rect::new(scr.x + 4.0, scr.y + 16.0, scr.w - 8.0, scr.h - 20.0), time);
        }
    }
    // Trois boutons de page
    let n = 3.0;
    let bw = 34.0;
    let gap = (r.w - n * bw) / (n + 1.0);
    for k in 0..3u8 {
        let b = Rect::new(r.x + gap + k as f32 * (bw + gap), r.y + r.h - bh - 6.0, bw, bh);
        if sq_button(b, *page == k) {
            *page = k;
        }
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

// ---------- MOTEURS / APU / CLIM ----------

pub fn eng_apu_clim(ac: &Aircraft, r: Rect) -> Option<Act> {
    use crate::apu::ApuState;
    use crate::menu::MenuAction as M;
    let mut out = None;
    rrect(r, 8.0, FACE);
    let pad = 6.0;
    let cols = 4.0;
    let bw = (r.w - pad * (cols + 1.0)) / cols;
    let bh = ((r.h - 3.0 * 16.0 - 4.0 * pad) / 3.0).clamp(30.0, 44.0);
    let mut y = r.y + 4.0;
    let sec = |y: &mut f32, t: &str| {
        txt(t, r.x + pad, *y + 11.0, 10.0, LBL);
        *y += 15.0;
    };
    let bx = |k: usize| r.x + pad + k as f32 * (bw + pad);
    // Moteurs
    sec(&mut y, "MOTEURS");
    let e = &ac.eng;
    for i in 0..2 {
        let on = e.sw[i] == EngSw::On;
        if push(Rect::new(bx(i), y, bw, bh), ["ENG L", "ENG R"][i], if on { "ON" } else { "OFF" }, on.then_some(GREEN_S)) {
            out = Some(Act::Menu(M::EngSwitch(i, if on { EngSw::Off } else { EngSw::On })));
        }
    }
    let starting = e.side.contains(&TurbState::Starting);
    let running = e.side.contains(&TurbState::Running);
    if push(Rect::new(bx(2), y, bw, bh), "START", if starting { "EN COURS" } else { "" }, starting.then_some(CYAN_S)) {
        out = Some(Act::Menu(M::Start));
    }
    if push(Rect::new(bx(3), y, bw, bh), "STOP", if running { "RUN" } else { "OFF" }, running.then_some(GREEN_S)) {
        out = Some(Act::Menu(M::Stop));
    }
    y += bh + pad;
    // APU
    sec(&mut y, "APU");
    let a = &ac.apu;
    let avail = a.state == ApuState::Running && a.n > 0.95;
    if push(Rect::new(bx(0), y, bw, bh), "MASTER", if a.master { "ON" } else { "OFF" }, a.master.then_some(GREEN_S)) {
        out = Some(Act::Menu(M::ApuMaster(!a.master)));
    }
    let st = match a.state {
        ApuState::Starting => format!("{:.0} %", a.n * 100.0),
        ApuState::Running if avail => "AVAIL".into(),
        ApuState::Stopping => "COOL".into(),
        _ => String::new(),
    };
    if push(Rect::new(bx(1), y, bw, bh), "START", &st, if avail { Some(GREEN_S) } else if a.state == ApuState::Starting { Some(CYAN_S) } else { None }) {
        out = Some(Act::Menu(if matches!(a.state, ApuState::Starting | ApuState::Running) { M::ApuStop } else { M::ApuStart }));
    }
    if push(Rect::new(bx(2), y, bw, bh), "GEN", if a.gen_online() { "ON LINE" } else if a.generator { "ON" } else { "OFF" }, if a.gen_online() { Some(GREEN_S) } else { None }) {
        out = Some(Act::Menu(M::ApuGen(!a.generator)));
    }
    if push(Rect::new(bx(3), y, bw, bh), "BLEED", if a.bleed_air() { "AIR" } else if a.bleed { "ON" } else { "OFF" }, if a.bleed_air() { Some(GREEN_S) } else { None }) {
        out = Some(Act::Menu(M::ApuBleed(!a.bleed)));
    }
    y += bh + pad;
    // Climatisation (baie équipements / poste)
    sec(&mut y, "CLIMATISATION");
    let el = &ac.elec;
    if push(Rect::new(bx(0), y, bw, bh), "AUTO TEMP", if el.auto_temp { "ON" } else { "OFF" }, el.auto_temp.then_some(GREEN_S)) {
        out = Some(Act::Env(crate::elec::EnvAction::AutoToggle));
    }
    if push(Rect::new(bx(1), y, bw, bh), "TEMP -", "FROID", None) {
        out = Some(Act::Env(crate::elec::EnvAction::TempKnob((el.temp_knob - 0.1).max(0.0))));
    }
    if push(Rect::new(bx(2), y, bw, bh), "TEMP +", "CHAUD", None) {
        out = Some(Act::Env(crate::elec::EnvAction::TempKnob((el.temp_knob + 0.1).min(1.0))));
    }
    let tc = if el.ebay_temp > 45.0 { AMBER } else { GREEN_S };
    let tb = Rect::new(bx(3), y, bw, bh);
    rrect(tb, 4.0, Color::new(0.02, 0.03, 0.03, 1.0));
    txt_c("E BAY", tb.x + tb.w / 2.0, tb.y + 14.0, 9.0, LBL);
    txt_c(&format!("{:.0} C", el.ebay_temp), tb.x + tb.w / 2.0, tb.y + tb.h - 8.0, 12.0, tc);
    out
}
