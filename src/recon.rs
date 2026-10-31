//! Charge utile de reconnaissance du SR-72 Darkstar : caméra EO/IR à la verticale (imagerie réelle),
//! radar à synthèse d'ouverture (SAR) à visée latérale, objectifs à collecter avec une qualité d'image
//! NIIRS, stockage embarqué et transmission des images au sol (LOS / SATCOM).

use crate::aircraft::*;
use crate::geo::*;
use crate::tiles::{self, Key, Layer, Tiles};
use crate::ui::*;
use macroquad::prelude::*;

pub const ZOOMS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
/// Champ de la caméra au grossissement 1 (degrés).
const FOV_DEG: f64 = 40.0;
/// Capteur : pixels sur la largeur de l'image.
const SENSOR_PX: f64 = 4000.0;
const STORAGE_GB: f64 = 512.0;
const IMG_GB: f64 = 1.6;
const SAR_GB: f64 = 4.0;
/// Fauchée SAR : distance latérale (m).
const SAR_NEAR: f64 = 15_000.0;
const SAR_FAR: f64 = 70_000.0;
const SAR_NIIRS: f64 = 4.0;

pub struct Target {
    pub code: &'static str,
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub req: f64,
    /// Meilleure image : (NIIRS, capteur, temps de mission).
    pub best: Option<(f64, &'static str, f64)>,
}

impl Target {
    pub fn done(&self) -> bool {
        self.best.is_some_and(|b| b.0 >= self.req)
    }
}

fn targets() -> Vec<Target> {
    let t = |code, name, lat, lon, req| Target { code, name, lat, lon, req, best: None };
    vec![
        t("A", "LAC SEC ROGERS (EDWARDS)", 34.92, -117.83, 3.0),
        t("B", "CHAMP DE TIR CHINA LAKE", 35.69, -117.69, 4.0),
        t("C", "AERODROME TONOPAH", 37.79, -116.78, 4.0),
        t("D", "VANDENBERG SLC-6", 34.58, -120.63, 5.0),
        t("E", "PORT DE LONG BEACH", 33.75, -118.21, 4.0),
        t("F", "BARRAGE HOOVER", 36.016, -114.737, 5.0),
        t("G", "ILE SAN NICOLAS", 33.25, -119.50, 3.0),
        t("H", "METEOR CRATER (ARIZONA)", 35.027, -111.022, 3.0),
    ]
}

pub struct Recon {
    pub cam_on: bool,
    pub ir: bool,
    pub zoom: usize,
    pub sar_on: bool,
    pub targets: Vec<Target>,
    pub selected: usize,
    pub steer: bool,
    pub stored_gb: f64,
    pub sent_gb: f64,
    pub events: Vec<(Level, String)>,
}

pub enum CamAction {
    ZoomIn,
    ZoomOut,
}

pub enum ReconAction {
    Camera,
    Mode,
    Sar,
    Select(usize),
    Steer,
    ZoomIn,
    ZoomOut,
}

/// Hauteur au-dessus du sol vue par les capteurs.
fn agl(ac: &Aircraft) -> f64 {
    (ac.alt - ac.d.elev).max(30.0)
}

impl Recon {
    pub fn new() -> Recon {
        Recon { cam_on: false, ir: false, zoom: 2, sar_on: false, targets: targets(), selected: 0, steer: false, stored_gb: 0.0, sent_gb: 0.0, events: Vec::new() }
    }

    /// Largeur au sol de l'image caméra (m).
    pub fn footprint(&self, ac: &Aircraft) -> f64 {
        2.0 * agl(ac) * (FOV_DEG / ZOOMS[self.zoom] / 2.0).to_radians().tan()
    }

    /// Résolution au sol (m / pixel).
    pub fn gsd(&self, ac: &Aircraft) -> f64 {
        // Limite optique (diffraction de l'objectif) : 8 cm au mieux
        (self.footprint(ac) / SENSOR_PX).max(0.08)
    }

