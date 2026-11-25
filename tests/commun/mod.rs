//! Banc d'essai : de vrais repos git, un faux GitHub, le vrai programme.
//!
//! Les repos "en ligne" sont des repos nus dans un dossier temporaire : Git
//! remplace https://github.com/ par ce dossier (url.<base>.insteadOf), donc les
//! adresses restent celles de GitHub. L'API GitHub est un petit serveur local.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Faux {
    pub login: String,
    pub repos: Vec<Value>,
    pub supprimes: HashSet<String>,
    /// un repo supprime reste dans la liste, comme GitHub juste apres
    pub retard_liste: bool,
    pub appels: Vec<(String, String)>,
    /// None : tout jeton peut supprimer
    pub jetons_admin: Option<HashSet<String>>,
    pub distants: PathBuf,
    /// repo supprime sur GitHub juste apres sa lecture, comme dans le navigateur
    pub disparait: Option<String>,
}

impl Faux {
    pub fn ajouter(&mut self, nom: &str, admin: bool) -> Value {
        let full = format!("{}/{nom}", self.login);
        let d = json!({
            "name": nom, "full_name": full, "private": true,
            "html_url": format!("https://github.com/{full}"),
            "clone_url": format!("https://github.com/{full}.git"),
            "default_branch": "main", "pushed_at": "2026-11-20T10:00:00Z",
            "permissions": {"admin": admin, "push": true, "pull": true},
        });
        self.repos.retain(|r| r["full_name"] != d["full_name"]);
        self.repos.push(d.clone());
        d
    }

    pub fn repo(&self, full: &str) -> Value {
        self.repos.iter().find(|r| r["full_name"] == full).cloned().unwrap_or(Value::Null)
    }

    fn repondre(&mut self, verbe: &str, chemin: &str, jeton: &str, corps: &str) -> (u16, Value, Vec<(String, String)>) {
        self.appels.push((verbe.to_string(), chemin.to_string()));
        let base = chemin.split('?').next().unwrap_or("");
        if base == "/user" {
            return (200, json!({"login": self.login, "name": "Saleh Kamil", "id": 1}), vec![]);
        }
        if base == "/user/repos" && verbe == "GET" {
            let liste: Vec<Value> = self.repos.iter()
                .filter(|r| self.retard_liste || !self.supprimes.contains(r["full_name"].as_str().unwrap()))
                .cloned().collect();
            return (200, Value::Array(liste), vec![]);
        }
        if base == "/user/repos" && verbe == "POST" {
            let data: Value = serde_json::from_str(corps).unwrap_or(Value::Null);
            let nom = data["name"].as_str().unwrap_or("x").to_string();
            let mut d = self.ajouter(&nom, true);
            d["private"] = data["private"].clone();
            self.supprimes.remove(d["full_name"].as_str().unwrap());
            let nu = self.distants.join(&self.login).join(format!("{nom}.git"));
            let _ = std::fs::remove_dir_all(&nu); // nouveau repo : rien de l'ancien
            Command::new("git").args(["init", "-q", "--bare", "-b", "main"]).arg(&nu).status().unwrap();
            return (201, d, vec![]);
        }
        if let Some(reste) = base.strip_prefix("/repos/") {
            let morceaux: Vec<&str> = reste.split('/').collect();
            let full = format!("{}/{}", morceaux[0], morceaux.get(1).unwrap_or(&""));
            let d = self.repo(&full);
            if d.is_null() || self.supprimes.contains(&full) {
                return (404, json!({"message": "Not Found"}), vec![]);
            }
            if morceaux.len() == 2 && verbe == "DELETE" {
                if let Some(admins) = &self.jetons_admin {
                    if !admins.contains(jeton) {
                        return (403, json!({"message": "Must have admin rights to Repository."}),
                                vec![("X-Accepted-OAuth-Scopes".into(), "delete_repo".into()),
                                     ("X-OAuth-Scopes".into(), "repo, gist, read:org".into())]);
                    }
                }
                self.supprimes.insert(full);
                return (204, Value::Null, vec![]);
            }
            if morceaux.len() == 2 && verbe == "PATCH" {
                let data: Value = serde_json::from_str(corps).unwrap_or(Value::Null);
                if let Some(r) = self.repos.iter_mut().find(|r| r["full_name"] == full) {
                    r["private"] = data["private"].clone();
                    return (200, r.clone(), vec![]);
                }
            }
            if morceaux.len() == 2 && verbe == "GET" {
                if self.disparait.as_deref() == Some(full.as_str()) {
                    self.supprimes.insert(full);
                    self.retard_liste = false;
                }
                return (200, d, vec![]);
            }
        }
        (404, json!({"message": "Not Found"}), vec![])
    }
}

pub struct Sortie {
    pub texte: String,
    pub code: i32,
}

impl Sortie {
    /// Le programme s'est arrete par le choix Quitter, sans question en trop.
    pub fn fini(&self) -> bool {
        self.texte.contains("Fin du programme.") && !self.texte.contains("Interrompu.")
    }
}

