//! Enregistreur de vol (FDR, « boîte noire ») : trace complète du vol sur disque et analyse
//! automatique après un crash.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;
use std::fmt::Write as _;
use std::path::PathBuf;

/// Un échantillon de la trace (2 Hz).
#[derive(Clone)]
pub struct Sample {
    pub t: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub agl_ft: f64,
    pub hdg: f64,
    pub keas: f64,
    pub mach: f64,
    pub vs_fpm: f64,
    pub gamma: f64,
    pub pitch: f64,
    pub bank: f64,
    pub alpha: f64,
    pub nz: f64,
    pub q_kpa: f64,
    pub thrust_kn: f64,
    pub drag_kn: f64,
    pub throttle: f64,
    pub fuel_kg: f64,
    pub h2_kg: f64,
    pub skin_c: f64,
    pub gear: f64,
    pub speedbrake: bool,
    pub ap: String,
    pub engines: String,
    pub alerts: String,
}

pub struct Recorder {
    pub samples: Vec<Sample>,
    pub events: Vec<(f64, String)>,
    next_t: f64,
    autosave_t: f64,
    pub started: String,
    pub saved_path: Option<PathBuf>,
    pub report: Vec<String>,
    finalized: bool,
}

/// Date et heure locales approximatives (UTC) sans dépendance externe : « 2026-10-31_14-05-09 ».
fn timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Conversion jours → date civile (algorithme de H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}", rem / 3600, (rem / 60) % 60, rem % 60)
}

/// Dossier des enregistrements : ~/Library/Application Support/Darkstar/vols
pub fn flights_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Library/Application Support/Darkstar/vols")
}

fn ap_string(ac: &Aircraft) -> String {
    let mut s = String::new();
    if ac.ap.on {
        let lat = match ac.ap.lat {
            LatMode::Hdg => format!("HDG{:03.0}", ac.ap.hdg_tgt),
            LatMode::Nav => "NAV".into(),
            LatMode::Loc => "LOC".into(),
        };
        let vert = match ac.ap.vert {
            VertMode::Alt => format!("ALT{:.0}", ac.ap.alt_tgt_ft),
            VertMode::Gs => "GS".into(),
            VertMode::Flare => "FLARE".into(),
            VertMode::Pitch => format!("PITCH{:+.0}", ac.ap.pitch_tgt),
            VertMode::QHold => format!("DESC_q{:.0}", ac.ap.q_tgt / 1000.0),
        };
        let _ = write!(s, "AP {lat} {vert}");
    } else {
        s.push_str("AP_OFF");
    }
    if ac.ap.at_on {
        match ac.ap.spd_keas {
            Some(k) => {
                let _ = write!(s, " ATHR{}{k:.0}", if ac.ap.spd_managed { "_AUTO" } else { "_" });
            }
            None => {
                let _ = write!(s, " ATHR_M{:.2}", ac.ap.mach_tgt);
            }
        }
    }
    if let Some(al) = &ac.autoland {
        let _ = write!(s, " AUTOLAND_{:?}", al.phase);
    }
    if ac.ovr_engine {
        s.push_str(" ENG_OVRD");
    }
    if ac.ovr_fbw {
        s.push_str(" FBW_OVRD");
    }
    s
}

fn engines_string(ac: &Aircraft) -> String {
    let e = &ac.eng;
    format!("L:{:?} R:{:?} RAM:{:?} SCRAM:{:?} INLET:{}", e.side[0], e.side[1], e.ram, e.scram, if e.inlet_open { "OPEN" } else { "CLOSED" })
}

impl Recorder {
    pub fn new() -> Self {
        Recorder {
            samples: Vec::new(),
            events: Vec::new(),
            next_t: 0.0,
            autosave_t: 30.0,
            started: timestamp(),
            saved_path: None,
            report: Vec::new(),
            finalized: false,
        }
    }

    pub fn event(&mut self, t: f64, msg: &str) {
        self.events.push((t, msg.to_string()));
    }

