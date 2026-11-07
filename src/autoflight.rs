//! Automatismes de vol : décollage automatique et atterrissage automatique (autoland).

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;

/// Vitesse d'approche selon la masse (incidence d'approche ~11°), bornée à 165-235 KEAS.
pub fn vapp(ac: &Aircraft) -> f64 {
    let v = (2.0 * ac.mass() * 9.81 / (crate::atmosphere::RHO0 * S_WING * 0.52)).sqrt() / KT;
    v.clamp(165.0, 235.0)
}
const IAF_DIST: f64 = 15.0 * NM;
const IAF_HGT_FT: f64 = 4_500.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AlPhase {
    ToIaf,
    Approach,
    Flare,
    Rollout,
}

#[derive(Clone)]
pub struct Autoland {
    pub apt: usize,
    pub thr_lat: f64,
    pub thr_lon: f64,
    pub hdg: f64,
    pub elev: f64,
    pub iaf: (f64, f64),
    pub phase: AlPhase,
    pub gs_captured: bool,
    /// Attente (orbite) à l'IAF pour perdre altitude / vitesse avant l'approche.
    pub holding: bool,
    pub go_arounds: u32,
    /// Atterrissage plané sans moteur : poussée nulle, pas de remise des gaz possible.
    pub dead_stick: bool,
    /// Circuit d'atterrissage plané : 0 vers le point haut, 1 branche d'éloignement, 2 demi-tour.
    pub pat: u8,
    /// Côté du circuit : -90 (gauche de l'axe) ou +90 (droite), choisi à l'entrée.
    pub pat_side: f64,
}

/// Plané sans moteur (façon navette spatiale) : pente extérieure forte vers un point d'aboutissement
/// avant le seuil, vitesse tenue aux aérofreins, puis pré-arrondi sur une pente intérieure faible où
/// la vitesse décroît jusqu'à l'arrondi.
const DS_OUTER_DEG: f64 = 9.0;
const DS_AIM: f64 = 4_500.0;
const DS_INNER_DEG: f64 = 3.0;

/// Hauteur du profil plané à la distance `along` avant le seuil (m).
pub fn dead_stick_path(along: f64) -> f64 {
    let outer = (along - DS_AIM) * DS_OUTER_DEG.to_radians().tan();
    let inner = (along + 600.0) * DS_INNER_DEG.to_radians().tan();
    outer.max(inner).max(0.0)
}

impl Autoland {
    /// Choisit le sens de piste qui minimise le trajet jusqu'à l'IAF.
    pub fn new(apt_idx: usize, lat: f64, lon: f64) -> Autoland {
        let a = &AIRPORTS[apt_idx];
        let mut best: Option<(f64, Autoland)> = None;
        for (along_thr, hdg) in [(-a.len / 2.0, a.hdg), (a.len / 2.0, wrap360(a.hdg + 180.0))] {
            let (tla, tlo) = a.point(along_thr, 0.0);
            let iaf = destination(tla, tlo, wrap360(hdg + 180.0), IAF_DIST);
            let (d, _) = dist_brg(lat, lon, iaf.0, iaf.1);
            let al = Autoland { apt: apt_idx, thr_lat: tla, thr_lon: tlo, hdg, elev: a.elev, iaf, phase: AlPhase::ToIaf, gs_captured: false, holding: false, go_arounds: 0, dead_stick: false, pat: 0, pat_side: -90.0 };
            if best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                best = Some((d, al));
            }
        }
        best.unwrap().1
    }

    pub fn runway_number(&self) -> u32 {
        crate::airports::Airport::runway_number(self.hdg)
    }
}

#[derive(Clone)]
pub struct Takeoff {
    pub hdg: f64,
    pub alt_ft: f64,
    pub thr_lat: f64,
    pub thr_lon: f64,
}

impl Aircraft {
    /// Engage l'atterrissage automatique ; renvoie une erreur lisible si impossible.
    pub fn engage_autoland(&mut self, apt: usize) -> Result<(), String> {
        if self.on_ground {
            return Err("AU SOL".into());
        }
        if self.eng.turb != TurbState::Running {
            return Err("TURBINES REQUISES (M<2.5 : I puis T, ou commande TURB START)".into());
        }
        let al = Autoland::new(apt, self.lat, self.lon);
        let num = al.runway_number();
        self.autoland = Some(al);
        self.autotakeoff = None;
        self.ap.on = true;
        self.ap.at_on = true;
        self.ap.vert = VertMode::Alt;
        self.ap.lat = LatMode::Hdg;
        self.msg(Level::Info, format!("AUTOLAND ARMÉ : {} PISTE {:02} — RALLIEMENT IAF 15 NM", AIRPORTS[apt].icao, num));
        Ok(())
    }

    /// Atterrissage automatique plané, sans moteur : sens de piste face à l'avion, approche directe.
    pub fn engage_autoland_dead_stick(&mut self, apt: usize) -> Result<(), String> {
        if self.on_ground {
            return Err("AU SOL".into());
        }
        let a = &AIRPORTS[apt];
        // Seuil le plus proche dans l'axe de la position actuelle (pas de circuit possible en plané)
        let mut best: Option<(f64, f64, f64, f64)> = None;
        for (along_thr, hdg) in [(-a.len / 2.0, a.hdg), (a.len / 2.0, wrap360(a.hdg + 180.0))] {
            let (tla, tlo) = a.point(along_thr, 0.0);
            let along = along_before(tla, tlo, hdg, self.lat, self.lon);
            if best.is_none_or(|b| along > b.0) {
                best = Some((along, tla, tlo, hdg));
            }
        }
        let (_, tla, tlo, hdg) = best.unwrap();
        let al = Autoland { apt, thr_lat: tla, thr_lon: tlo, hdg, elev: a.elev, iaf: (tla, tlo), phase: AlPhase::Approach, gs_captured: false, holding: false, go_arounds: 0, dead_stick: true, pat: 0, pat_side: -90.0 };
        let num = al.runway_number();
        self.autoland = Some(al);
        self.autotakeoff = None;
        self.ap.on = true;
        self.ap.at_on = false;
        self.throttle = 0.0;
        self.msg(Level::Warning, format!("AUTOLAND PLANE SANS MOTEUR : {} PISTE {num:02} — PAS DE REMISE DES GAZ POSSIBLE", a.icao));
        Ok(())
    }