pub struct Banc {
    _tmp: tempfile::TempDir,
    pub tmp: PathBuf,
    pub maison: PathBuf,
    pub distants: PathBuf,
    pub espace: PathBuf,
    pub gitconfig: PathBuf,
    pub github: Arc<Mutex<Faux>>,
    pub api: String,
    pub chemin_en_plus: Vec<PathBuf>,
}

impl Banc {
    pub fn new() -> Banc {
        let tmp_dir = tempfile::Builder::new().prefix("gm-essai-").tempdir().unwrap();
        // chemin reel (macOS : /private/var), sans la forme \\?\ de Windows, que Git ne lit pas
        let reel = tmp_dir.path().canonicalize().unwrap().display().to_string();
        let tmp = PathBuf::from(reel.strip_prefix(r"\\?\").unwrap_or(&reel));
        let (maison, distants) = (tmp.join("maison"), tmp.join("github"));
        let espace = maison.join("projets");
        for d in [&maison, &espace, &distants.join("saleh")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let gitconfig = tmp.join("gitconfig");
        let base = format!("file:///{}/", distants.display().to_string().replace('\\', "/").trim_start_matches('/'));
        std::fs::write(&gitconfig, format!(
            "[user]\n\tname = Essai\n\temail = essai@example.com\n[init]\n\tdefaultBranch = main\n\
             [url \"{base}\"]\n\tinsteadOf = https://github.com/\n")).unwrap();
        let aujourd_hui = chrono::Local::now().format("%Y-%m-%d").to_string();
        let cfg = json!({"verif_git": aujourd_hui, "espace": espace.display().to_string()});
        std::fs::write(maison.join(".gitmanager.json"), serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
        let github = Arc::new(Mutex::new(Faux {
            login: "saleh".into(), repos: vec![], supprimes: HashSet::new(), retard_liste: true,
            appels: vec![], jetons_admin: None, distants: distants.clone(), disparait: None,
        }));
        let serveur = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let api = format!("http://{}", serveur.server_addr().to_ip().unwrap());
        let gh = github.clone();
        std::thread::spawn(move || {
            for mut req in serveur.incoming_requests() {
                let mut corps = String::new();
                let _ = req.as_reader().read_to_string(&mut corps);
                let jeton = req.headers().iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.as_str().trim_start_matches("Bearer ").to_string())
                    .unwrap_or_default();
                let (code, rep, entetes) = gh.lock().unwrap()
                    .repondre(req.method().as_str(), req.url(), &jeton, &corps);
                let texte = if rep.is_null() { String::new() } else { rep.to_string() };
                let mut r = tiny_http::Response::from_string(texte).with_status_code(code);
                for (k, v) in entetes {
                    r.add_header(tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
                }
                let _ = req.respond(r);
            }
        });
        Banc { _tmp: tmp_dir, tmp, maison, distants, espace, gitconfig, github, api, chemin_en_plus: vec![] }
    }

    pub fn sh(&self, args: &[&str], cwd: Option<&Path>) -> String {
        let mut c = Command::new(args[0]);
        c.args(&args[1..]).env("GIT_CONFIG_GLOBAL", &self.gitconfig).env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", &self.maison);
        if let Some(d) = cwd {
            c.current_dir(d);
        }
        let o = c.output().unwrap();
        assert!(o.status.success(), "{args:?} : {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    /// Repo nu "en ligne" avec un premier commit, et des branches au choix ;
    /// retourne le dossier de travail de "quelqu'un d'autre".
    pub fn distant(&self, nom: &str, fichiers: &[(&str, &str)], branches: &[(&str, &str, &str)]) -> PathBuf {
        let nu = self.distants.join("saleh").join(format!("{nom}.git"));
        self.sh(&["git", "init", "-q", "--bare", "-b", "main", nu.to_str().unwrap()], None);
        let travail = self.tmp.join(format!("travail-{nom}"));
        self.sh(&["git", "clone", "-q", nu.to_str().unwrap(), travail.to_str().unwrap()], None);
        let fichiers = if fichiers.is_empty() { vec![("README.md", "# demo\n")] } else { fichiers.to_vec() };
        for (f, t) in fichiers {
            std::fs::write(travail.join(f), t).unwrap();
        }
        self.sh(&["git", "add", "-A"], Some(&travail));
        self.sh(&["git", "commit", "-q", "-m", "Premier commit"], Some(&travail));
        self.sh(&["git", "push", "-q", "origin", "main"], Some(&travail));
        for (b, f, t) in branches {
            self.sh(&["git", "switch", "-q", "-c", b], Some(&travail));
            std::fs::write(travail.join(f), t).unwrap();
            self.sh(&["git", "add", "-A"], Some(&travail));
            self.sh(&["git", "commit", "-q", "-m", &format!("Commit sur {b}")], Some(&travail));
            self.sh(&["git", "push", "-q", "origin", b], Some(&travail));
            self.sh(&["git", "switch", "-q", "main"], Some(&travail));
        }
        self.github.lock().unwrap().ajouter(nom, true);
        travail
    }

    /// Quelqu'un d'autre envoie un commit sur GitHub.
    pub fn autre_push(&self, travail: &Path, fichier: &str, texte: &str) {
        std::fs::write(travail.join(fichier), texte).unwrap();
        self.sh(&["git", "add", "-A"], Some(travail));
        self.sh(&["git", "commit", "-q", "-m", "Changement en ligne"], Some(travail));
        self.sh(&["git", "push", "-q", "origin", "HEAD"], Some(travail));
    }

    /// Copie du repo sur la machine, dans le dossier de travail.
    pub fn copie(&self, nom: &str) -> PathBuf {
        let dest = self.espace.join(nom);
        self.sh(&["git", "clone", "-q", &format!("https://github.com/saleh/{nom}.git"), dest.to_str().unwrap()], None);
        dest
    }

    pub fn config(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.maison.join(".gitmanager.json")).unwrap()).unwrap()
    }

    pub fn ecrire_config(&self, cfg: &Value) {
        std::fs::write(self.maison.join(".gitmanager.json"), serde_json::to_string_pretty(cfg).unwrap()).unwrap();
    }

    /// GitHub CLI factice dans le PATH : connexion par code, jeton, deconnexion.
    /// Retourne le fichier qui dit qu'il est connecte.
    #[cfg(unix)]
    pub fn faux_gh(&mut self) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dossier = self.tmp.join("bin");
        std::fs::create_dir_all(&dossier).unwrap();
        let etat = self.tmp.join("gh-connecte");
        let e = etat.display();
        let script = dossier.join("gh");
        std::fs::write(&script, format!(r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo "gh version 2.89.0 (2026-03-26)" ;;
  "auth login"|"auth refresh")
    echo "! First copy your one-time code: ABCD-1234" >&2
    echo "Open this URL to continue in your web browser: https://github.com/login/device" >&2
    echo "Authentication complete." >&2
    touch "{e}" ;;
  "auth token") [ -f "{e}" ] && echo "jeton-gh" || exit 1 ;;
  "auth setup-git") ;;
  "auth status") [ -f "{e}" ] && echo "Logged in to github.com account saleh" \
      || {{ echo "You are not logged into any GitHub hosts." >&2; exit 1; }} ;;
  "auth logout") rm -f "{e}" ;;
esac
"#)).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        self.chemin_en_plus.push(dossier);
        etat
    }

