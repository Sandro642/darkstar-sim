//! Page COMMANDES : assistant de vol + fonctions en cartes.
//! - Bandeau de PHASE (cold & dark -> mise en route -> roulage -> décollage -> montée -> croisière ->
//!   descente -> approche) et carte « PROCHAINE ACTION » : l'étape logique suivante, déduite de l'état
//!   de l'avion, exécutable d'un clic.
//! - Rail d'icônes des sous-menus (PILOTE AUTO, MOTEURS, CONFIGURATION, NAVIGATION, URGENCE,
//!   SIMULATION) ; cartes à interrupteur (nom, état, explication) en grille adaptée à la largeur,
//!   défilement à la molette si la place manque.
//! - Un sous-menu peut être affiché seul (élément « COMMANDES : … » de la page CONFIG).
//! Un bouton déclenche exactement l'action du raccourci clavier ou de l'instruction associée.

use crate::aircraft::*;
use crate::ui::*;
use crate::FT;
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
    /// Mode air-air et cible verrouillée
    pub aa: bool,
    pub lock: Option<String>,
    pub tws: bool,
}

thread_local! {
    /// Sous-menu affiché
    static TAB: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Éjection armée (premier appui) : instant de l'armement
    static EJECT_ARM: std::cell::Cell<f64> = const { std::cell::Cell::new(-100.0) };
    /// Défilement de chaque sous-menu
    static SCROLL: std::cell::RefCell<[f32; 6]> = const { std::cell::RefCell::new([0.0; 6]) };
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

/// Sous-menus : nom, nom court (rail étroit).
pub const TABS: [(&str, &str); 6] = [
    ("PILOTE AUTO", "AP"),
    ("MOTEURS", "MOT"),
    ("CONFIGURATION", "CONF"),
    ("NAVIGATION", "NAV"),
    ("URGENCE", "URG"),
    ("SIMULATION", "SIM"),
];

/// Action d'une carte.
enum Do {
    A(Act),
    Eject,
    Nothing,
}

struct Card {
    t: &'static str,
    st: String,
    help: &'static str,
    lit: Option<Color>,
    /// Interrupteur (Some(état)) ou bouton simple (None)
    sw: Option<bool>,
    act: Do,
}

fn c(t: &'static str, st: impl Into<String>, help: &'static str, lit: Option<Color>, sw: Option<bool>, act: Do) -> Card {
    Card { t, st: st.into(), help, lit, sw, act }
}

fn key(k: KeyCode) -> Do {
    Do::A(Act::Key(k))
}
fn cmd(s: impl Into<String>) -> Do {
    Do::A(Act::Cmd(s.into()))
}

const MAGENTA: Color = Color::new(1.0, 0.35, 0.95, 1.0);

// ---------- Icônes vectorielles ----------

fn icon(k: usize, cx: f32, cy: f32, s: f32, col: Color) {
    let th = (s * 0.11).max(1.5);
    match k {
        // Pilote auto : cible de cap
        0 => {
            draw_circle_lines(cx, cy, s * 0.42, th, col);
            draw_circle_lines(cx, cy, s * 0.18, th, col);
            draw_line(cx, cy - s * 0.5, cx, cy - s * 0.28, th, col);
            draw_line(cx, cy + s * 0.28, cx, cy + s * 0.5, th, col);
            draw_line(cx - s * 0.5, cy, cx - s * 0.28, cy, th, col);
            draw_line(cx + s * 0.28, cy, cx + s * 0.5, cy, th, col);
        }
        // Moteurs : flamme
        1 => {
            let p = |a: f32, b: f32| vec2(cx + a * s, cy + b * s);
            for (a, b) in [((0.0, -0.5), (0.28, 0.0)), ((0.28, 0.0), (0.2, 0.32)), ((0.2, 0.32), (0.0, 0.45)), ((0.0, 0.45), (-0.2, 0.32)), ((-0.2, 0.32), (-0.28, 0.0)), ((-0.28, 0.0), (0.0, -0.5))] {
                let (u, v) = (p(a.0, a.1), p(b.0, b.1));
                draw_line(u.x, u.y, v.x, v.y, th, col);
            }
            draw_circle_lines(cx, cy + s * 0.18, s * 0.12, th, col);
        }
        // Configuration : roue de train
        2 => {
            draw_circle_lines(cx, cy + s * 0.12, s * 0.3, th, col);
            draw_circle(cx, cy + s * 0.12, s * 0.08, col);
            draw_line(cx, cy - s * 0.5, cx, cy - s * 0.18, th, col);
            draw_line(cx - s * 0.2, cy - s * 0.5, cx + s * 0.2, cy - s * 0.5, th, col);
        }
        // Navigation : boussole
        3 => {
            draw_circle_lines(cx, cy, s * 0.44, th, col);
            draw_triangle(vec2(cx, cy - s * 0.34), vec2(cx - s * 0.1, cy), vec2(cx + s * 0.1, cy), col);
            draw_triangle_lines(vec2(cx, cy + s * 0.34), vec2(cx - s * 0.1, cy), vec2(cx + s * 0.1, cy), th * 0.7, col);
        }
        // Urgence : triangle
        4 => {
            draw_triangle_lines(vec2(cx, cy - s * 0.45), vec2(cx - s * 0.48, cy + s * 0.38), vec2(cx + s * 0.48, cy + s * 0.38), th, col);
            draw_line(cx, cy - s * 0.15, cx, cy + s * 0.12, th, col);
            draw_circle(cx, cy + s * 0.25, th * 0.7, col);
        }
        // Simulation : horloge
        _ => {
            draw_circle_lines(cx, cy, s * 0.42, th, col);
            draw_line(cx, cy, cx, cy - s * 0.28, th, col);
            draw_line(cx, cy, cx + s * 0.2, cy + s * 0.08, th, col);
        }
    }
}

// ---------- Assistant : phase et prochaine action ----------

const PHASES: [&str; 8] = ["COLD & DARK", "MISE EN ROUTE", "ROULAGE", "DECOLLAGE", "MONTEE", "CROISIERE", "DESCENTE", "APPROCHE"];

fn phase(ac: &Aircraft) -> usize {
    let turb = ac.eng.side.iter().any(|s| *s == TurbState::Running);
    if ac.on_ground {
        if !ac.elec.batt_sw && !turb {
            0
        } else if !turb {
            1
        } else if ac.autotakeoff.is_some() || ac.tas > 30.0 {
            3
        } else {
            2
        }
    } else if ac.autoland.is_some() || (ac.gear_down && ac.d.vs < 0.0) {
        7
    } else if ac.d.vs > 3.0 {
        if ac.d.agl < 1_500.0 * FT { 3 } else { 4 }
    } else if ac.d.vs < -3.0 {
        6
    } else {
        5
    }
}

/// Prochaine action conseillée : titre, raison, action (None : attendre).
fn next_action(ac: &Aircraft, cx: &Ctx) -> (String, String, Option<Act>) {
    use crate::apu::ApuState;
    use crate::ins::InsMode;
    let e = &ac.eng;
    let turb = e.side.iter().all(|s| *s == TurbState::Running);
    let turb_starting = e.side.iter().any(|s| *s == TurbState::Starting);
    let i = &ac.ins;
    let a = &ac.apu;
    let s = |t: &str, r: &str, act: Option<Act>| (t.to_string(), r.to_string(), act);
    if ac.autoland.is_some() {
        return s("ATTERRISSAGE AUTO EN COURS", "Surveiller la trajectoire ; REMISE DES GAZ si l'approche n'est pas stable.", None);
    }
    if !ac.elec.batt_sw && !ac.elec.bus_ok {
        return s("BATTERIE SUR ON", "Rien ne fonctionne sans alimentation : ecrans, INS et APU en dependent.", Some(Act::Cmd("BATT ON".into())));
    }
    if i.mode == InsMode::Off {
        return s("ALIGNER L'INS", "Sans centrale inertielle, pas d'horizon ni de cap. Ne pas bouger l'avion ~2 min.", Some(Act::Cmd("INS ALIGN".into())));
    }
    if ac.on_ground && !turb {
        if turb_starting {
            return s("DEMARRAGE TURBINES EN COURS", &format!("N2 {:.0} % - patienter jusqu'au ralenti stabilise.", e.n2s[0].min(e.n2s[1]) * 104.0), None);
        }
        match a.state {
            ApuState::Off | ApuState::Stopping => {
                return s("DEMARRER L'APU", "L'APU fournit le courant et l'air necessaire au demarrage des turbines.", Some(Act::Cmd("APU START".into())));
            }
            ApuState::Starting => return s("APU EN DEMARRAGE", &format!("Regime {:.0} % - attendre AVAIL (~35 s).", a.n * 100.0), None),
            ApuState::Running if !a.bleed_air() => {
                return s("APU : ATTENDRE L'AIR (BLEED)", "BLEED doit etre sur ON et l'APU a 100 %.", None);
            }
            ApuState::Running => {}
        }
        if !e.inlet_open {
            return s("OUVRIR LES ENTREES D'AIR", "Les turbines ont besoin des entrees d'air ouvertes.", Some(Act::Key(KeyCode::I)));
        }
        return s("DEMARRER LES TURBINES", "L'air de l'APU lance les deux turbines (~25 s), puis l'APU s'arrete seul.", Some(Act::Key(KeyCode::T)));
    }
    if i.mode == InsMode::Align && i.aligned {
        return s("INS EN NAV", "Alignement termine : passer en NAV pour le cap et la position.", Some(Act::Cmd("INS NAV".into())));
    }
    if i.mode == InsMode::Align && ac.on_ground {
        return s("ALIGNEMENT INS EN COURS", &format!("{} - {:.0} %. Ne pas rouler avant NAV RDY.", i.phase(), i.progress() * 100.0), None);
    }
    if ac.on_ground {
        if ac.autotakeoff.is_some() {
            return s("DECOLLAGE EN COURS", "Roulage, rotation et montee automatiques.", None);
        }
        return s("DECOLLAGE AUTOMATIQUE", "Pret : l'avion roule, decolle et monte a 15000 ft.", Some(Act::Cmd("DECOLLAGE".into())));
    }
    if ac.gear_down && ac.d.vs > 2.0 && ac.d.agl > 300.0 * FT && ac.autoland.is_none() {
        return s("RENTRER LE TRAIN", "Montee positive : le train sorti limite la vitesse.", Some(Act::Key(KeyCode::G)));
    }
    if !ac.ap.on && ac.autoland.is_none() {
        return s("ENGAGER LE PILOTE AUTO", "Tient cap et altitude ; ensuite VNAV gere le profil.", Some(Act::Key(KeyCode::P)));
    }
    if ac.d.mach >= 2.0 && ac.d.mach < 4.8 && e.ram == RamState::Off {
        return s("ARMER LE STATOREACTEUR", "Au-dessus de Mach 2 le stato prend le relais des turbines.", Some(Act::Key(KeyCode::Y)));
    }
    if e.ram == RamState::Armed {
        return s("ALLUMER LE STATOREACTEUR", "Stato arme : deuxieme appui pour l'allumage.", Some(Act::Key(KeyCode::Y)));
    }
    if ac.d.mach >= 4.8 && e.scram == ScramState::Off {
        return s("ARMER LE SCRAMJET", "Au-dessus de Mach 4,8 : combustion supersonique a l'hydrogene.", Some(Act::Key(KeyCode::U)));
    }
    if e.scram == ScramState::Armed {
        return s("ALLUMER LE SCRAMJET", "Scram arme : deuxieme appui pour l'allumage.", Some(Act::Key(KeyCode::U)));
    }
    if !ac.gear_down && ac.d.vs < -1.0 && ac.d.agl < 2_500.0 * FT && ac.d.keas < 280.0 {
        return s("SORTIR LE TRAIN", "Approche finale : train sorti avant 1000 ft.", Some(Act::Key(KeyCode::G)));
    }
    if ac.autoland.is_none() && ac.d.mach < 1.0 && ac.d.vs < -3.0 {
        return s("ATTERRISSAGE AUTOMATIQUE", &format!("Approche et pose automatiques a {}.", cx.dest), Some(Act::Cmd(format!("ATTERRIR {}", cx.dest))));
    }
    s("RIEN A FAIRE", "Configuration nominale : surveiller les parametres.", None)
}

// ---------- Contenu des sous-menus ----------

fn sections(tab: usize, ac: &Aircraft, cx: &Ctx) -> Vec<(&'static str, Vec<Card>)> {
    use crate::apu::ApuState;
    use crate::ins::InsMode;
    let g = Some(GREEN_S);
    let on = |b: bool| b.then_some(GREEN_S);
    let amb = |b: bool| b.then_some(AMBER);
    let ap = &ac.ap;
    let e = &ac.eng;
    match tab {
        0 => {
            let lat_nav = ap.on && ap.lat == LatMode::Nav;
            let loc = ap.on && (ap.loc_armed || ap.lat == LatMode::Loc);
            let qh = ap.on && ap.vert == VertMode::QHold;
            vec![
                ("MODES", vec![
                    c("PILOTE AUTO", if ap.on { "ENGAGE" } else { "OFF" }, "Tient le cap et l'altitude", on(ap.on), Some(ap.on), key(KeyCode::P)),
                    c("AUTO-MANETTE", if ap.at_on { "ACTIVE" } else { "OFF" }, "Tient la vitesse / le Mach", on(ap.at_on), Some(ap.at_on), key(KeyCode::O)),
                    c("SUIVI DU PLAN (LNAV)", if lat_nav { "NAV" } else { "" }, "Suit la route du MCDU", on(lat_nav), Some(lat_nav), key(KeyCode::N)),
                    c("PROFIL VERTICAL (VNAV)", if cx.vnav { "ON" } else { "OFF" }, "Altitude, Mach et moteurs par le FMS", on(cx.vnav), Some(cx.vnav), cmd(if cx.vnav { "VNAV OFF" } else { "VNAV ON" })),
                    c("APPROCHE ILS", if loc { "ARME" } else { "" }, "Capture l'axe et le plan", on(loc), Some(loc), key(KeyCode::L)),
                ]),
                ("AUTOMATISMES", vec![
                    c("DECOLLAGE AUTO", if ac.autotakeoff.is_some() { "ACTIF" } else { "" }, "Roulage, rotation, montee 15000 ft", on(ac.autotakeoff.is_some()), None, cmd("DECOLLAGE")),
                    c("ATTERRISSAGE AUTO", if ac.autoland.is_some() { "ACTIF".to_string() } else { cx.dest.to_string() }, "Approche et pose a destination", on(ac.autoland.is_some()), None, cmd(format!("ATTERRIR {}", cx.dest))),
                    c("DESCENTE HYPERSONIQUE", if qh { "ACTIVE" } else { "" }, "Descente a pression dynamique constante", on(qh), None, cmd("DESCENTE")),
                    c("REMISE DES GAZ", "", "Interrompt l'approche et remonte", None, None, cmd("GO AROUND")),
                    c("REPRISE EN MAIN", "", "Coupe AP, A/THR et automatismes", None, None, key(KeyCode::X)),
                ]),
            ]
        }
        1 => {
            let a = &ac.apu;
            let apu_run = matches!(a.state, ApuState::Starting | ApuState::Running);
            let avail = a.state == ApuState::Running && a.n > 0.95;
            let turb = matches!(e.turb, TurbState::Running | TurbState::Starting);
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
            let t = (ac.throttle * 100.0).round();
            vec![
                ("MISE EN ROUTE", vec![
                    c("BATTERIE", if ac.elec.batt_sw { format!("{:.0} %", ac.elec.batt * 100.0) } else { "OFF".into() }, "Alimente l'avion (duree limitee au sol)", if ac.elec.batt_sw && ac.elec.batt < 0.25 { amb(true) } else { on(ac.elec.batt_sw) }, Some(ac.elec.batt_sw), cmd(if ac.elec.batt_sw { "BATT OFF" } else { "BATT ON" })),
                    c("APU", if avail { "AVAIL".to_string() } else if apu_run { format!("{:.0} %", a.n * 100.0) } else { "OFF".into() }, "Courant et air de demarrage", if avail { g } else { apu_run.then_some(CYAN_S) }, Some(apu_run), cmd(if apu_run { "APU STOP" } else { "APU START" })),
                    c("TURBINES", if e.turb == TurbState::Starting { "DEMARRAGE" } else if turb { "MARCHE" } else { "ARRET" }, "Au sol : air de l'APU requis", on(turb), Some(turb), key(KeyCode::T)),
                ]),
                ("PROPULSION", vec![
                    c("ENTREES D'AIR", if e.inlet_open { "OUVERTES" } else { "FERMEES" }, "Ouvertes : turbines ; fermees : stato", if e.inlet_open { g } else { amb(true) }, Some(e.inlet_open), key(KeyCode::I)),
                    c("STATOREACTEUR", ram, "Armer puis allumer (Mach 2+)", if e.ram == RamState::Lit { g } else { (e.ram == RamState::Armed).then_some(CYAN_S) }, Some(e.ram != RamState::Off), key(KeyCode::Y)),
                    c("SCRAMJET", scr, "Armer puis allumer (Mach 4,8+)", if e.scram == ScramState::Lit { Some(MAGENTA) } else { (e.scram != ScramState::Off).then_some(CYAN_S) }, Some(e.scram != ScramState::Off), key(KeyCode::U)),
                ]),
                ("MANETTE (MANUEL)", vec![
                    c("REDUIRE", "-10 %", "Moins de poussee", None, None, cmd(format!("THR {:.0}", (t - 10.0).max(0.0)))),
                    c("MANETTE", format!("{t:.0} %"), "Position actuelle", g, None, Do::Nothing),
                    c("AUGMENTER", "+10 %", "Plus de poussee", None, None, cmd(format!("THR {:.0}", (t + 10.0).min(100.0)))),
                ]),
            ]
        }
        2 => vec![
            ("TRAIN ET FREINAGE", vec![
                c("TRAIN", if ac.gear_down { "SORTI" } else { "RENTRE" }, "Sortir avant l'atterrissage", on(ac.gear_down), Some(ac.gear_down), key(KeyCode::G)),
                c("AEROFREINS", if ac.speedbrake { "SORTIS" } else { "RENTRES" }, "Freinent en vol et au roulage", on(ac.speedbrake), Some(ac.speedbrake), key(KeyCode::K)),
                c("FREINS DE ROUES", if ac.brakes { "SERRES" } else { "LIBRES" }, "Au sol : arret / frein de parc", on(ac.brakes), Some(ac.brakes), key(KeyCode::B)),
            ]),
            ("CARBURANT (allege l'avion en urgence)", vec![
                c("VIDANGE JP-7", if ac.fuel_dump { "EN COURS".to_string() } else { format!("{:.1} T", ac.fuel / 1000.0) }, "Vide le kerosene", amb(ac.fuel_dump), Some(ac.fuel_dump), cmd(if ac.fuel_dump { "FUEL DUMP OFF" } else { "FUEL DUMP" })),
                c("VIDANGE H2", if ac.h2_dump { "EN COURS".to_string() } else { format!("{:.1} T", ac.h2 / 1000.0) }, "Vide l'hydrogene", amb(ac.h2_dump), Some(ac.h2_dump), cmd(if ac.h2_dump { "H2 DUMP OFF" } else { "H2 DUMP" })),
            ]),
        ],
        3 => {
            let i = &ac.ins;
            vec![
                ("CENTRALE INERTIELLE (INS)", vec![
                    c("ALIGNER", if i.mode == InsMode::Align { format!("{:.0} %", i.progress() * 100.0) } else { String::new() }, "Avion immobile, ~2 min", if i.fault { amb(true) } else { (i.mode == InsMode::Align).then_some(CYAN_S) }, None, cmd("INS ALIGN")),
                    c("NAV", if i.mode == InsMode::Nav { "NAV" } else if i.aligned { "NAV RDY" } else { "" }, "Attitude, cap et position", on(i.mode == InsMode::Nav), None, cmd("INS NAV")),
                    c("ATT (SECOURS)", if i.mode == InsMode::Att { i.phase() } else { "" }, "Horizon seul en 15 s", amb(i.mode == InsMode::Att), None, cmd("INS ATT")),
                ]),
                ("ROUTE", vec![
                    c("DIRECT", cx.dest.to_string(), "Cap direct vers la destination", None, None, cmd(format!("DIRECT {}", cx.dest))),
                    c("CALCULER UN PLAN", format!("{}>{}", cx.origin, cx.dest), "Du terrain proche a la destination", None, None, cmd(format!("DISPATCH {} {}", cx.origin, cx.dest))),
                    c("IMPORTER SIMBRIEF", "", "Dernier plan SimBrief dans le MCDU", None, None, cmd("SIMBRIEF")),
                    c("CENTRER LA CARTE", "", "La carte suit l'avion", None, None, key(KeyCode::C)),
                ]),
                ("MISSION", vec![
                    c("ROUTE RECON", "", "Suit les objectifs de reconnaissance", None, None, cmd("PLAN RECON")),
                    c("ANNULER RECON", "", "Revient au plan de vol", None, None, cmd("PLAN OFF")),
                    c("ESCORTE", if cx.escort { "EN PLACE" } else { "" }, "Deux chasseurs (sous Mach 1,5)", on(cx.escort), Some(cx.escort), cmd(if cx.escort { "ESCORTE OFF" } else { "ESCORTE" })),
                    c("RAVITAILLEMENT", "", "Appelle un ravitailleur", None, None, cmd("TANKER")),
                ]),
                ("TRAFIC / AIR-AIR", vec![
                    c("MODE A/A", if cx.aa { "ON" } else { "OFF" }, "Trafic et cibles sur l'horizon (F1)", on(cx.aa), Some(cx.aa), cmd("AA")),
                    c("VERROUILLER", cx.lock.clone().unwrap_or_default(), "Cible suivante devant l'avion (F2)", cx.lock.is_some().then_some(AMBER), None, cmd("LOCK")),
                    c("DEVERROUILLER", "", "Libere la cible (F3)", None, None, cmd("UNLOCK")),
                    c("IFF", "", "Interroge les pistes radar (F4)", None, None, cmd("IFF")),
                    c("RADAR", if cx.tws { "TWS" } else { "RWS" }, "RWS large / TWS pistes avec vitesse", None, Some(cx.tws), cmd(if cx.tws { "RWS" } else { "TWS" })),
                ]),
            ]
        }
        4 => {
            let armed = get_time() - EJECT_ARM.with(|c| c.get()) < 5.0;
            let lk = if cx.link_ok { "EN LIGNE" } else { "" };
            let relay = |s: &str| if cx.link_ok { Do::Nothing } else { cmd(s) };
            vec![
                ("URGENCE", vec![
                    c("DEROUTEMENT", if cx.emergency { "ACTIF" } else { "" }, "Vers le terrain le plus proche", cx.emergency.then_some(RED), None, cmd("URGENCE")),
                    c("OVERRIDE", "", "Plane ou atterrit sans moteur", None, None, cmd("EMERGENCY OVERRIDE")),
                    c("ANNULER", "", "Rend la main au pilote", None, None, cmd("URGENCE OFF")),
                    c("EJECTION", if armed { "CONFIRMER" } else { "2 APPUIS" }, "Abandon de l'avion (5 s)", armed.then_some(RED), None, Do::Eject),
                ]),
                (if cx.link_ok { "LIAISON : EN LIGNE" } else { "LIAISON PERDUE : CHOISIR UN RELAIS" }, vec![
                    c("RECONNEXION AUTO", lk, "Meilleur relais disponible", if cx.link_ok { g } else { amb(true) }, None, relay("RECONNEXION")),
                    c("DATALINK DIRECT", lk, "Liaison directe", None, None, relay("RELAIS LOS")),
                    c("SATCOM", lk, "Relais satellite", None, None, relay("RELAIS SATCOM")),
                    c("SECOURS IRIDIUM", "", "Instructions seulement", None, None, relay("RELAIS SECOURS")),
                ]),
            ]
        }
        _ => vec![("SIMULATION", vec![
            c("ACCELERER LE TEMPS", format!("x{:.0}", cx.warp), "x1 a x32 (limite pres du sol)", on(cx.warp > 1.0), None, key(KeyCode::H)),
            c("SON", if cx.muted { "COUPE" } else { "ON" }, "Son et alarmes", if cx.muted { amb(true) } else { g }, Some(!cx.muted), key(KeyCode::J)),
        ])],
    }
}

// ---------- Dessin ----------

fn fit(s: &str, size: f32, w: f32) -> String {
    let mut h = s.to_string();
    if txt_w(&h, size) <= w {
        return h;
    }
    while h.len() > 2 && txt_w(&format!("{h}.."), size) > w {
        h.pop();
    }
    format!("{}..", h.trim_end())
}

/// Carte à interrupteur ; vrai si cliquée.
fn draw_card(r: Rect, k: &Card, compact: bool) -> bool {
    let hov = hovered(r) && !matches!(k.act, Do::Nothing);
    let base = if hov { Color::new(0.17, 0.175, 0.19, 1.0) } else { Color::new(0.12, 0.125, 0.135, 1.0) };
    rrect(r, 8.0, base);
    if let Some(l) = k.lit {
        // Halo léger de la couleur d'état
        rrect(Rect::new(r.x, r.y, r.w, r.h), 8.0, Color::new(l.r, l.g, l.b, 0.07));
    }
    rrect_lines(r, 6.0, 1.0, k.lit.map(|l| Color::new(l.r, l.g, l.b, 0.6)).unwrap_or(Color::new(0.2, 0.21, 0.23, 1.0)));
    let pad = 12.0;
    let mut right = r.x + r.w - pad;
    // Interrupteur (pilule)
    if let Some(on) = k.sw {
        let (pw, ph) = (34.0, 18.0);
        let p = Rect::new(right - pw, r.y + 10.0, pw, ph);
        let col = k.lit.unwrap_or(Color::new(0.3, 0.32, 0.35, 1.0));
        rrect(p, ph / 2.0, if on { Color::new(col.r * 0.45, col.g * 0.45, col.b * 0.45, 1.0) } else { Color::new(0.14, 0.15, 0.17, 1.0) });
        let kx = if on { p.x + pw - ph / 2.0 } else { p.x + ph / 2.0 };
        draw_circle(kx, p.y + ph / 2.0, ph / 2.0 - 3.0, if on { col } else { Color::new(0.5, 0.52, 0.55, 1.0) });
        right = p.x - 8.0;
    } else if !matches!(k.act, Do::Nothing) {
        // Bouton simple : chevron
        let (x, y) = (right - 6.0, r.y + 19.0);
        draw_line(x - 5.0, y - 5.0, x, y, 2.0, if hov { WHITE } else { DIM });
        draw_line(x, y, x - 5.0, y + 5.0, 2.0, if hov { WHITE } else { DIM });
        right = x - 14.0;
    }
    let tsz = if compact { 12.0 } else { 13.0 };
    let title_w = txt_w(k.t, tsz);
    txt(&fit(k.t, tsz, right - r.x - pad), r.x + pad, r.y + 23.0, tsz, WHITE);
    if !k.st.is_empty() {
        let avail = right - (r.x + pad + title_w + 10.0);
        if avail > 30.0 {
            txt_r(&fit(&k.st, 11.0, avail), right, r.y + 23.0, 11.0, k.lit.unwrap_or(DIM));
        }
    }
    if !compact {
        txt(&fit(k.help, 10.0, r.w - 2.0 * pad), r.x + pad, r.y + r.h - 11.0, 10.0, Color::new(0.58, 0.63, 0.68, 1.0));
    }
    clicked(r) && !matches!(k.act, Do::Nothing)
}

/// Page COMMANDES ; `only` : un seul sous-menu (sans rail ni assistant).
pub fn draw(ac: &Aircraft, cx: &Ctx, r: Rect, only: Option<usize>) -> Option<Act> {
    rrect(r, 8.0, PANEL_BG);
    rrect_lines(r, 8.0, 1.0, PANEL_BORDER);
    let mut out: Option<Act> = None;
    let tab = only.unwrap_or_else(|| TAB.with(|c| c.get()));
    let narrow = r.w < 560.0;
    let mut y = r.y + 10.0;
    let x0 = r.x + 10.0;
    let w0 = r.w - 20.0;
    // ----- Titre -----
    if let Some(t) = only {
        icon(t, x0 + 10.0, y + 10.0, 18.0, ACCENT);
        txt(&format!("COMMANDES - {}", TABS[t].0), x0 + 26.0, y + 15.0, 13.0, Color::new(0.8, 0.85, 0.9, 1.0));
        y += 28.0;
    } else {
        // ----- Assistant : phase -----
        let ph = phase(ac);
        if r.h > 360.0 {
            let n = PHASES.len();
            let sw = w0 / n as f32;
            for (k, p) in PHASES.iter().enumerate() {
                let sx = x0 + k as f32 * sw;
                let col = if k == ph { ACCENT } else if k < ph { Color::new(0.3, 0.55, 0.65, 1.0) } else { Color::new(0.22, 0.24, 0.27, 1.0) };
                draw_rectangle(sx + 1.0, y + 2.0, sw - 2.0, 4.0, col);
                if !narrow || k == ph {
                    txt_c(&fit(p, 9.0, sw - 2.0), sx + sw / 2.0, y + 18.0, 9.0, if k == ph { WHITE } else { DIM });
                }
            }
            y += 26.0;
        }
        // ----- Prochaine action -----
        let (t, why, act) = next_action(ac, cx);
        let hh = if r.h < 330.0 { 40.0 } else { 58.0 };
        let hb = Rect::new(x0, y, w0, hh);
        let pulse = (0.5 + 0.5 * (get_time() * 3.0).sin()) as f32;
        let has = act.is_some();
        rrect(hb, 6.0, Color::new(0.13, 0.135, 0.15, 1.0));
        rrect_lines(hb, 10.0, if has { 1.0 + pulse } else { 1.0 }, if has { Color::new(ACCENT.r, ACCENT.g, ACCENT.b, 0.4 + 0.5 * pulse) } else { PANEL_BORDER });
        let bw = if has { if hh < 50.0 { 76.0 } else { 96.0 } } else { 0.0 };
        if hh < 50.0 {
            txt("SUIVANT", hb.x + 12.0, hb.y + 15.0, 9.0, ACCENT);
            txt(&fit(&t, 13.0, hb.w - bw - 24.0), hb.x + 12.0, hb.y + 32.0, 13.0, WHITE);
        } else {
            txt("PROCHAINE ACTION", hb.x + 14.0, hb.y + 16.0, 9.0, ACCENT);
            txt(&fit(&t, 15.0, hb.w - bw - 28.0), hb.x + 14.0, hb.y + 34.0, 15.0, WHITE);
        }
        if hh > 52.0 {
            txt(&fit(&why, 10.0, hb.w - bw - 28.0), hb.x + 14.0, hb.y + 50.0, 10.0, Color::new(0.62, 0.7, 0.76, 1.0));
        }
        if let Some(a) = act {
            let bh = (hh - 12.0).min(32.0);
            let b = Rect::new(hb.x + hb.w - bw - 8.0, hb.y + hh / 2.0 - bh / 2.0, bw, bh);
            rrect(b, 5.0, if hovered(b) { Color::new(0.45, 0.7, 1.0, 1.0) } else { ACCENT });
            txt_c("EXECUTER", b.x + b.w / 2.0, b.y + b.h / 2.0 + 4.0, if bw < 90.0 { 10.0 } else { 12.0 }, BLACK);
            if clicked(b) {
                out = Some(a);
            }
        }
        y += hh + if hh < 50.0 { 6.0 } else { 10.0 };
    }
    // ----- Rail des sous-menus -----
    let mut cr = Rect::new(x0, y, w0, r.y + r.h - y - 8.0);
    if only.is_none() {
        if narrow {
            // Rangée d'icônes
            let n = TABS.len();
            let bw = w0 / n as f32;
            for (k, (_, short)) in TABS.iter().enumerate() {
                let b = Rect::new(x0 + k as f32 * bw + 2.0, y, bw - 4.0, if r.h < 330.0 { 28.0 } else { 40.0 });
                let small = b.h < 34.0;
                let sel = k == tab;
                rrect(b, 6.0, if sel { Color::new(0.2, 0.21, 0.235, 1.0) } else if hovered(b) { Color::new(0.16, 0.17, 0.19, 1.0) } else { Color::new(0.12, 0.125, 0.14, 1.0) });
                if small {
                    icon(k, b.x + 14.0, b.y + b.h / 2.0, 14.0, if sel { ACCENT } else { DIM });
                    txt_c(short, b.x + b.w / 2.0 + 8.0, b.y + b.h / 2.0 + 4.0, 9.0, if sel { WHITE } else { DIM });
                } else {
                    icon(k, b.x + b.w / 2.0, b.y + 15.0, 16.0, if sel { ACCENT } else { DIM });
                    txt_c(short, b.x + b.w / 2.0, b.y + 35.0, 9.0, if sel { WHITE } else { DIM });
                }
                if clicked(b) {
                    TAB.with(|c| c.set(k));
                }
            }
            let rh = if r.h < 330.0 { 34.0 } else { 48.0 };
            cr = Rect::new(x0, y + rh, w0, r.y + r.h - y - rh - 8.0);
        } else {
            let rw = 92.0;
            let ih = ((cr.h - 5.0 * 6.0) / 6.0).min(64.0);
            for (k, (name, _)) in TABS.iter().enumerate() {
                let b = Rect::new(x0, y + k as f32 * (ih + 6.0), rw, ih);
                let sel = k == tab;
                rrect(b, 8.0, if sel { Color::new(0.2, 0.21, 0.235, 1.0) } else if hovered(b) { Color::new(0.16, 0.17, 0.19, 1.0) } else { Color::new(0.12, 0.125, 0.14, 1.0) });
                if sel {
                    draw_rectangle(b.x, b.y + 8.0, 3.0, b.h - 16.0, ACCENT);
                }
                let is = (ih * 0.42).min(24.0);
                icon(k, b.x + b.w / 2.0, b.y + ih * 0.4, is, if sel { ACCENT } else { Color::new(0.55, 0.6, 0.66, 1.0) });
                txt_c(&fit(name, 9.0, rw - 6.0), b.x + b.w / 2.0, b.y + ih - 8.0, 9.0, if sel { WHITE } else { DIM });
                if clicked(b) {
                    TAB.with(|c| c.set(k));
                }
            }
            cr = Rect::new(x0 + rw + 10.0, y, w0 - rw - 10.0, cr.h);
        }
    }
    // ----- Contenu (défilant) -----
    let secs = sections(tab, ac, cx);
    let compact = cr.w < 300.0 || r.h < 300.0;
    let cols = ((cr.w + 8.0) / 210.0).floor().clamp(1.0, 4.0) as usize;
    let ch = if compact { 36.0 } else { 54.0 };
    let gap = 8.0;
    let cw = (cr.w - (cols as f32 - 1.0) * gap) / cols as f32;
    let mut total = 0.0;
    for (_, cards) in &secs {
        total += 20.0 + (cards.len().div_ceil(cols)) as f32 * (ch + gap) + 6.0;
    }
    if tab == 0 {
        total += 20.0 + 5.0 * 34.0;
    }
    let max_scroll = (total - cr.h).max(0.0);
    let mut sc = SCROLL.with(|s| s.borrow()[tab]).clamp(0.0, max_scroll);
    if hovered(cr) {
        sc = (sc - mouse_wheel().1 * 30.0).clamp(0.0, max_scroll);
    }
    SCROLL.with(|s| s.borrow_mut()[tab] = sc);
    scissor(Some(cr));
    let mut yy = cr.y - sc;
    let vis = |y: f32, h: f32| y + h > cr.y && y < cr.y + cr.h;
    for (title, cards) in &secs {
        draw_rectangle(cr.x, yy + 4.0, 3.0, 11.0, ACCENT);
        txt(title, cr.x + 9.0, yy + 14.0, 10.0, Color::new(0.72, 0.78, 0.84, 1.0));
        yy += 20.0;
        for (k, card) in cards.iter().enumerate() {
            let b = Rect::new(cr.x + (k % cols) as f32 * (cw + gap), yy + (k / cols) as f32 * (ch + gap), cw, ch);
            if vis(b.y, b.h) && draw_card(b, card, compact) && mouse_position().1 >= cr.y && mouse_position().1 <= cr.y + cr.h {
                match &card.act {
                    Do::A(Act::Key(kc)) => out = Some(Act::Key(*kc)),
                    Do::A(Act::Cmd(s)) => out = Some(Act::Cmd(s.clone())),
                    Do::Eject => {
                        let now = get_time();
                        if now - EJECT_ARM.with(|c| c.get()) < 5.0 {
                            EJECT_ARM.with(|c| c.set(-100.0));
                            out = Some(Act::Cmd("EJECTION CONFIRME".into()));
                        } else {
                            EJECT_ARM.with(|c| c.set(now));
                        }
                    }
                    Do::Nothing => {}
                }
            }
        }
        yy += cards.len().div_ceil(cols) as f32 * (ch + gap) + 6.0;
    }
    // Consignes du pilote automatique
    if tab == 0 {
        draw_rectangle(cr.x, yy + 4.0, 3.0, 11.0, ACCENT);
        txt("CONSIGNES (suivies par le pilote auto)", cr.x + 9.0, yy + 14.0, 10.0, Color::new(0.72, 0.78, 0.84, 1.0));
        yy += 20.0;
        let ap = &ac.ap;
        let two = cr.w > 640.0;
        let sw = if two { (cr.w - 12.0) / 2.0 } else { cr.w };
        let pos = |k: usize| if two { (cr.x + (k % 2) as f32 * (sw + 12.0), yy + (k / 2) as f32 * 34.0) } else { (cr.x, yy + k as f32 * 34.0) };
        let hdg = ap.hdg_tgt.round();
        let alt = ap.alt_tgt_ft;
        let keas = ap.spd_keas.unwrap_or(ac.d.keas).round();
        let mach = ap.mach_tgt;
        let rows: [(&str, String, [&str; 4]); 5] = [
            ("CAP", format!("{:03.0}", if hdg <= 0.0 { hdg + 360.0 } else { hdg }), ["-10", "-1", "+1", "+10"]),
            ("ALTITUDE", format!("{alt:.0}"), ["-1K", "-100", "+100", "+1K"]),
            ("VITESSE", format!("{keas:.0} KT"), ["-10", "-1", "+1", "+10"]),
            ("MACH", format!("{mach:.2}"), ["-1", "-0.1", "+0.1", "+1"]),
            ("MINIMUMS", format!("{:.0} FT", cx.da_ft), ["-50", "-10", "+10", "+50"]),
        ];
        for (k, (l, v, st)) in rows.iter().enumerate() {
            let (sx, sy) = pos(k);
            if !vis(sy, 34.0) {
                continue;
            }
            if let Some(s) = selector(sx, sy, sw, l, v, *st) {
                out = Some(Act::Cmd(match k {
                    0 => format!("CAP {:.0}", crate::geo::wrap360(hdg + [-10.0, -1.0, 1.0, 10.0][s])),
                    1 => format!("ALT {:.0}", (alt + [-1000.0, -100.0, 100.0, 1000.0][s]).max(0.0)),
                    2 => format!("KEAS {:.0}", (keas + [-10.0, -1.0, 1.0, 10.0][s]).max(120.0)),
                    3 => format!("MACH {:.2}", (mach + [-1.0, -0.1, 0.1, 1.0][s]).clamp(0.3, 11.0)),
                    _ => format!("DA {:.0}", (cx.da_ft + [-50.0, -10.0, 10.0, 50.0][s]).clamp(50.0, 1_000.0)),
                }));
            }
        }
    }
    scissor(Some(r));
    // Barre de défilement
    if max_scroll > 0.0 {
        let th = cr.h * cr.h / total;
        let ty = cr.y + (cr.h - th) * sc / max_scroll;
        draw_rectangle(cr.x + cr.w + 2.0, ty, 3.0, th, Color::new(0.4, 0.45, 0.5, 0.7));
    }
    out
}
