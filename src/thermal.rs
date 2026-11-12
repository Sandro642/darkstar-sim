//! Télémesure thermique : thermocouples répartis sur la cellule et la propulsion.

use crate::aircraft::*;
use crate::ui::*;
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Skin(f64),     // fraction de l'échauffement aérodynamique du nez
    Inlet,
    Cit,
    Egt(f64),
    Oil,
    RamWall,
    ScramWall,
    Nozzle,
    FuelTank,
    FuelInj,
    Avionics,
    Hydraulic,
    GearBay,
    Brakes,
    Battery,
}

pub struct Sensor {
    pub id: &'static str,
    pub name: &'static str,
    pub pos: (f32, f32), // position sur la vue en plan (x -1..1, y 0 nez .. 1 tuyère)
    pub kind: Kind,
    pub tau: f64,
    pub caution: f64,
    pub limit: f64,
    pub truth: f64,
    pub value: f64,
    pub failed: bool,
    /// Capteur inhibé par l'opérateur (n'alarme plus).
    pub inhibited: bool,
    pub hist: VecDeque<f32>,
    pub rate: f64, // °C/min
}

pub struct ThermalSys {
    pub sensors: Vec<Sensor>,
    sample_t: f64,
    /// Capteur sélectionné (clic sur le schéma ou la liste, ou instruction CAPTEUR).
    pub selected: Option<usize>,
    pub events: Vec<(Level, String)>,
}

macro_rules! tc {
    ($id:expr, $n:expr, $x:expr, $y:expr, $k:expr, $tau:expr, $c:expr, $l:expr) => {
        Sensor { id: $id, name: $n, pos: ($x, $y), kind: $k, tau: $tau, caution: $c, limit: $l, truth: 15.0, value: 15.0, failed: false, inhibited: false, hist: VecDeque::new(), rate: 0.0 }
    };
}

impl ThermalSys {
    pub fn new() -> Self {
        let sensors = vec![
            tc!("TC-101", "NEZ (POINTE)", 0.0, 0.03, Kind::Skin(1.0), 6.0, 1150.0, 1250.0),
            tc!("TC-112", "BORD ATTAQUE AILE G", -0.42, 0.70, Kind::Skin(0.90), 10.0, 1050.0, 1150.0),
            tc!("TC-113", "BORD ATTAQUE AILE D", 0.42, 0.70, Kind::Skin(0.905), 10.0, 1050.0, 1150.0),
            tc!("TC-121", "LEVRE ENTREE D'AIR", 0.13, 0.42, Kind::Inlet, 8.0, 1100.0, 1200.0),
            tc!("TC-131", "INTRADOS AVANT", 0.0, 0.30, Kind::Skin(0.70), 25.0, 900.0, 1000.0),
            tc!("TC-132", "INTRADOS ARRIERE", 0.0, 0.76, Kind::Skin(0.55), 35.0, 800.0, 900.0),
            tc!("TC-141", "EXTRADOS", -0.26, 0.72, Kind::Skin(0.40), 40.0, 650.0, 750.0),
            tc!("TC-151", "DERIVE BORD ATTAQUE", 0.20, 0.80, Kind::Skin(0.80), 12.0, 1000.0, 1100.0),
            tc!("TC-201", "CIT ENTREE COMPRESSEUR", -0.12, 0.52, Kind::Cit, 2.0, 400.0, 430.0),
            tc!("TC-202", "EGT TURBINE G", -0.13, 0.88, Kind::Egt(8.0), 1.5, 900.0, 1000.0),
            tc!("TC-203", "EGT TURBINE D", 0.13, 0.88, Kind::Egt(-6.0), 1.5, 900.0, 1000.0),
            tc!("TC-204", "HUILE TURBINES", 0.0, 0.82, Kind::Oil, 60.0, 160.0, 180.0),
            tc!("TC-301", "PAROI COMB. STATO", 0.08, 0.70, Kind::RamWall, 6.0, 1400.0, 1500.0),
            tc!("TC-401", "PAROI COMB. SCRAM", -0.08, 0.70, Kind::ScramWall, 4.0, 1600.0, 1700.0),
            tc!("TC-402", "RAMPE TUYERE", 0.0, 0.97, Kind::Nozzle, 15.0, 1100.0, 1200.0),
            tc!("TC-501", "CARBURANT RESERVOIR", 0.0, 0.55, Kind::FuelTank, 300.0, 120.0, 150.0),
            tc!("TC-502", "CARBURANT INJECTEURS", 0.05, 0.63, Kind::FuelInj, 20.0, 350.0, 420.0),
            tc!("TC-601", "BAIE AVIONIQUE", 0.0, 0.17, Kind::Avionics, 120.0, 60.0, 75.0),
            tc!("TC-602", "FLUIDE HYDRAULIQUE", -0.12, 0.47, Kind::Hydraulic, 150.0, 260.0, 300.0),
            tc!("TC-701", "BAIE TRAIN", 0.14, 0.54, Kind::GearBay, 90.0, 180.0, 220.0),
            tc!("TC-702", "FREINS", 0.18, 0.62, Kind::Brakes, 300.0, 500.0, 700.0),
            tc!("TC-801", "BATTERIE", 0.0, 0.24, Kind::Battery, 300.0, 50.0, 60.0),
        ];
        ThermalSys { sensors, sample_t: 0.0, events: Vec::new(), selected: None }
    }

