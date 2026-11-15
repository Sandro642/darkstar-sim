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
    pub muted: bool,
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
    txt(&format!("GCS {}  v{}", crate::link::tail(), crate::updater::VERSION), r.x + 118.0, y + 16.0, 12.0, DIM);

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
    let lc = if t.link == "PERDUE" || t.link == "IRIDIUM" { AMBER } else { GREEN_S };
    chip(x, y, "LINK", t.link, lc);

    // Droite : alarmes maîtres, son, aide
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
    Systemes,
    Incendie,
    Mcdu,
    Config,
    Commandes,
}

/// Pages, dans l'ordre : touches 4..9 et 0 pour les 7 premières, boutons de lunette pour les 6 premières.
pub const TABS: [(Tab, &str); 17] = [
    (Tab::Moteurs, "MOTEURS"),
    (Tab::Alarmes, "ALARMES"),
    (Tab::Fcs, "FCS"),
    (Tab::Limites, "LIMITES"),
    (Tab::Elec, "ELEC/ENV"),
    (Tab::Thermique, "THERMIQUE"),
    (Tab::Essais, "ESSAIS"),
    (Tab::Domaine, "DOMAINE"),
    (Tab::Override, "OVERRIDE"),
    (Tab::Console, "PANNEAU DROIT"),
    (Tab::Recon, "RECON"),
    (Tab::Armement, "ARMEMENT"),
    (Tab::Systemes, "SYSTEMES"),
    (Tab::Incendie, "INCENDIE"),
    (Tab::Mcdu, "MCDU"),
    (Tab::Config, "CONFIG"),
    (Tab::Commandes, "COMMANDES"),
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

thread_local! {
    /// Capots relevés : instant d'ouverture de chaque interrupteur gardé (refermé après 6 s)
    static GUARD: std::cell::RefCell<[f64; 5]> = const { std::cell::RefCell::new([-100.0; 5]) };
}

/// Bandes de danger jaune / noir dans `r` (bord du capot).
fn hazard_stripes(r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.95, 0.78, 0.1, 1.0));
    let n = (r.w / 7.0).ceil() as i32;
    for k in 0..n {
        let x = r.x + k as f32 * 7.0;
        if x + 3.5 <= r.x + r.w {
            draw_triangle(vec2(x, r.y + r.h), vec2(x + 3.5, r.y), vec2((x + 7.0).min(r.x + r.w), r.y), Color::new(0.05, 0.05, 0.05, 1.0));
        }
    }
}

