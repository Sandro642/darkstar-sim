//! Trafic aérien autour du Darkstar : avions de ligne (types et indicatifs réels de compagnies) et
//! trafic militaire (chasseurs en patrouille, ravitailleur et AWACS en hippodrome, transports,
//! reconnaissance haute altitude). Position relative, distance, relèvement, rapprochement, aspect,
//! identification (AMI / NEUTRE / INCONNU) ; verrouillage d'une cible (fiche détaillée), affichage
//! sur le ND et sur l'horizon (mode A/A : losanges, cadre de désignation, repère de direction).

use crate::aircraft::*;
use crate::geo::*;
use crate::ui::*;
use macroquad::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Iff {
    Ami,
    Neutre,
    /// Interrogé sans réponse (ou piste AWACS non identifiée)
    Inconnu,
    /// Pas encore interrogé : identité inconnue
    #[default]
    NonId,
}

impl Iff {
    pub fn label(self) -> &'static str {
        match self {
            Iff::Ami => "AMI",
            Iff::Neutre => "NEUTRE",
            Iff::Inconnu => "INCONNU",
            Iff::NonId => "NON IDENT",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Iff::Ami => GREEN_S,
            Iff::Neutre => WHITE,
            Iff::Inconnu => AMBER,
            Iff::NonId => Color::new(0.8, 0.78, 0.6, 1.0),
        }
    }
}

/// Comportement d'un trafic.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Behave {
    /// Route rectiligne (changements de cap rares)
    #[default]
    Route,
    /// Hippodrome / orbite autour d'un point
    Orbit { lat: f64, lon: f64, radius: f64 },
    /// Patrouille : virages fréquents
    Patrol,
}

#[derive(Clone, Debug, Default)]
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
    /// Hostile (brouilleur, paillettes, manœuvre d'esquive sous STT)
    pub hostile: bool,
    /// Équipements : ADS-B out (civil), terminal Link 16 (militaire ami)
    pub adsb_out: bool,
    pub l16: bool,
    /// Identification connue de l'avion : IFF affiché, indicatif, type, code transpondeur
    pub ident: Iff,
    pub ident_call: bool,
    pub type_known: bool,
    pub squawk_known: bool,
    /// Interrogation IFF en cours (s restantes, <= 0 : aucune)
    pub iff_t: f64,
    /// Piste estimée (position, cap, vitesse, vario) ; vitesse de piste établie
    pub tr_lat: f64,
    pub tr_lon: f64,
    pub tr_alt: f64,
    pub tr_hdg: f64,
    pub tr_tas: f64,
    pub tr_vs: f64,
    pub vel_ok: bool,
    /// Détections successives (TWS / STT), vitesse de rapprochement mesurée (Doppler, kt)
    pub hits: u32,
    pub vc_meas: f64,
    /// SER vue sous l'aspect actuel (m²)
    pub rcs_eff: f64,
    /// Masquage (horizon radar / relief), recalculé toutes les 0.5 s
    pub masked: Option<&'static str>,
    mask_t: f64,
    /// Brouillage actif ; strobe (angle seul, avant burn-through) et son relèvement
    pub jam: bool,
    pub strobe: bool,
    pub strobe_brg: f64,
    /// Poursuite sur brouilleur (STT angle seul), site mesuré du brouilleur (deg)
    pub hoj: bool,
    pub strobe_el: f64,
    /// Télémétrie passive en HOJ (relèvements seuls + manœuvre de l'avion) : mesures
    /// (t, est m, nord m, relèvement rad) dans un repère local, estimation (distance m, écart-type m)
    tma_pts: Vec<(f64, f64, f64, f64)>,
    tma_ref: (f64, f64),
    tma_dt: f64,
    pub tma: Option<(f64, f64)>,
    /// Détections cumulées de la piste (confirmation 2 sur N), fausse alarme (bruit thermique)
    pub det_n: u32,
    false_alarm: bool,
    evade_t: f64,
    chaff_t: f64,
    adsb_t: f64,
    dl_t: f64,
}

/// Modes du radar : RWS (recherche large, détections brutes) et TWS (volume réduit, pistes avec
/// vitesse, cible non alertée).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RadarMode {
    Rws,
    Tws,
}

impl RadarMode {
    /// (demi-volume en azimut (deg), durée du cadre de balayage (s))
    pub fn volume(self) -> (f64, f64) {
        match self {
            RadarMode::Rws => (AZ_LIM, SCAN_S),
            RadarMode::Tws => (30.0, 1.0),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            RadarMode::Rws => "RWS",
            RadarMode::Tws => "TWS",
        }
    }
}

/// Pistes TWS maximales, durée d'une interrogation IFF (s), portée IFF (NM)
pub const TWS_MAX: usize = 10;
pub const IFF_S: f64 = 1.5;
pub const IFF_NM: f64 = 100.0;
/// Rayon terrestre équivalent (réfraction standard 4/3)
const RE_EFF: f64 = 6_371_000.0 * 4.0 / 3.0;
const JAM_WHY: &str = "BROUILLAGE (AVANT BURN-THROUGH)";

/// Facteur de SER selon l'aspect (0 = nez vers le radar) : flanc et arrière plus réfléchissants ;
/// furtifs : faible de face, pics étroits de flanc et arrière (tuyères).
pub fn aspect_factor(icao: &str, aspect_deg: f64) -> f64 {
    let a = aspect_deg.to_radians();
    let (sn, c) = (a.sin().abs(), a.cos());
    let tail = (-c).max(0.0);
    match icao {
        "F22" | "F35" => 1.0 + 30.0 * sn.powi(8) + 10.0 * tail.powi(4),
        _ => 1.0 + 4.0 * sn * sn + tail * tail,
    }
}

/// Portée de burn-through face à un brouilleur d'autoprotection (signal = bruit) : J/S ∝ R²/SER.
pub fn burn_through(rcs: f64) -> f64 {
    25.0 * NM * (rcs / 5.0).sqrt()
}

/// Masquage de la ligne de visée : horizon radar (terre 4/3) puis relief (tuiles d'altitude).
pub fn los_mask(ac: &Aircraft, t: &Target) -> Option<&'static str> {
    let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
    let (h1, h2) = (ac.alt.max(1.0), t.alt.max(1.0));
    if d > (2.0 * RE_EFF * h1).sqrt() + (2.0 * RE_EFF * h2).sqrt() {
        return Some("SOUS L'HORIZON RADAR");
    }
    // Relief : rien à craindre au-dessus de 9000 m de part et d'autre
    if h1.min(h2) > 9_000.0 {
        return None;
    }
    for k in 1..24 {
        let f = k as f64 / 24.0;
        let x = f * d;
        let hl = h1 + (h2 - h1) * f - x * (d - x) / (2.0 * RE_EFF);
        let (la, lo) = (ac.lat + (t.lat - ac.lat) * f, ac.lon + (t.lon - ac.lon) * f);
        if crate::tiles::elevation(la, lo).is_some_and(|z| z > hl) {
            return Some("MASQUE PAR LE RELIEF");
        }
    }
    None
}

/// Indicatif affiché : réel si identifié (ADS-B, Mode S, Link 16), sinon numéro de piste.
pub fn disp_call(t: &Target) -> String {
    if t.ident_call { t.call.clone() } else { format!("TRK {:02}", t.id % 100) }
}

