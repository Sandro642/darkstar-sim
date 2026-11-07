//! Système de reconnaissance du SR-72 Darkstar :
//! - caméra EO/IR sur tourelle (NADIR ou poursuite TRACK de l'objectif jusqu'à 60° hors verticale) ;
//! - radar à synthèse d'ouverture : bandes latérales (STRIP) ou visée détaillée de l'objectif (SPOT) ;
//! - récepteur ELINT : détection des émetteurs radar, relèvements successifs et triangulation ;
//! - menaces sol-air : domaines d'engagement (portée, plafond, Mach maximal), tir et esquive ;
//! - objectifs IMINT / SAR / ELINT avec priorités et score ;
//! - photothèque (vignettes rendues depuis l'imagerie réelle), stockage et transmission au sol ;
//! - planification automatique de la route de reconnaissance suivie par l'autopilote.

use crate::aircraft::*;
use crate::geo::*;
use crate::tiles::{self, Key, Layer, Tiles};
use crate::ui::*;
use macroquad::prelude::*;

pub const ZOOMS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
const FOV_DEG: f64 = 40.0;
const SENSOR_PX: f64 = 4000.0;
const GIMBAL_MAX_DEG: f64 = 60.0;
const STORAGE_GB: f64 = 512.0;
const IMG_GB: f64 = 1.6;
const SAR_GB: f64 = 4.0;
const SAR_NEAR: f64 = 15_000.0;
const SAR_FAR: f64 = 70_000.0;
const SAR_SPOT_FAR: f64 = 150_000.0;
const SAR_STRIP_NIIRS: f64 = 3.5;
const SAR_SPOT_NIIRS: f64 = 5.0;
const ELINT_SIGMA_DEG: f64 = 1.5;
const ELINT_LOCALIZED_M: f64 = 1_500.0;
const MISSILE_TOF: f64 = 25.0;
const MAX_SHOTS: usize = 80;
const GOLD: Color = Color::new(1.0, 0.85, 0.2, 1.0);

// ---------- Émetteurs et menaces ----------

#[derive(Clone, Copy, PartialEq)]
pub enum EmKind {
    Search,
    FireControl,
    Sam,
}

pub struct Emitter {
    pub name: &'static str,
    pub kind: EmKind,
    pub band: &'static str,
    pub lat: f64,
    pub lon: f64,
    /// Portée de détection (radar) ou d'engagement (SAM), m.
    pub range: f64,
    pub ceiling_ft: f64,
    pub mach_max: f64,
    /// Relèvements mesurés : (lat, lon de l'avion, relèvement en degrés).
    pub bearings: Vec<(f64, f64, f64)>,
    pub est: Option<(f64, f64, f64)>,
    pub localized: bool,
    pub heard: bool,
    meas_t: f64,
}

fn emitters() -> Vec<Emitter> {
    let e = |name, kind, band, lat, lon, range_km: f64, ceiling_ft, mach_max| Emitter {
        name,
        kind,
        band,
        lat,
        lon,
        range: range_km * 1000.0,
        ceiling_ft,
        mach_max,
        bearings: Vec::new(),
        est: None,
        localized: false,
        heard: false,
        meas_t: 0.0,
    };
    // Émetteurs d'exercice fictifs (champs de tir du Nevada et de Californie)
    vec![
        e("VEILLE EW-1", EmKind::Search, "E/F", 37.30, -116.20, 350.0, 0.0, 0.0),
        e("SAM LONGUE PORTEE LR-7", EmKind::Sam, "G/H", 37.55, -116.55, 150.0, 95_000.0, 5.0),
        e("SAM MOYENNE PORTEE MR-3", EmKind::Sam, "H/I", 36.40, -115.90, 70.0, 75_000.0, 3.5),
        e("CONDUITE DE TIR FC-2", EmKind::FireControl, "I/J", 35.75, -117.40, 60.0, 0.0, 0.0),
        e("RADAR COTIER CR-5", EmKind::Search, "E", 33.30, -119.45, 250.0, 0.0, 0.0),
        e("SAM MOBILE SH-9", EmKind::Sam, "J", 34.95, -116.70, 45.0, 50_000.0, 3.0),
    ]
}

/// Horizon radar (km) entre deux hauteurs (m), rayon terrestre effectif 4/3.
fn radar_horizon(h1: f64, h2: f64) -> f64 {
    4_120.0 * (h1.max(0.0).sqrt() + h2.max(0.0).sqrt())
}

// ---------- Objectifs ----------

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Imint(f64),
    Sar,
    Elint(usize),
}

pub struct Target {
    pub code: &'static str,
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub kind: Kind,
    pub prio: u8,
    pub best: Option<(f64, &'static str, f64)>,
    elint_ok: bool,
}

impl Target {
    pub fn done(&self) -> bool {
        match self.kind {
            Kind::Imint(req) => self.best.is_some_and(|b| b.0 >= req),
            Kind::Sar => self.best.is_some_and(|b| b.1.starts_with("SAR")),
            Kind::Elint(_) => self.elint_ok,
        }
    }
    pub fn kind_tag(&self) -> &'static str {
        match self.kind {
            Kind::Imint(_) => "IMINT",
            Kind::Sar => "SAR",
            Kind::Elint(_) => "ELINT",
        }
    }
    fn points(&self) -> u32 {
        match self.prio {
            1 => 30,
            2 => 20,
            _ => 10,
        }
    }
}

fn targets(em: &[Emitter]) -> Vec<Target> {
    let t = |code, name, lat, lon, kind, prio| Target { code, name, lat, lon, kind, prio, best: None, elint_ok: false };
    // Zone de recherche ELINT : centrée à ~12 km de l'émetteur (position réelle inconnue)
    let zone = |i: usize| (em[i].lat + 0.08, em[i].lon - 0.09);
    vec![
        t("A", "LAC SEC ROGERS (EDWARDS)", 34.92, -117.83, Kind::Imint(3.0), 3),
        t("B", "CHAMP DE TIR CHINA LAKE", 35.69, -117.69, Kind::Imint(4.0), 2),
        t("C", "AERODROME TONOPAH", 37.79, -116.78, Kind::Imint(5.0), 1),
        t("D", "VANDENBERG SLC-6", 34.58, -120.63, Kind::Imint(5.0), 1),
        t("E", "PORT DE LONG BEACH", 33.75, -118.21, Kind::Sar, 2),
        t("F", "BARRAGE HOOVER", 36.016, -114.737, Kind::Imint(5.0), 3),
        t("G", "ILE SAN NICOLAS", 33.25, -119.50, Kind::Sar, 2),
        t("H", "METEOR CRATER (ARIZONA)", 35.027, -111.022, Kind::Imint(3.0), 3),
        t("I", "SITE SAM LR-7 (NTTR)", zone(1).0, zone(1).1, Kind::Elint(1), 1),
        t("J", "CONDUITE DE TIR FC-2", zone(3).0, zone(3).1, Kind::Elint(3), 2),
        t("K", "RADAR COTIER CR-5", zone(4).0, zone(4).1, Kind::Elint(4), 3),
    ]
}

// ---------- Photothèque ----------

pub struct Shot {
    pub sensor: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub width: f64,
    pub hdg: f64,
    pub q: f64,
    pub t: f64,
    pub target: Option<usize>,
    pub gb: f64,
    pub sent: f64,
}

// ---------- État ----------

#[derive(Clone, Copy, PartialEq)]
pub enum SarMode {
    Off,
    Strip,
    Spot,
}

pub struct Recon {
    pub cam_on: bool,
    pub ir: bool,
    pub track: bool,
    pub zoom: usize,
    pub sar: SarMode,
    pub elint_on: bool,
    pub targets: Vec<Target>,
    pub emitters: Vec<Emitter>,
    pub shots: Vec<Shot>,
    pub selected: usize,
    pub steer: bool,
    pub route: Vec<usize>,
    pub route_on: bool,
    pub page: usize,
    pub img_sel: Option<usize>,
    pub events: Vec<(Level, String)>,
    /// Menace active : (émetteur, temps dans le domaine, temps de vol du missile éventuel).
    pub threat: Option<(usize, f64, Option<f64>)>,
    /// Menaces sol-air actives (instruction MENACES OFF pour les vols d'essai sans opposition).
    pub threats_on: bool,
    rng: u64,
}

pub enum CamAction {
    ZoomIn,
    ZoomOut,
}

pub enum ReconAction {
    Camera,
    Mode,
    Track,
    Sar,
    Elint,
    Select(usize),
    Steer,
    Route,
    ZoomIn,
    ZoomOut,
    Photo,
    Page(usize),
    Image(usize),
}

fn agl(ac: &Aircraft) -> f64 {
    (ac.alt - ac.d.elev).max(30.0)
}

