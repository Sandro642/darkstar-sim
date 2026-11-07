//! PFD au format Darkstar : fenêtre de vision synthétique centrale, échelles sur fond noir.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::svs::{dir, View};
use crate::ui::*;
use macroquad::prelude::*;

pub struct PfdInfo<'a> {
    pub alerts: &'a [(Level, String)],
    pub link_name: &'a str,
    pub latency_ms: f64,
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
}

const LINE: Color = Color::new(0.92, 0.94, 0.95, 1.0);
const LABEL: Color = Color::new(0.85, 0.88, 0.9, 1.0);

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
    let spd = Rect::new(r.x + 10.0, top, tw, bot - top);
    let vsi = Rect::new(r.x + r.w - 40.0, top, 30.0, bot - top);
    let alt = Rect::new(vsi.x - 4.0 - (tw + 16.0), top, tw + 16.0, bot - top);
    let hw = (r.w * 0.36).min(420.0);
    let hdg = Rect::new((r.x + (r.w - hw) / 2.0).round(), r.y + r.h - 38.0, hw.round(), 32.0);
    let ix = spd.x + spd.w + 16.0;
    let iy = fma.y + fma.h + 4.0;
    let inner = Rect::new(ix, iy, alt.x - 16.0 - ix, hdg.y - 6.0 - iy);
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
        txt_r(&format!("{:0width$.0}", v, width = digits), x_right - 3.0, y, size, WHITE);
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
    draw_speed(ac, &g, info.time);
    draw_altitude(ac, &g);
    draw_vsi(ac.d.vs, g.vsi);
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
            // Perte du signal : neige qui s'éteint, écran barré
            draw_rectangle(r.x, r.y, r.w, r.h, BLACK);
            crate::mishap::static_noise(r, (0.55 * (1.0 - since / 2.5)).max(0.0) as f32, info.time);
            crate::mishap::red_x(r, false);
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
        draw_line(a.x, a.y, b.x, b.y, 2.0, LINE);
    }
    draw_horizon_compass(ac, v);
    draw_tunnel(ac, v, info.dest, info.time);
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
                    draw_circle(dot.x, dot.y, 2.5, LINE);
                }
                if let Some(l) = at(side * 0.05) {
                    txt_c(&format!("{}", p.abs()), l.x, l.y + 6.0, 20.0, LINE);
                }
                if p < 0 {
                    // Tirets inclinés vers l'extérieur pour les assiettes négatives
                    for k in 0..3 {
                        let k0 = 0.075 + k as f32 * 0.018;
                        if let (Some(a), Some(b)) = (at(side * k0), at(side * (k0 + 0.010))) {
                            let drop = 3.0 + k as f32 * 3.0;
                            draw_line(a.x, a.y + drop, b.x, b.y + drop + 3.0, 2.0, LINE);
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
        for (w, col) in [(4.0, Color::new(0.0, 0.0, 0.0, 0.6)), (2.0, LINE)] {
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
    for (w, col) in [(4.0, Color::new(0.0, 0.0, 0.0, 0.55)), (2.0, LINE)] {
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
        draw_triangle(z, z + vec2(-7.0, -11.0), z + vec2(7.0, -11.0), LINE);
        let phi = ac.phi.to_degrees() as f32;
        let a = (-phi).clamp(-62.0, 62.0);
        let n = vec2(a.to_radians().sin(), -a.to_radians().cos());
        let tg = vec2(-n.y, n.x);
        let tip = pt(a, rad - 2.0);
        let bc = if phi.abs() > 60.0 { RED } else if phi.abs() > 35.0 { AMBER } else { LINE };
        draw_triangle(tip, tip - n * 13.0 + tg * 7.0, tip - n * 13.0 - tg * 7.0, bc);
        // Barre de dérapage (vol coordonné : centrée)
        let b0 = tip - n * 17.0;
        draw_line(b0.x - tg.x * 8.0, b0.y - tg.y * 8.0, b0.x + tg.x * 8.0, b0.y + tg.y * 8.0, 3.0, bc);
    }

    crate::pfd_plus::terrain_warning(clr, s, info.time);
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
            let ly = s.y + s.h - 50.0;
            for k in [-2.0f32, -1.0, 1.0, 2.0] {
                draw_circle_lines(cx + k * 36.0, ly, 4.0, 1.5, LINE);
            }
            draw_poly(cx + (-loc_dev as f32 / 1.25 * 36.0).clamp(-80.0, 80.0), ly, 4, 8.0, 0.0, MAGENTA_S);
            let gx = s.x + s.w - 24.0;
            for k in [-2.0f32, -1.0, 1.0, 2.0] {
                draw_circle_lines(gx, cy + k * 36.0, 4.0, 1.5, LINE);
            }
            draw_poly(gx, cy + (gs_dev as f32 / 0.35 * 36.0).clamp(-80.0, 80.0), 4, 8.0, 0.0, MAGENTA_S);
            let num = crate::airports::Airport::runway_number(thdg);
            let lbl = format!("ILS {:02} {}  {:.1} NM", num, a.icao, along / NM);
            let w = txt_w(&lbl, 14.0) + 14.0;
            rrect(Rect::new(cx - w / 2.0, ly + 10.0, w, 20.0), 5.0, BLOCK);
            txt_c(&lbl, cx, ly + 25.0, 14.0, MAGENTA_S);
        }
    }

    // Radioaltimètre
    if d.agl < 2500.0 * FT && !ac.on_ground {
        let ra = d.agl / FT;
        let c = if ra < 200.0 { AMBER } else { GREEN_S };
        let s_ra = if ra < 50.0 { format!("{:.0}", ra) } else { format!("{:.0}", (ra / 10.0).round() * 10.0) };
        txt_c(&s_ra, cx, cy + 80.0, 28.0, c);
    }

    // Enregistrement
    if info.time.fract() < 0.5 {
        draw_circle(s.x + 12.0, s.y + 13.0, 4.0, RED);
    }
    txt("REC  FTV-01", s.x + 20.0, s.y + 18.0, 13.0, Color::new(0.9, 0.9, 0.9, 0.8));
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
    let memos: [(bool, &str, Color, bool); 6] = [
        (ac.ovr_engine, "ENG OVRD", AMBER, true),
        (ac.ovr_fbw, "FBW OVRD", AMBER, true),
        (!ac.ovr_engine && ac.fadec_limit < 0.995, "FADEC LIMIT", AMBER, false),
        (ac.speedbrake, sb_label, sb_col, sb_col == AMBER),
        (park, "FREIN PARC", park_col, park_col == AMBER),
        (braking, "FREINAGE", WHITE, false),
    ];
    for (on, label, col, flash) in memos {
        if on {
            draw_rectangle(s.x + 8.0, oy - 13.0, 92.0, 17.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle_lines(s.x + 8.0, oy - 13.0, 92.0, 17.0, 1.0, col);
            if blink || !flash {
                txt_c(label, s.x + 54.0, oy, if label.len() > 10 { 11.0 } else { 13.0 }, col);
            }
            oy += 21.0;
        }
    }
    if braking {
        // Décélération au freinage
        let dec = (-ac.d.accel / 9.81).max(0.0);
        txt(&format!("DECEL {dec:.2} G"), s.x + 10.0, oy - 2.0, 11.0, WHITE);
    }

    // Alertes
    let mut yy = cy + 120.0;
    for (_, msg) in info.alerts.iter().filter(|(l, _)| *l == Level::Warning).take(3) {
        if (info.time * 2.0).fract() < 0.7 {
            let w = txt_w(msg, 22.0) + 24.0;
            draw_rectangle(cx - w / 2.0, yy - 22.0, w, 30.0, Color::new(0.0, 0.0, 0.0, 0.7));
            txt_c(msg, cx, yy, 22.0, RED);
        }
        yy += 32.0;
    }
    scissor(None);
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
    txt_c("MENU", m.x + m.w / 2.0, m.y + 13.0, 12.0, LABEL);
    let x0 = m.x + m.w + 10.0;
    let x1 = b.x + b.w - 180.0;
    crate::pfd_plus::fma(ac, x0, b.y + 8.0, x1 - x0, info.dest);
    draw_line(x1, b.y + 4.0, x1, b.y + b.h - 4.0, 1.0, Color::new(0.5, 0.52, 0.55, 0.7));
    let mode = ac.mode_name();
    txt_c(&format!("TBCC {mode}"), x1 + 90.0, b.y + 14.0, 13.0, tbcc_color(mode));
    let lc = if info.latency_ms > 200.0 { AMBER } else { GREEN_S };
    txt_c(&format!("{} {:.0}ms  x{:.0}", info.link_name, info.latency_ms, info.warp), x1 + 90.0, b.y + 26.0, 10.0, lc);
}

/// Fond d'échelle translucide.
fn tape_bg(t: Rect) {
    rrect(t, 6.0, PANEL);
    rrect_lines(t, 6.0, 1.0, EDGE);
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
        draw_line(lx - l, y, lx, y, 1.5, LINE);
        if v % 20 == 0 {
            txt_r(&format!("{:03}", v), lx - 13.0, y + 5.0, 15.0, LABEL);
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
    rrect_lines(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, 1.5, LINE);
    draw_triangle(vec2(lx + 2.0, cy), vec2(bx + bw, cy - 8.0), vec2(bx + bw, cy + 8.0), LINE);
    draw_rectangle(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, 1.5, LINE);
    let s10 = (spd / 10.0).floor();
    if s10 > 0.0 {
        txt_r(&format!("{s10:.0}"), bx + bw - 21.0, cy + 9.0, 24.0, WHITE);
    }
    drum(bx + bw, cy, 20.0, 46.0, spd as f64, 1.0, 10.0, 1, 22.0);
    // Au-dessus : vitesse vraie et consigne
    rrect(Rect::new(t.x, t.y - 34.0, t.w, 32.0), 5.0, BLOCK);
    txt(&format!("TAS {:.0}", ac.tas / KT), t.x + 2.0, t.y - 20.0, 13.0, WHITE);
    match ac.ap.spd_keas.filter(|_| ac.ap.at_on) {
        Some(k) => txt(&format!("SEL {k:.0}"), t.x + 2.0, t.y - 6.0, 13.0, MAGENTA_S),
        None if ac.ap.at_on => txt(&format!("SEL M{:.2}", ac.ap.mach_tgt), t.x + 2.0, t.y - 6.0, 13.0, MAGENTA_S),
        None => txt(if ac.air_data_ovrd { "INS" } else { "KEAS" }, t.x + 2.0, t.y - 6.0, 12.0, LABEL),
    }
    // En dessous : Mach, vitesse sol, incidence, facteur de charge
    let y = t.y + t.h + 24.0;
    rrect(Rect::new(t.x, t.y + t.h + 4.0, t.w + 52.0, 86.0), 5.0, BLOCK);
    let m = if d.mach < 1.0 { format!("M {:.2}", d.mach).replace("M 0.", "M .") } else { format!("M {:.2}", d.mach) };
    let mc = if d.mach >= 5.0 { MAGENTA_S } else if d.mach >= 1.0 { CYAN_S } else { WHITE };
    txt(&m, t.x + 2.0, y, 22.0, mc);
    txt(&format!("GS {:.0}", ac.ground_speed() / KT), t.x + 2.0, y + 17.0, 13.0, LABEL);
    let a = d.alpha.to_degrees();
    let ac_col = if a > 7.0 && ac.eng.scram == ScramState::Lit { AMBER } else { LABEL };
    txt(&format!("a {a:+.1}°"), t.x + 2.0, y + 33.0, 13.0, ac_col);
    if ac.adc_degraded() {
        draw_rectangle(lx - 34.0, y + 21.0, 34.0, 16.0, Color::new(0.0, 0.0, 0.0, 0.8));
        draw_rectangle_lines(lx - 34.0, y + 21.0, 34.0, 16.0, 1.0, AMBER);
        txt_c("ADC", lx - 17.0, y + 34.0, 12.0, AMBER);
    }
    let nz = d.nz;
    let gc = if !(-0.5..=2.5).contains(&nz) { AMBER } else { GREEN_S };
    txt(&format!("G {nz:+.2}"), t.x + 2.0, y + 49.0, 13.0, gc);
    let mc = if ac.g_max > 3.0 || ac.g_min < -1.0 { AMBER } else { DIM };
    txt(&format!("MAX {:+.1}  MIN {:+.1}", ac.g_max, ac.g_min), t.x + 2.0, y + 64.0, 11.0, mc);
}

fn draw_altitude(ac: &Aircraft, g: &Geo) {
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
        draw_line(lx, y, lx + l, y, 1.5, LINE);
        if a % 200 == 0 {
            txt(&format!("{a}"), lx + 14.0, y + 5.0, 15.0, LABEL);
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
    if let Some((_, _, _, _, el)) = ac.approach {
        let mda = (el / FT) as f32 + 200.0;
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
    rrect_lines(Rect::new(bx, cy - 15.0, bw, 30.0), 4.0, 1.5, LINE);
    draw_triangle(vec2(lx - 2.0, cy), vec2(bx, cy - 8.0), vec2(bx, cy + 8.0), LINE);
    draw_rectangle(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, 1.5, LINE);
    let a100 = (alt / 100.0).floor();
    txt_r(&format!("{a100:.0}"), bx + bw - 31.0, cy + 9.0, 22.0, WHITE);
    drum(bx + bw, cy, 30.0, 46.0, alt as f64, 20.0, 100.0, 2, 18.0);
    // Au-dessus : consigne d'altitude
    rrect(Rect::new(t.x, t.y - 34.0, t.w, 32.0), 5.0, BLOCK);
    txt("GPS ALT", t.x + 4.0, t.y - 20.0, 11.0, LABEL);
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        txt(&format!("{:.0}", ac.ap.alt_tgt_ft), t.x + 4.0, t.y - 6.0, 14.0, MAGENTA_S);
    } else {
        txt("STD", t.x + 4.0, t.y - 6.0, 12.0, LABEL);
    }
    // En dessous : niveau de vol, pression dynamique, peau
    let y = t.y + t.h + 24.0;
    rrect(Rect::new(t.x, t.y + t.h + 4.0, g.vsi.x + g.vsi.w - t.x, 60.0), 5.0, BLOCK);
    txt(&format!("FL{:03.0}", alt / 100.0), t.x + 2.0, y, 20.0, WHITE);
    let qcol = if d.q > Q_LIMIT { RED } else if d.q > 90_000.0 { AMBER } else { LABEL };
    txt(&format!("q {:.0} kPa", d.q / 1000.0), t.x + 2.0, y + 17.0, 13.0, qcol);
    let skin = ac.skin_temp - 273.15;
    let scol = if skin > SKIN_LIMIT_C { RED } else if skin > SKIN_LIMIT_C - 100.0 { AMBER } else { LABEL };
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
            draw_line(x, y, x + 5.0, y, 1.5, LINE);
            txt(&format!("{}", *k as i32), x + 8.0, y + 4.0, 11.0, LABEL);
        }
    }
    draw_line(x, cy, x + 4.0, cy, 1.5, LINE);
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
        draw_line(x, y, x, y + l, 1.5, LINE);
        if hh % 10 == 0 {
            let lbl = hh.rem_euclid(360) / 10;
            txt_c(&format!("{}", if lbl == 0 { 36 } else { lbl }), x, y + 25.0, 14.0, LABEL);
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
    txt_c("GEAR", cx, cy - 22.0, 12.0, LABEL);
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

/// « Tunnel dans le ciel » : portes 3D le long de la trajectoire à suivre (plan de descente ILS,
/// sinon route et altitude de l'autopilote). Le pilote vole « dans » les cadres.
fn draw_tunnel(ac: &Aircraft, v: &View, dest: usize, time: f64) {
    if ac.on_ground {
        return;
    }
    let eye = crate::terrain::EyeFrame::new(ac);
    let mut gates: Vec<(f64, f64, f64, f64)> = Vec::new();
    let (mut w, mut h) = (160.0, 100.0);
    let mut col = MAGENTA_S;
    if let Some((_, tla, tlo, hdg, elev)) = ac.approach {
        let along = along_before(tla, tlo, hdg, ac.lat, ac.lon);
        if along > 400.0 && along < 45_000.0 {
            let sp = 800.0;
            let mut s = ((along - 300.0) / sp).floor() * sp;
            while s >= 0.0 && gates.len() < 12 {
                let (la, lo) = destination(tla, tlo, wrap360(hdg + 180.0), s);
                gates.push((la, lo, elev + 15.0 + s * 3f64.to_radians().tan(), hdg));
                s -= sp;
            }
            col = Color::new(0.35, 1.0, 0.55, 1.0);
            (w, h) = (140.0, 90.0);
        }
    }
    if gates.is_empty() && ac.ap.on {
        let apt = &AIRPORTS[dest];
        let brg = if ac.ap.lat == LatMode::Nav { dist_brg(ac.lat, ac.lon, apt.lat, apt.lon).1 } else { ac.ap.hdg_tgt };
        let sp = (ac.tas * 6.0).max(1_500.0);
        let alt_t = if ac.ap.vert == VertMode::Alt { ac.ap.alt_tgt_ft * FT } else { ac.alt };
        for k in 2..=10 {
            let s = k as f64 * sp;
            let (la, lo) = destination(ac.lat, ac.lon, brg, s);
            // Raccord progressif vers l'altitude visée (pente visuelle de 6 % maxi)
            let alt = ac.alt + (alt_t - ac.alt).clamp(-s * 0.06, s * 0.06);
            gates.push((la, lo, alt, brg));
        }
        w = (ac.tas * 0.5).clamp(180.0, 900.0);
        h = w * 0.6;
    }
    let n = gates.len();
    // Du plus lointain au plus proche
    for (k, &(la, lo, alt, brg)) in gates.iter().enumerate().rev() {
        let (l_la, l_lo) = destination(la, lo, wrap360(brg - 90.0), w / 2.0);
        let (r_la, r_lo) = destination(la, lo, wrap360(brg + 90.0), w / 2.0);
        let c = [
            eye.local(l_la, l_lo, alt - h / 2.0),
            eye.local(r_la, r_lo, alt - h / 2.0),
            eye.local(r_la, r_lo, alt + h / 2.0),
            eye.local(l_la, l_lo, alt + h / 2.0),
        ];
        let pts: Vec<Option<Vec2>> = c.iter().map(|&p| v.project(p)).collect();
        if pts.iter().any(|p| p.is_none()) {
            continue;
        }
        let pts: Vec<Vec2> = pts.into_iter().flatten().collect();
        let fade = 1.0 - k as f32 / n.max(1) as f32 * 0.7;
        let pulse = 0.85 + 0.15 * ((time * 3.0 - k as f64 * 0.6).sin() as f32);
        let cc = Color::new(col.r, col.g, col.b, fade * pulse);
        // Coins en équerre (cadre ouvert, moins encombrant qu'un rectangle plein)
        for i in 0..4 {
            let p0 = pts[i];
            let pa = pts[(i + 1) % 4];
            let pb = pts[(i + 3) % 4];
            let a = p0 + (pa - p0) * 0.25;
            let b = p0 + (pb - p0) * 0.25;
            draw_line(p0.x, p0.y, a.x, a.y, 2.0, cc);
            draw_line(p0.x, p0.y, b.x, b.y, 2.0, cc);
        }
    }
}
