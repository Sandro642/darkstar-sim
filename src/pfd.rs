//! PFD au format Darkstar : fenêtre de vision synthétique centrale, échelles sur fond noir.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::svs::{dir, View};
use crate::ui::*;
use macroquad::prelude::*;

/// Horizon : cadres à angles nets (présentation d'afficheur de bord, pas d'arrondis).
fn rrect(r: Rect, _rad: f32, c: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, c);
}

fn rrect_lines(r: Rect, _rad: f32, th: f32, c: Color) {
    draw_rectangle_lines(r.x, r.y, r.w, r.h, th, c);
}

pub struct PfdInfo<'a> {
    pub alerts: &'a [(Level, String)],
    pub link_name: &'a str,
    pub dest: usize,
    pub warp: f64,
    pub time: f64,
    /// Objectif de reconnaissance sélectionné (repère 3D).
    pub cue: Option<crate::pfd_plus::Cue>,
    pub wx: crate::weather::WxInfo,
    pub fuel: crate::fuel::Plan,
    /// Secondes écoulées depuis le dernier changement de niveau carburant.
    pub fuel_since: f64,
    pub threats: Vec<crate::pfd_plus::ThreatCue>,
    /// Perte du véhicule : (dégradation 0..1, secondes depuis la perte du signal, < 0 avant).
    pub mishap: Option<(f32, f64)>,
    /// Hauteur de décision au-dessus du seuil (ft).
    /// Hauteur de décision, seulement pendant une approche (autoland / ILS)
    pub da_ft: Option<f64>,
}




thread_local! {
    static STYLE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Style de l'horizon : 0 Darkstar (échelles, blanc), 1 F-35 (casque : valeurs encadrées, vert),
/// 2 hybride (échelles Darkstar, langage visuel F-35 : vert, valeurs encadrées, fonds allégés).
pub fn set_style(s: u8) {
    STYLE.with(|c| c.set(s.min(2)));
}

pub fn f35() -> bool {
    STYLE.with(|c| c.get()) == 1
}

/// Symbologie verte (F-35 et hybride).
fn green() -> bool {
    STYLE.with(|c| c.get()) != 0
}

/// Vert de symbologie F-35 (casque) ou blanc Darkstar.
fn line() -> Color {
    if green() { Color::new(0.38, 1.0, 0.52, 1.0) } else { Color::new(0.92, 0.94, 0.95, 1.0) }
}

fn label_c() -> Color {
    if green() { Color::new(0.30, 0.85, 0.45, 1.0) } else { Color::new(0.85, 0.88, 0.9, 1.0) }
}

/// Couleur des valeurs principales (chiffres des échelles).
fn val_c() -> Color {
    if green() { Color::new(0.45, 1.0, 0.58, 1.0) } else { WHITE }
}

/// Texte de symbologie avec liseré sombre (lisible sur ciel clair comme sur relief).
fn ht(s: &str, x: f32, y: f32, size: f32, c: Color) {
    txt(s, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.75));
    txt(s, x, y, size, c);
}

fn ht_r(s: &str, x: f32, y: f32, size: f32, c: Color) {
    let w = txt_w(s, size);
    ht(s, x - w, y, size, c);
}

/// Cadre de valeur F-35 : filet vert fin, fond sombre léger.
fn f35_box(r: Rect, c: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, c);
}

/// Vitesse (gauche) : KEAS encadrée, consigne au-dessus, Mach / vitesses / g / incidence dessous.
fn f35_speed(ac: &Aircraft, g: &Geo, time: f64) {
    let d = &ac.d;
    let t = g.spd;
    let c = line();
    let cy = t.y + t.h / 2.0;
    let x = t.x + 4.0;
    let w = t.w + 8.0;
    let spd = (d.keas + adc_noise(ac, time)).max(0.0);
    match ac.ap.spd_keas.filter(|_| ac.ap.at_on) {
        Some(k) => ht(&format!("{k:.0}"), x, cy - 30.0, 14.0, CYAN_S),
        None if ac.ap.at_on => ht(&format!("M{:.2}", ac.ap.mach_tgt), x, cy - 30.0, 14.0, CYAN_S),
        None => {}
    }
    ht(if ac.air_data_ovrd { "INS" } else { "KEAS" }, x, cy - 46.0, 11.0, label_c());
    let b = Rect::new(x, cy - 16.0, w, 32.0);
    f35_box(b, if ac.adc_degraded() { AMBER } else { c });
    ht_r(&format!("{spd:.0}"), b.x + b.w - 8.0, cy + 9.0, 24.0, c);
    let mut y = cy + 42.0;
    let mut row = |s: String, col: Color| {
        ht(&s, x, y, 14.0, col);
        y += 19.0;
    };
    let m = if d.mach < 1.0 { format!("M .{:02.0}", d.mach * 100.0) } else { format!("M {:.2}", d.mach) };
    row(m, c);
    let nz = d.nz;
    row(format!("G {nz:.1}"), if !(-0.5..=2.5).contains(&nz) { AMBER } else { c });
    let a = d.alpha.to_degrees();
    row(format!("AOA {a:.1}"), if a > 7.0 && ac.eng.scram == ScramState::Lit { AMBER } else { c });
    row(format!("GS {:.0}", ac.ground_speed() / KT), label_c());
    row(format!("TAS {:.0}", ac.tas / KT), label_c());
    row(format!("MAX {:.1}", ac.g_max), if ac.g_max > 3.0 { AMBER } else { label_c() });
    if ac.adc_degraded() {
        row("ADC".into(), AMBER);
    }
}

/// Altitude (droite) : altitude encadrée, consigne au-dessus ; vario, niveau, q, peau, sonde
/// radio (R) et DA dessous.
fn f35_altitude(ac: &Aircraft, g: &Geo, da_ft: Option<f64>) {
    let d = &ac.d;
    let t = g.alt;
    let c = line();
    let cy = t.y + t.h / 2.0;
    let xr = g.vsi.x + g.vsi.w - 4.0;
    let w = t.w + 26.0;
    let alt = ac.alt / FT;
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        ht_r(&format!("{:.0}", ac.ap.alt_tgt_ft), xr, cy - 30.0, 14.0, CYAN_S);
    }
    ht_r("ALT", xr, cy - 46.0, 11.0, label_c());
    let b = Rect::new(xr - w, cy - 16.0, w, 32.0);
    f35_box(b, c);
    let shown = if alt.abs() < 1_000.0 { (alt / 10.0).round() * 10.0 } else { (alt / 20.0).round() * 20.0 };
    ht_r(&format!("{shown:.0}"), b.x + b.w - 8.0, cy + 9.0, 22.0, c);
    let mut y = cy + 42.0;
    let mut row = |s: String, col: Color| {
        ht_r(&s, xr, y, 14.0, col);
        y += 19.0;
    };
    let vv = d.vs / FT * 60.0;
    row(format!("VV {:+.0}", (vv / 10.0).round() * 10.0), c);
    row(format!("FL{:03.0}", alt / 100.0), label_c());
    row(format!("Q {:.0}", d.q / 1000.0), if d.q > Q_LIMIT { RED } else if d.q > 90_000.0 { AMBER } else { label_c() });
    let skin = ac.skin_temp - 273.15;
    row(format!("PEAU {skin:.0}"), if skin > SKIN_LIMIT_C - 100.0 { AMBER } else { label_c() });
    if d.agl < 5_000.0 * FT && !ac.on_ground {
        row(format!("R {:.0}", d.agl / FT), if d.agl < 200.0 * FT { AMBER } else { c });
    }
    if let (Some((_, _, _, _, el)), Some(da)) = (ac.approach, da_ft) {
        let dalt = el / FT + da;
        row(format!("DA {dalt:.0}"), if alt < dalt { AMBER } else { CYAN_S });
    }
}

const PANEL: Color = Color::new(0.02, 0.03, 0.05, 0.55);
/// Fond des blocs de données (lisibles sur ciel clair).
const BLOCK: Color = Color::new(0.01, 0.015, 0.025, 0.66);
const EDGE: Color = Color::new(1.0, 1.0, 1.0, 0.16);

/// Fenêtre de la vision synthétique : PFD v2, l'image couvre tout l'écran (« large area display »).
pub fn svs_rect(r: Rect) -> Rect {
    r
}

/// Zones du PFD v2 : bandeau de modes, échelles translucides posées sur l'image, ruban de cap,
/// et zone centrale libre (`inner`) pour la symbologie et les panneaux éphémères.
pub struct Geo {
    pub fma: Rect,
    pub spd: Rect,
    pub alt: Rect,
    pub vsi: Rect,
    pub hdg: Rect,
    pub inner: Rect,
}

pub fn geo(r: Rect) -> Geo {
    let fma = Rect::new(r.x, r.y, r.w, 30.0);
    let top = r.y + 30.0 + 36.0;
    let bot = r.y + r.h - 94.0;
    let tw = (r.w * 0.075).clamp(72.0, 92.0);
    // Style Darkstar : échelle G à gauche du ruban de vitesse
    let gx = if green() { 10.0 } else { 74.0 };
    let spd = Rect::new(r.x + gx, top, tw, bot - top);
    let vsi = Rect::new(r.x + r.w - 40.0, top, 30.0, bot - top);
    let alt = Rect::new(vsi.x - 4.0 - (tw + 16.0), top, tw + 16.0, bot - top);
    let hw = (r.w * 0.36).min(420.0);
    let hdg = Rect::new((r.x + (r.w - hw) / 2.0).round(), r.y + r.h - 38.0, hw.round(), 32.0);
    let ix = spd.x + spd.w + 16.0;
    let iy = fma.y + fma.h + 4.0;
    // Zone centrale symétrique autour de l'axe de l'horizon (maquette, cap) : les bannières et
    // blocs centrés tombent ainsi sur l'axe de l'avion
    let cx = r.x + r.w / 2.0;
    let half = (cx - ix).min(alt.x - 16.0 - cx);
    let inner = Rect::new((cx - half).round(), iy, (2.0 * half).round(), hdg.y - 6.0 - iy);
    Geo { fma, spd, alt, vsi, hdg, inner }
}

/// Bouton MENU (dans le bandeau de modes).
pub fn menu_rect(r: Rect) -> Rect {
    Rect::new(r.x + 6.0, r.y + 6.0, 58.0, 18.0)
}

/// Tambour de chiffres roulants : `step` = pas d'un cran, `modulo` = valeur bouclée affichée.
fn drum(x_right: f32, cy: f32, w: f32, h: f32, value: f64, step: f64, modulo: f64, digits: usize, size: f32) {
    let row = size * 0.95;
    scissor_push(Rect::new(x_right - w, cy - h / 2.0, w, h));
    let base = (value / step).floor() * step;
    let frac = ((value - base) / step) as f32;
    for k in -2i32..=2 {
        let v = (base + k as f64 * step).rem_euclid(modulo);
        let y = cy + (frac - k as f32) * row + size * 0.36;
        txt_r(&format!("{:0width$.0}", v, width = digits), x_right - 3.0, y, size, val_c());
    }
    scissor_pop();
}