impl Recon {
    pub fn new() -> Recon {
        let emitters = emitters();
        let targets = targets(&emitters);
        Recon {
            cam_on: false,
            ir: false,
            track: true,
            zoom: 2,
            sar: SarMode::Off,
            elint_on: true,
            targets,
            emitters,
            shots: Vec::new(),
            selected: 0,
            steer: false,
            route: Vec::new(),
            route_on: false,
            page: 0,
            img_sel: None,
            events: Vec::new(),
            threat: None,
            threats_on: true,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn noise(&mut self) -> f64 {
        // Gaussienne approchée (somme de 4 uniformes)
        let mut s = 0.0;
        for _ in 0..4 {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            s += (self.rng >> 11) as f64 / (1u64 << 53) as f64;
        }
        (s - 2.0) * 1.732
    }

    pub fn sar_on(&self) -> bool {
        self.sar != SarMode::Off
    }

    /// Point visé par la caméra (lat, lon) et angle hors verticale (rad).
    pub fn los(&self, ac: &Aircraft) -> (f64, f64, f64) {
        if self.track {
            let t = &self.targets[self.selected];
            if matches!(t.kind, Kind::Imint(_)) && !t.done() {
                let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
                let off = (d / agl(ac)).atan();
                if off.to_degrees() <= GIMBAL_MAX_DEG {
                    return (t.lat, t.lon, off);
                }
            }
        }
        (ac.lat, ac.lon, 0.0)
    }

    fn slant(&self, ac: &Aircraft) -> f64 {
        agl(ac) / self.los(ac).2.cos().max(0.2)
    }

    pub fn footprint(&self, ac: &Aircraft) -> f64 {
        2.0 * self.slant(ac) * (FOV_DEG / ZOOMS[self.zoom] / 2.0).to_radians().tan()
    }

    pub fn gsd(&self, ac: &Aircraft) -> f64 {
        (self.footprint(ac) / SENSOR_PX).max(0.08)
    }

    /// Qualité NIIRS prévue de la caméra et cause de dégradation.
    pub fn niirs(&self, ac: &Aircraft) -> (f64, Option<&'static str>) {
        let mut q = 5.5 - 3.32 * (self.gsd(ac) / 0.5).log10();
        let bank = ac.phi.to_degrees().abs();
        let mut why = None;
        if bank > 30.0 {
            return (0.0, Some("INCLINAISON > 30 deg"));
        }
        if bank > 10.0 {
            q -= (bank - 10.0) * 0.1;
            why = Some("ROULIS");
        }
        let off = self.los(ac).2;
        if off > 0.5 {
            q -= (1.0 - off.cos()) * 2.0;
            why = why.or(Some("VISEE OBLIQUE"));
        }
        if self.ir {
            q -= 0.8;
        }
        if ac.skin_temp - 273.15 > 600.0 {
            q -= 0.5;
            why = why.or(Some("HUBLOT CHAUD"));
        }
        (q.clamp(0.0, 9.0), why)
    }

    pub fn sensors_ok(ac: &Aircraft) -> bool {
        ac.elec.bus_ok && !ac.on_ground && ac.crashed.is_none()
    }

    pub fn stored_gb(&self) -> f64 {
        self.shots.iter().map(|s| s.gb).sum::<f64>() + 0.0
    }

    pub fn sent_gb(&self) -> f64 {
        self.shots.iter().map(|s| s.sent).sum::<f64>() + 0.0
    }

    pub fn score(&self) -> (u32, u32) {
        let done = self.targets.iter().filter(|t| t.done()).map(|t| t.points()).sum();
        let total = self.targets.iter().map(|t| t.points()).sum();
        (done, total)
    }

    fn add_shot(&mut self, shot: Shot) -> bool {
        if self.stored_gb() + shot.gb > STORAGE_GB {
            self.events.push((Level::Caution, "RECON : MEMOIRE PLEINE — IMAGE PERDUE".into()));
            return false;
        }
        if self.shots.len() >= MAX_SHOTS {
            // Libère la plus ancienne image déjà transmise
            if let Some(i) = self.shots.iter().position(|s| s.sent >= s.gb) {
                self.shots.remove(i);
            } else {
                self.events.push((Level::Caution, "RECON : PHOTOTHEQUE PLEINE".into()));
                return false;
            }
        }
        self.shots.push(shot);
        true
    }

    /// Prise de vue manuelle au point visé.
    pub fn photo(&mut self, ac: &Aircraft, t: f64) {
        if !(self.cam_on && Self::sensors_ok(ac)) {
            self.events.push((Level::Caution, "RECON : CAMERA INDISPONIBLE".into()));
            return;
        }
        let (la, lo, _) = self.los(ac);
        let (q, _) = self.niirs(ac);
        let sensor = if self.ir { "IR" } else { "EO" };
        if self.add_shot(Shot { sensor, lat: la, lon: lo, width: self.footprint(ac), hdg: ac.psi, q, t, target: None, gb: IMG_GB, sent: 0.0 }) {
            self.events.push((Level::Info, format!("RECON : PRISE DE VUE {sensor} NIIRS {q:.1}")));
        }
    }

    fn record(&mut self, i: usize, q: f64, sensor: &'static str, gb: f64, width: f64, ac: &Aircraft, t: f64) {
        let tg = &self.targets[i];
        if !tg.best.is_none_or(|b| q > b.0 + 0.3) {
            return;
        }
        let (code, name, kind, lat, lon) = (tg.code, tg.name, tg.kind, tg.lat, tg.lon);
        if !self.add_shot(Shot { sensor, lat, lon, width, hdg: ac.psi, q, t, target: Some(i), gb, sent: 0.0 }) {
            return;
        }
        let first = self.targets[i].best.is_none();
        self.targets[i].best = Some((q, sensor, t));
        let ok = self.targets[i].done();
        let req = match kind {
            Kind::Imint(r) => format!("requis {r:.0}"),
            _ => "SAR requis".into(),
        };
        self.events.push((
            if ok { Level::Info } else { Level::Caution },
            format!("RECON : CIBLE {code} {name} — {sensor} NIIRS {q:.1} ({req}) — {}{}", if ok { "OBJECTIF REMPLI" } else { "QUALITE INSUFFISANTE" }, if first { "" } else { " (AMELIOREE)" }),
        ));
    }

    pub fn update(&mut self, dt: f64, ac: &mut Aircraft, t: f64, link_los: bool) {
        let ok = Self::sensors_ok(ac);
        // --- Caméra ---
        if ok && self.cam_on {
            let (la, lo, _) = self.los(ac);
            let half = self.footprint(ac) * 0.45;
            let (q, _) = self.niirs(ac);
            let sensor = if self.ir { "IR" } else { "EO" };
            let width = self.footprint(ac);
            for i in 0..self.targets.len() {
                let tg = &self.targets[i];
                if !matches!(tg.kind, Kind::Imint(_)) || q < 0.5 {
                    continue;
                }
                let (e, n) = enu_offset(la, lo, tg.lat, tg.lon);
                if e.abs() < half && n.abs() < half {
                    self.record(i, q, sensor, IMG_GB, width, ac, t);
                }
            }
        }
        // --- SAR ---
        if ok && self.sar_on() && ac.alt > 9_000.0 {
            for i in 0..self.targets.len() {
                let tg = &self.targets[i];
                if matches!(tg.kind, Kind::Elint(_)) {
                    continue;
                }
                let (e, n) = enu_offset(ac.lat, ac.lon, tg.lat, tg.lon);
                let h = ac.psi;
                let along = e * h.sin() + n * h.cos();
                let cross = (e * h.cos() - n * h.sin()).abs();
                let shot = match self.sar {
                    SarMode::Strip if along.abs() < 2_500.0 && (SAR_NEAR..SAR_FAR).contains(&cross) => Some(("SAR STRIP", SAR_STRIP_NIIRS, 10_000.0)),
                    SarMode::Spot if i == self.selected && (SAR_NEAR..SAR_SPOT_FAR).contains(&cross) && along.abs() < cross * 0.58 => Some(("SAR SPOT", SAR_SPOT_NIIRS, 3_000.0)),
                    _ => None,
                };
                if let Some((s, q, w)) = shot {
                    self.record(i, q, s, SAR_GB, w, ac, t);
                }
            }
        }
        // --- ELINT et menaces ---
        let h_ac = ac.alt.max(0.0);
        let mut threat: Option<(usize, f64)> = None;
        for k in 0..self.emitters.len() {
            let (d, brg) = {
                let em = &self.emitters[k];
                dist_brg(ac.lat, ac.lon, em.lat, em.lon)
            };
            let em_range = self.emitters[k].range;
            let visible = d < radar_horizon(h_ac, 15.0) && d < em_range * 1.6 && ac.crashed.is_none();
            if ok && self.elint_on && visible {
                if !self.emitters[k].heard {
                    self.emitters[k].heard = true;
                    let em = &self.emitters[k];
                    self.events.push((Level::Caution, format!("ELINT : NOUVEL EMETTEUR {} BANDE {} — RELEVEMENT {brg:03.0}", em.name, em.band)));
                }
                self.emitters[k].meas_t -= dt;
                if self.emitters[k].meas_t <= 0.0 {
                    self.emitters[k].meas_t = 2.0;
                    let b = brg + self.noise() * ELINT_SIGMA_DEG;
                    let em = &mut self.emitters[k];
                    em.bearings.push((ac.lat, ac.lon, b));
                    if em.bearings.len() > 80 {
                        em.bearings.remove(0);
                    }
                    em.est = triangulate(&em.bearings);
                    if !em.localized && em.est.is_some_and(|e| e.2 < ELINT_LOCALIZED_M) {
                        em.localized = true;
                        let (name, err) = (em.name, em.est.map(|e| e.2).unwrap_or(0.0));
                        self.events.push((Level::Info, format!("ELINT : {name} LOCALISE (precision {err:.0} m)")));
                        for tg in self.targets.iter_mut() {
                            if tg.kind == Kind::Elint(k) && !tg.elint_ok {
                                tg.elint_ok = true;
                                tg.best = Some((9.0, "ELINT", t));
                                self.events.push((Level::Info, format!("RECON : CIBLE {} {} — OBJECTIF REMPLI", tg.code, tg.name)));
                            }
                        }
                        let _ = self.add_shot(Shot { sensor: "ELINT", lat: self.emitters[k].lat, lon: self.emitters[k].lon, width: err * 4.0, hdg: 0.0, q: 0.0, t, target: None, gb: 0.1, sent: 0.0 });
                    }
                }
            }
            let em = &self.emitters[k];
            if self.threats_on && em.kind == EmKind::Sam && d < em.range && ac.alt / FT < em.ceiling_ft && ac.d.mach < em.mach_max && !ac.on_ground && ac.crashed.is_none() {
                threat = Some((k, d));
            }
        }
        // Domaine d'engagement : menace -> tir après 12 s -> impact après le temps de vol
        match (threat, self.threat) {
            (Some((k, _)), None) => {
                self.threat = Some((k, 0.0, None));
                self.events.push((Level::Caution, format!("MENACE SAM : {} — SORTIR DU DOMAINE (> {:.0} ft OU > M{:.1})", self.emitters[k].name, self.emitters[k].ceiling_ft, self.emitters[k].mach_max)));
            }
            (Some((k, _)), Some((_, t_in, missile))) => {
                let t_in = t_in + dt;
                let missile = match missile {
                    None if t_in > 12.0 => {
                        self.events.push((Level::Warning, format!("TIR MISSILE — {} — MONTER / ACCELERER", self.emitters[k].name)));
                        Some(0.0)
                    }
                    Some(m) => Some(m + dt),
                    None => None,
                };
                if missile.is_some_and(|m| m > MISSILE_TOF) {
                    ac.crash(&format!("ABATTU PAR MISSILE SOL-AIR ({})", self.emitters[k].name));
                    self.threat = None;
                } else {
                    self.threat = Some((k, t_in, missile));
                }
            }
            (None, Some((k, _, missile))) => {
                if missile.is_some() {
                    self.events.push((Level::Info, format!("MISSILE ESQUIVE — HORS DU DOMAINE DE {}", self.emitters[k].name)));
                } else {
                    self.events.push((Level::Info, format!("HORS DU DOMAINE DE {}", self.emitters[k].name)));
                }
                self.threat = None;
            }
            (None, None) => {}
        }
        // --- Guidage : objectif sélectionné ou route planifiée ---
        if self.route_on {
            self.route.retain(|&i| !self.targets[i].done());
            match self.route.first() {
                Some(&i) => {
                    if self.selected != i {
                        self.selected = i;
                        let tg = &self.targets[i];
                        self.events.push((Level::Info, format!("ROUTE RECON : CAP SUR CIBLE {} {}", tg.code, tg.name)));
                    }
                    self.steer = true;
                }
                None => {
                    self.route_on = false;
                    self.steer = false;
                    self.events.push((Level::Info, "ROUTE RECON TERMINEE — TOUS LES OBJECTIFS PLANIFIES REMPLIS".into()));
                }
            }
        }
        if self.steer {
            let tg = &self.targets[self.selected];
            if tg.done() && !self.route_on {
                self.steer = false;
                self.events.push((Level::Info, format!("RECON : CIBLE {} REMPLIE — FIN DU GUIDAGE", tg.code)));
            } else if ac.ap.on && ac.ap.lat == LatMode::Hdg {
                ac.ap.hdg_tgt = dist_brg(ac.lat, ac.lon, tg.lat, tg.lon).1.round();
            } else {
                self.steer = false;
                self.route_on = false;
            }
        }
        // --- Transmission : priorité aux objectifs, puis ordre chronologique ---
        if ac.elec.bus_ok && ac.crashed.is_none() {
            let mut budget = if link_los { 0.6 } else { 0.08 } * dt;
            let mut order: Vec<usize> = (0..self.shots.len()).filter(|&i| self.shots[i].sent < self.shots[i].gb).collect();
            order.sort_by_key(|&i| (self.shots[i].target.map(|t| self.targets[t].prio).unwrap_or(9), (self.shots[i].t * 10.0) as i64));
            for i in order {
                let s = &mut self.shots[i];
                let take = (s.gb - s.sent).min(budget);
                s.sent += take;
                budget -= take;
                if budget <= 0.0 {
                    break;
                }
            }
        }
    }

    /// Route au plus proche voisin à travers les objectifs restants.
    pub fn plan(&mut self, ac: &Aircraft) -> f64 {
        let mut left: Vec<usize> = (0..self.targets.len()).filter(|&i| !self.targets[i].done()).collect();
        let (mut la, mut lo) = (ac.lat, ac.lon);
        let mut total = 0.0;
        self.route.clear();
        while !left.is_empty() {
            let (k, d) = left
                .iter()
                .enumerate()
                .map(|(k, &i)| (k, dist_brg(la, lo, self.targets[i].lat, self.targets[i].lon).0))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            let i = left.swap_remove(k);
            total += d;
            la = self.targets[i].lat;
            lo = self.targets[i].lon;
            self.route.push(i);
        }
        total
    }

    pub fn find(&self, arg: &str) -> Option<usize> {
        self.targets.iter().position(|t| t.code == arg || t.name.starts_with(arg))
    }

    /// Menaces pour l'horizon et le RWR : émetteurs entendus avec leur état.
    pub fn threat_cues(&self, ac: &Aircraft) -> Vec<crate::pfd_plus::ThreatCue> {
        let mut out = Vec::new();
        for (k, em) in self.emitters.iter().enumerate() {
            if !em.heard || !self.elint_on {
                continue;
            }
            let (d, _) = dist_brg(ac.lat, ac.lon, em.lat, em.lon);
            if d > radar_horizon(ac.alt.max(0.0), 15.0) || d > em.range * 1.6 {
                continue;
            }
            let (state, missile) = match self.threat {
                Some((tk, _, Some(m))) if tk == k => (4, Some((m / MISSILE_TOF, (MISSILE_TOF - m).max(0.0)))),
                Some((tk, t_in, None)) if tk == k => (if t_in > 4.0 { 3 } else { 2 }, None),
                _ if em.kind != EmKind::Search && d < em.range => (2, None),
                _ => (1, None),
            };
            let escape = if em.kind == EmKind::Sam {
                format!("MONTER > FL{:.0} OU ACCELERER > M{:.1}", em.ceiling_ft / 100.0 + 20.0, em.mach_max + 0.2)
            } else {
                String::new()
            };
            // Position : estimation ELINT si disponible, sinon émetteur entendu dans son relèvement
            let (la, lo) = em.est.map(|e| (e.0, e.1)).unwrap_or((em.lat, em.lon));
            out.push(crate::pfd_plus::ThreatCue { lat: la, lon: lo, name: em.name, sam: em.kind == EmKind::Sam, state, dist: d, missile, escape });
        }
        out
    }

    /// Alarmes de menace pour l'horizon et le panneau d'alarmes.
    pub fn alerts(&self) -> Vec<(Level, String)> {
        match self.threat {
            Some((k, _, Some(m))) => vec![(Level::Warning, format!("MISSILE {} — IMPACT {:.0}s", self.emitters[k].name, (MISSILE_TOF - m).max(0.0)))],
            Some((k, _, None)) => vec![(Level::Caution, format!("MENACE SAM {}", self.emitters[k].name))],
            None => vec![],
        }
    }

    fn engage_steer(&mut self, ac: &mut Aircraft) -> Result<String, String> {
        if ac.on_ground {
            return Err("AU SOL".into());
        }
        if !ac.ap.on {
            ac.ap.on = true;
            ac.ap.vert = VertMode::Alt;
            ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
        }
        ac.autoland = None;
        ac.ap.lat = LatMode::Hdg;
        self.steer = true;
        let t = &self.targets[self.selected];
        ac.ap.hdg_tgt = dist_brg(ac.lat, ac.lon, t.lat, t.lon).1.round();
        Ok(format!("GUIDAGE VERS CIBLE {} {} — CAP {:03.0}", t.code, t.name, ac.ap.hdg_tgt))
    }

    fn start_route(&mut self, ac: &mut Aircraft) -> Result<String, String> {
        let d = self.plan(ac);
        if self.route.is_empty() {
            return Err("AUCUN OBJECTIF RESTANT".into());
        }
        self.selected = self.route[0];
        self.engage_steer(ac)?;
        self.route_on = true;
        let codes: Vec<&str> = self.route.iter().map(|&i| self.targets[i].code).collect();
        Ok(format!("ROUTE RECON {} — {:.0} NM", codes.join("-"), d / NM))
    }

    /// Actions de la page RECON. Renvoie un message éventuel.
    pub fn apply(&mut self, a: ReconAction, ac: &mut Aircraft, t: f64) {
        match a {
            ReconAction::Camera => self.cam_on = !self.cam_on,
            ReconAction::Mode => self.ir = !self.ir,
            ReconAction::Track => self.track = !self.track,
            ReconAction::Sar => {
                self.sar = match self.sar {
                    SarMode::Off => SarMode::Strip,
                    SarMode::Strip => SarMode::Spot,
                    SarMode::Spot => SarMode::Off,
                }
            }
            ReconAction::Elint => self.elint_on = !self.elint_on,
            ReconAction::Select(i) => self.selected = i,
            ReconAction::ZoomIn => self.zoom = (self.zoom + 1).min(ZOOMS.len() - 1),
            ReconAction::ZoomOut => self.zoom = self.zoom.saturating_sub(1),
            ReconAction::Photo => self.photo(ac, t),
            ReconAction::Page(p) => self.page = p,
            ReconAction::Image(i) => self.img_sel = Some(i),
            ReconAction::Steer => {
                if self.steer {
                    self.steer = false;
                    self.route_on = false;
                } else if let Err(e) = self.engage_steer(ac) {
                    self.events.push((Level::Caution, format!("RECON : {e}")));
                }
            }
            ReconAction::Route => {
                if self.route_on {
                    self.route_on = false;
                    self.steer = false;
                } else {
                    match self.start_route(ac) {
                        Ok(m) => self.events.push((Level::Info, m)),
                        Err(e) => self.events.push((Level::Caution, format!("RECON : {e}"))),
                    }
                }
            }
        }
    }

    /// Instructions texte du système de reconnaissance.
    pub fn command(&mut self, w: &[&str], ac: &mut Aircraft, t: f64) -> Result<String, String> {
        let arg = |i: usize| w.get(i).copied().unwrap_or("");
        match w[0] {
            "CAMERA" | "CAM" => {
                match arg(1) {
                    "ON" | "" => self.cam_on = true,
                    "OFF" => self.cam_on = false,
                    "IR" => self.ir = true,
                    "EO" | "VISIBLE" => self.ir = false,
                    "TRACK" | "POURSUITE" => self.track = true,
                    "NADIR" => self.track = false,
                    _ => return Err("CAMERA ON|OFF|EO|IR|TRACK|NADIR".into()),
                }
                Ok(format!("CAMERA {} {} {}", if self.cam_on { "ON" } else { "OFF" }, if self.ir { "IR" } else { "EO" }, if self.track { "TRACK" } else { "NADIR" }))
            }
            "ZOOM" => match arg(1).parse::<f64>().ok().and_then(|z| ZOOMS.iter().position(|v| *v >= z)) {
                Some(i) => {
                    self.zoom = i;
                    Ok(format!("ZOOM CAMERA x{:.0}", ZOOMS[i]))
                }
                None => Err("ZOOM 1, 2, 4, 8, 16 OU 32".into()),
            },
            "SAR" | "RADAR" => {
                self.sar = match arg(1) {
                    "OFF" => SarMode::Off,
                    "SPOT" => SarMode::Spot,
                    _ => SarMode::Strip,
                };
                Ok(format!("RADAR SAR {}", ["OFF", "STRIP", "SPOT"][self.sar as usize]))
            }
            "ELINT" => {
                self.elint_on = arg(1) != "OFF";
                Ok(format!("RECEPTEUR ELINT {}", if self.elint_on { "ON" } else { "OFF" }))
            }
            "CIBLE" | "TARGET" => match self.find(arg(1)) {
                Some(i) => {
                    self.selected = i;
                    let tg = &self.targets[i];
                    let (d, b) = dist_brg(ac.lat, ac.lon, tg.lat, tg.lon);
                    Ok(format!("CIBLE {} {} ({}) — {b:03.0} deg {:.0} NM", tg.code, tg.name, tg.kind_tag(), d / NM))
                }
                None => Err("CIBLE INCONNUE (A a K)".into()),
            },
            "DIRECT" | "DCT" | "DIR" => self.engage_steer(ac),
            "MENACES" | "THREATS" => {
                self.threats_on = arg(1) != "OFF";
                if !self.threats_on {
                    self.threat = None;
                }
                Ok(format!("MENACES SOL-AIR {}", if self.threats_on { "ACTIVES" } else { "DESACTIVEES" }))
            }
            "PHOTO" => {
                self.photo(ac, t);
                Ok("PRISE DE VUE".into())
            }
            "PLAN" => {
                let d = self.plan(ac);
                let codes: Vec<&str> = self.route.iter().map(|&i| self.targets[i].code).collect();
                Ok(format!("PLAN RECON {} — {:.0} NM", codes.join("-"), d / NM))
            }
            "ROUTE" => {
                if arg(2) == "OFF" || arg(1) == "OFF" {
                    self.route_on = false;
                    self.steer = false;
                    Ok("ROUTE RECON ARRETEE".into())
                } else {
                    self.start_route(ac)
                }
            }
            _ => Err("INSTRUCTION RECON INCONNUE".into()),
        }
    }
}

/// Triangulation par moindres carrés des relèvements ; renvoie (lat, lon, erreur estimée en m).
fn triangulate(b: &[(f64, f64, f64)]) -> Option<(f64, f64, f64)> {
    if b.len() < 3 {
        return None;
    }
    let (rlat, rlon) = (b[0].0, b[0].1);
    let (mut a11, mut a12, mut a22, mut b1, mut b2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut bmin, mut bmax) = (f64::MAX, f64::MIN);
    for &(la, lo, brg) in b {
        let (ox, oy) = enu_offset(rlat, rlon, la, lo);
        let r = brg.to_radians();
        let (dx, dy) = (r.sin(), r.cos());
        // Projecteur orthogonal à la direction du relèvement
        let (p11, p12, p22) = (1.0 - dx * dx, -dx * dy, 1.0 - dy * dy);
        a11 += p11;
        a12 += p12;
        a22 += p22;
        b1 += p11 * ox + p12 * oy;
        b2 += p12 * ox + p22 * oy;
        let rel = wrap180(brg - b[0].2);
        bmin = bmin.min(rel);
        bmax = bmax.max(rel);
    }
    let det = a11 * a22 - a12 * a12;
    if det.abs() < 1e-6 {
        return None;
    }
    let x = (a22 * b1 - a12 * b2) / det;
    let y = (a11 * b2 - a12 * b1) / det;
    let (la, lo) = from_enu(rlat, rlon, x, y);
    // Erreur : distance moyenne x bruit angulaire / (racine du nombre x étalement des relèvements)
    let mean_r = b.iter().map(|&(a, o, _)| dist_brg(a, o, la, lo).0).sum::<f64>() / b.len() as f64;
    let spread = (bmax - bmin).to_radians().sin().abs().max(0.03);
    let err = mean_r * ELINT_SIGMA_DEG.to_radians() / ((b.len() as f64).sqrt() * spread);
    Some((la, lo, err))
}

// ---------- Rendu de l'imagerie au sol ----------

/// Rend l'imagerie réelle centrée sur (lat, lon), largeur `width_m`, cap `hdg` en haut.
/// Style : 0 = visible, 1 = infrarouge (vert), 2 = SAR (niveaux de gris + chatoiement).
fn render_ground(tiles: &mut Tiles, lat: f64, lon: f64, width_m: f64, hdg: f64, img: Rect, style: u8, prio_boost: f32) {
    let px_per_m = img.w as f64 / width_m;
    let dpi = screen_dpi_scale() as f64;
    let coslat = lat.to_radians().cos().max(0.05);
    let z = ((156_543.03 * coslat * px_per_m * dpi).log2().round().clamp(1.0, Layer::Sat.max_z() as f64)) as u8;
    let m_per_tpx = 156_543.03 * coslat / tiles::n_tiles(z);
    let ts = (256.0 * m_per_tpx * px_per_m) as f32;
    let (fx, fy) = tiles::tile_xy(lat, lon, z);
    let ctr = vec2(img.x + img.w / 2.0, img.y + img.h / 2.0);
    let reach = ((img.w.hypot(img.h) / 2.0) / ts).ceil() as i64 + 1;
    let n = tiles::n_tiles(z) as i64;
    let tint = match style {
        1 => Color::new(0.55, 1.0, 0.6, 1.0),
        2 => Color::new(0.85, 0.85, 0.85, 1.0),
        _ => WHITE,
    };
    draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.05, 0.06, 0.06, 1.0));
    scissor(Some(img));
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let (tx, ty) = (fx.floor() as i64 + dx, fy.floor() as i64 + dy);
            if ty < 0 || ty >= n {
                continue;
            }
            let key = Key { layer: Layer::Sat, z, x: tx.rem_euclid(n) as u32, y: ty as u32 };
            let prio = z as f32 * 0.2 + (dx.abs() + dy.abs()) as f32 * 0.1 + prio_boost;
            if let Some(t) = tiles.get(key, prio) {
                let off = vec2((tx as f64 - fx) as f32 * ts, (ty as f64 - fy) as f32 * ts);
                let (tw, th) = (t.tex.width(), t.tex.height());
                draw_texture_ex(
                    t.tex,
                    ctr.x + off.x,
                    ctr.y + off.y,
                    tint,
                    DrawTextureParams {
                        dest_size: Some(vec2(ts + 1.0, ts + 1.0)),
                        source: Some(Rect::new(t.uv.x * tw, t.uv.y * th, t.uv.w * tw, t.uv.h * th)),
                        rotation: -hdg as f32,
                        pivot: Some(ctr),
                        ..Default::default()
                    },
                );
            }
        }
    }
    if style == 2 {
        // SAR : désaturation et chatoiement (speckle) déterministe
        draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.5, 0.5, 0.5, 0.35));
        let mut h: u32 = (lat * 1e5) as i64 as u32 ^ ((lon * 1e5) as i64 as u32).rotate_left(16);
        let cell = (img.w / 70.0).max(2.0);
        for _ in 0..((img.w * img.h) / (cell * cell * 3.0)) as usize {
            h ^= h << 13;
            h ^= h >> 17;
            h ^= h << 5;
            let x = img.x + (h % 10_000) as f32 / 10_000.0 * img.w;
            let y = img.y + ((h >> 14) % 10_000) as f32 / 10_000.0 * img.h;
            let b = if h & 1 == 0 { 0.0 } else { 1.0 };
            draw_rectangle(x, y, cell, cell, Color::new(b, b, b, 0.22));
        }
    }
    scissor(None);
}

