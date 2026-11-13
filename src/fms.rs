//! FMS du Darkstar (« DK-FMS ») : plan de vol, import SimBrief, génération d'un plan hypersonique
//! (« DARKSTAR DISPATCH ») et MCDU complet.
//!
//! SimBrief : l'OFP le plus récent de l'utilisateur est téléchargé (API publique
//! `xml.fetcher.php?username=…&json=1`) ; la route (navlog) est reprise point par point, puis le
//! profil vertical est recalculé pour le Darkstar (turbines, stato, scram). La génération d'un OFP
//! SimBrief se fait dans le navigateur (page de dispatch pré-remplie), puis IMPORT OFP.
//!
//! Guidage latéral (mode NAV) : poursuite du point actif, séquencement avec anticipation du virage
//! (rayon de virage énorme en hypersonique), DIRECT TO.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::ui::*;
use macroquad::prelude::*;
use std::sync::{Arc, Mutex};

// ---------- JSON minimal ----------

#[derive(Clone, Debug)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(a, _)| a == k).map(|(_, b)| b),
            _ => None,
        }
    }
    pub fn path(&self, p: &[&str]) -> Option<&Json> {
        p.iter().try_fold(self, |j, k| j.get(k))
    }
    pub fn s(&self) -> String {
        match self {
            Json::Str(s) => s.clone(),
            Json::Num(n) => format!("{n}"),
            Json::Bool(b) => format!("{b}"),
            _ => String::new(),
        }
    }
    pub fn f(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    /// Tableau, ou objet unique vu comme un tableau d'un élément (XML converti par SimBrief).
    pub fn items(&self) -> Vec<&Json> {
        match self {
            Json::Arr(v) => v.iter().collect(),
            Json::Null => Vec::new(),
            j => vec![j],
        }
    }
}

pub fn parse_json(s: &str) -> Result<Json, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let v = value(b, &mut i)?;
    Ok(v)
}

fn ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && (b[*i] as char).is_whitespace() {
        *i += 1;
    }
}

fn value(b: &[u8], i: &mut usize) -> Result<Json, String> {
    ws(b, i);
    if *i >= b.len() {
        return Err("JSON tronque".into());
    }
    match b[*i] {
        b'{' => {
            *i += 1;
            let mut v = Vec::new();
            loop {
                ws(b, i);
                if *i < b.len() && b[*i] == b'}' {
                    *i += 1;
                    break;
                }
                let k = match value(b, i)? {
                    Json::Str(s) => s,
                    _ => return Err("cle JSON invalide".into()),
                };
                ws(b, i);
                if *i >= b.len() || b[*i] != b':' {
                    return Err("':' attendu".into());
                }
                *i += 1;
                let x = value(b, i)?;
                v.push((k, x));
                ws(b, i);
                if *i < b.len() && b[*i] == b',' {
                    *i += 1;
                }
            }
            Ok(Json::Obj(v))
        }
        b'[' => {
            *i += 1;
            let mut v = Vec::new();
            loop {
                ws(b, i);
                if *i < b.len() && b[*i] == b']' {
                    *i += 1;
                    break;
                }
                v.push(value(b, i)?);
                ws(b, i);
                if *i < b.len() && b[*i] == b',' {
                    *i += 1;
                }
            }
            Ok(Json::Arr(v))
        }
        b'"' => {
            *i += 1;
            let mut out = Vec::new();
            while *i < b.len() && b[*i] != b'"' {
                if b[*i] == b'\\' && *i + 1 < b.len() {
                    *i += 1;
                    match b[*i] {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            // \uXXXX : caractère non ASCII remplacé
                            *i += 4;
                            out.push(b'?');
                        }
                        c => out.push(c),
                    }
                } else {
                    out.push(b[*i]);
                }
                *i += 1;
            }
            *i += 1;
            Ok(Json::Str(String::from_utf8_lossy(&out).into_owned()))
        }
        b't' => {
            *i += 4;
            Ok(Json::Bool(true))
        }
        b'f' => {
            *i += 5;
            Ok(Json::Bool(false))
        }
        b'n' => {
            *i += 4;
            Ok(Json::Null)
        }
        _ => {
            let st = *i;
            while *i < b.len() && matches!(b[*i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                *i += 1;
            }
            std::str::from_utf8(&b[st..*i]).ok().and_then(|s| s.parse().ok()).map(Json::Num).ok_or_else(|| "nombre JSON invalide".into())
        }
    }
}

// ---------- Plan de vol ----------

#[derive(Clone, Debug, Default)]
pub struct Wpt {
    pub ident: String,
    pub lat: f64,
    pub lon: f64,
    /// Contrainte / prévision d'altitude (ft) et de Mach
    pub alt_ft: f64,
    pub mach: f64,
    /// Phase du profil Darkstar au point
    pub phase: &'static str,
    /// Voie / procédure (airway, SID, STAR) menant au point
    pub via: String,
    /// 0 en route, 1 SID, 2 STAR
    pub proc: u8,
    /// Altitude prévue par SimBrief au point (ft, 0 si inconnue) : contrainte des procédures
    pub sb_alt: f64,
}

#[derive(Clone, Default)]
pub struct Fpln {
    pub origin: String,
    pub dest: String,
    pub wpts: Vec<Wpt>,
    pub active: usize,
    pub source: &'static str,
    pub flt: String,
    pub crz_fl: f64,
    pub crz_mach: f64,
    /// Besoin carburant prévu (kg) et durée (s)
    pub jp7_req: f64,
    pub h2_req: f64,
    pub ete: f64,
    /// IFR : pistes de départ / d'arrivée et procédures
    pub dep_rwy: String,
    pub arr_rwy: String,
    pub sid: String,
    pub star: String,
    /// Croisière au plafond (FL MAX) : le niveau monte avec le Mach (couloir de q)
    pub crz_max: bool,
}

/// Altitude (ft) où la pression dynamique vaut `q` (Pa) à Mach `m` (atmosphère standard).
pub fn alt_for_q(m: f64, q: f64) -> f64 {
    let (mut lo, mut hi) = (5_000.0f64, 60_000.0f64);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        let a = crate::atmosphere::isa(mid);
        let qm = 0.5 * a.rho * (m * a.a).powi(2);
        if qm > q {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0 / FT
}

/// Plafond de croisière scramjet (FL) à Mach `m` : q = 35 kPa (marge au-dessus du minimum de
/// 25 kPa du couloir scram).
pub fn ceiling_fl(m: f64) -> f64 {
    (alt_for_q(m, 35_000.0) / 100.0 / 10.0).floor() * 10.0
}

/// JP-7 d'une remise des gaz avec nouveau circuit (turbines, ~10 min) et de l'approche finale (kg).
pub const GA_JP7: f64 = 1_800.0;
pub const APPR_JP7: f64 = 1_500.0;

/// Débits de croisière scram mesurés en vol simulé (q = 35 kPa) : (Mach, H2 kg/s, JP-7 kg/s —
/// le stato brûle encore du JP-7 sous Mach 7).
const CRZ_RATES: [(f64, f64, f64); 7] = [(5.0, 2.32, 6.27), (6.0, 3.32, 3.59), (7.0, 4.78, 0.0), (8.0, 4.46, 0.0), (9.0, 4.20, 0.0), (10.0, 4.00, 0.0), (11.0, 3.60, 0.0)];

fn interp(m: f64, k: usize) -> f64 {
    let t = &CRZ_RATES;
    let m = m.clamp(t[0].0, t[t.len() - 1].0);
    for w in t.windows(2) {
        if m <= w[1].0 {
            let f = (m - w[0].0) / (w[1].0 - w[0].0);
            let (a, b) = if k == 1 { (w[0].1, w[1].1) } else { (w[0].2, w[1].2) };
            return a + (b - a) * f;
        }
    }
    if k == 1 { t[t.len() - 1].1 } else { t[t.len() - 1].2 }
}

/// Débit d'hydrogène en croisière scram à Mach `m` (kg/s).
pub fn h2_rate(m: f64) -> f64 {
    interp(m, 1)
}

/// Débit de JP-7 en croisière scram à Mach `m` (kg/s).
pub fn jp7_rate_scram(m: f64) -> f64 {
    interp(m, 2)
}

/// Allongement de la descente au-delà de Mach 6 (énergie cinétique, décélération lente à q constant).
fn hyper_k(m: f64) -> f64 {
    (m / 6.0).max(1.0)
}

/// Distance de descente (m) : descente à q constant (~(1 + M/2) NM par 1000 ft, mesuré en vol
/// simulé) puis descente finale subsonique (~80 NM).
pub fn tod_dist(crz_fl: f64, crz_mach: f64) -> f64 {
    ((crz_fl / 10.0) * (1.0 + 0.5 * crz_mach) * hyper_k(crz_mach) + 80.0) * NM
}

/// Profil hypersonique du Darkstar : (phase, Mach, FL) en fonction de la distance parcourue et
/// restante (m), pour une croisière à `crz_fl` / `crz_mach`.
fn profile(d_from: f64, d_to: f64, crz_fl: f64, crz_mach: f64) -> (&'static str, f64, f64) {
    let nm = |x: f64| x / NM;
    // Descente à q constant : ~ 25 NM par 1000 ft de dénivelé en hypersonique (plus à Mach élevé)
    let tod = tod_dist(crz_fl, crz_mach);
    if nm(d_from) < 40.0 {
        ("CLB TURB", 0.9_f64.min(crz_mach), 350.0_f64.min(crz_fl))
    } else if nm(d_from) < 160.0 && crz_mach > 2.5 {
        ("ACC RAM", 2.6, 450.0_f64.min(crz_fl))
    } else if nm(d_from) < 420.0 && crz_mach > 4.8 {
        ("CLB RAM", 4.8, 700.0_f64.min(crz_fl))
    } else if d_to < tod {
        let k = (d_to / tod).clamp(0.0, 1.0);
        ("DES", (crz_mach * k).max(0.8), (crz_fl * k).max(100.0))
    } else {
        (if crz_mach > 4.8 { "CRZ SCRAM" } else if crz_mach > 2.5 { "CRZ RAM" } else { "CRZ TURB" }, crz_mach, crz_fl)
    }
}

impl Fpln {
    /// Distance totale restante depuis l'avion (m) par les points restants.
    pub fn remaining(&self, lat: f64, lon: f64) -> f64 {
        let mut d = 0.0;
        let (mut pl, mut po) = (lat, lon);
        for w in &self.wpts[self.active.min(self.wpts.len())..] {
            d += dist_brg(pl, po, w.lat, w.lon).0;
            pl = w.lat;
            po = w.lon;
        }
        d
    }

    pub fn total(&self) -> f64 {
        self.wpts.windows(2).map(|w| dist_brg(w[0].lat, w[0].lon, w[1].lat, w[1].lon).0).sum()
    }

    /// Recalcule le profil vertical Darkstar et les prévisions (temps, carburant).
    pub fn compute(&mut self) {
        let tot = self.total().max(1.0);
        let mut acc = 0.0;
        let mut t = 0.0;
        let (mut jp, mut h2) = (0.0, 0.0);
        let n = self.wpts.len();
        for i in 0..n {
            if i > 0 {
                let seg = dist_brg(self.wpts[i - 1].lat, self.wpts[i - 1].lon, self.wpts[i].lat, self.wpts[i].lon).0;
                let m = self.wpts[i - 1].mach.max(0.6);
                let v = m * 295.0;
                let dt = seg / v;
                t += dt;
                // Consommations moyennes par phase (kg/s) : turbines, stato, scram (H2)
                match self.wpts[i - 1].phase {
                    "CLB TURB" | "CRZ TURB" => jp += 3.2 * dt,
                    "ACC RAM" | "CLB RAM" | "CRZ RAM" => jp += 5.5 * dt,
                    "CRZ SCRAM" => {
                        h2 += h2_rate(m) * dt;
                        jp += jp7_rate_scram(m) * dt;
                    }
                    _ => jp += 0.6 * dt,
                }
                acc += seg;
            }
            let (ph, m, fl) = profile(acc, tot - acc, self.crz_fl, self.crz_mach);
            let w = &mut self.wpts[i];
            if i == 0 || i == n - 1 {
                w.phase = if i == 0 { "DEP" } else { "ARR" };
                w.mach = if i == 0 { 0.3 } else { 0.25 };
                w.alt_ft = AIRPORTS.iter().find(|a| a.icao == w.ident).map(|a| a.elev / FT).unwrap_or(0.0);
            } else if w.proc == 1 {
                w.phase = "SID";
                w.mach = 0.6;
                w.alt_ft = if w.sb_alt > 0.0 { w.sb_alt.min(fl * 100.0) } else { fl * 100.0 };
            } else if w.proc >= 2 {
                w.phase = if w.proc == 3 { "APPR" } else { "STAR" };
                w.mach = 0.7;
                w.alt_ft = if w.sb_alt > 0.0 { w.sb_alt } else { fl * 100.0 };
            } else {
                w.phase = ph;
                w.mach = m;
                w.alt_ft = fl * 100.0;
            }
        }
        self.ete = t;
        // Réserves : 15 % + 3 t JP-7 (approche, dégagement), 10 % H2
        // Réserve finale : approche + deux remises des gaz complètes (circuit aux turbines)
        self.jp7_req = jp * 1.10 + APPR_JP7 + 2.0 * GA_JP7;
        self.h2_req = h2 * 1.10;
    }

    /// Point de début de descente (distance à destination, m).
    pub fn tod(&self) -> f64 {
        // Avec une STAR : la descente hypersonique doit finir à son entrée (puis la STAR est volée
        // aux altitudes prévues)
        match self.wpts.iter().position(|w| w.proc >= 2) {
            Some(k) => {
                let star: f64 = self.wpts[k..].windows(2).map(|w| dist_brg(w[0].lat, w[0].lon, w[1].lat, w[1].lon).0).sum();
                star + ((self.crz_fl / 10.0) * (1.0 + 0.5 * self.crz_mach) * hyper_k(self.crz_mach) + 30.0) * NM
            }
            None => tod_dist(self.crz_fl, self.crz_mach),
        }
    }
}

fn apt(icao: &str) -> Option<&'static crate::airports::Airport> {
    AIRPORTS.iter().find(|a| a.icao == icao)
}