    /// Remise des gaz (décision aux minimums) : pleine poussée, montée vers 3000 ft sol, train rentré.
    pub fn go_around(&mut self) -> Result<String, String> {
        if self.on_ground {
            return Err("AU SOL".into());
        }
        let elev = self.approach.map(|a| a.4).unwrap_or(self.d.elev);
        if let Some(al) = self.autoland.as_mut() {
            if al.dead_stick {
                return Err("PLANE SANS MOTEUR : PAS DE REMISE DES GAZ POSSIBLE".into());
            }
            al.phase = AlPhase::ToIaf;
            al.go_arounds += 1;
            al.gs_captured = false;
            al.holding = false;
        } else {
            self.ap.lat = LatMode::Hdg;
            self.ap.hdg_tgt = self.hdg_deg().round();
        }
        self.ap.on = true;
        self.ap.loc_armed = false;
        self.ap.gs_armed = false;
        self.ap.vert = VertMode::Alt;
        self.ap.alt_tgt_ft = (elev / FT + 3_000.0).round();
        self.ap.at_on = true;
        self.ap.spd_keas = Some(230.0);
        self.speedbrake = false;
        self.throttle = self.thr_max().min(1.0);
        if self.gear_down {
            self.cmd_gear();
        }
        Ok(format!("REMISE DES GAZ — MONTEE {:.0} FT", self.ap.alt_tgt_ft))
    }

    pub fn engage_takeoff(&mut self, alt_ft: f64) -> Result<(), String> {
        if !self.on_ground {
            return Err("DÉJÀ EN VOL".into());
        }
        if self.eng.turb != TurbState::Running {
            return Err("TURBINES NON DÉMARRÉES (T)".into());
        }
        let Some(apt) = AIRPORTS.iter().find(|a| a.on_runway(self.lat, self.lon)) else {
            return Err("PAS SUR UNE PISTE".into());
        };
        // Axe de piste le plus proche du cap actuel
        let h = self.hdg_deg();
        let hdg = if wrap180(apt.hdg - h).abs() < 90.0 { apt.hdg } else { wrap360(apt.hdg + 180.0) };
        let (thr_lat, thr_lon) = apt.point(if hdg == apt.hdg { -apt.len / 2.0 } else { apt.len / 2.0 }, 0.0);
        self.autotakeoff = Some(Takeoff { hdg, alt_ft, thr_lat, thr_lon });
        self.autoland = None;
        self.msg(Level::Info, format!("DÉCOLLAGE AUTO : PLEINE PUISSANCE, ROTATION 170 KEAS, MONTÉE {alt_ft:.0} FT"));
        Ok(())
    }

    /// ILS manuel (touche L) : capture LOC puis G/S selon des critères réalistes, remise des gaz si long.
    fn ils_logic(&mut self) {
        let Some((apt, tla, tlo, hdg, elev)) = self.approach else {
            self.ap.loc_armed = false;
            self.ap.gs_armed = false;
            return;
        };
        if self.on_ground || !self.ap.on {
            return;
        }
        let along = along_before(tla, tlo, hdg, self.lat, self.lon);
        let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
        let herr = wrap180(self.hdg_deg() - hdg).abs();
        if self.ap.loc_armed && along > 0.5 * NM && along < 25.0 * NM && cross.abs() < 3.0 * NM && herr < 100.0 {
            self.ap.loc_armed = false;
            self.ap.lat = LatMode::Loc;
            self.msg(Level::Info, "LOC CAPTURE");
        }
        let path_alt = elev + (along + 300.0) * 3f64.to_radians().tan();
        if self.ap.gs_armed && self.ap.lat == LatMode::Loc && along > 0.3 * NM && self.alt <= path_alt + 60.0 && self.alt >= path_alt - 250.0 {
            self.ap.gs_armed = false;
            self.ap.vert = VertMode::Gs;
            self.msg(Level::Info, "G/S CAPTURE");
        }
        // Approche interrompue : au-delà de la mi-piste sans toucher, remise des gaz (cap piste, 3000 ft sol)
        let half = AIRPORTS[apt].len * 0.5;
        if (self.ap.lat == LatMode::Loc || self.ap.vert == VertMode::Gs) && along < -half {
            self.ap.lat = LatMode::Hdg;
            self.ap.hdg_tgt = hdg;
            self.ap.vert = VertMode::Alt;
            self.ap.alt_tgt_ft = elev / FT + 3_000.0;
            self.ap.loc_armed = false;
            self.ap.gs_armed = false;
            self.throttle = self.throttle.max(0.9);
            self.msg(Level::Caution, "REMISE DES GAZ — APPROCHE ILS INTERROMPUE (L POUR RÉARMER)");
        }
    }

