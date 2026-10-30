# Darkstar — simulateur d'essais en vol (Rust + macroquad)

## Règle de travail
- Après **chaque modification**, publier une release : `scripts/release.sh patch "notes en français"`
  (`minor`/`major` pour les grosses évolutions). Le script incrémente la version, lance les tests,
  construit `Darkstar.app`, commit, tag, push et crée la release GitHub (asset `Darkstar-macos.zip`).
  Le jeu installé détecte la release et propose la mise à jour (bandeau / instruction `MAJ`).
- Réinstaller localement si besoin : `scripts/bundle.sh && scripts/install.sh`.
- L'utilisateur parle français ; interface en français.

## Environnement
- Toolchain : `export RUSTUP_HOME=~/Rust/toolchain/rustup CARGO_HOME=~/Rust/toolchain/cargo`.
- Tests de vol headless : `cargo test --release -- --nocapture` (profil Mach 10, décollage + autoland).
- Capture d'écran : `DARKSTAR_SHOT=/chemin.png DARKSTAR_SHOT_FRAME=300 ./target/release/darkstar_sim [hyper|approche|autoland]`.
- Les sessions Claude tournent en root : remettre les fichiers à `sandrombp:staff` (`chown -R`).

## Pièges macroquad (GL sur Metal)
- Texte : passer par `ui::txt` (taille de rastérisation unique, ASCII seulement). Chaque nouvelle
  taille/caractère re-téléverse tout l'atlas et sature la mémoire GPU (texte noir, OOM).
- Capacité d'appel de dessin faible (4000 sommets / 6000 indices) : un tampon GPU est alloué par
  appel. Gros maillages via `svs::draw_mesh_chunked`.
- Raccourcis clavier : codes physiques ; n'utiliser que des touches identiques en AZERTY/QWERTY
  (pas A, Q, Z, W, M).
