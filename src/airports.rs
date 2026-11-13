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
    // --- Amérique du Nord ---
    apt!("KORD", "CHICAGO O'HARE", 41.9786, -87.9048, 90.0, 3962.0, 60.0, 204.0),
    apt!("KATL", "ATLANTA", 33.6367, -84.4281, 90.0, 3776.0, 46.0, 313.0),
    apt!("KDFW", "DALLAS FORT WORTH", 32.8998, -97.0403, 175.0, 4085.0, 61.0, 185.0),
    apt!("KIAH", "HOUSTON", 29.9902, -95.3368, 150.0, 3658.0, 46.0, 30.0),
    apt!("KMIA", "MIAMI", 25.7959, -80.2870, 90.0, 3202.0, 61.0, 3.0),
    apt!("KSEA", "SEATTLE", 47.4502, -122.3088, 180.0, 2873.0, 46.0, 132.0),
    apt!("KPHX", "PHOENIX", 33.4342, -112.0116, 90.0, 3502.0, 46.0, 345.0),
    apt!("KSLC", "SALT LAKE CITY", 40.7884, -111.9778, 170.0, 3658.0, 46.0, 1288.0),
    apt!("KBOS", "BOSTON", 42.3656, -71.0096, 40.0, 3073.0, 46.0, 6.0),
    apt!("KIAD", "WASHINGTON DULLES", 38.9531, -77.4565, 10.0, 3505.0, 46.0, 95.0),
    apt!("KMCO", "ORLANDO", 28.4312, -81.3081, 180.0, 3659.0, 61.0, 30.0),
    apt!("KSAN", "SAN DIEGO", 32.7338, -117.1933, 92.0, 2865.0, 61.0, 5.0),
    apt!("KMSP", "MINNEAPOLIS", 44.8848, -93.2223, 120.0, 3048.0, 61.0, 256.0),
    apt!("KDTW", "DETROIT", 42.2124, -83.3534, 40.0, 3659.0, 61.0, 196.0),
    apt!("KVBG", "VANDENBERG SFB", 34.7294, -120.5767, 150.0, 4573.0, 61.0, 112.0),
    apt!("KBAB", "BEALE AFB", 39.1361, -121.4366, 150.0, 3658.0, 91.0, 34.0),
    // Mesuré sur l'imagerie satellite : axe à 158,7° vrai (14L/32R magnétique)
    apt!("KXTA", "HOMEY AIRPORT (GROOM LAKE)", 37.2333, -115.7920, 158.7, 3658.0, 46.0, 1360.0),
    apt!("KTNX", "TONOPAH TEST RANGE", 37.7947, -116.7787, 140.0, 3658.0, 46.0, 1650.0),
    apt!("CYVR", "VANCOUVER", 49.1967, -123.1815, 100.0, 3505.0, 61.0, 4.0),
    apt!("CYUL", "MONTREAL", 45.4706, -73.7408, 60.0, 3353.0, 61.0, 36.0),
    apt!("MMMX", "MEXICO", 19.4363, -99.0721, 50.0, 3952.0, 45.0, 2230.0),
    apt!("MPTO", "PANAMA TOCUMEN", 9.0714, -79.3835, 30.0, 3050.0, 45.0, 41.0),
    // --- Amérique du Sud, Pacifique Est ---
    apt!("SKBO", "BOGOTA", 4.7016, -74.1469, 130.0, 3800.0, 45.0, 2548.0),
    apt!("SPJC", "LIMA", -12.0219, -77.1143, 160.0, 3507.0, 45.0, 34.0),
    apt!("SAEZ", "BUENOS AIRES EZEIZA", -34.8222, -58.5358, 110.0, 3300.0, 60.0, 20.0),
    apt!("SBGL", "RIO DE JANEIRO GALEAO", -22.8100, -43.2506, 100.0, 4000.0, 45.0, 9.0),
    apt!("SCIP", "ILE DE PAQUES", -27.1648, -109.4219, 100.0, 3318.0, 45.0, 69.0),
    // --- Europe ---
    apt!("EHAM", "AMSTERDAM SCHIPHOL", 52.3105, 4.7683, 180.0, 3800.0, 60.0, -3.0),
    apt!("EBBR", "BRUXELLES", 50.9014, 4.4844, 70.0, 3638.0, 45.0, 56.0),
    apt!("LSZH", "ZURICH", 47.4582, 8.5555, 160.0, 3700.0, 60.0, 432.0),
    apt!("LIRF", "ROME FIUMICINO", 41.8003, 12.2389, 160.0, 3900.0, 60.0, 5.0),
    apt!("LEBL", "BARCELONE", 41.2974, 2.0833, 70.0, 3352.0, 45.0, 4.0),
    apt!("LPPT", "LISBONNE", 38.7813, -9.1359, 30.0, 3805.0, 45.0, 114.0),
    apt!("EGVN", "RAF BRIZE NORTON", 51.7500, -1.5836, 70.0, 3050.0, 61.0, 88.0),
    apt!("EKCH", "COPENHAGUE", 55.6180, 12.6560, 40.0, 3600.0, 45.0, 5.0),
    apt!("ENGM", "OSLO GARDERMOEN", 60.1939, 11.1004, 10.0, 3600.0, 45.0, 208.0),
    apt!("ESSA", "STOCKHOLM ARLANDA", 59.6519, 17.9186, 10.0, 3301.0, 45.0, 42.0),
    apt!("EFHK", "HELSINKI", 60.3172, 24.9633, 40.0, 3500.0, 60.0, 55.0),
    apt!("EPWA", "VARSOVIE", 52.1657, 20.9671, 110.0, 3690.0, 60.0, 110.0),
    apt!("LOWW", "VIENNE", 48.1103, 16.5697, 110.0, 3500.0, 45.0, 183.0),
    apt!("LGAV", "ATHENES", 37.9364, 23.9445, 30.0, 4000.0, 45.0, 94.0),
    apt!("LTFM", "ISTANBUL", 41.2753, 28.7519, 170.0, 4100.0, 60.0, 99.0),
    apt!("LFML", "MARSEILLE PROVENCE", 43.4393, 5.2214, 130.0, 3500.0, 45.0, 23.0),
    apt!("LFLL", "LYON SAINT-EXUPERY", 45.7256, 5.0811, 180.0, 4000.0, 45.0, 250.0),
    apt!("UUEE", "MOSCOU SHEREMETYEVO", 55.9726, 37.4146, 60.0, 3700.0, 60.0, 192.0),
    // --- Moyen-Orient, Afrique ---
    apt!("OERK", "RIYAD", 24.9576, 46.6988, 150.0, 4205.0, 60.0, 625.0),
    apt!("OTHH", "DOHA HAMAD", 25.2731, 51.6081, 160.0, 4850.0, 60.0, 4.0),
    apt!("LLBG", "TEL AVIV", 32.0114, 34.8867, 120.0, 3112.0, 45.0, 41.0),
    apt!("HECA", "LE CAIRE", 30.1219, 31.4056, 50.0, 4000.0, 60.0, 116.0),
    apt!("GMMN", "CASABLANCA", 33.3675, -7.5900, 170.0, 3720.0, 45.0, 200.0),
    apt!("DNMM", "LAGOS", 6.5774, 3.3212, 180.0, 3900.0, 60.0, 41.0),
    apt!("HKJK", "NAIROBI", -1.3192, 36.9278, 60.0, 4117.0, 45.0, 1624.0),
    apt!("FACT", "LE CAP", -33.9648, 18.6017, 10.0, 3201.0, 61.0, 46.0),
    // --- Asie, Océanie, Pacifique ---
    apt!("VIDP", "DELHI", 28.5562, 77.1000, 110.0, 4430.0, 60.0, 237.0),
    apt!("VABB", "MUMBAI", 19.0896, 72.8656, 90.0, 3448.0, 60.0, 11.0),
    apt!("WSSS", "SINGAPOUR CHANGI", 1.3644, 103.9915, 20.0, 4000.0, 60.0, 7.0),
    apt!("VTBS", "BANGKOK", 13.6900, 100.7501, 10.0, 4000.0, 60.0, 2.0),
    apt!("RKSI", "SEOUL INCHEON", 37.4602, 126.4407, 150.0, 3750.0, 60.0, 7.0),
    apt!("ZBAA", "PEKIN CAPITAL", 40.0799, 116.6031, 180.0, 3800.0, 60.0, 35.0),
    apt!("ZSPD", "SHANGHAI PUDONG", 31.1443, 121.8083, 170.0, 4000.0, 60.0, 4.0),
    apt!("RJAA", "TOKYO NARITA", 35.7720, 140.3929, 160.0, 4000.0, 60.0, 43.0),
    apt!("RCTP", "TAIPEI", 25.0797, 121.2342, 50.0, 3660.0, 60.0, 33.0),
    apt!("RPLL", "MANILLE", 14.5086, 121.0198, 60.0, 3410.0, 60.0, 23.0),
    apt!("WIII", "JAKARTA", -6.1256, 106.6559, 70.0, 3660.0, 60.0, 10.0),
    apt!("YMML", "MELBOURNE", -37.6690, 144.8410, 160.0, 3657.0, 60.0, 132.0),
    apt!("YPPH", "PERTH", -31.9403, 115.9669, 30.0, 3444.0, 45.0, 20.0),
    apt!("NZAA", "AUCKLAND", -37.0081, 174.7917, 50.0, 3635.0, 45.0, 7.0),
    apt!("NFFN", "NADI (FIDJI)", -17.7554, 177.4431, 20.0, 3273.0, 45.0, 18.0),
    apt!("PWAK", "ILE DE WAKE", 19.2821, 166.6364, 100.0, 2988.0, 46.0, 5.0),
];

