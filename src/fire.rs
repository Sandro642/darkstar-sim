//! Détection et extinction d'incendie (façon avion de chasse) : quatre zones surveillées par deux
//! boucles de détection chacune, poignées FEU (coupure carburant / hydrogène de la zone, armement
//! des bouteilles) et deux bouteilles d'agent extincteur partagées.
//!
//! Risques d'inflammation (taux par seconde) : EGT au-delà de la limite, moteur détruit en
//! rotation, parois de chambre au-delà du seuil, rupture de chambre, fuite d'hydrogène (surtout
//! peau chaude ou scramjet allumé), pompe carburant en défaut. Un feu non maîtrisé détruit la zone
//! puis se propage ; dans la soute H2 il finit en explosion.

use crate::aircraft::*;
use crate::systems::{self, Fail};
use crate::ui::*;
use macroquad::prelude::*;

pub const ZONES: [(&str, &str); 4] = [("L ENG", "MOTEUR GAUCHE"), ("R ENG", "MOTEUR DROIT"), ("CHAMB", "BAIE DES CHAMBRES"), ("H2 BAY", "SOUTE HYDROGENE")];

#[derive(Clone, Copy, Default)]
pub struct Zone {
    pub fire: bool,
    /// Durée du feu en cours (s) et température de la zone (0 froide .. 1 destruction)
    pub t: f64,
    pub heat: f64,
    /// Poignée tirée : alimentation de la zone coupée, bouteilles armées sur cette zone
    pub handle: bool,
    pub destroyed: bool,
}

#[derive(Clone)]
pub struct Fire {
    pub zones: [Zone; 4],
    /// Bouteilles 1 et 2 : pleines ?
    pub bottles: [bool; 2],
    /// Essai des voyants en cours (s restantes)
    pub test: f64,
    rng: u64,
    /// Explosion à appliquer (cause)
    pub boom: Option<String>,
    eng_failed_prev: [bool; 2],
    comb_prev: bool,
}

impl Default for Fire {
    fn default() -> Self {
        Fire { zones: [Zone::default(); 4], bottles: [true, true], test: 0.0, rng: 0x2545_F491_4F6C_DD1D, boom: None, eng_failed_prev: [false; 2], comb_prev: false }
    }
}

impl Fire {
    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn any(&self) -> bool {
        self.zones.iter().any(|z| z.fire)
    }

    /// Allume un feu (essai ou conséquence d'un incident).
    pub fn ignite(&mut self, z: usize) -> bool {
        let zone = &mut self.zones[z];
        if zone.fire || zone.destroyed {
            return false;
        }
        zone.fire = true;
        zone.t = 0.0;
        true
    }

    /// Taux d'inflammation (1/s) de chaque zone d'après l'état de l'avion.
    fn hazard(&self, ac: &Aircraft) -> [f64; 4] {
        let e = &ac.eng;
        let mut h = [2e-7; 4];
        for k in 0..2 {
            if e.side[k] == TurbState::Running {
                if e.egt > 1000.0 {
                    h[k] += 0.01 * ((e.egt - 1000.0) / 100.0).min(3.0);
                }
                if ac.fails.has(Fail::FuelPump) {
                    h[k] += 2e-4;
                }
                if e.health < 0.3 {
                    h[k] += 5e-4;
                }
            }
        }
        let over = |w: f64, caution: f64, burst: f64| ((w - caution) / (burst - caution)).max(0.0);
        if e.ram == RamState::Lit {
            h[2] += 0.004 * over(e.ram_wall, systems::RAM_WALL_CAUTION, systems::RAM_WALL_BURST).powi(2);
        }
        if e.scram == ScramState::Lit {
            h[2] += 0.004 * over(e.scram_wall, systems::SCRAM_WALL_CAUTION, systems::SCRAM_WALL_BURST).powi(2);
        }
        if ac.fails.has(Fail::H2Leak) && ac.h2 > 50.0 {
            let hot = ac.skin_temp - 273.15 > 300.0 || e.scram == ScramState::Lit;
            h[3] += if hot { 0.02 } else { 0.002 };
        }
        h
    }

