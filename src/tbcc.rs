//! Vue en coupe technique du moteur combiné TBCC du Darkstar (une nacelle) :
//! - coque titane en dégradé, entrée d'air 2D à rampes de compression et lèvre de capot ;
//! - volet de répartition sur vérin (veine turbine / conduit stato-scram), animé avec les portes ;
//! - turbine : cône, étages de compresseur et de turbine (aubes qui scintillent avec le régime),
//!   chambre annulaire, accroche-flammes et post-combustion ;
//! - conduit stato / scram : isolateur avec train de chocs, gorge variable, mâts d'injection,
//!   flamme subsonique (stato) ou nappes de combustion supersonique (scram) ;
//! - tuyère commune à rampe de détente, panache et losanges de choc ;
//! - bandeau de valeurs et annotations techniques.
//! Chaque veine est un « tube » (bord haut / bord bas en fonction de l'abscisse) maillé en bandes
//! colorées : formes quelconques et dégradés sans artefact de remplissage.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;

thread_local! {
    /// Valeurs affichées lissées par moteur : gorge scram, post-combustion, flamme stato
    static VIS: std::cell::Cell<[[f32; 3]; 2]> = const { std::cell::Cell::new([[0.0; 3]; 2]) };
}

/// Interpolation linéaire par morceaux d'un profil (u croissant).
fn prof(pts: &[(f32, f32)], u: f32) -> f32 {
    if u <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        if u <= w[1].0 {
            let t = (u - w[0].0) / (w[1].0 - w[0].0).max(1e-3);
            // Raccord doux (smoothstep) entre points de contrôle
            let t = t * t * (3.0 - 2.0 * t);
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    pts[pts.len() - 1].1
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

/// Repère : coordonnées du dessin (u 0..1000, v 0..300) vers l'écran.
#[derive(Clone, Copy)]
struct M {
    ox: f32,
    oy: f32,
    s: f32,
}

impl M {
    fn p(&self, u: f32, v: f32) -> Vec2 {
        vec2(self.ox + u * self.s, self.oy + v * self.s)
    }
}

/// Tube maillé entre deux bords (fonctions de u), de u0 à u1, couleur par (u, travers 0..1).
fn band(m: M, u0: f32, u1: f32, top: &dyn Fn(f32) -> f32, bot: &dyn Fn(f32) -> f32, col: &dyn Fn(f32, f32) -> Color) {
    let nu = (((u1 - u0) / 12.0).ceil() as usize).clamp(2, 90);
    let nv = 4usize;
    let mut vertices = Vec::with_capacity((nu + 1) * (nv + 1));
    let mut indices: Vec<u16> = Vec::with_capacity(nu * nv * 6);
    for j in 0..=nu {
        let u = u0 + (u1 - u0) * j as f32 / nu as f32;
        let (a, b) = (top(u), bot(u));
        for k in 0..=nv {
            let t = k as f32 / nv as f32;
            let q = m.p(u, a + (b - a) * t);
            vertices.push(Vertex::new(q.x, q.y, 0.0, 0.0, 0.0, col(u, t)));
        }
    }
    for j in 0..nu {
        for k in 0..nv {
            let i0 = (j * (nv + 1) + k) as u16;
            let i1 = i0 + (nv + 1) as u16;
            indices.extend_from_slice(&[i0, i1, i0 + 1, i0 + 1, i1, i1 + 1]);
        }
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

/// Contour d'un profil (trait).
fn edge(m: M, u0: f32, u1: f32, f: &dyn Fn(f32) -> f32, th: f32, c: Color) {
    let n = (((u1 - u0) / 10.0).ceil() as usize).clamp(2, 100);
    let pts: Vec<Vec2> = (0..=n).map(|j| {
        let u = u0 + (u1 - u0) * j as f32 / n as f32;
        m.p(u, f(u))
    }).collect();
    for w in pts.windows(2) {
        draw_line(w[0].x, w[0].y, w[1].x, w[1].y, th, c);
    }
}

fn tri(m: M, a: (f32, f32), b: (f32, f32), c: (f32, f32), col: Color) {
    draw_triangle(m.p(a.0, a.1), m.p(b.0, b.1), m.p(c.0, c.1), col);
}

fn f_of(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

// ---------- Profils (u : avant -> arrière, v : haut -> bas) ----------
const SHELL_TOP: [(f32, f32); 9] = [(0.0, 120.0), (60.0, 102.0), (160.0, 80.0), (300.0, 62.0), (500.0, 54.0), (700.0, 54.0), (860.0, 64.0), (950.0, 88.0), (1000.0, 100.0)];
const SHELL_BOT: [(f32, f32); 7] = [(0.0, 130.0), (100.0, 150.0), (130.0, 240.0), (300.0, 248.0), (700.0, 252.0), (860.0, 250.0), (1000.0, 262.0)];
const RAMP: [(f32, f32); 4] = [(0.0, 130.0), (70.0, 136.0), (140.0, 146.0), (232.0, 152.0)];
const COWL: [(f32, f32); 2] = [(110.0, 234.0), (232.0, 228.0)];
const T_TOP: [(f32, f32); 8] = [(232.0, 152.0), (300.0, 104.0), (330.0, 94.0), (470.0, 100.0), (540.0, 98.0), (620.0, 100.0), (820.0, 104.0), (870.0, 110.0)];
const T_BOT: [(f32, f32); 8] = [(232.0, 176.0), (300.0, 168.0), (330.0, 160.0), (470.0, 150.0), (540.0, 152.0), (620.0, 150.0), (820.0, 146.0), (870.0, 142.0)];
const R_TOP: [(f32, f32); 6] = [(232.0, 180.0), (300.0, 184.0), (520.0, 186.0), (560.0, 180.0), (760.0, 176.0), (870.0, 168.0)];
const R_BOT: [(f32, f32); 6] = [(232.0, 228.0), (300.0, 224.0), (520.0, 218.0), (560.0, 224.0), (760.0, 230.0), (870.0, 238.0)];
const N_TOP: [(f32, f32); 2] = [(870.0, 110.0), (1000.0, 118.0)];
const N_BOT: [(f32, f32); 2] = [(870.0, 238.0), (1000.0, 256.0)];
const HUB_T: [(f32, f32); 4] = [(318.0, 127.0), (346.0, 117.0), (640.0, 119.0), (662.0, 126.0)];
const HUB_B: [(f32, f32); 4] = [(318.0, 127.0), (346.0, 137.0), (640.0, 135.0), (662.0, 128.0)];

/// Nacelle façon écran moteurs du Darkstar (Top Gun) : disposition et repères du film (FWD / AFT,
/// GEN, RPM, EGT, DOORS, CIT, repères orange, pastille de mode) sur le rendu en coupe réaliste.
pub fn draw_film(r: Rect, side: &str, ac: &Aircraft, i: usize, egt_off: f64, time: f64) {
    draw_ex(r, side, ac, i, egt_off, time, false, true);
}

/// Texte suivi d'un petit cercle « degré ».
fn deg(s: &str, x: f32, y: f32, size: f32, c: Color, unit: &str) {
    let w = txt_w(s, size);
    txt(s, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.6));
    txt(s, x, y, size, c);
    draw_circle_lines(x + w + size * 0.16, y - size * 0.66, size * 0.075, 1.2, c);
    txt(unit, x + w + size * 0.30, y, size, c);
}

#[allow(clippy::too_many_arguments)]
fn draw_ex(r: Rect, side: &str, ac: &Aircraft, i: usize, egt_off: f64, time: f64, both: bool, film: bool) {
    let e = &ac.eng;
    let running = e.side[i] == TurbState::Running;
    let starting = e.side[i] == TurbState::Starting;
    let n2 = e.n2s[i] as f32;
    let ram_lit = e.ram == RamState::Lit;
    let scram_lit = e.scram == ScramState::Lit;
    let unstart = e.scram == ScramState::Unstart;
    let d = e.doors as f32;
    // Valeurs animées lissées
    let dt = get_frame_time().min(0.1);
    let vis = VIS.with(|c| {
        let mut v = c.get();
        let tg = [if scram_lit || unstart { 1.0 } else { 0.0 }, if running { e.ab_frac as f32 } else { 0.0 }, if ram_lit { 1.0 } else if scram_lit { 0.0 } else { 0.0 }];
        for k in 0..3 {
            v[i][k] += (tg[k] - v[i][k]).clamp(-dt / 2.0, dt / 2.0);
        }
        c.set(v);
        v[i]
    });
    let (scr_v, ab_v, ram_v) = (vis[0], vis[1], vis[2]);

    // ---- Fond ----
    if film {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.115, 0.12, 0.125, 1.0));
    } else {
        rrect(r, 6.0, Color::new(0.05, 0.053, 0.058, 1.0));
    }
    // ---- Bandeau de valeurs ----
    let fs = (r.h * 0.05).clamp(9.0, 13.0);
    let rows: Vec<(usize, String, f64)> = if film { vec![] } else if both { vec![(0, "ENG L".into(), 8.0), (1, "ENG R".into(), -6.0)] } else { vec![(i, side.to_string(), egt_off)] };
    let line_h = fs + 8.0;
    let head = if film { 0.0 } else { line_h * rows.len() as f32 + 4.0 };
    for (row, (k, name, off)) in rows.iter().enumerate() {
        let k = *k;
        let state = match e.side[k] {
            TurbState::Running => ("RUN", GREEN_S),
            TurbState::Starting => ("START", AMBER),
            TurbState::Failed => ("FAIL", RED),
            TurbState::Off => ("OFF", DIM),
        };
        let gen_fail = ac.fails.has(if k == 0 { crate::systems::Fail::GenL } else { crate::systems::Fail::GenR });
        let gen_on = e.side[k] == TurbState::Running && e.n2s[k] > 0.55 && !gen_fail;
        let egt_k = e.egt + off;
        let n2k = e.n2s[k] as f32;
        let mode = if unstart { ("SCRAM UNSTART", RED) } else if scram_lit { ("SCRAM", MAGENTA_S) } else if ram_lit { ("STATO", CYAN_S) } else if e.side[k] == TurbState::Running { ("TURBINE", WHITE) } else { ("--", DIM) };
        let cit_c = (ac.d.oat + 273.15) * (1.0 + 0.2 * ac.d.mach * ac.d.mach) - 273.15;
        let items: Vec<(String, String, Color)> = vec![
            (name.clone(), state.0.to_string(), state.1),
            ("N2".into(), format!("{:.0}%", n2k * 104.0), if n2k * 104.0 > 102.0 { AMBER } else { WHITE }),
            ("EGT".into(), format!("{:.0}F", f_of(egt_k)), if egt_k > 1000.0 { RED } else if egt_k > 900.0 { AMBER } else { WHITE }),
            ("GEN".into(), if gen_fail { "FAIL".into() } else if gen_on { "ON".into() } else { "OFF".into() }, if gen_fail { RED } else if gen_on { GREEN_S } else { DIM }),
            ("MODE".into(), mode.0.to_string(), mode.1),
            ("POUSSEE".into(), format!("{:.0} kN", (e.t_side[k] + (e.t_ram + e.t_scram) * 0.5) / 1000.0), WHITE),
            ("VOLET".into(), format!("{:.0} deg", d * 90.0), if d > 0.01 && d < 0.99 { AMBER } else { WHITE }),
            ("CIT".into(), format!("{:.0}F", f_of(cit_c)), if cit_c > 430.0 { RED } else { WHITE }),
        ];
        let y = r.y + line_h * (row as f32 + 1.0) - 4.0;
        let mut x = r.x + 10.0;
        for (l, v, c) in &items {
            let w = txt_w(l, fs * 0.8) + txt_w(v, fs) + 14.0;
            if x + w > r.x + r.w - 6.0 {
                break;
            }
            txt(l, x, y, fs * 0.8, DIM);
            txt(v, x + txt_w(l, fs * 0.8) + 4.0, y, fs, *c);
            x += w + 8.0;
        }
    }
    if !film {
        draw_line(r.x + 8.0, r.y + head, r.x + r.w - 8.0, r.y + head, 1.0, Color::new(1.0, 1.0, 1.0, 0.06));
    }
    // ---- Repère du dessin ----
    let egt = e.egt + egt_off;
    let area = if film { Rect::new(r.x + 4.0, r.y + 2.0, r.w - 8.0, r.h - 4.0) } else { Rect::new(r.x + 6.0, r.y + head + 2.0, r.w - 12.0, r.h - head - 8.0) };
    let plume_len = 170.0;
    let s = (area.w / (1000.0 + plume_len)).min(area.h / 290.0);
    let m = M { ox: area.x + (area.w - (1000.0 + plume_len) * s) / 2.0, oy: area.y + (area.h - 290.0 * s) / 2.0 - 8.0 * s, s };
    let show_labels = !film && s * 1000.0 > 520.0;

    let pf = |p: &'static [(f32, f32)]| move |u: f32| prof(p, u);
    let shell_t = pf(&SHELL_TOP);
    let shell_b = pf(&SHELL_BOT);
    let ramp = pf(&RAMP);
    let cowl = pf(&COWL);
    let t_top = pf(&T_TOP);
    let t_bot = pf(&T_BOT);
    let r_top = pf(&R_TOP);
    let r_bot = pf(&R_BOT);
    let n_top = pf(&N_TOP);
    let n_bot = pf(&N_BOT);
    let hub_t = pf(&HUB_T);
    let hub_b = pf(&HUB_B);

    // ---- Coque titane (dégradé vertical, reflet supérieur) ----
    let metal = |_u: f32, t: f32| {
        let hi = Color::new(0.44, 0.46, 0.5, 1.0);
        let mid = Color::new(0.2, 0.21, 0.235, 1.0);
        let lo = Color::new(0.075, 0.08, 0.088, 1.0);
        if t < 0.25 { mix(hi, mid, t / 0.25) } else { mix(mid, lo, (t - 0.25) / 0.75) }
    };
    // Ombre portée douce sous la nacelle
    band(m, 60.0, 1000.0, &|u| shell_b(u) + 4.0, &|u| shell_b(u) + 26.0, &|_, t| Color::new(0.0, 0.0, 0.0, 0.35 * (1.0 - t)));
    band(m, 0.0, 1000.0, &shell_t, &shell_b, &metal);
    // Reflet spéculaire le long du dos et lignes de panneaux / rivets
    edge(m, 40.0, 960.0, &|u| shell_t(u) + 3.0, (1.4 * s).max(1.0), Color::new(1.0, 1.0, 1.0, 0.22));
    for u in [180.0f32, 320.0, 460.0, 600.0, 740.0, 880.0] {
        let a = m.p(u, shell_t(u) + 2.0);
        let b = m.p(u + 4.0, shell_t(u) + 14.0);
        draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.0, 0.0, 0.0, 0.25));
        for k in 0..3 {
            let q = m.p(u + 10.0, shell_t(u) + 5.0 + k as f32 * 4.0);
            draw_circle(q.x, q.y, (0.9 * s).max(0.6), Color::new(0.0, 0.0, 0.0, 0.3));
        }
    }
    // ---- Cavités (veines coupées) : intérieur sombre ----
    let cav = |_u: f32, t: f32| mix(Color::new(0.012, 0.014, 0.018, 1.0), Color::new(0.07, 0.075, 0.085, 1.0), ((t - 0.5).abs() * 2.0).powi(3));
    band(m, 110.0, 232.0, &ramp, &cowl, &cav);
    band(m, 232.0, 870.0, &t_top, &t_bot, &cav);
    band(m, 232.0, 870.0, &r_top, &r_bot, &cav);
    band(m, 870.0, 1000.0, &n_top, &n_bot, &cav);
    // Partie avant ouverte sous la rampe (air libre devant la lèvre)
    band(m, 0.0, 110.0, &ramp, &|u| prof(&[(0.0, 132.0), (110.0, 234.0)], u), &|_, _| Color::new(0.055, 0.058, 0.064, 1.0));
    // Arêtes de coupe (épaisseur des parois)
    let cut = Color::new(0.62, 0.66, 0.72, 0.55);
    edge(m, 0.0, 232.0, &ramp, 1.4, cut);
    edge(m, 110.0, 232.0, &cowl, 1.4, cut);
    edge(m, 232.0, 870.0, &t_top, 1.2, cut);
    edge(m, 232.0, 870.0, &t_bot, 1.2, cut);
    edge(m, 232.0, 870.0, &r_top, 1.2, cut);
    edge(m, 232.0, 870.0, &r_bot, 1.2, cut);
    edge(m, 870.0, 1000.0, &n_top, 1.2, cut);
    edge(m, 870.0, 1000.0, &n_bot, 1.2, cut);
    edge(m, 0.0, 1000.0, &shell_t, 1.3, Color::new(0.7, 0.73, 0.78, 0.8));
    edge(m, 100.0, 1000.0, &shell_b, 1.0, Color::new(0.25, 0.27, 0.3, 1.0));
    // Rampes de compression : chocs obliques externes (en vol rapide)
    if ac.d.mach > 1.2 {
        let a = ((1.0 / ac.d.mach).asin() as f32).min(1.2);
        for (u0, v0) in [(0.0f32, 130.0f32), (70.0, 136.0)] {
            let l = 150.0;
            let q0 = m.p(u0, v0);
            let q1 = q0 + vec2(a.cos(), a.sin()) * l * s;
            draw_line(q0.x, q0.y, q1.x, q1.y, 1.0, Color::new(1.0, 1.0, 1.0, 0.25));
        }
    }

    // ---- Turbine : moyeu, compresseur, chambre, turbine, post-combustion ----
    let hub_col = |_u: f32, t: f32| mix(Color::new(0.7, 0.72, 0.75, 1.0), Color::new(0.28, 0.3, 0.33, 1.0), t);
    band(m, 318.0, 662.0, &hub_t, &hub_b, &hub_col);
    edge(m, 346.0, 640.0, &|u| hub_t(u) + 2.0, (1.4 * s).max(0.8), Color::new(1.0, 1.0, 1.0, 0.45));
    tri(m, (296.0, 127.0), (320.0, 118.0), (320.0, 136.0), Color::new(0.6, 0.62, 0.66, 1.0));
    let spin = (time as f32 * (3.0 + 40.0 * n2)) % 1.0;
    let blade = |u: f32, hot: f32, k: usize| {
        let ph = ((spin + k as f32 * 0.17) % 1.0 - 0.5).abs() * 2.0;
        let base = mix(Color::new(0.5, 0.53, 0.58, 1.0), Color::new(0.95, 0.5, 0.18, 1.0), hot);
        let shine = if n2 > 0.05 { 0.45 * (1.0 - ph) } else { 0.0 };
        for (a, b, root_top) in [(t_top(u) + 1.0, hub_t(u) - 0.5, false), (hub_b(u) + 0.5, t_bot(u) - 1.0, true)] {
            // Trapèze : corde plus large en pied
            band(m, u - 5.0, u + 5.0, &|uu| a + (uu - u) * 0.15, &|_| b, &|_, t| {
                let root = if root_top { 1.0 - t } else { t };
                mix(mix(base, Color::new(0.2, 0.21, 0.23, 1.0), root * 0.5), WHITE, shine * (1.0 - (t - 0.4).abs()))
            });
            let q0 = m.p(u + 5.0, a);
            let q1 = m.p(u + 5.0, b);
            draw_line(q0.x, q0.y, q1.x, q1.y, 1.0, Color::new(0.0, 0.0, 0.0, 0.45));
        }
    };
    for k in 0..7 {
        blade(352.0 + k as f32 * 17.0, 0.0, k);
    }
    // Chambre annulaire (chemises) + flamme
    let comb = running || starting;
    for (a, b) in [(t_top(480.0) + 4.0, 112.0), (138.0, t_bot(480.0) - 4.0)] {
        let rc = Rect::new(m.p(474.0, 0.0).x, m.p(0.0, a.min(b)).y, 60.0 * s, (a - b).abs() * s);
        rrect(rc, 3.0 * s, Color::new(0.35, 0.33, 0.32, 1.0));
        if comb {
            let fl = 0.75 + 0.25 * ((time * 31.0 + i as f64 * 2.0).sin() as f32);
            draw_circle(rc.x + rc.w * 0.6, rc.y + rc.h / 2.0, rc.w * 0.55, Color::new(1.0, 0.5, 0.1, 0.10 * n2.max(0.3)));
            rrect(Rect::new(rc.x + 6.0 * s, rc.y + 1.5 * s, rc.w - 10.0 * s, rc.h - 3.0 * s), 2.0 * s, Color::new(1.0, 0.55 * fl, 0.15, 0.85 * n2.max(0.3)));
            rrect(Rect::new(rc.x + 14.0 * s, rc.y + rc.h * 0.35, rc.w - 26.0 * s, rc.h * 0.3), 1.5 * s, Color::new(1.0, 0.9, 0.6, 0.7 * n2.max(0.3)));
        }
    }
    let egt_k = ((egt - 300.0) / 700.0).clamp(0.0, 1.0) as f32;
    for k in 0..4 {
        blade(548.0 + k as f32 * 18.0, egt_k * (1.0 - k as f32 * 0.15), k + 7);
    }
    // Accroche-flammes (V) et post-combustion
    for v in [110.0f32, 126.0, 140.0] {
        let c = Color::new(0.6, 0.62, 0.66, 1.0);
        let (a, b, cc) = (m.p(668.0, v - 4.0), m.p(660.0, v), m.p(668.0, v + 4.0));
        draw_line(a.x, a.y, b.x, b.y, 1.4, c);
        draw_line(b.x, b.y, cc.x, cc.y, 1.4, c);
    }
    if ab_v > 0.02 {
        let fl = 0.9 + 0.1 * ((time * 23.0).sin() as f32);
        band(m, 672.0, 870.0, &|u| t_top(u) + 4.0, &|u| t_bot(u) - 4.0, &|u, t| {
            let k = (u - 672.0) / 198.0;
            let core = 1.0 - (t - 0.5).abs() * 2.0;
            Color::new(1.0, 0.45 + 0.4 * core, 0.1 + 0.3 * core * k, ab_v * fl * (0.25 + 0.6 * core) * (0.5 + 0.5 * k))
        });
    }
    // Tuyère turbine (pétales)
    let pet = Color::new(0.5, 0.52, 0.56, 1.0);
    round_line(m.p(820.0, 104.0), m.p(868.0, 112.0), (3.0 * s).max(1.5), pet);
    round_line(m.p(820.0, 146.0), m.p(868.0, 140.0), (3.0 * s).max(1.5), pet);

    // ---- Conduit stato / scram ----
    // Gorge variable (se resserre en scram)
    let th = 6.0 + 16.0 * scr_v;
    let wedge = Color::new(0.5, 0.52, 0.56, 1.0);
    tri(m, (320.0, r_top(320.0)), (420.0, r_top(420.0) + th), (520.0, r_top(520.0)), wedge);
    tri(m, (320.0, r_bot(320.0)), (420.0, r_bot(420.0) - th * 0.6), (520.0, r_bot(520.0)), wedge);
    // Train de chocs dans l'isolateur
    if ram_lit || scram_lit || unstart {
        let n = if scram_lit { 7 } else { 4 };
        let jit = (time * 6.0).sin() as f32 * 3.0;
        let mut up = true;
        let mut prev = m.p(300.0, (r_top(300.0) + r_bot(300.0)) / 2.0);
        for k in 1..=n {
            let u = 300.0 + k as f32 * (220.0 / n as f32) + jit;
            let v = if up { r_top(u) + th * 0.9 + 2.0 } else { r_bot(u) - th * 0.5 - 2.0 };
            let q = m.p(u, v);
            draw_line(prev.x, prev.y, q.x, q.y, 1.2, Color::new(0.85, 0.92, 1.0, if unstart { 0.9 } else { 0.5 }));
            prev = q;
            up = !up;
        }
    }
    // Mâts d'injection
    for v in [192.0f32, 202.0, 212.0] {
        tri(m, (540.0, v - 3.0), (566.0, v), (540.0, v + 3.0), Color::new(0.72, 0.74, 0.78, 1.0));
    }
    // Combustion
    if ram_v > 0.02 || scr_v > 0.02 {
        let fl = 0.85 + 0.15 * ((time * 19.0 + i as f64).sin() as f32);
        if ram_v > 0.02 {
            // Stato : flamme subsonique épaisse, jaune-orangé
            band(m, 566.0, 870.0, &|u| r_top(u) + 3.0, &|u| r_bot(u) - 3.0, &|u, t| {
                let k = (u - 566.0) / 304.0;
                let core = 1.0 - (t - 0.5).abs() * 2.0;
                Color::new(1.0, 0.5 + 0.35 * core, 0.12 + 0.2 * core, ram_v * fl * (0.35 + 0.55 * core) * (0.4 + 0.6 * k.min(0.8)))
            });
        }
        if scr_v > 0.02 {
            // Scram : nappes fines bleu-violet issues des mâts, cœur clair
            for v in [192.0f32, 202.0, 212.0] {
                band(m, 566.0, 870.0, &|u| v - 1.5 - (u - 566.0) * 0.02, &|u| v + 1.5 + (u - 566.0) * 0.02, &|_u, t| {
                    let core = 1.0 - (t - 0.5).abs() * 2.0;
                    Color::new(0.55 + 0.4 * core, 0.45 + 0.45 * core, 1.0, scr_v * fl * (0.3 + 0.6 * core))
                });
            }
        }
    }

    // Parois du conduit chauffées par la combustion (rougeoiement)
    let wall_k = (ram_v.max(scr_v) * (0.4 + 0.6 * ((e.ram_wall.max(e.scram_wall) as f32 - 600.0) / 1200.0).clamp(0.0, 1.0))).clamp(0.0, 1.0);
    if wall_k > 0.02 {
        for (f, sgn) in [(&r_top as &dyn Fn(f32) -> f32, 1.0f32), (&r_bot as &dyn Fn(f32) -> f32, -1.0)] {
            band(m, 540.0, 870.0, &|u| f(u), &|u| f(u) + sgn * 4.0, &|u, t| {
                let k = ((u - 540.0) / 330.0 * std::f32::consts::PI).sin();
                Color::new(1.0, 0.35 + 0.3 * k, 0.1, wall_k * k * (1.0 - t) * 0.8)
            });
        }
    }
    // Tuyère éclairée par le jet
    {
        let (gc, ga) = if scr_v > 0.05 {
            (Color::new(0.55, 0.45, 1.0, 1.0), scr_v)
        } else if ram_v > 0.05 {
            (Color::new(1.0, 0.55, 0.2, 1.0), ram_v)
        } else if ab_v > 0.05 {
            (Color::new(1.0, 0.5, 0.15, 1.0), ab_v)
        } else if running {
            (Color::new(0.9, 0.45, 0.2, 1.0), 0.25)
        } else {
            (Color::new(0.0, 0.0, 0.0, 1.0), 0.0)
        };
        if ga > 0.0 {
            band(m, 870.0, 1000.0, &|u| n_top(u) + 1.0, &|u| n_bot(u) - 1.0, &|u, t| {
                let k = (u - 870.0) / 130.0;
                let core = 1.0 - (t - 0.5).abs() * 2.0;
                Color::new(gc.r, gc.g, gc.b, ga * (0.15 + 0.45 * core) * (0.4 + 0.6 * k))
            });
        }
        // Volets de la tuyère (rampe de détente)
        round_line(m.p(870.0, 110.0), m.p(1000.0, 118.0), (3.0 * s).max(1.5), Color::new(0.5, 0.52, 0.56, 1.0));
        round_line(m.p(870.0, 238.0), m.p(1000.0, 256.0), (3.0 * s).max(1.5), Color::new(0.5, 0.52, 0.56, 1.0));
    }

    // ---- Volet de répartition (vérin) ----
    let hinge = m.p(234.0, 178.0);
    let tip_turb = m.p(312.0, 223.0);
    let tip_ram = m.p(302.0, 104.0);
    let a0 = (tip_turb.y - hinge.y).atan2(tip_turb.x - hinge.x);
    let a1 = (tip_ram.y - hinge.y).atan2(tip_ram.x - hinge.x);
    let ang = a0 + (a1 - a0) * d;
    let len = (tip_turb - hinge).length();
    let tip = hinge + vec2(ang.cos(), ang.sin()) * len;
    let moving = d > 0.01 && d < 0.99;
    let fc = if moving { AMBER } else { Color::new(0.85, 0.87, 0.9, 1.0) };
    round_line(hinge, tip, (9.0 * s).max(4.0), Color::new(0.06, 0.06, 0.07, 1.0));
    round_line(hinge, tip, (6.0 * s).max(2.6), fc);
    let jack = m.p(176.0, 226.0);
    round_line(jack, hinge + (tip - hinge) * 0.42, (3.0 * s).max(1.3), Color::new(0.58, 0.6, 0.64, 1.0));
    draw_circle(hinge.x, hinge.y, (6.0 * s).max(2.5), Color::new(0.18, 0.18, 0.2, 1.0));
    draw_circle_lines(hinge.x, hinge.y, (6.0 * s).max(2.5), 1.2, fc);

    // ---- Écoulement d'air (filets animés sur la veine active) ----
    let to_turb = (1.0 - d) * if running || starting { 1.0 } else { 0.3 };
    let to_duct = d * if ram_lit || scram_lit { 1.0 } else { 0.3 };
    let speed = 0.35 + 0.3 * (ac.d.mach as f32).min(6.0) / 6.0;
    for k in 0..10 {
        let t = ((k as f64 / 10.0 + time * speed as f64) % 1.0) as f32;
        let a = (t * 4.0).min(1.0) * (1.0 - t).sqrt();
        // Entrée commune
        let ui = 40.0 + t * 190.0;
        let vi = (ramp(ui) + if ui > 110.0 { cowl(ui) } else { 232.0 }) / 2.0;
        let qa = m.p(ui, vi);
        draw_line(qa.x, qa.y, qa.x + 14.0 * s, qa.y, 1.2, Color::new(0.85, 0.92, 1.0, 0.45 * a));
        if to_turb > 0.05 {
            let u = 240.0 + t * 600.0;
            let v = (t_top(u) + t_bot(u)) / 2.0 + if (330.0..662.0).contains(&u) { -14.0 + 28.0 * ((k % 2) as f32) } else { 0.0 };
            let q = m.p(u, v);
            draw_line(q.x, q.y, q.x + 16.0 * s, q.y, 1.2, Color::new(0.85, 0.92, 1.0, 0.5 * a * to_turb));
        }
        if to_duct > 0.05 {
            let u = 240.0 + t * 620.0;
            let v = (r_top(u) + r_bot(u)) / 2.0 + (k as f32 % 3.0 - 1.0) * 8.0;
            let q = m.p(u, v);
            draw_line(q.x, q.y, q.x + 18.0 * s, q.y, 1.2, Color::new(0.85, 0.92, 1.0, 0.5 * a * to_duct));
        }
    }

    // ---- Panache et losanges de choc ----
    let (pc, pl): (Option<Color>, f32) = if scr_v > 0.05 {
        (Some(Color::new(0.6, 0.5, 1.0, 1.0)), 1.0)
    } else if ram_v > 0.05 {
        (Some(Color::new(1.0, 0.62, 0.2, 1.0)), 0.85)
    } else if ab_v > 0.05 {
        (Some(Color::new(1.0, 0.55, 0.15, 1.0)), 0.7 * ab_v)
    } else if running {
        (Some(Color::new(0.7, 0.72, 0.78, 1.0)), 0.25)
    } else {
        (None, 0.0)
    };
    if let Some(c) = pc {
        let fl = 0.92 + 0.08 * ((time * 21.0 + i as f64).sin() as f32);
        let l = plume_len * pl * fl;
        let (v0, v1) = (n_top(1000.0) + 6.0, n_bot(1000.0) - 4.0);
        let vc = (v0 + v1) / 2.0;
        band(m, 1000.0, 1000.0 + l, &|u| v0 + (vc - v0) * ((u - 1000.0) / l).powf(0.8), &|u| v1 - (v1 - vc) * ((u - 1000.0) / l).powf(0.8), &|u, t| {
            let k = (u - 1000.0) / l;
            let core = 1.0 - (t - 0.5).abs() * 2.0;
            Color::new(mix(c, WHITE, core * 0.5).r, mix(c, WHITE, core * 0.5).g, mix(c, WHITE, core * 0.5).b, (1.0 - k) * (0.25 + 0.6 * core) * pl.min(1.0))
        });
        if scr_v > 0.05 || ram_v > 0.05 || ab_v > 0.3 {
            for k in 0..4 {
                let u = 1000.0 + (24.0 + k as f32 * 34.0) * fl;
                if u > 1000.0 + l * 0.85 {
                    break;
                }
                let (dw, dh) = (12.0, (v1 - v0) * 0.28 * (1.0 - k as f32 * 0.2));
                let pts = [m.p(u - dw, vc), m.p(u, vc - dh), m.p(u + dw, vc), m.p(u, vc + dh)];
                stroke(&pts, true, 1.2, Color::new(1.0, 1.0, 1.0, 0.7 - k as f32 * 0.15));
            }
        }
    }

    // ---- Annotations techniques ----
    if show_labels {
        let lc = Color::new(0.62, 0.65, 0.7, 1.0);
        let lab = |u: f32, v_from: f32, v_to: f32, t: &str| {
            let a = m.p(u, v_from);
            let b = m.p(u, v_to);
            draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.62, 0.65, 0.7, 0.6));
            draw_circle(a.x, a.y, 1.8, lc);
            txt_c(t, b.x, b.y + if v_to < v_from { -3.0 } else { 10.0 }, 9.0, lc);
        };
        lab(70.0, 134.0, 92.0, "RAMPES");
        lab(400.0, 98.0, 30.0, "COMPRESSEUR");
        lab(505.0, 106.0, 44.0, "CHAMBRE");
        lab(575.0, 104.0, 30.0, "TURBINE");
        lab(760.0, 108.0, 40.0, "POST-COMB.");
        lab(270.0, 190.0, 278.0, "VOLET");
        lab(420.0, 214.0, 284.0, "ISOLATEUR");
        lab(650.0, 226.0, 278.0, if scram_lit { "SCRAM" } else { "STATO / SCRAM" });
        lab(935.0, 240.0, 284.0, "TUYERE");
    }
    if moving {
        let lbl = if e.inlet_open { "VOLET -> TURBINES" } else { "VOLET -> STATO / SCRAM" };
        txt(&format!("{lbl}  {:.0} %", d * 100.0), area.x + 4.0, area.y + area.h - 4.0, fs * 0.85, AMBER);
    } else if scr_v > 0.02 && scr_v < 0.98 {
        txt(&format!("GORGE SCRAM {:.0} %", scr_v * 100.0), area.x + 4.0, area.y + area.h - 4.0, fs * 0.85, MAGENTA_S);
    }
    // ---- Repères de l'écran moteurs du film ----
    if film {
        let ffs = (s * 30.0).clamp(10.0, 22.0);
        let label = Color::new(0.62, 0.64, 0.66, 1.0);
        let orange = Color::new(0.93, 0.55, 0.16, 1.0);
        let orange_dim = Color::new(0.42, 0.28, 0.15, 1.0);
        let st = match e.side[i] {
            TurbState::Off if e.fadec_fault[i] => "FADEC FAULT",
            TurbState::Off if e.sw[i] == EngSw::Motor => "MOTOR",
            TurbState::Off => "OFF",
            TurbState::Starting => "START",
            TurbState::Running => "RUN",
            TurbState::Failed => "FAIL",
        };
        let top = m.p(0.0, 16.0).y;
        txt("FWD", r.x + 8.0, top, ffs * 0.7, label);
        txt_r("AFT", r.x + r.w - 8.0, top, ffs * 0.7, label);
        txt_c(&format!("{side}  {st}"), r.x + r.w / 2.0, top, ffs * 0.6, label);
        // Repères orange : lèvres d'entrée, pétales de tuyère, post-combustion
        let inlet_on = running || ram_lit || scram_lit;
        let oc = if inlet_on { orange } else { orange_dim };
        round_line(m.p(4.0, 131.0), m.p(66.0, 136.0), (3.0 * s).max(1.8), oc);
        round_line(m.p(112.0, 236.0), m.p(178.0, 232.0), (3.0 * s).max(1.8), oc);
        let pc = if running { orange } else { orange_dim };
        let ab_l = 1.0 + 0.7 * ab_v * (0.85 + 0.15 * (time * 23.0).sin() as f32);
        round_line(m.p(820.0, 104.0), m.p(868.0, 112.0), (3.0 * s).max(1.8), pc);
        round_line(m.p(820.0, 146.0), m.p(868.0, 140.0), (3.0 * s).max(1.8), pc);
        round_line(m.p(1002.0, 120.0), m.p(1002.0 + 60.0 * ab_l, 128.0), (3.0 * s).max(1.8), pc);
        round_line(m.p(1002.0, 254.0), m.p(1002.0 + 60.0 * ab_l, 246.0), (3.0 * s).max(1.8), pc);
        // GEN (sur le carter du compresseur)
        let gen_fail = ac.fails.has(if i == 0 { crate::systems::Fail::GenL } else { crate::systems::Fail::GenR });
        let gen_on = running && n2 > 0.55 && !gen_fail;
        let (g0, g1) = (m.p(398.0, 104.0), m.p(508.0, 146.0));
        let gr = Rect::new(g0.x, g0.y, g1.x - g0.x, g1.y - g0.y);
        let (gt, gc, gb) = if gen_fail {
            ("GEN FAIL", RED, Color::new(0.35, 0.06, 0.05, 0.92))
        } else if gen_on {
            ("GEN ON", orange, Color::new(0.38, 0.22, 0.12, 0.92))
        } else {
            ("GEN OFF", Color::new(0.6, 0.61, 0.62, 1.0), Color::new(0.24, 0.25, 0.26, 0.92))
        };
        rrect(gr, 3.0, gb);
        rrect_lines(gr, 3.0, 1.5, gc);
        txt_c(gt, gr.x + gr.w / 2.0, gr.y + gr.h * 0.5 + ffs * 0.25, ffs * 0.62, gc);
        // RPM (grand), sur la turbine
        let rpm = n2 * 104.0;
        let rc = if rpm > 106.0 { RED } else if rpm > 102.0 { AMBER } else { WHITE };
        txt_c("RPM", m.p(590.0, 0.0).x, m.p(0.0, 106.0).y, ffs * 0.42, label);
        let rs = format!("{rpm:.0}%");
        txt_c(&rs, m.p(590.0, 0.0).x + 1.0, m.p(0.0, 142.0).y + 1.0, ffs * 1.05, Color::new(0.0, 0.0, 0.0, 0.7));
        txt_c(&rs, m.p(590.0, 0.0).x, m.p(0.0, 142.0).y, ffs * 1.05, rc);
        // EGT
        let ec = if egt > 1000.0 { RED } else if egt > 900.0 { AMBER } else { WHITE };
        deg(&format!("{:.0}", f_of(egt)), m.p(700.0, 0.0).x, m.p(0.0, 138.0).y, ffs * 0.95, ec, "F");
        // CIT (conduit stato / scram : air d'arrêt isolé par les portes)
        let cit_c = (ac.d.oat + 273.15) * (1.0 + 0.2 * ac.d.mach * ac.d.mach) - 273.15;
        let cit_show = ac.d.oat + (cit_c - ac.d.oat) * (1.0 - e.doors * 0.94);
        let cc = if cit_show > 430.0 { RED } else if cit_show > 400.0 { AMBER } else { WHITE };
        deg(&format!("{:.0}", f_of(cit_show)), m.p(400.0, 0.0).x, m.p(0.0, 212.0).y, ffs * 0.85, cc, "F");
        // DOORS
        let dcol = if ac.fails.has(crate::systems::Fail::InletJam) { RED } else if moving { AMBER } else { WHITE };
        txt("DOORS", m.p(6.0, 0.0).x, m.p(0.0, 182.0).y, ffs * 0.42, label);
        deg(&format!("{:02.0}", d * 90.0), m.p(8.0, 0.0).x, m.p(0.0, 222.0).y, ffs * 1.0, dcol, "");
        // Pastille de mode dans le conduit
        let pill = if scram_lit {
            Some((MAGENTA_S, format!("SCRAM  {:.0} kN", e.t_scram / 2000.0)))
        } else if ram_lit {
            Some((CYAN_S, format!("STATO  {:.0} kN", e.t_ram / 2000.0)))
        } else if unstart {
            Some((RED, "UNSTART".to_string()))
        } else {
            None
        };
        if let Some((c, l)) = pill {
            let lw = txt_w(&l, ffs * 0.55) + 14.0;
            let cx = m.p(760.0, 0.0).x;
            let br = Rect::new(cx - lw / 2.0, m.p(0.0, 192.0).y, lw, (22.0 * s).max(12.0));
            rrect(br, br.h * 0.5, Color::new(0.04, 0.05, 0.06, 0.8));
            rrect_lines(br, br.h * 0.5, 1.2, c);
            txt_c(&l, cx, br.y + br.h * 0.5 + ffs * 0.2, ffs * 0.55, WHITE);
        }
    }
}
