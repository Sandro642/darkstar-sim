//! Perte du véhicule vue de la station sol, comme en vrai :
//! 1. cascade de pannes (messages type ECAM / EICAS selon la cause), panneau d'alarmes qui s'allume
//!    en « sapin de Noël », alarme sonore continue, commandes de vol qui ne répondent plus ;
//! 2. télémétrie qui se dégrade : trames corrompues, déchirures d'image, drapeaux rouges sur les
//!    instruments (ATT, SPD, ALT, V/S, HDG, FPV) ;
//! 3. perte du signal (LOS) : neige puis écrans noirs barrés d'une croix rouge, dernière trame
//!    valide (position, niveau, vitesse, cap, vario, facteur de charge) et cause probable.

use crate::aircraft::{Aircraft, Level};
use crate::geo::*;
use crate::ui::*;
use macroquad::prelude::*;

fn now_s() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Structural,
    Explosion,
    Impact,
    Missile,
    Electrical,
    SelfDestruct,
}

/// Dernière trame de télémétrie valide.
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub keas: f64,
    pub mach: f64,
    pub hdg: f64,
    pub vs_fpm: f64,
    pub nz: f64,
    pub sim_t: f64,
}

impl Snapshot {
    pub fn of(ac: &Aircraft, sim_t: f64) -> Snapshot {
        Snapshot { lat: ac.lat, lon: ac.lon, alt_ft: ac.alt / FT, keas: ac.d.keas, mach: ac.d.mach, hdg: ac.hdg_deg(), vs_fpm: ac.d.vs / FT * 60.0, nz: ac.d.nz, sim_t }
    }
}

/// Deuxième chance : une tentative de reconnexion par le canal de secours après la perte du signal.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Second {
    Available,
    Trying(f64),
    Failed,
}

pub struct Mishap {
    pub second: Second,
    start: f64,
    pub cause: String,
    pub kind: Kind,
    pub t_los: f64,
    msgs: Vec<(f64, Level, String)>,
    next: usize,
    /// Instant d'allumage (s) et niveau de chaque voyant du panneau d'alarmes.
    lights: [(f64, u8); 32],
    pub snap: Snapshot,
}

fn kind_of(cause: &str) -> Kind {
    let c = cause.to_uppercase();
    if c.contains("EJECTION") || c.contains("AUTODESTRUCTION") {
        Kind::SelfDestruct
    } else if c.contains("MISSILE") || c.contains("ABATTU") {
        Kind::Missile
    } else if c.contains("EXPLOSION") {
        Kind::Explosion
    } else if c.contains("ALIMENTATION") {
        Kind::Electrical
    } else if c.contains("IMPACT") || c.contains("TRAIN") || c.contains("PISTE") || c.contains("PNEUS") || c.contains("SAILLAGE") {
        Kind::Impact
    } else {
        Kind::Structural
    }
}

/// Graine pseudo-aléatoire stable (xorshift).
struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