    /// Une seconde de simulation (appelée avec `dt`). Renvoie les messages à afficher.
    pub fn update(&mut self, ac: &mut Aircraft, dt: f64) -> Vec<(Level, String)> {
        let mut msgs = Vec::new();
        if ac.crashed.is_some() || dt <= 0.0 {
            return msgs;
        }
        self.test = (self.test - dt).max(0.0);
        // Événements : moteur détruit en rotation, rupture de chambre
        for k in 0..2 {
            let failed = ac.eng.side[k] == TurbState::Failed;
            if failed && !self.eng_failed_prev[k] && ac.eng.n2s[k] > 0.5 && self.rand() < 0.35 && self.ignite(k) {
                msgs.push((Level::Warning, format!("FIRE {} — FEU {} (MOTEUR NON CONFINE)", ZONES[k].0, ZONES[k].1)));
            }
            self.eng_failed_prev[k] = failed;
        }
        if ac.eng.combustor_failed && !self.comb_prev && self.rand() < 0.6 && self.ignite(2) {
            msgs.push((Level::Warning, "FIRE CHAMB — FEU BAIE DES CHAMBRES (RUPTURE DE CHAMBRE)".into()));
        }
        self.comb_prev = ac.eng.combustor_failed;
        // Inflammations aléatoires selon les risques
        let hz = self.hazard(ac);
        for (z, &rate) in hz.iter().enumerate() {
            if !self.zones[z].fire && !self.zones[z].handle && self.rand() < rate * dt && self.ignite(z) {
                msgs.push((Level::Warning, format!("FIRE {} — FEU {}", ZONES[z].0, ZONES[z].1)));
            }
        }
        // Évolution des feux
        for z in 0..4 {
            let zone = self.zones[z];
            if zone.fire {
                // Alimentation coupée (poignée) : le feu se nourrit moins
                let k = if zone.handle { 0.5 } else { 1.0 };
                let zd = &mut self.zones[z];
                zd.t += dt;
                zd.heat = (zd.heat + dt / 45.0 * k).min(1.2);
                if zd.heat >= 1.0 && !zd.destroyed {
                    zd.destroyed = true;
                    match z {
                        0 | 1 => {
                            ac.eng.side[z] = TurbState::Failed;
                            ac.eng.n2s[z] = 0.0;
                            msgs.push((Level::Warning, format!("{} DETRUIT PAR LE FEU", ZONES[z].1)));
                            // Propagation vers la soute H2
                            if self.rand() < 0.5 && self.ignite(3) {
                                msgs.push((Level::Warning, "FIRE H2 BAY — LE FEU SE PROPAGE A LA SOUTE HYDROGENE".into()));
                            }
                        }
                        2 => {
                            ac.eng.combustor_failed = true;
                            ac.eng.ram = RamState::Off;
                            ac.eng.scram = ScramState::Off;
                            ac.fails.set(Fail::Hyd, true);
                            msgs.push((Level::Warning, "BAIE DES CHAMBRES DETRUITE — HYDRAULIQUE PERDUE".into()));
                        }
                        _ => {
                            self.boom = Some("EXPLOSION — FEU SOUTE HYDROGENE NON MAITRISE".into());
                        }
                    }
                }
                // Une zone détruite continue de brûler jusqu'à la cellule
                if self.zones[z].destroyed && self.zones[z].t > 90.0 && self.boom.is_none() {
                    self.boom = Some(format!("EXPLOSION — FEU {} NON MAITRISE", ZONES[z].1));
                }
            } else {
                self.zones[z].heat = (self.zones[z].heat - dt / 120.0).max(0.0);
            }
        }
        if let Some(c) = self.boom.take() {
            ac.crash(&c);
        }
        msgs
    }

    /// Poignée FEU de la zone `z` : coupe l'alimentation de la zone et arme les bouteilles.
    pub fn pull(&mut self, ac: &mut Aircraft, z: usize) -> String {
        let zone = &mut self.zones[z];
        zone.handle = !zone.handle;
        if !zone.handle {
            return format!("POIGNEE FEU {} REPOUSSEE", ZONES[z].0);
        }
        match z {
            0 | 1 => {
                if ac.eng.side[z] != TurbState::Failed {
                    ac.eng.side[z] = TurbState::Off;
                }
                ac.eng.sw[z] = EngSw::Off;
                format!("POIGNEE FEU {} TIREE — CARBURANT ET GENERATRICE COUPES, BOUTEILLES ARMEES", ZONES[z].0)
            }
            2 => {
                ac.eng.ram = RamState::Off;
                ac.eng.scram = ScramState::Off;
                "POIGNEE FEU CHAMB TIREE — STATO ET SCRAM COUPES, BOUTEILLES ARMEES".into()
            }
            _ => {
                ac.eng.scram = ScramState::Off;
                "POIGNEE FEU H2 BAY TIREE — VANNES HYDROGENE FERMEES, BOUTEILLES ARMEES".into()
            }
        }
    }

