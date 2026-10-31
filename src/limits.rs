//! Page LIMITES : seuil d'explosion des chambres de combustion en fonction du Mach et seuil de
//! destruction des gouvernes (FCS) en fonction de la vitesse, avec le point de vol courant.

use crate::aircraft::*;
use crate::geo::KT;
use crate::systems::{self, Fail};
use crate::ui::*;
use macroquad::prelude::*;

struct Chart {
    r: Rect,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl Chart {
    fn p(&self, x: f64, y: f64) -> Vec2 {
        vec2(
            self.r.x + ((x - self.x0) / (self.x1 - self.x0)) as f32 * self.r.w,
            self.r.y + self.r.h - (((y - self.y0) / (self.y1 - self.y0)).clamp(-0.02, 1.02)) as f32 * self.r.h,
        )
    }

    fn frame(&self, xlabel: &str, ylabel: &str, xticks: &[f64], yticks: &[f64]) {
        draw_rectangle(self.r.x, self.r.y, self.r.w, self.r.h, Color::new(0.0, 0.0, 0.0, 0.35));
        for &x in xticks {
            let p = self.p(x, self.y0);
            draw_line(p.x, self.r.y, p.x, self.r.y + self.r.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.06));
            txt_c(&format!("{x:.0}"), p.x, self.r.y + self.r.h + 13.0, 11.0, DIM);
        }
        for &y in yticks {
            let p = self.p(self.x0, y);
            draw_line(self.r.x, p.y, self.r.x + self.r.w, p.y, 1.0, Color::new(1.0, 1.0, 1.0, 0.06));
            txt_r(&format!("{y:.0}"), self.r.x - 4.0, p.y + 4.0, 11.0, DIM);
        }
        draw_rectangle_lines(self.r.x, self.r.y, self.r.w, self.r.h, 1.0, PANEL_BORDER);
        txt_r(xlabel, self.r.x + self.r.w, self.r.y + self.r.h + 26.0, 11.0, DIM);
        txt(ylabel, self.r.x + 4.0, self.r.y + 12.0, 11.0, DIM);
    }

    fn curve(&self, xs: impl Iterator<Item = f64>, f: impl Fn(f64) -> f64, th: f32, c: Color, dashed: bool) {
        let mut prev: Option<Vec2> = None;
        for (i, x) in xs.enumerate() {
            let p = self.p(x, f(x));
            if let Some(q) = prev {
                if !dashed || i % 2 == 0 {
                    draw_line(q.x, q.y, p.x, p.y, th, c);
                }
            }
            prev = Some(p);
        }
    }

    /// Zone dangereuse au-dessus d'une courbe (hachures légères).
    fn shade_above(&self, xs: &[f64], f: impl Fn(f64) -> f64, c: Color) {
        for w in xs.windows(2) {
            let (a, b) = (self.p(w[0], f(w[0])), self.p(w[1], f(w[1])));
            let top = self.r.y;
            draw_triangle(a, b, vec2(b.x, top), c);
            draw_triangle(a, vec2(b.x, top), vec2(a.x, top), c);
        }
    }
}

fn range(a: f64, b: f64, step: f64) -> Vec<f64> {
    let mut v = Vec::new();
    let mut x = a;
    while x <= b + 1e-9 {
        v.push(x);
        x += step;
    }
    v
}

