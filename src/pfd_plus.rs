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

/// Horizon : cadres à angles nets (présentation d'afficheur de bord, pas d'arrondis).
fn rrect(r: Rect, _rad: f32, c: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, c);
}

fn rrect_lines(r: Rect, _rad: f32, th: f32, c: Color) {
    draw_rectangle_lines(r.x, r.y, r.w, r.h, th, c);
}

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

// ---------- FMA ----------

thread_local! {
    static LNAV: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static ILS: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Panneau IFR de l'horizon : phase (DEPART / ARRIVEE / APPROCHE), titre (procédure, piste),
/// lignes (libellé, valeur, couleur : 0 blanc, 1 vert, 2 ambre, 3 cyan, 4 magenta).
#[derive(Clone, Debug)]
pub struct IfrPanel {
    pub phase: &'static str,
    pub title: String,
    pub rows: Vec<(String, String, u8)>,
}

thread_local! {
    static IFR: std::cell::RefCell<Option<IfrPanel>> = const { std::cell::RefCell::new(None) };
}

pub fn set_ifr(p: Option<IfrPanel>) {
    IFR.with(|c| *c.borrow_mut() = p);
}

/// Hauteur du panneau IFR (0 si aucun).
pub fn ifr_height() -> f32 {
    IFR.with(|c| c.borrow().as_ref().map(|p| 30.0 + p.rows.len() as f32 * 15.0).unwrap_or(0.0))
}

/// Infos IFR intégrées à l'horizon (coin gauche, sans cadre) : symbologie HUD avec liseré
/// sombre, titre de phase souligné, libellés atténués et valeurs en couleur.
pub fn ifr_draw(x: f32, y: f32, w: f32, time: f64) {
    IFR.with(|c| {
        let c = c.borrow();
        let Some(p) = c.as_ref() else { return };
        let accent = match p.phase {
            "DEPART" => CYAN_S,
            "ARRIVEE" => GREEN_S,
            _ => MAGENTA_S,
        };
        let sh = |s: &str, x: f32, y: f32, size: f32, col: Color| {
            txt(s, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.8));
            txt(s, x, y, size, col);
        };
        sh(&format!("{}  {}", p.phase, p.title), x, y + 12.0, 12.0, accent);
        draw_line(x, y + 17.0, x + w * 0.85, y + 17.0, 1.0, Color::new(accent.r, accent.g, accent.b, 0.5));
        let blink = (time * 2.0).fract() < 0.65;
        for (k, (l, v, col)) in p.rows.iter().enumerate() {
            let yy = y + 32.0 + k as f32 * 15.0;
            let cc = match col {
                1 => GREEN_S,
                2 => AMBER,
                3 => CYAN_S,
                4 => MAGENTA_S,
                _ => Color::new(0.92, 0.94, 0.95, 1.0),
            };
            sh(l, x, yy, 9.0, Color::new(0.72, 0.76, 0.8, 0.95));
            if *col != 2 || blink {
                sh(v, x + 82.0, yy, 10.0, cc);
            }
        }
    });
}

/// ILS calé par le FMS (indicatif, fréquence, aéroport, piste) pour l'étiquette de l'horizon.
pub fn set_ils(t: Option<String>) {
    ILS.with(|c| *c.borrow_mut() = t);
}

pub fn ils_label() -> Option<String> {
    ILS.with(|c| c.borrow().clone())
}

/// Point actif du plan de vol FMS (pour l'annonce LNAV du bandeau de modes).
pub fn set_lnav(w: Option<String>) {
    LNAV.with(|c| *c.borrow_mut() = w);
}

fn lnav_wpt() -> Option<String> {
    LNAV.with(|c| c.borrow().clone())
}

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
                LatMode::Nav => match lnav_wpt() {
                    Some(w) => format!("LNAV {w}"),
                    None => format!("NAV {}", AIRPORTS[dest].icao),
                },
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
        } else if ac.ap.on && ac.ap.max_perf {
            ("AP1 MAX".into(), AMBER, if ac.ap.at_on { "A/THR".into() } else { String::new() })
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
    let blink = (time * 3.0).fract() < 0.65;
    if c < 0.0 {
        if blink {
            for k in 0..3 {
                let y = cy - 40.0 - k as f32 * 14.0;
                stroke(&[vec2(cx - 26.0, y + 10.0), vec2(cx, y), vec2(cx + 26.0, y + 10.0)], false, 3.0, RED);
            }
        }
        tag(cx, cy - 96.0, "TERRAIN", &format!("PULL UP   T-{t:.0} s"), RED, 18.0, false, blink);
    }
}

