//! Base de navigation mondiale gratuite (OurAirports, domaine public, mise à jour quotidienne) :
//! aérodromes avec pistes d'au moins 6000 ft dont les deux seuils sont positionnés, désignations
//! réelles, longueur, largeur, altitude. Téléchargée au premier lancement (ou par MAJ NAVDATA),
//! réduite dans un fichier compact chargé au démarrage.

use std::sync::{Arc, Mutex};

const BASE: &str = "https://davidmegginson.github.io/ourairports-data";

pub fn dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Library/Application Support/Darkstar/navdata")
}

fn db_path() -> std::path::PathBuf {
    dir().join("navdb.tsv")
}

/// Piste de la base : (désignation basse, lat, lon, désignation haute, lat, lon, longueur m, largeur m).
#[derive(Clone, Debug)]
pub struct DbRwy {
    pub id_lo: String,
    pub lo: (f64, f64),
    pub id_hi: String,
    pub hi: (f64, f64),
    pub len: f64,
    pub width: f64,
}

#[derive(Clone, Debug)]
pub struct DbAirport {
    pub icao: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub elev_m: f64,
    pub rwys: Vec<DbRwy>,
}

/// Découpe une ligne CSV (guillemets doubles, virgules).
fn csv_line(l: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut q = false;
    let mut chars = l.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => q = !q,
            ',' if !q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Texte affichable (ASCII majuscule) pour la police du jeu.
fn ascii_upper(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' | 'À' | 'Á' | 'Â' | 'Ä' => 'A',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'E',
            'í' | 'ì' | 'î' | 'ï' | 'Í' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ö' => 'O',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ü' => 'U',
            'ç' | 'Ç' => 'C',
            'ñ' | 'Ñ' => 'N',
            c if c.is_ascii() => c.to_ascii_uppercase(),
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Construit la base compacte à partir des CSV OurAirports.
pub fn build(airports_csv: &str, runways_csv: &str) -> Vec<DbAirport> {
    use std::collections::HashMap;
    let mut lines = airports_csv.lines();
    let head = csv_line(lines.next().unwrap_or(""));
    let col = |h: &Vec<String>, n: &str| h.iter().position(|x| x == n);
    let (i_ident, i_name, i_lat, i_lon, i_elev, i_icao, i_type) = (col(&head, "ident"), col(&head, "name"), col(&head, "latitude_deg"), col(&head, "longitude_deg"), col(&head, "elevation_ft"), col(&head, "icao_code"), col(&head, "type"));
    let mut apts: HashMap<String, DbAirport> = HashMap::new();
    for l in lines {
        let f = csv_line(l);
        let g = |i: Option<usize>| i.and_then(|i| f.get(i)).cloned().unwrap_or_default();
        let ty = g(i_type);
        if ty == "heliport" || ty == "closed" || ty == "seaplane_base" || ty == "balloonport" {
            continue;
        }
        let ident = g(i_ident);
        let icao = { let c = g(i_icao); if c.len() == 4 { c } else { ident.clone() } };
        if icao.len() != 4 || !icao.chars().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let (Ok(lat), Ok(lon)) = (g(i_lat).parse::<f64>(), g(i_lon).parse::<f64>()) else { continue };
        let elev_m = g(i_elev).parse::<f64>().unwrap_or(0.0) * 0.3048;
        apts.insert(ident, DbAirport { icao: icao.to_uppercase(), name: ascii_upper(&g(i_name)), lat, lon, elev_m, rwys: Vec::new() });
    }
    let mut lines = runways_csv.lines();
    let head = csv_line(lines.next().unwrap_or(""));
    let c = |n: &str| col(&head, n);
    let (i_apt, i_len, i_w, i_closed, i_le, i_lela, i_lelo, i_he, i_hela, i_helo) = (c("airport_ident"), c("length_ft"), c("width_ft"), c("closed"), c("le_ident"), c("le_latitude_deg"), c("le_longitude_deg"), c("he_ident"), c("he_latitude_deg"), c("he_longitude_deg"));
    for l in lines {
        let f = csv_line(l);
        let g = |i: Option<usize>| i.and_then(|i| f.get(i)).cloned().unwrap_or_default();
        if g(i_closed) == "1" {
            continue;
        }
        let len_ft = g(i_len).parse::<f64>().unwrap_or(0.0);
        if len_ft < 6_000.0 {
            continue;
        }
        let p = |a: Option<usize>, b: Option<usize>| -> Option<(f64, f64)> { Some((g(a).parse().ok()?, g(b).parse().ok()?)) };
        let (Some(lo), Some(hi)) = (p(i_lela, i_lelo), p(i_hela, i_helo)) else { continue };
        let Some(a) = apts.get_mut(&g(i_apt)) else { continue };
        let width = (g(i_w).parse::<f64>().unwrap_or(150.0) * 0.3048).clamp(30.0, 90.0);
        a.rwys.push(DbRwy { id_lo: g(i_le), lo, id_hi: g(i_he), hi, len: len_ft * 0.3048, width });
    }
    let mut v: Vec<DbAirport> = apts.into_values().filter(|a| !a.rwys.is_empty()).collect();
    v.sort_by(|a, b| a.icao.cmp(&b.icao));
    v.dedup_by(|a, b| a.icao == b.icao);
    v
}

fn save(v: &[DbAirport], path: &std::path::Path) -> std::io::Result<()> {
    let mut s = String::new();
    for a in v {
        s.push_str(&format!("A\t{}\t{}\t{:.6}\t{:.6}\t{:.1}\n", a.icao, a.name, a.lat, a.lon, a.elev_m));
        for r in &a.rwys {
            s.push_str(&format!("R\t{}\t{:.6}\t{:.6}\t{}\t{:.6}\t{:.6}\t{:.0}\t{:.0}\n", r.id_lo, r.lo.0, r.lo.1, r.id_hi, r.hi.0, r.hi.1, r.len, r.width));
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, s)?;
    std::fs::rename(tmp, path)
}

/// Charge la base compacte (None si absente).
pub fn load() -> Option<Vec<DbAirport>> {
    let s = std::fs::read_to_string(db_path()).ok()?;
    let mut v: Vec<DbAirport> = Vec::new();
    for l in s.lines() {
        let f: Vec<&str> = l.split('\t').collect();
        match f.first() {
            Some(&"A") if f.len() >= 6 => v.push(DbAirport { icao: f[1].into(), name: f[2].into(), lat: f[3].parse().ok()?, lon: f[4].parse().ok()?, elev_m: f[5].parse().ok()?, rwys: Vec::new() }),
            Some(&"R") if f.len() >= 9 => {
                let r = DbRwy { id_lo: f[1].into(), lo: (f[2].parse().ok()?, f[3].parse().ok()?), id_hi: f[4].into(), hi: (f[5].parse().ok()?, f[6].parse().ok()?), len: f[7].parse().ok()?, width: f[8].parse().ok()? };
                v.last_mut()?.rwys.push(r);
            }
            _ => {}
        }
    }
    if v.is_empty() { None } else { Some(v) }
}

/// Date de la base installée (secondes Unix), si présente.
pub fn installed_at() -> Option<u64> {
    std::fs::metadata(db_path()).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

#[derive(Clone, Debug, PartialEq)]
pub enum Upd {
    Idle,
    Busy(String),
    Done(String),
    Error(String),
}

/// Téléchargement de la dernière version (tâche de fond). Le résultat s'applique au redémarrage.
pub fn update() -> Arc<Mutex<Upd>> {
    let st = Arc::new(Mutex::new(Upd::Busy("TELECHARGEMENT OURAIRPORTS".into())));
    let s2 = st.clone();
    std::thread::spawn(move || {
        let d = dir();
        let _ = std::fs::create_dir_all(&d);
        let get = |name: &str| -> Result<String, String> {
            let f = d.join(name);
            let ok = std::process::Command::new("/usr/bin/curl").args(["-sfL", "--max-time", "120", "-A", "darkstar-sim", "-o"]).arg(&f).arg(format!("{BASE}/{name}")).status().map(|s| s.success()).unwrap_or(false);
            if !ok {
                return Err(format!("{name} : TELECHARGEMENT IMPOSSIBLE"));
            }
            std::fs::read_to_string(&f).map_err(|e| e.to_string())
        };
        let res = (|| -> Result<String, String> {
            let a = get("airports.csv")?;
            *s2.lock().unwrap() = Upd::Busy("PISTES".into());
            let r = get("runways.csv")?;
            *s2.lock().unwrap() = Upd::Busy("CONSTRUCTION DE LA BASE".into());
            let v = build(&a, &r);
            save(&v, &db_path()).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(d.join("airports.csv"));
            let _ = std::fs::remove_file(d.join("runways.csv"));
            Ok(format!("{} AERODROMES, {} PISTES", v.len(), v.iter().map(|a| a.rwys.len()).sum::<usize>()))
        })();
        *s2.lock().unwrap() = match res {
            Ok(m) => Upd::Done(m),
            Err(e) => Upd::Error(e),
        };
    });
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_build() {
        let a = "\"id\",\"ident\",\"type\",\"name\",\"latitude_deg\",\"longitude_deg\",\"elevation_ft\",\"icao_code\"\n1,\"KEDW\",\"medium_airport\",\"Edwards Air Force Base\",34.905,-117.883,2312,\"KEDW\"\n2,\"00A\",\"heliport\",\"X\",1,1,1,\n";
        let r = "\"id\",\"airport_ref\",\"airport_ident\",\"length_ft\",\"width_ft\",\"surface\",\"lighted\",\"closed\",\"le_ident\",\"le_latitude_deg\",\"le_longitude_deg\",\"le_elevation_ft\",\"le_heading_degT\",\"le_displaced_threshold_ft\",\"he_ident\",\"he_latitude_deg\",\"he_longitude_deg\"\n9,1,\"KEDW\",16798,300,\"CON\",1,0,\"05R\",34.894565,-117.905,2303,58,,\"23L\",34.918858,-117.857377\n";
        let v = build(a, r);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rwys[0].id_lo, "05R");
        assert_eq!(csv_line("1,\"a,b\",\"c\"\"d\""), vec!["1", "a,b", "c\"d"]);
    }
}
