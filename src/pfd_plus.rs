//! Symbologie avancée de l'horizon (génération « glass cockpit » de combat) : annonciateur de modes
//! (FMA), trajectoire prédite 3D avec marge de franchissement du relief, repères 3D de destination et
//! d'objectif, couloir hypersonique, bilan énergie / autonomie, indexeur d'incidence.

use crate::aircraft::*;
use crate::airports::{self, AIRPORTS};
use crate::geo::*;
use crate::svs::View;
use crate::terrain::EyeFrame;
use crate::ui::*;
use macroquad::prelude::*;

const PANEL_BG: Color = Color::new(0.0, 0.0, 0.0, 0.55);
const ARMED: Color = Color::new(0.35, 0.85, 1.0, 1.0);

/// Repère 3D à afficher dans l'horizon (destination, objectif de reconnaissance...).
pub struct Cue {
    pub lat: f64,
    pub lon: f64,
    pub elev: f64,
    pub label: String,
    pub color: Color,
}

fn panel_box(r: Rect) {
    rrect(r, 6.0, PANEL_BG);
    rrect_lines(r, 6.0, 1.0, Color::new(0.55, 0.58, 0.62, 0.55));
}

// ---------- FMA ----------

/// Annonciateur de modes : quatre colonnes (poussée, latéral, vertical, AP), actif en vert,
/// armé en cyan dessous. Remplace la ligne de texte des modes.
pub fn fma(ac: &Aircraft, x0: f32, y: f32, w: f32, dest: usize) {
    let cols: [(String, Color, String); 4] = [
        if ac.ap.at_on {
            let a = match ac.ap.spd_keas {
                Some(k) if ac.ap.spd_managed => format!("SPD {k:.0}"),
                Some(k) => format!("SPD {k:.0}"),
                None => format!("MACH {:.2}", ac.ap.mach_tgt),
            };
            (a, GREEN_S, if ac.ap.spd_managed { "AUTO".into() } else { "SEL".into() })
        } else {
            ("MAN THR".into(), WHITE, format!("{:.0}%", ac.throttle * 100.0))
        },
        if ac.ap.on {
            let a = match ac.ap.lat {
                LatMode::Hdg => format!("HDG {:03.0}", ac.ap.hdg_tgt),
                LatMode::Nav => format!("NAV {}", AIRPORTS[dest].icao),
                LatMode::Loc => "LOC".into(),
            };
            (a, GREEN_S, if ac.ap.loc_armed { "LOC".into() } else { String::new() })
        } else {
            ("ROLL".into(), WHITE, String::new())
        },
        if ac.ap.on {
            let a = match ac.ap.vert {
                VertMode::Alt => format!("ALT {:.0}", ac.ap.alt_tgt_ft / 100.0 * 100.0),
                VertMode::Gs => "G/S".into(),
                VertMode::Flare => "FLARE".into(),
                VertMode::Pitch => format!("PITCH {:+.0}", ac.ap.pitch_tgt),
                VertMode::QHold => format!("DES q{:.0}", ac.ap.q_tgt / 1000.0),
            };
            (a, GREEN_S, if ac.ap.gs_armed { "G/S".into() } else { String::new() })
        } else {
            ("PITCH".into(), WHITE, String::new())
        },
        if ac.autoland.is_some() {
            ("AUTOLAND".into(), MAGENTA_S, "AP1".into())
        } else if ac.autotakeoff.is_some() {
            ("AUTO T/O".into(), MAGENTA_S, "AP1".into())
        } else if ac.ap.on {
            ("AP1".into(), GREEN_S, if ac.ap.at_on { "A/THR".into() } else { String::new() })
        } else {
            ("AP OFF".into(), AMBER, String::new())
        },
    ];
    let cw = w / 4.0;
    for (i, (active, c, armed)) in cols.iter().enumerate() {
        let x = x0 + i as f32 * cw;
        if i > 0 {
            draw_line(x, y - 4.0, x, y + 22.0, 1.0, Color::new(0.5, 0.52, 0.55, 0.7));
        }
        txt_c(active, x + cw / 2.0, y + 6.0, 13.0, *c);
        if !armed.is_empty() {
            txt_c(armed, x + cw / 2.0, y + 19.0, 10.0, if i == 3 || i == 0 { DIM } else { ARMED });
        }
    }
}

