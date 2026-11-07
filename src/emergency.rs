//! Procédure d'urgence automatique (instruction EMERGENCY) :
//! 1. ravitaillement en vol si un ravitailleur est joignable (ou le ravitailleur d'alerte) ;
//! 2. sinon déroutement vers le terrain joignable le plus proche (carburant, ou vol plané moteurs coupés),
//!    avec descente hypersonique et redémarrage des turbines si nécessaire, puis atterrissage automatique ;
//! 3. sinon éjection, balise de détresse, effacement des données et autodestruction de la cellule.
//!
//! La procédure pilote les systèmes existants par leurs instructions (TANKER, RAVITAILLEMENT, DESCENTE,
//! INLET, TURB, ATTERRIR), exécutées par la station sol.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::refuel::{Phase, Refuel};
use crate::ui::*;
use macroquad::prelude::*;

/// Finesse en vol plané (subsonique).
const GLIDE_RATIO: f64 = 7.0;
/// Carburant pour l'approche et l'atterrissage (kg).
const APPROACH: f64 = 1_500.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Refuel,
    Divert(usize),
    Glide(usize),
    Eject,
}

pub struct Emergency {
    pub mode: Option<Mode>,
    phase: u8,
    t: f64,
    /// Le ravitaillement a été essayé (pas de nouvelle tentative après abandon).
    refuel_tried: bool,
    pub events: Vec<(Level, String)>,
    /// Instructions à exécuter par la station sol.
    pub cmds: Vec<String>,
}

fn powered(ac: &Aircraft) -> bool {
    ac.fuel > 50.0 && !ac.eng.side.iter().all(|s| *s == TurbState::Failed)
}

/// Rayon de vol plané (m) vers un terrain d'altitude `elev`.
fn glide_range(ac: &Aircraft, elev: f64) -> f64 {
    let h = (ac.alt - elev).max(0.0);
    // Énergie cinétique convertie en hauteur (limitée : une partie se dissipe en décélération)
    let hk = (ac.tas * ac.tas / (2.0 * 9.81)).min(20_000.0) * 0.5;
    (h + hk) * GLIDE_RATIO
}

impl Emergency {
    pub fn new() -> Emergency {
        Emergency { mode: None, phase: 0, t: 0.0, refuel_tried: false, events: Vec::new(), cmds: Vec::new() }
    }

    /// Choix de l'option selon l'ordre de priorité ; renvoie aussi la justification.
    pub fn evaluate(&self, ac: &Aircraft, rf: &Refuel) -> (Mode, String) {
        let per_m = crate::fuel::burn_per_m(ac);
        let pw = powered(ac);
        // Mach recalculé (la donnée affichée peut ne pas être encore à jour)
        let mach = ac.tas / crate::atmosphere::isa(ac.alt).a;
        // 1. Ravitaillement
        if pw && !self.refuel_tried && mach < 1.5 && ac.fuel < FUEL_MAX * 0.95 {
            match &rf.tanker {
                Some(t) => {
                    let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
                    if ac.fuel > d * per_m + 2_000.0 && t.offload > 2_000.0 {
                        return (Mode::Refuel, format!("RAVITAILLEUR {} A {:.0} NM JOIGNABLE", t.callsign, d / NM));
                    }
                }
                None => {
                    let endu = if ac.eng.fuel_flow > 0.05 { ac.fuel / ac.eng.fuel_flow } else { f64::INFINITY };
                    // Le ravitailleur d'alerte met ~7 min à être rejoint : il faut de la marge
                    if endu > 1_200.0 && ac.fuel > 5_000.0 {
                        return (Mode::Refuel, format!("RAVITAILLEUR D'ALERTE DEMANDE (AUTONOMIE {})", crate::mission::fmt_time(endu.min(359_999.0))));
                    }
                }
            }
        }
        // 2. Terrain joignable le plus proche
        let mut apts: Vec<(usize, f64)> = AIRPORTS.iter().enumerate().map(|(i, a)| (i, dist_brg(ac.lat, ac.lon, a.lat, a.lon).0)).collect();
        apts.sort_by(|a, b| a.1.total_cmp(&b.1));
        if pw {
            if let Some(&(i, d)) = apts.iter().find(|(_, d)| ac.fuel > d * per_m * 1.1 + APPROACH) {
                return (Mode::Divert(i), format!("TERRAIN JOIGNABLE {} A {:.0} NM", AIRPORTS[i].icao, d / NM));
            }
        }
        if let Some(&(i, d)) = apts.iter().find(|(i, d)| *d < glide_range(ac, AIRPORTS[*i].elev)) {
            return (Mode::Glide(i), format!("VOL PLANE VERS {} A {:.0} NM", AIRPORTS[i].icao, d / NM));
        }
        // 3. Aucune solution
        let (i, d) = apts[0];
        (Mode::Eject, format!("AUCUN TERRAIN JOIGNABLE (PLUS PROCHE {} A {:.0} NM)", AIRPORTS[i].icao, d / NM))
    }

