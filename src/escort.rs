//! Escorte de chasse (patrouille « VIPER », deux F-16) au départ et à l'arrivée, tant que le
//! Darkstar vole sur turbines (avant l'allumage du statoréacteur / après sa coupure) :
//! - départ : rejointe après le décollage, échelon avant gauche / droite, séparation à l'allumage
//!   du stato ou au-delà de Mach 1,6 ;
//! - arrivée : rejointe en descente à moins de 150 NM de la destination, stato coupé et sous
//!   Mach 1,4 ; dégagement en finale (train sorti) ;
//! - messages radio ; trafic affiché sur le ND ; maillage 3D dans la vision synthétique.

use crate::aircraft::*;
use crate::geo::*;
use crate::svs::MeshBuilder;
use crate::terrain::EyeFrame;
use macroquad::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Wing {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    /// Cap (deg), inclinaison (deg)
    pub hdg: f64,
    pub bank: f64,
    /// Côté de la formation (-1 gauche, +1 droite)
    pub side: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Off,
    Joining,
    Formation,
    Leaving,
}

pub struct Escort {
    pub wings: Vec<Wing>,
    pub state: State,
    t: f64,
    done_dep: bool,
    done_arr: bool,
    was_ground: bool,
}

impl Escort {
    pub fn new() -> Escort {
        Escort { wings: Vec::new(), state: State::Off, t: 0.0, done_dep: false, done_arr: false, was_ground: true }
    }

    /// Position de formation du chasseur (échelon avant, côté `side`, rang `k`).
    fn slot(ac: &Aircraft, side: f64, k: usize) -> (f64, f64, f64) {
        // Échelon avancé : dans le champ de la caméra de nez (~25° de l'axe)
        let fwd = 130.0 - k as f64 * 20.0;
        let lat_off = side * (32.0 + k as f64 * 35.0);
        let h = ac.hdg_deg();
        let (la, lo) = destination(ac.lat, ac.lon, h, fwd);
        let (la, lo) = destination(la, lo, h + 90.0, lat_off);
        (la, lo, ac.alt + EYE_H + 8.0 + k as f64 * 4.0)
    }

    fn spawn(&mut self, ac: &Aircraft, behind: bool) {
        self.wings.clear();
        for (k, side) in [-1.0f64, 1.0].iter().enumerate() {
            let h = ac.hdg_deg();
            // Départ : rejointe par l'arrière ; arrivée : croisement par l'avant puis demi-tour rapide
            let (along, lateral) = if behind { (-2_500.0, side * 400.0) } else { (-1_200.0, side * 1_500.0) };
            let (la, lo) = destination(ac.lat, ac.lon, h, along);
            let (la, lo) = destination(la, lo, h + 90.0, lateral);
            self.wings.push(Wing { lat: la, lon: lo, alt: ac.alt + 150.0 * (k as f64 + 1.0), hdg: h, bank: 0.0, side: *side });
        }
        self.state = State::Joining;
        self.t = 0.0;
    }

