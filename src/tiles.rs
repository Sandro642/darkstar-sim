//! Tuiles cartographiques réelles et gratuites, téléchargées en arrière-plan (curl) avec cache disque :
//! imagerie satellite Esri World Imagery, fond de carte CARTO (données OpenStreetMap) et relief
//! AWS Terrain Tiles (encodage Terrarium). Projection Web Mercator (schéma XYZ).

use macroquad::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Layer {
    Sat,
    Plan,
    Labels,
    Elev,
}

impl Layer {
    fn name(self) -> &'static str {
        match self {
            Layer::Sat => "sat",
            Layer::Plan => "gris",
            Layer::Labels => "noms",
            Layer::Elev => "elev",
        }
    }
    fn url(self, z: u8, x: u32, y: u32) -> String {
        match self {
            Layer::Sat => format!("https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"),
            Layer::Plan => format!("https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}"),
            Layer::Labels => format!("https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Reference/MapServer/tile/{z}/{y}/{x}"),
            Layer::Elev => format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"),
        }
    }
    pub fn max_z(self) -> u8 {
        match self {
            Layer::Sat => 17,
            Layer::Plan | Layer::Labels => 16,
            Layer::Elev => ELEV_Z,
        }
    }
    pub fn credit(self) -> &'static str {
        match self {
            Layer::Sat => "Imagerie : Esri, Maxar, Earthstar Geographics",
            Layer::Plan | Layer::Labels => "Carte : Esri, HERE, Garmin, (c) OpenStreetMap contributors",
            Layer::Elev => "Relief : AWS Terrain Tiles (Mapzen)",
        }
    }
}

/// Zoom maximal du relief (≈ 30 m par échantillon à 35° de latitude).
pub const ELEV_Z: u8 = 12;
/// Côté d'une tuile de relief après sous-échantillonnage.
const ELEV_N: usize = 128;
/// Nombre maximal de textures gardées en mémoire graphique.
const TEX_CAP: usize = 720;
/// Nombre maximal de tuiles de relief gardées en mémoire.
const ELEV_CAP: usize = 900;
const UPLOADS_PER_FRAME: usize = 24;
/// Tuiles décodées en attente d'envoi au GPU au-delà desquelles les fils de travail patientent.
const DONE_MAX: usize = 48;
const WORKERS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub layer: Layer,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl Key {
    pub fn parent(self) -> Option<Key> {
        (self.z > 0).then(|| Key { z: self.z - 1, x: self.x / 2, y: self.y / 2, ..self })
    }
}

enum Decoded {
    Rgba(u16, u16, Vec<u8>),
    Failed,
}

struct ElevTile {
    h: Vec<f32>,
    used: std::sync::atomic::AtomicU64,
}

struct Shared {
    queue: Mutex<Vec<Key>>,
    cv: Condvar,
    inflight: Mutex<HashSet<Key>>,
    done: Mutex<Vec<(Key, Decoded)>>,
    elev: RwLock<HashMap<(u8, u32, u32), Arc<ElevTile>>>,
    elev_failed: Mutex<HashMap<(u8, u32, u32), f64>>,
    clock: std::sync::atomic::AtomicU64,
    cache: PathBuf,
    errors: std::sync::atomic::AtomicU32,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

fn now_s() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

// ---------- Web Mercator ----------

pub fn n_tiles(z: u8) -> f64 {
    (1u64 << z) as f64
}

/// Coordonnées de tuile fractionnaires (x, y) d'un point.
pub fn tile_xy(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let n = n_tiles(z);
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    let x = (lon + 180.0) / 360.0 * n;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
    (x, y)
}

pub fn tile_lon(x: f64, z: u8) -> f64 {
    x / n_tiles(z) * 360.0 - 180.0
}

pub fn tile_lat(y: f64, z: u8) -> f64 {
    (std::f64::consts::PI * (1.0 - 2.0 * y / n_tiles(z))).sinh().atan().to_degrees()
}

// ---------- Téléchargement / décodage (fils de travail) ----------

fn tile_path(cache: &PathBuf, k: Key) -> PathBuf {
    let ext = if matches!(k.layer, Layer::Sat | Layer::Plan) { "jpg" } else { "png" };
    cache.join(k.layer.name()).join(k.z.to_string()).join(k.x.to_string()).join(format!("{}.{ext}", k.y))
}

fn fetch(sh: &Shared, k: Key) -> Option<Vec<u8>> {
    let path = tile_path(&sh.cache, k);
    if let Ok(b) = std::fs::read(&path) {
        if !b.is_empty() {
            return Some(b);
        }
    }
    std::fs::create_dir_all(path.parent()?).ok()?;
    let tmp = path.with_extension("part");
    let ok = Command::new("/usr/bin/curl")
        .args(["-sfL", "--max-time", "20", "-A", concat!("DarkstarSim/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Sandro642/darkstar-sim)"), "-o"])
        .arg(&tmp)
        .arg(k.layer.url(k.z, k.x, k.y))
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        sh.errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return None;
    }
    std::fs::rename(&tmp, &path).ok()?;
    sh.errors.store(0, std::sync::atomic::Ordering::Relaxed);
    std::fs::read(&path).ok()
}