    pub fn start(&mut self, ac: &Aircraft, rf: &Refuel) -> Result<String, String> {
        if ac.on_ground {
            return Err("AU SOL".into());
        }
        if ac.crashed.is_some() {
            return Err("VEHICULE PERDU".into());
        }
        self.refuel_tried = false;
        let (mode, why) = self.evaluate(ac, rf);
        self.set(mode);
        self.events.push((Level::Warning, "MAYDAY MAYDAY MAYDAY — DARKSTAR 01 — TRANSPONDEUR 7700".into()));
        Ok(format!("URGENCE : {} — {why}", self.label(ac)))
    }

    fn set(&mut self, m: Mode) {
        self.mode = Some(m);
        self.phase = 0;
        self.t = 0.0;
    }

    pub fn cancel(&mut self) {
        if self.mode.is_some_and(|m| m != Mode::Eject) {
            self.mode = None;
            self.events.push((Level::Info, "URGENCE ANNULEE".into()));
        }
    }

    pub fn eject_now(&mut self) {
        self.set(Mode::Eject);
    }

    pub fn label(&self, ac: &Aircraft) -> String {
        match self.mode {
            None => String::new(),
            Some(Mode::Refuel) => "RAVITAILLEMENT EN VOL".into(),
            Some(Mode::Divert(i)) | Some(Mode::Glide(i)) => {
                let a = &AIRPORTS[i];
                let (d, b) = dist_brg(ac.lat, ac.lon, a.lat, a.lon);
                format!("{} {} {b:03.0} deg {:.0} NM", if matches!(self.mode, Some(Mode::Glide(_))) { "VOL PLANE" } else { "DEROUTEMENT" }, a.icao, d / NM)
            }
            Some(Mode::Eject) => "EJECTION / AUTODESTRUCTION".into(),
        }
    }