// ---------- Trajectoire prédite ----------

struct PathPt {
    lat: f64,
    lon: f64,
    alt: f64,
    t: f64,
}

/// Intégration simple (virage coordonné, facteur de charge actuel) sur `horizon` secondes.
fn predict(ac: &Aircraft, horizon: f64, dt: f64) -> Vec<PathPt> {
    let g = 9.81;
    let mut v = ac.tas.max(1.0);
    let (mut lat, mut lon, mut alt) = (ac.lat, ac.lon, ac.alt);
    let (mut psi, mut gam) = (ac.psi, ac.gamma);
    let psi_dot = g * ac.phi.sin() * ac.d.nz / (v * ac.gamma.cos().max(0.1));
    let gam_dot = g * (ac.d.nz * ac.phi.cos() - ac.gamma.cos()) / v;
    let mut out = vec![PathPt { lat, lon, alt, t: 0.0 }];
    let mut t = 0.0;
    while t < horizon {
        t += dt;
        psi += psi_dot * dt;
        gam = (gam + gam_dot * dt).clamp(-1.4, 1.4);
        v = (v + ac.d.accel * dt).max(1.0);
        let gs = v * gam.cos() * dt;
        let (la, lo) = destination(lat, lon, psi.to_degrees(), gs);
        lat = la;
        lon = lo;
        alt += v * gam.sin() * dt;
        out.push(PathPt { lat, lon, alt, t });
    }
    out
}

/// Trajectoire prédite à 30 s, repères à 10/20/30 s, marge minimale au-dessus du relief réel.
/// Renvoie la marge minimale (m) et l'instant où elle est atteinte.
pub fn predicted_path(ac: &Aircraft, v: &View, time: f64) -> Option<(f64, f64)> {
    if ac.on_ground || ac.tas < 30.0 {
        return None;
    }
    let horizon = if ac.d.mach > 3.0 { 60.0 } else { 30.0 };
    let pts = predict(ac, horizon, 1.0);
    let eye = EyeFrame::new(ac);
    let mut min_clr = (f64::MAX, 0.0);
    let mut impact: Option<Vec3> = None;
    for p in &pts[1..] {
        let clr = p.alt - airports::ground_elev(p.lat, p.lon);
        if clr < min_clr.0 {
            min_clr = (clr, p.t);
        }
        if clr < 0.0 && impact.is_none() {
            impact = Some(eye.local(p.lat, p.lon, p.alt));
        }
    }
    let col = if min_clr.0 < 0.0 {
        RED
    } else if min_clr.0 < 150.0 {
        AMBER
    } else {
        Color::new(0.45, 1.0, 0.65, 1.0)
    };
    let mut prev: Option<Vec2> = None;
    let mut last_mark: Option<Vec2> = None;
    for p in &pts {
        let Some(sp) = v.project(eye.local(p.lat, p.lon, p.alt)) else {
            prev = None;
            continue;
        };
        if let Some(q) = prev {
            // Pointillés qui défilent : lecture immédiate du sens de déplacement
            let phase = ((time * 2.0).fract() as f32 + p.t as f32) % 2.0;
            if phase < 1.2 {
                draw_line(q.x, q.y, sp.x, sp.y, 2.0, Color::new(col.r, col.g, col.b, 0.75));
            }
        }
        let mark = (p.t as i32) % 10 == 0 && p.t > 0.0 && last_mark.is_none_or(|m| (m - sp).length() > 28.0);
        if mark && v.rect.contains(sp) {
            last_mark = Some(sp);
            draw_circle_lines(sp.x, sp.y, 5.0, 1.5, col);
            txt(&format!("{:.0}", p.t), sp.x + 7.0, sp.y - 5.0, 11.0, col);
        }
        prev = Some(sp);
    }
    if let Some(ip) = impact.and_then(|p| v.project(p)) {
        let s = 9.0;
        draw_line(ip.x - s, ip.y - s, ip.x + s, ip.y + s, 3.0, RED);
        draw_line(ip.x - s, ip.y + s, ip.x + s, ip.y - s, 3.0, RED);
    }
    (min_clr.0 < f64::MAX).then_some(min_clr)
}

