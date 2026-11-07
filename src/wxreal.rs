//! Météo réelle : prévision de l'heure courante d'Open-Meteo (gratuit, sans clé) sur une grille 5 x 5
//! centrée sur l'avion. Profils verticaux sur 12 niveaux de pression (1000 à 30 hPa, ~0 à 78 000 ft) :
//! vent, hauteur géopotentielle, température, humidité, nébulosité ; en surface : CAPE (énergie
//! convective), précipitations, nébulosité basse / moyenne / haute, code temps.
//! Téléchargement en tâche de fond (curl), rafraîchi quand l'avion s'éloigne du centre de la grille
//! ou toutes les 15 minutes. Échantillonnage : bilinéaire horizontal, linéaire en altitude.

use std::process::Command;
use std::sync::mpsc::{Receiver, channel};

pub const LEVELS: [u32; 12] = [1000, 850, 700, 500, 300, 250, 200, 150, 100, 70, 50, 30];
const N: usize = 5;
const REFRESH_S: f64 = 900.0;

/// Un niveau du profil vertical.
#[derive(Clone, Copy, Default)]
pub struct Level {
    pub hpa: u32,
    pub h: f64,
    pub u: f64,
    pub v: f64,
    pub t_c: f64,
    pub rh: f64,
    pub cc: f64,
}

/// Point de grille : profil vertical et paramètres de surface.
#[derive(Clone, Default)]
pub struct Point {
    pub levels: Vec<Level>,
    pub cape: f64,
    pub precip: f64,
    pub cc_low: f64,
    pub cc_mid: f64,
    pub cc_high: f64,
    pub code: f64,
    pub elev: f64,
}

pub struct Grid {
    pub lat0: f64,
    pub lon0: f64,
    pub step: f64,
    pub pts: Vec<Point>,
    pub fetched: f64,
}

/// Échantillon interpolé en un point de l'espace.
#[derive(Clone, Copy, Default)]
pub struct Sample {
    pub u: f64,
    pub v: f64,
    pub t_c: f64,
    pub rh: f64,
    pub cc: f64,
    /// Cisaillement vertical du vent (s⁻¹).
    pub shear: f64,
    pub cape: f64,
    pub precip: f64,
    pub cc_low: f64,
    pub cc_mid: f64,
    pub cc_high: f64,
    pub code: f64,
    pub elev: f64,
}

pub struct Real {
    pub grid: Option<Grid>,
    rx: Option<Receiver<Result<Grid, String>>>,
    last_try: f64,
    pub status: String,
}

fn vars() -> String {
    let mut v: Vec<String> = Vec::new();
    for l in LEVELS {
        for k in ["wind_speed", "wind_direction", "geopotential_height", "temperature", "relative_humidity", "cloud_cover"] {
            v.push(format!("{k}_{l}hPa"));
        }
    }
    for k in ["cape", "precipitation", "cloud_cover_low", "cloud_cover_mid", "cloud_cover_high", "weather_code"] {
        v.push(k.into());
    }
    v.join(",")
}

/// Première valeur du tableau `"clé":[…]` dans un objet JSON (sans dépendance).
fn first(seg: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{key}\":[");
    let i = seg.find(&pat)? + pat.len();
    let rest = &seg[i..];
    let end = rest.find([',', ']'])?;
    rest[..end].trim().parse().ok()
}

fn field(seg: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{key}\":");
    let i = seg.find(&pat)? + pat.len();
    let rest = &seg[i..];
    let end = rest.find([',', '}'])?;
    rest[..end].trim().parse().ok()
}

