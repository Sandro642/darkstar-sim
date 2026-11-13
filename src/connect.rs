//! Écran d'établissement de la liaison : choix du Darkstar dans la flotte, de l'aérodrome de
//! départ (saisie OACI, base mondiale) et de la piste, puis poignée de main de la liaison.

use crate::airports::AIRPORTS;
use crate::link::{FLEET, STEPS, State};
use crate::ui::*;
use crate::ui::wrap_text;
use macroquad::prelude::*;

#[derive(Clone)]
pub struct ConnectUi {
    pub sel: usize,
    /// Aérodrome de départ (indice dans AIRPORTS) et extrémité de piste (désignation)
    pub apt: usize,
    pub rwy: String,
    /// Saisie OACI en cours
    pub query: String,
}

pub enum ConnectAction {
    Select(usize),
    Airport(usize),
    Runway(String),
    Query(String),
    Connect,
}

/// Raccourcis : grands terrains d'essais et internationaux.
const QUICK: [&str; 10] = ["KPMD", "KEDW", "KXTA", "KLAX", "KSFO", "KJFK", "EGLL", "LFPG", "LSZH", "RJTT"];

impl ConnectUi {
    pub fn new() -> ConnectUi {
        let apt = AIRPORTS.iter().position(|a| a.icao == FLEET[0].1).unwrap_or(0);
        ConnectUi { sel: 0, apt, rwy: default_rwy(apt), query: String::new() }
    }
}

/// Piste par défaut : extrémité la plus longue, sens « haut » de la piste principale.
pub fn default_rwy(apt: usize) -> String {
    let a = &AIRPORTS[apt];
    let e = crate::procs::ends(a);
    let want = crate::geo::wrap360(a.hdg + 180.0);
    e.iter()
        .filter(|x| x.len >= 0.9 * e.iter().map(|y| y.len).fold(0.0, f64::max))
        .min_by(|x, y| crate::geo::wrap180(x.hdg - want).abs().total_cmp(&crate::geo::wrap180(y.hdg - want).abs()))
        .map(|x| x.ident.clone())
        .unwrap_or_default()
}