/// Interrupteur gardé réaliste : platine vissée, nom, explication, voyant d'état, levier métallique
/// sous capot rouge à bord rayé. Manœuvre en deux temps : clic sur le capot pour le relever, puis
/// clic sur le levier pour basculer. Renvoie vrai quand le levier bascule.
fn guarded_switch(r: Rect, idx: usize, label: &str, help: &str, on: bool, warn: bool, time: f64) -> bool {
    let open = on || time - GUARD.with(|g| g.borrow()[idx]) < 6.0;
    let mut fired = false;
    // Platine
    rrect(r, 8.0, Color::new(0.075, 0.08, 0.085, 1.0));
    rrect_lines(r, 8.0, 1.0, if on { if warn { AMBER } else { GREEN_S } } else { Color::new(0.24, 0.25, 0.27, 1.0) });
    for (x, y) in [(r.x + 8.0, r.y + 8.0), (r.x + r.w - 8.0, r.y + 8.0), (r.x + 8.0, r.y + r.h - 8.0), (r.x + r.w - 8.0, r.y + r.h - 8.0)] {
        draw_circle(x, y, 3.2, Color::new(0.03, 0.03, 0.035, 1.0));
        draw_circle(x, y, 2.4, Color::new(0.2, 0.21, 0.22, 1.0));
        draw_line(x - 1.6, y, x + 1.6, y, 1.0, Color::new(0.05, 0.05, 0.05, 1.0));
    }
    // Zone du levier à droite
    let sw_w = 60.0f32.min(r.w * 0.32);
    let bx = r.x + r.w - sw_w - 10.0;
    let base = Rect::new(bx, r.y + 18.0, sw_w, r.h - 36.0);
    let text_w = bx - r.x - 22.0;
    // Nom, explication, voyant
    let mut y = r.y + 24.0;
    for l in wrap_text(label, 14.0, text_w) {
        txt(&l, r.x + 14.0, y, 14.0, Color::new(0.88, 0.92, 0.78, 1.0));
        y += 16.0;
    }
    y += 2.0;
    for l in wrap_text(help, 10.0, text_w).into_iter().take(if r.h < 100.0 { 1 } else { 3 }) {
        txt(&l, r.x + 14.0, y, 10.0, Color::new(0.58, 0.62, 0.66, 1.0));
        y += 12.0;
    }
    let lamp = Rect::new(r.x + 14.0, r.y + r.h - 28.0, 66.0f32.min(text_w), 18.0);
    let lc = if warn { AMBER } else { GREEN_S };
    rrect(lamp, 3.0, if on { Color::new(lc.r * 0.3, lc.g * 0.3, lc.b * 0.3, 1.0) } else { Color::new(0.04, 0.04, 0.045, 1.0) });
    rrect_lines(lamp, 3.0, 1.0, Color::new(0.2, 0.2, 0.22, 1.0));
    txt_c(if on { "ON" } else { "OFF" }, lamp.x + lamp.w / 2.0, lamp.y + 13.0, 11.0, if on { lc } else { DIM });
    if !on && open {
        txt(&format!("CAPOT LEVE {:.0}s", (6.0 - (time - GUARD.with(|g| g.borrow()[idx]))).max(0.0)), lamp.x + lamp.w + 8.0, lamp.y + 13.0, 9.0, AMBER);
    }
    // Socle du levier, repères ON / OFF
    rrect(base, 4.0, Color::new(0.04, 0.04, 0.045, 1.0));
    let (cx, cy) = (base.x + base.w / 2.0, base.y + base.h / 2.0);
    txt_c("ON", cx, base.y + 11.0, 9.0, DIM);
    txt_c("OFF", cx, base.y + base.h - 4.0, 9.0, DIM);
    draw_circle(cx, cy, 9.0, Color::new(0.22, 0.23, 0.24, 1.0));
    draw_circle(cx, cy, 6.0, Color::new(0.12, 0.12, 0.13, 1.0));
    // Levier (batte métallique) vers le haut (ON) ou le bas (OFF)
    let tip = if on { cy - base.h * 0.32 } else { cy + base.h * 0.32 };
    draw_line(cx, cy, cx, tip, 5.0, Color::new(0.55, 0.57, 0.6, 1.0));
    draw_circle(cx, tip, 4.5, Color::new(0.85, 0.87, 0.9, 1.0));
    // Capot rouge : rabattu sur le levier, ou relevé (vu par la tranche au-dessus)
    let red = Color::new(0.78, 0.07, 0.06, 1.0);
    if open {
        let lid = Rect::new(base.x - 3.0, base.y - 14.0, base.w + 6.0, 10.0);
        draw_rectangle(lid.x, lid.y, lid.w, lid.h, red);
        hazard_stripes(Rect::new(lid.x, lid.y + lid.h - 3.0, lid.w, 3.0));
        if clicked(base) {
            fired = true;
            GUARD.with(|g| g.borrow_mut()[idx] = -100.0);
        }
    } else {
        let lid = Rect::new(base.x - 3.0, base.y - 3.0, base.w + 6.0, base.h + 6.0);
        rrect(lid, 4.0, Color::new(red.r, red.g, red.b, 0.86));
        rrect(Rect::new(lid.x + 3.0, lid.y + 3.0, lid.w - 6.0, lid.h * 0.18), 3.0, Color::new(1.0, 0.4, 0.35, 0.35));
        hazard_stripes(Rect::new(lid.x, lid.y + lid.h - 6.0, lid.w, 6.0));
        txt_c("LEVER", lid.x + lid.w / 2.0, lid.y + lid.h / 2.0 + 4.0, 9.0, Color::new(1.0, 0.85, 0.8, 1.0));
        if clicked(lid) {
            GUARD.with(|g| g.borrow_mut()[idx] = time);
        }
    }
    fired
}

/// Jauge horizontale avec repère de limite.
fn bar(x: f32, y: f32, w: f32, label: &str, val: &str, frac: f32, mark: Option<f32>, col: Color) {
    txt(label, x, y, 11.0, DIM);
    txt_r(val, x + w, y, 11.0, col);
    let b = Rect::new(x, y + 5.0, w, 7.0);
    rrect(b, 3.0, Color::new(0.06, 0.065, 0.07, 1.0));
    rrect(Rect::new(b.x, b.y, (b.w * frac.clamp(0.0, 1.0)).max(2.0), b.h), 3.0, col);
    if let Some(m) = mark {
        let mx = b.x + b.w * m.clamp(0.0, 1.0);
        draw_line(mx, b.y - 3.0, mx, b.y + b.h + 3.0, 1.5, WHITE);
    }
}

