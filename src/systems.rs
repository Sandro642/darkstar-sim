//! Systèmes avion pour l'essai : pannes injectables, circuit électrique (génératrices, pile à
//! combustible H2/O2, batterie), environnement de la baie équipements, limites des chambres de
//! combustion et des gouvernes (partagées entre la physique et les graphiques de la page LIMITES).

/// Pannes injectables par l'ingénieur d'essai.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    EngL,
    EngR,
    GenL,
    GenR,
    FuelPump,
    Adc,
    FcsDirect,
    Hyd,
    ScramCool,
    InletJam,
    GearJam,
    H2Leak,
}

/// (panne, code d'instruction, libellé)
pub const FAILS: [(Fail, &str, &str); 12] = [
    (Fail::EngL, "ENGL", "MOTEUR GAUCHE"),
    (Fail::EngR, "ENGR", "MOTEUR DROIT"),
    (Fail::GenL, "GENL", "GENERATRICE G"),
    (Fail::GenR, "GENR", "GENERATRICE D"),
    (Fail::FuelPump, "POMPE", "POMPE CARBURANT"),
    (Fail::Adc, "ADC", "CENTRALE AERO (ADC)"),
    (Fail::FcsDirect, "FCS", "FCS : LOI DIRECTE"),
    (Fail::Hyd, "HYD", "HYDRAULIQUE"),
    (Fail::ScramCool, "REFROID", "REFROIDISSEMENT SCRAM"),
    (Fail::InletJam, "PORTES", "PORTES D'ENTREE BLOQUEES"),
    (Fail::GearJam, "TRAIN", "TRAIN BLOQUE"),
    (Fail::H2Leak, "FUITEH2", "FUITE HYDROGENE"),
];

#[derive(Clone, Default)]
pub struct Failures {
    active: [bool; 12],
}

impl Failures {
    fn idx(f: Fail) -> usize {
        FAILS.iter().position(|(x, _, _)| *x == f).unwrap()
    }
    pub fn has(&self, f: Fail) -> bool {
        self.active[Self::idx(f)]
    }
    pub fn set(&mut self, f: Fail, on: bool) {
        self.active[Self::idx(f)] = on;
    }
}

pub fn fail_by_code(code: &str) -> Option<Fail> {
    FAILS.iter().find(|(_, c, _)| *c == code).map(|(f, _, _)| *f)
}

/// Circuit électrique et environnement (panneau BATT / FUEL CELL / TEMP du cockpit).
#[derive(Clone)]
pub struct Elec {
    pub batt: f64,       // 0..1
    pub o2: f64,         // réserve d'oxygène de la pile, 0..1
    pub fc_on: bool,     // pile à combustible en service
    pub fc_temp: f64,    // °C
    pub fc_kw: f64,      // puissance fournie par la pile
    pub gen_kw: f64,     // puissance fournie par les génératrices
    pub bus_ok: bool,
    pub ebay_temp: f64,  // °C baie équipements
    pub ckpt_temp: f64,  // °C poste (inoccupé)
    pub temp_knob: f64,  // 0 = COLD .. 1 = WARM
    pub faceheat: u8,    // 0 = OFF, 1..7
    pub sel_ebay: bool,  // sélecteur d'indication : E BAY (sinon CKPT)
    pub auto_temp: bool, // HOLD AUTO TEMP
    pub lost: bool,      // perte totale d'alimentation
}

pub const ELEC_LOAD_KW: f64 = 40.0;
pub const GEN_KW: f64 = 60.0;
pub const FC_MAX_KW: f64 = 50.0;

impl Elec {
    pub fn new() -> Self {
        Elec {
            batt: 1.0,
            o2: 1.0,
            fc_on: false,
            fc_temp: 20.0,
            fc_kw: 0.0,
            gen_kw: 0.0,
            bus_ok: true,
            ebay_temp: 20.0,
            ckpt_temp: 20.0,
            temp_knob: 0.35,
            faceheat: 0,
            sel_ebay: true,
            auto_temp: true,
            lost: false,
        }
    }