    /// Une étape. `dest` : (lat, lon) de la destination, `desc` : descente vers l'arrivée.
    pub fn update(&mut self, ac: &Aircraft, dt: f64, dest: Option<(f64, f64)>, desc: bool) -> Vec<(Level, String)> {
        let mut out = Vec::new();
        if dt <= 0.0 {
            return out;
        }
        if ac.crashed.is_some() {
            self.wings.clear();
            self.state = State::Off;
            return out;
        }
        let ram = ac.eng.ram == RamState::Lit || ac.eng.scram == ScramState::Lit;
        let mach = ac.d.mach;
        let agl_ft = ac.d.agl / FT;
        // Nouveau décollage : escorte de départ disponible
        if ac.on_ground {
            if !self.was_ground {
                self.done_arr = true;
            }
            self.was_ground = true;
        } else if self.was_ground {
            self.was_ground = false;
            self.done_dep = false;
            self.done_arr = false;
        }
        let d_dest = dest.map(|(la, lo)| dist_brg(ac.lat, ac.lon, la, lo).0).unwrap_or(f64::MAX);
        match self.state {
            State::Off => {
                if !ac.on_ground && !self.done_dep && agl_ft > 800.0 && !ram && mach < 1.5 && !desc {
                    self.done_dep = true;
                    self.spawn(ac, true);
                    out.push((Level::Info, "VIPER 1 : DARKSTAR, VIPER EN REJOINTE POUR L'ESCORTE DE DEPART — DEUX F-16".into()));
                } else if !ac.on_ground && !ac.gear_down && !self.done_arr && !ram && mach < 1.4 && d_dest < 150.0 * NM && ac.alt / FT < 50_000.0 && (desc || d_dest < 80.0 * NM) {
                    self.done_arr = true;
                    self.done_dep = true;
                    self.spawn(ac, false);
                    out.push((Level::Info, "VIPER 1 : DARKSTAR, VIPER EN REJOINTE — ESCORTE POUR L'ARRIVEE".into()));
                }
            }
            State::Joining | State::Formation => {
                // Séparation : stato allumé / trop rapide, ou finale train sorti
                if ram || mach > 1.6 {
                    self.state = State::Leaving;
                    self.t = 0.0;
                    out.push((Level::Info, "VIPER 1 : BINGO VITESSE, ON SE SEPARE — BONNE CHASSE DARKSTAR".into()));
                } else if ac.gear_down || ac.on_ground || (agl_ft < 2_500.0 && d_dest < 12.0 * NM) {
                    self.state = State::Leaving;
                    self.t = 0.0;
                    out.push((Level::Info, "VIPER 1 : ON VOUS LAISSE EN FINALE, BON ATTERRISSAGE DARKSTAR".into()));
                }
            }
            State::Leaving => {}
        }
        self.t += dt;
        let v_ac = ac.tas.max(80.0);
        let h_ac = ac.hdg_deg();
        // Déplacement du Darkstar sur la route sol (vent compris) et vitesse verticale
        let (gs_ac, trk_ac, vz_ac) = (ac.ground_speed().max(60.0), ac.track_deg(), ac.tas * ac.gamma.sin());
        let mut all_in = true;
        for (k, w) in self.wings.iter_mut().enumerate() {
            match self.state {
                State::Joining | State::Formation => {
                    let (tla, tlo, talt) = Escort::slot(ac, w.side, k / 2);
                    let (d, b) = dist_brg(w.lat, w.lon, tla, tlo);
                    // Vitesse de rapprochement : jusqu'à +90 m/s, freinée près de la place
                    let close = (d * 0.6).min(90.0);
                    let (la, lo) = destination(w.lat, w.lon, trk_ac, gs_ac * dt);
                    let (la, lo) = destination(la, lo, b, (close * dt).min(d));
                    w.lat = la;
                    w.lon = lo;
                    w.alt += vz_ac * dt + (talt - w.alt) * (dt * 1.5).min(1.0);
                    // Cap et inclinaison : vers la place, puis alignés sur le Darkstar
                    let want = if d > 60.0 { h_ac + wrap180(b - h_ac).clamp(-25.0, 25.0) } else { h_ac };
                    w.hdg = wrap360(w.hdg + wrap180(want - w.hdg).clamp(-30.0 * dt, 30.0 * dt));
                    let bank_t = (wrap180(want - h_ac) * 1.6).clamp(-45.0, 45.0) + ac.phi.to_degrees();
                    w.bank += (bank_t - w.bank) * (dt * 2.0).min(1.0);
                    if d > 25.0 {
                        all_in = false;
                    }
                }
                State::Leaving => {
                    // Dégagement : virage vers l'extérieur, montée, vitesse réduite
                    w.hdg = wrap360(w.hdg + w.side * 12.0 * dt);
                    w.bank += (w.side * 50.0 - w.bank) * (dt * 1.5).min(1.0);
                    let v = (v_ac * 0.85).min(260.0);
                    let (la, lo) = destination(w.lat, w.lon, w.hdg, v * dt);
                    w.lat = la;
                    w.lon = lo;
                    w.alt += 25.0 * dt;
                }
                State::Off => {}
            }
        }
        if self.state == State::Joining && all_in && !self.wings.is_empty() {
            self.state = State::Formation;
            out.push((Level::Info, "VIPER 1 : VIPER EN PLACE, AILIERS GAUCHE ET DROIT".into()));
        }
        if self.state == State::Leaving && self.t > 50.0 {
            self.wings.clear();
            self.state = State::Off;
        }
        out
    }