/// Plan « DARKSTAR DISPATCH » : orthodromie découpée, points aux transitions du profil
/// (fin de montée turbine, allumage stato, transition scram, début de descente).
pub fn dispatch(from: &str, to: &str, crz_fl: f64, crz_mach: f64) -> Result<Fpln, String> {
    let a = apt(from).ok_or(format!("{from} INCONNU"))?;
    let b = apt(to).ok_or(format!("{to} INCONNU"))?;
    let (d, brg) = dist_brg(a.lat, a.lon, b.lat, b.lon);
    if d < 30.0 * NM {
        return Err("ETAPE TROP COURTE".into());
    }
    let mut marks: Vec<(f64, String)> = Vec::new();
    for (nm_from, name) in [(40.0, "T/TURB"), (160.0, "T/RAM"), (420.0, "T/SCRM")] {
        if nm_from * NM < d * 0.45 {
            marks.push((nm_from * NM, name.to_string()));
        }
    }
    let mut fp = Fpln { origin: from.into(), dest: to.into(), wpts: Vec::new(), active: 1, source: "DISPATCH", flt: "DKS01".into(), crz_fl, crz_mach, jp7_req: 0.0, h2_req: 0.0, ete: 0.0, ..Default::default() };
    let tod = fp.tod().min(d * 0.5);
    marks.push((d - tod, "T/D".into()));
    // Points intermédiaires toutes les ~400 NM
    let n = (d / (400.0 * NM)).floor() as usize;
    for k in 1..=n {
        let x = d * k as f64 / (n + 1) as f64;
        if marks.iter().all(|(m, _)| (m - x).abs() > 60.0 * NM) {
            marks.push((x, format!("DK{k:02}")));
        }
    }
    marks.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
    fp.wpts.push(Wpt { ident: from.into(), lat: a.lat, lon: a.lon, alt_ft: 0.0, mach: 0.0, phase: "DEP", ..Default::default() });
    // Orthodromie : point à la distance x sur l'arc a→b (relèvement initial)
    for (x, name) in marks {
        let (la, lo) = destination(a.lat, a.lon, brg, x);
        // Correction : on se recale vers b le long de l'orthodromie
        let (_, b2) = dist_brg(la, lo, b.lat, b.lon);
        let _ = b2;
        fp.wpts.push(Wpt { ident: name, lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "", ..Default::default() });
    }
    fp.wpts.push(Wpt { ident: to.into(), lat: b.lat, lon: b.lon, alt_ft: 0.0, mach: 0.0, phase: "ARR", ..Default::default() });
    fp.compute();
    Ok(fp)
}

/// Plan depuis un OFP SimBrief (JSON).
pub fn from_simbrief(j: &Json, crz_fl: f64, crz_mach: f64, crz_max: bool) -> Result<Fpln, String> {
    if let Some(e) = j.path(&["fetch", "status"]) {
        let st = e.s();
        if !st.eq_ignore_ascii_case("success") {
            return Err(format!("SIMBRIEF : {}", st.to_uppercase()));
        }
    }
    let o = j.get("origin").ok_or("OFP SANS ORIGINE")?;
    let d = j.get("destination").ok_or("OFP SANS DESTINATION")?;
    let pt = |x: &Json| -> Option<(String, f64, f64)> { Some((x.get("icao_code")?.s(), x.get("pos_lat")?.f()?, x.get("pos_long")?.f()?)) };
    let (oi, ola, olo) = pt(o).ok_or("ORIGINE INVALIDE")?;
    let (di, dla, dlo) = pt(d).ok_or("DESTINATION INVALIDE")?;
    let mut wpts = vec![Wpt { ident: oi.clone(), lat: ola, lon: olo, alt_ft: 0.0, mach: 0.0, phase: "DEP", ..Default::default() }];
    let (mut sid, mut star) = (String::new(), String::new());
    let dep_rwy = o.get("plan_rwy").map(|x| x.s()).unwrap_or_default();
    let arr_rwy = d.get("plan_rwy").map(|x| x.s()).unwrap_or_default();
    if let Some(fixes) = j.path(&["navlog", "fix"]) {
        for f in fixes.items() {
            let ident = f.get("ident").map(|x| x.s()).unwrap_or_default();
            let typ = f.get("type").map(|x| x.s()).unwrap_or_default();
            let (Some(la), Some(lo)) = (f.get("pos_lat").and_then(|x| x.f()), f.get("pos_long").and_then(|x| x.f())) else { continue };
            if ident.is_empty() || ident == di || ident == oi || typ == "apt" {
                continue;
            }
            // IFR : tous les points sont gardés (procédures comprises)
            let ss = f.get("is_sid_star").map(|x| x.s() == "1").unwrap_or(false);
            let stage = f.get("stage").map(|x| x.s()).unwrap_or_default();
            let via = f.get("via_airway").map(|x| x.s()).unwrap_or_default();
            let proc = if ss && stage == "CLB" { 1 } else if ss { 2 } else { 0 };
            if proc == 1 && sid.is_empty() {
                sid = via.clone();
            }
            if proc == 2 {
                star = via.clone();
            }
            let sb_alt = f.get("altitude_feet").and_then(|x| x.f()).unwrap_or(0.0);
            wpts.push(Wpt { ident: ident.chars().take(7).collect(), lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "", via, proc, sb_alt });
        }
    }
    wpts.push(Wpt { ident: di.clone(), lat: dla, lon: dlo, alt_ft: 0.0, mach: 0.0, phase: "ARR", ..Default::default() });
    let flt = j.path(&["atc", "callsign"]).map(|x| x.s()).filter(|s| !s.is_empty()).unwrap_or("DKS01".into());
    // Le niveau de croisière SimBrief (avion de ligne) est remplacé par le profil Darkstar
    let mut fp = Fpln { origin: oi, dest: di, wpts, active: 1, source: "SIMBRIEF", flt, crz_fl, crz_mach, jp7_req: 0.0, h2_req: 0.0, ete: 0.0, dep_rwy, arr_rwy, sid, star, crz_max };
    fp.compute();
    Ok(fp)
}

/// Lecture d'un point saisi : code OACI connu, ou « N3512.5W11745.0 » / « 35.2/-117.7 ».
pub fn parse_point(s: &str) -> Option<(String, f64, f64)> {
    let s = s.trim().to_uppercase();
    if let Some(a) = apt(&s) {
        return Some((a.icao.to_string(), a.lat, a.lon));
    }
    if let Some((a, b)) = s.split_once('/') {
        let (la, lo): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
        if la.abs() <= 90.0 && lo.abs() <= 180.0 {
            return Some((format!("{:.0}{:.0}", la.abs(), lo.abs()), la, lo));
        }
    }
    // N3512.5W11745.0
    let c: Vec<char> = s.chars().collect();
    if c.len() >= 9 && matches!(c[0], 'N' | 'S') {
        let ew = c.iter().position(|x| *x == 'E' || *x == 'W')?;
        let lat_s: String = c[1..ew].iter().collect();
        let lon_s: String = c[ew + 1..].iter().collect();
        let dm = |t: &str, deg_digits: usize| -> Option<f64> {
            let d: f64 = t.get(..deg_digits)?.parse().ok()?;
            let m: f64 = t.get(deg_digits..).filter(|x| !x.is_empty()).map(|x| x.parse().ok()).unwrap_or(Some(0.0))?;
            Some(d + m / 60.0)
        };
        let mut la = dm(&lat_s, 2)?;
        let mut lo = dm(&lon_s, 3)?;
        if c[0] == 'S' {
            la = -la;
        }
        if c[ew] == 'W' {
            lo = -lo;
        }
        return Some((format!("{}{}", &s[..3], &s[ew..ew + 4]), la, lo));
    }
    None
}

// ---------- Téléchargement SimBrief ----------

#[derive(Clone)]
pub enum Fetch {
    Idle,
    Busy,
    Done(Result<String, String>),
}

fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Library/Application Support/Darkstar/simbrief.txt")
}

// ---------- MCDU ----------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Init,
    Fpln,
    Perf,
    Prog,
    Fuel,
    Dir,
    /// DEPARTURES : piste, SID, transition
    Dep,
    /// ARRIVAL : approche (ILS), STAR, transition
    Arr,
    RadNav,
}

/// Sélection en cours sur les pages DEPARTURES / ARRIVAL : piste (ou approche), procédure,
/// transition (0 = aucune).
#[derive(Clone, Default)]
pub struct ProcSel {
    pub rwy: Option<String>,
    pub proc: Option<usize>,
    pub trans: Option<usize>,
}

/// ILS calé : (OACI, piste, indicatif, MHz, course vraie).
#[derive(Clone, PartialEq, Debug)]
pub struct IlsTune {
    pub apt: String,
    pub rwy: String,
    pub id: String,
    pub mhz: f64,
    pub crs: f64,
}

pub struct Fms {
    pub fpln: Option<Fpln>,
    /// Plan en attente d'activation (INIT / import)
    pub temp: Option<Fpln>,
    pub page: Page,
    pub scroll: usize,
    pub scratch: String,
    pub msg: String,
    pub user: String,
    pub from_to: (String, String),
    pub crz: (f64, f64),
    /// Croisière au plafond (FL MAX)
    pub crz_max: bool,
    fetch: Arc<Mutex<Fetch>>,
    /// Saisie clavier dirigée vers le MCDU
    pub focus: bool,
    /// Gestion verticale et de la vitesse par le FMS (VNAV + Mach géré + transitions moteur)
    pub vnav: bool,
    /// Type d'avion SimBrief utilisé pour générer la route (code SimBrief, ex. B77W, A359)
    pub acft: String,
    desc_started: bool,
    land_sent: bool,
    /// Dernière phase annoncée
    phase: &'static str,
    pub dep: ProcSel,
    pub arr: ProcSel,
    /// ILS calé automatiquement (départ, puis destination à l'approche)
    pub ils: Option<IlsTune>,
    /// Mach de croisière réduit par l'ordinateur de bord pour garder la réserve d'hydrogène
    pub eco_mach: Option<f64>,
    /// Réserve « deux remises des gaz » garantie à l'arrivée (JP-7 prévu à destination)
    pub ga_ok: bool,
    pub jp7_at_dest: f64,
    fuel_warned: bool,
    eco_decided: bool,
}

pub enum McduOut {
    /// Instruction à exécuter par la station sol (ALT, MACH, DESCENTE, DEST…)
    Cmd(String),
    Msg(Level, String),
}

impl Fms {
    /// Plan DARKSTAR DISPATCH calculé et activé directement (instruction DISPATCH).
    pub fn dispatch_now(&mut self, from: &str, to: &str, fl: f64, m: f64) -> Result<String, String> {
        let mut fp = dispatch(from, to, fl, m)?;
        fp.crz_max = self.crz_max;
        let msg = format!("DISPATCH {from}-{to} : {} POINTS, {:.0} NM, {} — JP-7 {:.1} t, H2 {:.1} t", fp.wpts.len(), fp.total() / NM, fmt_eta(fp.ete), fp.jp7_req / 1000.0, fp.h2_req / 1000.0);
        self.from_to = (from.into(), to.into());
        self.crz = (fl, m);
        self.fpln = Some(fp);
        self.temp = None;
        self.arm_vnav();
        Ok(msg)
    }

    fn arm_vnav(&mut self) {
        self.eco_mach = None;
        self.fuel_warned = false;
        self.eco_decided = false;
        self.vnav = true;
        self.desc_started = false;
        self.land_sent = false;
        self.phase = "";
    }