impl Mishap {
    pub fn new(cause: &str, snap: Snapshot) -> Mishap {
        let kind = kind_of(cause);
        let t_los = match kind {
            Kind::Impact => 1.6,
            Kind::SelfDestruct => 2.6,
            Kind::Electrical => 3.0,
            Kind::Missile => 4.2,
            Kind::Explosion => 4.6,
            Kind::Structural => 5.2,
        };
        use Level::{Caution as C, Warning as W};
        let script: Vec<(f64, Level, &str)> = match kind {
            Kind::Structural => vec![
                (0.0, W, "OVERSTRESS — LIMITE STRUCTURALE DEPASSEE"),
                (0.3, W, "FCS : POSITION GOUVERNES DISCORDANTE"),
                (0.7, W, "HYD SYS 1 BASSE PRESSION"),
                (0.9, W, "HYD SYS 2 BASSE PRESSION"),
                (1.3, W, "FLT CTL : COMMANDES DE VOL SANS REPONSE"),
                (1.6, W, "FCS CANAUX A/B/C/D EN PANNE"),
                (2.0, W, "ASSIETTE INCONTROLEE — ROULIS NON COMMANDE"),
                (2.5, W, "ENG L EXTINCTION   ENG R EXTINCTION"),
                (3.0, C, "ELEC BUS 1 OFF   ELEC BUS 2 OFF"),
                (3.4, C, "INS : DEFAUT CENTRALE INERTIELLE"),
                (4.0, C, "TELEMETRIE : ERREURS CRC, TRAMES PERDUES"),
            ],
            Kind::Explosion => vec![
                (0.0, W, "ENG FIRE — CHAMBRE DE COMBUSTION"),
                (0.4, W, "PERCEMENT CHAMBRE — FUITE H2"),
                (0.8, W, "FEU SOUTE ARRIERE"),
                (1.2, W, "ENG L EXTINCTION   ENG R EXTINCTION"),
                (1.6, W, "HYD SYS 1 / SYS 2 PERTE"),
                (2.1, W, "FLT CTL : COMMANDES DE VOL SANS REPONSE"),
                (2.6, C, "ELEC : GENERATRICES HORS LIGNE"),
                (3.2, C, "CAPTEURS : DONNEES INVALIDES"),
                (3.8, C, "TELEMETRIE : ERREURS CRC, TRAMES PERDUES"),
            ],
            Kind::Missile => vec![
                (0.0, W, "MISSILE — IMPACT DE PROXIMITE"),
                (0.4, W, "DEGATS STRUCTURE ARRIERE"),
                (0.8, W, "FEU MOTEUR GAUCHE   FEU MOTEUR DROIT"),
                (1.2, W, "FUITE CARBURANT MASSIVE"),
                (1.6, W, "HYD SYS 1 / SYS 2 PERTE"),
                (2.0, W, "FLT CTL : COMMANDES DE VOL SANS REPONSE"),
                (2.6, C, "ELEC BUS ESSENTIEL OFF"),
                (3.3, C, "TELEMETRIE : ERREURS CRC, TRAMES PERDUES"),
            ],
            Kind::Electrical => vec![
                (0.0, W, "GEN L OFF   GEN R OFF"),
                (0.4, W, "PILE A COMBUSTIBLE : DEFAUT"),
                (0.8, W, "BATTERIE : DECHARGE PROFONDE"),
                (1.3, W, "BUS ESSENTIEL OFF — CALCULATEURS EN REPLI"),
                (1.8, W, "FLT CTL : COMMANDES DE VOL SANS REPONSE"),
                (2.3, C, "TELEMETRIE : EMETTEUR HORS TENSION"),
            ],
            Kind::Impact => vec![(0.0, W, "IMPACT DETECTE — DECELERATION > 40 G"), (0.3, W, "TRAIN / STRUCTURE DETRUITS"), (0.7, W, "FEU — RESERVOIRS"), (1.1, C, "TELEMETRIE : TRAMES PERDUES")],
            Kind::SelfDestruct => vec![
                (0.0, W, "FTS ARME — SYSTEME DE TERMINAISON DE VOL"),
                (0.6, W, "FTS DECLENCHE — CHARGES DE DESTRUCTION"),
                (1.2, W, "CELLULE DETRUITE"),
                (1.8, C, "TELEMETRIE : TRAMES PERDUES"),
            ],
        };
        let mut msgs: Vec<(f64, Level, String)> = script.into_iter().map(|(t, l, s)| (t, l, s.to_string())).collect();
        msgs.push((t_los, Level::Warning, "LOS — PERTE DU SIGNAL TELEMETRIE".into()));
        // Panneau d'alarmes : voyants propres à la cause d'abord, puis la cascade (sapin de Noël)
        let primary: &[usize] = match kind {
            Kind::Structural => &[21, 23, 26, 22, 18, 4, 5, 29],
            Kind::Explosion => &[11, 10, 8, 6, 7, 15, 4, 5, 19],
            Kind::Missile => &[4, 5, 15, 12, 13, 26, 23, 20],
            Kind::Electrical => &[0, 1, 30, 2, 3, 27],
            Kind::Impact => &[28, 26, 23, 4, 5],
            Kind::SelfDestruct => &[16, 17, 27, 30],
        };
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (snap.lat.to_bits() >> 3) ^ snap.lon.to_bits());
        let mut lights = [(f64::INFINITY, 0u8); 32];
        for (k, &i) in primary.iter().enumerate() {
            lights[i] = (0.05 + k as f64 * 0.12, 2);
        }
        let span = (t_los * 0.95).max(0.8);
        for (i, l) in lights.iter_mut().enumerate() {
            if l.0.is_infinite() && rng.f() < 0.9 {
                let lvl = if rng.f() < 0.65 { 2 } else { 1 };
                *l = (0.4 + rng.f() * span, lvl);
            }
            let _ = i;
        }
        Mishap { second: Second::Available, start: now_s(), cause: cause.to_string(), kind, t_los, msgs, next: 0, lights, snap }
    }

    /// Probabilité que le véhicule réponde encore (selon la cause) : un avion qui a percuté le sol ou
    /// s'est autodétruit ne répondra jamais ; une perte électrique ou une explosion moteur laisse
    /// souvent la cellule intacte.
    pub fn chance(&self) -> f64 {
        let c = self.cause.to_uppercase();
        match self.kind {
            Kind::Impact | Kind::SelfDestruct => 0.0,
            Kind::Electrical => 0.7,
            Kind::Explosion => 0.5,
            Kind::Missile => 0.25,
            Kind::Structural if c.contains("GOUVERNES") => 0.35,
            Kind::Structural if c.contains("FACTEUR") => 0.2,
            Kind::Structural => 0.1,
        }
    }

    /// Lance la tentative de deuxième chance.
    pub fn try_second(&mut self) {
        if self.second == Second::Available {
            self.second = Second::Trying(now_s());
        }
    }

    /// Résultat de la tentative (Some(vrai) : le véhicule répond) une fois la poignée de main finie.
    pub fn second_result(&mut self) -> Option<bool> {
        if let Second::Trying(t0) = self.second {
            if now_s() - t0 >= 8.0 {
                let r = (now_s() * 1000.0).fract();
                let ok = r < self.chance();
                if !ok {
                    self.second = Second::Failed;
                }
                return Some(ok);
            }
        }
        None
    }

    pub fn elapsed(&self) -> f64 {
        now_s() - self.start
    }

    pub fn lost(&self) -> bool {
        self.elapsed() >= self.t_los
    }

    /// Phase de cascade (alarme sonore continue).
    pub fn cascading(&self) -> bool {
        !self.lost()
    }

    /// Messages dont l'heure est venue.
    pub fn due(&mut self) -> Vec<(Level, String)> {
        let e = self.elapsed();
        let mut out = Vec::new();
        while self.next < self.msgs.len() && self.msgs[self.next].0 <= e {
            let (_, l, s) = &self.msgs[self.next];
            out.push((*l, s.clone()));
            self.next += 1;
        }
        out
    }

    /// États forcés du panneau d'alarmes (0 éteint, 1 ambre, 2 rouge), avec scintillement des
    /// trames corrompues avant la perte du signal.
    pub fn lights(&self) -> [u8; 32] {
        let e = self.elapsed();
        let mut out = [0u8; 32];
        for (i, &(t, l)) in self.lights.iter().enumerate() {
            if e >= t {
                let glitch = !self.lost() && ((e * 23.0 + i as f64 * 1.7).sin() > 0.93);
                out[i] = if glitch { 0 } else { l };
            }
        }
        out
    }

    /// Avancement (0..1) de la dégradation avant la perte du signal.
    pub fn decay(&self) -> f32 {
        (self.elapsed() / self.t_los).clamp(0.0, 1.0) as f32
    }

    /// Écran de perte du signal par-dessus l'horizon (`r` : zone centrale du PFD). Renvoie vrai si
    /// la deuxième chance est demandée.
    pub fn draw_los(&self, r: Rect) -> bool {
        let e = self.elapsed() - self.t_los;
        if e < 0.0 {
            return false;
        }
        let blink = (e * 1.6).fract() < 0.7;
        let s = &self.snap;
        let lines: Vec<(String, Color, f32)> = vec![
            ("LOS — PERTE DU SIGNAL TELEMETRIE".into(), RED, 22.0),
            (
                match self.kind {
                    Kind::Structural => "RUPTURE EN VOL — VEHICULE PERDU",
                    Kind::Explosion => "FEU / EXPLOSION EN VOL — VEHICULE PERDU",
                    Kind::Impact => "IMPACT — VEHICULE DETRUIT",
                    Kind::Missile => "ABATTU — VEHICULE PERDU",
                    Kind::Electrical => "PERTE ELECTRIQUE TOTALE — VEHICULE PERDU",
                    Kind::SelfDestruct => "TERMINAISON DE VOL (FTS) — VEHICULE DETRUIT",
                }
                .into(),
                Color::new(1.0, 0.35, 0.3, 1.0),
                15.0,
            ),
            (format!("AUCUNE TRAME DEPUIS T+{}", crate::mission::fmt_time(e)), AMBER, 14.0),
            (String::new(), WHITE, 8.0),
            ("DERNIERE TRAME VALIDE".into(), Color::new(0.75, 0.8, 0.85, 1.0), 12.0),
            (format!("POS  {}  {}", dms(s.lat, 'N', 'S'), dms(s.lon, 'E', 'W')), WHITE, 14.0),
            (format!("FL{:03.0}   KEAS {:.0}   M {:.2}   CAP {:03.0}", s.alt_ft / 100.0, s.keas, s.mach, s.hdg), WHITE, 14.0),
            (format!("V/S {:+.0} FT/MIN   NZ {:+.1} G   T SIM {}", s.vs_fpm, s.nz, crate::mission::fmt_time(s.sim_t)), WHITE, 14.0),
            (String::new(), WHITE, 8.0),
            (format!("CAUSE PROBABLE : {}", self.cause), Color::new(1.0, 0.55, 0.45, 1.0), 13.0),
            ("ENTREE : NOUVEL ESSAI".into(), Color::new(0.6, 0.65, 0.7, 1.0), 12.0),
        ];
        let w = lines.iter().map(|(l, _, sz)| txt_w(l, *sz)).fold(0.0f32, f32::max) + 36.0;
        let h: f32 = lines.iter().map(|(_, _, sz)| sz + 6.0).sum::<f32>() + 22.0;
        let b = Rect::new(r.x + (r.w - w) / 2.0, r.y + r.h * 0.12, w, h);
        rrect(b, 8.0, Color::new(0.0, 0.0, 0.0, 0.88));
        rrect_lines(b, 8.0, 1.5, if blink { RED } else { Color::new(0.5, 0.08, 0.06, 1.0) });
        let mut y = b.y + 14.0;
        for (l, c, sz) in &lines {
            y += sz + 6.0;
            if l.starts_with("LOS") && !blink {
                continue;
            }
            txt_c(l, b.x + b.w / 2.0, y - 4.0, *sz, *c);
        }
        // Deuxième chance : une tentative de reconnexion par le secours Iridium
        let bt = Rect::new(b.x + b.w / 2.0 - 170.0, b.y + b.h + 8.0, 340.0, 26.0);
        match self.second {
            Second::Available => {
                let hov = hovered(bt);
                rrect(bt, 6.0, if hov { Color::new(0.08, 0.3, 0.36, 1.0) } else { Color::new(0.04, 0.18, 0.22, 0.95) });
                rrect_lines(bt, 6.0, 1.5, CYAN_S);
                txt_c("2E CHANCE : TENTER UNE RECONNEXION (SECOURS)", bt.x + bt.w / 2.0, bt.y + 17.0, 12.0, WHITE);
                return clicked(bt);
            }
            Second::Trying(t0) => {
                let k = ((now_s() - t0) / 8.0).clamp(0.0, 1.0) as f32;
                rrect(bt, 6.0, Color::new(0.0, 0.0, 0.0, 0.85));
                draw_rectangle(bt.x + 2.0, bt.y + bt.h - 5.0, (bt.w - 4.0) * k, 3.0, CYAN_S);
                let step = crate::link::STEPS[((k * 5.0) as usize).min(4)];
                txt_c(&format!("IRIDIUM : {step}..."), bt.x + bt.w / 2.0, bt.y + 16.0, 12.0, CYAN_S);
            }
            Second::Failed => {
                rrect(bt, 6.0, Color::new(0.0, 0.0, 0.0, 0.85));
                txt_c("AUCUNE REPONSE DU VEHICULE — PERTE CONFIRMEE", bt.x + bt.w / 2.0, bt.y + 17.0, 12.0, RED);
            }
        }
        false
    }
}