// ---------- Repères 3D ----------

thread_local! {
    static INNER: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// Zone libre du PFD (entre les échelles) : les flèches de repères hors champ s'y alignent.
pub fn set_inner(r: Rect) {
    INNER.with(|c| c.set(Some(r)));
}

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
            let inner = INNER.with(|c| c.get()).unwrap_or(s);
            let x = if side > 0.0 { inner.x + inner.w - 18.0 } else { inner.x + 18.0 };
            let _ = ctr;
            // Sous la ligne d'horizon : dégage les blocs météo et radar du haut
            let p = vec2(x, s.y + s.h * 0.60 + slot as f32 * 26.0);
            let dirv = vec2(side, 0.0);
            let n = vec2(0.0, 1.0);
            draw_triangle(p + dirv * 12.0, p - dirv * 6.0 + n * 8.0, p - dirv * 6.0 - n * 8.0, c.color);
            let lbl = format!("{} {:.0}NM {:+.0}", c.label, dist / NM, rel);
            let w = txt_w(&lbl, 11.0);
            txt(&lbl, if side > 0.0 { p.x - 14.0 - w } else { p.x + 14.0 }, p.y + 4.0, 11.0, c.color);
        }
    }
}

// ---------- Style HUD éphémère ----------

/// Couleur avec opacité globale de l'élément HUD.
fn ca(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// Texte HUD : liseré sombre pour rester lisible sur le ciel comme sur le relief.
fn htxt(s: &str, x: f32, y: f32, size: f32, c: Color, a: f32) {
    txt(s, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.7 * a));
    txt(s, x, y, size, ca(c, a));
}

fn htxt_r(s: &str, x: f32, y: f32, size: f32, c: Color, a: f32) {
    let w = txt_w(s, size);
    htxt(s, x - w, y, size, c, a);
}

/// Cadre HUD : seulement des coins en équerre.
/// Bloc de données : fond sombre opaque, filet gris fin (présentation d'afficheur de bord).
fn hud_frame(r: Rect, a: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.03, 0.04, 0.72 * a));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.55, 0.58, 0.62, 0.8 * a));
}

/// Visibilité (0..1) des éléments éphémères : vent, RWR, couloir, énergie.
pub struct HudVis {
    pub wind: f32,
    pub rwr: f32,
    pub corridor: f32,
    pub energy: f32,
}

#[derive(Default)]
struct HudMem {
    wind: (f64, f64),
    turb_bucket: i32,
    imc: bool,
    heard: usize,
    stage: i32,
    ram_ok: bool,
    scram_ok: bool,
    dest: usize,
    until: [f64; 4],
    all_until: f64,
    init: bool,
}

thread_local! {
    static HUD: std::cell::RefCell<HudMem> = std::cell::RefCell::new(HudMem::default());
}

/// Affiche tous les éléments éphémères pendant `secs` secondes (instruction HUD).
pub fn hud_show_all(secs: f64) {
    HUD.with(|h| h.borrow_mut().all_until = get_time() + secs);
}

