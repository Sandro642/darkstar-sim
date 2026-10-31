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
    DEST KEDW (choisir la destination ; DEST seul = liste) · DIRECT KEDW · ATTERRIR KEDW (autoland) · DECOLLAGE 15000
    TURB START/STOP · INLET OPEN/CLOSE · RAM ARM/IGN/OFF · SCRAM ENABLE/IGN/OFF · H2 DUMP [OFF]
    TRAIN UP/DOWN · FREINS ON/OFF · AP ON/OFF · MANUEL · WARP 8 · AIDE

Autoland : ralliement de l'IAF à 15 NM (attente en orbite si l'avion arrive trop haut ou trop vite),
capture LOC puis G/S, remise des gaz automatique si l'approche n'est pas stabilisée, sortie du train à 9 NM,
Vapp 170 KEAS, arrondi, réduction des gaz, toucher, aérofreins, freinage auto, maintien sur l'axe.
Il faut que les turbines tournent (sous Mach 2.5 : `INLET OPEN` puis `TURB START`).

## Raccourcis
T turbines · I entrées d'air · Y stato · U scram · G train · B freins · S aérofreins
P AP · O auto-manette · N NAV · L ILS · X reprise en main manuelle
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
  Vision synthétique en couleurs réalistes (imagerie satellite).
- Panneau THERMIQUE : 22 thermocouples (peau, bords d'attaque, entrée d'air, CIT, EGT, parois de
  combustion stato/scram, tuyère, carburant, avionique, hydraulique, train, freins, batterie).
  Valeur bruitée, tendance °C/min, seuils caution/limite, historique de 2 min, pannes de capteur
  (circuit ouvert). Les dépassements remontent en CAUTION/WARNING.

## Modèle Darkstar
- Deux turbines à PC, statoréacteur (JP-7, 38 t) et scramjet à **hydrogène liquide** (réservoir séparé de 16 t,
  Isp élevé). `SCRAM ENABLE` pressurise le circuit H2 ; `H2 DUMP` vidange l'hydrogène en urgence.
- EICAS façon Darkstar : nacelles G/D (portes d'entrée d'air en degrés, GEN ON/OFF, RPM %, EGT et
  CIT en °F, flamme de PC), page carburant (FUEL FLOW en PPH, FUEL PRES en psi, CG %, réservoirs JP-7/H2).
- Le profil de test atteint Mach 10 vers T+32 min avec ~21 t de JP-7 et ~8,6 t de H2 restants (~19 min de croisière à Mach 10).

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
- `OVERRIDE MOTEUR ON` (ou interrupteur ENGINE OVERRIDE) : FADEC inhibé et **manette sans butée**. La limite
  est celle du moteur : la poussée sature (~137 %) quand le mélange devient stœchiométrique ; au-delà, le
  carburant en excès ne fait que chauffer et user les chambres. À 0 % : rupture, stato/scram perdus.
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

## Train d'atterrissage
- Trois voyants nez / gauche / droite (bloc TRAIN dans la colonne de droite, à côté des messages, et onglet MOTEURS) :
  vert = sorti et verrouillé, rouge = en transit, éteint = rentré, rouge clignotant = train rentré trop bas.

## Descente depuis l'hypersonique (v1.2.6)
- Plonger fait exploser la pression dynamique q (limite structurale 100 kPa) : l'avion garde sa vitesse
  alors que l'air devient dense. Instruction **`DESCENTE [kPa]`** : descente à q constant (50 kPa par défaut),
  manette réduite ; l'avion décélère en descendant puis se met en palier automatiquement en subsonique sous le FL350.
- Protections (désactivées par FBW OVERRIDE) : le pilote auto limite sa pente quand q approche la limite ;
  la loi de vol limite l'angle de piqué selon le Mach (~-4° à M6, -10° à M2) et cabre automatiquement si la q
  prévue à 8 s dépasse 80 kPa. Aérofreins plus efficaces en supersonique.

## Capteurs thermiques (v1.2.7)
- `CAPTEUR 101 OFF` inhibe un capteur par son numéro (affiché dans la liste, ex. 101 = nez) : il n'alarme plus
  et apparaît « INHIB ». `CAPTEUR 101 ON` le réactive, `CAPTEURS ON` réactive tout.
- Page THERMIQUE : clic sur un point du schéma = sa ligne est surlignée dans la liste et une étiquette
  (numéro, nom, valeur) s'affiche ; clic sur une ligne = le point clignote sur le schéma. Re-clic = désélection.

## Auto-manette : vitesse gérée / sélectionnée (v1.2.9)
- **Sélectionnée** : `MACH 2.7` ou `KEAS 300` fixent une consigne (FMA « A/THR 300KEAS »).
- **Gérée** (FMA « A/THR 210 AUTO ») : `VITESSE AUTO`, automatique à l'armement ILS (touche L) et après `DECOLLAGE`.
  Approche ILS : 250 KEAS au-delà de 12 NM, ~210 entre 12 et 7 NM, puis vitesse d'approche selon la masse ;
  aérofreins automatiques si trop rapide, RETARD (manette réduite, A/THR off) à l'arrondi.
  Hors approche : 250 KEAS sous 10 000 ft sol, 350 KEAS au-dessus (plafond Mach 0.9).

## Enregistreur de vol (v1.2.11)
- Chaque vol est enregistré (trace 2 Hz + journal) dans `~/Library/Application Support/Darkstar/vols/`,
  sauvegardé toutes les 30 s. Au crash : analyse automatique (cause, état au crash, 60 dernières secondes,
  diagnostic et recommandations) affichée sur le PFD et écrite dans `<date>_CRASH.txt`.