fn dms(v: f64, pos: char, neg: char) -> String {
    let a = v.abs();
    let d = a.floor();
    let m = (a - d) * 60.0;
    format!("{}{:.0} {:05.2}'", if v >= 0.0 { pos } else { neg }, d, m)
}

/// Croix rouge d'écran sans source de données (comme sur un afficheur réel).
pub fn red_x(r: Rect, fill: bool) {
    if fill {
        draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
    }
    let c = Color::new(0.85, 0.08, 0.06, 1.0);
    draw_line(r.x + 6.0, r.y + 6.0, r.x + r.w - 6.0, r.y + r.h - 6.0, 4.0, c);
    draw_line(r.x + r.w - 6.0, r.y + 6.0, r.x + 6.0, r.y + r.h - 6.0, 4.0, c);
    draw_rectangle_lines(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0, 2.0, c);
}

/// Neige d'un récepteur sans signal (s'estompe vers le noir).
pub fn static_noise(r: Rect, strength: f32, time: f64) {
    if strength <= 0.01 {
        return;
    }
    let mut x = ((time * 1000.0) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 40) as f32 / (1u64 << 24) as f32
    };
    let cell = 6.0;
    let (nx, ny) = ((r.w / cell) as usize, (r.h / cell) as usize);
    for j in 0..ny {
        for i in 0..nx {
            let v = rnd();
            if v < 0.5 {
                continue;
            }
            let g = v * strength;
            draw_rectangle(r.x + i as f32 * cell, r.y + j as f32 * cell, cell, cell, Color::new(g, g, g, 0.9));
        }
    }
}

