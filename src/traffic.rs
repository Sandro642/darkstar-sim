//! Trafic aérien autour du Darkstar : avions de ligne (types et indicatifs réels de compagnies) et
//! trafic militaire (chasseurs en patrouille, ravitailleur et AWACS en hippodrome, transports,
//! reconnaissance haute altitude). Position relative, distance, relèvement, rapprochement, aspect,
//! identification (AMI / NEUTRE / INCONNU) ; verrouillage d'une cible (fiche détaillée), affichage
//! sur le ND et sur l'horizon (mode A/A : losanges, cadre de désignation, repère de direction).

use crate::aircraft::*;
use crate::geo::*;
use crate::ui::*;
use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Iff {
    Ami,
    Neutre,
    Inconnu,
}

impl Iff {
    pub fn label(self) -> &'static str {
        match self {
            Iff::Ami => "AMI",
            Iff::Neutre => "NEUTRE",
            Iff::Inconnu => "INCONNU",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Iff::Ami => GREEN_S,
            Iff::Neutre => WHITE,
            Iff::Inconnu => AMBER,
        }
    }
}

/// Comportement d'un trafic.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Behave {
    /// Route rectiligne (changements de cap rares)
    Route,
    /// Hippodrome / orbite autour d'un point
    Orbit { lat: f64, lon: f64, radius: f64 },
    /// Patrouille : virages fréquents
    Patrol,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: u32,
    pub call: String,
    pub icao: &'static str,
    pub name: &'static str,
    pub operator: &'static str,
    pub mission: &'static str,
    pub mil: bool,
    pub iff: Iff,
    pub squawk: u16,
    pub lat: f64,
    pub lon: f64,
    /// Altitude (m), vitesse vraie (m/s), cap (deg), vitesse verticale (m/s)
    pub alt: f64,
    pub tas: f64,
    pub hdg: f64,
    pub vs: f64,
    alt_tgt: f64,
    hdg_tgt: f64,
    behave: Behave,
    turn_t: f64,
    pub org: String,
    pub dest: String,
    /// Surface équivalente radar (m²)
    pub rcs: f64,
    /// Âge de la dernière détection radar (s) ; creux Doppler, dans le volume de balayage
    pub radar_age: f64,
    pub notch: bool,
    pub in_vol: bool,
    /// Vitesse radiale de la cible par rapport au sol, le long de la ligne de visée (kt)
    pub vr_kt: f64,
    /// Sources hors radar : ADS-B (civil à transpondeur) et Link 16 (amis, pistes AWACS)
    pub adsb: bool,
    pub dl: bool,
}

// ---------- Modèle radar (impulsions Doppler, balayage RWS, poursuite STT) ----------
/// Portée de détection d'une cible de 5 m² (NM) ; portée instrumentée maximale (NM)
pub const R0_NM: f64 = 85.0;
pub const R_MAX_NM: f64 = 160.0;
/// Volume de balayage (deg), cadre de balayage RWS (s)
pub const AZ_LIM: f64 = 60.0;
pub const EL_LIM: f64 = 30.0;
pub const SCAN_S: f64 = 2.0;
/// Creux Doppler (kt de vitesse radiale sol, en visée basse), mémoire de piste (s), rupture STT (s)
pub const NOTCH_KT: f64 = 60.0;
pub const MEM_S: f64 = 8.0;
pub const STT_BREAK_S: f64 = 2.0;
pub const GIMBAL: f64 = 70.0;

fn rcs_of(icao: &str) -> f64 {
    match icao {
        "A320" | "B738" => 20.0,
        "B77W" => 60.0,
        "A359" => 50.0,
        "B789" => 40.0,
        "E175" => 12.0,
        "B748" => 100.0,
        "C56X" => 5.0,
        "F16" => 1.2,
        "F35" => 0.005,
        "F22" => 0.0008,
        "FA18" => 1.5,
        "K35R" => 100.0,
        "E3TF" => 120.0,
        "C17" => 80.0,
        "U2" => 3.0,
        "Q4" => 2.0,
        _ => 15.0,
    }
}

/// Portée de détection (m) d'une cible : équation radar (R ∝ SER^1/4), bornée.
pub fn det_range(rcs: f64) -> f64 {
    (R0_NM * (rcs / 5.0).powf(0.25)).min(R_MAX_NM) * NM
}

/// Situation radar d'une cible : (dans le volume, creux Doppler, détectable, raison si non).
pub fn radar_state(ac: &Aircraft, t: &Target, stt: bool) -> (bool, bool, f64, Option<&'static str>) {
    let r = Traffic::rel(ac, t);
    let lim = if stt { GIMBAL } else { AZ_LIM };
    let in_vol = r.az.abs() < lim && r.el.abs() < EL_LIM;
    // Vitesse radiale de la cible (sol) le long de la visée ; visée basse = fouillis de sol derrière
    let b = r.brg.to_radians();
    let th = t.hdg.to_radians();
    let vr = (t.tas * th.sin() * b.sin() + t.tas * th.cos() * b.cos()) / KT;
    let look_down = r.el < -1.0;
    let notch = look_down && vr.abs() < NOTCH_KT;
    let why = if !in_vol {
        Some("HORS LIMITES D'ANTENNE")
    } else if r.range > det_range(t.rcs) {
        Some("HORS PORTEE (SER)")
    } else if notch {
        Some("CREUX DOPPLER (NOTCH)")
    } else {
        None
    };
    (in_vol, notch, vr, why)
}

