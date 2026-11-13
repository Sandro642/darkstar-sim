//! Sons synthétisés au démarrage (aucun fichier) : boucles moteur mixées en volume
//! et alarmes avioniques (chime, GPWS, vibreur de manche, déconnexion AP...).

use crate::aircraft::*;
use macroquad::audio::*;

const SR: f32 = 22_050.0;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Filtre passe-bas du premier ordre.
struct Lp(f32, f32);
impl Lp {
    fn new(fc: f32) -> Self {
        Lp(0.0, 1.0 - (-std::f32::consts::TAU * fc / SR).exp())
    }
    fn run(&mut self, x: f32) -> f32 {
        self.0 += (x - self.0) * self.1;
        self.0
    }
}

fn sine(f: f32, t: f32) -> f32 {
    (std::f32::consts::TAU * f * t).sin()
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let n = samples.len() as u32;
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&(SR as u32 * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
    }
    b
}

/// Génère un son ; pour une boucle, la fin est fondue dans le début pour éviter le clic.
fn synth(secs: f32, looped: bool, mut f: impl FnMut(f32, &mut Rng) -> f32) -> Vec<u8> {
    let n = (secs * SR) as usize;
    let fade = if looped { (0.08 * SR) as usize } else { 0 };
    let mut rng = Rng(0x9E37_79B9);
    let mut s: Vec<f32> = (0..n + fade).map(|i| f(i as f32 / SR, &mut rng)).collect();
    if looped {
        for i in 0..fade {
            let k = i as f32 / fade as f32;
            s[i] = s[i] * k + s[n + i] * (1.0 - k);
        }
        s.truncate(n);
    } else {
        // Attaque/relâchement courts pour éviter les clics
        let r = (0.004 * SR) as usize;
        for i in 0..r.min(n) {
            let k = i as f32 / r as f32;
            s[i] *= k;
            s[n - 1 - i] *= k;
        }
    }
    let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let g = 0.9 / peak;
    for v in &mut s {
        *v *= g;
    }
    wav(&s)
}

pub struct Sfx {
    // Boucles moteur
    turb_lo: Sound,
    turb_hi: Sound,
    roar: Sound,
    ab: Sound,
    ram: Sound,
    scram: Sound,
    wind: Sound,
    /// Grondement de structure (buffeting, roulage, turbulence).
    rumble: Sound,
    // Boucles d'alarme
    warn: Sound,
    siren: Sound,
    shaker: Sound,
    clacker: Sound,
    horn: Sound,
    // Ponctuels
    caution: Sound,
    ap_disc: Sound,
    bang: Sound,
    thump: Sound,
    blip: Sound,
    buzz: Sound,
    click: Sound,
    key: Sound,
    switch: Sound,
    ap_on: Sound,
    athr_off: Sound,
    alt_alert: Sound,
    mins: Sound,
    seq: Sound,
    whoosh: Sound,
    link_lost: Sound,
    link_ok: Sound,
    fire_bell: Sound,
    gcas: Sound,
    gear_hum: Sound,
    config: Sound,
    modem: Sound,
    liftoff: Sound,
    runway: Sound,
    taxi: Sound,
    screech: Sound,
    seam: Sound,
    nose: Sound,
    brake: Sound,
    rwr_new: Sound,
    rwr_lock: Sound,
    rwr_launch: Sound,
    pub voice: Voice,
    pub muted: bool,
    prev: Prev,
}

#[derive(Default)]
struct Prev {
    cautions: usize,
    ap: bool,
    ground: bool,
    unstart: bool,
    crashed: bool,
    started: bool,
    at: bool,
    sb: bool,
    scram_lit: bool,
    ram_lit: bool,
    alt_err: f64,
    gcas: bool,
    fire: bool,
    osr: bool,
    warns: usize,
    over_g: bool,
    bingo: bool,
    fuel_low: bool,
    gear_cue: bool,
    threat: u8,
}

/// Événements sonores ponctuels émis par la logique de jeu.
pub enum Cue {
    Ack,
    Refused,
    /// Clic d'interface (bouton, onglet, touche de ligne)
    Click,
    /// Frappe clavier (terminal, MCDU)
    Key,
    /// Interrupteur de console (levier)
    Switch,
    /// Minimums (carillon deux tons)
    Minimums,
    /// Séquencement d'un point du plan de vol
    Seq,
    /// Liaison rétablie
    LinkOk,
}

async fn load(bytes: Vec<u8>) -> Sound {
    load_sound_from_bytes(&bytes).await.expect("son")
}

