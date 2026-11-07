//! Gestion carburant : bingo (carburant de retour vers la destination + approche + réserve), joker,
//! carburant minimal, urgence ; bingo hydrogène pour la phase hypersonique.

use crate::aircraft::*;
use crate::airports::AIRPORTS;
use crate::geo::*;

/// Réserve finale (kg) : carburant minimal à l'atterrissage.
pub const RESERVE: f64 = 3_000.0;
/// Approche et atterrissage (kg).
const APPROACH: f64 = 1_500.0;
/// Marge du joker au-dessus du bingo (kg).
const JOKER_MARGIN: f64 = 4_000.0;
const EMERGENCY: f64 = 1_500.0;
/// Consommation de planification au retour (kg par mètre sol) : croisière subsonique.
const PLAN_BURN_PER_M: f64 = 1.6 / 250.0;

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default)]
pub enum Level {
    #[default]
    Ok,
    Joker,
    Bingo,
    MinFuel,
    Emergency,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Ok => "",
            Level::Joker => "JOKER",
            Level::Bingo => "BINGO FUEL",
            Level::MinFuel => "MIN FUEL",
            Level::Emergency => "EMERGENCY FUEL",
        }
    }
}

#[derive(Clone, Default)]
pub struct Plan {
    pub level: Level,
    pub bingo: f64,
    pub joker: f64,
    pub manual: bool,
    /// Retour vers la destination : (OACI, relèvement, distance m, durée s, carburant nécessaire kg).
    pub rtb: (&'static str, f64, f64, f64, f64),
    /// Hydrogène insuffisant pour finir la phase hypersonique.
    pub h2_bingo: bool,
    /// Ravitailleur disponible : (indicatif, distance m).
    pub tanker: Option<(&'static str, f64)>,
}

/// Consommation de JP-7 par mètre sol : régime actuel s'il est représentatif (subsonique), sinon
/// croisière de planification.
pub fn burn_per_m(ac: &Aircraft) -> f64 {
    let (gs, ff) = (ac.ground_speed(), ac.eng.fuel_flow);
    if ff > 0.05 && gs > 100.0 && ac.d.mach < 1.2 { (ff / gs).clamp(PLAN_BURN_PER_M * 0.5, PLAN_BURN_PER_M * 3.0) } else { PLAN_BURN_PER_M }
}

/// Calcule le plan carburant. `manual` : bingo imposé (kg).
pub fn plan(ac: &Aircraft, dest: usize, manual: Option<f64>, tanker: Option<(&'static str, f64, f64)>) -> Plan {
    let apt = &AIRPORTS[dest];
    let (dist, brg) = dist_brg(ac.lat, ac.lon, apt.lat, apt.lon);
    let gs = ac.ground_speed();
    let per_m = burn_per_m(ac);
    let need = dist * per_m;
    let auto_bingo = need + APPROACH + RESERVE;
    let bingo = manual.unwrap_or(auto_bingo);
    let joker = bingo + JOKER_MARGIN;
    let level = if ac.fuel <= EMERGENCY {
        Level::Emergency
    } else if ac.fuel <= RESERVE {
        Level::MinFuel
    } else if ac.fuel <= bingo {
        Level::Bingo
    } else if ac.fuel <= joker {
        Level::Joker
    } else {
        Level::Ok
    };
    let eta = dist / gs.max(150.0);
    // H2 : en stato/scram, l'hydrogène doit couvrir au moins 3 minutes de décélération
    let h2_bingo = ac.eng.h2_flow > 0.01 && ac.h2 / ac.eng.h2_flow < 180.0;
    Plan {
        level,
        bingo,
        joker,
        manual: manual.is_some(),
        rtb: (apt.icao, brg, dist, eta, need + APPROACH),
        h2_bingo,
        tanker: tanker.map(|t| (t.0, dist_brg(ac.lat, ac.lon, t.1, t.2).0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bingo_levels() {
        let edw = AIRPORTS.iter().position(|a| a.icao == "KEDW").unwrap();
        let mut ac = Aircraft::new_on_runway(0);
        ac.on_ground = false;
        let (la, lo) = destination(AIRPORTS[edw].lat, AIRPORTS[edw].lon, 90.0, 300.0 * NM);
        ac.lat = la;
        ac.lon = lo;
        ac.tas = 250.0;
        ac.fuel = 30_000.0;
        let p = plan(&ac, edw, None, None);
        println!("bingo a 300 NM : {:.1} t", p.bingo / 1000.0);
        assert_eq!(p.level, Level::Ok);
        ac.fuel = p.bingo - 100.0;
        assert_eq!(plan(&ac, edw, None, None).level, Level::Bingo);
        ac.fuel = 2_000.0;
        assert_eq!(plan(&ac, edw, None, None).level, Level::MinFuel);
        assert_eq!(plan(&ac, edw, Some(25_000.0), None).bingo, 25_000.0);
    }
}
