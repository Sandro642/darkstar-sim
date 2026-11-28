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
    /// Écho de sol mobile (véhicule, train) passé par le filtre Doppler, durée de vie (s)
    clutter: bool,
    life: f64,
    /// Altitude du sol sous la cible (m) ; multitrajet (réflexion sur le sol / la mer) et
    /// facteur de propagation F (0 = évanouissement, 2 = renforcement)
    pub gnd: f64,
    pub multipath: bool,
    pub mp_f: f64,
    /// Inclinaison (rad, déduite du taux de virage) pour l'affichage 3D
    pub bank: f64,
    /// Menace sur notre avion (radar de bord hostile) : 0 rien, 1 veille, 3 accrochage
    pub threat: u8,
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
    } else if r.range > det_range(rcs_eff) * if t.multipath { t.mp_f.min(1.6) } else { 1.0 } {
        Some(if t.multipath && t.mp_f < 0.7 { "EVANOUISSEMENT (MULTITRAJET)" } else { "HORS PORTEE (SER)" })
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
    /// Modes de combat rapproché (acquisition automatique à moins de 10 NM)
    pub acm: Acm,
}

/// Modes ACM : axe (cercle de 3° dans l'axe du nez) et balayage vertical (bande de 4° de large,
/// de -2 à +40° au-dessus du nez) ; verrouillage automatique de la première cible détectée.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Acm {
    Off,
    Bore,
    Vs,
}

pub const ACM_NM: f64 = 10.0;
/// Longueur d'onde du radar (bande X, m), coefficients de réflexion terre / mer
const LAMBDA: f64 = 0.03;
const RHO_LAND: f64 = 0.5;
const RHO_SEA: f64 = 0.9;
/// Largeur du faisceau (rad) : la cible et son image sont dans le même lobe en dessous
const BEAM: f64 = 0.052;

/// Facteur de propagation à deux trajets (direct + réfléchi) : F = |1 + rho e^(i phi)|,
/// phi = 4 pi h1 h2 / (lambda R) + pi. La puissance reçue varie en F^4, la portée en F.
pub fn multipath_f(h1: f64, h2: f64, range: f64, sea: bool) -> f64 {
    let rho = if sea { RHO_SEA } else { RHO_LAND };
    let phi = 4.0 * std::f64::consts::PI * h1.max(1.0) * h2.max(1.0) / (LAMBDA * range.max(1.0)) + std::f64::consts::PI;
    (1.0 + rho * rho + 2.0 * rho * phi.cos()).max(0.0).sqrt()
}