/// Volet caméra de l'écran Darkstar : image réelle au point visé, cap en haut.
pub fn draw_camera(ac: &Aircraft, rc: &Recon, tiles: &mut Tiles, r: Rect) -> Option<CamAction> {
    let mut out = None;
    let grey = Color::new(0.45, 0.47, 0.5, 1.0);
    let ax = r.x + r.w - 26.0;
    let up = Rect::new(ax - 12.0, r.y + 28.0, 24.0, 22.0);
    let dn = Rect::new(ax - 12.0, r.y + 72.0, 24.0, 22.0);
    let on = rc.cam_on && Recon::sensors_ok(ac);
    let zoom = ZOOMS[rc.zoom];
    let img = Rect::new(r.x + 6.0, r.y + 26.0, r.w - 50.0, r.h - 46.0);
    if on {
        let (la, lo, off) = rc.los(ac);
        let w_m = rc.footprint(ac);
        render_ground(tiles, la, lo, w_m, ac.psi, img, rc.ir as u8, -2.0);
        let px_per_m = img.w as f64 / w_m;
        let ctr = vec2(img.x + img.w / 2.0, img.y + img.h / 2.0);
        scissor(Some(img));
        for t in &rc.targets {
            let (e, n) = enu_offset(la, lo, t.lat, t.lon);
            let (s, c) = (ac.psi.sin(), ac.psi.cos());
            let (xr, yr) = (e * c - n * s, e * s + n * c);
            let p = ctr + vec2((xr * px_per_m) as f32, (-yr * px_per_m) as f32);
            if img.contains(p) && !matches!(t.kind, Kind::Elint(_)) {
                let col = if t.done() { GREEN_S } else { AMBER };
                stroke(&round_corners(&[p + vec2(-12.0, -12.0), p + vec2(12.0, -12.0), p + vec2(12.0, 12.0), p + vec2(-12.0, 12.0)], true, 4.0), true, 1.5, col);
                txt(t.code, p.x + 15.0, p.y - 8.0, 13.0, col);
            }
        }
        let rc_col = Color::new(1.0, 1.0, 1.0, 0.85);
        for (a, b) in [(vec2(-30.0, 0.0), vec2(-8.0, 0.0)), (vec2(8.0, 0.0), vec2(30.0, 0.0)), (vec2(0.0, -30.0), vec2(0.0, -8.0)), (vec2(0.0, 8.0), vec2(0.0, 30.0))] {
            round_line(ctr + a, ctr + b, 1.5, rc_col);
        }
        let bar_m = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0].iter().copied().find(|m| m * px_per_m > 50.0).unwrap_or(10_000.0);
        let bw = (bar_m * px_per_m) as f32;
        let (bx, by) = (img.x + 10.0, img.y + img.h - 10.0);
        round_line(vec2(bx, by), vec2(bx + bw, by), 2.0, WHITE);
        txt(&if bar_m >= 1000.0 { format!("{:.0} km", bar_m / 1000.0) } else { format!("{bar_m:.0} m") }, bx, by - 5.0, 11.0, WHITE);
        scissor(None);
        let (q, why) = rc.niirs(ac);
        txt(&format!("{} {} x{zoom:.0}  GSD {:.2} m  NIIRS {q:.1}", if rc.ir { "IR" } else { "EO" }, if off > 0.0 { "TRACK" } else { "NADIR" }, rc.gsd(ac)), r.x + 10.0, r.y + 17.0, 12.0, if why.is_some() { AMBER } else { GREEN_S });
        if let Some(w) = why {
            txt_r(w, img.x + img.w - 6.0, img.y + 16.0, 11.0, AMBER);
        }
        if off > 0.0 {
            txt_r(&format!("DEP {:.0} deg  OBL {:.0} km", off.to_degrees(), rc.slant(ac) / 1000.0), img.x + img.w - 6.0, img.y + 30.0, 11.0, CYAN_S);
        }
        draw_rectangle_lines(img.x, img.y, img.w, img.h, 1.0, Color::new(0.5, 0.52, 0.55, 1.0));
    } else {
        txt_c("Camera Zoom Elevation :N/A", r.x + r.w / 2.0 - 10.0, r.y + 17.0, 13.0, WHITE);
        txt_c("Camera unavailable", r.x + r.w / 2.0 - 8.0, r.y + r.h / 2.0 + 6.0, 18.0, WHITE);
        let why = if !ac.elec.bus_ok { "BUS ELEC PERDU" } else if ac.on_ground { "AU SOL" } else { "CAMERA ON pour l'activer" };
        txt_c(why, r.x + r.w / 2.0 - 8.0, r.y + r.h / 2.0 + 26.0, 11.0, DIM);
    }
    draw_triangle(vec2(ax, up.y + 2.0), vec2(ax - 10.0, up.y + 20.0), vec2(ax + 10.0, up.y + 20.0), if hovered(up) { WHITE } else { grey });
    txt_c(&format!("x{zoom:.0}"), ax, r.y + 64.0, 11.0, WHITE);
    draw_triangle(vec2(ax, dn.y + 20.0), vec2(ax - 10.0, dn.y + 2.0), vec2(ax + 10.0, dn.y + 2.0), if hovered(dn) { WHITE } else { grey });
    if clicked(up) {
        out = Some(CamAction::ZoomIn);
    }
    if clicked(dn) {
        out = Some(CamAction::ZoomOut);
    }
    out
}

