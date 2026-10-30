//! Trafic aérien IA et TCAS simplifié.

use crate::aircraft::Aircraft;
use crate::airports::AIRPORTS;
use crate::geo::*;
use macroquad::rand::gen_range;

const AIRLINES: &[&str] = &["AFR", "DAL", "UAL", "AAL", "BAW", "DLH", "SWA", "JAL", "UAE", "QFA", "ACA", "KLM", "FDX", "UPS", "ASA", "SKW"];
const TYPES: &[&str] = &["B738", "A320", "B77W", "A359", "B789", "A21N", "E175", "B744", "A388", "C17", "KC46"];

#[derive(Clone, Copy, PartialEq)]
pub enum Tcas {
    None,
    Proximate,
    Ta,
    Ra,
}

pub struct Traffic {
    pub callsign: String,
    pub actype: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub alt_tgt: f64,
    pub tas: f64,
    pub hdg: f64,
    pub vs: f64,
    pub dest: Option<(f64, f64)>,
    pub local: bool,
    pub tcas: Tcas,
}

pub struct TrafficSys {
    pub list: Vec<Traffic>,
    pub ra: Option<&'static str>,
    pub ta: bool,
    spawn_t: f64,
}

fn callsign() -> String {
    let a = AIRLINES[gen_range(0, AIRLINES.len())];
    format!("{a}{}", gen_range(10, 2999))
}

fn cruise_alt() -> f64 {
    (gen_range(24, 42) as f64) * 1000.0 * FT
}

impl TrafficSys {
    pub fn new() -> TrafficSys {
        let mut t = TrafficSys { list: Vec::new(), ra: None, ta: false, spawn_t: 0.0 };
        for _ in 0..220 {
            t.spawn_global(true);
        }
        t
    }

    fn spawn_global(&mut self, anywhere: bool) {
        let a = &AIRPORTS[gen_range(0, AIRPORTS.len())];
        let b = &AIRPORTS[gen_range(0, AIRPORTS.len())];
        if std::ptr::eq(a, b) {
            return;
        }
        let (d, brg) = dist_brg(a.lat, a.lon, b.lat, b.lon);
        let f = if anywhere { gen_range(0.05, 0.95) } else { 0.02 };
        let (lat, lon) = destination(a.lat, a.lon, brg, d * f);
        let (_, hdg) = dist_brg(lat, lon, b.lat, b.lon);
        let alt = cruise_alt();
        self.list.push(Traffic {
            callsign: callsign(),
            actype: TYPES[gen_range(0, TYPES.len())],
            lat,
            lon,
            alt,
            alt_tgt: alt,
            tas: gen_range(220.0, 255.0),
            hdg,
            vs: 0.0,
            dest: Some((b.lat, b.lon)),
            local: false,
            tcas: Tcas::None,
        });
    }

    fn spawn_local(&mut self, ac: &Aircraft) {
        let brg = gen_range(0.0, 360.0);
        let d = gen_range(30_000.0, 280_000.0);
        let (lat, lon) = destination(ac.lat, ac.lon, brg, d);
        let roll = gen_range(0, 10);
        // Répartition : croisière, montée/descente, trafic bas
        let (alt, alt_tgt, tas) = if roll < 5 {
            let a = cruise_alt();
            (a, a, gen_range(220.0, 250.0))
        } else if roll < 8 {
            let a = gen_range(8_000.0, 36_000.0) * FT;
            (a, cruise_alt().min(a + gen_range(-15_000.0, 15_000.0) * FT).max(3000.0), gen_range(150.0, 230.0))
        } else {
            let a = gen_range(3_000.0, 12_000.0) * FT;
            (a, a, gen_range(90.0, 140.0))
        };
        // Les avions locaux se dirigent souvent vers l'avion (pour créer des conflits) ou au hasard.
        let hdg = if gen_range(0, 3) == 0 {
            let (_, b) = dist_brg(lat, lon, ac.lat, ac.lon);
            wrap360(b + gen_range(-25.0, 25.0))
        } else {
            gen_range(0.0, 360.0)
        };
        self.list.push(Traffic {
            callsign: callsign(),
            actype: TYPES[gen_range(0, TYPES.len())],
            lat,
            lon,
            alt,
            alt_tgt,
            tas,
            hdg,
            vs: 0.0,
            dest: None,
            local: true,
            tcas: Tcas::None,
        });
    }

