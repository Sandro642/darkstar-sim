//! Missiles et contre-mesures.
//! - Missiles sol-air simulés : propulsion puis vol balistique freiné, navigation à poursuite
//!   anticipée, facteur de charge disponible qui s'effondre avec l'altitude, rayon létal, échecs
//!   (leurré, à court d'énergie, passé à côté).
//! - Contre-mesures (CMDS) : paillettes contre les autodirecteurs radar (plus efficaces quand le
//!   missile est par le travers : « notch »), leurres infrarouges, brouilleur ECM, mode automatique.
//! - Missile hypersonique HACM contre un site radar localisé par l'ELINT : impact sur la position
//!   estimée, le site n'est détruit que si l'estimation est assez précise.

use crate::aircraft::*;
use crate::atmosphere::isa;
use crate::geo::*;
use crate::recon::{EmKind, Emitter};
use crate::ui::*;
use macroquad::prelude::*;

const LETHAL_M: f64 = 40.0;

/// Horloge murale (s) : indépendante du contexte graphique (tests sans fenêtre).
fn now_s() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
const HACM_MACH: f64 = 7.0;
const HACM_RANGE: f64 = 600_000.0;
const HACM_KILL_M: f64 = 400.0;
/// Rayon dans lequel les leurres agissent (m).
const CM_RANGE: f64 = 18_000.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Seeker {
    Radar,
    Ir,
}

pub struct Missile {
    pub site: usize,
    pub name: &'static str,
    pub seeker: Seeker,
    /// Origine du repère local (site de lancement).
    o: (f64, f64),
    /// Position (est, nord, altitude) et vitesse (m/s).
    p: DVec3,
    v: DVec3,
    t: f64,
    burn: f64,
    accel: f64,
    vmax: f64,
    gmax: f64,
    max_t: f64,
    ceiling: f64,
    /// Point leurre visé (paillettes / leurre IR) au lieu de l'avion.
    decoy: Option<DVec3>,
    best_r: f64,
    pub range: f64,
    pub tti: f64,
}

impl Missile {
    pub fn latlon(&self) -> (f64, f64, f64) {
        let (la, lo) = from_enu(self.o.0, self.o.1, self.p.x, self.p.y);
        (la, lo, self.p.z)
    }
    pub fn speed(&self) -> f64 {
        self.v.length()
    }
}

pub struct Hacm {
    pub target: usize,
    pub name: &'static str,
    /// Point d'impact visé (position estimée par l'ELINT) et position réelle de l'émetteur.
    aim: (f64, f64),
    truth: (f64, f64),
    from: (f64, f64, f64),
    dist: f64,
    pub t: f64,
    pub tof: f64,
}

impl Hacm {
    pub fn latlon(&self) -> (f64, f64, f64) {
        let f = (self.t / self.tof).clamp(0.0, 1.0);
        let (_, b) = dist_brg(self.from.0, self.from.1, self.aim.0, self.aim.1);
        let (la, lo) = destination(self.from.0, self.from.1, b, self.dist * f);
        // Profil : montée vers 30 km puis piqué terminal
        let alt = self.from.2 + (30_000.0 - self.from.2) * (4.0 * f * (1.0 - f)).min(1.0) * (1.0 - f).sqrt().min(1.0);
        (la, lo, alt.max(0.0))
    }
}

pub struct Combat {
    pub missiles: Vec<Missile>,
    pub hacms: Vec<Hacm>,
    pub chaff: u32,
    pub flares: u32,
    pub hacm_left: u32,
    pub ecm: bool,
    pub cmds_auto: bool,
    auto_t: f64,
    pub flash: Option<(String, f64)>,
    pub events: Vec<(Level, String)>,
    /// Missiles terminés (sites) à signaler à la reconnaissance.
    pub done: Vec<usize>,
    /// Sites détruits par HACM.
    pub killed: Vec<usize>,
    rng: u64,
}

fn ac_pos(o: (f64, f64), ac: &Aircraft) -> (DVec3, DVec3) {
    let (e, n) = enu_offset(o.0, o.1, ac.lat, ac.lon);
    let (ve, vn) = ac.ground_vel();
    (dvec3(e, n, ac.alt), dvec3(ve, vn, ac.tas * ac.gamma.sin()))
}

