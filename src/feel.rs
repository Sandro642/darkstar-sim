//! Ressenti de vol (façon DCS / MSFS), calculé à partir de l'état réel de l'avion :
//! - mouvements de la vue : turbulence, buffeting (incidence proche du décrochage, transsonique,
//!   pression dynamique élevée, facteur de charge), vibration du statoréacteur, roulage sur la piste ;
//! - inertie de la tête : la vue s'enfonce sous facteur de charge et retarde sur le roulis ;
//! - à-coups : toucher des roues, passage du mur du son, allumage scram / postcombustion ;
//! - effets d'écran : voile gris (fort facteur de charge) ou rouge (négatif), lueur de plasma
//!   en hypersonique dense ;
//! - niveau de grondement et « bang » supersonique pour le son.

use crate::aircraft::*;
use crate::svs::Cam;
use macroquad::prelude::*;

pub struct Feel {
    pub on: bool,
    /// Décalage de la vue (lacet, site, roulis) en degrés et sa vitesse (ressort amorti).
    pos: Vec3,
    vel: Vec3,
    noise: Vec3,
    t: f64,
    prev: Option<Prev>,
    /// Voile de facteur de charge filtré (+ gris, − rouge).
    veil: f32,
    /// Intensité du grondement (0..1) pour le son.
    pub rumble: f32,
    /// Bang supersonique à jouer.
    pub boom: bool,
    /// Événements sol du cadre (consommés par le son) : joint de dalle franchi (intensité),
    /// toucher des roues principales (vitesse sol kt, vario ft/min), roulette de nez posée.
    pub joint: f32,
    pub touch: Option<(f32, f32)>,
    pub nose_down: bool,
    /// Distance roulée (m) : joints de dalles de la piste
    roll_dist: f64,
    /// Vibration résiduelle des roues qui tournent encore après l'envol (0..1)
    spin: f32,
}

struct Prev {
    ground: bool,
    vs: f64,
    mach: f64,
    scram: bool,
    ab: f64,
    gear: f64,
    theta: f64,
    psi: f64,
}

fn smooth(x: f64) -> f32 {
    let x = x.clamp(0.0, 1.0) as f32;
    x * x * (3.0 - 2.0 * x)
}

/// Bruit déterministe : somme de sinus aux fréquences incommensurables (Hz), dans [-1, 1].
fn wobble(t: f64, f: f64, seed: f64) -> f32 {
    let a = (t * f * std::f64::consts::TAU + seed).sin() * 0.5;
    let b = (t * f * 1.618 * std::f64::consts::TAU + seed * 2.3).sin() * 0.3;
    let c = (t * f * 2.718 * std::f64::consts::TAU + seed * 4.1).sin() * 0.2;
    (a + b + c) as f32
}

impl Feel {
    pub fn new() -> Feel {
        Feel { on: true, pos: Vec3::ZERO, vel: Vec3::ZERO, noise: Vec3::ZERO, t: 0.0, prev: None, veil: 0.0, rumble: 0.0, boom: false, joint: 0.0, touch: None, nose_down: false, roll_dist: 0.0, spin: 0.0 }
    }

    pub fn reset(&mut self) {
        let on = self.on;
        *self = Feel::new();
        self.on = on;
    }