pub fn draw_overrides(ac: &Aircraft, mon: &crate::health::ChamberMon, r: Rect) -> Option<Switch> {
    panel(r, "OVERRIDE / URGENCES");
    let time = get_time();
    let mut out = None;
    let scram_on = matches!(ac.eng.scram, ScramState::Armed | ScramState::Lit | ScramState::Unstart);
    // Bandeau : protections inhibées
    let inhib: Vec<&str> = [(ac.ovr_engine, "FADEC"), (ac.ovr_fbw, "FBW"), (ac.h2_dump, "H2 DUMP"), (ac.fuel_dump, "FUEL DUMP")].iter().filter(|x| x.0).map(|x| x.1).collect();
    let mut y0 = r.y + 30.0;
    if !inhib.is_empty() {
        let b = Rect::new(r.x + 10.0, y0, r.w - 20.0, 24.0);
        let blink = (time * 2.0).fract() < 0.6;
        rrect(b, 5.0, Color::new(0.3, 0.2, 0.02, 1.0));
        rrect_lines(b, 5.0, 1.5, if blink { AMBER } else { Color::new(0.5, 0.35, 0.05, 1.0) });
        txt_c(&format!("ACTIF : {}  -  PROTECTIONS REDUITES", inhib.join(" / ")), b.x + b.w / 2.0, b.y + 16.0, 12.0, AMBER);
        y0 += 32.0;
    }
    let items = [
        (Switch::ScramEnable, "SCRAM ENABLE", "Autorise le scramjet (Mach 4,8+) ; OFF le coupe", scram_on, false),
        (Switch::EngineOverride, "ENGINE OVERRIDE", "FADEC inhibe : manette sans butee, surchauffe et usure des chambres", ac.ovr_engine, true),
        (Switch::FbwOverride, "FBW OVERRIDE", "Protections d'incidence et de facteur de charge inhibees", ac.ovr_fbw, true),
        (Switch::H2Dump, "EMER H2 DUMP", "Vidange d'urgence de l'hydrogene", ac.h2_dump, true),
        (Switch::FuelDump, "EMER FUEL DUMP", "Vidange d'urgence du JP-7", ac.fuel_dump, true),
    ];
    // Grille adaptée à la largeur ; bloc d'état à la place de la 6e case
    let gap = 8.0;
    let cols = (((r.w - 20.0) + gap) / 240.0).floor().clamp(1.0, 3.0) as usize;
    let w = (r.w - 20.0 - gap * (cols as f32 - 1.0)) / cols as f32;
    let rows = (items.len() + 1).div_ceil(cols);
    let h = ((r.y + r.h - y0 - 22.0 - gap * (rows as f32 - 1.0)) / rows as f32).clamp(78.0, 132.0);
    for (i, (sw, label, help, on, warn)) in items.iter().enumerate() {
        let sr = Rect::new(r.x + 10.0 + (i % cols) as f32 * (w + gap), y0 + (i / cols) as f32 * (h + gap), w, h);
        if guarded_switch(sr, i, label, help, *on, *warn, time) {
            out = Some(*sw);
        }
    }
    // ----- Bloc d'état -----
    let i = items.len();
    let st = Rect::new(r.x + 10.0 + (i % cols) as f32 * (w + gap), y0 + (i / cols) as f32 * (h + gap), w, h);
    let step = ((st.h - 44.0) / 4.0).clamp(17.0, 26.0);
    rrect(st, 8.0, Color::new(0.06, 0.065, 0.07, 1.0));
    rrect_lines(st, 8.0, 1.0, Color::new(0.2, 0.21, 0.23, 1.0));
    let (x, bw) = (st.x + 12.0, st.w - 24.0);
    let mut y = st.y + 20.0;
    txt("CHAMBRES", x, y, 11.0, DIM);
    txt_r(mon.state.label(), x + bw, y, 12.0, mon.state.color());
    y += 13.0;
    let cause: String = mon.cause.chars().take(40).collect();
    if !cause.is_empty() {
        txt(&cause, x, y, 9.0, mon.state.color());
    }
    y += 12.0;
    let fl = ac.fadec_limit as f32;
    let fc = if ac.ovr_engine || fl < 0.995 { AMBER } else { GREEN_S };
    bar(x, y, bw, "FADEC", &if ac.ovr_engine { "INHIBE".into() } else if fl < 0.995 { format!("LIMITE {:.0} %", fl * 100.0) } else { "NOMINAL".to_string() }, if ac.ovr_engine { 1.0 } else { fl }, None, fc);
    y += step;
    let thr = ac.throttle as f32;
    bar(x, y, bw, "MANETTE", &format!("{:.0} % (MAX {})", thr * 100.0, if ac.ovr_engine { "MOTEUR" } else { "100 %" }), thr / 1.4, Some(1.0 / 1.4), if thr > 1.0 { AMBER } else { CYAN_S });
    y += step;
    let eff = ac.eng.th_eff as f32;
    let sat = ac.throttle > 1.6;
    bar(x, y, bw, "POUSSEE EFFECTIVE", &format!("{:.0} %{}", eff * 100.0, if sat { " SATUREE" } else { "" }), eff / 1.4, Some(1.0 / 1.4), if sat { RED } else if eff > 1.0 { AMBER } else { GREEN_S });
    y += step;
    let hp = ac.eng.health as f32;
    bar(x, y, bw, "SANTE CHAMBRES", &format!("{:.0} %", hp * 100.0), hp, Some(0.3), if hp < 0.3 { RED } else if hp < 0.6 { AMBER } else { GREEN_S });
    // Mode d'emploi
    let hint = "Lever le capot (clic), puis basculer le levier (clic) dans les 6 s.";
    let hy = r.y + r.h - 8.0;
    if hy > st.y + st.h + 14.0 {
        txt(hint, r.x + 12.0, hy, 10.0, DIM);
    }
    out
}
