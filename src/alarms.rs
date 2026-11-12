//! Page ALARMES / PANNES : panneau d'annonciateurs (comme sur la console du Darkstar) et injection
//! de pannes pour l'essai.

use crate::aircraft::*;
use crate::systems::{self, Fail};
use crate::ui::*;
use macroquad::prelude::*;

pub enum AlarmAction {
    Toggle(Fail),
    RepairAll,
}

type Light = (&'static str, fn(&Aircraft, &[(Level, String)]) -> u8);

fn has(al: &[(Level, String)], prefix: &str) -> bool {
    al.iter().any(|(_, a)| a.starts_with(prefix))
}

/// 0 = éteint, 1 = ambre (CAUTION), 2 = rouge (WARNING).
const LIGHTS: [Light; 32] = [
    ("L GEN", |ac, _| (ac.fails.has(Fail::GenL)) as u8),
    ("R GEN", |ac, _| (ac.fails.has(Fail::GenR)) as u8),
    ("L FADEC", |ac, _| (ac.eng.fadec_fault[0]) as u8),
    ("R FADEC", |ac, _| (ac.eng.fadec_fault[1]) as u8),
    ("L ENG FAIL", |ac, _| if ac.eng.side[0] == TurbState::Failed { 2 } else { 0 }),
    ("R ENG FAIL", |ac, _| if ac.eng.side[1] == TurbState::Failed { 2 } else { 0 }),
    ("EGT HOT", |ac, _| if ac.eng.egt > 1000.0 { 2 } else if ac.eng.egt > 900.0 { 1 } else { 0 }),
    ("CIT HOT", |_, al| if has(al, "OVERTEMP CIT") { 2 } else if has(al, "TEMP CIT") { 1 } else { 0 }),
    ("RAM WALL", |ac, _| if ac.eng.ram_wall > systems::RAM_WALL_BURST - 100.0 { 2 } else if ac.eng.ram_wall > systems::RAM_WALL_CAUTION { 1 } else { 0 }),
    ("SCRAM UNSTART", |ac, _| if ac.eng.scram == ScramState::Unstart { 2 } else { 0 }),
    ("SCRAM WALL", |ac, _| if ac.eng.scram_wall > systems::SCRAM_WALL_BURST - 100.0 { 2 } else if ac.eng.scram_wall > systems::SCRAM_WALL_CAUTION { 1 } else { 0 }),
    ("COMBUSTOR", |ac, _| if ac.eng.combustor_failed { 2 } else if ac.eng.health < 0.3 { 1 } else { 0 }),
    ("FUEL LOW", |ac, _| (ac.fuel < FUEL_MAX * 0.1) as u8),
    ("FUEL PRESS", |ac, _| (ac.fails.has(Fail::FuelPump)) as u8),
    ("H2 LOW", |ac, _| (ac.h2 < H2_MAX * 0.1) as u8),
    ("H2 LEAK", |ac, _| if ac.fails.has(Fail::H2Leak) { 2 } else { 0 }),
    ("FUEL DUMP", |ac, _| ac.fuel_dump as u8),
    ("H2 DUMP", |ac, _| ac.h2_dump as u8),
    ("CG LIMIT", |_, al| has(al, "CG LIMIT") as u8),
    ("FUEL HOT", |_, al| if has(al, "OVERTEMP CARBURANT") { 2 } else { has(al, "TEMP CARBURANT") as u8 }),
    ("SKIN TEMP", |_, al| if has(al, "SKIN TEMP") { 2 } else { 0 }),
    ("OVERSPEED", |_, al| if has(al, "OVERSPEED") { 2 } else { 0 }),
    ("AIR DATA", |ac, _| if ac.fails.has(Fail::Adc) && !ac.air_data_ovrd { 2 } else { ac.adc_degraded() as u8 }),
    ("FCS DIRECT", |ac, _| if ac.fails.has(Fail::FcsDirect) { 2 } else { ac.ovr_fbw as u8 }),
    ("ENG OVRD", |ac, _| ac.ovr_engine as u8),
    ("FADEC LIMIT", |ac, _| (!ac.ovr_engine && ac.fadec_limit < 0.995) as u8),
    ("HYD", |ac, _| if ac.fails.has(Fail::Hyd) { 2 } else { 0 }),
    ("BIT FAIL", |_, al| has(al, "BIT FAIL") as u8),
    ("GEAR", |ac, al| if has(al, "TOO LOW") { 2 } else { (ac.fails.has(Fail::GearJam) || (ac.gear_pos > 0.01 && ac.gear_pos < 0.99)) as u8 }),
    ("INLET JAM", |ac, _| (ac.fails.has(Fail::InletJam)) as u8),
    ("ELEC", |ac, _| if ac.elec.batt < 0.2 && ac.elec.gen_kw + ac.elec.fc_kw < systems::ELEC_LOAD_KW { 2 } else { (ac.elec.gen_kw < systems::ELEC_LOAD_KW && !ac.elec.fc_on) as u8 }),
    ("SENS FAIL", |_, al| has(al, "SENS FAIL") as u8),
];

/// Voyants allumés (nom, 1 ambre / 2 rouge), pour la fenêtre d'alarmes de l'horizon.
pub fn lit(ac: &Aircraft, alerts: &[(Level, String)]) -> Vec<(&'static str, u8)> {
    LIGHTS.iter().map(|(n, f)| (*n, f(ac, alerts))).filter(|(_, l)| *l > 0).collect()
}

/// Nombre de voyants allumés (pour signaler l'onglet).
pub fn lit_count(ac: &Aircraft, alerts: &[(Level, String)]) -> usize {
    LIGHTS.iter().filter(|(_, f)| f(ac, alerts) > 0).count()
}

pub fn draw_alarms(ac: &Aircraft, alerts: &[(Level, String)], r: Rect, time: f64, force: Option<[u8; 32]>) -> Option<AlarmAction> {
    let mut out = None;
    panel(r, "ALARMES / PANNES");
    // Panneau d'annonciateurs (platine vissée, 4 colonnes x 8 lignes)
    let ph = (r.h * 0.56).min(330.0);
    let plate = Rect::new(r.x + 12.0, r.y + 30.0, r.w - 24.0, ph);
    rrect(plate, 14.0, Color::new(0.07, 0.075, 0.08, 1.0));
    rrect_lines(plate, 14.0, 1.5, Color::new(0.22, 0.23, 0.24, 1.0));
    for (sx, sy) in [(plate.x + 18.0, plate.y + 16.0), (plate.x + plate.w - 18.0, plate.y + 16.0), (plate.x + 18.0, plate.y + plate.h - 16.0), (plate.x + plate.w - 18.0, plate.y + plate.h - 16.0)] {
        draw_circle(sx, sy, 7.0, Color::new(0.13, 0.14, 0.14, 1.0));
        draw_line(sx - 4.0, sy - 2.0, sx + 4.0, sy + 2.0, 1.5, Color::new(0.28, 0.29, 0.3, 1.0));
    }
    let gx = plate.x + 34.0;
    let gy = plate.y + 30.0;
    let gw = plate.w - 68.0;
    let gh = plate.h - 60.0;
    let cw = gw / 4.0;
    let rh = gh / 8.0;
    let blink = (time * 3.0).fract() < 0.6;
    for (i, (label, f)) in LIGHTS.iter().enumerate() {
        let (c, row) = (i % 4, i / 4);
        let cell = Rect::new(gx + c as f32 * cw + 3.0, gy + row as f32 * rh + 3.0, cw - 6.0, rh - 6.0);
        let st = match force {
            Some(fl) => fl[i].max(f(ac, alerts)),
            None => f(ac, alerts),
        };
        let (bg, fg) = match st {
            2 if blink => (Color::new(0.55, 0.05, 0.04, 1.0), Color::new(1.0, 0.75, 0.7, 1.0)),
            2 => (Color::new(0.35, 0.03, 0.03, 1.0), Color::new(1.0, 0.55, 0.5, 1.0)),
            1 => (Color::new(0.45, 0.30, 0.02, 1.0), Color::new(1.0, 0.85, 0.35, 1.0)),
            _ => (Color::new(0.10, 0.105, 0.11, 1.0), Color::new(0.32, 0.34, 0.35, 1.0)),
        };
        if st > 0 {
            draw_rectangle(cell.x - 2.0, cell.y - 2.0, cell.w + 4.0, cell.h + 4.0, Color::new(bg.r, bg.g, bg.b, 0.35));
        }
        draw_rectangle(cell.x, cell.y, cell.w, cell.h, bg);
        draw_rectangle_lines(cell.x, cell.y, cell.w, cell.h, 1.0, Color::new(0.2, 0.21, 0.22, 1.0));
        txt_c(label, cell.x + cell.w / 2.0, cell.y + cell.h / 2.0 + 4.0, 11.0, fg);
    }

    // Injection de pannes
    let y0 = plate.y + plate.h + 14.0;
    txt("INJECTION DE PANNES (ESSAI)", r.x + 14.0, y0 + 10.0, 13.0, ACCENT);
    let rep = Rect::new(r.x + r.w - 130.0, y0 - 4.0, 116.0, 20.0);
    rrect(rep, 5.0, Color::new(0.08, 0.2, 0.12, 1.0));
    txt_c("TOUT REPARER", rep.x + rep.w / 2.0, rep.y + 14.0, 12.0, GREEN_S);
    if clicked(rep) {
        out = Some(AlarmAction::RepairAll);
    }
    let bw = (r.w - 24.0 - 2.0 * 6.0) / 3.0;
    let avail = r.y + r.h - (y0 + 24.0) - 8.0;
    let bh = (avail / 4.0 - 6.0).clamp(18.0, 34.0);
    for (i, (fail, code, label)) in systems::FAILS.iter().enumerate() {
        let (c, row) = (i % 3, i / 3);
        let b = Rect::new(r.x + 12.0 + c as f32 * (bw + 6.0), y0 + 24.0 + row as f32 * (bh + 6.0), bw, bh);
        let on = ac.fails.has(*fail);
        rrect(b, 5.0, if on { Color::new(0.4, 0.05, 0.05, 1.0) } else if hovered(b) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(b, 5.0, 1.0, if on { RED } else { PANEL_BORDER });
        txt(label, b.x + 8.0, b.y + bh * 0.5 + 4.0, 11.0, if on { WHITE } else { DIM });
        txt_r(if on { "ACTIVE" } else { code }, b.x + b.w - 6.0, b.y + bh * 0.5 + 4.0, 10.0, if on { RED } else { Color::new(0.4, 0.43, 0.46, 1.0) });
        if clicked(b) {
            out = Some(AlarmAction::Toggle(*fail));
        }
    }
    out
}