pub fn hud_vis(ac: &Aircraft, wx: &crate::weather::WxInfo, threats: &[ThreatCue], dest: usize, s: Rect) -> HudVis {
    let now = get_time();
    HUD.with(|h| {
        let mut m = h.borrow_mut();
        let e = &ac.eng;
        let stage = if e.scram == ScramState::Lit { 2 } else if e.ram == RamState::Lit { 1 } else { 0 };
        let (ram_ok, scram_ok) = (ac.d.mach >= 2.0, ac.d.mach >= 4.8);
        let turb_bucket = if wx.turb > 0.6 { 3 } else if wx.turb > 0.3 { 2 } else if wx.turb > 0.15 { 1 } else { 0 };
        let imc = wx.imc > 0.3;
        if !m.init {
            *m = HudMem { wind: (wx.wind_dir, wx.wind_kt), turb_bucket, imc, heard: threats.len(), stage, ram_ok, scram_ok, dest, until: [now + 8.0; 4], all_until: 0.0, init: true };
        }
        let trig = |u: &mut [f64; 4], i: usize, secs: f64| u[i] = u[i].max(now + secs);
        // Vent : rotation > 15° ou variation > 8 kt, entrée dans un nuage, turbulence
        let (wd, ws) = m.wind;
        let wind_changed = wrap180(wx.wind_dir - wd).abs() > 15.0 || (wx.wind_kt - ws).abs() > 8.0;
        let turb_up = turb_bucket > m.turb_bucket;
        let imc_in = imc && !m.imc;
        if wind_changed || turb_up || imc_in {
            trig(&mut m.until, 0, 7.0);
        }
        if wind_changed {
            m.wind = (wx.wind_dir, wx.wind_kt);
        }
        m.turb_bucket = turb_bucket;
        m.imc = imc;
        // RWR : nouvel émetteur
        if threats.len() > m.heard {
            trig(&mut m.until, 1, 7.0);
        }
        m.heard = threats.len();
        // Couloir : changement de mode TBCC ou seuil de Mach franchi
        if stage != m.stage || ram_ok != m.ram_ok || scram_ok != m.scram_ok {
            trig(&mut m.until, 2, 8.0);
        }
        m.stage = stage;
        m.ram_ok = ram_ok;
        m.scram_ok = scram_ok;
        // Énergie : nouvelle destination
        if dest != m.dest {
            trig(&mut m.until, 3, 8.0);
        }
        m.dest = dest;
        // Conditions permanentes
        // Informations toujours affichées (plus seulement au survol de la souris)
        let _ = s;
        let all = true;
        let threat_hot = threats.iter().any(|t| t.state >= 3);
        let corridor_hot = ac.d.q > 90_000.0 || (stage > 0 && ac.d.q < 25_000.0) || ac.skin_temp - 273.15 > SKIN_LIMIT_C - 100.0;
        let ff = ac.eng.fuel_flow;
        let low_fuel = (ff > 0.05 && ac.fuel / ff < 600.0) || (ac.eng.h2_flow > 0.01 && ac.h2 / ac.eng.h2_flow < 300.0);
        let fade = |i: usize| ((m.until[i] - now) / 1.2).clamp(0.0, 1.0) as f32;
        let pick = |i: usize, hot: bool| if all || hot { 1.0 } else { fade(i) };
        HudVis {
            wind: pick(0, imc || turb_bucket >= 2),
            rwr: pick(1, threat_hot),
            corridor: pick(2, corridor_hot),
            energy: pick(3, low_fuel),
        }
    })
}

// ---------- Couloir hypersonique ----------

/// Couloir hypersonique : chaîne TBCC, pression dynamique dans le couloir 25-90 kPa, peau.
pub fn corridor(ac: &Aircraft, r: Rect, a: f32) {
    hud_frame(r, a);
    let x = r.x + 8.0;
    let w = r.w - 16.0;
    let e = &ac.eng;
    let stages = [
        ("TURB", e.side.iter().any(|s| *s == TurbState::Running) && e.doors < 0.5, 0.0, WHITE),
        ("RAM", e.ram == RamState::Lit, 2.0, CYAN_S),
        ("SCRAM", e.scram == ScramState::Lit, 4.8, MAGENTA_S),
    ];
    let sw = w / 3.0;
    for (i, (n, on, m, c)) in stages.iter().enumerate() {
        let cx = x + i as f32 * sw + sw / 2.0;
        let ready = ac.d.mach >= *m;
        let col = if *on { *c } else if ready { Color::new(c.r, c.g, c.b, 0.6) } else { Color::new(0.6, 0.6, 0.62, 0.5) };
        if *on {
            draw_line(cx - sw * 0.38, r.y + 22.0, cx + sw * 0.38, r.y + 22.0, 2.0, ca(*c, a));
        }
        htxt(n, cx - txt_w(n, 12.0) / 2.0, r.y + 17.0, 12.0, col, a);
        if i < 2 {
            htxt(">", x + (i + 1) as f32 * sw - 4.0, r.y + 17.0, 12.0, DIM, a);
        }
    }
    let next = if e.scram == ScramState::Lit {
        None
    } else if e.ram == RamState::Lit {
        Some(("SCRAM", 4.8))
    } else {
        Some(("RAM", 2.0))
    };
    let y = r.y + 40.0;
    match next {
        Some((n, m)) if ac.d.mach >= m => htxt(&format!("{n} DISPONIBLE"), x, y, 12.0, GREEN_S, a),
        Some((n, m)) => htxt(&format!("{n} a M{m:.1} (+{:.2})", m - ac.d.mach), x, y, 12.0, WHITE, a),
        None => htxt(&format!("SCRAM ETABLI  M{:.2}", ac.d.mach), x, y, 12.0, MAGENTA_S, a),
    }
    let bar = |y: f32, label: &str, v: f64, vmax: f64, zones: &[(f64, f64, Color)], unit: &str| {
        htxt(label, x, y + 9.0, 10.0, WHITE, a);
        let b = Rect::new(x + 34.0, y + 3.0, w - 34.0, 4.0);
        for (lo, hi, c) in zones {
            let xa = b.x + (lo / vmax) as f32 * b.w;
            let xb = b.x + (hi / vmax).min(1.0) as f32 * b.w;
            draw_rectangle(xa, b.y, xb - xa, b.h, ca(*c, a));
        }
        let xv = b.x + (v / vmax).clamp(0.0, 1.0) as f32 * b.w;
        draw_triangle(vec2(xv, b.y - 1.0), vec2(xv - 5.0, b.y - 8.0), vec2(xv + 5.0, b.y - 8.0), ca(WHITE, a));
        htxt_r(&format!("{:.0}{unit}", v.round() + 0.0), b.x + b.w, b.y - 4.0, 10.0, WHITE, a);
    };
    let ok = Color::new(0.2, 0.85, 0.35, 0.9);
    bar(y + 8.0, "q", ac.d.q / 1000.0, 120.0, &[(25.0, 90.0, ok), (90.0, 100.0, AMBER), (100.0, 120.0, RED)], " kPa");
    let skin = ac.skin_temp - 273.15;
    bar(y + 28.0, "PEAU", skin, SKIN_LIMIT_C * 1.15, &[(0.0, SKIN_LIMIT_C - 100.0, ok), (SKIN_LIMIT_C - 100.0, SKIN_LIMIT_C, AMBER), (SKIN_LIMIT_C, SKIN_LIMIT_C * 1.15, RED)], " C");
}

