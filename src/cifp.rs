//! Procédures réelles des États-Unis : FAA CIFP (Coded Instrument Flight Procedures, domaine
//! public, cycle de 28 jours comme l'AIRAC), au format ARINC 424 des FMS.
//! SID (PD), STAR (PE) et approches (PF) ; points résolus par les fiches de points terminaux (PC),
//! de points en route (EA), de balises VOR (D) / NDB (DB, PN) et de seuils de piste (PG).
//! Les branches sans point fixe (cap, altitude, guidage radar : VA, VM, CA…) ne sont pas
//! retenues : le Darkstar vole de point en point.

use crate::fms::Wpt;
use crate::procs::Proc;
use std::collections::HashMap;

pub fn path() -> std::path::PathBuf {
    crate::navdb::dir().join("cifp.tsv")
}

/// Branche de procédure résolue.
#[derive(Clone, Debug)]
pub struct Leg {
    /// D (SID), E (STAR), F (approche)
    pub sub: char,
    pub proc: String,
    pub rt: char,
    pub trans: String,
    pub seq: u32,
    pub fix: String,
    pub lat: f64,
    pub lon: f64,
    /// Contrainte : description (+ au-dessus, - au-dessous, B entre, espace à), altitudes (ft)
    pub desc: char,
    pub alt1: f64,
    pub alt2: f64,
}

/// Coordonnée ARINC (N33542830 / W118292809) en degrés.
fn coord(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.len() < 9 {
        return None;
    }
    let (h, d) = s.split_at(1);
    let (deg_len, total) = if h == "N" || h == "S" { (2, 8) } else { (3, 9) };
    if d.len() < total || !d[..total].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let deg: f64 = d[..deg_len].parse().ok()?;
    let min: f64 = d[deg_len..deg_len + 2].parse().ok()?;
    let sec: f64 = d[deg_len + 2..total].parse::<f64>().ok()? / 100.0;
    let v = deg + min / 60.0 + sec / 3600.0;
    Some(if h == "S" || h == "W" { -v } else { v })
}

fn alt(s: &str) -> f64 {
    let s = s.trim();
    if let Some(fl) = s.strip_prefix("FL") {
        fl.parse::<f64>().map(|v| v * 100.0).unwrap_or(0.0)
    } else {
        s.parse().unwrap_or(0.0)
    }
}

fn cut(l: &str, a: usize, b: usize) -> &str {
    l.get(a..b.min(l.len())).unwrap_or("")
}

/// Fichier ARINC 424 → base compacte (une ligne par branche résolue).
pub fn build(text: &str) -> String {
    // 1. Points : clé « section|zone|ident » (zone = aérodrome pour les points terminaux, région OACI sinon)
    let mut fixes: HashMap<String, (f64, f64)> = HashMap::new();
    for l in text.lines() {
        if l.len() < 51 || !l.starts_with('S') {
            continue;
        }
        let (sec, sub_e) = (cut(l, 4, 5), cut(l, 5, 6));
        let pos = || -> Option<(f64, f64)> {
            match (coord(cut(l, 32, 41)), coord(cut(l, 41, 51))) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => Some((coord(cut(l, 55, 64))?, coord(cut(l, 64, 74))?)),
            }
        };
        let key = match (sec, sub_e) {
            ("E", "A") => Some(format!("EA|{}|{}", cut(l, 19, 21), cut(l, 13, 18).trim())),
            ("D", " ") => Some(format!("D |{}|{}", cut(l, 19, 21), cut(l, 13, 17).trim())),
            ("D", "B") => Some(format!("DB|{}|{}", cut(l, 19, 21), cut(l, 13, 17).trim())),
            ("P", " ") => {
                let s = cut(l, 12, 13);
                match s {
                    "C" | "G" | "N" => Some(format!("P{s}|{}|{}", cut(l, 6, 10).trim(), cut(l, 13, 18).trim())),
                    _ => None,
                }
            }
            _ => None,
        };
        if let (Some(k), Some(p)) = (key, pos()) {
            fixes.insert(k, p);
        }
    }
    // 2. Localizers (ILS) : indicatif, fréquence, piste
    let mut out = String::new();
    for l in text.lines() {
        if l.starts_with("SUSAP") && cut(l, 12, 13) == "I" {
            let mhz = cut(l, 22, 27).trim().parse::<f64>().unwrap_or(0.0) / 100.0;
            let rwy = cut(l, 27, 32).trim().trim_start_matches("RW").to_string();
            if mhz > 100.0 && !rwy.is_empty() {
                out.push_str(&format!("{}\tI\t{rwy}\t \t \t0\t{}\t0\t0\t \t{mhz:.2}\t0\n", cut(l, 6, 10).trim(), cut(l, 13, 17).trim()));
            }
        }
    }
    // 3. Branches des procédures
    for l in text.lines() {
        if l.len() < 94 || !l.starts_with("SUSAP") {
            continue;
        }
        let sub = cut(l, 12, 13);
        if !matches!(sub, "D" | "E" | "F") || cut(l, 38, 39) != "0" && cut(l, 38, 39) != "1" {
            continue;
        }
        let apt = cut(l, 6, 10).trim();
        let fix = cut(l, 29, 34).trim();
        if fix.is_empty() {
            continue;
        }
        let (fsec, fsub, freg) = (cut(l, 36, 37), cut(l, 37, 38), cut(l, 34, 36));
        let key = match (fsec, fsub) {
            ("E", "A") => format!("EA|{freg}|{fix}"),
            ("D", " ") => format!("D |{freg}|{fix}"),
            ("D", "B") => format!("DB|{freg}|{fix}"),
            ("P", s) => format!("P{s}|{apt}|{fix}"),
            _ => continue,
        };
        let Some((la, lo)) = fixes.get(&key) else { continue };
        out.push_str(&format!(
            "{apt}\t{sub}\t{}\t{}\t{}\t{}\t{fix}\t{la:.6}\t{lo:.6}\t{}\t{:.0}\t{:.0}\n",
            cut(l, 13, 19).trim(),
            cut(l, 19, 20),
            cut(l, 20, 25).trim(),
            cut(l, 26, 29).trim(),
            if cut(l, 82, 83).is_empty() { " " } else { cut(l, 82, 83) },
            alt(cut(l, 84, 89)),
            alt(cut(l, 89, 94)),
        ));
    }
    out
}

