//! Ravitaillement en vol autonome (perche rigide) : ravitailleur en hippodrome, rendez-vous,
//! jonction sur la trace du ravitailleur, capture en précontact, contact sous la perche, transfert
//! de JP-7 et d'hydrogène liquide (scramjet), déconnexion et dégagement. Rendu 3D du ravitailleur et de la perche dans l'horizon.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use crate::svs::MeshBuilder;
use crate::terrain::EyeFrame;
use crate::ui::*;
use macroquad::prelude::*;

const TANKER_FL: f64 = 25_000.0;
const TANKER_KEAS: f64 = 300.0;
const LEG: f64 = 100.0 * NM;
const BANK_DEG: f64 = 25.0;
const OFFLOAD: f64 = 40_000.0;
/// Débit de transfert (kg/s) : ~2,5 t/min.
const RATE: f64 = 42.0;
/// Hydrogène liquide (scramjet) : réserve du ravitailleur et débit (kg/s, ~1 t/min).
const OFFLOAD_H2: f64 = 16_000.0;
const RATE_H2: f64 = 17.0;
/// Positions relatives au ravitailleur (m) : (en arrière, à droite, au-dessus).
const PRECONTACT: (f64, f64, f64) = (60.0, 0.0, -12.0);
/// Contact : perche déployée ~12 m sous la queue du ravitailleur, visible ~16° au-dessus du nez.
const CONTACT: (f64, f64, f64) = (32.0, 0.0, -9.0);
/// Décélération admise pour le rapprochement (m/s²) : la vitesse de rapprochement suit √(2·a·d).
const DECEL: f64 = 0.25;

/// Vitesse de rapprochement (kt) compatible avec la distance restante (m), signée.
fn closure_kt(dist: f64, max_kt: f64) -> f64 {
    let v = (2.0 * DECEL * dist.abs()).sqrt() / KT;
    v.min(max_kt) * dist.signum()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Off,
    Rendezvous,
    Join,
    Precontact,
    Contact,
    Disconnect,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Off => "",
            Phase::Rendezvous => "RENDEZ-VOUS",
            Phase::Join => "JONCTION",
            Phase::Precontact => "PRECONTACT",
            Phase::Contact => "CONTACT",
            Phase::Disconnect => "DECONNEXION",
        }
    }
}

pub struct Tanker {
    pub callsign: &'static str,
    /// Point d'entrée de l'hippodrome et cap de la branche aller.
    anchor: (f64, f64),
    inbound: f64,
    s: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub hdg: f64,
    pub bank: f64,
    pub tas: f64,
    pub offload: f64,
    /// Hydrogène liquide disponible (kg).
    pub offload_h2: f64,
}

impl Tanker {
    fn radius(&self) -> f64 {
        self.tas * self.tas / (9.81 * BANK_DEG.to_radians().tan())
    }

    /// Position (x le long de la branche aller, y à droite), cap relatif et inclinaison sur l'hippodrome.
    fn track_at(&self, s: f64) -> (f64, f64, f64, f64) {
        let r = self.radius();
        let turn = std::f64::consts::PI * r;
        let per = 2.0 * LEG + 2.0 * turn;
        let s = s.rem_euclid(per);
        let b = BANK_DEG;
        if s < LEG {
            (s, 0.0, 0.0, 0.0)
        } else if s < LEG + turn {
            let th = (s - LEG) / r;
            (LEG + r * th.sin(), r - r * th.cos(), th.to_degrees(), b)
        } else if s < 2.0 * LEG + turn {
            let u = s - LEG - turn;
            (LEG - u, 2.0 * r, 180.0, 0.0)
        } else {
            let th = (s - 2.0 * LEG - turn) / r;
            (-r * th.sin(), r + r * th.cos(), 180.0 + th.to_degrees(), b)
        }
    }

    fn place(&mut self) {
        let (x, y, rel, bank) = self.track_at(self.s);
        let h = self.inbound.to_radians();
        let (e, n) = (x * h.sin() + y * h.cos(), x * h.cos() - y * h.sin());
        let (la, lo) = from_enu(self.anchor.0, self.anchor.1, e, n);
        self.lat = la;
        self.lon = lo;
        self.hdg = wrap360(self.inbound + rel);
        self.bank = bank;
    }

    fn update(&mut self, dt: f64) {
        self.s += self.tas * dt;
        self.place();
    }