    pub fn update(&mut self, dt: f64, ac: &mut Aircraft, rf: &Refuel) {
        let Some(mode) = self.mode else { return };
        self.t += dt;
        if ac.crashed.is_some() {
            self.mode = None;
            return;
        }
        match mode {
            Mode::Refuel => match self.phase {
                0 => {
                    self.refuel_tried = true;
                    if rf.tanker.is_none() {
                        self.cmds.push("TANKER".into());
                    }
                    self.cmds.push("RAVITAILLEMENT".into());
                    self.phase = 1;
                    self.t = 0.0;
                }
                _ => {
                    if self.t > 2.0 && rf.phase == Phase::Off {
                        if ac.fuel > FUEL_MAX * 0.9 {
                            self.events.push((Level::Info, "URGENCE TERMINEE — CARBURANT RETABLI".into()));
                            self.mode = None;
                        } else {
                            // Ravitaillement impossible ou abandonné : option suivante
                            let (m, why) = self.evaluate(ac, rf);
                            self.events.push((Level::Caution, format!("RAVITAILLEMENT ECHOUE — {why}")));
                            self.set(m);
                        }
                    }
                }
            },
            Mode::Divert(i) => {
                let apt = &AIRPORTS[i];
                let brg = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon).1;
                match self.phase {
                    0 => {
                        if ac.d.mach > 1.6 {
                            self.cmds.push("DESCENTE 50".into());
                        }
                        self.cmds.push(format!("CAP {brg:03.0}"));
                        self.phase = 1;
                        self.t = 0.0;
                    }
                    1 => {
                        // Cap tenu vers le terrain pendant la décélération
                        if ac.ap.on && ac.ap.lat == LatMode::Hdg {
                            ac.ap.hdg_tgt = brg.round();
                        }
                        if ac.d.mach < 1.5 {
                            if ac.eng.turb != TurbState::Running {
                                if !ac.eng.inlet_open {
                                    self.cmds.push("INLET OPEN".into());
                                }
                                if ac.eng.turb == TurbState::Off {
                                    self.cmds.push("TURB START".into());
                                }
                                if self.t > 120.0 {
                                    let (m, why) = self.evaluate(ac, rf);
                                    self.events.push((Level::Caution, format!("REDEMARRAGE TURBINES IMPOSSIBLE — {why}")));
                                    self.set(if m == Mode::Divert(i) { Mode::Glide(i) } else { m });
                                }
                            } else {
                                self.cmds.push(format!("ATTERRIR {}", apt.icao));
                                self.phase = 2;
                                self.t = 0.0;
                            }
                        }
                    }
                    _ => {
                        if ac.on_ground && ac.tas < 3.0 {
                            self.events.push((Level::Info, format!("URGENCE TERMINEE — POSE A {}", apt.icao)));
                            self.mode = None;
                        } else if ac.autoland.is_none() && !ac.on_ground && self.t > 5.0 {
                            // Autoland refusé ou interrompu : nouvelle évaluation
                            let (m, why) = self.evaluate(ac, rf);
                            self.events.push((Level::Caution, format!("ATTERRISSAGE AUTO INDISPONIBLE — {why}")));
                            self.set(m);
                        }
                    }
                }
            }
            Mode::Glide(i) => {
                let apt = &AIRPORTS[i];
                let (d, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
                ac.ap.on = true;
                ac.ap.lat = LatMode::Hdg;
                ac.ap.hdg_tgt = brg.round();
                ac.ap.vert = VertMode::Pitch;
                ac.ap.pitch_tgt = if ac.d.mach > 1.2 { 2.0 } else { -3.0 };
                ac.ap.at_on = false;
                if self.phase == 0 {
                    self.events.push((Level::Warning, format!("VOL PLANE VERS {} — FINESSE {GLIDE_RATIO:.0}", apt.icao)));
                    self.phase = 1;
                }
                if d < 6.0 * NM && self.phase == 1 {
                    self.events.push((Level::Warning, format!("{} EN VUE — ATTERRISSAGE PLANE MANUEL (X, TRAIN)", apt.icao)));
                    self.phase = 2;
                    self.mode = None;
                } else if d > glide_range(ac, apt.elev) * 1.15 && self.t > 10.0 {
                    self.events.push((Level::Warning, "TERRAIN HORS DE PORTEE EN PLANE".into()));
                    self.set(Mode::Eject);
                }
            }
            Mode::Eject => {
                ac.ap.on = false;
                ac.ap.at_on = false;
                ac.autoland = None;
                ac.throttle = 0.0;
                let steps: [(f64, Level, String); 4] = [
                    (0.0, Level::Warning, "EJECTION ! EJECTION ! EJECTION !".into()),
                    (3.0, Level::Warning, format!("PARACHUTE OUVERT — BALISE DE DETRESSE 406 MHZ : {:.4} {:.4}", ac.lat, ac.lon)),
                    (6.0, Level::Warning, "AUTODESTRUCTION ARMEE — EFFACEMENT DES DONNEES ET DES CLES DE CHIFFREMENT".into()),
                    (9.0, Level::Warning, "CHARGES DE DESTRUCTION : 3... 2... 1...".into()),
                ];
                for (k, (at, l, m)) in steps.iter().enumerate() {
                    if self.phase as usize == k && self.t >= *at {
                        self.events.push((*l, m.clone()));
                        self.phase += 1;
                    }
                }
                if self.t >= 12.0 {
                    ac.crash("EJECTION — AUTODESTRUCTION DE LA CELLULE");
                    self.mode = None;
                }
            }
        }
    }
}