/// Base chargée : branches par aérodrome.
static DB: std::sync::LazyLock<HashMap<String, Vec<Leg>>> = std::sync::LazyLock::new(|| {
    let mut m: HashMap<String, Vec<Leg>> = HashMap::new();
    if cfg!(test) && std::env::var("DARKSTAR_REALDB").is_err() {
        return m;
    }
    let Ok(s) = std::fs::read_to_string(path()) else { return m };
    for l in s.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 12 {
            continue;
        }
        let leg = Leg {
            sub: f[1].chars().next().unwrap_or(' '),
            proc: f[2].into(),
            rt: f[3].chars().next().unwrap_or(' '),
            trans: f[4].into(),
            seq: f[5].parse().unwrap_or(0),
            fix: f[6].into(),
            lat: f[7].parse().unwrap_or(0.0),
            lon: f[8].parse().unwrap_or(0.0),
            desc: f[9].chars().next().unwrap_or(' '),
            alt1: f[10].parse().unwrap_or(0.0),
            alt2: f[11].parse().unwrap_or(0.0),
        };
        m.entry(f[0].to_string()).or_default().push(leg);
    }
    m
});

/// Nombre d'aérodromes couverts par la base CIFP installée.
pub fn airports() -> usize {
    DB.len()
}

/// ILS réel de la piste (indicatif, MHz).
pub fn ils(apt: &str, rwy: &str) -> Option<(String, f64)> {
    legs(apt, 'I').into_iter().find(|l| l.proc == rwy).map(|l| (l.fix.clone(), l.alt1))
}

/// L'aérodrome a-t-il des procédures réelles ?
pub fn has(apt: &str) -> bool {
    DB.contains_key(apt)
}

fn legs(apt: &str, sub: char) -> Vec<&'static Leg> {
    DB.get(apt).map(|v| v.iter().filter(|l| l.sub == sub).collect()).unwrap_or_default()
}

/// La transition de piste « RW06B » / « RW06L » / « ALL » convient-elle à la piste `rwy` (« 06L ») ?
fn rwy_match(trans: &str, rwy: &str) -> bool {
    if trans == "ALL" {
        return true;
    }
    let Some(t) = trans.strip_prefix("RW") else { return false };
    if t == rwy {
        return true;
    }
    // « RW06B » : les deux pistes parallèles 06L / 06R
    t.ends_with('B') && rwy.starts_with(&t[..t.len() - 1])
}

fn to_wpts(mut v: Vec<&Leg>, code: u8, name: &str) -> Vec<Wpt> {
    v.sort_by_key(|l| l.seq);
    let mut out: Vec<Wpt> = Vec::new();
    for l in v {
        if l.fix.starts_with("RW") || out.last().is_some_and(|w: &Wpt| w.ident == l.fix) {
            continue;
        }
        out.push(Wpt { ident: l.fix.clone(), lat: l.lat, lon: l.lon, via: name.to_string(), proc: code, sb_alt: if l.desc == 'B' && l.alt2 > 0.0 { (l.alt1 + l.alt2) / 2.0 } else { l.alt1 }, cstr: if l.alt1 > 0.0 { (l.desc, l.alt1, l.alt2) } else { ('\0', 0.0, 0.0) }, kind: "PROC".into(), ..Default::default() });
    }
    out
}