/// Pistes supplémentaires (la piste principale est celle de `AIRPORTS`) :
/// (OACI, lat, lon du centre, cap vrai du sens « bas », longueur, largeur). Coordonnées approchées.
pub const EXTRA_RWY: &[(&str, f64, f64, f64, f64, f64)] = &[
    ("KXTA", 37.2314, -115.7983, 158.7, 3048.0, 46.0),
    ("KEDW", 34.8960, -117.8990, 51.0, 2438.0, 46.0),
    ("KEDW", 34.9300, -117.8250, 170.0, 7000.0, 90.0), // lac sec Rogers 17/35
    ("KPMD", 34.6330, -118.0740, 40.0, 3658.0, 46.0),
    ("KNID", 35.6810, -117.6830, 140.0, 2700.0, 46.0),
    ("KLAX", 33.9490, -118.4180, 83.0, 2720.0, 46.0),
    ("KLAX", 33.9365, -118.4180, 83.0, 3382.0, 61.0),
    ("KSFO", 37.6160, -122.3800, 118.0, 3231.0, 61.0),
    ("KSFO", 37.6140, -122.3720, 28.0, 2286.0, 61.0),
    ("KSFO", 37.6110, -122.3800, 28.0, 2637.0, 61.0),
    ("KLAS", 36.0820, -115.1640, 20.0, 2739.0, 46.0),
    ("KLAS", 36.0850, -115.1580, 20.0, 2979.0, 46.0),
    ("KLAS", 36.0790, -115.1440, 77.0, 3208.0, 46.0),
    ("KDEN", 39.8400, -104.6400, 90.0, 3658.0, 46.0),
    ("KDEN", 39.8780, -104.6620, 90.0, 3658.0, 46.0),
    ("KDEN", 39.8600, -104.6460, 0.0, 3658.0, 46.0),
    ("KJFK", 40.6480, -73.7880, 121.0, 4442.0, 61.0),
    ("KJFK", 40.6390, -73.7650, 31.0, 2560.0, 46.0),
    ("KJFK", 40.6620, -73.7900, 121.0, 3048.0, 46.0),
    ("KORD", 41.9840, -87.8970, 90.0, 3292.0, 46.0),
    ("KORD", 41.9690, -87.8880, 40.0, 2286.0, 46.0),
    ("KORD", 41.9630, -87.9050, 90.0, 3048.0, 61.0),
    ("KATL", 33.6470, -84.4380, 90.0, 2743.0, 46.0),
    ("KATL", 33.6280, -84.4200, 90.0, 2743.0, 46.0),
    ("KATL", 33.6340, -84.4180, 90.0, 2743.0, 46.0),
    ("KDFW", 32.8950, -97.0230, 175.0, 2591.0, 46.0),
    ("KDFW", 32.9000, -97.0540, 175.0, 4085.0, 61.0),
    ("KDFW", 32.8850, -97.0570, 130.0, 2835.0, 61.0),
    ("KMIA", 25.7990, -80.2860, 90.0, 2621.0, 46.0),
    ("KMIA", 25.7900, -80.2950, 120.0, 2896.0, 61.0),
    ("KSEA", 47.4500, -122.3040, 180.0, 3627.0, 46.0),
    ("KSEA", 47.4500, -122.3170, 180.0, 2591.0, 46.0),
    ("KBOS", 42.3640, -71.0090, 150.0, 3050.0, 46.0),
    ("KBOS", 42.3600, -71.0070, 90.0, 2134.0, 46.0),
    ("KIAD", 38.9400, -77.4700, 10.0, 3505.0, 46.0),
    ("KIAD", 38.9400, -77.4420, 120.0, 3200.0, 46.0),
    ("KIAH", 29.9850, -95.3200, 150.0, 3658.0, 46.0),
    ("KIAH", 29.9930, -95.3500, 90.0, 2866.0, 46.0),
    ("KMCO", 28.4310, -81.2950, 180.0, 3659.0, 46.0),
    ("KPHX", 33.4290, -112.0100, 90.0, 3139.0, 46.0),
    ("KSLC", 40.7880, -111.9910, 170.0, 3658.0, 46.0),
    ("KMSP", 44.8780, -93.2090, 120.0, 3355.0, 61.0),
    ("KMSP", 44.8800, -93.2330, 170.0, 2438.0, 46.0),
    ("PANC", 61.1880, -149.9970, 150.0, 3200.0, 46.0),
    ("PANC", 61.1700, -149.9830, 70.0, 3231.0, 61.0),
    ("PHNL", 21.3060, -157.9200, 78.0, 3749.0, 61.0),
    ("PHNL", 21.3220, -157.9300, 40.0, 2743.0, 46.0),
    ("CYYZ", 43.6650, -79.6150, 53.0, 2956.0, 61.0),
    ("CYYZ", 43.6860, -79.6250, 150.0, 3389.0, 61.0),
    ("CYVR", 49.1900, -123.1830, 100.0, 3030.0, 61.0),
    ("EGLL", 51.4648, -0.4560, 90.0, 3660.0, 50.0),
    ("LFPG", 49.0240, 2.5260, 86.0, 2700.0, 45.0),
    ("LFPG", 48.9960, 2.5580, 86.0, 2700.0, 45.0),
    ("LFPG", 49.0250, 2.5650, 86.0, 4215.0, 45.0),
    ("EDDF", 50.0330, 8.5870, 70.0, 4000.0, 60.0),
    ("EDDF", 50.0450, 8.5450, 70.0, 2800.0, 45.0),
    ("EDDF", 50.0350, 8.5270, 180.0, 4000.0, 45.0),
    ("EHAM", 52.3040, 4.7790, 58.0, 3500.0, 45.0),
    ("EHAM", 52.3170, 4.7700, 87.0, 3453.0, 45.0),
    ("EHAM", 52.3130, 4.7380, 183.0, 3300.0, 45.0),
    ("LEMD", 40.4900, -3.5600, 143.0, 3500.0, 60.0),
    ("LEMD", 40.5050, -3.5620, 180.0, 4350.0, 60.0),
    ("LFBO", 43.6350, 1.3700, 143.0, 3000.0, 45.0),
    ("LEBL", 41.2920, 2.0650, 70.0, 2660.0, 60.0),
    ("LIRF", 41.8300, 12.2700, 160.0, 3900.0, 60.0),
    ("LIRF", 41.7950, 12.2650, 70.0, 3309.0, 45.0),
    ("EKCH", 55.6260, 12.6440, 120.0, 2800.0, 45.0),
    ("ENGM", 60.1910, 11.0750, 10.0, 2950.0, 45.0),
    ("ESSA", 59.6550, 17.9280, 10.0, 2500.0, 45.0),
    ("ESSA", 59.6450, 17.9450, 80.0, 2500.0, 45.0),
    ("LTFM", 41.2670, 28.7300, 170.0, 4100.0, 60.0),
    ("LTFM", 41.2550, 28.7700, 170.0, 3750.0, 60.0),
    ("UUEE", 55.9650, 37.4150, 60.0, 3550.0, 60.0),
    ("OMDB", 25.2480, 55.3550, 120.0, 4447.0, 60.0),
    ("OTHH", 25.2750, 51.6350, 160.0, 4250.0, 60.0),
    ("OERK", 24.9450, 46.7230, 150.0, 4205.0, 60.0),
    ("VHHH", 22.2960, 113.9250, 73.0, 3800.0, 60.0),
    ("VHHH", 22.3220, 113.9100, 73.0, 3800.0, 60.0),
    ("RJTT", 35.5560, 139.7780, 40.0, 2500.0, 60.0),
    ("RJTT", 35.5380, 139.8000, 50.0, 2500.0, 60.0),
    ("RJTT", 35.5530, 139.7720, 157.0, 3000.0, 60.0),
    ("RJAA", 35.7920, 140.3810, 160.0, 2500.0, 60.0),
    ("RKSI", 37.4700, 126.4300, 150.0, 3750.0, 60.0),
    ("RKSI", 37.4500, 126.4500, 150.0, 4000.0, 60.0),
    ("ZBAA", 40.0700, 116.6200, 180.0, 3800.0, 60.0),
    ("ZBAA", 40.0900, 116.5900, 180.0, 3200.0, 50.0),
    ("ZSPD", 31.1600, 121.7950, 170.0, 3800.0, 60.0),
    ("WSSS", 1.3560, 103.9850, 20.0, 4000.0, 60.0),
    ("VTBS", 13.6850, 100.7650, 10.0, 3700.0, 60.0),
    ("VIDP", 28.5700, 77.0900, 100.0, 3813.0, 46.0),
    ("VIDP", 28.5650, 77.1150, 100.0, 4400.0, 60.0),
    ("YSSY", -33.9480, 151.1800, 69.0, 2530.0, 45.0),
    ("YSSY", -33.9520, 151.1880, 155.0, 2438.0, 45.0),
    ("YMML", -37.6650, 144.8300, 90.0, 2286.0, 45.0),
    ("NZAA", -37.0050, 174.8000, 50.0, 3108.0, 45.0),
    ("SBGR", -23.4300, -46.4650, 95.0, 3000.0, 45.0),
    ("SAEZ", -34.8150, -58.5450, 170.0, 3105.0, 45.0),
    ("FAOR", -26.1300, 28.2350, 29.0, 3400.0, 60.0),
    ("HECA", 30.1100, 31.3950, 50.0, 3300.0, 60.0),
    ("HECA", 30.1300, 31.4200, 160.0, 3300.0, 60.0),
    ("MMMX", 19.4350, -99.0800, 50.0, 3900.0, 45.0),
];

