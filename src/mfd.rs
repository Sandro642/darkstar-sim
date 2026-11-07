//! Écran principal Darkstar, proportions du cockpit : rangée de 6 boutons, PFD en haut,
//! volets carburant / caméra / HSI en bas.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::pfd::{self, PfdInfo};
use crate::svs::{self, SvsStyle, View};
use crate::ui::*;
use crate::world::World;
use macroquad::prelude::*;

pub const HSI_RANGES: [f64; 7] = [10.0, 20.0, 40.0, 80.0, 160.0, 320.0, 640.0];

pub struct MfdState {
    pub hsi_range: usize,
    /// Radar météo (peint sur l'écran de navigation et dans l'horizon).
    pub wxr: crate::wxr::Wxr,
}

/// Portée du radar météo (limitée à 320 NM comme sur Airbus).
fn wxr_range(st: &MfdState) -> f64 {
    HSI_RANGES[st.hsi_range].min(320.0)
}

impl MfdState {
    pub fn new() -> Self {
        MfdState { hsi_range: 2, wxr: crate::wxr::Wxr::new() }
    }
}

/// Actions demandées par des clics sur l'écran.
#[derive(Default)]
pub struct MfdActions {
    pub fuel_dump: bool,
    /// Commande de grossissement de la caméra de reconnaissance.
    pub cam: Option<crate::recon::CamAction>,
    /// Bouton de lunette cliqué (0..5).
    pub bezel: Option<usize>,
}

/// Hauteur de la rangée de boutons au-dessus de l'écran.
const BEZEL_TOP: f32 = 58.0;

pub struct Layout {
    pub pfd: Rect,
    pub svs: Rect,
    /// Zone centrale libre du PFD v2 (entre les échelles).
    pub inner: Rect,
}

/// Lunette : fond, 6 boutons carrés (allumés en turquoise) et voyant ; renvoie le bouton cliqué.
fn bezel(r: Rect, lit: [bool; 6], time: f64) -> Option<usize> {
    rrect(r, 18.0, Color::new(0.11, 0.115, 0.12, 1.0));
    rrect_lines(r, 18.0, 1.5, Color::new(0.24, 0.25, 0.27, 1.0));
    let mut out = None;
    let s = 40.0;
    for i in 0..6 {
        let b = Rect::new(r.x + 34.0 + i as f32 * (s + 14.0), r.y + (BEZEL_TOP - s) / 2.0 + 4.0, s, s);
        let c = if lit[i] { Color::new(0.45, 0.95, 0.85, 1.0) } else { Color::new(0.16, 0.30, 0.27, 1.0) };
        if lit[i] {
            draw_rectangle(b.x - 3.0, b.y - 3.0, b.w + 6.0, b.h + 6.0, Color::new(0.4, 0.95, 0.85, 0.18));
        }
        rrect(b, 4.0, c);
        draw_rectangle(b.x + 4.0, b.y + 4.0, b.w - 8.0, b.h * 0.3, Color::new(1.0, 1.0, 1.0, if lit[i] { 0.25 } else { 0.06 }));
        if clicked(b) {
            out = Some(i);
        }
    }
    let glow = 0.85 + 0.15 * (time * 1.5).sin() as f32;
    draw_circle(r.x + r.w - 40.0, r.y + BEZEL_TOP / 2.0 + 4.0, 6.0, Color::new(0.9 * glow, 1.0 * glow, 0.5 * glow, 1.0));
    let d = display_rect(r);
    rrect(d, 8.0, BLACK);
    out
}

fn display_rect(r: Rect) -> Rect {
    Rect::new(r.x + 14.0, r.y + BEZEL_TOP, r.w - 28.0, r.h - BEZEL_TOP - 14.0)
}

/// Rapport largeur / hauteur de l'écran du cockpit (lunette comprise).
pub const COCKPIT_ASPECT: f32 = 1.30;