/// Chevrons « PULL UP » au-dessus du vecteur vitesse quand la trajectoire prédite percute le relief.
pub fn terrain_warning(clr: Option<(f64, f64)>, s: Rect, time: f64) {
    let Some((c, t)) = clr else { return };
    let (cx, cy) = (s.x + s.w / 2.0, s.y + s.h / 2.0);
    if c < 0.0 && (time * 3.0).fract() < 0.65 {
        for k in 0..3 {
            let y = cy - 40.0 - k as f32 * 14.0;
            stroke(&[vec2(cx - 26.0, y + 10.0), vec2(cx, y), vec2(cx + 26.0, y + 10.0)], false, 3.0, RED);
        }
        let msg = format!("TERRAIN  T-{t:.0}s");
        let w = txt_w(&msg, 18.0) + 20.0;
        rrect(Rect::new(cx - w / 2.0, cy - 110.0, w, 26.0), 5.0, Color::new(0.3, 0.0, 0.0, 0.8));
        txt_c(&msg, cx, cy - 91.0, 18.0, RED);
    }
}

// ---------- Repères 3D ----------

/// Repère d'un point au sol : losange + distance / ETA ; flèche au bord si hors champ.
pub fn cue(ac: &Aircraft, v: &View, c: &Cue, slot: usize) {
    let eye = EyeFrame::new(ac);
    let (dist, brg) = dist_brg(ac.lat, ac.lon, c.lat, c.lon);
    let gs = (ac.tas * ac.gamma.cos()).max(30.0);
    let eta = crate::mission::fmt_time(dist / gs);
    let s = v.rect;
    let ctr = vec2(s.x + s.w / 2.0, s.y + s.h / 2.0);
    let p3 = eye.local(c.lat, c.lon, c.elev);
    // Au-delà de l'horizon géométrique : repère posé sur la ligne d'horizon, dans son relèvement
    let h = (ac.alt - c.elev).max(1.0);
    let horizon = (2.0 * R_EARTH * h).sqrt();
    let proj = if dist > horizon * 0.95 {
        let dip = (R_EARTH / (R_EARTH + h)).acos();
        v.project_dir(crate::svs::dir(brg.to_radians(), -dip))
    } else {
        v.project(p3)
    };
    match proj.filter(|p| s.contains(*p)) {
        Some(p) => {
            let d = 9.0;
            let pts = [p + vec2(0.0, -d), p + vec2(d, 0.0), p + vec2(0.0, d), p + vec2(-d, 0.0)];
            stroke(&pts, true, 2.0, c.color);
            // Tige verticale jusqu'au sol : lecture de la position même de loin
            if let Some(top) = v.project(eye.local(c.lat, c.lon, c.elev + 600.0 + dist * 0.01)) {
                draw_line(p.x, p.y - d, top.x, top.y, 1.0, Color::new(c.color.r, c.color.g, c.color.b, 0.6));
            }
            let lbl = format!("{}  {:.0} NM  {eta}", c.label, dist / NM);
            let w = txt_w(&lbl, 12.0) + 10.0;
            let ly = p.y - 26.0 - slot as f32 * 20.0;
            rrect(Rect::new(p.x + 12.0, ly, w, 16.0), 4.0, PANEL_BG);
            txt(&lbl, p.x + 17.0, ly + 12.0, 12.0, c.color);
        }
        None => {
            // Flèche au bord gauche ou droit (moitié haute, hors des blocs du bas)
            let rel = wrap180(brg - ac.hdg_deg()) as f32;
            let side = if rel >= 0.0 { 1.0 } else { -1.0 };
            let p = vec2(ctr.x + side * (s.w / 2.0 - 18.0), s.y + s.h * 0.30 + slot as f32 * 34.0);
            let dirv = vec2(side, 0.0);
            let n = vec2(0.0, 1.0);
            draw_triangle(p + dirv * 12.0, p - dirv * 6.0 + n * 8.0, p - dirv * 6.0 - n * 8.0, c.color);
            let lbl = format!("{} {:.0}NM {:+.0}", c.label, dist / NM, rel);
            let w = txt_w(&lbl, 11.0);
            txt(&lbl, if side > 0.0 { p.x - 14.0 - w } else { p.x + 14.0 }, p.y + 4.0, 11.0, c.color);
        }
    }
}