/// Télémétrie corrompue : déchirures horizontales, bandes de pertes de trames, décalages.
pub fn glitch(r: Rect, k: f32, time: f64) {
    if k <= 0.01 {
        return;
    }
    let frame = (time * 24.0) as u64;
    let mut x = frame.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 40) as f32 / (1u64 << 24) as f32
    };
    let n = (2.0 + k * 14.0) as usize;
    for _ in 0..n {
        let y = r.y + rnd() * r.h;
        let h = 2.0 + rnd() * 14.0 * k;
        let v = rnd();
        let c = if v < 0.4 {
            Color::new(0.0, 0.0, 0.0, 0.85)
        } else if v < 0.7 {
            Color::new(0.85, 0.9, 1.0, 0.25 + 0.3 * k)
        } else if v < 0.85 {
            Color::new(0.1, 1.0, 0.4, 0.25)
        } else {
            Color::new(1.0, 0.1, 0.6, 0.25)
        };
        let x0 = r.x + (rnd() - 0.3) * r.w * 0.4;
        draw_rectangle(x0.max(r.x), y, (r.w * (0.4 + rnd() * 0.8)).min(r.x + r.w - x0.max(r.x)), h.min(r.y + r.h - y), c);
    }
    // Image figée par moments (trame perdue)
    if rnd() < 0.15 * k {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.35));
    }
}

/// Drapeau de panne d'un instrument (fond noir, cadre et texte rouges).
pub fn flag(label: &str, cx: f32, cy: f32) {
    let w = txt_w(label, 16.0) + 14.0;
    let b = Rect::new(cx - w / 2.0, cy - 12.0, w, 24.0);
    draw_rectangle(b.x, b.y, b.w, b.h, BLACK);
    draw_rectangle_lines(b.x, b.y, b.w, b.h, 2.0, RED);
    txt_c(label, cx, cy + 6.0, 16.0, RED);
}