    /// VNAV : altitude, Mach et mode du TBCC gérés selon la position sur le plan.
    /// Montée turbines → stato (armement, allumage, coupure turbines, fermeture des entrées) →
    /// scram (si la croisière l'exige), croisière, descente à q constant au T/D, rallumage des
    /// turbines en subsonique, descente finale et ATTERRIR à 60 NM.
    pub fn vnav_update(&mut self, ac: &mut Aircraft) -> Vec<McduOut> {
        let mut out = Vec::new();
        if !self.vnav || ac.on_ground || ac.crashed.is_some() || ac.autoland.is_some() || !ac.ap.on || ac.gear_down || ac.gcas_active || ac.osr_active {
            return out;
        }
        let Some(fp) = self.fpln.as_ref() else { return out };
        let rem = fp.remaining(ac.lat, ac.lon);
        let tot = fp.total();
        let tod = fp.tod().min(tot * 0.5);
        let (crz_ft, crz_m, dest) = (fp.crz_fl * 100.0, fp.crz_mach, fp.dest.clone());
        let crz_max = fp.crz_max;
        // Procédures IFR : point actif dans une SID / STAR et altitudes prévues par SimBrief
        let act = fp.wpts.get(fp.active).cloned().unwrap_or_default();
        let star_first = fp.wpts.iter().find(|w| w.proc >= 2).map(|w| w.sb_alt).unwrap_or(0.0);
        // Distance par la route jusqu'à l'entrée de la STAR / de l'approche
        let d_star = {
            let mut d = 0.0;
            let (mut pl, mut po) = (ac.lat, ac.lon);
            for w in &fp.wpts[fp.active.min(fp.wpts.len())..] {
                d += dist_brg(pl, po, w.lat, w.lon).0;
                if w.proc >= 2 {
                    break;
                }
                pl = w.lat;
                po = w.lon;
            }
            d
        };
        let mach = ac.d.mach;
        let e = ac.eng.clone();
        let turb_on = e.side.iter().any(|s| matches!(s, TurbState::Running | TurbState::Starting));
        let say = |m: &mut Vec<McduOut>, ph: &mut &'static str, p: &'static str, txt: String| {
            if *ph != p {
                *ph = p;
                m.push(McduOut::Msg(Level::Info, txt));
            }
        };
        if act.proc == 1 && !self.desc_started {
            // ----- SID : altitudes de la procédure, 250 kt sous le FL100 -----
            let tgt = if act.sb_alt > 0.0 { act.sb_alt } else { 10_000.0 };
            ac.ap.vert = VertMode::Alt;
            ac.ap.alt_tgt_ft = ((tgt / 500.0).ceil() * 500.0).max(3_000.0);
            ac.ap.at_on = true;
            ac.ap.spd_managed = false;
            ac.ap.spd_keas = Some(if ac.alt / FT < 10_000.0 { 250.0 } else { 300.0 });
            say(&mut out, &mut self.phase, "SID", format!("FMS : SID {} — ALTITUDES DE LA PROCEDURE", act.via));
        } else if rem > tod && !self.desc_started {
            // ----- Montée / accélération / croisière -----
            // Manette reprise à la main : le FMS ne réengage pas l'A/THR
            if !ac.thr_manual {
                ac.ap.at_on = true;
            }
            ac.ap.spd_keas = None;
            ac.ap.spd_managed = false;
            // Ordinateur de bord : à l'allumage du scram, Mach de croisière qui permet d'arriver avec
            // l'hydrogène nécessaire. Le scram consomme MOINS par kilomètre à haut Mach : on garde
            // le Mach demandé s'il passe, sinon le Mach qui minimise la consommation totale
            // (accélération ~0,95 t par Mach au-delà de l'allumage + croisière).
            let d_crz = (rem - tod).max(0.0);
            let need = |m: f64| 950.0 * (m - mach).max(0.0).powf(1.15) + h2_rate(m) * d_crz / (m * 297.0) * 1.10;
            if crz_m > 4.8 && e.scram == ScramState::Lit && !self.eco_decided {
                self.eco_decided = true;
                if ac.h2 < need(crz_m) {
                    let mut best = (crz_m, need(crz_m));
                    let mut m = 5.0;
                    while m <= crz_m {
                        if need(m) < best.1 {
                            best = (m, need(m));
                        }
                        m += 0.2;
                    }
                    if best.1 <= ac.h2 {
                        self.eco_mach = Some(best.0);
                        out.push(McduOut::Msg(Level::Caution, format!("FMS : CROISIERE A M{:.1} AU LIEU DE M{crz_m:.1} — HYDROGENE AU PLUS JUSTE", best.0)));
                    } else if !self.fuel_warned {
                        self.fuel_warned = true;
                        let near = AIRPORTS.iter().min_by(|a, b| dist_brg(ac.lat, ac.lon, a.lat, a.lon).0.partial_cmp(&dist_brg(ac.lat, ac.lon, b.lat, b.lon).0).unwrap()).map(|a| a.icao).unwrap_or("");
                        out.push(McduOut::Msg(Level::Warning, format!("FMS : {dest} HORS D'ATTEINTE (H2 {:.1} t, BESOIN {:.1} t) — DEROUTEMENT CONSEILLE ({near} LE PLUS PROCHE)", ac.h2 / 1000.0, best.1 / 1000.0)));
                    }
                }
            }
            let crz_m = self.eco_mach.map(|e| e.min(crz_m)).unwrap_or(crz_m);
            // JP-7 : montée stato restante + descente / approche + deux remises des gaz
            let jp_need = APPR_JP7 + 2.0 * GA_JP7 + if e.scram == ScramState::Lit { 600.0 } else { 3_000.0 };
            self.jp7_at_dest = ac.fuel - (APPR_JP7 + if e.scram == ScramState::Lit { 600.0 } else { 3_000.0 });
            self.ga_ok = ac.fuel >= jp_need;
            if !self.ga_ok && !self.fuel_warned {
                self.fuel_warned = true;
                let near = AIRPORTS.iter().min_by(|a, b| dist_brg(ac.lat, ac.lon, a.lat, a.lon).0.partial_cmp(&dist_brg(ac.lat, ac.lon, b.lat, b.lon).0).unwrap()).map(|a| a.icao).unwrap_or("");
                out.push(McduOut::Msg(Level::Warning, format!("FMS : JP-7 INSUFFISANT POUR DEUX REMISES DES GAZ A {dest} — DEROUTEMENT CONSEILLE ({near} LE PLUS PROCHE)")));
            }
            ac.ap.mach_tgt = crz_m;
            // Phase scram : dès que le scram est armé / allumé / désamorcé, ou au-delà de Mach 4,8 (un
            // désamorçage ne doit pas faire redescendre l'avion vers le palier du stato)
            let scram_phase = crz_m > 4.8 && (e.scram != ScramState::Off || mach > 4.8);
            let sched = if crz_m <= 2.5 {
                crz_ft
            } else if scram_phase {
                // Couloir de pression dynamique : q élevée pour accélérer, 35 kPa en croisière
                let q_t = if mach < crz_m - 0.15 { 42_000.0 } else { 35_000.0 };
                (alt_for_q(mach.max(5.0), q_t) / 500.0).round() * 500.0
            } else if e.ram == RamState::Lit && !turb_on {
                if mach < 3.5 { 58_000.0 } else if mach < 4.2 { 72_000.0 } else { 80_000.0 }
            } else if e.ram == RamState::Lit {
                58_000.0
            } else if mach < 1.6 {
                35_000.0
            } else {
                50_000.0
            };
            ac.ap.vert = VertMode::Alt;
            ac.ap.alt_tgt_ft = if crz_max { sched } else { sched.min(crz_ft) };
            if crz_m > 2.5 && e.scram == ScramState::Off {
                if mach > 2.05 && e.ram == RamState::Off {
                    ac.cmd_ram();
                }
                if ac.eng.ram == RamState::Armed && ac.ram_ignition_check().is_ok() {
                    ac.cmd_ram();
                }
                if ac.eng.ram == RamState::Lit && mach > 3.05 && turb_on {
                    ac.cmd_turbine();
                    if ac.eng.inlet_open {
                        ac.cmd_inlet();
                    }
                }
            }
            if crz_m > 4.8 {
                if mach > 4.0 && ac.eng.scram == ScramState::Off {
                    ac.cmd_scram();
                }
                if ac.eng.scram == ScramState::Armed && ac.scram_ignition_check().is_ok() {
                    ac.cmd_scram();
                }
                // Désamorçage : rallumage direct dès que l'incidence et la pression dynamique le permettent
                if ac.eng.scram == ScramState::Unstart {
                    let ok = ac.d.alpha.to_degrees() < 5.5 && ac.d.q > 26_000.0 && ac.d.mach > 4.8;
                    if ok {
                        ac.cmd_scram();
                        if ac.eng.scram == ScramState::Lit {
                            out.push(McduOut::Msg(Level::Info, "FMS : SCRAM RALLUME APRES DESAMORCAGE".into()));
                        }
                    }
                }
            }
            let at_crz = (ac.alt / FT - crz_ft).abs() < 2_000.0 && (mach - crz_m).abs() < 0.15;
            let p = if at_crz { "CRZ" } else { "CLB" };
            say(&mut out, &mut self.phase, p, if at_crz { format!("FMS : CROISIERE FL{:.0} M{crz_m:.1}", crz_ft / 100.0) } else { format!("FMS : MONTEE / ACCELERATION VERS FL{:.0} M{crz_m:.1}", crz_ft / 100.0) });
        } else {
            // ----- Descente -----
            if !self.desc_started {
                self.desc_started = true;
                if mach > 1.2 {
                    ac.ap.vert = VertMode::QHold;
                    ac.ap.q_tgt = 50_000.0;
                    ac.ap.at_on = false;
                    ac.throttle = 0.0;
                }
                out.push(McduOut::Msg(Level::Info, "FMS : T/D — DESCENTE A q CONSTANT (50 kPa), MANETTE REDUITE".into()));
            }
            // Turbines à rallumer en subsonique / bas supersonique (entrées ouvertes sous Mach 2,5)
            if mach < 2.3 {
                if !ac.eng.inlet_open {
                    ac.cmd_inlet();
                }
                if ac.eng.scram != ScramState::Off {
                    ac.eng.scram = ScramState::Off;
                }
                if !turb_on && ac.eng.inlet_open && ac.eng.side.iter().any(|s| *s == TurbState::Off) {
                    ac.cmd_turbine();
                }
            }
            // Coupures propres avant les limites de fonctionnement (pas de désamorçage ni d'extinction)
            if mach < 5.0 && ac.eng.scram == ScramState::Lit {
                ac.eng.scram = ScramState::Off;
                out.push(McduOut::Msg(Level::Info, "FMS : SCRAM OFF (MACH < 5) — STATO EN DESCENTE".into()));
            }
            if mach < 2.6 && ac.eng.ram == RamState::Lit {
                ac.eng.ram = RamState::Off;
                out.push(McduOut::Msg(Level::Info, "FMS : STATO OFF (MACH < 2,6) — RALLUMAGE TURBINES".into()));
            }
            if ac.ap.vert != VertMode::QHold && (act.proc >= 2 || star_first > 0.0) {
                // STAR : altitudes prévues de la procédure ; avant l'entrée, pente de 3,5° vers
                // l'altitude d'entrée (pas de palier bas à des centaines de NM)
                let tgt = if act.proc >= 2 && act.sb_alt > 0.0 {
                    act.sb_alt
                } else {
                    let st = star_first.max(3_000.0);
                    (st + d_star * 3.5f64.to_radians().tan() / FT).clamp(st, 35_000.0)
                };
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (tgt / 500.0).round() * 500.0;
                ac.ap.at_on = true;
                ac.ap.spd_managed = false;
                ac.ap.spd_keas = Some(if ac.alt / FT < 10_000.0 { 250.0 } else if act.proc >= 2 { 280.0 } else { 320.0 });
                if act.proc == 2 {
                    say(&mut out, &mut self.phase, "STAR", format!("FMS : STAR {} — ALTITUDES DE LA PROCEDURE", act.via));
                } else if act.proc == 3 {
                    say(&mut out, &mut self.phase, "APPR", format!("FMS : APPROCHE {} — ILS CALE", act.via));
                }
            } else if ac.ap.vert != VertMode::QHold {
                // Descente finale : plan à ~3,5° vers la destination, 250-300 KEAS
                let elev_ft = AIRPORTS.iter().find(|a| a.icao == dest).map(|a| a.elev / FT).unwrap_or(0.0);
                let tgt = (elev_ft + rem * 3.5f64.to_radians().tan() / FT).clamp(elev_ft + 3_000.0, 35_000.0);
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = ((tgt / 500.0).round() * 500.0).min(ac.ap.alt_tgt_ft.max(tgt));
                ac.ap.at_on = true;
                ac.ap.spd_keas = Some(if rem > 30.0 * NM { 300.0 } else { 250.0 });
                say(&mut out, &mut self.phase, "DES", "FMS : DESCENTE FINALE VERS LA DESTINATION".into());
            }
            if !self.land_sent && rem < 60.0 * NM && mach < 0.95 && AIRPORTS.iter().any(|a| a.icao == dest) {
                self.land_sent = true;
                out.push(McduOut::Cmd(format!("ATTERRIR {dest}")));
            }
        }
        out
    }

    /// Listes proposées : (procédures, transitions de la procédure choisie), avec la procédure
    /// de l'OFP SimBrief en tête si le plan en contient une.
    fn proc_lists(&self, arr: bool) -> (Vec<crate::procs::Proc>, Vec<crate::procs::Proc>) {
        use crate::procs::*;
        let Some(p) = self.plan() else { return (Vec::new(), Vec::new()) };
        let icao = if arr { &p.dest } else { &p.origin };
        let Some(apt) = airport(icao) else { return (Vec::new(), Vec::new()) };
        let sel = if arr { &self.arr } else { &self.dep };
        let Some(e) = sel.rwy.as_ref().and_then(|r| find_end(apt, r)) else { return (Vec::new(), Vec::new()) };
        let mut procs = if arr { stars(apt, &e) } else { sids(apt, &e) };
        let code = if arr { 2 } else { 1 };
        let ofp: Vec<Wpt> = p.wpts.iter().filter(|w| w.proc == code && p.source == "SIMBRIEF").cloned().collect();
        let name = if arr { &p.star } else { &p.sid };
        if !ofp.is_empty() && !name.is_empty() {
            procs.insert(0, Proc { name: format!("{name} OFP"), pts: ofp });
        }
        let trans = match sel.proc.and_then(|i| procs.get(i)) {
            Some(pr) if !pr.name.ends_with("OFP") => {
                if arr {
                    star_trans(apt, pr)
                } else {
                    sid_trans(apt, pr)
                }
            }
            _ => Vec::new(),
        };
        (procs, trans)
    }

    /// Insère la sélection DEPARTURES / ARRIVAL dans le plan (temporaire s'il existe, sinon actif).
    fn insert_procs(&mut self, arr: bool) -> Result<String, String> {
        use crate::procs::*;
        let (procs, trans) = self.proc_lists(arr);
        let sel = if arr { self.arr.clone() } else { self.dep.clone() };
        let rwy = sel.rwy.clone().ok_or("CHOISIR UNE PISTE")?;
        let p = self.plan().ok_or("PAS DE PLAN")?.clone();
        let icao = if arr { p.dest.clone() } else { p.origin.clone() };
        let apt = airport(&icao).ok_or("AEROPORT HORS BASE")?;
        let e = find_end(apt, &rwy).ok_or("PISTE INCONNUE")?;
        let pr = sel.proc.and_then(|i| procs.get(i).cloned());
        let tr = sel.trans.and_then(|i| if i == 0 { None } else { trans.get(i - 1).cloned() });
        let mut fp = p;
        if arr {
            fp.wpts.retain(|w| w.proc < 2);
            let mut add: Vec<Wpt> = Vec::new();
            if let Some(t) = &tr {
                add.extend(t.pts.clone());
            }
            if let Some(s) = &pr {
                add.extend(s.pts.clone());
            }
            add.extend(approach(apt, &e).pts);
            let at = fp.wpts.len() - 1;
            for (i, w) in add.into_iter().enumerate() {
                fp.wpts.insert(at + i, w);
            }
            fp.arr_rwy = rwy.clone();
            fp.star = pr.as_ref().map(|s| s.name.trim_end_matches(" OFP").to_string()).unwrap_or_default();
        } else {
            fp.wpts.retain(|w| w.proc != 1);
            let mut add: Vec<Wpt> = Vec::new();
            if let Some(s) = &pr {
                add.extend(s.pts.clone());
            }
            if let Some(t) = &tr {
                add.extend(t.pts.clone());
            }
            for (i, w) in add.into_iter().enumerate() {
                fp.wpts.insert(1 + i, w);
            }
            fp.dep_rwy = rwy.clone();
            fp.sid = pr.as_ref().map(|s| s.name.trim_end_matches(" OFP").to_string()).unwrap_or_default();
        }
        fp.active = fp.active.clamp(1, fp.wpts.len() - 1);
        fp.compute();
        let m = if arr { format!("ARRIVEE {} {} {}", icao, fp.arr_rwy, fp.star) } else { format!("DEPART {} {} {}", icao, fp.dep_rwy, fp.sid) };
        if self.temp.is_some() {
            self.temp = Some(fp);
        } else {
            self.fpln = Some(fp);
        }
        Ok(m)
    }