impl Combat {
    pub fn new() -> Combat {
        Combat {
            missiles: Vec::new(),
            hacms: Vec::new(),
            chaff: 60,
            flares: 60,
            hacm_left: 4,
            ecm: false,
            cmds_auto: true,
            auto_t: 0.0,
            flash: None,
            events: Vec::new(),
            done: Vec::new(),
            killed: Vec::new(),
            rng: 0xA076_1D64_78BD_642F,
        }
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Tir d'un site : caractéristiques dérivées de son domaine d'engagement.
    pub fn launch(&mut self, k: usize, em: &Emitter, ac: &Aircraft) {
        let ir = em.name.contains("MOBILE");
        let a = isa(20_000.0).a;
        let vmax = (em.mach_max + 1.0) * a;
        let burn = if ir { 4.0 } else if em.range > 100_000.0 { 10.0 } else { 6.0 };
        let o = (em.lat, em.lon);
        let (tp, _) = ac_pos(o, ac);
        // Départ vertical incliné vers la cible
        let dir = dvec3(tp.x, tp.y, 0.0).normalize_or_zero() * 0.5 + dvec3(0.0, 0.0, 0.87);
        self.missiles.push(Missile {
            site: k,
            name: em.name,
            seeker: if ir { Seeker::Ir } else { Seeker::Radar },
            o,
            p: dvec3(0.0, 0.0, 300.0),
            v: dir.normalize() * 50.0,
            t: 0.0,
            burn,
            accel: vmax / burn,
            vmax,
            gmax: if ir { 40.0 } else { 30.0 },
            max_t: if ir { 35.0 } else if em.range > 100_000.0 { 110.0 } else { 70.0 },
            ceiling: em.ceiling_ft * FT + 4_000.0,
            decoy: None,
            best_r: f64::MAX,
            range: tp.length(),
            tti: 0.0,
        });
        self.events.push((Level::Warning, format!("MISSILE LAUNCH — {} ({}) — {:.0} NM", em.name, if ir { "IR" } else { "RADAR" }, tp.length() / NM)));
    }

    /// Largage manuel ou automatique de leurres.
    pub fn dispense(&mut self, kind: Seeker, ac: &Aircraft) -> Result<String, String> {
        let left = match kind {
            Seeker::Radar => &mut self.chaff,
            Seeker::Ir => &mut self.flares,
        };
        if *left < 2 {
            return Err(if kind == Seeker::Radar { "PAILLETTES EPUISEES".into() } else { "LEURRES IR EPUISES".into() });
        }
        *left -= 2;
        let name = if kind == Seeker::Radar { "CHAFF" } else { "FLARE" };
        self.flash = Some((name.into(), now_s() + 1.2));
        // Chaque missile proche du bon type peut être leurré
        let mut msgs = Vec::new();
        for i in 0..self.missiles.len() {
            let (o, p, seeker, decoyed) = (self.missiles[i].o, self.missiles[i].p, self.missiles[i].seeker, self.missiles[i].decoy.is_some());
            if seeker != kind || decoyed {
                continue;
            }
            let (tp, tv) = ac_pos(o, ac);
            let los = tp - p;
            if los.length() > CM_RANGE {
                continue;
            }
            // Effet « notch » : un missile radar perd l'avion s'il voit peu de vitesse radiale
            let radial = if tv.length() > 1.0 { (tv.normalize().dot(los.normalize())).abs() } else { 1.0 };
            let beam = radial < 0.35;
            let mut p_decoy = match kind {
                Seeker::Radar => 0.16 + if beam { 0.30 } else { 0.0 },
                Seeker::Ir => 0.30,
            };
            if self.ecm && kind == Seeker::Radar {
                p_decoy += 0.08;
            }
            let roll = self.rand();
            if roll < p_decoy {
                // Le missile se reporte sur le nuage de leurres, qui reste en arrière
                self.missiles[i].decoy = Some(tp - tv * 1.5);
                msgs.push(format!("MISSILE {} LEURRE ({})", self.missiles[i].name, name));
            }
        }
        for m in msgs {
            self.events.push((Level::Info, m));
        }
        Ok(format!("{name} — RESTE {}", if kind == Seeker::Radar { self.chaff } else { self.flares }))
    }

    /// Tir HACM sur un site localisé par l'ELINT.
    pub fn fire_hacm(&mut self, k: usize, em: &Emitter, ac: &Aircraft) -> Result<String, String> {
        if self.hacm_left == 0 {
            return Err("PLUS DE HACM A BORD".into());
        }
        if em.destroyed {
            return Err("SITE DEJA DETRUIT".into());
        }
        let Some((ela, elo, err)) = em.est else {
            return Err("SITE NON LOCALISE (ELINT)".into());
        };
        if ac.alt < 30_000.0 * FT || ac.d.mach < 2.0 {
            return Err("CONDITIONS DE TIR : > 30 000 FT ET > MACH 2".into());
        }
        let (d, _) = dist_brg(ac.lat, ac.lon, ela, elo);
        if d > HACM_RANGE {
            return Err(format!("HORS DE PORTEE ({:.0} NM > {:.0} NM)", d / NM, HACM_RANGE / NM));
        }
        let v = HACM_MACH * isa(25_000.0).a;
        self.hacm_left -= 1;
        self.hacms.push(Hacm { target: k, name: em.name, aim: (ela, elo), truth: (em.lat, em.lon), from: (ac.lat, ac.lon, ac.alt), dist: d, t: 0.0, tof: d / v + 20.0 });
        self.flash = Some(("HACM AWAY".into(), now_s() + 2.0));
        Ok(format!("HACM TIRE SUR {} — {:.0} NM, IMPACT DANS {} (precision ELINT {:.0} m)", em.name, d / NM, crate::mission::fmt_time(d / v + 20.0), err))
    }

    pub fn update(&mut self, dt: f64, ac: &mut Aircraft) {
        // Sous-pas pour la stabilité de la navigation des missiles
        let n = (dt / 0.05).ceil().max(1.0) as usize;
        let h = dt / n as f64;
        for _ in 0..n {
            self.step(h, ac);
            if ac.crashed.is_some() {
                break;
            }
        }
        // Programme automatique : leurres adaptés au missile le plus menaçant
        self.auto_t -= dt;
        if self.cmds_auto && ac.crashed.is_none() && self.auto_t <= 0.0 {
            let threat = self.missiles.iter().filter(|m| m.decoy.is_none() && m.range < CM_RANGE && m.tti > 0.0).min_by(|a, b| a.tti.total_cmp(&b.tti)).map(|m| m.seeker);
            if let Some(k) = threat {
                let _ = self.dispense(k, ac);
                self.auto_t = 2.5;
            }
        }
        // HACM
        let mut i = 0;
        while i < self.hacms.len() {
            self.hacms[i].t += dt;
            if self.hacms[i].t >= self.hacms[i].tof {
                let hm = self.hacms.remove(i);
                let miss = dist_brg(hm.aim.0, hm.aim.1, hm.truth.0, hm.truth.1).0;
                if miss < HACM_KILL_M {
                    self.killed.push(hm.target);
                    self.events.push((Level::Info, format!("HACM : IMPACT — {} DETRUIT (ecart {:.0} m)", hm.name, miss)));
                } else {
                    self.events.push((Level::Caution, format!("HACM : IMPACT A {:.0} m — {} INTACT (affiner la localisation ELINT)", miss, hm.name)));
                }
            } else {
                i += 1;
            }
        }
    }

    fn step(&mut self, h: f64, ac: &mut Aircraft) {
        let ecm = self.ecm;
        let mut j = 0;
        while j < self.missiles.len() {
            let noise = (self.rand() - 0.5, self.rand() - 0.5);
            let m = &mut self.missiles[j];
            m.t += h;
            let (tp, tv) = ac_pos(m.o, ac);
            // Propulsion puis freinage aérodynamique
            let rho = isa(m.p.z.max(0.0)).rho / crate::atmosphere::RHO0;
            let mut spd = m.v.length();
            if m.t < m.burn {
                spd = (spd + m.accel * h).min(m.vmax);
            } else {
                spd -= (1.6e-5 * rho * spd * spd + 3.0) * h;
            }
            // Visée : poursuite anticipée de l'avion (ou du nuage de leurres), bruitée par l'ECM
            let aim = match m.decoy {
                Some(d) => d,
                None => {
                    let r = (tp - m.p).length();
                    let closing = spd.max(200.0);
                    let tgo = (r / closing).min(30.0);
                    let jam = if ecm && m.seeker == Seeker::Radar { 260.0 } else { 60.0 };
                    tp + tv * tgo + dvec3(noise.0 * jam, noise.1 * jam, 0.0)
                }
            };
            let want = (aim - m.p).normalize_or_zero();
            let dir = m.v.normalize_or_zero();
            // Facteur de charge disponible : chute avec la densité de l'air
            let g_av = m.gmax * (rho * 3.0).clamp(0.04, 1.0) * if m.t < m.burn { 1.0 } else { 0.8 };
            let max_turn = g_av * 9.81 / spd.max(50.0) * h;
            let ang = dir.dot(want).clamp(-1.0, 1.0).acos();
            let new_dir = if ang <= max_turn || ang < 1e-6 {
                want
            } else {
                let axis = dir.cross(want).normalize_or_zero();
                // Rotation de Rodrigues autour de l'axe de virage
                let (s, c) = max_turn.sin_cos();
                (dir * c + axis.cross(dir) * s + axis * axis.dot(dir) * (1.0 - c)).normalize()
            };
            // Gravité sur la trajectoire balistique
            m.v = new_dir * spd + dvec3(0.0, 0.0, -9.81 * h * (1.0 - (m.t < m.burn) as i32 as f64 * 0.8));
            m.p += m.v * h;
            let r = tp - m.p;
            let dist = r.length();
            let rel_v = m.v - tv;
            let closing = rel_v.dot(r.normalize_or_zero());
            m.range = dist;
            m.tti = if closing > 1.0 { dist / closing } else { -1.0 };
            m.best_r = m.best_r.min(dist);
            let mut end: Option<(Level, String)> = None;
            if m.decoy.is_none() && dist < LETHAL_M && ac.crashed.is_none() {
                ac.crash(&format!("ABATTU PAR MISSILE SOL-AIR ({})", m.name));
                end = Some((Level::Warning, format!("IMPACT MISSILE {}", m.name)));
            } else if m.decoy.is_some() && m.t > 4.0 && closing < 0.0 {
                end = Some((Level::Info, format!("MISSILE {} LEURRE — PASSE A {:.0} m", m.name, m.best_r)));
            } else if m.t > m.burn && closing < 0.0 && dist < 20_000.0 && m.best_r < 15_000.0 {
                end = Some((Level::Info, format!("MISSILE {} MANQUE — PASSE A {:.0} m", m.name, m.best_r)));
            } else if m.t > m.max_t || (m.t > m.burn && spd < 350.0) || m.p.z < 0.0 || (m.t > m.burn && m.p.z > m.ceiling && m.v.z > 0.0 && spd < 900.0) {
                end = Some((Level::Info, format!("MISSILE {} A COURT D'ENERGIE — ESQUIVE", m.name)));
            }
            if let Some(e) = end {
                let site = m.site;
                self.missiles.remove(j);
                self.done.push(site);
                self.events.push(e);
            } else {
                j += 1;
            }
        }
    }

    /// Alarmes : missile le plus proche de l'impact.
    pub fn alerts(&self) -> Vec<(Level, String)> {
        self.missiles
            .iter()
            .filter(|m| m.decoy.is_none() && m.tti > 0.0)
            .min_by(|a, b| a.tti.total_cmp(&b.tti))
            .map(|m| vec![(Level::Warning, format!("MISSILE {} IMPACT {:.0}s", m.name, m.tti))])
            .unwrap_or_default()
    }

    pub fn command(&mut self, w: &[&str], ac: &Aircraft) -> Result<String, String> {
        let arg = |i: usize| w.get(i).copied().unwrap_or("");
        match w[0] {
            "CHAFF" | "PAILLETTES" => self.dispense(Seeker::Radar, ac),
            "FLARE" | "FLARES" | "LEURRES" => self.dispense(Seeker::Ir, ac),
            "ECM" | "BROUILLEUR" => {
                self.ecm = arg(1) != "OFF";
                Ok(format!("BROUILLEUR ECM {}", if self.ecm { "ON" } else { "OFF" }))
            }
            "CMDS" => {
                self.cmds_auto = arg(1) != "MAN";
                Ok(format!("CONTRE-MESURES {}", if self.cmds_auto { "AUTO" } else { "MANUEL" }))
            }
            _ => Err("CHAFF | FLARE | ECM ON|OFF | CMDS AUTO|MAN".into()),
        }
    }
}

// ---------- Rendu ----------

/// Missiles (ennemis en rouge, HACM en cyan) et sites détruits sur la carte.
pub fn draw_map(cb: &Combat, emitters: &[Emitter], to_s: &dyn Fn(f64, f64) -> Vec2) {
    for m in &cb.missiles {
        let (la, lo, _) = m.latlon();
        let p = to_s(la, lo);
        let (sla, slo) = m.o;
        let s = to_s(sla, slo);
        draw_line(s.x, s.y, p.x, p.y, 1.0, Color::new(1.0, 0.3, 0.2, 0.5));
        draw_circle(p.x, p.y, 4.0, if m.decoy.is_some() { DIM } else { RED });
    }
    for h in &cb.hacms {
        let (la, lo, _) = h.latlon();
        let p = to_s(la, lo);
        let a = to_s(h.aim.0, h.aim.1);
        draw_line(p.x, p.y, a.x, a.y, 1.0, Color::new(0.3, 0.9, 1.0, 0.5));
        draw_circle(p.x, p.y, 4.0, CYAN_S);
    }
    for em in emitters.iter().filter(|e| e.destroyed) {
        let p = to_s(em.lat, em.lon);
        draw_line(p.x - 7.0, p.y - 7.0, p.x + 7.0, p.y + 7.0, 2.5, GREEN_S);
        draw_line(p.x - 7.0, p.y + 7.0, p.x + 7.0, p.y - 7.0, 2.5, GREEN_S);
    }
}

/// Annonce compacte des contre-mesures sous le RWR (HUD) et flash au largage.
pub fn draw_hud(cb: &Combat, s: Rect) {
    let x = s.x + 10.0;
    let y = s.y + 128.0;
    let c = if cb.chaff < 10 || cb.flares < 10 { AMBER } else { Color::new(0.75, 1.0, 0.85, 0.85) };
    let t = format!("CH {:02} FL {:02}{}{}", cb.chaff, cb.flares, if cb.ecm { " ECM" } else { "" }, if cb.cmds_auto { " A" } else { " M" });
    txt(&t, x + 1.0, y + 1.0, 12.0, Color::new(0.0, 0.0, 0.0, 0.7));
    txt(&t, x, y, 12.0, c);
    if let Some((f, until)) = &cb.flash {
        if now_s() < *until {
            let cx = s.x + s.w / 2.0;
            txt_c(f, cx + 1.0, s.y + s.h * 0.57 + 1.0, 20.0, Color::new(0.0, 0.0, 0.0, 0.7));
            txt_c(f, cx, s.y + s.h * 0.57, 20.0, if f.starts_with("HACM") { CYAN_S } else { WHITE });
        }
    }
}

/// Page ARMEMENT : contre-mesures, missiles en vol, HACM.
pub enum ArmAction {
    Chaff,
    Flare,
    Ecm,
    Auto,
    Fire(usize),
}

fn btn(r: Rect, label: &str, on: bool, c: Color) -> bool {
    rrect(r, 6.0, if on { Color::new(c.r * 0.3, c.g * 0.3, c.b * 0.3, 1.0) } else if hovered(r) { Color::new(0.11, 0.13, 0.16, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
    rrect_lines(r, 6.0, 1.2, if on { c } else { PANEL_BORDER });
    txt_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + 5.0, 13.0, if on { WHITE } else { c });
    clicked(r)
}

pub fn draw_page(cb: &Combat, emitters: &[Emitter], ac: &Aircraft, r: Rect) -> Option<ArmAction> {
    let mut out = None;
    panel(r, "ARMEMENT - CONTRE-MESURES ET MISSILES");
    let x = r.x + 12.0;
    let w = r.w - 24.0;
    let mut y = r.y + 34.0;
    txt("CONTRE-MESURES (CMDS)", x, y + 4.0, 12.0, ACCENT);
    y += 12.0;
    let bw = (w - 18.0) / 4.0;
    if btn(Rect::new(x, y, bw, 30.0), &format!("CHAFF {}", cb.chaff), false, WHITE) {
        out = Some(ArmAction::Chaff);
    }
    if btn(Rect::new(x + bw + 6.0, y, bw, 30.0), &format!("FLARE {}", cb.flares), false, AMBER) {
        out = Some(ArmAction::Flare);
    }
    if btn(Rect::new(x + 2.0 * (bw + 6.0), y, bw, 30.0), if cb.ecm { "ECM ON" } else { "ECM OFF" }, cb.ecm, CYAN_S) {
        out = Some(ArmAction::Ecm);
    }
    if btn(Rect::new(x + 3.0 * (bw + 6.0), y, bw, 30.0), if cb.cmds_auto { "PROG AUTO" } else { "MANUEL" }, cb.cmds_auto, GREEN_S) {
        out = Some(ArmAction::Auto);
    }
    y += 40.0;
    txt("Paillettes : missiles radar (efficaces par le travers). Leurres IR : missiles infrarouges.", x, y, 11.0, DIM);
    y += 14.0;
    txt("ECM : accrochage 2,2 fois plus long, guidage radar degrade. AUTO : largage sous 18 km.", x, y, 11.0, DIM);
    y += 22.0;
    txt("MISSILES EN VOL", x, y, 12.0, ACCENT);
    y += 8.0;
    if cb.missiles.is_empty() && cb.hacms.is_empty() {
        txt("aucun", x, y + 14.0, 12.0, DIM);
        y += 22.0;
    }
    for m in &cb.missiles {
        let c = if m.decoy.is_some() { DIM } else { RED };
        txt(&format!("{} {}", m.name, if m.seeker == Seeker::Ir { "IR" } else { "RADAR" }), x, y + 14.0, 12.0, c);
        let st = if m.decoy.is_some() { "LEURRE".to_string() } else if m.tti > 0.0 { format!("IMPACT {:.0}s", m.tti) } else { "S'ELOIGNE".into() };
        txt_r(&format!("{:.1} km  M{:.1}  {st}", m.range / 1000.0, m.speed() / 300.0), x + w, y + 14.0, 12.0, c);
        y += 18.0;
    }
    for h in &cb.hacms {
        txt(&format!("HACM -> {}", h.name), x, y + 14.0, 12.0, CYAN_S);
        txt_r(&format!("IMPACT DANS {}", crate::mission::fmt_time(h.tof - h.t)), x + w, y + 14.0, 12.0, CYAN_S);
        y += 18.0;
    }
    y += 12.0;
    txt(&format!("MISSILES HYPERSONIQUES HACM : {} / 4 (Mach {HACM_MACH:.0}, portee {:.0} NM)", cb.hacm_left, HACM_RANGE / NM), x, y, 12.0, ACCENT);
    y += 8.0;
    let ok_launch = ac.alt >= 30_000.0 * FT && ac.d.mach >= 2.0;
    txt(if ok_launch { "Conditions de tir reunies" } else { "Tir : > 30 000 ft et > Mach 2" }, x, y + 14.0, 11.0, if ok_launch { GREEN_S } else { AMBER });
    y += 22.0;
    for (k, em) in emitters.iter().enumerate().filter(|(_, e)| e.heard && e.kind != EmKind::Search || e.destroyed) {
        let row = Rect::new(x, y, w, 26.0);
        rrect(row, 5.0, Color::new(0.06, 0.07, 0.09, 1.0));
        let (status, c) = if em.destroyed {
            ("DETRUIT".to_string(), GREEN_S)
        } else if let Some(e) = em.est {
            (format!("LOCALISE +/- {:.0} m  {:.0} NM", e.2, dist_brg(ac.lat, ac.lon, e.0, e.1).0 / NM), if e.2 < HACM_KILL_M { GREEN_S } else { AMBER })
        } else {
            ("NON LOCALISE".into(), DIM)
        };
        txt(em.name, row.x + 8.0, row.y + 17.0, 12.0, WHITE);
        txt_r(&status, row.x + row.w - 96.0, row.y + 17.0, 11.0, c);
        let fb = Rect::new(row.x + row.w - 88.0, row.y + 3.0, 84.0, 20.0);
        if !em.destroyed && em.est.is_some() {
            rrect(fb, 4.0, if hovered(fb) { Color::new(0.3, 0.05, 0.05, 1.0) } else { Color::new(0.2, 0.03, 0.03, 1.0) });
            rrect_lines(fb, 4.0, 1.0, RED);
            txt_c("TIR HACM", fb.x + fb.w / 2.0, fb.y + 14.0, 11.0, RED);
            if clicked(fb) {
                out = Some(ArmAction::Fire(k));
            }
        }
        y += 30.0;
        if y > r.y + r.h - 30.0 {
            break;
        }
    }
    txt(&format!("Destruction si l'erreur ELINT < {HACM_KILL_M:.0} m : affiner la localisation avant de tirer."), x, r.y + r.h - 10.0, 11.0, DIM);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recon::Recon;

    fn flying(lat: f64, lon: f64, alt_ft: f64, mach: f64) -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.gear_down = false;
        ac.lat = lat;
        ac.lon = lon;
        ac.alt = alt_ft * FT;
        ac.tas = mach * isa(ac.alt).a;
        ac.d.mach = mach;
        ac.psi = 90f64.to_radians();
        ac
    }

    fn run(cb: &mut Combat, ac: &mut Aircraft, secs: f64) {
        let dt = 0.1;
        let mut t = 0.0;
        while t < secs && ac.crashed.is_none() && !cb.missiles.is_empty() {
            // L'avion avance en ligne droite
            let (la, lo) = destination(ac.lat, ac.lon, ac.hdg_deg(), ac.tas * dt);
            ac.lat = la;
            ac.lon = lo;
            cb.update(dt, ac);
            t += dt;
        }
    }

    /// Avion lent et bas, sans contre-mesures : le missile moyenne portée l'atteint.
    #[test]
    fn sam_hits_slow_low_target() {
        let rc = Recon::new();
        let em = &rc.emitters[2];
        let (la, lo) = from_enu(em.lat, em.lon, -15_000.0, 10_000.0);
        let mut ac = flying(la, lo, 20_000.0, 0.7);
        let mut cb = Combat::new();
        cb.cmds_auto = false;
        cb.launch(2, em, &ac);
        run(&mut cb, &mut ac, 120.0);
        assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("MISSILE")), "{:?}", ac.crashed);
    }

