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
    pub threats: Vec<crate::pfd_plus::ThreatCue>,
}

const LINE: Color = Color::new(0.92, 0.94, 0.95, 1.0);
const LABEL: Color = Color::new(0.85, 0.88, 0.9, 1.0);

/// Fenêtre de la vision synthétique dans l'écran PFD.
pub fn svs_rect(r: Rect) -> Rect {
    // Proportions mesurées sur l'écran du cockpit : SVS ~70 % x 81 %, décalé de 15 % / 10 %
    Rect::new((r.x + r.w * 0.152).round(), (r.y + r.h * 0.10).round(), (r.w * 0.70).round(), (r.h * 0.81).round())
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
    let s = v.rect;
    rrect_lines(r, 10.0, 1.5, Color::new(0.78, 0.80, 0.82, 0.85));
    draw_rectangle_lines(s.x - 1.0, s.y - 1.0, s.w + 2.0, s.h + 2.0, 1.0, Color::new(0.45, 0.47, 0.5, 1.0));

    draw_svs_overlay(ac, v, info);
    draw_speed(ac, r, s, info.time);
    draw_g_bracket(ac, r, s);
    draw_altitude(ac, r, s);
    draw_vsi(ac.d.vs, r, s);
    draw_heading(ac, s, info);
    draw_header(ac, r, s, info);

    if ac.crashed.is_some() {
        let (cx, cy) = (s.x + s.w / 2.0, s.y + s.h / 2.0);
        draw_rectangle(s.x, s.y, s.w, s.h, Color::new(0.0, 0.0, 0.0, 0.75));
        txt_c("LIAISON PERDUE - NO SIGNAL", cx, cy - 20.0, 30.0, RED);
        txt_c(ac.crashed.as_deref().unwrap_or(""), cx, cy + 14.0, 18.0, AMBER);
        txt_c("ENTREE : nouvel essai", cx, cy + 46.0, 18.0, WHITE);
    }
}