/// Données relatives à l'avion.
#[derive(Clone, Copy, Debug)]
pub struct Rel {
    /// Distance oblique (m), relèvement vrai (deg), altitude relative (ft), rapprochement (kt),
    /// aspect (deg, 0 = nez vers nous, 180 = de dos), azimut relatif au nez (deg), site (deg)
    pub range: f64,
    pub brg: f64,
    pub dalt_ft: f64,
    pub closure_kt: f64,
    pub aspect: f64,
    pub az: f64,
    pub el: f64,
}

/// Types : (OACI, nom, exploitant ou force, mission, militaire, Mach, FL de croisière, IFF)
type Kind = (&'static str, &'static str, &'static str, &'static str, bool, f64, f64, Iff);
const CIVIL: [Kind; 8] = [
    ("A320", "AIRBUS A320neo", "LIGNE", "TRANSPORT PASSAGERS", false, 0.78, 360.0, Iff::Neutre),
    ("B738", "BOEING 737-800", "LIGNE", "TRANSPORT PASSAGERS", false, 0.78, 350.0, Iff::Neutre),
    ("B77W", "BOEING 777-300ER", "LIGNE", "LONG-COURRIER", false, 0.84, 370.0, Iff::Neutre),
    ("A359", "AIRBUS A350-900", "LIGNE", "LONG-COURRIER", false, 0.85, 390.0, Iff::Neutre),
    ("B789", "BOEING 787-9", "LIGNE", "LONG-COURRIER", false, 0.85, 400.0, Iff::Neutre),
    ("E175", "EMBRAER E175", "LIGNE", "REGIONAL", false, 0.74, 330.0, Iff::Neutre),
    ("B748", "BOEING 747-8F", "FRET", "CARGO", false, 0.85, 350.0, Iff::Neutre),
    ("C56X", "CESSNA CITATION XLS", "AFFAIRES", "AVIATION D'AFFAIRES", false, 0.72, 410.0, Iff::Neutre),
];
const MILITARY: [Kind; 10] = [
    ("F16", "F-16C FIGHTING FALCON", "USAF", "PATROUILLE CAP", true, 0.9, 250.0, Iff::Ami),
    ("F35", "F-35A LIGHTNING II", "USAF", "PATROUILLE CAP", true, 0.9, 300.0, Iff::Ami),
    ("F22", "F-22A RAPTOR", "USAF", "SUPERIORITE AERIENNE", true, 1.2, 450.0, Iff::Ami),
    ("FA18", "F/A-18E SUPER HORNET", "US NAVY", "PATROUILLE", true, 0.85, 280.0, Iff::Ami),
    ("K35R", "KC-135R STRATOTANKER", "USAF", "RAVITAILLEMENT EN VOL", true, 0.72, 260.0, Iff::Ami),
    ("E3TF", "E-3G SENTRY (AWACS)", "USAF", "SURVEILLANCE / CONTROLE", true, 0.7, 300.0, Iff::Ami),
    ("C17", "C-17A GLOBEMASTER III", "USAF", "TRANSPORT MILITAIRE", true, 0.76, 320.0, Iff::Ami),
    ("U2", "U-2S DRAGON LADY", "USAF", "RECONNAISSANCE HAUTE ALTITUDE", true, 0.62, 700.0, Iff::Ami),
    ("Q4", "RQ-4B GLOBAL HAWK", "USAF", "DRONE DE RECONNAISSANCE", true, 0.55, 600.0, Iff::Ami),
    ("SU27", "TYPE NON IDENTIFIE", "INCONNU", "INCONNUE", true, 0.95, 330.0, Iff::Inconnu),
];
const AIRLINES: [&str; 10] = ["AFR", "DAL", "UAL", "AAL", "SWA", "BAW", "DLH", "KLM", "QFA", "JAL"];
const MIL_CALLS: [&str; 10] = ["VIPER", "RAPTOR", "DOOM", "HAWK", "SHELL", "SENTRY", "REACH", "DRAGON", "BLUE", "BOGEY"];

pub struct Traffic {
    pub list: Vec<Target>,
    pub lock: Option<u32>,
    pub aa: bool,
    rng: u64,
    next_id: u32,
    /// Balayage RWS : temps dans le cadre ; perte de poursuite STT en cours (s)
    scan_t: f64,
    lock_loss_t: f64,
    /// Messages à afficher (rupture de verrouillage…)
    pub events: Vec<String>,
}

