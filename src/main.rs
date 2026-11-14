//! DARKSTAR — simulateur d'essais en vol hypersoniques télépilotés.

mod aircraft;
mod alarms;
mod fire;
mod fms;
mod procs;
mod navdb;
mod cifp;
mod escort;
mod apu;
mod dev;
mod fcu;
mod airports;
mod atmosphere;
mod elec;
mod emergency;
mod health;
mod link;
mod connect;
mod appr;
mod mishap;
mod syspage;
mod feel;
mod audio;
mod combat;
mod autoflight;
mod fcs;
mod gcs;
mod fuel;
mod geo;
mod limits;
mod map;
mod menu;
mod recon;
mod refuel;
mod mfd;
mod mission;
mod panels;
mod recorder;
mod pfd;
mod pfd_plus;
mod svs;
mod terrain;
mod systems;
mod thermal;
mod tiles;
mod ui;
mod updater;
mod weather;
mod wx3d;
mod wxr;
mod wxreal;
mod world;

use aircraft::*;
use airports::AIRPORTS;
use geo::*;
use macroquad::prelude::*;
use std::collections::VecDeque;

const WARPS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
const PHYS_DT: f64 = 1.0 / 120.0;

fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: macroquad::miniquad::conf::Conf {
            window_title: format!("DARKSTAR — Station sol d'essais en vol v{}", updater::VERSION),
            window_width: 1600,
            window_height: 1000,
            high_dpi: true,
            window_resizable: true,
            // Plein écran à chaque lancement (fenêtre pour les captures d'essai ou DARKSTAR_WINDOW)
            fullscreen: std::env::var("DARKSTAR_SHOT").is_err() && std::env::var("DARKSTAR_WINDOW").is_err(),
            sample_count: 4,
            ..Default::default()
        },
        update_on: None,
        default_filter_mode: FilterMode::Linear,
        draw_call_vertex_capacity: 4_000,
        draw_call_index_capacity: 6_000,
    }
}

struct Game {
    ac: Aircraft,
    mission: mission::Mission,
    map: map::MapView,
    log: Vec<(f64, Level, String)>,
    warp_idx: usize,
    dest: usize,
    sim_time: f64,
    stick: (f64, f64),
    link_buf: VecDeque<(f64, f64, f64)>,
    last_cmd: (f64, f64),
    acked: Vec<String>,
    history: Vec<(f32, f32)>,
    hist_t: f64,
    atc: [bool; 6],
    console: Option<String>,
    update_cmd: bool,
    cues: Vec<audio::Cue>,
    fdr: recorder::Recorder,
    /// Infos éphémères affichées sur l'horizon : (texte, couleur, échéance en temps réel).
    flashes: Vec<(String, Color, f64)>,
    prev_agl_ft: f64,
    prev_mach_int: i32,
    /// États précédents des aérofreins et des freins (annonces d'événements sur l'horizon).
    prev_brk: Option<(bool, bool)>,
    /// Fin de vol (instruction FIN) : simulation figée, débriefing affiché.
    ended: bool,
    debrief: Vec<String>,
    crash_report: Vec<String>,
    /// Séquence de perte du véhicule (cascade de pannes, perte du signal).
    mishap: Option<mishap::Mishap>,
    /// Dernière trame de télémétrie valide.
    last_ok: mishap::Snapshot,
    /// Dernier état complet du véhicule avant sa perte (deuxième chance).
    last_ac: Option<Aircraft>,
    /// États récents (temps simulé, avion) toutes les 0,5 s sur 6 s : reprise avant l'incident
    ac_hist: std::collections::VecDeque<(f64, Aircraft)>,
    /// FMS / MCDU (plan de vol, SimBrief)
    fms: fms::Fms,
    typing: bool,
    /// Panneau d'alarmes resté allumé après une récupération (jusqu'à TOUT RÉPARER).
    tree: Option<[u8; 32]>,
    muted: bool,
    thermal: thermal::ThermalSys,
    style: svs::SvsStyle,
    tab: gcs::Tab,
    bit_timer: f64,
    bit_fail: Option<bool>,
    mfd: mfd::MfdState,
    recon: recon::Recon,
    wx: weather::Weather,
    refuel: refuel::Refuel,
    /// Escorte de chasse au départ et à l'arrivée
    escort: escort::Escort,
    /// Version développeur (instruction DEV)
    dev: Option<dev::DevUi>,
    /// MCDU affiché à l'image précédente (le clavier ne lui est envoyé que s'il est visible)
    mcdu_seen: bool,
    /// Touches virtuelles (boutons de la page COMMANDES)
    vkeys: Vec<KeyCode>,
    /// Page 1 composée (CONFIG) pour l'écran principal de la version normale
    layout: dev::DevUi,
    combat: combat::Combat,
    emerg: emergency::Emergency,
    /// Liaison de données avec le Darkstar contrôlé (relais, perte, reconnexion).
    link: link::Link,
    /// Dernière trame reçue avant la perte de liaison (affichage figé).
    stale: Option<Aircraft>,
    /// Écran de connexion à la flotte (démarrage).
    connect: Option<connect::ConnectUi>,
    link_msg: String,
    /// Minimums en approche : 0 rien, 1 « approaching minimums » annoncé, 2 décision attendue, 3 décidé.
    mins: u8,
    /// Hauteur de décision (ft au-dessus du seuil) et annonces déjà faites (bits).
    da_ft: f64,
    calls: u32,
    /// Liaison rétablie après un incident : mode dégradé (cause), manche indisponible, commandes filtrées.
    degraded: Option<mishap::Kind>,
    /// Surveillance des chambres de combustion (courbes et état calculé).
    chambers: health::ChamberMon,
    /// Instructions générées à bord (procédure d'urgence) : pas besoin de liaison.
    onboard: bool,
    /// Roulage automatique du point d'attente vers la piste : (points, index, cap final).
    taxi: Option<(Vec<(f64, f64)>, usize, f64)>,
    /// Terrain du point d'attente occupé.
    hold_apt: Option<usize>,
    /// Aérodrome de départ choisi au lancement
    dep_apt: usize,
    /// Ressenti de vol (mouvements de vue, vibrations, voile).
    feel: feel::Feel,
    bingo_manual: Option<f64>,
    fuel_plan: fuel::Plan,
    fuel_change_t: f64,
    cmd_hist: Vec<String>,
    hist_pos: usize,
}

const BEZEL_PAGES: [gcs::Tab; 6] = [gcs::Tab::Moteurs, gcs::Tab::Alarmes, gcs::Tab::Fcs, gcs::Tab::Limites, gcs::Tab::Elec, gcs::Tab::Thermique];

/// Données de l'horizon (PFD) : alarmes, liaison, objectif, météo, menaces avec missiles simulés, carburant.
fn pfd_info<'a>(g: &Game, alerts: &'a [(Level, String)], ln: &'a str, _lat: f64, warp: f64, time: f64) -> pfd::PfdInfo<'a> {
    let cue = {
        let t = &g.recon.targets[g.recon.selected];
        (!t.done()).then(|| pfd_plus::Cue { lat: t.lat, lon: t.lon, elev: 0.0, label: format!("CIBLE {}", t.code), color: Color::new(1.0, 0.85, 0.2, 1.0) })
    };
    let mut threats = g.recon.threat_cues(&g.ac);
    for c in threats.iter_mut() {
        if let Some(m) = g.combat.missiles.iter().filter(|m| m.site == c.site).min_by(|a, b| a.range.total_cmp(&b.range)) {
            let (la, lo, al) = m.latlon();
            c.missile = Some((la, lo, al, m.tti.max(0.0)));
            c.state = 4;
        }
    }
    let mishap = g.mishap.as_ref().map(|m| (m.decay(), m.elapsed() - m.t_los));
    pfd::PfdInfo { da_ft: if in_approach(&g.ac) { Some(g.da_ft) } else { None }, mishap, alerts, link_name: ln, dest: g.dest, warp, time, cue, wx: g.wx.info.clone(), threats, fuel: g.fuel_plan.clone(), fuel_since: get_time() - g.fuel_change_t }
}

/// Actions demandées sur l'écran Darkstar (vidange, zoom caméra, boutons de lunette).
fn handle_mfd(g: &mut Game, act: mfd::MfdActions) {
    if act.fuel_dump {
        g.apply_switch(gcs::Switch::FuelDump);
    }
    match act.cam {
        Some(recon::CamAction::ZoomIn) => g.recon.zoom = (g.recon.zoom + 1).min(recon::ZOOMS.len() - 1),
        Some(recon::CamAction::ZoomOut) => g.recon.zoom = g.recon.zoom.saturating_sub(1),
        None => {}
    }
    if let Some(b) = act.bezel {
        g.tab = BEZEL_PAGES[b];
    }
}

fn apt_index(icao: &str) -> usize {
    AIRPORTS.iter().position(|a| a.icao == icao).unwrap_or(0)
}

impl Game {
    fn new(scenario: &str) -> Game {
        let mut ac = Aircraft::new_on_runway(apt_index("KPMD"));
        let mut mission = mission::Mission::new();
        let mut dest = apt_index("KEDW");
        match scenario {
            "approche" | "autoland" => {
                // Finale ILS à Edwards, 10 NM, train sorti, turbines en marche
                let apt = &AIRPORTS[dest];
                let (la, lo) = apt.point(-apt.len / 2.0 - 10.0 * NM, 0.0);
                ac.lat = la;
                ac.lon = lo;
                ac.psi = apt.hdg.to_radians();
                ac.alt = apt.elev + 10.0 * NM * 3f64.to_radians().tan() + 15.0;
                ac.tas = 95.0;
                ac.gamma = -3f64.to_radians();
                ac.theta = ac.gamma + 8f64.to_radians();
                ac.on_ground = false;
                ac.brakes = false;
                ac.set_turbines(TurbState::Running, 0.75);
                ac.throttle = 0.35;
                ac.fuel = 8_000.0;
                ac.h2 = 600.0;
                mission.idx = 11;
                if scenario == "autoland" {
                    let (la, lo) = apt.point(-apt.len / 2.0 - 30.0 * NM, 8.0 * NM);
                    ac.lat = la;
                    ac.lon = lo;
                    ac.alt = apt.elev + 7000.0 * FT;
                    ac.gamma = 0.0;
                    ac.theta = 5f64.to_radians();
                    ac.tas = 140.0;
                    ac.gear_down = false;
                    ac.gear_pos = 0.0;
                    let _ = ac.engage_autoland(dest);
                }
            }
            "hyper" => {
                ac.lat = 33.0;
                ac.lon = -125.0;
                ac.psi = 250f64.to_radians();
                ac.alt = 95_000.0 * FT;
                let a = atmosphere::isa(ac.alt).a;
                ac.tas = 6.2 * a;
                ac.gamma = 0.0;
                ac.theta = 4f64.to_radians();
                ac.on_ground = false;
                ac.gear_down = false;
                ac.gear_pos = 0.0;
                ac.brakes = false;
                ac.eng.inlet_open = false;
                ac.eng.doors = 1.0;
                ac.eng.scram = ScramState::Lit;
                ac.throttle = 0.7;
                ac.fuel = 26_000.0;
                ac.h2 = 12_000.0;
                ac.skin_temp = skin_equilibrium(ac.alt, ac.tas);
                mission.idx = 8;
                dest = apt_index("PHNL");
            }
            _ => {}
        }
        Game {
            ac,
            mission,
            map: map::MapView::new(),
            log: vec![(0.0, Level::Info, "GCS EN LIGNE — DATALINK ÉTABLI AVEC DARKSTAR FTV-01".into())],
            warp_idx: 0,
            dest,
            sim_time: 0.0,
            stick: (0.0, 0.0),
            link_buf: VecDeque::new(),
            last_cmd: (0.0, 0.0),
            acked: Vec::new(),
            history: Vec::new(),
            hist_t: 0.0,
            atc: [false; 6],
            console: None,
            update_cmd: false,
            cues: Vec::new(),
            fdr: recorder::Recorder::new(),
            flashes: Vec::new(),
            prev_agl_ft: 0.0,
            prev_mach_int: 0,
            prev_brk: None,
            ended: false,
            debrief: Vec::new(),
            crash_report: Vec::new(),
            mishap: None,
            last_ok: mishap::Snapshot::default(),
            last_ac: None,
            ac_hist: std::collections::VecDeque::new(),
            fms: fms::Fms::new(),
            typing: false,
            tree: None,
            muted: false,
            thermal: thermal::ThermalSys::new(),
            style: svs::SvsStyle::Realiste,
            tab: gcs::Tab::Moteurs,
            bit_timer: 0.0,
            bit_fail: None,
            mfd: mfd::MfdState::new(),
            recon: recon::Recon::new(),
            wx: weather::Weather::new(),
            refuel: refuel::Refuel::new(),
            escort: escort::Escort::new(),
            dev: None,
            mcdu_seen: false,
            vkeys: Vec::new(),
            layout: dev::DevUi::new(),
            combat: combat::Combat::new(),
            emerg: emergency::Emergency::new(),
            link: link::Link::new(),
            stale: None,
            connect: None,
            link_msg: String::new(),
            mins: 0,
            da_ft: 200.0,
            calls: 0,
            degraded: None,
            chambers: health::ChamberMon::new(),
            onboard: false,
            taxi: None,
            hold_apt: None,
            dep_apt: 0,
            feel: feel::Feel::new(),
            bingo_manual: None,
            fuel_plan: fuel::Plan::default(),
            fuel_change_t: -100.0,
            cmd_hist: Vec::new(),
            hist_pos: 0,
        }
    }

    fn flash(&mut self, s: impl Into<String>, c: Color) {
        let s: String = s.into();
        self.flashes.retain(|(t, _, _)| *t != s);
        self.flashes.push((s, c, get_time() + 3.5));
        if self.flashes.len() > 3 {
            self.flashes.remove(0);
        }
    }

    /// Sélectionne les messages à rappeler brièvement sur l'horizon artificiel.
    fn flash_from_message(&mut self, l: Level, s: &str) {
        let short = |m: &str| {
            let m = m.split(" — ").next().unwrap_or(m).trim().to_string();
            if m.chars().count() > 46 { m.chars().take(46).collect() } else { m }
        };
        if let Some(rest) = s.strip_prefix("DARKSTAR : ") {
            if let Some(cmd) = rest.strip_suffix(" — COLLATIONNÉ") {
                self.flash(short(cmd), ui::CYAN_S);
            } else if rest.starts_with("IMPOSSIBLE") {
                self.flash(short(rest), ui::AMBER);
            }
            return;
        }
        let up = s.to_uppercase();
        if up.contains("CAPTURE") || up.contains("LIFTOFF") || up.contains("TOUCHER DES ROUES") || up.contains("AUTOLAND TERMIN") {
            self.flash(short(s), ui::GREEN_S);
        } else if up.contains("POINT D'ESSAI VALIDÉ") || up.contains("CAMPAGNE") {
            self.flash(short(&s.replace("POINT D'ESSAI VALIDÉ : ", "VALIDÉ ")), ui::CYAN_S);
        } else if up.contains("ALLUMÉ") || up.contains("LIGHT-OFF") || up.contains("RALENTI STABILISÉ") {
            self.flash(short(s), WHITE);
        } else if up.contains("RETARD") || up.contains("REMISE DES GAZ") || up.contains("UNSTART") || up.contains("EXTINCTION") || up.contains("AP OFF") || up.contains("DISCONNECT") || (l == Level::Warning && !up.contains("PERTE")) {
            self.flash(short(s), ui::AMBER);
        }
    }

    fn push(&mut self, l: Level, s: impl Into<String>) {
        let s = s.into();
        // Pendant la perte du véhicule, les messages vont au journal (pas d'infos sur l'horizon)
        if self.mishap.is_none() && self.ac.crashed.is_none() {
            self.flash_from_message(l, &s);
        }
        self.fdr.event(self.sim_time, &s);
        self.log.push((self.sim_time, l, s));
        if self.log.len() > 300 {
            self.log.remove(0);
        }
    }

    /// Accélération du temps effective : disponible à toute altitude, y compris au sol
    /// (la physique reste intégrée au pas fixe de 1/120 s, quelle que soit l'accélération).
    fn effective_warp(&self) -> (f64, bool) {
        (WARPS[self.warp_idx], false)
    }

    /// Termine le vol : enregistrement final et débriefing.
    fn end_flight(&mut self) {
        let s = &self.fdr.samples;
        let mut dist = 0.0;
        for w in s.windows(2) {
            dist += dist_brg(w[0].lat, w[0].lon, w[1].lat, w[1].lon).0;
        }
        let maxv = |f: &dyn Fn(&recorder::Sample) -> f64| s.iter().map(|x| f(x)).fold(f64::MIN, f64::max);
        let minv = |f: &dyn Fn(&recorder::Sample) -> f64| s.iter().map(|x| f(x)).fold(f64::MAX, f64::min);
        let ac = &self.ac;
        let (fuel0, h20) = s.first().map(|x| (x.fuel_kg, x.h2_kg)).unwrap_or((ac.fuel, ac.h2));
        let mut d = vec![
            format!("DURÉE {}   DISTANCE {:.0} NM", mission::fmt_time(self.sim_time), dist / NM),
            format!("MACH MAX {:.2}   ALTITUDE MAX {:.0} FT", ac.max_mach, ac.max_alt / FT),
        ];
        if !s.is_empty() {
            d.push(format!(
                "q MAX {:.0} kPa   PEAU MAX {:.0} °C   G {:+.1} / {:+.1}",
                maxv(&|x| x.q_kpa),
                maxv(&|x| x.skin_c),
                minv(&|x| x.nz),
                maxv(&|x| x.nz)
            ));
        }
        d.push(format!("CARBURANT CONSOMMÉ : JP-7 {:.1} t   H2 {:.1} t", (fuel0 - ac.fuel) / 1000.0, (h20 - ac.h2) / 1000.0));
        d.push(format!("POINTS D'ESSAI VALIDÉS : {} / {}", self.mission.completed_at.len(), mission::STEPS.len()));
        for (i, t) in self.mission.completed_at.iter().enumerate() {
            d.push(format!("  {}  {}", mission::fmt_time(*t), mission::STEPS[i].title));
        }
        if let Some((_, m)) = self.fdr.events.iter().rev().find(|(_, m)| m.contains("TOUCHER DES ROUES")) {
            d.push(format!("DERNIER ATTERRISSAGE : {}", m.trim_start_matches("TOUCHER DES ROUES : ")));
        }
        match &ac.crashed {
            Some(c) => d.push(format!("ISSUE : PERTE DU VÉHICULE — {c}")),
            None if ac.on_ground => d.push("ISSUE : AVION AU SOL, VOL TERMINÉ".into()),
            None => d.push(format!("ISSUE : VOL INTERROMPU EN L'AIR ({:.0} FT, M{:.2})", ac.alt / FT, ac.d.mach)),
        }
        self.fdr.finalize();
        if let Some(p) = &self.fdr.saved_path {
            d.push(format!("TRACE : {}", p.display()));
        }
        self.debrief = d;
        self.ended = true;
        self.push(Level::Info, "FIN DE VOL — DÉBRIEFING (ENTRÉE : NOUVEAU VOL)");
    }

    /// Défauts relevés par le test intégré (BIT).
    fn bit_faults(&self) -> Vec<String> {
        let ac = &self.ac;
        let mut f = Vec::new();
        for i in 0..2 {
            let n = ["L", "R"][i];
            if ac.eng.side[i] == TurbState::Failed {
                f.push(format!("ENG {n} HS"));
            }
            if ac.eng.fadec_fault[i] {
                f.push(format!("FADEC {n} EN DÉFAUT"));
            }
        }
        if ac.eng.combustor_failed {
            f.push("CHAMBRE DE COMBUSTION DÉTRUITE".into());
        } else if ac.eng.health < 0.3 {
            f.push(format!("CHAMBRES {} — {}", self.chambers.state.label(), self.chambers.cause));
        }
        for s in self.thermal.sensors.iter().filter(|s| s.failed) {
            f.push(format!("CAPTEUR {} CIRCUIT OUVERT", s.id));
        }
        if ac.adc_degraded() {
            f.push("DONNÉES AIR DÉGRADÉES (AIR DATA OVRD)".into());
        }
        f
    }

    fn start_bit(&mut self) {
        if self.bit_timer > 0.0 {
            return;
        }
        self.bit_timer = 6.0;
        self.bit_fail = None;
        self.push(Level::Info, "INITIATE BIT : TEST INTÉGRÉ EN COURS (FCS, FADEC, CAPTEURS, ADC)");
    }

    fn update_bit(&mut self, real_dt: f64) {
        if self.bit_timer <= 0.0 {
            return;
        }
        self.bit_timer -= real_dt;
        if self.bit_timer <= 0.0 {
            let faults = self.bit_faults();
            self.bit_fail = Some(!faults.is_empty());
            if faults.is_empty() {
                self.push(Level::Info, "BIT OK — AUCUN DÉFAUT");
            } else {
                self.push(Level::Caution, format!("BIT FAIL — {} DÉFAUT(S)", faults.len()));
                for f in faults {
                    self.push(Level::Caution, format!("  BIT : {f}"));
                }
            }
        }
    }

    fn system_reset(&mut self) {
        let ac = &mut self.ac;
        if ac.ovr_fbw {
            ac.cmd_override_fbw(false);
        }
        ac.yaw_trim = 0.0;
        ac.g_max = ac.d.nz.max(1.0);
        ac.g_min = ac.d.nz.min(1.0);
        if ac.eng.scram == ScramState::Unstart {
            ac.eng.scram = ScramState::Armed;
        }
        self.bit_fail = None;
        self.acked.clear();
        self.push(Level::Info, "SYSTEM RESET : CALCULATEURS DE VOL RÉINITIALISÉS (LOI NORMALE, TRIM CENTRÉ, MÉMOIRE G)");
    }

    fn mcdu_out(&mut self, o: fms::McduOut) {
        match o {
            fms::McduOut::Cmd(c) => self.exec_command(&c),
            fms::McduOut::Msg(l, m) => self.push(l, m),
        }
    }