    /// Exécuté à chaque pas avant step() : pilote les modes AP et renvoie
    /// éventuellement des ordres manche (au sol) qui remplacent ceux du pilote.
    pub fn autoflight(&mut self) -> Option<(f64, f64)> {
        if self.crashed.is_some() {
            self.autoland = None;
            self.autotakeoff = None;
            return None;
        }
        if self.autoland.is_none() {
            self.ils_logic();
        }
        if let Some(to) = &self.autotakeoff {
            let (hdg, alt_ft, tla, tlo) = (to.hdg, to.alt_ft, to.thr_lat, to.thr_lon);
            if self.on_ground {
                self.brakes = false;
                self.ap.at_on = false;
                self.throttle = 1.0;
                let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
                let herr = wrap180(self.hdg_deg() - hdg);
                let steer = (-(0.03 * cross + 0.2 * herr)).clamp(-1.0, 1.0);
                let pitch = if self.d.keas >= 170.0 && self.theta < 10f64.to_radians() { 1.0 } else { 0.0 };
                return Some((pitch, steer));
            }
            if self.d.agl > 30.0 {
                if self.gear_down {
                    self.cmd_gear();
                }
                self.ap.on = true;
                self.ap.lat = LatMode::Hdg;
                self.ap.hdg_tgt = hdg;
                self.ap.vert = VertMode::Alt;
                self.ap.alt_tgt_ft = alt_ft;
                self.ap.at_on = true;
                self.ap.spd_managed = true;
                self.ap.spd_keas = Some(250.0);
                self.autotakeoff = None;
                self.msg(Level::Info, format!("DÉCOLLAGE AUTO TERMINÉ — AP1 : CAP {hdg:03.0}, ALT {alt_ft:.0}, VITESSE GÉRÉE"));
            }
            return None;
        }

        let Some(al) = &self.autoland else { return None };
        let (tla, tlo, hdg, elev) = (al.thr_lat, al.thr_lon, al.hdg, al.elev);
        let phase = al.phase;
        let apt = al.apt;
        self.approach = Some((apt, tla, tlo, hdg, elev));
        let along = along_before(tla, tlo, hdg, self.lat, self.lon);
        let cross = cross_track(tla, tlo, hdg, self.lat, self.lon);
        let agl = self.alt - elev;
        let mut next = phase;
        let mut out = None;
        let mut say: Option<String> = None;

        let dead_stick = self.autoland.as_ref().is_some_and(|a| a.dead_stick);
        match phase {
            AlPhase::Approach if dead_stick => {
                self.ap.on = true;
                self.ap.at_on = false;
                // Gestion d'énergie : circuit d'atterrissage plané (« overhead pattern »). Trop haut
                // (ou seuil dépassé en vol) : rejoindre le point haut au-dessus du seuil décalé de
                // 3 NM à gauche, s'éloigner parallèlement à la piste en descendant fort (aérofreins),
                // puis demi-tour vers la piste quand la hauteur, après le virage, rejoint le profil.
                let a_eff = if cross.abs() > 300.0 { along + (cross.abs() - 300.0) * 0.7 } else { along };
                let excess = agl - dead_stick_path(a_eff.max(0.0));
                let done = self.autoland.as_ref().unwrap().gs_captured;
                let exit_h0 = dead_stick_path(5.0 * NM);
                let holding = self.autoland.as_ref().unwrap().holding;
                let enter = !holding && ((excess > 2_000.0 * FT && along > 1.0 * NM && !done) || (along < 0.0 && agl > exit_h0));
                if enter {
                    // Circuit du côté où se trouve déjà l'avion (transit le plus court)
                    let (le, ln) = enu_offset(tla, tlo, self.lat, self.lon);
                    let lr = wrap360(hdg - 90.0).to_radians();
                    let side = if le * lr.sin() + ln * lr.cos() >= 0.0 { -90.0 } else { 90.0 };
                    let al = self.autoland.as_mut().unwrap();
                    al.holding = true;
                    al.pat = 1;
                    al.pat_side = side;
                    say = Some("AUTOLAND PLANE : TROP HAUT — CONE D'ALIGNEMENT (SPIRALE DE DESCENTE)".into());
                }
                if self.autoland.as_ref().unwrap().holding {
                    // Cône d'alignement (façon navette) : cercle tangent à l'axe à 5 NM du seuil, du
                    // côté où arrive l'avion ; la sortie (point de tangence) tombe dans l'axe, face à
                    // la piste. Pente planifiée (6 à 15°, nombre de tours adapté) pour arriver au point
                    // de tangence à la hauteur du profil ; la vitesse reste prioritaire.
                    let va = vapp(self);
                    let tgt = va + 60.0;
                    let side = self.autoland.as_ref().unwrap().pat_side; // -90 gauche, +90 droite
                    let r = (1.3 * self.tas * self.tas / 9.81).clamp(1.3 * NM, 2.5 * NM);
                    let hac_along = 5.0 * NM;
                    let (tl, to) = destination(tla, tlo, wrap360(hdg + 180.0), hac_along);
                    let (cl, co) = destination(tl, to, wrap360(hdg + side), r);
                    let (dc, bc) = dist_brg(self.lat, self.lon, cl, co);
                    // Virage vers le centre : à gauche si le cercle est à gauche de l'axe
                    let turn = if side < 0.0 { 1.0 } else { -1.0 };
                    let corr = ((dc - r) / NM * 25.0).clamp(-50.0, 50.0) * turn;
                    self.ap.lat = LatMode::Hdg;
                    self.ap.hdg_tgt = if dc > r * 2.0 { bc } else { wrap360(bc + 90.0 * turn - corr) };
                    // Angle restant jusqu'au point de tangence (rayon centre → tangence = côté opposé)
                    let rad_now = wrap360(bc + 180.0);
                    let rad_exit = wrap360(hdg - side);
                    let theta = if turn > 0.0 { (rad_now - rad_exit).rem_euclid(360.0) } else { (rad_exit - rad_now).rem_euclid(360.0) }.to_radians();
                    let exit_h = dead_stick_path(hac_along);
                    let dh = agl - exit_h;
                    let on_circle = dc < r * 1.5;
                    let lap = std::f64::consts::TAU * r;
                    let dist0 = if on_circle { theta * r } else { dc + theta * r };
                    // Sortie au point de tangence seulement près de la hauteur du profil (sinon un tour de plus)
                    // Sortie : au point de tangence, ou dès que l'avion se trouve aligné dans l'axe à bonne distance
                    let aligned = wrap180(self.hdg_deg() - hdg).abs() < 25.0 && cross.abs() < 0.5 * NM && along > 3.0 * NM;
                    let at_exit = ((on_circle && theta.to_degrees() < 10.0) || aligned) && dh < 3_200.0 * FT;
                    // Pas assez de hauteur pour finir l'arc : on coupe tout droit vers le point de tangence
                    // Cible du raccourci : point de l'axe un peu devant l'avion (jamais derrière)
                    let a_t = (along - 1.5 * NM).clamp(2.0 * NM, hac_along);
                    let (ptl, pto) = destination(tla, tlo, wrap360(hdg + 180.0), a_t);
                    let (dt_t, bt_t) = dist_brg(self.lat, self.lon, ptl, pto);
                    // Raccourci seulement si sa pente est plus proche de la pente nominale (~8°) que
                    // celle de l'arc restant, et que l'arc serait trop plat
                    let g_arc = (dh.max(0.0) / dist0.max(300.0)).atan().to_degrees();
                    let g_short = ((agl - dead_stick_path(a_t)).max(0.0) / dt_t.max(300.0)).atan().to_degrees();
                    // Jamais de raccourci qui imposerait une finale trop raide (> 14°)
                    let short = on_circle && g_short <= 14.0 && (g_arc < 2.5 || (g_arc < 4.0 && (g_short - 8.0).abs() < (g_arc - 8.0).abs()));
                    if short || self.autoland.as_ref().unwrap().pat == 5 {
                        self.autoland.as_mut().unwrap().pat = 5;
                    }
                    let direct = self.autoland.as_ref().unwrap().pat == 5;
                    let reached = direct && (dt_t < 0.6 * NM || cross.abs() < 0.3 * NM);
                    if at_exit || reached || agl < 1_200.0 * FT {
                        let al = self.autoland.as_mut().unwrap();
                        al.holding = false;
                        al.gs_captured = true;
                        al.pat = 0;
                        say = Some(format!("AUTOLAND PLANE : SORTIE DU CONE D'ALIGNEMENT — FINALE ({})", if at_exit { "TANGENCE" } else if reached { "RACCOURCI" } else { "BAS" }));
                    } else {
                        let n = ((dh.max(0.0) / 10f64.to_radians().tan() - dist0) / lap).round().max(0.0);
                        let gamma_req = (dh.max(0.0) / (dist0 + n * lap).max(600.0)).atan().to_degrees();
                        if direct {
                            self.ap.hdg_tgt = bt_t;
                        }
                        // En route vers le cercle : vitesse tenue ; sur le cercle : pente planifiée
                        let mut gamma_cmd = if on_circle && !direct { -gamma_req.clamp(3.0, 15.0) } else { (-6.0 + (self.d.keas - tgt) * 0.25).clamp(-15.0, -2.0) };
                        // Vitesse prioritaire : trop lent → plus de pente ; trop vite → moins de pente
                        // La marge de vitesse (VAPP + 60) sert de réserve d'énergie ; plancher VAPP + 15
                        if self.d.keas < va + 15.0 {
                            gamma_cmd -= ((va + 15.0 - self.d.keas) * 0.3).min(8.0);
                        } else if self.d.keas > tgt + 25.0 {
                            gamma_cmd = (gamma_cmd + (self.d.keas - tgt - 25.0) * 0.3).min(-3.0);
                        }
                        self.speedbrake = self.d.keas > tgt + 10.0;
                        self.ap.vert = VertMode::Pitch;
                        // Correction sur la pente réellement suivie (en virage serré la pente obtenue
                        // diffère beaucoup de « assiette − incidence »)
                        let g_now = self.gamma.to_degrees();
                        self.ap.pitch_tgt = (self.theta.to_degrees() + 1.2 * (gamma_cmd.max(-20.0) - g_now)).clamp(-25.0, 20.0);
                        if let Some(al) = self.autoland.as_mut() {
                            al.phase = next;
                        }
                        if let Some(s) = say {
                            self.msg(Level::Info, s);
                        }
                        self.throttle = 0.0;
                        return out;
                    }
                }
                // Latéral : poursuite d'un point de l'axe 2 NM devant, puis LOC près de l'axe
                if cross.abs() < 600.0 && along < 10.0 * NM {
                    self.ap.lat = LatMode::Loc;
                } else {
                    let ahead = (along - 2.0 * NM).max(-1_000.0);
                    let (pla, plo) = destination(tla, tlo, wrap360(hdg + 180.0), ahead);
                    self.ap.lat = LatMode::Hdg;
                    self.ap.hdg_tgt = dist_brg(self.lat, self.lon, pla, plo).1;
                }
                // Vertical : suivi du profil plané par la pente (assiette = pente + incidence)
                let h_path = dead_stick_path(a_eff);
                // Pente du profil anticipée ~5 s devant : le pré-arrondi commence avant la cassure
                let look = (self.tas * 5.0).max(300.0);
                let h_ahead = dead_stick_path((a_eff - look).max(-600.0));
                let path_gamma = ((h_ahead - h_path) / look).atan().to_degrees();
                let slope = -path_gamma;
                let err = h_path - agl;
                // Correction d'écart douce sous 1500 ft (pas de plongée après le pré-arrondi)
                // Au-dessus du profil, la finale peut plonger jusqu'à ~19° (aérofreins par l'énergie)
                let corr = if agl < 1_500.0 * FT { (err / 100.0).clamp(-5.0, 2.5) } else { (err / 60.0).clamp(-10.0, 5.0) };
                let mut gamma_cmd = path_gamma + corr;
                // Protection de vitesse : sous VAPP - 10, on plonge pour garder l'énergie
                let va = vapp(self);
                // Protection de vitesse au-dessus de 1500 ft seulement : en courte finale la
                // décélération sur la pente intérieure est voulue (arrondi à vitesse réduite)
                if self.d.keas < va - 10.0 && agl > 1_500.0 * FT {
                    gamma_cmd = gamma_cmd.min(-(slope.max(4.0)) - (va - 10.0 - self.d.keas) * 0.3);
                }
                // Courte finale : sous VAPP, échanger un peu de hauteur contre de la vitesse
                // (arrondi d'autant plus tôt que la chute est forte)
                if self.d.keas < va + 5.0 && agl <= 1_500.0 * FT && agl > 60.0 * FT {
                    gamma_cmd = gamma_cmd.min(-(4.5 + (va + 5.0 - self.d.keas) * 0.2).min(8.0));
                }
                gamma_cmd = gamma_cmd.clamp(-14.0, 2.0);
                self.ap.vert = VertMode::Pitch;
                self.ap.pitch_tgt = gamma_cmd + self.d.alpha.to_degrees();
                // Vitesse tenue aux aérofreins sur la pente extérieure
                // Aérofreins : vitesse tenue sur la pente forte ; en courte finale, excès de vitesse
                // dissipé avant le seuil (pas de remise des gaz : il faut pouvoir s'arrêter)
                // Énergie totale (hauteur + vitesse) comparée à celle du profil à vitesse de référence
                let g = 9.81;
                let rho_k = (crate::atmosphere::isa(self.alt).rho / 1.225).sqrt();
                // Vitesse de référence : sur la pente intérieure l'avion décélère (~0,8 m/s²) jusqu'au
                // toucher à VAPP + 10 ; elle croît donc avec la distance restante
                let v_td = (va + 10.0) * KT / rho_k;
                let v_out = (va + 70.0) * KT / rho_k;
                let v_ref = (v_td * v_td + 2.0 * 0.8 * (a_eff + 600.0).max(0.0)).sqrt().min(v_out);
                let e_now = agl + self.tas * self.tas / (2.0 * g);
                let e_req = h_path + v_ref * v_ref / (2.0 * g);
                self.speedbrake = e_now > e_req + 120.0 || self.d.keas > va + 120.0;
                // S-turns : excès d'énergie en finale → virages alternés de ±35° autour de l'axe
                // (trajet rallongé), tant que l'avion est loin du seuil
                if e_now > e_req + 250.0 && along > 2.5 * NM && cross.abs() < 1.5 * NM {
                    let s = if (along / 2_500.0).sin() >= 0.0 { 1.0 } else { -1.0 };
                    self.ap.lat = LatMode::Hdg;
                    self.ap.hdg_tgt = wrap360(hdg + 35.0 * s);
                }
                if along < 2.0 * NM && along > -1_000.0 && agl < 2_000.0 * FT && !self.gear_down {
                    // Train bloqué : sortie de secours par gravité
                    if self.fails.has(crate::systems::Fail::GearJam) {
                        self.cmd_gear_alt();
                    } else {
                        self.cmd_gear();
                    }
                }
                // Arrondi déclenché d'autant plus haut que le taux de chute est fort
                if agl < 15.0 + (-self.d.vs).max(0.0) * 4.5 && along < 2.0 * NM {
                    next = AlPhase::Flare;
                    self.speedbrake = false;
                    say = Some("AUTOLAND PLANE : FLARE".into());
                }
            }
            AlPhase::ToIaf => {
                let (iaf_lat, iaf_lon) = self.autoland.as_ref().unwrap().iaf;
                let (d, brg) = dist_brg(self.lat, self.lon, iaf_lat, iaf_lon);
                let iaf_alt_ft = elev / FT + IAF_HGT_FT;
                self.ap.on = true;
                self.ap.at_on = true;
                self.ap.lat = LatMode::Hdg;
                self.ap.vert = VertMode::Alt;
                self.ap.alt_tgt_ft = iaf_alt_ft;
                self.ap.spd_keas = Some(if d < 40.0 * NM { 230.0 } else { 300.0 });
                // Prêt pour l'approche : altitude et vitesse d'IAF atteintes
                let ready = self.alt / FT < iaf_alt_ft + 1_200.0 && self.d.keas < 255.0 && self.d.mach < 0.9;
                self.speedbrake = !ready && (self.alt / FT > iaf_alt_ft + 3_000.0 || self.d.keas > 280.0);
                let holding = self.autoland.as_ref().unwrap().holding;
                if d < 3.0 * NM && !ready && !holding {
                    self.autoland.as_mut().unwrap().holding = true;
                    say = Some("AUTOLAND : ATTENTE (HOLD) À L'IAF — DESCENTE / DÉCÉLÉRATION".into());
                }
                if self.autoland.as_ref().unwrap().holding {
                    // Orbite autour de l'IAF (rayon ~3 NM) : cap tangent, recentré selon la distance
                    let corr = ((d - 3.0 * NM) / NM * 20.0).clamp(-40.0, 40.0);
                    self.ap.hdg_tgt = wrap360(brg + 90.0 - corr);
                } else {
                    self.ap.hdg_tgt = brg;
                }
                let aligned = wrap180(self.hdg_deg() - hdg).abs() < 70.0;
                let near_iaf = d < 2.5 * NM && !self.autoland.as_ref().unwrap().holding;
                let on_axis = along > 4.0 * NM && along < IAF_DIST + NM && cross.abs() < 3.0 * NM && aligned;
                let hold_done = self.autoland.as_ref().unwrap().holding && ready && d < 4.0 * NM;
                if ready && (near_iaf || on_axis || hold_done) {
                    let al = self.autoland.as_mut().unwrap();
                    al.holding = false;
                    al.gs_captured = false;
                    next = AlPhase::Approach;
                    say = Some("AUTOLAND : LOC CAPTURE".into());
                }
            }
            AlPhase::Approach => {
                self.ap.on = true;
                self.ap.at_on = true;
                self.ap.lat = LatMode::Loc;
                let path_alt = elev + (along + 300.0) * 3f64.to_radians().tan();
                let captured = self.autoland.as_ref().unwrap().gs_captured;
                // Capture du plan de descente par en dessous (ou à son niveau) uniquement
                // Capture : par en dessous, ou par au-dessus si l'écart reste rattrapable par la loi G/S (pente ~6° max)
                if !captured && self.alt <= path_alt + 300.0 && self.alt >= path_alt - 150.0 {
                    self.autoland.as_mut().unwrap().gs_captured = true;
                    say = Some("AUTOLAND : G/S CAPTURE".into());
                }
                if self.autoland.as_ref().unwrap().gs_captured {
                    self.ap.vert = VertMode::Gs;
                } else {
                    // Au-dessus du plan : descendre vers lui ; en dessous : palier jusqu'à l'interception
                    self.ap.vert = VertMode::Alt;
                    self.ap.alt_tgt_ft = (path_alt / FT).min(elev / FT + IAF_HGT_FT);
                }
                let va = vapp(self);
                let tgt = if along > 9.0 * NM { va.max(220.0) } else { va };
                self.ap.spd_keas = Some(tgt);
                self.speedbrake = self.d.keas > tgt + 25.0 || self.alt > path_alt + 150.0;
                if along < 9.0 * NM && !self.gear_down {
                    self.cmd_gear();
                }
                // Approche non stabilisée : remise des gaz
                let too_high = along < 3.0 * NM && self.alt > path_alt + 180.0;
                let off_axis = along < 3.0 * NM && cross.abs() > 250.0;
                if too_high || off_axis {
                    next = AlPhase::ToIaf;
                    let al = self.autoland.as_mut().unwrap();
                    al.go_arounds += 1;
                    al.gs_captured = false;
                    al.holding = false;
                    self.speedbrake = false;
                    self.throttle = self.thr_max().min(1.0);
                    self.ap.vert = VertMode::Alt;
                    say = Some(format!("AUTOLAND : REMISE DES GAZ — APPROCHE NON STABILISÉE ({})", if too_high { "TROP HAUT" } else { "HORS AXE" }));
                } else if agl < 18.0 && along < 2.0 * NM {
                    next = AlPhase::Flare;
                    say = Some("AUTOLAND : FLARE".into());
                    self.speedbrake = false;
                }
            }
            AlPhase::Flare => {
                self.ap.vert = VertMode::Flare;
                if agl < 9.0 && self.ap.at_on {
                    self.ap.at_on = false;
                    say = Some("RETARD, RETARD".into());
                }
                if !self.ap.at_on {
                    self.throttle = (self.throttle - 0.5 / 120.0).max(0.0);
                }
                let half = AIRPORTS[apt].len * 0.5;
                if !self.on_ground && along < -half && !dead_stick {
                    next = AlPhase::ToIaf;
                    let al = self.autoland.as_mut().unwrap();
                    al.go_arounds += 1;
                    al.gs_captured = false;
                    self.ap.at_on = true;
                    self.throttle = 1.0;
                    say = Some("AUTOLAND : REMISE DES GAZ — ATTERRISSAGE LONG".into());
                } else if self.on_ground {
                    next = AlPhase::Rollout;
                    say = Some("AUTOLAND : ROLLOUT — AÉROFREINS, AUTOBRAKE".into());
                }
            }
            AlPhase::Rollout => {
                self.ap.on = false;
                self.ap.at_on = false;
                self.throttle = 0.0;
                self.speedbrake = true;
                // Autobrake MAX dès que la roulette de nez est posée
                if self.theta < 2f64.to_radians() || self.d.keas < 200.0 {
                    self.brakes = true;
                }
                let herr = wrap180(self.hdg_deg() - hdg);
                let steer = (-(0.03 * cross + 0.2 * herr)).clamp(-1.0, 1.0);
                out = Some((-1.0, steer));
                if self.tas < 2.0 {
                    self.autoland = None;
                    self.speedbrake = false;
                    self.msg(Level::Info, "AUTOLAND TERMINÉ — AVION ARRÊTÉ SUR LA PISTE");
                    return out;
                }
            }
        }
        if dead_stick {
            self.ap.at_on = false;
            self.throttle = 0.0;
        }
        if let Some(s) = say {
            self.msg(Level::Info, s);
        }
        if let Some(al) = self.autoland.as_mut() {
            al.phase = next;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lance un autoland depuis une position donnée (relative au centre de piste de KEDW).
    fn run_case(name: &str, brg_from_apt: f64, dist_nm: f64, alt_ft: f64, mach: f64, hdg: f64, fuel: f64) -> Result<String, String> {
        run_case_wind(name, brg_from_apt, dist_nm, alt_ft, mach, hdg, fuel, (0.0, 0.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_case_wind(name: &str, brg_from_apt: f64, dist_nm: f64, alt_ft: f64, mach: f64, hdg: f64, fuel: f64, wind: (f64, f64)) -> Result<String, String> {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = destination(apt.lat, apt.lon, brg_from_apt, dist_nm * NM);
        ac.lat = la;
        ac.lon = lo;
        ac.alt = alt_ft * FT;
        ac.tas = mach * crate::atmosphere::isa(ac.alt).a;
        ac.psi = hdg.to_radians();
        ac.theta = 3f64.to_radians();
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.fuel = fuel;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.6;
        ac.wind = wind;
        ac.engage_autoland(edw)?;
        let dt = 1.0 / 60.0;
        let mut t: f64 = 0.0;
        let mut log = Vec::new();
        while t < 3600.0 {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            for (_, m) in ac.events.drain(..) {
                if m.contains("AUTOLAND") || m.contains("TOUCHER") || m.contains("PERTE") || m.contains("REMISE") {
                    log.push(format!("{t:.0}s {m}"));
                }
            }
            if ac.crashed.is_some() || (ac.autoland.is_none() && ac.on_ground) {
                break;
            }
            if std::env::var("AL_TRACE").is_ok() && name.contains("FL350") && (t as i64) % 15 == 0 && (t - t.floor()) < dt {
                if let Some(al) = &ac.autoland {
                    let along = crate::aircraft::along_before(al.thr_lat, al.thr_lon, al.hdg, ac.lat, ac.lon);
                    let cross = crate::aircraft::cross_track(al.thr_lat, al.thr_lon, al.hdg, ac.lat, ac.lon);
                    let path = al.elev + (along + 300.0) * 3f64.to_radians().tan();
                    println!("  t={t:4.0} {:?} hold={} gs={} along={:5.1}NM cross={:5.0}m alt-path={:5.0}m agl={:5.0}ft keas={:3.0} hdg={:3.0} vert={:?}", al.phase, al.holding, al.gs_captured, along / NM, cross, ac.alt - path, (ac.alt - al.elev) / FT, ac.d.keas, ac.hdg_deg(), ac.ap.vert);
                }
            }
            t += dt;
        }
        let res = format!("{name}: {:.0}s crash={:?} sol={} tas={:.0} | {}", t, ac.crashed, ac.on_ground, ac.tas, log.join(" / "));
        if ac.crashed.is_none() && ac.on_ground && ac.tas < 3.0 { Ok(res) } else { Err(res) }
    }

    #[test]
    fn autoland_cases() {
        let cases = [
            ("finale alignée 10NM", 231.0, 12.0, 3_800.0, 0.30, 51.0, 8_000.0),
            ("travers 40NM FL150", 320.0, 40.0, 15_000.0, 0.60, 140.0, 15_000.0),
            ("arrivée haute FL350 M0.95 40NM", 180.0, 40.0, 35_000.0, 0.95, 0.0, 20_000.0),
            ("arrivée très haute FL450 M1.6 80NM", 250.0, 80.0, 45_000.0, 1.6, 70.0, 20_000.0),
            ("mauvais côté 8NM cap opposé", 51.0, 8.0, 6_000.0, 0.35, 51.0, 10_000.0),
            ("au-dessus du terrain FL100", 0.0, 1.0, 10_000.0, 0.45, 300.0, 12_000.0),
            ("lourd 30NM", 140.0, 30.0, 12_000.0, 0.55, 300.0, 36_000.0),
        ];
        let mut fails = Vec::new();
        for (n, b, d, a, m, h, f) in cases {
            match run_case(n, b, d, a, m, h, f) {
                Ok(s) => println!("OK   {s}"),
                Err(s) => {
                    println!("FAIL {s}");
                    fails.push(n);
                }
            }
        }
        assert!(fails.is_empty(), "échecs : {fails:?}");
    }

    /// Autoland avec vent de travers de 16 kt : la route sol (LOC) compense la dérive.
    #[test]
    fn autoland_crosswind() {
        let to = (51.0f64 + 90.0).to_radians();
        let w = 8.0;
        for (n, wind) in [("travers droite 16 kt", (w * to.sin(), w * to.cos())), ("travers gauche 16 kt", (-w * to.sin(), -w * to.cos()))] {
            let r = run_case_wind(n, 231.0, 12.0, 3_800.0, 0.30, 51.0, 8_000.0, wind);
            println!("{r:?}");
            assert!(r.is_ok(), "{n}");
        }
    }

    /// ILS (touche L) engagé avec une vitesse sélectionnée de 300 KEAS : la vitesse gérée doit
    /// ramener l'avion à la vitesse d'approche, puis réduire la manette (RETARD) à l'arrondi.
    #[test]
    fn ils_managed_speed() {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 16.0 * NM, 0.0);
        ac.lat = la;
        ac.lon = lo;
        ac.psi = apt.hdg.to_radians();
        ac.alt = apt.elev + 4_500.0 * FT;
        ac.tas = 300.0 * KT / (crate::atmosphere::isa(ac.alt).rho / crate::atmosphere::RHO0).sqrt();
        ac.on_ground = false;
        ac.gear_down = true;
        ac.brakes = false;
        ac.fuel = 10_000.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.throttle = 0.5;
        ac.ap.on = true;
        ac.ap.at_on = true;
        ac.ap.spd_keas = Some(300.0); // vitesse sélectionnée avant l'ILS
        // Touche L : LOC + G/S, vitesse gérée
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((edw, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.lat = LatMode::Loc;
        ac.ap.vert = VertMode::Gs;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut keas_at = Vec::new();
        let mut retard = false;
        for _ in 0..(900 * 60) {
            ac.step(dt, 0.0, 0.0);
            let along = crate::aircraft::along_before(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon) / NM;
            for mark in [10.0, 4.0] {
                if (along - mark).abs() < 0.02 && !keas_at.iter().any(|(m, _, _)| *m == mark) {
                    keas_at.push((mark, ac.d.keas, vapp(&ac)));
                }
            }
            for (_, m) in ac.events.drain(..) {
                if m.contains("RETARD") {
                    retard = true;
                }
            }
            if retard || ac.on_ground || ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS vitesse gérée : {:?} (NM, KEAS, Vapp) retard={retard}", keas_at);
        let (_, k4, va) = keas_at.iter().find(|(m, _, _)| *m == 4.0).copied().expect("passage à 4 NM");
        assert!((k4 - va).abs() < 12.0, "à 4 NM : {k4:.0} KEAS pour Vapp {va:.0}");
        assert!(retard, "RETARD attendu à l'arrondi");
    }

    /// Reproduction du crash du 30/10 (KSFO) : ILS armé à M2.7 / FL500 à 37 NM, 14.6 km hors axe.
    /// L'ILS ne doit ni capturer le G/S très au-dessus du plan, ni faire tourner l'avion au-dessus du terrain.
    #[test]
    fn ils_armed_high_and_fast_no_crash() {
        let sfo = AIRPORTS.iter().position(|a| a.icao == "KSFO").unwrap();
        let apt = &AIRPORTS[sfo];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 37.0 * NM, 14_600.0);
        ac.lat = la;
        ac.lon = lo;
        ac.alt = 50_000.0 * FT;
        ac.tas = 2.7 * crate::atmosphere::isa(ac.alt).a;
        ac.psi = (apt.hdg - 10.0).to_radians();
        ac.on_ground = false;
        ac.gear_down = false;
        ac.gear_pos = 0.0;
        ac.brakes = false;
        ac.fuel = 20_000.0;
        ac.set_turbines(TurbState::Running, 0.9);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Hdg;
        ac.ap.hdg_tgt = ac.hdg_deg();
        ac.ap.vert = VertMode::Alt;
        ac.ap.alt_tgt_ft = 50_000.0;
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((sfo, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.loc_armed = true;
        ac.ap.gs_armed = true;
        ac.ap.at_on = true;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut gs_high = false;
        for _ in 0..(900 * 60) {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            let along = crate::aircraft::along_before(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon);
            let path = apt.elev + (along + 300.0) * 3f64.to_radians().tan();
            if ac.ap.vert == VertMode::Gs && ac.alt > path + 600.0 {
                gs_high = true;
            }
            if ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS armé haut/rapide : crash={:?} alt={:.0} ft M{:.2} modes {:?}/{:?} G/S très haut={gs_high}", ac.crashed, ac.alt / FT, ac.d.mach, ac.ap.lat, ac.ap.vert);
        assert!(ac.crashed.is_none());
        assert!(!gs_high, "G/S ne doit pas être capturé très au-dessus du plan");
    }

    /// ILS armé correctement (aligné, sous le plan) : capture LOC puis G/S et arrivée alignée aux minimums.
    #[test]
    fn ils_armed_normal_capture() {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let apt = &AIRPORTS[edw];
        let mut ac = Aircraft::new_on_runway(0);
        let (la, lo) = apt.point(-apt.len / 2.0 - 18.0 * NM, 2_500.0);
        ac.lat = la;
        ac.lon = lo;
        ac.psi = (apt.hdg + 25.0).to_radians();
        ac.alt = apt.elev + 4_000.0 * FT;
        ac.tas = 120.0;
        ac.on_ground = false;
        ac.gear_down = true;
        ac.brakes = false;
        ac.fuel = 10_000.0;
        ac.set_turbines(TurbState::Running, 0.8);
        ac.ap.on = true;
        ac.ap.lat = LatMode::Hdg;
        ac.ap.hdg_tgt = ac.hdg_deg();
        ac.ap.vert = VertMode::Alt;
        ac.ap.alt_tgt_ft = (ac.alt / FT).round();
        let a = apt.approach_for(ac.lat, ac.lon);
        ac.approach = Some((edw, a.thr_lat, a.thr_lon, a.hdg, apt.elev));
        ac.ap.loc_armed = true;
        ac.ap.gs_armed = true;
        ac.ap.at_on = true;
        ac.ap.spd_managed = true;
        let dt = 1.0 / 60.0;
        let mut at_minimums = None;
        for _ in 0..(900 * 60) {
            let c = ac.autoflight().unwrap_or((0.0, 0.0));
            ac.step(dt, c.0, c.1);
            if ac.d.agl < 70.0 && at_minimums.is_none() {
                at_minimums = Some(crate::aircraft::cross_track(a.thr_lat, a.thr_lon, a.hdg, ac.lat, ac.lon));
                break;
            }
            if ac.crashed.is_some() {
                break;
            }
        }
        println!("ILS normal : écart latéral aux minimums {:?} m, modes {:?}/{:?}", at_minimums, ac.ap.lat, ac.ap.vert);
        let c = at_minimums.expect("minimums atteints");
        assert!(c.abs() < 40.0, "aligné à ±40 m aux minimums");
    }
}