- Instructions : `FDR` (enregistrer maintenant), `ANALYSE` (analyse du vol ou du dernier crash).

## ILS manuel (v1.2.12)
- Touche L : l'ILS est **armé** (FMA « LOC* » / « G/S* »), le pilote auto garde son cap et son altitude.
  LOC capturé dans l'axe, devant la piste, à moins de 25 NM ; G/S capturé en arrivant par en dessous du plan.
  Sens d'approche figé dès l'armement. Au-delà de la mi-piste sans toucher : remise des gaz automatique.
- Trop haut ou trop rapide à l'armement : avertissement ; descendre d'abord (DESCENTE / ALT) ou utiliser ATTERRIR.

## Fin de vol, page FCS, infos éphémères (v1.2.13)
- `FIN` : termine le vol (simulation figée), sauvegarde l'enregistrement et affiche un débriefing
  (durée, distance, Mach/altitude max, q, peau, g, carburant consommé, points d'essai, atterrissage, issue).
  ENTRÉE : nouveau vol. `QUITTER` : sauvegarde et ferme l'application.
- Onglet **0 FCS** : loi de vol (normale / directe sous FBW OVERRIDE), protections (incidence, g, haute
  vitesse, piqué maxi, entrée d'air scram, inclinaison, données air) avec état ACTIVE / LIMITATION / INHIBÉE,
  jauges valeur/limites, manche (ordre pilote vs loi FCS), élevons et dérives.
- Infos éphémères sur l'horizon (~3,5 s) : instructions collationnées, captures LOC/G/S, points d'essai,
  allumages, décollage/toucher, remises des gaz, passages de Mach, annonces radio-altimètre (2500…10 ft).

## Simulateur d'essai avancé (v1.3.0)
- **Écran Darkstar aux proportions du cockpit** : 6 boutons carrés au-dessus (ils sélectionnent les pages
  MOTEURS, ALARMES, FCS, LIMITES, ÉLEC/ENV, THERMIQUE ; bouton allumé = page affichée), voyant, PFD en haut
  et trois volets FUEL / caméra (« Camera unavailable » + coordonnées) / HSI.
- **10 pages** sur deux rangées d'onglets (touches 4-9 et 0 pour les 7 premières).
- **ALARMES** : panneau de 32 annonciateurs (ambre = caution, rouge clignotant = warning) et injection de
  pannes : `PANNE ENGL|ENGR|GENL|GENR|POMPE|ADC|FCS|HYD|REFROID|PORTES|TRAIN|FUITEH2 [OFF]`, `PANNES OFF`.
  Train bloqué → `TRAIN SECOURS`.
- **MOTEURS** : nacelles TBCC « over-under » (FWD/AFT, GEN ON/OFF/FAIL, RPM %, EGT/CIT en °F, DOORS °) ;
  en stato/scram le conduit inférieur s'anime et prend le relais.
- **LIMITES** : seuil d'explosion de la chambre de combustion (manette vs Mach, stato et scram) et seuil de
  destruction des gouvernes (braquage vs KEAS) avec le point de vol courant. En loi normale un limiteur
  protège les gouvernes ; sous FBW OVERRIDE ou panne FCS, un braquage excessif à forte q les détruit.
- **ÉLEC/ENV** : batterie, pile à combustible H2/O2 (STS %), génératrices, bus essentiel ; sélecteurs
  TEMP COLD/WARM, FACEHEAT OFF/1-7, TEMP IND CKPT/E BAY, HOLD AUTO TEMP/MAN. Batterie vide = liaison perdue.

## Cartes réelles et terrain 3D (v1.4.0)
- **Carte** : tuiles réelles en projection Mercator, bouton **PLAN / SAT** (plan gris foncé Esri avec noms
  de lieux OpenStreetMap, ou imagerie satellite Esri World Imagery). Zoom jusqu'au niveau des rues.
- **Horizon artificiel (SVS)** : comme dans MSFS, imagerie satellite drapée sur le **relief réel**
  (AWS Terrain Tiles), détail progressif autour de l'avion, voile de brume vers l'horizon.
- Le **modèle de vol utilise le relief réel** (sol aplani autour des pistes) : attention au terrain en montagne.
- Tuiles téléchargées en arrière-plan et gardées en cache dans `~/Library/Caches/Darkstar/tiles` ;
  hors ligne, retour automatique à la carte Natural Earth et à un sol plat.
- **Roulis libre sur 360°** (tonneaux), en manuel comme sous autopilote.

## Écran moteurs fidèle, horizon nouvelle génération (v1.5.0)
- **Moteurs** reproduits d'après l'écran du cockpit : silhouette grise, turbine claire (cône, GEN ON,
  RPM %), EGT et CIT en °F, DOORS, repères orange d'entrée et de tuyère, contours arrondis. En stato /
  scram, le **conduit inférieur** s'illumine : dégradé jusqu'à la chambre de combustion, filets
  d'écoulement animés, jet avec losanges de choc (scram), pastille mode + poussée.
- **Horizon** : rose des caps posée sur la ligne d'horizon, **tunnel dans le ciel** (portes 3D le long
  de la route de l'autopilote, vertes sur le plan de descente ILS), **alerte relief** colorant le terrain
  proche de l'altitude de l'avion (ambre < 500 ft sous l'avion, rouge au-dessus).
- Pistes : marquages (seuils, axe, zone de toucher) de nouveau visibles sur le relief réel.
- Vision synthétique en mode réaliste uniquement (touche H retirée). Schémas aux tracés arrondis.