/// Situation radar d'une cible.
pub struct RState {
    pub in_vol: bool,
    pub notch: bool,
    pub vr: f64,
    pub rcs_eff: f64,
    pub why: Option<&'static str>,
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

/// Situation radar d'une cible (volume, SER sous l'aspect, creux Doppler, masquage, brouillage).
pub fn radar_state(ac: &Aircraft, t: &Target, stt: bool, az_lim: f64) -> RState {
    let r = Traffic::rel(ac, t);
    let lim = if stt { GIMBAL } else { az_lim };
    let rcs_eff = t.rcs * aspect_factor(t.icao, r.aspect);
    let in_vol = r.az.abs() < lim && r.el.abs() < EL_LIM;
    // Vitesse radiale de la cible (sol) le long de la visée ; visée basse = fouillis de sol derrière
    let b = r.brg.to_radians();
    let th = t.hdg.to_radians();
    let vr = (t.tas * th.sin() * b.sin() + t.tas * th.cos() * b.cos()) / KT;
    let look_down = r.el < -1.0;
    let notch = look_down && vr.abs() < NOTCH_KT;
    let why = if !in_vol {
        Some("HORS LIMITES D'ANTENNE")
    } else if let Some(m) = t.masked {
        Some(m)
    } else if t.jam && r.range > burn_through(rcs_eff) {
        Some(JAM_WHY)
    } else if r.range > det_range(rcs_eff) {
        Some("HORS PORTEE (SER)")
    } else if notch {
        Some("CREUX DOPPLER (NOTCH)")
    } else {
        None
    };
    RState { in_vol, notch, vr, rcs_eff, why }
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
    pub mode: RadarMode,
    /// Renouvellement automatique de la population (désactivé dans les essais)
    pub auto_spawn: bool,
    /// Fausses alarmes radar (franchissements du seuil CFAR par le bruit)
    pub false_alarms: bool,
    /// Horloge des capteurs (s)
    clock: f64,
}

/// Télémétrie passive : fenêtre (s), bruit d'angle HOJ (deg), précision requise (fraction de la
/// distance, 1 sigma) pour valider l'estimation
pub const TMA_WIN: f64 = 90.0;
pub const TMA_SIGMA_DEG: f64 = 0.3;
pub const TMA_OK: f64 = 0.25;

/// Estimation de trajectoire par relèvements seuls (TMA), cible à vitesse constante : pour chaque
/// distance candidate le long du dernier relèvement, vitesse de la cible par moindres carrés,
/// puis coût en écarts angulaires normalisés. Distance retenue = minimum ; incertitude = zone où
/// le coût reste à moins de 1 du minimum (1 sigma). En ligne droite à vitesse constante, le coût
/// est plat : distance inobservable tant que l'avion ne manœuvre pas.
pub struct TmaFix {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub range: f64,
    pub sigma: f64,
}

fn solve3(mut m: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    let scale = m.iter().flatten().fold(0.0f64, |a, v| a.max(v.abs()));
    for c in 0..3 {
        let p = (c..3).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        if m[p][c].abs() <= scale * 1e-14 {
            return None;
        }
        m.swap(c, p);
        b.swap(c, p);
        for r in c + 1..3 {
            let f = m[r][c] / m[c][c];
            for k in c..3 {
                m[r][k] -= f * m[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 3];
    for c in (0..3).rev() {
        let mut v = b[c];
        for k in c + 1..3 {
            v -= m[c][k] * x[k];
        }
        x[c] = v / m[c][c];
    }
    Some(x)
}

/// Écart-type de l'a priori de vitesse cible (m/s)
const TMA_VPRIOR: f64 = 300.0;

pub fn tma_solve(pts: &[(f64, f64, f64, f64)], now: f64, sigma_deg: f64) -> Option<TmaFix> {
    if pts.len() < 20 || now - pts[0].0 < 15.0 {
        return None;
    }
    let sa = sigma_deg.to_radians();
    let &(_, pxn, pyn, bn) = pts.last()?;
    let u = (bn.sin(), bn.cos());
    let n = 140;
    let (r_lo, r_hi) = (1.0 * NM, 200.0 * NM);
    let mut grid = Vec::with_capacity(n);
    for k in 0..n {
        let r = r_lo * (r_hi / r_lo).powf(k as f64 / (n - 1) as f64);
        let (x0, y0) = (pxn + r * u.0, pyn + r * u.1);
        // Décalage latéral c (le dernier relèvement est bruité) et vitesse : moindres carrés
        // pondérés sur les pseudo-mesures n_i . (X0 + c u_perp + v tau - p_i) = 0
        let up = (u.1, -u.0);
        let mut m = [[0.0; 3]; 3];
        let mut v = [0.0; 3];
        for &(t, px, py, b) in pts {
            let tau = t - now;
            let (cb, sb) = (b.cos(), b.sin());
            let ri = (x0 - px).hypot(y0 - py).max(100.0);
            let w = 1.0 / (ri * sa).powi(2);
            let g = [cb * up.0 - sb * up.1, cb * tau, -sb * tau];
            let h = -(cb * (x0 - px) - sb * (y0 - py));
            for i in 0..3 {
                v[i] += w * g[i] * h;
                for j in 0..3 {
                    m[i][j] += w * g[i] * g[j];
                }
            }
        }
        // A priori sur la vitesse (cible subsonique à supersonique) : régularise la composante
        // radiale, mal observée par les seuls relèvements
        m[1][1] += 1.0 / (TMA_VPRIOR * TMA_VPRIOR);
        m[2][2] += 1.0 / (TMA_VPRIOR * TMA_VPRIOR);
        let Some([c, vx, vy]) = solve3(m, v) else { continue };
        let (x, y) = (x0 + c * up.0, y0 + c * up.1);
        // Coût : écarts angulaires réels / bruit + a priori de vitesse
        let mut j = (vx * vx + vy * vy) / (TMA_VPRIOR * TMA_VPRIOR);
        for &(t, px, py, b) in pts {
            let tau = t - now;
            let (dx, dy) = (x + vx * tau - px, y + vy * tau - py);
            let e = wrap180((b - dx.atan2(dy)).to_degrees()).to_radians() / sa;
            j += e * e;
        }
        grid.push((r, j, x, y, vx, vy));
    }
    let best = grid.iter().cloned().min_by(|a, b| a.1.total_cmp(&b.1))?;
    // Seuil 1 sigma, élargi si l'ajustement est mauvais (cible qui manœuvre : khi2 réduit > 1)
    let dof = (pts.len() as f64 - 3.0).max(1.0);
    let thr = (best.1 / dof).max(1.0);
    let inside: Vec<f64> = grid.iter().filter(|g| g.1 <= best.1 + thr).map(|g| g.0).collect();
    let lo = inside.iter().cloned().fold(f64::MAX, f64::min);
    let hi = inside.iter().cloned().fold(0.0, f64::max);
    // Bornes de la grille atteintes : distance non contrainte de ce côté
    let step = (r_hi / r_lo).powf(1.0 / (n - 1) as f64);
    let lo = if lo <= r_lo * 1.001 { 0.0 } else { lo };
    let hi = if hi >= r_hi * 0.999 { 1e9 } else { hi };
    let sigma = ((hi - lo) / 2.0).max(best.0 * (step - 1.0) / 2.0);
    Some(TmaFix { x: best.2, y: best.3, vx: best.4, vy: best.5, range: (best.2 - pxn).hypot(best.3 - pyn), sigma })
}

/// Fausses alarmes par cadre de balayage (Pfa x nombre de cellules distance-Doppler du volume)
pub const FA_RWS: f64 = 0.25;
pub const FA_TWS: f64 = 0.15;

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
        Traffic { list: Vec::new(), lock: None, aa: false, rng: 0x9E37_79B9_7F4A_7C15, next_id: 1, scan_t: 0.0, lock_loss_t: 0.0, events: Vec::new(), mode: RadarMode::Rws, auto_spawn: true, false_alarms: true, clock: 0.0 }
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
        let adsb_r = self.rand();
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
            hostile: icao == "SU27",
            adsb_out: !is_mil && adsb_r < 0.85,
            l16: iff == Iff::Ami && matches!(icao, "F16" | "F35" | "F22" | "FA18" | "E3TF" | "K35R"),
            tr_lat: lat,
            tr_lon: lon,
            tr_alt: alt,
            tr_hdg: hdg,
            tr_tas: tas,
            iff_t: -1.0,
            ..Default::default()
        });
    }