fn draw_svs_overlay(ac: &Aircraft, v: &View, info: &PfdInfo) {
    let s = v.rect;
    // Visibilité des informations éphémères (façon HUD)
    let vis = crate::pfd_plus::hud_vis(ac, &info.wx, &info.threats, info.dest, s);
    let d = &ac.d;
    scissor(Some(s));
    let hr = Vec3::new(ac.psi.cos() as f32, 0.0, ac.psi.sin() as f32);
    let big = 100_000.0f32;
    let (cx, cy) = (s.x + s.w / 2.0, s.y + s.h / 2.0);

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

    // Inclinaison : graduations en bas, triangle mobile
    {
        let rad = s.h * 0.43;
        for a in [-45.0f32, -30.0, -20.0, -10.0, 0.0, 10.0, 20.0, 30.0, 45.0] {
            let t = (90.0 + a).to_radians();
            let l = if a == 0.0 || a.abs() == 30.0 { 10.0 } else { 6.0 };
            let (p0, p1) = (vec2(cx + rad * t.cos(), cy + rad * t.sin()), vec2(cx + (rad + l) * t.cos(), cy + (rad + l) * t.sin()));
            draw_line(p0.x, p0.y, p1.x, p1.y, 1.5, Color::new(1.0, 1.0, 1.0, 0.7));
        }
        let t = (90.0 - ac.phi.to_degrees() as f32).to_radians();
        let tip = vec2(cx + (rad - 2.0) * t.cos(), cy + (rad - 2.0) * t.sin());
        let n = vec2(t.cos(), t.sin());
        let tg = vec2(-n.y, n.x);
        let bc = if ac.phi.abs() > 35f64.to_radians() { AMBER } else { LINE };
        draw_triangle(tip, tip - n * 13.0 + tg * 7.0, tip - n * 13.0 - tg * 7.0, bc);
    }

    crate::pfd_plus::terrain_warning(clr, s, info.time);
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
        txt_c(&format!("BANK {:.0}", ac.phi.to_degrees().abs()), cx, cy + s.h * 0.43 - 18.0, 13.0, bc);
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
            txt(&format!("ILS {:02} {}  {:.1} NM", num, a.icao, along / NM), s.x + 10.0, s.y + s.h - 28.0, 15.0, MAGENTA_S);
        }
    }

    // Radioaltimètre
    if d.agl < 2500.0 * FT && !ac.on_ground {
        let ra = d.agl / FT;
        let c = if ra < 200.0 { AMBER } else { GREEN_S };
        let s_ra = if ra < 50.0 { format!("{:.0}", ra) } else { format!("{:.0}", (ra / 10.0).round() * 10.0) };
        txt_c(&s_ra, cx, cy + 80.0, 28.0, c);
    }

    // Mode TBCC, liaison, enregistrement
    let m = ac.mode_name();
    let mc = match m {
        "SCRAMJET" => MAGENTA_S,
        "RAMJET" | "DUAL TURB+RAM" => CYAN_S,
        "TURBINE" => WHITE,
        _ => AMBER,
    };
    txt_r(&format!("TBCC {m}"), s.x + s.w - 8.0, s.y + 18.0, 15.0, mc);
    let lc = if info.latency_ms > 200.0 { AMBER } else { GREEN_S };
    txt_r(&format!("LINK {} {:.0}ms  x{:.0}", info.link_name, info.latency_ms, info.warp), s.x + s.w - 8.0, s.y + 34.0, 13.0, lc);
    if info.time.fract() < 0.5 {
        draw_circle(s.x + 12.0, s.y + 13.0, 4.0, RED);
    }
    txt("REC  FTV-01", s.x + 20.0, s.y + 18.0, 13.0, Color::new(0.9, 0.9, 0.9, 0.8));
    // Informations éphémères façon HUD : RWR (haut gauche), vent / météo (haut droite)
    if !ac.on_ground {
        crate::pfd_plus::rwr(ac, &info.threats, Rect::new(s.x + 8.0, s.y + 26.0, 92.0, 92.0), info.time, vis.rwr);
        crate::pfd_plus::wind_block(ac, &info.wx, Rect::new(s.x + s.w - 150.0, s.y + 44.0, 142.0, 52.0), info.time, vis.wind);
    }
    // Annonces d'override et de limitation FADEC
    let mut oy = if ac.on_ground || vis.rwr < 0.01 { s.y + 36.0 } else { s.y + 138.0 };
    let blink = (info.time * 2.0).fract() < 0.65;
    for (on, label) in [(ac.ovr_engine, "ENG OVRD"), (ac.ovr_fbw, "FBW OVRD"), (!ac.ovr_engine && ac.fadec_limit < 0.995, "FADEC LIMIT")] {
        if on {
            draw_rectangle(s.x + 8.0, oy - 13.0, 92.0, 17.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle_lines(s.x + 8.0, oy - 13.0, 92.0, 17.0, 1.0, AMBER);
            if blink || label == "FADEC LIMIT" {
                txt_c(label, s.x + 54.0, oy, 13.0, AMBER);
            }
            oy += 21.0;
        }
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

fn draw_header(ac: &Aircraft, r: Rect, s: Rect, info: &PfdInfo) {
    let y = r.y + 8.0;
    let gs = ac.ground_speed() / KT;
    let x0 = r.x + r.w * 0.06;
    txt_r("GS", x0, y + 13.0, 14.0, LABEL);
    txt(&format!("{gs:.0}"), x0 + 8.0, y + 13.0, 16.0, WHITE);
    txt_r("M", x0, y + 29.0, 14.0, LABEL);
    let m = if ac.d.mach < 1.0 { format!("{:.2}", ac.d.mach).trim_start_matches('0').to_string() } else { format!("{:.2}", ac.d.mach) };
    let mc = if ac.d.mach >= 5.0 { MAGENTA_S } else if ac.d.mach >= 1.0 { CYAN_S } else { WHITE };
    txt(&m, x0 + 8.0, y + 29.0, 16.0, mc);
    txt("TRUE AIRSPEED", s.x + 12.0, y + 22.0, 14.0, LABEL);
    txt(&format!("{:.0}", ac.tas / KT), s.x + 128.0, y + 23.0, 22.0, WHITE);
    txt("Knots", s.x + 200.0, y + 22.0, 14.0, LABEL);

    // Annonciateur de modes (FMA)
    let fx = s.x + 290.0;
    crate::pfd_plus::fma(ac, fx, y + 4.0, s.x + s.w - 10.0 - fx, info.dest);
    // Consigne d'altitude
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        txt(&format!("{:.0}", ac.ap.alt_tgt_ft), s.x + s.w + 8.0, y + 22.0, 18.0, MAGENTA_S);
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

fn draw_speed(ac: &Aircraft, r: Rect, s: Rect, time: f64) {
    let d = &ac.d;
    let lx = s.x - 18.0;
    let cy = s.y + s.h / 2.0;
    let ppk = s.h / 150.0;
    let spd = (d.keas + adc_noise(ac, time)).max(0.0) as f32;
    scissor(Some(Rect::new(r.x, s.y - 4.0, s.x - r.x, s.h + 8.0)));
    draw_line(lx, s.y, lx, s.y + s.h, 1.5, LINE);
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
    // Bandes : limite q (rouge), VLE train (ambre), VR
    let q_lim = ((2.0 * Q_LIMIT / 1.225).sqrt() / KT) as f32;
    let yq = cy - (q_lim - spd) * ppk;
    draw_rectangle(lx, yq - 600.0, 4.0, 600.0, RED);
    if ac.gear_pos > 0.01 {
        let yg = cy - (GEAR_VLE as f32 - spd) * ppk;
        draw_rectangle(lx + 4.0, yg - 600.0, 3.0, 600.0, AMBER);
    }
    if !ac.on_ground {
        // Bande basse vitesse (rouge / noir) sous la vitesse de décrochage estimée
        let vs1g = (crate::autoflight::vapp(ac) / 1.23) as f32;
        let yl = cy - (vs1g - spd) * ppk;
        if yl < s.y + s.h {
            scissor(Some(Rect::new(lx, yl.max(s.y), 8.0, s.y + s.h - yl.max(s.y))));
            let mut yy = yl;
            while yy < s.y + s.h {
                draw_rectangle(lx, yy, 8.0, 6.0, RED);
                yy += 12.0;
            }
            scissor(Some(Rect::new(r.x, s.y - 4.0, s.x - r.x, s.h + 8.0)));
        }
        if ac.approach.is_some() && ac.gear_pos > 0.5 {
            let va = crate::autoflight::vapp(ac) as f32;
            let y = cy - (va - spd) * ppk;
            draw_line(lx, y, lx + 12.0, y, 2.0, CYAN_S);
            txt("VAPP", lx + 14.0, y + 4.0, 11.0, CYAN_S);
        }
    }
    if ac.on_ground {
        let y = cy - (170.0 - spd) * ppk;
        draw_line(lx, y, lx + 10.0, y, 2.0, CYAN_S);
        txt("R", lx + 12.0, y + 5.0, 14.0, CYAN_S);
    }
    if let Some(k) = ac.ap.spd_keas.filter(|_| ac.ap.at_on) {
        let y = (cy - (k as f32 - spd) * ppk).clamp(s.y, s.y + s.h);
        draw_triangle(vec2(lx, y), vec2(lx + 9.0, y - 6.0), vec2(lx + 9.0, y + 6.0), MAGENTA_S);
    }
    let trend = (d.accel * 10.0 / KT * (crate::atmosphere::isa(ac.alt).rho / 1.225).sqrt()) as f32;
    if trend.abs() > 2.0 {
        draw_line(lx + 3.0, cy, lx + 3.0, cy - trend * ppk, 2.0, GREEN_S);
    }
    scissor(None);
    // Fenêtre de lecture : centaines/dizaines fixes + tambour des unités
    let bw = 64.0;
    let bx = lx - 8.0 - bw;
    draw_rectangle(bx, cy - 15.0, bw, 30.0, BLACK);
    draw_rectangle_lines(bx, cy - 15.0, bw, 30.0, 1.5, LINE);
    draw_rectangle(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 20.0, cy - 24.0, 20.0, 48.0, 1.5, LINE);
    draw_triangle(vec2(lx - 1.0, cy), vec2(lx - 9.0, cy - 7.0), vec2(lx - 9.0, cy + 7.0), LINE);
    let s10 = (spd / 10.0).floor();
    if s10 > 0.0 {
        txt_r(&format!("{s10:.0}"), bx + bw - 21.0, cy + 9.0, 24.0, WHITE);
    }
    drum(bx + bw, cy, 20.0, 46.0, spd as f64, 1.0, 10.0, 1, 22.0);
    // Sous l'échelle
    // Sous la vue SVS : une seule ligne (place mesurée sur l'écran du cockpit)
    let y = s.y + s.h + 17.0;
    let a = d.alpha.to_degrees();
    let ac_col = if a > 7.0 && ac.eng.scram == ScramState::Lit { AMBER } else { LABEL };
    txt(&format!("a {a:+.1}°"), r.x + 14.0, y, 16.0, ac_col);
    if ac.adc_degraded() {
        draw_rectangle(lx - 42.0, y - 14.0, 40.0, 18.0, Color::new(0.0, 0.0, 0.0, 0.8));
        draw_rectangle_lines(lx - 42.0, y - 14.0, 40.0, 18.0, 1.0, AMBER);
        txt_c("ADC", lx - 22.0, y, 14.0, AMBER);
    } else {
        txt(if ac.air_data_ovrd { "INS" } else { "EAS" }, lx - 34.0, y, 15.0, if ac.air_data_ovrd { CYAN_S } else { LABEL });
    }
    let qcol = if d.q > Q_LIMIT { RED } else if d.q > 90_000.0 { AMBER } else { LABEL };
    // q et température de peau : colonne gauche, sous les mémoires G
    let yq = s.y + s.h * 0.72 + 58.0;
    txt(&format!("q {:.0} kPa", d.q / 1000.0), r.x + 14.0, yq, 13.0, qcol);
    let skin = ac.skin_temp - 273.15;
    let scol = if skin > SKIN_LIMIT_C { RED } else if skin > SKIN_LIMIT_C - 100.0 { AMBER } else { LABEL };
    txt(&format!("PEAU {skin:.0}°C"), r.x + 14.0, yq + 16.0, 13.0, scol);
    // Bouton MENU
    let mx = s.x + 6.0;
    draw_rectangle(mx, y - 13.0, 70.0, 18.0, Color::new(0.22, 0.24, 0.27, 1.0));
    txt_c("MENU", mx + 35.0, y + 1.0, 14.0, LABEL);
}

fn draw_g_bracket(ac: &Aircraft, r: Rect, s: Rect) {
    let nz = ac.d.nz;
    let x = r.x + 16.0;
    let top = s.y + s.h * 0.22;
    let bot = s.y + s.h * 0.72;
    let gy = |g: f32| top + (3.5 - g) / 6.0 * (bot - top);
    draw_line(x, top, x, bot, 1.5, LINE);
    for g in [3.5f32, 1.0, -2.5] {
        let y = gy(g);
        draw_line(x, y, x + 6.0, y, 1.5, LINE);
        txt(&format!("{:+.1}G", g), x + 8.0, y + 5.0, 14.0, LABEL);
    }
    // Mémoires G max / G min atteints (repères ambre)
    for g in [ac.g_max, ac.g_min] {
        if (g - 1.0).abs() > 0.05 {
            let y = gy(g.clamp(-2.6, 3.6) as f32);
            draw_line(x - 9.0, y, x + 4.0, y, 2.5, AMBER);
        }
    }
    let gc = if !(-0.5..=2.5).contains(&nz) { AMBER } else { GREEN_S };
    let y = gy(nz.clamp(-2.6, 3.6) as f32);
    draw_triangle(vec2(x, y), vec2(x - 8.0, y - 5.0), vec2(x - 8.0, y + 5.0), gc);
    let mc = if ac.g_max > 3.0 || ac.g_min < -1.0 { AMBER } else { LABEL };
    txt(&format!("MAX {:+.1}G", ac.g_max), x - 6.0, bot + 24.0, 13.0, mc);
    txt(&format!("MIN {:+.1}G", ac.g_min), x - 6.0, bot + 40.0, 13.0, mc);
    txt(&format!("G {:+.2}", nz), x - 6.0, top - 14.0, 13.0, gc);
}

fn draw_altitude(ac: &Aircraft, r: Rect, s: Rect) {
    let d = &ac.d;
    let lx = s.x + s.w + 16.0;
    let cy = s.y + s.h / 2.0;
    let alt = (ac.alt / FT) as f32;
    let ppf = s.h / 1000.0;
    let right = r.x + r.w - 48.0;
    scissor(Some(Rect::new(s.x + s.w + 2.0, s.y - 4.0, right - s.x - s.w, s.h + 8.0)));
    draw_line(lx, s.y, lx, s.y + s.h, 1.5, LINE);
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
    if ac.ap.on && ac.ap.vert == VertMode::Alt {
        let y = (cy - (ac.ap.alt_tgt_ft as f32 - alt) * ppf).clamp(s.y, s.y + s.h);
        draw_rectangle(lx - 6.0, y - 5.0, 6.0, 10.0, MAGENTA_S);
    }
    // Tendance d'altitude à 6 s
    let trend = (d.vs * 6.0 / FT) as f32 * ppf;
    if trend.abs() > 3.0 {
        let yt = (cy - trend).clamp(s.y, s.y + s.h);
        draw_line(lx + 4.0, cy, lx + 4.0, yt, 2.5, GREEN_S);
    }
    // Minimums (200 ft au-dessus du seuil) en approche
    if let Some((_, _, _, _, el)) = ac.approach {
        let mda = (el / FT) as f32 + 200.0;
        let y = cy - (mda - alt) * ppf;
        let c = if alt < mda { AMBER } else { CYAN_S };
        draw_line(lx - 10.0, y, lx + 30.0, y, 2.0, c);
        txt("MIN", lx + 32.0, y + 4.0, 11.0, c);
    }
    scissor(None);
    // Sol : hachures jaunes et noires sous l'élévation du terrain
    let yg = cy - ((d.elev / FT) as f32 - alt) * ppf;
    let top = yg.max(s.y);
    let bottom = s.y + s.h;
    if top < bottom {
        scissor(Some(Rect::new(lx + 1.0, top, 40.0, bottom - top)));
        draw_rectangle(lx + 1.0, top, 40.0, bottom - top, BLACK);
        let mut y0 = top - 40.0;
        while y0 < bottom + 40.0 {
            draw_line(lx + 1.0, y0 + 40.0, lx + 41.0, y0, 3.0, Color::new(0.95, 0.75, 0.1, 0.9));
            y0 += 10.0;
        }
        scissor(None);
    }
    // Trait ambre le long de l'échelle (bande de surveillance du sol)
    draw_line(lx - 3.0, s.y + 2.0, lx - 3.0, s.y + s.h - 2.0, 2.0, Color::new(0.95, 0.6, 0.15, 0.85));
    // Fenêtre de lecture en flèche : milliers/centaines fixes + tambour des dizaines (pas de 20 ft)
    let bw = 80.0;
    let bx = lx + 8.0;
    let pts = [vec2(lx + 1.0, cy), vec2(bx, cy - 15.0), vec2(bx + bw, cy - 15.0), vec2(bx + bw, cy + 15.0), vec2(bx, cy + 15.0)];
    draw_triangle(pts[0], pts[1], pts[4], BLACK);
    draw_rectangle(bx, cy - 15.0, bw, 30.0, BLACK);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        draw_line(a.x, a.y, b.x, b.y, 1.5, LINE);
    }
    draw_rectangle(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, BLACK);
    draw_rectangle_lines(bx + bw - 30.0, cy - 24.0, 30.0, 48.0, 1.5, LINE);
    let a100 = (alt / 100.0).floor();
    txt_r(&format!("{a100:.0}"), bx + bw - 31.0, cy + 9.0, 22.0, WHITE);
    drum(bx + bw, cy, 30.0, 46.0, alt as f64, 20.0, 100.0, 2, 18.0);
    let y = s.y + s.h + 17.0;
    txt("GPS ALT", lx, y, 13.0, LABEL);
    txt(&format!("FL{:03.0}", alt / 100.0), lx, y + 16.0, 13.0, LABEL);
}

fn draw_vsi(vs: f64, r: Rect, s: Rect) {
    let x = r.x + r.w - 40.0;
    let cy = s.y + s.h / 2.0;
    let half = s.h * 0.48;
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
            draw_line(x, y, x + 6.0, y, 1.5, LINE);
            txt(&format!("{}", *k as i32), x + 9.0, y + 5.0, 13.0, LABEL);
        }
    }
    draw_line(x, cy, x + 4.0, cy, 1.5, LINE);
    let kfpm = (vs / FT * 60.0 / 1000.0) as f32;
    let y = pos(kfpm);
    draw_line(x - 4.0, y, x + 12.0, y, 3.0, GREEN_S);
    if kfpm.abs() > 0.4 {
        txt_c(&format!("{:.0}", kfpm * 1000.0), x + 6.0, if kfpm > 0.0 { s.y - 6.0 } else { s.y + s.h + 16.0 }, 13.0, GREEN_S);
    }
}