    fn sample_now(ac: &Aircraft, t: f64, alerts: &[(Level, String)]) -> Sample {
        let d = &ac.d;
        Sample {
            t,
            lat: ac.lat,
            lon: ac.lon,
            alt_ft: ac.alt / FT,
            agl_ft: d.agl / FT,
            hdg: ac.hdg_deg(),
            keas: d.keas,
            mach: d.mach,
            vs_fpm: d.vs / FT * 60.0,
            gamma: ac.gamma.to_degrees(),
            pitch: ac.theta.to_degrees(),
            bank: ac.phi.to_degrees(),
            alpha: d.alpha.to_degrees(),
            nz: d.nz,
            q_kpa: d.q / 1000.0,
            thrust_kn: d.thrust / 1000.0,
            drag_kn: d.drag / 1000.0,
            throttle: ac.throttle * 100.0,
            fuel_kg: ac.fuel,
            h2_kg: ac.h2,
            skin_c: ac.skin_temp - 273.15,
            gear: ac.gear_pos,
            speedbrake: ac.speedbrake,
            ap: ap_string(ac),
            engines: engines_string(ac),
            alerts: alerts.iter().map(|(_, a)| a.replace(',', " ")).collect::<Vec<_>>().join("|"),
        }
    }

    /// Enregistre un échantillon (2 Hz) ; sauvegarde automatique toutes les 30 s.
    pub fn update(&mut self, ac: &Aircraft, t: f64, alerts: &[(Level, String)]) {
        if self.finalized {
            return;
        }
        if t >= self.next_t {
            self.next_t = t + 0.5;
            self.samples.push(Self::sample_now(ac, t, alerts));
        }
        if t >= self.autosave_t {
            self.autosave_t = t + 30.0;
            let _ = self.save(None);
        }
    }

    /// Écrit la trace CSV et le journal/rapport. Renvoie le chemin du fichier CSV.
    pub fn save(&mut self, crash: Option<&str>) -> std::io::Result<PathBuf> {
        let dir = flights_dir();
        std::fs::create_dir_all(&dir)?;
        let tag = if crash.is_some() { "CRASH" } else { "vol" };
        let base = dir.join(format!("{}_{}", self.started, tag));
        // Si le vol a été sauvegardé sous un autre nom (vol → CRASH), on retire l'ancien fichier
        if let Some(old) = &self.saved_path {
            if old != &base.with_extension("csv") {
                let _ = std::fs::remove_file(old);
                let _ = std::fs::remove_file(old.with_extension("txt"));
            }
        }
        let mut csv = String::from(
            "t_s,lat,lon,alt_ft,agl_ft,hdg,keas,mach,vs_fpm,gamma_deg,pitch_deg,bank_deg,alpha_deg,nz,q_kpa,thrust_kn,drag_kn,throttle_pct,fuel_kg,h2_kg,skin_c,gear,speedbrake,ap,engines,alerts\n",
        );
        for s in &self.samples {
            let _ = writeln!(
                csv,
                "{:.1},{:.6},{:.6},{:.0},{:.0},{:.1},{:.1},{:.3},{:.0},{:.2},{:.2},{:.1},{:.2},{:.2},{:.1},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0},{:.2},{},{},{},{}",
                s.t, s.lat, s.lon, s.alt_ft, s.agl_ft, s.hdg, s.keas, s.mach, s.vs_fpm, s.gamma, s.pitch, s.bank, s.alpha, s.nz, s.q_kpa,
                s.thrust_kn, s.drag_kn, s.throttle, s.fuel_kg, s.h2_kg, s.skin_c, s.gear, s.speedbrake as u8, s.ap, s.engines, s.alerts
            );
        }
        let csv_path = base.with_extension("csv");
        std::fs::write(&csv_path, csv)?;
        let mut txt = format!("DARKSTAR v{} — enregistrement de vol {}\n\n", crate::updater::VERSION, self.started);
        if !self.report.is_empty() {
            txt.push_str("=== ANALYSE ===\n");
            for l in &self.report {
                txt.push_str(l);
                txt.push('\n');
            }
            txt.push('\n');
        }
        txt.push_str("=== JOURNAL ===\n");
        for (t, m) in &self.events {
            let _ = writeln!(txt, "{} {m}", crate::mission::fmt_time(*t));
        }
        std::fs::write(base.with_extension("txt"), txt)?;
        self.saved_path = Some(csv_path.clone());
        Ok(csv_path)
    }