fn parse(body: &str, lat0: f64, lon0: f64, step: f64, now: f64) -> Result<Grid, String> {
    let segs: Vec<&str> = body.split("{\"latitude\"").skip(1).collect();
    if segs.len() != N * N {
        return Err(format!("{} points recus", segs.len()));
    }
    let mut pts = Vec::with_capacity(N * N);
    for seg in segs {
        let mut levels = Vec::new();
        for l in LEVELS {
            let g = |k: &str| first(seg, &format!("{k}_{l}hPa"));
            let (Some(spd), Some(dir), Some(h)) = (g("wind_speed"), g("wind_direction"), g("geopotential_height")) else { continue };
            let to = (dir + 180.0).to_radians();
            levels.push(Level { hpa: l, h, u: spd * to.sin(), v: spd * to.cos(), t_c: g("temperature").unwrap_or(-50.0), rh: g("relative_humidity").unwrap_or(0.0), cc: g("cloud_cover").unwrap_or(0.0) });
        }
        if levels.len() < 4 {
            return Err("profil incomplet".into());
        }
        levels.sort_by(|a, b| a.h.total_cmp(&b.h));
        let f = |k: &str| first(seg, k).unwrap_or(0.0);
        pts.push(Point {
            levels,
            cape: f("cape"),
            precip: f("precipitation"),
            cc_low: f("cloud_cover_low"),
            cc_mid: f("cloud_cover_mid"),
            cc_high: f("cloud_cover_high"),
            code: f("weather_code"),
            elev: field(seg, "elevation").unwrap_or(0.0),
        });
    }
    Ok(Grid { lat0, lon0, step, pts, fetched: now })
}

fn fetch(lat0: f64, lon0: f64, step: f64, now: f64) -> Result<Grid, String> {
    let mut la = Vec::new();
    let mut lo = Vec::new();
    for i in 0..N {
        for j in 0..N {
            la.push(format!("{:.3}", (lat0 + i as f64 * step).clamp(-89.0, 89.0)));
            lo.push(format!("{:.3}", crate::geo::wrap180(lon0 + j as f64 * step)));
        }
    }
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly={}&forecast_hours=1&wind_speed_unit=ms&timezone=GMT",
        la.join(","),
        lo.join(","),
        vars()
    );
    let out = Command::new("/usr/bin/curl").args(["-sf", "--max-time", "25", "-A", "darkstar-sim"]).arg(&url).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("serveur meteo injoignable".into());
    }
    parse(&String::from_utf8_lossy(&out.stdout), lat0, lon0, step, now)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Valeurs d'un profil à l'altitude `h` (m) : niveau interpolé et cisaillement local.
fn at_alt(p: &Point, h: f64) -> (Level, f64) {
    let l = &p.levels;
    let top = l[l.len() - 1];
    if h >= top.h {
        // Au-dessus de 30 hPa : vent stratosphérique décroissant, température quasi constante
        let k = (1.0 - (h - top.h) / 15_000.0).clamp(0.5, 1.0);
        return (Level { u: top.u * k, v: top.v * k, rh: 0.0, cc: 0.0, ..top }, 0.0);
    }
    if h <= l[0].h {
        return (l[0], 0.0);
    }
    let i = l.windows(2).position(|w| h >= w[0].h && h < w[1].h).unwrap_or(0);
    let (a, b) = (l[i], l[i + 1]);
    let t = (h - a.h) / (b.h - a.h).max(1.0);
    let shear = ((b.u - a.u).hypot(b.v - a.v)) / (b.h - a.h).max(1.0);
    (Level { hpa: 0, h, u: lerp(a.u, b.u, t), v: lerp(a.v, b.v, t), t_c: lerp(a.t_c, b.t_c, t), rh: lerp(a.rh, b.rh, t), cc: lerp(a.cc, b.cc, t) }, shear)
}

impl Grid {
    /// Coordonnées fractionnaires dans la grille (None hors grille).
    fn cell(&self, lat: f64, lon: f64) -> Option<(usize, usize, f64, f64)> {
        let fi = (lat - self.lat0) / self.step;
        let fj = crate::geo::wrap180(lon - self.lon0) / self.step;
        let m = (N - 1) as f64;
        if !(0.0..=m).contains(&fi) || !(0.0..=m).contains(&fj) {
            return None;
        }
        let (i, j) = ((fi.floor() as usize).min(N - 2), (fj.floor() as usize).min(N - 2));
        Some((i, j, fi - i as f64, fj - j as f64))
    }

    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        self.cell(lat, lon).is_some()
    }

    pub fn sample(&self, lat: f64, lon: f64, alt: f64) -> Option<Sample> {
        let (i, j, ti, tj) = self.cell(lat, lon)?;
        let corners = [(i, j, (1.0 - ti) * (1.0 - tj)), (i + 1, j, ti * (1.0 - tj)), (i, j + 1, (1.0 - ti) * tj), (i + 1, j + 1, ti * tj)];
        let mut s = Sample::default();
        for (a, b, w) in corners {
            let p = &self.pts[a * N + b];
            let (l, sh) = at_alt(p, alt);
            s.u += l.u * w;
            s.v += l.v * w;
            s.t_c += l.t_c * w;
            s.rh += l.rh * w;
            s.cc += l.cc * w;
            s.shear += sh * w;
            s.cape += p.cape * w;
            s.precip += p.precip * w;
            s.cc_low += p.cc_low * w;
            s.cc_mid += p.cc_mid * w;
            s.cc_high += p.cc_high * w;
            s.code = s.code.max(if w > 0.2 { p.code } else { 0.0 });
            s.elev += p.elev * w;
        }
        Some(s)
    }

    /// Hauteur (m) d'un niveau de pression au point (moyenne des coins).
    pub fn level_h(&self, lat: f64, lon: f64, hpa: u32) -> Option<f64> {
        let (i, j, _, _) = self.cell(lat, lon)?;
        self.pts[i * N + j].levels.iter().find(|l| l.hpa == hpa).map(|l| l.h)
    }
}

