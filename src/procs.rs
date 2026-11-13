//! Procédures aux instruments générées (pas de base de navigation réelle embarquée) : pour chaque
//! piste, des SID vers quatre points de sortie (N, E, S, O) avec transitions, des STAR depuis
//! quatre points d'entrée vers l'IAF de l'approche, et une approche ILS par extrémité de piste
//! (IAF 15 NM, FAF 6 NM, seuil), avec fréquence et course ILS.

use crate::airports::{Airport, AIRPORTS};
use crate::fms::Wpt;
use crate::geo::*;

/// Extrémité de piste : (indice de piste, cap d'approche / de décollage vrai, désignation, longueur).
#[derive(Clone, Debug)]
pub struct RwyEnd {
    pub k: usize,
    pub hdg: f64,
    pub ident: String,
    pub len: f64,
}

/// Piste issue des données AIRAC de l'OFP SimBrief (rapport décollage / atterrissage) :
/// désignation magnétique, cap vrai, longueur (m), fréquence ILS (MHz, 0 si aucune).
#[derive(Clone, Debug)]
pub struct AiracRwy {
    pub apt: String,
    pub ident: String,
    pub true_course: f64,
    pub len_m: f64,
    pub ils_mhz: f64,
}

thread_local! {
    static AIRAC: std::cell::RefCell<Vec<AiracRwy>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Données de pistes AIRAC de l'OFP actif (origine et destination).
pub fn set_airac(v: Vec<AiracRwy>) {
    AIRAC.with(|c| *c.borrow_mut() = v);
}

/// Cap vrai AIRAC (sens « bas », 0-180°) le plus proche de `hdg` pour l'aérodrome, à ±20°.
pub fn airac_course(icao: &str, hdg: f64) -> Option<f64> {
    AIRAC.with(|c| {
        let v = c.borrow();
        v.iter()
            .filter(|r| r.apt == icao)
            .map(|r| {
                let low = if wrap180(r.true_course - hdg).abs() <= 90.0 { r.true_course } else { wrap360(r.true_course + 180.0) };
                (low, wrap180(low - hdg).abs())
            })
            .filter(|(_, d)| *d <= 20.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(c, _)| wrap360(c))
    })
}

/// Piste AIRAC correspondant à l'extrémité (même aérodrome, cap vrai à ±20°, même lettre L/C/R).
pub fn airac_for(apt: &Airport, hdg: f64, calc_ident: &str) -> Option<AiracRwy> {
    let letter: String = calc_ident.chars().filter(|c| c.is_alphabetic()).collect();
    AIRAC.with(|c| {
        let v = c.borrow();
        let mut cands: Vec<&AiracRwy> = v.iter().filter(|r| r.apt == apt.icao && wrap180(r.true_course - hdg).abs() <= 20.0).collect();
        if cands.len() > 1 && !letter.is_empty() {
            cands.retain(|r| r.ident.ends_with(&letter));
        }
        cands.first().map(|r| (*r).clone())
    })
}

pub fn ends(apt: &Airport) -> Vec<RwyEnd> {
    let mut v = Vec::new();
    for (k, r) in apt.runways().iter().enumerate() {
        for hdg in [r.hdg, wrap360(r.hdg + 180.0)] {
            let calc = apt.runway_ident(k, hdg);
            // Désignation magnétique réelle (AIRAC SimBrief) quand elle est connue
            let ident = airac_for(apt, hdg, &calc).map(|a| a.ident).unwrap_or(calc);
            v.push(RwyEnd { k, hdg, ident, len: r.len });
        }
    }
    v.sort_by(|a, b| a.ident.cmp(&b.ident));
    v
}

pub fn find_end(apt: &Airport, ident: &str) -> Option<RwyEnd> {
    ends(apt).into_iter().find(|e| e.ident == ident)
}

/// Seuil de l'extrémité (lat, lon).
pub fn threshold(apt: &Airport, e: &RwyEnd) -> (f64, f64) {
    apt.runways()[e.k].threshold(e.hdg)
}

fn tag(apt: &Airport) -> String {
    apt.icao.chars().skip(1).take(3).collect()
}

/// Fréquence ILS stable (108,10 à 111,95 MHz, dixièmes impairs) et indicatif.
pub fn ils(apt: &Airport, e: &RwyEnd) -> (String, f64) {
    // Fréquence ILS réelle (AIRAC SimBrief) si disponible
    if let Some(a) = AIRAC.with(|c| c.borrow().iter().find(|r| r.apt == apt.icao && r.ident == e.ident && r.ils_mhz > 100.0).cloned()) {
        return (format!("I{}", &tag(apt)), a.ils_mhz);
    }
    let h: u32 = apt.icao.bytes().chain(e.ident.bytes()).fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    // 40 canaux : 108 à 111 MHz, dixièmes impairs (.10 .15 .30 .35 ... .95)
    let idx = (h % 40) as usize;
    let tenth = [0.1, 0.3, 0.5, 0.7, 0.9][(idx % 10) / 2] + if idx % 2 == 1 { 0.05 } else { 0.0 };
    let mhz = 108.0 + (idx / 10) as f64 + tenth;
    (format!("I{}", &tag(apt)), mhz)
}

const DIRS: [(&str, f64); 4] = [("N", 0.0), ("E", 90.0), ("S", 180.0), ("W", 270.0)];

fn wpt(ident: String, ll: (f64, f64), proc: u8, via: &str, alt_ft: f64) -> Wpt {
    Wpt { ident, lat: ll.0, lon: ll.1, via: via.to_string(), proc, sb_alt: alt_ft, ..Default::default() }
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub name: String,
    pub pts: Vec<Wpt>,
}

/// SID de l'extrémité `e` : montée dans l'axe 5 NM (3000 ft sol), puis point de sortie à 40 NM.
pub fn sids(apt: &Airport, e: &RwyEnd) -> Vec<Proc> {
    let (tla, tlo) = threshold(apt, e);
    // Fin de piste puis 5 NM dans l'axe
    let der = destination(tla, tlo, e.hdg, e.len);
    let p5 = destination(der.0, der.1, e.hdg, 5.0 * NM);
    let elev = apt.elev / FT;
    let digit = (b'1' + (e.ident.bytes().fold(0u32, |a, b| a + b as u32) % 9) as u8) as char;
    DIRS.iter()
        .map(|(d, brg)| {
            let name = format!("{}{d}{digit}{}", tag(apt), e.ident.chars().last().filter(|c| c.is_alphabetic()).unwrap_or('A'));
            let exit = destination(apt.lat, apt.lon, *brg, 40.0 * NM);
            let pts = vec![wpt(format!("D{}05", e.ident), p5, 1, &name, elev + 3_000.0), wpt(format!("{}{d}", tag(apt)), exit, 1, &name, 15_000.0)];
            Proc { name, pts }
        })
        .collect()
}

/// Transitions d'une SID : prolongement vers 100 NM dans la direction de sortie.
pub fn sid_trans(apt: &Airport, sid: &Proc) -> Vec<Proc> {
    let Some(last) = sid.pts.last() else { return Vec::new() };
    let (_, brg) = dist_brg(apt.lat, apt.lon, last.lat, last.lon);
    let far = destination(apt.lat, apt.lon, brg, 100.0 * NM);
    let name = format!("{}T", last.ident);
    vec![Proc { name: name.clone(), pts: vec![wpt(name.clone(), far, 1, &name, 25_000.0)] }]
}

/// Approche ILS de l'extrémité `e` : IAF 15 NM (4000 ft sol), FAF 6 NM (2000 ft sol).
pub fn approach(apt: &Airport, e: &RwyEnd) -> Proc {
    let (tla, tlo) = threshold(apt, e);
    let back = wrap360(e.hdg + 180.0);
    let elev = apt.elev / FT;
    let name = format!("ILS{}", e.ident);
    let iaf = destination(tla, tlo, back, 15.0 * NM);
    let faf = destination(tla, tlo, back, 6.0 * NM);
    Proc { name: name.clone(), pts: vec![wpt(format!("CI{}", e.ident), iaf, 3, &name, elev + 4_000.0), wpt(format!("FF{}", e.ident), faf, 3, &name, elev + 2_000.0)] }
}

/// STAR vers l'approche `e` : point d'entrée à 60 NM (FL150 → 10000 ft) puis IAF.
pub fn stars(apt: &Airport, e: &RwyEnd) -> Vec<Proc> {
    let iaf = approach(apt, e).pts[0].clone();
    DIRS.iter()
        .map(|(d, brg)| {
            let name = format!("{}{d}1", tag(apt));
            let entry = destination(apt.lat, apt.lon, *brg, 60.0 * NM);
            let mid_ll = ((entry.0 + iaf.lat) / 2.0, (entry.1 + iaf.lon) / 2.0);
            let pts = vec![wpt(format!("{}{d}A", tag(apt)), entry, 2, &name, 15_000.0), wpt(format!("{}{d}B", tag(apt)), mid_ll, 2, &name, 10_000.0)];
            Proc { name, pts }
        })
        .collect()
}

/// Transitions d'arrivée : point à 150 NM en amont de l'entrée de STAR.
pub fn star_trans(apt: &Airport, star: &Proc) -> Vec<Proc> {
    let Some(first) = star.pts.first() else { return Vec::new() };
    let (_, brg) = dist_brg(apt.lat, apt.lon, first.lat, first.lon);
    let far = destination(apt.lat, apt.lon, brg, 150.0 * NM);
    let name = format!("{}T", first.ident);
    vec![Proc { name: name.clone(), pts: vec![wpt(name.clone(), far, 2, &name, 24_000.0)] }]
}

pub fn airport(icao: &str) -> Option<&'static Airport> {
    AIRPORTS.iter().find(|a| a.icao == icao)
}
