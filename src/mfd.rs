//! Écran principal Darkstar : PFD en haut, volets carburant / caméra / HSI en bas.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::pfd::{self, PfdInfo};
use crate::svs::{self, CamView, SvsStyle, View};
use crate::traffic::{Tcas, TrafficSys};
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;

pub const CAM_ZOOMS: [f32; 4] = [1.0, 2.0, 4.0, 10.0];
pub const HSI_RANGES: [f64; 7] = [10.0, 20.0, 40.0, 80.0, 160.0, 320.0, 640.0];

pub struct MfdState {
    pub cam_zoom: usize,
    pub cam_elev: f64, // degrés
    pub hsi_range: usize,
}

impl MfdState {
    pub fn new() -> Self {
        MfdState { cam_zoom: 1, cam_elev: -4.0, hsi_range: 2 }
    }
}

/// Actions demandées par des clics sur l'écran.
#[derive(Default)]
pub struct MfdActions {
    pub fuel_dump: bool,
}

pub struct Layout {
    pub pfd: Rect,
    pub svs: Rect,
}

fn bezel(r: Rect) -> Rect {
    rrect(r, 18.0, Color::new(0.13, 0.14, 0.15, 1.0));
    rrect_lines(r, 18.0, 1.5, Color::new(0.26, 0.27, 0.29, 1.0));
    let d = Rect::new(r.x + 12.0, r.y + 12.0, r.w - 24.0, r.h - 24.0);
    rrect(d, 8.0, BLACK);
    d
}