    /// Hippodrome pour la carte.
    pub fn racetrack(&self) -> Vec<(f64, f64)> {
        let r = self.radius();
        let per = 2.0 * LEG + 2.0 * std::f64::consts::PI * r;
        let h = self.inbound.to_radians();
        (0..=96)
            .map(|k| {
                let (x, y, _, _) = self.track_at(per * k as f64 / 96.0);
                from_enu(self.anchor.0, self.anchor.1, x * h.sin() + y * h.cos(), x * h.cos() - y * h.sin())
            })
            .collect()
    }

    /// Point du repère ravitailleur (en arrière, à droite, au-dessus) en coordonnées géographiques.
    pub fn point(&self, aft: f64, right: f64, up: f64) -> (f64, f64, f64) {
        let h = self.hdg.to_radians();
        let (fwd, r) = (-aft, right);
        let (e, n) = (fwd * h.sin() + r * h.cos(), fwd * h.cos() - r * h.sin());
        let (la, lo) = from_enu(self.lat, self.lon, e, n);
        (la, lo, self.alt + up)
    }
}

pub struct Refuel {
    pub tanker: Option<Tanker>,
    pub phase: Phase,
    pub given: f64,
    /// Hydrogène transféré (kg).
    pub given_h2: f64,
    t_phase: f64,
    /// Glissement en maintien de position : (départ, arrivée, progression 0..1).
    glide: Option<((f64, f64, f64), (f64, f64, f64), f64)>,
    pub events: Vec<(Level, String)>,
}

impl Refuel {
    pub fn new() -> Refuel {
        Refuel { tanker: None, phase: Phase::Off, given: 0.0, given_h2: 0.0, t_phase: 0.0, glide: None, events: Vec::new() }
    }

    fn tanker_tas(alt: f64) -> f64 {
        TANKER_KEAS * KT / (crate::atmosphere::isa(alt).rho / crate::atmosphere::RHO0).sqrt()
    }

    /// Ravitailleur à 20 NM devant sur la route, ou en orbite au-dessus d'un aérodrome.
    pub fn spawn(&mut self, ac: &Aircraft, apt: Option<usize>) -> String {
        let alt = TANKER_FL * FT;
        let tas = Self::tanker_tas(alt);
        let (anchor, inbound) = match apt {
            Some(i) => ((AIRPORTS[i].lat, AIRPORTS[i].lon), wrap360(ac.track_deg())),
            None => (destination(ac.lat, ac.lon, ac.track_deg(), 20.0 * NM), ac.track_deg()),
        };
        let mut t = Tanker { callsign: "SHELL 71", anchor, inbound, s: 0.0, lat: 0.0, lon: 0.0, alt, hdg: 0.0, bank: 0.0, tas, offload: OFFLOAD, offload_h2: OFFLOAD_H2 };
        t.place();
        let (d, b) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
        let msg = format!("RAVITAILLEUR {} EN ORBITE FL{:.0} {:.0} KEAS — {b:03.0} deg {:.0} NM, {:.0} t DISPONIBLES", t.callsign, TANKER_FL / 100.0, TANKER_KEAS, d / NM, OFFLOAD / 1000.0);
        self.tanker = Some(t);
        msg
    }

    fn set_phase(&mut self, p: Phase) {
        if p != self.phase {
            self.phase = p;
            self.t_phase = 0.0;
            if p != Phase::Off {
                self.events.push((Level::Info, format!("RAVITAILLEMENT : {}", p.name())));
            }
        }
    }

    /// Position relative de l'avion dans le repère du ravitailleur (en arrière, à droite, au-dessus).
    pub fn rel(&self, ac: &Aircraft) -> Option<(f64, f64, f64)> {
        let t = self.tanker.as_ref()?;
        let (e, n) = enu_offset(t.lat, t.lon, ac.lat, ac.lon);
        let h = t.hdg.to_radians();
        let fwd = e * h.sin() + n * h.cos();
        let right = e * h.cos() - n * h.sin();
        Some((-fwd, right, ac.alt - t.alt))
    }

