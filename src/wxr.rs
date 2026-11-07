//! Radar météo façon Airbus (WXR) :
//! - antenne qui balaie ± 60° devant l'avion (aller-retour en ~ 8 s) ; l'image se rafraîchit au
//!   passage du faisceau, colonne par colonne ;
//! - réflectivité d'après les nuages (cœurs de cumulonimbus, cumulus développés) et les
//!   précipitations réelles (Open-Meteo) : vert (faible), jaune (modéré), rouge (fort) ;
//!   turbulence en magenta (mode WX+TURB, dans les 40 NM, comme sur Airbus) ;
//! - inclinaison d'antenne : AUTO (balayage multiple : tout le temps significatif sous l'avion et à
//!   son niveau) ou manuelle (faisceau de 3,5° : les échos dépendent de l'altitude visée) ;
//! - peint sur l'écran de navigation (sous les symboles) et, en 3D, dans l'horizon : rideaux
//!   colorés à l'emplacement des échos et bande de synthèse sur la ligne d'horizon.

use crate::aircraft::Aircraft;
use crate::geo::*;
use crate::svs::View;
use crate::terrain::EyeFrame;
use crate::ui::*;
use crate::weather::{CloudKind, Weather};
use macroquad::prelude::*;

pub const AZ: usize = 60;
pub const RG: usize = 40;
const HALF_SCAN: f32 = 60.0;
const SCAN_RATE: f32 = 30.0;
const BEAM_DEG: f64 = 3.5;

pub struct Wxr {
    pub on: bool,
    pub auto: bool,
    pub tilt: f32,
    pub turb: bool,
    /// Rideaux 3D et bande de synthèse dans l'horizon.
    pub horizon: bool,
    sweep: f32,
    dir: f32,
    /// Niveau (0 rien, 1 vert, 2 jaune, 3 rouge, 4 turbulence) et sommet de l'écho (m) par case.
    bins: Vec<(u8, f32)>,
    range_m: f64,
}

pub fn level_color(l: u8, a: f32) -> Color {
    match l {
        1 => Color::new(0.05, 0.75, 0.20, a),
        2 => Color::new(0.95, 0.88, 0.10, a),
        3 => Color::new(0.95, 0.12, 0.10, a),
        4 => Color::new(1.0, 0.20, 0.90, a),
        _ => Color::new(0.0, 0.0, 0.0, 0.0),
    }
}

/// Nuage ramené au repère avion (est, nord) pour un test rapide.
struct Echo<'a> {
    e: f64,
    n: f64,
    r: f64,
    base: f64,
    top: f64,
    kind: CloudKind,
    cloud: &'a crate::weather::Cloud,
}

impl Wxr {
    pub fn new() -> Wxr {
        Wxr { on: true, auto: true, tilt: 0.0, turb: true, horizon: true, sweep: -HALF_SCAN, dir: 1.0, bins: vec![(0, 0.0); AZ * RG], range_m: 0.0 }
    }

    /// Angle d'antenne effectif (degrés) pour l'affichage.
    pub fn tilt_label(&self) -> String {
        if self.auto { "AUTO".into() } else { format!("{:+.1}", self.tilt) }
    }