/// Altitude du sol (m) et présence de mer (pas de relief chargé ou altitude nulle).
fn ground_at(lat: f64, lon: f64) -> (f64, bool) {
    match crate::tiles::elevation(lat, lon) {
        Some(z) if z > 1.0 => (z, false),
        _ => (0.0, true),
    }
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
        Traffic { list: Vec::new(), lock: None, aa: false, rng: 0x9E37_79B9_7F4A_7C15, next_id: 1, scan_t: 0.0, lock_loss_t: 0.0, events: Vec::new(), mode: RadarMode::Rws, auto_spawn: true, false_alarms: true, clock: 0.0, acm: Acm::Off }
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

    /// Fait apparaître un trafic du type donné 3 NM devant l'avion, même cap, 500 ft plus haut
    /// (essais de verrouillage et d'identification visuelle).
    pub fn spawn_near(&mut self, ac: &Aircraft, icao: &str, dist_nm: f64) -> Result<String, String> {
        let kind = CIVIL.iter().chain(MILITARY.iter()).find(|k| k.0 == icao).ok_or_else(|| {
            let all: Vec<&str> = CIVIL.iter().chain(MILITARY.iter()).map(|k| k.0).collect();
            format!("TYPE INCONNU — {}", all.join(" "))
        })?;
        self.spawn(ac, kind.4);
        let t = self.list.last_mut().ok_or("ECHEC")?;
        let (icao_s, name, operator, mission, _, mach, _, iff) = *kind;
        let (lat, lon) = destination(ac.lat, ac.lon, ac.hdg_deg(), dist_nm.clamp(0.2, 40.0) * NM);
        let alt = ac.alt + 500.0 * FT;
        (t.icao, t.name, t.operator, t.mission, t.iff, t.rcs, t.hostile) = (icao_s, name, operator, mission, iff, rcs_of(icao_s), icao_s == "SU27");
        (t.lat, t.lon, t.alt, t.alt_tgt, t.hdg, t.hdg_tgt) = (lat, lon, alt, alt, ac.hdg_deg(), ac.hdg_deg());
        (t.tr_lat, t.tr_lon, t.tr_alt, t.tr_hdg) = (lat, lon, alt, ac.hdg_deg());
        t.tas = (mach * crate::atmosphere::isa(alt).a).min(ac.tas * 0.9).max(80.0);
        t.tr_tas = t.tas;
        t.behave = Behave::Route;
        t.turn_t = 120.0;
        Ok(format!("{} {} A {:.1} NM DEVANT, +500 FT", t.call, t.name, dist_nm.clamp(0.2, 40.0)))
    }

    /// Une étape : apparition / disparition autour de l'avion, déplacement, comportement.
    pub fn update(&mut self, ac: &Aircraft, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        // Population : ~10 civils et ~4 militaires dans 80 NM
        self.list.retain(|t| dist_brg(ac.lat, ac.lon, t.lat, t.lon).0 < 95.0 * NM && !(t.false_alarm && t.radar_age > MEM_S) && !(t.clutter && t.life < 0.0 && Some(t.id) != self.lock));
        let civ = self.list.iter().filter(|t| !t.mil && !t.false_alarm && !t.clutter).count();
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
            // Hostile qui nous accroche (radar de bord) : poursuite vers nous, à notre altitude
            if t.hostile && t.threat >= 3 && t.evade_t <= 0.0 {
                let (_, b) = dist_brg(t.lat, t.lon, ac.lat, ac.lon);
                t.hdg_tgt = b;
                t.alt_tgt = ac.alt.clamp(1_000.0, 16_000.0);
            }
            t.life -= dt;
            let dh = wrap180(t.hdg_tgt - t.hdg);
            let rate = if t.mil && (matches!(t.behave, Behave::Patrol) || t.evade_t > 0.0 || t.threat >= 3) { 6.0 } else { 2.5 };
            let turn = dh.clamp(-rate * dt, rate * dt);
            t.hdg = wrap360(t.hdg + turn);
            // Virage coordonné : inclinaison = atan(V omega / g), lissée
            let bank_tgt = (t.tas * (turn / dt).to_radians() / 9.80665).atan();
            t.bank += (bank_tgt - t.bank) * (2.0 * dt).min(1.0);
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
        let own_gnd = ground_at(ac.lat, ac.lon).0;
        for (k, t) in self.list.iter_mut().enumerate() {
            let rn = rnd[k];
            let gauss = |i: usize| (-2.0 * rn[i].max(1e-9).ln()).sqrt() * (std::f64::consts::TAU * rn[i + 1]).cos();
            let stt = Some(t.id) == lock;
            if !stt {
                t.hoj = false;
            }
            // Fausse alarme : un seul franchissement de seuil, jamais redétectée
            if t.false_alarm || (t.clutter && t.life < 0.0) {
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
            let sea_t;
            if t.mask_t <= 0.0 {
                t.masked = los_mask(ac, t);
                t.mask_t = 0.5;
                let (z, sea) = ground_at(t.lat, t.lon);
                t.gnd = z;
                sea_t = sea;
            } else {
                sea_t = t.gnd <= 0.0;
            }
            let rel = Traffic::rel(ac, t);
            // Multitrajet : cible basse dont l'image (sous la surface) tombe dans le même lobe
            let h_t = (t.alt - t.gnd).max(1.0);
            let h_r = (ac.alt - own_gnd).max(1.0);
            t.multipath = !t.clutter && 2.0 * h_t / rel.range.max(1.0) < BEAM && h_t < 1_500.0;
            t.mp_f = if t.multipath { multipath_f(h_r, h_t, rel.range, sea_t) } else { 1.0 };
            // Radar de bord hostile : veille si nous sommes dans son cône avant (+/-60°) à moins de
            // 45 NM, accrochage à moins de 25 NM
            if t.hostile {
                let (_, b_to_us) = dist_brg(t.lat, t.lon, ac.lat, ac.lon);
                let off = wrap180(b_to_us - t.hdg).abs();
                let prev = t.threat;
                t.threat = if rel.range < 25.0 * NM && off < 60.0 { 3 } else if rel.range < 45.0 * NM && off < 60.0 { 1 } else { 0 };
                if t.threat == 3 && prev < 3 {
                    let clock_h = ((wrap360(rel.brg - ac.hdg_deg()) / 30.0).round() as i32 + 11) % 12 + 1;
                    msgs.push(format!("RWR : ACCROCHAGE RADAR AIR — FLANKER {clock_h}H {:.0} NM", rel.range / NM));
                }
            }
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
                // (multitrajet : le site mesuré hésite entre la cible et son image sous la surface)
                t.tr_alt = if t.squawk_known {
                    (t.alt / FT / 100.0).round() * 100.0 * FT
                } else if t.multipath {
                    (t.alt + gauss(4) * (t.alt - t.gnd).max(30.0) + gauss(4) * rel.range * sa.to_radians()).max(t.gnd)
                } else {
                    t.alt + gauss(4) * rel.range * sa.to_radians()
                };
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
        // Échos de sol mobiles (véhicules rapides, trains) : vitesse radiale au-delà du creux
        // Doppler, dans la zone où le faisceau touche le sol ; terre seulement
        if frame && self.false_alarms && self.rand() < 0.2 {
            let agl = (ac.alt - own_gnd).max(30.0);
            let r_min = agl / EL_LIM.to_radians().sin();
            if r_min < 30.0 * NM {
                let az = (self.rand() * 2.0 - 1.0) * az_lim;
                let d = r_min + self.rand() * (30.0 * NM - r_min);
                let (lat, lon) = destination(ac.lat, ac.lon, wrap360(ac.hdg_deg() + az), d);
                if let Some(z) = crate::tiles::elevation(lat, lon).filter(|z| *z > 1.0) {
                    let id = self.next_id;
                    self.next_id += 1;
                    let hdg = self.rand() * 360.0;
                    let tas = 28.0 + self.rand() * 20.0;
                    let life = 10.0 + self.rand() * 30.0;
                    self.list.push(Target {
                        id,
                        call: String::new(),
                        icao: "----",
                        lat,
                        lon,
                        alt: z + 3.0,
                        alt_tgt: z + 3.0,
                        tas,
                        hdg,
                        hdg_tgt: hdg,
                        tr_lat: lat,
                        tr_lon: lon,
                        tr_alt: z + 3.0,
                        tr_hdg: hdg,
                        tr_tas: tas,
                        turn_t: 1e9,
                        rcs: 10.0,
                        radar_age: 1e9,
                        iff_t: -1.0,
                        gnd: z,
                        clutter: true,
                        life,
                        ..Default::default()
                    });
                }
            }
        }
        // ACM : acquisition automatique de la première cible détectable dans le volume
        if self.acm != Acm::Off && self.lock.is_none() {
            let pitch = ac.pitch_att().to_degrees();
            let mut best: Option<(f64, u32)> = None;
            for t in &self.list {
                if t.radar_age >= 1.0 || t.false_alarm {
                    continue;
                }
                let r = Traffic::rel(ac, t);
                let el = r.el - pitch;
                let inside = match self.acm {
                    Acm::Bore => r.az.abs() < 3.0 && el.abs() < 3.0,
                    Acm::Vs => r.az.abs() < 2.0 && (-2.0..40.0).contains(&el),
                    Acm::Off => false,
                };
                if inside && r.range < ACM_NM * NM && best.is_none_or(|b| r.range < b.0) {
                    best = Some((r.range, t.id));
                }
            }
            if let Some((_, id)) = best {
                if let Ok(c) = self.try_lock(id) {
                    self.events.push(format!("ACM {} : LOCK {c}", if self.acm == Acm::Bore { "BORE" } else { "VS" }));
                    self.acm = Acm::Off;
                }
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
        ACM.with(|m| m.set(self.acm));
        // Avions réels (vus à l'œil) pour le rendu 3D : position vraie, cap, pente, inclinaison
        VIS.with(|v| {
            *v.borrow_mut() = self
                .list
                .iter()
                .filter(|t| !t.false_alarm && !t.clutter)
                .map(|t| (t.icao, t.lat, t.lon, t.alt, t.hdg, (t.vs / t.tas.max(1.0)).atan(), t.bank, t.mil))
                .collect()
        });
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

    /// Menaces aériennes pour le RWR (radars de bord hostiles qui nous éclairent).
    pub fn threat_cues(&self, ac: &Aircraft) -> Vec<crate::pfd_plus::ThreatCue> {
        self.list
            .iter()
            .filter(|t| t.hostile && t.threat > 0)
            .map(|t| crate::pfd_plus::ThreatCue {
                site: 10_000 + t.id as usize,
                lat: t.lat,
                lon: t.lon,
                name: "FLANKER",
                sam: false,
                state: t.threat,
                dist: dist_brg(ac.lat, ac.lon, t.lat, t.lon).0,
                missile: None,
                escape: if t.threat >= 3 { "AIR : SE METTRE EN TRAVERS / ACCELERER".into() } else { String::new() },
            })
            .collect()
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


    /// Piste classée « mobile au sol » (GMT) : altitude estimée au ras du sol et vitesse de
    /// véhicule ; rejetée de l'acquisition automatique (écho parasite probable).
    pub fn gmt(t: &Target) -> bool {
        !t.adsb && !t.dl && t.tr_alt - t.gnd < 150.0 && t.vel_ok && t.tr_tas < 60.0
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
        let mut c: Vec<(f64, u32)> = self.list.iter().filter(|t| t.radar_age < 1.0 && Self::confirmed(t) && !Self::gmt(t)).map(|t| {
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

/// Infos défilantes de la cible verrouillée (une à la fois, au-dessus du cadre LOCK).
pub fn lock_ticker(ac: &Aircraft, t: &Target) -> Vec<(String, Color)> {
    let r = Traffic::rel(ac, t);
    let mut v: Vec<(String, Color)> = Vec::new();
    v.push((format!("{} {}", t.call, t.iff.label()), t.iff.color()));
    if t.icao != "----" {
        v.push((format!("{} {}", t.icao, t.name.split_whitespace().take(2).collect::<Vec<_>>().join(" ")), WHITE));
    }
    if t.hoj {
        v.push((match t.tma {
            Some((rr, sg)) if sg < TMA_OK * rr => format!("HOJ {:.0} NM +/-{:.0} TMA", rr / NM, sg / NM),
            _ => "HOJ ANGLE SEUL - ZIGZAG".into(),
        }, AMBER));
    } else {
        let vc = if t.radar_age < MEM_S { t.vc_meas } else { r.closure_kt };
        v.push((format!("{:.1} NM  VC {:+.0}", r.range / NM, vc), if vc > 0.0 { AMBER } else { WHITE }));
    }
    v.push((format!("FL{:03.0} {:+.0} FT{}", t.alt / FT / 100.0, r.dalt_ft, if t.squawk_known { "" } else { " (SITE)" }), WHITE));
    if t.vel_ok {
        let mach = t.tas / crate::atmosphere::isa(t.alt).a;
        v.push((format!("{:.0} KT M{mach:.2} CAP {:03.0}", t.tas / KT, t.hdg), WHITE));
        let asp = r.aspect.abs();
        let l = if asp < 45.0 { "HOT" } else if asp < 120.0 { "FLANC" } else { "COLD" };
        v.push((format!("ASPECT {:.0}{} {l}", asp, if r.aspect >= 0.0 { "R" } else { "L" }), WHITE));
    }
    let src = if t.hoj { "HOJ" } else { Traffic::source(t) };
    let st = if t.multipath { " MULTITRAJET" } else if t.notch { " NOTCH" } else if t.jam { " BROUILLEUR" } else { "" };
    v.push((format!("{src}{st}"), if st.is_empty() { GREEN_S } else { AMBER }));
    v
}

/// Point d'interception à cap constant (cible à vitesse constante) : (lat, lon, alt, temps s).
fn intercept(ac: &Aircraft, t: &Target) -> Option<(f64, f64, f64, f64)> {
    if !t.vel_ok {
        return None;
    }
    let (px, py) = enu_offset(ac.lat, ac.lon, t.lat, t.lon);
    let pz = t.alt - ac.alt;
    let th = t.hdg.to_radians();
    let (vx, vy, vz) = (t.tas * th.sin(), t.tas * th.cos(), t.vs);
    let v0 = ac.tas.max(50.0);
    let a = vx * vx + vy * vy + vz * vz - v0 * v0;
    let b = 2.0 * (px * vx + py * vy + pz * vz);
    let c = px * px + py * py + pz * pz;
    let tt = if a.abs() < 1e-6 {
        if b < 0.0 { -c / b } else { return None }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return None;
        }
        let (t1, t2) = ((-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a));
        [t1, t2].into_iter().filter(|x| *x > 0.0).fold(f64::NAN, f64::min)
    };
    if !tt.is_finite() || tt > 1_800.0 {
        return None;
    }
    let (la, lo) = from_enu(ac.lat, ac.lon, px + vx * tt, py + vy * tt);
    Some((la, lo, t.alt + vz * tt, tt))
}

/// Direction (repère local de l'œil) à cap / site donnés par rapport au nez.
fn nose_dir(ac: &Aircraft, d_az: f64, d_el: f64) -> Vec3 {
    let h = (ac.hdg_deg() + d_az).to_radians();
    let p = (ac.pitch_att().to_degrees() + d_el).to_radians();
    vec3((h.sin() * p.cos()) as f32, p.sin() as f32, (-h.cos() * p.cos()) as f32)
}

/// Horizon : symboles du trafic (mode A/A), cadre de désignation avec infos défilantes au-dessus,
/// échelle de distance et rapprochement, point d'interception et temps, volumes ACM, alerte
/// collision ; repère de direction hors champ.
pub fn draw_hud(ac: &Aircraft, v: &crate::svs::View, s: Rect, time: f64) {
    let (list, lock, aa) = SNAP.with(|x| x.borrow().clone());
    let mode = MODE.with(|m| m.get());
    let acm = ACM.with(|m| m.get());
    let eye = crate::terrain::EyeFrame::new(ac);
    let c = vec2(s.x + s.w / 2.0, s.y + s.h / 2.0);
    if !aa && lock.is_none() && acm == Acm::Off {
        return;
    }
    // Bandeau du mode
    let locked_t = lock.and_then(|id| list.iter().find(|t| t.id == id));
    let sub = match (acm, locked_t) {
        (_, Some(t)) if t.hoj => "HOJ".to_string(),
        (_, Some(_)) => "STT".to_string(),
        (Acm::Bore, None) => "ACM BORE".to_string(),
        (Acm::Vs, None) => "ACM VS".to_string(),
        _ => mode.label().to_string(),
    };
    txt(&format!("A/A  {sub}"), s.x + 10.0, s.y + s.h - 12.0, 16.0, CYAN_S);
    // Volumes ACM
    if acm != Acm::Off && lock.is_none() {
        let col = Color::new(0.3, 1.0, 0.45, 0.9);
        match acm {
            Acm::Bore => {
                if let (Some(p0), Some(p1)) = (v.project_dir(nose_dir(ac, 0.0, 0.0)), v.project_dir(nose_dir(ac, 0.0, 3.0))) {
                    draw_circle_lines(p0.x, p0.y, p0.distance(p1), 1.8, col);
                    txt_c("BORE 10NM", p0.x, p0.y + p0.distance(p1) + 14.0, 11.0, col);
                }
            }
            _ => {
                for az in [-2.0, 2.0] {
                    if let (Some(a), Some(b)) = (v.project_dir(nose_dir(ac, az, -2.0)), v.project_dir(nose_dir(ac, az, 40.0))) {
                        draw_line(a.x, a.y, b.x, b.y, 1.8, col);
                    }
                }
                if let Some(p0) = v.project_dir(nose_dir(ac, 0.0, -2.0)) {
                    txt_c("VS 10NM", p0.x, p0.y + 16.0, 11.0, col);
                }
            }
        }
    }
    for t in &list {
        let locked = Some(t.id) == lock;
        // Poursuite sur brouilleur : cadre barré sur la direction du brouilleur
        if locked && t.hoj {
            let (la, lo) = destination(ac.lat, ac.lon, wrap360(t.strobe_brg), 50.0 * NM);
            let alt = ac.alt + 50.0 * NM * t.strobe_el.to_radians().tan();
            if let Some(p) = v.project(eye.local(la, lo, alt)).filter(|p| s.contains(*p)) {
                draw_rectangle_lines(p.x - 16.0, p.y - 16.0, 32.0, 32.0, 2.0, AMBER);
                draw_line(p.x - 16.0, p.y - 16.0, p.x + 16.0, p.y + 16.0, 1.2, AMBER);
                draw_line(p.x + 16.0, p.y - 16.0, p.x - 16.0, p.y + 16.0, 1.2, AMBER);
                ticker(ac, t, p, 16.0, time);
            }
            continue;
        }
        if (!aa && !locked) || !Traffic::known(t) || (!locked && (!Traffic::confirmed(t) || Traffic::gmt(t))) {
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
                    // Cadre de désignation ; clignote si rapprochement rapide
                    let blink = r.closure_kt > 600.0 && (time * 4.0).fract() < 0.5;
                    let sz = 16.0;
                    if !blink {
                        draw_rectangle_lines(p.x - sz, p.y - sz, sz * 2.0, sz * 2.0, 2.0, AMBER);
                    }
                    ticker(ac, t, p, sz, time);
                } else {
                    draw_poly_lines(p.x, p.y, 4, 7.0, 45.0, 1.5, col);
                    let tag = if fresh { String::new() } else { format!(" {}", match Traffic::source(t) { "MEMOIRE" => "MEM", "LINK 16" => "L16", "ADS-B" => "ADS", x => x }) };
                    txt(&format!("{:.0}{tag}{}", r.range / NM, if t.notch { " NTCH" } else { "" }), p.x + 9.0, p.y + 4.0, 10.0, col);
                    if clicked(Rect::new(p.x - 10.0, p.y - 10.0, 20.0, 20.0)) {
                        CLICK.with(|x| x.set(Some(t.id)));
                    }
                }
            }
            _ => {
                if locked {
                    // Hors champ : ligne de repère depuis le centre, angle hors nez
                    let a = (r.az.to_radians() as f32, -(r.el.to_radians() as f32));
                    let d = vec2(a.0.sin(), a.1.sin()).normalize_or_zero();
                    let q = c + d * (s.h * 0.3);
                    draw_line(c.x + d.x * 40.0, c.y + d.y * 40.0, q.x, q.y, 2.0, AMBER);
                    draw_circle(q.x, q.y, 4.0, AMBER);
                    let ata = (r.az.to_radians().cos() * r.el.to_radians().cos()).clamp(-1.0, 1.0).acos().to_degrees();
                    txt_c(&format!("ATA {ata:.0}"), q.x, q.y + 16.0, 11.0, AMBER);
                    ticker(ac, t, q, 6.0, time);
                }
            }
        }
    }
    // Cible verrouillée : échelle de distance, interception, collision
    if let Some(t) = locked_t.filter(|t| !t.hoj) {
        let r = Traffic::rel(ac, t);
        let rn = r.range / NM;
        let scale = [5.0, 10.0, 20.0, 40.0, 80.0].into_iter().find(|k| rn * 1.1 < *k).unwrap_or(160.0);
        let (x, y0, y1) = (s.x + s.w * 0.74, c.y - 80.0, c.y + 80.0);
        draw_line(x, y0, x, y1, 1.5, GREEN_S);
        for k in 0..=4 {
            let y = y1 - (y1 - y0) * k as f32 / 4.0;
            draw_line(x, y, x + if k % 2 == 0 { 8.0 } else { 5.0 }, y, 1.2, GREEN_S);
        }
        txt(&format!("{scale:.0}"), x + 10.0, y0 + 4.0, 11.0, GREEN_S);
        let yc = y1 - (y1 - y0) * (rn / scale).min(1.0) as f32;
        draw_triangle(vec2(x - 2.0, yc), vec2(x - 11.0, yc - 6.0), vec2(x - 11.0, yc + 6.0), AMBER);
        let vc = if t.radar_age < MEM_S { t.vc_meas } else { r.closure_kt };
        txt_r(&format!("{vc:+.0}"), x - 14.0, yc + 4.0, 12.0, AMBER);
        txt_c("VC", x - 28.0, y1 + 16.0, 10.0, DIM);
        // Point d'interception (cap de collision) et temps
        if let Some((la, lo, al, tt)) = intercept(ac, t) {
            if let Some(p) = v.project(eye.local(la, lo, al)).filter(|p| s.contains(*p)) {
                draw_circle_lines(p.x, p.y, 6.0, 2.0, CYAN_S);
                draw_circle(p.x, p.y, 2.0, CYAN_S);
            }
            txt(&format!("TTI {:.0}:{:02.0}", (tt / 60.0).floor(), tt % 60.0), x + 10.0, y1 + 16.0, 12.0, CYAN_S);
        }
        // Distance visuelle et collision
        if rn < 6.0 {
            txt(if rn < 2.0 { "VISUEL" } else { "PORTEE VISUELLE" }, x + 10.0, y1 + 32.0, 11.0, WHITE);
        }
        if rn < 1.5 && r.dalt_ft.abs() < 1_000.0 && vc > 100.0 && (time * 3.0).fract() < 0.6 {
            let m = "COLLISION - BREAK";
            let w = txt_w(m, 18.0) + 16.0;
            draw_rectangle(c.x - w / 2.0, c.y + 40.0, w, 26.0, Color::new(0.5, 0.0, 0.0, 0.8));
            txt_c(m, c.x, c.y + 59.0, 18.0, WHITE);
        }
    }
}

/// Infos défilantes au-dessus du cadre : une ligne à la fois (2.2 s), fondu d'entrée, points de
/// progression ; « LOCK » rappelé à gauche.
fn ticker(ac: &Aircraft, t: &Target, p: Vec2, sz: f32, time: f64) {
    let items = lock_ticker(ac, t);
    if items.is_empty() {
        return;
    }
    let per = 2.2;
    let k = ((time / per) as usize) % items.len();
    let ph = ((time % per) / per) as f32;
    let a = (ph * 6.0).min(1.0).min((1.0 - ph) * 10.0).max(0.0);
    let (txt_s, col) = &items[k];
    let y = p.y - sz - 10.0;
    let w = txt_w(txt_s, 12.0) + 12.0;
    draw_rectangle(p.x - w / 2.0, y - 13.0, w, 17.0, Color::new(0.0, 0.0, 0.0, 0.45 * a));
    txt_c(txt_s, p.x, y, 12.0, Color::new(col.r, col.g, col.b, a));
    for i in 0..items.len() {
        let dx = (i as f32 - (items.len() as f32 - 1.0) / 2.0) * 7.0;
        let on = i == k;
        draw_circle(p.x + dx, y + 8.0, if on { 2.2 } else { 1.3 }, if on { AMBER } else { Color::new(1.0, 0.72, 0.0, 0.4) });
    }
    txt("LOCK", p.x - sz - 2.0, p.y + sz + 13.0, 10.0, AMBER);
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
    let etat = if !Traffic::confirmed(t) && t.radar_age < MEM_S { "PLOT NON CONFIRME (1 DETECTION)".to_string() } else if Traffic::gmt(t) { "MOBILE AU SOL ? (GMT)".to_string() } else if t.multipath { format!("MULTITRAJET  F {:.2}{}", t.mp_f, if t.mp_f < 0.7 { " EVANOUIS." } else { "" }) } else if let Some(m) = t.masked { m.to_string() } else if t.strobe { "BROUILLAGE - ANGLE SEUL".into() } else if t.jam { "BROUILLEUR ACTIF (BURN-THROUGH)".into() } else if t.radar_age < 1.0 { format!("VR {:+.0} KT{}", t.vr_kt, if t.notch { "  NOTCH" } else { "" }) } else { "--".into() };
    lines.push(("RADAR".into(), etat, if t.masked.is_some() || t.strobe || t.jam || t.notch { AMBER } else { WHITE }));
    lines
}

thread_local! {
    /// Mode ACM courant, avions vus (type, lat, lon, alt, cap, pente, inclinaison, militaire)
    static ACM: std::cell::Cell<Acm> = const { std::cell::Cell::new(Acm::Off) };
    static VIS: std::cell::RefCell<Vec<(&'static str, f64, f64, f64, f64, f64, f64, bool)>> = const { std::cell::RefCell::new(Vec::new()) };
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

    /// Multitrajet : facteur de propagation entre 1 - rho et 1 + rho, lobes plus espacés à
    /// basse altitude ; cible basse vue de loin : altitude radar entachée, portée modulée.
    #[test]
    fn multipath_lobing() {
        let (mut fmin, mut fmax) = (9.0f64, 0.0f64);
        for k in 0..2000 {
            let f = multipath_f(150.0, 60.0, 20_000.0 + k as f64 * 2.0, true);
            fmin = fmin.min(f);
            fmax = fmax.max(f);
        }
        println!("mer : F de {fmin:.2} a {fmax:.2}");
        assert!(fmin < 0.15 && fmax > 1.85);
        let mut ac = own();
        ac.alt = 500.0;
        let mut tr = Traffic::new();
        tr.next_id = 100;
        let mut t = tgt(&ac, "F16", 12.0, 0.0, 200.0, wrap360(ac.hdg_deg() + 180.0));
        t.radar_age = 0.0;
        tr.list.push(t);
        let mut fades = 0;
        let mut n = 0;
        tr.auto_spawn = false;
        tr.false_alarms = false;
        for _ in 0..300 {
            tr.update(&ac, 0.1);
            if tr.list[0].multipath {
                n += 1;
                if tr.list[0].mp_f < 0.7 {
                    fades += 1;
                }
            }
        }
        println!("cible a 200 ft : multitrajet {n}/300, evanouissements {fades}");
        assert!(n > 250 && fades > 10);
    }

    /// Échos de sol : une piste au ras du sol à vitesse de véhicule est classée GMT et LOCK auto
    /// l'ignore.
    #[test]
    fn ground_mover_rejected() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        let mut t = tgt(&ac, "----", 15.0, 0.0, 100.0, wrap360(ac.hdg_deg() + 180.0));
        (t.mil, t.iff, t.clutter, t.life, t.tas, t.gnd, t.radar_age, t.det_n) = (false, Iff::NonId, true, 100.0, 35.0, 0.0, 0.0, 3);
        (t.tr_alt, t.tr_tas, t.vel_ok) = (30.0, 35.0, true);
        t.alt = 30.0;
        tr.list.push(t);
        assert!(Traffic::gmt(&tr.list[0]));
        assert!(tr.lock_next(&ac).is_none());
    }

    /// ACM BORE : cible dans le cercle de 3° à moins de 10 NM verrouillée automatiquement.
    #[test]
    fn acm_bore_autolock() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        tr.auto_spawn = false;
        tr.false_alarms = false;
        let pitch = ac.pitch_att().to_degrees();
        let d = 6.0 * NM;
        let alt = ac.alt + d * pitch.to_radians().tan();
        let mut t = tgt(&ac, "F16", 6.0, 0.5, alt / FT, wrap360(ac.hdg_deg() + 180.0));
        t.radar_age = 0.0;
        tr.list.push(t);
        tr.acm = Acm::Bore;
        tr.update(&ac, 0.1);
        println!("{:?}", tr.events);
        assert!(tr.lock == Some(1) && tr.acm == Acm::Off);
    }

    /// RWR : un hostile qui nous a dans son cône avant à 20 NM nous accroche et vient vers nous.
    #[test]
    fn hostile_radar_spikes_rwr() {
        let ac = own();
        let mut tr = Traffic::new();
        tr.next_id = 100;
        tr.auto_spawn = false;
        tr.false_alarms = false;
        let mut t = tgt(&ac, "SU27", 20.0, 30.0, 28_000.0, wrap360(ac.hdg_deg() + 30.0 + 180.0 + 20.0));
        t.hostile = true;
        tr.list.push(t);
        tr.update(&ac, 0.1);
        let cues = tr.threat_cues(&ac);
        println!("{:?}", tr.events);
        assert!(cues.len() == 1 && cues[0].state == 3);
        for _ in 0..100 {
            tr.update(&ac, 0.1);
        }
        let (_, b) = dist_brg(tr.list[0].lat, tr.list[0].lon, ac.lat, ac.lon);
        assert!(wrap180(b - tr.list[0].hdg).abs() < 10.0, "poursuite vers nous");
    }

    /// Interception : point de collision et temps cohérents ; infos défilantes non vides.
    #[test]
    fn intercept_and_ticker() {
        let ac = own();
        let mut t = tgt(&ac, "B77W", 30.0, 0.0, 30_000.0, wrap360(ac.hdg_deg() + 180.0));
        t.vel_ok = true;
        let (la, lo, _, tt) = intercept(&ac, &t).expect("interception");
        let closing = ac.tas + t.tas;
        println!("TTI {tt:.0} s (attendu {:.0})", 30.0 * NM / closing);
        assert!((tt - 30.0 * NM / closing).abs() < 10.0);
        assert!(dist_brg(ac.lat, ac.lon, la, lo).0 > 0.0);
        assert!(lock_ticker(&ac, &t).len() >= 5);
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
        let step = |ac: &mut Aircraft, tr: &mut Traffic, secs: f64, rate_deg: f64| {
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

// ---------- Modèles 3D du trafic (dimensions réelles) ----------

#[derive(Clone, Copy)]
enum Tail {
    /// Dérive unique et empennage horizontal bas
    Conv,
    /// Empennage en T
    T,
    /// Deux dérives (inclinaison en degrés)
    Twin(f32),
    /// Empennage en V
    V,
}

/// Silhouette paramétrique : longueur, rayon de fuselage, envergure, cordes emplanture /
/// saumon, flèche du bord d'attaque, position de l'aile (fraction depuis le nez), hauteur d'aile
/// (fraction du rayon), dièdre, empennage (hauteur de dérive, envergure de profondeur), moteurs
/// (x depuis le nez en fraction, y, z, longueur, rayon), bosse (747), radôme (AWACS).
struct Shape {
    len: f32,
    r: f32,
    span: f32,
    c_root: f32,
    c_tip: f32,
    sweep: f32,
    wing_x: f32,
    wing_z: f32,
    dihedral: f32,
    tail: Tail,
    fin_h: f32,
    ht_span: f32,
    engines: &'static [(f32, f32, f32, f32, f32)],
    hump: bool,
    dome: bool,
    paint: [[u8; 4]; 3],
}

const AIRLINER: [[u8; 4]; 3] = [[236, 238, 240, 255], [200, 204, 210, 255], [150, 156, 164, 255]];
const MIL_GREY: [[u8; 4]; 3] = [[150, 156, 164, 255], [128, 134, 142, 255], [104, 110, 118, 255]];
const DARK: [[u8; 4]; 3] = [[96, 100, 108, 255], [80, 84, 92, 255], [64, 68, 74, 255]];

fn shape_of(icao: &str) -> Shape {
    let s = |len, r, span, c_root, c_tip, sweep, wing_x, wing_z, dihedral, tail, fin_h, ht_span, engines, paint| Shape {
        len, r, span, c_root, c_tip, sweep, wing_x, wing_z, dihedral, tail, fin_h, ht_span, engines, hump: false, dome: false, paint,
    };
    const TWIN_SMALL: [(f32, f32, f32, f32, f32); 2] = [(0.36, -5.8, -1.6, 4.4, 1.0), (0.36, 5.8, -1.6, 4.4, 1.0)];
    const TWIN_BIG: [(f32, f32, f32, f32, f32); 2] = [(0.33, -9.7, -2.4, 7.3, 1.75), (0.33, 9.7, -2.4, 7.3, 1.75)];
    const QUAD: [(f32, f32, f32, f32, f32); 4] = [(0.36, -11.8, -2.4, 5.5, 1.3), (0.36, 11.8, -2.4, 5.5, 1.3), (0.30, -21.0, -1.4, 5.5, 1.3), (0.30, 21.0, -1.4, 5.5, 1.3)];
    const QUAD_MIL: [(f32, f32, f32, f32, f32); 4] = [(0.37, -8.3, -2.0, 4.6, 0.95), (0.37, 8.3, -2.0, 4.6, 0.95), (0.31, -13.8, -1.2, 4.6, 0.95), (0.31, 13.8, -1.2, 4.6, 0.95)];
    const C17_ENG: [(f32, f32, f32, f32, f32); 4] = [(0.40, -7.6, 0.4, 5.0, 1.15), (0.40, 7.6, 0.4, 5.0, 1.15), (0.36, -13.6, 0.8, 5.0, 1.15), (0.36, 13.6, 0.8, 5.0, 1.15)];
    const BIZ_ENG: [(f32, f32, f32, f32, f32); 2] = [(0.72, -1.6, 0.6, 2.8, 0.5), (0.72, 1.6, 0.6, 2.8, 0.5)];
    const TOP_ENG: [(f32, f32, f32, f32, f32); 1] = [(0.62, 0.0, 1.0, 3.2, 0.6)];
    const NONE: [(f32, f32, f32, f32, f32); 0] = [];
    match icao {
        "A320" => s(37.6, 1.98, 35.8, 6.1, 1.5, 27.0, 0.36, -0.6, 5.0, Tail::Conv, 6.0, 12.5, &TWIN_SMALL, AIRLINER),
        "B738" => s(39.5, 1.88, 35.8, 6.0, 1.4, 27.0, 0.37, -0.6, 6.0, Tail::Conv, 6.2, 14.4, &TWIN_SMALL, AIRLINER),
        "E175" => s(31.7, 1.5, 26.0, 4.8, 1.2, 25.0, 0.36, -0.6, 5.0, Tail::Conv, 5.0, 10.0, &[(0.36, -4.6, -1.2, 3.6, 0.8), (0.36, 4.6, -1.2, 3.6, 0.8)], AIRLINER),
        "B77W" => s(73.9, 3.1, 64.8, 13.0, 2.6, 33.0, 0.34, -0.6, 6.0, Tail::Conv, 10.0, 21.5, &TWIN_BIG, AIRLINER),
        "A359" => s(66.8, 2.98, 64.75, 12.5, 2.4, 33.0, 0.34, -0.6, 5.0, Tail::Conv, 9.5, 19.0, &TWIN_BIG, AIRLINER),
        "B789" => s(62.8, 2.88, 60.1, 11.5, 2.2, 32.0, 0.35, -0.6, 6.0, Tail::Conv, 9.0, 19.5, &TWIN_BIG, AIRLINER),
        "B748" => Shape { hump: true, ..s(76.3, 3.25, 68.4, 14.0, 3.0, 40.0, 0.33, -0.6, 7.0, Tail::Conv, 11.0, 22.0, &QUAD, AIRLINER) },
        "C56X" => s(15.8, 0.85, 17.2, 2.6, 1.0, 15.0, 0.42, -0.7, 4.0, Tail::T, 3.2, 6.5, &BIZ_ENG, AIRLINER),
        "K35R" => s(41.5, 1.85, 39.9, 7.5, 2.0, 35.0, 0.36, -0.6, 7.0, Tail::Conv, 8.0, 13.6, &QUAD_MIL, MIL_GREY),
        "E3TF" => Shape { dome: true, ..s(46.6, 1.88, 44.4, 8.5, 2.2, 35.0, 0.36, -0.6, 7.0, Tail::Conv, 8.0, 14.0, &QUAD_MIL, MIL_GREY) },
        "C17" => s(53.0, 3.4, 51.8, 9.0, 3.0, 25.0, 0.38, 0.95, -3.0, Tail::T, 9.0, 19.8, &C17_ENG, MIL_GREY),
        "U2" => s(19.2, 0.75, 31.4, 3.2, 1.2, 5.0, 0.42, 0.0, 0.0, Tail::Conv, 3.4, 9.0, &NONE, DARK),
        "Q4" => s(14.5, 0.75, 39.9, 2.0, 0.7, 5.0, 0.44, 0.5, 2.0, Tail::V, 2.8, 6.0, &TOP_ENG, MIL_GREY),
        "F16" => s(15.0, 0.75, 9.45, 5.0, 1.0, 40.0, 0.42, -0.2, 0.0, Tail::Conv, 3.4, 5.6, &NONE, MIL_GREY),
        "F35" => s(15.7, 0.95, 10.7, 6.5, 1.6, 34.0, 0.44, 0.0, 0.0, Tail::Twin(25.0), 2.6, 6.6, &NONE, DARK),
        "F22" => s(18.9, 1.0, 13.6, 8.5, 1.4, 42.0, 0.42, 0.0, 0.0, Tail::Twin(28.0), 3.0, 8.8, &NONE, DARK),
        "FA18" => s(18.3, 0.95, 13.6, 5.6, 1.6, 26.0, 0.44, 0.0, 0.0, Tail::Twin(20.0), 3.0, 6.6, &NONE, MIL_GREY),
        _ => s(21.9, 1.0, 14.7, 7.0, 1.4, 42.0, 0.43, 0.0, 0.0, Tail::Twin(0.0), 3.6, 9.9, &NONE, [[130, 142, 150, 255], [112, 124, 132, 255], [92, 102, 110, 255]]),
    }
}

/// Rendu 3D du trafic à portée visuelle (< 25 km) : modèles aux dimensions réelles orientés
/// au cap, à la pente et à l'inclinaison.
pub fn draw_3d(ac: &Aircraft, mb: &mut crate::svs::MeshBuilder) {
    let vis = VIS.with(|v| v.borrow().clone());
    if vis.is_empty() {
        return;
    }
    let eye = crate::terrain::EyeFrame::new(ac);
    for (icao, lat, lon, alt, hdg, pitch, bank, _mil) in vis {
        let (d, _) = dist_brg(ac.lat, ac.lon, lat, lon);
        if d > 25_000.0 || (alt - ac.alt).abs() > 25_000.0 {
            continue;
        }
        let sh = shape_of(icao);
        let h = hdg.to_radians();
        let (sp, cp) = (pitch.sin() as f32, pitch.cos() as f32);
        let (shh, chh) = (h.sin() as f32, h.cos() as f32);
        let fwd = vec3(shh * cp, sp, -chh * cp);
        let right0 = vec3(chh, 0.0, shh);
        let up0 = vec3(-shh * sp, cp, chh * sp);
        let (sb, cb) = (bank.sin() as f32, bank.cos() as f32);
        let right = right0 * cb - up0 * sb;
        let up = up0 * cb + right0 * sb;
        let base = eye.local(lat, lon, alt);
        // x : depuis le nez vers l'arrière (m) ; y : à droite ; z : vers le haut
        let l = sh.len;
        let p = |x: f32, y: f32, z: f32| base + fwd * (l * 0.5 - x) + right * y + up * z;
        build(&sh, &p, mb);
    }
}

fn build(sh: &Shape, p: &dyn Fn(f32, f32, f32) -> Vec3, mb: &mut crate::svs::MeshBuilder) {
    let [top, side, under] = sh.paint;
    let (l, r) = (sh.len, sh.r);
    // Fuselage : octogone, nez effilé (12 % de la longueur), cône arrière relevé (22 %)
    let ring = |x: f32, rr: f32, zc: f32| -> [Vec3; 8] {
        std::array::from_fn(|k| {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            p(x, a.cos() * rr, zc + a.sin() * rr)
        })
    };
    let st = [(0.0, 0.05, -0.1), (0.04, 0.55, -0.05), (0.12, 1.0, 0.0), (0.78, 1.0, 0.0), (0.94, 0.45, 0.35), (1.0, 0.12, 0.6)];
    let rings: Vec<[Vec3; 8]> = st.iter().map(|&(f, k, zc)| ring(f * l, k * r, zc * r)).collect();
    for w in rings.windows(2) {
        for k in 0..8 {
            let k1 = (k + 1) % 8;
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            let c = if a.sin() > 0.3 { top } else if a.sin() < -0.3 { under } else { side };
            mb.quad([w[0][k], w[1][k], w[1][k1], w[0][k1]], c);
        }
    }
    // Bosse du pont supérieur (747) : demi-fuselage sur le premier tiers
    if sh.hump {
        let hr = r * 0.6;
        let hz = r * 0.75;
        let h = [ring(0.08 * l, hr * 0.4, hz), ring(0.14 * l, hr, hz), ring(0.30 * l, hr, hz), ring(0.36 * l, hr * 0.3, hz)];
        for w in h.windows(2) {
            for k in 0..4 {
                mb.quad([w[0][k], w[1][k], w[1][k + 1], w[0][k + 1]], top);
            }
        }
    }
    // Verrière / pare-brise
    let glass = [40, 46, 58, 255];
    mb.quad([p(0.06 * l, -r * 0.45, r * 0.55), p(0.10 * l, -r * 0.6, r * 0.75), p(0.10 * l, r * 0.6, r * 0.75), p(0.06 * l, r * 0.45, r * 0.55)], glass);
    // Ailes (deux faces), flèche du bord d'attaque, dièdre
    let half = sh.span * 0.5;
    let xr = sh.wing_x * l;
    let zw = sh.wing_z * r;
    let tan_sw = sh.sweep.to_radians().tan();
    let dz = half * sh.dihedral.to_radians().tan();
    for sg in [-1.0f32, 1.0] {
        let le_tip = xr + (half - r) * tan_sw;
        // Épaisseur de profil (12 % de la corde à l'emplanture) : faces séparées, pas de scintillement
        let th = sh.c_root * 0.06;
        let q = [p(xr, sg * r, zw + th), p(le_tip, sg * half, zw + dz + th * 0.3), p(le_tip + sh.c_tip, sg * half, zw + dz + th * 0.3), p(xr + sh.c_root, sg * r, zw + th)];
        mb.quad(q, top);
        let u = [p(xr, sg * r, zw - th), p(le_tip, sg * half, zw + dz - th * 0.3), p(le_tip + sh.c_tip, sg * half, zw + dz - th * 0.3), p(xr + sh.c_root, sg * r, zw - th)];
        mb.quad([u[3], u[2], u[1], u[0]], under);
        // Bord d'attaque
        mb.quad([q[0], q[1], u[1], u[0]], side);
    }
    // Empennages
    let x_t = l * 0.80;
    let c_fin = (l * 0.13).max(1.5);
    let fin = |y: f32, z0: f32, h: f32, cant: f32, mb: &mut crate::svs::MeshBuilder| {
        let (cy, cz) = (cant.to_radians().sin(), cant.to_radians().cos());
        let tip_x = x_t + h * 0.9;
        let e = c_fin * 0.04;
        for o in [-e, e] {
            let oy = o * cz;
            let oz = -o * cy;
            let q = [p(x_t, y + oy, z0 + oz), p(tip_x, y + cy * h + oy, z0 + cz * h + oz), p(tip_x + c_fin * 0.5, y + cy * h + oy, z0 + cz * h + oz), p(x_t + c_fin, y + oy, z0 + oz)];
            if o < 0.0 { mb.quad(q, side) } else { mb.quad([q[3], q[2], q[1], q[0]], side) }
        }
    };
    let ht = |z: f32, mb: &mut crate::svs::MeshBuilder| {
        for sg in [-1.0f32, 1.0] {
            let hs = sh.ht_span * 0.5;
            let e = c_fin * 0.03;
            let q = [p(x_t + c_fin * 0.2, sg * r * 0.4, z + e), p(x_t + c_fin * 0.2 + hs * 0.6, sg * hs, z + e), p(x_t + c_fin * 0.2 + hs * 0.6 + c_fin * 0.35, sg * hs, z + e), p(l * 0.99, sg * r * 0.3, z + e)];
            mb.quad(q, top);
            let u = [p(x_t + c_fin * 0.2, sg * r * 0.4, z - e), p(x_t + c_fin * 0.2 + hs * 0.6, sg * hs, z - e), p(x_t + c_fin * 0.2 + hs * 0.6 + c_fin * 0.35, sg * hs, z - e), p(l * 0.99, sg * r * 0.3, z - e)];
            mb.quad([u[3], u[2], u[1], u[0]], under);
        }
    };
    match sh.tail {
        Tail::Conv => {
            fin(0.0, r * 0.5, sh.fin_h, 0.0, mb);
            ht(r * 0.2, mb);
        }
        Tail::T => {
            fin(0.0, r * 0.5, sh.fin_h, 0.0, mb);
            ht(r * 0.5 + sh.fin_h, mb);
        }
        Tail::Twin(cant) => {
            fin(-r * 0.9, r * 0.3, sh.fin_h, -cant, mb);
            fin(r * 0.9, r * 0.3, sh.fin_h, cant, mb);
            ht(0.0, mb);
        }
        Tail::V => {
            fin(-r * 0.3, r * 0.5, sh.fin_h, -50.0, mb);
            fin(r * 0.3, r * 0.5, sh.fin_h, 50.0, mb);
        }
    }
    // Moteurs : nacelles octogonales
    let nac = [70, 74, 80, 255];
    for &(fx, y, z, el, er) in sh.engines {
        let x0 = fx * l;
        let a: [Vec3; 8] = std::array::from_fn(|k| {
            let t = k as f32 * std::f32::consts::TAU / 8.0;
            p(x0, y + t.cos() * er, z + t.sin() * er)
        });
        let b: [Vec3; 8] = std::array::from_fn(|k| {
            let t = k as f32 * std::f32::consts::TAU / 8.0;
            p(x0 + el, y + t.cos() * er * 0.8, z + t.sin() * er * 0.8)
        });
        for k in 0..8 {
            let k1 = (k + 1) % 8;
            mb.quad([a[k], b[k], b[k1], a[k1]], if k < 4 { side } else { nac });
        }
        mb.quad([a[0], a[2], a[4], a[6]], [24, 24, 28, 255]);
        // Mât vers l'aile (ou le fuselage pour les moteurs arrière / dorsaux)
        let zw = sh.wing_z * r;
        let (zt, zb) = if z < zw { (zw, z + er * 0.8) } else { (z - er * 0.8, zw.max(r * 0.6)) };
        let px0 = x0 + el * 0.25;
        mb.quad([p(px0, y, zb), p(px0 + el * 0.6, y, zb), p(px0 + el * 0.7, y, zt), p(px0 - el * 0.1, y, zt)], side);
    }
    // Radôme de l'AWACS (diamètre 9.1 m) sur mâts
    if sh.dome {
        let (xc, zc, rd) = (l * 0.62, r + 3.0, 4.55);
        let pts: Vec<Vec3> = (0..16).map(|k| {
            let t = k as f32 * std::f32::consts::TAU / 16.0;
            p(xc + t.cos() * rd, t.sin() * rd, zc)
        }).collect();
        let cen = p(xc, 0.0, zc + 0.6);
        for k in 0..16 {
            let k1 = (k + 1) % 16;
            mb.quad([cen, pts[k], pts[k1], cen], if k % 2 == 0 { [60, 62, 68, 255] } else { [210, 212, 214, 255] });
        }
        mb.quad([p(xc - 1.0, -0.3, r * 0.9), p(xc - 1.0, -0.3, zc), p(xc + 1.0, -0.3, zc), p(xc + 1.0, -0.3, r * 0.9)], side);
    }
}