    /// Calage automatique de l'ILS : piste de départ au sol et en montée, ILS de destination à
    /// 250 NM de l'arrivée ou dès le début de descente. Renvoie un message au changement.
    pub fn autotune(&mut self, ac: &Aircraft) -> Option<String> {
        use crate::procs::*;
        let p = self.fpln.as_ref()?;
        let rem = p.remaining(ac.lat, ac.lon);
        let to_dest = rem < 250.0 * NM || self.desc_started;
        let (icao, rwy) = if to_dest { (&p.dest, &p.arr_rwy) } else { (&p.origin, &p.dep_rwy) };
        let apt = airport(icao)?;
        // Sans piste choisie : piste la plus longue
        let e = if rwy.is_empty() { ends(apt).into_iter().max_by(|a, b| a.len.partial_cmp(&b.len).unwrap())? } else { find_end(apt, rwy)? };
        let (id, mhz) = ils(apt, &e);
        let t = IlsTune { apt: apt.icao.to_string(), rwy: e.ident.clone(), id, mhz, crs: e.hdg };
        if self.ils.as_ref() != Some(&t) {
            let m = format!("RAD NAV : ILS {} {} CALE AUTOMATIQUEMENT — {} {:.2} CRS {:03.0}", t.apt, t.rwy, t.id, t.mhz, t.crs);
            self.ils = Some(t);
            return Some(m);
        }
        None
    }

    pub fn new() -> Fms {
        let user = std::fs::read_to_string(config_path()).map(|s| s.trim().to_string()).unwrap_or_default();
        Fms { fpln: None, temp: None, page: Page::Init, scroll: 0, scratch: String::new(), msg: String::new(), user, from_to: (String::new(), String::new()), crz: (ceiling_fl(11.0), 11.0), crz_max: true, fetch: Arc::new(Mutex::new(Fetch::Idle)), focus: true, acft: "B77W".into(), vnav: true, desc_started: false, land_sent: false, phase: "", dep: ProcSel::default(), arr: ProcSel::default(), ils: None, eco_mach: None, ga_ok: true, jp7_at_dest: 0.0, fuel_warned: false, eco_decided: false }
    }

    /// Plan affiché / modifié : temporaire s'il existe, sinon actif.
    fn plan(&self) -> Option<&Fpln> {
        self.temp.as_ref().or(self.fpln.as_ref())
    }

    fn plan_mut(&mut self) -> Option<&mut Fpln> {
        if self.temp.is_some() { self.temp.as_mut() } else { self.fpln.as_mut() }
    }

    pub fn start_fetch(&mut self) {
        if self.user.is_empty() {
            self.msg = "SIMBRIEF ID REQUIS".into();
            return;
        }
        // Pilot ID numérique (userid) ou nom d'utilisateur (username)
        let key = if self.user.chars().all(|c| c.is_ascii_digit()) { "userid" } else { "username" };
        let url = format!("https://www.simbrief.com/api/xml.fetcher.php?{key}={}&json=1", self.user.to_lowercase());
        *self.fetch.lock().unwrap() = Fetch::Busy;
        let f = self.fetch.clone();
        std::thread::spawn(move || {
            let r = std::process::Command::new("/usr/bin/curl")
                .args(["-sfL", "--max-time", "30", "-A", "darkstar-sim"])
                .arg(&url)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).into_owned()) } else { Err("SIMBRIEF INJOIGNABLE OU ID INCONNU".into()) });
            *f.lock().unwrap() = Fetch::Done(r);
        });
        self.msg = "IMPORT OFP EN COURS".into();
    }

    /// Ouvre la page de génération SimBrief pré-remplie dans le navigateur.
    pub fn open_simbrief(&mut self) {
        let (o, d) = &self.from_to;
        // SimBrief ne connaît pas le Darkstar : un long-courrier sert à tracer la route (seule la
        // route est reprise à l'import, le profil vertical est celui du Darkstar)
        let url = format!("https://dispatch.simbrief.com/options/custom?orig={o}&dest={d}&type={}", self.acft);
        let ok = std::process::Command::new("/usr/bin/open").arg(&url).spawn().is_ok();
        self.msg = if ok { "SIMBRIEF OUVERT : GENERER PUIS IMPORT".into() } else { "NAVIGATEUR INDISPONIBLE".into() };
    }

    /// Suivi du téléchargement ; à appeler chaque image.
    pub fn poll(&mut self) -> Option<McduOut> {
        let st = self.fetch.lock().unwrap().clone();
        if let Fetch::Done(r) = st {
            *self.fetch.lock().unwrap() = Fetch::Idle;
            let res = r.and_then(|body| parse_json(&body)).and_then(|j| from_simbrief(&j, self.crz.0, self.crz.1, self.crz_max));
            return Some(match res {
                Ok(fp) => {
                    self.msg = format!("OFP {}-{} : {} PTS", fp.origin, fp.dest, fp.wpts.len());
                    self.from_to = (fp.origin.clone(), fp.dest.clone());
                    let m = format!("SIMBRIEF : OFP {} {}-{} IMPORTE ({} POINTS, {:.0} NM) — MCDU : ACTIVATE", fp.flt, fp.origin, fp.dest, fp.wpts.len(), fp.total() / NM);
                    self.temp = Some(fp);
                    self.page = Page::Fpln;
                    self.scroll = 0;
                    McduOut::Msg(Level::Info, m)
                }
                Err(e) => {
                    self.msg = e.chars().take(24).collect();
                    McduOut::Msg(Level::Caution, format!("SIMBRIEF : {e}"))
                }
            });
        }
        None
    }

    pub fn busy(&self) -> bool {
        matches!(*self.fetch.lock().unwrap(), Fetch::Busy)
    }

    /// Guidage latéral : cap vers le point actif, séquencement avec anticipation du virage.
    /// Renvoie None sans plan actif.
    pub fn guidance(&mut self, ac: &Aircraft) -> Option<(f64, Option<String>)> {
        let fp = self.fpln.as_mut()?;
        if fp.active >= fp.wpts.len() {
            return None;
        }
        let w = &fp.wpts[fp.active];
        let (d, brg) = dist_brg(ac.lat, ac.lon, w.lat, w.lon);
        let mut seq = None;
        if fp.active + 1 < fp.wpts.len() {
            let n = &fp.wpts[fp.active + 1];
            let (_, b2) = dist_brg(w.lat, w.lon, n.lat, n.lon);
            let turn = wrap180(b2 - brg).abs().to_radians();
            let v = ac.ground_speed().max(80.0);
            let r = v * v / (9.81 * 25f64.to_radians().tan());
            let anticip = (r * (turn / 2.0).tan()).clamp(1.5 * NM, 120.0 * NM);
            if d < anticip {
                fp.active += 1;
                seq = Some(format!("FMS : {} SEQUENCE — CAP SUR {}", w.ident, fp.wpts[fp.active].ident));
            }
        }
        let w = &fp.wpts[fp.active];
        Some((dist_brg(ac.lat, ac.lon, w.lat, w.lon).1, seq))
    }

    /// Points restants (pour la carte).
    pub fn route(&self) -> Vec<(f64, f64, String)> {
        match &self.fpln {
            Some(fp) => fp.wpts[fp.active.min(fp.wpts.len())..].iter().map(|w| (w.lat, w.lon, w.ident.clone())).collect(),
            None => Vec::new(),
        }
    }

    /// Touche de ligne (LSK) : `left`, ligne 0..5.
    pub fn lsk(&mut self, left: bool, line: usize, ac: &Aircraft) -> Option<McduOut> {
        let sp = self.scratch.trim().to_uppercase();
        let mut out = None;
        match (self.page, left, line) {
            // ----- INIT -----
            (Page::Init, true, 0) => {
                if let Some((a, b)) = sp.split_once('/') {
                    if apt(a).is_some() && apt(b).is_some() {
                        self.from_to = (a.into(), b.into());
                        self.scratch.clear();
                    } else {
                        self.msg = "OACI HORS BASE DARKSTAR".into();
                    }
                } else {
                    self.msg = "FORMAT : KPMD/PHNL".into();
                }
            }
            (Page::Init, false, 0) => {
                if !sp.is_empty() {
                    self.user = sp.to_lowercase();
                    let _ = std::fs::write(config_path(), &self.user);
                    self.scratch.clear();
                }
            }
            (Page::Init, true, 2) => {
                // « 950/6.0 », « FL950/M6 » ou « MAX/11 » (plafond selon le Mach)
                let t = sp.replace("FL", "").replace("MAX", "X").replace('M', "");
                if let Some((a, b)) = t.split_once('/') {
                    let max = a == "X";
                    let m = b.parse::<f64>().ok();
                    let fl = if max { m.map(ceiling_fl) } else { a.parse::<f64>().ok() };
                    if let (Some(fl), Some(m)) = (fl, m) {
                        if (100.0..=1_300.0).contains(&fl) && (0.5..=11.0).contains(&m) && (max || m <= 2.5 || fl <= ceiling_fl(m) + 50.0) {
                            self.crz = (fl, m);
                            self.crz_max = max;
                            if let Some(p) = self.plan_mut() {
                                p.crz_fl = fl;
                                p.crz_mach = m;
                                p.crz_max = max;
                                p.compute();
                            }
                            self.scratch.clear();
                        } else {
                            self.msg = format!("HORS DOMAINE (M11 MAX, FL{:.0} MAX)", ceiling_fl(m));
                        }
                    }
                } else {
                    self.msg = "FORMAT : 950/6.0 OU MAX/11".into();
                }
            }
            (Page::Init, false, 1) => self.start_fetch(),
            (Page::Init, false, 2) => self.open_simbrief(),
            (Page::Init, false, 3) => {
                // Code de type (B77W) ou identifiant d'un avion personnalisé SimBrief (ex. 793553_1731...)
                if (3..=30).contains(&sp.len()) && sp.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    self.acft = sp.clone();
                    self.scratch.clear();
                } else {
                    self.msg = "CODE SIMBRIEF : B77W".into();
                }
            }
            (Page::Init, true, 3) => {
                let (o, d) = self.from_to.clone();
                match dispatch(&o, &d, self.crz.0, self.crz.1) {
                    Ok(mut fp) => {
                        fp.crz_max = self.crz_max;
                        self.msg = format!("PLAN {o}-{d} CALCULE");
                        self.temp = Some(fp);
                        self.page = Page::Fpln;
                        self.scroll = 0;
                    }
                    Err(e) => self.msg = e,
                }
            }
            (_, false, 5) if self.temp.is_some() && matches!(self.page, Page::Init | Page::Fpln) => {
                let mut fp = self.temp.take().unwrap();
                // Point actif : le premier point devant l'avion
                fp.active = 1.min(fp.wpts.len() - 1);
                let dest = fp.dest.clone();
                let (cfl, cm) = (fp.crz_fl, fp.crz_mach);
                self.fpln = Some(fp);
                self.arm_vnav();
                self.msg = "F-PLN ACTIVE".into();
                out = Some(McduOut::Cmd(format!("DEST {dest}")));
                let _ = (cfl, cm);
            }
            (Page::Init, true, 5) => {
                self.page = Page::Fpln;
            }
            // ----- F-PLN -----
            (Page::Fpln, true, l) if l < 5 => {
                let idx = self.scroll + l;
                let active_view = self.temp.is_none();
                let nw = self.plan().map(|p| p.wpts.len()).unwrap_or(0);
                if sp.is_empty() && nw > 0 && (idx == 0 || idx == nw - 1) {
                    let arr = idx == nw - 1;
                    let p = self.plan().unwrap();
                    let r = if arr { p.arr_rwy.clone() } else { p.dep_rwy.clone() };
                    let sel = ProcSel { rwy: if r.is_empty() { None } else { Some(r) }, proc: None, trans: None };
                    if arr {
                        self.arr = sel;
                        self.page = Page::Arr;
                    } else {
                        self.dep = sel;
                        self.page = Page::Dep;
                    }
                    self.scroll = 0;
                    return None;
                }
                if let Some(p) = self.plan_mut() {
                    if idx < p.wpts.len() {
                        if sp == "CLR" {
                            if idx > 0 && idx + 1 < p.wpts.len() {
                                p.wpts.remove(idx);
                                if p.active > idx {
                                    p.active -= 1;
                                }
                                p.compute();
                                self.scratch.clear();
                            }
                        } else if !sp.is_empty() {
                            match parse_point(&sp) {
                                Some((id, la, lo)) if idx > 0 => {
                                    p.wpts.insert(idx, Wpt { ident: id, lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "", ..Default::default() });
                                    if p.active >= idx && active_view {
                                        p.active += 1;
                                    }
                                    p.compute();
                                    self.scratch.clear();
                                }
                                _ => self.msg = "FORMAT INVALIDE".into(),
                            }
                        } else if active_view && idx > 0 {
                            // DIRECT TO
                            p.active = idx;
                            let id = p.wpts[idx].ident.clone();
                            self.msg = format!("DIR TO {id}");
                            out = Some(McduOut::Msg(Level::Info, format!("FMS : DIRECT {id}")));
                        }
                    }
                }
            }
            (Page::Fpln, true, 5) => {
                if self.temp.is_some() {
                    self.temp = None;
                    self.msg = "PLAN TEMPORAIRE EFFACE".into();
                }
            }
            // ----- DEPARTURES / ARRIVAL -----
            (Page::Dep | Page::Arr, true, 5) => {
                self.page = Page::Fpln;
                self.scroll = 0;
            }
            (Page::Dep | Page::Arr, false, 5) => {
                let arr = self.page == Page::Arr;
                match self.insert_procs(arr) {
                    Ok(m) => {
                        self.msg = "INSERE".into();
                        self.page = Page::Fpln;
                        self.scroll = 0;
                        out = Some(McduOut::Msg(Level::Info, format!("FMS : {m}")));
                    }
                    Err(e) => self.msg = e,
                }
            }
            (Page::Dep | Page::Arr, true, l) if (1..5).contains(&l) => {
                let arr = self.page == Page::Arr;
                let icao = self.plan().map(|p| if arr { p.dest.clone() } else { p.origin.clone() }).unwrap_or_default();
                if let Some(apt) = crate::procs::airport(&icao) {
                    let ends = crate::procs::ends(apt);
                    if let Some(e) = ends.get(self.scroll + l - 1) {
                        let sel = ProcSel { rwy: Some(e.ident.clone()), proc: None, trans: None };
                        if arr {
                            self.arr = sel;
                        } else {
                            self.dep = sel;
                        }
                    }
                }
            }
            (Page::Dep | Page::Arr, false, l) if (1..5).contains(&l) => {
                let arr = self.page == Page::Arr;
                let (procs, trans) = self.proc_lists(arr);
                let sc = self.scroll;
                let sel = if arr { &mut self.arr } else { &mut self.dep };
                let i = l - 1;
                if sel.proc.is_none() {
                    if sc + i < procs.len() {
                        sel.proc = Some(sc + i);
                        sel.trans = Some(0);
                    }
                } else if i <= trans.len() {
                    sel.trans = Some(i);
                }
            }
            (Page::Dep | Page::Arr, false, 0) => {
                // Retour à la liste des procédures
                let sel = if self.page == Page::Arr { &mut self.arr } else { &mut self.dep };
                sel.proc = None;
                sel.trans = None;
            }
            // ----- PERF -----
            (Page::Perf, true, 5) => {
                self.vnav = !self.vnav;
                if self.vnav {
                    self.desc_started = false;
                    self.phase = "";
                }
                self.msg = if self.vnav { "VNAV ON".into() } else { "VNAV OFF".into() };
                out = Some(McduOut::Msg(Level::Info, if self.vnav { "FMS : VNAV ON — ALTITUDE, MACH ET MOTEURS GERES".into() } else { "FMS : VNAV OFF — CONSIGNES MANUELLES".into() }));
            }
            (Page::Perf, true, 4) => {
                out = Some(McduOut::Cmd("DESCENTE 50".into()));
                self.msg = "DESCENTE q CONSTANT".into();
            }
            (Page::Perf, false, 4) => {
                if let Some(p) = &self.fpln {
                    out = Some(McduOut::Cmd(format!("ATTERRIR {}", p.dest)));
                }
            }
            // ----- DIR -----
            (Page::Dir, true, l) if l < 5 => {
                if let Some(p) = self.fpln.as_mut() {
                    if !sp.is_empty() {
                        if let Some((id, la, lo)) = parse_point(&sp) {
                            let at = p.active.min(p.wpts.len());
                            p.wpts.insert(at, Wpt { ident: id.clone(), lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "", ..Default::default() });
                            p.compute();
                            self.scratch.clear();
                            out = Some(McduOut::Msg(Level::Info, format!("FMS : DIRECT {id}")));
                        } else {
                            self.msg = "FORMAT INVALIDE".into();
                        }
                    } else {
                        let idx = p.active + l;
                        if idx < p.wpts.len() {
                            p.active = idx;
                            let id = p.wpts[idx].ident.clone();
                            out = Some(McduOut::Msg(Level::Info, format!("FMS : DIRECT {id}")));
                        }
                    }
                }
            }
            _ => {}
        }
        let _ = ac;
        out
    }

    pub fn slew(&mut self, down: bool) {
        let n = self.plan().map(|p| p.wpts.len()).unwrap_or(0);
        if down {
            if self.scroll + 5 < n {
                self.scroll += 1;
            }
        } else {
            self.scroll = self.scroll.saturating_sub(1);
        }
    }
}