fn draw_heading(ac: &Aircraft, s: Rect, info: &PfdInfo) {
    let cx = s.x + s.w / 2.0;
    let y = s.y + s.h + 4.0;
    let hdg = ac.hdg_deg() as f32;
    let ppd = s.w / 60.0;
    let w = s.w * 0.62;
    scissor(Some(Rect::new(cx - w / 2.0, y - 2.0, w, 30.0)));
    let base = (hdg / 5.0).floor() as i32 * 5;
    for k in -8..=8 {
        let h = base + k * 5;
        let x = cx + (h as f32 - hdg) * ppd;
        let l = if h % 10 == 0 { 9.0 } else { 5.0 };
        draw_line(x, y, x, y + l, 1.5, LINE);
        if h % 10 == 0 {
            let lbl = h.rem_euclid(360) / 10;
            txt_c(&format!("{}", if lbl == 0 { 36 } else { lbl }), x, y + 23.0, 14.0, LABEL);
        }
    }
    if ac.ap.on {
        let bx = cx + wrap180(ac.ap.hdg_tgt - hdg as f64) as f32 * ppd;
        draw_rectangle(bx - 4.0, y, 8.0, 6.0, MAGENTA_S);
    }
    // Route sol (losange vert)
    if !ac.on_ground {
        let tx = cx + wrap180(ac.track_deg() - hdg as f64) as f32 * ppd;
        stroke(&[vec2(tx, y + 1.0), vec2(tx + 5.0, y + 7.0), vec2(tx, y + 13.0), vec2(tx - 5.0, y + 7.0)], true, 1.5, GREEN_S);
    }
    scissor(None);
    draw_rectangle(cx - 22.0, y + 9.0, 44.0, 18.0, BLACK);
    draw_rectangle_lines(cx - 22.0, y + 9.0, 44.0, 18.0, 1.5, MAGENTA_S);
    txt_c(&format!("{:03.0}", hdg.round() % 360.0), cx, y + 23.0, 15.0, WHITE);
    // Distance destination
    let apt = &AIRPORTS[info.dest];
    let (dist, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    let dx = s.x + s.w - 4.0;
    txt_r(&format!("DIST {:.2} NM", dist / NM), dx, y + 16.0, 15.0, MAGENTA_S);
    let gs = (ac.tas * ac.gamma.cos()).max(1.0);
    let _ = (brg, gs);
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
