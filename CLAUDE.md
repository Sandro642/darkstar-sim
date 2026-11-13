# Darkstar — simulateur d'essais en vol (Rust + macroquad)

## Règle de travail
- Après **chaque modification**, publier une release : `scripts/release.sh patch "notes en français"`
  (`minor`/`major` pour les grosses évolutions). Le script incrémente la version, lance les tests,
  construit `Darkstar.app`, commit, tag, push et crée la release GitHub (asset `Darkstar-macos.zip`).
  Le jeu installé détecte la release et propose la mise à jour (bandeau / instruction `MAJ`).
- Réinstaller localement si besoin : `scripts/bundle.sh && scripts/install.sh`.
- L'utilisateur parle français ; interface en français.

## Analyse de crash
- L'enregistreur de vol écrit chaque vol dans `~/Library/Application Support/Darkstar/vols/` :
  `<date>_vol.csv` / `<date>_CRASH.csv` (trace 2 Hz : position, alt, KEAS, Mach, vario, pente, assiette,
  incidence, g, q, poussée/traînée, manette, carburant, peau, train, modes AP, moteurs, alarmes) et
  `.txt` (analyse automatique + journal des messages). Quand l'utilisateur signale un crash, lire le
  dernier `*_CRASH.txt` puis la fin du `.csv` correspondant pour analyser.

## Environnement
- Toolchain : `export RUSTUP_HOME=~/Rust/toolchain/rustup CARGO_HOME=~/Rust/toolchain/cargo`.
- Tests de vol headless : `cargo test --release -- --nocapture` (profil Mach 10, décollage + autoland).
- Capture d'écran : `scripts/shot.sh target/shots/x.png 300 [hyper|approche|autoland] ["VAR=valeur" ...]`
  (lance l'app via `open` dans un paquet .app de test, au nom de l'utilisateur de la session :
  ne JAMAIS lancer `./target/release/darkstar_sim` directement depuis la session root, macOS
  laisse alors une application fantôme « darkstar_sim » ouverte en arrière-plan).
- Les sessions Claude tournent en root : remettre les fichiers à `sandrombp:staff` (`chown -R`).

## Pièges macroquad (GL sur Metal)
- Texte : passer par `ui::txt` (taille de rastérisation unique, ASCII seulement). Chaque nouvelle
  taille/caractère re-téléverse tout l'atlas et sature la mémoire GPU (texte noir, OOM).
- Capacité d'appel de dessin faible (4000 sommets / 6000 indices) : un tampon GPU est alloué par
  appel. Gros maillages via `svs::draw_mesh_chunked`.
- Raccourcis clavier : codes physiques ; n'utiliser que des touches identiques en AZERTY/QWERTY
  (pas A, Q, Z, W, M).