    /// Avance le balayage et calcule les colonnes balayées pendant `dt` (secondes réelles).
    pub fn update(&mut self, wx: &Weather, ac: &Aircraft, range_nm: f64, dt: f32) {
        if !self.on {
            return;
        }
        let range_m = range_nm * NM;
        if (range_m - self.range_m).abs() > 1.0 {
            self.range_m = range_m;
            self.bins.iter_mut().for_each(|b| *b = (0, 0.0));
        }
        let from = self.sweep;
        self.sweep += self.dir * SCAN_RATE * dt.min(0.1);
        if self.sweep.abs() >= HALF_SCAN {
            self.sweep = self.sweep.clamp(-HALF_SCAN, HALF_SCAN);
            self.dir = -self.dir;
        }
        let col = |a: f32| (((a + HALF_SCAN) / (2.0 * HALF_SCAN) * AZ as f32) as usize).min(AZ - 1);
        let (c0, c1) = (col(from.min(self.sweep)), col(from.max(self.sweep)));
        // Échos candidats dans la portée
        let echoes: Vec<Echo> = wx
            .clouds_near(ac.lat, ac.lon, range_m * 1.1)
            .filter(|c| c.kind != CloudKind::St)
            .map(|c| {
                let (e, n) = enu_offset(ac.lat, ac.lon, c.lat, c.lon);
                Echo { e, n, r: c.radius, base: c.base, top: c.top, kind: c.kind, cloud: c }
            })
            .collect();
        let hdg = ac.hdg_deg();
        for c in c0..=c1 {
            let az = -HALF_SCAN + (c as f32 + 0.5) / AZ as f32 * 2.0 * HALF_SCAN;
            let brg = (hdg + az as f64).to_radians();
            for k in 0..RG {
                let d = (k as f64 + 0.5) / RG as f64 * range_m;
                let (e, n) = (d * brg.sin(), d * brg.cos());
                // Faisceau : altitudes couvertes à cette distance (courbure terrestre comprise)
                let spread = d * (BEAM_DEG / 2.0).to_radians().tan();
                let drop = d * d / (2.0 * 8_500_000.0);
                let (lo, hi) = if self.auto {
                    (0.0, ac.alt + spread)
                } else {
                    let ctr = ac.alt + d * (self.tilt as f64).to_radians().tan() - drop;
                    (ctr - spread, ctr + spread)
                };
                let mut lvl = 0u8;
                let mut top = 0.0f32;
                for x in &echoes {
                    if hi < x.base || lo > x.top {
                        continue;
                    }
                    if (e - x.e).abs() > x.r * 4.0 || (n - x.n).abs() > x.r * 4.0 {
                        continue;
                    }
                    let q = x.cloud.shape_q(e - x.e, n - x.n);
                    let l = match x.kind {
                        CloudKind::Cb if q < 0.45 => 3,
                        CloudKind::Cb if q < 0.8 => 2,
                        CloudKind::Cb if q < 1.15 => 1,
                        CloudKind::Cu if q < 0.7 && x.top - x.base > 2_500.0 => 1,
                        _ => 0,
                    };
                    // Turbulence autour des cœurs convectifs, dans les 40 NM
                    let l = if self.turb && x.kind == CloudKind::Cb && (0.7..1.1).contains(&q) && d < 40.0 * NM { 4 } else { l };
                    if l > lvl {
                        lvl = l;
                    }
                    if l > 0 {
                        top = top.max(x.top.min(hi.max(x.base)) as f32);
                    }
                }
                // Précipitations réelles (sous l'isotherme 0 °C environ)
                if lvl < 3 && lo < 4_500.0 {
                    let (la, lo2) = from_enu(ac.lat, ac.lon, e, n);
                    if let Some(s) = wx.sample(la, lo2, 1_500.0) {
                        let l = if s.precip > 8.0 { 3 } else if s.precip > 2.5 { 2 } else if s.precip > 0.3 { 1 } else { 0 };
                        if l > lvl {
                            lvl = l;
                            top = top.max(4_000.0);
                        }
                    }
                }
                self.bins[c * RG + k] = (lvl, top);
            }
        }
    }

    /// Image radar sur l'écran de navigation (rose centrée en `c`, rayon `rad` pour la portée).
    pub fn draw_nd(&self, c: Vec2, rad: f32, time: f64) {
        if !self.on {
            return;
        }
        let pt = |az: f32, f: f32| {
            let a = (az - 90.0).to_radians();
            vec2(c.x + rad * f * a.cos(), c.y + rad * f * a.sin())
        };
        let da = 2.0 * HALF_SCAN / AZ as f32;
        for col in 0..AZ {
            let a0 = -HALF_SCAN + col as f32 * da;
            for k in 0..RG {
                let (l, _) = self.bins[col * RG + k];
                if l == 0 {
                    continue;
                }
                let (f0, f1) = (k as f32 / RG as f32, (k + 1) as f32 / RG as f32);
                let (p00, p01, p10, p11) = (pt(a0, f0), pt(a0, f1), pt(a0 + da, f0), pt(a0 + da, f1));
                let cl = level_color(l, 0.78);
                draw_triangle(p00, p01, p11, cl);
                draw_triangle(p00, p11, p10, cl);
            }
        }
        // Faisceau courant (léger)
        let s = pt(self.sweep, 1.0);
        draw_line(c.x, c.y, s.x, s.y, 1.0, Color::new(0.3, 1.0, 0.5, 0.18 + 0.05 * (time * 8.0).sin() as f32));
    }

    /// Écho le plus significatif : (niveau, azimut relatif deg, distance m) — le plus fort, puis le plus proche.
    pub fn strongest(&self) -> Option<(u8, f32, f64)> {
        let mut best: Option<(u8, f32, f64)> = None;
        for col in 0..AZ {
            for k in 0..RG {
                let (l, _) = self.bins[col * RG + k];
                if l == 0 {
                    continue;
                }
                let d = (k as f64 + 0.5) / RG as f64 * self.range_m;
                let az = -HALF_SCAN + (col as f32 + 0.5) / AZ as f32 * 2.0 * HALF_SCAN;
                if best.is_none_or(|b| l > b.0 || (l == b.0 && d < b.2)) {
                    best = Some((l, az, d));
                }
            }
        }
        best
    }