    /// `gens` : nombre de génératrices en ligne ; renvoie la consommation H2 de la pile (kg/s).
    pub fn update(&mut self, dt: f64, gens: u32, h2_avail: bool, skin_c: f64) -> f64 {
        self.gen_kw = gens as f64 * GEN_KW;
        let deficit = (ELEC_LOAD_KW - self.gen_kw).max(0.0);
        let mut h2_use = 0.0;
        self.fc_on = deficit > 0.0 && h2_avail && self.o2 > 0.0;
        self.fc_kw = if self.fc_on { deficit.min(FC_MAX_KW) } else { 0.0 };
        if self.fc_on {
            // ~0.06 kg H2 par kWh électrique (rendement ~50 %)
            h2_use = self.fc_kw * 0.06 / 3600.0;
            self.o2 = (self.o2 - self.fc_kw / FC_MAX_KW * dt / 7200.0).max(0.0);
        }
        let fc_target = if self.fc_on { 60.0 + 100.0 * self.fc_kw / FC_MAX_KW } else { 20.0 };
        self.fc_temp += (fc_target - self.fc_temp) * (dt / 60.0).min(1.0);
        let supplied = self.gen_kw + self.fc_kw;
        if supplied >= ELEC_LOAD_KW {
            self.batt = (self.batt + dt / 1800.0).min(1.0);
        } else {
            // Batterie seule : ~30 min à pleine charge du bus
            let short = (ELEC_LOAD_KW - supplied) / ELEC_LOAD_KW;
            self.batt = (self.batt - short * dt / 1800.0).max(0.0);
        }
        self.bus_ok = supplied >= ELEC_LOAD_KW || self.batt > 0.0;
        if !self.bus_ok {
            self.lost = true;
        }
        // Baie équipements : refroidie par la boucle carburant, consigne du sélecteur TEMP
        let target = if self.auto_temp { 18.0 + 20.0 * self.temp_knob } else { 18.0 + 20.0 * self.temp_knob + skin_c.max(0.0) * 0.02 };
        let load_heat = if self.bus_ok { 6.0 } else { 0.0 };
        self.ebay_temp += (target + load_heat - self.ebay_temp) * (dt / 90.0).min(1.0);
        let ck_target = 16.0 + 14.0 * self.temp_knob + self.faceheat as f64 * 1.5 + skin_c.max(0.0) * 0.01;
        self.ckpt_temp += (ck_target - self.ckpt_temp) * (dt / 120.0).min(1.0);
        h2_use
    }

    /// État de charge affiché sous la pile (« STS »), en %.
    pub fn fc_status(&self) -> f64 {
        (self.o2 * 100.0).min(100.0)
    }
}

// ---------- Chambres de combustion ----------

pub const RAM_WALL_BURST: f64 = 1650.0;
pub const SCRAM_WALL_BURST: f64 = 1850.0;
pub const RAM_WALL_CAUTION: f64 = 1400.0;
pub const SCRAM_WALL_CAUTION: f64 = 1600.0;

/// Manette (levier) → (poussée effective, excès de carburant) : saturation physique du moteur.
pub fn lever_effect(lever: f64) -> (f64, f64) {
    let th = if lever <= 1.0 { lever } else { 1.0 + 0.4 * (1.0 - (-(lever - 1.0) / 0.4).exp()) };
    (th, ((lever - 1.0).max(0.0) - (th - 1.0).max(0.0)).max(0.0))
}

fn stag_rise(mach: f64, oat_k: f64) -> f64 {
    oat_k * 0.2 * mach * mach
}

/// Température de paroi de la chambre du statoréacteur (°C).
pub fn ram_wall(mach: f64, lever: f64, oat_k: f64) -> f64 {
    let (th, rich) = lever_effect(lever);
    500.0 + 600.0 * th + 350.0 * rich + 0.12 * stag_rise(mach, oat_k)
}