    /// Une étape : apparition / disparition autour de l'avion, déplacement, comportement.
    pub fn update(&mut self, ac: &Aircraft, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        // Population : ~10 civils et ~4 militaires dans 80 NM
        self.list.retain(|t| dist_brg(ac.lat, ac.lon, t.lat, t.lon).0 < 95.0 * NM && !(t.false_alarm && t.radar_age > MEM_S));
        let civ = self.list.iter().filter(|t| !t.mil && !t.false_alarm).count();
        let mil = self.list.iter().filter(|t| t.mil).count();
        if civ < 10 && self.auto_spawn {
            self.spawn(ac, false);
        }
        if mil < 4 && self.auto_spawn {
            self.spawn(ac, true);
        }
        if self.lock.is_some_and(|id| !self.list.iter().any(|t| t.id == id)) {
            self.lock = None;
        }
        let r = self.rand();
        for t in self.list.iter_mut() {
            t.turn_t -= dt;
            t.evade_t -= dt;
            // Esquive en cours (hostile sous STT) : cap et altitude imposés par la manœuvre
            if t.evade_t > 0.0 {
            } else {
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
            }
            let dh = wrap180(t.hdg_tgt - t.hdg);
            let rate = if t.mil && (matches!(t.behave, Behave::Patrol) || t.evade_t > 0.0) { 6.0 } else { 2.5 };
            t.hdg = wrap360(t.hdg + dh.clamp(-rate * dt, rate * dt));
            let dz = t.alt_tgt - t.alt;
            t.vs = if t.evade_t > 0.0 { (dz * 0.3).clamp(-60.0, 40.0) } else { dz.clamp(-12.0, 12.0) * 0.3 };
            t.alt += t.vs * dt;
            let (la, lo) = destination(t.lat, t.lon, t.hdg, t.tas * dt);
            t.lat = la;
            t.lon = lo;
        }
        // ---- Capteurs : radar, IFF, ADS-B, Link 16 ; fusion en pistes ----
        let mode = self.mode;
        self.clock += dt;
        let clock = self.clock;
        let (az_lim, frame_s) = mode.volume();
        self.scan_t += dt;
        let frame = self.scan_t >= frame_s;
        if frame {
            self.scan_t -= frame_s;
        }
        let awacs: Vec<(f64, f64)> = self.list.iter().filter(|t| t.icao == "E3TF").map(|t| (t.lat, t.lon)).collect();
        let lock = self.lock;
        let mut lock_why = None;
        let mut chaff_break = None;
        let mut msgs = Vec::new();
        // Fichiers de piste TWS : les plus proches parmi les pistes vivantes
        let tws: Vec<u32> = if mode == RadarMode::Tws {
            let mut v: Vec<(f64, u32)> = self.list.iter().filter(|t| t.radar_age < MEM_S).map(|t| (dist_brg(ac.lat, ac.lon, t.lat, t.lon).0, t.id)).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.into_iter().take(TWS_MAX).map(|x| x.1).collect()
        } else {
            Vec::new()
        };
        let rnd: Vec<[f64; 8]> = (0..self.list.len()).map(|_| std::array::from_fn(|_| self.rand())).collect();
        for (k, t) in self.list.iter_mut().enumerate() {
            let rn = rnd[k];
            let gauss = |i: usize| (-2.0 * rn[i].max(1e-9).ln()).sqrt() * (std::f64::consts::TAU * rn[i + 1]).cos();
            let stt = Some(t.id) == lock;
            if !stt {
                t.hoj = false;
            }
            // Fausse alarme : un seul franchissement de seuil, jamais redétectée
            if t.false_alarm {
                t.radar_age += dt;
                if stt {
                    lock_why = Some("PAS DE RETOUR RADAR");
                }
                if t.iff_t > 0.0 {
                    t.iff_t -= dt;
                    if t.iff_t <= 0.0 {
                        t.ident = Iff::Inconnu;
                        msgs.push(format!("IFF {} : SANS REPONSE - INCONNU", disp_call(t)));
                    }
                }
                continue;
            }
            t.mask_t -= dt;
            if t.mask_t <= 0.0 {
                t.masked = los_mask(ac, t);
                t.mask_t = 0.5;
            }
            let rel = Traffic::rel(ac, t);
            // Brouilleur d'autoprotection quand la cible est illuminée par notre radar
            t.jam = t.hostile && t.in_vol && t.masked.is_none() && rel.range < 90.0 * NM;
            let st = radar_state(ac, t, stt, az_lim);
            t.in_vol = st.in_vol;
            t.notch = st.notch;
            t.vr_kt = st.vr;
            t.rcs_eff = st.rcs_eff;
            t.strobe = st.why == Some(JAM_WHY);
            // Angle du brouilleur : précis en poursuite HOJ, grossier en balayage
            let sj = if stt && t.hoj { TMA_SIGMA_DEG } else { 1.0 };
            t.strobe_brg = rel.brg + gauss(2) * sj;
            t.strobe_el = rel.el + gauss(4) * sj;
            t.radar_age += dt;
            let mut hit = false;
            if stt {
                if t.hoj {
                    // Poursuite sur brouilleur : angle seul tant que le brouillage domine ; au
                    // burn-through, passage en STT normal (distance mesurée)
                    if st.why.is_none() {
                        t.hoj = false;
                        hit = true;
                        msgs.push(format!("BURN-THROUGH {} — HOJ > STT, DISTANCE MESUREE", disp_call(t)));
                    } else if st.why != Some(JAM_WHY) {
                        lock_why = st.why;
                    }
                } else if st.why.is_none() {
                    // Poursuite continue tant que la cible reste détectable
                    hit = true;
                } else {
                    lock_why = st.why;
                }
            } else if frame && st.why.is_none() {
                // Probabilité de détection : forte à courte portée, décroissante vers la limite
                let x = rel.range / det_range(st.rcs_eff);
                let pd = if x < 0.7 { 0.95 } else { 0.95 - (x - 0.7) / 0.3 * 0.65 };
                hit = rn[6] < pd;
            }
            // Télémétrie passive en HOJ : relèvements à 2 Hz pendant que l'avion manœuvre
            let mut tma_good = false;
            if stt && t.hoj {
                if t.tma_pts.is_empty() {
                    t.tma_ref = (ac.lat, ac.lon);
                }
                t.tma_dt += dt;
                if t.tma_dt >= 0.5 {
                    t.tma_dt = 0.0;
                    let (px, py) = enu_offset(t.tma_ref.0, t.tma_ref.1, ac.lat, ac.lon);
                    t.tma_pts.push((clock, px, py, t.strobe_brg.to_radians()));
                    t.tma_pts.retain(|p| clock - p.0 <= TMA_WIN);
                    let fix = tma_solve(&t.tma_pts, clock, TMA_SIGMA_DEG);
                    t.tma = fix.as_ref().map(|f| (f.range, f.sigma));
                    if let Some(f) = fix.filter(|f| f.sigma < TMA_OK * f.range) {
                        (t.tr_lat, t.tr_lon) = from_enu(t.tma_ref.0, t.tma_ref.1, f.x, f.y);
                        t.tr_alt = ac.alt + f.range * t.strobe_el.to_radians().tan();
                        t.tr_hdg = wrap360(f.vx.atan2(f.vy).to_degrees());
                        t.tr_tas = f.vx.hypot(f.vy);
                    }
                }
                tma_good = t.tma.is_some_and(|(r, sg)| sg < TMA_OK * r);
            } else {
                t.tma_pts.clear();
                t.tma = None;
                t.tma_dt = 0.0;
            }
            // Hostile sous STT (alerte RWR) : mise en travers, descente (visée basse = creux
            // Doppler), paillettes toutes les 2.5 s
            if t.hostile && stt {
                let to_own = wrap360(rel.brg + 180.0);
                let (b1, b2) = (wrap360(to_own + 90.0), wrap360(to_own - 90.0));
                let beam = if wrap180(b1 - t.hdg).abs() < wrap180(b2 - t.hdg).abs() { b1 } else { b2 };
                // Cap de travers choisi à l'alerte, repris si la menace a bougé de plus de 30°
                if t.evade_t <= 0.0 || wrap180(beam - t.hdg_tgt).abs() > 30.0 {
                    t.hdg_tgt = beam;
                }
                t.evade_t = 15.0;
                if ac.alt > 3_000.0 {
                    t.alt_tgt = t.alt_tgt.min(1_500.0);
                }
                t.chaff_t -= dt;
                if t.chaff_t <= 0.0 {
                    t.chaff_t = 2.5;
                    // Paillettes efficaces quand la cible est proche du travers (même Doppler)
                    if !t.hoj && st.vr.abs() < 150.0 && rn[7] < 0.4 {
                        chaff_break = Some(disp_call(t));
                    }
                }
            }
            // ADS-B (1 Hz, position exacte), Link 16 (amis équipés, pistes AWACS)
            t.adsb = t.adsb_out && t.squawk > 0 && rel.range < 150.0 * NM;
            let seen_by_awacs = awacs.iter().any(|&(la, lo)| dist_brg(la, lo, t.lat, t.lon).0 < 250.0 * NM);
            t.dl = t.l16 || seen_by_awacs;
            if t.dl {
                t.ident = if t.hostile { Iff::Inconnu } else { t.iff };
                if !t.hostile {
                    t.ident_call = true;
                    t.type_known = true;
                }
            }
            if t.adsb {
                t.ident = Iff::Neutre;
                t.ident_call = true;
                t.type_known = true;
                t.squawk_known = true;
            }
            // Réponse IFF
            if t.iff_t > 0.0 {
                t.iff_t -= dt;
                if t.iff_t <= 0.0 {
                    let r = if t.hostile {
                        t.ident = Iff::Inconnu;
                        "SANS REPONSE - INCONNU"
                    } else if t.mil {
                        t.ident = Iff::Ami;
                        t.squawk_known = true;
                        "MODE 5 VALIDE - AMI"
                    } else {
                        t.ident = Iff::Neutre;
                        t.squawk_known = true;
                        t.ident_call = true;
                        t.type_known = true;
                        "MODE 3/C/S - NEUTRE"
                    };
                    msgs.push(format!("IFF {} : {r}", disp_call(t)));
                }
            }
            if t.radar_age > MEM_S {
                t.det_n = 0;
            }
            if hit {
                t.radar_age = 0.0;
                t.det_n += 1;
                t.hits = if stt || tws.contains(&t.id) { t.hits + 1 } else { 0 };
                t.vc_meas = rel.closure_kt;
            }
            // Mise à jour de la piste : ADS-B > radar > Link 16 > extrapolation
            t.adsb_t += dt;
            t.dl_t += dt;
            if t.adsb && t.adsb_t >= 1.0 {
                t.adsb_t = 0.0;
                (t.tr_lat, t.tr_lon, t.tr_alt, t.tr_hdg, t.tr_tas, t.tr_vs) = (t.lat, t.lon, t.alt, t.hdg, t.tas, t.vs);
            } else if hit && !t.adsb {
                let (sr, sa) = if stt { (15.0 + 0.0005 * rel.range, 0.1) } else { (30.0 + 0.002 * rel.range, 0.5) };
                let (d, b) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
                let (la, lo) = destination(ac.lat, ac.lon, b + gauss(2) * sa, (d + gauss(0) * sr).max(0.0));
                t.tr_lat = la;
                t.tr_lon = lo;
                // Altitude : Mode C si connu (au 100 ft), sinon site radar (erreur ∝ distance)
                t.tr_alt = if t.squawk_known { (t.alt / FT / 100.0).round() * 100.0 * FT } else { t.alt + gauss(4) * rel.range * sa.to_radians() };
                if t.hits >= 2 {
                    let (sh, sv) = if stt { (1.0, 0.01) } else { (4.0, 0.04) };
                    t.tr_hdg = wrap360(t.hdg + (rn[1] - 0.5) * 2.0 * sh);
                    t.tr_tas = t.tas * (1.0 + (rn[3] - 0.5) * 2.0 * sv);
                    t.tr_vs = t.vs;
                }
            } else if t.dl && !t.adsb && t.dl_t >= 4.0 {
                t.dl_t = 0.0;
                (t.tr_lat, t.tr_lon, t.tr_alt, t.tr_hdg, t.tr_tas, t.tr_vs) = (t.lat, t.lon, t.alt, t.hdg, t.tas, t.vs);
            } else if t.vel_ok && !tma_good {
                let (la, lo) = destination(t.tr_lat, t.tr_lon, t.tr_hdg, t.tr_tas * dt);
                t.tr_lat = la;
                t.tr_lon = lo;
                t.tr_alt += t.tr_vs * dt;
            }
            t.vel_ok = t.adsb || t.dl || t.hits >= 2 || tma_good;
        }
        self.events.extend(msgs);
        // Fausses alarmes : position aléatoire dans le volume de balayage
        if frame && self.false_alarms {
            let rate = if mode == RadarMode::Tws { FA_TWS } else { FA_RWS };
            if self.rand() < rate {
                let az = (self.rand() * 2.0 - 1.0) * az_lim;
                let d = (5.0 + self.rand() * (R0_NM - 5.0)) * NM;
                let el = ((self.rand() * 2.0 - 1.0) * 10.0f64).to_radians();
                let (lat, lon) = destination(ac.lat, ac.lon, wrap360(ac.hdg_deg() + az), d);
                let alt = (ac.alt + d * el.tan()).max(300.0);
                let id = self.next_id;
                self.next_id += 1;
                self.list.push(Target {
                    id,
                    call: String::new(),
                    icao: "----",
                    lat,
                    lon,
                    alt,
                    alt_tgt: alt,
                    tr_lat: lat,
                    tr_lon: lon,
                    tr_alt: alt,
                    turn_t: 1e9,
                    radar_age: 0.0,
                    det_n: 1,
                    iff_t: -1.0,
                    false_alarm: true,
                    ..Default::default()
                });
            }
        }
        if let Some(c) = chaff_break {
            if self.lock.is_some() {
                self.lock = None;
                self.lock_loss_t = 0.0;
                self.events.push(format!("LOCK PERDU {c} — PAILLETTES"));
            }
        }
        // Rupture de poursuite (STT) après 2 s sans détection
        if self.lock.is_some() {
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
        let views: Vec<Target> = self.list.iter().map(Self::view).collect();
        SNAP.with(|s| *s.borrow_mut() = (views, self.lock, self.aa));
        MODE.with(|m| m.set(self.mode));
    }

    /// Vue affichée d'une cible : piste estimée et identité connue (jamais la vérité).
    pub fn view(t: &Target) -> Target {
        let mut v = t.clone();
        (v.lat, v.lon, v.alt, v.hdg, v.vs) = (t.tr_lat, t.tr_lon, t.tr_alt, t.tr_hdg, t.tr_vs);
        v.tas = if t.vel_ok { t.tr_tas } else { 0.0 };
        v.iff = t.ident;
        v.call = disp_call(t);
        if !t.type_known {
            v.icao = "----";
            v.name = if t.ident == Iff::Ami { "AMI (MODE 5), TYPE INCONNU" } else { "TYPE INCONNU" };
            v.operator = "--";
            v.mission = "--";
        }
        if !t.squawk_known {
            v.squawk = 0;
        }
        if !t.ident_call || !t.type_known {
            v.org.clear();
            v.dest.clear();
        }
        v
    }

    /// Interrogation IFF des pistes radar fraîches dans le volume IFF (+/-60°, 100 NM).
    pub fn interrogate(&mut self, ac: &Aircraft) -> Result<String, String> {
        let mut n = 0;
        for t in self.list.iter_mut() {
            let r = Traffic::rel(ac, t);
            if t.radar_age < 1.0 && r.az.abs() < AZ_LIM && r.range < IFF_NM * NM && t.iff_t <= 0.0 {
                t.iff_t = IFF_S;
                n += 1;
            }
        }
        if n == 0 { Err("IFF : AUCUNE PISTE RADAR A INTERROGER".into()) } else { Ok(format!("IFF : {n} PISTE(S) INTERROGEE(S) — REPONSE {IFF_S:.1} S")) }
    }


    /// Piste confirmée : 2 détections au moins (ou source coopérative ADS-B / Link 16) ; une
    /// détection isolée reste un plot non confirmé (peut être une fausse alarme).
    pub fn confirmed(t: &Target) -> bool {
        t.det_n >= 2 || t.adsb || t.dl
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
        let mut c: Vec<(f64, u32)> = self.list.iter().filter(|t| t.radar_age < 1.0 && Self::confirmed(t)).map(|t| {
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
        let id = c[i].1;
        self.try_lock(id).ok()
    }

    pub fn lock_call(&mut self, call: &str) -> Result<String, String> {
        let t = self.list.iter().find(|t| disp_call(t).replace(' ', "") == call.replace(' ', "")).ok_or("CIBLE INCONNUE")?;
        self.try_lock(t.id)
    }

    /// Verrouillage (STT) : piste radar fraîche obligatoire.
    pub fn try_lock(&mut self, id: u32) -> Result<String, String> {
        let t = self.list.iter().find(|t| t.id == id).ok_or("CIBLE INCONNUE")?;
        let hoj = t.radar_age >= 1.0 && t.strobe && t.in_vol;
        if t.radar_age >= 1.0 && !hoj {
            return Err(format!("{} : PAS DE PISTE RADAR ({}) — VERROUILLAGE IMPOSSIBLE", disp_call(t), Self::source(t)));
        }
        let call = if hoj { format!("{} HOJ (ANGLE SEUL)", disp_call(t)) } else { disp_call(t) };
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            t.hoj = hoj;
        }
        self.lock = Some(id);
        self.lock_loss_t = 0.0;
        // Interrogation IFF automatique de la cible désignée
        if let Some(t) = self.list.iter_mut().find(|t| t.id == id) {
            if t.ident == Iff::NonId && t.iff_t <= 0.0 {
                t.iff_t = IFF_S;
            }
        }
        Ok(call)
    }

    pub fn locked(&self) -> Option<&Target> {
        self.lock.and_then(|id| self.list.iter().find(|t| t.id == id))
    }

    /// Indicatif de la cible verrouillée.
    pub fn locked_call(&self) -> Option<String> {
        self.locked().map(disp_call)
    }

    /// Clic sur un symbole de l'horizon (demande à traiter).
    pub fn take_click(&mut self) -> Option<Result<String, String>> {
        let id = CLICK.with(|c| c.take())?;
        Some(self.try_lock(id))
    }

    /// Trafic pour le ND : (lat, lon, alt m, indicatif, militaire, verrouillé, IFF, vs m/s)
    pub fn nd_items(&self) -> Vec<(f64, f64, f64, String, bool, bool, Iff, f64)> {
        self.list.iter().filter(|t| Self::known(t) && Self::confirmed(t)).map(Self::view).map(|t| (t.lat, t.lon, t.alt, t.call.clone(), t.mil && t.type_known, Some(t.id) == self.lock, t.iff, t.vs)).collect()
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
        // Poursuite sur brouilleur : cadre sur la direction du brouilleur, sans distance
        if locked && t.hoj {
            let (la, lo) = destination(ac.lat, ac.lon, wrap360(t.strobe_brg), 50.0 * NM);
            let alt = ac.alt + 50.0 * NM * t.strobe_el.to_radians().tan();
            if let Some(p) = v.project(eye.local(la, lo, alt)).filter(|p| s.contains(*p)) {
                draw_rectangle_lines(p.x - 16.0, p.y - 16.0, 32.0, 32.0, 2.0, AMBER);
                draw_line(p.x - 16.0, p.y - 16.0, p.x + 16.0, p.y + 16.0, 1.2, AMBER);
                draw_line(p.x + 16.0, p.y - 16.0, p.x - 16.0, p.y + 16.0, 1.2, AMBER);
                txt_c("HOJ", p.x, p.y - 20.0, 11.0, AMBER);
                txt_c(&t.call, p.x, p.y + 28.0, 11.0, AMBER);
            }
            continue;
        }
        if (!aa && !locked) || !Traffic::known(t) || (!locked && !Traffic::confirmed(t)) {
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
        ("IFF".into(), format!("{}  {}", t.iff.label(), if t.iff_t > 0.0 { "INTERROGATION...".into() } else if t.squawk > 0 { format!("XPDR {:04}", t.squawk) } else { match t.iff { Iff::NonId => "NON INTERROGE".to_string(), Iff::Inconnu => "SANS REPONSE".into(), _ => "MODE 5".into() } }), t.iff.color()),
        ("EXPL.".into(), format!("{} - {}", t.operator, t.mission), WHITE),
        ("ALT".into(), if t.squawk_known { format!("FL{:03.0}  MODE C  {:+.0} FPM", t.alt / FT / 100.0, t.vs / FT * 60.0) } else { format!("FL{:03.0}  SITE RADAR +/-{:.0} FT", t.alt / FT / 100.0, r.range * 0.5f64.to_radians() / FT) }, WHITE),
        ("VITESSE".into(), if t.vel_ok { format!("{:.0} KT  M{mach:.2}  CAP {:03.0}", t.tas / KT, t.hdg) } else { "-- (PISTE SANS VITESSE)".into() }, if t.vel_ok { WHITE } else { DIM }),
        if t.hoj {
            match t.tma {
                Some((r, sg)) if sg < TMA_OK * r => ("DIST".into(), format!("{:.0} NM +/-{:.0} TMA  REL {:03.0}", r / NM, sg / NM, wrap360(t.strobe_brg)), GREEN_S),
                Some((r, sg)) if sg < r => ("DIST".into(), format!("~{:.0} +/-{:.0} NM - MANOEUVRER", r / NM, sg / NM), AMBER),
                _ => ("DIST".into(), format!("-- HOJ : ZIGZAG +/-45 POUR TMA  REL {:03.0}", wrap360(t.strobe_brg)), AMBER),
            }
        } else {
            ("DIST".into(), format!("{:.1} NM  REL {:03.0}  {:+.0} FT", r.range / NM, r.brg, r.dalt_ft), WHITE)
        },
    ];
    // Rapprochement : Doppler mesuré (radar) ou dérivé de la piste ; aspect si la piste a un cap
    let vc = if t.radar_age < MEM_S { Some(t.vc_meas) } else if t.vel_ok { Some(r.closure_kt) } else { None };
    let vc_s = vc.map_or("--".to_string(), |v| format!("{v:+.0} KT"));
    let asp_s = if t.vel_ok { format!("ASPECT {asp:.0}{side} {asp_l}") } else { "ASPECT --".into() };
    lines.push(("RAPPR.".into(), format!("{vc_s}  {asp_s}"), if vc.is_some_and(|v| v > 0.0) { AMBER } else { WHITE }));
    if !t.org.is_empty() {
        lines.push(("ROUTE".into(), format!("{} > {}", t.org, t.dest), WHITE));
    }
    let age = if t.radar_age < 1e8 { format!(" ({:.0} s)", t.radar_age) } else { String::new() };
    let ser = if t.radar_age < MEM_S { if t.rcs_eff < 0.1 { format!("  SER EST {:.3} m2", t.rcs_eff) } else { format!("  SER EST {:.0} m2", t.rcs_eff) } } else { String::new() };
    lines.push(("PISTE".into(), format!("{}{age}{ser}", Traffic::source(t)), if t.radar_age < 1.0 { GREEN_S } else { DIM }));
    let etat = if !Traffic::confirmed(t) && t.radar_age < MEM_S { "PLOT NON CONFIRME (1 DETECTION)".to_string() } else if let Some(m) = t.masked { m.to_string() } else if t.strobe { "BROUILLAGE - ANGLE SEUL".into() } else if t.jam { "BROUILLEUR ACTIF (BURN-THROUGH)".into() } else if t.radar_age < 1.0 { format!("VR {:+.0} KT{}", t.vr_kt, if t.notch { "  NOTCH" } else { "" }) } else { "--".into() };
    lines.push(("RADAR".into(), etat, if t.masked.is_some() || t.strobe || t.jam || t.notch { AMBER } else { WHITE }));
    lines
}

thread_local! {
    /// Mode radar courant (affichage)
    static MODE: std::cell::Cell<RadarMode> = const { std::cell::Cell::new(RadarMode::Rws) };
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
    // Volume de balayage radar et faisceau (aller-retour en 2 cadres)
    let mode = MODE.with(|m| m.get());
    let (azl, fs) = mode.volume();
    let edge = ppn * rng as f32;
    for a in [-azl as f32, azl as f32] {
        let ar = a.to_radians();
        let q = own + vec2(ar.sin(), -ar.cos()) * edge;
        draw_line(own.x, own.y, q.x, q.y, 1.0, Color::new(0.3, 1.0, 0.4, 0.3));
    }
    let ph = ((time / fs) % 2.0) as f32;
    let w = azl as f32;
    let az = if ph < 1.0 { -w + 2.0 * w * ph } else { w - 2.0 * w * (ph - 1.0) };
    if lock.is_none() {
        let ar = az.to_radians();
        let q = own + vec2(ar.sin(), -ar.cos()) * edge;
        draw_line(own.x, own.y, q.x, q.y, 2.0, Color::new(0.3, 1.0, 0.4, 0.55));
    }
    txt(&format!("RADAR {}{}  +/-{azl:.0}  CADRE {fs:.0} S  SER 5m2 {:.0} NM", mode.label(), if lock.is_some() { " + STT" } else { "" }, R0_NM), sc.x + 6.0, sc.y + 14.0, 10.0, GREEN_S);
    txt("PLEIN : RADAR   PALE : MEMOIRE / LINK16 / ADS-B   NTCH : CREUX DOPPLER   J : BROUILLEUR", sc.x + 6.0, sc.y + sc.h - 6.0, 9.0, DIM);
    // Strobes de brouillage : relèvement seul (pas de distance avant burn-through)
    let mut strobe_click = None;
    for t in list.iter().filter(|t| t.strobe) {
        let a = (t.strobe_brg - hdg).to_radians() as f32;
        let d = vec2(a.sin(), -a.cos());
        let locked = Some(t.id) == lock;
        if locked {
            // Poursuite sur brouilleur : strobe plein ; télémétrie passive : segment +/-1 sigma
            let q = own + d * edge;
            draw_line(own.x, own.y, q.x, q.y, 2.5, AMBER);
            if let Some((r, sg)) = t.tma.filter(|(r, sg)| *sg < *r) {
                let ok = sg < TMA_OK * r;
                let c = if ok { GREEN_S } else { Color::new(1.0, 0.72, 0.0, 0.6) };
                let (a0, a1) = (((r - sg).max(0.0) / NM) as f32 * ppn, (((r + sg) / NM) as f32 * ppn).min(edge));
                let (p0, p1) = (own + d * a0, own + d * a1);
                draw_line(p0.x, p0.y, p1.x, p1.y, 6.0, Color::new(c.r, c.g, c.b, 0.45));
                let pm = own + d * (((r / NM) as f32 * ppn).min(edge));
                draw_poly_lines(pm.x, pm.y, 4, 7.0, 45.0, 1.8, c);
                txt(&format!("{:.0} NM{}", r / NM, if ok { " TMA" } else { " ?" }), pm.x + 10.0, pm.y + 4.0, 10.0, c);
            }
        } else {
            let mut x = 20.0;
            while x < edge {
                let (p, q) = (own + d * x, own + d * (x + 8.0).min(edge));
                draw_line(p.x, p.y, q.x, q.y, 1.5, AMBER);
                x += 14.0;
            }
        }
        let q = own + d * (edge * 0.9);
        txt_c(if locked { "HOJ" } else { "J" }, q.x + 10.0, q.y, 12.0, AMBER);
        // Clic sur l'étiquette du strobe : poursuite sur brouilleur
        if clicked(Rect::new(q.x - 6.0, q.y - 14.0, 34.0, 22.0)) {
            strobe_click = Some(t.id);
        }
    }
    // Avion
    draw_triangle(own + vec2(0.0, -9.0), own + vec2(-6.0, 7.0), own + vec2(6.0, 7.0), WHITE);
    txt_c(&format!("FL{:03.0}", ac.alt / FT / 100.0), own.x, own.y + 20.0, 10.0, WHITE);
    let mut clicked_id = None;
    for t in list.iter().filter(|t| Traffic::known(t) && !t.hoj) {
        let p = to_s(t.lat, t.lon);
        if !sc.contains(p) {
            continue;
        }
        let locked = Some(t.id) == lock;
        // Plot non confirmé (une seule détection) : petite croix, sans étiquette
        if !Traffic::confirmed(t) && !locked {
            let c = Color::new(0.8, 0.78, 0.6, 0.6);
            draw_line(p.x - 4.0, p.y, p.x + 4.0, p.y, 1.2, c);
            draw_line(p.x, p.y - 4.0, p.x, p.y + 4.0, 1.2, c);
            if clicked(Rect::new(p.x - 10.0, p.y - 10.0, 20.0, 20.0)) {
                clicked_id = Some(t.id);
            }
            continue;
        }
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
            Iff::NonId => draw_poly_lines(p.x, p.y, 3, 7.0, -90.0, 1.6, c),
        }
        if t.mil && t.type_known {
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
    let card_h = lock_t.map_or(0.0, |t| 14.0 + target_lines(ac, t).len() as f32 * 15.5);
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
        let src = if t.hoj { "HOJ" } else if !Traffic::confirmed(t) && t.radar_age < MEM_S { "PLOT" } else { match Traffic::source(t) { "RADAR" => "RDR", "MEMOIRE" => "MEM", "LINK 16" => "L16", "ADS-B" => "ADS", _ => "--" } };
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
    if let Some(id) = clicked_id.or(strobe_click) {
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
            id: 1, call: "TEST1".into(), icao, mil: true, iff: Iff::Inconnu, lat, lon, alt: alt_ft * FT, tas: 230.0, hdg,
            alt_tgt: alt_ft * FT, hdg_tgt: hdg, turn_t: 1e9, rcs: rcs_of(icao), radar_age: 1e9, iff_t: -1.0,
            tr_lat: lat, tr_lon: lon, tr_alt: alt_ft * FT, tr_hdg: hdg, tr_tas: 230.0,
            ..Default::default()
        }
    }

    /// Fait tourner le trafic sans apparition de nouveaux avions.
    fn run(tr: &mut Traffic, ac: &Aircraft, secs: f64, mut stop: impl FnMut(&Traffic) -> bool) -> f64 {
        let mut tt = 0.0;
        tr.auto_spawn = false;
        tr.false_alarms = false;
        while tt < secs && !stop(tr) {
            tr.update(ac, 0.1);
            tt += 0.1;
        }
        tt
    }

    /// Équation radar : un furtif n'est vu qu'à courte portée, un gros porteur de très loin.
    #[test]
    fn detection_range_by_rcs() {
        let ac = own();
        println!("portees : F-22 {:.1} NM, F-35 {:.1} NM, F-16 {:.0} NM, 777 {:.0} NM", det_range(rcs_of("F22")) / NM, det_range(rcs_of("F35")) / NM, det_range(rcs_of("F16")) / NM, det_range(rcs_of("B77W")) / NM);
        let far_stealth = tgt(&ac, "F22", 20.0, 0.0, 30_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &far_stealth, false, AZ_LIM).why.is_some(), "F-22 a 20 NM ne doit pas etre vu");
        let near_stealth = tgt(&ac, "F22", 6.0, 0.0, 30_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &near_stealth, false, AZ_LIM).why.is_none(), "F-22 a 6 NM vu");
        let liner = tgt(&ac, "B77W", 120.0, 10.0, 35_000.0, ac.hdg_deg() + 180.0);
        assert!(radar_state(&ac, &liner, false, AZ_LIM).why.is_none(), "777 a 120 NM vu");
        let behind = tgt(&ac, "B77W", 20.0, 150.0, 30_000.0, 0.0);
        assert_eq!(radar_state(&ac, &behind, false, AZ_LIM).why, Some("HORS LIMITES D'ANTENNE"));
    }

    /// Creux Doppler : cible en dessous, en travers (vitesse radiale sol ~ 0) -> perdue dans le
    /// fouillis ; la même cible de face est vue.
    #[test]
    fn doppler_notch() {
        let ac = own();
        let brg = ac.hdg_deg();
        let beam = tgt(&ac, "F16", 20.0, 0.0, 5_000.0, wrap360(brg + 90.0));
        let st = radar_state(&ac, &beam, false, AZ_LIM);
        let (notch, vr, why) = (st.notch, st.vr, st.why);
        println!("travers vue basse : VR {vr:+.0} kt, notch {notch}, {why:?}");
        assert!(notch && why == Some("CREUX DOPPLER (NOTCH)"));
        let hot = tgt(&ac, "F16", 20.0, 0.0, 5_000.0, wrap360(brg + 180.0));
        assert!(!radar_state(&ac, &hot, false, AZ_LIM).notch);
        // Visée haute : pas de fouillis, pas de creux
        let above = tgt(&ac, "F16", 20.0, 0.0, 45_000.0, wrap360(brg + 90.0));
        assert!(!radar_state(&ac, &above, false, AZ_LIM).notch);
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
        let tt = run(&mut tr, &ac, 5.0, |t| t.lock.is_none());
        println!("STT rompu apres {tt:.1} s : {:?}", tr.events);
        assert!(tr.lock.is_none() && tt > 1.9 && tr.events.iter().any(|e| e.contains("NOTCH")));
        // Piste perdue en mémoire puis plus verrouillable
        assert!(tr.try_lock(1).is_err());
    }

    /// SER selon l'aspect : un chasseur vu de flanc renvoie bien plus que de face ; un furtif a
    /// des pics de flanc et d'arrière.
    #[test]
    fn rcs_by_aspect() {
        for k in ["F16", "F22"] {
            let (n, b, t) = (aspect_factor(k, 0.0), aspect_factor(k, 90.0), aspect_factor(k, 180.0));
            println!("{k} : face x{n:.1}, flanc x{b:.1}, arriere x{t:.1} -> portee flanc {:.0} NM", det_range(rcs_of(k) * b) / NM);
            assert!(b > 4.0 * n && t > n);
        }
    }

    /// Horizon radar : deux avions à 500 ft se perdent sous l'horizon vers 55 NM ; à haute
    /// altitude, pas de masquage.
    #[test]
    fn radar_horizon() {
        let mut ac = own();
        ac.alt = 500.0 * FT;
        let far = tgt(&ac, "B77W", 70.0, 0.0, 500.0, 0.0);
        let near = tgt(&ac, "B77W", 40.0, 0.0, 500.0, 0.0);
        assert_eq!(los_mask(&ac, &far), Some("SOUS L'HORIZON RADAR"));
        assert_eq!(los_mask(&ac, &near), None);
        ac.alt = 30_000.0 * FT;
        assert_eq!(los_mask(&ac, &tgt(&ac, "B77W", 150.0, 0.0, 35_000.0, 0.0)), None);
    }

    /// IFF : identité inconnue tant que non interrogé ; réponse après 1.5 s ; hostile sans réponse.
    #[test]
    fn iff_interrogation() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        let mut civ = tgt(&ac, "A320", 30.0, 5.0, 33_000.0, wrap360(ac.hdg_deg() + 180.0));
        (civ.mil, civ.iff, civ.squawk, civ.radar_age, civ.call) = (false, Iff::Neutre, 4521, 0.0, "AFR123".into());
        let mut bog = tgt(&ac, "SU27", 30.0, -5.0, 30_000.0, wrap360(ac.hdg_deg() + 180.0));
        (bog.id, bog.hostile, bog.radar_age) = (2, true, 0.0);
        tr.list.push(civ);
        tr.list.push(bog);
        assert_eq!(Traffic::view(&tr.list[0]).call, "TRK 01");
        assert_eq!(Traffic::view(&tr.list[0]).iff, Iff::NonId);
        assert!(tr.interrogate(&ac).is_ok());
        run(&mut tr, &ac, 1.0, |_| false);
        assert_eq!(tr.list[0].ident, Iff::NonId, "pas de reponse avant 1.5 s");
        run(&mut tr, &ac, 1.0, |_| false);
        println!("{:?}", tr.events);
        let v = Traffic::view(&tr.list[0]);
        assert!(v.iff == Iff::Neutre && v.call == "AFR123" && v.squawk == 4521);
        assert_eq!(tr.list[1].ident, Iff::Inconnu);
        assert!(Traffic::view(&tr.list[1]).call.starts_with("TRK"));
    }

