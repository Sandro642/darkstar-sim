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
        Traffic { list: Vec::new(), lock: None, aa: false, rng: 0x9E37_79B9_7F4A_7C15, next_id: 1 }
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
        SNAP.with(|s| *s.borrow_mut() = (self.list.clone(), self.lock, self.aa));
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
        let mut c: Vec<(f64, u32)> = self.list.iter().map(|t| {
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

    pub fn lock_call(&mut self, call: &str) -> Option<String> {
        let t = self.list.iter().find(|t| t.call.replace(' ', "") == call.replace(' ', ""))?;
        self.lock = Some(t.id);
        Some(t.call.clone())
    }

    pub fn locked(&self) -> Option<&Target> {
        self.lock.and_then(|id| self.list.iter().find(|t| t.id == id))
    }

    /// Indicatif de la cible verrouillée.
    pub fn locked_call(&self) -> Option<String> {
        self.locked().map(|t| t.call.clone())
    }

    /// Clic sur un symbole de l'horizon (demande à traiter).
    pub fn take_click(&mut self) -> Option<String> {
        let id = CLICK.with(|c| c.take())?;
        self.lock = Some(id);
        self.list.iter().find(|t| t.id == id).map(|t| t.call.clone())
    }

    /// Trafic pour le ND : (lat, lon, alt m, indicatif, militaire, verrouillé, IFF, vs m/s)
    pub fn nd_items(&self) -> Vec<(f64, f64, f64, String, bool, bool, Iff, f64)> {
        self.list.iter().map(|t| (t.lat, t.lon, t.alt, t.call.clone(), t.mil, Some(t.id) == self.lock, t.iff, t.vs)).collect()
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
        if !aa && !locked {
            continue;
        }
        let r = Traffic::rel(ac, t);
        if r.range > 80.0 * NM {
            continue;
        }
        let p = v.project(eye.local(t.lat, t.lon, t.alt));
        let col = t.iff.color();
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
                    txt(&format!("{:.0}", r.range / NM), p.x + 9.0, p.y + 4.0, 10.0, col);
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
        let r = Traffic::rel(ac, t);
        let mach = t.tas / crate::atmosphere::isa(t.alt).a;
        let asp = r.aspect.abs();
        let asp_l = if asp < 45.0 { "HOT" } else if asp < 120.0 { "FLANC" } else { "COLD" };
        let side = if r.aspect >= 0.0 { "R" } else { "L" };
        let lines: Vec<(String, String, Color)> = vec![
            ("CIBLE".into(), t.call.clone(), AMBER),
            ("TYPE".into(), format!("{}  {}", t.icao, t.name), WHITE),
            ("IFF".into(), format!("{}  {}", t.iff.label(), if t.squawk > 0 { format!("XPDR {:04}", t.squawk) } else { "SANS XPDR".into() }), t.iff.color()),
            ("EXPL.".into(), format!("{} - {}", t.operator, t.mission), WHITE),
            ("ALT".into(), format!("FL{:03.0}  {:+.0} FPM", t.alt / FT / 100.0, t.vs / FT * 60.0), WHITE),
            ("VITESSE".into(), format!("{:.0} KT  M{mach:.2}  CAP {:03.0}", t.tas / KT, t.hdg), WHITE),
            ("DIST".into(), format!("{:.1} NM  REL {:03.0}  {:+.0} FT", r.range / NM, r.brg, r.dalt_ft), WHITE),
            ("RAPPR.".into(), format!("{:+.0} KT  ASPECT {:.0}{side} {asp_l}", r.closure_kt, asp), if r.closure_kt > 0.0 { AMBER } else { WHITE }),
        ];
        let mut lines = lines;
        if !t.org.is_empty() {
            lines.push(("ROUTE".into(), format!("{} > {}", t.org, t.dest), WHITE));
        }
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
