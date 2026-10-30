//! Atmosphère standard internationale (ISA / US Standard 1976) jusqu'à ~86 km.

pub const RHO0: f64 = 1.225;
const R_AIR: f64 = 287.053;
const G0M_R: f64 = 0.034_163_2; // g0 * M / R  (K/m)

// (altitude de base m, gradient K/m, T de base K, P de base Pa)
const LAYERS: [(f64, f64, f64, f64); 7] = [
    (0.0, -0.0065, 288.15, 101_325.0),
    (11_000.0, 0.0, 216.65, 22_632.1),
    (20_000.0, 0.001, 216.65, 5_474.89),
    (32_000.0, 0.0028, 228.65, 868.019),
    (47_000.0, 0.0, 270.65, 110.906),
    (51_000.0, -0.0028, 270.65, 66.9389),
    (71_000.0, -0.002, 214.65, 3.956_42),
];

#[derive(Clone, Copy, Debug)]
pub struct Air {
    pub t: f64,   // K
    pub p: f64,   // Pa
    pub rho: f64, // kg/m3
    pub a: f64,   // vitesse du son m/s
}

pub fn isa(h: f64) -> Air {
    let h = h.clamp(-500.0, 120_000.0);
    let mut i = 0;
    while i + 1 < LAYERS.len() && h >= LAYERS[i + 1].0 {
        i += 1;
    }
    let (hb, l, tb, pb) = LAYERS[i];
    let dh = h - hb;
    let (t, p) = if l == 0.0 {
        (tb, pb * (-G0M_R * dh / tb).exp())
    } else {
        let t = (tb + l * dh).max(186.0);
        (t, pb * (tb / t).powf(G0M_R / l))
    };
    Air {
        t,
        p,
        rho: p / (R_AIR * t),
        a: (1.4 * R_AIR * t).sqrt(),
    }
}

/// Altitude à laquelle la pression vaut p (recherche dichotomique).
pub fn altitude_for_pressure(p: f64) -> f64 {
    let (mut lo, mut hi) = (-500.0, 100_000.0);
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if isa(mid).p > p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Altitude où la pression dynamique vaut q (Pa) à un Mach donné : q = 0.7 * P * M^2.
pub fn altitude_for_q(q: f64, mach: f64) -> f64 {
    altitude_for_pressure(q / (0.7 * mach * mach))
}