thread_local! {
    static CLIP: std::cell::RefCell<Vec<Option<Rect>>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn scissor_push(r: Rect) {
    CLIP.with(|c| c.borrow_mut().push(Some(r)));
    scissor(Some(r));
}

fn scissor_pop() {
    let prev = CLIP.with(|c| {
        let mut v = c.borrow_mut();
        v.pop();
        v.last().copied().flatten()
    });
    scissor(prev);
}

pub fn draw_pfd(ac: &Aircraft, v: &View, r: Rect, info: &PfdInfo) {
    let g = geo(r);
    crate::pfd_plus::set_inner(g.inner);
    draw_svs_overlay(ac, v, g.inner, info);
    draw_fma_bar(ac, r, &g, info);
    if f35() {
        f35_speed(ac, &g, info.time);
        f35_altitude(ac, &g, info.da_ft);
    } else {
        if !green() {
            draw_bezels(r, &g);
            draw_g_bracket(ac, &g);
        }
        draw_speed(ac, &g, info.time);
        draw_altitude(ac, &g, info.da_ft);
        draw_vsi(ac.d.vs, g.vsi);
    }
    draw_heading(ac, &g, info);
    rrect_lines(r, 6.0, 1.5, Color::new(0.78, 0.80, 0.82, 0.85));

    if let Some((k, since)) = info.mishap {
        if since < 0.0 {
            // Télémétrie qui se dégrade : déchirures, drapeaux de panne, commandes sans réponse
            crate::mishap::glitch(v.rect, k, info.time);
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            for (th, lbl, x, y) in [
                (0.25, "ATT", cx, cy - 70.0),
                (0.35, "FPV", cx, cy + 40.0),
                (0.45, "SPD", g.spd.x + g.spd.w / 2.0, g.spd.y + g.spd.h * 0.3),
                (0.55, "ALT", g.alt.x + g.alt.w / 2.0, g.alt.y + g.alt.h * 0.3),
                (0.62, "V/S", g.vsi.x + g.vsi.w / 2.0, g.vsi.y + g.vsi.h * 0.7),
                (0.7, "HDG", g.hdg.x + g.hdg.w / 2.0, g.hdg.y + g.hdg.h / 2.0),
            ] {
                if k > th {
                    crate::mishap::flag(lbl, x, y);
                }
            }
            if k > 0.22 && (info.time * 3.0).fract() < 0.65 {
                let b = g.fma;
                draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.45, 0.02, 0.02, 0.92));
                txt_c("FLT CTL — COMMANDES DE VOL SANS REPONSE", b.x + b.w / 2.0, b.y + 20.0, 15.0, WHITE);
            }
        } else {
            // Perte du signal : écran noir (la reconnexion s'affiche par-dessus)
            draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
            crate::mishap::static_noise(r, (0.4 * (1.0 - since / 0.6)).max(0.0) as f32, info.time);
        }
    } else if ac.crashed.is_some() {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.75));
        txt_c("LIAISON PERDUE - NO SIGNAL", cx, cy - 20.0, 30.0, RED);
        txt_c(ac.crashed.as_deref().unwrap_or(""), cx, cy + 14.0, 18.0, AMBER);
        txt_c("ENTREE : nouvel essai", cx, cy + 46.0, 18.0, WHITE);
    }
}