impl Real {
    pub fn new() -> Real {
        Real { grid: None, rx: None, last_try: -1e9, status: "EN ATTENTE".into() }
    }

    /// Lance ou récupère le téléchargement selon la position (`now` en secondes, horloge réelle).
    pub fn poll(&mut self, lat: f64, lon: f64, mach: f64, now: f64) -> bool {
        let mut fresh = false;
        if let Some(rx) = &self.rx {
            if let Ok(r) = rx.try_recv() {
                self.rx = None;
                match r {
                    Ok(g) => {
                        self.grid = Some(g);
                        self.status = "REEL OPEN-METEO".into();
                        fresh = true;
                    }
                    Err(e) => self.status = format!("HORS LIGNE ({e})"),
                }
            }
        }
        if self.rx.is_some() || now - self.last_try < 30.0 {
            return fresh;
        }
        let need = match &self.grid {
            None => true,
            Some(g) => {
                let c = (N - 1) as f64 / 2.0;
                let (clat, clon) = (g.lat0 + c * g.step, g.lon0 + c * g.step);
                let far = (lat - clat).abs() > g.step * 0.9 || crate::geo::wrap180(lon - clon).abs() > g.step * 0.9;
                far || now - g.fetched > REFRESH_S
            }
        };
        if need {
            self.last_try = now;
            // Maille élargie en hypersonique (la grille couvre ~3 min de vol)
            let step = if mach > 3.0 { 2.0 } else { 0.8 };
            let c = (N - 1) as f64 / 2.0;
            let (lat0, lon0) = (lat - c * step, lon - c * step);
            let (tx, rx) = channel();
            self.rx = Some(rx);
            if self.grid.is_none() {
                self.status = "TELECHARGEMENT...".into();
            }
            std::thread::spawn(move || {
                let _ = tx.send(fetch(lat0, lon0, step, now));
            });
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_sample() {
        let mut body = String::from("[");
        for k in 0..N * N {
            body.push_str(&format!(
                "{{\"latitude\":{k},\"elevation\":100.0,\"hourly\":{{\"wind_speed_850hPa\":[10.0],\"wind_direction_850hPa\":[270],\"geopotential_height_850hPa\":[1500],\"wind_speed_500hPa\":[30.0],\"wind_direction_500hPa\":[270],\"geopotential_height_500hPa\":[5500],\"wind_speed_250hPa\":[60.0],\"wind_direction_250hPa\":[270],\"geopotential_height_250hPa\":[10500],\"wind_speed_100hPa\":[20.0],\"wind_direction_100hPa\":[270],\"geopotential_height_100hPa\":[16000],\"cape\":[1200.0]}}}},"
            ));
        }
        body.push(']');
        let g = parse(&body, 10.0, 20.0, 1.0, 0.0).expect("grille");
        let s = g.sample(12.0, 22.0, 8_000.0).expect("echantillon");
        // Vent d'ouest : souffle vers l'est
        assert!(s.u > 40.0 && s.u < 50.0 && s.v.abs() < 1e-6, "u={} v={}", s.u, s.v);
        assert!(s.shear > 0.005);
        assert!((s.cape - 1200.0).abs() < 1e-6);
        assert!(g.sample(30.0, 22.0, 8_000.0).is_none());
    }
}