    fn mcdu_key(&mut self, k: fms::McduKey) {
        match k {
            fms::McduKey::Lsk(left, line) => {
                if let Some(o) = self.fms.lsk(left, line, &self.ac) {
                    self.mcdu_out(o);
                }
                if !self.fms.msg.is_empty() && !self.fms.scratch.is_empty() {
                    // message affiché tant que la saisie n'est pas modifiée
                }
            }
            fms::McduKey::Page(p) => {
                self.fms.page = p;
                self.fms.scroll = 0;
                self.fms.msg.clear();
            }
            fms::McduKey::Slew(down) => self.fms.slew(down),
            fms::McduKey::Clr => {
                if !self.fms.msg.is_empty() {
                    self.fms.msg.clear();
                } else if self.fms.scratch.is_empty() {
                    self.fms.scratch = "CLR".into();
                } else {
                    self.fms.scratch.pop();
                }
            }
            fms::McduKey::Focus => self.fms.focus = true,
            fms::McduKey::Char(c) => {
                if self.fms.scratch == "CLR" {
                    self.fms.scratch.clear();
                }
                if self.fms.scratch.len() < 30 {
                    self.fms.scratch.push(c);
                }
                self.fms.msg.clear();
            }
            fms::McduKey::PlusMinus => {
                if self.fms.scratch.ends_with('-') {
                    self.fms.scratch.pop();
                    self.fms.scratch.push('+');
                } else if self.fms.scratch.ends_with('+') {
                    self.fms.scratch.pop();
                    self.fms.scratch.push('-');
                } else {
                    self.fms.scratch.push('-');
                }
            }
            fms::McduKey::Inop(k) => self.fms.msg = format!("{k} NON DISPONIBLE"),
            fms::McduKey::Bright(d) => fms::bright(d),
            fms::McduKey::Airport => {
                self.fms.page = fms::Page::Fpln;
                let n = self.fms.fpln.as_ref().map(|p| p.wpts.len()).unwrap_or(0);
                self.fms.scroll = n.saturating_sub(5);
            }
        }
        self.cues.push(audio::Cue::Click);
    }

    fn set_gcas(&mut self, on: bool) {
        self.ac.gcas_on = on;
        self.push(if on { Level::Info } else { Level::Caution }, if on { "AUTO GCAS ARME : RECUPERATION AUTOMATIQUE AVANT IMPACT" } else { "AUTO GCAS OFF : PLUS DE RECUPERATION AUTOMATIQUE" });
    }

    fn set_fuel_cool(&mut self, m: u8) {
        self.ac.fuel_cool = m.min(2);
        self.push(
            if m == 2 { Level::Caution } else { Level::Info },
            match m {
                0 => "FUEL COOL AUTO : ECHANGEUR CARBURANT / HYDROGENE LIQUIDE + AIR DYNAMIQUE",
                1 => "FUEL COOL RAM AIR : ECHANGEUR AIR DYNAMIQUE SEUL (INEFFICACE AU-DELA DE MACH 3)",
                _ => "FUEL COOL OFF : CARBURANT NON REFROIDI — SURVEILLER FUEL HOT",
            },
        );
    }

    fn set_glim(&mut self, high: bool) {
        self.ac.g_lim_high = high;
        self.push(if high { Level::Caution } else { Level::Info }, if high { "G LIMIT HIGH : LOI A 4 G (2,5 G AU-DELA DE MACH 4)" } else { "G LIMIT NORM : LOI A 3 G (2 G AU-DELA DE MACH 4)" });
    }

    fn set_pitot(&mut self, on: bool) {
        self.ac.pitot_heat = on;
        self.push(if on { Level::Info } else { Level::Caution }, if on { "PITOT HEAT ON : RECHAUFFAGE DES SONDES" } else { "PITOT HEAT OFF : RISQUE DE GIVRAGE DES SONDES PAR AIR FROID" });
    }

    fn apply_menu(&mut self, a: menu::MenuAction) {
        use menu::MenuAction::*;
        match a {
            EngSwitch(i, sw) => self.ac.cmd_eng_switch(i, sw),
            ApuMaster(on) => {
                if on {
                    self.ac.apu.master = true;
                    self.push(Level::Info, "APU MASTER ON");
                } else {
                    self.exec_command("APU STOP");
                }
            }
            ApuStart => {
                if self.ac.apu.master {
                    self.exec_command("APU START");
                } else {
                    self.push(Level::Caution, "APU START : METTRE D'ABORD APU MASTER SUR ON");
                }
            }
            ApuStop => self.exec_command("APU STOP"),
            ApuGen(on) => self.exec_command(if on { "APU GEN ON" } else { "APU GEN OFF" }),
            ApuBleed(on) => self.exec_command(if on { "APU BLEED ON" } else { "APU BLEED OFF" }),
            FadecReset(i) => self.ac.cmd_fadec_reset(i),
            Start => self.ac.cmd_start(),
            Stop => self.ac.cmd_stop(),
            AlertStart(on) => {
                self.ac.eng.alert_start = on;
                self.push(Level::Info, if on { "ALERT START ENABLE : DÉMARRAGE RAPIDE (8 s)" } else { "ALERT START : OFF (DÉMARRAGE NORMAL 25 s)" });
            }
            AirDataOvrd(on) => {
                self.ac.air_data_ovrd = on;
                self.push(Level::Info, if on { "AIR DATA OVRD : DONNÉES INERTIELLES / GPS" } else { "AIR DATA OVRD OFF : SONDES ANÉMOMÉTRIQUES" });
            }
            Bit => self.start_bit(),
            SystemReset => self.system_reset(),
            YawTrim(v) => self.ac.yaw_trim = v,
            Gcas(on) => self.set_gcas(on),
            SpeedBrake(on) => {
                self.ac.speedbrake = on;
                self.push(Level::Info, if on { "AEROFREINS SORTIS" } else { "AEROFREINS RENTRES" });
            }
            AutoTrim(on) => {
                self.ac.auto_trim = on;
                self.push(Level::Info, if on { "AUTO TRIM ON : DISSYMETRIE DE POUSSEE COMPENSEE AU LACET" } else { "AUTO TRIM OFF : YAW TRIM MANUEL" });
            }
            ApDisc => {
                if self.ac.ap.on {
                    self.ac.ap.on = false;
                    self.push(Level::Caution, "AP DISCONNECT — PILOTE AUTOMATIQUE DECONNECTE");
                } else {
                    self.push(Level::Info, "AP DEJA DECONNECTE (instruction AP pour l'engager)");
                }
            }
            FuelCool(m) => self.set_fuel_cool(m),
            GLim(h) => self.set_glim(h),
            PitotHeat(on) => self.set_pitot(on),
            Nws(h) => {
                self.ac.nws_hi = h;
                self.push(Level::Info, if h { "NWS HI : ROULETTE AVANT 30 DEG (MANOEUVRES AU SOL)" } else { "NWS LO : ROULETTE AVANT 12 DEG (ALIGNEMENT, COURSE AU DECOLLAGE)" });
            }
        }
        self.cues.push(audio::Cue::Switch);
    }

    fn apply_switch(&mut self, sw: gcs::Switch) {
        let ac = &mut self.ac;
        match sw {
            gcs::Switch::ScramEnable => {
                if ac.eng.scram == ScramState::Off {
                    ac.cmd_scram();
                } else {
                    ac.eng.scram = ScramState::Off;
                    ac.msg(Level::Info, "SCRAM ENABLE OFF — SCRAMJET COUPÉ");
                }
            }
            gcs::Switch::EngineOverride => {
                let on = !ac.ovr_engine;
                ac.cmd_override_engine(on);
            }
            gcs::Switch::FbwOverride => {
                let on = !ac.ovr_fbw;
                ac.cmd_override_fbw(on);
            }
            gcs::Switch::H2Dump => {
                let on = !ac.h2_dump;
                ac.cmd_h2_dump(on);
            }
            gcs::Switch::FuelDump => {
                let on = !ac.fuel_dump;
                ac.cmd_fuel_dump(on);
            }
        }
        self.cues.push(audio::Cue::Switch);
    }

    fn link(&self) -> (String, f64) {
        match self.link.state {
            link::State::Connected if self.link.path == link::Path::Emerg => ("IRIDIUM".into(), 0.0),
            link::State::Connected => (self.link.path.name(), self.link.latency(&self.ac)),
            link::State::Connecting { .. } => ("CONNEXION".into(), 0.0),
            _ => ("PERDUE".into(), 0.0),
        }
    }

    /// Deuxième chance réussie : le véhicule répond par le secours Iridium. L'autonomie de bord a
    /// stabilisé l'avion, moteurs perdus ; seules les instructions d'urgence passent.
    fn recover_vehicle(&mut self) {
        // Reconnexion immédiate : l'avion reprend exactement l'état de la dernière trame avant la
        // déconnexion (attitude, vitesse, configuration, modes, pannes réelles), liaison rétablie
        // État ~2 s avant l'incident (la dernière trame est souvent déjà irrécupérable : rupture,
        // surpression), sinon la dernière trame valide
        // Dernier état SÛR des 6 dernières secondes (au moins 1 s avant l'incident) : facteur de
        // charge, pression dynamique et incidence dans les limites ; sinon le plus ancien
        let t_inc = self.sim_time;
        let safe = |a: &Aircraft| (-0.5..=3.0).contains(&a.d.nz) && a.d.q < 0.9 * Q_LIMIT && a.d.alpha.to_degrees() < 12.0 && a.skin_temp - 273.15 < SKIN_LIMIT_C;
        let mut ac = self
            .ac_hist
            .iter()
            .rev()
            .filter(|(t, _)| t_inc - t >= 1.0)
            .find(|(_, a)| safe(a))
            .or_else(|| self.ac_hist.front())
            .map(|(_, a)| a.clone())
            .or_else(|| self.last_ac.clone())
            .unwrap_or_else(|| self.ac.clone());
        self.ac_hist.clear();
        // Plus de rotation en cours ni de manœuvre : l'avion repart stabilisé sur sa trajectoire
        ac.calm_rates();
        ac.theta = ac.gamma + ac.d.alpha.clamp(0.0, 4f64.to_radians());
        ac.crashed = None;
        let kind = self.mishap.as_ref().map(|m| m.kind);
        let cause = self.mishap.as_ref().map(|m| m.cause.to_uppercase()).unwrap_or_default();
        self.ac = ac;
        // Dégâts de l'incident, déterminés par sa cause (pas de pannes au hasard)
        let summary = self.incident_damage(kind, &cause);
        // Autonomie de bord : si les gouvernes répondent, l'AP stabilise l'avion (ailes à plat,
        // palier, protections de q actives) en attendant les instructions
        let fcs_ok = !(self.ac.fails.has(systems::Fail::FcsDirect) && self.ac.fails.has(systems::Fail::Hyd));
        if fcs_ok && !self.ac.on_ground {
            let ac = &mut self.ac;
            ac.ap.on = true;
            // Piqué ou q élevée : remise en palier ; sinon tenue d'assiette (pas d'à-coup en scram)
            if ac.gamma < -4f64.to_radians() || ac.d.q > 0.75 * Q_LIMIT {
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (ac.alt / FT / 1000.0).round() * 1000.0;
            } else {
                ac.ap.vert = VertMode::Pitch;
                ac.ap.pitch_tgt = ac.theta.to_degrees();
            }
            ac.ap.lat = LatMode::Hdg;
            ac.ap.hdg_tgt = ac.hdg_deg();
            self.push(Level::Info, "AUTONOMIE DE BORD : AP ENGAGE — AILES A PLAT, ASSIETTE TENUE OU REMISE EN PALIER, PROTECTIONS DE q ACTIVES");
        }
        self.degraded = kind;
        self.mishap = None;
        self.crash_report.clear();
        self.stale = None;
        self.tree = None;
        self.link.state = link::State::Connected;
        self.link.path = link::Path::Emerg;
        self.push(Level::Warning, "RECONNEXION REUSSIE PAR CANAL D'URGENCE (IRIDIUM) — MODE DEGRADE : TELEMESURE, CAMERA, TERMINAL ; MANCHE INDISPONIBLE");
        self.push(Level::Warning, format!("DEGATS : {summary}"));
        // Caméra allumée pour évaluer les dégâts (si le bus électrique le permet)
        let elec = self.ac.fails.has(systems::Fail::GenL) && self.ac.fails.has(systems::Fail::GenR);
        if !elec {
            self.recon.cam_on = true;
            self.push(Level::Info, "CAMERA ON — EVALUATION DES DEGATS");
        }
        self.push(Level::Caution, "INSTRUCTIONS SELON LES DEGATS — EMERGENCY OVERRIDE / EJECTION TOUJOURS POSSIBLES");
    }

    /// Dégâts conservés après la reconnexion, selon l'incident. Renvoie leur résumé.
    fn incident_damage(&mut self, kind: Option<mishap::Kind>, cause: &str) -> String {
        use mishap::Kind;
        use systems::Fail;
        let ac = &mut self.ac;
        let dmg = |ac: &mut Aircraft, f: Fail, on: bool| {
            ac.fails.set(f, on);
            ac.damage.set(f, on);
        };
        let kill_engines = |ac: &mut Aircraft| {
            for k in 0..2 {
                ac.eng.side[k] = TurbState::Failed;
                ac.eng.n2s[k] = 0.0;
            }
            ac.eng.turb = TurbState::Failed;
            ac.eng.ram = RamState::Off;
            ac.eng.scram = ScramState::Off;
            ac.throttle = 0.0;
        };
        let fcs_lost = |ac: &mut Aircraft| {
            dmg(ac, Fail::FcsDirect, true);
            dmg(ac, Fail::Hyd, true);
            ac.ap.on = false;
        };
        match kind {
            Some(Kind::Structural) if cause.contains("THERMIQUE") => {
                dmg(ac, Fail::Adc, true);
                dmg(ac, Fail::FcsDirect, true);
                "STRUCTURE CHAUFFEE, SONDES DETRUITES — FCS EN LOI DIRECTE, PILOTAGE PAR INSTRUCTIONS".into()
            }
            Some(Kind::Structural) if cause.contains("PRESSION") => {
                dmg(ac, Fail::Adc, true);
                dmg(ac, Fail::FcsDirect, true);
                "SURPRESSION : SONDES ET REVETEMENT ENDOMMAGES — FCS EN LOI DIRECTE (SANS PROTECTIONS), PILOTAGE PAR INSTRUCTIONS".into()
            }
            Some(Kind::Structural) => {
                fcs_lost(ac);
                "GOUVERNES / HYDRAULIQUE — PLUS DE FCS : AUCUNE INSTRUCTION DE PILOTAGE, MOTEURS DISPONIBLES".into()
            }
            Some(Kind::Explosion) => {
                kill_engines(ac);
                ac.eng.combustor_failed = true;
                dmg(ac, Fail::H2Leak, true);
                "CHAMBRE DETRUITE, DEUX MOTEURS PERDUS, FUITE H2 — FCS DISPONIBLE (VOL PLANE)".into()
            }
            Some(Kind::Missile) => {
                kill_engines(ac);
                dmg(ac, Fail::FuelPump, true);
                fcs_lost(ac);
                "MOTEURS, CIRCUIT CARBURANT ET HYDRAULIQUE — PLUS DE FCS NI DE POUSSEE".into()
            }
            Some(Kind::Electrical) => {
                dmg(ac, Fail::GenL, true);
                dmg(ac, Fail::GenR, true);
                "GENERATRICES PERDUES — CAPTEURS HORS TENSION (PAS DE CAMERA), FCS SUR BATTERIE".into()
            }
            _ => "AUCUN".into(),
        }
    }

