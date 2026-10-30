//! Modèle de vol et propulsion TBCC (turbine / statoréacteur / superstatoréacteur).

use crate::airports::*;
use crate::atmosphere::*;
use crate::geo::*;

pub const S_WING: f64 = 200.0;
pub const M_EMPTY: f64 = 28_000.0;
pub const FUEL_MAX: f64 = 29_000.0; // JP-7 (turbines, statoréacteur)
pub const H2_MAX: f64 = 7_000.0; // hydrogène liquide (scramjet)
pub const EYE_H: f64 = 4.0;
const G0: f64 = 9.80665;

pub const SKIN_LIMIT_C: f64 = 1250.0;
pub const Q_LIMIT: f64 = 100_000.0;
pub const GEAR_VLE: f64 = 270.0; // KEAS

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TurbState {
    Off,
    Starting,
    Running,
    Failed,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RamState {
    Off,
    Armed,
    Lit,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ScramState {
    Off,
    Armed,
    Lit,
    Unstart,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Info,
    Caution,
    Warning,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LatMode {
    Hdg,
    Nav,
    Loc,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VertMode {
    Alt,
    Gs,
    Flare,
}

pub struct Autopilot {
    pub on: bool,
    pub lat: LatMode,
    pub vert: VertMode,
    pub alt_tgt_ft: f64,
    pub hdg_tgt: f64,
    pub at_on: bool,
    pub mach_tgt: f64,
    /// Consigne de vitesse en KEAS (sinon Mach).
    pub spd_keas: Option<f64>,
}

pub struct Engines {
    pub turb: TurbState,
    pub n2: f64,
    pub egt: f64,
    pub inlet_open: bool,
    pub overtemp_t: f64,
    pub ram: RamState,
    pub ram_temp: f64,
    pub scram: ScramState,
    pub unstart_t: f64,
    pub scram_margin: f64,
    pub t_turb: f64,
    pub t_ram: f64,
    pub t_scram: f64,
    pub fuel_flow: f64,
    pub ab_frac: f64,
    pub h2_flow: f64,
    /// Portes d'entrée d'air turbine : 0 = ouvertes, 1 = fermées.
    pub doors: f64,
}

/// Grandeurs dérivées, recalculées à chaque pas, pour l'affichage.
#[derive(Clone, Copy, Default)]
pub struct Derived {
    pub mach: f64,
    pub q: f64,
    pub keas: f64,
    pub alpha: f64,
    pub nz: f64,
    pub vs: f64,
    pub agl: f64,
    pub elev: f64,
    pub thrust: f64,
    pub drag: f64,
    pub oat: f64,
    pub accel: f64,
    pub gamma_dot: f64,
}

pub struct Aircraft {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub tas: f64,
    pub gamma: f64,
    pub psi: f64,
    pub theta: f64,
    pub phi: f64,
    pub fuel: f64,
    pub h2: f64,
    pub h2_dump: bool,
    pub throttle: f64,
    pub gear_down: bool,
    pub gear_pos: f64,
    pub brakes: bool,
    pub speedbrake: bool,
    pub on_ground: bool,
    pub eng: Engines,
    pub skin_temp: f64,
    pub ap: Autopilot,
    pub d: Derived,
    pub crashed: Option<String>,
    pub landed: bool,
    pub max_mach: f64,
    pub max_alt: f64,
    pub events: Vec<(Level, String)>,
    pub jolt: f64,
    pub approach: Option<(usize, f64, f64, f64, f64)>, // (aéroport, lat seuil, lon seuil, cap, élévation)
    pub autoland: Option<crate::autoflight::Autoland>,
    pub autotakeoff: Option<crate::autoflight::Takeoff>,
}

/// Distance (m) restant avant le seuil, le long de l'axe d'approche (négative après le seuil).
pub fn along_before(tla: f64, tlo: f64, hdg: f64, lat: f64, lon: f64) -> f64 {
    let (e, n) = enu_offset(tla, tlo, lat, lon);
    let h = hdg.to_radians();
    -(e * h.sin() + n * h.cos())
}

/// Écart latéral (m) à droite de l'axe.
pub fn cross_track(tla: f64, tlo: f64, hdg: f64, lat: f64, lon: f64) -> f64 {
    let (e, n) = enu_offset(tla, tlo, lat, lon);
    let h = hdg.to_radians();
    e * h.cos() - n * h.sin()
}

fn cl_alpha(m: f64) -> f64 {
    if m < 0.8 {
        2.8
    } else if m < 1.1 {
        2.8 + (m - 0.8) / 0.3 * 0.4
    } else {
        (4.0 / (m * m - 1.0).sqrt()).clamp(0.8, 3.2)
    }
}

fn cd0(m: f64) -> f64 {
    if m < 0.85 {
        0.012
    } else if m < 1.05 {
        0.012 + (m - 0.85) / 0.2 * 0.016
    } else if m < 1.2 {
        0.028
    } else {
        (0.028 * (1.2 / m).powf(0.6)).max(0.0075)
    }
}

/// Coefficient de poussée du statoréacteur (m² équivalents par unité de q).
pub fn ram_k(m: f64) -> f64 {
    if m < 1.8 {
        0.0
    } else if m < 3.5 {
        8.0 * (m - 1.8) / 1.7
    } else if m < 4.5 {
        8.0
    } else if m < 6.8 {
        8.0 * (6.8 - m) / 2.3
    } else {
        0.0
    }
}

pub fn scram_k(m: f64) -> f64 {
    if m < 4.5 {
        0.0
    } else if m < 7.0 {
        5.0 + 1.5 * (m - 4.5) / 2.5
    } else if m < 10.0 {
        6.5 - 2.0 * (m - 7.0) / 3.0
    } else if m < 12.5 {
        4.5 * (12.5 - m) / 2.5
    } else {
        0.0
    }
}

/// Température d'équilibre de peau (bord d'attaque), K.
pub fn skin_equilibrium(alt: f64, v: f64) -> f64 {
    let air = isa(alt);
    let m = v / air.a;
    let stag = air.t * (1.0 + 0.18 * m * m);
    let rad = 5.6 * (air.rho.sqrt() * v * v * v).max(1.0).powf(0.25);
    stag.min(rad.max(air.t))
}

impl Aircraft {
    pub fn new_on_runway(apt_idx: usize) -> Aircraft {
        let a = &AIRPORTS[apt_idx];
        // Départ face au cap opposé (ex. piste 25 à Palmdale), aligné au seuil.
        let (lat, lon) = a.point(a.len / 2.0 - 120.0, 0.0);
        let psi = wrap360(a.hdg + 180.0).to_radians();
        Aircraft {
            lat,
            lon,
            alt: a.elev,
            tas: 0.0,
            gamma: 0.0,
            psi,
            theta: 0.0,
            phi: 0.0,
            fuel: FUEL_MAX,
            h2: H2_MAX,
            h2_dump: false,
            throttle: 0.0,
            gear_down: true,
            gear_pos: 1.0,
            brakes: true,
            speedbrake: false,
            on_ground: true,
            eng: Engines {
                turb: TurbState::Off,
                n2: 0.0,
                egt: 20.0,
                inlet_open: true,
                overtemp_t: 0.0,
                ram: RamState::Off,
                ram_temp: 20.0,
                scram: ScramState::Off,
                unstart_t: 0.0,
                scram_margin: 0.0,
                t_turb: 0.0,
                t_ram: 0.0,
                t_scram: 0.0,
                fuel_flow: 0.0,
                ab_frac: 0.0,
                h2_flow: 0.0,
                doors: 0.0,
            },
            skin_temp: 293.0,
            ap: Autopilot {
                on: false,
                lat: LatMode::Hdg,
                vert: VertMode::Alt,
                alt_tgt_ft: 10_000.0,
                hdg_tgt: psi.to_degrees(),
                at_on: false,
                mach_tgt: 0.9,
                spd_keas: None,
            },
            d: Derived { elev: a.elev, oat: 15.0, ..Default::default() },
            crashed: None,
            landed: false,
            max_mach: 0.0,
            max_alt: 0.0,
            events: Vec::new(),
            jolt: 0.0,
            approach: None,
            autoland: None,
            autotakeoff: None,
        }
    }

    pub fn mass(&self) -> f64 {
        M_EMPTY + self.fuel + self.h2
    }

    /// Centrage en % de corde moyenne (le JP-7 arrière est consommé en premier).
    pub fn cg(&self) -> f64 {
        25.0 - 3.5 * (1.0 - self.fuel / FUEL_MAX) + 1.5 * (1.0 - self.h2 / H2_MAX)
    }

    /// Pression du réservoir d'hydrogène (psi).
    pub fn h2_psi(&self) -> f64 {
        if self.h2 <= 0.0 {
            0.0
        } else {
            (30.0 + 170.0 * (self.h2 / H2_MAX).sqrt() - self.eng.h2_flow * 4.0).max(5.0)
        }
    }

    /// Pression d'alimentation JP-7 (psi).
    pub fn fuel_psi(&self) -> f64 {
        let pumps = matches!(self.eng.turb, TurbState::Running | TurbState::Starting) || self.eng.ram == RamState::Lit;
        if self.fuel <= 0.0 {
            0.0
        } else if pumps {
            100.0 - (1.0 - (self.fuel / FUEL_MAX / 0.05).min(1.0)) * 70.0
        } else {
            12.0
        }
    }

    pub fn cmd_h2_dump(&mut self, on: bool) {
        self.h2_dump = on;
        if on {
            self.msg(Level::Warning, "EMER H2 DUMP : VIDANGE HYDROGÈNE EN COURS");
        } else {
            self.msg(Level::Info, "EMER H2 DUMP : ARRÊT");
        }
    }

    pub fn msg(&mut self, l: Level, s: impl Into<String>) {
        self.events.push((l, s.into()));
    }

    fn crash(&mut self, why: &str) {
        if self.crashed.is_none() {
            self.crashed = Some(why.to_string());
            self.msg(Level::Warning, format!("PERTE DU VÉHICULE : {why}"));
            self.tas = 0.0;
        }
    }

    pub fn hdg_deg(&self) -> f64 {
        wrap360(self.psi.to_degrees())
    }

    // ---------- Commandes du pilote (procédures) ----------

    pub fn cmd_turbine(&mut self) {
        let m = self.d.mach;
        match self.eng.turb {
            TurbState::Off => {
                if !self.eng.inlet_open {
                    self.msg(Level::Caution, "DÉMARRAGE TURBINES REFUSÉ : ENTRÉES D'AIR FERMÉES (I)");
                } else if m > 2.5 {
                    self.msg(Level::Caution, "DÉMARRAGE TURBINES REFUSÉ : MACH > 2.5");
                } else if self.fuel <= 0.0 {
                    self.msg(Level::Caution, "DÉMARRAGE REFUSÉ : CARBURANT");
                } else {
                    self.eng.turb = TurbState::Starting;
                    self.msg(Level::Info, "TURBINES : SÉQUENCE DE DÉMARRAGE (STARTER / IGN)");
                }
            }
            TurbState::Starting | TurbState::Running => {
                self.eng.turb = TurbState::Off;
                self.msg(Level::Info, "TURBINES : COUPURE (FUEL CUTOFF)");
            }
            TurbState::Failed => self.msg(Level::Warning, "TURBINES HS — RALLUMAGE IMPOSSIBLE"),
        }
    }

    pub fn cmd_inlet(&mut self) {
        if self.eng.inlet_open {
            if self.eng.turb == TurbState::Running || self.eng.turb == TurbState::Starting {
                self.eng.turb = TurbState::Off;
                self.msg(Level::Caution, "FERMETURE ENTRÉES TURBINE : TURBINES COUPÉES AUTOMATIQUEMENT");
            }
            self.eng.inlet_open = false;
            self.msg(Level::Info, "ENTRÉES D'AIR TURBINE : FERMÉES — FLUX VERS STATO");
        } else if self.d.mach > 2.5 {
            self.msg(Level::Caution, "OUVERTURE ENTRÉES REFUSÉE : MACH > 2.5 (RISQUE DÉSAMORÇAGE)");
        } else {
            self.eng.inlet_open = true;
            self.msg(Level::Info, "ENTRÉES D'AIR TURBINE : OUVERTES");
        }
    }

    pub fn ram_ignition_check(&self) -> Result<(), String> {
        let (m, q, alt_ft) = (self.d.mach, self.d.q, self.alt / FT);
        if m < 2.5 {
            return Err(format!("MACH {m:.2} < 2.50"));
        }
        if alt_ft < 40_000.0 {
            return Err(format!("ALT {alt_ft:.0} FT < FL400"));
        }
        if !(15_000.0..=95_000.0).contains(&q) {
            return Err(format!("q {:.0} kPa HORS 15-95", q / 1000.0));
        }
        Ok(())
    }

    pub fn cmd_ram(&mut self) {
        match self.eng.ram {
            RamState::Off => {
                if self.d.mach < 2.0 {
                    self.msg(Level::Caution, "ARMEMENT STATO REFUSÉ : MACH < 2.0");
                } else {
                    self.eng.ram = RamState::Armed;
                    self.msg(Level::Info, "STATORÉACTEUR ARMÉ — CONDITIONS D'ALLUMAGE : M>=2.5, >=FL400, q 15-95 kPa");
                }
            }
            RamState::Armed => match self.ram_ignition_check() {
                Ok(()) => {
                    self.eng.ram = RamState::Lit;
                    self.msg(Level::Info, "STATORÉACTEUR ALLUMÉ — RAMJET LIGHT-OFF");
                }
                Err(e) => self.msg(Level::Caution, format!("ALLUMAGE STATO REFUSÉ : {e}")),
            },
            RamState::Lit => {
                self.eng.ram = RamState::Off;
                self.msg(Level::Info, "STATORÉACTEUR COUPÉ");
            }
        }
    }

    pub fn scram_ignition_check(&self) -> Result<(), String> {
        let (m, q, a) = (self.d.mach, self.d.q, self.d.alpha.to_degrees());
        if self.eng.ram != RamState::Lit {
            return Err("TRANSITION DEPUIS STATO ALLUMÉ REQUISE".into());
        }
        if m < 4.8 {
            return Err(format!("MACH {m:.2} < 4.80"));
        }
        if !(25_000.0..=90_000.0).contains(&q) {
            return Err(format!("q {:.0} kPa HORS 25-90", q / 1000.0));
        }
        if !(0.0..=7.0).contains(&a) {
            return Err(format!("AoA {a:.1}° HORS 0-7°"));
        }
        Ok(())
    }

    pub fn cmd_scram(&mut self) {
        match self.eng.scram {
            ScramState::Off => {
                if self.d.mach < 4.0 {
                    self.msg(Level::Caution, "ARMEMENT SCRAM REFUSÉ : MACH < 4.0");
                } else {
                    if self.h2 <= 0.0 {
                        self.msg(Level::Caution, "SCRAM ENABLE REFUSÉ : RÉSERVOIR H2 VIDE");
                        return;
                    }
                    self.eng.scram = ScramState::Armed;
                    self.msg(Level::Info, "SCRAM ENABLE — CIRCUIT H2 PRESSURISÉ, REFROIDISSEMENT RÉGÉNÉRATIF ACTIF, ALLUMEUR SILANE PRÊT");
                }
            }
            ScramState::Armed | ScramState::Unstart => {
                let relight = self.eng.scram == ScramState::Unstart;
                let check = if relight && self.eng.ram != RamState::Lit {
                    // Rallumage direct autorisé après désamorçage si les conditions sont tenues.
                    let mut c = self.scram_ignition_check();
                    if let Err(e) = &c {
                        if e.starts_with("TRANSITION") {
                            c = Ok(());
                        }
                    }
                    c
                } else {
                    self.scram_ignition_check()
                };
                match check {
                    Ok(()) => {
                        self.eng.scram = ScramState::Lit;
                        if self.eng.ram == RamState::Lit {
                            self.eng.ram = RamState::Off;
                        }
                        self.msg(Level::Info, "SCRAMJET ALLUMÉ — MODE SUPERSONIQUE DE COMBUSTION");
                    }
                    Err(e) => self.msg(Level::Caution, format!("ALLUMAGE SCRAM REFUSÉ : {e}")),
                }
            }
            ScramState::Lit => {
                self.eng.scram = ScramState::Off;
                self.msg(Level::Info, "SCRAMJET COUPÉ");
            }
        }
    }

    pub fn cmd_gear(&mut self) {
        if self.on_ground {
            self.msg(Level::Caution, "TRAIN : VERROUILLAGE SOL ACTIF");
            return;
        }
        self.gear_down = !self.gear_down;
        self.msg(Level::Info, if self.gear_down { "TRAIN : SORTIE" } else { "TRAIN : RENTRÉE" });
    }

    pub fn mode_name(&self) -> &'static str {
        let t = self.eng.turb == TurbState::Running;
        let r = self.eng.ram == RamState::Lit;
        let s = self.eng.scram == ScramState::Lit;
        match (t, r, s) {
            (_, _, true) => "SCRAMJET",
            (true, true, _) => "DUAL TURB+RAM",
            (false, true, _) => "RAMJET",
            (true, false, _) => "TURBINE",
            _ => "PLANÉ / NO THRUST",
        }
    }

    // ---------- Pas de simulation ----------

    pub fn step(&mut self, dt: f64, pitch_in: f64, roll_in: f64) {
        if self.crashed.is_some() {
            return;
        }
        let elev = ground_elev(self.lat, self.lon);
        let air = isa(self.alt);
        let v = self.tas.max(0.1);
        let mach = v / air.a;
        let q = 0.5 * air.rho * v * v;
        let mass = self.mass();
        let r = R_EARTH + self.alt;
        let g = G0 * (R_EARTH / r).powi(2);
        let agl = self.alt - elev;
        let cla = cl_alpha(mach);
        let alpha = self.theta - self.gamma;

        // --- Pilote automatique ---
        let pitch_rate;
        let roll_rate;
        let qmax = (6.0f64).to_radians() / (1.0 + mach / 3.0);
        let pmax = (35.0f64).to_radians() / (1.0 + mach / 6.0);
        if self.ap.on && !self.on_ground {
            let gamma_cmd = match self.ap.vert {
                VertMode::Alt => {
                    let vs_max = (0.2 * v).clamp(5.0, 120.0);
                    let vs_cmd = ((self.ap.alt_tgt_ft * FT - self.alt) / 25.0).clamp(-vs_max, vs_max);
                    (vs_cmd / v).clamp(-0.5, 0.5).asin()
                }
                VertMode::Gs => {
                    if let Some((_, tla, tlo, th, tel)) = self.approach {
                        // Plan de descente 3° vers le point d'aboutissement (300 m après le seuil)
                        let path_alt = tel + (along_before(tla, tlo, th, self.lat, self.lon) + 300.0) * 3f64.to_radians().tan();
                        -3f64.to_radians() + ((path_alt - self.alt) / (v * 6.0)).clamp(-0.05, 0.05)
                    } else {
                        0.0
                    }
                }
                VertMode::Flare => {
                    let vs_cmd = -(agl * 0.18).clamp(0.45, 4.0);
                    (vs_cmd / v).asin()
                }
            };
            // Protection basse vitesse : on rend de l'altitude si l'incidence approche la butée
            let a_deg = alpha.to_degrees();
            let gamma_cmd = if a_deg > 13.0 { gamma_cmd.min(-0.04 - (a_deg - 13.0).to_radians() * 2.0) } else { gamma_cmd };
            let c = (q * S_WING * cla / (mass * v)).max(0.01);
            let (wn, zeta) = (0.6, 0.8);
            let k = (wn * wn / c).min(5.0);
            let kd = ((2.0 * zeta * wn) / c - 1.0).clamp(0.0, 20.0);
            let mut pr = (k * (gamma_cmd - self.gamma) - kd * self.d.gamma_dot).clamp(-qmax * 0.6, qmax * 0.6);
            // Confort : facteur de charge limité à [0.4, 1.7] g sous AP
            if (self.d.nz > 1.7 && pr > 0.0) || (self.d.nz < 0.4 && pr < 0.0) {
                pr = 0.0;
            }
            pitch_rate = pr;

            let hdg_cmd = match self.ap.lat {
                LatMode::Hdg | LatMode::Nav => self.ap.hdg_tgt,
                LatMode::Loc => {
                    if let Some((_, tla, tlo, h, _)) = self.approach {
                        let (e, n) = enu_offset(tla, tlo, self.lat, self.lon);
                        let hr = h.to_radians();
                        let cross = e * hr.cos() - n * hr.sin(); // à droite de l'axe
                        wrap360(h + (-cross / 40.0).clamp(-30.0, 30.0))
                    } else {
                        self.ap.hdg_tgt
                    }
                }
            };
            let err = wrap180(hdg_cmd - self.hdg_deg());
            let bank_lim = if mach > 3.0 { 35.0 } else { 30.0 };
            let phi_cmd = (err * 2.0).clamp(-bank_lim, bank_lim).to_radians();
            roll_rate = (1.5 * (phi_cmd - self.phi)).clamp(-pmax * 0.5, pmax * 0.5);

            if self.ap.vert == VertMode::Gs && agl < 60.0 && self.autoland.is_none() {
                self.ap.on = false;
                self.msg(Level::Caution, "AP OFF — MINIMUMS : ARRONDI MANUEL");
            }
        } else {
            pitch_rate = pitch_in * qmax;
            roll_rate = roll_in * pmax;
        }
        if self.ap.at_on {
            let tas_tgt = match self.ap.spd_keas {
                Some(k) => k * KT / (air.rho / RHO0).sqrt(),
                None => self.ap.mach_tgt * air.a,
            };
            let err_kt = (tas_tgt - v) / KT;
            let rate = (0.006 * err_kt - 0.08 * self.d.accel).clamp(-0.3, 0.3);
            self.throttle = (self.throttle + rate * dt).clamp(0.0, 1.0);
        }

        // --- Assiette (loi de vol FBW : commande en vitesse de tangage, protections) ---
        if self.on_ground {
            if v > 55.0 {
                self.theta += pitch_rate.max(-qmax) * dt;
            } else {
                self.theta -= 3f64.to_radians() * dt;
            }
            if pitch_rate <= 0.0 && v < 65.0 {
                self.theta -= 1.5f64.to_radians() * dt;
            }
            self.theta = self.theta.clamp(0.0, 13f64.to_radians());
            self.phi = 0.0;
            let steer = (12f64).to_radians() * (1.0 - (v / 80.0)).max(0.08);
            self.psi += roll_in * steer * dt;
        } else {
            self.theta += pitch_rate * dt;
            let nmax = if mach > 4.0 { 2.0 } else { 3.0 };
            let alpha_g = nmax * mass * g / (q.max(1.0) * S_WING * cla);
            let mut amax = alpha_g.min(20f64.to_radians());
            if self.eng.scram == ScramState::Lit {
                // Loi FBW : protection d'incidence de l'entrée d'air (plus stricte sous AP)
                let lim: f64 = if self.ap.on { 6.5 } else { 9.0 };
                amax = amax.min(lim.to_radians());
            }
            let amin = -5f64.to_radians();
            self.theta = self.theta.clamp(self.gamma + amin, self.gamma + amax);
            self.phi = (self.phi + roll_rate * dt).clamp(-80f64.to_radians(), 80f64.to_radians());
        }
        if self.jolt > 0.0 {
            self.jolt -= dt;
            self.phi += (self.jolt * 37.0).sin() * 0.02;
        }
        let alpha_now = if self.on_ground { self.theta } else { self.theta - self.gamma };
        let alpha = alpha_now;

        // --- Aérodynamique ---
        let a_stall = 20f64.to_radians();
        let mut cl = if alpha <= a_stall { cla * alpha } else { cla * a_stall * (1.0 - (alpha - a_stall) * 2.0).max(0.3) };
        if agl < 30.0 {
            cl *= 1.0 + 0.15 * (1.0 - agl.max(0.0) / 30.0);
        }
        let k_ind = if mach < 0.9 { 0.22 } else { (1.0 / cla).max(0.3) };
        let keas = v * (air.rho / RHO0).sqrt() / KT;
        let mut cd = cd0(mach) + k_ind * cl * cl + 0.015 * self.gear_pos;
        if self.speedbrake {
            cd += if mach < 2.0 { 0.025 } else { 0.006 };
        }
        let lift = q * S_WING * cl;
        let drag = q * S_WING * cd;

        // --- Propulsion ---
        let thrust = self.engines(dt, mach, &air, q, alpha);

        // --- Dynamique du point matériel ---
        let gamma_prev = self.gamma;
        let vert_force = lift + thrust * alpha.sin();
        if self.on_ground {
            let n = (mass * g - vert_force).max(0.0);
            let mu = if self.brakes { 0.42 } else { 0.018 };
            let fric = if self.tas > 0.05 { mu * n } else { 0.0 };
            let acc = (thrust * alpha.cos() - drag - fric) / mass;
            self.tas = (self.tas + acc * dt).max(0.0);
            if self.tas < 0.05 && thrust < mu * n {
                self.tas = 0.0;
            }
            self.gamma = 0.0;
            self.alt = elev;
            if vert_force > mass * g && self.theta > 0.02 {
                self.on_ground = false;
                self.gamma = 0.002;
                self.msg(Level::Info, format!("DÉCOLLAGE — LIFTOFF {keas:.0} KEAS"));
            }
            if self.tas > 15.0 && !AIRPORTS.iter().any(|a| a.on_runway(self.lat, self.lon)) {
                self.crash("SORTIE DE PISTE");
            }
        } else {
            let acc = (thrust * alpha.cos() - drag) / mass - g * self.gamma.sin();
            self.tas = (self.tas + acc * dt).max(1.0);
            let v = self.tas;
            self.gamma += (vert_force * self.phi.cos() / (mass * v) - (g / v - v / r) * self.gamma.cos()) * dt;
            self.gamma = self.gamma.clamp(-1.5, 1.5);
            self.psi += vert_force * self.phi.sin() / (mass * v * self.gamma.cos().max(0.05)) * dt;
        }
        self.psi = self.psi.rem_euclid(std::f64::consts::TAU);

        // --- Navigation ---
        let v = self.tas;
        self.alt += v * self.gamma.sin() * dt;
        let gs = v * self.gamma.cos();
        self.lat += (gs * self.psi.cos() / r * dt).to_degrees();
        self.lon += (gs * self.psi.sin() / (r * self.lat.to_radians().cos().max(0.01)) * dt).to_degrees();
        if self.lat.abs() > 89.5 {
            self.lat = self.lat.signum() * (179.0 - self.lat.abs());
            self.lon += 180.0;
            self.psi = (self.psi + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
        }
        self.lon = wrap180(self.lon);

        // --- Train, thermique ---
        let gt = if self.gear_down { 1.0 } else { 0.0 };
        self.gear_pos += (gt - self.gear_pos).clamp(-dt / 8.0, dt / 8.0);
        let teq = skin_equilibrium(self.alt, v);
        self.skin_temp += (teq - self.skin_temp) * (dt / 15.0).min(1.0);

        // --- Dérivées pour l'affichage ---
        self.d = Derived {
            mach: v / air.a,
            q,
            keas,
            alpha,
            nz: vert_force / (mass * g),
            vs: v * self.gamma.sin(),
            agl: self.alt - elev,
            elev,
            thrust,
            drag,
            oat: air.t - 273.15,
            accel: if self.on_ground { 0.0 } else { (thrust * alpha.cos() - drag) / mass - g * self.gamma.sin() },
            gamma_dot: (self.gamma - gamma_prev) / dt,
        };
        self.max_mach = self.max_mach.max(self.d.mach);
        self.max_alt = self.max_alt.max(self.alt);

        // --- Contact sol / limites ---
        if !self.on_ground && self.alt <= elev {
            self.touchdown(elev);
        }
        if q > 130_000.0 {
            self.crash("RUPTURE STRUCTURELLE — PRESSION DYNAMIQUE EXCESSIVE");
        }
        if self.d.nz > 5.5 || self.d.nz < -2.5 {
            self.crash("RUPTURE STRUCTURELLE — FACTEUR DE CHARGE");
        }
        if self.skin_temp - 273.15 > 1400.0 {
            self.crash("DÉFAILLANCE THERMIQUE DE LA STRUCTURE");
        }
    }

    fn touchdown(&mut self, elev: f64) {
        let vs = self.tas * self.gamma.sin();
        let fpm = vs / FT * 60.0;
        let on_rwy = AIRPORTS.iter().any(|a| a.on_runway(self.lat, self.lon));
        self.alt = elev;
        if self.gear_pos < 0.99 {
            self.crash("ATTERRISSAGE TRAIN RENTRÉ");
        } else if !on_rwy {
            self.crash(if elev <= 1.0 { "IMPACT — HORS PISTE / OCÉAN" } else { "IMPACT — HORS PISTE" });
        } else if vs < -4.0 {
            self.crash(&format!("IMPACT {fpm:.0} FT/MIN — TRAIN DÉTRUIT"));
        } else if self.phi.abs() > 12f64.to_radians() {
            self.crash("SAILLAGE AILE — INCLINAISON EXCESSIVE AU TOUCHER");
        } else if self.d.keas > 260.0 {
            self.crash("ÉCLATEMENT PNEUS — VITESSE DE TOUCHER EXCESSIVE");
        } else {
            self.on_ground = true;
            self.gamma = 0.0;
            self.phi = 0.0;
            self.ap.on = false;
            let l = if vs < -2.5 { Level::Caution } else { Level::Info };
            let q = if vs < -2.5 { "DUR" } else if vs > -1.0 { "KISS" } else { "BON" };
            self.msg(l, format!("TOUCHER DES ROUES : {fpm:.0} FT/MIN ({q}) à {:.0} KEAS", self.d.keas));
        }
    }

    fn engines(&mut self, dt: f64, mach: f64, air: &Air, q: f64, alpha: f64) -> f64 {
        let th = self.throttle;
        let sigma = air.rho / RHO0;
        let has_fuel = self.fuel > 0.0;
        let has_h2 = self.h2 > 0.0;
        let e = &mut self.eng;
        let dt_door = if e.inlet_open { 0.0 } else { 1.0 };
        e.doors += (dt_door - e.doors).clamp(-dt / 6.0, dt / 6.0);
        let mut ev: Vec<(Level, String)> = Vec::new();

        // Turbines (2 x turboréacteurs à PC)
        match e.turb {
            TurbState::Starting => {
                e.n2 += dt / 25.0 * 0.62;
                if e.n2 >= 0.62 {
                    e.turb = TurbState::Running;
                    ev.push((Level::Info, "TURBINES : RALENTI STABILISÉ (IDLE) — N2 62%".into()));
                }
            }
            TurbState::Running => {
                let tgt = 0.62 + 0.38 * (th / 0.8).min(1.0);
                e.n2 += (tgt - e.n2) * (dt / 3.0).min(1.0);
            }
            _ => {
                let wm = (mach * 0.15).min(0.3);
                e.n2 += (wm - e.n2) * (dt / 5.0).min(1.0);
            }
        }
        if !has_fuel && matches!(e.turb, TurbState::Running | TurbState::Starting) {
            e.turb = TurbState::Off;
            ev.push((Level::Warning, "EXTINCTION TURBINES — PLUS DE CARBURANT".into()));
        }
        e.ab_frac = if e.turb == TurbState::Running { ((th - 0.8) / 0.2).clamp(0.0, 1.0) } else { 0.0 };
        e.t_turb = if e.turb == TurbState::Running && e.inlet_open {
            let nf = ((e.n2 - 0.62) / 0.38).clamp(0.0, 1.0).powf(1.5) * 0.92 + 0.08;
            let mf = (1.0 + 0.7 * mach.min(2.6)) * ((3.3 - mach) / 0.7).clamp(0.0, 1.0);
            2.0 * 120_000.0 * sigma.powf(0.75) * mf * nf * (1.0 + 0.65 * e.ab_frac)
        } else {
            0.0
        };
        let egt_tgt = if e.turb == TurbState::Running || e.turb == TurbState::Starting {
            if e.n2 < 0.2 {
                air.t - 273.15
            } else {
                400.0 + 450.0 * ((e.n2 - 0.6) / 0.4).max(0.0) + 150.0 * e.ab_frac + (mach - 2.8).max(0.0) * 900.0
            }
        } else if e.doors < 0.5 {
            // Moulinet : l'air d'impact traverse la turbine arrêtée
            air.t - 273.15 + 30.0 * mach * mach
        } else {
            // Portes fermées : cavité isolée, simple échauffement par conduction
            air.t - 273.15 + 3.0 * mach * mach
        };
        e.egt += (egt_tgt - e.egt) * (dt / 2.0).min(1.0);
        if e.turb == TurbState::Running && mach > 3.2 {
            e.overtemp_t += dt;
            if e.overtemp_t > 8.0 {
                e.turb = TurbState::Failed;
                ev.push((Level::Warning, "SURCHAUFFE TURBINES — MOTEURS DÉTRUITS (COUPER AVANT M3.2 !)".into()));
            }
        } else {
            e.overtemp_t = (e.overtemp_t - dt).max(0.0);
        }

        // Statoréacteur
        if e.ram == RamState::Lit {
            if mach < 1.8 || q < 10_000.0 || mach > 7.0 || !has_fuel {
                e.ram = RamState::Off;
                ev.push((Level::Warning, "EXTINCTION STATORÉACTEUR (FLAMEOUT)".into()));
            }
        }
        e.t_ram = if e.ram == RamState::Lit { th * q * ram_k(mach) } else { 0.0 };
        let rt = if e.ram == RamState::Lit { 600.0 + 1500.0 * th } else { air.t - 273.15 };
        e.ram_temp += (rt - e.ram_temp) * (dt / 1.5).min(1.0);

        // Superstatoréacteur
        let a_deg = alpha.to_degrees();
        e.scram_margin = ((8.0 - a_deg) / 8.0).min((q - 18_000.0) / 30_000.0).min(mach - 4.5).min(a_deg + 2.0).clamp(0.0, 1.0);
        match e.scram {
            ScramState::Lit => {
                if a_deg > 8.0 || a_deg < -2.0 || q < 18_000.0 || mach < 4.5 || !has_h2 {
                    e.scram = ScramState::Unstart;
                    e.unstart_t = 0.0;
                    self.jolt = 1.5;
                    ev.push((Level::Warning, format!("SCRAM UNSTART ! (AoA {a_deg:.1}°, q {:.0} kPa, M{mach:.2})", q / 1000.0)));
                }
            }
            ScramState::Unstart => {
                e.unstart_t += dt;
            }
            _ => {}
        }
        e.t_scram = if e.scram == ScramState::Lit { th * q * scram_k(mach) } else { 0.0 };

        // Débit carburant (Isp effectifs)
        let isp_turb = 3800.0 + (1700.0 - 3800.0) * e.ab_frac;
        let mut ff = e.t_turb / (isp_turb * G0) + e.t_ram / (1600.0 * G0);
        if matches!(e.turb, TurbState::Running | TurbState::Starting) {
            ff += 0.25;
        }
        if !has_fuel {
            e.t_turb = 0.0;
            e.t_ram = 0.0;
        }
        e.fuel_flow = if has_fuel { ff } else { 0.0 };
        // Hydrogène liquide : Isp élevé, réservé au scramjet
        e.h2_flow = if has_h2 { e.t_scram / (3300.0 * G0) } else { 0.0 };
        if !has_h2 {
            e.t_scram = 0.0;
        }
        let total = e.t_turb + e.t_ram + e.t_scram;
        self.fuel = (self.fuel - self.eng.fuel_flow * dt).max(0.0);
        let dump = if self.h2_dump { 40.0 } else { 0.0 };
        self.h2 = (self.h2 - (self.eng.h2_flow + dump) * dt).max(0.0);
        if self.h2 <= 0.0 && self.h2_dump {
            self.h2_dump = false;
            ev.push((Level::Info, "EMER H2 DUMP TERMINÉ — RÉSERVOIR H2 VIDE".into()));
        }
        self.events.extend(ev);
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vol complet scripté : décollage, montée, stato, scram, Mach 10.
    #[test]
    fn profile() {
        let mut ac = Aircraft::new_on_runway(0);
        let dt = 1.0 / 120.0;
        let mut t = 0.0;
        let mut last_print = -100.0;
        ac.cmd_turbine();
        let mut phase = 0;
        while t < 4000.0 {
            let d = ac.d;
            let alt_ft = ac.alt / FT;
            let mut pitch = 0.0;
            match phase {
                0 => {
                    if ac.eng.turb == TurbState::Running {
                        ac.brakes = false;
                        ac.throttle = 1.0;
                        phase = 1;
                    }
                }
                1 => {
                    if d.keas > 170.0 {
                        pitch = if ac.theta < 10f64.to_radians() { 1.0 } else { 0.0 };
                    }
                    if !ac.on_ground && d.agl > 150.0 {
                        ac.cmd_gear();
                        ac.ap.on = true;
                        ac.ap.alt_tgt_ft = 35_000.0;
                        ac.ap.hdg_tgt = ac.hdg_deg();
                        phase = 2;
                    }
                }
                2 => {
                    if d.mach > 1.6 && alt_ft > 34_000.0 {
                        ac.ap.alt_tgt_ft = 50_000.0;
                    }
                    if d.mach > 2.05 && ac.eng.ram == RamState::Off {
                        ac.cmd_ram();
                    }
                    if ac.eng.ram == RamState::Armed && ac.ram_ignition_check().is_ok() {
                        ac.cmd_ram();
                    }
                    if ac.eng.ram == RamState::Lit {
                        ac.ap.alt_tgt_ft = 65_000.0;
                        if d.mach > 2.8 {
                            ac.cmd_turbine();
                            ac.cmd_inlet();
                            phase = 3;
                        }
                    }
                }
                3 => {
                    if d.mach > 3.5 {
                        ac.ap.alt_tgt_ft = 85_000.0;
                    }
                    if d.mach > 4.0 && ac.eng.scram == ScramState::Off {
                        ac.cmd_scram();
                    }
                    if ac.eng.scram == ScramState::Armed && ac.scram_ignition_check().is_ok() {
                        ac.cmd_scram();
                        phase = 4;
                    }
                }
                4 => {
                    ac.ap.alt_tgt_ft = if d.mach < 6.0 { 88_000.0 } else if d.mach < 8.0 { 98_000.0 } else { 108_000.0 };
                    if d.mach > 10.0 && phase == 4 {
                        println!(">>> MACH 10 à t={t:.0}s  JP={:.0} H2={:.0}", ac.fuel, ac.h2);
                        phase = 5;
                    }
                }
                _ => {}
            }
            ac.step(dt, pitch, 0.0);
            for (_, m) in ac.events.drain(..) {
                println!("[{t:7.1}] {m}");
            }
            if let Some(c) = &ac.crashed {
                println!("CRASH {c}");
                break;
            }
            if t - last_print >= 30.0 {
                last_print = t;
                println!(
                    "t={t:6.0} ph={phase} M={:5.2} alt={:7.0}ft keas={:4.0} q={:5.1}k a={:5.1} g={:4.2} T={:4.0}kN D={:4.0}kN fuel={:5.0} skin={:5.0}C",
                    d.mach, alt_ft, d.keas, d.q / 1000.0, d.alpha.to_degrees(), d.nz, d.thrust / 1000.0, d.drag / 1000.0, ac.fuel, ac.skin_temp - 273.15
                );
            }
            if phase == 5 || ac.fuel <= 0.0 {
                break;
            }
            t += dt;
        }
        assert!(ac.crashed.is_none());
    }

    fn run(ac: &mut Aircraft, max_t: f64, until: impl Fn(&Aircraft) -> bool) -> f64 {
        let dt = 1.0 / 120.0;
        let mut t = 0.0;
        let mut lp = -100.0;
        while t < max_t && !until(ac) && ac.crashed.is_none() {
            let cmd = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, cmd.0, cmd.1);
            for (_, m) in ac.events.drain(..) {
                println!("[{t:6.1}] {m}");
            }
            if t - lp > 20.0 {
                lp = t;
                println!("t={t:5.0} keas={:4.0} agl={:6.0}ft vs={:5.0}fpm thr={:.2} hdg={:3.0} gear={:.1}", ac.d.keas, ac.d.agl / FT, ac.d.vs / FT * 60.0, ac.throttle, ac.hdg_deg(), ac.gear_pos);
            }
            t += dt;
        }
        t
    }

    #[test]
    fn autoland_and_takeoff() {
        // Décollage auto depuis Palmdale
        let mut ac = Aircraft::new_on_runway(0);
        ac.eng.turb = TurbState::Running;
        ac.eng.n2 = 0.62;
        ac.engage_takeoff(12_000.0).unwrap();
        run(&mut ac, 600.0, |a| a.alt / FT > 11_500.0 && a.autotakeoff.is_none());
        assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        // Autoland à Edwards depuis la position courante
        let edw = crate::airports::AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        ac.engage_autoland(edw).unwrap();
        run(&mut ac, 3000.0, |a| a.autoland.is_none());
        println!("crash={:?} sol={} tas={:.1}", ac.crashed, ac.on_ground, ac.tas);
        assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
        assert!(ac.on_ground && ac.tas < 3.0);
    }
}
