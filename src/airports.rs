//! Aéroports et pistes (coordonnées approximatives, cap vrai).

use crate::geo::*;

pub struct Airport {
    pub icao: &'static str,
    pub name: &'static str,
    pub lat: f64, // centre de piste
    pub lon: f64,
    pub hdg: f64, // cap vrai de la piste (sens "bas" de numérotation)
    pub len: f64, // m
    pub width: f64,
    pub elev: f64, // m
}

macro_rules! apt {
    ($i:expr, $n:expr, $la:expr, $lo:expr, $h:expr, $l:expr, $w:expr, $e:expr) => {
        Airport { icao: $i, name: $n, lat: $la, lon: $lo, hdg: $h, len: $l, width: $w, elev: $e }
    };
}

pub const AIRPORTS: &[Airport] = &[
    apt!("KPMD", "PALMDALE PLANT 42 (SKUNK WORKS)", 34.6294, -118.0846, 71.0, 3658.0, 61.0, 775.0),
    apt!("KEDW", "EDWARDS AFB", 34.9054, -117.8839, 51.0, 4572.0, 91.0, 704.0),
    apt!("KLAX", "LOS ANGELES", 33.9425, -118.4081, 83.0, 3685.0, 61.0, 38.0),
    apt!("KSFO", "SAN FRANCISCO", 37.6189, -122.3750, 118.0, 3618.0, 61.0, 4.0),
    apt!("KLAS", "LAS VEGAS", 36.0840, -115.1537, 77.0, 4423.0, 46.0, 664.0),
    apt!("KNID", "CHINA LAKE NAWS", 35.6854, -117.6922, 33.0, 3048.0, 61.0, 696.0),
    apt!("KDEN", "DENVER", 39.8561, -104.6737, 0.0, 4877.0, 61.0, 1655.0),
    apt!("KJFK", "NEW YORK JFK", 40.6413, -73.7781, 31.0, 4423.0, 61.0, 4.0),
    apt!("CYYZ", "TORONTO", 43.6777, -79.6248, 53.0, 3368.0, 61.0, 173.0),
    apt!("PANC", "ANCHORAGE", 61.1743, -149.9962, 70.0, 3780.0, 61.0, 46.0),
    apt!("PHNL", "HONOLULU", 21.3187, -157.9225, 78.0, 3753.0, 61.0, 4.0),
    apt!("PGUA", "ANDERSEN AFB GUAM", 13.5840, 144.9300, 60.0, 3411.0, 61.0, 190.0),
    apt!("LFPG", "PARIS CDG", 49.0097, 2.5479, 86.0, 4200.0, 45.0, 119.0),
    apt!("LFBO", "TOULOUSE BLAGNAC", 43.6293, 1.3638, 143.0, 3500.0, 45.0, 152.0),
    apt!("LFMI", "ISTRES BA125", 43.5227, 4.9238, 152.0, 3750.0, 80.0, 23.0),
    apt!("EGLL", "LONDON HEATHROW", 51.4700, -0.4543, 90.0, 3902.0, 50.0, 25.0),
    apt!("EDDF", "FRANKFURT", 50.0379, 8.5622, 70.0, 4000.0, 60.0, 111.0),
    apt!("LEMD", "MADRID BARAJAS", 40.4983, -3.5676, 180.0, 4350.0, 60.0, 610.0),
    apt!("OMDB", "DUBAI", 25.2532, 55.3657, 120.0, 4000.0, 60.0, 19.0),
    apt!("FJDG", "DIEGO GARCIA", -7.3133, 72.4111, 130.0, 3659.0, 61.0, 2.0),
    apt!("VHHH", "HONG KONG", 22.3080, 113.9185, 73.0, 3800.0, 60.0, 9.0),
    apt!("RJTT", "TOKYO HANEDA", 35.5494, 139.7798, 157.0, 3000.0, 60.0, 6.0),
    apt!("YSSY", "SYDNEY", -33.9399, 151.1753, 155.0, 3962.0, 45.0, 6.0),
    apt!("SBGR", "SAO PAULO", -23.4356, -46.4731, 95.0, 3700.0, 45.0, 750.0),
    apt!("FAOR", "JOHANNESBURG", -26.1392, 28.2460, 29.0, 4418.0, 60.0, 1694.0),
    apt!("SCEL", "SANTIAGO", -33.3930, -70.7858, 175.0, 3800.0, 55.0, 474.0),
    apt!("BIKF", "KEFLAVIK", 63.9850, -22.6056, 10.0, 3065.0, 60.0, 52.0),
];