    /// Instructions impossibles en mode dégradé, selon les dégâts.
    fn degraded_refusal(&self, cmd: &str) -> Option<&'static str> {
        let k = self.degraded?;
        const FLIGHT: [&str; 20] = ["CAP", "HDG", "ALT", "FL", "MACH", "SPD", "VITESSE", "PITCH", "ASSIETTE", "AP", "DIRECT", "DCT", "DIR", "LAND", "ATTERRIR", "AUTOLAND", "ATTERRISSAGE", "DESCENTE", "TAKEOFF", "DECOLLAGE"];
        const ENGINE: [&str; 10] = ["TURB", "TURBINE", "TURBINES", "STATO", "SCRAM", "THR", "MANETTE", "PC", "AB", "INLET"];
        const SENSORS: [&str; 8] = ["CAMERA", "CAM", "ZOOM", "SAR", "PHOTO", "ELINT", "WXR", "RADAR-METEO"];
        let fcs = self.ac.fails.has(systems::Fail::FcsDirect) && self.ac.fails.has(systems::Fail::Hyd);
        let engines = self.ac.eng.side.iter().all(|s| *s == TurbState::Failed) || self.ac.eng.combustor_failed;
        let elec = self.ac.fails.has(systems::Fail::GenL) && self.ac.fails.has(systems::Fail::GenR);
        let _ = k;
        if fcs && FLIGHT.contains(&cmd) {
            Some("GOUVERNES / HYDRAULIQUE ENDOMMAGEES : PILOTAGE NON DISPONIBLE — EMERGENCY OVERRIDE OU EJECTION")
        } else if engines && ENGINE.contains(&cmd) {
            Some("MOTEURS DETRUITS : COMMANDE MOTEUR SANS EFFET")
        } else if elec && SENSORS.contains(&cmd) {
            Some("BUS ELECTRIQUE DEGRADE : CAPTEURS HORS TENSION")
        } else {
            None
        }
    }

    /// Place l'appareil choisi sur l'aérodrome `ai`, aligné au seuil de la piste `rwy`.
    fn place_at(&mut self, fleet: usize, ai: usize, rwy: &str) {
        link::set_tail(fleet);
        let mut ac = Aircraft::new_on_runway(ai);
        let a = &AIRPORTS[ai];
        if let Some(e) = procs::find_end(a, rwy) {
            let (tla, tlo) = procs::threshold(a, &e);
            let (la, lo) = destination(tla, tlo, e.hdg, 120.0);
            ac.lat = la;
            ac.lon = lo;
            ac.psi = e.hdg.to_radians();
        }
        self.taxi = None;
        self.hold_apt = None;
        self.ac = ac;
        self.stale = None;
        self.dep_apt = ai;
    }

    /// Place l'appareil choisi sur son terrain : aligné sur la piste ou au point d'attente.
    #[allow(dead_code)]
    fn place(&mut self, fleet: usize, hold: bool) {
        link::set_tail(fleet);
        let ai = apt_index(link::FLEET[fleet].1);
        let mut ac = Aircraft::new_on_runway(ai);
        self.taxi = None;
        self.hold_apt = None;
        if hold {
            let a = &AIRPORTS[ai];
            let along = a.len / 2.0 - 160.0;
            let (la, lo) = a.point(along, a.width / 2.0 + 110.0);
            let (cla, clo) = a.point(along, 0.0);
            ac.lat = la;
            ac.lon = lo;
            ac.psi = dist_brg(la, lo, cla, clo).1.to_radians();
            self.hold_apt = Some(ai);
        }
        self.ac = ac;
        self.stale = None;
    }

    fn alerts(&self) -> Vec<(Level, String)> {
        let ac = &self.ac;
        let d = &ac.d;
        let mut a = Vec::new();
        if ac.crashed.is_some() {
            return a;
        }
        a.extend(ac.fire.alerts());
        let agl_ft = d.agl / FT;
        let fpm = d.vs / FT * 60.0;
        if !ac.on_ground && !ac.landing_config() && agl_ft < 2500.0 && -fpm > agl_ft * 1.4 + 800.0 {
            a.push((Level::Warning, "PULL UP".to_string()));
        }
        if ac.win_state > 0 || ac.win_temp > 1_150.0 {
            a.push((if ac.win_state > 0 { Level::Warning } else { Level::Caution }, "WINDOW HOT".to_string()));
        }
        if !ac.on_ground && d.alpha.to_degrees() > 17.0 && d.mach < 1.2 {
            a.push((Level::Warning, "STALL".to_string()));
        }
        if d.q > Q_LIMIT {
            a.push((Level::Warning, "OVERSPEED — q LIMIT".to_string()));
        }
        if ac.skin_temp - 273.15 > SKIN_LIMIT_C {
            a.push((Level::Warning, "SKIN TEMP".to_string()));
        }
        if ac.eng.scram == ScramState::Unstart {
            a.push((Level::Warning, "SCRAM UNSTART".to_string()));
        }
        if ac.eng.overtemp_t > 0.0 {
            a.push((Level::Warning, "TURBINE OVERTEMP".to_string()));
        }
        if !ac.on_ground && agl_ft < 600.0 && ac.gear_pos < 0.99 && fpm < -200.0 {
            a.push((Level::Warning, "TOO LOW — GEAR".to_string()));
        }
        if ac.fuel < FUEL_MAX * 0.1 {
            a.push((Level::Caution, "FUEL LOW".to_string()));
        }
        if ac.h2_dump {
            a.push((Level::Caution, "H2 DUMP".to_string()));
        }
        if ac.eng.scram == ScramState::Lit && ac.h2 < H2_MAX * 0.1 {
            a.push((Level::Caution, "H2 LOW".to_string()));
        }
        if !(21.0..=29.0).contains(&ac.cg()) {
            a.push((Level::Caution, "CG LIMIT".to_string()));
        }
        if ac.gear_pos > 0.01 && d.keas > GEAR_VLE {
            a.push((Level::Caution, "GEAR OVERSPEED".to_string()));
        }
        if ac.phi.abs() > 45f64.to_radians() {
            a.push((Level::Caution, "BANK ANGLE".to_string()));
        }
        if ac.eng.inlet_open && d.mach > 2.6 {
            a.push((Level::Caution, "TURB INLET OPEN > M2.6".to_string()));
        }
        if ac.on_ground && ac.brakes && ac.throttle > 0.5 {
            a.push((Level::Caution, "PARKING BRAKE".to_string()));
        }
        if !ac.on_ground && ac.d.thrust < 1.0 && ac.d.mach > 0.3 {
            a.push((Level::Caution, "NO THRUST".to_string()));
        }
        if ac.eng.combustor_failed {
            a.push((Level::Warning, "COMBUSTOR FAIL".to_string()));
        } else if ac.eng.health < 0.3 {
            a.push((Level::Caution, "ENGINE DEGRADED".to_string()));
        }
        if ac.ovr_engine {
            a.push((Level::Caution, "ENG OVRD".to_string()));
        } else if ac.fadec_limit < 0.995 {
            a.push((Level::Caution, "FADEC LIMIT".to_string()));
        }
        if ac.ovr_fbw {
            a.push((Level::Caution, "FBW OVRD".to_string()));
        }
        for i in 0..2 {
            if ac.eng.fadec_fault[i] {
                a.push((Level::Caution, format!("FADEC FAULT {}", ["L", "R"][i])));
            }
        }
        if ac.adc_degraded() {
            a.push((Level::Caution, "AIR DATA".to_string()));
        }
        if self.bit_fail == Some(true) {
            a.push((Level::Caution, "BIT FAIL".to_string()));
        }
        if !ac.on_ground {
            match self.fuel_plan.level {
                fuel::Level::Bingo => a.push((Level::Caution, "BINGO FUEL".to_string())),
                fuel::Level::MinFuel => a.push((Level::Warning, "MIN FUEL".to_string())),
                fuel::Level::Emergency => a.push((Level::Warning, "EMERGENCY FUEL".to_string())),
                _ => {}
            }
            if self.fuel_plan.h2_bingo {
                a.push((Level::Caution, "H2 BINGO".to_string()));
            }
        }
        if ac.fuel_dump {
            a.push((Level::Caution, "FUEL DUMP".to_string()));
        }
        a.extend(self.thermal.alerts());
        a.extend(self.recon.alerts());
        a.extend(self.combat.alerts());
        a
    }

    /// Manche seul (flèches) pendant une saisie MCDU.
    fn input_stick(&mut self, real_dt: f64) {
        self.typing = true;
        self.input(real_dt);
        self.typing = false;
    }

    fn input(&mut self, real_dt: f64) {
        // Boutons de la page COMMANDES : mêmes actions que les raccourcis
        let vk = std::mem::take(&mut self.vkeys);
        let kp = |k: KeyCode| is_key_pressed(k) || vk.contains(&k);
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let ac = &mut self.ac;

        // Manche (clavier lissé)
        let fine = if shift { 0.3 } else { 1.0 };
        let mut tp = 0.0;
        let mut tr = 0.0;
        if !ac.ap.on {
            if is_key_down(KeyCode::Down) {
                tp += fine;
            }
            if is_key_down(KeyCode::Up) {
                tp -= fine;
            }
        }
        if !ac.ap.on || ac.on_ground {
            if is_key_down(KeyCode::Right) {
                tr += fine;
            }
            if is_key_down(KeyCode::Left) {
                tr -= fine;
            }
        }
        let mv = |cur: f64, tgt: f64| {
            let rate = if tgt == 0.0 { 6.0 } else { 3.0 };
            cur + (tgt - cur).clamp(-rate * real_dt, rate * real_dt)
        };
        self.stick = (mv(self.stick.0, tp), mv(self.stick.1, tr));
        if self.typing {
            return;
        }

        // Consignes AP
        if ac.ap.on && ac.autoland.is_none() {
            let k = if shift { 10_000.0 } else { 1_000.0 };
            if ac.ap.vert == VertMode::Pitch {
                let pk = if shift { 5.0 } else { 1.0 };
                if is_key_pressed(KeyCode::Up) {
                    ac.ap.pitch_tgt = (ac.ap.pitch_tgt + pk).min(30.0);
                }
                if is_key_pressed(KeyCode::Down) {
                    ac.ap.pitch_tgt = (ac.ap.pitch_tgt - pk).max(-20.0);
                }
            } else {
                if is_key_pressed(KeyCode::Up) {
                    ac.ap.alt_tgt_ft = (ac.ap.alt_tgt_ft + k).min(130_000.0);
                    ac.ap.vert = VertMode::Alt;
                }
                if is_key_pressed(KeyCode::Down) {
                    ac.ap.alt_tgt_ft = (ac.ap.alt_tgt_ft - k).max(1_000.0);
                    ac.ap.vert = VertMode::Alt;
                }
            }
            let hk = if shift { 30.0 } else { 5.0 };
            if is_key_pressed(KeyCode::Right) && !ac.on_ground {
                ac.ap.hdg_tgt = wrap360(ac.ap.hdg_tgt + hk);
                ac.ap.lat = LatMode::Hdg;
            }
            if is_key_pressed(KeyCode::Left) && !ac.on_ground {
                ac.ap.hdg_tgt = wrap360(ac.ap.hdg_tgt - hk);
                ac.ap.lat = LatMode::Hdg;
            }
        }

        let mut msgs: Vec<(Level, String)> = Vec::new();
        // Manette : R/F agissent toujours sur la manette ; l'A/THR se désengage (consignes via MACH / KEAS)
        let thr_up = is_key_down(KeyCode::R);
        let thr_dn = is_key_down(KeyCode::F);
        if (thr_up || thr_dn) && ac.autoland.is_none() {
            ac.thr_manual = true;
        }
        if (thr_up || thr_dn) && ac.ap.at_on && ac.autoland.is_none() {
            ac.ap.at_on = false;
            msgs.push((Level::Caution, "A/THR OFF — MANETTE MANUELLE (consigne : instruction MACH ou KEAS)".into()));
        }
        if !ac.ap.at_on || ac.autoland.is_none() {
            if thr_up {
                ac.throttle = (ac.throttle + 0.35 * real_dt).min(ac.thr_max());
            }
            if thr_dn {
                ac.throttle = (ac.throttle - 0.35 * real_dt).max(0.0);
            }
        }

        if kp(KeyCode::T) || is_key_pressed(KeyCode::Key1) {
            ac.cmd_turbine();
        }
        if kp(KeyCode::I) {
            ac.cmd_inlet();
        }
        if kp(KeyCode::Y) || is_key_pressed(KeyCode::Key2) {
            ac.cmd_ram();
        }
        if kp(KeyCode::U) || is_key_pressed(KeyCode::Key3) {
            ac.cmd_scram();
        }
        if kp(KeyCode::G) {
            ac.cmd_gear();
        }
        if kp(KeyCode::B) {
            ac.brakes = !ac.brakes;
        }
        if kp(KeyCode::S) {
            ac.speedbrake = !ac.speedbrake;
        }
        if kp(KeyCode::P) {
            if ac.on_ground {
                msgs.push((Level::Caution, "AP INDISPONIBLE AU SOL".into()));
            } else if ac.ap.on {
                ac.ap.on = false;
                msgs.push((Level::Caution, "AUTOPILOT DISCONNECT".into()));
            } else {
                ac.ap.on = true;
                ac.ap.lat = LatMode::Hdg;
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
                ac.ap.hdg_tgt = ac.hdg_deg().round();
                msgs.push((Level::Info, "AP1 ENGAGÉ — ALT HOLD / HDG HOLD".into()));
            }
        }
        if kp(KeyCode::O) {
            ac.ap.at_on = !ac.ap.at_on;
            if ac.ap.at_on {
                ac.ap.mach_tgt = (ac.d.mach * 10.0).round() / 10.0;
                msgs.push((Level::Info, format!("A/THR ENGAGÉE — MACH {:.1}", ac.ap.mach_tgt)));
            } else {
                msgs.push((Level::Caution, "A/THR OFF".into()));
            }
        }
        if kp(KeyCode::N) && !ac.on_ground {
            if !ac.ap.on {
                ac.ap.on = true;
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
            }
            ac.ap.lat = LatMode::Nav;
            let m = match self.fms.fpln.as_ref().and_then(|p| p.wpts.get(p.active)) {
                Some(w) => format!("LNAV — PLAN DE VOL FMS, CAP SUR {}{}", w.ident, if self.fms.vnav { " (VNAV ON)" } else { "" }),
                None => format!("NAV — DIRECT {} (pas de plan FMS actif)", AIRPORTS[self.dest].icao),
            };
            msgs.push((Level::Info, m));
        }
        if kp(KeyCode::L) && !ac.on_ground {
            if let Some((_, tla, tlo, th, tel)) = ac.approach {
                if !ac.ap.on {
                    // AP engagé sur le cap et l'altitude du moment jusqu'à la capture
                    ac.ap.on = true;
                    ac.ap.lat = LatMode::Hdg;
                    ac.ap.hdg_tgt = ac.hdg_deg().round();
                    ac.ap.vert = VertMode::Alt;
                    ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
                }
                ac.ap.loc_armed = true;
                ac.ap.gs_armed = true;
                ac.ap.at_on = true;
                ac.ap.spd_managed = true;
                msgs.push((Level::Info, "ILS ARMÉ (LOC* G/S*) — CAPTURE LOC DANS L'AXE < 25 NM, G/S PAR EN DESSOUS ; A/THR VITESSE GÉRÉE".into()));
                let along = along_before(tla, tlo, th, ac.lat, ac.lon);
                let path = tel + (along + 300.0) * 3f64.to_radians().tan();
                if ac.d.mach > 0.9 || ac.alt > path + 3_000.0 * FT {
                    msgs.push((
                        Level::Caution,
                        format!(
                            "ATTENTION : {:.0} FT AU-DESSUS DU PLAN À M{:.2} — DESCENDRE (DESCENTE / ALT) OU UTILISER ATTERRIR (L'AUTOLAND GÈRE L'ÉNERGIE)",
                            (ac.alt - path) / FT,
                            ac.d.mach
                        ),
                    ));
                }
            } else {
                msgs.push((Level::Caution, "ILS HORS PORTÉE (< 80 km de la destination)".into()));
            }
        }
        if kp(KeyCode::X) {
            if self.refuel.active() {
                self.refuel.stop(ac, "REPRISE EN MAIN");
            }
            ac.ap.on = false;
            ac.ap.at_on = false;
            ac.autoland = None;
            ac.autotakeoff = None;
            msgs.push((Level::Caution, "REPRISE EN MAIN MANUELLE — AP / A/THR / AUTOLAND OFF".into()));
        }
        if kp(KeyCode::E) {
            self.warp_idx = (self.warp_idx + 1) % WARPS.len();
        }
        if kp(KeyCode::C) {
            self.map.follow = true;
        }
        let page_keys = [KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9, KeyCode::Key0];
        for (k, (tab, _)) in page_keys.iter().zip(gcs::TABS.iter()) {
            let (k, tab) = (*k, *tab);
            if is_key_pressed(k) {
                self.tab = tab;
            }
        }
        if kp(KeyCode::J) {
            self.muted = !self.muted;
        }
        for (l, s) in msgs {
            self.push(l, s);
        }
    }

    fn update(&mut self, real_dt: f64) {
        // Limitation de l'accélération du temps près du sol
        let (warp, _) = self.effective_warp();

        // Latence de la liaison de données sur les commandes de vol
        let (_, lat_ms) = self.link();
        let now = get_time();
        // Sans liaison complète, les ordres de manche ne sont pas transmis
        let st = if self.link.full() && self.degraded.is_none() { self.stick } else { (0.0, 0.0) };
        self.link_buf.push_back((now, st.0, st.1));
        while let Some(&(t, p, r)) = self.link_buf.front() {
            if now - t >= lat_ms / 1000.0 {
                self.last_cmd = (p, r);
                self.link_buf.pop_front();
            } else {
                break;
            }
        }
        let cmd = self.last_cmd;

        // Approche / destination
        let apt = &AIRPORTS[self.dest];
        let (dd, brg) = dist_brg(self.ac.lat, self.ac.lon, apt.lat, apt.lon);
        // Sens d'approche figé pendant l'autoland et dès qu'un ILS est armé ou capturé
        let ils_active = self.ac.ap.loc_armed || self.ac.ap.gs_armed || self.ac.ap.lat == LatMode::Loc || matches!(self.ac.ap.vert, VertMode::Gs | VertMode::Flare);
        self.ac.approach = if self.ac.autoland.is_some() || (ils_active && self.ac.approach.is_some()) {
            self.ac.approach
        } else if dd < 80_000.0 {
            // ILS calé par le FMS sur la piste choisie, sinon piste dans l'axe de l'avion
            let tuned = self.fms.ils.as_ref().filter(|t| t.apt == apt.icao).and_then(|t| procs::find_end(apt, &t.rwy));
            match tuned {
                Some(e) => {
                    let (tla, tlo) = procs::threshold(apt, &e);
                    Some((self.dest, tla, tlo, e.hdg, apt.elev))
                }
                None => {
                    let a = apt.approach_for(self.ac.lat, self.ac.lon);
                    Some((self.dest, a.thr_lat, a.thr_lon, a.hdg, apt.elev))
                }
            }
        } else {
            None
        };
        if self.ac.ap.lat == LatMode::Nav {
            // Plan de vol FMS actif : poursuite du point actif ; sinon direct destination
            match self.fms.guidance(&self.ac) {
                Some((b, seq)) => {
                    self.ac.ap.hdg_tgt = b;
                    if let Some(m) = seq {
                        self.push(Level::Info, m);
                        self.cues.push(audio::Cue::Seq);
                    }
                }
                None => self.ac.ap.hdg_tgt = brg,
            }
        }
        if let Some(o) = self.fms.poll() {
            self.mcdu_out(o);
        }
        // Base de navigation : fin de téléchargement annoncée une fois
        if let Some(u) = self.fms.navupd.clone() {
            let st = u.lock().unwrap().clone();
            match st {
                navdb::Upd::Done(m) => {
                    *u.lock().unwrap() = navdb::Upd::Idle;
                    self.push(Level::Info, format!("NAVDATA A JOUR : {m} — REDEMARRER POUR L'APPLIQUER"));
                }
                navdb::Upd::Error(e) => {
                    *u.lock().unwrap() = navdb::Upd::Idle;
                    self.push(Level::Caution, format!("NAVDATA : {e}"));
                }
                _ => {}
            }
        }
        if let Some(m) = self.fms.autotune(&self.ac) {
            self.push(Level::Info, m);
        }
        pfd_plus::set_ils(self.fms.ils.as_ref().map(|t| format!("{} {:.2} {} {}", t.id, t.mhz, t.apt, t.rwy)));
        pfd_plus::set_ifr(self.fms.ifr_panel(&self.ac, self.da_ft));
        pfd_plus::set_lnav(self.fms.fpln.as_ref().and_then(|p| p.wpts.get(p.active)).map(|w| w.ident.clone()));
        // Piste d'arrivée du plan de vol (SimBrief) pour l'autoland
        self.ac.rwy_pref = self.fms.fpln.as_ref().map(|p| p.arr_rwy.clone()).unwrap_or_default();
        // En urgence, l'avion actionne lui-même train et aérofreins (l'opérateur n'a peut-être plus la main)
        self.ac.auto_config = self.emerg.mode.is_some();
        // Pendant une urgence (override, déroutement, plané), le FMS ne pilote plus
        let vnav_out = if self.emerg.mode.is_some() { Vec::new() } else { self.fms.vnav_update(&mut self.ac) };
        for o in vnav_out {
            self.mcdu_out(o);
        }
        if self.ac.ap.lat == LatMode::Loc && self.ac.approach.is_none() {
            self.ac.ap.lat = LatMode::Hdg;
            self.ac.ap.vert = VertMode::Alt;
        }

        let sim_dt = real_dt * warp;
        // Météo : vent et turbulence au point de l'avion
        self.wx.update(&mut self.ac, sim_dt);
        let n = (sim_dt / PHYS_DT).ceil().max(1.0) as usize;
        let h = sim_dt / n as f64;
        let was_ground = self.ac.on_ground;
        for _ in 0..n {
            let c = self.ac.autoflight().unwrap_or(cmd);
            self.ac.step(h, c.0, c.1);
        }
        if was_ground && !self.ac.on_ground {
            self.ac.landed = false;
        }
        if !was_ground && self.ac.on_ground && self.ac.crashed.is_none() {
            self.ac.landed = true;
        }
        self.sim_time += sim_dt;
        // Incendie : risques d'inflammation, évolution des feux
        let mut fs = std::mem::take(&mut self.ac.fire);
        let fm = fs.update(&mut self.ac, sim_dt);
        self.ac.fire = fs;
        if !fm.is_empty() {
            self.tab = gcs::Tab::Incendie;
        }
        for (l, m) in fm {
            self.push(l, m);
        }
        // Ravitaillement en vol : ravitailleur, rendez-vous, maintien de position sous la perche
        self.refuel.update(sim_dt, &mut self.ac);
        let rfe: Vec<_> = self.refuel.events.drain(..).collect();
        for (l, m) in rfe {
            self.push(l, m);
        }
        // Escorte de chasse (départ / arrivée sur turbines)
        let dest_ll = self.fms.fpln.as_ref().and_then(|p| AIRPORTS.iter().find(|a| a.icao == p.dest)).or_else(|| AIRPORTS.get(self.dest)).map(|a| (a.lat, a.lon));
        let desc = self.fms.desc_started || self.ac.approach.is_some() || self.ac.autoland.is_some();
        for (l, m) in self.escort.update(&self.ac, sim_dt, dest_ll, desc) {
            self.push(l, m);
        }
        escort::set_render(&self.escort.wings);
        self.map.traffic = self.escort.traffic();
        // Procédure d'urgence : décisions et instructions automatiques
        self.emerg.update(sim_dt, &mut self.ac, &self.refuel);
        for c in std::mem::take(&mut self.emerg.cmds) {
            self.onboard = true;
            self.exec_command(&c);
            self.onboard = false;
        }
        let eev: Vec<_> = self.emerg.events.drain(..).collect();
        for (l, m) in eev {
            self.push(l, m);
        }
        // Carburant : bingo / joker / minimum, annonces sur changement de niveau
        let tk = self.refuel.tanker.as_ref().map(|t| (t.callsign, t.lat, t.lon));
        let plan = fuel::plan(&self.ac, self.dest, self.bingo_manual, tk);
        let (prev, prev_h2) = (self.fuel_plan.level, self.fuel_plan.h2_bingo);
        if !self.ac.on_ground && self.ac.crashed.is_none() && (plan.level != prev || plan.h2_bingo != prev_h2) {
            self.fuel_change_t = get_time();
            if plan.level > prev {
                let (icao, brg, dist, _, _) = plan.rtb;
                let lvl = if plan.level >= fuel::Level::MinFuel { Level::Warning } else if plan.level == fuel::Level::Bingo { Level::Caution } else { Level::Info };
                self.push(lvl, format!("{} — JP7 {:.1} t (BINGO {:.1} t) — RTB {icao} CAP {brg:03.0} {:.0} NM", plan.level.name(), self.ac.fuel / 1000.0, plan.bingo / 1000.0, dist / NM));
            } else if plan.level < prev && plan.level == fuel::Level::Ok {
                self.push(Level::Info, "CARBURANT : AU-DESSUS DU JOKER");
            }
            if plan.h2_bingo && !prev_h2 {
                self.push(Level::Caution, "H2 BINGO — HYDROGENE POUR MOINS DE 3 MIN — DECELERER");
            }
        }
        self.fuel_plan = plan;

        self.thermal.update(sim_dt, &self.ac);
        let tev: Vec<_> = self.thermal.events.drain(..).collect();
        for (l, m) in tev {
            self.push(l, m);
        }
        self.map.record(&self.ac, sim_dt);
        self.hist_t -= sim_dt;
        if self.hist_t <= 0.0 && !self.ac.on_ground {
            self.hist_t = 2.0;
            self.history.push((self.ac.d.mach as f32, (self.ac.alt / FT) as f32));
            if self.history.len() > 4000 {
                self.history.remove(0);
            }
        }

        if self.ac.crashed.is_none() {
            self.chambers.update(&self.ac, sim_dt);
        }
        // Liaison de données : bascules, perte, procédure autonome
        if self.ac.crashed.is_none() {
            let lost_now = self.link.update(&self.ac, real_dt, sim_dt);
            if lost_now {
                self.stale = Some(self.ac.clone());
                let ac = &mut self.ac;
                if !ac.on_ground && ac.autoland.is_none() && self.emerg.mode.is_none() {
                    ac.ap.on = true;
                    ac.ap.lat = LatMode::Hdg;
                    ac.ap.hdg_tgt = ac.hdg_deg().round();
                    ac.ap.vert = VertMode::Alt;
                    ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
                }
            }
            if self.link.state == link::State::Connected {
                self.stale = None;
            } else if self.stale.is_none() {
                self.stale = Some(self.ac.clone());
            }
            // Procédure autonome : après 5 min sans liaison, l'avion déclenche lui-même l'urgence
            if self.link.state == link::State::Lost && self.link.lost_t > 300.0 && self.emerg.mode.is_none() && !self.ac.on_ground && self.ac.autoland.is_none() {
                self.push(Level::Warning, "PERTE DE LIAISON PROLONGEE — URGENCE AUTONOME DECLENCHEE PAR L'AVION");
                if let Ok(m) = self.emerg.start(&self.ac, &self.refuel) {
                    self.push(Level::Warning, m);
                }
            }
        }
        let lev: Vec<_> = self.link.events.drain(..).collect();
        for (l, s) in lev {
            self.link_msg = s.clone();
            self.push(l, s);
        }
        // Minimums (hauteur de décision 200 ft au-dessus du seuil) en approche ILS / autoland
        match self.ac.approach.filter(|_| in_approach(&self.ac)) {
            Some((_, _, _, _, tel)) => {
                let h = (self.ac.alt - tel) / FT;
                let dead = self.ac.autoland.as_ref().is_some_and(|a| a.dead_stick);
                let da = self.da_ft;
                // Annonces de hauteur (une fois chacune en descente)
                if h > 1_500.0 {
                    self.calls = 0;
                } else if self.ac.d.vs < 0.0 {
                    let radio = self.ac.d.agl / FT;
                    for (k, (lim, txt)) in [(1_000.0, "1000"), (500.0, "500"), (da + 100.0, "100 ABOVE")].iter().enumerate() {
                        if h < *lim && h > lim - 150.0 && self.calls & (1 << k) == 0 && *lim > da {
                            self.calls |= 1 << k;
                            self.flash(*txt, WHITE);
                        }
                    }
                    for (k, (lim, txt)) in [(50.0, "50"), (40.0, "40"), (30.0, "30"), (20.0, "20"), (10.0, "10"), (5.0, "RETARD")].iter().enumerate() {
                        let bit = 1 << (8 + k);
                        if radio < *lim && self.calls & bit == 0 {
                            self.calls |= bit;
                            self.flash(*txt, if *txt == "RETARD" { ui::AMBER } else { WHITE });
                        }
                    }
                }
                if h > 1_500.0 {
                    self.mins = 0;
                } else if !dead && self.mins == 0 && h < da + 100.0 && self.ac.d.vs < 0.0 {
                    self.mins = 1;
                    self.flash("APPROACHING MINIMUMS", ui::AMBER);
                    self.cues.push(audio::Cue::Ack);
                } else if !dead && self.mins == 1 && h < da {
                    self.mins = 2;
                    self.flash("MINIMUMS", ui::AMBER);
                    self.cues.push(audio::Cue::Minimums);
                    self.push(Level::Caution, "MINIMUMS — DECISION : CONTINUE OU GO AROUND");
                } else if self.mins == 2 && h < (da * 0.3).min(60.0) {
                    // Sans décision avant 60 ft : l'approche continue
                    self.mins = 3;
                    self.push(Level::Info, "MINIMUMS : PAS DE DECISION — APPROCHE CONTINUEE");
                }
            }
            None => self.mins = 0,
        }
        // Roulage automatique vers la piste
        if let Some((pts, i, fin)) = &mut self.taxi {
            let ac = &mut self.ac;
            ac.tas = 0.0;
            ac.brakes = true;
            let turn = 20f64.to_radians() * sim_dt;
            if *i < pts.len() {
                let (d, brg) = dist_brg(ac.lat, ac.lon, pts[*i].0, pts[*i].1);
                let err = wrap180(brg - ac.hdg_deg()).to_radians();
                ac.psi = (ac.psi + err.clamp(-turn, turn)).rem_euclid(std::f64::consts::TAU);
                if err.abs() < 30f64.to_radians() {
                    let step = (6.0 * sim_dt).min(d);
                    let (la, lo) = destination(ac.lat, ac.lon, brg, step);
                    ac.lat = la;
                    ac.lon = lo;
                }
                if d < 2.0 {
                    *i += 1;
                }
            } else {
                let err = wrap180(*fin - ac.hdg_deg()).to_radians();
                ac.psi = (ac.psi + err.clamp(-turn, turn)).rem_euclid(std::f64::consts::TAU);
                if err.abs() < 0.5f64.to_radians() {
                    let num = match self.hold_apt { Some(ai) => AIRPORTS[ai].end_ident(*fin, Some((ac.lat, ac.lon))), None => format!("{:02}", airports::Airport::runway_number(*fin)) };
                    self.taxi = None;
                    self.hold_apt = None;
                    self.flash(format!("ALIGNE PISTE {num} — PRET AU DECOLLAGE"), ui::GREEN_S);
                    self.push(Level::Info, format!("TOUR : {}, ALIGNE PISTE {num}, AUTORISE DECOLLAGE A VOTRE CONVENANCE", link::tail()));
                }
            }
        }
        let ev: Vec<_> = self.ac.events.drain(..).collect();
        for (l, s) in ev {
            self.push(l, s);
        }
        let los = self.link().0 == "LOS";
        self.recon.update(sim_dt, &mut self.ac, self.mission.elapsed, los);
        let rev: Vec<_> = self.recon.events.drain(..).collect();
        for (l, m) in rev {
            self.push(l, m);
        }
        // Missiles et contre-mesures : tirs des sites, vol des missiles, HACM
        self.recon.ecm = self.combat.ecm;
        for k in std::mem::take(&mut self.recon.launches) {
            self.combat.launch(k, &self.recon.emitters[k], &self.ac);
        }
        self.combat.update(sim_dt, &mut self.ac);
        for k in std::mem::take(&mut self.combat.done) {
            self.recon.missile_done(k);
        }
        for k in std::mem::take(&mut self.combat.killed) {
            self.recon.emitters[k].destroyed = true;
            if self.recon.threat.is_some_and(|t| t.0 == k) {
                self.recon.threat = None;
            }
        }
        let cev: Vec<_> = self.combat.events.drain(..).collect();
        for (l, m) in cev {
            self.push(l, m);
        }
        if let Some(m) = self.mission.update(sim_dt, &self.ac) {
            self.push(Level::Info, m);
        }
        self.atc_flavor();
        // Infos éphémères : annonces radio-altimètre et passages de Mach entiers
        let agl_ft = self.ac.d.agl / FT;
        if !self.ac.on_ground && self.ac.crashed.is_none() && self.ac.d.vs < 0.0 {
            for c in [2500.0, 1000.0, 500.0, 400.0, 300.0, 200.0, 100.0, 50.0, 40.0, 30.0, 20.0, 10.0] {
                if self.prev_agl_ft > c && agl_ft <= c {
                    self.flash(format!("{c:.0}"), if c <= 100.0 { ui::AMBER } else { ui::GREEN_S });
                }
            }
        }
        self.prev_agl_ft = agl_ft;
        let m_int = self.ac.d.mach.floor() as i32;
        if m_int > self.prev_mach_int && m_int >= 1 && self.sim_time > 1.0 {
            self.flash(format!("MACH {m_int}"), ui::MAGENTA_S);
        } else if m_int == 0 && self.prev_mach_int >= 1 {
            self.flash("SUBSONIQUE", WHITE);
        }
        self.prev_mach_int = m_int;
        // Événements aérofreins / freins
        let now_brk = (self.ac.speedbrake, self.ac.brakes);
        if let Some((sb, br)) = self.prev_brk {
            if self.ac.crashed.is_none() {
                if now_brk.0 != sb {
                    let auto = if self.ac.ap.at_speedbrake { " (AUTO)" } else { "" };
                    self.flash(if now_brk.0 { format!("AEROFREINS SORTIS{auto}") } else { "AEROFREINS RENTRES".into() }, ui::CYAN_S);
                }
                if now_brk.1 != br {
                    let s = match (now_brk.1, self.ac.on_ground, self.ac.ground_speed() < 3.0) {
                        (true, true, true) => "FREIN DE PARC SERRE",
                        (true, true, false) => "FREINAGE",
                        (true, false, _) => "FREINS ARMES",
                        (false, true, true) => "FREIN DE PARC DESSERRE",
                        (false, _, _) => "FREINS RELACHES",
                    };
                    self.flash(s, if now_brk.1 { ui::GREEN_S } else { WHITE });
                }
            }
        }
        self.prev_brk = Some(now_brk);
        // Aide à la rotation au décollage manuel : rappel du sens du manche (flèche BAS = tirer)
        if self.ac.on_ground && !self.ac.ap.on && self.ac.autotakeoff.is_none() && self.ac.d.keas > 175.0 && self.ac.theta < 2f64.to_radians() && self.ac.crashed.is_none() {
            self.flash("ROTATION : FLECHE BAS (TIRER)", ui::AMBER);
        }

        // Enregistreur de vol : trace continue, analyse et sauvegarde définitive au crash
        let alerts = self.alerts();
        match &mut self.mishap {
            None if self.ac.crashed.is_none() => {
                self.last_ok = mishap::Snapshot::of(&self.ac, self.sim_time);
                self.last_ac = Some(self.ac.clone());
                if self.ac_hist.back().is_none_or(|(t, _)| self.sim_time - t >= 0.5) {
                    self.ac_hist.push_back((self.sim_time, self.ac.clone()));
                    while self.ac_hist.len() > 12 {
                        self.ac_hist.pop_front();
                    }
                }
            }
            None => {
                let cause = self.ac.crashed.clone().unwrap_or_default();
                // Crash dès la première trame : la trame courante sert de dernière trame valide
                let snap = if self.last_ok.lat == 0.0 && self.last_ok.lon == 0.0 { mishap::Snapshot::of(&self.ac, self.sim_time) } else { self.last_ok };
                self.mishap = Some(mishap::Mishap::new(&cause, snap));
                self.flashes.clear();
                // Le panneau d'alarmes s'affiche : il s'allume en cascade
                self.tab = gcs::Tab::Alarmes;
            }
            Some(_) => {}
        }
        let (due, second) = match &mut self.mishap {
            Some(m) => (m.due(), m.second_result()),
            None => (Vec::new(), None),
        };
        for (l, s) in due {
            self.push(l, s);
        }
        match second {
            Some(true) => self.recover_vehicle(),
            Some(false) => self.push(Level::Warning, "RECONNEXION : AUCUNE REPONSE DU VEHICULE SUR LE CANAL D'URGENCE"),
            None => {}
        }
        if self.ac.crashed.is_some() {
            if self.crash_report.is_empty() {
                self.crash_report = self.fdr.finalize_crash(&self.ac, self.sim_time, &alerts);
                let lines = self.crash_report.clone();
                self.push(Level::Warning, "=== ANALYSE DU CRASH (enregistreur de vol) ===");
                for l in lines {
                    let lvl = if l.starts_with("DIAGNOSTIC") || l.starts_with("CAUSE") { Level::Caution } else { Level::Info };
                    self.log.push((self.sim_time, lvl, l));
                }
            }
        } else {
            self.fdr.update(&self.ac, self.sim_time, &alerts);
        }
        self.update_bit(real_dt);
    }

    /// Interprète une instruction texte de la station sol (FR ou EN).
    fn exec_command(&mut self, line: &str) {
        let norm: String = line
            .to_uppercase()
            .chars()
            .map(|c| match c {
                'É' | 'È' | 'Ê' => 'E',
                'À' | 'Â' => 'A',
                'Ç' => 'C',
                _ => c,
            })
            .collect();
        let w: Vec<&str> = norm.split_whitespace().collect();
        if w.is_empty() {
            return;
        }
        self.push(Level::Info, format!("GCS > {}", w.join(" ")));
        // Sans liaison : seules les instructions locales et de liaison passent. Le canal de secours
        // transmet toutes les instructions (avec sa latence) ; c'est l'avion qui refuse celles qu'il
        // ne peut pas exécuter (moteurs détruits...)
        const LOCAL: [&str; 18] = ["AIDE", "HELP", "HUD", "INFOS", "METEO", "WX", "WEATHER", "RESSENTI", "FEEL", "PANNE", "PANNES", "FAIL", "MAJ", "RECONNEXION", "RECONNECT", "RELAIS", "LIAISON", "INCIDENT"];
        const EMERG: [&str; 5] = ["EMERGENCY", "URGENCE", "MAYDAY", "EJECTION", "EJECT"];
        let _ = EMERG;
        if !self.onboard {
            if let Some(why) = self.degraded_refusal(w[0]) {
                self.cues.push(audio::Cue::Refused);
                self.push(Level::Caution, format!("DARKSTAR : IMPOSSIBLE — {why}"));
                return;
            }
        }
        if !self.link.emergency_ok() && !LOCAL.contains(&w[0]) && !self.onboard {
            self.cues.push(audio::Cue::Refused);
            self.push(Level::Caution, "LIAISON PERDUE — INSTRUCTION NON TRANSMISE (RECONNEXION / RELAIS)");
            return;
        }
        let num = |i: usize| w.get(i).and_then(|s| s.trim_start_matches("FL").parse::<f64>().ok());
        let arg = |i: usize| w.get(i).copied().unwrap_or("");
        let apt_arg = |i: usize| AIRPORTS.iter().position(|a| a.icao == arg(i));
        let ac = &mut self.ac;
        let airborne = !ac.on_ground;
        let mut reply: Result<String, String> = Err(format!("INSTRUCTION INCONNUE : {} (AIDE pour la liste)", w[0]));
        let engage_ap = |ac: &mut Aircraft| {
            if !ac.ap.on {
                ac.ap.on = true;
                ac.ap.lat = LatMode::Hdg;
                ac.ap.hdg_tgt = ac.hdg_deg().round();
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
            }
            ac.autoland = None;
        };
        match w[0] {
            "CAP" | "HDG" | "HEADING" => match num(1) {
                Some(h) if airborne => {
                    engage_ap(ac);
                    ac.ap.lat = LatMode::Hdg;
                    ac.ap.hdg_tgt = wrap360(h);
                    reply = Ok(format!("CAP {:03.0}", ac.ap.hdg_tgt));
                }
                Some(_) => reply = Err("AU SOL".into()),
                None => reply = Err("USAGE : CAP 270".into()),
            },
            "ALT" | "FL" | "ALTITUDE" => match num(1) {
                Some(mut a) if airborne => {
                    if w[0] == "FL" || arg(1).starts_with("FL") {
                        a *= 100.0;
                    }
                    engage_ap(ac);
                    ac.ap.vert = VertMode::Alt;
                    ac.ap.alt_tgt_ft = a.clamp(1_000.0, 130_000.0);
                    reply = Ok(format!("ALTITUDE {:.0} FT", ac.ap.alt_tgt_ft));
                }
                Some(_) => reply = Err("AU SOL".into()),
                None => reply = Err("USAGE : ALT 90000  ou  FL350".into()),
            },
            "ASSIETTE" | "PITCH" | "ASS" => match w.get(1).and_then(|s| s.trim_start_matches('+').parse::<f64>().ok()) {
                Some(p) if airborne => {
                    engage_ap(ac);
                    ac.ap.vert = VertMode::Pitch;
                    ac.ap.pitch_tgt = p.clamp(-20.0, 30.0);
                    reply = Ok(format!("ASSIETTE {:+.1} DEG", ac.ap.pitch_tgt));
                }
                Some(_) => reply = Err("AU SOL".into()),
                None => reply = Err("USAGE : ASSIETTE 10  (degrés, -20 à +30)".into()),
            },
            "MACH" | "M" => match num(1) {
                Some(m) => {
                    ac.ap.at_on = true;
                    ac.thr_manual = false;
                    ac.ap.spd_managed = false;
                    ac.ap.spd_keas = None;
                    ac.ap.mach_tgt = m.clamp(0.3, 12.0);
                    reply = Ok(format!("A/THR MACH {:.2}", ac.ap.mach_tgt));
                }
                None => reply = Err("USAGE : MACH 6".into()),
            },
            "KEAS" | "VIT" | "VITESSE" | "SPD" | "SPEED" if matches!(arg(1), "AUTO" | "GEREE" | "MANAGED") => {
                ac.ap.at_on = true;
                ac.ap.spd_managed = true;
                reply = Ok("A/THR VITESSE GÉRÉE (APPROCHE / 250 KEAS SOUS 10 000 FT / 350 KEAS - M0.9)".into());
            }
            "KEAS" | "VIT" | "VITESSE" | "SPD" | "SPEED" => match num(1) {
                Some(k) => {
                    ac.ap.at_on = true;
                    ac.ap.spd_managed = false;
                    ac.ap.spd_keas = Some(k.clamp(140.0, 800.0));
                    reply = Ok(format!("A/THR {:.0} KEAS", k.clamp(140.0, 800.0)));
                }
                None => reply = Err("USAGE : KEAS 300".into()),
            },
            "THR" | "GAZ" | "POUSSEE" => match num(1) {
                Some(p) => {
                    ac.ap.at_on = false;
                    ac.throttle = (p / 100.0).clamp(0.0, 1.0);
                    reply = Ok(format!("MANETTE {:.0}%", ac.throttle * 100.0));
                }
                None => reply = Err("USAGE : THR 80".into()),
            },
            "DEST" | "DESTINATION" => {
                if arg(1).is_empty() {
                    // Sans argument : destination actuelle et liste des codes disponibles
                    let codes: Vec<&str> = AIRPORTS.iter().map(|a| a.icao).collect();
                    self.log.push((self.sim_time, Level::Info, format!("AÉROPORTS : {}", codes.join(" "))));
                    reply = Ok(format!("DESTINATION ACTUELLE : {} — {}", AIRPORTS[self.dest].icao, AIRPORTS[self.dest].name));
                } else {
                    match apt_arg(1) {
                        Some(i) => {
                            self.dest = i;
                            let (d, b) = dist_brg(ac.lat, ac.lon, AIRPORTS[i].lat, AIRPORTS[i].lon);
                            reply = Ok(format!("DESTINATION {} — {} ({:.0} NM, {:03.0}°)", AIRPORTS[i].icao, AIRPORTS[i].name, d / NM, b));
                        }
                        None => reply = Err(format!("AÉROPORT INCONNU : {} (DEST seul = liste)", arg(1))),
                    }
                }
            }
            "DIRECT" | "DCT" | "DIR" if arg(1) == "CIBLE" => {
                reply = self.recon.command(&w, ac, self.mission.elapsed);
            }
            "CAMERA" | "CAM" | "ZOOM" | "SAR" | "RADAR" | "ELINT" | "CIBLE" | "TARGET" | "PHOTO" | "MENACES" | "THREATS" => {
                reply = self.recon.command(&w, ac, self.mission.elapsed);
            }
            "TANKER" | "RAVITAILLEUR" | "RAVITAILLEMENT" | "REFUEL" | "AAR" | "DECROCHER" | "DISCONNECT" => {
                reply = self.refuel.command(&w, ac);
            }
            "BINGO" => {
                if arg(1) == "AUTO" {
                    self.bingo_manual = None;
                    reply = Ok("BINGO AUTOMATIQUE (RETOUR DESTINATION + APPROCHE + RESERVE)".into());
                } else if let Some(t) = num(1) {
                    self.bingo_manual = Some(t * 1000.0);
                    reply = Ok(format!("BINGO MANUEL {t:.1} t"));
                } else {
                    let p = &self.fuel_plan;
                    reply = Ok(format!("BINGO {:.1} t{} — JOKER {:.1} t — JP7 {:.1} t (BINGO AUTO | BINGO 8)", p.bingo / 1000.0, if p.manual { " MANUEL" } else { "" }, p.joker / 1000.0, ac.fuel / 1000.0));
                }
            }
            "CHAFF" | "PAILLETTES" | "FLARE" | "FLARES" | "LEURRES" | "ECM" | "BROUILLEUR" | "CMDS" => {
                reply = self.combat.command(&w, ac);
            }
            "TIR" | "FIRE" | "HACM" => {
                // Cible : objectif ELINT (code), nom d'émetteur, ou objectif sélectionné
                let k = match self.recon.find(arg(1)) {
                    Some(i) => match self.recon.targets[i].kind {
                        recon::Kind::Elint(k) => Some(k),
                        _ => None,
                    },
                    None if arg(1).is_empty() => match self.recon.targets[self.recon.selected].kind {
                        recon::Kind::Elint(k) => Some(k),
                        _ => None,
                    },
                    None => self.recon.emitters.iter().position(|e| e.name.contains(arg(1))),
                };
                reply = match k {
                    Some(k) => self.combat.fire_hacm(k, &self.recon.emitters[k], ac),
                    None => Err("TIR <objectif ELINT I|J|K ou nom de site> — site localise par l'ELINT".into()),
                };
            }
            "ARMEMENT" | "ARMES" => {
                self.tab = gcs::Tab::Armement;
                reply = Ok("PAGE ARMEMENT".into());
            }
            "RECONNEXION" | "RECONNECT" | "RELAIS" | "LIAISON" if self.mishap.as_ref().is_some_and(|m| m.second == mishap::Second::Failed) => {
                if let Some(m) = &mut self.mishap {
                    m.try_second();
                }
                reply = Ok("NOUVEL ESSAI DE RECONNEXION (CANAL D'URGENCE)".into());
            }
            // Injection d'incident grave (essais) : perte du véhicule de la cause choisie
            "INCIDENT" if airborne => {
                let why = match arg(1) {
                    "ELEC" => Some("PERTE TOTALE D'ALIMENTATION ÉLECTRIQUE — LIAISON PERDUE"),
                    "MOTEUR" | "STATO" => Some("EXPLOSION DE LA CHAMBRE DE COMBUSTION DU STATORÉACTEUR"),
                    "GOUVERNES" | "FCS" => Some("DESTRUCTION DES GOUVERNES (FCS) — CHARGE AÉRODYNAMIQUE EXCESSIVE"),
                    "STRUCTURE" => Some("RUPTURE STRUCTURELLE — FACTEUR DE CHARGE"),
                    "SURPRESSION" | "Q" => Some("RUPTURE STRUCTURELLE — PRESSION DYNAMIQUE EXCESSIVE"),
                    _ => None,
                };
                match why {
                    Some(w) => {
                        ac.crash(w);
                        reply = Ok(format!("INCIDENT INJECTE : {w}"));
                    }
                    None => reply = Err("INCIDENT ELEC | MOTEUR | GOUVERNES | STRUCTURE".into()),
                }
            }
            "RECONNEXION" | "RECONNECT" | "RELAIS" | "LIAISON" => {
                let p = match (arg(1), arg(2)) {
                    ("LOS", _) => Some(link::Path::Los),
                    ("SAT" | "SATCOM", _) => Some(link::Path::Satcom),
                    ("SECOURS" | "IRIDIUM" | "URGENCE", _) => Some(link::Path::Emerg),
                    ("BACN" | "BACN1" | "BACN-1", n) => Some(link::Path::Bacn(if n == "2" { 1 } else { 0 })),
                    ("BACN2" | "BACN-2", _) => Some(link::Path::Bacn(1)),
                    ("SOL", icao) => link::STATIONS.iter().position(|s| *s == icao).map(link::Path::Ground),
                    ("", _) => {
                        // Meilleur chemin estimé d'après la dernière position connue, sinon le secours
                        let last = self.stale.as_ref().unwrap_or(&self.ac);
                        let best = link::Path::all()
                            .into_iter()
                            .filter(|p| *p != link::Path::Emerg)
                            .filter_map(|p| self.link.estimate(p, last).ok().map(|(l, _)| (p, l)))
                            .min_by(|a, b| a.1.total_cmp(&b.1))
                            .map(|(p, _)| p);
                        Some(best.unwrap_or(link::Path::Emerg))
                    }
                    _ => None,
                };
                match p {
                    Some(_) if self.link.state == link::State::Connected && self.link.path != link::Path::Emerg && w[0] == "LIAISON" => {
                        reply = Ok(format!("LIAISON {} ETABLIE", self.link.path.name()));
                    }
                    Some(p) => {
                        self.link.connect(p);
                        reply = Ok(format!("TENTATIVE DE LIAISON VIA {}", p.name()));
                    }
                    None => reply = Err("RELAIS : LOS | SOL <ICAO> | BACN 1|2 | SATCOM | SECOURS".into()),
                }
            }
            "ALIGNEMENT" | "ALIGNER" | "LINEUP" | "ALIGN" => match self.hold_apt {
                Some(ai) if ac.on_ground && self.taxi.is_none() => {
                    let a = &AIRPORTS[ai];
                    let along = a.len / 2.0 - 160.0;
                    let entry = a.point(along, 0.0);
                    let fin = wrap360(a.hdg + 180.0);
                    self.taxi = Some((vec![entry], 0, fin));
                    reply = Ok(format!("ROULAGE VERS LA PISTE {} — ALIGNEMENT", a.end_ident(fin, None)));
                }
                _ => reply = Err("PAS AU POINT D'ATTENTE".into()),
            },
            "HUD" if arg(1) == "STYLE" => {
                let (st, m) = match arg(2) {
                    "DARKSTAR" => (0, "HORIZON : STYLE DARKSTAR (ECHELLES DEFILANTES, BLANC)"),
                    "F35" | "F-35" => (1, "HORIZON : STYLE F-35 (SYMBOLOGIE DE CASQUE, VALEURS ENCADREES)"),
                    _ => (2, "HORIZON : STYLE HYBRIDE DARKSTAR / F-35 (ECHELLES, SYMBOLOGIE VERTE)"),
                };
                pfd::set_style(st);
                reply = Ok(m.into());
            }
            "LNAV" | "NAV" => {
                if airborne {
                    engage_ap(ac);
                    ac.ap.lat = LatMode::Nav;
                    reply = Ok(match self.fms.fpln.as_ref().and_then(|p| p.wpts.get(p.active)) {
                        Some(w) => format!("LNAV : PLAN DE VOL FMS, CAP SUR {}", w.ident),
                        None => format!("NAV : DIRECT {} (pas de plan FMS actif)", AIRPORTS[self.dest].icao),
                    });
                } else {
                    reply = Err("AU SOL : DECOLLAGE D'ABORD (instruction DECOLLAGE), PUIS LNAV".into());
                }
            }
            // Version développeur : DEV (bascule) / DEV OFF
            "DEV" if arg(1) == "PAGE" && self.dev.is_some() => {
                let d = self.dev.as_mut().unwrap();
                reply = match d.set_page(arg(2)) {
                    true => Ok(format!("ECRAN : PAGE {}", arg(2))),
                    false => Err("DEV PAGE 1..7 / CONFIG / ARMEMENT / MOTEUR".into()),
                };
            }
            "DEV" => {
                if self.dev.is_some() || arg(1) == "OFF" {
                    self.dev = None;
                    self.layout = dev::DevUi::new();
                    pfd::hide_menu(false);
                    reply = Ok("VERSION SIMULATEUR".into());
                } else {
                    self.dev = Some(dev::DevUi::new());
                    pfd::hide_menu(true);
                    reply = Ok("VERSION DEVELOPPEUR — FENETRES DEPLACABLES, CONSOLE DANS LA BARRE (MENU CONSOLE)".into());
                }
            }
            // Panneau droit de la console (démarrage moteurs / APU)
            "PANNEAU" | "CONSOLE" if matches!(arg(1), "DROIT" | "DROITE") => {
                menu::show_right(true);
                self.tab = gcs::Tab::Console;
                reply = Ok("CONSOLE : PANNEAU DROIT (APU)".into());
            }
            // APU : APU START|ON / STOP|OFF / GEN ON|OFF / BLEED ON|OFF
            "APU" => {
                let ac = &mut self.ac;
                reply = match (arg(1), arg(2)) {
                    ("START" | "ON" | "", _) => {
                        let (alt, batt, fuel) = (ac.alt / FT, ac.elec.bus_ok || ac.elec.batt > 0.05, ac.fuel > 0.0);
                        ac.apu.start(alt, batt, fuel)
                    }
                    ("STOP" | "OFF", _) => ac.apu.stop(),
                    ("GEN", v) => {
                        ac.apu.generator = v != "OFF";
                        Ok(format!("APU GEN {}", if ac.apu.generator { "ON" } else { "OFF" }))
                    }
                    ("BLEED", v) => {
                        ac.apu.bleed = v != "OFF";
                        Ok(format!("APU BLEED {}", if ac.apu.bleed { "ON" } else { "OFF" }))
                    }
                    _ => Err("APU START / STOP / GEN ON|OFF / BLEED ON|OFF".into()),
                };
            }
            // Escorte de chasse : ESCORTE (rejointe) / ESCORTE OFF (séparation)
            "ESCORTE" | "ESCORT" => {
                reply = if arg(1) == "OFF" { self.escort.release() } else { self.escort.request(&self.ac) };
            }
            // Touche de ligne du MCDU : LSK L1..L6 / R1..R6
            "LSK" => {
                let k = arg(1);
                let left = k.starts_with('L');
                match k.get(1..).and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=6).contains(n)) {
                    Some(n) if left || k.starts_with('R') => {
                        self.mcdu_key(fms::McduKey::Lsk(left, n - 1));
                        reply = Ok(format!("LSK {k}"));
                    }
                    _ => reply = Err("LSK L1..L6 / R1..R6".into()),
                }
            }
            "NAVDATA" => {
                self.fms.navupd = Some(navdb::update());
                reply = Ok("NAVDATA : TELECHARGEMENT DE LA DERNIERE BASE MONDE (OURAIRPORTS)".into());
            }
            "VNAV" => {
                self.fms.vnav = arg(1) != "OFF";
                if self.fms.vnav {
                    self.ac.thr_manual = false;
                }
                reply = Ok(if self.fms.vnav { "VNAV ON : ALTITUDE, MACH ET MOTEURS GERES PAR LE FMS".into() } else { "VNAV OFF".into() });
            }
            "DISPATCH" => {
                let (o, d) = (arg(1).to_string(), arg(2).to_string());
                // Par défaut : croisière au plafond, Mach 11 (DISPATCH KPMD PHNL 950 6 pour un niveau fixe)
                let m = num(4).unwrap_or(if num(3).is_some() { 6.0 } else { 11.0 });
                let fl = num(3).unwrap_or(fms::ceiling_fl(m));
                self.fms.crz_max = num(3).is_none();
                reply = self.fms.dispatch_now(&o, &d, fl, m);
                if reply.is_ok() {
                    if let Some(i) = AIRPORTS.iter().position(|a| a.icao == d) {
                        self.dest = i;
                    }
                }
            }
            "MCDU" | "FMS" => {
                self.tab = gcs::Tab::Mcdu;
                self.fms.focus = true;
                let p = match arg(1) {
                    "FPLN" | "F-PLN" => Some(fms::Page::Fpln),
                    "PERF" => Some(fms::Page::Perf),
                    "PROG" => Some(fms::Page::Prog),
                    "FUEL" => Some(fms::Page::Fuel),
                    "DIR" => Some(fms::Page::Dir),
                    "INIT" => Some(fms::Page::Init),
                    "DEP" => Some(fms::Page::Dep),
                    "ARR" => Some(fms::Page::Arr),
                    "RADNAV" => Some(fms::Page::RadNav),
                    "DATA" => Some(fms::Page::Data),
                    _ => None,
                };
                if let Some(p) = p {
                    self.fms.page = p;
                    self.fms.scroll = 0;
                }
                reply = Ok("MCDU".into());
            }
            "SIMBRIEF" => {
                if !arg(1).is_empty() {
                    self.fms.user = arg(1).to_lowercase();
                }
                self.fms.start_fetch();
                self.tab = gcs::Tab::Mcdu;
                reply = if self.fms.user.is_empty() { Err("SIMBRIEF <identifiant>".into()) } else { Ok(format!("SIMBRIEF : IMPORT DE L'OFP DE {}", self.fms.user.to_uppercase())) };
            }
            "FEU" => {
                let z = match arg(1) {
                    "L" | "G" | "GAUCHE" => Some(0),
                    "R" | "D" | "DROIT" => Some(1),
                    "CHAMB" | "CH" | "CHAMBRES" => Some(2),
                    "H2" | "SOUTE" => Some(3),
                    _ => None,
                };
                if arg(1) == "TEST" {
                    self.ac.fire.test = 3.0;
                    reply = Ok("FIRE TEST : VOYANTS ET BOUCLES DE DETECTION".into());
                } else if let Some(z) = z {
                    let mut f = std::mem::take(&mut self.ac.fire);
                    let m = f.pull(&mut self.ac, z);
                    self.ac.fire = f;
                    reply = Ok(m);
                } else {
                    reply = Err("FEU L | R | CHAMB | H2 (poignee)  ou FEU TEST".into());
                }
            }
            "BTL" | "BOUTEILLE" | "EXTINCTEUR" => match num(1) {
                Some(b) if b == 1.0 || b == 2.0 => reply = self.ac.fire.discharge(b as usize - 1),
                _ => reply = Err("BTL 1 | BTL 2".into()),
            },
            "INCENDIE" => {
                let z = match arg(1) {
                    "L" | "G" => Some(0),
                    "R" | "D" => Some(1),
                    "CHAMB" | "CH" => Some(2),
                    "H2" => Some(3),
                    _ => None,
                };
                match z {
                    Some(z) if self.ac.fire.ignite(z) => {
                        self.tab = gcs::Tab::Incendie;
                        reply = Ok(format!("FEU INJECTE : {}", fire::ZONES[z].1));
                    }
                    Some(_) => reply = Err("ZONE DEJA EN FEU OU DETRUITE".into()),
                    None => reply = Err("INCENDIE L | R | CHAMB | H2 (essai)".into()),
                }
            }
            "GCAS" | "AUTOGCAS" => match arg(1) {
                "ON" => {
                    self.set_gcas(true);
                    reply = Ok("AUTO GCAS ARME".into());
                }
                "OFF" => {
                    self.set_gcas(false);
                    reply = Ok("AUTO GCAS OFF".into());
                }
                _ => reply = Err(format!("AUTO GCAS {} (GCAS ON | GCAS OFF)", if self.ac.gcas_on { "ARME" } else { "OFF" })),
            },
            "FUELCOOL" | "REFROID" | "FTMS" => match arg(1) {
                "AUTO" | "ON" => {
                    self.set_fuel_cool(0);
                    reply = Ok("FUEL COOL AUTO".into());
                }
                "RAM" => {
                    self.set_fuel_cool(1);
                    reply = Ok("FUEL COOL RAM AIR".into());
                }
                "OFF" => {
                    self.set_fuel_cool(2);
                    reply = Ok("FUEL COOL OFF".into());
                }
                _ => reply = Err("FUELCOOL AUTO | RAM | OFF".into()),
            },
            "GLIM" | "GLIMIT" => match arg(1) {
                "HIGH" | "HAUT" => {
                    self.set_glim(true);
                    reply = Ok("G LIMIT HIGH".into());
                }
                "NORM" | "NORMAL" => {
                    self.set_glim(false);
                    reply = Ok("G LIMIT NORM".into());
                }
                _ => reply = Err(format!("G LIMIT {} {:.1} G (GLIM NORM | GLIM HIGH)", if self.ac.g_lim_high { "HIGH" } else { "NORM" }, self.ac.nz_law_max())),
            },
            "PITOT" => match arg(1) {
                "ON" => {
                    self.set_pitot(true);
                    reply = Ok("PITOT HEAT ON".into());
                }
                "OFF" => {
                    self.set_pitot(false);
                    reply = Ok("PITOT HEAT OFF".into());
                }
                _ => reply = Err("PITOT ON | PITOT OFF".into()),
            },
            "NWS" => match arg(1) {
                "HI" | "HIGH" => {
                    self.ac.nws_hi = true;
                    reply = Ok("NWS HI (30 DEG)".into());
                }
                "LO" | "LOW" => {
                    self.ac.nws_hi = false;
                    reply = Ok("NWS LO (12 DEG)".into());
                }
                _ => reply = Err("NWS HI | NWS LO".into()),
            },
            "DA" | "DH" | "MINIMUMS" | "MINIMA" => match num(1) {
                Some(v) if (50.0..=1_000.0).contains(&v) => {
                    self.da_ft = v;
                    reply = Ok(format!("DA : SEUIL + {v:.0} FT"));
                }
                _ => reply = Err(format!("DA ACTUELLE : SEUIL + {:.0} FT (DA 50 a 1000)", self.da_ft)),
            },
            "CONTINUE" | "CONTINUER" | "LAND" if w.len() == 1 && self.mins == 2 => {
                self.mins = 3;
                reply = Ok("CONTINUE — ATTERRISSAGE".into());
            }
            "GO" | "GA" | "REMISE" | "REMETTRE" | "GOAROUND" => {
                self.mins = 3;
                reply = ac.go_around();
            }
            "EMERGENCY" | "URGENCE" | "MAYDAY" if matches!(arg(1), "OVERRIDE" | "OVRD") => {
                reply = self.emerg.start_override(ac);
            }
            "EMERGENCY" | "URGENCE" | "MAYDAY" => {
                if arg(1) == "OFF" {
                    self.emerg.cancel();
                    reply = Ok("URGENCE ANNULEE".into());
                } else {
                    reply = self.emerg.start(ac, &self.refuel);
                }
            }
            "EJECTION" | "EJECT" => {
                if matches!(arg(1), "CONFIRME" | "CONFIRM") && airborne {
                    self.emerg.eject_now();
                    reply = Ok("EJECTION CONFIRMEE".into());
                } else {
                    reply = Err("CONFIRMER : EJECTION CONFIRME".into());
                }
            }
            "RESSENTI" | "FEEL" => {
                self.feel.on = arg(1) != "OFF";
                reply = Ok(if self.feel.on { "RESSENTI DE VOL ON (vibrations, inertie de la tete, voile)".into() } else { "RESSENTI DE VOL OFF".into() });
            }
            "HUD" | "INFOS" => {
                pfd_plus::hud_show_all(10.0);
                reply = Ok("INFOS HUD AFFICHEES 10 s (survol de l'horizon : toutes les infos)".into());
            }
            "METEO" | "WEATHER" | "WX" if matches!(arg(1), "REEL" | "REELLE" | "REAL" | "LIVE") => {
                self.wx.set_real();
                reply = Ok(format!("METEO REELLE (Open-Meteo, grille 5x5 autour de l'avion) — {}", self.wx.real.status));
            }
            "WXR" | "RADAR-METEO" | "WXRADAR" => {
                let r = &mut self.mfd.wxr;
                match arg(1) {
                    "OFF" => r.on = false,
                    "ON" | "" => r.on = true,
                    "AUTO" => r.auto = true,
                    "TILT" => match arg(2).replace(',', ".").parse::<f32>() {
                        Ok(t) => {
                            r.tilt = t.clamp(-15.0, 15.0);
                            r.auto = false;
                        }
                        Err(_) => r.auto = true,
                    },
                    "TURB" => r.turb = arg(2) != "OFF",
                    "HORIZON" | "3D" => r.horizon = arg(2) != "OFF",
                    _ => {}
                }
                if matches!(arg(1), "RANGE" | "PORTEE") {
                    if let Some(i) = arg(2).parse::<f64>().ok().and_then(|v| mfd::HSI_RANGES.iter().position(|x| *x >= v)) {
                        self.mfd.hsi_range = i;
                    }
                }
                let r = &mut self.mfd.wxr;
                reply = Ok(if r.on {
                    format!("RADAR METEO ON  TILT {}  TURB {}  HORIZON {}", r.tilt_label(), if r.turb { "ON" } else { "OFF" }, if r.horizon { "ON" } else { "OFF" })
                } else {
                    "RADAR METEO OFF".into()
                });
            }
            "METEO" | "WEATHER" | "WX" if arg(1) == "3D" => {
                self.wx.show3d = arg(2) != "OFF";
                reply = Ok(if self.wx.show3d { "METEO 3D ON (flux de vent, vecteurs, volumes de danger)".into() } else { "METEO 3D OFF".into() });
            }
            "METEO" | "WEATHER" | "WX" => {
                let i = weather::SEVERITIES.iter().position(|n| n.starts_with(arg(1))).or_else(|| arg(1).parse::<usize>().ok().filter(|v| *v <= 4));
                match i {
                    Some(i) if !arg(1).is_empty() => {
                        self.wx.set_severity(i);
                        reply = Ok(format!("METEO {}", weather::SEVERITIES[i]));
                    }
                    _ => {
                        let w = &self.wx.info;
                        reply = Ok(format!("METEO {} — VENT {:03.0}/{:.0} KT — DANGER {} {} (REEL|CALME|BEAU|NUAGEUX|COUVERT|ORAGEUX, 3D ON/OFF)", w.source, w.wind_dir, w.wind_kt, wx3d::danger_label(w.danger), w.cause));
                    }
                }
            }
            "PLAN" | "ROUTE" if arg(1) == "RECON" || arg(1) == "OFF" => {
                reply = self.recon.command(&w, ac, self.mission.elapsed);
            }
            "RECON" => {
                self.tab = gcs::Tab::Recon;
                reply = Ok("PAGE RECON".into());
            }
            "DIRECT" | "DCT" | "DIR" => match apt_arg(1) {
                Some(i) if airborne => {
                    self.dest = i;
                    engage_ap(ac);
                    ac.ap.lat = LatMode::Nav;
                    reply = Ok(format!("DIRECT {} — {}", AIRPORTS[i].icao, AIRPORTS[i].name));
                }
                Some(_) => reply = Err("AU SOL".into()),
                None => reply = Err("USAGE : DIRECT KEDW (code OACI)".into()),
            },
            "LAND" | "ATTERRIR" | "AUTOLAND" | "ATTERRISSAGE" if matches!(arg(1), "PLANE" | "DEADSTICK") => {
                let i = if arg(2).is_empty() { Some(self.dest) } else { apt_arg(2) };
                match i {
                    Some(i) => {
                        self.dest = i;
                        reply = ac.engage_autoland_dead_stick(i).map(|_| format!("AUTOLAND PLANE {}", AIRPORTS[i].icao));
                    }
                    None => reply = Err(format!("AÉROPORT INCONNU : {}", arg(2))),
                }
            }
            "LAND" | "ATTERRIR" | "AUTOLAND" | "ATTERRISSAGE" => {
                let i = if arg(1).is_empty() { Some(self.dest) } else { apt_arg(1) };
                match i {
                    Some(i) => {
                        self.dest = i;
                        reply = ac.engage_autoland(i).map(|_| format!("AUTOLAND {}", AIRPORTS[i].icao));
                    }
                    None => reply = Err(format!("AÉROPORT INCONNU : {}", arg(1))),
                }
            }
            "TAKEOFF" | "DECOLLAGE" | "DECOLLER" => {
                let a = num(1).unwrap_or(15_000.0);
                reply = ac.engage_takeoff(a).map(|_| "DÉCOLLAGE AUTO".to_string());
            }
            "TURB" | "TURBINES" | "TURBINE" => {
                let running = matches!(ac.eng.turb, TurbState::Running | TurbState::Starting);
                match arg(1) {
                    "START" | "ON" | "DEMARRER" if !running => ac.cmd_turbine(),
                    "STOP" | "OFF" | "COUPER" if running => ac.cmd_turbine(),
                    _ => {}
                }
                reply = Ok("TURBINES".into());
            }
            "INLET" | "ENTREES" | "ENTREE" => {
                match arg(1) {
                    "OPEN" | "OUVRIR" if !ac.eng.inlet_open => ac.cmd_inlet(),
                    "CLOSE" | "FERMER" if ac.eng.inlet_open => ac.cmd_inlet(),
                    _ => {}
                }
                reply = Ok("ENTRÉES D'AIR".into());
            }
            "RAM" | "STATO" | "RAMJET" => {
                match arg(1) {
                    "ARM" | "ARMER" if ac.eng.ram == RamState::Off => ac.cmd_ram(),
                    "IGN" | "ALLUMER" | "ON" => {
                        if ac.eng.ram == RamState::Off {
                            ac.cmd_ram();
                        }
                        if ac.eng.ram == RamState::Armed {
                            ac.cmd_ram();
                        }
                    }
                    "OFF" | "COUPER" if ac.eng.ram != RamState::Off => {
                        ac.eng.ram = RamState::Off;
                        ac.msg(Level::Info, "STATORÉACTEUR COUPÉ");
                    }
                    _ => {}
                }
                reply = Ok("STATO".into());
            }
            "SCRAM" | "SCRAMJET" => {
                match arg(1) {
                    "ARM" | "ARMER" | "ENABLE" if ac.eng.scram == ScramState::Off => ac.cmd_scram(),
                    "IGN" | "ALLUMER" | "ON" => {
                        if ac.eng.scram == ScramState::Off {
                            ac.cmd_scram();
                        }
                        if matches!(ac.eng.scram, ScramState::Armed | ScramState::Unstart) {
                            ac.cmd_scram();
                        }
                    }
                    "OFF" | "COUPER" if ac.eng.scram != ScramState::Off => {
                        ac.eng.scram = ScramState::Off;
                        ac.msg(Level::Info, "SCRAMJET COUPÉ");
                    }
                    _ => {}
                }
                reply = Ok("SCRAM".into());
            }
            "DESCENTE" | "DESCENT" | "DESC" => {
                if airborne {
                    engage_ap(ac);
                    ac.ap.vert = VertMode::QHold;
                    ac.ap.q_tgt = num(1).unwrap_or(50.0).clamp(20.0, 80.0) * 1000.0;
                    ac.ap.at_on = false;
                    ac.throttle = 0.0;
                    reply = Ok(format!(
                        "DESCENTE À q {:.0} kPa CONSTANTE, MANETTE RÉDUITE — PALIER AUTO EN SUBSONIQUE SOUS FL350",
                        ac.ap.q_tgt / 1000.0
                    ));
                } else {
                    reply = Err("AU SOL".into());
                }
            }
            "CAPTEUR" | "CAPTEURS" | "SENSOR" | "SENSORS" | "TC" => {
                let on_off = |w: &str| match w {
                    "ON" | "ACTIF" | "ACTIVER" => Some(false),
                    "OFF" | "INHIB" | "INHIBER" | "DESACTIVER" => Some(true),
                    _ => None,
                };
                // « CAPTEURS ON » : réactive tout
                if w[0].ends_with('S') && on_off(arg(1)) == Some(false) {
                    for s in self.thermal.sensors.iter_mut() {
                        s.inhibited = false;
                    }
                    reply = Ok("TOUS LES CAPTEURS RÉACTIVÉS".into());
                } else {
                    // Le numéro et l'action peuvent venir dans les deux ordres : CAPTEUR 101 OFF / CAPTEUR OFF 101
                    let (num_s, act) = if on_off(arg(1)).is_some() { (arg(2), on_off(arg(1))) } else { (arg(1), on_off(arg(2))) };
                    match self.thermal.find(num_s) {
                        Some(i) => {
                            let s = &mut self.thermal.sensors[i];
                            if let Some(inh) = act {
                                s.inhibited = inh;
                            }
                            let (id, name, inh) = (s.id, s.name, s.inhibited);
                            self.thermal.selected = Some(i);
                            self.tab = gcs::Tab::Thermique;
                            let v = if inh { "INHIBÉ".to_string() } else { format!("ACTIF, {:.0} °C", self.thermal.sensors[i].value) };
                            reply = Ok(format!("{id} {name} : {v}"));
                        }
                        None => reply = Err(format!("CAPTEUR INCONNU : {num_s} (ex : CAPTEUR 101 OFF)")),
                    }
                }
            }
            "G" | "GMAX" => {
                ac.g_max = ac.d.nz.max(1.0);
                ac.g_min = ac.d.nz.min(1.0);
                reply = Ok("MÉMOIRE G MAX / MIN REMISE À ZÉRO".into());
            }
            "PANNE" | "FAIL" if matches!(arg(1), "LIAISON" | "EMETTEUR" | "SATCOM") => {
                let on = !matches!(arg(2), "OFF" | "REPARER");
                if arg(1) == "SATCOM" {
                    self.link.sat_fail = on;
                } else {
                    self.link.tx_fail = on;
                }
                reply = Ok(format!("PANNE {} {}", arg(1), if on { "INJECTÉE" } else { "RÉPARÉE" }));
            }
            "PANNE" | "FAIL" => {
                let on = !matches!(arg(2), "OFF" | "REPARER");
                match systems::fail_by_code(arg(1)) {
                    Some(f) => {
                        ac.set_failure(f, on);
                        reply = Ok(format!("PANNE {} {}", arg(1), if on { "INJECTÉE" } else { "RÉPARÉE" }));
                    }
                    None => {
                        let codes: Vec<&str> = systems::FAILS.iter().map(|(_, c, _)| *c).collect();
                        reply = Err(format!("PANNE INCONNUE — CODES : {}", codes.join(" ")));
                    }
                }
            }
            "PANNES" | "REPARER" | "REPAIR" => {
                for (f, _, _) in systems::FAILS {
                    if ac.fails.has(f) {
                        ac.set_failure(f, false);
                    }
                }
                ac.damage = systems::Failures::default();
                self.link.tx_fail = false;
                self.link.sat_fail = false;
                self.degraded = None;
                reply = Ok("TOUTES LES PANNES RÉPARÉES".into());
            }
            "FIN" | "END" | "TERMINER" => {
                self.end_flight();
                return;
            }
            "QUITTER" | "QUIT" | "EXIT" => {
                self.fdr.finalize();
                std::process::exit(0);
            }
            "FDR" | "ENREGISTRER" | "SAVE" | "TRACK" => {
                match self.fdr.save(None) {
                    Ok(p) => reply = Ok(format!("TRACE ENREGISTRÉE : {}", p.display())),
                    Err(e) => reply = Err(format!("ENREGISTREMENT IMPOSSIBLE : {e}")),
                }
            }
            "ANALYSE" | "ANALYSER" => {
                let lines = if self.crash_report.is_empty() {
                    recorder::analyze(&self.fdr.samples, &self.fdr.events, "(analyse en vol, pas de crash)")
                } else {
                    self.crash_report.clone()
                };
                for l in lines {
                    self.log.push((self.sim_time, Level::Info, l));
                }
                return;
            }
            "MENU" | "CONSOLE" => {
                self.tab = gcs::Tab::Console;
                reply = Ok("CONSOLE MOTEURS / COMMANDES DE VOL".into());
            }
            "START" => {
                ac.cmd_start();
                reply = Ok("START".into());
            }
            "STOP" => {
                ac.cmd_stop();
                reply = Ok("STOP".into());
            }
            "ENG" | "MOTEUR" => {
                let i = match arg(1) {
                    "L" | "G" | "GAUCHE" => Some(0),
                    "R" | "D" | "DROIT" => Some(1),
                    _ => None,
                };
                let sw = match arg(2) {
                    "ON" => Some(EngSw::On),
                    "OFF" => Some(EngSw::Off),
                    "MOTOR" => Some(EngSw::Motor),
                    _ => None,
                };
                match (i, sw) {
                    (Some(i), Some(sw)) => {
                        ac.cmd_eng_switch(i, sw);
                        reply = Ok(format!("ENG {} {}", arg(1), arg(2)));
                    }
                    _ => reply = Err("USAGE : ENG L ON|OFF|MOTOR".into()),
                }
            }
            "FADEC" => {
                match arg(2) {
                    "L" | "G" => ac.cmd_fadec_reset(0),
                    "R" | "D" => ac.cmd_fadec_reset(1),
                    _ => {
                        ac.cmd_fadec_reset(0);
                        ac.cmd_fadec_reset(1);
                    }
                }
                reply = Ok("FADEC RESET".into());
            }
            "ALERT" => {
                ac.eng.alert_start = !w.contains(&"OFF");
                reply = Ok(format!("ALERT START {}", if ac.eng.alert_start { "ON" } else { "OFF" }));
            }
            "AIR" | "AIRDATA" => {
                ac.air_data_ovrd = !w.contains(&"OFF");
                reply = Ok(format!("AIR DATA OVRD {}", if ac.air_data_ovrd { "ON (INS)" } else { "OFF (PITOT)" }));
            }
            "BIT" => {
                self.start_bit();
                reply = Ok("INITIATE BIT".into());
            }
            "SYSTEM" | "RESET" => {
                self.system_reset();
                reply = Ok("SYSTEM RESET".into());
            }
            "YAW" | "TRIM" => {
                let v = if w[0] == "YAW" { num(2) } else { num(1) };
                match v {
                    Some(t) => {
                        ac.yaw_trim = (t / 10.0).clamp(-1.0, 1.0);
                        reply = Ok(format!("YAW TRIM {:+.1}", ac.yaw_trim * 10.0));
                    }
                    None => reply = Err("USAGE : YAW TRIM -2.5  (-10 gauche .. +10 droite)".into()),
                }
            }
            "OVERRIDE" | "OVRD" => {
                let on = !matches!(arg(2), "OFF") && !matches!(arg(1), "OFF");
                match arg(1) {
                    "MOTEUR" | "MOTEURS" | "ENGINE" | "ENG" => ac.cmd_override_engine(on),
                    "FBW" | "COMMANDES" => ac.cmd_override_fbw(on),
                    "OFF" => {
                        if ac.ovr_engine {
                            ac.cmd_override_engine(false);
                        }
                        if ac.ovr_fbw {
                            ac.cmd_override_fbw(false);
                        }
                    }
                    "ALL" | "TOUT" => {
                        ac.cmd_override_engine(on);
                        ac.cmd_override_fbw(on);
                    }
                    _ => {}
                }
                reply = Ok(format!("OVERRIDE MOTEUR {} / FBW {}", if ac.ovr_engine { "ON" } else { "OFF" }, if ac.ovr_fbw { "ON" } else { "OFF" }));
            }
            "FUEL" | "CARBURANT" => {
                if matches!(arg(1), "DUMP" | "VIDANGE") {
                    ac.cmd_fuel_dump(!matches!(arg(2), "OFF" | "STOP"));
                }
                reply = Ok(format!("JP-7 {:.1} T", ac.fuel / 1000.0));
            }
            "HSI" | "RANGE" => {
                let v = if w[0] == "HSI" { num(2).or(num(1)) } else { num(1) };
                if let Some(rng) = v {
                    self.mfd.hsi_range = mfd::HSI_RANGES.iter().position(|&x| x >= rng).unwrap_or(mfd::HSI_RANGES.len() - 1);
                }
                reply = Ok(format!("HSI {:.0} NM", mfd::HSI_RANGES[self.mfd.hsi_range]));
            }
            "H2" => {
                match arg(1) {
                    "DUMP" | "VIDANGE" => ac.cmd_h2_dump(!matches!(arg(2), "OFF" | "STOP")),
                    _ => {}
                }
                reply = Ok(format!("H2 {:.0} KG / {:.0} PSI", ac.h2, ac.h2_psi()));
            }
            "GEAR" | "TRAIN" if matches!(arg(1), "SECOURS" | "ALTERNATE" | "ALT") => {
                ac.cmd_gear_alt();
                reply = Ok("SORTIE DE SECOURS DU TRAIN".into());
            }
            "GEAR" | "TRAIN" => {
                let want_down = matches!(arg(1), "DOWN" | "SORTIR" | "BAS");
                if want_down != ac.gear_down {
                    ac.cmd_gear();
                }
                reply = Ok("TRAIN".into());
            }
            "FREINS" | "BRAKES" | "BRAKE" => {
                ac.brakes = !matches!(arg(1), "OFF" | "RELACHER");
                reply = Ok(format!("FREINS {}", if ac.brakes { "SERRÉS" } else { "RELÂCHÉS" }));
            }
            "AP" => {
                if arg(1) == "OFF" {
                    ac.ap.on = false;
                    ac.autoland = None;
                    reply = Ok("AP OFF".into());
                } else if matches!(arg(1), "MAX" | "PERF" | "COMBAT") && airborne {
                    engage_ap(ac);
                    ac.ap.max_perf = true;
                    let nz = if ac.d.mach > 4.0 { 2.0 } else { 3.0 };
                    reply = Ok(format!("AP MAX — DOMAINE COMPLET DE LA LOI NORMALE ({:.1} G, INCLINAISON {:.0} deg), PROTECTIONS ACTIVES", nz * 0.95, (1.0 / (nz * 0.95f64)).acos().to_degrees().min(67.0)));
                } else if matches!(arg(1), "NORMAL" | "CONFORT" | "NORM") {
                    ac.ap.max_perf = false;
                    reply = Ok("AP CONFORT (0.4-1.7 G, INCLINAISON 30-35 deg)".into());
                } else if airborne {
                    engage_ap(ac);
                    reply = Ok("AP1 ENGAGÉ".into());
                }
            }
            "MANUEL" | "MANUAL" => {
                ac.ap.on = false;
                ac.ap.at_on = false;
                ac.autoland = None;
                ac.autotakeoff = None;
                reply = Ok("COMMANDES MANUELLES (manche clavier)".into());
            }
            "MAJ" | "UPDATE" | "MISEAJOUR" => {
                self.update_cmd = true;
                reply = Ok("VÉRIFICATION / MISE À JOUR".into());
            }
            "VERSION" => {
                reply = Ok(format!("DARKSTAR v{} — github.com/{}", updater::VERSION, updater::REPO));
            }
            "SON" | "SOUND" | "AUDIO" => {
                self.muted = matches!(arg(1), "OFF" | "COUPE");
                reply = Ok(format!("SON {}", if self.muted { "COUPE" } else { "ACTIF" }));
            }
            "WARP" | "TEMPS" => {
                if let Some(x) = num(1) {
                    self.warp_idx = WARPS.iter().position(|&v| v >= x).unwrap_or(WARPS.len() - 1);
                }
                reply = Ok(format!("ACCÉLÉRATION x{:.0}", WARPS[self.warp_idx]));
            }
            "AIDE" | "HELP" | "?" => {
                for l in [
                    "CAP 270 | ALT 90000 / FL350 | ASSIETTE 10 | MACH 6 | KEAS 300 | VITESSE AUTO | THR 80",
                    "DESCENTE [50] : descente hypersonique à q constant (kPa) jusqu'au subsonique",
                    "DEST KEDW (destination, DEST seul = liste) | DIRECT KEDW | ATTERRIR [KEDW] | DECOLLAGE [15000]",
                    "TURB START/STOP | INLET OPEN/CLOSE | RAM ARM/IGN/OFF | SCRAM ENABLE/IGN/OFF",
                    "H2 DUMP [OFF] | FUEL DUMP [OFF] | OVERRIDE MOTEUR/FBW ON/OFF | OVERRIDE OFF",
                    "HSI 80 (portée NM)",
                    "MENU | START | STOP | ENG L ON/OFF/MOTOR | FADEC RESET [L|R] | ALERT START ON/OFF",
                    "AIR DATA OVRD ON/OFF | BIT | SYSTEM RESET | YAW TRIM -2.5 | G RESET (mémoire G max)",
                    "CAPTEUR 101 OFF|ON (inhiber/réactiver un capteur par son numéro) | CAPTEURS ON",
                    "FDR (enregistrer la trace) | ANALYSE (analyse du vol / du crash)",
                    "FIN (terminer le vol + débriefing) | QUITTER (sauvegarder et fermer)",
                    "PANNE ENGL|ENGR|GENL|GENR|POMPE|ADC|FCS|HYD|REFROID|PORTES|TRAIN|FUITEH2 [OFF] | PANNES OFF | TRAIN SECOURS",
                    "CAMERA ON|OFF|EO|IR|TRACK|NADIR  ZOOM 8  PHOTO  SAR STRIP|SPOT|OFF  ELINT ON|OFF",
                    "CIBLE B  DIRECT CIBLE  PLAN RECON  ROUTE RECON|OFF  MENACES ON|OFF  RECON (page)",
                    "METEO REEL (defaut) | CALME|BEAU|NUAGEUX|COUVERT|ORAGEUX  METEO 3D ON/OFF  HUD (infos 10 s)",
                    "WXR ON/OFF  WXR TILT -3 | WXR AUTO  WXR TURB ON/OFF  WXR HORIZON ON/OFF  WXR RANGE 160 (radar meteo)",
                    "GCAS ON/OFF (auto recovery)  FUELCOOL AUTO/RAM/OFF  GLIM NORM/HIGH  PITOT ON/OFF  NWS HI/LO",
                    "FEU L/R/CHAMB/H2 (poignee feu)  BTL 1/2 (extincteur)  FEU TEST  INCENDIE L/R/CHAMB/H2 (essai)",
                    "MCDU (plan de vol, page INIT/F-PLN/PERF/PROG/FUEL/DIR)  SIMBRIEF <id> (import de l'OFP)",
                    "TANKER [OACI|OFF]  RAVITAILLEMENT (autonome)  DECROCHER  BINGO [AUTO|t]",
                    "CHAFF  FLARE  ECM ON|OFF  CMDS AUTO|MAN  TIR [I|J|K] (HACM)  ARMEMENT (page)",
                    "EMERGENCY [OFF] (ravitaillement > terrain proche > ejection)  EJECTION CONFIRME",
                    "AP MAX (domaine complet de la loi FBW normale)  AP NORMAL (confort)  RESSENTI ON/OFF",
                    "TRAIN UP/DOWN | FREINS ON/OFF | AP ON/OFF | MANUEL | WARP 8",
                ] {
                    self.log.push((self.sim_time, Level::Info, l.into()));
                }
                return;
            }
            _ => {}
        }
        match reply {
            Ok(r) => {
                self.cues.push(audio::Cue::Ack);
                self.push(Level::Info, format!("DARKSTAR : {r} — COLLATIONNÉ"));
            }
            Err(e) => {
                self.cues.push(audio::Cue::Refused);
                self.push(Level::Caution, format!("DARKSTAR : IMPOSSIBLE — {e}"));
            }
        }
    }

    fn atc_flavor(&mut self) {
        let ac = &self.ac;
        let alt_ft = ac.alt / FT;
        let mut say: Option<&str> = None;
        if !self.atc[0] && ac.eng.turb == TurbState::Running && ac.on_ground {
            self.atc[0] = true;
            say = Some("PALMDALE TOUR : DARKSTAR 01, VENT CALME, PISTE 25 ALIGNEZ-VOUS, AUTORISÉ DÉCOLLAGE.");
        } else if !self.atc[1] && !ac.on_ground && alt_ft > 18_000.0 {
            self.atc[1] = true;
            say = Some("JOSHUA APPROCHE : DARKSTAR 01, CONTACT RADAR, ESPACE R-2508 ACTIF, MONTÉE SANS RESTRICTION.");
        } else if !self.atc[2] && alt_ft > 60_000.0 {
            self.atc[2] = true;
            say = Some("OAKLAND CENTRE : DARKSTAR 01, AU-DESSUS DU FL600, ESPACE CLASSE E, BONNE CHANCE.");
        } else if !self.atc[3] && ac.d.mach > 5.0 {
            self.atc[3] = true;
            say = Some("CONTRÔLE ESSAIS EDWARDS : HYPERSONIQUE CONFIRMÉ, TÉLÉMESURE NOMINALE.");
        } else if !self.atc[4] && ac.d.mach > 10.0 {
            self.atc[4] = true;
            say = Some("CONTRÔLE ESSAIS : MACH 10 ! L'AVION LE PLUS RAPIDE DU MONDE.");
        }
        if let Some(s) = say {
            self.push(Level::Info, s);
        }
    }
}