pub fn layout(r: Rect) -> (Rect, Layout, [Rect; 3]) {
    let d = Rect::new(r.x + 12.0, r.y + 12.0, r.w - 24.0, r.h - 24.0);
    let pfd_r = Rect::new(d.x + 6.0, d.y + 6.0, d.w - 12.0, (d.h * 0.58).floor());
    let y0 = pfd_r.y + pfd_r.h + 8.0;
    let h = d.y + d.h - 6.0 - y0;
    let w = (d.w - 12.0 - 12.0) / 3.0;
    let panes = [
        Rect::new(d.x + 6.0, y0, w, h),
        Rect::new(d.x + 12.0 + w, y0, w, h),
        Rect::new(d.x + 18.0 + 2.0 * w, y0, w, h),
    ];
    (d, Layout { pfd: pfd_r, svs: pfd::svs_rect(pfd_r) }, panes)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_mfd(world: &World, ac: &Aircraft, traffic: &TrafficSys, r: Rect, st: &mut MfdState, info: &PfdInfo, style: SvsStyle, time: f64) -> (View, MfdActions) {
    bezel(r);
    let (_, lay, panes) = layout(r);
    let view = svs::draw_svs(world, ac, traffic, lay.svs, time, style, None);
    pfd::draw_pfd(ac, &view, lay.pfd, info);
    let mut act = MfdActions::default();
    act.fuel_dump = draw_fuel(ac, panes[0]);
    draw_camera(world, ac, traffic, panes[1], st, style, time);
    draw_hsi(ac, traffic, panes[2], st, info.dest);
    (view, act)
}

fn pane_frame(r: Rect) {
    rrect_lines(r, 10.0, 1.5, Color::new(0.78, 0.80, 0.82, 0.85));
}

fn value_box(x: f32, y: f32, w: f32, s: &str, c: Color) {
    draw_rectangle(x, y - 14.0, w, 18.0, BLACK);
    draw_rectangle_lines(x, y - 14.0, w, 18.0, 1.0, Color::new(0.7, 0.72, 0.75, 1.0));
    txt_c(s, x + w / 2.0, y, 15.0, c);
}

/// Contenu des réservoirs JP-7 (le n°4, arrière, se vide en premier).
fn tank_fracs(ac: &Aircraft) -> [f32; 4] {
    let cap = FUEL_MAX / 4.0;
    let mut out = [0.0f32; 4];
    for (i, o) in out.iter_mut().enumerate() {
        *o = ((ac.fuel - i as f64 * cap) / cap).clamp(0.0, 1.0) as f32;
    }
    out
}

/// Volet carburant : débit, pressions des pompes, réservoirs, centrage, vidange. Renvoie vrai si DUMP cliqué.
fn draw_fuel(ac: &Aircraft, r: Rect) -> bool {
    pane_frame(r);
    let e = &ac.eng;
    let s = (r.w / 270.0).min(r.h / 260.0).clamp(0.7, 1.3);
    // Débit carburant
    txt("FUEL FLOW", r.x + 10.0, r.y + 18.0, 13.0, WHITE);
    let pph = e.fuel_flow * 3600.0 * 2.2046 / 1000.0;
    let g = 20.0 * s;
    arc_gauge(r.x + 18.0 + g, r.y + 30.0 + g, g, (pph / 300.0) as f32, 0.85);
    txt("PPH", r.x + 12.0, r.y + 52.0 + 2.0 * g, 14.0, WHITE);
    value_box(r.x + 46.0, r.y + 52.0 + 2.0 * g, 38.0, &format!("{pph:03.0}"), WHITE);
    txt("(x1000)", r.x + 10.0, r.y + 66.0 + 2.0 * g, 10.0, DIM);
    if e.h2_flow > 0.0 {
        txt(&format!("H2 {:.1}kg/s", e.h2_flow), r.x + 10.0, r.y + 80.0 + 2.0 * g, 11.0, CYAN_S);
    }

    // Pressions : jauge principale + pompes 1 à 4
    let fp = ac.fuel_psi();
    txt_r("FUEL PRES.", r.x + r.w - 10.0, r.y + 18.0, 13.0, WHITE);
    let gx = r.x + r.w - 14.0 - g;
    arc_gauge(gx, r.y + 30.0 + g, g, (fp / 120.0) as f32, 0.88);
    txt_c("1", gx, r.y + 30.0 + g * 1.5, 11.0, WHITE);
    txt("Psi", r.x + r.w - 104.0, r.y + 52.0 + 2.0 * g, 13.0, WHITE);
    value_box(r.x + r.w - 72.0, r.y + 52.0 + 2.0 * g, 62.0, &format!("{fp:.2}"), if fp < 40.0 { AMBER } else { WHITE });
    let tanks = tank_fracs(ac);
    let pumps_on = fp > 20.0;
    let sg = 13.0 * s;
    for (i, t) in tanks.iter().enumerate() {
        let cy = r.y + 92.0 + 2.0 * g + i as f32 * (2.0 * sg + 10.0);
        if cy + sg > r.y + r.h - 4.0 {
            break;
        }
        let p = if pumps_on && *t > 0.0 { fp / 120.0 } else { 0.0 };
        arc_gauge(gx, cy, sg, p as f32, 0.88);
        txt_c(&format!("{}", i + 1), gx, cy + sg * 0.75, 10.0, WHITE);
    }

    // Silhouette et réservoirs
    let cx = r.x + r.w * 0.47;
    let top = r.y + 26.0;
    let bot = r.y + r.h - 12.0;
    let h = bot - top;
    let hw = r.w * 0.17;
    let outline = [(0.0, 0.0), (0.22, 0.48), (1.0, 0.82), (1.0, 0.96), (0.35, 1.0), (-0.35, 1.0), (-1.0, 0.96), (-1.0, 0.82), (-0.22, 0.48)];
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        draw_line(cx + a.0 * hw, top + a.1 * h, cx + b.0 * hw, top + b.1 * h, 2.0, GREEN_S);
    }
    let h2f = (ac.h2 / H2_MAX) as f32;
    let blocks = [(0.30, 0.42, tanks[0], GREEN_S), (0.46, 0.26, h2f, CYAN_S), (0.62, 0.30, tanks[1], GREEN_S), (0.76, 0.34, tanks[2], GREEN_S), (0.90, 0.60, tanks[3], GREEN_S)];
    for (fy, fw, f, c) in blocks {
        let bw = hw * fw * 2.0;
        let bh = h * 0.075;
        let (bx, by) = (cx - bw / 2.0, top + fy * h - bh / 2.0);
        draw_rectangle_lines(bx, by, bw, bh, 1.0, Color::new(c.r, c.g, c.b, 0.6));
        draw_rectangle(bx + 1.0, by + 1.0 + (bh - 2.0) * (1.0 - f), bw - 2.0, (bh - 2.0) * f, c);
    }

    // Centrage et vidange
    let cg = ac.cg();
    let cgc = if (21.0..=29.0).contains(&cg) { GREEN_S } else { AMBER };
    txt(&format!("CG {cg:.0}%"), r.x + 10.0, r.y + r.h * 0.62, 15.0, cgc);
    txt(&format!("JP7 {:.1}t", ac.fuel / 1000.0), r.x + 10.0, r.y + r.h * 0.62 + 16.0, 11.0, GREEN_S);
    txt(&format!("H2 {:.1}t {:.0}psi", ac.h2 / 1000.0, ac.h2_psi()), r.x + 10.0, r.y + r.h * 0.62 + 30.0, 11.0, CYAN_S);
    let b = Rect::new(r.x + 10.0, r.y + r.h * 0.62 + 40.0, 48.0, 20.0);
    if ac.fuel_dump {
        draw_rectangle(b.x, b.y, b.w, b.h, AMBER);
    }
    for i in 0..8 {
        // contour pointillé
        let t0 = i as f32 / 8.0;
        let t1 = t0 + 0.07;
        draw_line(b.x + b.w * t0, b.y, b.x + b.w * t1, b.y, 1.5, WHITE);
        draw_line(b.x + b.w * t0, b.y + b.h, b.x + b.w * t1, b.y + b.h, 1.5, WHITE);
    }
    draw_line(b.x, b.y, b.x, b.y + b.h, 1.5, WHITE);
    draw_line(b.x + b.w, b.y, b.x + b.w, b.y + b.h, 1.5, WHITE);
    txt_c("DUMP", b.x + b.w / 2.0, b.y + 15.0, 13.0, if ac.fuel_dump { BLACK } else { WHITE });
    clicked(b)
}