// ---------- Couloir hypersonique ----------

/// Bloc couloir hypersonique : chaîne TBCC, pression dynamique dans le couloir 25-90 kPa, peau.
pub fn corridor(ac: &Aircraft, r: Rect) {
    panel_box(r);
    let x = r.x + 8.0;
    let w = r.w - 16.0;
    // Chaîne des modes de propulsion
    let e = &ac.eng;
    let stages = [
        ("TURB", e.side.iter().any(|s| *s == TurbState::Running) && e.doors < 0.5, 0.0, WHITE),
        ("RAM", e.ram == RamState::Lit, 2.0, CYAN_S),
        ("SCRAM", e.scram == ScramState::Lit, 4.8, MAGENTA_S),
    ];
    let sw = w / 3.0;
    for (i, (n, on, m, c)) in stages.iter().enumerate() {
        let b = Rect::new(x + i as f32 * sw + 1.0, r.y + 6.0, sw - 2.0, 18.0);
        let ready = ac.d.mach >= *m;
        if *on {
            rrect(b, 4.0, Color::new(c.r * 0.35, c.g * 0.35, c.b * 0.35, 1.0));
        }
        rrect_lines(b, 4.0, 1.0, if *on { *c } else if ready { Color::new(c.r, c.g, c.b, 0.5) } else { Color::new(0.4, 0.4, 0.42, 0.6) });
        txt_c(n, b.x + b.w / 2.0, b.y + 13.0, 11.0, if *on { WHITE } else if ready { *c } else { DIM });
    }
    // Prochaine transition
    let next = if e.scram == ScramState::Lit {
        None
    } else if e.ram == RamState::Lit {
        Some(("SCRAM", 4.8))
    } else {
        Some(("RAM", 2.0))
    };
    let mut y = r.y + 40.0;
    if let Some((n, m)) = next {
        let c = if ac.d.mach >= m { GREEN_S } else { DIM };
        txt(&if ac.d.mach >= m { format!("{n} DISPONIBLE") } else { format!("{n} a M{m:.1} (+{:.2})", m - ac.d.mach) }, x, y, 11.0, c);
    } else {
        txt(&format!("SCRAM ETABLI  M{:.2}", ac.d.mach), x, y, 11.0, MAGENTA_S);
    }
    y += 10.0;
    // Barre q : couloir vert 25-90 kPa, limite rouge 100
    let bar = |y: f32, label: &str, v: f64, vmax: f64, zones: &[(f64, f64, Color)], unit: &str| {
        txt(label, x, y + 9.0, 10.0, DIM);
        let b = Rect::new(x + 34.0, y, w - 34.0, 10.0);
        rrect(b, 3.0, Color::new(0.08, 0.08, 0.09, 1.0));
        for (a, z, c) in zones {
            let xa = b.x + (a / vmax) as f32 * b.w;
            let xb = b.x + (z / vmax).min(1.0) as f32 * b.w;
            draw_rectangle(xa, b.y + 3.0, xb - xa, 4.0, *c);
        }
        let xv = b.x + (v / vmax).clamp(0.0, 1.0) as f32 * b.w;
        draw_triangle(vec2(xv, b.y + 1.0), vec2(xv - 5.0, b.y - 6.0), vec2(xv + 5.0, b.y - 6.0), WHITE);
        txt_r(&format!("{v:.0}{unit}"), b.x + b.w, b.y - 2.0, 10.0, WHITE);
    };
    let ok = Color::new(0.2, 0.75, 0.35, 0.9);
    bar(y + 6.0, "q", ac.d.q / 1000.0, 120.0, &[(25.0, 90.0, ok), (90.0, 100.0, AMBER), (100.0, 120.0, RED)], " kPa");
    let skin = ac.skin_temp - 273.15;
    bar(y + 26.0, "PEAU", skin, SKIN_LIMIT_C * 1.15, &[(0.0, SKIN_LIMIT_C - 100.0, ok), (SKIN_LIMIT_C - 100.0, SKIN_LIMIT_C, AMBER), (SKIN_LIMIT_C, SKIN_LIMIT_C * 1.15, RED)], " C");
}