/// Seuil d'approche choisi en fonction de la position de l'avion : (lat, lon, cap d'approche).
pub struct Approach {
    pub thr_lat: f64,
    pub thr_lon: f64,
    pub hdg: f64,
}

impl Airport {
    /// Coordonnées géographiques d'un point (le long, latéral à droite) dans le repère piste.
    pub fn point(&self, along: f64, cross: f64) -> (f64, f64) {
        let h = self.hdg.to_radians();
        let e = along * h.sin() + cross * h.cos();
        let n = along * h.cos() - cross * h.sin();
        from_enu(self.lat, self.lon, e, n)
    }

    /// Position (le long, latéral) d'un point géographique dans le repère piste.
    pub fn to_runway(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (e, n) = enu_offset(self.lat, self.lon, lat, lon);
        let h = self.hdg.to_radians();
        (e * h.sin() + n * h.cos(), e * h.cos() - n * h.sin())
    }

    pub fn runway_number(hdg: f64) -> u32 {
        let n = ((wrap360(hdg) / 10.0).round() as u32) % 36;
        if n == 0 {
            36
        } else {
            n
        }
    }

    /// Choisit l'extrémité de piste selon le côté où se trouve l'avion.
    pub fn approach_for(&self, lat: f64, lon: f64) -> Approach {
        let (along, _) = self.to_runway(lat, lon);
        let (a, hdg) = if along < 0.0 {
            (-self.len / 2.0, self.hdg)
        } else {
            (self.len / 2.0, wrap360(self.hdg + 180.0))
        };
        let (thr_lat, thr_lon) = self.point(a, 0.0);
        Approach { thr_lat, thr_lon, hdg }
    }

    pub fn on_runway(&self, lat: f64, lon: f64) -> bool {
        let (a, c) = self.to_runway(lat, lon);
        a.abs() <= self.len / 2.0 + 60.0 && c.abs() <= self.width / 2.0 + 15.0
    }
}

/// Élévation du sol : relief réel (tuiles de terrain) aplani autour des pistes ; hors ligne, altitude
/// de l'aéroport le plus proche dans un rayon de 40 km, sinon 0.
pub fn ground_elev(lat: f64, lon: f64) -> f64 {
    match crate::tiles::elevation(lat, lon) {
        Some(t) => flatten_airports(lat, lon, t, AIRPORTS.iter()),
        None => fallback_elev(lat, lon),
    }
}

/// Plateforme aplanie à l'altitude de l'aérodrome jusqu'à demi-longueur de piste + 1,5 km,
/// puis raccord progressif au relief sur 3 km.
pub fn flatten_airports<'a>(lat: f64, lon: f64, terrain: f64, apts: impl Iterator<Item = &'a Airport>) -> f64 {
    let mut h = terrain;
    for a in apts {
        if (a.lat - lat).abs() > 0.12 {
            continue;
        }
        let (d, _) = dist_brg(lat, lon, a.lat, a.lon);
        let r0 = a.len / 2.0 + 1_500.0;
        if d < r0 + 3_000.0 {
            let w = ((d - r0) / 3_000.0).clamp(0.0, 1.0);
            let w = w * w * (3.0 - 2.0 * w);
            h = a.elev + (h - a.elev) * w;
        }
    }
    h
}

fn fallback_elev(lat: f64, lon: f64) -> f64 {
    let mut best = (f64::MAX, 0.0);
    for a in AIRPORTS {
        if (a.lat - lat).abs() > 1.0 {
            continue;
        }
        let (d, _) = dist_brg(lat, lon, a.lat, a.lon);
        if d < best.0 {
            best = (d, a.elev);
        }
    }
    let (d, e) = best;
    if d < 40_000.0 {
        e
    } else if d < 80_000.0 {
        e * (1.0 - (d - 40_000.0) / 40_000.0)
    } else {
        0.0
    }
}