/// Approche réellement en cours (autoland engagé ou ILS armé / capturé, en vol) : seule
/// situation où la DA et les minimums ont un sens.
fn in_approach(ac: &Aircraft) -> bool {
    let ils = ac.ap.loc_armed || ac.ap.gs_armed || ac.ap.lat == LatMode::Loc || matches!(ac.ap.vert, VertMode::Gs | VertMode::Flare);
    !ac.on_ground && ac.crashed.is_none() && ac.approach.is_some() && (ac.autoland.is_some() || ils)
}

#[macroquad::main(window_conf)]
async fn main() {
    let scenario = std::env::args().nth(1).unwrap_or_default();
    let shot = std::env::var("DARKSTAR_SHOT").ok();

    clear_background(BLACK);
    ui::txt_c("DARKSTAR — chargement de la carte du monde...", screen_width() / 2.0, screen_height() / 2.0, 24.0, WHITE);
    ui::load_font();
    next_frame().await;
    ui::load_font();
    next_frame().await;
    let world = world::World::load();
    let mut tiles = tiles::Tiles::new();
    let perf = std::env::var("DARKSTAR_PERF").is_ok();
    let mut perf_t0 = get_time();
    let mut perf_frames = 0u32;
    let mut sfx = audio::Sfx::new().await;
    // Suivi de la liaison pour les sons (perte / rétablissement)
    let mut snd_link_down = false;
    let mut snd_threats = 0usize;
    let upd = updater::Updater::new();
    upd.check();
    let mut last_check = get_time();
    let auto_update = std::env::var("DARKSTAR_AUTOUPDATE").is_ok();
    let mut last_upd_msg = String::new();
    let mut g = Game::new(&scenario);
    // Dernier choix de l'écran de lancement (appareil, aérodrome, piste), reproposé au nouvel essai
    let mut last_spawn: Option<connect::ConnectUi> = None;
    // Premier lancement : téléchargement de la base mondiale (appliquée au lancement suivant)
    if navdb::installed_at().is_none() || cifp::cycle().is_none() {
        g.fms.navupd = Some(navdb::update());
        g.push(Level::Info, "NAVDATA : PREMIER LANCEMENT — TELECHARGEMENT DE LA BASE MONDE (OURAIRPORTS, GRATUITE)");
    }
    // Position de départ forcée pour les essais : DARKSTAR_POS=lat,lon,altitude_ft
    if let Ok(p) = std::env::var("DARKSTAR_POS") {
        let v: Vec<f64> = p.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() == 3 {
            g.ac.lat = v[0];
            g.ac.lon = v[1];
            g.ac.alt = v[2] * geo::FT;
        }
    }
    if let Ok(t) = std::env::var("DARKSTAR_TAB") {
        if let Some((tab, _)) = gcs::TABS.iter().find(|(_, l)| l.starts_with(t.to_uppercase().as_str())) {
            g.tab = *tab;
        }
    }
    // Instructions exécutées au démarrage (tests, scénarios) : DARKSTAR_CMD="CAPTEUR 101;ALT 30000"
    if let Ok(cmds) = std::env::var("DARKSTAR_CMD") {
        for c in cmds.split(';').filter(|c| !c.trim().is_empty()) {
            g.exec_command(c);
        }
    }
    // Lancement normal : établir d'abord la liaison avec un Darkstar de la flotte
    if (scenario.is_empty() && shot.is_none()) || std::env::var("DARKSTAR_CONNECT").is_ok() {
        let ui = connect::ConnectUi::new();
        last_spawn = Some(ui.clone());
        g.place_at(ui.sel, ui.apt, &ui.rwy);
        g.link.state = link::State::Idle;
        g.log.clear();
        g.connect = Some(ui);
    }
    // Départ imposé (essais) : DARKSTAR_START="EGLL 27L"
    if let Ok(st) = std::env::var("DARKSTAR_START") {
        let mut it = st.split_whitespace();
        if let Some(i) = it.next().and_then(|c| AIRPORTS.iter().position(|a| a.icao == c)) {
            let r = it.next().map(str::to_string).unwrap_or_else(|| connect::default_rwy(i));
            g.place_at(0, i, &r);
        }
    }
    let mut show_help = scenario.is_empty() && g.connect.is_none() && std::env::var("DARKSTAR_SHOT").is_err();
    let mut fullscreen = std::env::var("DARKSTAR_SHOT").is_err() && std::env::var("DARKSTAR_WINDOW").is_err();
    let mut map_full = false;
    let mut frame = 0u64;

    // Instructions différées pour les essais : DARKSTAR_LATE="1400:CAP 090;1500:TURB START"
    let late: Vec<(u64, String)> = std::env::var("DARKSTAR_LATE")
        .unwrap_or_default()
        .split(';')
        .filter_map(|c| c.split_once(':').and_then(|(f, cmd)| f.trim().parse().ok().map(|f| (f, cmd.to_string()))))
        .collect();
    loop {
        let real_dt = (get_frame_time() as f64).min(0.05);
        frame += 1;
        for (_, c) in late.iter().filter(|(f, _)| *f == frame) {
            g.exec_command(c);
        }

        // Écran de lancement : le clavier sert à la saisie de l'aérodrome (Tab = première suggestion)
        let in_connect = g.connect.is_some();
        // F11 : plein écran / fenêtre
        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }
        if is_key_pressed(KeyCode::Tab) && !in_connect {
            show_help = !show_help;
        }
        if is_key_pressed(KeyCode::V) && g.console.is_none() && !in_connect {
            map_full = !map_full;
        }
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter)) && !in_connect;
        if enter && (g.ac.crashed.is_some() || g.ended || (shift && g.console.is_none())) {
            let muted = g.muted;
            let dev = g.dev.take();
            g.fdr.finalize();
            g = Game::new(&scenario);
            g.muted = muted;
            g.dev = dev;
            pfd::hide_menu(g.dev.is_some());
            sfx.reset();
            g.feel.reset();
            // Nouvel essai : l'écran de lancement repropose l'appareil, l'aérodrome et la piste
            if let Some(ui) = last_spawn.clone().filter(|_| shot.is_none()) {
                let ui = connect::ConnectUi { query: String::new(), ..ui };
                g.place_at(ui.sel, ui.apt, &ui.rwy);
                g.link.state = link::State::Idle;
                g.log.clear();
                g.connect = Some(ui);
            }
        } else if enter {
            match g.console.take() {
                None => {
                    while get_char_pressed().is_some() {}
                    g.console = Some(String::new());
                    g.hist_pos = g.cmd_hist.len();
                }
                Some(line) => {
                    if !line.trim().is_empty() {
                        g.cmd_hist.push(line.clone());
                        g.exec_command(&line);
                    }
                }
            }
        }
        let alerts = g.alerts();
        if is_key_pressed(KeyCode::Space) && g.console.is_none() {
            g.acked = alerts.iter().map(|(_, s)| s.clone()).collect();
        }
        if let Some(line) = g.console.as_mut() {
            while let Some(ch) = get_char_pressed() {
                if !ch.is_control() && line.len() < 60 {
                    line.extend(ch.to_uppercase());
                    g.cues.push(audio::Cue::Key);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                line.pop();
            }
            if is_key_pressed(KeyCode::Up) && g.hist_pos > 0 {
                g.hist_pos -= 1;
                *line = g.cmd_hist[g.hist_pos].clone();
            }
            if is_key_pressed(KeyCode::Escape) {
                g.console = None;
            }
            g.stick = (0.0, 0.0);
        } else if g.mcdu_seen && g.fms.focus && !g.ended && g.connect.is_none() {
            // Saisie MCDU : les touches alphanumériques vont dans la ligne de saisie
            while let Some(ch) = get_char_pressed() {
                let c = ch.to_ascii_uppercase();
                if (c.is_ascii_alphanumeric() || "/.-+_ ".contains(c)) && g.fms.scratch.len() < 30 {
                    if g.fms.scratch == "CLR" {
                        g.fms.scratch.clear();
                    }
                    g.fms.scratch.push(c);
                    g.fms.msg.clear();
                    g.cues.push(audio::Cue::Key);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                g.fms.scratch.pop();
            }
            if is_key_pressed(KeyCode::Escape) {
                g.fms.focus = false;
            }
            if is_key_pressed(KeyCode::PageDown) {
                g.fms.slew(true);
            }
            if is_key_pressed(KeyCode::PageUp) {
                g.fms.slew(false);
            }
            g.input_stick(real_dt);
        } else if !g.ended && g.connect.is_none() {
            while get_char_pressed().is_some() {}
            g.input(real_dt);
        }
        if g.connect.is_some() {
            // Poignée de main de la liaison (simulation suspendue)
            g.link.update(&g.ac, real_dt, 0.0);
            let lev: Vec<_> = g.link.events.drain(..).collect();
            for (l, s) in lev {
                g.link_msg = s.clone();
                g.push(l, s);
            }
            if g.link.state == link::State::Connected {
                last_spawn = g.connect.take();
                let tail = link::tail();
                let a = &AIRPORTS[g.dep_apt];
                g.push(Level::Info, format!("GCS EN LIGNE — DARKSTAR {tail} CONTROLE PAR GCS-1"));
                let num = a.end_ident(g.ac.hdg_deg(), Some((g.ac.lat, g.ac.lon)));
                g.push(Level::Info, format!("TOUR {} ({}) : {tail}, ALIGNE PISTE {num}, PRET AU DECOLLAGE", a.name, a.icao));
            }
        } else if !g.ended {
            g.update(real_dt);
        }

        let alerts = g.alerts();
        let unacked = |lv: Level| alerts.iter().any(|(l, s)| *l == lv && !g.acked.contains(s));
        let master_warn = unacked(Level::Warning) || g.ac.crashed.is_some();
        let master_caut = unacked(Level::Caution);
        sfx.muted = g.muted;
        sfx.update(&g.ac, &alerts, &g.acked);
        g.feel.update(&g.ac, get_frame_time());
        sfx.feel(g.feel.rumble, std::mem::take(&mut g.feel.boom), std::mem::take(&mut g.feel.joint), g.feel.touch.take(), std::mem::take(&mut g.feel.nose_down));
        sfx.mishap(g.mishap.as_ref().is_some_and(|m| m.cascading()));
        // Voix d'alerte : chargement des phrases générées ; RWR (niveau de menace, nouvel émetteur)
        sfx.voice.poll().await;
        {
            let cues = g.recon.threat_cues(&g.ac);
            let lvl = cues.iter().map(|c| c.state).max().unwrap_or(0).max(if g.combat.missiles.is_empty() { 0 } else { 4 });
            let heard = cues.len();
            sfx.threat(lvl, heard > snd_threats);
            snd_threats = heard;
        }
        // Liaison : perte (incident ou liaison coupée), gazouillis de modem pendant la reconnexion
        let link_down = g.mishap.is_some() || g.link.state == link::State::Lost;
        let reconnecting = g.mishap.as_ref().is_some_and(|m| matches!(m.second, mishap::Second::Trying(_))) || g.connect.is_some();
        sfx.link(link_down && !snd_link_down, reconnecting);
        if snd_link_down && !link_down && g.ac.crashed.is_none() {
            g.cues.push(audio::Cue::LinkOk);
        }
        snd_link_down = link_down;
        if is_mouse_button_pressed(MouseButton::Left) {
            g.cues.push(audio::Cue::Click);
        }
        for c in g.cues.drain(..) {
            sfx.cue(c);
        }

        // --- Mise en page ---
        g.mcdu_seen = false;
        tiles.begin_frame();
        clear_background(ui::BG);
        let (sw, sh) = (screen_width(), screen_height());
        let time = get_time();
        let (warp_eff, warp_capped) = g.effective_warp();
        let (ln, lat) = g.link();
        let bar = Rect::new(0.0, 0.0, sw, 34.0);
        let body = Rect::new(6.0, 40.0, sw - 12.0, sh - 46.0);
        if let Some(mut d) = g.dev.take() {
            let quit = draw_dev(&mut g, &mut d, &world, &mut tiles, &alerts, sw, sh, time, &ln, lat, warp_eff, master_warn, master_caut);
            if quit {
                pfd::hide_menu(false);
                g.layout = dev::DevUi::new();
                g.push(Level::Info, "VERSION SIMULATEUR");
            } else {
                g.dev = Some(d);
            }
        } else if map_full {
            g.map.input(body);
            let route = g.fms.route();
            g.map.draw(&world, &mut tiles, &g.wx, &g.refuel, &g.combat, &g.recon, &g.ac, body, g.dest, &route, time);
        } else {
            // Écran principal Darkstar, aux proportions de l'écran du cockpit
            let mfd_w = (body.w * 0.67).min((body.h - 120.0) * mfd::COCKPIT_ASPECT).floor();
            let mfd_h = (mfd_w / mfd::COCKPIT_ASPECT).floor();
            let mfd_r = Rect::new(body.x, body.y, mfd_w, mfd_h);
            let lit: [bool; 6] = std::array::from_fn(|i| g.tab == BEZEL_PAGES[i]);
            let info = pfd_info(&g, &alerts, &ln, lat, warp_eff, time);
            let shown = g.stale.as_ref().unwrap_or(&g.ac);
            // Écran du cockpit : le même ECRAN à pages que la version développeur, lunette conservée
            drop(info);
            let _ = shown;
            let bez = mfd::bezel_only(mfd_r, lit, time);
            handle_mfd(&mut g, mfd::MfdActions { bezel: bez, ..Default::default() });
            let disp = mfd::display_rect(mfd_r);
            draw_rectangle(disp.x, disp.y, disp.w, disp.h, BLACK);
            let mut lay_ui = std::mem::take(&mut g.layout);
            let hz = draw_screen(&mut g, &mut lay_ui, disp, &world, &mut tiles, &alerts, time, &ln, lat, warp_eff);
            g.layout = lay_ui;
            let (svs_r, pfd_r) = match hz {
                Some(r) => (pfd::geo(r).inner, r),
                None => (disp, disp),
            };
            draw_console(&g, svs_r);
            // Écran noir : reconnexion automatique par le canal d'urgence ; nouvel essai après échec
            if g.mishap.as_ref().is_some_and(|m| m.draw_los(svs_r)) {
                if let Some(m) = &mut g.mishap {
                    m.try_second();
                }
                g.push(Level::Info, "GCS > NOUVEL ESSAI DE RECONNEXION (CANAL D'URGENCE)");
            }
            if let Some(m) = &g.mishap {
                // Analyse de l'enregistreur une fois la perte confirmée
                if m.second == mishap::Second::Failed && !g.crash_report.is_empty() {
                    let lr = Rect::new(svs_r.x, svs_r.y + svs_r.h * 0.55, svs_r.w, svs_r.h * 0.45);
                    draw_crash_report(&g.crash_report, lr);
                }
            } else if g.ac.crashed.is_some() && !g.crash_report.is_empty() {
                draw_crash_report(&g.crash_report, svs_r);
            }
            draw_flashes(&mut g.flashes, svs_r);
            if !g.ac.on_ground && g.mishap.is_none() {
                combat::draw_hud(&g.combat, svs_r);
            }
            emergency::draw_hud(&g.emerg, &g.ac, svs_r, time);
            // Mode dégradé après reconnexion
            if g.degraded.is_some() && g.mishap.is_none() {
                let fcs = g.ac.fails.has(systems::Fail::FcsDirect) && g.ac.fails.has(systems::Fail::Hyd);
                let m = if fcs { "IRIDIUM — TELEMESURE, CAMERA, TERMINAL — FCS PERDU" } else { "IRIDIUM — TELEMESURE, CAMERA, TERMINAL — MANCHE INDISPONIBLE" };
                ui::tag(svs_r.x + svs_r.w / 2.0, svs_r.y + 18.0, "LIAISON", m, if fcs { Color::new(1.0, 0.3, 0.25, 1.0) } else { ui::AMBER }, 12.0, false, true);
            }
            // Décision aux minimums : CONTINUE ou GO AROUND
            if g.mins == 2 {
                let (cx, y) = (svs_r.x + svs_r.w / 2.0, svs_r.y + svs_r.h * 0.62);
                let blink = (time * 2.0).fract() < 0.6;
                ui::rrect(Rect::new(cx - 160.0, y - 30.0, 320.0, 68.0), 8.0, Color::new(0.0, 0.0, 0.0, 0.8));
                if blink {
                    ui::txt_c("MINIMUMS", cx, y - 10.0, 20.0, ui::AMBER);
                }
                let bc = Rect::new(cx - 150.0, y + 4.0, 140.0, 26.0);
                let bg = Rect::new(cx + 10.0, y + 4.0, 140.0, 26.0);
                for (b, l, c) in [(bc, "CONTINUE", ui::GREEN_S), (bg, "GO AROUND", ui::AMBER)] {
                    ui::rrect(b, 6.0, if ui::hovered(b) { Color::new(0.15, 0.17, 0.2, 1.0) } else { Color::new(0.08, 0.09, 0.11, 1.0) });
                    ui::rrect_lines(b, 6.0, 1.5, c);
                    ui::txt_c(l, b.x + b.w / 2.0, b.y + 18.0, 14.0, c);
                }
                if ui::clicked(bc) {
                    g.exec_command("CONTINUE");
                } else if ui::clicked(bg) {
                    g.exec_command("GO AROUND");
                }
            }
            // Liaison perdue ou réduite au secours : bandeau et choix du relais
            // (en mode dégradé après reconnexion, le canal d'urgence est voulu : pas de panneau)
            if g.connect.is_none() && g.ac.crashed.is_none() && !g.link.full() && g.degraded.is_none() {
                let last = g.stale.as_ref().unwrap_or(&g.ac);
                if let Some(p) = link::draw_lost(&g.link, last, svs_r, time) {
                    g.link.connect(p);
                }
            }
            // Bouton MENU du PFD : ouvre l'onglet CONSOLE
            let menu_btn = pfd::menu_rect(pfd_r);
            if g.tab == gcs::Tab::Console {
                draw_rectangle_lines(menu_btn.x - 1.0, menu_btn.y - 1.0, menu_btn.w + 2.0, menu_btn.h + 2.0, 2.0, ui::ACCENT);
            }
            if ui::clicked(menu_btn) {
                g.tab = gcs::Tab::Console;
            }

            // Sous l'écran : bloc TRAIN et messages
            let by = mfd_r.y + mfd_r.h + 6.0;
            let bh = body.y + body.h - by;
            let gear_r = Rect::new(body.x, by, 112.0, bh);
            ui::panel(gear_r, "TRAIN");
            let unsafe_gear = alerts.iter().any(|(_, a)| a == "TOO LOW — GEAR");
            pfd::draw_gear(&g.ac, gear_r.x + gear_r.w / 2.0, gear_r.y + 24.0 + (gear_r.h - 24.0) / 2.0, time, unsafe_gear);
            panels::draw_messages(&g.log, Rect::new(body.x + 118.0, by, mfd_w - 118.0, bh));

            // Colonne de droite : carte, onglets (2 rangées), page sélectionnée
            let rx = mfd_r.x + mfd_r.w + 8.0;
            let rw = body.x + body.w - rx;
            let map_h = (body.h * 0.36).floor();
            let mr = Rect::new(rx, body.y, rw, map_h);
            g.map.input(mr);
            let route = g.fms.route();
            g.map.draw(&world, &mut tiles, &g.wx, &g.refuel, &g.combat, &g.recon, &g.ac, mr, g.dest, &route, time);
            let tabs_r = Rect::new(rx, mr.y + mr.h + 6.0, rw, 54.0);
            let mut alert_tabs = Vec::new();
            if g.ac.ovr_engine || g.ac.ovr_fbw || g.ac.h2_dump || g.ac.fuel_dump {
                alert_tabs.push(gcs::Tab::Override);
            }
            if alarms::lit_count(&g.ac, &alerts) > 0 {
                alert_tabs.push(gcs::Tab::Alarmes);
            }
            if let Some(t) = gcs::draw_tabs(tabs_r, g.tab, &alert_tabs) {
                g.tab = t;
            }
            let cr = Rect::new(rx, tabs_r.y + tabs_r.h + 4.0, rw, body.y + body.h - (tabs_r.y + tabs_r.h + 4.0));
            let tab = g.tab;
            if let Some(t) = draw_tab_page(&mut g, tab, cr, &alerts, &mut tiles, time) {
                g.tab = t;
            }
        }
        let top = if g.dev.is_some() { gcs::TopActions::default() } else { gcs::draw_topbar(
            bar,
            &gcs::TopBar {
                sim_time: g.sim_time,
                mission_t: g.mission.elapsed,
                warp: warp_eff,
                warp_capped,
                link: &ln,
                master_warn,
                master_caut,
                muted: g.muted,
                time,
            },
        ) };
        if top.ack {
            g.acked = alerts.iter().map(|(_, s)| s.clone()).collect();
        }
        if top.warp {
            g.warp_idx = (g.warp_idx + 1) % WARPS.len();
        }
        if top.mute {
            g.muted = !g.muted;
        }
        if top.help {
            show_help = !show_help;
        }
        if g.ended {
            draw_debrief(&g.debrief, Rect::new(0.0, 0.0, sw, sh));
        }
        if show_help {
            panels::draw_help(Rect::new(0.0, 0.0, sw, sh));
        }
        if let Some(ui) = &g.connect {
            match connect::draw(ui, &g.link, &g.link_msg, sw, sh, time) {
                Some(connect::ConnectAction::Select(i)) => {
                    // Appareil de la flotte : son terrain de base par défaut
                    let apt = apt_index(link::FLEET[i].1);
                    let u = connect::ConnectUi { sel: i, apt, rwy: connect::default_rwy(apt), query: String::new() };
                    g.place_at(u.sel, u.apt, &u.rwy);
                    g.connect = Some(u);
                }
                Some(connect::ConnectAction::Airport(a)) => {
                    let u = connect::ConnectUi { sel: ui.sel, apt: a, rwy: connect::default_rwy(a), query: String::new() };
                    g.place_at(u.sel, u.apt, &u.rwy);
                    g.connect = Some(u);
                }
                Some(connect::ConnectAction::Runway(r)) => {
                    let u = connect::ConnectUi { rwy: r, ..ui.clone() };
                    g.place_at(u.sel, u.apt, &u.rwy);
                    g.connect = Some(u);
                }
                Some(connect::ConnectAction::Query(q)) => {
                    g.connect = Some(connect::ConnectUi { query: q, ..ui.clone() });
                }
                Some(connect::ConnectAction::Connect) => g.link.connect(link::Path::Los),
                None => {}
            }
        }

        // --- Mise à jour ---
        // Vérification toutes les 5 min (1 min après une erreur réseau)
        let every = if matches!(upd.get(), updater::State::Error(_)) { 60.0 } else { 300.0 };
        if get_time() - last_check > every {
            last_check = get_time();
            upd.check();
        }
        let st = upd.get();
        if std::mem::take(&mut g.update_cmd) {
            match st {
                updater::State::Available { .. } => upd.download(),
                _ => upd.check(),
            }
        }
        let banner_click = draw_update_banner(&st, sw);
        if banner_click || (auto_update && matches!(st, updater::State::Available { .. })) {
            upd.download();
        }
        let msg = match &st {
            updater::State::Available { version, .. } => format!("MISE À JOUR DISPONIBLE : v{version} (instruction MAJ ou clic sur le bandeau)"),
            updater::State::UpToDate => format!("DARKSTAR v{} — À JOUR", updater::VERSION),
            updater::State::Error(e) => format!("MISE À JOUR : {e}"),
            updater::State::Downloading { version } => format!("TÉLÉCHARGEMENT DE LA v{version}..."),
            _ => String::new(),
        };
        if !msg.is_empty() && msg != last_upd_msg {
            g.push(Level::Info, msg.clone());
            last_upd_msg = msg;
        }
        if let updater::State::Ready { script } = &st {
            clear_background(BLACK);
            ui::txt_c("INSTALLATION DE LA MISE À JOUR — REDÉMARRAGE...", sw / 2.0, sh / 2.0, 26.0, WHITE);
            next_frame().await;
            if updater::Updater::launch_swap(script) {
                std::process::exit(0);
            }
        }

        if let Some(path) = &shot {
            let at: u64 = std::env::var("DARKSTAR_SHOT_FRAME").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
            if frame == at {
                get_screen_data().export_png(path);
                std::process::exit(0);
            }
        }
        tiles.end_frame();
        // Diagnostic de performance (DARKSTAR_PERF=1) : cadence et état du cache de tuiles
        if perf {
            // Ralentissement artificiel (DARKSTAR_PERF=<ms>) pour éprouver le cache à faible cadence
            if let Ok(ms) = std::env::var("DARKSTAR_PERF").unwrap_or_default().parse::<u64>() {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
            perf_frames += 1;
            if get_time() - perf_t0 > 2.0 {
                let (t, d, i, e) = tiles.stats();
                eprintln!("PERF {:.0} img/s | tuiles GPU {t} attente {d} en cours {i} relief {e}", perf_frames as f64 / (get_time() - perf_t0));
                perf_t0 = get_time();
                perf_frames = 0;
            }
        }
        next_frame().await;
    }
}

fn draw_console(g: &Game, r: Rect) {
    let y = r.y + 42.0;
    let w = (r.w * 0.6).max(420.0).min(r.w - 20.0);
    let x = r.x + (r.w - w) / 2.0;
    match &g.console {
        Some(line) => {
            draw_rectangle(x, y, w, 30.0, Color::new(0.0, 0.05, 0.1, 0.92));
            draw_rectangle_lines(x, y, w, 30.0, 2.0, ui::CYAN_S);
            let cursor = if (get_time() * 2.0).fract() < 0.5 { "_" } else { " " };
            ui::txt(&format!("INSTRUCTION > {line}{cursor}"), x + 10.0, y + 21.0, 20.0, ui::CYAN_S);
            ui::txt("ENTRÉE valider  ÉCHAP annuler  AIDE = liste", x + 10.0, y + 46.0, 14.0, ui::DIM);
        }
        None => {
            let s = match (&g.ac.autoland, &g.ac.autotakeoff) {
                // L'autoland est affiché par la bannière d'approche
                (Some(_), _) => String::new(),
                (_, Some(_)) => "DÉCOLLAGE AUTOMATIQUE EN COURS".to_string(),
                // Rappel de la saisie seulement en début d'essai (dégage le bas de l'horizon)
                _ if g.sim_time < 45.0 => "ENTRÉE : saisir une instruction (CAP, ALT, MACH, DIRECT, ATTERRIR...)".to_string(),
                _ => String::new(),
            };
            let active = g.ac.autotakeoff.is_some();
            let c = if active { ui::GREEN_S } else { Color::new(0.75, 0.8, 0.85, 0.55) };
            if !s.is_empty() && (active || g.ac.approach.is_none()) {
                ui::txt_c(&s, r.x + r.w / 2.0, r.y + r.h - 10.0, if active { 15.0 } else { 12.0 }, c);
            }
        }
    }
}

/// Bandeau de mise à jour en haut de l'écran ; renvoie vrai si l'utilisateur a cliqué dessus.
/// Page d'onglet dans le rectangle `cr` (écran principal ou version développeur). Renvoie l'onglet
/// demandé par la page (navigation depuis SYSTEMES).
fn draw_tab_page(g: &mut Game, tab: gcs::Tab, cr: Rect, alerts: &[(Level, String)], tiles: &mut tiles::Tiles, time: f64) -> Option<gcs::Tab> {
    let mut nav = None;
    match tab {
        gcs::Tab::Systemes => {
            if let Some(t) = syspage::draw(&g.ac, &g.thermal, &g.mfd.wxr, cr, time) {
                nav = Some(t);
            }
        }
        gcs::Tab::Moteurs => panels::draw_engines(&g.ac, &g.chambers, cr, time),
        gcs::Tab::Mcdu => {
            g.mcdu_seen = true;
            // Clic hors du MCDU : il rend le clavier (raccourcis, pilotage) ; clic sur l'écran : focus
            if is_mouse_button_pressed(MouseButton::Left) && !cr.contains(mouse_position().into()) {
                g.fms.focus = false;
            }
            if let Some(k) = fms::draw_mcdu(&g.fms, &g.ac, cr, time) {
                g.mcdu_key(k);
            }
        }
        gcs::Tab::Incendie => {
            if let Some(a) = fire::draw_fire(&g.ac, cr, time) {
                let cmd = match a {
                    fire::FireAction::Handle(z) => format!("FEU {}", ["L", "R", "CHAMB", "H2"][z]),
                    fire::FireAction::Bottle(b) => format!("BTL {}", b + 1),
                    fire::FireAction::Test => "FEU TEST".into(),
                };
                g.exec_command(&cmd);
            }
        }
        gcs::Tab::Alarmes => match alarms::draw_alarms(&g.ac, &alerts, cr, time, g.mishap.as_ref().map(|m| m.lights()).or(g.tree)) {
            Some(alarms::AlarmAction::Toggle(f)) => {
                let on = !g.ac.fails.has(f);
                g.ac.set_failure(f, on);
                g.cues.push(audio::Cue::Ack);
            }
            Some(alarms::AlarmAction::RepairAll) => {
                g.tree = None;
                g.exec_command("PANNES OFF");
            }
            None => {}
        },
        gcs::Tab::Limites => limits::draw_limits(&g.ac, cr),
        gcs::Tab::Elec => {
            if let Some(a) = elec::draw_elec(&g.ac, cr, time) {
                let el = &mut g.ac.elec;
                match a {
                    elec::EnvAction::TempKnob(v) => el.temp_knob = v,
                    elec::EnvAction::FaceHeat(v) => el.faceheat = v,
                    elec::EnvAction::SelToggle => el.sel_ebay = !el.sel_ebay,
                    elec::EnvAction::AutoToggle => el.auto_temp = !el.auto_temp,
                }
                g.cues.push(audio::Cue::Ack);
            }
        }
        gcs::Tab::Thermique => {
            if let Some(i) = thermal::draw_thermal(&g.thermal, cr, time) {
                // Clic sur un capteur déjà sélectionné : désélection
                g.thermal.selected = if g.thermal.selected == Some(i) { None } else { Some(i) };
            }
        }
        gcs::Tab::Essais => panels::draw_card(&g.ac, &g.mission, cr),
        gcs::Tab::Domaine => panels::draw_envelope(&g.ac, cr, &g.history),
        gcs::Tab::Override => {
            if let Some(sw) = gcs::draw_overrides(&g.ac, &g.chambers, cr) {
                g.apply_switch(sw);
            }
        }
        gcs::Tab::Fcs => fcs::draw_fcs(&g.ac, cr),
        // Commandes en boutons (version embarquée : pas de console dans l'avion réel)
        gcs::Tab::Commandes => draw_commandes(g, cr),
        // Composition de la page 1 (écran principal) : canevas et palette
        gcs::Tab::Config => g.layout.config(cr),
        // Panneau droit : boutons moteurs et APU
        gcs::Tab::Apu => {
            if let Some(c) = dev::right_panel(&g.ac, cr) {
                g.exec_command(&c);
                g.cues.push(audio::Cue::Switch);
            }
        }
        gcs::Tab::Armement => {
            if let Some(a) = combat::draw_page(&g.combat, &g.recon.emitters, &g.ac, cr) {
                let r = match a {
                    combat::ArmAction::Chaff => g.combat.dispense(combat::Seeker::Radar, &g.ac),
                    combat::ArmAction::Flare => g.combat.dispense(combat::Seeker::Ir, &g.ac),
                    combat::ArmAction::Ecm => {
                        g.combat.ecm = !g.combat.ecm;
                        Ok(format!("ECM {}", if g.combat.ecm { "ON" } else { "OFF" }))
                    }
                    combat::ArmAction::Auto => {
                        g.combat.cmds_auto = !g.combat.cmds_auto;
                        Ok(format!("CMDS {}", if g.combat.cmds_auto { "AUTO" } else { "MANUEL" }))
                    }
                    combat::ArmAction::Fire(k) => g.combat.fire_hacm(k, &g.recon.emitters[k], &g.ac),
                };
                match r {
                    Ok(m) => g.push(Level::Info, m),
                    Err(e) => g.push(Level::Caution, format!("ARMEMENT : {e}")),
                }
                g.cues.push(audio::Cue::Ack);
            }
        }
        gcs::Tab::Recon => {
            let ln = g.link().0;
            if let Some(a) = recon::draw_recon(&g.ac, &g.recon, tiles, cr, &ln) {
                let t = g.mission.elapsed;
                g.recon.apply(a, &mut g.ac, t);
                g.cues.push(audio::Cue::Ack);
            }
        }
        gcs::Tab::Console => {
            let bit = menu::BitState { running: g.bit_timer > 0.0, fail: g.bit_fail };
            if let Some(a) = menu::draw_menu(&g.ac, &bit, cr, time) {
                g.apply_menu(a);
            }
        }
    }
    nav
}

/// Version développeur : fenêtres (arrière vers avant), pages de l'ECRAN, page 1 composée, barre de
/// menus avec la console. Renvoie vrai pour revenir à la version simulateur.
#[allow(clippy::too_many_arguments)]
fn draw_dev(g: &mut Game, d: &mut dev::DevUi, world: &world::World, tiles: &mut tiles::Tiles, alerts: &[(Level, String)], sw: f32, sh: f32, time: f64, ln: &str, lat: f64, warp: f64, warn: bool, caut: bool) -> bool {
    d.begin(sw, sh);
    let console_open = g.console.is_some();
    for (i, kind) in d.order() {
        ui::set_blockers(d.blockers_above(i, sw, console_open));
        let Some(cr) = d.frame(i) else { continue };
        ui::scissor(Some(cr));
        match kind {
            dev::WinKind::Ecran => {
                let _ = draw_screen(g, d, cr, world, tiles, alerts, time, ln, lat, warp);
            }
            dev::WinKind::Messages => panels::draw_messages(&g.log, cr),
            dev::WinKind::Train => draw_elem(g, dev::Elem::Train, cr, world, tiles, alerts, time, ln, lat, warp),
            dev::WinKind::Nd => draw_elem(g, dev::Elem::Nd, cr, world, tiles, alerts, time, ln, lat, warp),
            dev::WinKind::Moteurs => panels::draw_engines(&g.ac, &g.chambers, cr, time),
            dev::WinKind::Elec => {
                let _ = draw_tab_page(g, gcs::Tab::Elec, cr, alerts, tiles, time);
            }
            // Panneaux de la console : droit (démarrage moteurs / APU) et gauche (platines)
            dev::WinKind::PanneauDroit => {
                if let Some(a) = menu::right_console(&g.ac, cr, time) {
                    g.apply_menu(a);
                    g.cues.push(audio::Cue::Switch);
                }
            }
            dev::WinKind::Commandes => draw_commandes(g, cr),
            dev::WinKind::PanneauGauche => {
                let bit = menu::BitState { running: g.bit_timer > 0.0, fail: g.bit_fail };
                if let Some(a) = menu::draw_platines(&g.ac, &bit, cr, time) {
                    g.apply_menu(a);
                    g.cues.push(audio::Cue::Switch);
                }
            }
            dev::WinKind::Pannes => {
                let _ = draw_tab_page(g, gcs::Tab::Alarmes, cr, alerts, tiles, time);
            }
        }
        ui::scissor(None);
    }
    ui::set_blockers(Vec::new());
    let mut status = format!("SIM {}   x{:.0}   LIAISON {}", mission::fmt_time(g.sim_time), warp, ln);
    if warn {
        status = format!("MASTER WARNING   {status}");
    } else if caut {
        status = format!("MASTER CAUTION   {status}");
    }
    let log: Vec<String> = g.log.iter().rev().take(8).rev().map(|(t, _, m)| format!("{} {m}", mission::fmt_time(*t))).collect();
    let out = d.bar(sw, g.console.as_deref(), &g.cmd_hist, &log, &status);
    if let Some(open) = out.console_open {
        g.console = if open { Some(String::new()) } else { None };
        g.hist_pos = g.cmd_hist.len();
    }
    out.quit
}

/// ECRAN à pages (version dev et écran du cockpit de la version normale) : bandeau des pages,
/// page 1 composée, pages FCS / THERMIQUE / RECON / SYSTEMES (+ ARMEMENT, DONNEES MOTEUR) /
/// INCENDIE / MCDU, page CONFIG. Renvoie la zone de l'horizon (bandeaux et alertes de l'horizon).
#[allow(clippy::too_many_arguments)]
fn draw_screen(g: &mut Game, d: &mut dev::DevUi, cr: Rect, world: &world::World, tiles: &mut tiles::Tiles, alerts: &[(Level, String)], time: f64, ln: &str, lat: f64, warp: f64) -> Option<Rect> {
    let area = d.screen_tabs(cr);
    let tab = match d.page {
        dev::Page::Fcs => Some(gcs::Tab::Fcs),
        dev::Page::Thermique => Some(gcs::Tab::Thermique),
        dev::Page::Recon => Some(gcs::Tab::Recon),
        dev::Page::Systemes => Some(gcs::Tab::Systemes),
        dev::Page::Incendie => Some(gcs::Tab::Incendie),
        dev::Page::Mcdu => Some(gcs::Tab::Mcdu),
        dev::Page::Armement => Some(gcs::Tab::Armement),
        dev::Page::DonneesMoteur => Some(gcs::Tab::Moteurs),
        _ => None,
    };
    let mut horizon = None;
    match (d.page, tab) {
        (dev::Page::Config, _) => d.config(area),
        (dev::Page::Horizon, _) => {
            for (e, r) in d.page1(area) {
                ui::scissor(Some(r));
                draw_elem(g, e, r, world, tiles, alerts, time, ln, lat, warp);
                ui::scissor(None);
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::new(0.25, 0.27, 0.3, 1.0));
                if e == dev::Elem::Horizon {
                    horizon = Some(r);
                }
            }
        }
        (_, Some(t)) => {
            match draw_tab_page(g, t, area, alerts, tiles, time) {
                Some(gcs::Tab::Armement) => d.page = dev::Page::Armement,
                Some(gcs::Tab::Moteurs) => d.page = dev::Page::DonneesMoteur,
                Some(gcs::Tab::Fcs) => d.page = dev::Page::Fcs,
                Some(gcs::Tab::Thermique) => d.page = dev::Page::Thermique,
                Some(gcs::Tab::Incendie) => d.page = dev::Page::Incendie,
                _ => {}
            }
        }
        _ => {}
    }
    horizon
}