fn draw_svs_overlay(ac: &Aircraft, v: &View, s: Rect, info: &PfdInfo) {
    let full = v.rect;
    // Visibilité des informations éphémères (façon HUD)
    let vis = crate::pfd_plus::hud_vis(ac, &info.wx, &info.threats, info.dest, s);
    let d = &ac.d;
    scissor(Some(full));
    let hr = Vec3::new(ac.psi.cos() as f32, 0.0, ac.psi.sin() as f32);
    let big = 100_000.0f32;
    let (cx, cy) = (full.x + full.w / 2.0, full.y + full.h / 2.0);

    // Ligne d'horizon (assiette nulle)
    let h0 = dir(ac.psi, 0.0) * big;
    if let (Some(a), Some(b)) = (v.project(h0 - hr * big * 3.0), v.project(h0 + hr * big * 3.0)) {
        draw_line(a.x, a.y, b.x, b.y, 2.0, line());
    }
    draw_horizon_compass(ac, v);
    let clr = crate::pfd_plus::predicted_path(ac, v, info.time);
    if !ac.on_ground {
        let apt = &AIRPORTS[info.dest];
        crate::pfd_plus::cue(ac, v, &crate::pfd_plus::Cue { lat: apt.lat, lon: apt.lon, elev: apt.elev, label: apt.icao.to_string(), color: MAGENTA_S }, 0);
        if let Some(c) = &info.cue {
            crate::pfd_plus::cue(ac, v, c, 1);
        }
        crate::pfd_plus::threats(ac, v, &info.threats, info.time);
    }

    // Échelle de tangage « 10 •  • 10 »
    for p in (-90i32..=90).step_by(5) {
        if p == 0 {
            continue;
        }
        let pr = (p as f64).to_radians();
        if (pr - ac.theta).abs() > 0.4 {
            continue;
        }
        let c = dir(ac.psi, pr) * big;
        let at = |k: f32| v.project(c + hr * big * k);
        if p % 10 == 0 {
            for side in [-1.0f32, 1.0] {
                if let Some(dot) = at(side * 0.022) {
                    draw_circle(dot.x, dot.y, 2.5, line());
                }
                if let Some(l) = at(side * 0.05) {
                    txt_c(&format!("{}", p.abs()), l.x, l.y + 6.0, 20.0, line());
                }
                if p < 0 {
                    // Tirets inclinés vers l'extérieur pour les assiettes négatives
                    for k in 0..3 {
                        let k0 = 0.075 + k as f32 * 0.018;
                        if let (Some(a), Some(b)) = (at(side * k0), at(side * (k0 + 0.010))) {
                            let drop = 3.0 + k as f32 * 3.0;
                            draw_line(a.x, a.y + drop, b.x, b.y + drop + 3.0, 2.0, line());
                        }
                    }
                }
            }
        } else {
            for side in [-1.0f32, 1.0] {
                if let Some(dot) = at(side * 0.022) {
                    draw_circle(dot.x, dot.y, 1.6, Color::new(1.0, 1.0, 1.0, 0.7));
                }
            }
        }
    }

    // Consigne d'assiette (mode PITCH) : repères magenta
    if ac.ap.on && ac.ap.vert == VertMode::Pitch {
        let c = dir(ac.psi, ac.ap.pitch_tgt.to_radians()) * big;
        for side in [-1.0f32, 1.0] {
            if let (Some(a), Some(b)) = (v.project(c + hr * big * side * 0.032), v.project(c + hr * big * side * 0.07)) {
                draw_line(a.x, a.y, b.x, b.y, 3.0, MAGENTA_S);
            }
        }
    }

    // Marqueur de trajectoire : cercle, ailes et dérive
    // Vecteur vitesse sur la route sol : la dérive due au vent le décale latéralement
    let fpv = if !ac.on_ground && ac.tas > 20.0 { v.project_dir(dir(ac.track_deg().to_radians(), ac.gamma)) } else { v.project_dir(dir(ac.psi, ac.theta)) };
    if let Some(f) = fpv {
        let (sn, co) = (ac.phi.sin() as f32, ac.phi.cos() as f32);
        let rot = |x: f32, y: f32| vec2(f.x + x * co - y * sn, f.y + x * sn + y * co);
        for (w, col) in [(4.0, Color::new(0.0, 0.0, 0.0, 0.6)), (2.0, line())] {
            draw_circle_lines(f.x, f.y, 10.0, w, col);
            for (x0, x1) in [(-46.0, -10.0), (10.0, 46.0)] {
                let (a, b) = (rot(x0, 0.0), rot(x1, 0.0));
                draw_line(a.x, a.y, b.x, b.y, w, col);
            }
            let (a, b) = (rot(0.0, -10.0), rot(0.0, -22.0));
            draw_line(a.x, a.y, b.x, b.y, w, col);
        }
        if !ac.on_ground {
            crate::pfd_plus::aoa_index(ac, f);
        }
        // Chevron d'énergie
        let acc = (d.accel / 9.81 * 60.0).clamp(-40.0, 40.0) as f32;
        let p = rot(-56.0, -acc);
        draw_line(p.x - 6.0, p.y - 4.0, p.x, p.y, 2.0, GREEN_S);
        draw_line(p.x - 6.0, p.y + 4.0, p.x, p.y, 2.0, GREEN_S);
    }
    // Symbole avion fixe (barres de référence avec crochets), comme sur le PFD Darkstar
    for (w, col) in [(4.0, Color::new(0.0, 0.0, 0.0, 0.55)), (2.0, line())] {
        for sd in [-1.0f32, 1.0] {
            let (x0, x1) = (cx + sd * 34.0, cx + sd * 112.0);
            draw_line(x0, cy, x1, cy, w, col);
            draw_line(x1, cy, x1, cy + 9.0, w, col);
        }
        draw_line(cx, cy - 4.0, cx, cy - 18.0, w, col);
    }

    // Inclinaison : arc en haut (± 60°), index fixe et pointeur de ciel
    let bank_rad = (cy - (s.y + 22.0)).max(60.0);
    {
        let rad = bank_rad;
        let pt = |a: f32, r: f32| vec2(cx + r * a.to_radians().sin(), cy - r * a.to_radians().cos());
        let arc: Vec<Vec2> = (-15..=15).map(|k| pt(k as f32 * 4.0, rad)).collect();
        stroke(&arc, false, 1.5, Color::new(1.0, 1.0, 1.0, 0.55));
        for a in [-60.0f32, -45.0, -30.0, -20.0, -10.0, 10.0, 20.0, 30.0, 45.0, 60.0] {
            let l = if a.abs() == 30.0 || a.abs() == 60.0 { 12.0 } else { 7.0 };
            let (p0, p1) = (pt(a, rad), pt(a, rad + l));
            draw_line(p0.x, p0.y, p1.x, p1.y, 1.5, Color::new(1.0, 1.0, 1.0, 0.75));
        }
        let z = pt(0.0, rad + 1.0);
        draw_triangle(z, z + vec2(-7.0, -11.0), z + vec2(7.0, -11.0), line());
        let phi = ac.phi.to_degrees() as f32;
        let a = (-phi).clamp(-62.0, 62.0);
        let n = vec2(a.to_radians().sin(), -a.to_radians().cos());
        let tg = vec2(-n.y, n.x);
        let tip = pt(a, rad - 2.0);
        let bc = if phi.abs() > 60.0 { RED } else if phi.abs() > 35.0 { AMBER } else { line() };
        draw_triangle(tip, tip - n * 13.0 + tg * 7.0, tip - n * 13.0 - tg * 7.0, bc);
        // Barre de dérapage (vol coordonné : centrée)
        let b0 = tip - n * 17.0;
        draw_line(b0.x - tg.x * 8.0, b0.y - tg.y * 8.0, b0.x + tg.x * 8.0, b0.y + tg.y * 8.0, 3.0, bc);
    }

    if !ac.on_ground {
        let reveal = s.contains(vec2(mouse_position().0, mouse_position().1));
        crate::pfd_plus::fuel_alert(ac, &info.fuel, s, info.time, info.fuel_since, reveal && info.fuel.level != crate::fuel::Level::Ok);
    }
    // Panneaux couloir hypersonique et énergie (masqués en finale ILS)
    let on_ils = ac.approach.is_some_and(|(_, tla, tlo, thdg, _)| along_before(tla, tlo, thdg, ac.lat, ac.lon) < 60_000.0);
    if !ac.on_ground && !on_ils {
        let pw = (s.w * 0.27).clamp(170.0, 230.0);
        // Au-dessus de la ligne de saisie des instructions
        if vis.corridor > 0.01 {
            crate::pfd_plus::corridor(ac, Rect::new(s.x + 8.0, s.y + s.h - 122.0, pw, 92.0), vis.corridor);
        }
        if vis.energy > 0.01 {
            crate::pfd_plus::energy(ac, Rect::new(s.x + s.w - pw - 8.0, s.y + s.h - 88.0, pw, 62.0), info.dest, vis.energy);
        }
    }
    // Inclinaison chiffrée au-delà de 30°
    if ac.phi.abs() > 30f64.to_radians() {
        let bc = if ac.phi.abs() > 60f64.to_radians() { RED } else { AMBER };
        txt_c(&format!("BANK {:.0}", ac.phi.to_degrees().abs()), cx, cy - bank_rad + 46.0, 13.0, bc);
    }
    // ILS
    if let Some((ai, tla, tlo, thdg, tel)) = ac.approach {
        let a = &AIRPORTS[ai];
        let along = along_before(tla, tlo, thdg, ac.lat, ac.lon);
        let cross = cross_track(tla, tlo, thdg, ac.lat, ac.lon);
        if along > -a.len && along < 60_000.0 && !ac.on_ground {
            let loc_dev = (cross / (along + a.len)).atan().to_degrees();
            let gs_dev = ((ac.alt - tel) / (along + 300.0).max(50.0)).atan().to_degrees() - 3.0;
            // Au-dessus de la bannière d'approche quand elle est affichée
            let banner = crate::appr::visible(ac);
            let ly = if banner { s.y + s.h - crate::appr::BANNER_H - 38.0 } else { s.y + s.h - 50.0 };
            for k in [-2.0f32, -1.0, 1.0, 2.0] {
                draw_circle_lines(cx + k * 36.0, ly, 4.0, 1.5, line());
            }
            draw_poly(cx + (-loc_dev as f32 / 1.25 * 36.0).clamp(-80.0, 80.0), ly, 4, 8.0, 0.0, MAGENTA_S);
            let gx = s.x + s.w - 24.0;
            for k in [-2.0f32, -1.0, 1.0, 2.0] {
                draw_circle_lines(gx, cy + k * 36.0, 4.0, 1.5, line());
            }
            draw_poly(gx, cy + (gs_dev as f32 / 0.35 * 36.0).clamp(-80.0, 80.0), 4, 8.0, 0.0, MAGENTA_S);
            let rid = a.end_ident(thdg, Some((tla, tlo)));
            if !banner {
                let lbl = match crate::pfd_plus::ils_label().filter(|l| l.contains(a.icao)) {
                    Some(l) => format!("ILS {l}  {:.1} NM", along / NM),
                    None => format!("ILS {rid} {}  {:.1} NM", a.icao, along / NM),
                };
                let w = txt_w(&lbl, 14.0) + 14.0;
                rrect(Rect::new(cx - w / 2.0, ly + 10.0, w, 20.0), 5.0, BLOCK);
                txt_c(&lbl, cx, ly + 25.0, 14.0, MAGENTA_S);
            }
        }
    }

    // Radioaltimètre
    if d.agl < 2500.0 * FT && !ac.on_ground {
        let ra = d.agl / FT;
        let c = if ra < 200.0 { AMBER } else { GREEN_S };
        let s_ra = if ra < 50.0 { format!("{:.0}", ra) } else { format!("{:.0}", (ra / 10.0).round() * 10.0) };
        // Sous l'aile droite de la maquette (la fenêtre d'alarmes occupe l'axe)
        txt_c(&s_ra, cx + 120.0, cy + 46.0, 26.0, c);
    }

    // Enregistrement
    if info.time.fract() < 0.5 {
        draw_circle(s.x + 12.0, s.y + 13.0, 4.0, RED);
    }
    txt(&format!("REC  {}", crate::link::tail()), s.x + 20.0, s.y + 18.0, 13.0, Color::new(0.9, 0.9, 0.9, 0.8));
    // Position de l'avion (auparavant sous l'image caméra)
    let pos = |v: f64, p: char, n: char| {
        let a = v.abs();
        format!("{}{:.0} {:05.2}'", if v >= 0.0 { p } else { n }, a.floor(), (a - a.floor()) * 60.0)
    };
    txt(&format!("{}  {}", pos(ac.lat, 'N', 'S'), pos(ac.lon, 'E', 'W')), s.x + 112.0, s.y + 18.0, 11.0, Color::new(0.85, 0.88, 0.9, 0.75));
    // Informations éphémères façon HUD : RWR (haut gauche), vent / météo (haut droite)
    if !ac.on_ground {
        crate::pfd_plus::rwr(ac, &info.threats, Rect::new(s.x + 8.0, s.y + 26.0, 92.0, 92.0), info.time, vis.rwr);
        crate::pfd_plus::wind_block(ac, &info.wx, Rect::new(s.x + s.w - 160.0, s.y + 44.0, 152.0, 86.0), info.time, vis.wind);
    }
    // Annonces d'override et de limitation FADEC
    let mut oy = if ac.on_ground || vis.rwr < 0.01 { s.y + 36.0 } else { s.y + 152.0 };
    let blink = (info.time * 2.0).fract() < 0.65;
    // Aérofreins et freins (mémos permanents tant qu'ils sont actifs)
    let gs = ac.ground_speed() / KT;
    let sb_label = if ac.ap.at_speedbrake { "AEROFREINS A" } else { "AEROFREINS" };
    // Aérofreins sortis avec poussée : ambre (configuration incohérente)
    let sb_col = if ac.throttle > 0.3 { AMBER } else { CYAN_S };
    let park = ac.on_ground && ac.brakes && gs < 3.0;
    let park_col = if ac.throttle > 0.5 { AMBER } else { GREEN_S };
    let braking = ac.on_ground && ac.brakes && gs >= 3.0;
    let win_lbl = match ac.win_state {
        2 => "VITRE ROMPUE",
        1 => "VITRES FISSUREES",
        _ => "VITRES CHAUDES",
    };
    let win_on = ac.win_state > 0 || ac.win_temp > 1_120.0;
    let memos: [(bool, &str, Color, bool); 11] = [
        (win_on, win_lbl, if ac.win_state > 0 { RED } else { AMBER }, true),
        (!ac.gcas_on && !ac.on_ground, "GCAS OFF", AMBER, false),
        (ac.pitot_iced(), "PITOT ICE", AMBER, true),
        (ac.g_lim_high, "G LIM HIGH", AMBER, false),
        (ac.fuel_cool == 2, "FUEL COOL OFF", AMBER, false),
        (ac.ovr_engine, "ENG OVRD", AMBER, true),
        (ac.ovr_fbw, "FBW OVRD", AMBER, true),
        (!ac.ovr_engine && ac.fadec_limit < 0.995, "FADEC LIMIT", AMBER, false),
        (ac.speedbrake, sb_label, sb_col, sb_col == AMBER),
        (park, "FREIN PARC", park_col, park_col == AMBER),
        (braking, "FREINAGE", WHITE, false),
    ];
    for (on, label, col, flash) in memos {
        if on {
            tag(s.x + 8.0, oy, "", label, col, 12.0, true, blink || !flash);
            oy += 24.0;
        }
    }
    if braking {
        // Décélération au freinage
        let dec = (-ac.d.accel / 9.81).max(0.0);
        txt(&format!("DECEL {dec:.2} G"), s.x + 10.0, oy - 2.0, 11.0, WHITE);
    }

    // Coin gauche : récupération automatique (prioritaire), sinon panneau IFR départ / arrivée / approche
    let left_y = s.y + s.h * 0.36;
    let pw = (s.w * 0.30).clamp(250.0, 300.0);
    if recovery_panel(ac, s.x + 8.0, left_y, pw, info.time) <= 0.0 {
        let h = crate::pfd_plus::ifr_height();
        // Au-dessus des panneaux du bas (couloir, ligne de saisie)
        let low = if on_ils || ac.on_ground { 64.0 } else { 130.0 };
        let y = (s.y + s.h - low - h).max(oy + 4.0);
        crate::pfd_plus::ifr_draw(s.x + 8.0, y, pw, info.time);
    }
    // Graphe de survitesse (prédiction de q, droite de rupture, braquage des gouvernes)
    overspeed_graph(ac, Rect::new(s.x + s.w - 250.0, s.y + s.h * 0.40, 240.0, 132.0), info.time);
    // Demandes de configuration de l'autoland : c'est l'opérateur qui exécute
    config_cues(ac, cx, s.y + s.h * 0.30, info.time);
    // Alarmes façon F/A-18 / Rafale
    hud_cues(ac, cx, cy, bank_rad, info.time, fpv);
    takeoff_pitch(ac, v, cx, cy, info.time);
    runway_hud(ac, cx, cy);
    // Au-dessus de la bannière d'approche / des barres ILS quand elles sont affichées
    let floor = if crate::appr::visible(ac) { s.y + s.h - crate::appr::BANNER_H - 64.0 } else { s.y + s.h - 60.0 };
    alarm_window(ac, info, cx, (cy + 118.0).min(floor - 10.0));
    // Alarme relief au premier plan (au-dessus des blocs carburant) ; pas en finale train sorti
    if !ac.landing_config() && !ac.gcas_active && !(ac.gcas_on && ac.gcas_t < 8.0) {
        crate::pfd_plus::terrain_warning(clr, s, info.time);
    }
    scissor(None);
}

