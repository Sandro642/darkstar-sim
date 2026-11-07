//! Surveillance des chambres de combustion (remplace le « % santé ») : l'état est calculé à partir
//! de cinq courbes enregistrées sur les 3 dernières minutes — EGT turbines, paroi du statoréacteur,
//! paroi du scramjet, excès de carburant (manette au-delà de la poussée utile) et usure cumulée.
//! Pour chaque courbe : marge à la limite, temps passé au-dessus du seuil d'alerte, tendance.
//! État : NOMINAL, SURVEILLANCE, DÉGRADÉ, CRITIQUE ou DÉTRUITES, avec la cause principale.

use crate::aircraft::Aircraft;
use crate::systems::{RAM_WALL_BURST, RAM_WALL_CAUTION, SCRAM_WALL_BURST, SCRAM_WALL_CAUTION};
use crate::ui::*;
use macroquad::prelude::*;
use std::collections::VecDeque;

const N: usize = 180;
const PERIOD: f64 = 1.0;

/// Courbes : (nom, seuil d'alerte, limite, unité).
pub const SERIES: [(&str, f64, f64, &str); 5] = [
    ("EGT TURBINES", 900.0, 1000.0, "C"),
    ("PAROI STATO", RAM_WALL_CAUTION, RAM_WALL_BURST, "C"),
    ("PAROI SCRAM", SCRAM_WALL_CAUTION, SCRAM_WALL_BURST, "C"),
    ("EXCES CARBURANT", 0.05, 0.37, ""),
    ("USURE CUMULEE", 0.3, 1.0, ""),
];

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum State {
    Nominal,
    Surveillance,
    Degrade,
    Critique,
    Detruites,
}

impl State {
    pub fn label(&self) -> &'static str {
        match self {
            State::Nominal => "NOMINAL",
            State::Surveillance => "SURVEILLANCE",
            State::Degrade => "DEGRADE",
            State::Critique => "CRITIQUE",
            State::Detruites => "DETRUITES",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            State::Nominal => GREEN_S,
            State::Surveillance => CYAN_S,
            State::Degrade => AMBER,
            _ => RED,
        }
    }
}

pub struct ChamberMon {
    hist: VecDeque<[f64; 5]>,
    t: f64,
    pub state: State,
    pub cause: String,
}

fn values(ac: &Aircraft) -> [f64; 5] {
    let e = &ac.eng;
    [e.egt, e.ram_wall, e.scram_wall, (ac.throttle - e.th_eff).max(0.0), 1.0 - e.health]
}

impl ChamberMon {
    pub fn new() -> ChamberMon {
        ChamberMon { hist: VecDeque::new(), t: 0.0, state: State::Nominal, cause: String::new() }
    }

    /// Échantillonnage (1 s simulée) puis évaluation de l'état.
    pub fn update(&mut self, ac: &Aircraft, dt: f64) {
        self.t += dt;
        if self.t >= PERIOD || self.hist.is_empty() {
            self.t = 0.0;
            self.hist.push_back(values(ac));
            if self.hist.len() > N {
                self.hist.pop_front();
            }
        }
        let (s, c) = self.evaluate(ac);
        self.state = s;
        self.cause = c;
    }

    /// État calculé à partir des courbes : marges, temps au-dessus des seuils, tendance de l'usure.
    fn evaluate(&self, ac: &Aircraft) -> (State, String) {
        if ac.eng.combustor_failed {
            return (State::Detruites, "RUPTURE DE CHAMBRE".into());
        }
        let n = self.hist.len().max(1) as f64;
        let mut worst = (State::Nominal, String::new());
        let mut put = |s: State, c: String| {
            if s > worst.0 {
                worst = (s, c);
            }
        };
        for (i, (name, caution, limit, _)) in SERIES.iter().enumerate() {
            let last = self.hist.back().map(|v| v[i]).unwrap_or(0.0);
            let above = self.hist.iter().filter(|v| v[i] > *caution).count() as f64 / n;
            let margin = (limit - last) / (limit - caution);
            if margin < 0.1 {
                put(State::Critique, format!("{name} EN LIMITE"));
            } else if above > 0.25 || margin < 0.5 {
                put(State::Degrade, format!("{name} : {:.0} s AU-DELA DU SEUIL", above * n));
            } else if last > *caution || above > 0.0 {
                put(State::Surveillance, format!("{name} AU-DELA DU SEUIL"));
            }
        }
        // Tendance de l'usure sur la fenêtre (par minute)
        if let (Some(a), Some(b)) = (self.hist.front(), self.hist.back()) {
            let rate = (b[4] - a[4]) / (n * PERIOD / 60.0).max(1.0 / 60.0);
            if rate > 0.05 {
                put(State::Degrade, "USURE RAPIDE EN COURS".into());
            } else if rate > 0.005 {
                put(State::Surveillance, "USURE EN COURS".into());
            }
        }
        worst
    }