/// Température de paroi de la chambre du scramjet (°C), refroidie par l'hydrogène.
pub fn scram_wall(mach: f64, lever: f64, oat_k: f64, cooling_failed: bool) -> f64 {
    let (th, rich) = lever_effect(lever);
    700.0 + 260.0 * th + 900.0 * (th - 1.0).max(0.0) + 350.0 * rich + 0.06 * stag_rise(mach, oat_k) + if cooling_failed { 420.0 } else { 0.0 }
}

/// Manette maximale avant d'atteindre une température donnée (recherche sur 0..200 %).
pub fn lever_for_wall(f: impl Fn(f64) -> f64, wall: f64) -> f64 {
    let mut lever = 0.0;
    while lever < 2.0 {
        if f(lever + 0.01) > wall {
            return lever;
        }
        lever += 0.01;
    }
    2.0
}

// ---------- Gouvernes ----------

/// Charge de charnière admissible des élevons : q (kPa) × braquage (deg).
pub const HINGE_LIMIT: f64 = 3600.0;
pub const DEFL_MAX: f64 = 30.0;

/// Braquage autorisé par la loi normale (limiteur), en degrés.
pub fn defl_limit_normal(q_kpa: f64) -> f64 {
    (0.5 * HINGE_LIMIT / q_kpa.max(1.0)).min(DEFL_MAX)
}

/// Braquage provoquant la rupture des gouvernes, en degrés.
pub fn defl_destroy(q_kpa: f64) -> f64 {
    (HINGE_LIMIT / q_kpa.max(1.0)).min(90.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft::*;
    use crate::geo::FT;

    fn hyper(mach: f64, alt_ft: f64) -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = alt_ft * FT;
        ac.tas = mach * crate::atmosphere::isa(ac.alt).a;
        ac.skin_temp = skin_equilibrium(ac.alt, ac.tas);
        ac
    }

    /// Gouvernes : manche à fond à forte q -> limiteur en loi normale, rupture sous FBW OVERRIDE.
    #[test]
    fn fcs_destruction_vs_limiter() {
        for ovr in [false, true] {
            let mut ac = hyper(2.2, 32_000.0); // q ~ 95 kPa
            if ovr {
                ac.cmd_override_fbw(true);
            }
            for _ in 0..(5 * 120) {
                ac.step(1.0 / 120.0, 1.0, 1.0);
                if ac.crashed.is_some() {
                    break;
                }
            }
            println!("FBW OVERRIDE {ovr} : q {:.0} kPa, braquage {:.1} deg, crash {:?}", ac.d.q / 1000.0, ac.fcs.defl_cmd, ac.crashed);
            if ovr {
                assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("GOUVERNES") || c.contains("FACTEUR")), "{:?}", ac.crashed);
            } else {
                assert!(!ac.crashed.as_deref().is_some_and(|c| c.contains("GOUVERNES")));
            }
        }
    }

    /// Électricité : sans génératrice ni hydrogène, la batterie se vide puis la liaison est perdue.
    #[test]
    fn elec_battery_drain() {
        let mut ac = hyper(3.0, 70_000.0);
        ac.h2 = 0.0;
        ac.elec.batt = 0.02;
        ac.ap.on = true;
        ac.ap.alt_tgt_ft = 70_000.0;
        ac.ap.hdg_tgt = ac.hdg_deg();
        for _ in 0..(120 * 120) {
            ac.step(1.0 / 120.0, 0.0, 0.0);
            if ac.crashed.is_some() {
                break;
            }
        }
        println!("ELEC : batt {:.0} %, crash {:?}", ac.elec.batt * 100.0, ac.crashed);
        assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("ALIMENTATION")));
    }

    /// Train bloqué : la commande normale ne sort pas le train, la sortie de secours si.
    #[test]
    fn gear_jam_and_alternate() {
        let mut ac = hyper(0.4, 8_000.0);
        ac.set_failure(Fail::GearJam, true);
        ac.cmd_gear();
        for _ in 0..(10 * 120) {
            ac.step(1.0 / 120.0, 0.0, 0.0);
        }
        assert!(ac.gear_pos < 0.01, "train bloqué : pas de sortie");
        ac.cmd_gear_alt();
        for _ in 0..(25 * 120) {
            ac.step(1.0 / 120.0, 0.0, 0.0);
        }
        assert!(ac.gear_pos > 0.99, "sortie de secours");
    }

    /// Courbes de seuil cohérentes : le seuil d'explosion est au-dessus de 100 % en usage normal.
    #[test]
    fn burst_threshold_above_normal_range() {
        for m in [3.0, 4.0, 5.0, 6.0] {
            assert!(lever_for_wall(|l| ram_wall(m, l, 220.0), RAM_WALL_BURST) > 1.0, "stato M{m}");
        }
        for m in [6.0, 8.0, 10.0, 11.0] {
            assert!(lever_for_wall(|l| scram_wall(m, l, 225.0, false), SCRAM_WALL_BURST) > 1.0, "scram M{m}");
        }
    }
}

