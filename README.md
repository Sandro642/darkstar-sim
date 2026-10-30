# DARKSTAR — station sol d'essais en vol hypersoniques

Tu télépilotes un démonstrateur hypersonique (type SR-72 / Darkstar) depuis une station sol :
vision synthétique 3D, PFD complet, EICAS propulsion TBCC, carte du monde avec trafic et TCAS,
carte d'essais en vol, domaine de vol Mach/altitude.

## Installer
Télécharger `Darkstar-macos.zip` depuis la dernière [release](https://github.com/Sandro642/darkstar-sim/releases/latest),
le dézipper et glisser `Darkstar.app` dans Applications. L'app n'est pas notarisée : au premier lancement,
clic droit > Ouvrir (ou Réglages Système > Confidentialité > « Ouvrir quand même »).
Ensuite le jeu vérifie seul les nouvelles releases et se met à jour (bandeau en haut, ou instruction `MAJ`).
Mac Apple Silicon (arm64), macOS 11+.

## Lancer depuis les sources

    cd ~/Rust/projets/darkstar_sim
    cargo run --release                # départ piste 25 Palmdale, moteurs coupés
    cargo run --release -- autoland    # démo : autoland automatique à Edwards
    cargo run --release -- approche    # finale ILS 10 NM à Edwards (pilotage à la main)
    cargo run --release -- hyper       # croisière Mach 6.2 à 95 000 ft, scramjet allumé

## Pilotage par instructions (ENTRÉE ouvre la console)

    CAP 270 · ALT 90000 / FL350 · ASSIETTE 10 · MACH 6 · KEAS 300 · THR 80
    DIRECT KEDW · ATTERRIR KEDW (autoland) · DECOLLAGE 15000
    TURB START/STOP · INLET OPEN/CLOSE · RAM ARM/IGN/OFF · SCRAM ENABLE/IGN/OFF · H2 DUMP [OFF]
    TRAIN UP/DOWN · FREINS ON/OFF · AP ON/OFF · MANUEL · WARP 8 · AIDE

Autoland : ralliement de l'IAF à 15 NM, capture LOC puis G/S, sortie du train à 9 NM,
Vapp 170 KEAS, arrondi, réduction des gaz, toucher, aérofreins, freinage auto, maintien sur l'axe.
Il faut que les turbines tournent (sous Mach 2.5 : `INLET OPEN` puis `TURB START`).

## Raccourcis
T turbines · I entrées d'air · Y stato · U scram · G train · B freins · S aérofreins
P AP · O auto-manette · N NAV · L ILS · D/K destination · X reprise en main manuelle
(flèches = manche, R/F = manette) · V carte plein écran · E accélération du temps · TAB aide

## Procédure TBCC
Turbines (M0–3.2, PC au-delà de 80 % de manette) → stato : armer à M≥2.0, allumer à M≥2.5
au-dessus du FL400, q 15–95 kPa → couper les turbines et fermer leurs entrées avant M3.2
→ scram : armer à M≥4.0, allumer à M≥4.8, q 25–90 kPa, AoA 0–7° → Mach 10.
Limites : q < 100 kPa, peau < 1250 °C, AoA scram < 8° (sinon UNSTART).

Tests de vol automatiques : `cargo test --release -- --nocapture`
Données carte : Natural Earth (domaine public).

## Écran PFD et télémesure
- PFD au format Darkstar : fenêtre SVS centrale (ciel ardoise, sol vert), EAS à gauche avec crochet de G,
  altitude GPS à droite avec sol hachuré, vario logarithmique, cap en bas, DIST vers la destination.
  Touche H : bascule entre la palette Darkstar et une palette réaliste.
- Panneau THERMIQUE : 22 thermocouples (peau, bords d'attaque, entrée d'air, CIT, EGT, parois de
  combustion stato/scram, tuyère, carburant, avionique, hydraulique, train, freins, batterie).
  Valeur bruitée, tendance °C/min, seuils caution/limite, historique de 2 min, pannes de capteur
  (circuit ouvert). Les dépassements remontent en CAUTION/WARNING.

## Modèle Darkstar
- Deux turbines à PC, statoréacteur (JP-7) et scramjet à **hydrogène liquide** (réservoir séparé de 7 t,
  Isp élevé). `SCRAM ENABLE` pressurise le circuit H2 ; `H2 DUMP` vidange l'hydrogène en urgence.
- EICAS façon Darkstar : nacelles G/D (portes d'entrée d'air en degrés, GEN ON/OFF, RPM %, EGT et
  CIT en °F, flamme de PC), page carburant (FUEL FLOW en PPH, FUEL PRES en psi, CG %, réservoirs JP-7/H2).
- Le profil de test atteint Mach 10 vers T+24 min avec ~2,5 t de H2 restant : l'hydrogène est la ressource critique.

## Sons (synthétisés au démarrage, aucun fichier)
- Moteur : sifflement turbine (ralenti -> plein régime selon N2), grondement du jet, post-combustion
  crépitante, statoréacteur, scramjet, souffle aérodynamique (selon q). Chaque boucle suit l'état réel.
- Alarmes : carillon répétitif MASTER WARNING, chime CAUTION, sirène GPWS « PULL UP », vibreur de
  manche STALL, claquoir de survitesse (q), klaxon de train, tons TCAS (TA puis RA), arpège de
  déconnexion AP, détonation d'unstart scram, impact du toucher, bips d'instruction acceptée/refusée.
- ESPACE acquitte (coupe le carillon), J ou `SON OFF` / `SON ON` coupe ou rétablit le son.

## Interface (v1.1)
- Barre de statut : temps simulé, chrono d'essai, **TEMPS x1..x32** (clic pour changer), liaison, WARNING/CAUTION (clic = acquitter), son, aide.
- Écran Darkstar : PFD (tambours de vitesse/altitude, collines à l'horizon), volet carburant (débit PPH,
  pressions pompes 1-4, réservoirs, CG, bouton DUMP), volet HSI
  (rose des caps, route magenta, portée 10-640 NM, trafic TCAS).
- À droite : carte monde, onglets 4 MOTEURS / 5 THERMIQUE / 6 ESSAIS / 7 DOMAINE / 8 OVERRIDE / 9 CONSOLE, messages.

## Overrides
- `OVERRIDE MOTEUR ON` (ou interrupteur ENGINE OVERRIDE) : manette jusqu'à 120 %, limiteur FADEC
  (peau / pression dynamique) inhibé. Les chambres s'usent en surrégime ; à 0 % : rupture, stato/scram perdus.
- `OVERRIDE FBW ON` : protections d'incidence et de facteur de charge inhibées (unstart, rupture possibles).
- `OVERRIDE OFF` réarme tout. `FUEL DUMP` / `H2 DUMP` : vidanges d'urgence.

## Onglet CONSOLE (v1.2.1) — 9e onglet à droite, bouton MENU du PFD, touche 9 ou instruction `MENU`
- **ENGINE/APU** : interrupteurs L ENG / R ENG (ON / OFF / MOTOR), boutons FADEC RESET (allumés en cas de
  défaut), START (STBY / RDY), STOP (RUN / OFF), ALERT START ENABLE (démarrage rapide 8 s).
  Turbines gauche et droite indépendantes ; un seul moteur = poussée dissymétrique (lacet) à compenser au YAW TRIM.
  En surchauffe (M>3.2) le FADEC coupe les turbines (FADEC RESET requis) ; sous ENGINE OVERRIDE elles sont détruites.
- **FLT CNTRL** : AIR DATA OVRD (au-delà de Mach 5.5 les sondes se dégradent : vitesse bruitée, drapeau ADC ;
  l'override passe en données inertielles INS), INITIATE BIT (test intégré, ON puis FAIL/OK), SYSTEM RESET
  (loi normale, trim centré, mémoire G), bouton YAW TRIM (clic gauche/droite ou molette).
- Instructions équivalentes : `START`, `STOP`, `ENG L ON|OFF|MOTOR`, `FADEC RESET [L|R]`, `ALERT START ON|OFF`,
  `AIR DATA OVRD ON|OFF`, `BIT`, `SYSTEM RESET`, `YAW TRIM -2.5`, `G RESET`.
- PFD : mémoire G MAX / G MIN atteints (repères ambre sur le crochet de G).
- Refroidissement : à 100 % de manette aucun capteur ne dépasse son seuil (EGT régulée ~800 °C,
  refroidissement régénératif H2 du scramjet, régulation thermique FADEC du nez vers 1100 °C).