/// Une piste (centre, cap du sens « bas », dimensions).
#[derive(Clone, Copy, Debug)]
pub struct Rwy {
    pub lat: f64,
    pub lon: f64,
    pub hdg: f64,
    pub len: f64,
    pub width: f64,
}

impl Rwy {
    pub fn point(&self, along: f64, cross: f64) -> (f64, f64) {
        let h = self.hdg.to_radians();
        let e = along * h.sin() + cross * h.cos();
        let n = along * h.cos() - cross * h.sin();
        from_enu(self.lat, self.lon, e, n)
    }
    pub fn to_runway(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (e, n) = enu_offset(self.lat, self.lon, lat, lon);
        let h = self.hdg.to_radians();
        (e * h.sin() + n * h.cos(), e * h.cos() - n * h.sin())
    }
    pub fn on(&self, lat: f64, lon: f64) -> bool {
        let (a, c) = self.to_runway(lat, lon);
        a.abs() <= self.len / 2.0 + 60.0 && c.abs() <= self.width / 2.0 + 15.0
    }
    /// Seuil d'atterrissage dans le sens `hdg` (cap d'approche).
    pub fn threshold(&self, hdg: f64) -> (f64, f64) {
        let low = wrap180(hdg - self.hdg).abs() < 90.0;
        self.point(if low { -self.len / 2.0 } else { self.len / 2.0 }, 0.0)
    }
}