// ---------- Rendu ----------

const COLS: usize = 24;
const GREEN_M: Color = Color::new(0.2, 1.0, 0.35, 1.0);
const CYAN_M: Color = Color::new(0.3, 0.9, 1.0, 1.0);
const MAG_M: Color = Color::new(1.0, 0.35, 1.0, 1.0);
const AMB_M: Color = Color::new(1.0, 0.68, 0.15, 1.0);
const WHITE_M: Color = Color::new(0.95, 0.95, 0.95, 1.0);
const LBL_M: Color = Color::new(0.75, 0.78, 0.8, 1.0);

/// Ligne d'écran : texte gauche, droite, centre, couleurs.
struct Line {
    l: String,
    r: String,
    lc: Color,
    rc: Color,
    /// Colonne centrale (heure UTC sur F-PLN), facultative
    c: String,
    cc: Color,
}

fn ln(l: impl Into<String>, lc: Color, r: impl Into<String>, rc: Color) -> Line {
    Line { l: l.into(), r: r.into(), lc, rc, c: String::new(), cc: WHITE_M }
}

fn ln3(l: impl Into<String>, lc: Color, c: impl Into<String>, cc: Color, r: impl Into<String>, rc: Color) -> Line {
    Line { l: l.into(), r: r.into(), lc, rc, c: c.into(), cc }
}

/// Heure UTC (HHMM) dans `secs` secondes.
fn utc_in(secs: f64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let t = ((now + secs.max(0.0)) / 60.0) as i64 % (24 * 60);
    format!("{:02}{:02}", t / 60, t % 60)
}

fn fmt_eta(s: f64) -> String {
    let m = (s / 60.0).round() as i64;
    format!("{:02}{:02}", m / 60, m % 60)
}

/// Contenu de la page : titre, 6 paires (étiquette, donnée).
fn page_lines(f: &Fms, ac: &Aircraft) -> (String, Color, Vec<(Line, Line)>) {
    let blank = || ln("", WHITE_M, "", WHITE_M);
    let mut rows: Vec<(Line, Line)> = Vec::new();
    let title;
    let mut tc = WHITE_M;
    match f.page {
        Page::Dep | Page::Arr => {
            let arr = f.page == Page::Arr;
            let icao = f.plan().map(|p| if arr { p.dest.clone() } else { p.origin.clone() }).unwrap_or_default();
            title = if arr { format!("ARRIVAL TO {icao}") } else { format!("DEPARTURES FROM {icao}") };
            tc = if f.temp.is_some() { AMB_M } else { WHITE_M };
            let sel = if arr { &f.arr } else { &f.dep };
            let (procs, trans) = f.proc_lists(arr);
            let pname = sel.proc.and_then(|i| procs.get(i)).map(|p| p.name.clone()).unwrap_or("------".into());
            let tname = match (sel.proc, sel.trans) {
                (Some(_), Some(0)) | (Some(_), None) => "NONE".to_string(),
                (Some(_), Some(i)) => trans.get(i - 1).map(|t| t.name.clone()).unwrap_or("------".into()),
                _ => "------".into(),
            };
            let rw = sel.rwy.clone().unwrap_or("---".into());
            let (lr, lp) = if arr { ("APPR", "STAR") } else { ("RWY", "SID") };
            rows.push((ln(format!(" {lr}"), LBL_M, format!("{lp}  TRANS "), LBL_M), ln(if arr { format!("ILS{rw}") } else { rw }, AMB_M, format!("{pname} {tname}"), AMB_M)));
            let apt = crate::procs::airport(&icao);
            let ends = apt.map(crate::procs::ends).unwrap_or_default();
            // Colonne droite : procédures, puis transitions une fois la procédure choisie
            let right: Vec<String> = if sel.rwy.is_none() {
                Vec::new()
            } else if sel.proc.is_none() {
                procs.iter().skip(f.scroll).map(|p| p.name.clone()).collect()
            } else {
                std::iter::once("NO TRANS".to_string()).chain(trans.iter().map(|t| t.name.clone())).collect()
            };
            for k in 0..4 {
                let e = ends.get(f.scroll + k);
                let left = match (e, apt) {
                    (Some(e), Some(a)) => {
                        let (id, mhz) = crate::procs::ils(a, e);
                        let _ = id;
                        (format!("<{}{}", if arr { "ILS" } else { "" }, e.ident), format!(" {:.0}M CRS{:03.0} {:.2}", e.len, e.hdg, mhz))
                    }
                    _ => (String::new(), String::new()),
                };
                let sel_l = e.is_some_and(|e| Some(&e.ident) == sel.rwy.as_ref());
                let r = right.get(k).cloned().unwrap_or_default();
                rows.push((ln(left.1, LBL_M, "", LBL_M), ln(left.0, if sel_l { GREEN_M } else { CYAN_M }, if r.is_empty() { String::new() } else { format!("{r}>") }, CYAN_M)));
            }
            rows.push((blank(), ln("<RETURN", WHITE_M, "INSERT*", AMB_M)));
        }
        Page::RadNav => {
            title = "RAD NAV".into();
            match &f.ils {
                Some(t) => {
                    rows.push((ln(" ILS / FREQ", LBL_M, "CRS ", LBL_M), ln(format!("{}/{:.2}", t.id, t.mhz), CYAN_M, format!("{:03.0}", t.crs), CYAN_M)));
                    rows.push((ln(" AEROPORT  PISTE", LBL_M, "MODE ", LBL_M), ln(format!("{} {}", t.apt, t.rwy), GREEN_M, "AUTO", GREEN_M)));
                }
                None => rows.push((ln(" ILS / FREQ", LBL_M, "", LBL_M), ln("[ ]/[ ]", AMB_M, "", WHITE_M))),
            }
            rows.push((ln(" CALAGE AUTO", LBL_M, "", LBL_M), ln("DEP PUIS DEST A 250NM", LBL_M, "", WHITE_M)));
            while rows.len() < 6 {
                rows.push((blank(), blank()));
            }
        }
        Page::Init => {
            title = "INIT  DK-FMS".to_string();
            let (o, d) = &f.from_to;
            let ft = if o.is_empty() { "____/____".to_string() } else { format!("{o}/{d}") };
            let user = if f.user.is_empty() { "________".to_string() } else { f.user.to_uppercase() };
            rows.push((ln(" CO RTE", LBL_M, "SIMBRIEF ID ", LBL_M), ln(ft, if o.is_empty() { AMB_M } else { CYAN_M }, user, if f.user.is_empty() { AMB_M } else { CYAN_M })));
            let st = if f.busy() { "EN COURS" } else { "IMPORT OFP>" };
            rows.push((ln(" FLT NBR", LBL_M, "", LBL_M), ln(f.plan().map(|p| p.flt.clone()).unwrap_or("DKS01".into()), CYAN_M, st, WHITE_M)));
            rows.push((ln(" CRZ FL/MACH", LBL_M, "GENERER", LBL_M), ln(if f.crz_max { format!("MAX({:.0})/M{:.1}", f.crz.0, f.crz.1) } else { format!("FL{:.0}/M{:.1}", f.crz.0, f.crz.1) }, CYAN_M, "SIMBRIEF>", WHITE_M)));
            rows.push((ln(" CALCUL PLAN", LBL_M, "AVION SIMBRIEF ", LBL_M), ln("<DARKSTAR DISPATCH", WHITE_M, f.acft.clone(), CYAN_M)));
            let info = match f.plan() {
                Some(p) => format!("{} {}-{} {:.0}NM", p.source, p.origin, p.dest, p.total() / NM),
                None => "AUCUN PLAN".into(),
            };
            rows.push((ln(" PLAN", LBL_M, "", LBL_M), ln(info, if f.plan().is_some() { GREEN_M } else { LBL_M }, "", WHITE_M)));
            rows.push((blank(), if f.temp.is_some() { ln("<F-PLN", WHITE_M, "ACTIVATE*", AMB_M) } else { ln("<F-PLN", WHITE_M, "", WHITE_M) }));
        }
        Page::Fpln => {
            let tmp = f.temp.is_some();
            title = if tmp { "TMPY F-PLN".into() } else { format!("FROM  {}", f.plan().map(|p| p.flt.clone()).unwrap_or_default()) };
            tc = if tmp { AMB_M } else { WHITE_M };
            match f.plan() {
                None => {
                    rows.push((blank(), ln("PAS DE PLAN", AMB_M, "", WHITE_M)));
                }
                Some(p) => {
                    // Format Airbus : voie au-dessus du point, distance de la branche au centre,
                    // point en vert (actif blanc, temporaire jaune), heure UTC, vitesse / altitude
                    let gs = ac.ground_speed().max(120.0);
                    let mut prev: Option<&Wpt> = if f.scroll > 0 { p.wpts.get(f.scroll - 1) } else { None };
                    // Distance cumulée depuis l'avion jusqu'au point i (pour l'heure estimée)
                    let mut cum = vec![0.0f64; p.wpts.len()];
                    {
                        let (mut pl, mut po, mut acc) = (ac.lat, ac.lon, 0.0);
                        for k in p.active.min(p.wpts.len())..p.wpts.len() {
                            acc += dist_brg(pl, po, p.wpts[k].lat, p.wpts[k].lon).0;
                            cum[k] = acc;
                            pl = p.wpts[k].lat;
                            po = p.wpts[k].lon;
                        }
                    }
                    for i in f.scroll..(f.scroll + 5).min(p.wpts.len()) {
                        let w = &p.wpts[i];
                        let dd = prev.map(|q| dist_brg(q.lat, q.lon, w.lat, w.lon).0).unwrap_or(0.0);
                        let act = !tmp && i == p.active;
                        let passed = !tmp && i < p.active;
                        let col = if tmp { AMB_M } else if act { WHITE_M } else if passed { LBL_M } else { GREEN_M };
                        let via = if i == 0 { if p.sid.is_empty() { "".to_string() } else { p.sid.clone() } } else if w.via.is_empty() { if w.phase.starts_with("T/") || w.ident.starts_with("T/") { String::new() } else { "DCT".into() } } else { w.via.clone() };
                        let lab_c = if i > 0 && dd > 0.0 { format!("{:.0}", dd / NM) } else { String::new() };
                        let ident = if i == 0 { format!("{}{}", w.ident, p.dep_rwy) } else if w.ident.starts_with("T/") { format!("({})", w.ident) } else { w.ident.clone() };
                        let time = if i == 0 || passed || tmp { "----".to_string() } else { utc_in(cum[i] / gs) };
                        let spd = if w.mach > 1.0 { format!("M{:.1}", w.mach) } else if i == 0 || i + 1 == p.wpts.len() { "---".into() } else { format!("{:.0}", (w.mach * 600.0).clamp(250.0, 340.0)) };
                        let alt = if w.alt_ft > 18_000.0 { format!("FL{:03.0}", w.alt_ft / 100.0) } else { format!("{:.0}", w.alt_ft) };
                        rows.push((ln3(format!(" {via}"), LBL_M, lab_c, LBL_M, "", LBL_M), ln3(ident, col, time, GREEN_M, format!("{spd}/{alt}"), if act { WHITE_M } else { CYAN_M })));
                        prev = Some(w);
                    }
                    while rows.len() < 5 {
                        rows.push((blank(), blank()));
                    }
                    let rem = if tmp { p.total() } else { p.remaining(ac.lat, ac.lon) };
                    // Carburant estimé à destination (JP-7 + H2, t)
                    let tot = p.total().max(1.0);
                    let efob = ((ac.fuel + ac.h2) - (p.jp7_req - APPR_JP7 - 2.0 * GA_JP7 + p.h2_req) * (rem / tot)) / 1000.0;
                    let dest = format!("{}{}", p.dest, p.arr_rwy);
                    rows.push((
                        ln3(" DEST", LBL_M, "UTC", LBL_M, "DIST  EFOB ", LBL_M),
                        if tmp { ln("<ERASE", AMB_M, "INSERT*", AMB_M) } else { ln3(dest, WHITE_M, utc_in(rem / gs), GREEN_M, format!("{:.0}  {:.1}", rem / NM, efob.max(0.0)), GREEN_M) },
                    ));
                }
            }
        }
        Page::Perf => {
            title = "PERF  DARKSTAR".into();
            let (crz_fl, crz_m) = f.plan().map(|p| (p.crz_fl, p.crz_mach)).unwrap_or(f.crz);
            let mode = ac.mode_name();
            let phases = [
                ("TURB", "CLB M0.9 FL350", "<M2.5"),
                ("RAM", "ACC M2.6 FL450", "M2.5-4.8"),
                ("RAM", "CLB M4.8 FL700", ">FL400"),
                ("SCRAM", &*format!("CRZ M{crz_m:.1} FL{crz_fl:.0}"), "q 25-90"),
            ];
            for (m, t, cond) in phases {
                let cur = mode.contains(m);
                rows.push((ln(format!(" {m}"), LBL_M, cond, LBL_M), ln(t, if cur { GREEN_M } else { CYAN_M }, if cur { "ACTIF" } else { "" }, GREEN_M)));
            }
            rows.push((ln(" DES q CONST", LBL_M, "AUTOLAND ", LBL_M), ln("<DESCENTE", WHITE_M, "ATTERRIR>", WHITE_M)));
            rows.push((ln(" VNAV / MACH GERES", LBL_M, "CRZ ", LBL_M), ln(if f.vnav { "<VNAV ON" } else { "<VNAV OFF" }, if f.vnav { GREEN_M } else { AMB_M }, format!("FL{crz_fl:.0}/M{crz_m:.1}"), CYAN_M)));
        }
        Page::Prog => {
            title = format!("PROG  {}", ac.mode_name());
            rows.push((ln(" CRZ", LBL_M, "MACH  GS", LBL_M), ln(format!("FL{:.0}", ac.alt / FT / 100.0), GREEN_M, format!("M{:.2} {:.0}", ac.d.mach, ac.ground_speed() / KT), GREEN_M)));
            match &f.fpln {
                Some(p) if p.active < p.wpts.len() => {
                    let w = &p.wpts[p.active];
                    let (d, b) = dist_brg(ac.lat, ac.lon, w.lat, w.lon);
                    let gs = ac.ground_speed().max(50.0);
                    rows.push((ln(" TO", LBL_M, "BRG   DIST  ETE", LBL_M), ln(w.ident.clone(), MAG_M, format!("{b:03.0} {:.0} {}", d / NM, fmt_eta(d / gs)), GREEN_M)));
                    let rem = p.remaining(ac.lat, ac.lon);
                    rows.push((ln(" DEST", LBL_M, "DIST  ETE", LBL_M), ln(p.dest.clone(), WHITE_M, format!("{:.0} {}", rem / NM, fmt_eta(rem / gs)), GREEN_M)));
                    let tod = rem - p.tod();
                    rows.push((ln(" T/D", LBL_M, "", LBL_M), ln(if tod > 0.0 { format!("DANS {:.0}NM", tod / NM) } else { "DESCENTE".into() }, if tod > 0.0 { GREEN_M } else { AMB_M }, "", WHITE_M)));
                }
                _ => {
                    rows.push((blank(), ln("PAS DE F-PLN ACTIF", AMB_M, "", WHITE_M)));
                }
            }
            rows.push((ln(" POSITION", LBL_M, "", LBL_M), ln(format!("{:.2}/{:.2}", ac.lat, ac.lon), GREEN_M, "", WHITE_M)));
            while rows.len() < 6 {
                rows.push((blank(), blank()));
            }
        }
        Page::Fuel => {
            title = "FUEL PRED".into();
            rows.push((ln(" JP-7 BORD", LBL_M, "H2 BORD ", LBL_M), ln(format!("{:.1}T", ac.fuel / 1000.0), GREEN_M, format!("{:.1}T", ac.h2 / 1000.0), GREEN_M)));
            match f.plan() {
                Some(p) => {
                    rows.push((ln(" JP-7 REQ", LBL_M, "H2 REQ ", LBL_M), ln(format!("{:.1}T", p.jp7_req / 1000.0), CYAN_M, format!("{:.1}T", p.h2_req / 1000.0), CYAN_M)));
                    let ej = ac.fuel - p.jp7_req;
                    let eh = ac.h2 - p.h2_req;
                    rows.push((ln(" EXTRA JP-7", LBL_M, "EXTRA H2 ", LBL_M), ln(format!("{:+.1}T", ej / 1000.0), if ej < 0.0 { AMB_M } else { GREEN_M }, format!("{:+.1}T", eh / 1000.0), if eh < 0.0 { AMB_M } else { GREEN_M })));
                    rows.push((ln(" RESERVE 2 REMISES GAZ", LBL_M, "MACH ECO ", LBL_M), ln(format!("{:.1}T {}", 2.0 * GA_JP7 / 1000.0, if f.ga_ok { "OK" } else { "NON GARANTIE" }), if f.ga_ok { GREEN_M } else { AMB_M }, f.eco_mach.map(|m| format!("M{m:.1}")).unwrap_or("---".into()), if f.eco_mach.is_some() { AMB_M } else { LBL_M })));
                    rows.push((ln(" TEMPS PLAN", LBL_M, "", LBL_M), ln(fmt_eta(p.ete), GREEN_M, "", WHITE_M)));
                    if ej < 0.0 || eh < 0.0 {
                        rows.push((blank(), ln("CARBURANT INSUFFISANT", AMB_M, "", WHITE_M)));
                    }
                }
                None => rows.push((blank(), ln("PAS DE PLAN", AMB_M, "", WHITE_M))),
            }
            let ff = ac.eng.fuel_flow + ac.eng.h2_flow;
            rows.push((ln(" DEBIT TOTAL", LBL_M, "", LBL_M), ln(format!("{ff:.2} KG/S"), GREEN_M, "", WHITE_M)));
            while rows.len() < 6 {
                rows.push((blank(), blank()));
            }
        }
        Page::Dir => {
            title = "DIR TO".into();
            match &f.fpln {
                Some(p) => {
                    for k in 0..5 {
                        let i = p.active + k;
                        if let Some(w) = p.wpts.get(i) {
                            rows.push((blank(), ln(format!("<{}", w.ident), if k == 0 { MAG_M } else { GREEN_M }, "", WHITE_M)));
                        } else {
                            rows.push((blank(), blank()));
                        }
                    }
                    rows.push((ln(" SAISIR OACI OU LAT/LON", LBL_M, "", LBL_M), ln("PUIS LSK GAUCHE", LBL_M, "", WHITE_M)));
                }
                None => rows.push((blank(), ln("PAS DE F-PLN ACTIF", AMB_M, "", WHITE_M))),
            }
            while rows.len() < 6 {
                rows.push((blank(), blank()));
            }
        }
    }
    (title, tc, rows)
}