// ---------- Carte ----------

/// Objectifs, route planifiée, émetteurs (domaines, positions estimées), empreintes des capteurs.
pub fn draw_map(rc: &Recon, ac: &Aircraft, to_s: &dyn Fn(f64, f64) -> Vec2, z: f32, r: Rect) {
    let m_per_px = 111_195.0 / z as f64;
    // Domaines d'engagement des SAM localisés ou entendus
    for em in &rc.emitters {
        if !em.heard {
            continue;
        }
        let (la, lo) = em.est.map(|e| (e.0, e.1)).unwrap_or((em.lat, em.lon));
        let p = to_s(la, lo);
        if em.kind == EmKind::Sam {
            let rad = (em.range / m_per_px) as f32 / la.to_radians().cos() as f32;
            if rad > 4.0 {
                draw_circle(p.x, p.y, rad, Color::new(1.0, 0.15, 0.1, 0.07));
                draw_circle_lines(p.x, p.y, rad, 1.5, Color::new(1.0, 0.25, 0.2, 0.7));
            }
        }
        if let Some(e) = em.est {
            let er = ((e.2 / m_per_px) as f32).clamp(3.0, 200.0);
            draw_circle_lines(p.x, p.y, er, 1.0, if em.localized { GREEN_S } else { AMBER });
        }
        // Derniers relèvements
        for &(bla, blo, b) in em.bearings.iter().rev().take(6) {
            let a = to_s(bla, blo);
            let (ela, elo) = destination(bla, blo, b, 250_000.0);
            let e = to_s(ela, elo);
            if (e.x - a.x).abs() < r.w {
                draw_line(a.x, a.y, e.x, e.y, 1.0, Color::new(1.0, 0.6, 0.1, 0.25));
            }
        }
        let c = if em.kind == EmKind::Sam { RED } else { AMBER };
        draw_triangle(p + vec2(0.0, -7.0), p + vec2(6.0, 5.0), p + vec2(-6.0, 5.0), c);
        if z > 25.0 {
            txt(em.name, p.x + 9.0, p.y + 4.0, 11.0, c);
        }
    }
    // Route planifiée
    if !rc.route.is_empty() {
        let mut prev = to_s(ac.lat, ac.lon);
        for &i in &rc.route {
            let t = &rc.targets[i];
            let p = to_s(t.lat, t.lon);
            if (p.x - prev.x).abs() < r.w {
                draw_line(prev.x, prev.y, p.x, p.y, 1.5, Color::new(1.0, 0.85, 0.2, if rc.route_on { 0.8 } else { 0.35 }));
            }
            prev = p;
        }
    }
    // Objectifs
    for (i, t) in rc.targets.iter().enumerate() {
        let p = to_s(t.lat, t.lon);
        if !r.contains(p) {
            continue;
        }
        let c = if t.done() { GREEN_S } else if t.best.is_some() { AMBER } else { GOLD };
        let d = 7.0;
        let shape: Vec<Vec2> = match t.kind {
            Kind::Imint(_) => vec![p + vec2(0.0, -d), p + vec2(d, 0.0), p + vec2(0.0, d), p + vec2(-d, 0.0)],
            Kind::Sar => vec![p + vec2(-d, -d), p + vec2(d, -d), p + vec2(d, d), p + vec2(-d, d)],
            Kind::Elint(_) => (0..6).map(|k| p + vec2((k as f32 * 1.047).cos(), (k as f32 * 1.047).sin()) * d).collect(),
        };
        stroke(&round_corners(&shape, true, 2.0), true, 2.0, c);
        if i == rc.selected {
            draw_circle_lines(p.x, p.y, 12.0, 1.5, MAGENTA_S);
        }
        if z > 8.0 {
            txt(&format!("{} {}", t.code, if z > 40.0 { t.name } else { "" }), p.x + 10.0, p.y + 4.0, 12.0, c);
        }
    }
    if ac.on_ground {
        return;
    }
    // Empreinte caméra
    if rc.cam_on {
        let (la, lo, _) = rc.los(ac);
        let half = rc.footprint(ac) / 2.0;
        let corners: Vec<Vec2> = [(-1.0f64, -1.0f64), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .iter()
            .map(|&(a, b)| {
                let (pla, plo) = from_enu(la, lo, half * (a * ac.psi.cos() + b * ac.psi.sin()), half * (-a * ac.psi.sin() + b * ac.psi.cos()));
                to_s(pla, plo)
            })
            .collect();
        stroke(&corners, true, 1.5, Color::new(0.4, 1.0, 0.6, 0.9));
        if rc.los(ac).2 > 0.0 {
            let (a, b) = (to_s(ac.lat, ac.lon), to_s(la, lo));
            draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.4, 1.0, 0.6, 0.5));
        }
    }
    // Fauchées SAR
    let band = |near: f64, far: f64, half_len: f64, side: f64| -> Vec<Vec2> {
        [(-half_len, near), (half_len, near), (half_len, far), (-half_len, far)]
            .iter()
            .map(|&(along, cross)| {
                let cr = cross * side;
                let (e, n) = (along * ac.psi.sin() + cr * ac.psi.cos(), along * ac.psi.cos() - cr * ac.psi.sin());
                let (la, lo) = from_enu(ac.lat, ac.lon, e, n);
                to_s(la, lo)
            })
            .collect()
    };
    match rc.sar {
        SarMode::Strip => {
            for side in [-1.0, 1.0] {
                let pts = band(SAR_NEAR, SAR_FAR, 20_000.0, side);
                fill_shape(&pts, Color::new(0.3, 0.8, 1.0, 0.10));
                stroke(&pts, true, 1.0, Color::new(0.3, 0.8, 1.0, 0.5));
            }
        }
        SarMode::Spot => {
            let t = &rc.targets[rc.selected];
            let (a, b) = (to_s(ac.lat, ac.lon), to_s(t.lat, t.lon));
            draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.3, 0.8, 1.0, 0.6));
            draw_circle_lines(b.x, b.y, 9.0, 1.5, Color::new(0.3, 0.8, 1.0, 0.8));
        }
        SarMode::Off => {}
    }
}

