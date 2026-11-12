//! Météo réelle (Open-Meteo, voir `wxreal`) avec repli procédural hors ligne. Météo procédurale déterministe : vent fonction de l'altitude (courant-jet vers FL350, vent faible
//! dans la stratosphère), champs de cumulus, couches de stratus et cellules orageuses (cumulonimbus
//! à enclume). Effets : dérive de l'avion, turbulence, vol aux instruments dans les nuages. Rendu :
//! nuages volumétriques en panneaux orientés vers la caméra dans la vision synthétique, éclairs,
//! radar météo sur la carte.

use crate::aircraft::Aircraft;
use crate::geo::*;
use crate::terrain::EyeFrame;
use crate::ui::*;
use macroquad::miniquad;
use macroquad::prelude::*;
use std::collections::HashMap;

const CELL_DEG: f64 = 0.2;
/// Rayon de génération / d'affichage des nuages (m).
const VIEW_RANGE: f64 = 260_000.0;
const MAX_PUFFS: usize = 2_000;

pub const SEVERITIES: [&str; 5] = ["CALME", "BEAU", "NUAGEUX", "COUVERT", "ORAGEUX"];

#[derive(Clone, Copy, PartialEq)]
pub enum CloudKind {
    Cu,
    St,
    Cb,
}

pub struct Puff {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub r: f64,
}

pub struct Cloud {
    pub kind: CloudKind,
    pub lat: f64,
    pub lon: f64,
    pub base: f64,
    pub top: f64,
    pub radius: f64,
    pub puffs: Vec<Puff>,
    seed: f64,
}

/// État météo au point de l'avion, pour l'horizon et le modèle de vol.
#[derive(Clone, Default)]
pub struct WxInfo {
    /// Direction d'où vient le vent (degrés) et force (kt).
    pub wind_dir: f64,
    pub wind_kt: f64,
    /// 0..1 : immersion dans un nuage (vol sans visibilité).
    pub imc: f64,
    /// 0..1 : intensité de turbulence.
    pub turb: f64,
    /// Orage le plus proche devant : (relèvement, distance m, sommet ft).
    pub storm: Option<(f64, f64, f64)>,
    /// Source des données (« REEL OPEN-METEO », « SIMULE »...).
    pub source: String,
    /// Température de l'air extérieur (°C) si connue.
    pub oat: Option<f64>,
    /// Danger au point de l'avion (0..1) et sa cause.
    pub danger: f32,
    pub cause: &'static str,
    /// Danger le plus fort devant sur la trajectoire : (cause, distance m, niveau 0..1, altitude ft).
    pub ahead: Option<(&'static str, f64, f32, f64)>,
}


pub struct Weather {
    pub severity: usize,
    cells: HashMap<(i32, i32), Vec<Cloud>>,
    pub info: WxInfo,
    gust_t: f64,
    /// Origine de la masse turbulente traversée (THERMIQUE, ONDE, CAT)
    pub turb_cause: &'static str,
    /// Météo réelle (prioritaire quand disponible).
    pub use_real: bool,
    pub real: crate::wxreal::Real,
    /// Visualisation 3D dans l'horizon (zones de danger, nappes de turbulence).
    pub show3d: bool,
    /// Portée de la visualisation 3D (m), selon la vitesse.
    pub part_r: f64,
}

impl Cloud {
    /// Distance normalisée d'un point (est / nord en m depuis le centre) à la cellule : < 1 dedans.
    /// Forme irrégulière comme une vraie cellule : étirement selon un axe aléatoire (lignes de
    /// grains), bords déchiquetés par un bruit à deux échelles, lobes secondaires.
    pub fn shape_q(&self, e: f64, n: f64) -> f64 {
        shape_q(self.seed, self.radius, e, n)
    }