    pub fn start(&mut self, ac: &mut Aircraft) -> Result<String, String> {
        if ac.on_ground {
            return Err("AU SOL".into());
        }
        if ac.d.mach > 1.5 {
            return Err("TROP RAPIDE : DECELERER SOUS MACH 1.5 (DESCENTE) PUIS RAVITAILLEMENT".into());
        }
        if self.tanker.is_none() {
            let m = self.spawn(ac, None);
            self.events.push((Level::Info, m));
        }
        if ac.fuel >= FUEL_MAX - 50.0 && ac.h2 >= H2_MAX - 50.0 {
            return Err("RESERVOIRS JP-7 ET H2 DEJA PLEINS".into());
        }
        ac.autoland = None;
        ac.ap.on = true;
        ac.ap.at_on = true;
        ac.ap.lat = LatMode::Hdg;
        ac.ap.vert = VertMode::Alt;
        self.set_phase(Phase::Rendezvous);
        Ok(format!("RAVITAILLEMENT AUTONOME — RENDEZ-VOUS AVEC {}", self.tanker.as_ref().unwrap().callsign))
    }

    pub fn stop(&mut self, ac: &mut Aircraft, why: &str) {
        if matches!(self.phase, Phase::Precontact | Phase::Contact) {
            self.events.push((Level::Caution, format!("RAVITAILLEMENT : DECONNEXION — {why}")));
            self.release(ac);
            self.set_phase(Phase::Disconnect);
        } else {
            self.set_phase(Phase::Off);
        }
    }

    /// Libère le maintien de position : l'autopilote reprend légèrement en retrait.
    fn release(&mut self, ac: &mut Aircraft) {
        self.glide = None;
        if let Some(t) = &self.tanker {
            ac.ap.on = true;
            ac.ap.at_on = true;
            ac.ap.lat = LatMode::Hdg;
            ac.ap.vert = VertMode::Alt;
            ac.ap.hdg_tgt = t.hdg;
            ac.ap.alt_tgt_ft = ((t.alt - 300.0) / FT).round();
            ac.ap.spd_keas = Some(TANKER_KEAS - 25.0);
            ac.ap.spd_managed = false;
        }
    }

