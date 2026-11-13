//! Liaison de données station sol ↔ Darkstar, et flotte :
//! - flotte de plusieurs Darkstar ; l'opérateur établit la liaison avec l'appareil à contrôler ;
//! - liaison mondiale sans portee radio ni latence ; relais (sol, BACN, SATCOM) au choix en reconnexion,
//!   et canal de secours Iridium (urgence seulement, latence élevée) ;
//! - en exploitation normale la liaison est permanente (SATCOM couvre le globe, bascules
//!   automatiques sans coupure) ; elle n'est perdue que sur incident : panne de l'émetteur ou du
//!   terminal SATCOM hors portée radio, perte du bus électrique ;
//! - bascule automatique vers un autre chemin ; sinon perte de liaison (télémétrie figée, procédure
//!   autonome de l'avion) et reconnexion manuelle via le relais choisi (poignée de main de quelques
//!   secondes, réussite incertaine en limite de portée).

use crate::aircraft::{Aircraft, Level};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Flotte : (immatriculation, terrain de stationnement, disponibilité).
pub const FLEET: [(&str, &str, Option<&str>); 4] = [
    ("FTV-01", "KPMD", None),
    ("FTV-02", "KEDW", None),
    ("FTV-03", "KEDW", Some("MAINTENANCE — VISITE 200 H")),
    ("FTV-04", "KNID", Some("EN VOL — CONTROLE PAR GCS-2")),
];

static TAIL: AtomicUsize = AtomicUsize::new(0);

/// Immatriculation de l'appareil contrôlé.
pub fn tail() -> &'static str {
    FLEET[TAIL.load(Ordering::Relaxed).min(FLEET.len() - 1)].0
}

pub fn set_tail(i: usize) {
    TAIL.store(i, Ordering::Relaxed);
}

/// Stations sol relais (terrains).
pub const STATIONS: [&str; 6] = ["KEDW", "KNID", "PHNL", "PGUA", "LFMI", "FJDG"];
/// Avions relais BACN en hippodrome au FL450 : (nom, lat, lon).
pub const BACN: [(&str, f64, f64); 2] = [("BACN-1", 35.6, -123.5), ("BACN-2", 24.5, -142.0)];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Path {
    Los,
    Ground(usize),
    Bacn(usize),
    Satcom,
    /// Canal de secours Iridium : instructions d'urgence seulement.
    Emerg,
}

impl Path {
    pub fn name(&self) -> String {
        match self {
            Path::Los => "DATALINK".into(),
            Path::Ground(i) => format!("SOL {}", STATIONS[*i]),
            Path::Bacn(i) => BACN[*i].0.into(),
            Path::Satcom => "SATCOM".into(),
            Path::Emerg => "IRIDIUM SECOURS".into(),
        }
    }

    pub fn all() -> Vec<Path> {
        let mut v = vec![Path::Los];
        v.extend((1..STATIONS.len()).map(Path::Ground));
        v.extend((0..BACN.len()).map(Path::Bacn));
        v.push(Path::Satcom);
        v.push(Path::Emerg);
        v
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    /// Pas encore de liaison (écran de connexion).
    Idle,
    Connecting { path: Path, t: f64, dur: f64 },
    Connected,
    Lost,
}

pub struct Link {
    pub state: State,
    pub path: Path,
    /// Temps écoulé depuis la perte de liaison (s simulées).
    pub lost_t: f64,
    /// Panne de l'émetteur principal (injection) et du terminal SATCOM.
    pub tx_fail: bool,
    pub sat_fail: bool,
    pub events: Vec<(Level, String)>,
    /// Étapes de la poignée de main affichées pendant la connexion.
    pub step: usize,
    rng: u64,
}

pub const STEPS: [&str; 5] = ["RECHERCHE DE LA PORTEUSE", "SYNCHRONISATION DES TRAMES", "AUTHENTIFICATION CRYPTO", "TELEMESURE VERROUILLEE", "VOIE DE COMMANDE ETABLIE"];



impl Link {
    pub fn new() -> Link {
        Link { state: State::Connected, path: Path::Los, lost_t: 0.0, tx_fail: false, sat_fail: false, events: Vec::new(), step: 0, rng: 0x5DEE_CE66_D1CE_4E5B }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Disponibilité d'un chemin : latence (ms) et marge (0..1), ou la raison de l'indisponibilité.
    pub fn avail(&self, p: Path, ac: &Aircraft) -> Result<(f64, f64), &'static str> {
        if p == Path::Emerg {
            return Ok((0.0, 1.0));
        }
        if !ac.elec.bus_ok {
            return Err("EMETTEUR HORS TENSION");
        }
        if self.tx_fail {
            return Err("EMETTEUR EN PANNE");
        }
        // Pas de portée radio ni de latence : la liaison couvre tout le globe sans délai ;
        // elle n'est perdue que sur panne (émetteur, terminal, bus électrique) ou dégâts
        if p == Path::Satcom && self.sat_fail {
            return Err("TERMINAL SATCOM EN PANNE");
        }
        Ok((0.0, 1.0))
    }