    /// Courbes et état dans `r`.
    pub fn draw(&self, r: Rect) {
        txt("SURVEILLANCE DES CHAMBRES", r.x, r.y + 12.0, 12.0, DIM);
        txt_r(&format!("ETAT {}", self.state.label()), r.x + r.w, r.y + 12.0, 13.0, self.state.color());
        if !self.cause.is_empty() {
            txt_r(&self.cause, r.x + r.w, r.y + 26.0, 10.0, self.state.color());
        }
        let gy = r.y + 32.0;
        let cols = 3usize;
        let gw = (r.w - (cols as f32 - 1.0) * 10.0) / cols as f32;
        let gh = ((r.h - 40.0) / 2.0 - 8.0).max(30.0);
        for (i, (name, caution, limit, unit)) in SERIES.iter().enumerate() {
            let (c, row) = (i % cols, i / cols);
            let g = Rect::new(r.x + c as f32 * (gw + 10.0), gy + row as f32 * (gh + 10.0), gw, gh);
            rrect(g, 4.0, Color::new(0.04, 0.05, 0.06, 1.0));
            rrect_lines(g, 4.0, 1.0, Color::new(0.2, 0.22, 0.25, 1.0));
            // Échelle : 0 → 1,15 x limite
            let top = limit * 1.15;
            let base = if i < 3 { (caution * 0.3).min(*caution - (limit - caution) * 3.0).max(0.0) } else { 0.0 };
            let py = |v: f64| g.y + g.h - 4.0 - (((v - base) / (top - base)).clamp(0.0, 1.0) as f32) * (g.h - 18.0);
            draw_line(g.x + 2.0, py(*caution), g.x + g.w - 2.0, py(*caution), 1.0, Color::new(1.0, 0.75, 0.2, 0.55));
            draw_line(g.x + 2.0, py(*limit), g.x + g.w - 2.0, py(*limit), 1.0, Color::new(1.0, 0.2, 0.15, 0.7));
            let n = self.hist.len();
            let mut prev: Option<Vec2> = None;
            for (k, v) in self.hist.iter().enumerate() {
                let x = g.x + 2.0 + (k as f32 + (N - n) as f32) / (N - 1) as f32 * (g.w - 4.0);
                let p = vec2(x, py(v[i]));
                let col = if v[i] > *limit { RED } else if v[i] > *caution { AMBER } else { GREEN_S };
                if let Some(q) = prev {
                    draw_line(q.x, q.y, p.x, p.y, 1.5, col);
                }
                prev = Some(p);
            }
            txt(name, g.x + 5.0, g.y + 11.0, 9.0, Color::new(0.75, 0.8, 0.85, 1.0));
            if let Some(v) = self.hist.back() {
                let s = if unit.is_empty() { format!("{:.2}", v[i]) } else { format!("{:.0}{unit}", v[i]) };
                let col = if v[i] > *limit { RED } else if v[i] > *caution { AMBER } else { GREEN_S };
                txt_r(&s, g.x + g.w - 5.0, g.y + 11.0, 9.0, col);
            }
        }
        // Dernière case : légende
        let g = Rect::new(r.x + 2.0 * (gw + 10.0), gy + gh + 10.0, gw, gh);
        txt("3 MIN", g.x + 6.0, g.y + 14.0, 10.0, DIM);
        draw_line(g.x + 6.0, g.y + 24.0, g.x + 26.0, g.y + 24.0, 1.0, Color::new(1.0, 0.75, 0.2, 0.7));
        txt("SEUIL D'ALERTE", g.x + 30.0, g.y + 28.0, 9.0, DIM);
        draw_line(g.x + 6.0, g.y + 38.0, g.x + 26.0, g.y + 38.0, 1.0, Color::new(1.0, 0.2, 0.15, 0.8));
        txt("LIMITE", g.x + 30.0, g.y + 42.0, 9.0, DIM);
    }
}