impl Sfx {
    pub async fn new() -> Sfx {
        // --- Moteur ---
        let turb_lo = load(synth(2.0, true, |t, r| {
            let am = 1.0 + 0.08 * sine(3.0, t);
            (sine(620.0, t) * 0.5 + sine(1240.0, t) * 0.25 + sine(2480.0, t) * 0.1) * am + r.next() * 0.12
        }))
        .await;
        let turb_hi = load(synth(2.0, true, |t, r| {
            let am = 1.0 + 0.05 * sine(5.0, t);
            (sine(1450.0, t) * 0.45 + sine(2900.0, t) * 0.25 + sine(4350.0, t) * 0.12 + sine(725.0, t) * 0.15) * am + r.next() * 0.15
        }))
        .await;
        let mut lp1 = Lp::new(380.0);
        let mut lp1b = Lp::new(60.0);
        let roar = load(synth(2.0, true, move |_, r| {
            let n = r.next();
            lp1.run(n) * 1.0 + lp1b.run(n) * 2.5
        }))
        .await;
        let mut lp2 = Lp::new(900.0);
        let mut lp2b = Lp::new(90.0);
        let ab = load(synth(2.0, true, move |_, r| {
            let n = r.next();
            let crackle = if r.next() > 0.995 { r.next() * 3.0 } else { 0.0 };
            lp2.run(n) * 0.9 + lp2b.run(n) * 2.0 + crackle
        }))
        .await;
        let mut lp3 = Lp::new(1400.0);
        let mut lp3b = Lp::new(250.0);
        let ram = load(synth(2.0, true, move |t, r| {
            let n = r.next();
            (lp3.run(n) - lp3b.run(n)) * 1.4 + sine(90.0, t) * 0.25 + sine(180.0, t) * 0.1
        }))
        .await;
        let mut lp4 = Lp::new(5000.0);
        let mut lp4b = Lp::new(1500.0);
        let mut lp4c = Lp::new(120.0);
        let scram = load(synth(2.0, true, move |t, r| {
            let n = r.next();
            (lp4.run(n) - lp4b.run(n)) * 1.2 + lp4c.run(n) * 2.0 + sine(70.0, t) * 0.3 + sine(140.0, t) * 0.12
        }))
        .await;
        let mut lp5 = Lp::new(1200.0);
        let mut lp5b = Lp::new(200.0);
        let wind = load(synth(3.0, true, move |t, r| {
            let n = r.next();
            (lp5.run(n) - lp5b.run(n) * 0.6) * (1.0 + 0.3 * sine(0.666_666_7, t))
        }))
        .await;

        let mut lp6 = Lp::new(110.0);
        let mut lp6b = Lp::new(25.0);
        let rumble = load(synth(2.0, true, move |t, r| {
            let n = r.next();
            (lp6.run(n) * 2.0 + lp6b.run(n) * 4.0) * (1.0 + 0.5 * sine(7.5, t)) + sine(32.0, t) * 0.2
        }))
        .await;

        // --- Alarmes en boucle ---
        // Carillon répétitif continu (MASTER WARNING)
        let warn = load(synth(1.0, true, |t, _| {
            let ph = t % 0.5;
            let env = (-ph * 9.0).exp();
            (sine(1050.0, t) * 0.6 + sine(2100.0, t) * 0.25) * env
        }))
        .await;
        // Sirène GPWS « PULL UP » : balayage montant
        let mut ph_s = 0.0f32;
        let siren = load(synth(1.0, true, move |t, _| {
            let ph = (t % 0.5) / 0.5;
            ph_s += (500.0 + 700.0 * ph) / SR;
            let gate = if ph < 0.85 { 1.0 } else { 0.0 };
            (std::f32::consts::TAU * ph_s).sin().signum() * 0.35 * gate
        }))
        .await;
        // Vibreur de manche (STALL)
        let shaker = load(synth(1.0, true, |t, r| {
            let s = sine(28.0, t).signum();
            s * 0.6 + r.next() * 0.25 * s.abs()
        }))
        .await;
        // Claquoir de survitesse
        let clacker = load(synth(1.0, true, |t, r| {
            let ph = t % 0.125;
            r.next() * (-ph * 120.0).exp() + sine(2200.0, t) * (-ph * 200.0).exp() * 0.5
        }))
        .await;
        // Klaxon de train
        let horn = load(synth(1.0, true, |t, _| {
            let gate = if t % 0.5 < 0.35 { 1.0 } else { 0.0 };
            (sine(400.0, t).signum() * 0.4 + sine(800.0, t) * 0.2) * gate
        }))
        .await;

        // --- Sons ponctuels ---
        let caution = load(synth(0.7, false, |t, _| {
            let f = if t < 0.25 { 800.0 } else { 620.0 };
            let ph = if t < 0.25 { t } else { t - 0.25 };
            sine(f, t) * (-ph * 6.0).exp() * 0.8
        }))
        .await;
        // Déconnexion pilote automatique : arpèges descendants répétés
        let mut ph_a = 0.0f32;
        let ap_disc = load(synth(1.5, false, move |t, _| {
            let ph = (t % 0.3) / 0.3;
            ph_a += (1600.0 - 700.0 * ph) / SR;
            (std::f32::consts::TAU * ph_a).sin() * 0.55
        }))
        .await;
        let mut lpb = Lp::new(700.0);
        let bang = load(synth(1.2, false, move |t, r| lpb.run(r.next()) * (-t * 4.0).exp() * 3.0 + sine(45.0, t) * (-t * 3.0).exp()))
            .await;
        let mut lpt = Lp::new(300.0);
        let thump = load(synth(0.5, false, move |t, r| sine(55.0, t) * (-t * 9.0).exp() + lpt.run(r.next()) * (-t * 14.0).exp() * 2.0))
            .await;
        let blip = load(synth(0.16, false, |t, _| sine(if t < 0.07 { 1500.0 } else { 2000.0 }, t) * 0.6)).await;
        let buzz = load(synth(0.3, false, |t, _| sine(180.0, t).signum() * 0.4)).await;
        // --- Interface de la station sol ---
        let mut lpc = Lp::new(3500.0);
        let click = load(synth(0.035, false, move |t, r| lpc.run(r.next()) * (-t * 160.0).exp() + sine(3200.0, t) * (-t * 220.0).exp() * 0.4)).await;
        let mut lpk = Lp::new(2500.0);
        let key = load(synth(0.03, false, move |t, r| lpk.run(r.next()) * (-t * 200.0).exp() * 0.8)).await;
        // Levier de console : deux claquements secs
        let mut lps = Lp::new(1800.0);
        let switch = load(synth(0.12, false, move |t, r| {
            let a = (-t * 90.0).exp();
            let b = if t > 0.045 { (-(t - 0.045) * 120.0).exp() * 0.7 } else { 0.0 };
            lps.run(r.next()) * (a + b) + sine(900.0, t) * a * 0.3
        }))
        .await;
        // AP engagé : tonalité douce montante ; A/THR déconnecté : double tic
        let ap_on = load(synth(0.35, false, |t, _| {
            let f = if t < 0.15 { 880.0 } else { 1320.0 };
            sine(f, t) * (-(t % 0.15) * 7.0).exp() * 0.5
        }))
        .await;
        let athr_off = load(synth(0.3, false, |t, _| {
            let ph = t % 0.12;
            sine(2400.0, t) * (-ph * 60.0).exp() * if t < 0.24 { 0.6 } else { 0.0 }
        }))
        .await;
        // Alerte d'altitude (1000 ft avant la consigne) : accord bref
        let alt_alert = load(synth(0.9, false, |t, _| (sine(523.0, t) + sine(659.0, t) * 0.8 + sine(784.0, t) * 0.6) * (-t * 3.0).exp() * 0.35)).await;
        // Minimums : carillon deux tons
        let mins = load(synth(1.0, false, |t, _| {
            let (f, ph) = if t < 0.4 { (988.0, t) } else { (784.0, t - 0.4) };
            (sine(f, t) + sine(f * 2.0, t) * 0.3) * (-ph * 4.0).exp() * 0.5
        }))
        .await;
        // Séquencement FMS : tic léger
        let seq = load(synth(0.08, false, |t, _| sine(1800.0, t) * (-t * 50.0).exp() * 0.5)).await;
        // Aérofreins : souffle filtré
        let mut lpw = Lp::new(600.0);
        let whoosh = load(synth(0.8, false, move |t, r| lpw.run(r.next()) * (t * 6.0).min(1.0) * (-t * 3.0).exp() * 2.5)).await;
        // Liaison perdue : salve de parasites puis tonalité descendante ; rétablie : trois notes montantes
        let mut lpl = Lp::new(4000.0);
        let mut ph_l = 0.0f32;
        let link_lost = load(synth(1.0, false, move |t, r| {
            let noise = if t < 0.35 { lpl.run(r.next()) * 0.9 } else { 0.0 };
            ph_l += (900.0 - 500.0 * t) / SR;
            let tone = if t > 0.3 { (std::f32::consts::TAU * ph_l).sin() * 0.45 * (-(t - 0.3) * 2.5).exp() } else { 0.0 };
            noise + tone
        }))
        .await;
        let link_ok = load(synth(0.6, false, |t, _| {
            let i = ((t / 0.18) as usize).min(2);
            let f = [660.0, 880.0, 1320.0][i];
            sine(f, t) * (-(t - i as f32 * 0.18) * 8.0).exp() * 0.5
        }))
        .await;

        // --- Boucles d'événements ---
        // Sonnerie d'incendie (cloche modulée)
        let fire_bell = load(synth(1.0, true, |t, _| {
            let ph = t % 0.125;
            (sine(1400.0, t) + sine(2780.0, t) * 0.5 + sine(4150.0, t) * 0.25) * (-ph * 25.0).exp() * 0.5
        }))
        .await;
        // Auto-GCAS : tonalité pulsée aiguë (« FLY UP »)
        let gcas = load(synth(1.0, true, |t, _| {
            let gate = if t % 0.25 < 0.16 { 1.0 } else { 0.0 };
            (sine(2000.0, t) * 0.5 + sine(2500.0, t) * 0.3) * gate
        }))
        .await;
        // Train en mouvement : moteur hydraulique
        let mut lpg = Lp::new(250.0);
        let gear_hum = load(synth(1.0, true, move |t, r| lpg.run(r.next()) * 1.5 + sine(120.0, t) * 0.25 + sine(240.0, t) * 0.1)).await;
        // Demande de configuration (GEAR DOWN / SPEED BRAKE) : carillon doux répété
        let config = load(synth(2.0, true, |t, _| {
            let ph = t % 1.0;
            sine(1175.0, t) * (-ph * 5.0).exp() * if ph < 0.6 { 0.35 } else { 0.0 }
        }))
        .await;
        // RWR façon F-16 : nouvel émetteur (« deedle »), accrochage (tonalité continue), tir (warble)
        let rwr_new = load(synth(0.5, false, |t, _| {
            let f = if (t * 16.0) as u32 % 2 == 0 { 1300.0 } else { 1700.0 };
            sine(f, t) * if t < 0.4 { 0.45 } else { 0.0 }
        }))
        .await;
        let rwr_lock = load(synth(1.0, true, |t, _| sine(1000.0, t) * 0.35)).await;
        let rwr_launch = load(synth(1.0, true, |t, _| {
            let f = if (t * 20.0) as u32 % 2 == 0 { 1450.0 } else { 2900.0 };
            sine(f, t) * 0.4
        }))
        .await;
        // Décollement : détente des amortisseurs (clac sourd) puis roues qui ralentissent (sifflement
        // descendant) ; roulement de piste en boucle (pneus, joints)
        let mut lpo = Lp::new(500.0);
        let mut ph_w = 0.0f32;
        let liftoff = load(synth(2.2, false, move |t, r| {
            let clunk = lpo.run(r.next()) * (-t * 18.0).exp() * 2.0 + sine(70.0, t) * (-t * 10.0).exp();
            ph_w += (520.0 * (-t * 1.1).exp() + 60.0) / SR;
            let whine = (std::f32::consts::TAU * ph_w).sin() * 0.25 * (-t * 1.4).exp() * (t * 8.0).min(1.0);
            clunk + whine
        }))
        .await;
        let mut lpr = Lp::new(180.0);
        let mut lpr2 = Lp::new(900.0);
        let runway = load(synth(2.0, true, move |t, r| {
            let n = r.next();
            lpr.run(n) * 2.2 + lpr2.run(n) * 0.35 + sine(38.0, t) * 0.2
        }))
        .await;
        // Roulage lent : pneus sur béton (grain, frottement du caoutchouc), creux rythmé
        let mut lpt1 = Lp::new(320.0);
        let mut lpt2 = Lp::new(1600.0);
        let taxi = load(synth(2.0, true, move |t, r| {
            let n = r.next();
            (lpt1.run(n) * 1.6 + (lpt2.run(n) - lpt1.0) * 0.25) * (1.0 + 0.25 * sine(2.0, t)) + sine(55.0, t) * 0.08
        }))
        .await;
        // Toucher : crissement des pneus (mise en rotation des roues, bande 2-5 kHz, glissando)
        let mut hps = Lp::new(1800.0);
        let mut lps = Lp::new(5200.0);
        let screech = load(synth(0.9, false, move |t, r| {
            let n = r.next();
            let band = lps.run(n) - hps.run(n);
            let chirp = sine(2600.0 - 900.0 * t, t) * 0.35;
            (band * 2.2 + chirp) * (-t * 4.5).exp() * (t * 60.0).min(1.0)
        }))
        .await;
        // Joint de dalle : claquement sourd et court
        let mut lpj = Lp::new(400.0);
        let seam = load(synth(0.12, false, move |t, r| lpj.run(r.next()) * (-t * 45.0).exp() * 2.0 + sine(90.0, t) * (-t * 35.0).exp() * 0.5)).await;
        // Roulette de nez qui retombe : coup lourd
        let mut lpn = Lp::new(250.0);
        let nose = load(synth(0.6, false, move |t, r| lpn.run(r.next()) * (-t * 10.0).exp() * 2.5 + sine(48.0, t) * (-t * 7.0).exp())).await;
        // Freins : grincement (crissement tonal + souffle)
        let mut lpb2 = Lp::new(3000.0);
        let brake = load(synth(1.5, true, move |t, r| {
            let w = 1.0 + 0.15 * sine(5.0, t);
            sine(2900.0 * w, t) * 0.25 + sine(4400.0, t) * 0.08 + lpb2.run(r.next()) * 0.4
        }))
        .await;
        // Reconnexion Iridium : gazouillis de modem
        let modem = load(synth(2.0, true, |t, r| {
            let slot = (t * 12.0) as u32;
            let f = [1200.0, 2200.0, 1800.0, 2400.0, 1300.0, 2100.0][(slot % 6) as usize];
            sine(f, t) * 0.25 + r.next() * 0.04
        }))
        .await;

        let sfx = Sfx {
            turb_lo, turb_hi, roar, ab, ram, scram, wind, rumble, warn, siren, shaker, clacker, horn,
            caution, ap_disc, bang, thump, blip, buzz, click, key, switch, ap_on, athr_off, alt_alert, mins, seq, whoosh, link_lost, link_ok, fire_bell, gcas, gear_hum, config, modem, liftoff, runway, taxi, screech, seam, nose, brake, rwr_new, rwr_lock, rwr_launch,
            voice: Voice::new(),
            muted: false,
            prev: Prev::default(),
        };
        for s in sfx.loops() {
            play_sound(s, PlaySoundParams { looped: true, volume: 0.0 });
        }
        sfx
    }

