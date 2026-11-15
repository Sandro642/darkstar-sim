//! Centrale inertielle (INS) : plate-forme gyroscopique qui fournit l'attitude (horizon), le cap et
//! la position. Sélecteur OFF / ATT / ALIGN / NAV :
//! - ALIGN (au sol, avion immobile) : 1. STABILISATION (mise à niveau de la plate-forme, ~25 s) ->
//!   attitude valide ; 2. GYROCOMPAS (recherche du nord, jusqu'à ~2 min) -> cap valide ; NAV RDY.
//!   Un mouvement de l'avion pendant l'alignement le fait recommencer (ALIGN FAULT).
//! - NAV : centrale alignée, attitude + cap + position (passage en NAV avant NAV RDY refusé).
//! - ATT (secours, en vol possible) : stabilisation rapide, attitude seule, cap non fiable.
//! - Sans alimentation électrique la plate-forme s'arrête : il faut tout réaligner.

use crate::aircraft::Level;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum InsMode {
    #[default]
    Off,
    Att,
    Align,
    Nav,
}

/// Durée de la stabilisation (s) et de l'alignement complet (s).
pub const STAB_S: f64 = 25.0;
pub const ALIGN_S: f64 = 120.0;
const ATT_S: f64 = 15.0;

#[derive(Clone, Debug)]
pub struct Ins {
    pub mode: InsMode,
    /// Temps d'alignement écoulé (s)
    pub t: f64,
    /// Alignement complet (gyrocompas terminé)
    pub aligned: bool,
    /// Plate-forme stabilisée (attitude)
    pub stab: bool,
    pub fault: bool,
}

impl Default for Ins {
    fn default() -> Self {
        Self::ready()
    }
}

impl Ins {
    /// Centrale déjà alignée en NAV (départs « prêt au décollage », scénarios).
    pub fn ready() -> Ins {
        Ins { mode: InsMode::Nav, t: ALIGN_S, aligned: true, stab: true, fault: false }
    }

    pub fn off() -> Ins {
        Ins { mode: InsMode::Off, t: 0.0, aligned: false, stab: false, fault: false }
    }

    /// Attitude valide (horizon).
    pub fn att_ok(&self) -> bool {
        self.mode != InsMode::Off && self.stab
    }

    /// Cap vrai valide.
    pub fn hdg_ok(&self) -> bool {
        matches!(self.mode, InsMode::Nav | InsMode::Align) && self.aligned
    }

    /// Avancement de l'alignement (0..1).
    pub fn progress(&self) -> f64 {
        (self.t / ALIGN_S).clamp(0.0, 1.0)
    }

    /// Phase lisible.
    pub fn phase(&self) -> &'static str {
        match self.mode {
            InsMode::Off => "OFF",
            InsMode::Att if self.stab => "ATT SEULE",
            InsMode::Att => "STABILISATION",
            InsMode::Align if self.fault => "ALIGN FAULT",
            InsMode::Align if !self.stab => "1 STABILISATION",
            InsMode::Align if !self.aligned => "2 GYROCOMPAS",
            InsMode::Align => "NAV RDY",
            InsMode::Nav => "NAV",
        }
    }

    /// Changement de mode : message de réponse.
    pub fn set_mode(&mut self, m: InsMode, powered: bool) -> Result<String, String> {
        if m != InsMode::Off && !powered {
            return Err("INS : PAS D'ALIMENTATION (BATTERIE)".into());
        }
        match m {
            InsMode::Off => {
                *self = Ins::off();
                Ok("INS OFF — PLATE-FORME ARRETEE".into())
            }
            InsMode::Nav if !self.aligned => Err(format!("INS : ALIGNEMENT INCOMPLET ({:.0} %) — ATTENDRE NAV RDY", self.progress() * 100.0)),
            InsMode::Nav => {
                self.mode = InsMode::Nav;
                Ok("INS NAV — ATTITUDE, CAP ET POSITION".into())
            }
            InsMode::Align => {
                if self.mode == InsMode::Off || self.mode == InsMode::Att {
                    self.t = 0.0;
                    self.stab = false;
                    self.aligned = false;
                }
                self.mode = InsMode::Align;
                self.fault = false;
                Ok("INS ALIGN — NE PAS BOUGER L'AVION (STABILISATION PUIS GYROCOMPAS)".into())
            }
            InsMode::Att => {
                if self.mode == InsMode::Off {
                    self.t = 0.0;
                    self.stab = false;
                }
                self.mode = InsMode::Att;
                self.aligned = false;
                Ok("INS ATT — ATTITUDE SEULE (SECOURS), CAP NON FIABLE".into())
            }
        }
    }

    /// Une étape : `moving` = l'avion roule ou vole.
    pub fn update(&mut self, dt: f64, powered: bool, moving: bool) -> Vec<(Level, String)> {
        let mut ev = Vec::new();
        if !powered {
            if self.mode != InsMode::Off {
                *self = Ins::off();
                ev.push((Level::Warning, "INS : PERTE D'ALIMENTATION — PLATE-FORME ARRETEE, REALIGNER".into()));
            }
            return ev;
        }
        match self.mode {
            InsMode::Off | InsMode::Nav => {}
            InsMode::Att => {
                self.t += dt;
                if !self.stab && self.t >= ATT_S {
                    self.stab = true;
                    ev.push((Level::Info, "INS ATT : PLATE-FORME STABILISEE — HORIZON DISPONIBLE".into()));
                }
            }
            InsMode::Align => {
                if moving && !self.aligned {
                    if !self.fault {
                        self.fault = true;
                        ev.push((Level::Caution, "INS ALIGN FAULT — AVION EN MOUVEMENT, ALIGNEMENT REPRIS A ZERO".into()));
                    }
                    self.t = 0.0;
                    self.stab = false;
                    return ev;
                }
                self.fault = false;
                self.t += dt;
                if !self.stab && self.t >= STAB_S {
                    self.stab = true;
                    ev.push((Level::Info, "INS : STABILISATION TERMINEE — HORIZON DISPONIBLE, GYROCOMPAS EN COURS".into()));
                }
                if !self.aligned && self.t >= ALIGN_S {
                    self.aligned = true;
                    ev.push((Level::Info, "INS NAV RDY — ALIGNEMENT TERMINE, PASSER LE SELECTEUR SUR NAV".into()));
                }
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_sequence() {
        let mut i = Ins::off();
        assert!(i.set_mode(InsMode::Align, false).is_err());
        i.set_mode(InsMode::Align, true).unwrap();
        assert!(i.set_mode(InsMode::Nav, true).is_err());
        for _ in 0..300 {
            i.update(0.1, true, false);
        }
        assert!(i.att_ok() && !i.hdg_ok());
        // Mouvement : tout recommence
        i.update(0.1, true, true);
        assert!(!i.att_ok() && i.fault);
        for _ in 0..1250 {
            i.update(0.1, true, false);
        }
        assert!(i.aligned);
        i.set_mode(InsMode::Nav, true).unwrap();
        assert!(i.att_ok() && i.hdg_ok());
        // Perte d'alimentation
        i.update(0.1, false, true);
        assert!(!i.att_ok());
    }
}