    /// Décharge de la bouteille `b` dans la zone dont la poignée est tirée.
    pub fn discharge(&mut self, b: usize) -> Result<String, String> {
        if !self.bottles[b] {
            return Err(format!("BOUTEILLE {} VIDE", b + 1));
        }
        let Some(z) = (0..4).find(|&z| self.zones[z].handle) else {
            return Err("AUCUNE POIGNEE FEU TIREE : TIRER LA POIGNEE DE LA ZONE (ARMEMENT DES BOUTEILLES)".into());
        };
        self.bottles[b] = false;
        if !self.zones[z].fire {
            return Ok(format!("BOUTEILLE {} DECHARGEE DANS {} (AUCUN FEU)", b + 1, ZONES[z].0));
        }
        // Efficacité : zone pas encore trop chaude et alimentation coupée
        let p = (0.9 - self.zones[z].heat * 0.4).max(0.3);
        if self.rand() < p {
            let zd = &mut self.zones[z];
            zd.fire = false;
            zd.t = 0.0;
            Ok(format!("BOUTEILLE {} DECHARGEE — FEU {} ETEINT", b + 1, ZONES[z].0))
        } else {
            Ok(format!("BOUTEILLE {} DECHARGEE — FEU {} PERSISTE", b + 1, ZONES[z].0))
        }
    }

    /// Alertes en cours (pour l'horizon et le bandeau).
    pub fn alerts(&self) -> Vec<(Level, String)> {
        self.zones.iter().enumerate().filter(|(_, z)| z.fire).map(|(i, _)| (Level::Warning, format!("FIRE {}", ZONES[i].0))).collect()
    }
}

pub enum FireAction {
    Handle(usize),
    Bottle(usize),
    Test,
}