// ---------- Page RECON ----------

fn button(r: Rect, label: &str, value: &str, on: bool) -> bool {
    rrect(r, 6.0, if on { Color::new(0.07, 0.25, 0.14, 1.0) } else if hovered(r) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
    rrect_lines(r, 6.0, 1.2, if on { GREEN_S } else { PANEL_BORDER });
    txt(label, r.x + 10.0, r.y + r.h / 2.0 + 5.0, 12.0, WHITE);
    txt_r(value, r.x + r.w - 10.0, r.y + r.h / 2.0 + 5.0, 12.0, if on { GREEN_S } else { DIM });
    clicked(r)
}

fn bar(r: Rect, f: f64, c: Color, label: &str) {
    rrect(r, 4.0, Color::new(0.06, 0.07, 0.08, 1.0));
    if f > 0.005 {
        rrect(Rect::new(r.x, r.y, r.w * f.clamp(0.0, 1.0) as f32, r.h), 4.0, c);
    }
    txt_c(label, r.x + r.w / 2.0, r.y + r.h - 3.0, 11.0, WHITE);
}

pub fn draw_recon(ac: &Aircraft, rc: &Recon, tiles: &mut Tiles, r: Rect, link_name: &str) -> Option<ReconAction> {
    let mut out = None;
    panel(r, "RECON - SYSTEME DE RECONNAISSANCE");
    let (sc, tot) = rc.score();
    txt_r(&format!("SCORE {sc} / {tot}"), r.x + r.w - 12.0, r.y + 19.0, 14.0, if sc == tot { GREEN_S } else { GOLD });
    // Sous-pages
    let pages = ["CAPTEURS", "OBJECTIFS", "ELINT / MENACES", "PHOTOTHEQUE"];
    let pw = (r.w - 24.0) / pages.len() as f32;
    for (i, p) in pages.iter().enumerate() {
        let b = Rect::new(r.x + 12.0 + i as f32 * pw, r.y + 28.0, pw - 4.0, 22.0);
        let on = rc.page == i;
        rrect(b, 5.0, if on { Color::new(0.10, 0.16, 0.22, 1.0) } else { Color::new(0.06, 0.075, 0.095, 1.0) });
        if on {
            draw_rectangle(b.x + 8.0, b.y + b.h - 2.0, b.w - 16.0, 2.0, ACCENT);
        }
        txt_c(p, b.x + b.w / 2.0, b.y + 15.0, 12.0, if on { WHITE } else { DIM });
        if clicked(b) {
            out = Some(ReconAction::Page(i));
        }
    }
    if let Some((k, t_in, missile)) = rc.threat {
        let msg = match missile {
            Some(m) => format!("MISSILE EN VOL — {} — IMPACT {:.0} s", rc.emitters[k].name, (MISSILE_TOF - m).max(0.0)),
            None => format!("DOMAINE SAM {} — TIR DANS {:.0} s", rc.emitters[k].name, (12.0 - t_in).max(0.0)),
        };
        let b = Rect::new(r.x + 12.0, r.y + 54.0, r.w - 24.0, 20.0);
        rrect(b, 5.0, Color::new(0.35, 0.03, 0.03, 1.0));
        txt_c(&msg, b.x + b.w / 2.0, b.y + 15.0, 13.0, WHITE);
    }
    let body = Rect::new(r.x + 12.0, r.y + 80.0, r.w - 24.0, r.h - 92.0);
    let act = match rc.page {
        0 => page_sensors(ac, rc, body, link_name),
        1 => page_targets(ac, rc, body),
        2 => page_elint(ac, rc, body),
        _ => page_images(rc, tiles, body),
    };
    out.or(act)
}

fn page_sensors(ac: &Aircraft, rc: &Recon, b: Rect, link_name: &str) -> Option<ReconAction> {
    let mut out = None;
    let lw = (b.w * 0.5 - 8.0).min(340.0);
    let x = b.x;
    let mut y = b.y;
    let bh = 24.0;
    let half = (lw - 6.0) / 2.0;
    if button(Rect::new(x, y, lw, bh), "CAMERA EO/IR (tourelle)", if rc.cam_on { "ON" } else { "OFF" }, rc.cam_on) {
        out = Some(ReconAction::Camera);
    }
    y += bh + 5.0;
    if button(Rect::new(x, y, half, bh), "SPECTRE", if rc.ir { "IR" } else { "EO" }, rc.ir) {
        out = Some(ReconAction::Mode);
    }
    if button(Rect::new(x + half + 6.0, y, half, bh), "POINTAGE", if rc.track { "TRACK" } else { "NADIR" }, rc.track) {
        out = Some(ReconAction::Track);
    }
    y += bh + 5.0;
    let zr = Rect::new(x, y, half, bh);
    rrect(zr, 6.0, Color::new(0.08, 0.09, 0.11, 1.0));
    rrect_lines(zr, 6.0, 1.2, PANEL_BORDER);
    txt_c(&format!("ZOOM x{:.0}", ZOOMS[rc.zoom]), zr.x + zr.w / 2.0, zr.y + 17.0, 12.0, WHITE);
    let (zm, zp) = (Rect::new(zr.x, zr.y, 24.0, bh), Rect::new(zr.x + zr.w - 24.0, zr.y, 24.0, bh));
    txt_c("-", zm.x + 12.0, zm.y + 17.0, 15.0, if hovered(zm) { WHITE } else { DIM });
    txt_c("+", zp.x + 12.0, zp.y + 17.0, 15.0, if hovered(zp) { WHITE } else { DIM });
    if clicked(zm) {
        out = Some(ReconAction::ZoomOut);
    } else if clicked(zp) {
        out = Some(ReconAction::ZoomIn);
    }
    if button(Rect::new(x + half + 6.0, y, half, bh), "PRISE DE VUE", "PHOTO", false) {
        out = Some(ReconAction::Photo);
    }
    y += bh + 5.0;
    if button(Rect::new(x, y, lw, bh), "RADAR SAR", ["OFF", "STRIP", "SPOT"][rc.sar as usize], rc.sar_on()) {
        out = Some(ReconAction::Sar);
    }
    y += bh + 5.0;
    if button(Rect::new(x, y, lw, bh), "RECEPTEUR ELINT", if rc.elint_on { "ON" } else { "OFF" }, rc.elint_on) {
        out = Some(ReconAction::Elint);
    }
    // Colonne droite : performances
    let rx = b.x + lw + 16.0;
    let rw = b.x + b.w - rx;
    let mut ry = b.y + 12.0;
    let line = |y: &mut f32, l: &str, v: String, c: Color| {
        txt(l, rx, *y, 12.0, DIM);
        txt_r(&v, rx + rw, *y, 12.0, c);
        *y += 17.0;
    };
    let (q, why) = rc.niirs(ac);
    let (_, _, off) = rc.los(ac);
    line(&mut ry, "LARGEUR IMAGE", format!("{:.2} km", rc.footprint(ac) / 1000.0), WHITE);
    line(&mut ry, "RESOLUTION (GSD)", format!("{:.2} m", rc.gsd(ac)), WHITE);
    line(&mut ry, "DEPOINTAGE / DISTANCE", format!("{:.0} deg / {:.0} km", off.to_degrees(), rc.slant(ac) / 1000.0), if off > 0.0 { CYAN_S } else { WHITE });
    line(&mut ry, "QUALITE CAMERA", format!("NIIRS {q:.1}"), if why.is_some() { AMBER } else { GREEN_S });
    if let Some(w) = why {
        line(&mut ry, "DEGRADATION", w.into(), AMBER);
    }
    line(&mut ry, "SAR STRIP / SPOT", format!("NIIRS {SAR_STRIP_NIIRS:.1} / {SAR_SPOT_NIIRS:.1}"), if rc.sar_on() { CYAN_S } else { DIM });
    line(&mut ry, "HORIZON RADAR", format!("{:.0} NM", radar_horizon(ac.alt, 15.0) / NM), WHITE);
    line(&mut ry, "ALIMENTATION", if ac.elec.bus_ok { "BUS OK".into() } else { "PERDUE".into() }, if ac.elec.bus_ok { GREEN_S } else { RED });
    // Stockage / transmission
    let by = b.y + b.h - 44.0;
    let (st, sent) = (rc.stored_gb(), rc.sent_gb());
    bar(Rect::new(b.x, by, b.w, 16.0), st / STORAGE_GB, Color::new(0.2, 0.45, 0.7, 1.0), &format!("MEMOIRE {st:.0} / {STORAGE_GB:.0} Go  ({} images)", rc.shots.len()));
    let f = if st > 0.0 { sent / st } else { 1.0 };
    bar(Rect::new(b.x, by + 22.0, b.w, 16.0), f, Color::new(0.2, 0.6, 0.35, 1.0), &format!("TRANSMIS AU SOL {:.0} %  — LIAISON {link_name}", f * 100.0));
    out
}

fn page_targets(ac: &Aircraft, rc: &Recon, b: Rect) -> Option<ReconAction> {
    let mut out = None;
    let rh = ((b.h - 44.0) / rc.targets.len() as f32).clamp(18.0, 32.0);
    let mut y = b.y;
    for (i, t) in rc.targets.iter().enumerate() {
        let row = Rect::new(b.x, y, b.w, rh - 3.0);
        let sel = i == rc.selected;
        rrect(row, 5.0, if sel { Color::new(0.10, 0.16, 0.22, 1.0) } else if hovered(row) { Color::new(0.08, 0.10, 0.13, 1.0) } else { Color::new(0.06, 0.07, 0.09, 1.0) });
        if sel {
            rrect_lines(row, 5.0, 1.2, ACCENT);
        }
        let cy = row.y + row.h / 2.0 + 4.0;
        let st_col = if t.done() { GREEN_S } else if t.best.is_some() { AMBER } else { DIM };
        draw_circle(row.x + 12.0, row.y + row.h / 2.0, 7.0, st_col);
        txt_c(t.code, row.x + 12.0, cy, 11.0, BLACK);
        txt(&format!("P{}", t.prio), row.x + 24.0, cy, 11.0, if t.prio == 1 { RED } else if t.prio == 2 { AMBER } else { DIM });
        txt(t.kind_tag(), row.x + 48.0, cy, 11.0, CYAN_S);
        txt(t.name, row.x + 96.0, cy, 11.0, WHITE);
        let (d, brg) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let status = match (t.best, t.kind) {
            (Some((_, "ELINT", _)), _) => "LOCALISE".to_string(),
            (Some((q, s, _)), _) => format!("{s} {q:.1}"),
            (None, Kind::Imint(req)) => format!("{brg:03.0}/{:.0}NM  NIIRS {req:.0}", d / NM),
            (None, _) => format!("{brg:03.0}/{:.0}NM", d / NM),
        };
        txt_r(&status, row.x + row.w - 8.0, cy, 11.0, st_col);
        if clicked(row) {
            out = Some(ReconAction::Select(i));
        }
        y += rh;
    }
    let half = (b.w - 6.0) / 2.0;
    let s1 = Rect::new(b.x, b.y + b.h - 34.0, half, 30.0);
    let s2 = Rect::new(b.x + half + 6.0, b.y + b.h - 34.0, half, 30.0);
    for (rb, on, lbl) in [
        (s1, rc.steer && !rc.route_on, if rc.steer && !rc.route_on { format!("GUIDAGE CIBLE {} ACTIF", rc.targets[rc.selected].code) } else { format!("DIRECT CIBLE {}", rc.targets[rc.selected].code) }),
        (s2, rc.route_on, if rc.route_on { "ROUTE RECON ACTIVE".to_string() } else { "PLANIFIER + SUIVRE LA ROUTE".to_string() }),
    ] {
        rrect(rb, 6.0, if on { Color::new(0.3, 0.1, 0.3, 1.0) } else if hovered(rb) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
        rrect_lines(rb, 6.0, 1.2, MAGENTA_S);
        txt_c(&lbl, rb.x + rb.w / 2.0, rb.y + 20.0, 12.0, MAGENTA_S);
    }
    if clicked(s1) {
        out = Some(ReconAction::Steer);
    }
    if clicked(s2) {
        out = Some(ReconAction::Route);
    }
    out
}

fn page_elint(ac: &Aircraft, rc: &Recon, b: Rect) -> Option<ReconAction> {
    let mut y = b.y + 4.0;
    let heard: Vec<&Emitter> = rc.emitters.iter().filter(|e| e.heard).collect();
    if heard.is_empty() {
        txt("AUCUN EMETTEUR DETECTE", b.x, y + 14.0, 13.0, DIM);
        txt("Le recepteur ELINT entend les radars dans l'horizon radar (plus haut = plus loin).", b.x, y + 34.0, 11.0, DIM);
        txt(&format!("Horizon radar actuel : {:.0} NM", radar_horizon(ac.alt, 15.0) / NM), b.x, y + 50.0, 11.0, WHITE);
        return None;
    }
    let rh = ((b.h - 60.0) / heard.len().max(1) as f32).clamp(34.0, 52.0);
    for em in heard {
        let row = Rect::new(b.x, y, b.w, rh - 4.0);
        rrect(row, 5.0, Color::new(0.06, 0.07, 0.09, 1.0));
        let c = if em.kind == EmKind::Sam { RED } else { AMBER };
        rrect_lines(row, 5.0, 1.0, Color::new(c.r, c.g, c.b, 0.6));
        let kind = match em.kind {
            EmKind::Search => "RADAR DE VEILLE",
            EmKind::FireControl => "CONDUITE DE TIR",
            EmKind::Sam => "SYSTEME SOL-AIR",
        };
        txt(em.name, row.x + 10.0, row.y + 16.0, 12.0, c);
        txt(&format!("{kind}  BANDE {}", em.band), row.x + 10.0, row.y + 31.0, 11.0, DIM);
        let (d, brg) = dist_brg(ac.lat, ac.lon, em.lat, em.lon);
        let status = match em.est {
            Some(e) if em.localized => format!("LOCALISE +/- {:.0} m", e.2),
            Some(e) => format!("{} RELEVEMENTS  +/- {:.1} km", em.bearings.len(), e.2 / 1000.0),
            None => format!("{} RELEVEMENTS  REL {brg:03.0}", em.bearings.len()),
        };
        txt_r(&status, row.x + row.w - 10.0, row.y + 16.0, 12.0, if em.localized { GREEN_S } else { WHITE });
        if em.kind == EmKind::Sam {
            let inside = d < em.range;
            txt_r(
                &format!("DOMAINE {:.0} km  PLAFOND {:.0} ft  M<{:.1}{}", em.range / 1000.0, em.ceiling_ft, em.mach_max, if inside { "  — DEDANS" } else { "" }),
                row.x + row.w - 10.0,
                row.y + 31.0,
                11.0,
                if inside { RED } else { DIM },
            );
        }
        y += rh;
    }
    txt("Triangulation : varier le relevement (virer ou passer a cote) reduit l'erreur plus vite.", b.x, b.y + b.h - 6.0, 11.0, DIM);
    None
}

fn page_images(rc: &Recon, tiles: &mut Tiles, b: Rect) -> Option<ReconAction> {
    let mut out = None;
    if rc.shots.is_empty() {
        txt("PHOTOTHEQUE VIDE", b.x, b.y + 14.0, 13.0, DIM);
        txt("Les images se prennent automatiquement au-dessus des objectifs (ou bouton PRISE DE VUE).", b.x, b.y + 34.0, 11.0, DIM);
        return None;
    }
    let sel = rc.img_sel.unwrap_or(rc.shots.len() - 1).min(rc.shots.len() - 1);
    // Grande image à droite
    let gw = (b.w * 0.55).min(b.h - 40.0);
    let big = Rect::new(b.x + b.w - gw, b.y, gw, gw);
    let s = &rc.shots[sel];
    let style = match s.sensor {
        "IR" => 1,
        x if x.starts_with("SAR") => 2,
        _ => 0,
    };
    if s.sensor == "ELINT" {
        draw_rectangle(big.x, big.y, big.w, big.h, Color::new(0.03, 0.05, 0.04, 1.0));
        txt_c("PRODUIT ELINT : LOCALISATION", big.x + big.w / 2.0, big.y + big.h / 2.0, 13.0, GREEN_S);
    } else {
        render_ground(tiles, s.lat, s.lon, s.width, s.hdg, big, style, -1.0);
    }
    draw_rectangle_lines(big.x, big.y, big.w, big.h, 1.0, Color::new(0.6, 0.62, 0.65, 1.0));
    let tgt = s.target.map(|i| format!("CIBLE {} {}", rc.targets[i].code, rc.targets[i].name)).unwrap_or_else(|| "PRISE DE VUE LIBRE".into());
    txt(&tgt, big.x, big.y + big.h + 14.0, 12.0, WHITE);
    txt(
        &format!("{}  NIIRS {:.1}  {:.2} km  T+{}  {}", s.sensor, s.q, s.width / 1000.0, crate::mission::fmt_time(s.t), if s.sent >= s.gb { "TRANSMISE" } else { "EN ATTENTE" }),
        big.x,
        big.y + big.h + 30.0,
        11.0,
        if s.sent >= s.gb { GREEN_S } else { AMBER },
    );
    // Vignettes à gauche (les plus récentes)
    let gx_w = big.x - b.x - 10.0;
    let cols = 3usize;
    let tw = (gx_w - (cols as f32 - 1.0) * 6.0) / cols as f32;
    let rows = ((b.h + 6.0) / (tw + 6.0)).floor().max(1.0) as usize;
    let n = (cols * rows).min(rc.shots.len());
    for k in 0..n {
        let i = rc.shots.len() - 1 - k;
        let sh = &rc.shots[i];
        let cell = Rect::new(b.x + (k % cols) as f32 * (tw + 6.0), b.y + (k / cols) as f32 * (tw + 6.0), tw, tw);
        if sh.sensor == "ELINT" {
            draw_rectangle(cell.x, cell.y, cell.w, cell.h, Color::new(0.03, 0.06, 0.04, 1.0));
            txt_c("ELINT", cell.x + cell.w / 2.0, cell.y + cell.h / 2.0, 11.0, GREEN_S);
        } else {
            let st = match sh.sensor {
                "IR" => 1,
                x if x.starts_with("SAR") => 2,
                _ => 0,
            };
            render_ground(tiles, sh.lat, sh.lon, sh.width, sh.hdg, cell, st, 0.0);
        }
        draw_rectangle_lines(cell.x, cell.y, cell.w, cell.h, if i == sel { 2.0 } else { 1.0 }, if i == sel { ACCENT } else { Color::new(0.4, 0.42, 0.45, 1.0) });
        let lbl = sh.target.map(|t| rc.targets[t].code).unwrap_or("-");
        draw_rectangle(cell.x, cell.y + cell.h - 14.0, cell.w, 14.0, Color::new(0.0, 0.0, 0.0, 0.6));
        txt(&format!("{lbl} {}", sh.sensor), cell.x + 3.0, cell.y + cell.h - 3.0, 10.0, WHITE);
        if clicked(cell) {
            out = Some(ReconAction::Image(i));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn over(lat: f64, lon: f64, alt_ft: f64) -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.lat = lat;
        ac.lon = lon;
        ac.alt = alt_ft * FT;
        ac
    }

    /// Survol à la verticale avec caméra : cible collectée et image dans la photothèque.
    #[test]
    fn camera_collects_target() {
        let mut rc = Recon::new();
        let t = &rc.targets[0];
        let mut ac = over(t.lat, t.lon, 80_000.0);
        rc.cam_on = true;
        rc.zoom = 4;
        rc.update(0.1, &mut ac, 10.0, true);
        assert!(rc.targets[0].done(), "{:?}", rc.targets[0].best);
        assert_eq!(rc.shots.len(), 1);
    }

    /// Poursuite : la tourelle vise l'objectif à 20 km à côté de la verticale.
    #[test]
    fn gimbal_tracks_offset_target() {
        let mut rc = Recon::new();
        let t = &rc.targets[1];
        let (la, lo) = from_enu(t.lat, t.lon, -20_000.0, 0.0);
        let mut ac = over(la, lo, 80_000.0);
        rc.cam_on = true;
        rc.selected = 1;
        rc.zoom = 3;
        let (_, _, off) = rc.los(&ac);
        assert!(off.to_degrees() > 30.0);
        rc.update(0.1, &mut ac, 10.0, true);
        assert!(rc.targets[1].best.is_some());
    }

    /// SAR STRIP : objectif SAR à 30 km sur le côté.
    #[test]
    fn sar_strip_side_looking() {
        let mut rc = Recon::new();
        let t = &rc.targets[4];
        let (la, lo) = from_enu(t.lat, t.lon, -30_000.0, 0.0);
        let mut ac = over(la, lo, 75_000.0);
        ac.psi = 0.0;
        rc.sar = SarMode::Strip;
        rc.update(0.1, &mut ac, 10.0, true);
        assert!(rc.targets[4].done());
    }

    /// ELINT : une passe latérale autour d'un émetteur suffit à le localiser.
    #[test]
    fn elint_triangulation() {
        let mut rc = Recon::new();
        let em = &rc.emitters[3];
        let (la, lo) = from_enu(em.lat, em.lon, -40_000.0, -60_000.0);
        let mut ac = over(la, lo, 80_000.0);
        ac.psi = 0.0;
        for _ in 0..120 {
            // vol vers le nord à Mach 3 environ (900 m/s), pas de 1 s
            let (n_la, n_lo) = destination(ac.lat, ac.lon, 0.0, 900.0);
            ac.lat = n_la;
            ac.lon = n_lo;
            rc.update(1.0, &mut ac, 0.0, true);
        }
        let em = &rc.emitters[3];
        let e = em.est.expect("estimation");
        let err = dist_brg(e.0, e.1, em.lat, em.lon).0;
        println!("ELINT : {} relevements, erreur reelle {:.0} m, estimee {:.0} m", em.bearings.len(), err, e.2);
        assert!(em.localized && err < 3_000.0);
    }

    /// Menace : rester dans le domaine du SAM moyenne portée à basse altitude = abattu.
    #[test]
    fn sam_shoots_down_slow_low_aircraft() {
        let mut rc = Recon::new();
        let em = &rc.emitters[2];
        let (la, lo) = from_enu(em.lat, em.lon, 20_000.0, 0.0);
        let mut ac = over(la, lo, 30_000.0);
        for _ in 0..60 {
            rc.update(1.0, &mut ac, 0.0, true);
        }
        assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("MISSILE")));
        // À Mach 6 et 100 000 ft : hors domaine
        let mut rc = Recon::new();
        let mut ac = over(la, lo, 100_000.0);
        ac.tas = 6.0 * crate::atmosphere::isa(ac.alt).a;
        ac.d.mach = 6.0;
        for _ in 0..60 {
            rc.update(1.0, &mut ac, 0.0, true);
        }
        assert!(ac.crashed.is_none());
    }
}