/// Page COMMANDES : toutes les fonctions en boutons ; touches virtuelles ou instructions.
fn draw_commandes(g: &mut Game, cr: Rect) {
    let dest = g.fms.fpln.as_ref().map(|p| p.dest.clone()).unwrap_or_else(|| AIRPORTS[g.dest].icao.to_string());
    let cx = fcu::Ctx {
        vnav: g.fms.vnav,
        dest: &dest,
        warp: WARPS[g.warp_idx],
        muted: g.muted,
        escort: matches!(g.escort.state, escort::State::Joining | escort::State::Formation),
        emergency: g.emerg.mode.is_some(),
    };
    match fcu::draw(&g.ac, &cx, cr) {
        Some(fcu::Act::Key(k)) => {
            g.vkeys.push(k);
            g.fms.focus = false;
            g.cues.push(audio::Cue::Switch);
        }
        Some(fcu::Act::Cmd(c)) => {
            g.exec_command(&c);
            g.cues.push(audio::Cue::Switch);
        }
        None => {}
    }
}

/// Élément de la page 1 (ou fenêtre) dessiné dans `r`.
#[allow(clippy::too_many_arguments)]
fn draw_elem(g: &mut Game, e: dev::Elem, r: Rect, world: &world::World, tiles: &mut tiles::Tiles, alerts: &[(Level, String)], time: f64, ln: &str, lat: f64, warp: f64) {
    let tab = |t| Some(t);
    let t = match e {
        dev::Elem::Horizon => {
            let info = pfd_info(g, alerts, ln, lat, warp, time);
            let shown = g.stale.as_ref().unwrap_or(&g.ac);
            let _ = mfd::draw_horizon(world, tiles, &g.wx, &g.refuel, shown, &g.recon, r, &mut g.mfd, &info, g.style, time, &g.feel);
            None
        }
        dev::Elem::Carburant => {
            if mfd::fuel_pane(&g.ac, r) {
                handle_mfd(g, mfd::MfdActions { fuel_dump: true, ..Default::default() });
            }
            None
        }
        dev::Elem::Camera => {
            let cam = mfd::camera_pane(&g.ac, &g.recon, tiles, r);
            if cam.is_some() {
                handle_mfd(g, mfd::MfdActions { cam, ..Default::default() });
            }
            None
        }
        dev::Elem::Hsi => {
            let dest = g.dest;
            mfd::hsi_pane(&g.ac, r, &mut g.mfd, dest);
            None
        }
        dev::Elem::Nd => {
            g.map.input(r);
            let route = g.fms.route();
            g.map.draw(world, tiles, &g.wx, &g.refuel, &g.combat, &g.recon, &g.ac, r, g.dest, &route, time);
            None
        }
        dev::Elem::Train => {
            let unsafe_gear = alerts.iter().any(|(_, a)| a == "TOO LOW — GEAR");
            pfd::draw_gear(&g.ac, r.x + r.w / 2.0, r.y + r.h / 2.0, time, unsafe_gear);
            None
        }
        dev::Elem::Messages => {
            panels::draw_messages(&g.log, r);
            None
        }
        dev::Elem::DonneesMoteur => tab(gcs::Tab::Moteurs),
        dev::Elem::Systemes => tab(gcs::Tab::Systemes),
        dev::Elem::Fcs => tab(gcs::Tab::Fcs),
        dev::Elem::Alarmes => tab(gcs::Tab::Alarmes),
        dev::Elem::Thermique => tab(gcs::Tab::Thermique),
        dev::Elem::Elec => tab(gcs::Tab::Elec),
        dev::Elem::Mcdu => tab(gcs::Tab::Mcdu),
        dev::Elem::Incendie => tab(gcs::Tab::Incendie),
        dev::Elem::Limites => tab(gcs::Tab::Limites),
        dev::Elem::Commandes => {
            draw_commandes(g, r);
            None
        }
    };
    if let Some(t) = t {
        let _ = draw_tab_page(g, t, r, alerts, tiles, time);
    }
}