pub enum McduKey {
    Lsk(bool, usize),
    Page(Page),
    Slew(bool),
    Clr,
    Focus,
    /// Touche du clavier alphanumérique (caractère à ajouter à la ligne de saisie)
    Char(char),
    /// +/- : inverse le signe de la saisie
    PlusMinus,
    /// Touches sans fonction sur le Darkstar (SEC F-PLN, ATC COMM…)
    Inop(&'static str),
    /// Luminosité de l'écran (+1 / -1)
    Bright(i32),
    /// AIRPORT : défile jusqu'à la destination
    Airport,
}

/// Luminosité de l'écran du MCDU (BRT / DIM).
pub fn bright(d: i32) {
    BRT.with(|c| c.set((c.get() + d as f32 * 0.15).clamp(0.3, 1.0)));
}

thread_local! {
    static BRT: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Touche du MCDU : fond gris foncé, légende blanche ; renvoie vrai si cliquée.
fn mkey(r: Rect, label: &str, round: bool, framed: bool) -> bool {
    let hov = hovered(r);
    let face = if hov { Color::new(0.30, 0.31, 0.33, 1.0) } else { Color::new(0.20, 0.21, 0.22, 1.0) };
    if round {
        let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
        draw_circle(c.x + 1.0, c.y + 2.0, r.w / 2.0, Color::new(0.05, 0.05, 0.06, 1.0));
        draw_circle(c.x, c.y, r.w / 2.0, face);
        draw_circle_lines(c.x, c.y, r.w / 2.0, 1.0, Color::new(0.45, 0.46, 0.48, 1.0));
    } else {
        draw_rectangle(r.x + 1.0, r.y + 2.0, r.w, r.h, Color::new(0.05, 0.05, 0.06, 1.0));
        draw_rectangle(r.x, r.y, r.w, r.h, face);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.45, 0.46, 0.48, 1.0));
    }
    let sz = if label.len() > 4 { 9.5 } else { 12.0 };
    if label.contains(' ') && !round {
        let (a, b) = label.split_once(' ').unwrap();
        txt_c(a, r.x + r.w / 2.0, r.y + r.h / 2.0 - 2.0, sz, WHITE_M);
        txt_c(b, r.x + r.w / 2.0, r.y + r.h / 2.0 + 10.0, sz, WHITE_M);
    } else {
        txt_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + sz * 0.36, if round { 14.0 } else { sz }, WHITE_M);
    }
    if framed {
        draw_rectangle_lines(r.x + 5.0, r.y + 5.0, r.w - 10.0, r.h - 10.0, 1.0, WHITE_M);
    }
    clicked(r)
}