// ---------- Énergie / autonomie ----------

/// Bilan : autonomie JP-7 et H2, distance franchissable, carburant estimé à destination.
pub fn energy(ac: &Aircraft, r: Rect, dest: usize, a: f32) {
    hud_frame(r, a);
    let x = r.x + 8.0;
    let xr = r.x + r.w - 8.0;
    let mut y = r.y + 15.0;
    let gs = ac.ground_speed().max(1.0);
    let mut line = |l: &str, v: String, c: Color| {
        htxt(l, x, y, 11.0, WHITE, a * 0.85);
        htxt_r(&v, xr, y, 11.0, c, a);
        y += 14.0;
    };
    let ff = ac.eng.fuel_flow;
    let endu = if ff > 0.05 { ac.fuel / ff } else { f64::INFINITY };
    line("AUTONOMIE JP7", if endu.is_finite() { crate::mission::fmt_time(endu) } else { "--".into() }, if endu < 600.0 { AMBER } else { WHITE });
    let h2e = if ac.eng.h2_flow > 0.01 { ac.h2 / ac.eng.h2_flow } else { f64::INFINITY };
    line("AUTONOMIE H2", if h2e.is_finite() { crate::mission::fmt_time(h2e) } else { "--".into() }, if h2e < 300.0 { AMBER } else { CYAN_S });
    let lim = endu.min(h2e);
    line("RAYON D'ACTION", if lim.is_finite() { format!("{:.0} NM", lim * gs / NM) } else { "--".into() }, WHITE);
    let apt = &AIRPORTS[dest];
    let (d, _) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    let at_dest = ac.fuel - ff * d / gs;
    line(&format!("JP7 A {}", apt.icao), if ff > 0.05 { format!("{:.1} t", at_dest / 1000.0) } else { "--".into() }, if at_dest < FUEL_MAX * 0.08 { RED } else if at_dest < FUEL_MAX * 0.15 { AMBER } else { GREEN_S });
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

// ---------- Vent, météo ----------

/// Vent : flèche (sens vers lequel souffle le vent, relatif au nez), direction / force, dérive, vitesse
/// sol ; annonces IMC, turbulence, orage devant (affichées tant qu'elles sont actives).
pub fn wind_block(ac: &Aircraft, wx: &crate::weather::WxInfo, r: Rect, time: f64, a: f32) {
    if a > 0.01 {
        hud_frame(r, a);
        let c = vec2(r.x + 26.0, r.y + r.h / 2.0);
        let rel = (wrap180(wx.wind_dir + 180.0 - ac.hdg_deg()) as f32).to_radians();
        let d = vec2(rel.sin(), -rel.cos());
        let n = vec2(-d.y, d.x);
        draw_circle_lines(c.x, c.y, 18.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.35 * a));
        round_line(c - d * 14.0, c + d * 12.0, 2.5, ca(CYAN_S, a));
        draw_triangle(c + d * 16.0, c + d * 6.0 + n * 6.0, c + d * 6.0 - n * 6.0, ca(CYAN_S, a));
        let x = r.x + 52.0;
        htxt(&format!("{:03.0}/{:.0}", wx.wind_dir, wx.wind_kt), x, r.y + 16.0, 14.0, CYAN_S, a);
        let drift = wrap180(ac.track_deg() - ac.hdg_deg());
        htxt(&format!("DERIVE {}{:.0}", if drift >= 0.0 { "D" } else { "G" }, drift.abs()), x, r.y + 31.0, 11.0, WHITE, a);
        htxt(&format!("GS {:.0} kt", ac.ground_speed() / KT), x, r.y + 45.0, 11.0, WHITE, a);
        // Source des données et température extérieure (sans légende décorative)
        let real = wx.source.starts_with("REEL");
        htxt(if real { "WX REEL" } else { "WX SIMULE" }, r.x + 6.0, r.y + r.h - 7.0, 10.0, if real { GREEN_S } else { AMBER }, a);
        if let Some(t) = wx.oat {
            htxt_r(&format!("OAT {t:.0}C"), r.x + r.w - 6.0, r.y + r.h - 7.0, 10.0, WHITE, a);
        }
    }
    // Annonces météo actives
    let mut y = r.y + r.h + 16.0;
    let blink = (time * 2.0).fract() < 0.6;
    let mut x = r.x;
    if wx.imc > 0.3 {
        htxt("IMC", x, y, 13.0, WHITE, 1.0);
        x += 44.0;
    }
    if wx.turb > 0.15 {
        let (lbl, c) = if wx.turb > 0.6 { ("TURB SEV", RED) } else if wx.turb > 0.3 { ("TURB MOD", AMBER) } else { ("TURB LGT", WHITE) };
        if blink || wx.turb < 0.6 {
            htxt(lbl, x, y, 13.0, c, 1.0);
        }
    }
    y += 18.0;
    // Danger météo le plus fort sur la trajectoire (toujours affiché)
    if let Some((cause, dist, lvl, alt_ft)) = wx.ahead {
        let c = crate::wx3d::danger_color(lvl, 1.0);
        if blink || lvl < 0.8 {
            htxt(&format!("{cause} {} {:.0}NM FL{:.0}", crate::wx3d::danger_label(lvl), dist / NM, alt_ft / 100.0), r.x, y, 12.0, c, 1.0);
        }
        y += 16.0;
    }
    if let Some((brg, dist, top)) = wx.storm {
        if dist < 60.0 * NM || a > 0.01 {
            let clock = ((wrap360(brg - ac.hdg_deg()) / 30.0).round() as i32 + 11) % 12 + 1;
            let c = if dist < 30.0 * NM { RED } else { AMBER };
            htxt(&format!("ORAGE {clock}H {:.0}NM FL{:.0}", dist / NM, top / 100.0), r.x, y, 12.0, c, 1.0);
        }
    }
}