/// Prédiction de la pression dynamique sur 10 s (courbe dynamique) face aux droites LIMITE et
/// RUPTURE (q et KEAS équivalents), et braquage des gouvernes face à leur limite de rupture pendant
/// l'auto recovery. Visible dès que la courbe s'approche de la limite.
fn overspeed_graph(ac: &Aircraft, r: Rect, time: f64) {
    if ac.on_ground || ac.crashed.is_some() {
        return;
    }
    let peak = ac.osr_curve.iter().copied().fold(0.0f32, f32::max);
    let lim = (Q_LIMIT / 1000.0) as f32;
    let rupt = (Q_RUPTURE / 1000.0) as f32;
    if !ac.osr_active && peak < 0.85 * lim {
        return;
    }
    let blink = (time * 2.5).fract() < 0.6;
    rrect(r, 6.0, Color::new(0.0, 0.0, 0.0, 0.62));
    rrect_lines(r, 6.0, 1.5, if ac.osr_active { RED } else { AMBER });
    let title = if ac.osr_active { "AUTO RECOVERY SURVITESSE" } else { "RISQUE SURVITESSE" };
    if !ac.osr_active || blink {
        txt(title, r.x + 8.0, r.y + 14.0, 11.0, if ac.osr_active { RED } else { AMBER });
    }
    txt_r("q 10 s", r.x + r.w - 8.0, r.y + 14.0, 10.0, DIM);
    // Zone de tracé : x = temps (0..10 s), y = q (kPa)
    let g = Rect::new(r.x + 34.0, r.y + 22.0, r.w - 80.0, r.h - 52.0);
    let (q0, q1) = (40.0f32, 150.0f32);
    let py = |q: f32| g.y + g.h - ((q - q0) / (q1 - q0)).clamp(0.0, 1.0) * g.h;
    let keas = |q: f32| ((2.0 * q as f64 * 1000.0 / crate::atmosphere::RHO0).sqrt() / KT) as f32;
    for (q, c, l) in [(lim, AMBER, "LIM"), (rupt, RED, "RUPT")] {
        let y = py(q);
        draw_line(g.x, y, g.x + g.w, y, 1.5, c);
        txt_r(&format!("{q:.0}"), g.x - 4.0, y + 4.0, 10.0, c);
        txt(&format!("{l} {:.0}KT", keas(q)), g.x + g.w + 4.0, y + 4.0, 9.0, c);
    }
    draw_line(g.x, g.y + g.h, g.x + g.w, g.y + g.h, 1.0, Color::new(0.35, 0.38, 0.4, 1.0));
    let n = ac.osr_curve.len();
    let mut prev: Option<Vec2> = None;
    for (i, q) in ac.osr_curve.iter().enumerate() {
        let p = vec2(g.x + i as f32 / (n - 1) as f32 * g.w, py(*q));
        let c = if *q > rupt { RED } else if *q > lim { AMBER } else { GREEN_S };
        if let Some(a) = prev {
            draw_line(a.x, a.y, p.x, p.y, 2.5, c);
        }
        prev = Some(p);
    }
    let q_now = (ac.d.q / 1000.0) as f32;
    draw_circle(g.x, py(q_now), 4.0, WHITE);
    txt(&format!("{q_now:.0} kPa  {:.0} KEAS", ac.d.keas), g.x + 8.0, g.y + 10.0, 10.0, WHITE);
    // Gouvernes : braquage commandé face à la droite de rupture (plafond 95 %)
    let (d, dr) = ac.osr_defl;
    let by = r.y + r.h - 18.0;
    let bw = r.w - 16.0;
    let f = if dr > 0.0 { (d / dr).clamp(0.0, 1.2) as f32 / 1.2 } else { 0.0 };
    txt(&format!("GOUVERNES {d:.0}/{dr:.0} DEG"), r.x + 8.0, by - 3.0, 10.0, if ac.osr_active { CYAN_S } else { DIM });
    draw_rectangle_lines(r.x + 8.0, by + 2.0, bw, 8.0, 1.0, DIM);
    draw_rectangle(r.x + 9.0, by + 3.0, (bw - 2.0) * f, 6.0, if f > 0.95 / 1.2 { AMBER } else { CYAN_S });
    let xr = r.x + 8.0 + bw / 1.2;
    draw_line(xr, by, xr, by + 12.0, 2.0, RED);
}

/// Panneau de données de la récupération automatique (coin gauche) : avant déclenchement
/// (Auto-GCAS armé : délai, marge prévue), pendant (étapes AILES A PLAT / RESSOURCE / MONTEE,
/// inclinaison, g commandé / réel / max, pente, vario, hauteur sol, temps avant impact, marge,
/// perte d'altitude, q et gouvernes en survitesse, libération du manche), puis bilan 8 s.
/// Renvoie la hauteur occupée (0 si rien).
fn recovery_panel(ac: &Aircraft, x: f32, y: f32, w: f32, time: f64) -> f32 {
    if ac.on_ground || ac.crashed.is_some() {
        return 0.0;
    }
    let r = &ac.rec;
    let armed = !r.active && ac.gcas_on && ac.gcas_t < 8.0;
    let done = !r.active && r.done_age < 8.0;
    if !(r.active || armed || done) {
        return 0.0;
    }
    let d = &ac.d;
    let fast = (time * 3.0).fract() < 0.6;
    let mut rows: Vec<(&str, String, Color)> = Vec::new();
    let ft = |m: f64| m / FT;
    let vs = d.vs / FT * 60.0;
    let sink = -d.vs;
    let tti = if sink > 0.5 { format!("{:.1} S", d.agl / sink) } else { "--".into() };
    let marge = |m: f64| if m.is_finite() { format!("{:+.0} FT", ft(m)) } else { "--".into() };
    let (title, col) = if r.active {
        (if r.kind == 1 { "AUTO-GCAS  RECUPERATION" } else { "AUTO RECOVERY  SURVITESSE" }, RED)
    } else if armed {
        ("AUTO-GCAS  ARME", if ac.gcas_t < 3.0 { RED } else { AMBER })
    } else {
        ("BILAN RECUPERATION", GREEN_S)
    };
    // Étape courante (1 ailes à plat, 2 ressource, 3 montée / stabilisation)
    let phi = ac.phi.to_degrees();
    let step = if !r.active {
        0
    } else if phi.abs() > 15.0 {
        1
    } else if (r.kind == 1 && ac.gamma < 3f64.to_radians()) || (r.kind == 2 && ac.osr_clear_t <= 0.0) {
        2
    } else {
        3
    };
    if r.active || armed {
        let nz_cmd = if ac.gcas_ovr { 5.0 } else { ac.nz_law_max() * 0.95 };
        rows.push(("INCLINAISON", format!("{phi:+.0} DEG  -> 0"), if phi.abs() > 15.0 { AMBER } else { GREEN_S }));
        rows.push(("CHARGE", format!("{:.1} G  CMD {:.1}  MAX {:.1}", d.nz, nz_cmd, if r.active { r.g_max } else { d.nz }), if ac.gcas_ovr { RED } else { WHITE }));
        rows.push(("PENTE / VARIO", format!("{:+.1} DEG  {vs:+.0} FPM", ac.gamma.to_degrees()), if ac.gamma < 0.0 { AMBER } else { GREEN_S }));
        rows.push(("HAUTEUR SOL", format!("{:.0} FT{}", ft(d.agl), if r.active { format!("  MINI {:.0}", ft(r.min_agl)) } else { String::new() }), if d.agl < 500.0 * FT { RED } else { WHITE }));
        rows.push(("AVANT IMPACT", tti, if sink > 0.5 && d.agl / sink < 10.0 { RED } else { WHITE }));
        rows.push(("MARGE PREVUE", marge(ac.gcas_m), if ac.gcas_m < 0.0 { RED } else { GREEN_S }));
        if armed {
            rows.push(("DECLENCHEMENT", format!("DANS {:.1} S", ac.gcas_t.max(0.0)), if ac.gcas_t < 3.0 { RED } else { AMBER }));
            rows.push(("ACTION", "AILES A PLAT PUIS TIRER".into(), CYAN_S));
        }
        if r.active {
            rows.push(("PERTE ALT", format!("{:.0} FT  DUREE {:.1} S", ft(r.alt0 - r.alt_min), r.t), WHITE));
            if ac.gcas_ovr {
                rows.push(("FCS", "PROTECTIONS DEPASSEES 5 G".into(), RED));
            }
        }
        if r.kind == 2 && r.active || ac.osr_active {
            let rupt = Q_RUPTURE / 1000.0;
            let peak = ac.osr_curve.iter().copied().fold(0.0f32, f32::max) as f64;
            rows.push(("PRESSION q", format!("{:.0}  PIC {:.0}  RUPT {:.0} kPa", d.q / 1000.0, peak, rupt), if peak > rupt * 0.97 { RED } else { AMBER }));
            let (dc, dr) = ac.osr_defl;
            rows.push(("GOUVERNES", format!("{dc:.0} / {dr:.0} DEG ({:.0} %)", if dr > 0.0 { dc / dr * 100.0 } else { 0.0 }), CYAN_S));
            rows.push(("CONFIG", format!("MANETTE IDLE  AEROFREINS {}", if ac.speedbrake { "EXT" } else { "RET" }), CYAN_S));
        }
    } else {
        rows.push(("TYPE", if r.kind == 1 { "AUTO-GCAS".into() } else { "SURVITESSE".into() }, WHITE));
        rows.push(("DUREE", format!("{:.1} S", r.t), WHITE));
        rows.push(("PERTE ALT", format!("{:.0} FT", ft(r.alt0 - r.alt_min)), WHITE));
        rows.push(("SOL MINI", format!("{:.0} FT", ft(r.min_agl)), if r.min_agl < 300.0 * FT { AMBER } else { GREEN_S }));
        rows.push(("CHARGE MAX", format!("{:.1} G", r.g_max), if r.g_max > ac.nz_law_max() + 0.3 { AMBER } else { GREEN_S }));
        rows.push(("q MAX", format!("{:.0} kPa", r.q_max / 1000.0), if r.q_max > Q_LIMIT { AMBER } else { GREEN_S }));
        rows.push(("ETAT", "PILOTE A VOUS".into(), GREEN_S));
    }
    let steps_h = if r.active { 22.0 } else { 0.0 };
    let rel_h = if r.active { 22.0 } else { 0.0 };
    let h = 24.0 + steps_h + rows.len() as f32 * 15.0 + rel_h + 6.0;
    let b = Rect::new(x, y, w, h);
    rrect(b, 0.0, Color::new(0.0, 0.0, 0.0, 0.66));
    rrect_lines(b, 0.0, 1.5, col);
    draw_rectangle(b.x, b.y, b.w, 19.0, Color::new(col.r, col.g, col.b, 0.25));
    if !r.active || fast || armed {
        txt(title, b.x + 6.0, b.y + 14.0, 11.0, col);
    }
    let mut yy = b.y + 24.0;
    if r.active {
        // Étapes de la manœuvre
        let names = ["AILES A PLAT", "RESSOURCE", if r.kind == 1 { "MONTEE" } else { "STABILISE" }];
        let sw = (b.w - 12.0) / 3.0;
        for (k, n) in names.iter().enumerate() {
            let bx = b.x + 6.0 + k as f32 * sw;
            let st = k + 1;
            let (bg, fg) = if st == step { (Color::new(0.0, 0.85, 1.0, 0.35), WHITE) } else if st < step { (Color::new(0.2, 0.8, 0.3, 0.25), GREEN_S) } else { (Color::new(0.3, 0.32, 0.35, 0.25), Color::new(0.6, 0.62, 0.65, 0.8)) };
            draw_rectangle(bx + 1.0, yy, sw - 2.0, 16.0, bg);
            txt_c(&format!("{st} {n}"), bx + sw / 2.0, yy + 12.0, 9.0, fg);
        }
        yy += steps_h;
    }
    for (l, v, c) in &rows {
        txt(l, b.x + 6.0, yy + 10.0, 9.0, Color::new(0.6, 0.65, 0.7, 0.9));
        txt(v, b.x + 98.0, yy + 10.0, 10.0, *c);
        yy += 15.0;
    }
    if r.active {
        // Libération du manche : critère tenu (montée et marge) pendant 1,5 s (GCAS) / 2 s (survitesse)
        let (t, need) = if r.kind == 1 { (ac.gcas_clear_t, 1.5) } else { (ac.osr_clear_t, 2.0) };
        let f = (t / need).clamp(0.0, 1.0) as f32;
        let bw = b.w - 12.0;
        txt(&if f > 0.0 { format!("PILOTE A VOUS DANS {:.1} S", (need - t).max(0.0)) } else { "MANCHE INHIBE — CRITERE DE SORTIE NON ATTEINT".into() }, b.x + 6.0, yy + 9.0, 9.0, if f > 0.0 { GREEN_S } else { CYAN_S });
        draw_rectangle_lines(b.x + 6.0, yy + 12.0, bw, 6.0, 1.0, Color::new(0.5, 0.55, 0.6, 0.8));
        draw_rectangle(b.x + 7.0, yy + 13.0, (bw - 2.0) * f, 4.0, GREEN_S);
    }
    h
}