/// Volet caméra : SVS stabilisé (cap, élévation) avec zoom, coordonnées en bas.
fn draw_camera(world: &World, ac: &Aircraft, traffic: &TrafficSys, r: Rect, st: &mut MfdState, style: SvsStyle, time: f64) {
    pane_frame(r);
    let zoom = CAM_ZOOMS[st.cam_zoom];
    txt_c(&format!("Camera Zoom {:.0}x  Elevation {:+.0}", zoom, st.cam_elev), r.x + r.w / 2.0, r.y + 17.0, 13.0, WHITE);
    let img = Rect::new(r.x + 8.0, r.y + 24.0, r.w - 44.0, r.h - 50.0);
    let cam = CamView { hdg: ac.psi, pitch: st.cam_elev.to_radians(), fovy: svs::FOVY_DEG / zoom };
    if ac.crashed.is_some() {
        txt_c("Camera unavailable", img.x + img.w / 2.0, img.y + img.h / 2.0, 18.0, WHITE);
    } else {
        let v = svs::draw_svs(world, ac, traffic, img, time, style, Some(cam));
        let (cx, cy) = (img.x + img.w / 2.0, img.y + img.h / 2.0);
        let c = Color::new(1.0, 1.0, 1.0, 0.8);
        for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            draw_line(cx + dx * 8.0, cy + dy * 8.0, cx + dx * 22.0, cy + dy * 22.0, 1.5, c);
        }
        let _ = v;
    }
    draw_rectangle_lines(img.x, img.y, img.w, img.h, 1.0, Color::new(0.4, 0.42, 0.45, 1.0));
    // Boutons zoom (flèches) à droite
    let bx = r.x + r.w - 30.0;
    let up = Rect::new(bx, r.y + 30.0, 22.0, 22.0);
    let dn = Rect::new(bx, r.y + 78.0, 22.0, 22.0);
    let grey = Color::new(0.55, 0.57, 0.6, 1.0);
    draw_triangle(vec2(up.x + 11.0, up.y + 2.0), vec2(up.x + 1.0, up.y + 20.0), vec2(up.x + 21.0, up.y + 20.0), grey);
    draw_triangle(vec2(dn.x + 11.0, dn.y + 20.0), vec2(dn.x + 1.0, dn.y + 2.0), vec2(dn.x + 21.0, dn.y + 2.0), grey);
    txt_c(&format!("1/{:.0}", zoom), bx + 11.0, r.y + 70.0, 11.0, WHITE);
    if clicked(up) {
        st.cam_zoom = (st.cam_zoom + 1).min(CAM_ZOOMS.len() - 1);
    }
    if clicked(dn) {
        st.cam_zoom = st.cam_zoom.saturating_sub(1);
    }
    // Élévation : flèches basses
    let eu = Rect::new(bx, r.y + r.h - 82.0, 22.0, 20.0);
    let ed = Rect::new(bx, r.y + r.h - 56.0, 22.0, 20.0);
    draw_triangle(vec2(eu.x + 11.0, eu.y + 2.0), vec2(eu.x + 3.0, eu.y + 18.0), vec2(eu.x + 19.0, eu.y + 18.0), grey);
    draw_triangle(vec2(ed.x + 11.0, ed.y + 18.0), vec2(ed.x + 3.0, ed.y + 2.0), vec2(ed.x + 19.0, ed.y + 2.0), grey);
    if clicked(eu) {
        st.cam_elev = (st.cam_elev + 2.0).min(20.0);
    }
    if clicked(ed) {
        st.cam_elev = (st.cam_elev - 2.0).max(-60.0);
    }
    // Coordonnées
    let dms = |v: f64, pos: char, neg: char| {
        let a = v.abs();
        let d = a.floor();
        let m = ((a - d) * 60.0).floor();
        let s = ((a - d) * 60.0 - m) * 60.0;
        format!("{d:.0}°{m:02.0}'{s:05.2}\" {}", if v >= 0.0 { pos } else { neg })
    };
    txt_c(&format!("{} / {}", dms(ac.lat, 'N', 'S'), dms(ac.lon, 'E', 'W')), r.x + r.w / 2.0, r.y + r.h - 8.0, 13.0, WHITE);
}