    /// Pistes : en RWS pas de vitesse (détections brutes), en TWS la piste a cap et vitesse après
    /// 2 détections ; l'erreur de position reste dans le bruit de mesure.
    #[test]
    fn tws_builds_tracks() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        let mut t = tgt(&ac, "F16", 30.0, 10.0, 28_000.0, wrap360(ac.hdg_deg() + 170.0));
        (t.tr_lat, t.tr_lon) = (0.0, 0.0);
        tr.list.push(t);
        run(&mut tr, &ac, 8.0, |_| false);
        assert!(!tr.list[0].vel_ok, "RWS : pas de vitesse de piste");
        tr.mode = RadarMode::Tws;
        run(&mut tr, &ac, 6.0, |_| false);
        let t = &tr.list[0];
        let err = dist_brg(t.lat, t.lon, t.tr_lat, t.tr_lon).0;
        println!("TWS : {} detections, erreur position {err:.0} m, cap estime {:.0} (vrai {:.0})", t.hits, t.tr_hdg, t.hdg);
        assert!(t.vel_ok && t.hits >= 2 && err < 1_500.0);
    }

    /// Hostile verrouillé en STT : se met en travers et descend, puis le lock casse (creux Doppler
    /// ou paillettes) ; brouilleur : angle seul au-delà du burn-through.
    #[test]
    fn hostile_evades_and_jams() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        let mut t = tgt(&ac, "SU27", 25.0, 0.0, 25_000.0, wrap360(ac.hdg_deg() + 180.0));
        (t.hostile, t.radar_age) = (true, 0.0);
        tr.list.push(t);
        assert!(tr.try_lock(1).is_ok());
        let tt = run(&mut tr, &ac, 60.0, |t| t.lock.is_none());
        println!("hostile : lock perdu apres {tt:.1} s : {:?}", tr.events);
        assert!(tr.lock.is_none() && tr.events.iter().any(|e| e.contains("PAILLETTES") || e.contains("NOTCH")));
        // Brouilleur à 60 NM, de face : SER 15 m2 -> burn-through ~43 NM, angle seul
        let mut far = tgt(&ac, "SU27", 60.0, 0.0, 30_000.0, wrap360(ac.hdg_deg() + 180.0));
        (far.hostile, far.jam) = (true, true);
        let st = radar_state(&ac, &far, false, AZ_LIM);
        println!("burn-through {:.0} NM : {:?}", burn_through(st.rcs_eff) / NM, st.why);
        assert_eq!(st.why, Some(JAM_WHY));
    }

    /// Poursuite sur brouilleur : verrouillage angle seul au-delà du burn-through, puis STT normal
    /// quand la cible se rapproche.
    #[test]
    fn home_on_jam_then_burn_through() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        // Brouilleur à 60 NM qui fonce vers nous (Mach 0.95 + nous)
        let mut t = tgt(&ac, "SU27", 60.0, 0.0, 30_000.0, wrap360(ac.hdg_deg() + 180.0));
        (t.hostile, t.in_vol, t.tas) = (true, true, 290.0);
        tr.list.push(t);
        run(&mut tr, &ac, 1.0, |_| false);
        assert!(tr.list[0].strobe && tr.list[0].radar_age > 1.0, "strobe, pas de distance");
        let r = tr.try_lock(1);
        println!("HOJ : {r:?}");
        assert!(r.is_ok() && tr.list[0].hoj);
        // La cible esquive (travers) mais reste dans le volume : on la suit en angle
        let tt = run(&mut tr, &ac, 120.0, |t| t.lock.is_none() || !t.list[0].hoj);
        let d = Traffic::rel(&ac, &tr.list[0]).range / NM;
        println!("apres {tt:.0} s : lock {:?}, hoj {}, {d:.0} NM, {:?}", tr.lock, tr.list[0].hoj, tr.events);
        assert!(tr.events.iter().any(|e| e.contains("BURN-THROUGH")) || tr.events.iter().any(|e| e.contains("LOCK PERDU")));
    }

    /// Fausses alarmes : plots isolés, jamais confirmés, disparaissent ; un verrouillage dessus
    /// casse faute de retour ; LOCK auto ne les choisit pas.
    #[test]
    fn false_alarms_never_confirm() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.auto_spawn = false;
        let mut seen = 0;
        let mut tt = 0.0;
        while tt < 120.0 {
            tr.update(&ac, 0.1);
            tt += 0.1;
            seen = seen.max(tr.list.iter().filter(|t| t.false_alarm).count());
            assert!(tr.list.iter().filter(|t| t.false_alarm).all(|t| !Traffic::confirmed(t)));
        }
        println!("fausses alarmes simultanees max {seen}, en cours {}", tr.list.len());
        assert!(seen >= 1);
        assert!(tr.lock_next(&ac).is_none(), "pas de LOCK auto sur un plot");
        // Verrouillage forcé sur une fausse alarme fraîche
        let fa = loop {
            tr.update(&ac, 0.1);
            if let Some(t) = tr.list.iter().find(|t| t.false_alarm && t.radar_age < 0.5) {
                break t.id;
            }
        };
        assert!(tr.try_lock(fa).is_ok());
        tr.false_alarms = false;
        for _ in 0..30 {
            tr.update(&ac, 0.1);
        }
        println!("{:?}", tr.events);
        assert!(tr.lock.is_none() && tr.events.iter().any(|e| e.contains("PAS DE RETOUR")));
    }

    /// Télémétrie passive : en ligne droite, distance du brouilleur inobservable ; après un
    /// zigzag de +/-45°, estimation à mieux que 25 % (1 sigma) et erreur réelle < 20 %.
    #[test]
    fn hoj_passive_ranging_needs_maneuver() {
        let mut ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        tr.auto_spawn = false;
        tr.false_alarms = false;
        let hdg0 = ac.hdg_deg();
        let mut t = tgt(&ac, "SU27", 80.0, 20.0, 30_000.0, wrap360(hdg0 + 20.0 + 180.0 + 90.0));
        // SER réduite (petit brouilleur) : burn-through au-delà de la zone d'essai
        (t.hostile, t.in_vol, t.rcs) = (true, true, 3.0);
        tr.list.push(t);
        tr.update(&ac, 0.1);
        assert!(tr.try_lock(1).is_ok() && tr.list[0].hoj);
        let gs = 250.0;
        let mut step = |ac: &mut Aircraft, tr: &mut Traffic, secs: f64, rate_deg: f64| {
            let mut tt = 0.0;
            while tt < secs {
                ac.psi = (ac.hdg_deg() + rate_deg * 0.1).to_radians();
                let (la, lo) = destination(ac.lat, ac.lon, ac.hdg_deg(), gs * 0.1);
                (ac.lat, ac.lon) = (la, lo);
                tr.update(ac, 0.1);
                tt += 0.1;
            }
        };
        step(&mut ac, &mut tr, 30.0, 0.0);
        let st = tr.list[0].tma;
        println!("ligne droite 30 s : {:?}", st.map(|(r, s)| (r / NM, s / NM)));
        assert!(tr.list[0].hoj, "toujours en HOJ");
        assert!(st.is_none_or(|(r, s)| s > TMA_OK * r), "pas de telemetrie sans manoeuvre");
        // Virage de 45° à 3°/s (la cible reste dans les +/-70° de l'antenne) puis ligne droite
        step(&mut ac, &mut tr, 15.0, 3.0);
        step(&mut ac, &mut tr, 25.0, 0.0);
        // Deuxième branche du zigzag
        step(&mut ac, &mut tr, 15.0, -3.0);
        step(&mut ac, &mut tr, 15.0, 0.0);
        println!("lock {:?} hoj {} az {:.0}", tr.lock, tr.list[0].hoj, Traffic::rel(&ac, &tr.list[0]).az);
        let t = &tr.list[0];
        let truth = Traffic::rel(&ac, t).range;
        let (r, sg) = t.tma.expect("estimation");
        println!("apres manoeuvre : estime {:.1} NM +/-{:.1}, vrai {:.1} NM, cap estime {:.0} vrai {:.0}", r / NM, sg / NM, truth / NM, t.tr_hdg, t.hdg);
        assert!(t.hoj && sg < TMA_OK * r && (r - truth).abs() < 0.2 * truth);
    }
}