/// Décode une tuile Terrarium : altitude = R*256 + G + B/256 - 32768 (m), sous-échantillonnée.
fn decode_elev(bytes: &[u8]) -> Option<Vec<f32>> {
    let img = image::load_from_memory(bytes).ok()?.to_rgb8();
    let (w, h) = img.dimensions();
    let f = (w as usize / ELEV_N).max(1);
    let mut out = vec![0f32; ELEV_N * ELEV_N];
    for j in 0..ELEV_N {
        for i in 0..ELEV_N {
            let mut s = 0.0;
            for dj in 0..f {
                for di in 0..f {
                    let p = img.get_pixel(((i * f + di) as u32).min(w - 1), ((j * f + dj) as u32).min(h - 1));
                    s += p[0] as f32 * 256.0 + p[1] as f32 + p[2] as f32 / 256.0 - 32768.0;
                }
            }
            out[j * ELEV_N + i] = s / (f * f) as f32;
        }
    }
    Some(out)
}

fn worker(sh: Arc<Shared>) {
    loop {
        // Contre-pression : ne pas décoder plus vite que le fil principal n'envoie au GPU
        while sh.done.lock().unwrap().len() >= DONE_MAX {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let k = {
            let mut q = sh.queue.lock().unwrap();
            while q.is_empty() {
                q = sh.cv.wait(q).unwrap();
            }
            let k = q.remove(0);
            sh.inflight.lock().unwrap().insert(k);
            k
        };
        let bytes = fetch(&sh, k);
        if k.layer == Layer::Elev {
            match bytes.as_deref().and_then(decode_elev) {
                Some(h) => {
                    let t = Arc::new(ElevTile { h, used: std::sync::atomic::AtomicU64::new(sh.clock.load(std::sync::atomic::Ordering::Relaxed)) });
                    let mut m = sh.elev.write().unwrap();
                    if m.len() >= ELEV_CAP {
                        // Éviction des tuiles les moins récemment utilisées
                        let mut v: Vec<_> = m.iter().map(|(k, t)| (*k, t.used.load(std::sync::atomic::Ordering::Relaxed))).collect();
                        v.sort_by_key(|e| e.1);
                        for (k, _) in v.iter().take(ELEV_CAP / 4) {
                            m.remove(k);
                        }
                    }
                    m.insert((k.z, k.x, k.y), t);
                }
                None => {
                    sh.elev_failed.lock().unwrap().insert((k.z, k.x, k.y), now_s());
                }
            }
        } else {
            let dec = bytes
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|img| {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    Decoded::Rgba(w as u16, h as u16, rgba.into_raw())
                })
                .unwrap_or(Decoded::Failed);
            // Reste « en cours » jusqu'à son envoi au GPU : sinon elle serait redemandée et
            // décodée en double à chaque image (fuite mémoire quand la cadence d'affichage chute)
            sh.done.lock().unwrap().push((k, dec));
            continue;
        }
        sh.inflight.lock().unwrap().remove(&k);
    }
}

// ---------- Relief (accessible aussi au modèle de vol) ----------

fn elev_tile(sh: &Shared, z: u8, x: u32, y: u32) -> Option<Arc<ElevTile>> {
    let t = sh.elev.read().unwrap().get(&(z, x, y)).cloned()?;
    t.used.store(sh.clock.load(std::sync::atomic::Ordering::Relaxed), std::sync::atomic::Ordering::Relaxed);
    Some(t)
}