thread_local! {
    static GCAS_END: std::cell::Cell<(bool, f64)> = const { std::cell::Cell::new((false, -100.0)) };
}

/// Vrai pendant 4 s après la fin d'une récupération automatique.
fn gcas_done_recently(ac: &Aircraft, time: f64) -> bool {
    let rec = ac.gcas_active || ac.osr_active;
    GCAS_END.with(|c| {
        let (was, t_end) = c.get();
        let t_end = if was && !rec { time } else { t_end };
        c.set((rec, t_end));
        !rec && time - t_end < 4.0
    })
}

/// Piste sous l'avion (au sol), façon HUD : désignation, terrain, longueur totale et restante.
fn runway_hud(ac: &Aircraft, cx: f32, cy: f32) {
    if !ac.on_ground || ac.crashed.is_some() {
        return;
    }
    let Some((a, id, len, ahead)) = crate::airports::runway_under(ac.lat, ac.lon, ac.hdg_deg()) else { return };
    let y = cy + 64.0;
    let c = line();
    let main = format!("RWY {id}  {}", a.icao);
    let sub = format!("{len:.0} M  ({:.0} FT)   RESTANT {ahead:.0} M", len / FT);
    let w = txt_w(&main, 18.0).max(txt_w(&sub, 12.0)) + 24.0;
    // Crochets d'encadrement façon HUD
    for (x0, s) in [(cx - w / 2.0, 1.0f32), (cx + w / 2.0, -1.0)] {
        draw_line(x0, y - 18.0, x0, y + 22.0, 1.5, c);
        draw_line(x0, y - 18.0, x0 + s * 8.0, y - 18.0, 1.5, c);
        draw_line(x0, y + 22.0, x0 + s * 8.0, y + 22.0, 1.5, c);
    }
    let txt_cs = |s: &str, y: f32, size: f32, col: Color| {
        txt_c(s, cx + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.75));
        txt_c(s, cx, y, size, col);
    };
    txt_cs(&main, y, 18.0, c);
    let rc = if ahead < 1_000.0 { AMBER } else { label_c() };
    txt_cs(&sub, y + 17.0, 12.0, rc);
}

/// Guide d'assiette au décollage (anti tail strike) : cible 10° (crochets verts), limite de
/// contact arrière 12° (chevron « V » ambre puis rouge), assiette courante, consigne ROTATE à VR.
fn takeoff_pitch(ac: &Aircraft, v: &View, cx: f32, cy: f32, time: f64) {
    let d = &ac.d;
    let vr = ac.vr_keas();
    let rolling = ac.on_ground && d.keas > 60.0 && ac.throttle > 0.4;
    let just_off = !ac.on_ground && ac.gear_down && d.agl < 150.0 * FT && d.vs > 0.0;
    if !(rolling || just_off) || ac.crashed.is_some() {
        return;
    }
    let th = ac.theta.to_degrees();
    let blink = (time * 3.0).fract() < 0.6;
    const TARGET: f64 = 10.0;
    const LIMIT: f64 = 12.0;
    let near = th > LIMIT - 1.5;
    let pc = |deg: f64| v.project_dir(dir(ac.psi, deg.to_radians()));
    // Cible 10° : crochets verts de part et d'autre
    if let Some(p) = pc(TARGET) {
        let c = GREEN_S;
        for sgn in [-1.0f32, 1.0] {
            let x = p.x + sgn * 70.0;
            stroke(&[vec2(x + sgn * 22.0, p.y - 8.0), vec2(x, p.y - 8.0), vec2(x, p.y + 8.0), vec2(x + sgn * 22.0, p.y + 8.0)], false, 3.0, c);
        }
        ht("10", p.x + 100.0, p.y + 5.0, 13.0, c);
    }
    // Limite de contact arrière 12° : chevron « V » pointé vers le bas
    if let Some(p) = pc(LIMIT) {
        let c = if near { RED } else { AMBER };
        if !near || blink {
            for (w, col) in [(6.0, Color::new(0.0, 0.0, 0.0, 0.6)), (3.5, c)] {
                stroke(&[vec2(p.x - 30.0, p.y - 18.0), vec2(p.x, p.y), vec2(p.x + 30.0, p.y - 18.0)], false, w, col);
            }
        }
        ht("TAIL 12", p.x + 36.0, p.y - 4.0, 12.0, c);
    }
    // Assiette courante (barre sur l'axe)
    if let Some(p) = pc(th) {
        let c = if near { RED } else { CYAN_S };
        draw_line(p.x - 46.0, p.y, p.x - 14.0, p.y, 4.0, c);
        draw_line(p.x + 14.0, p.y, p.x + 46.0, p.y, 4.0, c);
    }
    // Annonce
    let y = cy + 150.0;
    if ac.on_ground && d.keas < vr - 10.0 {
        tag(cx, y, "DECOLLAGE", &format!("ROTATION A {vr:.0} KT  ({:.0})", d.keas), WHITE, 15.0, false, true);
    } else if near {
        tag(cx, y, "TAIL STRIKE", &format!("ASSIETTE {th:.1}°  MAX {LIMIT:.0}° — RELACHER"), RED, 17.0, false, blink);
    } else if ac.on_ground && th < 1.0 {
        tag(cx, y, "DECOLLAGE", &format!("ROTATE  ^  CIBLE {TARGET:.0}°  ({:.0} KT)", d.keas), CYAN_S, 17.0, false, blink);
    } else {
        tag(cx, y, "ASSIETTE", &format!("{th:.1}°  CIBLE {TARGET:.0}°  MAX {LIMIT:.0}°"), if th > TARGET + 0.5 { AMBER } else { GREEN_S }, 16.0, false, true);
    }
}

/// Consignes au pilote pour éviter la casse des gouvernes / la survitesse / l'impact :
/// RELEASE STICK, ROLL LEFT / ROLL RIGHT, PULL (flèches) ; cyan pendant une récupération automatique.
fn directives(ac: &Aircraft, cx: f32, y: f32, fast: bool) {
    if ac.on_ground || ac.crashed.is_some() {
        return;
    }
    let q_kpa = ac.d.q / 1000.0;
    let hinge = q_kpa * ac.fcs.defl_cmd / crate::systems::HINGE_LIMIT;
    let stick = ac.fcs.pitch_in.abs() + ac.fcs.roll_in.abs() > 0.15;
    let release = hinge > 0.85 && stick && !ac.ap.on && !ac.osr_active && !ac.gcas_active;
    let rupt = (Q_RUPTURE / 1000.0) as f32;
    let peak = ac.osr_curve.iter().copied().fold(0.0f32, f32::max);
    let over_risk = peak > 0.9 * rupt && ac.gamma < 0.0;
    let gcas_risk = ac.gcas_on && ac.gcas_t < 6.0;
    let auto = ac.osr_active || ac.gcas_active;
    if !(release || over_risk || gcas_risk || auto) {
        return;
    }
    let col = if auto { CYAN_S } else { RED };
    let label = if auto { "AUTO" } else { "MANCHE" };
    if release {
        tag(cx, y, "MANCHE", "RELEASE STICK", RED, 18.0, false, fast);
        return;
    }
    let phi = ac.phi.to_degrees();
    let roll = if phi.abs() > 30.0 { if phi > 0.0 { -1 } else { 1 } } else { 0 };
    let pull = phi.abs() < 70.0;
    let mut parts = Vec::new();
    if roll < 0 {
        parts.push("ROLL LEFT");
    } else if roll > 0 {
        parts.push("ROLL RIGHT");
    }
    if pull {
        parts.push("PULL");
    }
    let msg = parts.join("  ");
    if msg.is_empty() {
        return;
    }
    let w = tag(cx, y, label, &msg, col, 18.0, false, auto || fast);
    // Flèches : roulis (latérale) et ressource (vers le haut)
    if roll != 0 {
        let sx = roll as f32;
        let ax = cx + sx * (w / 2.0 + 22.0);
        draw_triangle(vec2(ax + sx * 14.0, y - 5.0), vec2(ax, y - 15.0), vec2(ax, y + 5.0), col);
        draw_line(ax - sx * 16.0, y - 5.0, ax, y - 5.0, 3.0, col);
    }
    if pull {
        let ay = y - 30.0;
        draw_triangle(vec2(cx, ay - 14.0), vec2(cx - 9.0, ay), vec2(cx + 9.0, ay), col);
        draw_line(cx, ay, cx, ay + 6.0, 3.0, col);
    }
}

/// GEAR DOWN / SPEED BRAKE EXT|RET demandés par l'autoland (touches G et S).
fn config_cues(ac: &Aircraft, cx: f32, y: f32, time: f64) {
    let mut items: Vec<(&str, Color)> = Vec::new();
    if ac.cue_gear && !ac.gear_down {
        items.push(("GEAR DOWN  (G)", GREEN_S));
    }
    match ac.cue_sb {
        Some(true) => items.push(("SPEED BRAKE EXT  (S)", CYAN_S)),
        Some(false) => items.push(("SPEED BRAKE RET  (S)", CYAN_S)),
        None => {}
    }
    let blink = (time * 2.0).fract() < 0.7;
    for (k, (m, c)) in items.iter().enumerate() {
        tag(cx, y + k as f32 * 30.0, "CONFIG", m, *c, 17.0, false, blink);
    }
}