/// Seuil d'approche choisi en fonction de la position de l'avion : (lat, lon, cap d'approche).
pub struct Approach {
    pub thr_lat: f64,
    pub thr_lon: f64,
    pub hdg: f64,
}

impl Airport {
    /// Toutes les pistes de l'aérodrome (principale en premier).
    pub fn runways(&self) -> Vec<Rwy> {
        let mut v = vec![Rwy { lat: self.lat, lon: self.lon, hdg: self.hdg, len: self.len, width: self.width }];
        v.extend(EXTRA_RWY.iter().filter(|r| r.0 == self.icao).map(|r| Rwy { lat: r.1, lon: r.2, hdg: r.3, len: r.4, width: r.5 }));
        // Cap vrai exact des pistes d'après l'AIRAC de l'OFP SimBrief (départ / arrivée), à ±20°
        for r in v.iter_mut() {
            if let Some(tc) = crate::procs::airac_course(self.icao, r.hdg) {
                r.hdg = tc;
            }
        }
        v
    }

    /// Désignation de l'extrémité de piste (« 28L ») pour la piste `k` et le cap d'approche `hdg` :
    /// numéro, puis gauche / centre / droite parmi les pistes parallèles vues dans ce sens.
    pub fn runway_ident(&self, k: usize, hdg: f64) -> String {
        let rws = self.runways();
        let num = Airport::runway_number(hdg);
        let par: Vec<usize> = (0..rws.len()).filter(|&i| Airport::runway_number(if wrap180(hdg - rws[i].hdg).abs() < 90.0 { rws[i].hdg } else { wrap360(rws[i].hdg + 180.0) }) == num).collect();
        if par.len() < 2 {
            return format!("{num:02}");
        }
        // Position latérale (à droite > 0) vue dans le sens d'approche
        let h = hdg.to_radians();
        let side = |i: usize| {
            let (e, n) = enu_offset(self.lat, self.lon, rws[i].lat, rws[i].lon);
            e * h.cos() - n * h.sin()
        };
        let mut sorted = par.clone();
        sorted.sort_by(|a, b| side(*a).partial_cmp(&side(*b)).unwrap());
        let pos = sorted.iter().position(|&i| i == k).unwrap_or(0);
        let letter = if pos == 0 { "L" } else if pos + 1 == sorted.len() { "R" } else { "C" };
        format!("{num:02}{letter}")
    }

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
        // Piste dont l'axe prolongé passe au plus près de l'avion
        let rws = self.runways();
        if rws.len() > 1 {
            let k = (0..rws.len()).min_by(|&a, &b| rws[a].to_runway(lat, lon).1.abs().partial_cmp(&rws[b].to_runway(lat, lon).1.abs()).unwrap()).unwrap();
            let r = rws[k];
            let (along, _) = r.to_runway(lat, lon);
            let hdg = if along < 0.0 { r.hdg } else { wrap360(r.hdg + 180.0) };
            let (thr_lat, thr_lon) = r.threshold(hdg);
            return Approach { thr_lat, thr_lon, hdg };
        }
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
        self.runways().iter().any(|r| r.on(lat, lon))
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