    /// Fin d'enregistrement après un crash : analyse puis sauvegarde définitive.
    pub fn finalize_crash(&mut self, ac: &Aircraft, t: f64, alerts: &[(Level, String)]) -> Vec<String> {
        if self.finalized {
            return self.report.clone();
        }
        self.samples.push(Self::sample_now(ac, t, alerts));
        let reason = ac.crashed.clone().unwrap_or_default();
        self.report = analyze(&self.samples, &self.events, &reason);
        self.finalized = true;
        match self.save(Some(&reason)) {
            Ok(p) => self.report.push(format!("Trace enregistrée : {}", p.display())),
            Err(e) => self.report.push(format!("Échec d'enregistrement : {e}")),
        }
        self.report.clone()
    }

    /// Sauvegarde finale d'un vol sans crash (nouvel essai, atterrissage).
    pub fn finalize(&mut self) {
        if !self.finalized && self.samples.len() > 4 {
            self.finalized = true;
            let _ = self.save(None);
        }
    }
}

fn nearest_runway(lat: f64, lon: f64) -> (String, f64, f64) {
    let mut best = (String::new(), f64::MAX, 0.0);
    for a in AIRPORTS {
        let (d, _) = dist_brg(lat, lon, a.lat, a.lon);
        if d < best.1 {
            let (along, cross) = a.to_runway(lat, lon);
            let past_end = (along.abs() - a.len / 2.0).max(0.0);
            best = (a.icao.to_string(), d, cross.abs().max(past_end));
        }
    }
    best
}

