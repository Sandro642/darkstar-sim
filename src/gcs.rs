//! Habillage de la station sol : barre de statut, onglets, panneau d'overrides à interrupteurs gardés.

use crate::aircraft::*;
use crate::mission::fmt_time;
use crate::ui::*;
use crate::ui::wrap_text;
use macroquad::prelude::*;

pub struct TopBar<'a> {
    pub sim_time: f64,
    pub mission_t: f64,
    pub warp: f64,
    pub warp_capped: bool,
    pub link: &'a str,
    pub latency_ms: f64,
    pub master_warn: bool,
    pub master_caut: bool,
    pub muted: bool,
    pub time: f64,
}

#[derive(Default)]
pub struct TopActions {
    pub ack: bool,
    pub warp: bool,
    pub mute: bool,
    pub help: bool,
}

fn chip(x: f32, y: f32, label: &str, value: &str, vc: Color) -> f32 {
    let lw = txt_w(label, 12.0);
    let vw = txt_w(value, 16.0);
    let w = lw + vw + 22.0;
    rrect(Rect::new(x, y, w, 22.0), 6.0, Color::new(0.09, 0.11, 0.14, 1.0));
    txt(label, x + 8.0, y + 15.0, 12.0, DIM);
    txt(value, x + lw + 14.0, y + 16.0, 16.0, vc);
    w
}

pub fn draw_topbar(r: Rect, t: &TopBar) -> TopActions {
    let mut act = TopActions::default();
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.05, 0.06, 0.075, 1.0));
    draw_line(r.x, r.y + r.h, r.x + r.w, r.y + r.h, 1.0, PANEL_BORDER);
    let y = r.y + (r.h - 22.0) / 2.0;
    draw_rectangle(r.x + 12.0, y + 3.0, 4.0, 16.0, ACCENT);
    txt("DARKSTAR", r.x + 22.0, y + 17.0, 19.0, WHITE);
    txt(&format!("GCS FTV-01  v{}", crate::updater::VERSION), r.x + 118.0, y + 16.0, 12.0, DIM);

    let mut x = r.x + 280.0;
    x += chip(x, y, "SIM", &fmt_time(t.sim_time), WHITE) + 8.0;
    x += chip(x, y, "ESSAI", &fmt_time(t.mission_t), WHITE) + 8.0;
    // Accélération du temps (clic = cran suivant)
    let wc = if t.warp > 1.0 { AMBER } else { GREEN_S };
    let wlabel = if t.warp_capped { format!("x{:.0} (LIMITÉ)", t.warp) } else { format!("x{:.0}", t.warp) };
    let ww = chip(x, y, "TEMPS", &wlabel, wc);
    if clicked(Rect::new(x, y, ww, 22.0)) {
        act.warp = true;
    }
    x += ww + 8.0;
    let lc = if t.latency_ms > 200.0 { AMBER } else { GREEN_S };
    chip(x, y, "LINK", &format!("{} {:.0}ms", t.link, t.latency_ms), lc);

    // Droite : alarmes maîtres, son, aide
    let blink = (t.time * 2.5).fract() < 0.6;
    let mut rx = r.x + r.w - 12.0;
    let help = Rect::new(rx - 54.0, y, 54.0, 22.0);
    rrect(help, 6.0, Color::new(0.09, 0.11, 0.14, 1.0));
    txt_c("AIDE", help.x + help.w / 2.0, y + 16.0, 13.0, DIM);
    act.help = clicked(help);
    rx -= 62.0;
    let snd = Rect::new(rx - 70.0, y, 70.0, 22.0);
    rrect(snd, 6.0, Color::new(0.09, 0.11, 0.14, 1.0));
    txt_c(if t.muted { "SON OFF" } else { "SON ON" }, snd.x + snd.w / 2.0, y + 16.0, 13.0, if t.muted { AMBER } else { DIM });
    act.mute = clicked(snd);
    rx -= 78.0;
    let caut = Rect::new(rx - 92.0, y, 92.0, 22.0);
    rrect(caut, 6.0, if t.master_caut { AMBER } else { Color::new(0.2, 0.15, 0.02, 1.0) });
    txt_c("CAUTION", caut.x + caut.w / 2.0, y + 16.0, 13.0, if t.master_caut { BLACK } else { DIM });
    rx -= 100.0;
    let warn = Rect::new(rx - 92.0, y, 92.0, 22.0);
    rrect(warn, 6.0, if t.master_warn && blink { RED } else { Color::new(0.22, 0.03, 0.03, 1.0) });
    txt_c("WARNING", warn.x + warn.w / 2.0, y + 16.0, 13.0, if t.master_warn { WHITE } else { DIM });
    act.ack = clicked(warn) || clicked(caut);
    act
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Moteurs,
    Alarmes,
    Fcs,
    Limites,
    Elec,
    Thermique,
    Essais,
    Domaine,
    Override,
    Console,
    Recon,
    Armement,
}