    /// Bloc radar météo sur l'horizon (zone centrale `r`, côté droit sous le bloc vent).
    pub fn draw_hud(&self, r: Rect, time: f64) {
        // Une ligne compacte sous le bloc vent (colonne droite)
        let w = 196.0;
        let x = r.x + r.w - w - 30.0;
        let y = r.y + 196.0;
        let b = Rect::new(x, y, w, 18.0);
        rrect(b, 4.0, Color::new(0.0, 0.0, 0.0, 0.55));
        if !self.on {
            txt("WXR OFF", x + 6.0, y + 13.0, 11.0, AMBER);
            return;
        }
        txt(&format!("WXR {}", self.tilt_label()), x + 6.0, y + 13.0, 10.0, GREEN_S);
        match self.strongest() {
            Some((l, az, d)) => {
                let clock = ((az / 30.0).round() as i32).rem_euclid(12);
                let clock = if clock == 0 { 12 } else { clock };
                let what = match l {
                    4 => "TURB",
                    3 => "FORT",
                    2 => "MODERE",
                    _ => "FAIBLE",
                };
                let blink = l < 3 || (time * 2.0).fract() < 0.7;
                draw_rectangle(x + 64.0, y + 5.0, 7.0, 7.0, level_color(l, 1.0));
                if blink {
                    txt(&format!("{what} {clock}H {:.0}NM", d / NM), x + 76.0, y + 13.0, 10.0, level_color(l, 1.0));
                }
            }
            None => txt("AUCUN ECHO", x + 64.0, y + 13.0, 10.0, Color::new(0.6, 0.65, 0.7, 1.0)),
        }
    }

    /// Dans l'horizon : rideaux translucides aux échos jaunes, rouges et magenta, et bande de synthèse
    /// le long de la ligne d'horizon (écho le plus fort par direction).
    pub fn draw_horizon(&self, ac: &Aircraft, v: &View, time: f64) {
        if !self.on || !self.horizon || ac.crashed.is_some() || self.range_m <= 0.0 {
            return;
        }
        let eye = EyeFrame::new(ac);
        let hdg = ac.hdg_deg();
        let da = 2.0 * HALF_SCAN as f64 / AZ as f64;
        scissor(Some(v.rect));
        let mut strongest = vec![(0u8, 1.0f32); AZ];
        for col in 0..AZ {
            let a0 = -HALF_SCAN as f64 + col as f64 * da;
            for k in 0..RG {
                let (l, top) = self.bins[col * RG + k];
                if l == 0 {
                    continue;
                }
                let f = (k as f32 + 0.5) / RG as f32;
                if l > strongest[col].0 || (l == strongest[col].0 && f < strongest[col].1) {
                    strongest[col] = (l, f);
                }
                if l < 2 {
                    continue;
                }
                let d = (k as f64 + 0.5) / RG as f64 * self.range_m;
                let geo = |az: f64, alt: f64| {
                    let (la, lo) = destination(ac.lat, ac.lon, wrap360(hdg + az), d);
                    eye.local(la, lo, alt)
                };
                let (bot, tp) = (ac.d.elev.max(0.0) + 300.0, (top as f64).max(ac.d.elev + 3_000.0));
                let q = [geo(a0, bot), geo(a0 + da, bot), geo(a0 + da, tp), geo(a0, tp)];
                let p: Vec<Vec2> = q.iter().filter_map(|x| v.project(*x)).collect();
                if p.len() < 4 {
                    continue;
                }
                let fade = 1.0 - f * 0.5;
                let shimmer = 0.85 + 0.15 * ((time * 2.0 + col as f64 * 0.37 + k as f64 * 0.21).sin() as f32);
                let cl = level_color(l, 0.16 * fade * shimmer);
                draw_triangle(p[0], p[1], p[2], cl);
                draw_triangle(p[0], p[2], p[3], cl);
                // Sommet de l'écho souligné
                draw_line(p[3].x, p[3].y, p[2].x, p[2].y, 1.5, level_color(l, 0.55 * fade));
            }
        }
        // Bande de synthèse sur la ligne d'horizon : plus épaisse quand l'écho est proche
        for (col, &(l, f)) in strongest.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let a0 = -HALF_SCAN as f64 + col as f64 * da;
            let dip = (crate::geo::R_EARTH / (crate::geo::R_EARTH + ac.alt.max(1.0))).acos();
            let (Some(p), Some(q)) = (v.project_dir(crate::svs::dir(wrap360(hdg + a0).to_radians(), -dip)), v.project_dir(crate::svs::dir(wrap360(hdg + a0 + da).to_radians(), -dip))) else { continue };
            let h = 4.0 + 8.0 * (1.0 - f);
            draw_triangle(p, q, q + vec2(0.0, h), level_color(l, 0.85));
            draw_triangle(p, q + vec2(0.0, h), p + vec2(0.0, h), level_color(l, 0.85));
        }
        scissor(None);
    }
}