    fn loops(&self) -> [&Sound; 23] {
        [&self.runway, &self.taxi, &self.brake, &self.turb_lo, &self.turb_hi, &self.roar, &self.ab, &self.ram, &self.scram, &self.wind, &self.rumble, &self.warn, &self.siren, &self.shaker, &self.clacker, &self.horn, &self.fire_bell, &self.gcas, &self.gear_hum, &self.config, &self.modem, &self.rwr_lock, &self.rwr_launch]
    }

    /// RWR et voix de menace : `level` 0 rien, 1 veille, 2 poursuite, 3 accrochage, 4 tir ;
    /// `new` : nouvel émetteur entendu.
    pub fn threat(&mut self, level: u8, new: bool) {
        let m = if self.muted { 0.0 } else { 1.0 };
        if new {
            self.once(&self.rwr_new, 0.4);
        }
        set_sound_volume(&self.rwr_lock, m * if level == 3 { 0.25 } else { 0.0 });
        set_sound_volume(&self.rwr_launch, m * if level >= 4 { 0.35 } else { 0.0 });
        if level >= 4 && self.prev.threat < 4 {
            self.voice.say("missile", 3.0);
        } else if level == 3 && self.prev.threat < 3 {
            self.voice.say("lock", 4.0);
        }
        if level >= 4 {
            self.voice.say("missile", 4.0);
        }
        self.prev.threat = level;
    }

