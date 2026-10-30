//! Carte d'essais en vol (flight test card) : points d'essai successifs.

use crate::aircraft::*;
use crate::geo::FT;

pub struct Check {
    pub label: String,
    pub ok: bool,
}

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub hold: f64,
}

pub const STEPS: &[Step] = &[
    Step { title: "TP-01  PRÉ-VOL / DÉMARRAGE", text: "Entrées d'air turbine ouvertes. Démarrer les turbines (T). Attendre N2 62% (IDLE) et EGT stabilisée.", hold: 0.0 },
    Step { title: "TP-02  DÉCOLLAGE KPMD", text: "Relâcher les freins (B). Plein gaz PC (R). Rotation vers 170 KEAS, assiette 10°, train rentré avant 270 KEAS. Instruction DECOLLAGE 15000 pour le décollage automatique.", hold: 0.0 },
    Step { title: "TP-03  MONTÉE SUBSONIQUE", text: "Monter au FL350 et accélérer à M0.90. Stabiliser 10 s.", hold: 10.0 },
    Step { title: "TP-04  PASSAGE SUPERSONIQUE", text: "PC plein. Accélérer à M2.0 au-dessus du FL450 (montée en 'zoom' déconseillée : garder q < 100 kPa).", hold: 3.0 },
    Step { title: "TP-05  ALLUMAGE STATORÉACTEUR", text: "Armer le stato (Y) à M>=2.0. Allumer (Y) à M>=2.5, au-dessus du FL400, q entre 15 et 95 kPa. Mode DUAL.", hold: 0.0 },
    Step { title: "TP-06  TRANSFERT DE POUSSÉE", text: "Couper les turbines (T) puis fermer les entrées d'air turbine (I) AVANT M3.2 (sinon coupure FADEC par surchauffe, ou destruction sous override).", hold: 0.0 },
    Step { title: "TP-07  ACCÉLÉRATION STATO", text: "Accélérer à M4.5 au-dessus de 80 000 ft. Surveiller q et la température de peau.", hold: 5.0 },
    Step { title: "TP-08  TRANSITION SCRAMJET", text: "Armer le scram (U) à M>=4.0. Allumer (U) à M>=4.8, q 25-90 kPa, AoA 0-7°. Le stato s'éteint : mode SCRAM.", hold: 0.0 },
    Step { title: "TP-09  CROISIÈRE HYPERSONIQUE", text: "Tenir M6.0+ entre 90 000 et 110 000 ft pendant 60 s. AoA < 7° sinon UNSTART. Pilote auto conseillé (P, O).", hold: 60.0 },
    Step { title: "TP-10  DARKSTAR — MACH 10", text: "Pousser jusqu'à M10.0. Peau < 1250 °C : monter si la température grimpe (q plus faible).", hold: 5.0 },
    Step { title: "TP-11  DÉCÉLÉRATION / RALLUMAGE", text: "Couper le scram (U). Plané de décélération. Sous M2.5 ouvrir les entrées (I) puis rallumer les turbines (T). Descendre sous le FL450 et M1.5.", hold: 0.0 },
    Step { title: "TP-12  RETOUR ET ATTERRISSAGE", text: "Instruction ATTERRIR KEDW : autoland complet (IAF 15 NM, LOC, G/S, arrondi, freinage). Ou à la main : ILS (L), train sous 270 KEAS, toucher < 600 ft/min.", hold: 0.0 },
];

pub struct Mission {
    pub idx: usize,
    pub timer: f64,
    pub done: bool,
    pub elapsed: f64,
    pub completed_at: Vec<f64>,
}

impl Mission {
    pub fn new() -> Mission {
        Mission { idx: 0, timer: 0.0, done: false, elapsed: 0.0, completed_at: Vec::new() }
    }