    /// Estimation côté station sol (portée et couverture d'après la dernière position connue) :
    /// la station ne connaît pas l'état de l'émetteur de bord.
    pub fn estimate(&self, p: Path, ac: &Aircraft) -> Result<(f64, f64), &'static str> {
        let probe = Link { tx_fail: false, sat_fail: false, events: Vec::new(), ..*self };
        probe.avail(p, ac)
    }

    /// Meilleur chemin complet disponible (latence la plus faible).
    pub fn best(&self, ac: &Aircraft) -> Option<Path> {
        Path::all()
            .into_iter()
            .filter(|p| *p != Path::Emerg)
            .filter_map(|p| self.avail(p, ac).ok().map(|(l, _)| (p, l)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p)
    }

    /// Commande de vol possible (liaison complète).
    pub fn full(&self) -> bool {
        self.state == State::Connected && self.path != Path::Emerg
    }

    /// Instructions d'urgence acceptées (liaison complète ou canal de secours).
    pub fn emergency_ok(&self) -> bool {
        self.state == State::Connected
    }

    pub fn latency(&self, ac: &Aircraft) -> f64 {
        self.avail(self.path, ac).map(|(l, _)| l).unwrap_or(0.0)
    }

    /// Lance une tentative de connexion par un chemin.
    pub fn connect(&mut self, p: Path) {
        let dur = match p {
            Path::Satcom => 6.0,
            Path::Emerg => 8.0,
            Path::Bacn(_) => 5.0,
            _ => 4.0,
        };
        self.state = State::Connecting { path: p, t: 0.0, dur };
        self.step = 0;
        self.events.push((Level::Info, format!("TENTATIVE DE LIAISON VIA {}", p.name())));
    }

    /// Mise à jour : `dt_real` pour la poignée de main, `dt_sim` pour le chronomètre de perte.
    /// Renvoie vrai à l'instant où la liaison est perdue.
    pub fn update(&mut self, ac: &Aircraft, dt_real: f64, dt_sim: f64) -> bool {
        match self.state {
            State::Idle => false,
            State::Connected => {
                if self.avail(self.path, ac).is_ok() && (self.path != Path::Emerg) {
                    // Retour en portée LOS : bascule vers le chemin le plus direct
                    if self.path != Path::Los && self.avail(Path::Los, ac).is_ok() {
                        self.events.push((Level::Info, "LIAISON : RETOUR EN PORTEE — BASCULE SUR LOS".into()));
                        self.path = Path::Los;
                    }
                    return false;
                }
                if self.path == Path::Emerg {
                    // Canal de secours : passe en liaison complète dès qu'un chemin revient
                    if self.avail(Path::Emerg, ac).is_err() {
                        self.state = State::Lost;
                        self.lost_t = 0.0;
                        self.events.push((Level::Warning, "LIAISON DE SECOURS PERDUE".into()));
                        return true;
                    }
                    return false;
                }
                let why = self.avail(self.path, ac).err().unwrap_or("");
                match self.best(ac) {
                    Some(p) => {
                        self.events.push((Level::Caution, format!("LIAISON {} : {why} — BASCULE SUR {}", self.path.name(), p.name())));
                        self.path = p;
                        false
                    }
                    None => {
                        self.state = State::Lost;
                        self.lost_t = 0.0;
                        self.events.push((Level::Warning, format!("LOST LINK — LIAISON PERDUE ({why})")));
                        true
                    }
                }
            }
            State::Lost => {
                self.lost_t += dt_sim;
                false
            }
            State::Connecting { path, t, dur } => {
                let t = t + dt_real;
                let k = ((t / dur) * STEPS.len() as f64) as usize;
                // Les étapes avancent tant que le chemin répond
                let ok = self.avail(path, ac);
                if let Err(why) = ok {
                    if k >= 1 {
                        self.events.push((Level::Caution, format!("ECHEC VIA {} : {why}", path.name())));
                        self.state = State::Lost;
                        return false;
                    }
                }
                self.step = k.min(STEPS.len() - 1);
                if t >= dur {
                    let margin = ok.map(|(_, m)| m).unwrap_or(0.0);
                    let p_ok = if margin > 0.15 { 1.0 } else { 0.45 + margin * 3.0 };
                    if self.rand() < p_ok {
                        self.state = State::Connected;
                        self.path = path;
                        self.events.push((Level::Info, if path == Path::Emerg {
                            "LIAISON DE SECOURS IRIDIUM ETABLIE — INSTRUCTIONS SEULEMENT, PAS DE MANCHE".to_string()
                        } else {
                            format!("LIAISON ETABLIE AVEC DARKSTAR {} VIA {}", tail(), path.name())
                        }));
                    } else {
                        self.events.push((Level::Caution, format!("ECHEC VIA {} : AUTHENTIFICATION NON ABOUTIE (SIGNAL FAIBLE)", path.name())));
                        self.state = State::Lost;
                    }
                } else {
                    self.state = State::Connecting { path, t, dur };
                }
                false
            }
        }
    }
}

/// Bandeau et choix du relais quand la liaison est perdue ou réduite au secours (zone `r` de
/// l'horizon). La disponibilité est estimée d'après la dernière position connue. Renvoie le chemin
/// choisi pour une tentative.
pub fn draw_lost(link: &Link, last: &Aircraft, r: macroquad::prelude::Rect, time: f64) -> Option<Path> {
    use crate::ui::*;
    use macroquad::prelude::*;
    let emerg = link.state == State::Connected && link.path == Path::Emerg;
    let connecting = matches!(link.state, State::Connecting { .. });
    let blink = (time * 2.0).fract() < 0.65;
    let title = if emerg {
        "LIAISON DE SECOURS IRIDIUM".to_string()
    } else if connecting {
        "LIAISON PERDUE — TENTATIVE EN COURS".to_string()
    } else {
        format!("LOST LINK — LIAISON PERDUE  T+{}", crate::mission::fmt_time(link.lost_t))
    };
    let paths = Path::all();
    let w = 360.0f32.min(r.w - 20.0);
    let h = 70.0 + paths.len() as f32 * 21.0;
    let b = Rect::new(r.x + (r.w - w) / 2.0, r.y + 40.0, w, h);
    rrect(b, 8.0, Color::new(0.0, 0.0, 0.0, 0.86));
    rrect_lines(b, 8.0, 1.5, if emerg { CYAN_S } else { AMBER });
    if blink || emerg {
        txt_c(&title, b.x + b.w / 2.0, b.y + 20.0, 14.0, if emerg { CYAN_S } else { AMBER });
    }
    let sub = if emerg {
        "INSTRUCTIONS POSSIBLES, MANCHE INDISPONIBLE"
    } else {
        "TELEMESURE FIGEE — PROCEDURE AUTONOME : MAINTIEN CAP / ALTITUDE"
    };
    txt_c(sub, b.x + b.w / 2.0, b.y + 38.0, 10.0, Color::new(0.75, 0.8, 0.85, 1.0));
    let mut out = None;
    for (k, p) in paths.iter().enumerate() {
        let row = Rect::new(b.x + 10.0, b.y + 50.0 + k as f32 * 21.0, b.w - 20.0, 19.0);
        let est = link.estimate(*p, last);
        let (state, c) = match (&est, p) {
            (_, Path::Emerg) => ("SANS MANCHE".to_string(), CYAN_S),
            (Ok(_), _) => ("POSSIBLE".to_string(), GREEN_S),
            (Err(why), _) => (format!("{why} (estime)"), DIM),
        };
        let cur = matches!(link.state, State::Connecting { path, .. } if path == *p);
        let hov = hovered(row) && !connecting;
        rrect(row, 4.0, if cur { Color::new(0.05, 0.2, 0.25, 1.0) } else if hov { Color::new(0.1, 0.12, 0.15, 1.0) } else { Color::new(0.05, 0.06, 0.07, 1.0) });
        txt(&p.name(), row.x + 8.0, row.y + 14.0, 12.0, WHITE);
        txt_r(&if cur { STEPS[link.step].to_string() } else { state }, row.x + row.w - 8.0, row.y + 14.0, 10.0, if cur { CYAN_S } else { c });
        if hov && clicked(row) {
            out = Some(*p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_range_loss() {
        let mut ac = Aircraft::new_on_runway(crate::airports::AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap());
        ac.alt += 10_000.0;
        ac.on_ground = false;
        let mut l = Link::new();
        // Partout dans le monde : liaison directe, sans latence ni bascule
        for (lat, lon) in [(30.0, -135.0), (80.0, 0.0), (-70.0, 120.0), (10.0, 60.0)] {
            ac.lat = lat;
            ac.lon = lon;
            assert!(!l.update(&ac, 0.016, 0.016), "perte a {lat},{lon}");
            assert_eq!((l.state, l.path), (State::Connected, Path::Los));
            assert_eq!(l.latency(&ac), 0.0);
        }
        // Panne de l'émetteur : perte de liaison, seul le secours reste
        l.tx_fail = true;
        assert!(l.update(&ac, 0.016, 0.016));
        assert_eq!(l.state, State::Lost);
        assert!(l.avail(Path::Emerg, &ac).is_ok());
    }
}