/// Repères critiques au-dessus de la maquette : FEU, GCAS (chevrons, PULL UP, FLY UP), limites
/// (OVER G, AOA, OVERSPEED, SPEED). Rouge clignotant, ambre fixe.
fn hud_cues(ac: &Aircraft, cx: f32, cy: f32, bank_rad: f32, time: f64, fpv: Option<Vec2>) {
    if ac.on_ground || ac.crashed.is_some() {
        return;
    }
    let d = &ac.d;
    let fast = (time * 3.0).fract() < 0.6;
    let y = cy - bank_rad * 0.42;
    directives(ac, cx, y + 44.0, fast);
    if ac.fire.any() {
        let names: Vec<&str> = ac.fire.zones.iter().enumerate().filter(|(_, z)| z.fire).map(|(i, _)| crate::fire::ZONES[i].0).collect();
        tag(cx, y - 24.0, "FEU", &names.join("  "), RED, 22.0, false, fast);
        return;
    }
    // Auto-GCAS / récupération survitesse, façon F-16 : chevrons qui se referment sur le vecteur
    // vitesse, « Break-X » à l'instant du déclenchement et pendant la ressource, flèche FLY UP
    let f = fpv.unwrap_or(vec2(cx, cy));
    let recovering = ac.gcas_active || ac.osr_active;
    let break_x = |c: Color| {
        for (w, col) in [(7.0, Color::new(0.0, 0.0, 0.0, 0.6)), (4.0, c)] {
            draw_line(f.x - 70.0, f.y - 70.0, f.x + 70.0, f.y + 70.0, w, col);
            draw_line(f.x - 70.0, f.y + 70.0, f.x + 70.0, f.y - 70.0, w, col);
        }
    };
    if recovering {
        if fast {
            break_x(RED);
        }
        // Grande flèche de ressource au-dessus du vecteur vitesse
        let ay = f.y - 95.0;
        draw_triangle(vec2(f.x, ay - 34.0), vec2(f.x - 26.0, ay), vec2(f.x + 26.0, ay), RED);
        draw_rectangle(f.x - 8.0, ay, 16.0, 28.0, RED);
        let what = if ac.osr_active { "SURVITESSE" } else { "GCAS" };
        let body = if ac.gcas_ovr { format!("FLY UP  5G  {:.1} G", ac.d.nz) } else { format!("FLY UP  {:.1} G", ac.d.nz) };
        tag(cx, y - 24.0, what, &body, RED, 22.0, false, true);
        tag(cx, y + 6.0, "AUTO", "RECUPERATION EN COURS — MANCHE INHIBE", CYAN_S, 13.0, false, true);
        return;
    }
    if ac.gcas_on && ac.gcas_t < 8.0 {
        let k = ((ac.gcas_t - 1.0) / 7.0).clamp(0.0, 1.0) as f32;
        let gap = 18.0 + 130.0 * k;
        let c = if ac.gcas_t < 3.0 { RED } else { AMBER };
        // Chevrons autour du vecteur vitesse (pointes vers lui)
        for sgn in [-1.0f32, 1.0] {
            let x = f.x + sgn * gap;
            for (w, col) in [(6.0, Color::new(0.0, 0.0, 0.0, 0.6)), (3.5, c)] {
                stroke(&[vec2(x + sgn * 20.0, f.y - 18.0), vec2(x, f.y), vec2(x + sgn * 20.0, f.y + 18.0)], false, w, col);
            }
        }
        if ac.gcas_t < 1.2 && fast {
            break_x(RED);
        }
        let label = if ac.gcas_t < 3.0 { "PULL UP" } else { "TERRAIN" };
        tag(cx, y - 26.0, "GCAS", &format!("{label}  {:.0} S", ac.gcas_t.max(0.0)), c, 20.0, false, ac.gcas_t >= 3.0 || fast);
        return;
    }
    // Récupération terminée : confirmation verte quelques secondes
    if gcas_done_recently(ac, time) {
        tag(cx, y - 24.0, "AGCAS", "RECUPERATION TERMINEE — PILOTE A VOUS", GREEN_S, 16.0, false, true);
    }
    let nmax = ac.fcs.nz_max.max(1.5);
    let amax = ac.fcs.alpha_max.max(1.0);
    let cue: Option<(&str, bool)> = if d.nz > nmax * 0.95 || d.nz < -1.5 {
        Some(("OVER G", true))
    } else if d.alpha.to_degrees() > amax * 0.92 && d.keas < 300.0 {
        Some(("AOA", true))
    } else if d.q > 0.92 * Q_LIMIT {
        Some(("OVERSPEED", true))
    } else if !ac.gear_down && d.keas < 150.0 && d.agl > 100.0 {
        Some(("SPEED", false))
    } else {
        None
    };
    if let Some((m, red)) = cue {
        tag(cx, y, "LIMITE", m, if red { RED } else { AMBER }, 20.0, false, !red || fast);
    }
}

/// Fenêtre d'alarmes : « ALARME » (voyants rouges, clignotants) et « PANNE » (voyants ambre),
/// sans répéter les mémos déjà affichés à gauche.
fn alarm_window(ac: &Aircraft, info: &PfdInfo, cx: f32, y: f32) {
    if ac.crashed.is_some() {
        return;
    }
    const MEMO: [&str; 5] = ["ENG OVRD", "FADEC LIMIT", "FUEL DUMP", "H2 DUMP", "FCS DIRECT"];
    let lit = crate::alarms::lit(ac, info.alerts);
    let skip = |n: &str, l: u8| MEMO.contains(&n) && !(n == "FCS DIRECT" && l == 2);
    let list = |lvl: u8| -> String {
        let v: Vec<&str> = lit.iter().filter(|(n, l)| *l == lvl && !skip(n, *l)).map(|(n, _)| *n).collect();
        let more = if v.len() > 3 { format!("  +{}", v.len() - 3) } else { String::new() };
        if v.is_empty() { String::new() } else { format!("{}{more}", v.iter().take(3).copied().collect::<Vec<_>>().join("  ")) }
    };
    let (reds, ambers) = (list(2), list(1));
    let blink = (info.time * 2.0).fract() < 0.65;
    let size = 15.0;
    let wr = if reds.is_empty() { 0.0 } else { txt_w("ALARME", size * 0.72) + 12.0 + txt_w(&reds, size) + 16.0 };
    let wa = if ambers.is_empty() { 0.0 } else { txt_w("PANNE", size * 0.72) + 12.0 + txt_w(&ambers, size) + 16.0 };
    let gap = if wr > 0.0 && wa > 0.0 { 8.0 } else { 0.0 };
    let mut x = cx - (wr + gap + wa) / 2.0;
    if wr > 0.0 {
        tag(x, y, "ALARME", &reds, RED, size, true, blink);
        x += wr + gap;
    }
    if wa > 0.0 {
        tag(x, y, "PANNE", &ambers, AMBER, size, true, true);
    }
}

/// Erreur des données air quand les sondes sont dégradées (Mach > 5.5 sans AIR DATA OVRD).
fn adc_noise(ac: &Aircraft, time: f64) -> f64 {
    if !ac.adc_degraded() {
        return 0.0;
    }
    let k = (ac.d.mach - 5.5).min(4.0);
    k * (8.0 * (time * 7.3).sin() + 5.0 * (time * 17.1).sin() + 3.0 * (time * 31.7).cos())
}

fn tbcc_color(m: &str) -> Color {
    match m {
        "SCRAMJET" => MAGENTA_S,
        "RAMJET" | "DUAL TURB+RAM" => CYAN_S,
        "TURBINE" => WHITE,
        _ => AMBER,
    }
}

/// Bandeau de modes : MENU, annonciateur (FMA), mode du moteur combiné et liaison.
fn draw_fma_bar(ac: &Aircraft, r: Rect, g: &Geo, info: &PfdInfo) {
    let b = g.fma;
    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.0, 0.0, 0.0, 0.62));
    draw_line(b.x, b.y + b.h, b.x + b.w, b.y + b.h, 1.0, EDGE);
    let m = menu_rect(r);
    rrect(m, 4.0, Color::new(0.22, 0.24, 0.27, 0.95));
    txt_c("MENU", m.x + m.w / 2.0, m.y + 13.0, 12.0, label_c());
    let x0 = m.x + m.w + 10.0;
    let x1 = b.x + b.w - 180.0;
    crate::pfd_plus::fma(ac, x0, b.y + 8.0, x1 - x0, info.dest);
    draw_line(x1, b.y + 4.0, x1, b.y + b.h - 4.0, 1.0, Color::new(0.5, 0.52, 0.55, 0.7));
    let mode = ac.mode_name();
    txt_c(&format!("TBCC {mode}"), x1 + 90.0, b.y + 14.0, 13.0, tbcc_color(mode));
    let lc = if info.link_name == "PERDUE" || info.link_name == "IRIDIUM" { AMBER } else { GREEN_S };
    txt_c(&format!("{}  x{:.0}", info.link_name, info.warp), x1 + 90.0, b.y + 26.0, 10.0, lc);
}

/// Fond d'échelle translucide.
fn tape_bg(t: Rect) {
    if green() {
        // Hybride : échelle allégée, filet vert discret
        draw_rectangle(t.x, t.y, t.w, t.h, Color::new(0.0, 0.02, 0.01, 0.30));
        let c = line();
        draw_rectangle_lines(t.x, t.y, t.w, t.h, 1.0, Color::new(c.r, c.g, c.b, 0.45));
    } else {
        rrect(t, 6.0, PANEL);
        rrect_lines(t, 6.0, 1.0, EDGE);
    }
}

/// Bords noirs de l'afficheur Darkstar : colonne gauche (échelle G + vitesse) et colonne droite
/// (altitude + vario), l'image de l'horizon au centre.
fn draw_bezels(r: Rect, g: &Geo) {
    let top = g.fma.y + g.fma.h;
    let h = r.y + r.h - top;
    let bg = Color::new(0.015, 0.017, 0.02, 1.0);
    let edge = Color::new(0.30, 0.32, 0.34, 1.0);
    let lx1 = g.spd.x + g.spd.w + 4.0;
    draw_rectangle(r.x, top, lx1 - r.x, h, bg);
    draw_line(lx1, top, lx1, r.y + r.h, 1.0, edge);
    let rx0 = g.alt.x - 4.0;
    draw_rectangle(rx0, top, r.x + r.w - rx0, h, bg);
    draw_line(rx0, top, rx0, r.y + r.h, 1.0, edge);
}

/// Échelle G Darkstar (colonne gauche) : crochet +3.5 / +1.0 / -2.5 G, facteur de charge courant
/// (triangle), mémoires G max / min (repères ambre), valeur numérique au-dessus.
fn draw_g_bracket(ac: &Aircraft, g: &Geo) {
    let nz = ac.d.nz;
    let x = g.spd.x - 52.0;
    let top = g.spd.y + g.spd.h * 0.16;
    let bot = g.spd.y + g.spd.h * 0.66;
    let gy = |v: f32| top + (3.5 - v) / 6.0 * (bot - top);
    let ln = Color::new(0.85, 0.87, 0.9, 1.0);
    draw_line(x, top, x, bot, 1.5, ln);
    for v in [3.5f32, 1.0, -2.5] {
        let y = gy(v);
        draw_line(x, y, x + 7.0, y, 1.5, ln);
        txt(&format!("{v:+.1}G"), x + 10.0, y + 4.0, 11.0, label_c());
    }
    // Graduations intermédiaires
    for v in [3.0f32, 2.0, 0.0, -1.0, -2.0] {
        let y = gy(v);
        draw_line(x, y, x + 4.0, y, 1.0, Color::new(0.6, 0.62, 0.65, 1.0));
    }
    for m in [ac.g_max, ac.g_min] {
        if (m - 1.0).abs() > 0.05 {
            let y = gy(m.clamp(-2.6, 3.6) as f32);
            draw_line(x - 8.0, y, x, y, 2.5, AMBER);
        }
    }
    let gc = if !(-0.5..=2.5).contains(&nz) { AMBER } else { GREEN_S };
    let y = gy(nz.clamp(-2.6, 3.6) as f32);
    draw_triangle(vec2(x - 1.0, y), vec2(x - 10.0, y - 6.0), vec2(x - 10.0, y + 6.0), gc);
    txt_c("G", x + 10.0, top - 26.0, 11.0, label_c());
    txt_c(&format!("{nz:+.1}"), x + 10.0, top - 11.0, 14.0, gc);
}