/// Pages, dans l'ordre : touches 4..9 et 0 pour les 7 premières, boutons de lunette pour les 6 premières.
pub const TABS: [(Tab, &str); 12] = [
    (Tab::Moteurs, "MOTEURS"),
    (Tab::Alarmes, "ALARMES"),
    (Tab::Fcs, "FCS"),
    (Tab::Limites, "LIMITES"),
    (Tab::Elec, "ELEC/ENV"),
    (Tab::Thermique, "THERMIQUE"),
    (Tab::Essais, "ESSAIS"),
    (Tab::Domaine, "DOMAINE"),
    (Tab::Override, "OVERRIDE"),
    (Tab::Console, "CONSOLE"),
    (Tab::Recon, "RECON"),
    (Tab::Armement, "ARMEMENT"),
];

/// Barre d'onglets sur deux rangées ; renvoie l'onglet cliqué. `alert` : onglets à signaler en ambre.
pub fn draw_tabs(r: Rect, cur: Tab, alert: &[Tab]) -> Option<Tab> {
    let per_row = TABS.len().div_ceil(2);
    let w = r.w / per_row as f32;
    let rh = r.h / 2.0;
    let mut out = None;
    for (i, (tab, label)) in TABS.iter().enumerate() {
        let tr = Rect::new(r.x + (i % per_row) as f32 * w + 2.0, r.y + (i / per_row) as f32 * rh + 1.0, w - 4.0, rh - 2.0);
        let active = *tab == cur;
        let bg = if active { Color::new(0.10, 0.16, 0.22, 1.0) } else if hovered(tr) { Color::new(0.08, 0.10, 0.13, 1.0) } else { Color::new(0.06, 0.075, 0.095, 1.0) };
        rrect(tr, 6.0, bg);
        if active {
            draw_rectangle(tr.x + 8.0, tr.y + tr.h - 3.0, tr.w - 16.0, 3.0, ACCENT);
        }
        let c = if alert.contains(tab) { AMBER } else if active { WHITE } else { DIM };
        let key = if i < 7 { format!("{} ", (i + 4) % 10) } else { String::new() };
        txt_c(&format!("{key}{label}"), tr.x + tr.w / 2.0, tr.y + tr.h / 2.0 + 5.0, 12.0, c);
        if clicked(tr) {
            out = Some(*tab);
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
pub enum Switch {
    ScramEnable,
    EngineOverride,
    FbwOverride,
    H2Dump,
    FuelDump,
}

/// Interrupteur à capot rouge (comme SCRAM ENABLE / EMER H2 DUMP sur le Darkstar) ; vrai si cliqué.
fn guarded_switch(r: Rect, label: &str, on: bool, warn: bool) -> bool {
    rrect(r, 8.0, Color::new(0.07, 0.075, 0.08, 1.0));
    rrect_lines(r, 8.0, 1.0, Color::new(0.25, 0.26, 0.28, 1.0));
    // Vis de fixation
    for (x, y) in [(r.x + 9.0, r.y + 9.0), (r.x + r.w - 9.0, r.y + 9.0), (r.x + 9.0, r.y + r.h - 9.0), (r.x + r.w - 9.0, r.y + r.h - 9.0)] {
        draw_circle(x, y, 3.0, Color::new(0.2, 0.21, 0.22, 1.0));
    }
    let lines: Vec<&str> = label.split(' ').collect();
    for (i, l) in lines.iter().enumerate() {
        txt(l, r.x + 14.0, r.y + 24.0 + i as f32 * 17.0, 15.0, Color::new(0.85, 0.9, 0.7, 1.0));
    }
    // Levier et capot
    let sx = r.x + r.w - 46.0;
    let cy = r.y + r.h / 2.0;
    draw_rectangle(sx - 4.0, cy - 22.0, 36.0, 44.0, Color::new(0.12, 0.12, 0.13, 1.0));
    let lever_y = if on { cy - 18.0 } else { cy + 4.0 };
    draw_rectangle(sx + 10.0, lever_y, 8.0, 14.0, Color::new(0.75, 0.77, 0.8, 1.0));
    let red = Color::new(0.80, 0.08, 0.08, 1.0);
    if on {
        // capot relevé
        draw_rectangle(sx - 6.0, cy - 40.0, 40.0, 12.0, red);
        draw_rectangle(sx - 6.0, cy - 30.0, 40.0, 3.0, Color::new(0.5, 0.03, 0.03, 1.0));
    } else {
        draw_rectangle(sx - 6.0, cy - 24.0, 40.0, 48.0, red);
        draw_rectangle(sx - 6.0, cy - 24.0, 40.0, 8.0, Color::new(0.95, 0.25, 0.25, 1.0));
    }
    let st_c = if on { if warn { AMBER } else { GREEN_S } } else { DIM };
    txt(if on { "ON" } else { "OFF" }, r.x + 14.0, r.y + r.h - 12.0, 14.0, st_c);
    clicked(r)
}

pub fn draw_overrides(ac: &Aircraft, r: Rect) -> Option<Switch> {
    panel(r, "OVERRIDE / URGENCES");
    let mut out = None;
    let gap = 8.0;
    let cols = 3.0;
    let w = (r.w - 20.0 - gap * (cols - 1.0)) / cols;
    let h = ((r.h - 120.0) / 2.0).clamp(70.0, 96.0);
    let scram_on = matches!(ac.eng.scram, ScramState::Armed | ScramState::Lit | ScramState::Unstart);
    let items = [
        (Switch::ScramEnable, "SCRAM ENABLE", scram_on, false),
        (Switch::EngineOverride, "ENGINE OVERRIDE", ac.ovr_engine, true),
        (Switch::FbwOverride, "FBW OVERRIDE", ac.ovr_fbw, true),
        (Switch::H2Dump, "EMER H2 DUMP", ac.h2_dump, true),
        (Switch::FuelDump, "EMER FUEL DUMP", ac.fuel_dump, true),
    ];
    for (i, (sw, label, on, warn)) in items.iter().enumerate() {
        let (cx, cy) = (i % 3, i / 3);
        let sr = Rect::new(r.x + 10.0 + cx as f32 * (w + gap), r.y + 30.0 + cy as f32 * (h + gap), w, h);
        if guarded_switch(sr, label, *on, *warn) {
            out = Some(*sw);
        }
    }
    // État moteur
    let x = r.x + 10.0 + 2.0 * (w + gap);
    let y = r.y + 30.0 + h + gap;
    let e = &ac.eng;
    txt("CHAMBRES", x + 4.0, y + 16.0, 13.0, DIM);
    let hc = if e.combustor_failed { RED } else if e.health < 0.3 { AMBER } else { GREEN_S };
    draw_rectangle_lines(x + 4.0, y + 22.0, w - 8.0, 12.0, 1.0, DIM);
    draw_rectangle(x + 5.0, y + 23.0, (w - 10.0) * e.health as f32, 10.0, hc);
    txt(&if e.combustor_failed { "DETRUITES".to_string() } else { format!("SANTE {:.0}%", e.health * 100.0) }, x + 4.0, y + 50.0, 13.0, hc);
    let lim = if ac.ovr_engine { "FADEC : INHIBE".to_string() } else if ac.fadec_limit < 0.995 { format!("FADEC : LIMITE {:.0}%", ac.fadec_limit * 100.0) } else { "FADEC : NOMINAL".to_string() };
    txt(&lim, x + 4.0, y + 68.0, 13.0, if ac.ovr_engine || ac.fadec_limit < 0.995 { AMBER } else { GREEN_S });
    let sat = ac.throttle > 1.6;
    let max_s = if ac.ovr_engine { "LIMITE MOTEUR" } else { "100%" };
    txt(&format!("MANETTE {:.0}% (MAX {max_s})", ac.throttle * 100.0), x + 4.0, y + 86.0, 13.0, if ac.throttle > 1.0 { AMBER } else { WHITE });
    txt(&format!("POUSSEE EFFECTIVE {:.0}%{}", ac.eng.th_eff * 100.0, if sat { " - SATUREE" } else { "" }), x + 4.0, y + 104.0, 13.0, if sat { RED } else { DIM });

    let ty = r.y + 30.0 + 2.0 * (h + gap) + 6.0;
    let notes = [
        "ENGINE OVERRIDE : FADEC inhibe, manette sans butee. La poussee sature a la limite physique du moteur (~137 %) ; au-dela le carburant en exces ne fait que chauffer et user les chambres (rupture a 0 %).",
        "FBW OVERRIDE : protections d'incidence (entree d'air scram) et de facteur de charge inhibees. Unstart et rupture possibles.",
    ];
    let mut yy = ty;
    for n in notes {
        for l in wrap_text(n, 12.0, r.w - 24.0) {
            if yy > r.y + r.h - 6.0 {
                break;
            }
            txt(&l, r.x + 12.0, yy, 12.0, DIM);
            yy += 15.0;
        }
        yy += 3.0;
    }
    out
}