    /// Escorte demandée par l'opérateur (instruction ESCORTE) : rejointe immédiate, sur turbines.
    pub fn request(&mut self, ac: &Aircraft) -> Result<String, String> {
        if ac.on_ground {
            return Err("ESCORTE : EN VOL SEULEMENT".into());
        }
        if ac.eng.ram == RamState::Lit || ac.eng.scram == ScramState::Lit || ac.d.mach > 1.5 {
            return Err("ESCORTE IMPOSSIBLE : TROP RAPIDE POUR LES F-16 (STATO / MACH > 1,5)".into());
        }
        if matches!(self.state, State::Joining | State::Formation) {
            return Err("ESCORTE DEJA EN PLACE".into());
        }
        self.spawn(ac, true);
        Ok("VIPER 1 : DARKSTAR, VIPER EN ROUTE POUR VOTRE ESCORTE".into())
    }

    /// Fin d'escorte demandée par l'opérateur.
    pub fn release(&mut self) -> Result<String, String> {
        if !matches!(self.state, State::Joining | State::Formation) {
            return Err("PAS D'ESCORTE".into());
        }
        self.state = State::Leaving;
        self.t = 0.0;
        Ok("VIPER 1 : BIEN RECU, ON SE SEPARE".into())
    }

    /// Positions pour le ND : (lat, lon, altitude m).
    pub fn traffic(&self) -> Vec<(f64, f64, f64)> {
        self.wings.iter().map(|w| (w.lat, w.lon, w.alt)).collect()
    }
}