impl Default for Traffic {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    /// Vue figée pour l'affichage (horizon, ND) : (trafic, cible verrouillée, mode A/A)
    static SNAP: std::cell::RefCell<(Vec<Target>, Option<u32>, bool)> = const { std::cell::RefCell::new((Vec::new(), None, false)) };
    /// Clic sur un symbole (horizon) : identifiant à verrouiller
    static CLICK: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

impl Traffic {
    pub fn new() -> Self {
        Traffic { list: Vec::new(), lock: None, aa: false, rng: 0x9E37_79B9_7F4A_7C15, next_id: 1, scan_t: 0.0, lock_loss_t: 0.0, events: Vec::new() }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn spawn(&mut self, ac: &Aircraft, mil: bool) {
        let k = if mil { MILITARY[(self.rand() * MILITARY.len() as f64) as usize % MILITARY.len()] } else { CIVIL[(self.rand() * CIVIL.len() as f64) as usize % CIVIL.len()] };
        let (icao, name, operator, mission, is_mil, mach, fl, iff) = k;
        let dist = (15.0 + self.rand() * 60.0) * NM;
        let brg = self.rand() * 360.0;
        let (lat, lon) = destination(ac.lat, ac.lon, brg, dist);
        let alt = if self.rand() < 0.25 && !is_mil { (8_000.0 + self.rand() * 20_000.0) * FT } else { (fl * 100.0 + (self.rand() - 0.5) * 4_000.0) * FT };
        let a = crate::atmosphere::isa(alt).a;
        let hdg = self.rand() * 360.0;
        let behave = match icao {
            "K35R" | "E3TF" => Behave::Orbit { lat, lon, radius: (12.0 + self.rand() * 10.0) * NM },
            "F16" | "F35" | "F22" | "FA18" | "SU27" => Behave::Patrol,
            _ => Behave::Route,
        };
        let id = self.next_id;
        self.next_id += 1;
        let num = (self.rand() * 9000.0) as u32 + 100;
        let call = if is_mil {
            let c = match icao {
                "K35R" => "SHELL",
                "E3TF" => "SENTRY",
                "C17" => "REACH",
                "U2" => "DRAGON",
                "Q4" => "BLUE",
                "SU27" => "BOGEY",
                _ => MIL_CALLS[(self.rand() * 4.0) as usize % 4],
            };
            if icao == "SU27" { format!("{c} {}", id) } else { format!("{c} {}{}", num % 9 + 1, num % 4 + 1) }
        } else {
            format!("{}{}", AIRLINES[(self.rand() * AIRLINES.len() as f64) as usize % AIRLINES.len()], num)
        };
        let squawk = if iff == Iff::Inconnu { 0 } else { (0..4).fold(0u16, |acc, _| acc * 10 + (self.rand() * 8.0) as u16) };
        let apts: Vec<&str> = crate::airports::AIRPORTS.iter().map(|a| a.icao).collect();
        let pick = |r: f64| apts[(r * apts.len() as f64) as usize % apts.len()].to_string();
        let (org, dest) = if is_mil { (String::new(), String::new()) } else { (pick(self.rand()), pick(self.rand())) };
        let tas = mach * a * (0.95 + self.rand() * 0.1);
        let turn_t = 20.0 + self.rand() * 60.0;
        self.list.push(Target {
            id,
            call,
            icao,
            name,
            operator,
            mission,
            mil: is_mil,
            iff,
            squawk,
            lat,
            lon,
            alt,
            tas,
            hdg,
            vs: 0.0,
            alt_tgt: alt,
            hdg_tgt: hdg,
            behave,
            turn_t,
            org,
            dest,
            rcs: rcs_of(icao),
            radar_age: 1e9,
            notch: false,
            in_vol: false,
            vr_kt: 0.0,
            adsb: false,
            dl: false,
        });
    }

    /// Une étape : apparition / disparition autour de l'avion, déplacement, comportement.
    pub fn update(&mut self, ac: &Aircraft, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        // Population : ~10 civils et ~4 militaires dans 80 NM
        self.list.retain(|t| dist_brg(ac.lat, ac.lon, t.lat, t.lon).0 < 95.0 * NM);
        let civ = self.list.iter().filter(|t| !t.mil).count();
        let mil = self.list.iter().filter(|t| t.mil).count();
        if civ < 10 {
            self.spawn(ac, false);
        }
        if mil < 4 {
            self.spawn(ac, true);
        }
        if self.lock.is_some_and(|id| !self.list.iter().any(|t| t.id == id)) {
            self.lock = None;
        }
        let r = self.rand();
        for t in self.list.iter_mut() {
            t.turn_t -= dt;
            match t.behave {
                Behave::Route => {
                    if t.turn_t < 0.0 {
                        t.hdg_tgt = wrap360(t.hdg + (r - 0.5) * 60.0);
                        t.turn_t = 60.0 + r * 120.0;
                        if r < 0.2 {
                            t.alt_tgt = (t.alt / FT + if r < 0.1 { 2_000.0 } else { -2_000.0 }) * FT;
                        }
                    }
                }
                Behave::Patrol => {
                    if t.turn_t < 0.0 {
                        t.hdg_tgt = wrap360(t.hdg + 90.0 + r * 180.0);
                        t.turn_t = 25.0 + r * 50.0;
                    }
                }
                Behave::Orbit { lat, lon, radius } => {
                    let (d, b) = dist_brg(t.lat, t.lon, lat, lon);
                    // Tangente à l'orbite, corrigée vers le cercle
                    let corr = ((d - radius) / radius * 60.0).clamp(-60.0, 60.0);
                    t.hdg_tgt = wrap360(b - 90.0 + corr);
                }
            }
            let dh = wrap180(t.hdg_tgt - t.hdg);
            let rate = if t.mil && matches!(t.behave, Behave::Patrol) { 6.0 } else { 2.5 };
            t.hdg = wrap360(t.hdg + dh.clamp(-rate * dt, rate * dt));
            let dz = t.alt_tgt - t.alt;
            t.vs = dz.clamp(-12.0, 12.0) * 0.3;
            t.alt += t.vs * dt;
            let (la, lo) = destination(t.lat, t.lon, t.hdg, t.tas * dt);
            t.lat = la;
            t.lon = lo;
        }
        // ---- Radar, ADS-B, Link 16 ----
        self.scan_t += dt;
        let frame = self.scan_t >= SCAN_S;
        if frame {
            self.scan_t -= SCAN_S;
        }
        let awacs: Vec<(f64, f64)> = self.list.iter().filter(|t| t.icao == "E3TF").map(|t| (t.lat, t.lon)).collect();
        let lock = self.lock;
        let mut lock_why = None;
        let mut pds = Vec::with_capacity(self.list.len());
        for _ in 0..self.list.len() {
            pds.push(self.rand());
        }
        for (k, t) in self.list.iter_mut().enumerate() {
            let stt = Some(t.id) == lock;
            let (in_vol, notch, vr, why) = radar_state(ac, t, stt);
            t.in_vol = in_vol;
            t.notch = notch;
            t.vr_kt = vr;
            t.radar_age += dt;
            let range = Traffic::rel(ac, t).range;
            if stt {
                // Poursuite continue : la piste est rafraîchie tant que la cible reste détectable
                if why.is_none() {
                    t.radar_age = 0.0;
                } else {
                    lock_why = why;
                }
            } else if frame && why.is_none() {
                // Probabilité de détection : forte à courte portée, décroissante vers la limite
                let x = range / det_range(t.rcs);
                let pd = if x < 0.7 { 0.95 } else { 0.95 - (x - 0.7) / 0.3 * 0.65 };
                if pds[k] < pd {
                    t.radar_age = 0.0;
                }
            }
            t.adsb = !t.mil && t.squawk > 0 && range < 150.0 * NM;
            let seen_by_awacs = awacs.iter().any(|&(la, lo)| dist_brg(la, lo, t.lat, t.lon).0 < 250.0 * NM);
            t.dl = (t.mil && t.iff == Iff::Ami) || seen_by_awacs;
        }
        // Rupture de poursuite (STT) après 2 s sans détection
        if lock.is_some() {
            match lock_why {
                Some(w) => {
                    self.lock_loss_t += dt;
                    if self.lock_loss_t > STT_BREAK_S {
                        let call = self.locked_call().unwrap_or_default();
                        self.lock = None;
                        self.lock_loss_t = 0.0;
                        self.events.push(format!("LOCK PERDU {call} — {w}"));
                    }
                }
                None => self.lock_loss_t = 0.0,
            }
        }
        SNAP.with(|s| *s.borrow_mut() = (self.list.clone(), self.lock, self.aa));
    }

    /// Contact connu (piste radar, mémoire, ADS-B ou Link 16).
    pub fn known(t: &Target) -> bool {
        t.radar_age < MEM_S || t.adsb || t.dl
    }

    /// Source de la piste affichée.
    pub fn source(t: &Target) -> &'static str {
        if t.radar_age < 1.0 {
            "RADAR"
        } else if t.radar_age < MEM_S {
            "MEMOIRE"
        } else if t.dl {
            "LINK 16"
        } else if t.adsb {
            "ADS-B"
        } else {
            "--"
        }
    }