fn draw_update_banner(st: &updater::State, sw: f32) -> bool {
    let (text, col, clickable) = match st {
        updater::State::Available { version, notes, .. } => {
            let first = notes.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim_start_matches(['-', '*', ' ']);
            (format!("MISE À JOUR v{version} DISPONIBLE — CLIQUER POUR INSTALLER   {first}"), Color::new(0.1, 0.45, 0.15, 0.95), true)
        }
        updater::State::Downloading { version } => (format!("TÉLÉCHARGEMENT DE LA v{version}..."), Color::new(0.1, 0.3, 0.5, 0.95), false),
        _ => return false,
    };
    let w = (ui::txt_w(&text, 16.0) + 40.0).min(sw - 20.0);
    let r = Rect::new((sw - w) / 2.0, 4.0, w, 26.0);
    draw_rectangle(r.x, r.y, r.w, r.h, col);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, WHITE);
    ui::txt_c(&text, sw / 2.0, r.y + 18.0, 16.0, WHITE);
    clickable && is_mouse_button_pressed(MouseButton::Left) && r.contains(mouse_position().into())
}

#[cfg(test)]
mod icon {
    use macroquad::prelude::*;

    /// Génère l'icône de l'application (silhouette Darkstar) : cargo test --release make_icon -- --ignored
    #[test]
    #[ignore]
    fn make_icon() {
        let n = 1024u32;
        let mut img = Image::gen_image_color(n as u16, n as u16, Color::new(0.0, 0.0, 0.0, 0.0));
        let outline: [(f32, f32); 21] = [
            (0.0, 0.0), (0.035, 0.10), (0.075, 0.24), (0.12, 0.40), (0.20, 0.55), (0.42, 0.70), (0.66, 0.84),
            (0.68, 0.90), (0.42, 0.91), (0.26, 0.96), (0.10, 0.99), (-0.10, 0.99), (-0.26, 0.96), (-0.42, 0.91),
            (-0.68, 0.90), (-0.66, 0.84), (-0.42, 0.70), (-0.20, 0.55), (-0.12, 0.40), (-0.075, 0.24), (-0.035, 0.10),
        ];
        let inside = |x: f32, y: f32| {
            let mut c = false;
            for i in 0..outline.len() {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                    c = !c;
                }
            }
            c
        };
        let rad = n as f32 * 0.22;
        for py in 0..n {
            for px in 0..n {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                // Carré arrondi (gabarit macOS), dégradé ciel ardoise -> sol vert
                let m = n as f32 * 0.09;
                let (dx, dy) = ((fx - n as f32 / 2.0).abs() - (n as f32 / 2.0 - m - rad), (fy - n as f32 / 2.0).abs() - (n as f32 / 2.0 - m - rad));
                let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
                if d > rad {
                    continue;
                }
                let t = fy / n as f32;
                let mut c = if t < 0.66 {
                    Color::new(0.22 + 0.2 * t, 0.27 + 0.25 * t, 0.42 + 0.25 * t, 1.0)
                } else {
                    Color::new(0.16, 0.42 + (t - 0.66) * 0.3, 0.12, 1.0)
                };
                if (t - 0.66).abs() < 0.004 {
                    c = WHITE;
                }
                // Silhouette (nez en haut), centrée
                let sx = (fx - n as f32 / 2.0) / (n as f32 * 0.40);
                let sy = (fy - n as f32 * 0.14) / (n as f32 * 0.74);
                if (0.0..=1.0).contains(&sy) && inside(sx, sy) {
                    c = Color::new(0.06, 0.07, 0.08, 1.0);
                    if !inside(sx * 1.035, (sy - 0.5) * 1.02 + 0.5) {
                        c = Color::new(0.55, 0.95, 0.6, 1.0);
                    }
                }
                img.set_pixel(px, n - 1 - py, c); // export_png inverse l'axe vertical
            }
        }
        img.export_png(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png"));
    }
}