thread_local! {
    static RENDER: std::cell::RefCell<Vec<Wing>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Chasseurs à dessiner dans la vision synthétique (mis à jour à chaque image).
pub fn set_render(w: &[Wing]) {
    RENDER.with(|c| *c.borrow_mut() = w.to_vec());
}

/// Maillage simple de F-16 (fuselage, aile delta tronquée, dérive, empennage, entrée d'air).
pub fn draw_3d(ac: &Aircraft, mb: &mut MeshBuilder) {
    let wings = RENDER.with(|c| c.borrow().clone());
    if wings.is_empty() {
        return;
    }
    let eye = EyeFrame::new(ac);
    for w in wings {
        let (d, _) = dist_brg(ac.lat, ac.lon, w.lat, w.lon);
        if d > 20_000.0 {
            continue;
        }
        let h = w.hdg.to_radians();
        let bank = w.bank.to_radians();
        let base = eye.local(w.lat, w.lon, w.alt);
        let fwd = vec3(h.sin() as f32, 0.0, -h.cos() as f32);
        let right0 = vec3(h.cos() as f32, 0.0, h.sin() as f32);
        let up0 = vec3(0.0, 1.0, 0.0);
        let (sb, cb) = (bank.sin() as f32, bank.cos() as f32);
        let right = right0 * cb - up0 * sb;
        let up = up0 * cb + right0 * sb;
        let p = |x: f32, y: f32, z: f32| base + fwd * x + right * y + up * z;
        let top = [150, 156, 164, 255];
        let side = [118, 124, 132, 255];
        let under = [96, 100, 108, 255];
        let canopy = [40, 46, 58, 255];
        // Fuselage (15 m)
        let (l0, l1, r) = (-7.5f32, 6.0f32, 0.75f32);
        mb.quad([p(l0, -r, r), p(l1, -r, r), p(l1, r, r), p(l0, r, r)], top);
        mb.quad([p(l0, -r, -r), p(l1, -r, -r), p(l1, -r, r), p(l0, -r, r)], side);
        mb.quad([p(l0, r, -r), p(l1, r, -r), p(l1, r, r), p(l0, r, r)], side);
        mb.quad([p(l0, -r, -r), p(l1, -r, -r), p(l1, r, -r), p(l0, r, -r)], under);
        // Nez pointu et verrière
        mb.quad([p(l1, -r, r), p(l1 + 3.5, -0.1, 0.1), p(l1 + 3.5, 0.1, 0.1), p(l1, r, r)], top);
        mb.quad([p(l1, -r, -r), p(l1 + 3.5, -0.1, 0.0), p(l1 + 3.5, 0.1, 0.0), p(l1, r, -r)], under);
        mb.quad([p(2.0, -0.45, r), p(5.5, -0.3, r), p(5.5, 0.3, r), p(2.0, 0.45, r)], canopy);
        mb.quad([p(2.5, -0.35, r + 0.55), p(5.0, -0.2, r + 0.2), p(5.0, 0.2, r + 0.2), p(2.5, 0.35, r + 0.55)], canopy);
        for s in [-1.0f32, 1.0] {
            // Aile (envergure 9,8 m), bord d'attaque en flèche
            mb.quad([p(1.0, s * r, 0.0), p(-3.5, s * 4.9, 0.0), p(-5.0, s * 4.9, 0.0), p(-4.5, s * r, 0.0)], top);
            // Empennage horizontal
            mb.quad([p(-5.5, s * r, 0.0), p(-7.5, s * 2.8, -0.1), p(-8.3, s * 2.8, -0.1), p(-7.5, s * r, 0.0)], side);
            // Missile en bout d'aile
            mb.quad([p(-1.0, s * 5.0, 0.05), p(-4.5, s * 5.0, 0.05), p(-4.5, s * 5.0, -0.2), p(-1.0, s * 5.0, -0.2)], [210, 212, 214, 255]);
        }
        // Dérive
        mb.quad([p(-4.5, 0.0, r), p(-7.0, 0.0, r + 3.6), p(-8.3, 0.0, r + 3.6), p(-7.5, 0.0, r)], side);
        // Entrée d'air ventrale
        mb.quad([p(3.0, -0.6, -r), p(-1.0, -0.6, -r - 0.6), p(-1.0, 0.6, -r - 0.6), p(3.0, 0.6, -r)], [60, 62, 66, 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn departure_and_arrival() {
        let mut ac = Aircraft::new_on_runway(0);
        let mut e = Escort::new();
        let dt = 0.1;
        let _ = e.update(&ac, dt, None, false);
        // Montée après décollage : rejointe puis mise en place
        ac.on_ground = false;
        ac.gear_down = false;
        ac.alt += 1_500.0;
        ac.d.agl = 1_500.0;
        ac.tas = 180.0;
        let mut msgs = Vec::new();
        for _ in 0..1200 {
            let (la, lo) = destination(ac.lat, ac.lon, ac.hdg_deg(), ac.tas * dt);
            ac.lat = la;
            ac.lon = lo;
            msgs.extend(e.update(&ac, dt, None, false).into_iter().map(|m| m.1));
        }
        assert_eq!(e.state, State::Formation, "{msgs:?}");
        let (d, _) = dist_brg(ac.lat, ac.lon, e.wings[0].lat, e.wings[0].lon);
        assert!(d < 180.0, "ailier a {d:.0} m");
        // Allumage du stato : séparation
        ac.eng.ram = RamState::Lit;
        msgs.extend(e.update(&ac, dt, None, false).into_iter().map(|m| m.1));
        assert_eq!(e.state, State::Leaving);
        for _ in 0..600 {
            let _ = e.update(&ac, dt, None, false);
        }
        assert!(e.wings.is_empty() && e.state == State::Off);
        // Arrivée : stato coupé, descente à 100 NM de la destination
        ac.eng.ram = RamState::Off;
        let dest = destination(ac.lat, ac.lon, ac.hdg_deg(), 100.0 * NM);
        msgs.extend(e.update(&ac, dt, Some(dest), true).into_iter().map(|m| m.1));
        assert_eq!(e.state, State::Joining, "{msgs:?}");
        ac.gear_down = true;
        msgs.extend(e.update(&ac, dt, Some(dest), true).into_iter().map(|m| m.1));
        assert_eq!(e.state, State::Leaving);
        println!("{msgs:#?}");
    }
}