    /// Qualité d'image NIIRS prévue (échelle 0-9) et raison d'une dégradation éventuelle.
    pub fn niirs(&self, ac: &Aircraft) -> (f64, Option<&'static str>) {
        let mut q = 5.5 - 3.32 * (self.gsd(ac) / 0.5).log10();
        let bank = ac.phi.to_degrees().abs();
        let mut why = None;
        if bank > 30.0 {
            return (0.0, Some("INCLINAISON > 30 deg"));
        }
        if bank > 10.0 {
            q -= (bank - 10.0) * 0.1;
            why = Some("VISEE OBLIQUE");
        }
        if self.ir {
            q -= 0.8;
        }
        if ac.skin_temp - 273.15 > 600.0 {
            // Fenêtre optique chaude : turbulence thermique devant le hublot
            q -= 0.5;
            why = why.or(Some("HUBLOT CHAUD"));
        }
        (q.clamp(0.0, 9.0), why)
    }

    pub fn sensors_ok(ac: &Aircraft) -> bool {
        ac.elec.bus_ok && !ac.on_ground && ac.crashed.is_none()
    }

    pub fn update(&mut self, dt: f64, ac: &mut Aircraft, mission_t: f64, link_los: bool) {
        let ok = Self::sensors_ok(ac);
        let half = self.footprint(ac) * 0.45;
        let (q_cam, _) = self.niirs(ac);
        let sensor = if self.ir { "IR" } else { "EO" };
        for t in self.targets.iter_mut() {
            let (e, n) = enu_offset(ac.lat, ac.lon, t.lat, t.lon);
            // Caméra : cible dans l'image à la verticale
            let mut shot: Option<(f64, &'static str, f64)> = None;
            if ok && self.cam_on && e.abs() < half && n.abs() < half && q_cam > 0.5 {
                shot = Some((q_cam, sensor, IMG_GB));
            }
            // SAR : fauchée latérale gauche ou droite, perpendiculaire à la route
            if ok && self.sar_on && ac.alt > 9_000.0 {
                let h = ac.psi;
                let along = e * h.sin() + n * h.cos();
                let cross = (e * h.cos() - n * h.sin()).abs();
                if along.abs() < 2_500.0 && (SAR_NEAR..SAR_FAR).contains(&cross) && shot.is_none_or(|s| s.0 < SAR_NIIRS) {
                    shot = Some((SAR_NIIRS, "SAR", SAR_GB));
                }
            }
            if let Some((q, s, gb)) = shot {
                if t.best.is_none_or(|b| q > b.0 + 0.3) {
                    if self.stored_gb + gb > STORAGE_GB {
                        self.events.push((Level::Caution, "RECON : MEMOIRE PLEINE — IMAGE PERDUE".into()));
                        continue;
                    }
                    let first = t.best.is_none();
                    t.best = Some((q, s, mission_t));
                    self.stored_gb += gb;
                    let verdict = if q >= t.req { "OBJECTIF REMPLI" } else { "QUALITE INSUFFISANTE" };
                    let lvl = if q >= t.req { Level::Info } else { Level::Caution };
                    self.events.push((lvl, format!("RECON : CIBLE {} {} — {s} NIIRS {q:.1} (requis {:.0}) — {verdict}{}", t.code, t.name, t.req, if first { "" } else { " (IMAGE AMELIOREE)" })));
                }
            }
        }
        // Guidage vers la cible sélectionnée
        if self.steer {
            let t = &self.targets[self.selected];
            if t.done() {
                self.steer = false;
                self.events.push((Level::Info, format!("RECON : CIBLE {} COLLECTEE — FIN DU GUIDAGE", t.code)));
            } else if ac.ap.on && ac.ap.lat == LatMode::Hdg {
                ac.ap.hdg_tgt = dist_brg(ac.lat, ac.lon, t.lat, t.lon).1.round();
            } else {
                self.steer = false;
            }
        }
        // Transmission au sol
        let rate = if link_los { 0.6 } else { 0.08 };
        if ac.elec.bus_ok {
            self.sent_gb = (self.sent_gb + rate * dt).min(self.stored_gb);
        }
    }

    pub fn find(&self, arg: &str) -> Option<usize> {
        self.targets.iter().position(|t| t.code == arg || t.name.starts_with(arg))
    }
}

/// Volet caméra de l'écran Darkstar : image réelle à la verticale de l'avion, cap en haut.
pub fn draw_camera(ac: &Aircraft, rc: &Recon, tiles: &mut Tiles, r: Rect) -> Option<CamAction> {
    let mut out = None;
    let grey = Color::new(0.45, 0.47, 0.5, 1.0);
    let ax = r.x + r.w - 26.0;
    let up = Rect::new(ax - 12.0, r.y + 28.0, 24.0, 22.0);
    let dn = Rect::new(ax - 12.0, r.y + 72.0, 24.0, 22.0);
    let on = rc.cam_on && Recon::sensors_ok(ac);
    let zoom = ZOOMS[rc.zoom];
    let img = Rect::new(r.x + 6.0, r.y + 26.0, r.w - 50.0, r.h - 46.0);
    if on {
        let w_m = rc.footprint(ac);
        let px_per_m = img.w as f64 / w_m;
        let dpi = screen_dpi_scale() as f64;
        let coslat = ac.lat.to_radians().cos().max(0.05);
        let z = ((156_543.03 * coslat * px_per_m * dpi).log2().round().clamp(1.0, Layer::Sat.max_z() as f64)) as u8;
        let m_per_tpx = 156_543.03 * coslat / tiles::n_tiles(z);
        let ts = (256.0 * m_per_tpx * px_per_m) as f32;
        let (fx, fy) = tiles::tile_xy(ac.lat, ac.lon, z);
        let ctr = vec2(img.x + img.w / 2.0, img.y + img.h / 2.0);
        let reach = ((img.w.hypot(img.h) / 2.0) / ts).ceil() as i64 + 1;
        let n = tiles::n_tiles(z) as i64;
        let tint = if rc.ir { Color::new(0.55, 1.0, 0.6, 1.0) } else { WHITE };
        draw_rectangle(img.x, img.y, img.w, img.h, Color::new(0.05, 0.06, 0.06, 1.0));
        scissor(Some(img));
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (tx, ty) = (fx.floor() as i64 + dx, fy.floor() as i64 + dy);
                if ty < 0 || ty >= n {
                    continue;
                }
                let key = Key { layer: Layer::Sat, z, x: tx.rem_euclid(n) as u32, y: ty as u32 };
                let prio = z as f32 * 0.2 + (dx.abs() + dy.abs()) as f32 * 0.1 - 2.0;
                if let Some(t) = tiles.get(key, prio) {
                    let off = vec2((tx as f64 - fx) as f32 * ts, (ty as f64 - fy) as f32 * ts);
                    let (tw, th) = (t.tex.width(), t.tex.height());
                    draw_texture_ex(
                        t.tex,
                        ctr.x + off.x,
                        ctr.y + off.y,
                        tint,
                        DrawTextureParams {
                            dest_size: Some(vec2(ts + 1.0, ts + 1.0)),
                            source: Some(Rect::new(t.uv.x * tw, t.uv.y * th, t.uv.w * tw, t.uv.h * th)),
                            rotation: -ac.psi as f32,
                            pivot: Some(ctr),
                            ..Default::default()
                        },
                    );
                }
            }
        }
        // Cibles visibles dans l'image
        for t in &rc.targets {
            let (e, nn) = enu_offset(ac.lat, ac.lon, t.lat, t.lon);
            let (s, c) = (ac.psi.sin(), ac.psi.cos());
            let (xr, yr) = (e * c - nn * s, e * s + nn * c);
            let p = ctr + vec2((xr * px_per_m) as f32, (-yr * px_per_m) as f32);
            if img.contains(p) {
                let col = if t.done() { GREEN_S } else { AMBER };
                stroke(&round_corners(&[p + vec2(-12.0, -12.0), p + vec2(12.0, -12.0), p + vec2(12.0, 12.0), p + vec2(-12.0, 12.0)], true, 4.0), true, 1.5, col);
                txt(t.code, p.x + 15.0, p.y - 8.0, 13.0, col);
            }
        }
        // Réticule et échelle
        let rc_col = Color::new(1.0, 1.0, 1.0, 0.85);
        for (a, b) in [(vec2(-30.0, 0.0), vec2(-8.0, 0.0)), (vec2(8.0, 0.0), vec2(30.0, 0.0)), (vec2(0.0, -30.0), vec2(0.0, -8.0)), (vec2(0.0, 8.0), vec2(0.0, 30.0))] {
            round_line(ctr + a, ctr + b, 1.5, rc_col);
        }
        let bar_m = [10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0].iter().copied().find(|m| m * px_per_m > 50.0).unwrap_or(10_000.0);
        let bw = (bar_m * px_per_m) as f32;
        let (bx, by) = (img.x + 10.0, img.y + img.h - 10.0);
        round_line(vec2(bx, by), vec2(bx + bw, by), 2.0, WHITE);
        txt(&if bar_m >= 1000.0 { format!("{:.0} km", bar_m / 1000.0) } else { format!("{bar_m:.0} m") }, bx, by - 5.0, 11.0, WHITE);
        scissor(None);
        let (q, why) = rc.niirs(ac);
        txt(&format!("{} x{zoom:.0}  GSD {:.2} m  NIIRS {q:.1}", if rc.ir { "IR" } else { "EO" }, rc.gsd(ac)), r.x + 10.0, r.y + 17.0, 12.0, if why.is_some() { AMBER } else { GREEN_S });
        if let Some(w) = why {
            txt_r(w, img.x + img.w - 6.0, img.y + 16.0, 11.0, AMBER);
        }
        draw_rectangle_lines(img.x, img.y, img.w, img.h, 1.0, Color::new(0.5, 0.52, 0.55, 1.0));
    } else {
        txt_c("Camera Zoom Elevation :N/A", r.x + r.w / 2.0 - 10.0, r.y + 17.0, 13.0, WHITE);
        txt_c("Camera unavailable", r.x + r.w / 2.0 - 8.0, r.y + r.h / 2.0 + 6.0, 18.0, WHITE);
        let why = if !ac.elec.bus_ok { "BUS ELEC PERDU" } else if ac.on_ground { "AU SOL" } else { "CAMERA ON pour l'activer" };
        txt_c(why, r.x + r.w / 2.0 - 8.0, r.y + r.h / 2.0 + 26.0, 11.0, DIM);
    }
    // Commandes de grossissement
    draw_triangle(vec2(ax, up.y + 2.0), vec2(ax - 10.0, up.y + 20.0), vec2(ax + 10.0, up.y + 20.0), if hovered(up) { WHITE } else { grey });
    txt_c(&format!("x{zoom:.0}"), ax, r.y + 64.0, 11.0, WHITE);
    draw_triangle(vec2(ax, dn.y + 20.0), vec2(ax - 10.0, dn.y + 2.0), vec2(ax + 10.0, dn.y + 2.0), if hovered(dn) { WHITE } else { grey });
    if clicked(up) {
        out = Some(CamAction::ZoomIn);
    }
    if clicked(dn) {
        out = Some(CamAction::ZoomOut);
    }
    out
}