#[cfg(test)]
mod roll_tests {
    use crate::aircraft::*;
    use crate::geo::FT;

    /// Tonneau complet : le roulis n'est plus bloqué et revient à l'horizontale.
    #[test]
    fn full_roll_360() {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 20_000.0 * FT;
        ac.tas = 0.8 * crate::atmosphere::isa(ac.alt).a;
        let mut max_abs: f64 = 0.0;
        let mut total = 0.0;
        let mut prev = ac.phi;
        for _ in 0..(20 * 120) {
            ac.step(1.0 / 120.0, 0.0, 1.0);
            total += wrap_pi(ac.phi - prev);
            prev = ac.phi;
            max_abs = max_abs.max(ac.phi.abs());
            if total > std::f64::consts::TAU {
                break;
            }
        }
        println!("roulis cumulé {:.0} deg, crash {:?}", total.to_degrees(), ac.crashed);
        assert!(max_abs > 170f64.to_radians(), "dos atteint");
        assert!(total > std::f64::consts::TAU, "360 deg effectués");
    }
}

#[cfg(test)]
mod ap_perf_tests {
    use crate::aircraft::*;
    use crate::geo::FT;

    /// Virage de 90° sous AP à Mach 3 / FL700 : AP MAX tourne nettement plus vite que l'AP confort,
    /// sans dépasser le facteur de charge de la loi normale.
    #[test]
    fn ap_max_turns_faster_within_law() {
        let mut times = Vec::new();
        for perf in [false, true] {
            let mut ac = Aircraft::new_on_runway(0);
            ac.on_ground = false;
            ac.gear_down = false;
            ac.gear_pos = 0.0;
            ac.brakes = false;
            ac.alt = 70_000.0 * FT;
            ac.tas = 3.0 * crate::atmosphere::isa(ac.alt).a;
            ac.psi = 0.0;
            ac.set_turbines(TurbState::Running, 0.8);
            ac.ap.on = true;
            ac.ap.vert = VertMode::Alt;
            ac.ap.alt_tgt_ft = 70_000.0;
            ac.ap.lat = LatMode::Hdg;
            ac.ap.hdg_tgt = 90.0;
            ac.ap.max_perf = perf;
            let dt = 1.0 / 60.0;
            let mut t = 0.0;
            let mut nz_max: f64 = 0.0;
            while t < 600.0 && (ac.hdg_deg() - 90.0).abs() > 2.0 {
                ac.step(dt, 0.0, 0.0);
                nz_max = nz_max.max(ac.d.nz);
                t += dt;
            }
            println!("AP {} : virage 90 deg en {t:.0} s, nz max {nz_max:.2}, alt {:.0} ft", if perf { "MAX" } else { "CONFORT" }, ac.alt / FT);
            assert!(ac.crashed.is_none());
            assert!(nz_max <= 3.05, "facteur de charge {nz_max}");
            times.push(t);
        }
        assert!(times[1] < times[0] * 0.7, "AP MAX {:.0} s vs confort {:.0} s", times[1], times[0]);
    }
}
