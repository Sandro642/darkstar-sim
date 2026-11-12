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

#[derive(Clone, Debug)]
pub struct Wpt {
    pub ident: String,
    pub lat: f64,
    pub lon: f64,
    /// Contrainte / prévision d'altitude (ft) et de Mach
    pub alt_ft: f64,
    pub mach: f64,
    /// Phase du profil Darkstar au point
    pub phase: &'static str,
}

#[derive(Clone)]
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
}

/// Distance de descente (m) : descente à q constant (~(1 + M/2) NM par 1000 ft, mesuré en vol
/// simulé) puis descente finale subsonique (~80 NM).
pub fn tod_dist(crz_fl: f64, crz_mach: f64) -> f64 {
    ((crz_fl / 10.0) * (1.0 + 0.5 * crz_mach) + 80.0) * NM
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
                    "CRZ SCRAM" => h2 += 1.6 * dt,
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
            } else {
                w.phase = ph;
                w.mach = m;
                w.alt_ft = fl * 100.0;
            }
        }
        self.ete = t;
        // Réserves : 15 % + 3 t JP-7 (approche, dégagement), 10 % H2
        self.jp7_req = jp * 1.15 + 3_000.0;
        self.h2_req = h2 * 1.10;
    }

    /// Point de début de descente (distance à destination, m).
    pub fn tod(&self) -> f64 {
        tod_dist(self.crz_fl, self.crz_mach)
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
    let mut fp = Fpln { origin: from.into(), dest: to.into(), wpts: Vec::new(), active: 1, source: "DISPATCH", flt: "DKS01".into(), crz_fl, crz_mach, jp7_req: 0.0, h2_req: 0.0, ete: 0.0 };
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
    fp.wpts.push(Wpt { ident: from.into(), lat: a.lat, lon: a.lon, alt_ft: 0.0, mach: 0.0, phase: "DEP" });
    // Orthodromie : point à la distance x sur l'arc a→b (relèvement initial)
    for (x, name) in marks {
        let (la, lo) = destination(a.lat, a.lon, brg, x);
        // Correction : on se recale vers b le long de l'orthodromie
        let (_, b2) = dist_brg(la, lo, b.lat, b.lon);
        let _ = b2;
        fp.wpts.push(Wpt { ident: name, lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "" });
    }
    fp.wpts.push(Wpt { ident: to.into(), lat: b.lat, lon: b.lon, alt_ft: 0.0, mach: 0.0, phase: "ARR" });
    fp.compute();
    Ok(fp)
}

/// Plan depuis un OFP SimBrief (JSON).
pub fn from_simbrief(j: &Json, crz_mach: f64) -> Result<Fpln, String> {
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
    let mut wpts = vec![Wpt { ident: oi.clone(), lat: ola, lon: olo, alt_ft: 0.0, mach: 0.0, phase: "DEP" }];
    if let Some(fixes) = j.path(&["navlog", "fix"]) {
        for f in fixes.items() {
            let ident = f.get("ident").map(|x| x.s()).unwrap_or_default();
            let typ = f.get("type").map(|x| x.s()).unwrap_or_default();
            let (Some(la), Some(lo)) = (f.get("pos_lat").and_then(|x| x.f()), f.get("pos_long").and_then(|x| x.f())) else { continue };
            if ident.is_empty() || ident == di || ident == oi || typ == "apt" {
                continue;
            }
            // Pas de points trop rapprochés : inutiles à Mach 6
            if let Some(l) = wpts.last() {
                if dist_brg(l.lat, l.lon, la, lo).0 < 15.0 * NM {
                    continue;
                }
            }
            wpts.push(Wpt { ident: ident.chars().take(7).collect(), lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "" });
        }
    }
    wpts.push(Wpt { ident: di.clone(), lat: dla, lon: dlo, alt_ft: 0.0, mach: 0.0, phase: "ARR" });
    let flt = j.path(&["atc", "callsign"]).map(|x| x.s()).filter(|s| !s.is_empty()).unwrap_or("DKS01".into());
    // Le niveau de croisière SimBrief (avion de ligne) est remplacé par le profil Darkstar
    let mut fp = Fpln { origin: oi, dest: di, wpts, active: 1, source: "SIMBRIEF", flt, crz_fl: if crz_mach > 4.8 { 950.0 } else { 650.0 }, crz_mach, jp7_req: 0.0, h2_req: 0.0, ete: 0.0 };
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
    fetch: Arc<Mutex<Fetch>>,
    /// Saisie clavier dirigée vers le MCDU
    pub focus: bool,
    /// Gestion verticale et de la vitesse par le FMS (VNAV + Mach géré + transitions moteur)
    pub vnav: bool,
    desc_started: bool,
    land_sent: bool,
    /// Dernière phase annoncée
    phase: &'static str,
}