/// Bandeau d'urgence sur l'horizon.
pub fn draw_hud(em: &Emergency, ac: &Aircraft, s: Rect, time: f64) {
    let Some(mode) = em.mode else { return };
    let cx = s.x + s.w / 2.0;
    let y = s.y + s.h * 0.12;
    let blink = (time * 2.0).fract() < 0.6;
    let (title, col) = match mode {
        Mode::Eject => (format!("EJECTION — AUTODESTRUCTION {:.0}", (12.0 - em.t).max(0.0)), RED),
        _ => ("EMERGENCY 7700".to_string(), RED),
    };
    let w = txt_w(&title, 20.0) + 26.0;
    if blink || mode != Mode::Eject {
        rrect(Rect::new(cx - w / 2.0, y - 21.0, w, 28.0), 5.0, Color::new(0.3, 0.0, 0.0, 0.85));
        rrect_lines(Rect::new(cx - w / 2.0, y - 21.0, w, 28.0), 5.0, 1.5, col);
        txt_c(&title, cx, y, 20.0, col);
    }
    if mode != Mode::Eject {
        let l = em.label(ac);
        txt_c(&l, cx + 1.0, y + 23.0, 14.0, Color::new(0.0, 0.0, 0.0, 0.7));
        txt_c(&l, cx, y + 22.0, 14.0, AMBER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flying(lat: f64, lon: f64, alt_ft: f64, fuel: f64) -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.lat = lat;
        ac.lon = lon;
        ac.alt = alt_ft * FT;
        ac.tas = 220.0;
        ac.d.mach = 0.7;
        ac.fuel = fuel;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.eng.fuel_flow = 1.5;
        ac
    }

    #[test]
    fn decision_order() {
        let rf = Refuel::new();
        let em = Emergency::new();
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let (la, lo) = destination(AIRPORTS[edw].lat, AIRPORTS[edw].lon, 90.0, 40.0 * NM);
        // Autonomie > 15 min : ravitailleur d'alerte
        let ac = flying(la, lo, 25_000.0, 20_000.0);
        assert_eq!(em.evaluate(&ac, &rf).0, Mode::Refuel);
        // Peu de carburant : déroutement
        let ac = flying(la, lo, 25_000.0, 2_500.0);
        assert!(matches!(em.evaluate(&ac, &rf).0, Mode::Divert(_)));
        // Moteurs perdus, haut : vol plané
        let mut ac = flying(la, lo, 40_000.0, 0.0);
        ac.eng.side = [TurbState::Failed, TurbState::Failed];
        assert!(matches!(em.evaluate(&ac, &rf).0, Mode::Glide(_)));
        // Moteurs perdus, bas, loin de tout (Pacifique) : éjection
        let mut ac = flying(30.0, -140.0, 5_000.0, 0.0);
        ac.eng.side = [TurbState::Failed, TurbState::Failed];
        assert_eq!(em.evaluate(&ac, &rf).0, Mode::Eject);
    }

    #[test]
    fn ejection_sequence_destroys_airframe() {
        let rf = Refuel::new();
        let mut em = Emergency::new();
        let mut ac = flying(30.0, -140.0, 5_000.0, 0.0);
        em.eject_now();
        for _ in 0..200 {
            em.update(0.1, &mut ac, &rf);
        }
        assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("AUTODESTRUCTION")));
        assert!(em.events.iter().any(|(_, m)| m.contains("PARACHUTE")));
    }
}