    /// Mise à jour après le pas physique : ravitailleur, logique de rendez-vous, maintien de position.
    pub fn update(&mut self, dt: f64, ac: &mut Aircraft) {
        let Some(t) = self.tanker.as_mut() else { return };
        t.update(dt);
        self.t_phase += dt;
        if ac.crashed.is_some() {
            self.phase = Phase::Off;
            return;
        }
        let (aft, right, up) = self.rel(ac).unwrap();
        let t = self.tanker.as_ref().unwrap();
        let (thdg, talt, ttas) = (t.hdg, t.alt, t.tas);
        let dist = (aft * aft + right * right).sqrt();
        // Reprise en main (X) ou AP coupé pendant l'approche : abandon
        if matches!(self.phase, Phase::Rendezvous | Phase::Join) && !ac.ap.on {
            self.events.push((Level::Caution, "RAVITAILLEMENT ABANDONNE — AP DECONNECTE".into()));
            self.set_phase(Phase::Off);
            return;
        }
        // Aérofreins pendant le rapprochement si la vitesse dépasse nettement la consigne
        if matches!(self.phase, Phase::Rendezvous | Phase::Join) {
            if let Some(k) = ac.ap.spd_keas {
                ac.speedbrake = ac.d.keas > k + 12.0 || (ac.speedbrake && ac.d.keas > k + 3.0);
            }
        } else if matches!(self.phase, Phase::Precontact | Phase::Contact) {
            ac.speedbrake = false;
        }
        match self.phase {
            Phase::Off => {}
            Phase::Rendezvous => {
                // Poursuite d'un point 3 km derrière le ravitailleur, rattrapage à +150 kt
                let (pla, plo, _) = t.point(3_000.0, 0.0, 0.0);
                ac.ap.hdg_tgt = dist_brg(ac.lat, ac.lon, pla, plo).1.round();
                ac.ap.alt_tgt_ft = ((talt - 150.0) / FT).round();
                ac.ap.spd_keas = Some(TANKER_KEAS + closure_kt(aft - PRECONTACT.0, 150.0).max(-60.0));
                ac.ap.spd_managed = false;
                if dist < 6_000.0 && aft > 0.0 {
                    self.set_phase(Phase::Join);
                }
            }
            Phase::Join => {
                // Alignement sur la trace du ravitailleur (loi type LOC) et rapprochement contrôlé
                ac.ap.hdg_tgt = wrap360(thdg + (-right / 30.0).clamp(-25.0, 25.0));
                ac.ap.alt_tgt_ft = ((talt + PRECONTACT.2) / FT).round();
                let along_err = aft - PRECONTACT.0;
                // En avance (devant le ravitailleur) : on ralentit pour le laisser repasser devant
                let closure = closure_kt(along_err, 120.0).max(-60.0);
                ac.ap.spd_keas = Some(TANKER_KEAS + closure);
                let rel_v = (ac.tas - ttas) / KT;
                if along_err.abs() < 150.0 && right.abs() < 60.0 && (up - PRECONTACT.2).abs() < 45.0 && rel_v.abs() < 30.0 {
                    self.glide = Some(((aft, right, up), PRECONTACT, 0.0));
                    self.events.push((Level::Info, "RAVITAILLEMENT : CAPTURE — MAINTIEN DE POSITION AUTOMATIQUE".into()));
                    self.set_phase(Phase::Precontact);
                }
                if dist > 15_000.0 {
                    // Trop loin : reprise de la poursuite
                    self.set_phase(Phase::Rendezvous);
                }
            }
            Phase::Precontact | Phase::Contact => {
                // Glissement vers la position visée puis maintien rigoureux
                let target = if self.phase == Phase::Precontact { PRECONTACT } else { CONTACT };
                let (from, to, k) = self.glide.unwrap_or(((aft, right, up), target, 1.0));
                let k = (k + dt / if self.phase == Phase::Precontact { 6.0 } else { 9.0 }).min(1.0);
                self.glide = Some((from, to, k));
                let sk = k * k * (3.0 - 2.0 * k);
                let p = (from.0 + (to.0 - from.0) * sk, from.1 + (to.1 - from.1) * sk, from.2 + (to.2 - from.2) * sk);
                let t = self.tanker.as_ref().unwrap();
                let (la, lo, al) = t.point(p.0, p.1, p.2);
                ac.lat = la;
                ac.lon = lo;
                ac.alt = al;
                ac.psi = thdg.to_radians();
                ac.tas = ttas;
                ac.gamma = 0.0;
                ac.phi = t.bank.to_radians();
                ac.ap.hdg_tgt = thdg;
                ac.ap.alt_tgt_ft = (al / FT).round();
                ac.ap.spd_keas = Some(TANKER_KEAS);
                if self.phase == Phase::Precontact && k >= 1.0 && self.t_phase > 9.0 {
                    self.glide = Some((PRECONTACT, CONTACT, 0.0));
                    self.set_phase(Phase::Contact);
                    self.events.push((Level::Info, "PERCHE : CONTACT — TRANSFERT JP-7 + H2 EN COURS".into()));
                }
                if self.phase == Phase::Contact && k >= 1.0 {
                    let t = self.tanker.as_mut().unwrap();
                    // JP-7 et hydrogène liquide en parallèle (deux circuits dans la perche)
                    let q = (RATE * dt).min((FUEL_MAX - ac.fuel).max(0.0)).min(t.offload);
                    ac.fuel += q;
                    t.offload -= q;
                    self.given += q;
                    let qh = (RATE_H2 * dt).min((H2_MAX - ac.h2).max(0.0)).min(t.offload_h2);
                    ac.h2 += qh;
                    t.offload_h2 -= qh;
                    self.given_h2 += qh;
                    let jp_done = ac.fuel >= FUEL_MAX - 1.0 || t.offload <= 0.0;
                    let h2_done = ac.h2 >= H2_MAX - 1.0 || t.offload_h2 <= 0.0;
                    if jp_done && h2_done {
                        let why = if t.offload <= 0.0 || t.offload_h2 <= 0.0 { "RAVITAILLEUR VIDE" } else { "RESERVOIRS PLEINS" };
                        self.events.push((Level::Info, format!("RAVITAILLEMENT TERMINE : JP-7 +{:.1} t, H2 +{:.1} t ({why})", self.given / 1000.0, self.given_h2 / 1000.0)));
                        self.release(ac);
                        self.set_phase(Phase::Disconnect);
                    }
                }
            }
            Phase::Disconnect => {
                if self.t_phase > 25.0 || dist > 1_500.0 {
                    self.events.push((Level::Info, "DEGAGEMENT TERMINE — REPRENDRE LA NAVIGATION".into()));
                    self.set_phase(Phase::Off);
                }
            }
        }
    }