    pub fn reading(&self, id: &str) -> f64 {
        self.sensors.iter().find(|s| s.id == id).map(|s| s.truth).unwrap_or(0.0)
    }

    pub fn update(&mut self, dt: f64, ac: &Aircraft) {
        if dt <= 0.0 {
            return;
        }
        let oat = ac.d.oat;
        let nose_eq = skin_equilibrium(ac.alt, ac.tas) - 273.15;
        let rise = (nose_eq - oat).max(0.0);
        let mach = ac.d.mach;
        let t_amb_k = oat + 273.15;
        let stag = t_amb_k * (1.0 + 0.2 * mach * mach) - 273.15;
        let alpha = ac.d.alpha.to_degrees().max(0.0);
        let e = &ac.eng;
        let fuel_frac = ac.fuel / FUEL_MAX;
        // FTMS : part de la chaleur restant dans le carburant. AUTO : échangeur H2 (tant qu'il reste
        // de l'hydrogène) + air dynamique ; RAM AIR : air dynamique seul, inefficace quand l'air
        // d'impact devient plus chaud que le carburant (haut Mach) ; OFF : aucun refroidissement.
        let ram_k = (0.45 + ((mach - 2.0) / 4.0).clamp(0.0, 1.0) * 0.55).min(1.0);
        let ftms_k = match ac.fuel_cool {
            0 if ac.h2 > 300.0 && !ac.fails.has(crate::systems::Fail::ScramCool) => 0.22,
            0 | 1 => ram_k,
            _ => 1.0,
        };
        let tank = self.reading("TC-501");
        let skin_avg = self.reading("TC-131");
        let avion = self.reading("TC-601");
        let ram_w = self.reading("TC-301");
        let scram_w = self.reading("TC-401");

        for s in &mut self.sensors {
            let target = match s.kind {
                Kind::Skin(f) => {
                    let mut t = oat + rise * f;
                    if s.id.starts_with("TC-13") {
                        t += rise * alpha * 0.015; // intrados plus chaud avec l'incidence
                    }
                    if s.id == "TC-141" {
                        t -= rise * alpha * 0.01;
                    }
                    t
                }
                Kind::Inlet => oat + rise * 0.93 + if e.scram == ScramState::Lit { 30.0 } else { 0.0 },
                Kind::Cit => {
                    // Portes fermées : la sonde CIT ne voit plus que l'air de la cavité
                    oat + (stag - oat) * (1.0 - e.doors * 0.94)
                }
                Kind::Egt(off) => {
                    if matches!(e.turb, TurbState::Running | TurbState::Starting) {
                        e.egt + off
                    } else {
                        e.egt
                    }
                }
                Kind::Oil => {
                    if e.turb == TurbState::Running {
                        // Huile refroidie par échangeur carburant
                        60.0 + 55.0 * e.n2 + stag.max(0.0) * 0.08
                    } else {
                        oat.max(avion - 10.0)
                    }
                }
                Kind::RamWall => e.ram_wall,
                Kind::ScramWall => e.scram_wall,
                Kind::Nozzle => ram_w.max(scram_w) * 0.45 + rise * 0.25 + e.egt.max(0.0) * 0.08,
                // Réservoirs : chaleur de la cellule, évacuée par le FTMS (boucle carburant → échangeur
                // à hydrogène liquide, puits froid à -253 °C, puis échangeur air dynamique en appoint)
                Kind::FuelTank => 20.0 + (skin_avg - 20.0).max(0.0) * 0.25 * (1.0 - fuel_frac * 0.6) * ftms_k,
                // Carburant après l'échangeur : échauffé par la cellule, pas par le débit (puits de chaleur)
                Kind::FuelInj => tank + 30.0 + (rise * 0.12 + if e.ram == RamState::Lit || e.scram == ScramState::Lit { 40.0 } else { 0.0 }) * ftms_k.max(0.4),
                Kind::Avionics => ac.elec.ebay_temp + (tank - 20.0).max(0.0) * 0.15,
                Kind::Hydraulic => 40.0 + rise * 0.12,
                // Baie isolée et refroidie par la boucle carburant (pneus)
                Kind::GearBay => oat + rise * 0.15,
                Kind::Brakes => oat.max(15.0),
                Kind::Battery => 25.0 + (avion - 25.0).max(0.0) * 0.4,
            };
            if s.kind == Kind::Brakes {
                // Énergie de freinage : échauffement rapide, refroidissement lent
                if ac.on_ground && ac.brakes && ac.tas > 1.0 {
                    s.truth += ac.tas * 0.25 * dt;
                }
                s.truth += (target - s.truth) * (dt / s.tau).min(1.0);
            } else {
                s.truth += (target - s.truth) * (dt / s.tau).min(1.0);
            }
            // Bruit de mesure (thermocouple type K : ±0.75 % ou ±2.2 °C)
            let noise = gen_range(-1.0, 1.0) * (0.4 + s.truth.abs() * 0.0015);
            s.value = s.truth + noise;
        }

        // Pannes aléatoires de capteurs (circuit ouvert)
        for s in &mut self.sensors {
            if !s.failed && gen_range(0.0, 1.0) < dt / 90_000.0 {
                s.failed = true;
                self.events.push((Level::Caution, format!("CAPTEUR {} ({}) : CIRCUIT OUVERT", s.id, s.name)));
            }
        }

        // Échantillonnage pour l'historique et la tendance
        self.sample_t -= dt;
        if self.sample_t <= 0.0 {
            self.sample_t = 1.0;
            for s in &mut self.sensors {
                s.hist.push_back(s.truth as f32);
                if s.hist.len() > 120 {
                    s.hist.pop_front();
                }
                let n = s.hist.len();
                if n > 10 {
                    s.rate = (s.hist[n - 1] - s.hist[n - 11]) as f64 * 6.0;
                }
            }
        }
    }

