# DARKSTAR — station sol d'essais en vol hypersoniques

Tu télépilotes un démonstrateur hypersonique (type SR-72 / Darkstar) depuis une station sol :
vision synthétique 3D, PFD complet, EICAS propulsion TBCC, carte du monde,
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
  manche STALL, claquoir de survitesse (q), klaxon de train, arpège de
  déconnexion AP, détonation d'unstart scram, impact du toucher, bips d'instruction acceptée/refusée.
- ESPACE acquitte (coupe le carillon), J ou `SON OFF` / `SON ON` coupe ou rétablit le son.

## Interface (v1.1)
- Barre de statut : temps simulé, chrono d'essai, **TEMPS x1..x32** (clic pour changer), liaison, WARNING/CAUTION (clic = acquitter), son, aide.
- Écran Darkstar : PFD (tambours de vitesse/altitude, collines à l'horizon), volet carburant (débit PPH,
  pressions pompes 1-4, réservoirs, CG, bouton DUMP), volet HSI
  (rose des caps, route magenta, portée 10-640 NM).
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

## Reconnaissance (v1.6.0)
Le SR-72 est un avion de reconnaissance : sa charge utile est simulée.
- **Caméra EO/IR à la verticale** : le volet caméra de l'écran Darkstar filme le sol réel (imagerie
  satellite), cap en haut, réticule, échelle, grossissement x1 à x32 (flèches du volet ou `ZOOM 8`),
  mode visible `EO` ou infrarouge `IR`. Qualité d'image **NIIRS** calculée : résolution au sol (GSD,
  fonction de la hauteur et du zoom), visée oblique (> 10° d'inclinaison), hublot chaud à haut Mach.
- **Radar SAR** à visée latérale, tout temps : fauchées de 15 à 70 km à gauche et à droite (visibles
  sur la carte), au-dessus de 30 000 ft.
- **8 objectifs** (lettres A à H) avec un NIIRS requis ; une image insuffisante peut être améliorée en
  repassant. Stockage embarqué (512 Go) et transmission au sol : rapide en LOS, lente en SATCOM.
- Page **RECON** (11e onglet) : capteurs, qualité prévue, mémoire, transmission, objectifs et guidage.
- Instructions : `CAMERA ON|OFF|EO|IR`, `ZOOM 1..32`, `SAR ON|OFF`, `CIBLE B`, `DIRECT CIBLE`
  (l'autopilote rejoint la cible sélectionnée), `RECON`.

## Correctifs v1.6.1
- **Fuite mémoire corrigée** (jusqu'à 56 Go, fermeture du jeu par macOS) : quand la cadence d'affichage
  chutait, des tuiles décodées mais pas encore envoyées au GPU étaient redemandées et décodées en double
  sans fin. Elles restent maintenant « en cours » jusqu'à leur envoi, avec contre-pression des fils de
  décodage. Cache GPU agrandi : plus de rechargements incessants (CPU ~70 % -> ~20 %).
- **Trafic aérien supprimé** (avions IA, TCAS, alertes et sons associés).
- Aide au décollage manuel : « ROTATION : FLECHE BAS (TIRER) » s'affiche à la vitesse de rotation.
- Diagnostic : `DARKSTAR_PERF=1` affiche la cadence et l'état du cache de tuiles dans le terminal.

## Horizon de nouvelle génération (v1.7.0)
- **FMA** (annonciateur de modes) : poussée / latéral / vertical / AP, actif en vert, armé en cyan.
- **Trajectoire prédite 3D** (30 s, 60 s au-delà de Mach 3) avec repères 10/20/30 s, calculée sur le
  facteur de charge et l'inclinaison réels ; couleur selon la **marge au-dessus du relief réel** (vert,
  ambre < 150 m, rouge = impact) ; croix au point d'impact et alerte **TERRAIN T-xx s** à chevrons.
- **Repères 3D** de la destination (magenta) et de l'objectif de reconnaissance (jaune) avec distance et
  ETA ; au-delà de l'horizon, posés sur la ligne d'horizon ; hors champ, flèche au bord avec l'écart de cap.
- **Couloir hypersonique** : chaîne TURB > RAM > SCRAM (prochaine transition et Mach restant), barres q
  (couloir 25-90 kPa, limite 100) et température de peau.
- **Énergie** : autonomie JP-7 et H2, rayon d'action, JP-7 estimé à l'arrivée.
- **Indexeur d'incidence** à côté du vecteur vitesse (limite de la loi en ambre).
- Échelles : bande basse vitesse, repère VAPP, tendance d'altitude 6 s, minimums ; inclinaison chiffrée > 30°.

## Système de reconnaissance complet (v1.8.0)
- **Caméra EO/IR sur tourelle** : `CAMERA NADIR` (verticale) ou `CAMERA TRACK` (poursuite de l'objectif
  jusqu'à 60° hors verticale : distance oblique et obliquité dégradent la qualité NIIRS).
- **Radar SAR** : `SAR STRIP` (bandes latérales 15-70 km, NIIRS 3,5) ou `SAR SPOT` (objectif sélectionné
  jusqu'à 150 km sur le côté, NIIRS 5).
- **ELINT** : détection des émetteurs radar dans l'horizon radar, relèvements toutes les 2 s,
  triangulation par moindres carrés avec cercle d'erreur ; varier le relèvement accélère la localisation.
- **Menaces sol-air** (émetteurs d'exercice fictifs) : domaine portée / plafond / Mach maximal. Dans le
  domaine : alerte MENACE, tir après 12 s, impact 25 s plus tard (abattu) sauf sortie du domaine
  (monter ou accélérer : le SR-72 se protège par la vitesse et l'altitude). `MENACES OFF` pour les désactiver.
- **11 objectifs** IMINT / SAR / ELINT avec priorités P1-P3 et score.
- **Photothèque** : vignettes rendues depuis l'imagerie réelle (EO couleur, IR vert, SAR gris chatoyé),
  agrandissement et métadonnées, transmission au sol par priorité (LOS rapide, SATCOM lente).
- **Planification** : `PLAN RECON` (route au plus proche voisin), `ROUTE RECON` (l'autopilote enchaîne
  les objectifs), `DIRECT CIBLE`, `PHOTO` ; route, émetteurs, domaines et relèvements sur la carte.
- Page RECON en 4 sous-pages : CAPTEURS, OBJECTIFS, ELINT / MENACES, PHOTOTHEQUE.
- Horizon agrandi (PFD sur 66 % de l'écran Darkstar) ; tuiles « Map data not yet available » ignorées.

## Météo, vent, menaces et HUD éphémère (v1.9.0)
- **Météo procédurale** (`METEO CALME|BEAU|NUAGEUX|COUVERT|ORAGEUX`) : cumulus, couches de stratus,
  cumulonimbus à enclume (sommets FL380-FL520) ; radar météo sur la carte (vert / gris / jaune-rouge).
- **Nuages 3D sur l'horizon** : nuages volumétriques éclairés par le dessus, masqués par le relief,
  éclairs dans les orages ; voile léger en IMC (la vision synthétique garde le relief visible).
- **Vent** fonction de l'altitude (couche limite, courant-jet vers FL350 jusqu'à ~150 kt, stratosphère
  calme) : dérive de l'avion, vitesse sol, vecteur vitesse sur la route sol, losange de route sur
  l'échelle de cap ; l'autopilote tient la route sol en NAV/LOC (autoland validé par vent de travers).
- **Turbulence** dans les nuages (sévère dans les orages), mécanique près du sol, en air clair près du jet.
- **Menaces sur l'horizon** : RWR (relèvement, distance, accrochage), symboles 3D des émetteurs
  (VEILLE / POURSUITE / LOCK / LAUNCH), missile visible en vol, bandeau MISSILE LAUNCH avec temps
  d'impact et consigne d'esquive.
- **Infos éphémères façon HUD** : vent, RWR, couloir hypersonique et énergie apparaissent quand elles
  changent ou deviennent critiques puis s'effacent ; survol de l'horizon à la souris ou `HUD` (10 s)
  pour tout afficher.
- v1.9.1 : portée d'accrochage de la caméra portée à 80° hors verticale (≈ 155 km devant à 90 000 ft),
  indicateur « LOCK CIBLE » et compte à rebours « LOCK DANS ... » dans le volet caméra.

## Ravitaillement en vol autonome (v1.10.0)
- `TANKER` : ravitailleur SHELL 71 (type KC-46) en hippodrome à FL250 / 300 KEAS, 20 NM devant sur la
  route (ou `TANKER KEDW` au-dessus d'un aérodrome, `TANKER OFF` pour le renvoyer), 40 t disponibles.
- `RAVITAILLEMENT` (ou `REFUEL`, `AAR`) : séquence autonome comme sur un drone — rendez-vous (poursuite,
  rattrapage avec profil de décélération, aérofreins), jonction sur la trace du ravitailleur, capture
  en précontact (60 m derrière, 12 m sous), contact sous la perche (32 m / 9 m), transfert de JP-7 à
  2,5 t/min jusqu'au plein, déconnexion et dégagement. Refusé au-dessus de Mach 1,5.
- `DECROCHER` ou touche X : déconnexion immédiate.
- Horizon : ravitailleur 3D avec perche (déployée jusqu'au réceptacle au contact), repère distance/ETA,
  directeur de ravitaillement (phase, écarts avant/arrière, gauche/droite, haut/bas, feux UP/ALT/DOWN
  et AFT/POS/FWD, jauge et quantité transférée). Carte : ravitailleur et hippodrome.

## Alertes carburant bingo (v1.11.0)
- **Bingo automatique** = carburant pour rejoindre la destination (régime actuel en subsonique, sinon
  croisière de planification) + approche (1,5 t) + réserve finale (3 t). `BINGO 8` pour l'imposer
  (tonnes), `BINGO AUTO` pour revenir au calcul, `BINGO` seul pour l'afficher.
- Niveaux : **JOKER** (bingo + 4 t, information), **BINGO FUEL** (caution), **MIN FUEL** (réserve
  atteinte, warning), **EMERGENCY FUEL** (< 1,5 t) ; **H2 BINGO** quand l'hydrogène tient moins de
  3 min en stato/scram.
- Horizon : bandeau HUD au changement de niveau (12 s, ou survol) avec JP-7 / bingo, cap, distance,
  temps et carburant nécessaires pour le retour, ravitailleur disponible ; ensuite annonciateur compact
  permanent à partir du bingo ; chevron ambre du cap de retour sur l'échelle de cap.
- Alarmes maîtres, messages et sons (caution / warning) associés.

## Missiles et contre-mesures (v1.12.0)
- **Missiles sol-air simulés** (sites d'exercice fictifs) : après accrochage (12 s, 26 s sous ECM) le
  site tire un vrai missile — propulsion puis vol freiné, poursuite anticipée, facteur de charge qui
  s'effondre en altitude, rayon létal 40 m. Il peut manquer, être leurré ou tomber à court d'énergie :
  monter et accélérer restent la meilleure défense du SR-72. Missile visible en vol sur l'horizon et la carte.
- **Contre-mesures (CMDS)** : `CHAFF` (paillettes, contre les missiles radar, bien plus efficaces quand
  le missile est par le travers), `FLARE` (leurres IR, contre le SAM mobile infrarouge), `ECM ON|OFF`
  (brouilleur : accrochage plus long, guidage radar dégradé), `CMDS AUTO|MAN` (programme automatique
  sous 18 km). Compteurs sur l'horizon sous le RWR.
- **HACM** (4 missiles hypersoniques Mach 7, portée 324 NM) : `TIR I|J|K` ou bouton TIR HACM sur un site
  localisé par l'ELINT ; tir au-dessus de 30 000 ft et de Mach 2. Impact sur la position estimée : le
  site n'est détruit que si l'erreur ELINT est inférieure à 400 m ; un site détruit n'émet plus.
- Page **ARMEMENT** (12e onglet, instruction `ARMEMENT`) : contre-mesures, missiles en vol (distance,
  vitesse, temps d'impact, leurrés), HACM et sites tirables.

## Procédure d'urgence (v1.13.0)
- `EMERGENCY` (ou `URGENCE`, `MAYDAY`) : MAYDAY / transpondeur 7700, puis décision automatique dans
  cet ordre :
  1. **ravitaillement en vol** si un ravitailleur est joignable avec le carburant restant (ou le
     ravitailleur d'alerte si l'autonomie dépasse 20 min et qu'il reste plus de 5 t), sous Mach 1,5 ;
  2. sinon **terrain joignable le plus proche** : au carburant (moteurs disponibles) avec descente
     hypersonique, réouverture des entrées et redémarrage des turbines si besoin puis atterrissage
     automatique ; ou en **vol plané** (finesse 7) si les moteurs sont perdus ;
  3. sinon **éjection** : siège, parachute, balise de détresse avec la position, effacement des
     données et des clés, puis **autodestruction** de la cellule.
- Réévaluation automatique en cas d'échec (ravitaillement abandonné, turbines qui ne redémarrent pas,
  autoland refusé, terrain hors de portée en plané).
- `EMERGENCY OFF` pour annuler ; `EJECTION CONFIRME` pour s'éjecter immédiatement.
- Bandeau EMERGENCY 7700 sur l'horizon avec l'option en cours ; compte à rebours d'autodestruction.

## AP MAX (v1.13.1)
- `AP MAX` : l'autopilote exploite tout le domaine de la **loi FBW normale** au lieu de ses limites de
  confort — facteur de charge jusqu'à 95 % de la limite de la loi (2,85 g, 1,9 g au-delà de Mach 4),
  inclinaison jusqu'à celle du virage en palier correspondant (≈ 67° / 58°), vitesses de tangage et de
  roulis complètes, vario jusqu'à 35 % de la vitesse, incidence scram 9°. Les protections (incidence,
  facteur de charge, q, entrée d'air) restent actives : seul FBW OVERRIDE permet d'aller au-delà.
  Exemple : virage de 90° à Mach 3 / FL700 en 77 s au lieu de 207 s.
- `AP NORMAL` : retour aux limites de confort. FMA : « AP1 MAX » en ambre. Approche ILS et autoland
  restent toujours en mode confort.

## Vue cockpit (v1.14.0)
- Touche **K** ou instruction `COCKPIT` : vue depuis le siège, d'après l'intérieur du Darkstar — vue
  extérieure réelle (imagerie, relief, nuages, ravitailleur, pistes) par les grandes fenêtres
  latérales, arceaux de verrière, **pyramide centrale** portant l'écran Darkstar (interactif, avec ses
  6 boutons), cadran, DSPLY et boîtier radio (transpondeur 1200 / 7700 en urgence) au sommet, écran
  moteurs à gauche, jauges CG et H2 à droite, interrupteurs gardés rouges **SCRAM ENABLE**,
  **EMER FUEL DUMP**, **EMER H2 DUMP** (fonctionnels), levier de train et trois vertes, manche,
  manette des gaz sur la console gauche (suit la manette réelle).
- **Clic droit glissé** : tourner la tête (l'intérieur suit) ; double clic droit : recentrer.
- **H** : se pencher vers l'écran Darkstar (affiché en grand) ; `COCKPIT HUD` : HUD vert facultatif
  (le Darkstar n'en a pas). K de nouveau : retour à la station sol.

## Cockpit 3D texturé (v1.15.0)
- La vue cockpit (K) est désormais un **vrai cockpit 3D** dans le repère de l'avion, rendu avec la même
  caméra que la vue extérieure (parallaxe réelle en tournant la tête au clic droit) : arceaux de
  verrière, plafond, **pyramide centrale**, appuis de fenêtres nervurés, consoles latérales à boutons
  éclairés, plancher antidérapant, manche avec soufflet, manette des gaz qui suit la poussée.
- **Textures procédurales** générées au démarrage (métal brossé, platine vissée et biseautée, parois
  nervurées, antidérapant, consoles, soufflet jaune/noir) et éclairage par face depuis la verrière.
- Les instruments (écran Darkstar, écran moteurs, radio, DSPLY, jauges CG/H2, interrupteurs gardés,
  levier et voyants de train) sont posés sur leurs emplacements 3D projetés et restent interactifs ;
  ils se masquent quand on regarde sur le côté. H : écran Darkstar en grand.
- v1.16.0 : cockpit remodelé d'après l'intérieur du Darkstar — arceaux tubulaires rivetés (courbes),
  ciel de cabine épousant les fenêtres, pyramide étroite à arêtes chanfreinées et ailes latérales,
  lunettes d'instruments en relief, boutons éclairés et capots rouges en 3D, appuis rembourrés,
  consoles inclinées denses, manette bi-levier, manche cylindrique à soufflet annelé ; palette
  quasi noire, éclairage lissé par sommet depuis les fenêtres, atlas de textures unique, rendu par lots.
- v1.17.0 : cockpit enrichi — pièces arrondies (arêtes chanfreinées), reflets des fenêtres sur les
  métaux, capots rouges SCRAM ENABLE / EMER FUEL / EMER H2 DUMP en 3D sur les ailes de la console
  (comme sur l'appareil), boutons verts sous l'écran moteurs, poignée d'éjection jaune et noire,
  bourrelets du siège, rangées d'interrupteurs à bascule en relief sur les consoles latérales.
- v1.18.0 : cockpit **modélisé dans Blender** (`tools/cockpit_model.py`, régénérable) — courbes en
  tubes, chanfreins réels, occlusion ambiante cuite par Cycles, ~27 000 faces ; chargé depuis
  `assets/cockpit.obj` et rendu avec un vrai tampon de profondeur (occlusion exacte), leviers de
  manette animés.
- v1.19.0 : vue cockpit retirée (touche K, commande COCKPIT, modèle 3D) ; retour à la station sol seule.
- v1.20.0 : **horizon v2** (évolution type Hornet → Super Hornet) — vision synthétique sur toute la
  largeur de l'écran, échelles vitesse / altitude / variomètre translucides posées sur l'image,
  bandeau de modes unique (MENU, FMA, mode TBCC, liaison), arc d'inclinaison en haut avec pointeur
  de ciel, ruban de cap avec relèvement destination, blocs de données lisibles sur ciel clair.
- v1.21.0 : **ressenti de vol** (commande `RESSENTI ON/OFF`, actif par défaut) — vibrations de la vue
  (turbulence, buffeting près du décrochage, en transsonique, à forte pression dynamique ou fort
  facteur de charge, vibration du scramjet), inertie de la tête (la vue s'enfonce sous g et retarde
  sur le roulis), à-coups (toucher des roues, mur du son, allumage scram / PC), roulage sur la piste,
  voile gris / rouge sous facteur de charge, lueur de plasma en hypersonique dense, grondement de
  structure et bang supersonique.
- v1.22.0 : **météo réelle et horizon intelligent** — prévision Open-Meteo de l'heure courante (gratuit,
  sans clé) sur une grille 5 x 5 autour de l'avion, 12 niveaux de 1000 à 30 hPa (vent, température,
  humidité, nébulosité) + CAPE, précipitations, nébulosité basse / moyenne / haute. Vent, turbulence en
  air clair (cisaillement réel), nuages et orages générés d'après ces données ; repli simulé hors
  ligne (`METEO REEL` par défaut, `METEO ORAGEUX`... pour simuler). Visualisation 3D dans l'horizon
  (`METEO 3D ON/OFF`) : flux de particules de vent coloré selon le danger (épaisseur selon
  l'humidité), vecteurs de vent devant l'avion, volumes filaires autour des cumulonimbus, nappes de
  turbulence en air clair ; bloc météo avec source, température extérieure, légende ; alerte du danger
  le plus fort sur la trajectoire.
- v1.23.0 : **radar météo façon Airbus** (`WXR`) — antenne balayant ± 60° (image rafraîchie au passage
  du faisceau), échos vert / jaune / rouge d'après les cumulonimbus, cumulus développés et
  précipitations réelles, turbulence en magenta dans les 40 NM (WX+TURB), inclinaison AUTO
  (balayage multiple) ou manuelle (`WXR TILT -3`, faisceau de 3,5°), portée de l'écran de navigation
  (max 320 NM, `WXR RANGE 160`). Dans l'horizon (`WXR HORIZON ON/OFF`) : rideaux translucides aux
  échos significatifs et bande de synthèse colorée sur la ligne d'horizon.
- v1.23.1 : zones orageuses de forme irrégulière (étirement, bords déchiquetés, lobes) dans l'horizon,
  sur le radar météo et sur la carte ; flèches de vent retirées de l'horizon.
- v1.23.2 : particules de flux de vent retirées de l'horizon (restent : zones orageuses, nappes de
  turbulence, radar météo, bloc vent).
- v1.24.0 : **perte du véhicule réaliste** — cascade de pannes selon la cause (rupture en vol,
  feu / explosion, missile, perte électrique, impact, terminaison FTS) avec messages type ECAM,
  panneau d'alarmes qui s'allume en « sapin de Noël », alarme continue, bandeau « FLT CTL —
  commandes de vol sans réponse », télémétrie corrompue (déchirures, trames perdues) et drapeaux
  rouges ATT / FPV / SPD / ALT / V/S / HDG ; puis perte du signal : neige, écrans barrés d'une croix
  rouge, dernière trame valide (position, niveau, vitesse, cap, vario, facteur de charge) et cause
  probable ; analyse de l'enregistreur de vol ensuite.
- v1.25.0 : **page SYSTEMES** (onglet SYSTEMES), d'après l'écran principal du Darkstar : servitudes
  (hydraulique, électricité, pile H2, oxygène, baie avionique), vue en plan structure / thermique au
  contour orange (organes colorés selon leur état, capteurs de température), tableau des moteurs
  (turbines G / D, statoréacteur, scramjet), éventail du radar météo, échelles de centrage et
  d'incidence, vue en plan carburant (réservoirs JP-7 et H2 remplis selon le niveau), rangée
  d'icônes d'accès direct aux pages.
- v1.25.1 : horizon — annonces d'événements aérofreins (sortis / rentrés, auto) et freins (frein de
  parc serré / desserré, freinage, freins armés / relâchés) ; mémos permanents AEROFREINS (ambre si
  poussée), FREIN PARC (ambre si poussée au frein), FREINAGE avec décélération.
- v1.26.0 : informations caméra sur l'horizon (mode EO/IR, nadir/poursuite, zoom, GSD, NIIRS, raison
  d'image dégradée, dépointage, LOCK CIBLE ou compte à rebours) ; position de l'avion sur l'horizon ;
  l'image caméra reste dégagée. Volet carburant refait : silhouette du Darkstar de la page SYSTEMES
  avec réservoirs JP-7 / H2 remplis selon le niveau, débit et quantités à gauche, pression et
  pompes à droite.
- v1.26.1 : bloc radar météo sur l'horizon (mode WXR / WXR+TURB, inclinaison, portée, écho le plus
  significatif avec position horaire et distance, couleur selon l'intensité) ; annonce retirée de
  l'écran de navigation.
- v1.27.0 : **flotte et liaison de données** — au lancement, choix du Darkstar (FTV-01 à FTV-04,
  disponibilité réelle : maintenance, en vol avec une autre station), du départ (point d'attente :
  instruction `ALIGNEMENT` pour rouler jusqu'à la piste ; ou aligné sur la piste) et établissement
  de la liaison (poignée de main en 5 étapes). Chemins : LOS (Edwards / Palmdale), stations sol
  relais, avions relais BACN, SATCOM, secours Iridium (urgence seulement) ; horizon radio selon
  l'altitude, occultation plasma en hypersonique dense, pannes `PANNE LIAISON` / `PANNE SATCOM`.
  Bascule automatique ; à la perte : télémétrie figée, ordres non transmis, l'avion maintient cap
  et altitude puis déclenche lui-même l'urgence après 5 min ; reconnexion via le relais choisi
  (panneau sur l'horizon ou `RECONNEXION`, `RELAIS SOL KNID`, `RELAIS BACN 1`, `RELAIS SECOURS`).
  `EMERGENCY OVERRIDE` : vol plané moteurs au ralenti puis autoland ; `EJECTION CONFIRME` : boîte
  noire éjectée avec la capsule, données sauvegardées.
- v1.28.0 : **bannière d'approche** discrète en bas de l'horizon (ILS dans les 15 NM ou autoland) :
  phases (IAF, LOC, G/S, FLARE, ROLLOUT, ARRÊT ; en manuel LOC, G/S, MINIMUMS, TOUCHER, ROULAGE),
  profil vertical avec le fil du plan de descente à 3°, arrondi, piste, repères, traînée et avion
  abstrait, écart au plan. Silhouette du Darkstar de la page SYSTEMES partout (thermique, FCS,
  symbole avion de l'écran de navigation et de la carte, écran de connexion).
- v1.28.1 : **autoland plané réellement sans moteur** (`ATTERRIR PLANE`, et via `EMERGENCY OVERRIDE`
  qui coupe turbines / statoréacteur / scramjet et vidange JP-7 et H2) : profil façon navette —
  pente extérieure de 9° vers un point d'aboutissement 4,5 km avant le seuil, vitesse tenue aux
  aérofreins, pré-arrondi anticipé vers une pente intérieure de 3°, train à 2 NM, arrondi au-dessus
  du seuil ; poussée et manette nulles imposées, pas de remise des gaz. Essai de vol automatique :
  14 NM d'Edwards moteurs coupés → toucher à 109 KEAS, -618 ft/min, arrêt sur la piste, poussée
  maximale mesurée 0 N.
- v1.28.2 : liaison permanente en exploitation normale (couverture SATCOM mondiale, pas
  d'occultation plasma, connexion initiale toujours établie) ; la perte de liaison ne survient que
  sur incident (panne émetteur, panne SATCOM hors portée radio, perte du bus électrique), et le
  canal de secours Iridium reste disponible pour l'EMERGENCY OVERRIDE.
- v1.29.0 : **deuxième chance** après la perte du véhicule sur incident : bouton sous l'écran LOS
  (une tentative par le secours Iridium, poignée de main de 8 s) ; selon la cause, le véhicule peut
  répondre (perte électrique 70 %, explosion moteur 50 %, gouvernes 35 %, missile 25 %, rupture
  20 / 10 %, impact ou autodestruction jamais) : l'autonomie de bord a stabilisé l'avion, moteurs
  perdus, liaison d'urgence seulement → EMERGENCY OVERRIDE ou EJECTION CONFIRME. Injection
  d'essai : `INCIDENT ELEC | MOTEUR | GOUVERNES | STRUCTURE`. **Surveillance des chambres** à la
  place du « % santé » : cinq courbes sur 3 min (EGT, parois stato / scram, excès de carburant,
  usure cumulée) avec seuils et limites ; état calculé (NOMINAL, SURVEILLANCE, DÉGRADÉ, CRITIQUE,
  DÉTRUITES) d'après marges, temps au-delà des seuils et tendance de l'usure, avec la cause.
- v1.29.1 : horizon agrandi (écran principal 67 % de la largeur, horizon 71 % de sa hauteur) et
  données réagencées : alerte carburant sous l'arc d'inclinaison sur fond sombre, radar météo sur
  une ligne compacte, repères hors champ sous la ligne d'horizon, rappel de saisie limité au début
  de l'essai, statut autoland porté par la bannière d'approche ; pompes carburant ajustées.
- v1.29.2 : **bords de carte** — tuiles correctement placées quand la vue chevauche ±180°,
  calottes polaires (au-delà de 85°, hors projection Mercator) affichées en banquise avec repère,
  centre de carte limité à ±80° en suivi. **Chambres** : usure 4 fois plus lente avec alertes à
  50 %, 30 % et 15 % (sans pourcentage affiché), inertie thermique des parois de 20 s, alerte dès
  le seuil de paroi chaude, compte à rebours de 20 s au-delà de la limite avant la rupture ; la
  rupture détruit les deux moteurs d'un coup sans perdre la cellule (vol plané, EMERGENCY OVERRIDE
  ou éjection). Pire cas mesuré (manette 200 %, override, Mach 9,8) : 55 s entre la première
  alerte et la rupture.
- v1.29.3 : deuxième chance garantie — le véhicule répond toujours à la reconnexion par le secours
  Iridium, sauf s'il a percuté le sol (ou s'est autodétruit après éjection).
- v1.29.4 : après la reconnexion de secours, **toutes les instructions** passent par Iridium
  (latence 2,5 s, manche indisponible) ; l'avion refuse ce qu'il ne peut pas exécuter.
  **Panneau d'alarmes cohérent** : le « sapin de Noël » n'allume que les systèmes touchés par
  l'incident puis ceux qui en dépendent (moteurs → génératrices / FADEC / hydraulique,
  hydraulique → FCS / train / portes d'entrée d'air, électricité → calculateurs...) ; à la
  récupération, chaque voyant de panne allumé devient une panne réelle (FCS en loi directe,
  hydraulique, génératrices, train bloqué → sortie par gravité en autoland plané...).
- v1.29.5 : EMERGENCY OVERRIDE corrigé (rejeu du vol utilisateur) — train sorti seulement en courte
  finale, vitesse tenue pendant le plané, cône d'alignement façon navette quand l'avion est trop
  haut (spirale tangente à l'axe, pente planifiée), gestion d'énergie totale aux aérofreins en
  finale. Essais : départ FL459 à 16 NM, FL300 au nord, FL150 par le travers → posés sur la piste.
- v1.30.0 : **minimums** en approche ILS / autoland — « APPROACHING MINIMUMS » (300 ft), puis
  « MINIMUMS » (200 ft) avec choix CONTINUE / GO AROUND sur l'horizon (boutons ou instructions
  `CONTINUE`, `GO AROUND`) ; sans décision à 60 ft l'approche continue ; remise des gaz : pleine
  poussée, montée 3000 ft sol, train rentré, autoland relancé (impossible en plané sans moteur).
  **Reconnexion immédiate** (remplace la « 2e chance ») : l'avion reprend exactement l'état de la
  dernière trame avant la déconnexion (attitude, vitesse, configuration, modes, pannes réelles),
  liaison rétablie sur le chemin précédent ; le panneau d'alarmes reprend l'état réel.
- v1.30.1 : **reconnexion = l'avion endommagé reprend**, pas réparé — l'avion reprend l'état de vol
  précédant la perte avec les dégâts de l'incident (chaque système touché du panneau devient une
  panne réelle) ; liaison en **mode dégradé** (télémesure + terminal, manche indisponible, bandeau
  sur l'horizon) avec instructions refusées selon les dégâts (gouvernes / hydraulique → pas de
  pilotage AP ; moteurs détruits → pas de commande moteur ; électricité → capteurs hors tension) ;
  EMERGENCY OVERRIDE et éjection toujours possibles ; TOUT RÉPARER lève le mode dégradé.
  **DA** réglable (`DA 250`, seuil + hauteur), affichée sur l'altimètre (cyan, ambre en dessous),
  repère MIN calé dessus ; annonces 1000, 500, 100 ABOVE, APPROACHING MINIMUMS, MINIMUMS,
  50-40-30-20-10, RETARD.
- v1.30.2 : EMERGENCY OVERRIDE — arrivée par le sud-ouest corrigée : cône d'alignement à rayon
  adapté (virages à 45° en plané), sortie au point de tangence seulement près du profil, vitesse
  tenue en approche du cercle, raccourci vers l'axe choisi selon la pente, finale pouvant plonger
  jusqu'à ~19° aérofreins sortis, arrondi anticipé à fort taux de chute. Essais : FL459 à 16 NM,
  FL300 au nord, FL250 au sud-ouest → posés sur la piste sans poussée.
- v1.30.3 : EMERGENCY OVERRIDE — cas « FL150 très proche du terrain » corrigé : pente de la
  spirale asservie à la pente réellement suivie (virages à 45°), sortie dès que l'avion est aligné
  dans l'axe, S-turns en finale pour dissiper un excès d'énergie, correction plus forte en courte
  finale. Quatre départs d'essai (FL459 à 16 NM, FL300 nord, FL250 sud-ouest, FL150 à 7,5 NM par
  le travers) → posés et arrêtés sur la piste, poussée nulle.
- v1.30.4 : EMERGENCY OVERRIDE — prise en compte du vent (dernier vol : posé 500 m court avec ~20 kt
  de face en finale). Profil plané calculé en distance « air » (vent de face → plus de hauteur
  requise), sortie de spirale avec marge de vent, VAPP majorée de la moitié du vent de face,
  correction forte sous le profil en échangeant l'excès de vitesse contre de la hauteur. Essais :
  7 départs, dont le vol rejoué avec 20 kt de face et deux cas vent arrière / travers.
- v1.30.5 : reconnexion immédiate — plus aucune panne injectée. L'avion reprend exactement l'état et
  les pannes réelles du moment de la déconnexion ; le « sapin de Noël » de l'écran de perte du signal
  reste une indication et n'est plus converti en pannes. Mode dégradé (manche indisponible, terminal)
  et refus d'instructions basés sur les pannes réellement présentes.
- v1.30.6 : DA / minimums seulement pendant une approche réelle (autoland engagé ou ILS armé /
  capturé, en vol) — plus d'affichage « DA » ni de ligne MIN au sol ou en croisière près du terrain.
- v1.31.0 : ressenti de vol et réalisme.
  - Pilotage : inertie de la cellule (vitesses angulaires du 2e ordre, réponse molle à basse
    vitesse, vive à forte pression dynamique) et autorité manuelle d'avion d'arme (≈10°/s en
    tangage, 55°/s en roulis en subsonique, décroissantes avec le Mach).
  - Turbulence physique (bruit filtré façon Dryden) : rafales verticales qui changent l'incidence
    (bosses de facteur de charge), perturbations de roulis et de tangage.
  - Masses d'air turbulentes localisées : thermiques en basse couche, ondes / rotors en aval du
    relief par vent fort, poches de turbulence en air clair entre FL200 et FL500.
  - Décollage : annonces 100 KT, V1, ROTATE (VR selon la masse), POSITIVE RATE ; rotation sans
    effet sous ~120 KEAS ; TAIL STRIKE au-delà de 12° ; effet girouette par vent de travers ;
    joints de piste, levée de la roulette avant, claquement du train, tête poussée à l'accélération.
  - Reconnaissance : objectifs et émetteurs répartis dans le monde entier (Gizeh, Trois-Gorges,
    Baïkonour, Panama, Rotterdam, Uluru, Perito Moreno, Kourou ; sites ELINT Sahara, Gobi,
    Australie, golfe Persique, océan Indien, Groenland).
- v1.31.1 : horizon — zone centrale symétrique autour de l'axe de l'avion (bannières carburant,
  approche, ILS, blocs éphémères centrés sur la maquette et le ruban de cap) ; bandeau « WXR … » retiré
  de l'horizon (le radar météo reste sur le ND, la page SYSTEMES et en rideaux dans l'horizon).
- v1.31.2 : ravitaillement en vol — transfert simultané de JP-7 et d'hydrogène liquide (scramjet) :
  ravitailleur avec 40 t de JP-7 et 16 t de H2 (~1 t/min), deux jauges en contact, bilan JP-7 + H2,
  ravitaillement possible si l'un des deux réservoirs n'est pas plein.
- v1.32.0 : protections et systèmes.
  - AUTO GCAS (récupération automatique avant impact sol) : prédiction remise à plat + ressource
    avec le relief devant ; chevrons convergents puis PULL UP sur l'horizon, déclenchement FLY UP
    (ailes à plat, ressource), protections FCS dépassées jusqu'à 5 g si la loi normale ne suffit
    plus (FCS en loi directe ensuite : avion sauvé). Inhibé train sorti / autoland. Interrupteur
    AUTO GCAS à la console, instruction GCAS ON/OFF.
  - FUEL COOL (FTMS) : carburant refroidi par échangeur à hydrogène liquide + air dynamique (AUTO),
    air dynamique seul (RAM, inefficace au-delà de Mach 3) ou OFF ; fini le FUEL HOT en croisière
    hypersonique normale. Instruction FUELCOOL AUTO/RAM/OFF.
  - Console : nouvelle platine SYSTEMS (AUTO GCAS, FUEL COOL, G LIMIT NORM/HIGH, PITOT HEAT avec
    givrage des sondes s'il est coupé par air froid, NWS GAIN HI/LO) ; instructions GLIM, PITOT, NWS.
  - Horizon façon F/A-18 / Rafale : repères critiques en grosses lettres (FLY UP, PULL UP, OVER G,
    AOA, OVERSPEED, SPEED), fenêtre « ALARME » rouge clignotante et ligne « PANNE » ambre.
- v1.33.0 : reconnexion, moteurs, console, horizon.
  - Incident : l'écran devient noir presque aussitôt, « RECONNEXION EN COURS PAR CANAL D'URGENCE
    (IRIDIUM) » démarre seule (7 s). Réussite la plupart du temps (90 % rupture structurelle, 85 %
    explosion, 75 % missile, 70 % perte électrique, 0 % impact / autodestruction) ; après un échec,
    deux nouveaux essais possibles à chance réduite. Reconnecté : état d'avant l'incident, mode
    dégradé (télémesure, caméra allumée, terminal, manche indisponible) et dégâts selon la cause :
    gouvernes / structure → plus de FCS ; explosion → deux moteurs perdus, FCS disponible ;
    missile → moteurs + hydraulique ; électrique → génératrices, capteurs (caméra) hors tension.
  - Page MOTEURS : tableau turbines G/D (N1, N2, EGT, débit PPH, pression / température d'huile,
    vibrations, tuyère), statoréacteur et scramjet (poussée, débit, parois), entrée d'air
    (température d'arrêt, rendement, rampe, portes), Isp / TSFC, carburant restant.
  - Console FLT CNTRL : SPD BRK (EXT/RET), AUTO TRIM (compensation de la dissymétrie de poussée),
    AP ENGD/DISC.
  - Horizon : colonne TAS / FPA / limite de g / hauteur d'énergie, mémos GCAS OFF, PITOT ICE,
    G LIM HIGH, FUEL COOL OFF.
- v1.33.1 : reconnexion après incident — les dégâts de l'incident ne s'affichent plus comme des
  pannes injectées (« DEGAT » ambre, non commutables, effacés par TOUT REPARER) ; surpression (q
  excessive) : sondes + FCS en loi directe, pilotage par instructions possible (plus de perte totale
  du FCS) ; reprise ~2 s avant l'incident (la dernière trame était déjà irrécupérable) ; autonomie
  de bord : l'AP s'engage (ailes à plat, assiette tenue ou remise en palier) si les gouvernes
  répondent. Instruction d'essai INCIDENT SURPRESSION.
- v1.34.0 : survitesse, consignes pilote, incendie.
  - AUTO RECOVERY SURVITESSE : prédiction de la pression dynamique sur 10 s ; si la courbe atteint
    la rupture (130 kPa), ressource maximale (ailes à plat, 4,8 g au plus, plein réduit,
    aérofreins) avec un braquage plafonné à 95 % de la droite de rupture des gouvernes. Graphe
    sur l'horizon : courbe q(t) dynamique face aux droites LIMITE / RUPTURE (en kPa et KEAS) et
    jauge de braquage face à la rupture. Interrupteur AUTO RECOV (console) commun avec l'Auto-GCAS.
    Les récupérations automatiques plafonnent aussi le g en FBW OVERRIDE.
  - Consignes façon avion de chasse : RELEASE STICK (braquage proche de la rupture des
    charnières), ROLL LEFT / ROLL RIGHT, PULL avec flèches (rouge en alerte, cyan pendant une
    récupération automatique).
  - Incendie : quatre zones (moteurs G/D, baie des chambres, soute H2) avec risques
    d'inflammation (EGT, moteur non confiné, parois / rupture de chambre, fuite H2 sur peau chaude),
    propagation, destruction et explosion ; onglet INCENDIE avec poignées FEU, deux bouteilles,
    boucles de détection, FIRE TEST ; repère FIRE sur l'horizon. Instructions FEU, BTL, INCENDIE.
- v1.35.0 : FMS Darkstar (« DK-FMS ») et MCDU.
  - Onglet MCDU : écran 24 colonnes, touches de ligne, pages INIT, F-PLN, PERF, PROG, FUEL PRED,
    DIR TO ; saisie au clavier (clic sur l'écran, ÉCHAP pour rendre le clavier au vol).
  - SimBrief : identifiant mémorisé, IMPORT OFP (dernier OFP de l'utilisateur, route point par
    point), GENERER SIMBRIEF (page de dispatch pré-remplie dans le navigateur). Le profil vertical
    de l'OFP est remplacé par le profil Darkstar.
  - DARKSTAR DISPATCH : plan calculé (orthodromie, points T/TURB, T/RAM, T/SCRM, T/D), Mach et
    niveau par phase, temps, besoins JP-7 / H2 avec réserves.
  - Guidage NAV sur le plan : poursuite du point actif, séquencement avec anticipation du virage,
    DIRECT TO, insertion / suppression de points (OACI ou coordonnées). Route sur la carte.
  - Instructions : MCDU [INIT|FPLN|PERF|PROG|FUEL|DIR], SIMBRIEF <id>, DISPATCH KPMD PHNL [FL] [M].
- v1.36.0 : VNAV et Mach gérés par le FMS, base d'aéroports étendue.
  - VNAV (par défaut à l'activation du plan) : montée / accélération selon le TBCC (FL350 →
    FL500 → stato armé puis allumé, turbines coupées et entrées fermées à Mach 3, FL580 → FL800,
    scram activé puis allumé), croisière FL / Mach du plan par l'A/THR ; au T/D descente à q
    constant, coupure propre du scram sous Mach 5 et du stato sous Mach 2,6, réouverture des
    entrées et rallumage des turbines, descente finale vers la destination, ATTERRIR à 60 NM.
    Distance de descente recalibrée. Page PERF : VNAV ON/OFF ; instruction VNAV ON/OFF.
    Essai KPMD → PHNL de bout en bout : M6 FL900, posé à Honolulu en 69 min.
  - 69 aéroports ajoutés (Amériques, Europe, Moyen-Orient, Afrique, Asie, Océanie, îles du
    Pacifique, bases : Vandenberg, Beale, Groom Lake, Tonopah, Brize Norton, Wake…) : 96 au total.
- v1.36.1 : SimBrief — la page de génération s'ouvrait avec un type d'avion inconnu (« CONC ») et
  demandait de choisir un avion : type long-courrier valide par défaut (B77W), modifiable dans le
  MCDU (INIT, ligne 4 à droite) ; nouvelle adresse de dispatch SimBrief.
- v1.37.0 : SimBrief IFR.
  - Import : le Pilot ID numérique est envoyé comme « userid » (il était envoyé comme nom
    d'utilisateur : « Unknown UserID »). Nom d'utilisateur ou Pilot ID acceptés.
  - Procédures : tous les points de l'OFP sont gardés ; SID et STAR reconnues (nom de procédure,
    pistes de départ / d'arrivée affichées dans le F-PLN, voie ou procédure de chaque point).
  - VNAV IFR : SID volée aux altitudes prévues par SimBrief (250 kt sous le FL100), profil
    Darkstar en route, descente hypersonique terminée à l'entrée de STAR, STAR volée aux
    altitudes prévues, puis autoland.
- v1.38.0 : plusieurs pistes par aéroport (106 pistes supplémentaires : parallèles et sécantes des
  grands aéroports, lac sec d'Edwards 17/35…). Rendu de toutes les pistes, détection « sur la
  piste » sur chacune. Autoland : piste du plan de vol (SimBrief, numérotation magnétique, L/C/R)
  en priorité, sinon face au vent (> 10 kt), sinon trajet le plus court ; désignation L/C/R
  calculée parmi les parallèles ; plané sans moteur et décollage auto sur n'importe quelle piste.
- v1.38.1 : horizon — tunnel vert d'aide à l'atterrissage retiré. MCDU : l'avion SimBrief accepte
  l'identifiant d'un avion personnalisé (ex. un « Darkstar » créé dans SimBrief).