fn draw_speed(ac: &Aircraft, g: &Geo, time: f64) {
    let d = &ac.d;
    let t = g.spd;
    tape_bg(t);
    let lx = t.x + t.w;
    let cy = t.y + t.h / 2.0;
    let ppk = t.h / 150.0;
    let spd = (d.keas + adc_noise(ac, time)).max(0.0) as f32;
    scissor_push(t);
    let base = (spd / 10.0).floor() as i32 * 10;
    for k in -9..=9 {
        let v = base + k * 10;
        if v < 0 {
            continue;
        }
        let y = cy - (v as f32 - spd) * ppk;
        let l = if v % 20 == 0 { 9.0 } else { 5.0 };
        draw_line(lx - l, y, lx, y, 1.5, line());
        if v % 20 == 0 {
            txt_r(&format!("{:03}", v), lx - 13.0, y + 5.0, 15.0, label_c());
        }
    }
    // Bandes : limite q (rouge), VLE train (ambre), basse vitesse
    let q_lim = ((2.0 * Q_LIMIT / 1.225).sqrt() / KT) as f32;
    let yq = cy - (q_lim - spd) * ppk;
    draw_rectangle(lx - 4.0, yq - 600.0, 4.0, 600.0, RED);
    if ac.gear_pos > 0.01 {
        let yg = cy - (GEAR_VLE as f32 - spd) * ppk;
        draw_rectangle(lx - 7.0, yg - 600.0, 3.0, 600.0, AMBER);
    }
    if !ac.on_ground {
        let vs1g = (crate::autoflight::vapp(ac) / 1.23) as f32;
        let yl = cy - (vs1g - spd) * ppk;
        if yl < t.y + t.h {
            scissor_push(Rect::new(lx - 6.0, yl.max(t.y), 6.0, t.y + t.h - yl.max(t.y)));
            let mut yy = yl;
            while yy < t.y + t.h {
                draw_rectangle(lx - 6.0, yy, 6.0, 6.0, RED);
                yy += 12.0;
            }
            scissor_pop();
        }
        if ac.approach.is_some() && ac.gear_pos > 0.5 {
            let va = crate::autoflight::vapp(ac) as f32;
            let y = cy - (va - spd) * ppk;
            draw_line(lx - 12.0, y, lx, y, 2.0, CYAN_S);
            txt_r("VAPP", lx - 14.0, y - 3.0, 11.0, CYAN_S);
        }
    }
    if ac.on_ground {
        let y = cy - (170.0 - spd) * ppk;
        draw_line(lx - 10.0, y, lx, y, 2.0, CYAN_S);
        txt_r("R", lx - 12.0, y + 5.0, 14.0, CYAN_S);
    }
    if let Some(k) = ac.ap.spd_keas.filter(|_| ac.ap.at_on) {
        let y = (cy - (k as f32 - spd) * ppk).clamp(t.y, t.y + t.h);
        draw_triangle(vec2(lx - 9.0, y), vec2(lx, y - 6.0), vec2(lx, y + 6.0), MAGENTA_S);
    }
    let trend = (d.accel * 10.0 / KT * (crate::atmosphere::isa(ac.alt).rho / 1.225).sqrt()) as f32;
    if trend.abs() > 2.0 {
        draw_line(lx - 3.0, cy, lx - 3.0, cy - trend * ppk, 2.0, GREEN_S);
    }
    scissor_pop();
    // Fenêtre de lecture en flèche vers l'échelle
    let bw = t.w - 16.0;
    let bx = t.x + 3.0;
    rrect(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, BLACK);
    rrect_lines(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, 1.5, line());
    draw_triangle(vec2(lx + 2.0, cy), vec2(bx + bw, cy - 8.0), vec2(bx + bw, cy + 8.0), line());
    draw_rectangle(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, 1.5, line());
    let s10 = (spd / 10.0).floor();
    if s10 > 0.0 {
        txt_r(&format!("{s10:.0}"), bx + bw - 21.0, cy + 9.0, 24.0, val_c());
    }
    drum(bx + bw, cy, 20.0, 46.0, spd as f64, 1.0, 10.0, 1, 22.0);
    // Au-dessus : vitesse vraie et consigne
    rrect(Rect::new(t.x, t.y - 34.0, t.w, 32.0), 5.0, BLOCK);
    txt(&format!("TAS {:.0}", ac.tas / KT), t.x + 2.0, t.y - 20.0, 13.0, WHITE);
    match ac.ap.spd_keas.filter(|_| ac.ap.at_on) {
        Some(k) => txt(&format!("SEL {k:.0}"), t.x + 2.0, t.y - 6.0, 13.0, MAGENTA_S),
        None if ac.ap.at_on => txt(&format!("SEL M{:.2}", ac.ap.mach_tgt), t.x + 2.0, t.y - 6.0, 13.0, MAGENTA_S),
        None => txt(if ac.air_data_ovrd { "INS" } else { "KEAS" }, t.x + 2.0, t.y - 6.0, 12.0, label_c()),
    }
    // En dessous : Mach, vitesse sol, incidence, facteur de charge
    // Style Darkstar : bloc logé dans la colonne noire de gauche
    let bx = if green() { t.x } else { t.x + t.w + 4.0 - (t.w + 52.0) };
    let y = t.y + t.h + 24.0;
    rrect(Rect::new(bx, t.y + t.h + 4.0, t.w + 52.0, 86.0), 5.0, BLOCK);
    let m = if d.mach < 1.0 { format!("M {:.2}", d.mach).replace("M 0.", "M .") } else { format!("M {:.2}", d.mach) };
    let mc = if d.mach >= 5.0 { MAGENTA_S } else if d.mach >= 1.0 { CYAN_S } else { WHITE };
    txt(&m, bx + 2.0, y, 22.0, mc);
    txt(&format!("GS {:.0}", ac.ground_speed() / KT), bx + 2.0, y + 17.0, 13.0, label_c());
    let a = d.alpha.to_degrees();
    let ac_col = if a > 7.0 && ac.eng.scram == ScramState::Lit { AMBER } else { label_c() };
    txt(&format!("a {a:+.1}°"), bx + 2.0, y + 33.0, 13.0, ac_col);
    if ac.adc_degraded() {
        draw_rectangle(lx - 34.0, y + 21.0, 34.0, 16.0, Color::new(0.0, 0.0, 0.0, 0.8));
        draw_rectangle_lines(lx - 34.0, y + 21.0, 34.0, 16.0, 1.0, AMBER);
        txt_c("ADC", lx - 17.0, y + 34.0, 12.0, AMBER);
    }
    let nz = d.nz;
    let gc = if !(-0.5..=2.5).contains(&nz) { AMBER } else { GREEN_S };
    txt(&format!("G {nz:+.2}"), bx + 2.0, y + 49.0, 13.0, gc);
    let mc = if ac.g_max > 3.0 || ac.g_min < -1.0 { AMBER } else { DIM };
    txt(&format!("MAX {:+.1}  MIN {:+.1}", ac.g_max, ac.g_min), bx + 2.0, y + 64.0, 11.0, mc);
    // Colonne droite façon HUD de chasse : vitesse vraie, pente, limite de g de la loi, énergie
    let xr = bx + t.w + 48.0;
    txt_r(&format!("TAS {:.0}", ac.tas / KT), xr, y - 4.0, 12.0, label_c());
    txt_r(&format!("FPA {:+.1}", ac.gamma.to_degrees()), xr, y + 17.0, 12.0, label_c());
    let lim = ac.fcs.nz_max;
    let lc = if ac.gcas_ovr || ac.ovr_fbw { AMBER } else { label_c() };
    txt_r(&format!("LIM {lim:+.1}"), xr, y + 33.0, 12.0, lc);
    // Hauteur d'énergie spécifique (h + V²/2g), en milliers de pieds
    let es = (ac.alt + ac.tas * ac.tas / (2.0 * 9.81)) / FT / 1000.0;
    txt_r(&format!("Es {es:.0}k"), xr, y + 49.0, 12.0, label_c());
}

