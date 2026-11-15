//! Page COMMANDES : toutes les fonctions en boutons, rangées en sous-menus (PILOTE AUTO, MOTEURS,
//! CONFIGURATION, NAVIGATION, URGENCE / LIAISON, SIMULATION). Chaque bouton est une carte : nom de
//! la fonction, état en couleur et une ligne d'explication, pour comprendre sans manuel.
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
    /// Hauteur de décision (ft au-dessus du seuil) et terrain le plus proche (DISPATCH)
    pub da_ft: f64,
    pub origin: &'a str,
    /// Liaison complète établie (les relais ne servent qu'en cas de perte)
    pub link_ok: bool,
}

thread_local! {
    /// Sous-menu affiché
    static TAB: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Éjection armée (premier appui) : instant de l'armement
    static EJECT_ARM: std::cell::Cell<f64> = const { std::cell::Cell::new(-100.0) };
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

const TABS: [(&str, &str); 6] = [
    ("PILOTE AUTO", "consignes et modes"),
    ("MOTEURS", "demarrage, stato, scram"),
    ("CONFIGURATION", "train, freins, carburant"),
    ("NAVIGATION", "route, plan de vol, mission"),
    ("URGENCE / LIAISON", "deroutement, ejection"),
    ("SIMULATION", "temps, son"),
];

/// Carte : nom, état (couleur si actif), explication ; renvoie vrai si cliquée.
fn card(r: Rect, title: &str, state: &str, help: &str, lit: Option<Color>) -> bool {
    let hov = hovered(r);
    rrect(r, 6.0, if hov { Color::new(0.12, 0.14, 0.17, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
    rrect_lines(r, 6.0, if lit.is_some() { 1.5 } else { 1.0 }, lit.unwrap_or(Color::new(0.24, 0.27, 0.31, 1.0)));
    // Voyant vertical à gauche
    draw_rectangle(r.x + 6.0, r.y + 8.0, 4.0, r.h - 16.0, lit.unwrap_or(Color::new(0.2, 0.22, 0.25, 1.0)));
    txt(title, r.x + 18.0, r.y + 20.0, 13.0, WHITE);
    if !state.is_empty() {
        txt_r(state, r.x + r.w - 10.0, r.y + 20.0, 11.0, lit.unwrap_or(DIM));
    }
    // Explication coupée proprement si la carte est étroite
    let mut h = help.to_string();
    while h.len() > 4 && txt_w(&h, 10.0) > r.w - 26.0 {
        h.pop();
    }
    if h.len() < help.len() {
        h = format!("{}..", h.trim_end());
    }
    txt(&h, r.x + 18.0, r.y + r.h - 11.0, 10.0, Color::new(0.6, 0.65, 0.7, 1.0));
    clicked(r)
}

/// Grille de cartes (colonnes, hauteur) : renvoie l'indice cliqué.
fn grid(r: Rect, cols: usize, h: f32, items: &[(&str, String, &str, Option<Color>)]) -> Option<usize> {
    let gap = 8.0;
    let w = (r.w - (cols as f32 - 1.0) * gap) / cols as f32;
    let mut out = None;
    for (k, (t, st, help, lit)) in items.iter().enumerate() {
        let c = Rect::new(r.x + (k % cols) as f32 * (w + gap), r.y + (k / cols) as f32 * (h + gap), w, h);
        if card(c, t, st, help, *lit) {
            out = Some(k);
        }
    }
    out
}

fn section(x: f32, y: f32, t: &str) {
    draw_rectangle(x, y + 3.0, 3.0, 12.0, ACCENT);
    txt(t, x + 9.0, y + 14.0, 11.0, Color::new(0.75, 0.8, 0.85, 1.0));
}

pub fn draw(ac: &Aircraft, cx: &Ctx, r: Rect) -> Option<Act> {
    panel(r, "COMMANDES");
    let mut out: Option<Act> = None;
    let g = GREEN_S;
    let on = |b: bool| b.then_some(g);
    let amber = |b: bool| b.then_some(AMBER);
    // Sous-menus
    let cur = TAB.with(|c| c.get());
    let tx = r.x + 10.0;
    let tw = (r.w - 20.0 - 5.0 * 5.0) / 6.0;
    let ty = r.y + 30.0;
    for (k, (t, sub)) in TABS.iter().enumerate() {
        let b = Rect::new(tx + k as f32 * (tw + 5.0), ty, tw, 40.0);
        let sel = k == cur;
        rrect(b, 6.0, if sel { Color::new(0.08, 0.22, 0.30, 1.0) } else if hovered(b) { Color::new(0.10, 0.12, 0.15, 1.0) } else { Color::new(0.06, 0.07, 0.09, 1.0) });
        if sel {
            draw_rectangle(b.x + 6.0, b.y + b.h - 3.0, b.w - 12.0, 3.0, ACCENT);
        }
        txt_c(t, b.x + b.w / 2.0, b.y + 17.0, 11.0, if sel { WHITE } else { Color::new(0.75, 0.8, 0.85, 1.0) });
        txt_c(sub, b.x + b.w / 2.0, b.y + 31.0, 8.0, DIM);
        if clicked(b) {
            TAB.with(|c| c.set(k));
        }
    }
    let body = Rect::new(r.x + 10.0, ty + 50.0, r.w - 20.0, r.y + r.h - (ty + 50.0) - 8.0);
    let ap = &ac.ap;
    let e = &ac.eng;
    let ch = 58.0;
    match cur {
        // ----- PILOTE AUTOMATIQUE -----
        0 => {
            section(body.x, body.y, "MODES");
            let lat_nav = ap.on && ap.lat == LatMode::Nav;
            let loc = ap.on && (ap.loc_armed || ap.lat == LatMode::Loc);
            let items = [
                ("PILOTE AUTO (AP)", if ap.on { "ENGAGE".into() } else { "OFF".into() }, "Tient le cap et l'altitude actuels", on(ap.on)),
                ("AUTO-MANETTE (A/THR)", if ap.at_on { "ACTIVE".into() } else { "OFF".into() }, "Tient la vitesse / le Mach affiche", on(ap.at_on)),
                ("SUIVI DU PLAN (LNAV)", if lat_nav { "NAV".into() } else { "".into() }, "Suit le plan de vol du MCDU", on(lat_nav)),
                ("PROFIL VERTICAL (VNAV)", if cx.vnav { "ON".into() } else { "OFF".into() }, "Altitude, Mach et moteurs geres par le FMS", on(cx.vnav)),
                ("APPROCHE ILS", if loc { "ARME".into() } else { "".into() }, "Capture l'axe et le plan de descente", on(loc)),
                ("ATTERRISSAGE AUTO", if ac.autoland.is_some() { "ACTIF".into() } else { cx.dest.to_string() }, "Approche et pose automatiques a destination", on(ac.autoland.is_some())),
                ("DECOLLAGE AUTO", if ac.autotakeoff.is_some() { "ACTIF".into() } else { "".into() }, "Roulage, rotation et montee a 15000 ft", on(ac.autotakeoff.is_some())),
                ("DESCENTE HYPERSONIQUE", if ap.on && ap.vert == VertMode::QHold { "ACTIVE".into() } else { "".into() }, "Descend a pression dynamique constante", on(ap.on && ap.vert == VertMode::QHold)),
                ("REMISE DES GAZ", "".into(), "Interrompt l'approche et remonte", None),
                ("REPRISE EN MAIN", "".into(), "Coupe AP, A/THR et automatismes", amber(false)),
            ];
            let mh = 46.0;
            let gr = Rect::new(body.x, body.y + 22.0, body.w, mh * 5.0 + 32.0);
            if let Some(k) = grid(gr, 2, mh, &items) {
                out = Some(match k {
                    0 => Act::Key(KeyCode::P),
                    1 => Act::Key(KeyCode::O),
                    2 => Act::Key(KeyCode::N),
                    3 => Act::Cmd(if cx.vnav { "VNAV OFF".into() } else { "VNAV ON".into() }),
                    4 => Act::Key(KeyCode::L),
                    5 => Act::Cmd(format!("ATTERRIR {}", cx.dest)),
                    6 => Act::Cmd("DECOLLAGE".into()),
                    7 => Act::Cmd("DESCENTE".into()),
                    8 => Act::Cmd("GO AROUND".into()),
                    _ => Act::Key(KeyCode::X),
                });
            }
            // Consignes
            let sy = gr.y + gr.h + 6.0;
            section(body.x, sy, "CONSIGNES (le pilote automatique les suit)");
            let sw = (body.w - 12.0) / 2.0;
            let y0 = sy + 22.0;
            let hdg = ap.hdg_tgt.round();
            if let Some(k) = selector(body.x, y0, sw, "CAP", &format!("{:03.0}", if hdg <= 0.0 { hdg + 360.0 } else { hdg }), ["-10", "-1", "+1", "+10"]) {
                out = Some(Act::Cmd(format!("CAP {:.0}", crate::geo::wrap360(hdg + [-10.0, -1.0, 1.0, 10.0][k]))));
            }
            let alt = ap.alt_tgt_ft;
            if let Some(k) = selector(body.x + sw + 12.0, y0, sw, "ALTITUDE", &format!("{alt:.0}"), ["-1K", "-100", "+100", "+1K"]) {
                out = Some(Act::Cmd(format!("ALT {:.0}", (alt + [-1000.0, -100.0, 100.0, 1000.0][k]).max(0.0))));
            }
            let keas = ap.spd_keas.unwrap_or(ac.d.keas).round();
            if let Some(k) = selector(body.x, y0 + 36.0, sw, "VITESSE", &format!("{keas:.0} KT"), ["-10", "-1", "+1", "+10"]) {
                out = Some(Act::Cmd(format!("KEAS {:.0}", (keas + [-10.0, -1.0, 1.0, 10.0][k]).max(120.0))));
            }
            let mach = ap.mach_tgt;
            if let Some(k) = selector(body.x + sw + 12.0, y0 + 36.0, sw, "MACH", &format!("{mach:.2}"), ["-1", "-0.1", "+0.1", "+1"]) {
                out = Some(Act::Cmd(format!("MACH {:.2}", (mach + [-1.0, -0.1, 0.1, 1.0][k]).clamp(0.3, 11.0))));
            }
            if let Some(k) = selector(body.x, y0 + 72.0, sw, "MINIMUMS", &format!("{:.0} FT", cx.da_ft), ["-50", "-10", "+10", "+50"]) {
                out = Some(Act::Cmd(format!("DA {:.0}", (cx.da_ft + [-50.0, -10.0, 10.0, 50.0][k]).clamp(50.0, 1_000.0))));
            }
        }
        // ----- MOTEURS -----
        1 => {
            section(body.x, body.y, "DEMARRAGE : 1 BATTERIE -> 2 APU -> 3 TURBINES (INS : SOUS-MENU NAVIGATION)");
            let turb = matches!(e.turb, TurbState::Running | TurbState::Starting);
            let a = &ac.apu;
            let apu_run = matches!(a.state, crate::apu::ApuState::Starting | crate::apu::ApuState::Running);
            let avail = a.state == crate::apu::ApuState::Running && a.n > 0.95;
            let ram = match e.ram {
                RamState::Lit => "ALLUME",
                RamState::Armed => "ARME",
                _ => "OFF",
            };
            let scr = match e.scram {
                ScramState::Lit => "ALLUME",
                ScramState::Armed => "ARME",
                ScramState::Unstart => "DESAMORCE",
                _ => "OFF",
            };
            let items = [
                ("1. BATTERIE", if ac.elec.batt_sw { format!("ON {:.0} %", ac.elec.batt * 100.0) } else { "OFF".into() }, "Alimente l'avion (duree limitee au sol)", if ac.elec.batt_sw { if ac.elec.batt < 0.25 { amber(true) } else { on(true) } } else { None }),
                ("2. APU", if avail { "AVAIL".into() } else if apu_run { format!("{:.0} %", a.n * 100.0) } else { "OFF".into() }, "Groupe auxiliaire : courant et air de demarrage", if avail { on(true) } else { apu_run.then_some(CYAN_S) }),
                ("3. TURBINES", if e.turb == TurbState::Starting { "DEMARRAGE".into() } else if turb { "EN MARCHE".into() } else { "ARRETEES".into() }, "Au sol : APU disponible requis (air)", on(turb)),
                ("ENTREES D'AIR", if e.inlet_open { "OUVERTES".into() } else { "FERMEES".into() }, "Ouvertes pour les turbines, fermees en stato", if e.inlet_open { on(true) } else { amber(true) }),
                ("STATOREACTEUR", ram.into(), "1er appui : armer, 2e : allumer (des Mach 2)", if e.ram == RamState::Lit { on(true) } else { (e.ram == RamState::Armed).then_some(CYAN_S) }),
                ("SCRAMJET", scr.into(), "1er appui : armer, 2e : allumer (des Mach 4,8)", if e.scram == ScramState::Lit { on(true) } else { (e.scram != ScramState::Off).then_some(CYAN_S) }),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch * 2.0 + 8.0), 3, ch, &items) {
                out = Some(match k {
                    0 => Act::Cmd(if ac.elec.batt_sw { "BATT OFF".into() } else { "BATT ON".into() }),
                    1 => Act::Cmd(if apu_run { "APU STOP".into() } else { "APU START".into() }),
                    2 => Act::Key(KeyCode::T),
                    3 => Act::Key(KeyCode::I),
                    4 => Act::Key(KeyCode::Y),
                    _ => Act::Key(KeyCode::U),
                });
            }
            let sy = body.y + 22.0 + ch * 2.0 + 20.0;
            section(body.x, sy, "MANETTE DES GAZ (manuel : coupe l'auto-manette)");
            let t = (ac.throttle * 100.0).round();
            let items = [
                ("REDUIRE", "-10 %".into(), "Moins de poussee", None),
                ("MANETTE", format!("{t:.0} %"), "Position actuelle", on(true)),
                ("AUGMENTER", "+10 %".into(), "Plus de poussee", None),
            ];
            match grid(Rect::new(body.x, sy + 22.0, body.w, ch), 3, ch, &items) {
                Some(0) => out = Some(Act::Cmd(format!("THR {:.0}", (t - 10.0).max(0.0)))),
                Some(2) => out = Some(Act::Cmd(format!("THR {:.0}", (t + 10.0).min(100.0)))),
                _ => {}
            }
        }
        // ----- CONFIGURATION -----
        2 => {
            section(body.x, body.y, "TRAIN ET FREINAGE");
            let items = [
                ("TRAIN D'ATTERRISSAGE", if ac.gear_down { "SORTI".into() } else { "RENTRE".into() }, "Sortir pour l'atterrissage, rentrer apres decollage", if ac.gear_down { on(true) } else { None }),
                ("AEROFREINS", if ac.speedbrake { "SORTIS".into() } else { "RENTRES".into() }, "Freinent l'avion en vol et au roulage", on(ac.speedbrake)),
                ("FREINS DE ROUES", if ac.brakes { "SERRES".into() } else { "LIBRES".into() }, "Au sol : arret / frein de parc", on(ac.brakes)),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch), 3, ch, &items) {
                out = Some(Act::Key([KeyCode::G, KeyCode::S, KeyCode::B][k]));
            }
            let sy = body.y + 22.0 + ch + 14.0;
            section(body.x, sy, "CARBURANT (vidanges : allegent l'avion en urgence)");
            let items = [
                ("VIDANGE JP-7", if ac.fuel_dump { "EN COURS".into() } else { format!("{:.1} T", ac.fuel / 1000.0) }, "Vide le kerosene (turbines / stato)", amber(ac.fuel_dump)),
                ("VIDANGE H2", if ac.h2_dump { "EN COURS".into() } else { format!("{:.1} T", ac.h2 / 1000.0) }, "Vide l'hydrogene (scramjet)", amber(ac.h2_dump)),
            ];
            if let Some(k) = grid(Rect::new(body.x, sy + 22.0, body.w, ch), 2, ch, &items) {
                out = Some(Act::Cmd(match k {
                    0 => (if ac.fuel_dump { "FUEL DUMP OFF" } else { "FUEL DUMP" }).into(),
                    _ => (if ac.h2_dump { "H2 DUMP OFF" } else { "H2 DUMP" }).into(),
                }));
            }
        }
        // ----- NAVIGATION -----
        3 => {
            let i = &ac.ins;
            use crate::ins::InsMode;
            section(body.x, body.y, "CENTRALE INERTIELLE (INS) : HORIZON, CAP ET POSITION");
            let items = [
                ("ALIGNER L'INS", if i.mode == InsMode::Align { format!("{} {:.0} %", i.phase(), i.progress() * 100.0) } else { "".into() }, "Au sol, avion immobile : stabilisation puis cap (2 min)", if i.fault { amber(true) } else if i.mode == InsMode::Align { Some(CYAN_S) } else { None }),
                ("INS EN NAV", if i.mode == InsMode::Nav { "NAV".into() } else if i.aligned { "NAV RDY".into() } else { "".into() }, "Apres NAV RDY : attitude, cap et position", on(i.mode == InsMode::Nav)),
                ("INS ATT (SECOURS)", if i.mode == InsMode::Att { i.phase().into() } else { "".into() }, "Horizon seul en 15 s, cap non fiable", amber(i.mode == InsMode::Att)),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch), 3, ch, &items) {
                out = Some(Act::Cmd(["INS ALIGN", "INS NAV", "INS ATT"][k].into()));
            }
            let body = Rect::new(body.x, body.y + ch + 36.0, body.w, body.h - ch - 36.0);
            section(body.x, body.y, "ROUTE ET PLAN DE VOL");
            let items = [
                ("DIRECT DESTINATION", cx.dest.to_string(), "Cap direct vers la destination", None),
                ("CALCULER UN PLAN", format!("{} > {}", cx.origin, cx.dest), "Plan de vol du terrain proche vers la destination", None),
                ("IMPORTER SIMBRIEF", "".into(), "Charge ton dernier plan SimBrief dans le MCDU", None),
                ("CENTRER LA CARTE", "".into(), "La carte / ND suit de nouveau l'avion", None),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch * 2.0 + 8.0), 2, ch, &items) {
                out = Some(match k {
                    0 => Act::Cmd(format!("DIRECT {}", cx.dest)),
                    1 => Act::Cmd(format!("DISPATCH {} {}", cx.origin, cx.dest)),
                    2 => Act::Cmd("SIMBRIEF".into()),
                    _ => Act::Key(KeyCode::C),
                });
            }
            let sy = body.y + 22.0 + ch * 2.0 + 22.0;
            section(body.x, sy, "MISSION");
            let items = [
                ("ROUTE DE RECONNAISSANCE", "".into(), "Suit les objectifs de la mission recon", None),
                ("ANNULER LA ROUTE RECON", "".into(), "Revient au plan de vol", None),
                ("ESCORTE DE CHASSE", if cx.escort { "EN PLACE".into() } else { "".into() }, "Deux F-16 en formation (sous Mach 1,5)", on(cx.escort)),
                ("RAVITAILLEMENT", "".into(), "Appelle un avion ravitailleur", None),
            ];
            if let Some(k) = grid(Rect::new(body.x, sy + 22.0, body.w, ch * 2.0 + 8.0), 2, ch, &items) {
                out = Some(Act::Cmd(match k {
                    0 => "PLAN RECON".into(),
                    1 => "PLAN OFF".into(),
                    2 => (if cx.escort { "ESCORTE OFF" } else { "ESCORTE" }).into(),
                    _ => "TANKER".into(),
                }));
            }
        }
        // ----- URGENCE / LIAISON -----
        4 => {
            section(body.x, body.y, "URGENCE");
            let now = get_time();
            let armed = now - EJECT_ARM.with(|c| c.get()) < 5.0;
            let items = [
                ("DEROUTEMENT D'URGENCE", if cx.emergency { "ACTIF".into() } else { "".into() }, "Vers le terrain le plus proche, automatique", cx.emergency.then_some(RED)),
                ("EMERGENCY OVERRIDE", "".into(), "Plane ou atterrit sans moteur, automatique", None),
                ("ANNULER L'URGENCE", "".into(), "Rend la main au pilote", None),
                ("EJECTION", if armed { "APPUYER A NOUVEAU".into() } else { "2 APPUIS".into() }, "Abandon de l'avion (confirmation dans les 5 s)", armed.then_some(RED)),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch * 2.0 + 8.0), 2, ch, &items) {
                match k {
                    0 => out = Some(Act::Cmd("URGENCE".into())),
                    1 => out = Some(Act::Cmd("EMERGENCY OVERRIDE".into())),
                    2 => out = Some(Act::Cmd("URGENCE OFF".into())),
                    _ => {
                        if armed {
                            EJECT_ARM.with(|c| c.set(-100.0));
                            out = Some(Act::Cmd("EJECTION CONFIRME".into()));
                        } else {
                            EJECT_ARM.with(|c| c.set(now));
                        }
                    }
                }
            }
            let sy = body.y + 22.0 + ch * 2.0 + 22.0;
            section(body.x, sy, if cx.link_ok { "LIAISON DE DONNEES : EN LIGNE" } else { "LIAISON DE DONNEES : PERDUE — CHOISIR UN RELAIS" });
            let lk = if cx.link_ok { "EN LIGNE" } else { "" };
            let items = [
                ("RECONNEXION AUTO", lk.to_string(), "Meilleur relais disponible", if cx.link_ok { on(true) } else { amber(true) }),
                ("DATALINK DIRECT", lk.to_string(), "Liaison directe avec la station", None),
                ("SATCOM", lk.to_string(), "Relais par satellite", None),
                ("SECOURS IRIDIUM", "".into(), "Instructions seulement, pas de manche", None),
            ];
            if let Some(k) = grid(Rect::new(body.x, sy + 22.0, body.w, ch * 2.0 + 8.0), 2, ch, &items) {
                if !cx.link_ok {
                    out = Some(Act::Cmd(["RECONNEXION", "RELAIS LOS", "RELAIS SATCOM", "RELAIS SECOURS"][k].into()));
                }
            }
        }
        // ----- SIMULATION -----
        _ => {
            section(body.x, body.y, "SIMULATION");
            let items = [
                ("ACCELERER LE TEMPS", format!("x{:.0}", cx.warp), "x1, x2, x4 ... x32 (limite pres du sol)", on(cx.warp > 1.0)),
                ("SON", if cx.muted { "COUPE".into() } else { "ON".into() }, "Coupe / remet le son et les alarmes", if cx.muted { amber(true) } else { on(true) }),
            ];
            if let Some(k) = grid(Rect::new(body.x, body.y + 22.0, body.w, ch), 2, ch, &items) {
                out = Some(Act::Key([KeyCode::E, KeyCode::J][k]));
            }
        }
    }
    out
}