    pub fn update(&mut self, dt: f64, ac: &Aircraft) {
        for t in &mut self.list {
            if let Some((dla, dlo)) = t.dest {
                let (d, b) = dist_brg(t.lat, t.lon, dla, dlo);
                let err = wrap180(b - t.hdg);
                t.hdg = wrap360(t.hdg + err.clamp(-2.0 * dt, 2.0 * dt));
                if d < 60_000.0 {
                    t.alt_tgt = 3000.0 * FT;
                    t.dest = None;
                    t.local = true; // sera supprimé en s'éloignant
                }
            }
            let dz = t.alt_tgt - t.alt;
            t.vs = (dz * 0.05).clamp(-12.0, 12.0);
            t.alt += t.vs * dt;
            let (la, lo) = destination(t.lat, t.lon, t.hdg, t.tas * dt);
            // conserve le cap orthodromique local
            let (_, nb) = dist_brg(la, lo, t.lat, t.lon);
            if t.dest.is_none() {
                t.hdg = wrap360(nb + 180.0);
            }
            t.lat = la;
            t.lon = lo;
        }
        // Nettoyage
        self.list.retain(|t| {
            if !t.local {
                return true;
            }
            let (d, _) = dist_brg(t.lat, t.lon, ac.lat, ac.lon);
            d < 400_000.0
        });
        let globals = self.list.iter().filter(|t| !t.local).count();
        for _ in globals..220 {
            self.spawn_global(false);
        }
        self.spawn_t -= dt;
        if self.spawn_t <= 0.0 {
            self.spawn_t = 2.0;
            let near = self
                .list
                .iter()
                .filter(|t| t.local && dist_brg(t.lat, t.lon, ac.lat, ac.lon).0 < 300_000.0)
                .count();
            if near < 28 {
                self.spawn_local(ac);
            }
        }
    }

    /// TCAS : renvoie un éventuel avis de résolution et détecte les collisions.
    pub fn tcas(&mut self, ac: &Aircraft) -> Option<String> {
        self.ra = None;
        self.ta = false;
        let mut collision = None;
        let own_v = (
            ac.tas * ac.gamma.cos() * ac.psi.sin(),
            ac.tas * ac.gamma.cos() * ac.psi.cos(),
            ac.d.vs,
        );
        let mut worst_tau = f64::MAX;
        for t in &mut self.list {
            t.tcas = Tcas::None;
            if (t.lat - ac.lat).abs() > 1.0 {
                continue;
            }
            let (e, n) = enu_offset(ac.lat, ac.lon, t.lat, t.lon);
            let range = (e * e + n * n).sqrt();
            if range > 60_000.0 {
                continue;
            }
            let dz = t.alt - ac.alt;
            let tv = (t.tas * t.hdg.to_radians().sin(), t.tas * t.hdg.to_radians().cos(), t.vs);
            let rv = (tv.0 - own_v.0, tv.1 - own_v.1);
            let closing = -(e * rv.0 + n * rv.1) / range.max(1.0);
            let tau = if closing > 1.0 { range / closing } else { f64::MAX };
            let dz_cpa = dz + (tv.2 - own_v.2) * tau.min(60.0);
            if range < 120.0 && dz.abs() < 40.0 {
                collision = Some(format!("COLLISION EN VOL AVEC {}", t.callsign));
            }
            if ac.on_ground || ac.d.agl < 300.0 {
                continue;
            }
            if range < 11_000.0 && dz.abs() < 360.0 {
                t.tcas = Tcas::Proximate;
            }
            let thr_z = 850.0 * FT;
            if (tau < 40.0 || range < 1500.0) && (dz.abs() < 1200.0 * FT || dz_cpa.abs() < thr_z) {
                t.tcas = Tcas::Ta;
                self.ta = true;
            }
            if (tau < 25.0 || range < 900.0) && dz_cpa.abs() < thr_z && ac.d.agl > 1000.0 * FT {
                t.tcas = Tcas::Ra;
                if tau < worst_tau {
                    worst_tau = tau;
                    self.ra = Some(if dz > 0.0 { "DESCEND, DESCEND" } else { "CLIMB, CLIMB" });
                }
            }
        }
        collision
    }
}