    pub fn command(&mut self, w: &[&str], ac: &mut Aircraft) -> Result<String, String> {
        let arg = |i: usize| w.get(i).copied().unwrap_or("");
        match w[0] {
            "TANKER" | "RAVITAILLEUR" => match arg(1) {
                "OFF" => {
                    self.stop(ac, "RAVITAILLEUR RENVOYE");
                    self.tanker = None;
                    self.phase = Phase::Off;
                    Ok("RAVITAILLEUR RENVOYE".into())
                }
                "" => Ok(self.spawn(ac, None)),
                icao => match AIRPORTS.iter().position(|a| a.icao == icao) {
                    Some(i) => Ok(self.spawn(ac, Some(i))),
                    None => Err("TANKER [OACI|OFF]".into()),
                },
            },
            "DECROCHER" | "DISCONNECT" => {
                self.stop(ac, "DEMANDE GCS");
                Ok("DECONNEXION".into())
            }
            _ => {
                if arg(1) == "OFF" {
                    self.stop(ac, "DEMANDE GCS");
                    Ok("RAVITAILLEMENT ARRETE".into())
                } else {
                    self.start(ac)
                }
            }
        }
    }

    pub fn active(&self) -> bool {
        self.phase != Phase::Off
    }
}

// ---------- Rendu ----------

/// Ravitailleur 3D (fuselage, ailes en flèche, empennage, réacteurs) et perche rigide.
pub fn draw_tanker(rf: &Refuel, ac: &Aircraft, mb: &mut MeshBuilder) {
    let Some(t) = &rf.tanker else { return };
    let eye = EyeFrame::new(ac);
    let (d, _) = dist_brg(ac.lat, ac.lon, t.lat, t.lon);
    if d > 40_000.0 {
        return;
    }
    let h = t.hdg.to_radians();
    let bank = t.bank.to_radians();
    // Repère : x avant, y droite, z haut (inclinaison autour de l'axe avant)
    let base = eye.local(t.lat, t.lon, t.alt);
    let fwd = vec3(h.sin() as f32, 0.0, -h.cos() as f32);
    let right0 = vec3(h.cos() as f32, 0.0, h.sin() as f32);
    let up0 = vec3(0.0, 1.0, 0.0);
    let (sb, cb) = (bank.sin() as f32, bank.cos() as f32);
    let right = right0 * cb - up0 * sb;
    let up = up0 * cb + right0 * sb;
    let p = |x: f32, y: f32, z: f32| base + fwd * x + right * y + up * z;
    let grey = [168, 172, 178, 255];
    let dark = [120, 124, 130, 255];
    let light = [200, 204, 210, 255];
    // Fuselage (caisson octogonal approché par 4 faces)
    let (l0, l1, rr) = (-24.0f32, 24.0f32, 2.6f32);
    mb.quad([p(l0, -rr, rr), p(l1, -rr, rr), p(l1, rr, rr), p(l0, rr, rr)], light);
    mb.quad([p(l0, -rr, -rr), p(l1, -rr, -rr), p(l1, -rr, rr), p(l0, -rr, rr)], grey);
    mb.quad([p(l0, rr, -rr), p(l1, rr, -rr), p(l1, rr, rr), p(l0, rr, rr)], grey);
    mb.quad([p(l0, -rr, -rr), p(l1, -rr, -rr), p(l1, rr, -rr), p(l0, rr, -rr)], dark);
    // Nez et cône arrière
    mb.quad([p(l1, -rr, rr), p(l1 + 5.0, -0.5, 0.5), p(l1 + 5.0, 0.5, 0.5), p(l1, rr, rr)], light);
    mb.quad([p(l1, -rr, -rr), p(l1 + 5.0, -0.5, 0.0), p(l1 + 5.0, 0.5, 0.0), p(l1, rr, -rr)], dark);
    // Ailes en flèche
    for s in [-1.0f32, 1.0] {
        mb.quad([p(4.0, s * rr, 0.0), p(-8.0, s * 24.0, 1.5), p(-12.0, s * 24.0, 1.5), p(-6.0, s * rr, 0.0)], grey);
        // Réacteurs sous l'aile
        let (ex, ey) = (-1.0, s * 9.0);
        mb.quad([p(ex + 3.0, ey - 1.2, -2.2), p(ex - 3.0, ey - 1.2, -2.2), p(ex - 3.0, ey + 1.2, -2.2), p(ex + 3.0, ey + 1.2, -2.2)], dark);
        mb.quad([p(ex + 3.0, ey - 1.2, -0.4), p(ex - 3.0, ey - 1.2, -0.4), p(ex - 3.0, ey - 1.2, -2.2), p(ex + 3.0, ey - 1.2, -2.2)], grey);
        // Empennage horizontal
        mb.quad([p(-18.0, s * rr, 1.0), p(-23.0, s * 9.0, 1.8), p(-25.0, s * 9.0, 1.8), p(-23.0, s * rr, 1.0)], grey);
    }
    // Dérive
    mb.quad([p(-16.0, 0.0, rr), p(-21.0, 0.0, rr + 9.0), p(-24.5, 0.0, rr + 9.0), p(-24.0, 0.0, rr)], grey);
    // Perche : de la queue vers le réceptacle (avion) en contact, sinon rangée à 30° sous l'horizontale
    let root = p(-22.0, 0.0, -rr);
    let tip = if rf.phase == Phase::Contact {
        // Réceptacle dorsal, juste derrière le poste
        let nose = vec3(0.0, 0.8, 0.0) + vec3(ac.psi.sin() as f32, 0.0, -ac.psi.cos() as f32) * 4.0;
        nose
    } else {
        p(-22.0 - 14.0, 0.0, -rr - 8.0)
    };
    let ax = (tip - root).normalize_or_zero();
    let side = ax.cross(up).normalize_or_zero() * 0.35;
    mb.quad([root - side, tip - side, tip + side, root + side], [60, 62, 66, 255]);
    // Ailerons de la perche
    let mid = root + (tip - root) * 0.75;
    mb.quad([mid - right * 1.6, mid + right * 1.6, mid + right * 1.6 + ax * 1.2, mid - right * 1.6 + ax * 1.2], [80, 82, 86, 255]);
}

