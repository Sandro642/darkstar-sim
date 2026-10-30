//! Mise à jour automatique depuis les releases GitHub (curl + ditto, outils fournis avec macOS).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const REPO: &str = "Sandro642/darkstar-sim";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const ASSET: &str = "Darkstar-macos.zip";

#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String, notes: String },
    Downloading { version: String },
    Ready { script: PathBuf },
    Error(String),
}

#[derive(Clone)]
pub struct Updater {
    pub state: Arc<Mutex<State>>,
}

fn parse_version(s: &str) -> (u32, u32, u32) {
    let mut it = s.trim_start_matches('v').split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// Extrait la valeur d'une chaîne JSON `"key":"..."` à partir d'une position (sans dépendance).
fn json_str(json: &str, key: &str, from: usize) -> Option<(String, usize)> {
    let pat = format!("\"{key}\"");
    let k = json[from..].find(&pat)? + from + pat.len();
    let colon = json[k..].find(':')? + k + 1;
    let start = json[colon..].find('"')? + colon + 1;
    let mut out = String::new();
    let mut chars = json[start..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, start + i + 1)),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push('\n'),
                Some('r') => {}
                Some('t') => out.push(' '),
                Some('u') => {
                    let hex: String = (0..4).filter_map(|_| chars.next().map(|(_, c)| c)).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(c) => out.push(c),
                None => break,
            },
            c => out.push(c),
        }
    }
    None
}

/// Racine du bundle .app si le jeu tourne depuis une application installée.
pub fn app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf)
}

impl Updater {
    pub fn new() -> Self {
        Updater { state: Arc::new(Mutex::new(State::Idle)) }
    }

    pub fn get(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    /// Interroge la dernière release GitHub en arrière-plan.
    pub fn check(&self) {
        if matches!(self.get(), State::Checking | State::Downloading { .. } | State::Ready { .. }) {
            return;
        }
        self.set(State::Checking);
        let me = self.clone();
        std::thread::spawn(move || {
            let out = Command::new("/usr/bin/curl")
                .args(["-sfL", "--max-time", "15", "-H", "Accept: application/vnd.github+json"])
                .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
                .output();
            let json = match out {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                Ok(_) => return me.set(State::Error("aucune release trouvée".into())),
                Err(e) => return me.set(State::Error(format!("curl : {e}"))),
            };
            let Some((tag, _)) = json_str(&json, "tag_name", 0) else {
                return me.set(State::Error("réponse GitHub illisible".into()));
            };
            let notes = json_str(&json, "body", 0).map(|(b, _)| b).unwrap_or_default();
            // Recherche de l'URL de l'archive de l'application parmi les assets
            let mut url = None;
            let mut pos = 0;
            while let Some((u, next)) = json_str(&json, "browser_download_url", pos) {
                if u.ends_with(ASSET) {
                    url = Some(u);
                    break;
                }
                pos = next;
            }
            if parse_version(&tag) > parse_version(VERSION) {
                match url {
                    Some(url) => me.set(State::Available { version: tag.trim_start_matches('v').to_string(), url, notes }),
                    None => me.set(State::Error(format!("release {tag} sans {ASSET}"))),
                }
            } else {
                me.set(State::UpToDate);
            }
        });
    }

    /// Télécharge et prépare l'installation ; le remplacement se fait après la fermeture du jeu.
    pub fn download(&self) {
        let State::Available { version, url, .. } = self.get() else { return };
        let Some(app) = app_bundle() else {
            return self.set(State::Error("lancé hors de Darkstar.app : mise à jour impossible".into()));
        };
        self.set(State::Downloading { version: version.clone() });
        let me = self.clone();
        std::thread::spawn(move || {
            let tmp = std::env::temp_dir().join(format!("darkstar_update_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&tmp);
            if let Err(e) = std::fs::create_dir_all(&tmp) {
                return me.set(State::Error(format!("dossier temporaire : {e}")));
            }
            let zip = tmp.join(ASSET);
            let ok = Command::new("/usr/bin/curl").args(["-sfL", "--max-time", "300", "-o"]).arg(&zip).arg(&url).status();
            if !matches!(ok, Ok(s) if s.success()) {
                return me.set(State::Error("échec du téléchargement".into()));
            }
            let x = tmp.join("x");
            let ok = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip).arg(&x).status();
            let new_app = x.join("Darkstar.app");
            if !matches!(ok, Ok(s) if s.success()) || !new_app.join("Contents/MacOS/darkstar_sim").exists() {
                return me.set(State::Error("archive invalide".into()));
            }
            let pid = std::process::id();
            let q = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "'\\''"));
            let script = tmp.join("swap.sh");
            let body = format!(
                "#!/bin/sh\n\
                 while kill -0 {pid} 2>/dev/null; do sleep 0.2; done\n\
                 rm -rf {old}\n\
                 if mv {app} {old} && mv {new} {app}; then rm -rf {old}; else mv {old} {app} 2>/dev/null; fi\n\
                 xattr -dr com.apple.quarantine {app} 2>/dev/null\n\
                 open {app}\n",
                app = q(&app),
                old = q(&app.with_extension("app.old")),
                new = q(&new_app),
            );
            if let Err(e) = std::fs::write(&script, body) {
                return me.set(State::Error(format!("script : {e}")));
            }
            me.set(State::Ready { script });
        });
    }

    /// Lance le script de remplacement (détaché) ; l'appelant doit ensuite quitter le jeu.
    pub fn launch_swap(script: &Path) -> bool {
        Command::new("/bin/sh").arg(script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_json() {
        assert!(parse_version("v1.2.10") > parse_version("1.2.9"));
        let j = r#"{"tag_name":"v1.4.0","body":"Nouveautés\nligne 2","assets":[{"browser_download_url":"https://x/a.txt"},{"browser_download_url":"https://x/Darkstar-macos.zip"}]}"#;
        assert_eq!(json_str(j, "tag_name", 0).unwrap().0, "v1.4.0");
        let (u1, p) = json_str(j, "browser_download_url", 0).unwrap();
        assert_eq!(u1, "https://x/a.txt");
        assert_eq!(json_str(j, "browser_download_url", p).unwrap().0, "https://x/Darkstar-macos.zip");
    }
}