/// Types de route ARINC : (transitions de piste, tronc commun, transitions en route).
fn route_kinds(sub: char) -> (&'static str, &'static str, &'static str) {
    if sub == 'D' { ("14F", "25M", "36S") } else { ("369S", "258M", "147F") }
}

/// SID (D) ou STAR (E) applicables à la piste : tronc commun + transition de piste.
pub fn procs_for(apt: &str, sub: char, rwy: &str) -> Vec<Proc> {
    let all = legs(apt, sub);
    let (rwk, com, _) = route_kinds(sub);
    let mut names: Vec<&str> = all.iter().map(|l| l.proc.as_str()).collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for n in names {
        let p: Vec<&Leg> = all.iter().copied().filter(|l| l.proc == n).collect();
        let rw: Vec<&Leg> = p.iter().copied().filter(|l| rwk.contains(l.rt)).collect();
        let rw_ok: Vec<&Leg> = rw.iter().copied().filter(|l| rwy_match(&l.trans, rwy)).collect();
        if !rw.is_empty() && rw_ok.is_empty() {
            continue;
        }
        let common: Vec<&Leg> = p.iter().copied().filter(|l| com.contains(l.rt)).collect();
        // Ordre : SID = piste puis tronc commun ; STAR = tronc commun puis piste
        let seq: Vec<&Leg> = if sub == 'D' { rw_ok.into_iter().chain(common).collect() } else { common.into_iter().chain(rw_ok).collect() };
        let code = if sub == 'D' { 1 } else { 2 };
        let pts = to_wpts_ordered(seq, code, n);
        if !pts.is_empty() {
            out.push(Proc { name: n.to_string(), pts });
        }
    }
    out
}

/// Comme `to_wpts`, mais en gardant l'ordre des segments (chaque segment trié par séquence).
fn to_wpts_ordered(v: Vec<&Leg>, code: u8, name: &str) -> Vec<Wpt> {
    let mut segs: Vec<Vec<&Leg>> = Vec::new();
    for l in v {
        match segs.last_mut() {
            Some(s) if s[0].trans == l.trans && s[0].rt == l.rt => s.push(l),
            _ => segs.push(vec![l]),
        }
    }
    let mut out: Vec<Wpt> = Vec::new();
    for s in segs {
        for w in to_wpts(s, code, name) {
            if out.last().is_none_or(|p| p.ident != w.ident) {
                out.push(w);
            }
        }
    }
    out
}

/// Transitions en route d'une SID / STAR.
pub fn transitions(apt: &str, sub: char, proc: &str) -> Vec<Proc> {
    let all = legs(apt, sub);
    let (_, _, enr) = route_kinds(sub);
    let mut names: Vec<&str> = all.iter().filter(|l| l.proc == proc && enr.contains(l.rt)).map(|l| l.trans.as_str()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|t| {
            let v: Vec<&Leg> = all.iter().copied().filter(|l| l.proc == proc && l.trans == t && enr.contains(l.rt)).collect();
            let pts = to_wpts(v, if sub == 'D' { 1 } else { 2 }, proc);
            (!pts.is_empty()).then(|| Proc { name: t.to_string(), pts })
        })
        .collect()
}

/// Approche de la piste : ILS de préférence, sinon LOC, RNAV ; segment final jusqu'au seuil
/// (approche interrompue exclue), précédé de la première transition d'approche si elle existe.
pub fn approach(apt: &str, rwy: &str) -> Option<Proc> {
    let all = legs(apt, 'F');
    let mut names: Vec<&str> = all.iter().map(|l| l.proc.as_str()).filter(|p| p.get(1..).is_some_and(|r| r.starts_with(rwy))).collect();
    names.sort();
    names.dedup();
    let rank = |n: &str| match n.chars().next() {
        Some('I') => 0,
        Some('L') => 1,
        Some('R') | Some('H') => 2,
        _ => 3,
    };
    names.sort_by_key(|n| rank(n));
    let n = *names.first()?;
    let fin: Vec<&Leg> = all.iter().copied().filter(|l| l.proc == n && l.rt != 'A').collect();
    // Coupe au seuil : les branches suivantes sont l'approche interrompue
    let mut fin_sorted = fin.clone();
    fin_sorted.sort_by_key(|l| l.seq);
    let cut_at = fin_sorted.iter().position(|l| l.fix.starts_with("RW")).unwrap_or(fin_sorted.len());
    let fin_sorted: Vec<&Leg> = fin_sorted.into_iter().take(cut_at).collect();
    let label = match n.chars().next() {
        Some('I') => format!("ILS{rwy}"),
        Some('L') => format!("LOC{rwy}"),
        Some('R') | Some('H') => format!("RNV{rwy}"),
        _ => format!("APP{rwy}"),
    };
    let mut pts = to_wpts(fin_sorted, 3, &label);
    if pts.is_empty() {
        return None;
    }
    // Transition d'approche se raccordant au premier point du final, si elle existe
    let first = pts[0].ident.clone();
    let mut trans: Vec<&str> = all.iter().filter(|l| l.proc == n && l.rt == 'A').map(|l| l.trans.as_str()).collect();
    trans.sort();
    trans.dedup();
    if let Some(t) = trans.first() {
        let v: Vec<&Leg> = all.iter().copied().filter(|l| l.proc == n && l.rt == 'A' && l.trans == *t).collect();
        let mut tp = to_wpts(v, 3, &label);
        tp.retain(|w| w.ident != first);
        tp.extend(pts);
        pts = tp;
    }
    Some(Proc { name: label, pts })
}

