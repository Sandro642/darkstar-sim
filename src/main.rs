//! DARKSTAR — simulateur d'essais en vol hypersoniques télépilotés.

mod aircraft;
mod alarms;
mod airports;
mod atmosphere;
mod elec;
mod audio;
mod autoflight;
mod fcs;
mod gcs;
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
    /// Fin de vol (instruction FIN) : simulation figée, débriefing affiché.
    ended: bool,
    debrief: Vec<String>,
    crash_report: Vec<String>,
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
    cmd_hist: Vec<String>,
    hist_pos: usize,
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
            log: vec![(0.0, Level::Info, "GCS EN LIGNE — LIAISON LOS ÉTABLIE AVEC DARKSTAR FTV-01".into())],
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
            ended: false,
            debrief: Vec::new(),
            crash_report: Vec::new(),
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
        self.flash_from_message(l, &s);
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
            f.push(format!("CHAMBRES DÉGRADÉES {:.0}%", ac.eng.health * 100.0));
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

    fn apply_menu(&mut self, a: menu::MenuAction) {
        use menu::MenuAction::*;
        match a {
            EngSwitch(i, sw) => self.ac.cmd_eng_switch(i, sw),
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
        }
        self.cues.push(audio::Cue::Ack);
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
        self.cues.push(audio::Cue::Ack);
    }

    fn link(&self) -> (&'static str, f64) {
        let base = &AIRPORTS[apt_index("KEDW")];
        let (d, _) = dist_brg(self.ac.lat, self.ac.lon, base.lat, base.lon);
        if d < 350_000.0 {
            ("LOS", 45.0)
        } else {
            ("SATCOM", 380.0)
        }
    }

    fn alerts(&self) -> Vec<(Level, String)> {
        let ac = &self.ac;
        let d = &ac.d;
        let mut a = Vec::new();
        if ac.crashed.is_some() {
            return a;
        }
        let agl_ft = d.agl / FT;
        let fpm = d.vs / FT * 60.0;
        if !ac.on_ground && agl_ft < 2500.0 && -fpm > agl_ft * 1.4 + 800.0 {
            a.push((Level::Warning, "PULL UP".to_string()));
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
        if ac.fuel_dump {
            a.push((Level::Caution, "FUEL DUMP".to_string()));
        }
        a.extend(self.thermal.alerts());
        a.extend(self.recon.alerts());
        a
    }

    fn input(&mut self, real_dt: f64) {
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

        if is_key_pressed(KeyCode::T) || is_key_pressed(KeyCode::Key1) {
            ac.cmd_turbine();
        }
        if is_key_pressed(KeyCode::I) {
            ac.cmd_inlet();
        }
        if is_key_pressed(KeyCode::Y) || is_key_pressed(KeyCode::Key2) {
            ac.cmd_ram();
        }
        if is_key_pressed(KeyCode::U) || is_key_pressed(KeyCode::Key3) {
            ac.cmd_scram();
        }
        if is_key_pressed(KeyCode::G) {
            ac.cmd_gear();
        }
        if is_key_pressed(KeyCode::B) {
            ac.brakes = !ac.brakes;
        }
        if is_key_pressed(KeyCode::S) {
            ac.speedbrake = !ac.speedbrake;
        }
        if is_key_pressed(KeyCode::P) {
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
        if is_key_pressed(KeyCode::O) {
            ac.ap.at_on = !ac.ap.at_on;
            if ac.ap.at_on {
                ac.ap.mach_tgt = (ac.d.mach * 10.0).round() / 10.0;
                msgs.push((Level::Info, format!("A/THR ENGAGÉE — MACH {:.1}", ac.ap.mach_tgt)));
            } else {
                msgs.push((Level::Caution, "A/THR OFF".into()));
            }
        }
        if is_key_pressed(KeyCode::N) && !ac.on_ground {
            if !ac.ap.on {
                ac.ap.on = true;
                ac.ap.vert = VertMode::Alt;
                ac.ap.alt_tgt_ft = (ac.alt / FT / 100.0).round() * 100.0;
            }
            ac.ap.lat = LatMode::Nav;
            msgs.push((Level::Info, format!("NAV — DIRECT {}", AIRPORTS[self.dest].icao)));
        }
        if is_key_pressed(KeyCode::L) && !ac.on_ground {
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
        if is_key_pressed(KeyCode::X) {
            if self.refuel.active() {
                self.refuel.stop(ac, "REPRISE EN MAIN");
            }
            ac.ap.on = false;
            ac.ap.at_on = false;
            ac.autoland = None;
            ac.autotakeoff = None;
            msgs.push((Level::Caution, "REPRISE EN MAIN MANUELLE — AP / A/THR / AUTOLAND OFF".into()));
        }
        if is_key_pressed(KeyCode::E) {
            self.warp_idx = (self.warp_idx + 1) % WARPS.len();
        }
        if is_key_pressed(KeyCode::C) {
            self.map.follow = true;
        }
        let page_keys = [KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9, KeyCode::Key0];
        for (k, (tab, _)) in page_keys.iter().zip(gcs::TABS.iter()) {
            let (k, tab) = (*k, *tab);
            if is_key_pressed(k) {
                self.tab = tab;
            }
        }
        if is_key_pressed(KeyCode::J) {
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
        self.link_buf.push_back((now, self.stick.0, self.stick.1));
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
            let a = apt.approach_for(self.ac.lat, self.ac.lon);
            Some((self.dest, a.thr_lat, a.thr_lon, a.hdg, apt.elev))
        } else {
            None
        };
        if self.ac.ap.lat == LatMode::Nav {
            self.ac.ap.hdg_tgt = brg;
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
        // Ravitaillement en vol : ravitailleur, rendez-vous, maintien de position sous la perche
        self.refuel.update(sim_dt, &mut self.ac);
        let rfe: Vec<_> = self.refuel.events.drain(..).collect();
        for (l, m) in rfe {
            self.push(l, m);
        }

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
        // Aide à la rotation au décollage manuel : rappel du sens du manche (flèche BAS = tirer)
        if self.ac.on_ground && !self.ac.ap.on && self.ac.autotakeoff.is_none() && self.ac.d.keas > 175.0 && self.ac.theta < 2f64.to_radians() && self.ac.crashed.is_none() {
            self.flash("ROTATION : FLECHE BAS (TIRER)", ui::AMBER);
        }

        // Enregistreur de vol : trace continue, analyse et sauvegarde définitive au crash
        let alerts = self.alerts();
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
            "HUD" | "INFOS" => {
                pfd_plus::hud_show_all(10.0);
                reply = Ok("INFOS HUD AFFICHEES 10 s (survol de l'horizon : toutes les infos)".into());
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
                        reply = Ok(format!("METEO {} — VENT {:03.0}/{:.0} KT (CALME|BEAU|NUAGEUX|COUVERT|ORAGEUX)", weather::SEVERITIES[self.wx.severity], w.wind_dir, w.wind_kt));
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
                    "METEO CALME|BEAU|NUAGEUX|COUVERT|ORAGEUX  HUD (infos ephemeres 10 s, ou survol)",
                    "TANKER [OACI|OFF]  RAVITAILLEMENT (autonome)  DECROCHER",
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
        } else if !self.atc[5] && self.link().0 == "SATCOM" {
            self.atc[5] = true;
            say = Some("GCS : HORS PORTÉE LOS — BASCULE SUR LIAISON SATCOM, LATENCE ACCRUE.");
        }
        if let Some(s) = say {
            self.push(Level::Info, s);
        }
    }
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
    let upd = updater::Updater::new();
    upd.check();
    let mut last_check = get_time();
    let auto_update = std::env::var("DARKSTAR_AUTOUPDATE").is_ok();
    let mut last_upd_msg = String::new();
    let mut g = Game::new(&scenario);
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
    let mut show_help = scenario.is_empty();
    let mut map_full = false;
    let mut frame = 0u64;

    loop {
        let real_dt = (get_frame_time() as f64).min(0.05);
        frame += 1;

        if is_key_pressed(KeyCode::Tab) {
            show_help = !show_help;
        }
        if is_key_pressed(KeyCode::V) && g.console.is_none() {
            map_full = !map_full;
        }
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        if enter && (g.ac.crashed.is_some() || g.ended || (shift && g.console.is_none())) {
            let muted = g.muted;
            g.fdr.finalize();
            g = Game::new(&scenario);
            g.muted = muted;
            sfx.reset();
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
        } else if !g.ended {
            while get_char_pressed().is_some() {}
            g.input(real_dt);
        }
        if !g.ended {
            g.update(real_dt);
        }

        let alerts = g.alerts();
        let unacked = |lv: Level| alerts.iter().any(|(l, s)| *l == lv && !g.acked.contains(s));
        let master_warn = unacked(Level::Warning) || g.ac.crashed.is_some();
        let master_caut = unacked(Level::Caution);
        sfx.muted = g.muted;
        sfx.update(&g.ac, &alerts, &g.acked);
        for c in g.cues.drain(..) {
            sfx.cue(c);
        }

        // --- Mise en page ---
        tiles.begin_frame();
        clear_background(ui::BG);
        let (sw, sh) = (screen_width(), screen_height());
        let time = get_time();
        let (warp_eff, warp_capped) = g.effective_warp();
        let (ln, lat) = g.link();
        let bar = Rect::new(0.0, 0.0, sw, 34.0);
        let body = Rect::new(6.0, 40.0, sw - 12.0, sh - 46.0);
        if map_full {
            g.map.input(body);
            g.map.draw(&world, &mut tiles, &g.wx, &g.refuel, &g.recon, &g.ac, body, g.dest, time);
        } else {
            // Écran principal Darkstar, aux proportions de l'écran du cockpit
            let mfd_w = (body.w * 0.63).min((body.h - 120.0) * mfd::COCKPIT_ASPECT).floor();
            let mfd_h = (mfd_w / mfd::COCKPIT_ASPECT).floor();
            let mfd_r = Rect::new(body.x, body.y, mfd_w, mfd_h);
            const BEZEL_PAGES: [gcs::Tab; 6] = [gcs::Tab::Moteurs, gcs::Tab::Alarmes, gcs::Tab::Fcs, gcs::Tab::Limites, gcs::Tab::Elec, gcs::Tab::Thermique];
            let lit: [bool; 6] = std::array::from_fn(|i| g.tab == BEZEL_PAGES[i]);
            let cue = {
                let t = &g.recon.targets[g.recon.selected];
                (!t.done()).then(|| pfd_plus::Cue { lat: t.lat, lon: t.lon, elev: 0.0, label: format!("CIBLE {}", t.code), color: Color::new(1.0, 0.85, 0.2, 1.0) })
            };
            let info = pfd::PfdInfo { alerts: &alerts, link_name: ln, latency_ms: lat, dest: g.dest, warp: warp_eff, time, cue, wx: g.wx.info.clone(), threats: g.recon.threat_cues(&g.ac) };
            let (_, act) = mfd::draw_mfd(&world, &mut tiles, &g.wx, &g.refuel, &g.ac, &g.recon, mfd_r, &mut g.mfd, &info, g.style, time, lit);
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
            let svs_r = mfd::layout(mfd_r).1.svs;
            draw_console(&g, svs_r);
            if g.ac.crashed.is_some() && !g.crash_report.is_empty() {
                draw_crash_report(&g.crash_report, svs_r);
            }
            draw_flashes(&mut g.flashes, svs_r);
            // Bouton MENU du PFD : ouvre l'onglet CONSOLE
            let menu_btn = Rect::new(svs_r.x + 6.0, svs_r.y + svs_r.h + 7.0, 70.0, 18.0);
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
            g.map.draw(&world, &mut tiles, &g.wx, &g.refuel, &g.recon, &g.ac, mr, g.dest, time);
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
            match g.tab {
                gcs::Tab::Moteurs => panels::draw_engines(&g.ac, cr, time),
                gcs::Tab::Alarmes => match alarms::draw_alarms(&g.ac, &alerts, cr, time) {
                    Some(alarms::AlarmAction::Toggle(f)) => {
                        let on = !g.ac.fails.has(f);
                        g.ac.set_failure(f, on);
                        g.cues.push(audio::Cue::Ack);
                    }
                    Some(alarms::AlarmAction::RepairAll) => g.exec_command("PANNES OFF"),
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
                    if let Some(sw) = gcs::draw_overrides(&g.ac, cr) {
                        g.apply_switch(sw);
                    }
                }
                gcs::Tab::Fcs => fcs::draw_fcs(&g.ac, cr),
                gcs::Tab::Recon => {
                    let ln = g.link().0;
                    if let Some(a) = recon::draw_recon(&g.ac, &g.recon, &mut tiles, cr, ln) {
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
        }
        let top = gcs::draw_topbar(
            bar,
            &gcs::TopBar {
                sim_time: g.sim_time,
                mission_t: g.mission.elapsed,
                warp: warp_eff,
                warp_capped,
                link: ln,
                latency_ms: lat,
                master_warn,
                master_caut,
                muted: g.muted,
                time,
            },
        );
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

        // --- Mise à jour ---
        if get_time() - last_check > 1800.0 {
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
                (Some(al), _) => format!("AUTOLAND {} {:02} — {:?}", AIRPORTS[al.apt].icao, al.runway_number(), al.phase).to_uppercase(),
                (_, Some(_)) => "DÉCOLLAGE AUTOMATIQUE EN COURS".to_string(),
                _ => "ENTRÉE : saisir une instruction (CAP, ALT, MACH, DIRECT, ATTERRIR...)".to_string(),
            };
            let active = g.ac.autoland.is_some() || g.ac.autotakeoff.is_some();
            let c = if active { ui::GREEN_S } else { Color::new(0.75, 0.8, 0.85, 0.55) };
            // Rappel discret en bas de la vue (hors échelle de tangage), sauf pendant une approche ILS
            if active || g.ac.approach.is_none() {
                ui::txt_c(&s, r.x + r.w / 2.0, r.y + r.h - 10.0, if active { 15.0 } else { 12.0 }, c);
            }
        }
    }
}

/// Bandeau de mise à jour en haut de l'écran ; renvoie vrai si l'utilisateur a cliqué dessus.
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
        let w = ui::txt_w(s, 18.0) + 24.0;
        let x = r.x + (r.w - w) / 2.0;
        ui::rrect(Rect::new(x, y - 18.0, w, 26.0), 6.0, Color::new(0.0, 0.0, 0.0, 0.55 * a));
        ui::rrect_lines(Rect::new(x, y - 18.0, w, 26.0), 6.0, 1.0, Color::new(c.r, c.g, c.b, 0.6 * a));
        ui::txt_c(s, r.x + r.w / 2.0, y, 18.0, Color::new(c.r, c.g, c.b, a));
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
    ui::txt("FIN DE VOL - DEBRIEFING DARKSTAR FTV-01", b.x + 22.0, b.y + 34.0, 20.0, ui::ACCENT);
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