pub fn draw_limits(ac: &Aircraft, r: Rect) {
    panel(r, "LIMITES - CHAMBRES DE COMBUSTION / GOUVERNES");
    let oat_k = ac.d.oat + 273.15;
    let cool_fail = ac.fails.has(Fail::ScramCool);
    let h = (r.h - 60.0) / 2.0;

    // --- Chambres : manette maximale avant explosion, en fonction du Mach ---
    let c1 = Chart { r: Rect::new(r.x + 50.0, r.y + 50.0, r.w - 70.0, h - 50.0), x0: 0.0, x1: 12.0, y0: 0.0, y1: 200.0 };
    txt("SEUIL D'EXPLOSION DES CHAMBRES (manette % vs Mach)", r.x + 14.0, r.y + 38.0, 12.0, WHITE);
    c1.frame("MACH", "MANETTE %", &[0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0], &[0.0, 50.0, 100.0, 150.0, 200.0]);
    let ram_x = range(1.8, 7.0, 0.1);
    let scram_x = range(4.5, 12.0, 0.1);
    let ram_burst = |m: f64| systems::lever_for_wall(|l| systems::ram_wall(m, l, oat_k), systems::RAM_WALL_BURST) * 100.0;
    let ram_caut = |m: f64| systems::lever_for_wall(|l| systems::ram_wall(m, l, oat_k), systems::RAM_WALL_CAUTION) * 100.0;
    let scram_burst = |m: f64| systems::lever_for_wall(|l| systems::scram_wall(m, l, oat_k, cool_fail), systems::SCRAM_WALL_BURST) * 100.0;
    let scram_caut = |m: f64| systems::lever_for_wall(|l| systems::scram_wall(m, l, oat_k, cool_fail), systems::SCRAM_WALL_CAUTION) * 100.0;
    scissor(Some(c1.r));
    c1.shade_above(&scram_x, scram_burst, Color::new(1.0, 0.2, 0.9, 0.10));
    c1.shade_above(&ram_x, ram_burst, Color::new(0.2, 0.9, 1.0, 0.08));
    scissor(None);
    c1.curve(ram_x.iter().copied(), ram_caut, 1.2, Color::new(0.2, 0.9, 1.0, 0.6), true);
    c1.curve(ram_x.iter().copied(), ram_burst, 2.0, CYAN_S, false);
    c1.curve(scram_x.iter().copied(), scram_caut, 1.2, Color::new(1.0, 0.35, 1.0, 0.6), true);
    c1.curve(scram_x.iter().copied(), scram_burst, 2.0, MAGENTA_S, false);
    // Butée normale (100 %) et butée mécanique sous override (200 %)
    let (a, b) = (c1.p(0.0, 100.0), c1.p(12.0, 100.0));
    draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(0.3, 1.0, 0.4, 0.5));
    txt("100 % (FADEC)", a.x + 4.0, a.y - 4.0, 10.0, Color::new(0.3, 1.0, 0.4, 0.8));
    txt("STATO", c1.p(2.2, ram_burst(2.2)).x, c1.p(2.2, ram_burst(2.2).min(190.0)).y + 14.0, 11.0, CYAN_S);
    let lm = 10.5;
    txt("SCRAM", c1.p(lm, 0.0).x, c1.p(lm, scram_burst(lm).min(190.0)).y - 4.0, 11.0, MAGENTA_S);
    // Point courant
    let e = &ac.eng;
    let lit = if e.scram == ScramState::Lit { Some(("SCRAM", e.scram_wall, systems::SCRAM_WALL_BURST, MAGENTA_S)) } else if e.ram == RamState::Lit { Some(("STATO", e.ram_wall, systems::RAM_WALL_BURST, CYAN_S)) } else { None };
    let p = c1.p(ac.d.mach.min(12.0), (ac.throttle * 100.0).min(200.0));
    let pc = match lit {
        Some((_, w, burst, _)) if w > burst - 150.0 => RED,
        Some((_, _, _, c)) => c,
        None => WHITE,
    };
    draw_circle(p.x, p.y, 5.0, pc);
    draw_circle_lines(p.x, p.y, 8.0, 1.5, pc);
    let info = match lit {
        Some((n, w, burst, _)) => format!("{n} : paroi {w:.0} C / explosion {burst:.0} C  (marge {:.0} C)", burst - w),
        None => "aucune chambre stato/scram allumee".to_string(),
    };
    txt(&info, c1.r.x, c1.r.y + c1.r.h + 28.0, 12.0, if pc == RED { RED } else { DIM });
    if cool_fail {
        txt("PANNE REFROIDISSEMENT SCRAM : seuils abaisses", c1.r.x + c1.r.w - 300.0, c1.r.y + 14.0, 11.0, AMBER);
    }

    // --- Gouvernes : braquage maximal avant destruction, en fonction de la vitesse (KEAS) ---
    let y2 = r.y + h + 40.0;
    txt("SEUIL DE DESTRUCTION DES GOUVERNES (braquage vs vitesse)", r.x + 14.0, y2 - 2.0, 12.0, WHITE);
    let c2 = Chart { r: Rect::new(r.x + 50.0, y2 + 10.0, r.w - 70.0, h - 50.0), x0: 0.0, x1: 900.0, y0: 0.0, y1: 32.0 };
    c2.frame("KEAS", "BRAQUAGE deg", &[0.0, 150.0, 300.0, 450.0, 600.0, 750.0, 900.0], &[0.0, 10.0, 20.0, 30.0]);
    let q_of = |keas: f64| 0.5 * 1.225 * (keas * KT).powi(2) / 1000.0;
    let xs = range(60.0, 900.0, 10.0);
    scissor(Some(c2.r));
    c2.shade_above(&xs, |k| systems::defl_destroy(q_of(k)), Color::new(1.0, 0.15, 0.1, 0.12));
    scissor(None);
    c2.curve(xs.iter().copied(), |k| systems::defl_destroy(q_of(k)), 2.0, RED, false);
    c2.curve(xs.iter().copied(), |k| systems::defl_limit_normal(q_of(k)), 1.5, GREEN_S, true);
    let qlim_keas = ((2.0 * Q_LIMIT / 1.225).sqrt() / KT) as f64;
    let (a, b) = (c2.p(qlim_keas, 0.0), c2.p(qlim_keas, 32.0));
    draw_line(a.x, a.y, b.x, b.y, 1.0, Color::new(1.0, 0.6, 0.1, 0.6));
    txt("q 100 kPa", a.x + 3.0, b.y + 12.0, 10.0, AMBER);
    txt("DESTRUCTION", c2.p(640.0, 0.0).x, c2.p(640.0, systems::defl_destroy(q_of(640.0))).y - 6.0, 11.0, RED);
    txt("LIMITEUR LOI NORMALE", c2.p(250.0, 0.0).x, c2.p(250.0, 30.0).y + 14.0, 11.0, GREEN_S);
    let f = &ac.fcs;
    let defl = f.defl_cmd.max(f.elevon_l.abs().min(30.0) * 0.0);
    let prot_off = ac.ovr_fbw || ac.fails.has(Fail::FcsDirect);
    let destroy_now = systems::defl_destroy(ac.d.q / 1000.0);
    let pc = if defl > destroy_now * 0.85 { RED } else if f.defl_limited { AMBER } else { WHITE };
    let p = c2.p(ac.d.keas.min(900.0), defl);
    draw_circle(p.x, p.y, 5.0, pc);
    draw_circle_lines(p.x, p.y, 8.0, 1.5, pc);
    let law = if prot_off { "LOI DIRECTE : pas de limiteur, rupture possible" } else if f.defl_limited { "LOI NORMALE : limiteur actif" } else { "LOI NORMALE" };
    txt(
        &format!("braquage {defl:.1} deg / rupture {destroy_now:.1} deg a {:.0} KEAS - {law}", ac.d.keas),
        c2.r.x,
        c2.r.y + c2.r.h + 28.0,
        12.0,
        if pc == RED { RED } else if prot_off { AMBER } else { DIM },
    );
}