/// Analyse automatique : cause probable, état à l'impact, chronologie et recommandations.
pub fn analyze(s: &[Sample], events: &[(f64, String)], reason: &str) -> Vec<String> {
    let mut r = Vec::new();
    let Some(last) = s.last() else { return r };
    let t0 = last.t;
    let win: Vec<&Sample> = s.iter().filter(|x| x.t >= t0 - 60.0).collect();
    let maxf = |f: &dyn Fn(&Sample) -> f64| win.iter().map(|x| f(x)).fold(f64::MIN, f64::max);
    let minf = |f: &dyn Fn(&Sample) -> f64| win.iter().map(|x| f(x)).fold(f64::MAX, f64::min);

    r.push(format!("CAUSE : {reason}"));
    r.push(format!(
        "ÉTAT AU CRASH T+{} : alt {:.0} ft (hauteur {:.0} ft), {:.0} KEAS / M{:.2}, vario {:.0} ft/min, pente {:+.1}°, assiette {:+.1}°, inclinaison {:+.0}°",
        crate::mission::fmt_time(t0), last.alt_ft, last.agl_ft, last.keas, last.mach, last.vs_fpm, last.gamma, last.pitch, last.bank
    ));
    r.push(format!(
        "  incidence {:+.1}°, {:+.2} g, q {:.0} kPa, poussée {:.0} kN / traînée {:.0} kN, manette {:.0} %, train {:.0} %",
        last.alpha, last.nz, last.q_kpa, last.thrust_kn, last.drag_kn, last.throttle, last.gear * 100.0
    ));
    r.push(format!("  {} | {}", last.ap, last.engines));
    r.push(format!(
        "60 DERNIÈRES S : q max {:.0} kPa, g {:+.1}/{:+.1}, incidence max {:+.1}°, vario min {:.0} ft/min, peau max {:.0} °C",
        maxf(&|x| x.q_kpa), minf(&|x| x.nz), maxf(&|x| x.nz), maxf(&|x| x.alpha), minf(&|x| x.vs_fpm), maxf(&|x| x.skin_c)
    ));

    // Diagnostic selon la cause
    let first_over = |pred: &dyn Fn(&Sample) -> bool| s.iter().rev().take_while(|x| pred(x)).last().map(|x| x.t);
    let up = reason.to_uppercase();
    let mut diag = Vec::new();
    if up.contains("PRESSION DYNAMIQUE") {
        if let Some(t) = first_over(&|x| x.q_kpa > 90.0) {
            let x = s.iter().find(|x| x.t >= t).unwrap();
            diag.push(format!(
                "q a dépassé 90 kPa {:.0} s avant la rupture, à M{:.2} / {:.0} ft avec une pente de {:+.1}° : descente trop raide pour la vitesse.",
                t0 - t, x.mach, x.alt_ft, x.gamma
            ));
        }
        if last.ap.contains("FBW_OVRD") {
            diag.push("FBW OVERRIDE actif : la protection haute vitesse (limitation de piqué, cabré auto) était inhibée.".into());
        }
        diag.push("Recommandation : décélérer en altitude ou utiliser l'instruction DESCENTE (q constant), pas de piqué en hypersonique.".into());
    } else if up.contains("FACTEUR DE CHARGE") {
        diag.push(format!("Facteur de charge hors limites ({:+.1} / {:+.1} g).", minf(&|x| x.nz), maxf(&|x| x.nz)));
        if last.ap.contains("FBW_OVRD") {
            diag.push("FBW OVERRIDE actif : limitation de g inhibée.".into());
        }
    } else if up.contains("THERMIQUE") {
        diag.push(format!("Peau à {:.0} °C (rupture à 1400 °C) à M{:.2} / {:.0} ft : trop vite pour l'altitude.", last.skin_c, last.mach, last.alt_ft));
        if last.ap.contains("ENG_OVRD") {
            diag.push("ENGINE OVERRIDE actif : la régulation thermique du FADEC était inhibée.".into());
        }
        diag.push("Recommandation : monter (q plus faible) ou réduire, surveiller la page THERMIQUE.".into());
    } else if up.contains("TRAIN RENTRÉ") {
        diag.push("Toucher avec le train rentré : vérifier les trois vertes (bloc TRAIN) avant 500 ft.".into());
    } else if up.contains("PNEUS") {
        diag.push(format!("Toucher à {:.0} KEAS (limite pneus 260) : vitesse d'approche trop élevée.", last.keas));
    } else if up.contains("SAILLAGE") {
        diag.push(format!("Inclinaison de {:+.0}° au toucher (limite 12°).", last.bank));
    } else if up.contains("COLLISION") {
        diag.push("Collision avec un trafic : surveiller les alertes TCAS (TA puis RA) et suivre l'ordre CLIMB/DESCEND.".into());
    } else if up.contains("SORTIE DE PISTE") {
        diag.push(format!("Sortie de piste à {:.0} KEAS : axe non tenu ou freinage insuffisant.", last.keas));
    } else if up.contains("IMPACT") {
        let (icao, d, off) = nearest_runway(last.lat, last.lon);
        diag.push(format!("Impact à {:.1} NM de {icao}, à {:.0} m hors de l'emprise de piste.", d / NM, off));
        if last.vs_fpm < -1500.0 {
            diag.push(format!("Vario {:.0} ft/min à l'impact : descente non contrôlée.", last.vs_fpm));
        }
        if last.thrust_kn < 1.0 && last.drag_kn > 1.0 {
            diag.push("Aucune poussée à l'impact (moteurs éteints ou carburant épuisé).".into());
        }
        if last.fuel_kg < 1.0 {
            diag.push("Carburant JP-7 épuisé.".into());
        }
        if last.ap.contains("AP_OFF") {
            diag.push("Pilote automatique déconnecté.".into());
        } else {
            diag.push(format!("Pilote automatique engagé ({}) : vérifier la consigne d'altitude / le mode.", last.ap));
        }
        let pull_up = events.iter().filter(|(t, m)| *t >= t0 - 60.0 && m.contains("PULL UP")).count();
        if pull_up > 0 || win.iter().any(|x| x.alerts.contains("PULL UP")) {
            diag.push("Alarme PULL UP active avant l'impact.".into());
        }
    }
    for l in diag {
        r.push(format!("DIAGNOSTIC : {l}"));
    }
    // Alarmes actives juste avant le crash
    if let Some(x) = win.iter().rev().find(|x| !x.alerts.is_empty()) {
        r.push(format!("ALARMES : {}", x.alerts.replace('|', ", ")));
    }
    // Chronologie : derniers événements
    let recent: Vec<&(f64, String)> = events.iter().filter(|(t, _)| *t >= t0 - 120.0).collect();
    for (t, m) in recent.iter().rev().take(8).rev() {
        r.push(format!("  {} {m}", crate::mission::fmt_time(*t)));
    }
    r
}