/// Repère et directeur de ravitaillement sur l'horizon (HUD).
pub fn draw_hud(rf: &Refuel, ac: &Aircraft, s: Rect, time: f64) {
    let Some(t) = &rf.tanker else { return };
    let Some((aft, right, up)) = rf.rel(ac) else { return };
    if !rf.active() {
        return;
    }
    let cx = s.x + s.w / 2.0;
    let y0 = s.y + s.h * 0.62;
    let shadow = |m: &str, x: f32, y: f32, sz: f32, c: Color| {
        txt_c(m, x + 1.0, y + 1.0, sz, Color::new(0.0, 0.0, 0.0, 0.7));
        txt_c(m, x, y, sz, c);
    };
    let ph = rf.phase;
    let col = match ph {
        Phase::Contact => GREEN_S,
        Phase::Precontact => CYAN_S,
        Phase::Disconnect => AMBER,
        _ => WHITE,
    };
    shadow(&format!("AAR {}  {}", ph.name(), t.callsign), cx, y0, 15.0, col);
    let target = if ph == Phase::Contact { CONTACT } else { PRECONTACT };
    let (da, dr, du) = (aft - target.0, right - target.1, up - target.2);
    let dist = (aft * aft + right * right).sqrt();
    if matches!(ph, Phase::Rendezvous) {
        shadow(&format!("{:.1} NM  ECART ALT {:+.0} ft  V REL {:+.0} kt", dist / NM, up / FT, (ac.tas - t.tas) / KT), cx, y0 + 18.0, 12.0, WHITE);
    } else {
        shadow(&format!("AV/AR {:+.0} m   G/D {:+.0} m   H/B {:+.0} m", -da, dr, du), cx, y0 + 18.0, 12.0, WHITE);
    }
    if ph == Phase::Contact {
        // Deux jauges : JP-7 (vert) et hydrogène liquide (cyan)
        for (k, (lbl, f, c)) in [("JP-7", ac.fuel / FUEL_MAX, Color::new(0.3, 1.0, 0.45, 0.8)), ("H2", ac.h2 / H2_MAX, Color::new(0.35, 0.85, 1.0, 0.85))].into_iter().enumerate() {
            let b = Rect::new(cx - 80.0, y0 + 26.0 + k as f32 * 15.0, 160.0, 10.0);
            txt_r(lbl, b.x - 6.0, b.y + 9.0, 11.0, c);
            draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, c);
            draw_rectangle(b.x + 1.0, b.y + 1.0, (b.w - 2.0) * f as f32, b.h - 2.0, c);
        }
        let blink = (time * 2.0).fract() < 0.7;
        if blink {
            shadow(&format!("TRANSFERT JP-7 +{:.1} t   H2 +{:.1} t", rf.given / 1000.0, rf.given_h2 / 1000.0), cx, y0 + 68.0, 13.0, GREEN_S);
        }
    }
    // Directeur de perche : feux UP/DN et FWD/AFT (comme les feux directeurs sous le ravitailleur)
    if matches!(ph, Phase::Join | Phase::Precontact | Phase::Contact) {
        let lights = |x: f32, lbl: [&str; 3], v: f64, tol: f64| {
            for (k, l) in lbl.iter().enumerate() {
                let on = match k {
                    0 => v > tol,
                    1 => v.abs() <= tol,
                    _ => v < -tol,
                };
                let c = if k == 1 { GREEN_S } else { AMBER };
                let y = s.y + s.h * 0.30 + k as f32 * 18.0;
                draw_rectangle(x - 22.0, y - 12.0, 44.0, 15.0, Color::new(0.0, 0.0, 0.0, 0.45));
                txt_c(l, x, y, 11.0, if on { c } else { Color::new(0.4, 0.4, 0.4, 0.6) });
            }
        };
        lights(s.x + s.w * 0.30, ["DOWN", "ALT", "UP"], du, 3.0);
        lights(s.x + s.w * 0.70, ["AFT", "POS", "FWD"], da, 4.0);
    }
}

