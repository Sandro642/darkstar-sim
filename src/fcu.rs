//! Page COMMANDES (version normale : pas de console dans l'avion réel) — toutes les fonctions en
//! boutons lumineux rangés par catégorie : AUTOPILOTE (FCU, avec sélecteurs CAP / ALT / VITESSE /
//! MACH), PROPULSION, CONFIGURATION, CARBURANT, NAVIGATION / MISSION, URGENCE, SIMULATION.
//! Un bouton déclenche exactement l'action du raccourci clavier ou de l'instruction associée.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;

/// Action demandée : touche virtuelle (même effet que le raccourci) ou instruction.
pub enum Act {
    Key(KeyCode),
    Cmd(String),
}

/// État utile à l'affichage (hors avion).
pub struct Ctx<'a> {
    pub vnav: bool,
    pub dest: &'a str,
    pub warp: f64,
    pub muted: bool,
    pub escort: bool,
    pub emergency: bool,
}

const BG: Color = Color::new(0.05, 0.06, 0.075, 1.0);

/// Bouton lumineux : renvoie vrai si cliqué. `lit` : voyant allumé (couleur).
fn button(r: Rect, label: &str, sub: &str, lit: Option<Color>) -> bool {
    let hov = hovered(r);
    rrect(r, 4.0, if hov { Color::new(0.13, 0.15, 0.18, 1.0) } else { Color::new(0.09, 0.10, 0.12, 1.0) });
    rrect_lines(r, 4.0, 1.0, Color::new(0.26, 0.29, 0.33, 1.0));
    // Voyant en haut du bouton
    let c = lit.unwrap_or(Color::new(0.22, 0.24, 0.27, 1.0));
    draw_rectangle(r.x + 6.0, r.y + 4.0, r.w - 12.0, 3.0, c);
    txt_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + if sub.is_empty() { 4.0 } else { 0.0 }, 11.0, if lit.is_some() { WHITE } else { Color::new(0.78, 0.81, 0.85, 1.0) });
    if !sub.is_empty() {
        txt_c(sub, r.x + r.w / 2.0, r.y + r.h / 2.0 + 13.0, 9.0, lit.unwrap_or(DIM));
    }
    clicked(r)
}

/// Cadre de catégorie.
fn group(r: Rect, title: &str) {
    rrect(r, 6.0, BG);
    rrect_lines(r, 6.0, 1.0, Color::new(0.2, 0.23, 0.27, 1.0));
    draw_rectangle(r.x + 8.0, r.y + 7.0, 3.0, 12.0, ACCENT);
    txt(title, r.x + 16.0, r.y + 17.0, 11.0, Color::new(0.8, 0.84, 0.88, 1.0));
}

/// Rangée de boutons dans une catégorie : renvoie l'indice cliqué.
fn row(x: f32, y: f32, w: f32, items: &[(&str, &str, Option<Color>)]) -> Option<usize> {
    let n = items.len().max(1) as f32;
    let bw = (w - (n - 1.0) * 5.0) / n;
    let mut out = None;
    for (k, (l, s, lit)) in items.iter().enumerate() {
        if button(Rect::new(x + k as f32 * (bw + 5.0), y, bw, 34.0), l, s, *lit) {
            out = Some(k);
        }
    }
    out
}

/// Sélecteur : libellé, valeur, boutons −−, −, +, ++ ; renvoie le pas choisi (index 0..3).
fn selector(x: f32, y: f32, w: f32, label: &str, value: &str, steps: [&str; 4]) -> Option<usize> {
    txt(label, x, y + 21.0, 11.0, Color::new(0.62, 0.66, 0.7, 1.0));
    let vw = 86.0;
    let vx = x + 64.0;
    draw_rectangle(vx, y + 5.0, vw, 24.0, BLACK);
    draw_rectangle_lines(vx, y + 5.0, vw, 24.0, 1.0, Color::new(0.3, 0.33, 0.37, 1.0));
    txt_c(value, vx + vw / 2.0, y + 22.0, 14.0, Color::new(0.35, 0.9, 1.0, 1.0));
    let bx = vx + vw + 6.0;
    let bw = ((x + w) - bx - 3.0 * 4.0) / 4.0;
    let mut out = None;
    for (k, s) in steps.iter().enumerate() {
        let b = Rect::new(bx + k as f32 * (bw + 4.0), y + 4.0, bw, 26.0);
        rrect(b, 4.0, if hovered(b) { Color::new(0.15, 0.17, 0.2, 1.0) } else { Color::new(0.1, 0.11, 0.13, 1.0) });
        rrect_lines(b, 4.0, 1.0, Color::new(0.26, 0.29, 0.33, 1.0));
        txt_c(s, b.x + b.w / 2.0, b.y + 17.0, 11.0, WHITE);
        if clicked(b) {
            out = Some(k);
        }
    }
    out
}