    /// Données relatives d'une cible.
    pub fn rel(ac: &Aircraft, t: &Target) -> Rel {
        let (d, brg) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let dz = t.alt - ac.alt;
        let range = d.hypot(dz);
        let gs = ac.ground_speed();
        let trk = ac.track_deg().to_radians();
        let own = (gs * trk.sin(), gs * trk.cos(), ac.d.vs);
        let th = t.hdg.to_radians();
        let tv = (t.tas * th.sin(), t.tas * th.cos(), t.vs);
        let b = brg.to_radians();
        let los = (b.sin() * d / range.max(1.0), b.cos() * d / range.max(1.0), dz / range.max(1.0));
        let rv = (tv.0 - own.0, tv.1 - own.1, tv.2 - own.2);
        let closure = -(rv.0 * los.0 + rv.1 * los.1 + rv.2 * los.2) / KT;
        let aspect = wrap180(wrap360(brg + 180.0) - t.hdg);
        let az = wrap180(brg - ac.hdg_deg());
        let el = dz.atan2(d).to_degrees();
        Rel { range, brg, dalt_ft: dz / FT, closure_kt: closure, aspect, az, el }
    }

    /// Verrouille la cible suivante : devant l'avion (cône de 60°) par distance croissante, sinon
    /// la plus proche.
    pub fn lock_next(&mut self, ac: &Aircraft) -> Option<String> {
        let mut c: Vec<(f64, u32)> = self.list.iter().filter(|t| t.radar_age < 1.0).map(|t| {
            let r = Self::rel(ac, t);
            let front = r.az.abs() < 60.0;
            ((if front { 0.0 } else { 1e9 }) + r.range, t.id)
        }).collect();
        c.sort_by(|a, b| a.0.total_cmp(&b.0));
        if c.is_empty() {
            self.lock = None;
            return None;
        }
        let i = match self.lock.and_then(|id| c.iter().position(|x| x.1 == id)) {
            Some(i) => (i + 1) % c.len(),
            None => 0,
        };
        self.lock = Some(c[i].1);
        self.list.iter().find(|t| t.id == c[i].1).map(|t| t.call.clone())
    }

    pub fn lock_call(&mut self, call: &str) -> Result<String, String> {
        let t = self.list.iter().find(|t| t.call.replace(' ', "") == call.replace(' ', "")).ok_or("CIBLE INCONNUE")?;
        self.try_lock(t.id)
    }

    /// Verrouillage (STT) : piste radar fraîche obligatoire.
    pub fn try_lock(&mut self, id: u32) -> Result<String, String> {
        let t = self.list.iter().find(|t| t.id == id).ok_or("CIBLE INCONNUE")?;
        if t.radar_age >= 1.0 {
            return Err(format!("{} : PAS DE PISTE RADAR ({}) — VERROUILLAGE IMPOSSIBLE", t.call, Self::source(t)));
        }
        self.lock = Some(id);
        self.lock_loss_t = 0.0;
        Ok(t.call.clone())
    }

    pub fn locked(&self) -> Option<&Target> {
        self.lock.and_then(|id| self.list.iter().find(|t| t.id == id))
    }

    /// Indicatif de la cible verrouillée.
    pub fn locked_call(&self) -> Option<String> {
        self.locked().map(|t| t.call.clone())
    }

