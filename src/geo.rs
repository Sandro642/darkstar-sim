//! Géodésie sphérique : distances, caps, projections locales.

pub const R_EARTH: f64 = 6_371_000.0;
pub const FT: f64 = 0.3048;
pub const KT: f64 = 0.514444;
pub const NM: f64 = 1852.0;

pub fn wrap180(d: f64) -> f64 {
    let mut x = (d + 180.0) % 360.0;
    if x < 0.0 {
        x += 360.0;
    }
    x - 180.0
}

pub fn wrap360(d: f64) -> f64 {
    let x = d % 360.0;
    if x < 0.0 {
        x + 360.0
    } else {
        x
    }
}

/// Point d'arrivée (lat, lon en degrés) depuis un point, un cap vrai (deg) et une distance (m).
pub fn destination(lat: f64, lon: f64, brg_deg: f64, dist: f64) -> (f64, f64) {
    let d = dist / R_EARTH;
    let (la1, lo1, b) = (lat.to_radians(), lon.to_radians(), brg_deg.to_radians());
    let la2 = (la1.sin() * d.cos() + la1.cos() * d.sin() * b.cos()).clamp(-1.0, 1.0).asin();
    let lo2 = lo1 + (b.sin() * d.sin() * la1.cos()).atan2(d.cos() - la1.sin() * la2.sin());
    (la2.to_degrees(), wrap180(lo2.to_degrees()))
}

/// Distance orthodromique (m) et cap initial (deg) entre deux points.
pub fn dist_brg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> (f64, f64) {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let a = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    let d = 2.0 * R_EARTH * a.sqrt().min(1.0).asin();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (d, wrap360(y.atan2(x).to_degrees()))
}

/// Décalage local plat (Est, Nord) en mètres d'un point par rapport à une référence.
/// Valable à quelques centaines de km.
pub fn enu_offset(ref_lat: f64, ref_lon: f64, lat: f64, lon: f64) -> (f64, f64) {
    let m_per_deg = R_EARTH.to_radians();
    let e = wrap180(lon - ref_lon) * m_per_deg * ref_lat.to_radians().cos();
    let n = (lat - ref_lat) * m_per_deg;
    (e, n)
}

/// Inverse de enu_offset.
pub fn from_enu(ref_lat: f64, ref_lon: f64, e: f64, n: f64) -> (f64, f64) {
    let m_per_deg = R_EARTH.to_radians();
    let lat = ref_lat + n / m_per_deg;
    let lon = wrap180(ref_lon + e / (m_per_deg * ref_lat.to_radians().cos().max(1e-6)));
    (lat, lon)
}

/// Abaissement dû à la courbure terrestre à une distance horizontale donnée.
pub fn curvature_drop(d: f64) -> f64 {
    d * d / (2.0 * R_EARTH)
}