    /// Avance d'une image (`dt` réel, en secondes).
    pub fn update(&mut self, ac: &Aircraft, dt: f32) {
        let dt = dt.clamp(0.0, 0.05);
        self.t += dt as f64;
        let t = self.t;
        let d = &ac.d;
        let alive = ac.crashed.is_none();

        // --- Sources de vibration ---
        let alpha_frac = d.alpha.to_degrees() / ac.fcs.alpha_max.max(1.0);
        let stall = smooth((alpha_frac - 0.78) / 0.22);
        let transonic = (-((d.mach - 1.0) / 0.07).powi(2)).exp() as f32 * smooth(d.q / 30_000.0);
        let q_buf = smooth((d.q - 85_000.0) / 25_000.0);
        let g_buf = smooth((d.nz - 2.6) / 1.2) * 0.6;
        let scram = if ac.eng.scram == ScramState::Lit { 0.18 * smooth(d.q / 50_000.0) } else { 0.0 };
        let buffet = stall.max(transonic).max(q_buf).max(g_buf).max(scram);
        let gs = ac.ground_speed();
        let roll = if ac.on_ground && alive { (smooth(gs / 90.0) * 1.1).min(1.0) } else { 0.0 };
        // Après l'envol : légère vibration des roues qui ralentissent (≈1,5 s)
        self.spin = (self.spin - dt / 1.5).max(0.0);
        let spin = if ac.on_ground { 0.0 } else { self.spin * self.spin * 0.35 };
        let turb = ac.turb as f32;

        // --- Bruit direct (haute fréquence) ---
        let hi = buffet * 0.30;
        let gr = roll * 0.22 + spin * 0.22;
        let lo = turb * 0.9;
        let n = if self.on && alive {
            vec3(
                wobble(t, 11.0, 1.0) * hi * 0.5 + wobble(t, 1.3, 7.0) * lo * 0.5 + wobble(t, 15.0, 3.0) * gr * 0.4,
                wobble(t, 12.5, 2.0) * hi + wobble(t, 1.7, 5.0) * lo + wobble(t, 17.0, 9.0) * gr,
                wobble(t, 9.0, 4.0) * hi * 0.7 + wobble(t, 0.9, 11.0) * lo * 1.3 + wobble(t, 13.0, 6.0) * gr * 0.5,
            )
        } else {
            Vec3::ZERO
        };
        self.noise = n;

        // Taux de virage (deg/s) pour l'inclinaison de la tête au roulage
        let turn_deg = self.prev.as_ref().map(|p| (((ac.psi - p.psi + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI) / (dt as f64).max(1e-3)).to_degrees()).unwrap_or(0.0);
        // --- Inertie de la tête (ressort amorti) ---
        // En vol : enfoncement sous facteur de charge, retard sur le roulis ; au sol : la tête part
        // en arrière à l'accélération (vue qui monte) et en avant au freinage
        let surge = (-(d.accel as f32) * 0.06).clamp(-0.9, 0.9);
        let target = if self.on && alive && !ac.on_ground {
            vec3(0.0, (-(d.nz - 1.0) * 0.28).clamp(-1.2, 1.2) as f32 + surge * 0.5, (-ac.fcs.roll_rate * 0.012).clamp(-2.0, 2.0) as f32)
        } else if self.on && alive {
            // Au sol : la tête part en arrière à l'accélération, s'incline dans les virages de roulage
            let lean = if gs > 2.0 { (-(turn_deg as f32) * (gs as f32 / 20.0).min(1.0) * 0.25).clamp(-1.5, 1.5) } else { 0.0 };
            vec3(0.0, surge, lean)
        } else {
            Vec3::ZERO
        };
        // Roulage : joints de dalles (à-coup tous les ~25 m), plus secs avec la vitesse
        if self.on && alive && ac.on_ground && gs > 3.0 {
            let before = (self.roll_dist / 25.0).floor();
            self.roll_dist += gs * dt as f64;
            if (self.roll_dist / 25.0).floor() != before {
                let k = smooth(gs / 80.0) * 2.2 + 0.3;
                self.vel.y -= k;
                self.joint = self.joint.max(0.3 + smooth(gs / 60.0) * 0.7);
                self.vel.z += k * 0.35 * if (t * 7.0).sin() > 0.0 { 1.0 } else { -1.0 };
            }
        }
        // À-coups sur transition
        if let Some(p) = &self.prev {
            if self.on && alive {
                if !p.ground && ac.on_ground {
                    // Toucher : écrasement des amortisseurs proportionnel au taux de chute
                    // Butter (< 60 ft/min) : simple frémissement ; dur : écrasement sec du train
                    let s = (-p.vs).max(0.0) as f32;
                    let k = (s.powf(1.4) * 1.1).clamp(0.08, 8.0);
                    self.vel.y -= 11.0 * k;
                    self.vel.z += 4.0 * k * if (t * 10.0).sin() > 0.0 { 1.0 } else { -1.0 };
                    self.touch = Some(((gs / 0.514) as f32, (-p.vs / 0.3048 * 60.0) as f32));
                }
                // Roulette de nez qui retombe sur la piste (dérotation)
                if ac.on_ground && p.theta > 0.6f64.to_radians() && ac.theta <= 0.2f64.to_radians() && gs > 10.0 {
                    self.vel.y -= 16.0;
                    self.vel.x += 2.0 * if (t * 7.0).sin() > 0.0 { 1.0 } else { -1.0 };
                    self.nose_down = true;
                }
                if !ac.on_ground && (p.mach < 1.0) != (d.mach < 1.0) {
                    self.boom = true;
                    self.vel.y += 12.0;
                }
                let lit = ac.eng.scram == ScramState::Lit;
                if lit && !p.scram {
                    self.vel.y += 18.0;
                }
                if ac.eng.ab_frac > 0.5 && p.ab <= 0.5 {
                    self.vel.y += 8.0;
                }
                // Rotation : la roulette avant quitte la piste
                if ac.on_ground && p.theta < 0.01 && ac.theta >= 0.01 {
                    self.vel.y += 6.0;
                }
                // Décollement des roues : le grondement de piste s'arrête net, les amortisseurs se
                // détendent (à-coup vers le haut puis flottement), les roues tournent encore un instant
                if p.ground && !ac.on_ground {
                    self.vel.y += 14.0;
                    self.vel.z += 3.0 * if (t * 13.0).sin() > 0.0 { 1.0 } else { -1.0 };
                    self.spin = 1.0;
                }
                // Train verrouillé (rentré ou sorti) : claquement des trappes
                if (p.gear > 0.02 && ac.gear_pos <= 0.02) || (p.gear < 0.98 && ac.gear_pos >= 0.98) {
                    self.vel.y += 5.0;
                    self.vel.z += 2.0;
                }
            }
        }
        self.prev = Some(Prev { ground: ac.on_ground, vs: d.vs, mach: d.mach, scram: ac.eng.scram == ScramState::Lit, ab: ac.eng.ab_frac, gear: ac.gear_pos, theta: ac.theta, psi: ac.psi });
        let (k, c) = (60.0, 8.5);
        let acc = (target - self.pos) * k - self.vel * c;
        self.vel += acc * dt;
        self.pos += self.vel * dt;
        self.pos = self.pos.clamp(Vec3::splat(-4.0), Vec3::splat(4.0));

        // --- Voile de facteur de charge (montée lente, retour plus rapide) ---
        let want = if !self.on || !alive {
            0.0
        } else if d.nz > 2.8 {
            smooth((d.nz - 2.8) / 1.6)
        } else if d.nz < -0.8 {
            -smooth((-0.8 - d.nz) / 1.2)
        } else {
            0.0
        };
        let tau = if want.abs() > self.veil.abs() { 1.6 } else { 0.7 };
        self.veil += (want - self.veil) * (dt / tau).min(1.0);

        self.rumble = if self.on && alive { (buffet * 0.9).max(roll).max(turb * 0.5).min(1.0) } else { 0.0 };
    }

    /// Caméra de la vision synthétique avec les mouvements de la vue.
    pub fn cam(&self) -> Cam {
        let p = self.pos + self.noise;
        Cam { yaw: p.x, pitch: p.y, roll: p.z, ..Cam::default() }
    }

    /// Effets d'écran posés sur la vision synthétique (sous la symbologie).
    pub fn draw_overlay(&self, ac: &Aircraft, r: Rect, time: f64) {
        if !self.on {
            return;
        }
        // Lueur de plasma : hypersonique dans l'air encore dense
        let d = &ac.d;
        let plasma = smooth((d.mach - 5.0) / 3.0) * smooth(d.q / 45_000.0);
        if plasma > 0.01 && ac.crashed.is_none() {
            let fl = 0.8 + 0.2 * ((time * 23.0).sin() * (time * 7.3).sin()) as f32;
            vignette(r, Color::new(1.0, 0.42, 0.18, 0.22 * plasma * fl), 0.16);
        }
        if self.veil > 0.01 {
            vignette(r, Color::new(0.08, 0.08, 0.09, 0.85 * self.veil), 0.34);
        } else if self.veil < -0.01 {
            vignette(r, Color::new(0.55, 0.0, 0.0, 0.7 * -self.veil), 0.30);
        }
    }
}

/// Assombrissement progressif des bords (anneaux sans recouvrement, plus denses au bord).
fn vignette(r: Rect, c: Color, depth: f32) {
    let n = 14;
    let step = (r.w.min(r.h) * depth / n as f32).max(1.0);
    for i in 0..n {
        let k = 1.0 - i as f32 / n as f32;
        let col = Color::new(c.r, c.g, c.b, c.a * k * k);
        let inset = i as f32 * step;
        let (x, y, w, h) = (r.x + inset, r.y + inset, r.w - 2.0 * inset, r.h - 2.0 * inset);
        if w <= 2.0 * step || h <= 2.0 * step {
            break;
        }
        draw_rectangle(x, y, w, step, col);
        draw_rectangle(x, y + h - step, w, step, col);
        draw_rectangle(x, y + step, step, h - 2.0 * step, col);
        draw_rectangle(x + w - step, y + step, step, h - 2.0 * step, col);
    }
}