    /// Clic sur un symbole de l'horizon (demande à traiter).
    pub fn take_click(&mut self) -> Option<Result<String, String>> {
        let id = CLICK.with(|c| c.take())?;
        Some(self.try_lock(id))
    }

    /// Trafic pour le ND : (lat, lon, alt m, indicatif, militaire, verrouillé, IFF, vs m/s)
    pub fn nd_items(&self) -> Vec<(f64, f64, f64, String, bool, bool, Iff, f64)> {
        self.list.iter().filter(|t| Self::known(t)).map(|t| (t.lat, t.lon, t.alt, t.call.clone(), t.mil, Some(t.id) == self.lock, t.iff, t.vs)).collect()
    }
}

/// Horizon : symboles du trafic (mode A/A : tous, sinon la cible verrouillée), cadre de
/// désignation, repère de direction hors champ, fiche de la cible.
pub fn draw_hud(ac: &Aircraft, v: &crate::svs::View, s: Rect, time: f64) {
    let (list, lock, aa) = SNAP.with(|x| x.borrow().clone());
    if list.is_empty() || (!aa && lock.is_none()) {
        if aa {
            txt("A/A", s.x + 10.0, s.y + s.h - 12.0, 16.0, CYAN_S);
        }
        return;
    }
    let eye = crate::terrain::EyeFrame::new(ac);
    let c = vec2(s.x + s.w / 2.0, s.y + s.h / 2.0);
    if aa {
        // Mode A/A : repère et zone de balayage radar
        txt("A/A", s.x + 10.0, s.y + s.h - 12.0, 16.0, CYAN_S);
        draw_circle_lines(c.x, c.y, s.h * 0.32, 1.0, Color::new(0.2, 1.0, 0.35, 0.35));
    }
    for t in &list {
        let locked = Some(t.id) == lock;
        if (!aa && !locked) || !Traffic::known(t) {
            continue;
        }
        let r = Traffic::rel(ac, t);
        if r.range > 80.0 * NM {
            continue;
        }
        let p = v.project(eye.local(t.lat, t.lon, t.alt));
        let fresh = t.radar_age < 1.0;
        let base = t.iff.color();
        let col = if fresh { base } else { Color::new(base.r, base.g, base.b, 0.45) };
        match p {
            Some(p) if s.contains(p) => {
                if locked {
                    // Cadre de désignation (TD box), clignote si rapprochement rapide
                    let blink = r.closure_kt > 600.0 && (time * 4.0).fract() < 0.5;
                    let sz = 16.0;
                    if !blink {
                        draw_rectangle_lines(p.x - sz, p.y - sz, sz * 2.0, sz * 2.0, 2.0, AMBER);
                    }
                    txt_c("LOCK", p.x, p.y - sz - 4.0, 11.0, AMBER);
                    txt_c(&t.call, p.x, p.y + sz + 12.0, 11.0, AMBER);
                } else {
                    draw_poly_lines(p.x, p.y, 4, 7.0, 45.0, 1.5, col);
                    let tag = if fresh { String::new() } else { format!(" {}", match Traffic::source(t) { "MEMOIRE" => "MEM", "LINK 16" => "L16", "ADS-B" => "ADS", x => x }) };
                    txt(&format!("{:.0}{tag}{}", r.range / NM, if t.notch { " NTCH" } else { "" }), p.x + 9.0, p.y + 4.0, 10.0, col);
                    let hit = Rect::new(p.x - 10.0, p.y - 10.0, 20.0, 20.0);
                    if clicked(hit) {
                        CLICK.with(|x| x.set(Some(t.id)));
                    }
                }
            }
            _ => {
                if locked {
                    // Hors champ : flèche de direction depuis le centre
                    let a = (r.az.to_radians() as f32, -(r.el.to_radians() as f32));
                    let d = vec2(a.0.sin(), a.1.sin()).normalize_or_zero();
                    let q = c + d * (s.h * 0.3);
                    draw_line(c.x + d.x * 40.0, c.y + d.y * 40.0, q.x, q.y, 2.0, AMBER);
                    draw_circle(q.x, q.y, 4.0, AMBER);
                    txt_c(&format!("{} {:.0}NM", t.call, r.range / NM), q.x, q.y - 8.0, 11.0, AMBER);
                }
            }
        }
    }
    // Fiche de la cible verrouillée
    if let Some(t) = lock.and_then(|id| list.iter().find(|t| t.id == id)) {
        let lines = target_lines(ac, t);
        let bw = 300.0f32.min(s.w * 0.42);
        let bh = 18.0 + lines.len() as f32 * 15.0;
        let b = Rect::new(s.x + s.w - bw - 12.0, s.y + s.h * 0.42, bw, bh);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.0, 0.0, 0.0, 0.62));
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.2, AMBER);
        for (k, (l, v, c)) in lines.iter().enumerate() {
            let y = b.y + 16.0 + k as f32 * 15.0;
            txt(l, b.x + 8.0, y, 10.0, DIM);
            let vv: String = v.chars().take(34).collect();
            txt(&vv, b.x + 70.0, y, 11.0, *c);
        }
    }
}