/// Page INCENDIE : poignées FEU (voyant rouge dans la poignée), bouteilles, détection, températures.
pub fn draw_fire(ac: &Aircraft, r: Rect, time: f64) -> Option<FireAction> {
    let f = &ac.fire;
    let mut out = None;
    panel(r, "INCENDIE - DETECTION / EXTINCTION");
    let blink = (time * 2.5).fract() < 0.6;
    let test = f.test > 0.0;
    let n = ZONES.len() as f32;
    let x0 = r.x + 14.0;
    let w = r.w - 28.0;
    let cw = w / n;
    let hy = r.y + 44.0;
    for (i, (code, name)) in ZONES.iter().enumerate() {
        let z = f.zones[i];
        let cx = x0 + cw * (i as f32 + 0.5);
        // Poignée en T : rouge allumée en cas de feu, sortie quand tirée
        let pulled = if z.handle { 18.0 } else { 0.0 };
        let hr = Rect::new(cx - cw * 0.40, hy + pulled, cw * 0.80, 46.0);
        let lit = (z.fire && blink) || test;
        rrect(hr, 6.0, if lit { Color::new(0.85, 0.06, 0.04, 1.0) } else { Color::new(0.30, 0.05, 0.04, 1.0) });
        rrect_lines(hr, 6.0, 2.0, Color::new(0.95, 0.75, 0.2, 1.0));
        txt_c("FIRE", cx, hr.y + 20.0, 15.0, if lit { WHITE } else { Color::new(0.75, 0.45, 0.4, 1.0) });
        txt_c(code, cx, hr.y + 38.0, 12.0, if lit { WHITE } else { Color::new(0.75, 0.45, 0.4, 1.0) });
        draw_rectangle(cx - 4.0, hr.y + hr.h, 8.0, 14.0 - pulled.min(10.0), Color::new(0.5, 0.5, 0.5, 1.0));
        if clicked(hr) {
            out = Some(FireAction::Handle(i));
        }
        let ty = hy + 92.0;
        txt_c(name, cx, ty, 10.0, DIM);
        let (st, c) = if z.destroyed {
            ("DETRUIT", RED)
        } else if z.fire {
            ("FEU", RED)
        } else if z.heat > 0.05 {
            ("CHAUD", AMBER)
        } else {
            ("NORMAL", GREEN_S)
        };
        txt_c(st, cx, ty + 16.0, 13.0, c);
        txt_c(if z.handle { "POIGNEE TIREE" } else { "POIGNEE" }, cx, ty + 32.0, 10.0, if z.handle { AMBER } else { DIM });
        // Température de zone et boucles de détection A / B
        let bw = cw * 0.7;
        let b = Rect::new(cx - bw / 2.0, ty + 40.0, bw, 7.0);
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, DIM);
        draw_rectangle(b.x + 1.0, b.y + 1.0, (b.w - 2.0) * z.heat.min(1.0) as f32, b.h - 2.0, if z.heat > 0.6 { RED } else { AMBER });
        txt_c("BOUCLES A B", cx, ty + 62.0, 9.0, DIM);
        for (k, dx) in [(0, -10.0f32), (1, 10.0)] {
            let _ = k;
            draw_circle(cx + dx, ty + 72.0, 4.0, if z.fire || test { RED } else { GREEN_S });
        }
    }
    // Bouteilles
    let by = hy + 190.0;
    txt("BOUTEILLES (DECHARGE DANS LA ZONE DONT LA POIGNEE EST TIREE)", x0, by - 8.0, 11.0, DIM);
    let armed = f.zones.iter().any(|z| z.handle);
    for b in 0..2 {
        let br = Rect::new(x0 + b as f32 * 150.0, by, 136.0, 40.0);
        let full = f.bottles[b];
        rrect(br, 5.0, if !full { Color::new(0.08, 0.08, 0.08, 1.0) } else if armed { Color::new(0.35, 0.25, 0.02, 1.0) } else { Color::new(0.1, 0.12, 0.13, 1.0) });
        rrect_lines(br, 5.0, 1.5, if armed && full { AMBER } else { DIM });
        txt_c(&format!("BTL {}", b + 1), br.x + br.w / 2.0, br.y + 17.0, 13.0, if full { WHITE } else { DIM });
        txt_c(if !full { "VIDE" } else if armed { "ARMEE - DECHARGER" } else { "PLEINE" }, br.x + br.w / 2.0, br.y + 33.0, 10.0, if !full { DIM } else if armed { AMBER } else { GREEN_S });
        if clicked(br) {
            out = Some(FireAction::Bottle(b));
        }
    }
    let tr = Rect::new(x0 + 310.0, by, 110.0, 40.0);
    rrect(tr, 5.0, Color::new(0.1, 0.12, 0.13, 1.0));
    rrect_lines(tr, 5.0, 1.5, DIM);
    txt_c("FIRE TEST", tr.x + tr.w / 2.0, tr.y + 24.0, 12.0, WHITE);
    if clicked(tr) {
        out = Some(FireAction::Test);
    }
    // Procédure
    let mut y = by + 66.0;
    for l in [
        "PROCEDURE FEU : 1. POIGNEE FEU DE LA ZONE (coupe carburant / H2, arme les bouteilles)",
        "2. BTL 1 ; si FEU persiste apres 10 s : BTL 2   3. Si non maitrise : atterrir au plus vite / EJECTION",
        "Instructions : FEU L|R|CHAMB|H2 (poignee)   BTL 1|2   FEU TEST   INCENDIE L|R|CHAMB|H2 (essai)",
    ] {
        txt(l, x0, y, 11.0, DIM);
        y += 16.0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airborne() -> Aircraft {
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        ac.alt = 9_000.0;
        ac.tas = 250.0;
        ac
    }

    /// Feu de soute H2 non traité → explosion ; poignée + bouteilles → feu éteint, avion intact.
    #[test]
    fn h2_bay_fire() {
        let mut ac = airborne();
        let mut f = Fire::default();
        f.ignite(3);
        let mut t = 0.0;
        while ac.crashed.is_none() && t < 200.0 {
            f.update(&mut ac, 0.1);
            t += 0.1;
        }
        println!("sans action : {:?} a {t:.0} s", ac.crashed);
        assert!(ac.crashed.as_deref().is_some_and(|c| c.contains("EXPLOSION")));

        let mut ac = airborne();
        let mut f = Fire::default();
        f.ignite(3);
        for _ in 0..50 {
            f.update(&mut ac, 0.1);
        }
        f.pull(&mut ac, 3);
        let r1 = f.discharge(0).unwrap();
        if f.zones[3].fire {
            for _ in 0..100 {
                f.update(&mut ac, 0.1);
            }
            let _ = f.discharge(1);
        }
        println!("avec procedure : {r1} ; feu {}", f.zones[3].fire);
        assert!(!f.zones[3].fire && ac.crashed.is_none());
        assert!(f.discharge(0).is_err(), "bouteille vide");
    }

    /// Fuite H2 avec peau chaude : le risque finit par allumer un feu.
    #[test]
    fn h2_leak_ignites() {
        let mut ac = airborne();
        ac.fails.set(Fail::H2Leak, true);
        ac.skin_temp = 700.0 + 273.15;
        let mut f = Fire::default();
        let mut t = 0.0;
        while !f.zones[3].fire && t < 600.0 {
            f.update(&mut ac, 0.1);
            t += 0.1;
        }
        println!("feu soute H2 apres {t:.0} s");
        assert!(f.zones[3].fire);
    }
}