    /// Retrouve un capteur par son numéro (« 101 », « TC-101 »).
    pub fn find(&self, num: &str) -> Option<usize> {
        let n = num.trim_start_matches("TC-").trim_start_matches("TC");
        self.sensors.iter().position(|s| s.id.trim_start_matches("TC-") == n)
    }

    pub fn alerts(&self) -> Vec<(Level, String)> {
        let mut a = Vec::new();
        for s in &self.sensors {
            if s.inhibited {
                continue;
            }
            if s.failed {
                a.push((Level::Caution, format!("SENS FAIL {}", s.id)));
            } else if s.value > s.limit {
                a.push((Level::Warning, format!("OVERTEMP {}", s.name)));
            } else if s.value > s.caution {
                a.push((Level::Caution, format!("TEMP {}", s.name)));
            }
        }
        a
    }
}

fn temp_color(v: f64, caution: f64, limit: f64) -> Color {
    if v > limit {
        RED
    } else if v > caution {
        AMBER
    } else {
        let f = (v / caution).clamp(0.0, 1.0) as f32;
        if f < 0.5 {
            Color::new(0.2, 0.6 + f * 0.6, 1.0 - f, 1.0)
        } else {
            Color::new((f - 0.5) * 1.6, 1.0, 0.25, 1.0)
        }
    }
}