    fn once(&self, s: &Sound, vol: f32) {
        if !self.muted {
            play_sound(s, PlaySoundParams { looped: false, volume: vol });
        }
    }

    pub fn cue(&mut self, c: Cue) {
        match c {
            Cue::Ack => self.once(&self.blip, 0.35),
            Cue::Refused => self.once(&self.buzz, 0.35),
            Cue::Click => self.once(&self.click, 0.25),
            Cue::Key => self.once(&self.key, 0.18),
            Cue::Switch => self.once(&self.switch, 0.45),
            Cue::Minimums => {
                self.once(&self.mins, 0.5);
                self.voice.say("minimums", 3.0);
            }
            Cue::Seq => self.once(&self.seq, 0.3),
            Cue::LinkOk => {
                self.once(&self.link_ok, 0.45);
                self.voice.say("link_ok", 3.0);
            }
        }
    }

    /// Mise à jour à chaque image à partir de l'état de l'avion et des alarmes.
    pub fn update(&mut self, ac: &Aircraft, alerts: &[(Level, String)], acked: &[String]) {
        let m = if self.muted { 0.0 } else { 1.0 };
        let alive = ac.crashed.is_none();
        let e = &ac.eng;
        let k = if alive { m } else { 0.0 };

        // --- Moteur ---
        let n2 = e.n2 as f32;
        let mix = ((n2 - 0.62) / 0.38).clamp(0.0, 1.0);
        let spool = (n2 / 0.62).clamp(0.0, 1.0);
        set_sound_volume(&self.turb_lo, k * spool * (1.0 - mix) * 0.30);
        set_sound_volume(&self.turb_hi, k * (mix * 0.32 + spool * 0.04));
        set_sound_volume(&self.roar, k * ((e.t_turb / 300_000.0) as f32).clamp(0.0, 1.0) * 0.55);
        set_sound_volume(&self.ab, k * e.ab_frac as f32 * 0.60);
        set_sound_volume(&self.ram, k * ((e.t_ram / 400_000.0) as f32).clamp(0.0, 1.0) * 0.55);
        set_sound_volume(&self.scram, k * ((e.t_scram / 300_000.0) as f32).clamp(0.0, 1.0) * 0.55);
        set_sound_volume(&self.wind, k * ((ac.d.q / 80_000.0) as f32).clamp(0.0, 1.0) * 0.30);

        // --- Alarmes continues ---
        let has = |s: &str| alerts.iter().any(|(_, a)| a == s);
        let own_sound = ["PULL UP", "STALL", "TOO LOW — GEAR", "OVERSPEED — q LIMIT"];
        let warn_active = alerts.iter().any(|(l, s)| *l == Level::Warning && !acked.contains(s) && !own_sound.contains(&s.as_str()));
        set_sound_volume(&self.warn, k * if warn_active { 0.45 } else { 0.0 });
        set_sound_volume(&self.siren, k * if has("PULL UP") { 0.45 } else { 0.0 });
        set_sound_volume(&self.shaker, k * if has("STALL") { 0.6 } else { 0.0 });
        set_sound_volume(&self.clacker, k * if has("OVERSPEED — q LIMIT") { 0.4 } else { 0.0 });
        set_sound_volume(&self.horn, k * if has("TOO LOW — GEAR") { 0.4 } else { 0.0 });
        // Roulement sur la piste : croît avec la vitesse, s'arrête net au décollement
        let gs = (ac.ground_speed() / 90.0).clamp(0.0, 1.0) as f32;
        // Roulage lent (taxi) puis grondement de piste à grande vitesse ; freins qui grincent
        let gs_kt = (ac.ground_speed() / 0.514) as f32;
        let taxi_v = (gs_kt / 8.0).min(1.0) * (1.0 - ((gs_kt - 40.0) / 60.0).clamp(0.0, 1.0));
        set_sound_volume(&self.runway, k * if ac.on_ground { gs * 0.45 } else { 0.0 });
        set_sound_volume(&self.taxi, k * if ac.on_ground { taxi_v * 0.45 } else { 0.0 });
        let braking = ac.on_ground && ac.brakes && gs_kt > 3.0 && gs_kt < 90.0;
        set_sound_volume(&self.brake, k * if braking { (0.15 + 0.25 * (1.0 - gs_kt / 90.0)).min(0.35) } else { 0.0 });
        set_sound_volume(&self.fire_bell, k * if ac.fire.any() { 0.4 } else { 0.0 });
        let gcas_tone = ac.gcas_active || ac.osr_active || (ac.gcas_on && ac.gcas_t < 2.5);
        set_sound_volume(&self.gcas, k * if gcas_tone { 0.35 } else { 0.0 });
        let gear_moving = ac.gear_pos > 0.01 && ac.gear_pos < 0.99;
        set_sound_volume(&self.gear_hum, k * if gear_moving { 0.35 } else { 0.0 });
        let config_cue = (ac.cue_gear && !ac.gear_down) || ac.cue_sb.is_some();
        set_sound_volume(&self.config, k * if config_cue { 0.3 } else { 0.0 });

        // --- Événements ponctuels (sur transition) ---
        let p = &self.prev;
        let cautions = alerts.iter().filter(|(l, s)| *l == Level::Caution && !acked.contains(s)).count();
        let mut play: Vec<(&Sound, f32)> = Vec::new();
        let mut vq: Vec<(&'static str, f64)> = Vec::new();
        if p.started {
            if cautions > p.cautions {
                play.push((&self.caution, 0.4));
            }
            if p.ap && !ac.ap.on && !ac.on_ground && alive {
                play.push((&self.ap_disc, 0.4));
            }
            if p.ground && !ac.on_ground && alive {
                play.push((&self.liftoff, 0.6));
            }
            let unstart = e.scram == ScramState::Unstart;
            if unstart && !p.unstart {
                play.push((&self.bang, 0.6));
            }
            if !alive && !p.crashed {
                play.push((&self.bang, 1.0));
            }
            if !p.ap && ac.ap.on && alive {
                play.push((&self.ap_on, 0.35));
            }
            if p.at && !ac.ap.at_on && alive && ac.autoland.is_none() {
                play.push((&self.athr_off, 0.35));
            }
            if p.sb != ac.speedbrake && alive {
                play.push((&self.whoosh, 0.4));
            }
            let lit = e.scram == ScramState::Lit;
            if lit && !p.scram_lit {
                play.push((&self.thump, 0.6));
            }
            let ram = e.ram == RamState::Lit;
            if ram && !p.ram_lit {
                play.push((&self.thump, 0.5));
            }
            // --- Voix d'alerte (F-16 / F-35) ---
            let gcas_imminent = ac.gcas_on && ac.gcas_t < 2.5 && !ac.gcas_active;
            let rec = ac.gcas_active || ac.osr_active;
            if rec && !p.gcas {
                vq.push(("auto_rec", 2.0));
            }
            if rec {
                // F-16 : annonce répétée tant que la ressource dure
                vq.push(("fly_up", 3.0));
            }
            if !rec && p.gcas && alive {
                vq.push(("rec_done", 4.0));
            }
            // Décollage : ROTATE à VR, PITCH avant le contact arrière
            if ac.on_ground && ac.throttle > 0.4 && ac.d.keas >= ac.vr_keas() && ac.d.keas < ac.vr_keas() + 15.0 && ac.theta < 1f64.to_radians() {
                vq.push(("rotate", 20.0));
            }
            if ac.on_ground && ac.d.keas > 80.0 && ac.theta.to_degrees() > 10.5 {
                vq.push(("pitch", 2.0));
            }
            if gcas_imminent || has("PULL UP") {
                vq.push(("pull_up", 2.5));
            }
            if ac.fire.any() && !p.fire {
                vq.push(("fire", 3.0));
            }
            if ac.osr_active && !p.osr {
                vq.push(("overspeed", 3.0));
            }
            let _ = p.osr;
            let warns = alerts.iter().filter(|(l, s)| *l == Level::Warning && !acked.contains(s)).count();
            if warns > p.warns && !ac.fire.any() && !gcas_imminent && !ac.gcas_active {
                vq.push(("warning", 4.0));
            } else if cautions > p.cautions {
                vq.push(("caution", 4.0));
            }
            let over_g = ac.d.nz > ac.fcs.nz_max.max(1.5) * 0.95;
            if over_g && !p.over_g {
                vq.push(("over_g", 3.0));
            }
            if has("STALL") {
                vq.push(("stall", 2.5));
            }
            if has("TOO LOW — GEAR") {
                vq.push(("gear", 2.5));
            }
            let gear_cue = ac.cue_gear && !ac.gear_down;
            if gear_cue && !p.gear_cue {
                vq.push(("gear_down", 6.0));
            }
            let bingo = has("BINGO FUEL") || has("MIN FUEL") || has("EMERGENCY FUEL");
            if bingo && !p.bingo {
                vq.push(("bingo", 10.0));
            }
            let fuel_low = has("FUEL LOW");
            if fuel_low && !p.fuel_low {
                vq.push(("fuel_low", 10.0));
            }
            // Basse altitude (ALOW F-16) : moins de 1000 ft sol, train rentré, en descente, hors autoland
            if !ac.on_ground && ac.d.agl < 1_000.0 * crate::geo::FT && !ac.gear_down && ac.d.vs < -5.0 && ac.autoland.is_none() && !gcas_imminent {
                vq.push(("altitude", 4.0));
            }
            // Alerte d'altitude : passage à 1000 ft de la consigne en s'en approchant
            let err = (ac.ap.alt_tgt_ft - ac.alt / crate::geo::FT).abs();
            if ac.ap.on && ac.ap.vert == VertMode::Alt && p.alt_err > 1_000.0 && err <= 1_000.0 {
                play.push((&self.alt_alert, 0.4));
            }
        }
        for (s, v) in play {
            self.once(s, v);
        }
        for (id, g) in vq {
            self.voice.say(id, g);
        }
        self.prev = Prev {
            cautions,
            ap: ac.ap.on,
            ground: ac.on_ground,
            unstart: e.scram == ScramState::Unstart,
            crashed: !alive,
            started: true,
            at: ac.ap.at_on,
            sb: ac.speedbrake,
            scram_lit: e.scram == ScramState::Lit,
            ram_lit: e.ram == RamState::Lit,
            alt_err: (ac.ap.alt_tgt_ft - ac.alt / crate::geo::FT).abs(),
            gcas: ac.gcas_active || ac.osr_active,
            fire: ac.fire.any(),
            osr: ac.osr_active,
            warns: alerts.iter().filter(|(l, s)| *l == Level::Warning && !acked.contains(s)).count(),
            over_g: ac.d.nz > ac.fcs.nz_max.max(1.5) * 0.95,
            bingo: has("BINGO FUEL") || has("MIN FUEL") || has("EMERGENCY FUEL"),
            fuel_low: has("FUEL LOW"),
            gear_cue: ac.cue_gear && !ac.gear_down,
            threat: self.prev.threat,
        };
        self.voice.update(self.muted);
    }

    /// Ressenti de vol : grondement de structure et bang supersonique.
    pub fn feel(&self, rumble: f32, boom: bool, joint: f32, touch: Option<(f32, f32)>, nose_down: bool) {
        let m = if self.muted { 0.0 } else { 1.0 };
        set_sound_volume(&self.rumble, m * rumble * 0.55);
        if boom {
            self.once(&self.bang, 0.45);
        }
        if joint > 0.0 {
            self.once(&self.seam, 0.12 + joint * 0.2);
        }
        // Toucher : crissement selon la vitesse sol, impact selon le taux de chute
        if let Some((gs_kt, sink_fpm)) = touch {
            self.once(&self.screech, (gs_kt / 180.0).clamp(0.2, 0.75));
            self.once(&self.thump, (0.35 + sink_fpm / 900.0).clamp(0.35, 1.0));
        }
        if nose_down {
            self.once(&self.nose, 0.6);
        }
    }

    /// Perte du véhicule : alarme continue pendant la cascade de pannes (jusqu'à la perte du signal).
    pub fn mishap(&self, cascade: bool) {
        if cascade {
            let m = if self.muted { 0.0 } else { 1.0 };
            set_sound_volume(&self.warn, m * 0.5);
            set_sound_volume(&self.siren, m * 0.25);
        }
    }

    /// Liaison : perte (parasites + tonalité), gazouillis de modem pendant la reconnexion.
    pub fn link(&mut self, lost_now: bool, reconnecting: bool) {
        let m = if self.muted { 0.0 } else { 1.0 };
        if lost_now {
            self.once(&self.link_lost, 0.5);
            self.voice.say("link_lost", 3.0);
        }
        set_sound_volume(&self.modem, m * if reconnecting { 0.25 } else { 0.0 });
    }

    /// Réinitialise le suivi des transitions (nouvel essai).
    pub fn reset(&mut self) {
        self.prev = Prev::default();
    }
}


// ---------- Voix d'alerte (façon « Bitching Betty » F-16 / F-35) ----------

/// Phrases de la voix d'alerte : (identifiant, texte prononcé).
const PHRASES: [(&str, &str); 22] = [
    ("pull_up", "Pull up. Pull up."),
    ("fly_up", "Fly up."),
    ("warning", "Warning. Warning."),
    ("caution", "Caution."),
    ("fire", "Engine fire. Engine fire."),
    ("bingo", "Bingo. Bingo."),
    ("over_g", "Over G."),
    ("stall", "Stall. Stall."),
    ("gear", "Gear. Gear."),
    ("gear_down", "Gear down."),
    ("overspeed", "Overspeed."),
    ("altitude", "Altitude. Altitude."),
    ("link_lost", "Data link lost."),
    ("link_ok", "Data link."),
    ("minimums", "Minimums."),
    ("missile", "Missile. Missile."),
    ("lock", "Lock."),
    ("fuel_low", "Fuel low."),
    ("rotate", "Rotate."),
    ("pitch", "Pitch. Pitch."),
    ("auto_rec", "Auto recovery."),
    ("rec_done", "Recovery complete."),
];

fn voice_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join("Library/Application Support/Darkstar/voice/v1")
}