pub fn layout(r: Rect) -> (Rect, Layout, [Rect; 3]) {
    let d = display_rect(r);
    let pfd_r = Rect::new(d.x + 5.0, d.y + 5.0, d.w - 10.0, (d.h * 0.66).floor());
    let y0 = pfd_r.y + pfd_r.h + 6.0;
    let h = d.y + d.h - 5.0 - y0;
    let w = (d.w - 10.0 - 12.0) / 3.0;
    let panes = [Rect::new(d.x + 5.0, y0, w, h), Rect::new(d.x + 11.0 + w, y0, w, h), Rect::new(d.x + 17.0 + 2.0 * w, y0, w, h)];
    (d, Layout { pfd: pfd_r, svs: pfd::svs_rect(pfd_r), inner: pfd::geo(pfd_r).inner }, panes)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn draw_mfd(world: &World, tiles: &mut crate::tiles::Tiles, wx: &crate::weather::Weather, rf: &crate::refuel::Refuel, ac: &Aircraft, recon: &crate::recon::Recon, r: Rect, st: &mut MfdState, info: &PfdInfo, style: SvsStyle, time: f64, lit: [bool; 6], feel: &crate::feel::Feel) -> (View, MfdActions) {
    let mut act = MfdActions { bezel: bezel(r, lit, time), ..Default::default() };
    let (_, lay, panes) = layout(r);
    let view = svs::draw_svs(world, tiles, wx, rf, ac, lay.svs, time, style, feel.cam());
    feel.draw_overlay(ac, lay.svs, time);
    crate::wx3d::draw(wx, ac, &view, time);
    st.wxr.update(wx, ac, wxr_range(st), get_frame_time());
    st.wxr.draw_horizon(ac, &view, time);
    crate::pfd_plus::set_inner(lay.inner);
    pfd::draw_pfd(ac, &view, lay.pfd, info);
    // Ravitailleur : repère tant qu'il est loin, directeur de ravitaillement en approche
    if let Some(t) = &rf.tanker {
        if !ac.on_ground && !matches!(rf.phase, crate::refuel::Phase::Precontact | crate::refuel::Phase::Contact) {
            scissor(Some(lay.svs));
            crate::pfd_plus::cue(ac, &view, &crate::pfd_plus::Cue { lat: t.lat, lon: t.lon, elev: t.alt, label: format!("{} FL{:.0}", t.callsign, t.alt / FT / 100.0), color: Color::new(0.4, 0.85, 1.0, 1.0) }, 2);
            scissor(None);
        }
    }
    crate::refuel::draw_hud(rf, ac, lay.inner, time);
    if info.mishap.is_none() {
        crate::recon::draw_cam_hud(ac, recon, lay.inner, time);
    }
    act.fuel_dump = draw_fuel(ac, panes[0]);
    pane_frame(panes[1]);
    act.cam = crate::recon::draw_camera(ac, recon, tiles, panes[1]);

    draw_hsi(ac, panes[2], st, info.dest);
    // Perte du signal : écrans sans source de données
    if info.mishap.is_some_and(|(_, since)| since >= 0.0) {
        for p in panes {
            crate::mishap::red_x(p, true);
        }
    } else if let Some((k, _)) = info.mishap {
        for p in panes {
            crate::mishap::glitch(p, k * 0.8, time + p.x as f64);
        }
    }
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

/// Volet carburant : débit (gauche), synoptique des réservoirs sur la silhouette du Darkstar (centre),
/// pressions des pompes (droite), quantités, centrage et vidange. Renvoie vrai si DUMP cliqué.
fn draw_fuel(ac: &Aircraft, r: Rect) -> bool {
    pane_frame(r);
    let e = &ac.eng;
    let faint = Color::new(0.55, 0.58, 0.62, 0.6);
    let lw = r.w * 0.30;
    let rw = r.w * 0.26;
    let g = (lw * 0.24).clamp(14.0, 22.0);

    // --- Gauche : débit et quantités ---
    let x = r.x + 10.0;
    txt("FUEL FLOW", x, r.y + 18.0, 12.0, WHITE);
    let pph = e.fuel_flow * 3600.0 * 2.2046 / 1000.0;
    arc_gauge(x + g + 2.0, r.y + 30.0 + g, g, (pph / 300.0) as f32, 0.85);
    txt("PPH", x, r.y + 48.0 + 2.0 * g, 12.0, DIM);
    value_box(x + 32.0, r.y + 48.0 + 2.0 * g, 40.0, &format!("{pph:03.0}"), WHITE);
    txt("x1000", x + 76.0, r.y + 48.0 + 2.0 * g, 9.0, DIM);
    let mut y = r.y + 70.0 + 2.0 * g;
    draw_line(x, y - 8.0, r.x + lw - 6.0, y - 8.0, 1.0, faint);
    let cg = ac.cg();
    let rows = [
        ("JP-7", format!("{:.1} t", ac.fuel / 1000.0), GREEN_S),
        ("H2", format!("{:.1} t", ac.h2 / 1000.0), CYAN_S),
        ("H2 DEB", format!("{:.1} kg/s", e.h2_flow), CYAN_S),
        ("CG", format!("{cg:.1} %"), if (21.0..=29.0).contains(&cg) { GREEN_S } else { AMBER }),
    ];
    for (l, v, c) in rows {
        if y > r.y + r.h - 34.0 {
            break;
        }
        txt(l, x, y + 4.0, 11.0, DIM);
        txt_r(&v, r.x + lw - 6.0, y + 4.0, 12.0, c);
        y += 16.0;
    }
    // Vidange (bouton à contour pointillé)
    let b = Rect::new(x, r.y + r.h - 28.0, 52.0, 20.0);
    if ac.fuel_dump {
        draw_rectangle(b.x, b.y, b.w, b.h, AMBER);
    }
    for i in 0..8 {
        let t0 = i as f32 / 8.0;
        let t1 = t0 + 0.07;
        draw_line(b.x + b.w * t0, b.y, b.x + b.w * t1, b.y, 1.5, WHITE);
        draw_line(b.x + b.w * t0, b.y + b.h, b.x + b.w * t1, b.y + b.h, 1.5, WHITE);
    }
    draw_line(b.x, b.y, b.x, b.y + b.h, 1.5, WHITE);
    draw_line(b.x + b.w, b.y, b.x + b.w, b.y + b.h, 1.5, WHITE);
    txt_c("DUMP", b.x + b.w / 2.0, b.y + 15.0, 13.0, if ac.fuel_dump { BLACK } else { WHITE });

    // --- Centre : silhouette du Darkstar et réservoirs ---
    let cr = Rect::new(r.x + lw, r.y + 8.0, r.w - lw - rw, r.h - 16.0);
    crate::syspage::fuel_tanks(ac, cr);
    let total = ((ac.fuel / FUEL_MAX + ac.h2 / H2_MAX) / 2.0 * 100.0) as f32;
    txt_c(&format!("{total:.0}%"), cr.x + cr.w / 2.0, r.y + r.h - 6.0, 11.0, DIM);

    // --- Droite : pression et pompes ---
    let rx = r.x + r.w - rw;
    let fp = ac.fuel_psi();
    txt_r("FUEL PRES.", r.x + r.w - 10.0, r.y + 18.0, 12.0, WHITE);
    let gx = r.x + r.w - 12.0 - g;
    arc_gauge(gx, r.y + 30.0 + g, g, (fp / 120.0) as f32, 0.88);
    txt("PSI", rx + 2.0, r.y + 48.0 + 2.0 * g, 12.0, DIM);
    value_box(r.x + r.w - 56.0, r.y + 48.0 + 2.0 * g, 46.0, &format!("{fp:.0}"), if fp < 40.0 { AMBER } else { WHITE });
    let tanks = tank_fracs(ac);
    let pumps_on = fp > 20.0;
    let sg = ((rw - 24.0) / 4.4).clamp(8.0, 14.0);
    let py0 = r.y + 76.0 + 2.0 * g;
    txt("POMPES", rx + 2.0, py0 - 4.0, 10.0, DIM);
    for (i, t) in tanks.iter().enumerate() {
        let (c, row) = (i % 2, i / 2);
        let cx = rx + 8.0 + sg + c as f32 * (2.0 * sg + 14.0);
        let cy = py0 + 10.0 + sg + row as f32 * (2.0 * sg + 12.0);
        if cy + sg > r.y + r.h - 4.0 {
            break;
        }
        let p = if pumps_on && *t > 0.0 { fp / 120.0 } else { 0.0 };
        arc_gauge(cx, cy, sg, p as f32, 0.88);
        txt_c(&format!("{}", i + 1), cx, cy + sg * 0.8, 10.0, WHITE);
    }
    clicked(b)
}

/// Volet HSI : rose des caps tournante, route magenta, cercles de distance, aérodromes.
fn draw_hsi(ac: &Aircraft, r: Rect, st: &mut MfdState, dest: usize) {
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
    let rad = (r.w * 0.40).min(r.h * 0.40);
    let cy = r.y + 40.0 + rad + 14.0;
    scissor(Some(Rect::new(r.x + 2.0, r.y + 26.0, r.w - 4.0, r.h - 28.0)));
    // Image du radar météo, sous les symboles
    st.wxr.draw_nd(vec2(cx, cy), rad * (wxr_range(st) / range) as f32, get_time());
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
    // Avion
    let y = Color::new(1.0, 0.85, 0.1, 1.0);
    draw_triangle(vec2(cx, cy - 12.0), vec2(cx - 9.0, cy + 7.0), vec2(cx + 9.0, cy + 7.0), y);
    draw_rectangle(cx - 2.0, cy + 6.0, 4.0, 5.0, y);
    scissor(None);
    st.wxr.annunciate(r.x + 8.0, r.y + r.h - 8.0);
    let gs = (ac.tas * ac.gamma.cos()).max(1.0);
    txt_r(&format!("{} {:.0}NM  ETE {}", apt.icao, dist / NM, crate::mission::fmt_time(dist / gs)), r.x + r.w - 8.0, r.y + r.h - 8.0, 12.0, MAGENTA_S);
}
