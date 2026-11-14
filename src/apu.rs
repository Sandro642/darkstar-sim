//! Groupe auxiliaire de puissance (APU) : petite turbine dans la baie arrière.
//! - MASTER puis START : séquence de démarrage (~35 s), pic d'EGT au décrochage du démarreur ;
//! - génératrice APU GEN : alimente le bus (compte comme une génératrice en ligne) ;
//! - prélèvement d'air APU BLEED : démarrage des turbines au sol (25 s au lieu de 45 s) ;
//! - consommation de JP-7 ; démarrage sous le FL250, arrêt automatique au-dessus du FL350 ou de
//!   Mach 0,9 ; arrêt avec refroidissement.

use crate::aircraft::Level;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ApuState {
    #[default]
    Off,
    Starting,
    Running,
    Stopping,
}

#[derive(Clone, Debug, Default)]
pub struct Apu {
    pub master: bool,
    pub state: ApuState,
    /// Régime (0..1)
    pub n: f64,
    /// Température des gaz d'échappement (°C)
    pub egt: f64,
    pub generator: bool,
    pub bleed: bool,
    /// Débit carburant (kg/s)
    pub ff: f64,
}

impl Apu {
    pub fn new() -> Apu {
        Apu { generator: true, bleed: true, egt: 20.0, ..Default::default() }
    }

    pub fn running(&self) -> bool {
        self.state == ApuState::Running
    }

    /// Génératrice APU en ligne.
    pub fn gen_online(&self) -> bool {
        self.running() && self.generator && self.n > 0.95
    }

    /// Air de démarrage disponible.
    pub fn bleed_air(&self) -> bool {
        self.running() && self.bleed && self.n > 0.95
    }

    /// Commande START (MASTER mis sur ON si besoin).
    pub fn start(&mut self, alt_ft: f64, batt_ok: bool, fuel_ok: bool) -> Result<String, String> {
        if matches!(self.state, ApuState::Starting | ApuState::Running) {
            return Err("APU DEJA EN MARCHE".into());
        }
        if !batt_ok {
            return Err("APU : BATTERIE / BUS HORS TENSION".into());
        }
        if !fuel_ok {
            return Err("APU : PAS DE CARBURANT".into());
        }
        if alt_ft > 25_000.0 {
            return Err("APU : DEMARRAGE AU-DESSOUS DU FL250 SEULEMENT".into());
        }
        self.master = true;
        self.state = ApuState::Starting;
        Ok("APU START — SEQUENCE DE DEMARRAGE".into())
    }

    pub fn stop(&mut self) -> Result<String, String> {
        if matches!(self.state, ApuState::Off | ApuState::Stopping) {
            self.master = false;
            return Err("APU DEJA ARRETE".into());
        }
        self.master = false;
        self.state = ApuState::Stopping;
        Ok("APU STOP — REFROIDISSEMENT PUIS ARRET".into())
    }

    /// Une étape : messages éventuels.
    pub fn update(&mut self, dt: f64, alt_ft: f64, mach: f64, fuel_ok: bool, oat: f64) -> Vec<(Level, String)> {
        let mut ev = Vec::new();
        match self.state {
            ApuState::Off => {
                self.n = (self.n - dt / 15.0).max(0.0);
                self.egt += (oat - self.egt) * (dt / 60.0).min(1.0);
            }
            ApuState::Starting => {
                self.n = (self.n + dt / 35.0).min(1.0);
                // Pic d'EGT vers 50 % de régime (allumage), puis stabilisation
                let peak = 690.0 * (1.0 - ((self.n - 0.5) / 0.35).powi(2)).max(0.0);
                let tgt = (oat + 460.0 * self.n).max(peak);
                self.egt += (tgt - self.egt) * (dt / 2.0).min(1.0);
                if self.n >= 1.0 {
                    self.state = ApuState::Running;
                    ev.push((Level::Info, "APU AVAIL — 100 %, GENERATRICE ET AIR DISPONIBLES".into()));
                }
            }
            ApuState::Running => {
                let load = if self.generator { 35.0 } else { 0.0 } + if self.bleed { 70.0 } else { 0.0 };
                self.egt += ((oat + 430.0 + load) - self.egt) * (dt / 4.0).min(1.0);
                self.n += (1.0 - self.n) * (dt / 2.0).min(1.0);
            }
            ApuState::Stopping => {
                self.n = (self.n - dt / 20.0).max(0.0);
                self.egt += (oat - self.egt) * (dt / 25.0).min(1.0);
                if self.n <= 0.0 {
                    self.state = ApuState::Off;
                    ev.push((Level::Info, "APU ARRETE".into()));
                }
            }
        }
        // Protections : carburant, altitude, vitesse
        if matches!(self.state, ApuState::Starting | ApuState::Running) {
            let why = if !fuel_ok {
                Some("PLUS DE CARBURANT")
            } else if alt_ft > 35_000.0 {
                Some("AU-DESSUS DU FL350")
            } else if mach > 0.9 {
                Some("MACH > 0,9")
            } else {
                None
            };
            if let Some(w) = why {
                self.master = false;
                self.state = ApuState::Stopping;
                ev.push((Level::Caution, format!("APU AUTO SHUTDOWN — {w}")));
            }
        }
        self.ff = match self.state {
            ApuState::Starting => 0.02 * self.n,
            ApuState::Running => 0.025 + if self.bleed { 0.012 } else { 0.0 } + if self.generator { 0.006 } else { 0.0 },
            _ => 0.0,
        };
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_run_stop() {
        let mut a = Apu::new();
        assert!(a.start(30_000.0, true, true).is_err());
        a.start(0.0, true, true).unwrap();
        let mut max_egt: f64 = 0.0;
        for _ in 0..(40 * 10) {
            let _ = a.update(0.1, 0.0, 0.0, true, 15.0);
            max_egt = max_egt.max(a.egt);
        }
        assert!(a.running() && a.gen_online() && a.bleed_air());
        assert!(max_egt > 600.0, "pic d'EGT {max_egt:.0}");
        // Montée au-dessus du FL350 : arrêt automatique
        let ev = a.update(0.1, 36_000.0, 0.5, true, -50.0);
        assert!(ev.iter().any(|e| e.1.contains("AUTO SHUTDOWN")));
        for _ in 0..(30 * 10) {
            let _ = a.update(0.1, 36_000.0, 0.5, true, -50.0);
        }
        assert_eq!(a.state, ApuState::Off);
    }
}