/// Lignes de la fiche d'une cible : (libellé, valeur, couleur).
pub fn target_lines(ac: &Aircraft, t: &Target) -> Vec<(String, String, Color)> {
        let r = Traffic::rel(ac, t);
    let mach = t.tas / crate::atmosphere::isa(t.alt).a;
    let asp = r.aspect.abs();
    let asp_l = if asp < 45.0 { "HOT" } else if asp < 120.0 { "FLANC" } else { "COLD" };
    let side = if r.aspect >= 0.0 { "R" } else { "L" };
    let mut lines: Vec<(String, String, Color)> = vec![
        ("CIBLE".into(), t.call.clone(), AMBER),
        ("TYPE".into(), format!("{}  {}", t.icao, t.name), WHITE),
        ("IFF".into(), format!("{}  {}", t.iff.label(), if t.squawk > 0 { format!("XPDR {:04}", t.squawk) } else { "SANS XPDR".into() }), t.iff.color()),
        ("EXPL.".into(), format!("{} - {}", t.operator, t.mission), WHITE),
        ("ALT".into(), format!("FL{:03.0}  {:+.0} FPM", t.alt / FT / 100.0, t.vs / FT * 60.0), WHITE),
        ("VITESSE".into(), format!("{:.0} KT  M{mach:.2}  CAP {:03.0}", t.tas / KT, t.hdg), WHITE),
        ("DIST".into(), format!("{:.1} NM  REL {:03.0}  {:+.0} FT", r.range / NM, r.brg, r.dalt_ft), WHITE),
        ("RAPPR.".into(), format!("{:+.0} KT  ASPECT {:.0}{side} {asp_l}", r.closure_kt, asp), if r.closure_kt > 0.0 { AMBER } else { WHITE }),
    ];
    if !t.org.is_empty() {
        lines.push(("ROUTE".into(), format!("{} > {}", t.org, t.dest), WHITE));
    }
    let age = if t.radar_age < 1e8 { format!(" ({:.0} s)", t.radar_age) } else { String::new() };
    lines.push(("PISTE".into(), format!("{}{age}  SER {} m2", Traffic::source(t), if t.rcs < 0.1 { format!("{:.3}", t.rcs) } else { format!("{:.0}", t.rcs) }), if t.radar_age < 1.0 { GREEN_S } else { DIM }));
    lines.push(("DOPPLER".into(), format!("VR {:+.0} KT{}", t.vr_kt, if t.notch { "  NOTCH" } else { "" }), if t.notch { AMBER } else { WHITE }));
    lines
}

thread_local! {
    /// Portée de la page LOCK (NM)
    static TSD_RANGE: std::cell::Cell<usize> = const { std::cell::Cell::new(2) };
}
const TSD_RANGES: [f64; 4] = [10.0, 20.0, 40.0, 80.0];