// ---------- Menaces : RWR et repères 3D ----------

pub struct ThreatCue {
    pub site: usize,
    pub lat: f64,
    pub lon: f64,
    pub name: &'static str,
    pub sam: bool,
    /// 1 veille, 2 poursuite, 3 accrochage (LOCK), 4 tir (LAUNCH).
    pub state: u8,
    pub dist: f64,
    /// Missile en vol : (lat, lon, altitude, temps avant impact).
    pub missile: Option<(f64, f64, f64, f64)>,
    pub escape: String,
}

fn threat_color(state: u8) -> Color {
    match state {
        4 | 3 => RED,
        2 => AMBER,
        _ => Color::new(1.0, 0.85, 0.3, 0.9),
    }
}

/// Écran d'alerte radar (RWR) : émetteurs au relèvement relatif, distance radiale.
pub fn rwr(ac: &Aircraft, cues: &[ThreatCue], r: Rect, time: f64, a: f32) {
    if a < 0.01 {
        return;
    }
    hud_frame(r, a);
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0 + 4.0);
    let rad = r.w.min(r.h - 10.0) / 2.0 - 6.0;
    htxt("RWR", r.x + 5.0, r.y + 12.0, 10.0, WHITE, a);
    for k in [0.5f32, 1.0] {
        draw_circle_lines(c.x, c.y, rad * k, 1.0, Color::new(0.4, 0.9, 0.5, 0.35 * a));
    }
    draw_line(c.x, c.y - rad, c.x, c.y - rad + 5.0, 1.5, Color::new(0.4, 0.9, 0.5, 0.6 * a));
    let blink = (time * 4.0).fract() < 0.55;
    for t in cues {
        let (_, brg) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let rel = (wrap180(brg - ac.hdg_deg()) as f32).to_radians();
        // Distance logarithmique : 20 km au centre, 400 km au bord
        let k = (((t.dist / 20_000.0).max(1.0).ln() / (20.0f64).ln()) as f32).clamp(0.15, 1.0);
        let p = c + vec2(rel.sin(), -rel.cos()) * rad * k;
        let col = threat_color(t.state);
        if t.state == 4 && !blink {
            continue;
        }
        htxt(if t.sam { "S" } else { "R" }, p.x - 4.0, p.y + 4.0, 12.0, col, a);
        if t.state >= 3 {
            stroke(&[p + vec2(0.0, -9.0), p + vec2(9.0, 0.0), p + vec2(0.0, 9.0), p + vec2(-9.0, 0.0)], true, 1.5, ca(col, a));
        }
    }
    draw_triangle(c + vec2(0.0, -5.0), c + vec2(-4.0, 4.0), c + vec2(4.0, 4.0), ca(WHITE, a));
}