/// Génère (une fois) les phrases avec la synthèse vocale de macOS, en tâche de fond.
pub fn generate_voices() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let d2 = done.clone();
    std::thread::spawn(move || {
        let dir = voice_dir();
        let _ = std::fs::create_dir_all(&dir);
        for (id, text) in PHRASES {
            let f = dir.join(format!("{id}.wav"));
            if !f.exists() {
                // Fichier temporaire puis renommage : pas de phrase tronquée si le jeu est quitté
                let tmp = dir.join(format!("{id}.tmp.wav"));
                let ok = std::process::Command::new("/usr/bin/say")
                    .args(["-v", "Samantha", "-r", "205", "--data-format=LEI16@22050", "-o"])
                    .arg(&tmp)
                    .arg(text)
                    .status()
                    .is_ok_and(|st| st.success());
                if ok {
                    let _ = std::fs::rename(&tmp, &f);
                }
            }
        }
        d2.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    done
}

/// Rendu « radio de cockpit » : passe-bande 350-3200 Hz, légère saturation, clic de micro.
fn radio_voice(bytes: &[u8]) -> Option<(Vec<u8>, f32)> {
    // Lecture du WAV PCM 16 bits mono (chunks « fmt » et « data »)
    let mut i = 12;
    let mut data: Option<&[u8]> = None;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let len = u32::from_le_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        if id == b"data" {
            data = bytes.get(i + 8..(i + 8 + len).min(bytes.len()));
            break;
        }
        i += 8 + len + (len & 1);
    }
    let data = data?;
    let x: Vec<f32> = data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
    let mut hp = Lp::new(350.0);
    let mut lp = Lp::new(3200.0);
    let mut out: Vec<f32> = Vec::with_capacity(x.len() + 600);
    // Clic d'alternat avant la phrase
    for k in 0..300 {
        out.push(((k as f32 * 1.7).sin()) * (-(k as f32) / 40.0).exp() * 0.3);
    }
    for v in x {
        let b = lp.run(v - hp.run(v));
        out.push((b * 2.2).tanh() * 0.9);
    }
    let dur = out.len() as f32 / SR;
    Some((wav(&out), dur))
}