    /// À Mach 3,4 et 90 000 ft, le même missile ne peut plus manœuvrer : il manque.
    #[test]
    fn high_fast_target_survives() {
        let rc = Recon::new();
        let em = &rc.emitters[2];
        let (la, lo) = from_enu(em.lat, em.lon, -15_000.0, 10_000.0);
        let mut ac = flying(la, lo, 90_000.0, 3.4);
        let mut cb = Combat::new();
        cb.cmds_auto = false;
        cb.launch(2, em, &ac);
        run(&mut cb, &mut ac, 120.0);
        assert!(ac.crashed.is_none(), "{:?}", ac.crashed);
    }

    /// Contre-mesures automatiques : sur une série d'engagements, une part notable des missiles est leurrée.
    #[test]
    fn countermeasures_decoy_missiles() {
        let rc = Recon::new();
        let em = &rc.emitters[2];
        let mut survived = 0;
        for seed in 0..20u64 {
            let (la, lo) = from_enu(em.lat, em.lon, -15_000.0, 10_000.0);
            let mut ac = flying(la, lo, 20_000.0, 0.7);
            let mut cb = Combat::new();
            cb.rng ^= seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            cb.ecm = true;
            cb.launch(2, em, &ac);
            run(&mut cb, &mut ac, 120.0);
            if ac.crashed.is_none() {
                survived += 1;
            }
        }
        println!("survie avec ECM + CMDS auto : {survived}/20");
        assert!((6..=17).contains(&survived), "equilibrage : {survived}/20");
    }
}