/// Rapport d'analyse du crash, affiché sur la vue synthétique.
fn draw_crash_report(report: &[String], r: Rect) {
    let x = r.x + 16.0;
    let w = r.w - 32.0;
    let mut lines: Vec<(String, Color)> = Vec::new();
    for l in report.iter().take(9) {
        let c = if l.starts_with("CAUSE") {
            RED
        } else if l.starts_with("DIAGNOSTIC") {
            ui::AMBER
        } else if l.starts_with("Trace") {
            ui::CYAN_S
        } else {
            WHITE
        };
        for wl in ui::wrap_text(l, 13.0, w - 16.0) {
            lines.push((wl, c));
        }
    }
    let h = 30.0 + lines.len() as f32 * 15.0;
    let y = (r.y + r.h - h - 34.0).max(r.y + 110.0);
    ui::rrect(Rect::new(x, y, w, h), 8.0, Color::new(0.0, 0.0, 0.0, 0.85));
    ui::rrect_lines(Rect::new(x, y, w, h), 8.0, 1.0, RED);
    ui::txt("ANALYSE ENREGISTREUR DE VOL", x + 10.0, y + 18.0, 14.0, RED);
    for (i, (l, c)) in lines.iter().enumerate() {
        ui::txt(l, x + 10.0, y + 36.0 + i as f32 * 15.0, 13.0, *c);
    }
}