/// Repères 3D des menaces dans l'horizon, missile en vol et bandeau d'esquive.
pub fn threats(ac: &Aircraft, v: &View, cues: &[ThreatCue], time: f64) {
    let eye = EyeFrame::new(ac);
    let s = v.rect;
    let blink = (time * 3.0).fract() < 0.6;
    for t in cues {
        let col = threat_color(t.state);
        let h = (ac.alt).max(1.0);
        let horizon = (2.0 * R_EARTH * h).sqrt();
        let (_, brg) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let p = if t.dist > horizon * 0.95 {
            let dip = (R_EARTH / (R_EARTH + h)).acos();
            v.project_dir(crate::svs::dir(brg.to_radians(), -dip))
        } else {
            v.project(eye.local(t.lat, t.lon, 0.0))
        };
        if let Some(p) = p.filter(|p| s.contains(*p)) {
            // Symbole : losange (radar) ou cercle cranté (SAM), accrochage = double cadre
            if t.sam {
                draw_circle_lines(p.x, p.y, 9.0, 2.0, col);
                for a in 0..4 {
                    let an = a as f32 * std::f32::consts::FRAC_PI_2;
                    let d = vec2(an.cos(), an.sin());
                    draw_line(p.x + d.x * 9.0, p.y + d.y * 9.0, p.x + d.x * 14.0, p.y + d.y * 14.0, 2.0, col);
                }
            } else {
                stroke(&[p + vec2(0.0, -8.0), p + vec2(8.0, 0.0), p + vec2(0.0, 8.0), p + vec2(-8.0, 0.0)], true, 1.5, col);
            }
            if t.state >= 3 && (blink || t.state == 3) {
                draw_rectangle_lines(p.x - 17.0, p.y - 17.0, 34.0, 34.0, 2.0, col);
            }
            let st = ["", "VEILLE", "POURSUITE", "LOCK", "LAUNCH"][t.state.min(4) as usize];
            let lbl = format!("{} {st} {:.0}NM", t.name, t.dist / NM);
            let w = txt_w(&lbl, 11.0) + 8.0;
            rrect(Rect::new(p.x - w / 2.0, p.y + 18.0, w, 15.0), 4.0, PANEL_BG);
            txt_c(&lbl, p.x, p.y + 29.0, 11.0, col);
        }
        // Missile en vol : position simulée, traînée vers le site de lancement
        if let Some((mla, mlo, malt, _)) = t.missile {
            if let Some(mp) = v.project(eye.local(mla, mlo, malt)) {
                let (d, b) = dist_brg(mla, mlo, t.lat, t.lon);
                let (tla, tlo) = destination(mla, mlo, b, (d * 0.08).min(3_000.0));
                if let Some(tp) = v.project(eye.local(tla, tlo, malt * 0.97)) {
                    draw_line(tp.x, tp.y, mp.x, mp.y, 3.0, Color::new(0.85, 0.85, 0.85, 0.5));
                }
                draw_circle(mp.x, mp.y, 7.0, Color::new(1.0, 0.6, 0.1, 0.5));
                draw_circle(mp.x, mp.y, 3.0, WHITE);
            }
        }
    }
    // Bandeau de la menace la plus grave
    if let Some(t) = cues.iter().filter(|t| t.state >= 3).max_by_key(|t| t.state) {
        let (cx, y) = (s.x + s.w / 2.0, s.y + s.h * 0.43);
        let msg = match t.missile {
            Some((_, _, _, tti)) => format!("MISSILE LAUNCH  {}  IMPACT {:.0}s", t.name, tti),
            None => format!("SAM LOCK  {}", t.name),
        };
        let w = txt_w(&msg, 18.0) + 24.0;
        if t.state == 3 || blink {
            rrect(Rect::new(cx - w / 2.0, y - 20.0, w, 26.0), 5.0, Color::new(0.35, 0.0, 0.0, 0.85));
            rrect_lines(Rect::new(cx - w / 2.0, y - 20.0, w, 26.0), 5.0, 1.5, RED);
            txt_c(&msg, cx, y, 18.0, RED);
        }
        let esc = format!("ESQUIVE : {}", t.escape);
        let w2 = txt_w(&esc, 13.0) + 16.0;
        rrect(Rect::new(cx - w2 / 2.0, y + 10.0, w2, 18.0), 4.0, PANEL_BG);
        txt_c(&esc, cx, y + 24.0, 13.0, AMBER);
    }
}

