//! Écran d'établissement de la liaison : choix du Darkstar dans la flotte, du point de départ
//! (point d'attente ou piste), puis poignée de main de la liaison de données.

use crate::airports::AIRPORTS;
use crate::link::{FLEET, STEPS, State};
use crate::ui::*;
use crate::ui::wrap_text;
use macroquad::prelude::*;

pub struct ConnectUi {
    pub sel: usize,
    /// Départ au point d'attente (sinon aligné sur la piste).
    pub hold: bool,
}

pub enum ConnectAction {
    Select(usize),
    Hold(bool),
    Connect,
}

impl ConnectUi {
    pub fn new() -> ConnectUi {
        ConnectUi { sel: 0, hold: true }
    }
}

pub fn draw(ui: &ConnectUi, link: &crate::link::Link, last: &str, sw: f32, sh: f32, time: f64) -> Option<ConnectAction> {
    let mut out = None;
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.02, 0.03, 0.05, 0.94));
    let w = (sw * 0.62).clamp(560.0, 900.0);
    let h = (sh * 0.78).clamp(460.0, 680.0);
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    rrect(r, 12.0, Color::new(0.06, 0.07, 0.09, 1.0));
    rrect_lines(r, 12.0, 1.5, PANEL_BORDER);
    txt("STATION SOL GCS-1 — ETABLISSEMENT DE LA LIAISON", r.x + 24.0, r.y + 36.0, 20.0, ACCENT);
    txt("Choisir l'appareil a controler, le point de depart, puis etablir la liaison de donnees.", r.x + 24.0, r.y + 58.0, 12.0, DIM);
    let connecting = matches!(link.state, State::Connecting { .. });

    // --- Flotte ---
    txt("FLOTTE DARKSTAR", r.x + 24.0, r.y + 92.0, 13.0, WHITE);
    let cw = (r.w - 48.0 - 3.0 * 12.0) / 4.0;
    for (i, (tail, apt, busy)) in FLEET.iter().enumerate() {
        let c = Rect::new(r.x + 24.0 + i as f32 * (cw + 12.0), r.y + 102.0, cw, 112.0);
        let sel = ui.sel == i;
        let avail = busy.is_none();
        rrect(c, 8.0, if sel { Color::new(0.07, 0.16, 0.2, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(c, 8.0, if sel { 2.0 } else { 1.0 }, if sel { ACCENT } else if avail { PANEL_BORDER } else { Color::new(0.3, 0.2, 0.1, 1.0) });
        // Silhouette en plan
        let sr = Rect::new(c.x + 8.0, c.y + 8.0, 48.0, 64.0);
        crate::syspage::silhouette(sr, if avail { GREEN_S } else { DIM }, Color::new(0.0, 0.0, 0.0, 0.0));
        txt(tail, c.x + 62.0, c.y + 26.0, 18.0, if avail { WHITE } else { DIM });
        let a = AIRPORTS.iter().find(|a| a.icao == *apt).map(|a| a.name).unwrap_or("");
        let short: String = a.split(" (").next().unwrap_or(a).chars().take(22).collect();
        txt(apt, c.x + 62.0, c.y + 44.0, 12.0, CYAN_S);
        txt(&short, c.x + 62.0, c.y + 58.0, 10.0, DIM);
        match busy {
            None => txt("DISPONIBLE", c.x + 10.0, c.y + c.h - 12.0, 12.0, GREEN_S),
            Some(why) => {
                for (k, l) in wrap_text(why, 10.0, c.w - 20.0).iter().take(2).enumerate() {
                    txt(l, c.x + 10.0, c.y + c.h - 24.0 + k as f32 * 12.0, 10.0, AMBER);
                }
            }
        }
        if avail && !connecting && clicked(c) {
            out = Some(ConnectAction::Select(i));
        }
    }

    // --- Point de départ ---
    let y = r.y + 244.0;
    txt("POINT DE DEPART", r.x + 24.0, y, 13.0, WHITE);
    let apt = AIRPORTS.iter().find(|a| a.icao == FLEET[ui.sel].1).expect("terrain");
    let num = crate::airports::Airport::runway_number(apt.hdg + 180.0);
    for (k, (hold, label, sub)) in [(true, "POINT D'ATTENTE", format!("Bretelle, piste {num:02} — instruction ALIGNEMENT pour entrer sur la piste")), (false, "PISTE", format!("Aligne au seuil de la piste {num:02}, pret au decollage"))].iter().enumerate() {
        let b = Rect::new(r.x + 24.0 + k as f32 * ((r.w - 60.0) / 2.0 + 12.0), y + 10.0, (r.w - 60.0) / 2.0, 54.0);
        let on = ui.hold == *hold;
        rrect(b, 8.0, if on { Color::new(0.07, 0.16, 0.2, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(b, 8.0, if on { 2.0 } else { 1.0 }, if on { ACCENT } else { PANEL_BORDER });
        txt(label, b.x + 14.0, b.y + 22.0, 15.0, if on { WHITE } else { DIM });
        for (j, l) in wrap_text(sub, 10.0, b.w - 28.0).iter().take(2).enumerate() {
            txt(l, b.x + 14.0, b.y + 38.0 + j as f32 * 12.0, 10.0, DIM);
        }
        if !connecting && clicked(b) {
            out = Some(ConnectAction::Hold(*hold));
        }
    }

    // --- Liaison ---
    let y = r.y + 350.0;
    txt(&format!("LIAISON DE DONNEES — {} VIA LOS (antenne de la base)", FLEET[ui.sel].0), r.x + 24.0, y, 13.0, WHITE);
    let (step, prog) = match link.state {
        State::Connecting { t, dur, .. } => (link.step, (t / dur) as f32),
        State::Connected => (STEPS.len(), 1.0),
        _ => (0, 0.0),
    };
    for (k, s) in STEPS.iter().enumerate() {
        let yy = y + 22.0 + k as f32 * 20.0;
        let (mark, c) = if connecting && k < step || (!connecting && prog >= 1.0) {
            ("OK", GREEN_S)
        } else if connecting && k == step {
            (if (time * 4.0).fract() < 0.5 { ".." } else { "  " }, CYAN_S)
        } else {
            ("--", DIM)
        };
        txt(mark, r.x + 30.0, yy, 12.0, c);
        txt(s, r.x + 64.0, yy, 12.0, if connecting && k <= step { WHITE } else { DIM });
    }
    let pb = Rect::new(r.x + 24.0, y + 132.0, r.w - 48.0, 6.0);
    draw_rectangle(pb.x, pb.y, pb.w, pb.h, Color::new(0.12, 0.13, 0.15, 1.0));
    draw_rectangle(pb.x, pb.y, pb.w * prog.clamp(0.0, 1.0), pb.h, ACCENT);
    // Dernier message de liaison (échec éventuel)
    if !last.is_empty() {
        txt(last, r.x + 24.0, y + 156.0, 11.0, if last.starts_with("ECHEC") { AMBER } else { DIM });
    }
    let b = Rect::new(r.x + r.w - 244.0, r.y + r.h - 54.0, 220.0, 36.0);
    let lbl = if connecting { "CONNEXION EN COURS..." } else { "ETABLIR LA LIAISON" };
    rrect(b, 8.0, if connecting { Color::new(0.1, 0.12, 0.14, 1.0) } else if hovered(b) { Color::new(0.1, 0.35, 0.4, 1.0) } else { Color::new(0.07, 0.26, 0.3, 1.0) });
    rrect_lines(b, 8.0, 1.5, ACCENT);
    txt_c(lbl, b.x + b.w / 2.0, b.y + 23.0, 14.0, WHITE);
    if !connecting && (clicked(b) || is_key_pressed(KeyCode::Enter)) {
        out = Some(ConnectAction::Connect);
    }
    out
}
