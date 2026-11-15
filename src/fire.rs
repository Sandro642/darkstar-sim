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
    /// Temps écoulé depuis la dernière décharge de bouteille (s)
    pub since_disch: f64,
}

impl Default for Fire {
    fn default() -> Self {
        Fire { zones: [Zone::default(); 4], bottles: [true, true], test: 0.0, rng: 0x2545_F491_4F6C_DD1D, boom: None, eng_failed_prev: [false; 2], comb_prev: false, since_disch: 1e9 }
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
        // Aucun feu spontané : seulement des causes physiques ou des pannes injectées
        let mut h = [0.0; 4];
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
        // Très haute vitesse en air dense : nacelles, chambres et soute H2 surchauffées
        let x = ((ac.skin_temp - 273.15 - 1_150.0) / 100.0).max(0.0);
        if x > 0.0 {
            h[0] += 0.0012 * x * x;
            h[1] += 0.0012 * x * x;
            h[2] += 0.0018 * x * x;
            if ac.skin_temp - 273.15 > 1_200.0 && ac.h2 > 50.0 {
                h[3] += 0.0008 * x * x;
            }
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
        self.since_disch += dt;
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
        self.since_disch = 0.0;
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

/// Couleur d'état d'une zone.
fn zone_col(z: &Zone, test: bool, blink: bool) -> Color {
    if z.destroyed {
        Color::new(0.25, 0.05, 0.05, 1.0)
    } else if z.fire || test {
        if blink { Color::new(0.95, 0.12, 0.08, 1.0) } else { Color::new(0.55, 0.06, 0.04, 1.0) }
    } else if z.heat > 0.05 {
        AMBER
    } else {
        Color::new(0.15, 0.55, 0.25, 1.0)
    }
}

/// Synoptique vu de dessus : silhouette delta, nacelles (L ENG / R ENG), baie des chambres et
/// soute H2 colorées selon l'état ; clic sur une zone = sa poignée.
fn synoptic(f: &Fire, r: Rect, test: bool, blink: bool) -> Option<usize> {
    let (cx, top, h) = (r.x + r.w / 2.0, r.y + 6.0, r.h - 12.0);
    let w = (h * 0.62).min(r.w * 0.9);
    let p = |x: f32, y: f32| vec2(cx + x * w / 2.0, top + y * h);
    let line = Color::new(0.55, 0.6, 0.66, 1.0);
    let body = Color::new(0.1, 0.11, 0.13, 1.0);
    // Fuselage et aile delta
    let outline = [p(0.0, 0.0), p(0.12, 0.25), p(0.18, 0.5), p(1.0, 0.86), p(0.95, 0.95), p(0.3, 0.92), p(0.2, 1.0), p(-0.2, 1.0), p(-0.3, 0.92), p(-0.95, 0.95), p(-1.0, 0.86), p(-0.18, 0.5), p(-0.12, 0.25)];
    let c0 = p(0.0, 0.62);
    for k in 0..outline.len() {
        draw_triangle(c0, outline[k], outline[(k + 1) % outline.len()], body);
    }
    for k in 0..outline.len() {
        let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
        draw_line(a.x, a.y, b.x, b.y, 1.5, line);
    }
    // Zones : soute H2 (avant-centre), chambres (arrière-centre), nacelles L / R
    let zr = [
        Rect::new(p(-0.40, 0.62).x, p(0.0, 0.62).y, w * 0.11, h * 0.3),
        Rect::new(p(0.18, 0.62).x, p(0.0, 0.62).y, w * 0.11, h * 0.3),
        Rect::new(p(-0.13, 0.7).x, p(0.0, 0.7).y, w * 0.13, h * 0.22),
        Rect::new(p(-0.1, 0.3).x, p(0.0, 0.3).y, w * 0.1, h * 0.34),
    ];
    let mut out = None;
    for (z, zr) in zr.iter().enumerate() {
        let zone = &f.zones[z];
        let c = zone_col(zone, test, blink);
        rrect(*zr, 3.0, Color::new(c.r, c.g, c.b, 0.85));
        rrect_lines(*zr, 3.0, if hovered(*zr) { 2.0 } else { 1.0 }, if zone.handle { AMBER } else { Color::new(0.8, 0.82, 0.85, 1.0) });
        // Libellé vertical court (L / R / CH / H2) pour tenir dans la zone
        let short = ["L", "R", "CH", "H2"][z];
        txt_c(short, zr.x + zr.w / 2.0, zr.y + zr.h / 2.0 + 4.0, 10.0, WHITE);
        if zone.destroyed {
            draw_line(zr.x, zr.y, zr.x + zr.w, zr.y + zr.h, 2.0, RED);
            draw_line(zr.x + zr.w, zr.y, zr.x, zr.y + zr.h, 2.0, RED);
        }
        if clicked(*zr) {
            out = Some(z);
        }
    }
    out
}

/// Poignée FEU en T (façon chasseur) : voyant rouge dans la poignée, tige visible quand tirée.
fn t_handle(r: Rect, z: &Zone, code: &str, test: bool, blink: bool) -> bool {
    let pulled = if z.handle { r.h * 0.28 } else { 0.0 };
    // Logement
    rrect(Rect::new(r.x + r.w * 0.35, r.y + r.h * 0.55, r.w * 0.3, r.h * 0.45), 3.0, Color::new(0.04, 0.04, 0.045, 1.0));
    // Tige
    let hy = r.y + r.h * 0.06 + pulled * 0.0;
    let stem_top = hy + r.h * 0.42;
    draw_rectangle(r.x + r.w / 2.0 - 3.0, stem_top, 6.0, (r.y + r.h * 0.62 + pulled) - stem_top, Color::new(0.62, 0.64, 0.68, 1.0));
    // Barre du T (poignée), avec voyant
    let bar = Rect::new(r.x + 2.0, hy + pulled * -0.0, r.w - 4.0, r.h * 0.42);
    let bar = Rect::new(bar.x, bar.y + if z.handle { -2.0 } else { 0.0 }, bar.w, bar.h);
    let lit = (z.fire && blink) || test;
    rrect(Rect::new(bar.x + 2.0, bar.y + 3.0, bar.w, bar.h), 7.0, Color::new(0.0, 0.0, 0.0, 0.5));
    rrect(bar, 7.0, if lit { Color::new(0.9, 0.08, 0.05, 1.0) } else { Color::new(0.32, 0.05, 0.04, 1.0) });
    rrect(Rect::new(bar.x + 4.0, bar.y + 3.0, bar.w - 8.0, bar.h * 0.25), 4.0, Color::new(1.0, 1.0, 1.0, if lit { 0.22 } else { 0.07 }));
    rrect_lines(bar, 7.0, 1.5, if z.handle { AMBER } else { Color::new(0.1, 0.02, 0.02, 1.0) });
    let tc = if lit { WHITE } else { Color::new(0.8, 0.45, 0.4, 1.0) };
    let sz = (bar.h * 0.32).clamp(9.0, 16.0);
    txt_c("FIRE", bar.x + bar.w / 2.0, bar.y + bar.h * 0.45, sz, tc);
    txt_c(code, bar.x + bar.w / 2.0, bar.y + bar.h * 0.45 + sz + 1.0, sz * 0.8, tc);
    if z.handle {
        txt_c("TIREE", r.x + r.w / 2.0, r.y + r.h - 2.0, 9.0, AMBER);
    }
    clicked(bar)
}

/// Bouteille d'agent extincteur : cylindre, manomètre, bouton DISCH.
fn bottle(r: Rect, k: usize, full: bool, armed: bool) -> bool {
    let cyl = Rect::new(r.x + 10.0, r.y + 22.0, (r.w * 0.2).min(36.0), r.h - 30.0);
    let col = if full { Color::new(0.75, 0.12, 0.08, 1.0) } else { Color::new(0.25, 0.22, 0.22, 1.0) };
    rrect(cyl, cyl.w * 0.45, col);
    rrect(Rect::new(cyl.x + 3.0, cyl.y + 4.0, cyl.w * 0.25, cyl.h - 8.0), 3.0, Color::new(1.0, 1.0, 1.0, 0.12));
    draw_rectangle(cyl.x + cyl.w * 0.35, cyl.y - 6.0, cyl.w * 0.3, 7.0, Color::new(0.5, 0.52, 0.55, 1.0));
    txt_c(&format!("BTL {}", k + 1), r.x + r.w / 2.0, r.y + 12.0, 11.0, WHITE);
    // Manomètre
    let (gx, gy, gr) = (cyl.x + cyl.w + 22.0, cyl.y + 16.0, 13.0);
    draw_circle(gx, gy, gr, Color::new(0.85, 0.86, 0.84, 1.0));
    draw_circle_lines(gx, gy, gr, 1.5, Color::new(0.3, 0.3, 0.32, 1.0));
    let a = if full { -0.6f32 } else { -2.5 };
    draw_line(gx, gy, gx + (gr - 3.0) * a.cos(), gy + (gr - 3.0) * a.sin(), 2.0, BLACK);
    draw_line(gx + gr * (-0.6f32).cos(), gy + gr * (-0.6f32).sin(), gx + (gr - 4.0) * (-0.6f32).cos(), gy + (gr - 4.0) * (-0.6f32).sin(), 2.0, GREEN_S);
    txt(if full { "PLEINE" } else { "VIDE" }, gx + gr + 6.0, gy + 4.0, 10.0, if full { GREEN_S } else { DIM });
    // Bouton DISCH
    let b = Rect::new(cyl.x + cyl.w + 8.0, cyl.y + cyl.h - 26.0, r.x + r.w - (cyl.x + cyl.w + 8.0) - 4.0, 26.0);
    let ready = full && armed;
    rrect(b, 5.0, if ready { if hovered(b) { Color::new(0.55, 0.38, 0.04, 1.0) } else { Color::new(0.4, 0.28, 0.03, 1.0) } } else { Color::new(0.08, 0.085, 0.09, 1.0) });
    rrect_lines(b, 5.0, 1.5, if ready { AMBER } else { DIM });
    txt_c(if !full { "DECHARGEE" } else if armed { "DISCH" } else { "NON ARMEE" }, b.x + b.w / 2.0, b.y + 17.0, 11.0, if ready { WHITE } else { DIM });
    rrect_lines(r, 6.0, 1.0, Color::new(0.2, 0.21, 0.23, 1.0));
    clicked(b)
}

/// Page INCENDIE : synoptique des zones, procédure guidée (étapes cochées, compte à rebours),
/// poignées FEU en T, bouteilles d'agent, essai des voyants.
pub fn draw_fire(ac: &Aircraft, r: Rect, time: f64) -> Option<FireAction> {
    let f = &ac.fire;
    let mut out = None;
    panel(r, "INCENDIE - DETECTION / EXTINCTION");
    let blink = (time * 2.5).fract() < 0.6;
    let test = f.test > 0.0;
    let x0 = r.x + 12.0;
    let w0 = r.w - 24.0;
    let mut y = r.y + 30.0;
    // ----- Bandeau d'état / procédure guidée -----
    let burning: Vec<usize> = (0..4).filter(|&z| f.zones[z].fire).collect();
    let bh = if burning.is_empty() { 30.0 } else { 108.0 };
    let b = Rect::new(x0, y, w0, bh);
    if let Some(&z) = burning.first() {
        let zone = &f.zones[z];
        rrect(b, 6.0, Color::new(0.22, 0.03, 0.02, 1.0));
        rrect_lines(b, 6.0, 2.0, if blink { RED } else { Color::new(0.5, 0.05, 0.03, 1.0) });
        let k = if zone.handle { 0.5 } else { 1.0 };
        let left = ((1.0 - zone.heat).max(0.0) * 45.0 / k) as i32;
        txt(&format!("FEU {}", ZONES[z].1), b.x + 12.0, b.y + 20.0, 15.0, WHITE);
        txt_r(&format!("DESTRUCTION DANS ~{left} s"), b.x + b.w - 12.0, b.y + 20.0, 13.0, if left < 15 && blink { WHITE } else { AMBER });
        let any_disch = !f.bottles[0] || !f.bottles[1];
        let wait = any_disch && f.since_disch < 10.0;
        let steps = [
            (format!("1. TIRER LA POIGNEE FIRE {}", ZONES[z].0), zone.handle),
            ("2. DECHARGER BTL 1 (DISCH)".to_string(), !f.bottles[0]),
            (if wait { format!("3. ATTENDRE 10 s ({:.0} s)", 10.0 - f.since_disch) } else { "3. FEU PERSISTANT : DECHARGER BTL 2".to_string() }, !f.bottles[1]),
            ("4. SI NON MAITRISE : ATTERRIR AU PLUS VITE / EJECTION".to_string(), false),
        ];
        let cur = steps.iter().position(|s| !s.1).unwrap_or(3);
        for (i, (t, done)) in steps.iter().enumerate() {
            let yy = b.y + 40.0 + i as f32 * 16.0;
            let c = if *done { GREEN_S } else if i == cur { if blink { WHITE } else { AMBER } } else { DIM };
            txt(if *done { "[X]" } else if i == cur { "[>]" } else { "[ ]" }, b.x + 12.0, yy, 11.0, c);
            txt(t, b.x + 44.0, yy, 11.0, c);
        }
    } else {
        let hot = f.zones.iter().any(|z| z.heat > 0.05 && !z.destroyed);
        let destroyed = f.zones.iter().any(|z| z.destroyed);
        rrect(b, 6.0, Color::new(0.05, 0.08, 0.06, 1.0));
        rrect_lines(b, 6.0, 1.0, Color::new(0.15, 0.3, 0.18, 1.0));
        let (t, c) = if test { ("ESSAI DES VOYANTS EN COURS", AMBER) } else if destroyed { ("AUCUN FEU - ZONE DETRUITE", AMBER) } else if hot { ("AUCUN FEU - ZONE CHAUDE, SURVEILLER", AMBER) } else { ("AUCUN FEU DETECTE - BOUCLES A / B NOMINALES", GREEN_S) };
        txt_c(t, b.x + b.w / 2.0, b.y + 20.0, 12.0, c);
    }
    y += bh + 10.0;
    // ----- Synoptique (gauche) + poignées (droite) -----
    let mid_h = ((r.y + r.h - y) * 0.58).clamp(150.0, 260.0);
    let syn_w = (w0 * 0.34).clamp(140.0, 260.0);
    let syn = Rect::new(x0, y, syn_w, mid_h);
    rrect(syn, 6.0, Color::new(0.045, 0.05, 0.055, 1.0));
    if let Some(z) = synoptic(f, syn, test, blink) {
        out = Some(FireAction::Handle(z));
    }
    let hx = x0 + syn_w + 10.0;
    let hw = (w0 - syn_w - 10.0) / 4.0;
    for (i, (code, name)) in ZONES.iter().enumerate() {
        let z = &f.zones[i];
        let col = Rect::new(hx + i as f32 * hw, y, hw - 6.0, mid_h);
        rrect(col, 6.0, Color::new(0.06, 0.065, 0.07, 1.0));
        let hr = Rect::new(col.x + 6.0, col.y + 8.0, col.w - 12.0, (mid_h * 0.42).min(96.0));
        if t_handle(hr, z, code, test, blink) {
            out = Some(FireAction::Handle(i));
        }
        let mut yy = hr.y + hr.h + 16.0;
        txt_c(&fit_name(name, col.w - 6.0), col.x + col.w / 2.0, yy, 9.0, DIM);
        yy += 15.0;
        let (st, c) = if z.destroyed { ("DETRUIT", RED) } else if z.fire { ("FEU", RED) } else if z.heat > 0.05 { ("CHAUD", AMBER) } else { ("NORMAL", GREEN_S) };
        txt_c(st, col.x + col.w / 2.0, yy, 13.0, c);
        yy += 10.0;
        // Température de zone
        let tb = Rect::new(col.x + 8.0, yy, col.w - 16.0, 7.0);
        rrect(tb, 3.0, Color::new(0.03, 0.03, 0.035, 1.0));
        rrect(Rect::new(tb.x, tb.y, (tb.w * z.heat.min(1.0) as f32).max(2.0), tb.h), 3.0, if z.heat > 0.6 { RED } else if z.heat > 0.05 { AMBER } else { GREEN_S });
        yy += 22.0;
        // Boucles de détection A / B
        if yy + 6.0 < col.y + col.h {
            for (k, l) in ["A", "B"].iter().enumerate() {
                let lx = col.x + col.w / 2.0 + (k as f32 - 0.5) * 26.0;
                draw_circle(lx, yy, 5.0, if z.fire || test { RED } else { GREEN_S });
                txt_c(l, lx, yy + 16.0, 9.0, DIM);
            }
        }
    }
    y += mid_h + 10.0;
    // ----- Bouteilles + essai -----
    let armed = f.zones.iter().any(|z| z.handle);
    let rem = r.y + r.h - y - 8.0;
    if rem > 60.0 {
        let bh = rem.min(96.0);
        let bw = ((w0 - 20.0) / 3.0).min(220.0);
        for k in 0..2 {
            if bottle(Rect::new(x0 + k as f32 * (bw + 10.0), y, bw, bh), k, f.bottles[k], armed) {
                out = Some(FireAction::Bottle(k));
            }
        }
        let tr = Rect::new(x0 + 2.0 * (bw + 10.0), y + bh / 2.0 - 20.0, bw.min(140.0), 40.0);
        rrect(tr, 6.0, if hovered(tr) { Color::new(0.14, 0.15, 0.17, 1.0) } else { Color::new(0.09, 0.1, 0.11, 1.0) });
        rrect_lines(tr, 6.0, 1.5, if test { AMBER } else { DIM });
        txt_c("FIRE TEST", tr.x + tr.w / 2.0, tr.y + 18.0, 12.0, WHITE);
        txt_c("essai des voyants", tr.x + tr.w / 2.0, tr.y + 32.0, 9.0, DIM);
        if clicked(tr) {
            out = Some(FireAction::Test);
        }
    }
    out
}

fn fit_name(s: &str, w: f32) -> String {
    let mut h = s.to_string();
    while h.len() > 3 && txt_w(&h, 9.0) > w {
        h.pop();
    }
    h
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