    /// Lance gitmanager avec ces reponses aux questions, puis la fin de l'entree.
    pub fn lancer(&self, reponses: &[&str]) -> Sortie {
        let mut chemin = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
        for d in self.chemin_en_plus.iter().rev() {
            chemin.insert(0, d.clone());
        }
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_gitmanager"));
        cmd.env("HOME", &self.maison).env("USERPROFILE", &self.maison)
            .env("GIT_CONFIG_GLOBAL", &self.gitconfig).env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GITMANAGER_API", &self.api).env("GITMANAGER_SANS_NAVIGATEUR", "1")
            .env("NO_COLOR", "1").env("COLUMNS", "120").env("PATH", std::env::join_paths(chemin).unwrap())
            .env_remove("GITHUB_TOKEN").env_remove("GH_TOKEN").env_remove("XDG_CONFIG_HOME")
            .env_remove("DISPLAY").env_remove("WAYLAND_DISPLAY").env_remove("FORCE_COLOR")
            .env_remove("GITMANAGER_DEBUG")
            .current_dir(&self.tmp)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut enfant = cmd.spawn().unwrap();
        let mut entree = enfant.stdin.take().unwrap();
        let texte = reponses.iter().map(|r| format!("{r}\n")).collect::<String>();
        let _ = entree.write_all(texte.as_bytes());
        drop(entree);
        let mut out = enfant.stdout.take().unwrap();
        let lecteur = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = out.read_to_end(&mut s);
            s
        });
        let debut = Instant::now();
        let code = loop {
            if let Some(s) = enfant.try_wait().unwrap() {
                break s.code().unwrap_or(-1);
            }
            if debut.elapsed() > Duration::from_secs(120) {
                let _ = enfant.kill();
                panic!("gitmanager bloque : plus de 2 minutes");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let mut err = String::new();
        let _ = enfant.stderr.take().unwrap().read_to_string(&mut err);
        let texte = String::from_utf8_lossy(&lecteur.join().unwrap()).into_owned() + &err;
        if std::env::var_os("GM_VOIR").is_some() {
            println!("{texte}");
        }
        Sortie { texte, code }
    }

    /// Connexion par jeton, puis ces reponses.
    pub fn session(&self, reponses: &[&str]) -> Sortie {
        let mut tout = vec!["1", "2", "jeton"];
        tout.extend_from_slice(reponses);
        self.lancer(&tout)
    }
}