/// Cycle CIFP installé (ex. « 261029 »).
pub fn cycle() -> Option<String> {
    std::fs::read_to_string(crate::navdb::dir().join("cifp.cycle")).ok().map(|s| s.trim().to_string())
}

/// Téléchargement du cycle en vigueur (lien le plus récent dont la date est passée).
pub fn download() -> Result<String, String> {
    let page = std::process::Command::new("/usr/bin/curl")
        .args(["-sfL", "--max-time", "60", "-A", "darkstar-sim", "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/cifp/download/"])
        .output()
        .map_err(|e| e.to_string())?;
    let html = String::from_utf8_lossy(&page.stdout);
    let today = {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
        let z = now / 86_400 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        format!("{:02}{m:02}{d:02}", y % 100)
    };
    let mut links: Vec<(String, String)> = html
        .split("CIFP_")
        .skip(1)
        .filter_map(|s| {
            let d: String = s.chars().take(6).collect();
            (d.len() == 6 && d.chars().all(|c| c.is_ascii_digit()) && s[6..].starts_with(".zip")).then(|| (d.clone(), format!("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_{d}.zip")))
        })
        .collect();
    links.sort();
    links.dedup();
    let (cyc, url) = links.iter().rev().find(|(d, _)| *d <= today).or(links.first()).cloned().ok_or("CIFP : AUCUN CYCLE TROUVE")?;
    let d = crate::navdb::dir();
    let zip = d.join("cifp.zip");
    let ok = std::process::Command::new("/usr/bin/curl").args(["-sfL", "--max-time", "300", "-A", "darkstar-sim", "-o"]).arg(&zip).arg(&url).status().map(|s| s.success()).unwrap_or(false);
    if !ok {
        return Err("CIFP : TELECHARGEMENT IMPOSSIBLE".into());
    }
    let out = std::process::Command::new("/usr/bin/unzip").arg("-p").arg(&zip).arg("FAACIFP18").output().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&zip);
    let text = String::from_utf8_lossy(&out.stdout);
    let db = build(&text);
    if db.is_empty() {
        return Err("CIFP : FICHIER ILLISIBLE".into());
    }
    let tmp = path().with_extension("tmp");
    std::fs::write(&tmp, &db).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path()).map_err(|e| e.to_string())?;
    let _ = std::fs::write(d.join("cifp.cycle"), &cyc);
    let n = db.lines().map(|l| l.split('\t').next().unwrap_or("")).collect::<std::collections::HashSet<_>>().len();
    Ok(format!("CIFP {cyc} : {n} AERODROMES US"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arinc_parse() {
        let text = "SUSAP KLAXK2CCLVVR K20    W     N33561905W118315904                       E0114     NAR           CLVVR                    359672605\n\
SUSAP KLAXK2FI06L  ACLVVR 010CLVVRK2PC0E  A    IF                                   07000     18000                 A FS   376061612\n\
SUSAP KLAXK2GRW06L   0089260710 N33565680W118255218         -0001500113000055150IIUWU1                                     383591612\n\
SUSAP KLAXK2FI06L  I      020CLVVRK2PC0E    010TF                                 + 04000                           A FS   376041612\n\
SUSAP KLAXK2FI06L  I      030RW06LK2PG0E    010TF                                   00128                           A FS   376041612\n";
        let db = build(text);
        assert!(db.contains("CLVVR"));
        assert_eq!(coord("N33561905").map(|v| (v * 1000.0).round()), Some(33939.0));
        assert!(rwy_match("RW06B", "06L") && rwy_match("RW06L", "06L") && !rwy_match("RW24R", "06L"));
    }
}
