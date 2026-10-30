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
    /// Attente (orbite) à l'IAF pour perdre altitude / vitesse avant l'approche.
    pub holding: bool,
    pub go_arounds: u32,
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
            let al = Autoland { apt: apt_idx, thr_lat: tla, thr_lon: tlo, hdg, elev: a.elev, iaf, phase: AlPhase::ToIaf, gs_captured: false, holding: false, go_arounds: 0 };
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

    /// ILS manuel (touche L) : capture LOC puis G/S selon des critères réalistes, remise des gaz si long.
    fn ils_logic(&mut self) {
        let Some((apt, tla, tlo, hdg, elev)) = self.approach else {
            self.ap.loc_armed = false;
            self.ap.gs_armed = false;
            return;
        };
        if self.on_ground || !self.ap.on {
            return;
        }
        let along = along_before(tla, tlo, hdg, self.lat, self.lon);
        let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
        let herr = wrap180(self.hdg_deg() - hdg).abs();
        if self.ap.loc_armed && along > 0.5 * NM && along < 25.0 * NM && cross.abs() < 3.0 * NM && herr < 100.0 {
            self.ap.loc_armed = false;
            self.ap.lat = LatMode::Loc;
            self.msg(Level::Info, "LOC CAPTURE");
        }
        let path_alt = elev + (along + 300.0) * 3f64.to_radians().tan();
        if self.ap.gs_armed && self.ap.lat == LatMode::Loc && along > 0.3 * NM && self.alt <= path_alt + 60.0 && self.alt >= path_alt - 250.0 {
            self.ap.gs_armed = false;
            self.ap.vert = VertMode::Gs;
            self.msg(Level::Info, "G/S CAPTURE");
        }
        // Approche interrompue : au-delà de la mi-piste sans toucher, remise des gaz (cap piste, 3000 ft sol)
        let half = AIRPORTS[apt].len * 0.5;
        if (self.ap.lat == LatMode::Loc || self.ap.vert == VertMode::Gs) && along < -half {
            self.ap.lat = LatMode::Hdg;
            self.ap.hdg_tgt = hdg;
            self.ap.vert = VertMode::Alt;
            self.ap.alt_tgt_ft = elev / FT + 3_000.0;
            self.ap.loc_armed = false;
            self.ap.gs_armed = false;
            self.throttle = self.throttle.max(0.9);
            self.msg(Level::Caution, "REMISE DES GAZ — APPROCHE ILS INTERROMPUE (L POUR RÉARMER)");
        }
    }

    /// Exécuté à chaque pas avant step() : pilote les modes AP et renvoie
    /// éventuellement des ordres manche (au sol) qui remplacent ceux du pilote.
    pub fn autoflight(&mut self) -> Option<(f64, f64)> {
        if self.crashed.is_some() {
            self.autoland = None;
            self.autotakeoff = None;
            return None;
        }
        if self.autoland.is_none() {
            self.ils_logic();
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
                self.ap.spd_managed = true;
                self.ap.spd_keas = Some(250.0);
                self.autotakeoff = None;
                self.msg(Level::Info, format!("DÉCOLLAGE AUTO TERMINÉ — AP1 : CAP {hdg:03.0}, ALT {alt_ft:.0}, VITESSE GÉRÉE"));
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
                let iaf_alt_ft = elev / FT + IAF_HGT_FT;
                self.ap.on = true;
                self.ap.at_on = true;
                self.ap.lat = LatMode::Hdg;
                self.ap.vert = VertMode::Alt;
                self.ap.alt_tgt_ft = iaf_alt_ft;
                self.ap.spd_keas = Some(if d < 40.0 * NM { 230.0 } else { 300.0 });
                // Prêt pour l'approche : altitude et vitesse d'IAF atteintes
                let ready = self.alt / FT < iaf_alt_ft + 1_200.0 && self.d.keas < 255.0 && self.d.mach < 0.9;
                self.speedbrake = !ready && (self.alt / FT > iaf_alt_ft + 3_000.0 || self.d.keas > 280.0);
                let holding = self.autoland.as_ref().unwrap().holding;
                if d < 3.0 * NM && !ready && !holding {
                    self.autoland.as_mut().unwrap().holding = true;
                    say = Some("AUTOLAND : ATTENTE (HOLD) À L'IAF — DESCENTE / DÉCÉLÉRATION".into());
                }
                if self.autoland.as_ref().unwrap().holding {
                    // Orbite autour de l'IAF (rayon ~3 NM) : cap tangent, recentré selon la distance
                    let corr = ((d - 3.0 * NM) / NM * 20.0).clamp(-40.0, 40.0);
                    self.ap.hdg_tgt = wrap360(brg + 90.0 - corr);
                } else {
                    self.ap.hdg_tgt = brg;
                }
                let aligned = wrap180(self.hdg_deg() - hdg).abs() < 70.0;
                let near_iaf = d < 2.5 * NM && !self.autoland.as_ref().unwrap().holding;
                let on_axis = along > 4.0 * NM && along < IAF_DIST + NM && cross.abs() < 3.0 * NM && aligned;
                let hold_done = self.autoland.as_ref().unwrap().holding && ready && d < 4.0 * NM;
                if ready && (near_iaf || on_axis || hold_done) {
                    let al = self.autoland.as_mut().unwrap();
                    al.holding = false;
                    al.gs_captured = false;
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
                // Capture du plan de descente par en dessous (ou à son niveau) uniquement
                // Capture : par en dessous, ou par au-dessus si l'écart reste rattrapable par la loi G/S (pente ~6° max)
                if !captured && self.alt <= path_alt + 300.0 && self.alt >= path_alt - 150.0 {
                    self.autoland.as_mut().unwrap().gs_captured = true;
                    say = Some("AUTOLAND : G/S CAPTURE".into());
                }
                if self.autoland.as_ref().unwrap().gs_captured {
                    self.ap.vert = VertMode::Gs;
                } else {
                    // Au-dessus du plan : descendre vers lui ; en dessous : palier jusqu'à l'interception
                    self.ap.vert = VertMode::Alt;
                    self.ap.alt_tgt_ft = (path_alt / FT).min(elev / FT + IAF_HGT_FT);
                }
                let va = vapp(self);
                let tgt = if along > 9.0 * NM { va.max(220.0) } else { va };
                self.ap.spd_keas = Some(tgt);
                self.speedbrake = self.d.keas > tgt + 25.0 || self.alt > path_alt + 150.0;
                if along < 9.0 * NM && !self.gear_down {
                    self.cmd_gear();
                }
                // Approche non stabilisée : remise des gaz
                let too_high = along < 3.0 * NM && self.alt > path_alt + 180.0;
                let off_axis = along < 3.0 * NM && cross.abs() > 250.0;
                if too_high || off_axis {
                    next = AlPhase::ToIaf;
                    let al = self.autoland.as_mut().unwrap();
                    al.go_arounds += 1;
                    al.gs_captured = false;
                    al.holding = false;
                    self.speedbrake = false;
                    self.throttle = self.thr_max().min(1.0);
                    self.ap.vert = VertMode::Alt;
                    say = Some(format!("AUTOLAND : REMISE DES GAZ — APPROCHE NON STABILISÉE ({})", if too_high { "TROP HAUT" } else { "HORS AXE" }));
                } else if agl < 18.0 && along < 2.0 * NM {
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
                let half = AIRPORTS[apt].len * 0.5;
                if !self.on_ground && along < -half {
                    next = AlPhase::ToIaf;
                    let al = self.autoland.as_mut().unwrap();
                    al.go_arounds += 1;
                    al.gs_captured = false;
                    self.ap.at_on = true;
                    self.throttle = 1.0;
                    say = Some("AUTOLAND : REMISE DES GAZ — ATTERRISSAGE LONG".into());
                } else if self.on_ground {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Lance un autoland depuis une position donnée (relative au centre de piste de KEDW).
    fn run_case(name: &str, brg_from_apt: f64, dist_nm: f64, alt_ft: f64, mach: f64, hdg: f64, fuel: f64) -> Result<String, String> {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = destination(apt.lat, apt.lon, brg_from_apt, dist_nm * NM);
        ac.lat = la;
        ac.lon = lo;
        ac.alt = alt_ft * FT;
        ac.tas = mach * crate::atmosphere::isa(ac.alt).a;
        ac.psi = hdg.to_radians();
        ac.theta = 3f64.to_radians();
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.fuel = fuel;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.6;
        ac.engage_autoland(edw)?;
        let dt = 1.0 / 60.0;
        let mut t: f64 = 0.0;
        let mut log = Vec::new();
        while t < 3600.0 {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            for (_, m) in ac.events.drain(..) {
                if m.contains("AUTOLAND") || m.contains("TOUCHER") || m.contains("PERTE") || m.contains("REMISE") {
                    log.push(format!("{t:.0}s {m}"));
                }
            }
            if ac.crashed.is_some() || (ac.autoland.is_none() && ac.on_ground) {
                break;
            }
            if std::env::var("AL_TRACE").is_ok() && name.contains("FL350") && (t as i64) % 15 == 0 && (t - t.floor()) < dt {
                if let Some(al) = &ac.autoland {
                    let along = crate::aircraft::along_before(al.thr_lat, al.thr_lon, al.hdg, ac.lat, ac.lon);
                    let cross = crate::aircraft::cross_track(al.thr_lat, al.thr_lon, al.hdg, ac.lat, ac.lon);
                    let path = al.elev + (along + 300.0) * 3f64.to_radians().tan();
                    println!("  t={t:4.0} {:?} hold={} gs={} along={:5.1}NM cross={:5.0}m alt-path={:5.0}m agl={:5.0}ft keas={:3.0} hdg={:3.0} vert={:?}", al.phase, al.holding, al.gs_captured, along / NM, cross, ac.alt - path, (ac.alt - al.elev) / FT, ac.d.keas, ac.hdg_deg(), ac.ap.vert);
                }
            }
            t += dt;
        }
        let res = format!("{name}: {:.0}s crash={:?} sol={} tas={:.0} | {}", t, ac.crashed, ac.on_ground, ac.tas, log.join(" / "));
        if ac.crashed.is_none() && ac.on_ground && ac.tas < 3.0 { Ok(res) } else { Err(res) }
    }

    #[test]
    fn autoland_cases() {
        let cases = [
            ("finale alignée 10NM", 231.0, 12.0, 3_800.0, 0.30, 51.0, 8_000.0),
            ("travers 40NM FL150", 320.0, 40.0, 15_000.0, 0.60, 140.0, 15_000.0),
            ("arrivée haute FL350 M0.95 40NM", 180.0, 40.0, 35_000.0, 0.95, 0.0, 20_000.0),
            ("arrivée très haute FL450 M1.6 80NM", 250.0, 80.0, 45_000.0, 1.6, 70.0, 20_000.0),
            ("mauvais côté 8NM cap opposé", 51.0, 8.0, 6_000.0, 0.35, 51.0, 10_000.0),
            ("au-dessus du terrain FL100", 0.0, 1.0, 10_000.0, 0.45, 300.0, 12_000.0),
            ("lourd 30NM", 140.0, 30.0, 12_000.0, 0.55, 300.0, 36_000.0),
        ];
        let mut fails = Vec::new();
        for (n, b, d, a, m, h, f) in cases {
            match run_case(n, b, d, a, m, h, f) {
                Ok(s) => println!("OK   {s}"),
                Err(s) => {
                    println!("FAIL {s}");
                    fails.push(n);
                }
            }
        }
        assert!(fails.is_empty(), "échecs : {fails:?}");
    }

    /// ILS (touche L) engagé avec une vitesse sélectionnée de 300 KEAS : la vitesse gérée doit
    /// ramener l'avion à la vitesse d'approche, puis réduire la manette (RETARD) à l'arrondi.
    #[test]
    fn ils_managed_speed() {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 16.0 * NM, 0.0);
        ac.lat = la;
        ac.lon = lo;
        ac.psi = apt.hdg.to_radians();
        ac.alt = apt.elev + 4_500.0 * FT;
        ac.tas = 300.0 * KT / (crate::atmosphere::isa(ac.alt).rho / crate::atmosphere::RHO0).sqrt();
        ac.on_ground = false;
        ac.gear_down = true;
        ac.brakes = false;
        ac.fuel = 10_000.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.5;
        ac.ap.on = true;
        ac.ap.at_on = true;
        ac.ap.spd_keas = Some(300.0); // vitesse sélectionnée avant l'ILS
        // Touche L : LOC + G/S, vitesse gérée
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((edw, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.lat = LatMode::Loc;
        ac.ap.vert = VertMode::Gs;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut keas_at = Vec::new();
        let mut retard = false;
        for _ in 0..(900 * 60) {
            ac.step(dt, 0.0, 0.0);
            let along = crate::aircraft::along_before(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon) / NM;
            for mark in [10.0, 4.0] {
                if (along - mark).abs() < 0.02 && !keas_at.iter().any(|(m, _, _)| *m == mark) {
                    keas_at.push((mark, ac.d.keas, vapp(&ac)));
                }
            }
            for (_, m) in ac.events.drain(..) {
                if m.contains("RETARD") {
                    retard = true;
                }
            }
            if retard || ac.on_ground || ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS vitesse gérée : {:?} (NM, KEAS, Vapp) retard={retard}", keas_at);
        let (_, k4, va) = keas_at.iter().find(|(m, _, _)| *m == 4.0).copied().expect("passage à 4 NM");
        assert!((k4 - va).abs() < 12.0, "à 4 NM : {k4:.0} KEAS pour Vapp {va:.0}");
        assert!(retard, "RETARD attendu à l'arrondi");
    }

    /// Reproduction du crash du 30/10 (KSFO) : ILS armé à M2.7 / FL500 à 37 NM, 14.6 km hors axe.
    /// L'ILS ne doit ni capturer le G/S très au-dessus du plan, ni faire tourner l'avion au-dessus du terrain.
    #[test]
    fn ils_armed_high_and_fast_no_crash() {
        let sfo = AIRPORTS.iter().position(|a| a.icao == "KSFO").unwrap();
        let apt = &AIRPORTS[sfo];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 37.0 * NM, 14_600.0);
        ac.lat = la;
        ac.lon = lo;
        ac.alt = 50_000.0 * FT;
        ac.tas = 2.7 * crate::atmosphere::isa(ac.alt).a;
        ac.psi = (apt.hdg - 10.0).to_radians();
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.fuel = 20_000.0;
        ac.set_turbines(TurbState::Running, 0.9);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Hdg;
        ac.ap.hdg_tgt = ac.hdg_deg();
        ac.ap.vert = VertMode::Alt;
        ac.ap.alt_tgt_ft = 50_000.0;
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((sfo, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.loc_armed = true;
        ac.ap.gs_armed = true;
        ac.ap.at_on = true;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut gs_high = false;
        for _ in 0..(900 * 60) {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            let along = crate::aircraft::along_before(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon);
            let path = apt.elev + (along + 300.0) * 3f64.to_radians().tan();
            if ac.ap.vert == VertMode::Gs && ac.alt > path + 600.0 {
                gs_high = true;
            }
            if ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS armé haut/rapide : crash={:?} alt={:.0} ft M{:.2} modes {:?}/{:?} G/S très haut={gs_high}", ac.crashed, ac.alt / FT, ac.d.mach, ac.ap.lat, ac.ap.vert);
        assert!(ac.crashed.is_none());
        assert!(!gs_high, "G/S ne doit pas être capturé très au-dessus du plan");
    }

    /// ILS armé correctement (aligné, sous le plan) : capture LOC puis G/S et arrivée alignée aux minimums.
    #[test]
    fn ils_armed_normal_capture() {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 18.0 * NM, 2_500.0);
        ac.lat = la;
        ac.lon = lo;
        ac.psi = (apt.hdg + 25.0).to_radians();
        ac.alt = apt.elev + 4_000.0 * FT;
        ac.tas = 120.0;
        ac.on_ground = false;
        ac.gear_down = true;
        ac.brakes = false;
        ac.fuel = 10_000.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Hdg;
        ac.ap.hdg_tgt = ac.hdg_deg();
        ac.ap.vert = VertMode::Alt;
        ac.ap.alt_tgt_ft = (ac.alt / FT).round();
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((edw, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.loc_armed = true;
        ac.ap.gs_armed = true;
        ac.ap.at_on = true;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut at_minimums = None;
        for _ in 0..(900 * 60) {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            if ac.d.agl < 70.0 && at_minimums.is_none() {
                at_minimums = Some(crate::aircraft::cross_track(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon));
                break;
            }
            if ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS normal : écart latéral aux minimums {:?} m, modes {:?}/{:?}", at_minimums, ac.ap.lat, ac.ap.vert);
        let c = at_minimums.expect("minimums atteints");
        assert!(c.abs() < 40.0, "aligné à ±40 m aux minimums");
    }
}