fn toggle(r: Rect, label: &str, on: bool, on_txt: &str, off_txt: &str) -> bool {
    rrect(r, 6.0, if on { Color::new(0.07, 0.25, 0.14, 1.0) } else if hovered(r) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
    rrect_lines(r, 6.0, 1.2, if on { GREEN_S } else { PANEL_BORDER });
    txt(label, r.x + 10.0, r.y + r.h / 2.0 + 5.0, 13.0, WHITE);
    txt_r(if on { on_txt } else { off_txt }, r.x + r.w - 10.0, r.y + r.h / 2.0 + 5.0, 13.0, if on { GREEN_S } else { DIM });
    clicked(r)
}

/// Page RECON : capteurs, objectifs, stockage et transmission.
pub fn draw_recon(ac: &Aircraft, rc: &Recon, r: Rect, link_name: &str) -> Option<ReconAction> {
    let mut out = None;
    panel(r, "RECON - CHARGE UTILE DE RECONNAISSANCE");
    let lw = (r.w * 0.40).min(330.0);
    let x = r.x + 12.0;
    let mut y = r.y + 34.0;
    let bh = 26.0;
    txt("CAPTEURS", x, y + 4.0, 12.0, ACCENT);
    y += 12.0;
    if toggle(Rect::new(x, y, lw, bh), "CAMERA EO/IR (verticale)", rc.cam_on, "ON", "OFF") {
        out = Some(ReconAction::Camera);
    }
    y += bh + 6.0;
    let half = (lw - 6.0) / 2.0;
    if toggle(Rect::new(x, y, half, bh), "MODE", rc.ir, "IR", "EO") {
        out = Some(ReconAction::Mode);
    }
    let zr = Rect::new(x + half + 6.0, y, half, bh);
    rrect(zr, 6.0, Color::new(0.08, 0.09, 0.11, 1.0));
    rrect_lines(zr, 6.0, 1.2, PANEL_BORDER);
    txt_c(&format!("ZOOM x{:.0}", ZOOMS[rc.zoom]), zr.x + zr.w / 2.0, zr.y + 18.0, 13.0, WHITE);
    let (zm, zp) = (Rect::new(zr.x, zr.y, 26.0, bh), Rect::new(zr.x + zr.w - 26.0, zr.y, 26.0, bh));
    txt_c("-", zm.x + 13.0, zm.y + 18.0, 15.0, if hovered(zm) { WHITE } else { DIM });
    txt_c("+", zp.x + 13.0, zp.y + 18.0, 15.0, if hovered(zp) { WHITE } else { DIM });
    if clicked(zm) {
        out = Some(ReconAction::ZoomOut);
    } else if clicked(zp) {
        out = Some(ReconAction::ZoomIn);
    }
    y += bh + 6.0;
    if toggle(Rect::new(x, y, lw, bh), "RADAR SAR (lateral, tout temps)", rc.sar_on, "ON", "OFF") {
        out = Some(ReconAction::Sar);
    }
    y += bh + 14.0;
    let line = |y: &mut f32, l: &str, v: String, c: Color| {
        txt(l, x, *y, 12.0, DIM);
        txt_r(&v, x + lw, *y, 12.0, c);
        *y += 17.0;
    };
    let (q, why) = rc.niirs(ac);
    line(&mut y, "LARGEUR AU SOL", format!("{:.1} km", rc.footprint(ac) / 1000.0), WHITE);
    line(&mut y, "RESOLUTION (GSD)", format!("{:.2} m", rc.gsd(ac)), WHITE);
    line(&mut y, "QUALITE PREVUE", format!("NIIRS {q:.1}"), if why.is_some() { AMBER } else { GREEN_S });
    if let Some(w) = why {
        line(&mut y, "DEGRADATION", w.to_string(), AMBER);
    }
    line(&mut y, "FAUCHEE SAR", format!("{:.0}-{:.0} km G/D", SAR_NEAR / 1000.0, SAR_FAR / 1000.0), if rc.sar_on { CYAN_S } else { DIM });
    line(&mut y, "ALIMENTATION", if ac.elec.bus_ok { "BUS OK".into() } else { "PERDUE".into() }, if ac.elec.bus_ok { GREEN_S } else { RED });
    y += 6.0;
    // Stockage et liaison
    txt("STOCKAGE / TRANSMISSION", x, y, 12.0, ACCENT);
    y += 8.0;
    let bar = |y: f32, f: f64, c: Color, label: String| {
        let b = Rect::new(x, y, lw, 14.0);
        rrect(b, 4.0, Color::new(0.06, 0.07, 0.08, 1.0));
        if f > 0.005 {
            rrect(Rect::new(b.x, b.y, b.w * f.clamp(0.0, 1.0) as f32, b.h), 4.0, c);
        }
        txt_c(&label, b.x + b.w / 2.0, b.y + 11.0, 11.0, WHITE);
    };
    bar(y, rc.stored_gb / STORAGE_GB, Color::new(0.2, 0.45, 0.7, 1.0), format!("MEMOIRE {:.0} / {:.0} Go", rc.stored_gb, STORAGE_GB));
    y += 20.0;
    let pend = if rc.stored_gb > 0.0 { rc.sent_gb / rc.stored_gb } else { 1.0 };
    bar(y, pend, Color::new(0.2, 0.6, 0.35, 1.0), format!("TRANSMIS {:.0} %  ({link_name})", pend * 100.0));

    // Objectifs
    let tx = x + lw + 18.0;
    let tw = r.x + r.w - 12.0 - tx;
    let mut ty = r.y + 38.0;
    txt("OBJECTIFS", tx, ty, 12.0, ACCENT);
    let done = rc.targets.iter().filter(|t| t.done()).count();
    txt_r(&format!("{done} / {} REMPLIS", rc.targets.len()), tx + tw, ty, 12.0, if done == rc.targets.len() { GREEN_S } else { WHITE });
    ty += 10.0;
    let rh = ((r.y + r.h - ty - 56.0) / rc.targets.len() as f32).clamp(22.0, 40.0);
    for (i, t) in rc.targets.iter().enumerate() {
        let row = Rect::new(tx, ty, tw, rh - 4.0);
        let sel = i == rc.selected;
        rrect(row, 6.0, if sel { Color::new(0.10, 0.16, 0.22, 1.0) } else if hovered(row) { Color::new(0.08, 0.10, 0.13, 1.0) } else { Color::new(0.06, 0.07, 0.09, 1.0) });
        if sel {
            rrect_lines(row, 6.0, 1.2, ACCENT);
        }
        let (d, b) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let st_col = if t.done() { GREEN_S } else if t.best.is_some() { AMBER } else { DIM };
        draw_circle(row.x + 14.0, row.y + row.h / 2.0, 8.0, st_col);
        txt_c(t.code, row.x + 14.0, row.y + row.h / 2.0 + 5.0, 12.0, BLACK);
        txt(t.name, row.x + 30.0, row.y + row.h / 2.0 + 5.0, 12.0, WHITE);
        let status = match t.best {
            Some((q, s, _)) => format!("{s} NIIRS {q:.1}/{:.0}", t.req),
            None => format!("{:03.0} deg {:.0} NM  req {:.0}", b, d / NM, t.req),
        };
        txt_r(&status, row.x + row.w - 10.0, row.y + row.h / 2.0 + 5.0, 11.0, st_col);
        if clicked(row) {
            out = Some(ReconAction::Select(i));
        }
        ty += rh;
    }
    let sb = Rect::new(tx, ty + 4.0, tw, 30.0);
    let steer_lbl = if rc.steer { format!("GUIDAGE AP VERS CIBLE {} ACTIF", rc.targets[rc.selected].code) } else { format!("DIRECT CIBLE {} (cap autopilote)", rc.targets[rc.selected].code) };
    rrect(sb, 6.0, if rc.steer { Color::new(0.3, 0.1, 0.3, 1.0) } else if hovered(sb) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
    rrect_lines(sb, 6.0, 1.2, MAGENTA_S);
    txt_c(&steer_lbl, sb.x + sb.w / 2.0, sb.y + 20.0, 13.0, MAGENTA_S);
    if clicked(sb) {
        out = Some(ReconAction::Steer);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn over(lat: f64, lon: f64, alt_ft: f64) -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.lat = lat;
        ac.lon = lon;
        ac.alt = alt_ft * FT;
        ac
    }

    /// Survol à la verticale avec caméra : cible collectée, qualité fonction du grossissement.
    #[test]
    fn camera_collects_target() {
        let mut rc = Recon::new();
        let t = &rc.targets[0];
        let mut ac = over(t.lat, t.lon, 80_000.0);
        rc.cam_on = true;
        rc.zoom = 4;
        let (q, _) = rc.niirs(&ac);
        println!("x16 a 80 000 ft : largeur {:.1} km, GSD {:.2} m, NIIRS {q:.1}", rc.footprint(&ac) / 1000.0, rc.gsd(&ac));
        rc.update(0.1, &mut ac, 10.0, true);
        assert!(rc.targets[0].done(), "{:?}", rc.targets[0].best);
        assert!(rc.stored_gb > 0.0);
    }

    /// Le SAR voit une cible située à 30 km sur le côté, pas la caméra.
    #[test]
    fn sar_side_looking() {
        let mut rc = Recon::new();
        let t = &rc.targets[1];
        let (la, lo) = from_enu(t.lat, t.lon, -30_000.0, 0.0);
        let mut ac = over(la, lo, 75_000.0);
        ac.psi = 0.0;
        rc.sar_on = true;
        rc.update(0.1, &mut ac, 10.0, true);
        assert_eq!(rc.targets[1].best.map(|b| b.1), Some("SAR"));
    }

    /// Inclinaison excessive : pas d'image exploitable.
    #[test]
    fn steep_bank_no_image() {
        let mut rc = Recon::new();
        let t = &rc.targets[0];
        let mut ac = over(t.lat, t.lon, 60_000.0);
        ac.phi = 45f64.to_radians();
        rc.cam_on = true;
        rc.update(0.1, &mut ac, 10.0, true);
        assert!(rc.targets[0].best.is_none());
    }
}