/// Volet HSI : rose des caps tournante, route magenta, cercles de distance, aérodromes et trafic.
fn draw_hsi(ac: &Aircraft, traffic: &TrafficSys, r: Rect, st: &mut MfdState, dest: usize) {
    pane_frame(r);
    let hdg = ac.hdg_deg();
    let range = HSI_RANGES[st.hsi_range];
    // En-tête
    txt("HDG", r.x + r.w * 0.30, r.y + 18.0, 13.0, GREEN_S);
    value_box(r.x + r.w * 0.30 + 32.0, r.y + 18.0, 38.0, &format!("{:03.0}", hdg.round() % 360.0), WHITE);
    txt("TRU", r.x + r.w * 0.30 + 76.0, r.y + 18.0, 13.0, GREEN_S);
    let up = Rect::new(r.x + r.w - 30.0, r.y + 12.0, 22.0, 20.0);
    let dn = Rect::new(r.x + r.w - 30.0, r.y + 52.0, 22.0, 20.0);
    let grey = Color::new(0.55, 0.57, 0.6, 1.0);
    draw_triangle(vec2(up.x + 11.0, up.y + 2.0), vec2(up.x + 1.0, up.y + 18.0), vec2(up.x + 21.0, up.y + 18.0), grey);
    draw_triangle(vec2(dn.x + 11.0, dn.y + 18.0), vec2(dn.x + 1.0, dn.y + 2.0), vec2(dn.x + 21.0, dn.y + 2.0), grey);
    txt_c(&format!("{range:.0}"), up.x + 11.0, r.y + 47.0, 13.0, WHITE);
    if clicked(dn) {
        st.hsi_range = (st.hsi_range + 1).min(HSI_RANGES.len() - 1);
    }
    if clicked(up) {
        st.hsi_range = st.hsi_range.saturating_sub(1);
    }

    let cx = r.x + r.w / 2.0;
    let cy = r.y + r.h * 0.60;
    let rad = (r.w * 0.44).min(r.h * 0.48);
    scissor(Some(Rect::new(r.x + 2.0, r.y + 26.0, r.w - 4.0, r.h - 28.0)));
    // Rose des caps
    let white = Color::new(0.92, 0.94, 0.95, 1.0);
    for k in 0..72 {
        let b = k as f64 * 5.0;
        let a = ((b - hdg) as f32 - 90.0).to_radians();
        let l = if k % 6 == 0 { 12.0 } else if k % 2 == 0 { 8.0 } else { 5.0 };
        draw_line(cx + rad * a.cos(), cy + rad * a.sin(), cx + (rad - l) * a.cos(), cy + (rad - l) * a.sin(), 1.5, white);
        if k % 6 == 0 {
            let lbl = format!("{}", (k / 2) % 36);
            txt_c(&lbl, cx + (rad - 24.0) * a.cos(), cy + (rad - 24.0) * a.sin() + 5.0, 15.0, white);
        }
    }
    arc(cx, cy, rad, 0.0, std::f32::consts::TAU, 1.0, Color::new(1.0, 1.0, 1.0, 0.25));
    for k in [1.0f32 / 3.0, 2.0 / 3.0] {
        arc(cx, cy, rad * k, 0.0, std::f32::consts::TAU, 1.0, Color::new(1.0, 1.0, 1.0, 0.45));
    }
    // Repère de cap fixe
    let lub = [vec2(cx, cy - rad + 2.0), vec2(cx - 7.0, cy - rad - 10.0), vec2(cx + 7.0, cy - rad - 10.0)];
    for i in 0..3 {
        draw_line(lub[i].x, lub[i].y, lub[(i + 1) % 3].x, lub[(i + 1) % 3].y, 1.5, white);
    }
    let px_per_m = rad / (range * NM) as f32;
    let to_s = |lat: f64, lon: f64| {
        let (e, n) = enu_offset(ac.lat, ac.lon, lat, lon);
        let a = -(hdg.to_radians());
        let (sa, ca) = (a.sin(), a.cos());
        let x = e * ca + n * sa;
        let y = -e * sa + n * ca;
        vec2(cx + x as f32 * px_per_m, cy - y as f32 * px_per_m)
    };
    // Aérodromes
    for (i, a) in AIRPORTS.iter().enumerate() {
        let (d, _) = dist_brg(ac.lat, ac.lon, a.lat, a.lon);
        if d > range * NM * 1.3 {
            continue;
        }
        let p = to_s(a.lat, a.lon);
        let c = if i == dest { MAGENTA_S } else { CYAN_S };
        draw_circle_lines(p.x, p.y, 4.0, 1.5, c);
        txt(a.icao, p.x + 6.0, p.y + 4.0, 11.0, c);
    }
    // Route vers la destination
    let apt = &AIRPORTS[dest];
    let (dist, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    let a = ((brg - hdg) as f32 - 90.0).to_radians();
    let len = ((dist * px_per_m as f64) as f32).min(rad * 1.2);
    draw_line(cx, cy, cx + len * a.cos(), cy + len * a.sin(), 3.0, MAGENTA_S);
    // Trafic
    for t in &traffic.list {
        let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        if d > range * NM * 1.2 {
            continue;
        }
        let p = to_s(t.lat, t.lon);
        let c = match t.tcas {
            Tcas::Ra => RED,
            Tcas::Ta => AMBER,
            Tcas::Proximate => WHITE,
            Tcas::None => Color::new(0.6, 0.9, 1.0, 0.8),
        };
        match t.tcas {
            Tcas::Ra => draw_rectangle(p.x - 4.0, p.y - 4.0, 8.0, 8.0, c),
            Tcas::Ta => draw_circle(p.x, p.y, 4.5, c),
            _ => draw_poly_lines(p.x, p.y, 4, 5.0, 0.0, 1.2, c),
        }
        let dz = ((t.alt - ac.alt) / FT / 100.0).round() as i32;
        if dz.abs() < 100 {
            txt(&format!("{dz:+03}"), p.x + 6.0, p.y - 3.0, 10.0, c);
        }
    }
    // Avion
    let y = Color::new(1.0, 0.85, 0.1, 1.0);
    draw_triangle(vec2(cx, cy - 12.0), vec2(cx - 9.0, cy + 7.0), vec2(cx + 9.0, cy + 7.0), y);
    draw_rectangle(cx - 2.0, cy + 6.0, 4.0, 5.0, y);
    scissor(None);
    txt_r(&format!("{} {:.0}NM", apt.icao, dist / NM), r.x + r.w - 8.0, r.y + r.h - 8.0, 12.0, MAGENTA_S);
}