/// Altitude du terrain réel (m, mer = 0) à partir des tuiles chargées, au zoom `zmax` ou moins.
/// `None` si aucune tuile n'est disponible (hors ligne, pas encore chargée, ou tests).
pub fn elevation_z(lat: f64, lon: f64, zmax: u8) -> Option<f64> {
    let sh = SHARED.get()?;
    let mut z = zmax.min(ELEV_Z);
    loop {
        let (fx, fy) = tile_xy(lat, lon, z);
        let n = n_tiles(z) as u32;
        let (tx, ty) = ((fx.floor() as u32) % n, (fy.floor() as u32).min(n - 1));
        if let Some(t) = elev_tile(sh, z, tx, ty) {
            let px = ((fx - fx.floor()) * ELEV_N as f64 - 0.5).clamp(0.0, ELEV_N as f64 - 1.001);
            let py = ((fy - fy.floor()) * ELEV_N as f64 - 0.5).clamp(0.0, ELEV_N as f64 - 1.001);
            let (i, j) = (px as usize, py as usize);
            let (u, v) = ((px - i as f64) as f32, (py - j as f64) as f32);
            let at = |i: usize, j: usize| t.h[j.min(ELEV_N - 1) * ELEV_N + i.min(ELEV_N - 1)];
            let a = at(i, j) + (at(i + 1, j) - at(i, j)) * u;
            let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * u;
            return Some(((a + (b - a) * v) as f64).max(0.0));
        }
        if z <= 4 {
            return None;
        }
        z -= 1;
    }
}

/// Tuile de relief couvrant entièrement une tuile (z, x, y) : échantillonnage rapide sans verrou.
pub struct ElevPatch {
    t: Arc<ElevTile>,
    z: u8,
    x: u32,
    y: u32,
}

impl ElevPatch {
    /// Recherche la tuile de relief au zoom `ze` (ou moins) qui contient la tuile (z, x, y).
    pub fn find(z: u8, x: u32, y: u32, ze: u8) -> Option<ElevPatch> {
        let sh = SHARED.get()?;
        let mut e = ze.min(z).min(ELEV_Z);
        loop {
            let (ex, ey) = (x >> (z - e), y >> (z - e));
            if let Some(t) = elev_tile(sh, e, ex, ey) {
                return Some(ElevPatch { t, z: e, x: ex, y: ey });
            }
            if e <= 3 {
                return None;
            }
            e -= 1;
        }
    }
    /// Altitude (m, >= 0) en coordonnées de tuile fractionnaires au zoom `z`.
    pub fn sample_xy(&self, fx: f64, fy: f64, z: u8) -> f64 {
        let k = n_tiles(z) / n_tiles(self.z);
        let px = ((fx / k - self.x as f64) * ELEV_N as f64 - 0.5).clamp(0.0, ELEV_N as f64 - 1.001);
        let py = ((fy / k - self.y as f64) * ELEV_N as f64 - 0.5).clamp(0.0, ELEV_N as f64 - 1.001);
        let (i, j) = (px as usize, py as usize);
        let (u, v) = ((px - i as f64) as f32, (py - j as f64) as f32);
        let h = &self.t.h;
        let at = |i: usize, j: usize| h[j.min(ELEV_N - 1) * ELEV_N + i.min(ELEV_N - 1)];
        let a = at(i, j) + (at(i + 1, j) - at(i, j)) * u;
        let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * u;
        ((a + (b - a) * v) as f64).max(0.0)
    }
}

pub fn elevation(lat: f64, lon: f64) -> Option<f64> {
    elevation_z(lat, lon, ELEV_Z)
}

// ---------- Côté rendu (fil principal) ----------

pub struct TexRef<'a> {
    pub tex: &'a Texture2D,
    /// Sous-rectangle UV (tuile ancêtre agrandie quand la tuile demandée n'est pas encore là).
    pub uv: Rect,
    pub exact: bool,
}

pub struct Tiles {
    sh: Arc<Shared>,
    tex: HashMap<Key, (Texture2D, u64)>,
    failed: HashMap<Key, f64>,
    wanted: Vec<(f32, Key)>,
    frame: u64,
}

impl Tiles {
    pub fn new() -> Tiles {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let sh = Arc::new(Shared {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
            inflight: Mutex::new(HashSet::new()),
            done: Mutex::new(Vec::new()),
            elev: RwLock::new(HashMap::new()),
            elev_failed: Mutex::new(HashMap::new()),
            clock: std::sync::atomic::AtomicU64::new(0),
            cache: PathBuf::from(home).join("Library/Caches/Darkstar/tiles"),
            errors: std::sync::atomic::AtomicU32::new(0),
        });
        for _ in 0..WORKERS {
            let s = sh.clone();
            std::thread::spawn(move || worker(s));
        }
        let _ = SHARED.set(sh.clone());
        Tiles { sh, tex: HashMap::new(), failed: HashMap::new(), wanted: Vec::new(), frame: 0 }
    }

    /// Hors ligne (échecs de téléchargement répétés).
    pub fn offline(&self) -> bool {
        self.sh.errors.load(std::sync::atomic::Ordering::Relaxed) > 12
    }