// ---------- Alertes carburant (bingo) ----------

/// Bandeau carburant façon HUD : détail pendant 12 s après chaque changement de niveau (ou au survol),
/// puis annonciateur compact permanent à partir du bingo. Repère de cap retour sur l'échelle de cap.
pub fn fuel_alert(ac: &Aircraft, p: &crate::fuel::Plan, s: Rect, time: f64, since_change: f64, reveal: bool) {
    use crate::fuel::Level as F;
    if p.level == F::Ok && !p.h2_bingo {
        return;
    }
    let col = match p.level {
        F::Joker => CYAN_S,
        F::Bingo => AMBER,
        F::MinFuel | F::Emergency => RED,
        F::Ok => AMBER,
    };
    let blink = (time * 2.5).fract() < 0.6;
    let cx = s.x + s.w / 2.0;
    let detail = since_change < 12.0 || reveal;
    let title = if p.level == F::Ok { "H2 BINGO" } else { p.level.name() };
    let flashing = matches!(p.level, F::MinFuel | F::Emergency | F::Bingo) && since_change < 6.0;
    let y = s.y + s.h * 0.30;
    if detail {
        tag(cx, y, "CARBURANT", &format!("{title}   JP-7 {:.1} t   BINGO {:.1} t{}", ac.fuel / 1000.0, p.bingo / 1000.0, if p.manual { " (MAN)" } else { "" }), col, 16.0, false, !flashing || blink);
        let (icao, brg, dist, eta, need) = p.rtb;
        let short = need + crate::fuel::RESERVE > ac.fuel;
        tag(cx, y + 28.0, "RTB", &format!("{icao}  CAP {brg:03.0}  {:.0} NM  {}  BESOIN {:.1} t", dist / NM, crate::mission::fmt_time(eta), need / 1000.0), if short { RED } else { GREEN_S }, 13.0, false, true);
        let mut yy = y + 52.0;
        if let Some((cs, d)) = p.tanker {
            tag(cx, yy, "AAR", &format!("RAVITAILLEUR {cs}  {:.0} NM", d / NM), CYAN_S, 13.0, false, true);
            yy += 24.0;
        }
        if p.h2_bingo {
            tag(cx, yy, "H2", "HYDROGENE < 3 MIN — DECELERER (DESCENTE)", AMBER, 13.0, false, true);
        }
    } else if p.level >= F::Bingo || p.h2_bingo {
        // Annonciateur compact permanent
        let lbl = if p.level >= F::Bingo { format!("{}  {:.1} t", p.level.name(), ac.fuel / 1000.0) } else { "H2 BINGO".to_string() };
        tag(cx, s.y + 70.0, "CARBURANT", &lbl, col, 14.0, false, p.level < F::MinFuel || blink);
    }
}
