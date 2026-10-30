//! Automatismes de vol : décollage automatique et atterrissage automatique (autoland).

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;

/// Vitesse d'approche selon la masse (incidence d'approche ~11°), bornée à 165-235 KEAS.
pub fn vapp(ac: &Aircraft) -> f64 {
    let v = (2.0 * ac.mass() * 9.81 / (crate::atmosphere::RHO0 * S_WING * 0.52)).sqrt() / KT;
    v.clamp(165.0, 235.0)
}
const IAF_DIST: f64 = 15.0 * NM;
const IAF_HGT_FT: f64 = 4_500.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlPhase {
    ToIaf,
    Approach,
    Flare,
    Rollout,
}

pub struct Autoland {
    pub apt: usize,
    pub thr_lat: f64,
    pub thr_lon: f64,
    pub hdg: f64,
    pub elev: f64,
    pub iaf: (f64, f64),
    pub phase: AlPhase,
    pub gs_captured: bool,
}

impl Autoland {
    /// Choisit le sens de piste qui minimise le trajet jusqu'à l'IAF.
    pub fn new(apt_idx: usize, lat: f64, lon: f64) -> Autoland {
        let a = &AIRPORTS[apt_idx];
        let mut best: Option<(f64, Autoland)> = None;
        for (along_thr, hdg) in [(-a.len / 2.0, a.hdg), (a.len / 2.0, wrap360(a.hdg + 180.0))] {
            let (tla, tlo) = a.point(along_thr, 0.0);
            let iaf = destination(tla, tlo, wrap360(hdg + 180.0), IAF_DIST);
            let (d, _) = dist_brg(lat, lon, iaf.0, iaf.1);
            let al = Autoland { apt: apt_idx, thr_lat: tla, thr_lon: tlo, hdg, elev: a.elev, iaf, phase: AlPhase::ToIaf, gs_captured: false };
            if best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                best = Some((d, al));
            }
        }
        best.unwrap().1
    }

    pub fn runway_number(&self) -> u32 {
        crate::airports::Airport::runway_number(self.hdg)
    }
}

pub struct Takeoff {
    pub hdg: f64,
    pub alt_ft: f64,
    pub thr_lat: f64,
    pub thr_lon: f64,
}

impl Aircraft {
    /// Engage l'atterrissage automatique ; renvoie une erreur lisible si impossible.
    pub fn engage_autoland(&mut self, apt: usize) -> Result<(), String> {
        if self.on_ground {
            return Err("AU SOL".into());
        }
        if self.eng.turb != TurbState::Running {
            return Err("TURBINES REQUISES (M<2.5 : I puis T, ou commande TURB START)".into());
        }
        let al = Autoland::new(apt, self.lat, self.lon);
        let num = al.runway_number();
        self.autoland = Some(al);
        self.autotakeoff = None;
        self.ap.on = true;
        self.ap.at_on = true;
        self.ap.vert = VertMode::Alt;
        self.ap.lat = LatMode::Hdg;
        self.msg(Level::Info, format!("AUTOLAND ARMÉ : {} PISTE {:02} — RALLIEMENT IAF 15 NM", AIRPORTS[apt].icao, num));
        Ok(())
    }

    pub fn engage_takeoff(&mut self, alt_ft: f64) -> Result<(), String> {
        if !self.on_ground {
            return Err("DÉJÀ EN VOL".into());
        }
        if self.eng.turb != TurbState::Running {
            return Err("TURBINES NON DÉMARRÉES (T)".into());
        }
        let Some(apt) = AIRPORTS.iter().find(|a| a.on_runway(self.lat, self.lon)) else {
            return Err("PAS SUR UNE PISTE".into());
        };
        // Axe de piste le plus proche du cap actuel
        let h = self.hdg_deg();
        let hdg = if wrap180(apt.hdg - h).abs() < 90.0 { apt.hdg } else { wrap360(apt.hdg + 180.0) };
        let (thr_lat, thr_lon) = apt.point(if hdg == apt.hdg { -apt.len / 2.0 } else { apt.len / 2.0 }, 0.0);
        self.autotakeoff = Some(Takeoff { hdg, alt_ft, thr_lat, thr_lon });
        self.autoland = None;
        self.msg(Level::Info, format!("DÉCOLLAGE AUTO : PLEINE PUISSANCE, ROTATION 170 KEAS, MONTÉE {alt_ft:.0} FT"));
        Ok(())
    }