/// MCDU façon A320 : écran 24 colonnes x 14 lignes, 6 touches de ligne de chaque côté, touches
/// de page, flèches, BRT/DIM, pavé numérique rond, clavier alphabétique, SP, OVFY, CLR.
pub fn draw_mcdu(f: &Fms, ac: &Aircraft, r: Rect, time: f64) -> Option<McduKey> {
    let mut out = None;
    // Boîtier gris
    let bw = (r.w - 16.0).min(560.0);
    let b = Rect::new(r.x + (r.w - bw) / 2.0, r.y + 6.0, bw, r.h - 12.0);
    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.33, 0.35, 0.37, 1.0));
    draw_rectangle_lines(b.x, b.y, b.w, b.h, 2.0, Color::new(0.18, 0.19, 0.2, 1.0));
    for k in 0..5 {
        draw_rectangle(b.x + b.w * 0.36 + k as f32 * b.w * 0.07, b.y + 7.0, b.w * 0.05, 5.0, Color::new(0.1, 0.1, 0.11, 1.0));
    }
    let brt = BRT.with(|c| c.get());
    let lsk_w = 30.0;
    let scr = Rect::new(b.x + lsk_w + 22.0, b.y + 20.0, b.w - 2.0 * (lsk_w + 22.0), (b.h * 0.45).min(300.0));
    draw_rectangle(scr.x - 4.0, scr.y - 4.0, scr.w + 8.0, scr.h + 8.0, Color::new(0.1, 0.1, 0.11, 1.0));
    draw_rectangle(scr.x, scr.y, scr.w, scr.h, BLACK);
    if clicked(scr) {
        out = Some(McduKey::Focus);
    }
    let dim = |c: Color| Color::new(c.r * brt, c.g * brt, c.b * brt, c.a);
    let rowh = scr.h / 14.0;
    let fs = (rowh * 1.0).min(19.0);
    let small = fs * 0.72;
    let (title, tc, rows) = page_lines(f, ac);
    txt_c(&title, scr.x + scr.w / 2.0, scr.y + rowh * 0.9, fs * 0.9, dim(tc));
    if f.page == Page::Fpln {
        // Flèches de défilement latéral / vertical (dessinées)
        let (ax, ay) = (scr.x + scr.w - 22.0, scr.y + rowh * 0.6);
        draw_line(ax - 8.0, ay, ax + 8.0, ay, 1.5, dim(WHITE_M));
        draw_triangle(vec2(ax - 11.0, ay), vec2(ax - 6.0, ay - 3.0), vec2(ax - 6.0, ay + 3.0), dim(WHITE_M));
        draw_triangle(vec2(ax + 11.0, ay), vec2(ax + 6.0, ay - 3.0), vec2(ax + 6.0, ay + 3.0), dim(WHITE_M));
    }
    let cx_col = scr.x + scr.w * 0.47;
    for (k, (lab, dat)) in rows.iter().enumerate().take(6) {
        let yl = scr.y + rowh * (2.0 * k as f32 + 1.85);
        let yd = yl + rowh;
        txt(&lab.l, scr.x + 4.0, yl, small, dim(lab.lc));
        txt_c(&lab.c, cx_col, yl, small, dim(lab.cc));
        txt_r(&lab.r, scr.x + scr.w - 4.0, yl, small, dim(lab.rc));
        txt(&dat.l, scr.x + 4.0, yd, fs, dim(dat.lc));
        txt_c(&dat.c, cx_col, yd, fs * 0.8, dim(dat.cc));
        txt_r(&dat.r, scr.x + scr.w - 4.0, yd, fs, dim(dat.rc));
        // Touches de ligne (LSK) avec leur trait vers l'écran
        for (left, x) in [(true, b.x + 10.0), (false, b.x + b.w - 10.0 - lsk_w)] {
            let kr = Rect::new(x, yd - rowh * 0.75, lsk_w, rowh * 0.95);
            let m1 = if left { kr.x + kr.w + 3.0 } else { kr.x - 12.0 };
            draw_line(m1, kr.y + kr.h / 2.0, m1 + 9.0, kr.y + kr.h / 2.0, 2.0, Color::new(0.85, 0.86, 0.87, 1.0));
            if mkey(kr, "", false, false) {
                out = Some(McduKey::Lsk(left, k));
            }
        }
    }
    // Ligne de saisie
    let ys = scr.y + rowh * 13.7;
    let cursor = if f.focus && (time * 2.0).fract() < 0.5 { "_" } else { "" };
    if !f.msg.is_empty() && f.scratch.is_empty() {
        txt(&f.msg, scr.x + 4.0, ys, fs, dim(AMB_M));
    } else {
        txt(&format!("{}{cursor}", f.scratch), scr.x + 4.0, ys, fs, dim(WHITE_M));
    }
    // Touches de page (3 rangées de 6) + BRT / DIM
    let ky = scr.y + scr.h + 14.0;
    let kw = (b.w - 2.0 * lsk_w - 70.0) / 6.0;
    // 9 rangées sous l'écran (touches de page, AIRPORT, flèches, pavé / clavier) : hauteur adaptée
    let kh = (((b.y + b.h - 8.0) - ky) / 9.0 - 6.0).clamp(16.0, 30.0);
    let x0 = b.x + lsk_w + 8.0;
    let page_keys: [[(&str, Option<McduKey>); 6]; 2] = [
        [("DIR", Some(McduKey::Page(Page::Dir))), ("PROG", Some(McduKey::Page(Page::Prog))), ("PERF", Some(McduKey::Page(Page::Perf))), ("INIT", Some(McduKey::Page(Page::Init))), ("DATA", Some(McduKey::Page(Page::Fuel))), ("", None)],
        [("F-PLN", Some(McduKey::Page(Page::Fpln))), ("RAD NAV", Some(McduKey::Page(Page::RadNav))), ("FUEL PRED", Some(McduKey::Page(Page::Fuel))), ("SEC F-PLN", Some(McduKey::Inop("SEC F-PLN"))), ("ATC COMM", Some(McduKey::Inop("ATC COMM"))), ("MCDU MENU", Some(McduKey::Page(Page::Init)))],
    ];
    for (row, keys) in page_keys.into_iter().enumerate() {
        for (c, (lbl, act)) in keys.into_iter().enumerate() {
            let kr = Rect::new(x0 + c as f32 * (kw + 4.0), ky + row as f32 * (kh + 6.0), kw, kh);
            if mkey(kr, lbl, false, false) {
                if let Some(a) = act {
                    out = Some(a);
                }
            }
        }
    }
    for (k, (lbl, d)) in [("BRT", 1), ("DIM", -1)].into_iter().enumerate() {
        let kr = Rect::new(b.x + b.w - lsk_w - 10.0, ky + k as f32 * (kh + 6.0), lsk_w + 4.0, kh);
        if mkey(kr, lbl, false, false) {
            out = Some(McduKey::Bright(d));
        }
    }
    // AIRPORT, flèches
    let ly = ky + 2.0 * (kh + 6.0);
    if mkey(Rect::new(x0, ly, ((3.0 * (kh + 10.0) - 4.0) / 2.0).min(kw), kh), "AIR PORT", false, false) {
        out = Some(McduKey::Airport);
    }
    let arrows = [("<", McduKey::Slew(false)), ("^", McduKey::Slew(false)), (">", McduKey::Slew(true)), ("v", McduKey::Slew(true))];
    // Largeur limitée à la colonne du pavé numérique (le clavier alphabétique est à droite)
    let aw = ((3.0 * (kh + 10.0) - 4.0) / 2.0).min(kw);
    for (k, (lbl, a)) in arrows.into_iter().enumerate() {
        let kr = Rect::new(x0 + (k % 2) as f32 * (aw + 4.0), ly + (1 + k / 2) as f32 * (kh + 6.0), aw, kh);
        if mkey(kr, lbl, false, false) {
            out = Some(a);
        }
    }
    // Pavé numérique (ronds) et clavier alphabétique (carrés)
    let ny = ly + 3.0 * (kh + 6.0);
    let nd = kh;
    let nums = ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "+/-"];
    for (k, n) in nums.iter().enumerate() {
        let kr = Rect::new(x0 + (k % 3) as f32 * (nd + 10.0), ny + (k / 3) as f32 * (nd + 6.0), nd, nd);
        if mkey(kr, n, true, false) {
            out = Some(if *n == "+/-" { McduKey::PlusMinus } else { McduKey::Char(n.chars().next().unwrap()) });
        }
    }
    let ax0 = x0 + 3.0 * (nd + 10.0) + 14.0;
    let akw = ((b.x + b.w - lsk_w - 10.0) - ax0 - 4.0 * 5.0) / 5.0;
    let alpha_y = ly;
    let letters = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "/", "SP", "OVFY", "CLR"];
    for (k, l) in letters.iter().enumerate() {
        let kr = Rect::new(ax0 + (k % 5) as f32 * (akw + 5.0), alpha_y + (k / 5) as f32 * (kh + 6.0), akw, kh);
        let framed = matches!(*l, "E" | "N" | "S" | "W");
        if mkey(kr, l, false, framed) {
            out = Some(match *l {
                "CLR" => McduKey::Clr,
                "SP" => McduKey::Char(' '),
                "OVFY" => McduKey::Inop("OVFY"),
                _ => McduKey::Char(l.chars().next().unwrap()),
            });
        }
    }
    let _ = (COLS, CYAN_M, MAG_M);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_and_simbrief() {
        let body = r#"{"fetch":{"status":"Success"},"origin":{"icao_code":"KLAX","pos_lat":"33.94","pos_long":"-118.40"},
            "destination":{"icao_code":"PHNL","pos_lat":"21.32","pos_long":"-157.92"},
            "atc":{"callsign":"DKS42"},"general":{"route":"DCT SLI DCT"},
            "navlog":{"fix":[{"ident":"SLI","type":"vor","pos_lat":"33.78","pos_long":"-118.05"},
            {"ident":"BEBOP","type":"wpt","pos_lat":"30.0","pos_long":"-130.0"},
            {"ident":"PHNL","type":"apt","pos_lat":"21.32","pos_long":"-157.92"}]}}"#;
        let j = parse_json(body).unwrap();
        let fp = from_simbrief(&j, 950.0, 6.0, false).unwrap();
        let ids: Vec<_> = fp.wpts.iter().map(|w| w.ident.as_str()).collect();
        println!("{ids:?} {:.0} NM jp7 {:.0} h2 {:.0}", fp.total() / NM, fp.jp7_req, fp.h2_req);
        assert_eq!(ids, vec!["KLAX", "SLI", "BEBOP", "PHNL"]);
        assert_eq!(fp.flt, "DKS42");
    }

    #[test]
    fn dispatch_profile() {
        let fp = dispatch("KPMD", "PHNL", 950.0, 6.0).unwrap();
        for w in &fp.wpts {
            println!("{:7} {:10} M{:.1} FL{:.0}", w.ident, w.phase, w.mach, w.alt_ft / 100.0);
        }
        assert!(fp.wpts.iter().any(|w| w.phase == "CRZ SCRAM"));
        assert!(fp.wpts.iter().any(|w| w.ident == "T/D"));
        assert!(fp.h2_req > 0.0 && fp.ete > 0.0);
        assert_eq!(parse_point("N3530.0W11800.0").map(|p| (p.1 * 10.0).round()), Some(355.0));
    }

    /// Guidage NAV : l'avion suit le plan et séquence les points.
    #[test]
    fn lnav_sequences() {
        let mut f = Fms::new();
        f.dispatch_now("KPMD", "KSFO", 650.0, 3.0).unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 35_000.0 * FT;
        ac.tas = 0.9 * crate::atmosphere::isa(ac.alt).a;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.8;
        ac.ap.on = true;
        ac.ap.vert = VertMode::Alt;
        ac.ap.alt_tgt_ft = 35_000.0;
        ac.ap.lat = LatMode::Nav;
        let mut seq = Vec::new();
        for _ in 0..(3_000 * 10) {
            if let Some((b, m)) = f.guidance(&ac) {
                ac.ap.hdg_tgt = b;
                if let Some(m) = m {
                    seq.push(m);
                }
            }
            for _ in 0..12 {
                ac.step(1.0 / 120.0, 0.0, 0.0);
            }
        }
        let fp = f.fpln.as_ref().unwrap();
        let (d, _) = dist_brg(ac.lat, ac.lon, fp.wpts.last().unwrap().lat, fp.wpts.last().unwrap().lon);
        println!("{seq:?} ; reste {:.0} NM", d / NM);
        assert!(seq.len() >= 2 && d < 40.0 * NM);
    }

    /// VNAV de bout en bout : KPMD → PHNL, montée turbines, stato, scram, croisière M6 FL950,
    /// descente à q constant, rallumage des turbines, autoland.
    #[test]
    fn vnav_full_flight() {
        let mut f = Fms::new();
        f.dispatch_now("KPMD", "PHNL", 950.0, 6.0).unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 15_000.0 * FT;
        ac.tas = 0.7 * crate::atmosphere::isa(ac.alt).a;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.8;
        ac.ap.on = true;
        ac.ap.lat = LatMode::Nav;
        let dt = 1.0 / 120.0;
        let (mut t, mut mmax, mut altmax, mut lp) = (0.0, 0.0f64, 0.0f64, -1e9);
        let mut log = Vec::new();
        while t < 3.0 * 3600.0 && ac.crashed.is_none() && !(ac.on_ground && ac.tas < 3.0) {
            if (t * 10.0f64).fract() < dt * 10.0 {
                if let Some((b, _)) = f.guidance(&ac) {
                    ac.ap.hdg_tgt = b;
                }
                for o in f.vnav_update(&mut ac) {
                    match o {
                        McduOut::Cmd(c) => {
                            if let Some(code) = c.strip_prefix("ATTERRIR ") {
                                let i = AIRPORTS.iter().position(|a| a.icao == code).unwrap();
                                let r = ac.engage_autoland(i);
                                log.push(format!("{t:.0}s {c} -> {r:?}"));
                            }
                        }
                        McduOut::Msg(_, m) => log.push(format!("{t:.0}s {m}")),
                    }
                }
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            for (_, m) in ac.events.drain(..) {
                if m.contains("STATO") || m.contains("SCRAM") || m.contains("TURBINE") || m.contains("TOUCHER") || m.contains("AUTOLAND") || m.contains("PALIER") {
                    log.push(format!("{t:.0}s {m}"));
                }
            }
            mmax = mmax.max(ac.d.mach);
            altmax = altmax.max(ac.alt / FT);
            if t - lp > 600.0 {
                lp = t;
                log.push(format!("{t:.0}s M{:.2} FL{:.0} mode {} reste {:.0} NM", ac.d.mach, ac.alt / FT / 100.0, ac.mode_name(), f.fpln.as_ref().unwrap().remaining(ac.lat, ac.lon) / NM));
            }
            t += dt;
        }
        for l in &log {
            println!("{l}");
        }
        println!("crash {:?} sol {} M max {mmax:.2} FL max {:.0}", ac.crashed, ac.on_ground, altmax / 100.0);
        assert!(mmax > 5.5 && altmax > 85_000.0, "croisiere scram atteinte");
        assert!(ac.crashed.is_none() && ac.on_ground, "pose a destination");
    }

    /// OFP SimBrief réel (fichier fourni par DARKSTAR_OFP) : import sans erreur.
    #[test]
    fn real_ofp_file() {
        let Ok(path) = std::env::var("DARKSTAR_OFP") else { return };
        let body = std::fs::read_to_string(path).unwrap();
        let fp = from_simbrief(&parse_json(&body).unwrap(), 950.0, 6.0, false).unwrap();
        println!("{}-{} RWY {} / {} SID {} STAR {} : {} points", fp.origin, fp.dest, fp.dep_rwy, fp.arr_rwy, fp.sid, fp.star, fp.wpts.len());
    }

    /// OFP IFR avec SID et STAR : procédures reconnues, T/D avant l'entrée de STAR.
    #[test]
    fn simbrief_sid_star() {
        let body = r#"{"fetch":{"status":"Success"},"origin":{"icao_code":"KLAX","pos_lat":"33.94","pos_long":"-118.40","plan_rwy":"25R"},
            "destination":{"icao_code":"KSFO","pos_lat":"37.62","pos_long":"-122.37","plan_rwy":"28L"},
            "navlog":{"fix":[
            {"ident":"GARDY","type":"wpt","stage":"CLB","is_sid_star":"1","via_airway":"ORCKA5","altitude_feet":"6000","pos_lat":"33.90","pos_long":"-118.70"},
            {"ident":"ORCKA","type":"wpt","stage":"CLB","is_sid_star":"1","via_airway":"ORCKA5","altitude_feet":"15000","pos_lat":"34.20","pos_long":"-119.20"},
            {"ident":"AVE","type":"vor","stage":"CRZ","is_sid_star":"0","via_airway":"J1","altitude_feet":"37000","pos_lat":"35.65","pos_long":"-119.98"},
            {"ident":"SERFR","type":"wpt","stage":"DSC","is_sid_star":"1","via_airway":"SERFR4","altitude_feet":"19000","pos_lat":"36.60","pos_long":"-121.70"},
            {"ident":"EPICK","type":"wpt","stage":"DSC","is_sid_star":"1","via_airway":"SERFR4","altitude_feet":"10000","pos_lat":"37.05","pos_long":"-122.00"}]}}"#;
        let fp = from_simbrief(&parse_json(body).unwrap(), 650.0, 3.0, false).unwrap();
        for w in &fp.wpts {
            println!("{:6} {:6} {:5} proc {} alt {:.0}", w.ident, w.via, w.phase, w.proc, w.alt_ft);
        }
        assert_eq!((fp.sid.as_str(), fp.star.as_str(), fp.dep_rwy.as_str(), fp.arr_rwy.as_str()), ("ORCKA5", "SERFR4", "25R", "28L"));
        assert_eq!(fp.wpts.iter().filter(|w| w.proc == 1).count(), 2);
        assert_eq!(fp.wpts[4].alt_ft, 19_000.0);
        let star_d = dist_brg(36.60, -121.70, 37.05, -122.00).0 + dist_brg(37.05, -122.00, 37.62, -122.37).0;
        assert!(fp.tod() > star_d);
    }

    /// MCDU : DEPARTURES (piste, SID, transition) et ARRIVAL (ILS, STAR, transition) par les
    /// touches de ligne, ILS calé au départ puis à destination, vol complet posé sur la piste choisie.
    #[test]
    fn mcdu_dep_arr_and_flight() {
        let mut f = Fms::new();
        f.dispatch_now("KEDW", "KSFO", 350.0, 0.9).unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        // DEPARTURES : LSK1G sur l'aéroport de départ
        f.page = Page::Fpln;
        f.scroll = 0;
        f.lsk(true, 0, &ac);
        assert_eq!(f.page, Page::Dep);
        f.lsk(true, 1, &ac); // première piste
        f.lsk(false, 1, &ac); // première SID
        f.lsk(false, 2, &ac); // première transition
        let r = f.lsk(false, 5, &ac); // INSERT
        assert!(matches!(r, Some(McduOut::Msg(..))), "{}", f.msg);
        let p = f.fpln.as_ref().unwrap();
        println!("depart : {} {} ; {:?}", p.dep_rwy, p.sid, p.wpts.iter().map(|w| w.ident.as_str()).collect::<Vec<_>>());
        assert!(!p.sid.is_empty() && p.wpts.iter().filter(|w| w.proc == 1).count() >= 3);
        // ARRIVAL : LSK sur la destination (dernière ligne visible)
        let n = p.wpts.len();
        f.page = Page::Fpln;
        f.scroll = n.saturating_sub(5);
        f.lsk(true, n - 1 - f.scroll, &ac);
        assert_eq!(f.page, Page::Arr);
        // Piste 28L (ou la première proposée), première STAR, sans transition
        let sfo = crate::procs::airport("KSFO").unwrap();
        let ends = crate::procs::ends(sfo);
        let k = ends.iter().position(|e| e.ident == "30L").unwrap_or(0);
        f.scroll = k;
        f.lsk(true, 1, &ac);
        f.scroll = 0;
        f.lsk(false, 1, &ac);
        f.lsk(false, 1, &ac); // NO TRANS
        f.lsk(false, 5, &ac);
        let p = f.fpln.as_ref().unwrap();
        println!("arrivee : {} {} ; {:?}", p.arr_rwy, p.star, p.wpts.iter().map(|w| format!("{}:{}", w.ident, w.proc)).collect::<Vec<_>>());
        assert!(p.wpts.iter().any(|w| w.proc == 3) && !p.star.is_empty());
        let arr_rwy = p.arr_rwy.clone();
        // ILS : départ au sol, puis destination
        let m = f.autotune(&ac).unwrap();
        println!("{m}");
        assert!(f.ils.as_ref().unwrap().apt == "KEDW");
        // Vol complet : décollage déjà fait (en vol au-dessus d'Edwards), LNAV + VNAV, autoland
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        let edw = crate::procs::airport("KEDW").unwrap();
        ac.lat = edw.lat;
        ac.lon = edw.lon;
        ac.alt = edw.elev + 1_500.0 * FT;
        ac.tas = 120.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Nav;
        f.fpln.as_mut().unwrap().active = 1;
        ac.rwy_pref = arr_rwy.clone();
        let dt = 1.0 / 120.0;
        let mut t = 0.0;
        let mut tuned_dest = false;
        let mut landed_rwy = String::new();
        while t < 4_000.0 && ac.crashed.is_none() && !(ac.on_ground && ac.tas < 3.0) {
            if (t * 10.0f64).fract() < dt * 10.0 {
                if let Some((b, _)) = f.guidance(&ac) {
                    if ac.autoland.is_none() {
                        ac.ap.hdg_tgt = b;
                    }
                }
                if let Some(m) = f.autotune(&ac) {
                    tuned_dest |= m.contains("KSFO");
                }
                for o in f.vnav_update(&mut ac) {
                    if let McduOut::Cmd(c) = o {
                        if let Some(code) = c.strip_prefix("ATTERRIR ") {
                            let i = AIRPORTS.iter().position(|a| a.icao == code).unwrap();
                            ac.engage_autoland(i).unwrap();
                            landed_rwy = ac.autoland.as_ref().unwrap().rwy.clone();
                        }
                    }
                }
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            ac.events.clear();
            t += dt;
        }
        println!("ILS destination cale {tuned_dest}, autoland piste {landed_rwy}, sol {} crash {:?} t {t:.0}", ac.on_ground, ac.crashed);
        assert!(tuned_dest && landed_rwy == arr_rwy && ac.on_ground && ac.crashed.is_none());
    }

    /// Croisière MAX / Mach 11 : KPMD → EGLL, plafond suivant le couloir de q, posé à Londres.
    #[test]
    fn vnav_mach11_max() {
        let mut f = Fms::new();
        f.crz_max = true;
        f.dispatch_now("KPMD", "EGLL", ceiling_fl(11.0), 11.0).unwrap();
        println!("plafond M11 : FL{:.0}", ceiling_fl(11.0));
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 15_000.0 * FT;
        ac.tas = 0.7 * crate::atmosphere::isa(ac.alt).a;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.8;
        ac.ap.on = true;
        ac.ap.lat = LatMode::Nav;
        let dt = 1.0 / 60.0;
        let (mut t, mut mmax, mut altmax, mut lp) = (0.0, 0.0f64, 0.0f64, -1e9);
        while t < 4.0 * 3600.0 && ac.crashed.is_none() && !(ac.on_ground && ac.tas < 3.0) {
            if (t * 10.0f64).fract() < dt * 10.0 {
                if let Some((b, _)) = f.guidance(&ac) {
                    if ac.autoland.is_none() {
                        ac.ap.hdg_tgt = b;
                    }
                }
                for o in f.vnav_update(&mut ac) {
                    if let McduOut::Cmd(c) = o {
                        if let Some(code) = c.strip_prefix("ATTERRIR ") {
                            let i = AIRPORTS.iter().position(|a| a.icao == code).unwrap();
                            let _ = ac.engage_autoland(i);
                        }
                    }
                }
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            ac.events.clear();
            mmax = mmax.max(ac.d.mach);
            altmax = altmax.max(ac.alt / FT);
            if t - lp > 600.0 {
                lp = t;
                println!("{t:5.0}s M{:.2} FL{:.0} q {:.0} kPa peau {:.0}C H2 {:.0} JP7 {:.0} reste {:.0} NM", ac.d.mach, ac.alt / FT / 100.0, ac.d.q / 1000.0, ac.skin_temp - 273.15, ac.h2, ac.fuel, f.fpln.as_ref().unwrap().remaining(ac.lat, ac.lon) / NM);
            }
            t += dt;
        }
        println!("crash {:?} sol {} M max {mmax:.2} FL max {:.0} t {t:.0}", ac.crashed, ac.on_ground, altmax / 100.0);
        assert!(mmax > 10.8 && altmax > 115_000.0);
        assert!(ac.crashed.is_none() && ac.on_ground);
    }

    /// Hydrogène insuffisant (7,5 t) pour KPMD → EGLL : l'ordinateur de bord le détecte à l'allumage
    /// du scram et demande un déroutement.
    #[test]
    fn eco_mach_low_h2() {
        let mut f = Fms::new();
        f.dispatch_now("KPMD", "EGLL", ceiling_fl(11.0), 11.0).unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 15_000.0 * FT;
        ac.tas = 0.7 * crate::atmosphere::isa(ac.alt).a;
        ac.h2 = 7_500.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.8;
        ac.ap.on = true;
        ac.ap.lat = LatMode::Nav;
        let dt = 1.0 / 60.0;
        let (mut t, mut mmax) = (0.0, 0.0f64);
        let mut msgs = Vec::new();
        while t < 3600.0 && ac.crashed.is_none() && !msgs.iter().any(|m: &String| m.contains("ATTEINTE")) {
            if (t * 10.0f64).fract() < dt * 10.0 {
                if let Some((b, _)) = f.guidance(&ac) {
                    if ac.autoland.is_none() {
                        ac.ap.hdg_tgt = b;
                    }
                }
                for o in f.vnav_update(&mut ac) {
                    match o {
                        McduOut::Cmd(c) => {
                            if let Some(code) = c.strip_prefix("ATTERRIR ") {
                                let i = AIRPORTS.iter().position(|a| a.icao == code).unwrap();
                                let _ = ac.engage_autoland(i);
                            }
                        }
                        McduOut::Msg(_, m) => msgs.push(format!("{t:.0}s {m}")),
                    }
                }
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            ac.events.clear();
            mmax = mmax.max(ac.d.mach);
            t += dt;
        }
        for m in msgs.iter().filter(|m| m.contains("CROISIERE A") || m.contains("ATTEINTE") || m.contains("INSUFFISANT")) {
            println!("{m}");
        }
        println!("eco {:?} M max {mmax:.2} H2 restant {:.0} JP7 restant {:.0} sol {} crash {:?} t {t:.0}", f.eco_mach, ac.h2, ac.fuel, ac.on_ground, ac.crashed);
        assert!(msgs.iter().any(|m| m.contains("HORS D'ATTEINTE")), "deroutement demande");
    }

    #[test]
    #[ignore]
    fn probe_h2_rates() {
        for m in [5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0] {
            let mut ac = Aircraft::new_on_runway(0);
            ac.on_ground = false;
            ac.gear_down = false;
            ac.gear_pos = 0.0;
            ac.brakes = false;
            ac.alt = alt_for_q(m, 35_000.0) * FT;
            ac.tas = m * crate::atmosphere::isa(ac.alt).a;
            ac.set_turbines(TurbState::Off, 0.0);
            ac.eng.inlet_open = false;
            ac.eng.ram = RamState::Lit;
            ac.eng.scram = ScramState::Lit;
            ac.ap.on = true;
            ac.ap.vert = VertMode::Alt;
            ac.ap.alt_tgt_ft = ac.alt / FT;
            ac.ap.at_on = true;
            ac.ap.mach_tgt = m;
            ac.throttle = 0.6;
            let h0 = ac.h2;
            let j0 = ac.fuel;
            for _ in 0..(300 * 60) {
                ac.step(1.0 / 60.0, 0.0, 0.0);
            }
            let h1 = ac.h2;
            let j1 = ac.fuel;
            for _ in 0..(300 * 60) {
                ac.step(1.0 / 60.0, 0.0, 0.0);
            }
            println!("M{m:.0} (M{:.2}) FL{:.0} : H2 {:.2} kg/s  JP7 {:.2} kg/s thr {:.2} mode {}", ac.d.mach, ac.alt / FT / 100.0, (h1 - ac.h2) / 300.0, (j1 - ac.fuel) / 300.0, ac.throttle, ac.mode_name());
            let _ = (h0, j0);
        }
    }

    /// Rejeu du vol de l'utilisateur : OFP SimBrief réel (DARKSTAR_OFP), MAX/11, LNAV + VNAV.
    #[test]
    #[ignore]
    fn replay_ofp_flight() {
        let Ok(path) = std::env::var("DARKSTAR_OFP") else { return };
        let body = std::fs::read_to_string(path).unwrap();
        let mut f = Fms::new();
        let fp = from_simbrief(&parse_json(&body).unwrap(), ceiling_fl(11.0), 11.0, true).unwrap();
        let dest = fp.dest.clone();
        f.fpln = Some(fp);
        f.arm_vnav();
        let org = crate::procs::airport(&f.fpln.as_ref().unwrap().origin).unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.lat = org.lat;
        ac.lon = org.lon;
        ac.alt = org.elev + 15_000.0 * FT;
        ac.tas = 0.7 * crate::atmosphere::isa(ac.alt).a;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Nav;
        let dt = 1.0 / 60.0;
        let (mut t, mut lp) = (0.0, -1e9);
        while t < 4.0 * 3600.0 && ac.crashed.is_none() && !(ac.on_ground && ac.tas < 3.0) {
            if (t * 10.0f64).fract() < dt * 10.0 {
                if let Some((b, _)) = f.guidance(&ac) {
                    if ac.autoland.is_none() {
                        ac.ap.hdg_tgt = b;
                    }
                }
                for o in f.vnav_update(&mut ac) {
                    match o {
                        McduOut::Cmd(c) => {
                            if let Some(code) = c.strip_prefix("ATTERRIR ") {
                                if let Some(i) = AIRPORTS.iter().position(|a| a.icao == code) {
                                    let _ = ac.engage_autoland(i);
                                }
                            }
                        }
                        McduOut::Msg(_, m) => println!("{t:5.0}s {m}"),
                    }
                }
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            for (_, m) in ac.events.drain(..) {
                if m.contains("UNSTART") || m.contains("FLAMEOUT") || m.contains("TOUCHER") {
                    println!("{t:5.0}s {m}");
                }
            }
            if t - lp > 600.0 {
                lp = t;
                println!("{t:5.0}s M{:.2} FL{:.0} {} H2 {:.0} JP7 {:.0} reste {:.0} NM", ac.d.mach, ac.alt / FT / 100.0, ac.mode_name(), ac.h2, ac.fuel, f.fpln.as_ref().unwrap().remaining(ac.lat, ac.lon) / NM);
            }
            t += dt;
        }
        println!("{dest} : crash {:?} sol {} JP7 {:.0} H2 {:.0} t {t:.0}", ac.crashed, ac.on_ground, ac.fuel, ac.h2);
    }
}