    /// Début d'image : envoi au GPU des tuiles décodées, éviction des plus anciennes.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.sh.clock.store(self.frame, std::sync::atomic::Ordering::Relaxed);
        let ready: Vec<(Key, Decoded)> = {
            let mut d = self.sh.done.lock().unwrap();
            let n = d.len().min(UPLOADS_PER_FRAME);
            d.drain(..n).collect()
        };
        let t = get_time();
        if !ready.is_empty() {
            let mut inf = self.sh.inflight.lock().unwrap();
            for (k, _) in &ready {
                inf.remove(k);
            }
        }
        for (k, dec) in ready {
            match dec {
                Decoded::Rgba(w, h, px) => {
                    let tex = Texture2D::from_rgba8(w, h, &px);
                    tex.set_filter(FilterMode::Linear);
                    self.tex.insert(k, (tex, self.frame));
                }
                Decoded::Failed => {
                    self.failed.insert(k, t);
                }
            }
        }
        if self.tex.len() > TEX_CAP {
            let mut v: Vec<(Key, u64)> = self.tex.iter().map(|(k, (_, f))| (*k, *f)).collect();
            v.sort_by_key(|e| e.1);
            let excess = self.tex.len() - TEX_CAP * 9 / 10;
            for (k, f) in v.into_iter().take(excess) {
                if f + 1 < self.frame {
                    self.tex.remove(&k);
                }
            }
        }
    }

    /// Fin d'image : la file de téléchargement est remplacée par les tuiles demandées cette image.
    pub fn end_frame(&mut self) {
        let mut w = std::mem::take(&mut self.wanted);
        w.sort_by(|a, b| a.0.total_cmp(&b.0));
        let inflight = self.sh.inflight.lock().unwrap().clone();
        let mut seen = HashSet::new();
        let q: Vec<Key> = w.into_iter().map(|(_, k)| k).filter(|k| !inflight.contains(k) && seen.insert(*k)).take(400).collect();
        let has = !q.is_empty();
        *self.sh.queue.lock().unwrap() = q;
        if has {
            self.sh.cv.notify_all();
        }
    }

    /// Demande une tuile (priorité : plus petit = plus urgent).
    pub fn request(&mut self, k: Key, prio: f32) {
        if self.tex.contains_key(&k) {
            return;
        }
        if let Some(t) = self.failed.get(&k) {
            if get_time() - t < 60.0 {
                return;
            }
        }
        self.wanted.push((prio, k));
    }

    /// Demande la tuile de relief couvrant un point.
    pub fn request_elev(&mut self, lat: f64, lon: f64, z: u8, prio: f32) {
        let z = z.min(ELEV_Z);
        let (fx, fy) = tile_xy(lat, lon, z);
        let n = n_tiles(z) as u32;
        let (x, y) = ((fx.floor() as u32) % n, (fy.floor() as u32).min(n - 1));
        if self.sh.elev.read().unwrap().contains_key(&(z, x, y)) {
            return;
        }
        if let Some(t) = self.sh.elev_failed.lock().unwrap().get(&(z, x, y)) {
            if now_s() - t < 60.0 {
                return;
            }
        }
        self.wanted.push((prio, Key { layer: Layer::Elev, z, x, y }));
    }

    /// Texture de la tuile, ou à défaut de l'ancêtre chargé le plus proche (sous-rectangle UV).
    /// La tuile manquante est demandée.
    pub fn get(&mut self, k: Key, prio: f32) -> Option<TexRef<'_>> {
        self.request(k, prio);
        // Ancêtre grossier demandé en priorité : il sert de repli pendant le chargement
        if k.z >= 3 {
            let a = Key { z: k.z - 3, x: k.x >> 3, y: k.y >> 3, ..k };
            self.request(a, prio - 3.0);
        }
        let mut cur = k;
        let mut uv = Rect::new(0.0, 0.0, 1.0, 1.0);
        loop {
            if self.tex.contains_key(&cur) {
                break;
            }
            let p = cur.parent()?;
            // Position de `cur` dans son parent
            uv = Rect::new((uv.x + (cur.x & 1) as f32) / 2.0, (uv.y + (cur.y & 1) as f32) / 2.0, uv.w / 2.0, uv.h / 2.0);
            cur = p;
            if k.z - cur.z > 6 {
                return None;
            }
        }
        let frame = self.frame;
        let e = self.tex.get_mut(&cur)?;
        e.1 = frame;
        Some(TexRef { tex: &e.0, uv, exact: cur == k })
    }
}

impl Tiles {
    /// Statistiques pour le diagnostic de performance : (textures, décodées en attente, en cours, relief).
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (self.tex.len(), self.sh.done.lock().unwrap().len(), self.sh.inflight.lock().unwrap().len(), self.sh.elev.read().unwrap().len())
    }
}