    /// Exécuté à chaque pas avant step() : pilote les modes AP et renvoie
    /// éventuellement des ordres manche (au sol) qui remplacent ceux du pilote.
    pub fn autoflight(&mut self) -> Option<(f64, f64)> {
        if self.crashed.is_some() {
            self.autoland = None;
            self.autotakeoff = None;
            return None;
        }
        if let Some(to) = &self.autotakeoff {
            let (hdg, alt_ft, tla, tlo) = (to.hdg, to.alt_ft, to.thr_lat, to.thr_lon);
            if self.on_ground {
                self.brakes = false;
                self.ap.at_on = false;
                self.throttle = 1.0;
                let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
                let herr = wrap180(self.hdg_deg() - hdg);
                let steer = (-(0.03 * cross + 0.2 * herr)).clamp(-1.0, 1.0);
                let pitch = if self.d.keas >= 170.0 && self.theta < 10f64.to_radians() { 1.0 } else { 0.0 };
                return Some((pitch, steer));
            }
            if self.d.agl > 30.0 {
                if self.gear_down {
                    self.cmd_gear();
                }
                self.ap.on = true;
                self.ap.lat = LatMode::Hdg;
                self.ap.hdg_tgt = hdg;
                self.ap.vert = VertMode::Alt;
                self.ap.alt_tgt_ft = alt_ft;
                self.ap.at_on = true;
                self.ap.spd_keas = Some(350.0);
                self.autotakeoff = None;
                self.msg(Level::Info, format!("DÉCOLLAGE AUTO TERMINÉ — AP1 : CAP {hdg:03.0}, ALT {alt_ft:.0}, 350 KEAS"));
            }
            return None;
        }

        let Some(al) = &self.autoland else { return None };
        let (tla, tlo, hdg, elev) = (al.thr_lat, al.thr_lon, al.hdg, al.elev);
        let phase = al.phase;
        let apt = al.apt;
        self.approach = Some((apt, tla, tlo, hdg, elev));
        let along = along_before(tla, tlo, hdg, self.lat, self.lon);
        let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
        let agl = self.alt - elev;
        let mut next = phase;
        let mut out = None;
        let mut say: Option<String> = None;

        match phase {
            AlPhase::ToIaf => {
                let (iaf_lat, iaf_lon) = self.autoland.as_ref().unwrap().iaf;
                let (d, brg) = dist_brg(self.lat, self.lon, iaf_lat, iaf_lon);
                self.ap.on = true;
                self.ap.at_on = true;
                self.ap.lat = LatMode::Hdg;
                self.ap.hdg_tgt = brg;
                self.ap.vert = VertMode::Alt;
                self.ap.alt_tgt_ft = elev / FT + IAF_HGT_FT;
                self.ap.spd_keas = Some(if d < 40.0 * NM { 230.0 } else { 300.0 });
                let aligned = wrap180(self.hdg_deg() - hdg).abs() < 70.0;
                if d < 2.5 * NM || (along > 2.0 * NM && along < IAF_DIST + NM && cross.abs() < 3.0 * NM && aligned) {
                    next = AlPhase::Approach;
                    say = Some("AUTOLAND : LOC CAPTURE".into());
                }
            }
            AlPhase::Approach => {
                self.ap.on = true;
                self.ap.at_on = true;
                self.ap.lat = LatMode::Loc;
                let path_alt = elev + (along + 300.0) * 3f64.to_radians().tan();
                let captured = self.autoland.as_ref().unwrap().gs_captured;
                if !captured && self.alt >= path_alt - 20.0 {
                    self.autoland.as_mut().unwrap().gs_captured = true;
                    say = Some("AUTOLAND : G/S CAPTURE".into());
                }
                if self.autoland.as_ref().unwrap().gs_captured {
                    self.ap.vert = VertMode::Gs;
                } else {
                    self.ap.vert = VertMode::Alt;
                    self.ap.alt_tgt_ft = (elev / FT + IAF_HGT_FT).min(self.ap.alt_tgt_ft);
                }
                let va = vapp(self);
                let tgt = if along > 9.0 * NM { va.max(220.0) } else { va };
                self.ap.spd_keas = Some(tgt);
                self.speedbrake = self.d.keas > tgt + 25.0;
                if along < 9.0 * NM && !self.gear_down {
                    self.cmd_gear();
                }
                if agl < 18.0 && along < 2.0 * NM {
                    next = AlPhase::Flare;
                    say = Some("AUTOLAND : FLARE".into());
                    self.speedbrake = false;
                }
            }
            AlPhase::Flare => {
                self.ap.vert = VertMode::Flare;
                if agl < 9.0 && self.ap.at_on {
                    self.ap.at_on = false;
                    say = Some("RETARD, RETARD".into());
                }
                if !self.ap.at_on {
                    self.throttle = (self.throttle - 0.5 / 120.0).max(0.0);
                }
                if self.on_ground {
                    next = AlPhase::Rollout;
                    say = Some("AUTOLAND : ROLLOUT — AÉROFREINS, AUTOBRAKE".into());
                }
            }
            AlPhase::Rollout => {
                self.ap.on = false;
                self.ap.at_on = false;
                self.throttle = 0.0;
                self.speedbrake = true;
                // Autobrake MAX dès que la roulette de nez est posée
                if self.theta < 2f64.to_radians() || self.d.keas < 200.0 {
                    self.brakes = true;
                }
                let herr = wrap180(self.hdg_deg() - hdg);
                let steer = (-(0.03 * cross + 0.2 * herr)).clamp(-1.0, 1.0);
                out = Some((-1.0, steer));
                if self.tas < 2.0 {
                    self.autoland = None;
                    self.speedbrake = false;
                    self.msg(Level::Info, "AUTOLAND TERMINÉ — AVION ARRÊTÉ SUR LA PISTE");
                    return out;
                }
            }
        }
        if let Some(s) = say {
            self.msg(Level::Info, s);
        }
        if let Some(al) = self.autoland.as_mut() {
            al.phase = next;
        }
        out
    }
}