pub fn draw(ui: &ConnectUi, link: &crate::link::Link, last: &str, sw: f32, sh: f32, time: f64) -> Option<ConnectAction> {
    let mut out = None;
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.02, 0.03, 0.05, 0.94));
    let w = (sw * 0.62).clamp(560.0, 900.0);
    let h = (sh * 0.84).clamp(600.0, 720.0);
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    rrect(r, 12.0, Color::new(0.06, 0.07, 0.09, 1.0));
    rrect_lines(r, 12.0, 1.5, PANEL_BORDER);
    txt("STATION SOL GCS-1 — ETABLISSEMENT DE LA LIAISON", r.x + 24.0, r.y + 36.0, 20.0, ACCENT);
    txt("Choisir l'appareil, l'aerodrome de depart (taper le code OACI, Tab = 1re suggestion) et la piste, puis etablir la liaison.", r.x + 24.0, r.y + 58.0, 12.0, DIM);
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

    // --- Aérodrome et piste de départ ---
    let y = r.y + 236.0;
    txt("AERODROME DE DEPART", r.x + 24.0, y, 13.0, WHITE);
    let apt = &AIRPORTS[ui.apt];
    txt(&format!("{}  {}", apt.icao, apt.name.chars().take(40).collect::<String>()), r.x + 200.0, y, 13.0, CYAN_S);
    // Saisie OACI : taper le code (lettres), Retour arrière pour effacer
    let fb = Rect::new(r.x + 24.0, y + 10.0, 120.0, 28.0);
    rrect(fb, 6.0, Color::new(0.03, 0.04, 0.05, 1.0));
    rrect_lines(fb, 6.0, 1.5, ACCENT);
    let caret = if (time * 2.0).fract() < 0.5 { "_" } else { " " };
    txt(&format!("{}{caret}", ui.query), fb.x + 10.0, fb.y + 19.0, 15.0, WHITE);
    if !connecting {
        let mut q = ui.query.clone();
        while let Some(c) = get_char_pressed() {
            if c.is_ascii_alphanumeric() && q.len() < 4 {
                q.push(c.to_ascii_uppercase());
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            q.pop();
        }
        if q != ui.query {
            out = Some(ConnectAction::Query(q.clone()));
        }
    }
    // Suggestions (saisie) ou raccourcis
    let list: Vec<usize> = if ui.query.is_empty() {
        QUICK.iter().filter_map(|c| AIRPORTS.iter().position(|a| a.icao == *c)).collect()
    } else {
        let mut v: Vec<usize> = crate::airports::BY_SIZE.iter().copied().filter(|&i| AIRPORTS[i].icao.starts_with(ui.query.as_str())).take(10).collect();
        v.sort_by_key(|&i| AIRPORTS[i].icao);
        v
    };
    let bw = (r.w - 48.0 - 132.0 - 4.0 * 6.0) / 5.0;
    for (k, &i) in list.iter().enumerate() {
        let b = Rect::new(r.x + 156.0 + (k % 5) as f32 * (bw + 6.0), y + 10.0 + (k / 5) as f32 * 32.0, bw, 28.0);
        let on = i == ui.apt;
        rrect(b, 6.0, if on { Color::new(0.07, 0.16, 0.2, 1.0) } else if hovered(b) { Color::new(0.1, 0.12, 0.15, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(b, 6.0, if on { 2.0 } else { 1.0 }, if on { ACCENT } else { PANEL_BORDER });
        txt(AIRPORTS[i].icao, b.x + 8.0, b.y + 18.0, 13.0, if on { WHITE } else { CYAN_S });
        let short: String = AIRPORTS[i].name.chars().take(((bw - 54.0) / 6.0).max(0.0) as usize).collect();
        txt(&short, b.x + 48.0, b.y + 18.0, 9.0, DIM);
        if !connecting && clicked(b) {
            out = Some(ConnectAction::Airport(i));
        }
    }
    if !ui.query.is_empty() && list.is_empty() {
        txt("AUCUN AERODROME (base : pistes >= 6000 ft)", r.x + 156.0, y + 28.0, 11.0, AMBER);
    }
    if !connecting && is_key_pressed(KeyCode::Tab) {
        if let Some(&i) = list.first() {
            out = Some(ConnectAction::Airport(i));
        }
    }
    // Pistes : départ aligné au seuil
    let yr = y + 82.0;
    txt("PISTE (aligne au seuil, pret au decollage)", r.x + 24.0, yr, 12.0, WHITE);
    let ends = crate::procs::ends(apt);
    let n = ends.len().max(1);
    let rw = ((r.w - 48.0 - (n as f32 - 1.0) * 6.0) / n as f32).min(110.0);
    for (k, e) in ends.iter().enumerate() {
        let b = Rect::new(r.x + 24.0 + k as f32 * (rw + 6.0), yr + 8.0, rw, 26.0);
        let on = e.ident == ui.rwy;
        rrect(b, 6.0, if on { Color::new(0.07, 0.16, 0.2, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(b, 6.0, if on { 2.0 } else { 1.0 }, if on { ACCENT } else { PANEL_BORDER });
        txt(&e.ident, b.x + 8.0, b.y + 17.0, 13.0, if on { WHITE } else { CYAN_S });
        txt(&format!("{:.0}M", e.len), b.x + rw - 46.0, b.y + 17.0, 9.0, DIM);
        if !connecting && clicked(b) {
            out = Some(ConnectAction::Runway(e.ident.clone()));
        }
    }

    // --- Liaison ---
    let y = r.y + 380.0;
    txt(&format!("LIAISON DE DONNEES — DARKSTAR {}", FLEET[ui.sel].0), r.x + 24.0, y, 13.0, WHITE);
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