    pub fn checks(&self, ac: &Aircraft) -> Vec<Check> {
        let d = &ac.d;
        let alt_ft = ac.alt / FT;
        let c = |l: String, ok: bool| Check { label: l, ok };
        let e = &ac.eng;
        match self.idx {
            0 => vec![
                c("ENTRÉES TURBINE OUVERTES".into(), e.inlet_open),
                c(format!("N2 {:.0}% >= 62%", e.n2 * 100.0), e.turb == TurbState::Running),
            ],
            1 => vec![
                c("EN VOL".into(), !ac.on_ground),
                c(format!("HAUTEUR {:.0} ft > 1500", d.agl / FT), d.agl / FT > 1500.0),
                c("TRAIN RENTRÉ".into(), ac.gear_pos < 0.01),
            ],
            2 => vec![
                c(format!("ALT {alt_ft:.0} >= 34 500 ft"), alt_ft >= 34_500.0),
                c(format!("MACH {:.2} >= 0.88", d.mach), d.mach >= 0.88),
            ],
            3 => vec![
                c(format!("MACH {:.2} >= 2.00", d.mach), d.mach >= 2.0),
                c(format!("ALT {alt_ft:.0} >= 45 000 ft"), alt_ft >= 45_000.0),
            ],
            4 => vec![c("STATO ALLUMÉ".into(), e.ram == RamState::Lit)],
            5 => vec![
                c("TURBINES COUPÉES".into(), e.turb != TurbState::Running && e.turb != TurbState::Starting),
                c("ENTRÉES TURBINE FERMÉES".into(), !e.inlet_open),
                c("STATO ALLUMÉ".into(), e.ram == RamState::Lit),
            ],
            6 => vec![
                c(format!("MACH {:.2} >= 4.50", d.mach), d.mach >= 4.5),
                c(format!("ALT {alt_ft:.0} >= 80 000 ft"), alt_ft >= 80_000.0),
            ],
            7 => vec![c("SCRAMJET ALLUMÉ".into(), e.scram == ScramState::Lit)],
            8 => vec![
                c(format!("MACH {:.2} >= 6.00", d.mach), d.mach >= 6.0),
                c(format!("ALT {alt_ft:.0} dans [90k,110k]"), (90_000.0..=110_000.0).contains(&alt_ft)),
                c("SCRAM ALLUMÉ".into(), e.scram == ScramState::Lit),
            ],
            9 => vec![
                c(format!("MACH {:.2} >= 10.00", d.mach), d.mach >= 10.0),
                c(format!("PEAU {:.0}°C < {SKIN_LIMIT_C:.0}", ac.skin_temp - 273.15), ac.skin_temp - 273.15 < SKIN_LIMIT_C),
            ],
            10 => vec![
                c("TURBINES EN MARCHE".into(), e.turb == TurbState::Running),
                c(format!("MACH {:.2} < 1.50", d.mach), d.mach < 1.5),
                c(format!("ALT {alt_ft:.0} < 45 000 ft"), alt_ft < 45_000.0),
            ],
            11 => vec![
                c("AU SOL, SUR PISTE".into(), ac.on_ground && ac.landed),
                c(format!("VITESSE {:.0} kt < 20", ac.tas / crate::geo::KT), ac.on_ground && ac.tas < 20.0 * crate::geo::KT),
            ],
            _ => vec![],
        }
    }

    /// Avance la carte ; renvoie un message quand un point d'essai est validé.
    pub fn update(&mut self, dt: f64, ac: &Aircraft) -> Option<String> {
        if self.done || ac.crashed.is_some() {
            return None;
        }
        self.elapsed += dt;
        let ok = self.checks(ac).iter().all(|c| c.ok);
        if ok {
            self.timer += dt;
        } else {
            self.timer = 0.0;
        }
        if ok && self.timer >= STEPS[self.idx].hold {
            let title = STEPS[self.idx].title;
            self.completed_at.push(self.elapsed);
            self.idx += 1;
            self.timer = 0.0;
            if self.idx >= STEPS.len() {
                self.done = true;
                return Some(format!("CAMPAGNE D'ESSAIS TERMINÉE EN {}", fmt_time(self.elapsed)));
            }
            return Some(format!("POINT D'ESSAI VALIDÉ : {title}"));
        }
        None
    }
}

pub fn fmt_time(s: f64) -> String {
    let s = s as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}