/// Infos éphémères sur l'horizon : affichées ~3.5 s puis estompées.
fn draw_flashes(flashes: &mut Vec<(String, Color, f64)>, r: Rect) {
    let now = get_time();
    flashes.retain(|(_, _, t)| *t > now);
    let mut y = r.y + 64.0;
    for (s, c, t) in flashes.iter() {
        let a = ((t - now) as f32).min(1.0);
        // Catégorie selon la couleur du message (rouge : alarme, ambre : panne, sinon info)
        let label = if c.r > 0.8 && c.g < 0.5 { "ALARME" } else if c.r > 0.8 && c.g > 0.5 && c.b < 0.4 { "PANNE" } else { "INFO" };
        ui::tag(r.x + r.w / 2.0, y, label, s, Color::new(c.r, c.g, c.b, a), 16.0, false, true);
        y += 30.0;
    }
}

/// Débriefing de fin de vol (instruction FIN).
fn draw_debrief(lines: &[String], r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.6));
    let w = 760.0f32.min(r.w - 40.0);
    let h = (90.0 + lines.len() as f32 * 22.0).min(r.h - 40.0);
    let b = Rect::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h);
    ui::rrect(b, 12.0, Color::new(0.03, 0.04, 0.06, 0.97));
    ui::rrect_lines(b, 12.0, 1.5, ui::ACCENT);
    ui::txt(&format!("FIN DE VOL - DEBRIEFING DARKSTAR {}", link::tail()), b.x + 22.0, b.y + 34.0, 20.0, ui::ACCENT);
    let mut y = b.y + 66.0;
    for l in lines {
        let c = if l.starts_with("ISSUE : PERTE") { RED } else if l.starts_with("  ") { ui::DIM } else if l.starts_with("TRACE") { ui::CYAN_S } else { WHITE };
        for wl in ui::wrap_text(l, 15.0, w - 44.0) {
            if y > b.y + b.h - 34.0 {
                break;
            }
            ui::txt(&wl, b.x + 22.0, y, 15.0, c);
            y += 22.0;
        }
    }
    ui::txt_c("ENTREE : NOUVEAU VOL   -   instruction QUITTER pour fermer", b.x + w / 2.0, b.y + b.h - 14.0, 14.0, ui::DIM);
}