pub fn draw(ac: &Aircraft, cx: &Ctx, r: Rect) -> Option<Act> {
    panel(r, "COMMANDES — AUTOPILOTE / PROPULSION / CONFIGURATION / MISSION");
    let mut out: Option<Act> = None;
    let g = GREEN_S;
    let on = |b: bool| b.then_some(g);
    let amber = |b: bool| b.then_some(AMBER);
    let x0 = r.x + 10.0;
    let w = r.w - 20.0;
    let col_w = (w - 10.0) / 2.0;
    let x1 = x0 + col_w + 10.0;
    let mut y = r.y + 30.0;
    let ap = &ac.ap;

    // ----- AUTOPILOTE (FCU) : pleine largeur -----
    let gh = 190.0;
    group(Rect::new(x0, y, w, gh), "AUTOPILOTE (FCU)");
    let iy = y + 24.0;
    let lat_nav = ap.on && ap.lat == LatMode::Nav;
    let loc = ap.on && (ap.loc_armed || ap.lat == LatMode::Loc);
    if let Some(k) = row(x0 + 8.0, iy, w - 16.0, &[
        ("AP", if ap.on { "ENGAGE" } else { "OFF" }, on(ap.on)),
        ("A/THR", if ap.at_on { "ACTIVE" } else { "OFF" }, on(ap.at_on)),
        ("LNAV", if lat_nav { "NAV" } else { "" }, on(lat_nav)),
        ("VNAV", if cx.vnav { "ON" } else { "OFF" }, on(cx.vnav)),
        ("ILS / LOC", if loc { "ARME" } else { "" }, on(loc)),
        ("AUTOLAND", if ac.autoland.is_some() { "ACTIF" } else { cx.dest }, on(ac.autoland.is_some())),
    ]) {
        out = Some(match k {
            0 => Act::Key(KeyCode::P),
            1 => Act::Key(KeyCode::O),
            2 => Act::Key(KeyCode::N),
            3 => Act::Cmd(if cx.vnav { "VNAV OFF".into() } else { "VNAV ON".into() }),
            4 => Act::Key(KeyCode::L),
            _ => Act::Cmd(format!("ATTERRIR {}", cx.dest)),
        });
    }
    if let Some(k) = row(x0 + 8.0, iy + 40.0, w - 16.0, &[
        ("DECOLLAGE", "AUTO", on(ac.autotakeoff.is_some())),
        ("DESCENTE", "q 50 kPa", on(ap.on && ap.vert == VertMode::QHold)),
        ("GO AROUND", "REMISE", None),
        ("MANUEL", "REPRISE", None),
    ]) {
        out = Some(match k {
            0 => Act::Cmd("DECOLLAGE".into()),
            1 => Act::Cmd("DESCENTE".into()),
            2 => Act::Cmd("GO AROUND".into()),
            _ => Act::Key(KeyCode::X),
        });
    }
    // Sélecteurs
    let sy = iy + 82.0;
    let sw = (w - 16.0 - 10.0) / 2.0;
    let hdg = ap.hdg_tgt.round();
    if let Some(k) = selector(x0 + 8.0, sy, sw, "CAP", &format!("{:03.0}", if hdg <= 0.0 { hdg + 360.0 } else { hdg }), ["-10", "-1", "+1", "+10"]) {
        let d = [-10.0, -1.0, 1.0, 10.0][k];
        out = Some(Act::Cmd(format!("CAP {:.0}", crate::geo::wrap360(hdg + d))));
    }
    let alt = ap.alt_tgt_ft;
    if let Some(k) = selector(x0 + 18.0 + sw, sy, sw, "ALT", &format!("{alt:.0}"), ["-1K", "-100", "+100", "+1K"]) {
        let d = [-1000.0, -100.0, 100.0, 1000.0][k];
        out = Some(Act::Cmd(format!("ALT {:.0}", (alt + d).max(0.0))));
    }
    let keas = ap.spd_keas.unwrap_or(ac.d.keas).round();
    if let Some(k) = selector(x0 + 8.0, sy + 36.0, sw, "VITESSE", &format!("{keas:.0} KT"), ["-10", "-1", "+1", "+10"]) {
        let d = [-10.0, -1.0, 1.0, 10.0][k];
        out = Some(Act::Cmd(format!("KEAS {:.0}", (keas + d).max(120.0))));
    }
    let mach = ap.mach_tgt;
    if let Some(k) = selector(x0 + 18.0 + sw, sy + 36.0, sw, "MACH", &format!("{mach:.2}"), ["-1", "-0.1", "+0.1", "+1"]) {
        let d = [-1.0, -0.1, 0.1, 1.0][k];
        out = Some(Act::Cmd(format!("MACH {:.2}", (mach + d).clamp(0.3, 11.0))));
    }
    y += gh + 8.0;

    // ----- PROPULSION | CONFIGURATION -----
    let e = &ac.eng;
    let gh2 = 112.0;
    group(Rect::new(x0, y, col_w, gh2), "PROPULSION");
    let turb = matches!(e.turb, TurbState::Running | TurbState::Starting);
    if let Some(k) = row(x0 + 8.0, y + 24.0, col_w - 16.0, &[
        ("TURBINES", if e.turb == TurbState::Starting { "START" } else if turb { "ON" } else { "OFF" }, on(turb)),
        ("ENTREES", if e.inlet_open { "OUVERTES" } else { "FERMEES" }, if e.inlet_open { on(true) } else { amber(true) }),
        ("STATO", match e.ram { RamState::Lit => "LIT", RamState::Armed => "ARM", _ => "OFF" }, if e.ram == RamState::Lit { on(true) } else { (e.ram == RamState::Armed).then_some(CYAN_S) }),
        ("SCRAM", match e.scram { ScramState::Lit => "LIT", ScramState::Armed => "ARM", ScramState::Unstart => "UNSTART", _ => "OFF" }, if e.scram == ScramState::Lit { on(true) } else { (e.scram != ScramState::Off).then_some(CYAN_S) }),
    ]) {
        out = Some(Act::Key([KeyCode::T, KeyCode::I, KeyCode::Y, KeyCode::U][k]));
    }
    if let Some(k) = row(x0 + 8.0, y + 64.0, col_w - 16.0, &[("MANETTE -", "-10 %", None), ("", &format!("{:.0} %", ac.throttle * 100.0), None), ("MANETTE +", "+10 %", None)]) {
        let t = (ac.throttle * 100.0).round();
        match k {
            0 => out = Some(Act::Cmd(format!("THR {:.0}", (t - 10.0).max(0.0)))),
            2 => out = Some(Act::Cmd(format!("THR {:.0}", (t + 10.0).min(100.0)))),
            _ => {}
        }
    }
    group(Rect::new(x1, y, col_w, gh2), "CONFIGURATION");
    if let Some(k) = row(x1 + 8.0, y + 24.0, col_w - 16.0, &[
        ("TRAIN", if ac.gear_down { "SORTI" } else { "RENTRE" }, if ac.gear_down { on(true) } else { None }),
        ("AEROFREINS", if ac.speedbrake { "SORTIS" } else { "RENTRES" }, on(ac.speedbrake)),
        ("FREINS", if ac.brakes { "SERRES" } else { "LIBRES" }, on(ac.brakes)),
    ]) {
        out = Some(Act::Key([KeyCode::G, KeyCode::S, KeyCode::B][k]));
    }
    if let Some(k) = row(x1 + 8.0, y + 64.0, col_w - 16.0, &[("VIDANGE JP-7", if ac.fuel_dump { "EN COURS" } else { "" }, amber(ac.fuel_dump)), ("VIDANGE H2", if ac.h2_dump { "EN COURS" } else { "" }, amber(ac.h2_dump))]) {
        out = Some(Act::Cmd(match k {
            0 => (if ac.fuel_dump { "FUEL DUMP OFF" } else { "FUEL DUMP" }).into(),
            _ => (if ac.h2_dump { "H2 DUMP OFF" } else { "H2 DUMP" }).into(),
        }));
    }
    y += gh2 + 8.0;

    // ----- NAVIGATION / MISSION | URGENCE -----
    let gh3 = 72.0;
    group(Rect::new(x0, y, col_w, gh3), "NAVIGATION / MISSION");
    if let Some(k) = row(x0 + 8.0, y + 24.0, col_w - 16.0, &[
        ("DIRECT", cx.dest, None),
        ("ESCORTE", if cx.escort { "EN PLACE" } else { "VIPER" }, on(cx.escort)),
        ("RAVITAILL.", "TANKER", None),
        ("CARTE", "CENTRER", None),
    ]) {
        out = Some(match k {
            0 => Act::Cmd(format!("DIRECT {}", cx.dest)),
            1 => Act::Cmd(if cx.escort { "ESCORTE OFF".into() } else { "ESCORTE".into() }),
            2 => Act::Cmd("TANKER".into()),
            _ => Act::Key(KeyCode::C),
        });
    }
    group(Rect::new(x1, y, col_w, gh3), "URGENCE");
    if let Some(k) = row(x1 + 8.0, y + 24.0, col_w - 16.0, &[
        ("URGENCE", if cx.emergency { "ACTIVE" } else { "DEROUTEMENT" }, cx.emergency.then_some(RED)),
        ("OVERRIDE", "EMERGENCY", None),
        ("ANNULER", "URGENCE", None),
    ]) {
        out = Some(Act::Cmd(match k {
            0 => "URGENCE".into(),
            1 => "EMERGENCY OVERRIDE".into(),
            _ => "URGENCE OFF".into(),
        }));
    }
    y += gh3 + 8.0;

    // ----- SIMULATION -----
    if y + 72.0 <= r.y + r.h {
        group(Rect::new(x0, y, w, 72.0), "SIMULATION");
        if let Some(k) = row(x0 + 8.0, y + 24.0, w - 16.0, &[("TEMPS", &format!("x{:.0}", cx.warp), on(cx.warp > 1.0)), ("SON", if cx.muted { "COUPE" } else { "ON" }, if cx.muted { amber(true) } else { on(true) })]) {
            out = Some(Act::Key([KeyCode::E, KeyCode::J][k]));
        }
    }
    out
}