fn draw_altitude(ac: &Aircraft, g: &Geo, da_ft: Option<f64>) {
    let d = &ac.d;
    let t = g.alt;
    tape_bg(t);
    let lx = t.x;
    let cy = t.y + t.h / 2.0;
    let alt = (ac.alt / FT) as f32;
    let ppf = t.h / 1000.0;
    scissor_push(t);
    let base = (alt / 100.0).floor() as i32 * 100;
    for k in -7..=7 {
        let a = base + k * 100;
        let y = cy - (a as f32 - alt) * ppf;
        let l = if a % 200 == 0 { 9.0 } else { 5.0 };
        draw_line(lx, y, lx + l, y, 1.5, line());
        if a % 200 == 0 {
            txt(&format!("{a}"), lx + 14.0, y + 5.0, 15.0, label_c());
        }
    }
    // Sol : hachures jaunes et noires sous l'élévation du terrain (bord droit)
    let yg = cy - ((d.elev / FT) as f32 - alt) * ppf;
    let top = yg.max(t.y);
    let bottom = t.y + t.h;
    if top < bottom {
        let hx = t.x + t.w - 12.0;
        scissor_push(Rect::new(hx, top, 12.0, bottom - top));
        draw_rectangle(hx, top, 12.0, bottom - top, BLACK);
        let mut y0 = top - 12.0;
        while y0 < bottom + 12.0 {
            draw_line(hx, y0 + 12.0, hx + 12.0, y0, 3.0, Color::new(0.95, 0.75, 0.1, 0.9));
            y0 += 8.0;
        }
        scissor_pop();
    }
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        let y = (cy - (ac.ap.alt_tgt_ft as f32 - alt) * ppf).clamp(t.y, t.y + t.h);
        draw_rectangle(lx, y - 5.0, 7.0, 10.0, MAGENTA_S);
    }
    // Tendance d'altitude à 6 s
    let trend = (d.vs * 6.0 / FT) as f32 * ppf;
    if trend.abs() > 3.0 {
        let yt = (cy - trend).clamp(t.y, t.y + t.h);
        draw_line(lx + 4.0, cy, lx + 4.0, yt, 2.5, GREEN_S);
    }
    // Minimums (200 ft au-dessus du seuil) en approche
    if let (Some((_, _, _, _, el)), Some(da_ft)) = (ac.approach, da_ft) {
        let mda = (el / FT) as f32 + da_ft as f32;
        let y = cy - (mda - alt) * ppf;
        let c = if alt < mda { AMBER } else { CYAN_S };
        draw_line(lx, y, lx + 40.0, y, 2.0, c);
        txt("MIN", lx + 42.0, y + 4.0, 11.0, c);
    }
    scissor_pop();
    // Fenêtre de lecture en flèche : milliers/centaines fixes + tambour des dizaines (pas de 20 ft)
    let bw = t.w - 14.0;
    let bx = lx + 10.0;
    rrect(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, BLACK);
    rrect_lines(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, 1.5, line());
    draw_triangle(vec2(lx - 2.0, cy), vec2(bx, cy - 8.0), vec2(bx, cy + 8.0), line());
    draw_rectangle(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, 1.5, line());
    let a100 = (alt / 100.0).floor();
    txt_r(&format!("{a100:.0}"), bx + bw - 31.0, cy + 9.0, 22.0, val_c());
    drum(bx + bw, cy, 30.0, 46.0, alt as f64, 20.0, 100.0, 2, 18.0);
    // Au-dessus : consigne d'altitude
    rrect(Rect::new(t.x, t.y - 34.0, t.w, 32.0), 5.0, BLOCK);
    txt("GPS ALT", t.x + 4.0, t.y - 20.0, 11.0, label_c());
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        txt(&format!("{:.0}", ac.ap.alt_tgt_ft), t.x + 4.0, t.y - 6.0, 14.0, MAGENTA_S);
    } else {
        txt("STD", t.x + 4.0, t.y - 6.0, 12.0, label_c());
    }
    // Altitude de décision (seuil + hauteur de décision) en approche
    if let (Some((_, _, _, _, el)), Some(da_ft)) = (ac.approach, da_ft) {
        let da = el / FT + da_ft;
        let below = ac.alt / FT < da;
        txt_r(&format!("DA {da:.0}"), t.x + t.w - 4.0, t.y - 6.0, 12.0, if below { AMBER } else { CYAN_S });
    }
    // En dessous : niveau de vol, pression dynamique, peau
    let y = t.y + t.h + 24.0;
    rrect(Rect::new(t.x, t.y + t.h + 4.0, g.vsi.x + g.vsi.w - t.x, 60.0), 5.0, BLOCK);
    txt(&format!("FL{:03.0}", alt / 100.0), t.x + 2.0, y, 20.0, WHITE);
    let qcol = if d.q > Q_LIMIT { RED } else if d.q > 90_000.0 { AMBER } else { label_c() };
    txt(&format!("q {:.0} kPa", d.q / 1000.0), t.x + 2.0, y + 17.0, 13.0, qcol);
    let skin = ac.skin_temp - 273.15;
    let scol = if skin > SKIN_LIMIT_C { RED } else if skin > SKIN_LIMIT_C - 100.0 { AMBER } else { label_c() };
    txt(&format!("PEAU {skin:.0}°C"), t.x + 2.0, y + 33.0, 13.0, scol);
}

fn draw_vsi(vs: f64, v: Rect) {
    tape_bg(v);
    let x = v.x + 4.0;
    let cy = v.y + v.h / 2.0;
    let half = v.h * 0.46;
    let marks = [(0.0f32, 0.0f32), (2.0, 0.25), (4.0, 0.45), (10.0, 0.70), (40.0, 0.95)];
    let pos = |kfpm: f32| {
        let a = kfpm.abs();
        let mut f = 1.0;
        for w in marks.windows(2) {
            if a <= w[1].0 {
                f = w[0].1 + (a - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1);
                break;
            }
        }
        cy - kfpm.signum() * f * half
    };
    for (k, _) in marks.iter().skip(1) {
        for sg in [-1.0f32, 1.0] {
            let y = pos(sg * k);
            draw_line(x, y, x + 5.0, y, 1.5, line());
            txt(&format!("{}", *k as i32), x + 8.0, y + 4.0, 11.0, label_c());
        }
    }
    draw_line(x, cy, x + 4.0, cy, 1.5, line());
    let kfpm = (vs / FT * 60.0 / 1000.0) as f32;
    let y = pos(kfpm);
    draw_line(x - 3.0, y, x + 22.0, y, 3.0, GREEN_S);
    if kfpm.abs() > 0.4 {
        txt_r(&format!("{:.0}", kfpm * 1000.0), v.x + v.w, if kfpm > 0.0 { v.y - 6.0 } else { v.y + v.h + 14.0 }, 12.0, GREEN_S);
    }
}

fn draw_heading(ac: &Aircraft, g: &Geo, info: &PfdInfo) {
    let h = g.hdg;
    tape_bg(h);
    let cx = h.x + h.w / 2.0;
    let y = h.y + 2.0;
    let hdg = ac.hdg_deg() as f32;
    let ppd = h.w / 50.0;
    scissor_push(h);
    let base = (hdg / 5.0).floor() as i32 * 5;
    for k in -6..=6 {
        let hh = base + k * 5;
        let x = cx + (hh as f32 - hdg) * ppd;
        let l = if hh % 10 == 0 { 9.0 } else { 5.0 };
        draw_line(x, y, x, y + l, 1.5, line());
        if hh % 10 == 0 {
            let lbl = hh.rem_euclid(360) / 10;
            txt_c(&format!("{}", if lbl == 0 { 36 } else { lbl }), x, y + 25.0, 14.0, label_c());
        }
    }
    if ac.ap.on {
        let bx = cx + wrap180(ac.ap.hdg_tgt - hdg as f64) as f32 * ppd;
        draw_rectangle(bx - 4.0, y, 8.0, 6.0, MAGENTA_S);
    }
    // Relèvement de la destination (chevron magenta creux)
    let apt = &AIRPORTS[info.dest];
    let (dist, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    if !ac.on_ground {
        let bx = cx + wrap180(brg - hdg as f64).clamp(-24.0, 24.0) as f32 * ppd;
        stroke(&[vec2(bx - 6.0, h.y + h.h - 2.0), vec2(bx, h.y + h.h - 10.0), vec2(bx + 6.0, h.y + h.h - 2.0)], false, 2.0, MAGENTA_S);
    }
    // Cap de retour (bingo) : chevron ambre
    if info.fuel.level >= crate::fuel::Level::Bingo && !ac.on_ground {
        let bx = cx + wrap180(info.fuel.rtb.1 - hdg as f64).clamp(-24.0, 24.0) as f32 * ppd;
        draw_triangle(vec2(bx, y + 1.0), vec2(bx - 6.0, y + 11.0), vec2(bx + 6.0, y + 11.0), AMBER);
    }
    // Route sol (losange vert)
    if !ac.on_ground {
        let tx = cx + wrap180(ac.track_deg() - hdg as f64) as f32 * ppd;
        stroke(&[vec2(tx, y + 1.0), vec2(tx + 5.0, y + 7.0), vec2(tx, y + 13.0), vec2(tx - 5.0, y + 7.0)], true, 1.5, GREEN_S);
    }
    scissor_pop();
    let b = Rect::new(cx - 24.0, h.y - 11.0, 48.0, 20.0);
    rrect(b, 4.0, BLACK);
    rrect_lines(b, 4.0, 1.5, MAGENTA_S);
    txt_c(&format!("{:03.0}", hdg.round() % 360.0), cx, h.y + 4.0, 15.0, WHITE);
    // Destination à droite du ruban
    let lbl = format!("{}  {:.1} NM", apt.icao, dist / NM);
    rrect(Rect::new(h.x + h.w + 6.0, h.y + 4.0, txt_w(&lbl, 14.0) + 12.0, 24.0), 5.0, BLOCK);
    txt(&lbl, h.x + h.w + 12.0, h.y + 21.0, 14.0, MAGENTA_S);
}

/// Indicateur de train « trois vertes » : vert = sorti verrouillé, rouge = en transit ou configuration
/// dangereuse (train rentré trop bas), éteint = rentré verrouillé.
pub fn draw_gear(ac: &Aircraft, cx: f32, cy: f32, time: f64, unsafe_gear: bool) {
    let legs = ac.gear_legs();
    let box_r = Rect::new(cx - 40.0, cy - 36.0, 80.0, 78.0);
    rrect(box_r, 8.0, Color::new(0.0, 0.0, 0.0, 0.7));
    rrect_lines(box_r, 8.0, 1.0, Color::new(0.5, 0.52, 0.55, 0.8));
    txt_c("GEAR", cx, cy - 22.0, 12.0, label_c());
    let blink = (time * 3.0).fract() < 0.55;
    let pos = [(cx, cy - 6.0), (cx - 21.0, cy + 16.0), (cx + 21.0, cy + 16.0)];
    for (i, (x, y)) in pos.iter().enumerate() {
        let l = legs[i];
        let (c, lit) = if l >= 0.999 {
            (GREEN_S, true)
        } else if l > 0.001 {
            (RED, true)
        } else if unsafe_gear {
            (RED, blink)
        } else {
            (Color::new(0.15, 0.16, 0.17, 1.0), false)
        };
        let col = if lit { c } else { Color::new(0.12, 0.13, 0.14, 1.0) };
        if lit {
            draw_circle(*x, *y, 12.0, Color::new(c.r, c.g, c.b, 0.22));
        }
        draw_circle(*x, *y, 8.0, col);
        draw_circle_lines(*x, *y, 8.0, 1.0, Color::new(0.4, 0.42, 0.45, 1.0));
    }
    let (label, c) = if legs.iter().all(|l| *l >= 0.999) {
        ("DOWN", GREEN_S)
    } else if legs.iter().all(|l| *l <= 0.001) {
        if unsafe_gear { ("UP !", RED) } else { ("UP", DIM) }
    } else {
        ("TRANSIT", RED)
    };
    txt_c(label, cx, cy + 37.0, 11.0, c);
}

/// Rose des caps posée sur la ligne d'horizon : repère direct des caps dans le paysage.
fn draw_horizon_compass(ac: &Aircraft, v: &View) {
    let hdg = ac.psi.to_degrees();
    let base = (hdg / 10.0).round() as i32 * 10;
    for k in -5..=5 {
        let h = base + k * 10;
        let hn = h.rem_euclid(360);
        let Some(p) = v.project_dir(dir((h as f64).to_radians(), 0.0)) else { continue };
        if !v.rect.contains(p) {
            continue;
        }
        let major = hn % 30 == 0;
        let len = if major { 9.0 } else { 5.0 };
        // Inclinée avec le roulis
        let (sn, co) = (ac.phi.sin() as f32, ac.phi.cos() as f32);
        let up = vec2(sn, -co);
        let a = p + up * len;
        draw_line(p.x, p.y, a.x, a.y, 1.5, Color::new(1.0, 1.0, 1.0, 0.85));
        if major {
            let lbl = match hn {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "W".to_string(),
                _ => format!("{}", hn / 10),
            };
            let t = p + up * (len + 9.0);
            txt_c(&lbl, t.x, t.y + 5.0, 13.0, Color::new(1.0, 1.0, 1.0, 0.85));
        }
    }
}