pub struct Voice {
    sounds: Vec<(&'static str, Sound, f32)>,
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    loaded: bool,
    queue: std::collections::VecDeque<&'static str>,
    busy_until: f64,
    last: std::collections::HashMap<&'static str, f64>,
}

impl Voice {
    pub fn new() -> Voice {
        Voice { sounds: Vec::new(), ready: generate_voices(), loaded: false, queue: Default::default(), busy_until: 0.0, last: Default::default() }
    }

    /// Charge les phrases une fois la génération terminée (à appeler dans la boucle principale).
    pub async fn poll(&mut self) {
        if self.loaded || !self.ready.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.loaded = true;
        for (id, _) in PHRASES {
            if let Ok(b) = std::fs::read(voice_dir().join(format!("{id}.wav"))) {
                if let Some((w, dur)) = radio_voice(&b) {
                    if let Ok(snd) = load_sound_from_bytes(&w).await {
                        self.sounds.push((id, snd, dur));
                    }
                }
            }
        }
    }

    /// Annonce `id` (sans doublon en file ; pas plus d'une fois toutes les `min_gap` secondes).
    pub fn say(&mut self, id: &'static str, min_gap: f64) {
        let now = macroquad::time::get_time();
        if self.last.get(id).is_some_and(|t| now - t < min_gap) || self.queue.contains(&id) {
            return;
        }
        self.last.insert(id, now);
        if self.queue.len() < 4 {
            self.queue.push_back(id);
        }
    }

    /// Joue la phrase suivante quand la précédente est terminée.
    pub fn update(&mut self, muted: bool) {
        let now = macroquad::time::get_time();
        if now < self.busy_until {
            return;
        }
        if let Some(id) = self.queue.pop_front() {
            if let Some((_, s, dur)) = self.sounds.iter().find(|(i, _, _)| *i == id) {
                if !muted {
                    play_sound(s, PlaySoundParams { looped: false, volume: 0.75 });
                }
                self.busy_until = now + *dur as f64 + 0.15;
            }
        }
    }
}