/// Dessine la page thermique ; renvoie l'indice du capteur cliqué (schéma ou liste).
pub fn draw_thermal(th: &ThermalSys, r: Rect, time: f64) -> Option<usize> {
    let mut clicked_idx = None;
    panel(r, "THERMIQUE — TELEMESURE CAPTEURS");
    let n_bad = th.sensors.iter().filter(|s| !s.failed && !s.inhibited && s.value > s.caution).count();
    let n_fail = th.sensors.iter().filter(|s| s.failed).count();
    let n_inh = th.sensors.iter().filter(|s| s.inhibited).count();
    txt_r(&format!("{} CAPTEURS  {} ALERTE  {} HS  {} INHIB", th.sensors.len(), n_bad, n_fail, n_inh), r.x + r.w - 8.0, r.y + 15.0, 13.0, if n_bad > 0 { AMBER } else { DIM });
    let sel = th.selected;
    let pulse = (time * 4.0).sin() as f32 * 0.5 + 0.5;

    // Vue en plan de la cellule (silhouette de la page SYSTEMES)
    let pw = (r.w * 0.36).min(r.h * 0.62);
    let pr = Rect::new(r.x + 10.0, r.y + 30.0, pw, r.h - 40.0);
    let to = crate::syspage::plan(pr);
    crate::syspage::silhouette(pr, Color::new(0.25, 0.85, 0.35, 0.9), Color::new(0.05, 0.16, 0.08, 1.0));
    for (i, s) in th.sensors.iter().enumerate() {
        let p = to(s.pos.0, s.pos.1);
        if clicked(Rect::new(p.x - 9.0, p.y - 9.0, 18.0, 18.0)) {
            clicked_idx = Some(i);
        }
        if s.inhibited {
            draw_circle_lines(p.x, p.y, 5.0, 1.0, DIM);
            draw_line(p.x - 4.0, p.y - 4.0, p.x + 4.0, p.y + 4.0, 1.0, DIM);
        } else if s.failed {
            let blink = (time * 2.0).fract() < 0.5;
            draw_circle_lines(p.x, p.y, 5.0, 1.5, if blink { AMBER } else { DIM });
        } else {
            let c = temp_color(s.value, s.caution, s.limit);
            draw_circle(p.x, p.y, 7.0, Color::new(c.r, c.g, c.b, 0.25));
            draw_circle(p.x, p.y, 4.0, c);
        }
        if sel == Some(i) {
            draw_circle_lines(p.x, p.y, 9.0 + pulse * 4.0, 2.0, ACCENT);
        }
    }
    // Étiquettes des capteurs de peau principaux
    for id in ["TC-101", "TC-112", "TC-113", "TC-402"] {
        if let Some(s) = th.sensors.iter().find(|s| s.id == id && !s.failed) {
            let p = to(s.pos.0, s.pos.1);
            let c = temp_color(s.value, s.caution, s.limit);
            let dx = if s.pos.0 < 0.0 { -38.0 } else if s.pos.0 > 0.0 { 6.0 } else { 9.0 };
            txt(&format!("{:.0}", s.value), p.x + dx, p.y - 6.0, 13.0, c);
        }
    }
    // Étiquette du capteur sélectionné, à côté du point
    if let Some(i) = sel {
        let s = &th.sensors[i];
        let p = to(s.pos.0, s.pos.1);
        let val = if s.inhibited { "INHIB".to_string() } else if s.failed { "OPEN".to_string() } else { format!("{:.0}C", s.value) };
        let l1 = format!("{} {}", s.id, val);
        let w = txt_w(&l1, 13.0).max(txt_w(s.name, 11.0)) + 12.0;
        let bx = if s.pos.0 < 0.0 { p.x + 14.0 } else { (p.x - 14.0 - w).max(r.x + 4.0) };
        let bx = bx.min(r.x + pw + 10.0 - w);
        let by = p.y - 30.0;
        rrect(Rect::new(bx, by, w, 34.0), 5.0, Color::new(0.02, 0.05, 0.08, 0.92));
        rrect_lines(Rect::new(bx, by, w, 34.0), 5.0, 1.0, ACCENT);
        txt(&l1, bx + 6.0, by + 14.0, 13.0, WHITE);
        txt(s.name, bx + 6.0, by + 28.0, 11.0, DIM);
    }
    // Échelle de couleurs
    let lx = r.x + 10.0;
    let ly = r.y + r.h - 12.0;
    for i in 0..40 {
        let f = i as f64 / 40.0;
        draw_rectangle(lx + i as f32 * 2.0, ly - 6.0, 2.0, 6.0, temp_color(f * 1.1, 1.0, 1.05));
    }
    txt("FROID", lx, ly - 9.0, 11.0, DIM);
    txt("LIMITE", lx + 64.0, ly - 9.0, 11.0, DIM);

    // Tableau
    let tx = r.x + pw + 22.0;
    let tw = r.x + r.w - tx - 8.0;
    let row_h = ((r.h - 46.0) / th.sensors.len() as f32).clamp(11.0, 16.0);
    let fs = (row_h - 1.0).min(13.0);
    let mut y = r.y + 36.0;
    let c_val = tx + tw * 0.56;
    let c_rate = tx + tw * 0.70;
    let c_lim = tx + tw * 0.80;
    let spark_x = tx + tw * 0.82;
    let spark_w = tw * 0.18;
    txt("CAPTEUR / ZONE", tx, y - 4.0, 11.0, DIM);
    txt_r("°C", c_val, y - 4.0, 11.0, DIM);
    txt_r("/MIN", c_rate, y - 4.0, 11.0, DIM);
    txt_r("LIM", c_lim, y - 4.0, 11.0, DIM);
    y += row_h - 2.0;
    for (i, s) in th.sensors.iter().enumerate() {
        let row_r = Rect::new(tx - 4.0, y - row_h + 3.0, tw + 8.0, row_h);
        if sel == Some(i) {
            rrect(row_r, 3.0, Color::new(0.12, 0.25, 0.35, 0.9));
            draw_rectangle(row_r.x, row_r.y, 3.0, row_r.h, ACCENT);
        } else if hovered(row_r) {
            draw_rectangle(row_r.x, row_r.y, row_r.w, row_r.h, Color::new(1.0, 1.0, 1.0, 0.04));
        }
        if clicked(row_r) {
            clicked_idx = Some(i);
        }
        let c = if s.failed || s.inhibited { DIM } else { temp_color(s.value, s.caution, s.limit) };
        let name_col = if s.inhibited {
            DIM
        } else if s.failed || s.value <= s.caution {
            Color::new(0.75, 0.82, 0.88, 1.0)
        } else {
            c
        };
        let label = format!("{} {}", &s.id[3..], s.name);
        txt(&label, tx, y, fs, name_col);
        if s.inhibited {
            txt_r("INHIB", c_val, y, fs, DIM);
        } else if s.failed {
            txt_r("OPEN", c_val, y, fs, AMBER);
        } else {
            txt_r(&format!("{:.0}", s.value), c_val, y, fs, c);
            let rc = if s.rate.abs() < 0.5 { DIM } else if s.rate > 0.0 { AMBER } else { CYAN_S };
            txt_r(&format!("{:+.0}", s.rate), c_rate, y, fs, rc);
        }
        txt_r(&format!("{:.0}", s.limit), c_lim, y, fs, DIM);
        // Courbe des 2 dernières minutes, normalisée sur la limite
        if s.hist.len() > 1 && !s.failed && !s.inhibited {
            let n = s.hist.len();
            let sy = y - 1.0;
            let sh = row_h - 3.0;
            draw_line(spark_x, sy - sh * (s.caution / s.limit) as f32, spark_x + spark_w, sy - sh * (s.caution / s.limit) as f32, 1.0, Color::new(1.0, 0.7, 0.0, 0.25));
            let mut prev: Option<Vec2> = None;
            for (i, v) in s.hist.iter().enumerate() {
                let p = vec2(spark_x + spark_w * i as f32 / 119.0, sy - sh * (*v as f64 / s.limit).clamp(0.0, 1.05) as f32);
                if let Some(q) = prev {
                    draw_line(q.x, q.y, p.x, p.y, 1.0, c);
                }
                prev = Some(p);
            }
            let _ = n;
        }
        y += row_h;
    }
    clicked_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::FT;

    /// À 100 % de manette, aucun capteur ne doit dépasser son seuil d'alerte (refroidissement nominal).
    #[test]
    fn full_throttle_no_overtemp() {
        let cases: [(&str, f64, f64); 4] = [("sol PC", 0.0, 0.0), ("turbine M2.2", 50_000.0, 2.2), ("stato M4", 75_000.0, 4.0), ("scram M9", 105_000.0, 9.0)];
        for (name, alt_ft, mach) in cases {
            let mut ac = Aircraft::new_on_runway(0);
            let mut th = ThermalSys::new();
            if mach > 0.0 {
                ac.on_ground = false;
                ac.brakes = false;
                ac.gear_down = false;
                ac.gear_pos = 0.0;
                ac.alt = alt_ft * FT;
                ac.tas = mach * crate::atmosphere::isa(ac.alt).a;
                ac.theta = 3f64.to_radians();
                ac.ap.on = true;
                ac.ap.alt_tgt_ft = alt_ft;
                ac.ap.hdg_tgt = ac.hdg_deg();
                ac.ap.at_on = true;
                ac.ap.mach_tgt = mach;
                ac.skin_temp = skin_equilibrium(ac.alt, ac.tas);
            }
            match name {
                "sol PC" | "turbine M2.2" => ac.set_turbines(TurbState::Running, 1.0),
                "stato M4" => {
                    ac.eng.inlet_open = false;
                    ac.eng.ram = RamState::Lit;
                }
                _ => {
                    ac.eng.inlet_open = false;
                    ac.eng.scram = ScramState::Lit;
                }
            }
            ac.throttle = 1.0;
            let dt = 1.0 / 60.0;
            for _ in 0..(600 * 60) {
                if mach == 0.0 {
                    // Point fixe moteur : avion calé
                    ac.throttle = 1.0;
                    ac.tas = 0.0;
                } else {
                    ac.ap.at_on = false;
                    ac.throttle = 1.0;
                }
                ac.step(dt, 0.0, 0.0);
                th.update(dt, &ac);
            }
            let hot: Vec<String> = th.sensors.iter().filter(|s| !s.failed && s.truth > s.caution).map(|s| format!("{} {:.0}/{:.0}", s.name, s.truth, s.caution)).collect();
            let peak = th.sensors.iter().map(|s| (s.truth / s.caution, s.name)).fold((0.0, ""), |a, b| if b.0 > a.0 { b } else { a });
            println!("{name}: M{:.2} crash={:?} — capteur le plus proche du seuil : {} {:.0}%", ac.d.mach, ac.crashed, peak.1, peak.0 * 100.0);
            assert!(ac.crashed.is_none(), "{name}: {:?}", ac.crashed);
            assert!(hot.is_empty(), "{name}: surchauffe {:?}", hot);
        }
    }
}