    /// Contour (est, nord en m depuis le centre) où `shape_q` vaut `q`, sur `n` rayons.
    pub fn contour(&self, q: f64, n: usize) -> Vec<(f64, f64)> {
        (0..n)
            .map(|k| {
                let a = k as f64 / n as f64 * std::f64::consts::TAU;
                let (se, sn) = (a.sin(), a.cos());
                let mut last = 0.0;
                let step = self.radius * 0.1;
                let mut d = step;
                while d < self.radius * 4.0 {
                    if self.shape_q(se * d, sn * d) < q {
                        last = d;
                    }
                    d += step;
                }
                (se * last, sn * last)
            })
            .collect()
    }
}

/// Forme irrégulière d'une cellule (graine, rayon nominal) : voir `Cloud::shape_q`.
pub fn shape_q(s: f64, r: f64, e: f64, n: f64) -> f64 {
    let ang = s * std::f64::consts::PI * 7.0;
    let (sa, ca) = ang.sin_cos();
    let (x, y) = (e * ca + n * sa, -e * sa + n * ca);
    let aspect = 1.0 + 0.9 * ((s * 13.7).fract());
    let (xs, ys) = (x / aspect.sqrt(), y * aspect.sqrt());
    let seed = (s * 1000.0) as i64;
    let big = vnoise(xs / (r * 0.9) + s * 50.0, ys / (r * 0.9), 41 + seed % 7);
    let small = vnoise(xs / (r * 0.3) + s * 90.0, ys / (r * 0.3), 43 + seed % 5);
    let warp = 1.0 + 0.55 * (big - 0.5) * 2.0 + 0.25 * (small - 0.5) * 2.0;
    (xs.hypot(ys) / r) / warp.max(0.35)
}

fn hash(a: i64, b: i64, c: i64) -> f64 {
    let mut h = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (c as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 29;
    ((h >> 11) as f64) / ((1u64 << 53) as f64)
}

fn vnoise(x: f64, y: f64, seed: i64) -> f64 {
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |i, j| hash(i, j, seed);
    let a = h(ix, iy) + (h(ix + 1, iy) - h(ix, iy)) * sx;
    let b = h(ix, iy + 1) + (h(ix + 1, iy + 1) - h(ix, iy + 1)) * sx;
    a + (b - a) * sy
}

impl Weather {
    pub fn new() -> Weather {
        Weather {
            severity: 2,
            cells: HashMap::new(),
            info: WxInfo::default(),
            gust_t: 0.0,
            turb_cause: "",
            use_real: true,
            real: crate::wxreal::Real::new(),
            show3d: true,
            part_r: 5_000.0,
        }
    }

    pub fn set_severity(&mut self, s: usize) {
        self.severity = s.min(4);
        self.use_real = false;
        self.cells.clear();
    }

    pub fn set_real(&mut self) {
        self.use_real = true;
        self.cells.clear();
    }

    fn grid(&self) -> Option<&crate::wxreal::Grid> {
        if self.use_real { self.real.grid.as_ref() } else { None }
    }

    /// Échantillon météo réel au point (None : météo simulée ou hors grille).
    pub fn sample(&self, lat: f64, lon: f64, alt: f64) -> Option<crate::wxreal::Sample> {
        self.grid()?.sample(lat, lon, alt)
    }

    /// Danger météo (0..1) au point et sa cause, hors nuages : turbulence en air clair (cisaillement),
    /// convection, givrage, vent fort ; en météo simulée, voisinage du courant-jet.
    pub fn field_danger(&self, lat: f64, lon: f64, alt: f64) -> (f32, &'static str, f32) {
        let sm = |x: f64| {
            let x = x.clamp(0.0, 1.0);
            (x * x * (3.0 - 2.0 * x)) as f32
        };
        let mut d = 0.0f32;
        let mut cause = "";
        let mut put = |v: f32, c: &'static str| {
            if v > d {
                d = v;
                cause = c;
            }
        };
        if let Some(s) = self.sample(lat, lon, alt) {
            put(sm((s.shear - 0.004) / 0.014), "CAT");
            let conv_top = 9_000.0 + s.cape * 1.5;
            if alt < conv_top {
                put(sm((s.cape - 400.0) / 2_000.0) * (0.55 + 0.45 * sm(s.precip / 3.0)), "CONVECTION");
            }
            if (-20.0..0.0).contains(&s.t_c) && s.rh > 85.0 {
                put(0.45 + 0.2 * sm((s.rh - 85.0) / 15.0), "GIVRAGE");
            }
            put(sm((s.u.hypot(s.v) - 45.0) / 50.0) * 0.6, "VENT FORT");
            return (d, cause, (s.rh / 100.0) as f32);
        }
        let (_, _, _, spd) = self.wind_at(lat, lon, alt);
        let h_ft = alt / FT;
        if (28_000.0..42_000.0).contains(&h_ft) {
            put(sm((spd - 70.0) / 80.0) * 0.7, "CAT");
        }
        (d, cause, 0.4)
    }

    fn sev_k(&self) -> f64 {
        [0.3, 0.7, 1.0, 1.15, 1.35][self.severity]
    }

    /// Vent (vers l'est, vers le nord) en m/s, et direction d'origine / force en kt.
    pub fn wind_at(&self, lat: f64, lon: f64, alt: f64) -> (f64, f64, f64, f64) {
        if let Some(s) = self.sample(lat, lon, alt) {
            let spd = s.u.hypot(s.v);
            let dir = wrap360(s.u.atan2(s.v).to_degrees() + 180.0);
            return (s.u, s.v, dir, spd / KT);
        }
        let h_ft = alt / FT;
        // Profil vertical : couche limite, courant-jet ~FL350, stratosphère calme
        let jet = 55.0 + 70.0 * vnoise(lat / 9.0, lon / 9.0, 7);
        let spd = if h_ft < 10_000.0 {
            8.0 + 14.0 * h_ft / 10_000.0
        } else if h_ft < 35_000.0 {
            22.0 + (jet - 22.0) * ((h_ft - 10_000.0) / 25_000.0)
        } else if h_ft < 60_000.0 {
            jet + (22.0 - jet) * ((h_ft - 35_000.0) / 25_000.0)
        } else {
            22.0 - 8.0 * ((h_ft - 60_000.0) / 40_000.0).min(1.0)
        } * self.sev_k()
            * (0.75 + 0.5 * vnoise(lat / 4.0, lon / 4.0, 3));
        // Direction : dominante d'ouest, tourne avec l'altitude et le lieu
        let veer = (h_ft / 35_000.0).min(1.0) * 25.0;
        let dir = wrap360(240.0 + veer + 90.0 * (vnoise(lat / 6.0, lon / 6.0, 11) - 0.5));
        let ms = spd * KT;
        let to = (dir + 180.0).to_radians();
        (ms * to.sin(), ms * to.cos(), dir, spd)
    }

    /// Nuages d'une cellule en météo simulée (niveau de sévérité).
    fn specs_proc(&self, i: i32, j: i32) -> Vec<(CloudKind, f64, f64, f64, f64, f64)> {
        let mut out = Vec::new();
        let (lat0, lon0) = (i as f64 * CELL_DEG, j as f64 * CELL_DEG);
        let cov = vnoise(lat0 / 2.5, lon0 / 2.5, 21);
        let sev = self.severity as f64;
        let p = ((cov - (0.78 - 0.17 * sev)) * 3.0).clamp(0.0, 1.0);
        let r = |k: i64| hash(i as i64, j as i64, 100 + k);
        if sev == 0.0 || r(0) > p {
            return out;
        }
        let n = 1 + (r(1) * (1.0 + sev)) as usize;
        for c in 0..n {
            let rr = |k: i64| hash(i as i64 * 31 + c as i64, j as i64, 200 + k);
            let lat = lat0 + rr(0) * CELL_DEG;
            let lon = lon0 + rr(1) * CELL_DEG;
            let storm = sev >= 3.0 && cov > 0.62 && rr(2) < 0.12 * (sev - 2.0) && c == 0;
            let kind = if storm {
                CloudKind::Cb
            } else if sev >= 2.0 && rr(3) < 0.18 + 0.1 * (sev - 2.0) {
                CloudKind::St
            } else {
                CloudKind::Cu
            };
            let (base_ft, top_ft, radius) = match kind {
                CloudKind::Cu => {
                    let b = 7_000.0 + rr(4) * 3_000.0;
                    (b, b + 3_000.0 + rr(5) * (3_000.0 + 2_500.0 * sev), 1_500.0 + rr(6) * 2_000.0)
                }
                CloudKind::St => {
                    let b = 11_000.0 + rr(4) * 4_000.0;
                    (b, b + 1_200.0 + rr(5) * 800.0, 5_000.0 + rr(6) * 5_000.0)
                }
                CloudKind::Cb => (5_500.0 + rr(4) * 1_500.0, 38_000.0 + rr(5) * 14_000.0, 6_000.0 + rr(6) * 3_500.0),
            };
            out.push((kind, lat, lon, base_ft * FT, top_ft * FT, radius));
        }
        out
    }

    /// Nuages d'une cellule d'après la météo réelle : cumulus (nébulosité basse, développement par la
    /// CAPE), couches moyennes et cirrus (nébulosité moyenne / haute aux hauteurs des niveaux de
    /// pression), cumulonimbus si la convection est forte et qu'il pleut.
    fn specs_real(&self, i: i32, j: i32, g: &crate::wxreal::Grid) -> Option<Vec<(CloudKind, f64, f64, f64, f64, f64)>> {
        let (lat0, lon0) = (i as f64 * CELL_DEG, j as f64 * CELL_DEG);
        let (lc, oc) = (lat0 + CELL_DEG / 2.0, lon0 + CELL_DEG / 2.0);
        let s = g.sample(lc, oc, 3_000.0)?;
        let h = |hpa: u32, dflt: f64| g.level_h(lc, oc, hpa).unwrap_or(dflt);
        let mut out = Vec::new();
        let r = |k: i64| hash(i as i64, j as i64, 700 + k);
        let mut c = 0i64;
        let next = |c: &mut i64| {
            *c += 1;
            let k = *c;
            move |x: i64| hash(i as i64 * 31 + k, j as i64, 800 + x)
        };
        // Cumulonimbus
        let storm = (s.cape - 500.0).max(0.0) / 2_000.0 * if s.precip > 0.2 || s.code >= 95.0 { 1.0 } else { 0.25 };
        if r(0) < storm.min(0.6) {
            let rr = next(&mut c);
            let top = h(200, 11_800.0) + (s.cape / 2_500.0).min(1.5) * 1_500.0;
            out.push((CloudKind::Cb, lat0 + rr(0) * CELL_DEG, lon0 + rr(1) * CELL_DEG, s.elev + 1_200.0 + rr(4) * 500.0, top, 5_000.0 + rr(6) * 3_500.0));
        }
        // Cumulus de la couche basse
        let n_cu = (s.cc_low / 100.0 * 4.0 + r(1)) as usize;
        for _ in 0..n_cu {
            let rr = next(&mut c);
            let base = s.elev + 700.0 + rr(4) * 900.0;
            let depth = 600.0 + s.cc_low / 100.0 * 900.0 + (s.cape / 1_000.0).min(2.0) * 1_500.0 * rr(5);
            out.push((CloudKind::Cu, lat0 + rr(0) * CELL_DEG, lon0 + rr(1) * CELL_DEG, base, base + depth, 1_300.0 + rr(6) * 1_800.0));
        }
        // Couche moyenne (altostratus) et cirrus
        for (cc, lo, hi, thick, rad) in [(s.cc_mid, h(700, 3_000.0), h(500, 5_600.0), 500.0, 5_000.0), (s.cc_high, h(300, 9_200.0), h(250, 10_400.0), 350.0, 8_000.0)] {
            let n = (cc / 100.0 * 2.5 + r(2 + c)) as usize;
            for _ in 0..n {
                let rr = next(&mut c);
                let base = lo + (hi - lo) * rr(4);
                out.push((CloudKind::St, lat0 + rr(0) * CELL_DEG, lon0 + rr(1) * CELL_DEG, base, base + thick, rad * (0.7 + 0.6 * rr(6))));
            }
        }
        Some(out)
    }

    fn gen_cell(&self, i: i32, j: i32) -> Vec<Cloud> {
        let mut out = Vec::new();
        let specs = match self.grid().and_then(|g| self.specs_real(i, j, g)) {
            Some(sp) => sp,
            None => self.specs_proc(i, j),
        };
        for (c, (kind, lat, lon, base, top, radius)) in specs.into_iter().enumerate() {
            let rr = |k: i64| hash(i as i64 * 31 + c as i64, j as i64, 200 + k);
            let mut puffs = Vec::new();
            let mut add = |de: f64, dn: f64, alt: f64, pr: f64| {
                let (pla, plo) = from_enu(lat, lon, de, dn);
                puffs.push(Puff { lat: pla, lon: plo, alt, r: pr });
            };
            match kind {
                CloudKind::Cu => {
                    let k = 6 + (rr(7) * 7.0) as usize;
                    for m in 0..k {
                        let q = |x: i64| hash(i as i64 * 97 + c as i64 * 13 + m as i64, j as i64, 300 + x);
                        let a = q(0) * std::f64::consts::TAU;
                        let d = q(1).sqrt() * radius * 0.75;
                        let h = base + (top - base) * (0.25 + 0.6 * q(2) * (1.0 - d / radius));
                        add(d * a.cos(), d * a.sin(), h, radius * (0.35 + 0.3 * q(3)));
                    }
                }
                CloudKind::St => {
                    for gx in -2..=2 {
                        for gy in -2..=2 {
                            let q = |x: i64| hash(i as i64 * 53 + gx + c as i64 * 7, j as i64 * 11 + gy, 400 + x);
                            if q(0) < 0.25 {
                                continue;
                            }
                            let s = radius / 2.5;
                            add(gx as f64 * s + (q(1) - 0.5) * s, gy as f64 * s + (q(2) - 0.5) * s, (base + top) / 2.0, s * (0.8 + 0.3 * q(3)));
                        }
                    }
                }
                CloudKind::Cb => {
                    // Tour convective puis enclume étalée sous la tropopause
                    let mut h = base + radius * 0.4;
                    let mut m = 0;
                    while h < top - radius * 0.3 {
                        let q = |x: i64| hash(i as i64 * 17 + m, j as i64 * 5, 500 + x);
                        let rad = radius * (0.55 + 0.15 * q(0));
                        add((q(1) - 0.5) * radius * 0.4, (q(2) - 0.5) * radius * 0.4, h, rad);
                        add((q(3) - 0.5) * radius, (q(4) - 0.5) * radius, h + rad * 0.3, rad * 0.7);
                        h += rad * 0.9;
                        m += 1;
                    }
                    for a in 0..10 {
                        let q = |x: i64| hash(i as i64 * 19 + a, j as i64 * 23, 600 + x);
                        let ang = a as f64 / 10.0 * std::f64::consts::TAU + q(0);
                        let d = radius * (0.6 + 1.2 * q(1));
                        add(d * ang.cos() + radius * 0.8, d * ang.sin(), top - 600.0 * q(2), radius * 0.6);
                    }
                }
            }
            out.push(Cloud { kind, lat, lon, base, top, radius, puffs, seed: rr(9) });
        }
        out
    }

    /// Nuages des cellules dans le rayon donné (générées et mises en cache à la demande).
    fn ensure(&mut self, lat: f64, lon: f64, range: f64) {
        let dlat = range / 111_195.0;
        let dlon = dlat / lat.to_radians().cos().max(0.2);
        let (i0, i1) = (((lat - dlat) / CELL_DEG).floor() as i32, ((lat + dlat) / CELL_DEG).floor() as i32);
        let (j0, j1) = (((lon - dlon) / CELL_DEG).floor() as i32, ((lon + dlon) / CELL_DEG).floor() as i32);
        for i in i0..=i1 {
            for j in j0..=j1 {
                if !self.cells.contains_key(&(i, j)) {
                    let c = self.gen_cell(i, j);
                    self.cells.insert((i, j), c);
                }
            }
        }
        if self.cells.len() > 6_000 {
            let (ci, cj) = ((lat / CELL_DEG) as i32, (lon / CELL_DEG) as i32);
            self.cells.retain(|(i, j), _| (i - ci).abs() < 30 && (j - cj).abs() < 40);
        }
    }

    pub fn clouds_near(&self, lat: f64, lon: f64, range: f64) -> impl Iterator<Item = &Cloud> {
        self.cells.values().flatten().filter(move |c| (c.lat - lat).abs() < range / 111_000.0 + 0.2 && dist_brg(lat, lon, c.lat, c.lon).0 < range)
    }

    /// Mise à jour : vent et turbulence appliqués à l'avion, état météo pour l'horizon.
    pub fn update(&mut self, ac: &mut Aircraft, dt: f64) {
        if self.use_real {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            if self.real.poll(ac.lat, ac.lon, ac.d.mach, now) {
                // Nouvelles données : les nuages sont régénérés d'après la météo réelle
                self.cells.clear();
            }
        }
        self.ensure(ac.lat, ac.lon, VIEW_RANGE);
        self.gust_t += dt;
        let (we, wn, dir, spd) = self.wind_at(ac.lat, ac.lon, ac.alt);
        let mut imc = 0.0f64;
        let mut turb = 0.0f64;
        let mut storm: Option<(f64, f64, f64)> = None;
        for c in self.clouds_near(ac.lat, ac.lon, VIEW_RANGE) {
            let (d, brg) = dist_brg(ac.lat, ac.lon, c.lat, c.lon);
            if c.kind == CloudKind::Cb {
                let rel = wrap180(brg - ac.hdg_deg()).abs();
                if rel < 70.0 && d < 110_000.0 && storm.is_none_or(|s| d < s.1) {
                    storm = Some((brg, d, c.top / FT));
                }
                // Turbulence au voisinage de l'orage
                if d < c.radius * 2.0 && ac.alt < c.top + 2_000.0 {
                    turb = turb.max(0.6 * (1.0 - d / (c.radius * 2.0)));
                }
            }
            if d > c.radius * 2.5 {
                continue;
            }
            for p in &c.puffs {
                let (pd, _) = dist_brg(ac.lat, ac.lon, p.lat, p.lon);
                let dz = ac.alt - p.alt;
                let k = 1.0 - (pd * pd + dz * dz).sqrt() / p.r;
                if k > 0.0 {
                    imc = imc.max((k * 2.0).min(1.0));
                    let t = match c.kind {
                        CloudKind::Cu => 0.35,
                        CloudKind::St => 0.1,
                        CloudKind::Cb => 1.0,
                    };
                    turb = turb.max(t * k.sqrt());
                }
            }
        }
        // Masses d'air turbulentes localisées (cellules de ~20 à 60 km) : convection thermique
        // basse couche, ondes / rotors en aval du relief par vent fort, poches de turbulence en air
        // clair en altitude ; air calme au-dessus du FL600
        let (pt, pc) = turb_mass(ac.lat, ac.lon, ac.alt, ac.d.elev, spd);
        if pt > turb {
            turb = pt;
            self.turb_cause = pc;
        } else if pt < 0.05 {
            self.turb_cause = "";
        }
        // Turbulence mécanique près du sol par vent fort, turbulence en air clair près du jet
        let h_ft = (ac.alt - ac.d.elev) / FT;
        if h_ft < 3_000.0 && spd > 18.0 {
            turb = turb.max(((spd - 18.0) / 30.0).min(0.4));
        }
        // Turbulence en air clair (cisaillement réel ou voisinage du courant-jet), convection
        let (danger, cause, _) = self.field_danger(ac.lat, ac.lon, ac.alt);
        if cause == "CAT" || cause == "CONVECTION" {
            turb = turb.max(danger as f64 * 0.7);
        }
        // Rafales : composante aléatoire lissée
        let g = turb * 6.0 + spd * KT * 0.04;
        let gx = g * ((self.gust_t * 0.7).sin() + 0.5 * (self.gust_t * 1.9 + 1.3).sin());
        let gy = g * ((self.gust_t * 0.9 + 2.0).sin() + 0.5 * (self.gust_t * 2.3).sin());
        ac.wind = (we + gx, wn + gy);
        // Au-dessus du FL600 : plus de météo ni de turbulence (air très raréfié)
        let hf = ac.alt / FT;
        let turb = turb * (1.0 - ((hf - 60_000.0) / 10_000.0).clamp(0.0, 1.0));
        ac.turb = turb;
        let real = self.grid().is_some_and(|g| g.contains(ac.lat, ac.lon));
        let source = if real {
            self.real.status.clone()
        } else if self.use_real {
            format!("SIMULE ({})", self.real.status)
        } else {
            format!("SIMULE {}", SEVERITIES[self.severity])
        };
        let oat = self.sample(ac.lat, ac.lon, ac.alt).map(|s| s.t_c).or_else(|| Some(crate::atmosphere::isa(ac.alt).t - 273.15));
        // Orage proche : danger maximal
        let (mut danger, mut cause) = (danger, cause);
        if let Some((_, d, _)) = storm {
            let v = (1.0 - d / 40_000.0).clamp(0.0, 1.0) as f32;
            if v > danger {
                danger = v;
                cause = "ORAGE";
            }
        }
        let ahead = self.scan_ahead(ac);
        self.info = WxInfo { wind_dir: dir, wind_kt: spd, imc, turb, storm, source, oat, danger, cause, ahead };
        self.part_r = (ac.tas * 25.0).clamp(5_000.0, 80_000.0);
    }

    /// Danger le plus fort sur la trajectoire (10 points jusqu'à ~5 min de vol, max 300 km).
    fn scan_ahead(&self, ac: &Aircraft) -> Option<(&'static str, f64, f32, f64)> {
        let range = (ac.tas * 300.0).clamp(20_000.0, 300_000.0);
        let climb = ac.gamma.sin();
        let mut best: Option<(&'static str, f64, f32, f64)> = None;
        for k in 1..=10 {
            let d = range * k as f64 / 10.0;
            let (la, lo) = destination(ac.lat, ac.lon, ac.track_deg(), d);
            let alt = (ac.alt + d * climb).max(0.0);
            let (mut v, mut c, _) = self.field_danger(la, lo, alt);
            for cl in self.clouds_near(la, lo, 15_000.0) {
                if cl.kind == CloudKind::Cb && alt < cl.top + 600.0 && {
                    let (e, n) = enu_offset(cl.lat, cl.lon, la, lo);
                    cl.shape_q(e, n) < 1.15
                } {
                    v = 1.0;
                    c = "ORAGE";
                }
            }
            if v > 0.35 && best.is_none_or(|b| v > b.2 + 0.1) {
                best = Some((c, d, v, alt / FT));
            }
        }
        best
    }
}

// ---------- Rendu 3D ----------

thread_local! {
    static PUFF_TEX: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
}

/// Texture de « bouffée » : disque doux bruité (généré une fois).
fn puff_texture() -> Texture2D {
    PUFF_TEX.with(|t| {
        if let Some(tx) = t.borrow().as_ref() {
            return tx.clone();
        }
        let n = 64u16;
        let mut img = Image::gen_image_color(n, n, Color::new(1.0, 1.0, 1.0, 0.0));
        for y in 0..n {
            for x in 0..n {
                let (fx, fy) = ((x as f32 + 0.5) / n as f32 * 2.0 - 1.0, (y as f32 + 0.5) / n as f32 * 2.0 - 1.0);
                let d = (fx * fx + fy * fy).sqrt();
                let nz = vnoise(x as f64 / 9.0, y as f64 / 9.0, 41) as f32 * 0.35 + vnoise(x as f64 / 4.0, y as f64 / 4.0, 43) as f32 * 0.15;
                let a = ((1.0 - d) * 1.6 - 0.1 + nz - 0.25).clamp(0.0, 1.0);
                let shade = 0.88 + 0.12 * (1.0 - fy).min(1.0);
                img.set_pixel(x as u32, y as u32, Color::new(shade, shade, shade, a * a * (3.0 - 2.0 * a)));
            }
        }
        let tx = Texture2D::from_image(&img);
        tx.set_filter(FilterMode::Linear);
        *t.borrow_mut() = Some(tx.clone());
        tx
    })
}

thread_local! {
    static CLOUD_MAT: std::cell::RefCell<Option<Option<Material>>> = const { std::cell::RefCell::new(None) };
}

/// Matériau des nuages : test de profondeur (le relief masque les nuages) sans écriture de
/// profondeur (les panneaux translucides ne se découpent pas entre eux), mélange alpha.
fn cloud_material() -> Option<Material> {
    CLOUD_MAT.with(|m| {
        if let Some(v) = m.borrow().as_ref() {
            return v.clone();
        }
        const VS: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;
        const FS: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;
        let params = MaterialParams {
            pipeline_params: PipelineParams {
                depth_write: false,
                depth_test: Comparison::LessOrEqual,
                color_blend: Some(miniquad::BlendState::new(
                    miniquad::Equation::Add,
                    miniquad::BlendFactor::Value(miniquad::BlendValue::SourceAlpha),
                    miniquad::BlendFactor::OneMinusValue(miniquad::BlendValue::SourceAlpha),
                )),
                ..Default::default()
            },
            ..Default::default()
        };
        let mat = load_material(ShaderSource::Glsl { vertex: VS, fragment: FS }, params).ok();
        *m.borrow_mut() = Some(mat.clone());
        mat
    })
}

/// Nuages dans la vision synthétique (caméra 3D avec profondeur active). Panneaux triés du plus
/// lointain au plus proche, éclairés du dessus, éclairs dans les orages. Renvoie l'immersion IMC.
pub fn draw_clouds(wx: &Weather, ac: &Aircraft, right: Vec3, up: Vec3, time: f64) -> f32 {
    let eye = EyeFrame::new(ac);
    // (priorité de sélection, distance, position, rayon, couleur)
    let mut list: Vec<(f32, f32, Vec3, f32, [u8; 4])> = Vec::new();
    for c in wx.clouds_near(ac.lat, ac.lon, VIEW_RANGE) {
        let flash = c.kind == CloudKind::Cb && ((time * 0.43 + c.seed * 7.0).fract() < 0.018 || (time * 0.43 + c.seed * 7.0 + 0.03).fract() < 0.01);
        for p in &c.puffs {
            let pos = eye.local(p.lat, p.lon, p.alt);
            let d = pos.length();
            if d < p.r as f32 * 0.9 || d > VIEW_RANGE as f32 {
                continue;
            }
            // Éclairage : base grise, sommet blanc ; orages plus sombres
            let k = ((p.alt - c.base) / (c.top - c.base).max(1.0)).clamp(0.0, 1.0) as f32;
            let mut sh = 0.66 + 0.34 * k;
            if c.kind == CloudKind::Cb {
                sh *= 0.78;
            }
            if flash {
                sh = 1.0;
            }
            let fade = (1.0 - d / VIEW_RANGE as f32).clamp(0.0, 1.0).powf(0.6);
            let a = (0.80 * fade).min(1.0);
            let col = [(sh * 255.0) as u8, (sh * 255.0) as u8, ((sh * 1.03).min(1.0) * 255.0) as u8, (a * 255.0) as u8];
            // Les orages restent visibles de loin : priorité sur les petits cumulus proches
            let prio = if c.kind == CloudKind::Cb { d * 0.25 } else { d };
            list.push((prio, d, pos, p.r as f32, col));
        }
    }
    // Les plus proches dans la limite du budget, puis peints du plus lointain au plus proche
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    list.truncate(MAX_PUFFS);
    list.sort_by(|a, b| b.1.total_cmp(&a.1));
    let tex = puff_texture();
    let mat = cloud_material();
    if let Some(m) = &mat {
        gl_use_material(m);
    }
    for chunk in list.chunks(900) {
        let mut v = Vec::with_capacity(chunk.len() * 4);
        let mut idx: Vec<u16> = Vec::with_capacity(chunk.len() * 6);
        for (k, (_, _, p, r, col)) in chunk.iter().enumerate() {
            let (rx, uy) = (right * *r, up * *r);
            let b = (k * 4) as u16;
            for (q, uv) in [(-rx - uy, vec2(0.0, 1.0)), (rx - uy, vec2(1.0, 1.0)), (rx + uy, vec2(1.0, 0.0)), (-rx + uy, vec2(0.0, 0.0))] {
                v.push(Vertex { position: *p + q, uv, color: *col, normal: Vec4::ZERO });
            }
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
        draw_mesh(&Mesh { vertices: v, indices: idx, texture: Some(tex.clone()) });
    }
    if mat.is_some() {
        gl_use_default_material();
    }
    wx.info.imc as f32
}

/// Radar météo sur la carte : vert (cumulus), gris (stratus), jaune/rouge (orages).
pub fn draw_map(wx: &Weather, ac: &Aircraft, to_s: &dyn Fn(f64, f64) -> Vec2, z: f32, r: Rect) {
    let m_per_px = 111_195.0 / z as f64 * ac.lat.to_radians().cos().max(0.2);
    for c in wx.clouds_near(ac.lat, ac.lon, VIEW_RANGE) {
        let p = to_s(c.lat, c.lon);
        let rad = (c.radius / m_per_px) as f32;
        if rad < 1.5 || !r.contains(p) {
            continue;
        }
        match c.kind {
            CloudKind::Cu => draw_circle(p.x, p.y, rad, Color::new(0.2, 0.85, 0.3, 0.22)),
            CloudKind::St => draw_circle(p.x, p.y, rad, Color::new(0.7, 0.75, 0.8, 0.14)),
            CloudKind::Cb => {
                let scale = 1.0 / m_per_px as f32;
                for (q, col) in [(1.15, Color::new(0.2, 0.85, 0.3, 0.22)), (0.8, Color::new(1.0, 0.85, 0.1, 0.35)), (0.45, Color::new(1.0, 0.1, 0.1, 0.5))] {
                    let pts: Vec<Vec2> = c.contour(q, 28).into_iter().map(|(e, n)| vec2(p.x + e as f32 * scale, p.y - n as f32 * scale)).collect();
                    for k in 0..pts.len() {
                        draw_triangle(p, pts[k], pts[(k + 1) % pts.len()], col);
                    }
                }
                if z > 20.0 {
                    txt(&format!("CB FL{:.0}", c.top / FT / 100.0), p.x + rad + 3.0, p.y + 4.0, 11.0, Color::new(1.0, 0.4, 0.3, 1.0));
                }
            }
        }
    }
}

/// Turbulence (0..1) des masses d'air localisées au point, et son origine.
pub fn turb_mass(lat: f64, lon: f64, alt: f64, elev: f64, wind_kt: f64) -> (f64, &'static str) {
    let sm = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let h = alt - elev;
    let h_ft = alt / FT;
    let mut best = (0.0f64, "");
    // Thermiques : sous ~8000 ft sol, au-dessus des terres chaudes (latitudes < 45°)
    if h < 2_500.0 && elev > 0.0 && lat.abs() < 45.0 {
        let n = vnoise(lat * 4.0, lon * 4.0, 901);
        let v = sm((n - 0.45) / 0.35) * 0.45 * sm(h / 300.0) * (1.0 - sm((h - 1_800.0) / 700.0));
        if v > best.0 {
            best = (v, "THERMIQUE");
        }
    }
    // Ondes et rotors : relief marqué et vent > 20 kt, jusqu'à ~4 km au-dessus du sol
    if elev > 1_000.0 && wind_kt > 20.0 && h < 4_000.0 {
        let n = vnoise(lat * 6.0, lon * 6.0, 902);
        let v = sm((elev - 1_000.0) / 1_200.0) * sm((wind_kt - 20.0) / 30.0) * (0.35 + 0.4 * n) * (1.0 - sm((h - 2_500.0) / 1_500.0));
        if v > best.0 {
            best = (v, "ONDE / ROTOR");
        }
    }
    // Poches de turbulence en air clair entre FL200 et FL500 (couches de ~4000 ft)
    if (20_000.0..50_000.0).contains(&h_ft) {
        let layer = (h_ft / 4_000.0).floor() as i64;
        let n = vnoise(lat * 2.5, lon * 2.5, 910 + layer);
        let edge = sm((h_ft - 20_000.0) / 2_000.0) * (1.0 - sm((h_ft - 46_000.0) / 4_000.0));
        let v = sm((n - 0.72) / 0.2) * 0.65 * edge;
        if v > best.0 {
            best = (v, "CAT");
        }
    }
    best
}