/// Page LOCK : situation tactique vue de dessus façon liaison de données (avion en bas, cercles
/// de distance, contacts par IFF avec vecteur vitesse, indicatif et niveau), liste par distance ;
/// clic sur un contact ou une ligne = verrouillage ; fiche de la cible verrouillée.
pub fn draw_tsd(ac: &Aircraft, r: Rect, time: f64) {
    let (list, lock, _) = SNAP.with(|x| x.borrow().clone());
    panel(r, "LOCK - SITUATION TACTIQUE (DATALINK)");
    let ri = TSD_RANGE.with(|c| c.get());
    let rng = TSD_RANGES[ri];
    // Sélecteur de portée
    for (k, v) in TSD_RANGES.iter().enumerate() {
        let b = Rect::new(r.x + r.w - 12.0 - (4 - k) as f32 * 52.0, r.y + 6.0, 48.0, 20.0);
        let on = k == ri;
        rrect(b, 4.0, if on { Color::new(0.2, 0.21, 0.235, 1.0) } else if hovered(b) { Color::new(0.16, 0.17, 0.19, 1.0) } else { Color::new(0.12, 0.125, 0.14, 1.0) });
        txt_c(&format!("{v:.0} NM"), b.x + b.w / 2.0, b.y + 14.0, 10.0, if on { WHITE } else { DIM });
        if on {
            draw_rectangle(b.x + 6.0, b.y + b.h - 2.0, b.w - 12.0, 2.0, ACCENT);
        }
        if clicked(b) {
            TSD_RANGE.with(|c| c.set(k));
        }
    }
    let lw = (r.w * 0.34).clamp(200.0, 320.0);
    let sc = Rect::new(r.x + 10.0, r.y + 32.0, r.w - lw - 30.0, r.h - 42.0);
    draw_rectangle(sc.x, sc.y, sc.w, sc.h, Color::new(0.01, 0.015, 0.02, 1.0));
    scissor(Some(sc));
    // Avion au tiers inférieur, cap vers le haut
    let own = vec2(sc.x + sc.w / 2.0, sc.y + sc.h * 0.72);
    let ppn = (sc.h * 0.68) / rng as f32;
    let hdg = ac.hdg_deg();
    let to_s = |la: f64, lo: f64| -> Vec2 {
        let (d, b) = dist_brg(ac.lat, ac.lon, la, lo);
        let a = (b - hdg).to_radians() as f32;
        own + vec2(a.sin(), -a.cos()) * (d / NM) as f32 * ppn
    };
    let ring = Color::new(0.2, 0.55, 0.65, 0.35);
    for k in 1..=4 {
        let rr = ppn * rng as f32 * k as f32 / 4.0;
        draw_circle_lines(own.x, own.y, rr, 1.0, ring);
        txt(&format!("{:.0}", rng * k as f64 / 4.0), own.x + 4.0, own.y - rr + 12.0, 9.0, ring);
    }
    draw_line(own.x, own.y, own.x, sc.y, 1.0, Color::new(0.2, 0.55, 0.65, 0.2));
    // Volume de balayage radar (+/-60°) et faisceau RWS (aller-retour en 2 x 2 s)
    let edge = ppn * rng as f32;
    for a in [-AZ_LIM as f32, AZ_LIM as f32] {
        let ar = a.to_radians();
        let q = own + vec2(ar.sin(), -ar.cos()) * edge;
        draw_line(own.x, own.y, q.x, q.y, 1.0, Color::new(0.3, 1.0, 0.4, 0.3));
    }
    let ph = ((time / SCAN_S) % 2.0) as f32;
    let az = if ph < 1.0 { -60.0 + 120.0 * ph } else { 60.0 - 120.0 * (ph - 1.0) };
    if lock.is_none() {
        let ar = az.to_radians();
        let q = own + vec2(ar.sin(), -ar.cos()) * edge;
        draw_line(own.x, own.y, q.x, q.y, 2.0, Color::new(0.3, 1.0, 0.4, 0.55));
    }
    txt(&format!("RADAR {}  +/-60  SER 5m2 {:.0} NM", if lock.is_some() { "STT" } else { "RWS" }, R0_NM), sc.x + 6.0, sc.y + 14.0, 10.0, GREEN_S);
    txt("PLEIN : RADAR   PALE : MEMOIRE / LINK16 / ADS-B   NTCH : CREUX DOPPLER", sc.x + 6.0, sc.y + sc.h - 6.0, 9.0, DIM);
    // Avion
    draw_triangle(own + vec2(0.0, -9.0), own + vec2(-6.0, 7.0), own + vec2(6.0, 7.0), WHITE);
    txt_c(&format!("FL{:03.0}", ac.alt / FT / 100.0), own.x, own.y + 20.0, 10.0, WHITE);
    let mut clicked_id = None;
    for t in list.iter().filter(|t| Traffic::known(t)) {
        let p = to_s(t.lat, t.lon);
        if !sc.contains(p) {
            continue;
        }
        let locked = Some(t.id) == lock;
        let fresh = t.radar_age < 1.0;
        let c0 = if locked { AMBER } else { t.iff.color() };
        let c = if fresh || locked { c0 } else { Color::new(c0.r, c0.g, c0.b, 0.45) };
        // Vecteur vitesse : position dans 1 min
        let a = (t.hdg - hdg).to_radians() as f32;
        let lead = (t.tas * 60.0 / NM) as f32 * ppn;
        let q = p + vec2(a.sin(), -a.cos()) * lead;
        draw_line(p.x, p.y, q.x, q.y, 1.2, c);
        match t.iff {
            Iff::Ami => draw_circle_lines(p.x, p.y, 6.0, 1.6, c),
            Iff::Neutre => draw_rectangle_lines(p.x - 5.5, p.y - 5.5, 11.0, 11.0, 1.6, c),
            Iff::Inconnu => draw_poly_lines(p.x, p.y, 4, 7.0, 45.0, 1.6, c),
        }
        if t.mil {
            draw_circle(p.x, p.y, 2.0, c);
        }
        txt(&t.call, p.x + 9.0, p.y - 2.0, 10.0, c);
        let src = if fresh { "" } else { match Traffic::source(t) { "MEMOIRE" => " MEM", "LINK 16" => " L16", "ADS-B" => " ADS", _ => "" } };
        txt(&format!("{:03.0}{src}{}", t.alt / FT / 100.0, if t.notch { " NTCH" } else { "" }), p.x + 9.0, p.y + 9.0, 9.0, c);
        if locked {
            let blink = (time * 3.0).fract() < 0.7;
            if blink {
                draw_rectangle_lines(p.x - 11.0, p.y - 11.0, 22.0, 22.0, 2.0, AMBER);
            }
            draw_line(own.x, own.y, p.x, p.y, 1.0, Color::new(1.0, 0.72, 0.0, 0.5));
        }
        if clicked(Rect::new(p.x - 12.0, p.y - 12.0, 24.0, 24.0)) {
            clicked_id = Some(t.id);
        }
    }
    scissor(Some(r));
    // Liste des contacts par distance
    let lx = sc.x + sc.w + 10.0;
    let mut rows: Vec<(&Target, Rel)> = list.iter().filter(|t| Traffic::known(t)).map(|t| (t, Traffic::rel(ac, t))).collect();
    rows.sort_by(|a, b| a.1.range.total_cmp(&b.1.range));
    let lock_t = lock.and_then(|id| list.iter().find(|t| t.id == id));
    let card_h = if lock_t.is_some() { 170.0 } else { 0.0 };
    let list_h = r.y + r.h - 10.0 - card_h - (sc.y);
    let rh = 22.0;
    txt("CONTACTS", lx, sc.y + 12.0, 10.0, DIM);
    for (k, (t, rel)) in rows.iter().enumerate() {
        let y = sc.y + 18.0 + k as f32 * rh;
        if y + rh > sc.y + list_h {
            break;
        }
        let b = Rect::new(lx, y, lw, rh - 2.0);
        let locked = Some(t.id) == lock;
        rrect(b, 3.0, if locked { Color::new(0.3, 0.22, 0.05, 1.0) } else if hovered(b) { Color::new(0.16, 0.17, 0.19, 1.0) } else { Color::new(0.1, 0.105, 0.12, 1.0) });
        let c = if locked { AMBER } else { t.iff.color() };
        txt(&t.call, b.x + 6.0, b.y + 14.0, 11.0, c);
        let src = match Traffic::source(t) { "RADAR" => "RDR", "MEMOIRE" => "MEM", "LINK 16" => "L16", "ADS-B" => "ADS", _ => "--" };
        txt(&format!("{} {src}", t.icao), b.x + lw * 0.36, b.y + 14.0, 10.0, if t.radar_age < 1.0 { GREEN_S } else { DIM });
        txt_r(&format!("{:.0}NM  FL{:03.0}", rel.range / NM, t.alt / FT / 100.0), b.x + b.w - 6.0, b.y + 14.0, 10.0, WHITE);
        if clicked(b) {
            clicked_id = Some(t.id);
        }
    }
    // Fiche de la cible
    if let Some(t) = lock_t {
        let b = Rect::new(lx, r.y + r.h - 10.0 - card_h, lw, card_h);
        rrect(b, 4.0, Color::new(0.06, 0.05, 0.03, 1.0));
        rrect_lines(b, 4.0, 1.2, AMBER);
        for (k, (l, v, c)) in target_lines(ac, t).iter().enumerate() {
            let y = b.y + 15.0 + k as f32 * 15.5;
            if y > b.y + b.h - 4.0 {
                break;
            }
            txt(l, b.x + 6.0, y, 9.0, DIM);
            let vv: String = v.chars().take(30).collect();
            txt(&vv, b.x + 60.0, y, 10.0, *c);
        }
    }
    if let Some(id) = clicked_id {
        CLICK.with(|x| x.set(Some(id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own() -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.alt = 30_000.0 * FT;
        ac.tas = 250.0;
        ac
    }

    fn tgt(ac: &Aircraft, icao: &'static str, dist_nm: f64, rel_brg: f64, alt_ft: f64, hdg: f64) -> Target {
        let (lat, lon) = destination(ac.lat, ac.lon, wrap360(ac.hdg_deg() + rel_brg), dist_nm * NM);
        Target {
            id: 1, call: "TEST1".into(), icao, name: "", operator: "", mission: "", mil: true, iff: Iff::Inconnu, squawk: 0,
            lat, lon, alt: alt_ft * FT, tas: 230.0, hdg, vs: 0.0, alt_tgt: alt_ft * FT, hdg_tgt: hdg, behave: Behave::Route, turn_t: 1e9,
            org: String::new(), dest: String::new(), rcs: rcs_of(icao), radar_age: 1e9, notch: false, in_vol: false, vr_kt: 0.0, adsb: false, dl: false,
        }
    }

    /// Équation radar : un furtif n'est vu qu'à courte portée, un gros porteur de très loin.
    #[test]
    fn detection_range_by_rcs() {
        let ac = own();
        println!("portees : F-22 {:.1} NM, F-35 {:.1} NM, F-16 {:.0} NM, 777 {:.0} NM", det_range(rcs_of("F22")) / NM, det_range(rcs_of("F35")) / NM, det_range(rcs_of("F16")) / NM, det_range(rcs_of("B77W")) / NM);
        let far_stealth = tgt(&ac, "F22", 20.0, 0.0, 30_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &far_stealth, false).3.is_some(), "F-22 a 20 NM ne doit pas etre vu");
        let near_stealth = tgt(&ac, "F22", 6.0, 0.0, 30_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &near_stealth, false).3.is_none(), "F-22 a 6 NM vu");
        let liner = tgt(&ac, "B77W", 120.0, 10.0, 35_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &liner, false).3.is_none(), "777 a 120 NM vu");
        let behind = tgt(&ac, "B77W", 20.0, 150.0, 30_000.0, 0.0);
        assert_eq!(radar_state(&ac, &behind, false).3, Some("HORS LIMITES D'ANTENNE"));
    }

    /// Creux Doppler : cible en dessous, en travers (vitesse radiale sol ~ 0) -> perdue dans le
    /// fouillis ; la même cible de face est vue.
    #[test]
    fn doppler_notch() {
        let ac = own();
        let brg = ac.hdg_deg();
        let beam = tgt(&ac, "F16", 20.0, 0.0, 5_000.0, wrap360(brg + 90.0));
        let (_, notch, vr, why) = radar_state(&ac, &beam, false);
        println!("travers vue basse : VR {vr:+.0} kt, notch {notch}, {why:?}");
        assert!(notch && why == Some("CREUX DOPPLER (NOTCH)"));
        let hot = tgt(&ac, "F16", 20.0, 0.0, 5_000.0, wrap360(brg + 180.0));
        assert!(!radar_state(&ac, &hot, false).1);
        // Visée haute : pas de fouillis, pas de creux
        let above = tgt(&ac, "F16", 20.0, 0.0, 45_000.0, wrap360(brg + 90.0));
        assert!(!radar_state(&ac, &above, false).1);
    }

    /// Verrouillage : piste radar requise ; la cible qui se met en travers casse le STT en ~2 s.
    #[test]
    fn stt_breaks_in_notch() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100; // identifiants des trafics créés distincts de la cible d'essai
        let mut t = tgt(&ac, "F16", 18.0, 0.0, 8_000.0, wrap360(ac.hdg_deg() + 180.0));
        t.radar_age = 0.0;
        tr.list.push(t);
        assert!(tr.try_lock(1).is_ok());

        // La cible vire en travers
        tr.list[0].hdg = wrap360(ac.hdg_deg() + 90.0);
        tr.list[0].hdg_tgt = tr.list[0].hdg;
        let mut tt = 0.0;
        while tr.lock.is_some() && tt < 5.0 {
            // Population figée : on évite l'apparition de nouveaux trafics
            let keep = tr.list.clone();
            tr.update(&ac, 0.1);
            tr.list.retain(|x| keep.iter().any(|k| k.id == x.id));
            tt += 0.1;
        }
        println!("STT rompu apres {tt:.1} s : {:?}", tr.events);
        assert!(tr.lock.is_none() && tt > 1.9 && tr.events.iter().any(|e| e.contains("NOTCH")));
        // Piste perdue en mémoire puis plus verrouillable
        assert!(tr.try_lock(1).is_err());
    }
}