/// Ravitailleur et hippodrome sur la carte.
pub fn draw_map(rf: &Refuel, to_s: &dyn Fn(f64, f64) -> Vec2, r: Rect) {
    let Some(t) = &rf.tanker else { return };
    let pts: Vec<Vec2> = t.racetrack().iter().map(|&(a, b)| to_s(a, b)).collect();
    for w in pts.windows(2) {
        if (w[0].x - w[1].x).abs() < r.w {
            draw_line(w[0].x, w[0].y, w[1].x, w[1].y, 1.0, Color::new(0.4, 0.8, 1.0, 0.5));
        }
    }
    let p = to_s(t.lat, t.lon);
    let h = (t.hdg as f32).to_radians();
    let f = vec2(h.sin(), -h.cos());
    let sd = vec2(-f.y, f.x);
    let c = Color::new(0.4, 0.85, 1.0, 1.0);
    draw_triangle(p + f * 10.0, p - f * 6.0 + sd * 7.0, p - f * 6.0 - sd * 7.0, c);
    txt(&format!("{} FL{:.0} {:.0}t + H2 {:.0}t", t.callsign, t.alt / FT / 100.0, t.offload / 1000.0, t.offload_h2 / 1000.0), p.x + 10.0, p.y - 6.0, 12.0, c);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ravitaillement complet depuis Mach 0,75 et Mach 0,95 (sans dépassement du ravitailleur).
    #[test]
    fn autonomous_refuel() {
        for m in [0.75, 0.95] {
            refuel_from(m);
        }
    }

    fn refuel_from(mach: f64) {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.alt = 24_000.0 * FT;
        ac.tas = mach * crate::atmosphere::isa(ac.alt).a;
        ac.psi = 0.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.6;
        ac.fuel = 12_000.0;
        ac.h2 = 4_000.0;
        let mut rf = Refuel::new();
        rf.spawn(&ac, None);
        rf.start(&mut ac).expect("start");
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        let mut log = Vec::new();
        let mut min_aft = f64::MAX;
        while t < 3_600.0 && rf.active() {
            if rf.phase == Phase::Join {
                min_aft = min_aft.min(rf.rel(&ac).unwrap().0);
            }
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            rf.update(dt, &mut ac);
            for (_, m) in rf.events.drain(..) {
                log.push(format!("{t:.0}s {m}"));
            }
            assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
            t += dt;
        }
        println!("--- depart M{mach} : plus proche en jonction {min_aft:.0} m derriere");
        for l in &log {
            println!("{l}");
        }
        assert!(min_aft > 0.0, "depassement du ravitailleur");
        assert!(rf.given > 20_000.0, "transfert {:.0} kg", rf.given);
        assert!(ac.fuel > FUEL_MAX - 100.0);
        assert!(ac.h2 > H2_MAX - 100.0, "H2 {:.0} kg", ac.h2);
    }
}