pub enum McduOut {
    /// Instruction à exécuter par la station sol (ALT, MACH, DESCENTE, DEST…)
    Cmd(String),
    Msg(Level, String),
}

impl Fms {
    /// Plan DARKSTAR DISPATCH calculé et activé directement (instruction DISPATCH).
    pub fn dispatch_now(&mut self, from: &str, to: &str, fl: f64, m: f64) -> Result<String, String> {
        let fp = dispatch(from, to, fl, m)?;
        let msg = format!("DISPATCH {from}-{to} : {} POINTS, {:.0} NM, {} — JP-7 {:.1} t, H2 {:.1} t", fp.wpts.len(), fp.total() / NM, fmt_eta(fp.ete), fp.jp7_req / 1000.0, fp.h2_req / 1000.0);
        self.from_to = (from.into(), to.into());
        self.crz = (fl, m);
        self.fpln = Some(fp);
        self.temp = None;
        self.arm_vnav();
        Ok(msg)
    }

    fn arm_vnav(&mut self) {
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
        let mach = ac.d.mach;
        let e = ac.eng.clone();
        let turb_on = e.side.iter().any(|s| matches!(s, TurbState::Running | TurbState::Starting));
        let say = |m: &mut Vec<McduOut>, ph: &mut &'static str, p: &'static str, txt: String| {
            if *ph != p {
                *ph = p;
                m.push(McduOut::Msg(Level::Info, txt));
            }
        };
        if rem > tod && !self.desc_started {
            // ----- Montée / accélération / croisière -----
            ac.ap.at_on = true;
            ac.ap.spd_keas = None;
            ac.ap.spd_managed = false;
            ac.ap.mach_tgt = crz_m;
            let sched = if crz_m <= 2.5 {
                crz_ft
            } else if e.scram == ScramState::Lit {
                if mach < 6.0 { 88_000.0 } else if mach < 8.0 { 98_000.0 } else { 108_000.0 }
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
            ac.ap.alt_tgt_ft = sched.min(crz_ft);
            if crz_m > 2.5 {
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
            if ac.ap.vert != VertMode::QHold {
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

    pub fn new() -> Fms {
        let user = std::fs::read_to_string(config_path()).map(|s| s.trim().to_string()).unwrap_or_default();
        Fms { fpln: None, temp: None, page: Page::Init, scroll: 0, scratch: String::new(), msg: String::new(), user, from_to: (String::new(), String::new()), crz: (950.0, 6.0), fetch: Arc::new(Mutex::new(Fetch::Idle)), focus: true, vnav: true, desc_started: false, land_sent: false, phase: "" }
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
        let url = format!("https://www.simbrief.com/api/xml.fetcher.php?username={}&json=1", self.user.to_lowercase());
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
        let url = format!("https://www.simbrief.com/system/dispatch.php?orig={o}&dest={d}&type=CONC&fl={:.0}&callsign=DKS01", self.crz.0 * 100.0);
        let ok = std::process::Command::new("/usr/bin/open").arg(&url).spawn().is_ok();
        self.msg = if ok { "SIMBRIEF OUVERT : GENERER PUIS IMPORT".into() } else { "NAVIGATEUR INDISPONIBLE".into() };
    }

    /// Suivi du téléchargement ; à appeler chaque image.
    pub fn poll(&mut self) -> Option<McduOut> {
        let st = self.fetch.lock().unwrap().clone();
        if let Fetch::Done(r) = st {
            *self.fetch.lock().unwrap() = Fetch::Idle;
            let res = r.and_then(|body| parse_json(&body)).and_then(|j| from_simbrief(&j, self.crz.1));
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
                // « 950/6.0 » ou « FL950/M6 »
                let t = sp.replace("FL", "").replace('M', "");
                if let Some((a, b)) = t.split_once('/') {
                    if let (Ok(fl), Ok(m)) = (a.parse::<f64>(), b.parse::<f64>()) {
                        if (100.0..=1_100.0).contains(&fl) && (0.5..=9.5).contains(&m) {
                            self.crz = (fl, m);
                            if let Some(p) = self.plan_mut() {
                                p.crz_fl = fl;
                                p.crz_mach = m;
                                p.compute();
                            }
                            self.scratch.clear();
                        } else {
                            self.msg = "HORS DOMAINE".into();
                        }
                    }
                } else {
                    self.msg = "FORMAT : 950/6.0".into();
                }
            }
            (Page::Init, false, 1) => self.start_fetch(),
            (Page::Init, false, 2) => self.open_simbrief(),
            (Page::Init, true, 3) => {
                let (o, d) = self.from_to.clone();
                match dispatch(&o, &d, self.crz.0, self.crz.1) {
                    Ok(fp) => {
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
                                    p.wpts.insert(idx, Wpt { ident: id, lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "" });
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
                            p.wpts.insert(at, Wpt { ident: id.clone(), lat: la, lon: lo, alt_ft: 0.0, mach: 0.0, phase: "" });
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
}

fn ln(l: impl Into<String>, lc: Color, r: impl Into<String>, rc: Color) -> Line {
    Line { l: l.into(), r: r.into(), lc, rc }
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
        Page::Init => {
            title = "INIT  DK-FMS".to_string();
            let (o, d) = &f.from_to;
            let ft = if o.is_empty() { "____/____".to_string() } else { format!("{o}/{d}") };
            let user = if f.user.is_empty() { "________".to_string() } else { f.user.to_uppercase() };
            rows.push((ln(" CO RTE", LBL_M, "SIMBRIEF ID ", LBL_M), ln(ft, if o.is_empty() { AMB_M } else { CYAN_M }, user, if f.user.is_empty() { AMB_M } else { CYAN_M })));
            let st = if f.busy() { "EN COURS" } else { "IMPORT OFP>" };
            rows.push((ln(" FLT NBR", LBL_M, "", LBL_M), ln(f.plan().map(|p| p.flt.clone()).unwrap_or("DKS01".into()), CYAN_M, st, WHITE_M)));
            rows.push((ln(" CRZ FL/MACH", LBL_M, "GENERER", LBL_M), ln(format!("FL{:.0}/M{:.1}", f.crz.0, f.crz.1), CYAN_M, "SIMBRIEF>", WHITE_M)));
            rows.push((ln(" CALCUL PLAN", LBL_M, "", LBL_M), ln("<DARKSTAR DISPATCH", WHITE_M, "", WHITE_M)));
            let info = match f.plan() {
                Some(p) => format!("{} {}-{} {:.0}NM", p.source, p.origin, p.dest, p.total() / NM),
                None => "AUCUN PLAN".into(),
            };
            rows.push((ln(" PLAN", LBL_M, "", LBL_M), ln(info, if f.plan().is_some() { GREEN_M } else { LBL_M }, "", WHITE_M)));
            rows.push((blank(), if f.temp.is_some() { ln("<F-PLN", WHITE_M, "ACTIVATE*", AMB_M) } else { ln("<F-PLN", WHITE_M, "", WHITE_M) }));
        }
        Page::Fpln => {
            let tmp = f.temp.is_some();
            title = if tmp { "TMPY F-PLN".into() } else { "F-PLN".into() };
            tc = if tmp { AMB_M } else { WHITE_M };
            match f.plan() {
                None => {
                    rows.push((blank(), ln("PAS DE PLAN", AMB_M, "", WHITE_M)));
                }
                Some(p) => {
                    let mut prev: Option<&Wpt> = None;
                    if f.scroll > 0 {
                        prev = p.wpts.get(f.scroll - 1);
                    }
                    for i in f.scroll..(f.scroll + 5).min(p.wpts.len()) {
                        let w = &p.wpts[i];
                        let (dd, brg) = match prev {
                            Some(q) => dist_brg(q.lat, q.lon, w.lat, w.lon),
                            None => (0.0, 0.0),
                        };
                        let act = !tmp && i == p.active;
                        let col = if tmp { AMB_M } else if act { MAG_M } else if i < p.active { LBL_M } else { GREEN_M };
                        let lab = if i == 0 { format!(" {}", w.phase) } else { format!(" C{:03.0} {:.0}NM {}", brg, dd / NM, w.phase) };
                        let cons = if w.mach > 1.0 { format!("M{:.1}/FL{:.0}", w.mach, w.alt_ft / 100.0) } else if w.alt_ft > 0.0 && i > 0 && i + 1 < p.wpts.len() { format!("M.{:.0}/FL{:.0}", w.mach * 100.0, w.alt_ft / 100.0) } else { format!("{:.0}", w.alt_ft) };
                        rows.push((ln(lab, LBL_M, "", LBL_M), ln(w.ident.clone(), col, cons, if act { MAG_M } else { CYAN_M })));
                        prev = Some(w);
                    }
                    while rows.len() < 5 {
                        rows.push((blank(), blank()));
                    }
                    let rem = if tmp { p.total() } else { p.remaining(ac.lat, ac.lon) };
                    let dest = format!("{} {:.0}NM {}", p.dest, rem / NM, fmt_eta(p.ete));
                    rows.push((ln(" DEST   DIST  TEMPS", LBL_M, "", LBL_M), if tmp { ln("<EFFACER", AMB_M, "ACTIVATE*", AMB_M) } else { ln(dest, WHITE_M, "", WHITE_M) }));
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
}

/// MCDU complet : écran 24 colonnes x 14 lignes, LSK, touches de page, ligne de saisie.
pub fn draw_mcdu(f: &Fms, ac: &Aircraft, r: Rect, time: f64) -> Option<McduKey> {
    let mut out = None;
    panel(r, "MCDU  DK-FMS");
    // Boîtier
    let bw = (r.w - 24.0).min(520.0);
    let b = Rect::new(r.x + (r.w - bw) / 2.0, r.y + 28.0, bw, r.h - 36.0);
    rrect(b, 10.0, Color::new(0.16, 0.17, 0.18, 1.0));
    rrect_lines(b, 10.0, 1.5, Color::new(0.3, 0.32, 0.34, 1.0));
    let lsk_w = 34.0;
    let scr = Rect::new(b.x + lsk_w + 14.0, b.y + 12.0, b.w - 2.0 * (lsk_w + 14.0), (b.h * 0.58).min(330.0));
    draw_rectangle(scr.x, scr.y, scr.w, scr.h, Color::new(0.01, 0.02, 0.02, 1.0));
    draw_rectangle_lines(scr.x, scr.y, scr.w, scr.h, 2.0, Color::new(0.05, 0.06, 0.06, 1.0));
    if clicked(scr) {
        out = Some(McduKey::Focus);
    }
    let rowh = scr.h / 14.0;
    let fs = (rowh * 0.95).min(18.0);
    let small = fs * 0.75;
    let (title, tc, rows) = page_lines(f, ac);
    txt_c(&title, scr.x + scr.w / 2.0, scr.y + rowh * 0.85, fs, tc);
    for (k, (lab, dat)) in rows.iter().enumerate().take(6) {
        let yl = scr.y + rowh * (2.0 * k as f32 + 1.85);
        let yd = yl + rowh;
        txt(&lab.l, scr.x + 4.0, yl, small, lab.lc);
        txt_r(&lab.r, scr.x + scr.w - 4.0, yl, small, lab.rc);
        txt(&dat.l, scr.x + 4.0, yd, fs, dat.lc);
        txt_r(&dat.r, scr.x + scr.w - 4.0, yd, fs, dat.rc);
        // LSK
        for (left, x) in [(true, b.x + 8.0), (false, b.x + b.w - 8.0 - lsk_w)] {
            let kr = Rect::new(x, yd - rowh * 0.8, lsk_w, rowh * 0.95);
            rrect(kr, 3.0, if hovered(kr) { Color::new(0.38, 0.4, 0.42, 1.0) } else { Color::new(0.28, 0.29, 0.3, 1.0) });
            draw_line(if left { kr.x + kr.w } else { kr.x }, kr.y + kr.h / 2.0, if left { kr.x + kr.w + 8.0 } else { kr.x - 8.0 }, kr.y + kr.h / 2.0, 2.0, Color::new(0.5, 0.52, 0.54, 1.0));
            if clicked(kr) {
                out = Some(McduKey::Lsk(left, k));
            }
        }
    }
    // Ligne de saisie / message
    let ys = scr.y + rowh * 13.7;
    let cursor = if f.focus && (time * 2.0).fract() < 0.5 { "_" } else { "" };
    if !f.msg.is_empty() && f.scratch.is_empty() {
        txt(&f.msg, scr.x + 4.0, ys, fs, AMB_M);
    } else {
        txt(&format!("{}{cursor}", f.scratch), scr.x + 4.0, ys, fs, WHITE_M);
    }
    let _ = COLS;
    // Touches de page
    let ky = scr.y + scr.h + 14.0;
    let keys: [(&str, Option<Page>); 8] = [("INIT", Some(Page::Init)), ("F-PLN", Some(Page::Fpln)), ("PERF", Some(Page::Perf)), ("PROG", Some(Page::Prog)), ("FUEL", Some(Page::Fuel)), ("DIR", Some(Page::Dir)), ("CLR", None), ("^ v", None)];
    let kw = (b.w - 24.0) / 4.0 - 6.0;
    for (i, (k, p)) in keys.iter().enumerate() {
        let (c, rw) = (i % 4, i / 4);
        let kr = Rect::new(b.x + 12.0 + c as f32 * (kw + 6.0), ky + rw as f32 * 36.0, kw, 30.0);
        let on = p.is_some_and(|p| p == f.page);
        rrect(kr, 4.0, if hovered(kr) { Color::new(0.3, 0.32, 0.34, 1.0) } else { Color::new(0.22, 0.23, 0.24, 1.0) });
        rrect_lines(kr, 4.0, 1.0, if on { CYAN_M } else { Color::new(0.4, 0.42, 0.44, 1.0) });
        txt_c(k, kr.x + kr.w / 2.0, kr.y + 20.0, 13.0, WHITE_M);
        if clicked(kr) {
            out = Some(match (p, *k) {
                (Some(p), _) => McduKey::Page(*p),
                (None, "CLR") => McduKey::Clr,
                _ => McduKey::Slew(mouse_position().0 > kr.x + kr.w / 2.0),
            });
        }
    }
    let hy = ky + 84.0;
    txt(if f.focus { "CLAVIER : SAISIE MCDU (ECHAP pour rendre le clavier au vol)" } else { "CLIC SUR L'ECRAN POUR SAISIR" }, b.x + 12.0, hy, 11.0, if f.focus { CYAN_M } else { LBL_M });
    txt("FROM/TO : KPMD/PHNL > LSK1G   SIMBRIEF ID > LSK1D   IMPORT OFP : LSK2D", b.x + 12.0, hy + 16.0, 10.0, LBL_M);
    txt("F-PLN : saisie + LSK = insertion, CLR + LSK = suppression, LSK seule = DIRECT", b.x + 12.0, hy + 30.0, 10.0, LBL_M);
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
        let fp = from_simbrief(&j, 6.0).unwrap();
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
}