// ---------- Énergie / autonomie ----------

/// Bilan : autonomie JP-7 et H2, distance franchissable, carburant estimé à destination.
pub fn energy(ac: &Aircraft, r: Rect, dest: usize) {
    panel_box(r);
    let x = r.x + 8.0;
    let xr = r.x + r.w - 8.0;
    let mut y = r.y + 15.0;
    let gs = (ac.tas * ac.gamma.cos()).max(1.0);
    let line = |y: &mut f32, l: &str, v: String, c: Color| {
        txt(l, x, *y, 11.0, DIM);
        txt_r(&v, xr, *y, 11.0, c);
        *y += 14.0;
    };
    let ff = ac.eng.fuel_flow;
    let endu = if ff > 0.05 { ac.fuel / ff } else { f64::INFINITY };
    line(&mut y, "AUTONOMIE JP7", if endu.is_finite() { crate::mission::fmt_time(endu) } else { "--".into() }, if endu < 600.0 { AMBER } else { WHITE });
    let h2e = if ac.eng.h2_flow > 0.01 { ac.h2 / ac.eng.h2_flow } else { f64::INFINITY };
    line(&mut y, "AUTONOMIE H2", if h2e.is_finite() { crate::mission::fmt_time(h2e) } else { "--".into() }, if h2e < 300.0 { AMBER } else { CYAN_S });
    // Rayon d'action sur la ressource qui s'épuise en premier (H2 seul en scram)
    let lim = endu.min(h2e);
    line(&mut y, "RAYON D'ACTION", if lim.is_finite() { format!("{:.0} NM", lim * gs / NM) } else { "--".into() }, WHITE);
    let apt = &AIRPORTS[dest];
    let (d, _) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    let at_dest = ac.fuel - ff * d / gs;
    line(&mut y, &format!("JP7 A {}", apt.icao), if ff > 0.05 { format!("{:.1} t", at_dest / 1000.0) } else { "--".into() }, if at_dest < FUEL_MAX * 0.08 { RED } else if at_dest < FUEL_MAX * 0.15 { AMBER } else { GREEN_S });
}

// ---------- Indexeur d'incidence ----------

/// Échelle d'incidence verticale à droite du vecteur vitesse : limite de la loi en ambre.
pub fn aoa_index(ac: &Aircraft, fpv: Vec2) {
    let a = ac.d.alpha.to_degrees() as f32;
    let amax = (ac.fcs.alpha_max as f32).max(5.0);
    let (x, h) = (fpv.x + 62.0, 44.0);
    let y0 = fpv.y + h / 2.0;
    let fy = |v: f32| y0 - (v / amax).clamp(-0.2, 1.1) * h;
    draw_line(x, fy(0.0), x, fy(amax), 1.5, Color::new(1.0, 1.0, 1.0, 0.5));
    draw_line(x - 4.0, fy(amax), x + 4.0, fy(amax), 2.0, AMBER);
    draw_line(x - 3.0, fy(amax * 0.8), x + 3.0, fy(amax * 0.8), 1.0, Color::new(1.0, 1.0, 1.0, 0.5));
    let c = if a > amax * 0.92 { AMBER } else { GREEN_S };
    let yv = fy(a);
    draw_triangle(vec2(x - 1.0, yv), vec2(x - 8.0, yv - 4.0), vec2(x - 8.0, yv + 4.0), c);
    txt(&format!("{a:.1}"), x + 6.0, yv + 4.0, 11.0, c);
}
