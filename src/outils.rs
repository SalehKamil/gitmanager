//! Outils du poste : Git (derniere version, empreinte SHA-256), GitHub CLI, Git
//! Credential Manager, navigateur et gestionnaire de fichiers.

use crate::affichage::{c, err, info, ok, warn};
use crate::config::{maison, Config};
use crate::git::{git, run};
use crate::saisie::ask_yn;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const VERIF_GIT_JOURS: i64 = 7; // recherche d'une nouvelle version de Git une fois par semaine

fn nom_outil(outil: &str) -> &'static str {
    if outil == "git" { "Git" } else { "GitHub CLI" }
}

fn paquet(outil: &str, g: &str) -> &'static str {
    match (outil, g) {
        ("git", "winget") => "Git.Git",
        ("git", _) => "git",
        ("gh", "winget") => "GitHub.cli",
        ("gh", "pacman") => "github-cli",
        _ => "gh",
    }
}

/// Chemin d'un programme du PATH, avec les extensions de Windows.
pub fn ou_est(prog: &str) -> Option<PathBuf> {
    let chemins = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or(".EXE;.CMD;.BAT".into()).split(';').map(|e| e.to_lowercase()).collect()
    } else {
        vec![String::new()]
    };
    for d in std::env::split_paths(&chemins) {
        for e in &exts {
            let p = d.join(format!("{prog}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Gestionnaire de paquets du systeme. Il verifie lui-meme la signature ou
/// l'empreinte de ce qu'il telecharge.
pub fn gestionnaire_paquets() -> Option<&'static str> {
    let candidats: &[&'static str] = if cfg!(windows) {
        &["winget"]
    } else if cfg!(target_os = "macos") {
        &["brew"]
    } else {
        &["apt-get", "dnf", "pacman", "zypper"]
    };
    candidats.iter().copied().find(|g| ou_est(g).is_some())
}

#[cfg(unix)]
fn root() -> bool {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() == 0 }
}

#[cfg(not(unix))]
fn root() -> bool {
    false
}

/// Commandes qui installent (ou mettent a jour) la derniere version d'un paquet.
pub fn commandes_paquet(g: &str, p: &str, mise_a_jour: bool) -> Vec<Vec<String>> {
    let sudo: Vec<String> = if cfg!(windows) || g == "brew" || root() { vec![] } else { vec!["sudo".into()] };
    let v = |a: &[&str]| -> Vec<String> { sudo.iter().cloned().chain(a.iter().map(|s| s.to_string())).collect() };
    match g {
        "winget" => vec![[if mise_a_jour { "upgrade" } else { "install" }, "--id", p, "-e", "--source", "winget",
                          "--accept-source-agreements", "--accept-package-agreements"]
                         .iter().fold(vec!["winget".to_string()], |mut a, s| { a.push(s.to_string()); a })],
        "brew" => vec![vec!["brew".into(), "install".into(), p.into()]],
        "apt-get" => vec![v(&["apt-get", "update"]), v(&["apt-get", "install", "-y", p])],
        "dnf" => vec![v(&["dnf", if mise_a_jour { "upgrade" } else { "install" }, "-y", p])],
        "pacman" => vec![v(&["pacman", "-S", "--noconfirm", "--needed", p])],
        "zypper" => vec![v(&["zypper", "--non-interactive", "install", p])],
        _ => vec![],
    }
}

/// (majeur, mineur) de Git, ou None s'il n'est pas installe.
pub fn version_git() -> Option<(u32, u32)> {
    ou_est("git")?;
    let (_, out, _) = run(&["git", "--version"], None, None);
    let m = regex::Regex::new(r"(\d+)\.(\d+)").unwrap().captures(&out);
    Some(m.map(|m| (m[1].parse().unwrap_or(0), m[2].parse().unwrap_or(0))).unwrap_or((0, 0)))
}

/// Juste apres une installation sous Windows, le PATH du terminal n'est pas a jour.
pub fn actualiser_path() {
    if !cfg!(windows) {
        return;
    }
    let programmes = std::env::var("ProgramFiles").unwrap_or(r"C:\Program Files".into());
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    for d in [Path::new(&programmes).join("Git").join("cmd"), Path::new(&local).join("Programs").join("Git").join("cmd")] {
        let path = std::env::var("PATH").unwrap_or_default();
        if d.is_dir() && !path.contains(&d.display().to_string()) {
            std::env::set_var("PATH", format!("{};{path}", d.display()));
        }
    }
}

/// Sortie d'une commande d'information, sans bloquer plus d'une minute.
pub fn sortie_cmd(cmd: &[&str]) -> (i32, String) {
    let Ok(mut enfant) = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stdout(Stdio::piped())
        .stderr(Stdio::null()).spawn() else { return (-1, String::new()) };
    let mut out = enfant.stdout.take().unwrap();
    let lecteur = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut s);
        s
    });
    let debut = Instant::now();
    let code = loop {
        match enfant.try_wait() {
            Ok(Some(s)) => break s.code().unwrap_or(-1),
            Ok(None) if debut.elapsed() < Duration::from_secs(60) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = enfant.kill();
                return (-1, String::new());
            }
        }
    };
    (code, String::from_utf8_lossy(&lecteur.join().unwrap_or_default()).into_owned())
}

pub fn empreinte_fichier(chemin: &Path) -> String {
    let mut h = Sha256::new();
    if let Ok(mut f) = std::fs::File::open(chemin) {
        let mut bloc = vec![0u8; 1 << 20];
        while let Ok(n) = std::io::Read::read(&mut f, &mut bloc) {
            if n == 0 {
                break;
            }
            h.update(&bloc[..n]);
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Empreinte SHA-256 du paquet Git, celle que le gestionnaire a verifiee.
pub fn empreinte_paquet(g: &str) -> String {
    let hexa = regex::Regex::new(r"\b[0-9A-Fa-f]{64}\b").unwrap();
    if g == "winget" {
        let (_, out) = sortie_cmd(&["winget", "show", "--id", "Git.Git", "-e", "--source", "winget",
                                    "--accept-source-agreements"]);
        return hexa.find(&out).map(|m| m.as_str().to_lowercase()).unwrap_or_default();
    }
    if g == "apt-get" {
        let (_, version) = sortie_cmd(&["dpkg-query", "-W", "-f=${Version}", "git"]);
        let (_, out) = sortie_cmd(&["apt-cache", "show", "git"]);
        for bloc in out.split("\n\n") {
            if !version.trim().is_empty() && format!("\n{bloc}\n").contains(&format!("\nVersion: {}\n", version.trim())) {
                return regex::Regex::new(r"(?m)^SHA256: ([0-9a-f]{64})$").unwrap()
                    .captures(bloc).map(|m| m[1].to_string()).unwrap_or_default();
            }
        }
    }
    String::new()
}

/// Version installee et empreintes SHA-256, pour verifier l'installation.
pub fn afficher_empreinte_git(g: &str) {
    let (_, version, _) = git(&["--version"], None);
    ok(&format!("{version} : installation OK."));
    let p = empreinte_paquet(g);
    if !p.is_empty() {
        info(&format!("Empreinte SHA-256 du paquet, selon {g} :"));
        println!("       {p}");
        if g == "winget" {
            info("Pour comparer : https://github.com/git-for-windows/git/releases");
        }
    }
    if let Some(exe) = ou_est("git") {
        info("Empreinte SHA-256 du programme git :");
        println!("       {}", empreinte_fichier(&std::fs::canonicalize(&exe).unwrap_or(exe)));
    }
}

/// True si le gestionnaire de paquets propose une version plus recente de Git.
pub fn mise_a_jour_git_dispo(g: &str) -> bool {
    match g {
        "winget" => {
            let (code, out) = sortie_cmd(&["winget", "list", "--id", "Git.Git", "-e", "--upgrade-available",
                                           "--accept-source-agreements"]);
            code == 0 && out.contains("Git.Git")
        }
        "brew" => sortie_cmd(&["brew", "outdated", "--quiet", "--formula"]).1.split_whitespace().any(|x| x == "git"),
        "apt-get" => sortie_cmd(&["apt", "list", "--upgradable"]).1.lines().any(|l| l.starts_with("git/")),
        "dnf" => sortie_cmd(&["dnf", "-q", "check-update", "git"]).0 == 100,
        "pacman" => sortie_cmd(&["pacman", "-Qu", "git"]).0 == 0,
        "zypper" => regex::Regex::new(r"\|\s*git\s*\|").unwrap().is_match(&sortie_cmd(&["zypper", "-q", "list-updates"]).1),
        _ => false,
    }
}

fn adresse_outil(outil: &str) -> &'static str {
    if outil == "git" { "https://git-scm.com/downloads" } else { "https://cli.github.com" }
}

/// Installe ou met a jour un outil avec le gestionnaire de paquets du systeme.
/// Retourne true si l'outil est pret.
pub fn installer_outil(outil: &str, mise_a_jour: bool) -> bool {
    let nom = nom_outil(outil);
    let Some(g) = gestionnaire_paquets() else {
        if outil == "git" && cfg!(target_os = "macos") {
            info("Les outils Xcode d'Apple contiennent Git : suivre les instructions.");
            if ask_yn("Les installer maintenant ?", true) {
                run(&["xcode-select", "--install"], None, None);
                info("Relancer le programme une fois l'installation finie.");
            }
            return false;
        }
        info(&format!("Installer {nom} depuis {}, puis relancer.", adresse_outil(outil)));
        return false;
    };
    let verbe = if mise_a_jour { "Actualiser" } else { "Installer" };
    if !ask_yn(&format!("{verbe} {nom}, version la plus neuve, avec {g} ?"), true) {
        return false;
    }
    info("Installation en cours, cela peut prendre quelques minutes...");
    let cmds = commandes_paquet(g, paquet(outil, g), mise_a_jour);
    if cmds.iter().any(|c| c.first().map(String::as_str) == Some("sudo")) {
        info("sudo peut demander le mot de passe de cette machine.");
    }
    for cmd in &cmds {
        println!("{}", c(&format!("  $ {}", cmd.join(" ")), "36"));
        let p = Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::inherit()).output();
        let code = p.as_ref().map(|o| o.status.code().unwrap_or(-1)).unwrap_or(-1);
        if code != 0 {
            warn(&format!("{g} : code de sortie {code}."));
            if crate::debug() {
                if let Ok(o) = &p {
                    let t = String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr);
                    println!("{}", t.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect::<String>());
                }
            }
        }
    }
    actualiser_path();
    let pret = if outil == "gh" {
        let pret = chemin_gh().is_some();
        if pret {
            ok("GitHub CLI : installation OK.");
        }
        pret
    } else {
        let pret = version_git().unwrap_or((0, 0)) >= (2, 31);
        if pret {
            afficher_empreinte_git(g);
        }
        pret
    };
    if !pret && cfg!(windows) {
        warn(&format!("{nom} : installation OK, mais pas encore visible. Fermer ce terminal, \
le rouvrir et relancer le programme."));
    } else if !pret {
        err(&format!("L'installation de {nom} avec {g} n'a pas abouti."));
        info(&format!("L'installer depuis {}, puis relancer.", adresse_outil(outil)));
    }
    pret
}

/// Au premier lancement, installe ce qui manque pour que tout soit pret.
/// Ensuite, une fois par semaine, propose la derniere version de Git.
pub fn preparer_outils(cfg: &mut Config) -> bool {
    let version = version_git();
    let aujourd_hui = chrono::Local::now().date_naive();
    if version.is_none_or(|v| v < (2, 31)) {
        match version {
            None => warn("Git manque sur ce poste."),
            Some((a, b)) => warn(&format!("Git {a}.{b} est trop ancien : il faut la version 2.31 ou plus.")),
        }
        if !installer_outil("git", version.is_some()) {
            return false;
        }
        cfg.set("verif_git", aujourd_hui.format("%Y-%m-%d").to_string());
    } else {
        let ecart = chrono::NaiveDate::parse_from_str(&cfg.texte("verif_git"), "%Y-%m-%d")
            .map(|d| (aujourd_hui - d).num_days())
            .unwrap_or(VERIF_GIT_JOURS);
        if let Some(g) = gestionnaire_paquets().filter(|_| ecart >= VERIF_GIT_JOURS) {
            cfg.set("verif_git", aujourd_hui.format("%Y-%m-%d").to_string());
            cfg.sauver();
            info("Recherche d'une nouvelle version de Git...");
            if mise_a_jour_git_dispo(g) {
                warn("Une nouvelle version de Git est disponible.");
                installer_outil("git", true);
            }
        }
    }
    cfg.sauver();
    verifier_identifiants_en_clair(cfg);
    true
}

/// Alerte si Git ecrit des identifiants en clair sur le poste : reglage
/// credential.helper store et fichier ~/.git-credentials.
pub fn verifier_identifiants_en_clair(cfg: &mut Config) {
    let (_, out, _) = git(&["config", "--global", "--get-regexp", r"^credential(\..+)?\.helper$"], None);
    let stores: Vec<String> = out.lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(_, v)| v.trim().starts_with("store"))
        .map(|(k, _)| k.to_string())
        .collect();
    let fichier = maison().join(".git-credentials");
    if stores.is_empty() && !fichier.exists() {
        return;
    }
    warn("Git garde des identifiants en clair sur ce poste.");
    if cfg.vrai("store_garde") {
        info("Option credential.helper store toujours active, sur demande : ~/.git-credentials.");
        return;
    }
    let mut garde = false;
    if !stores.is_empty() {
        if ask_yn("Retirer l'option credential.helper store de Git ?", true) {
            for cle in &stores {
                git(&["config", "--global", "--unset-all", cle, "^store"], None);
            }
            ok("Option store : retrait OK.");
        } else {
            garde = true;
        }
    }
    if fichier.exists() {
        if ask_yn(&format!("Supprimer {}, qui contient ces identifiants en clair ?", fichier.display()), true) {
            let _ = std::fs::remove_file(&fichier);
            ok("Fichier : suppression OK.");
            info("Retirer aussi sur GitHub les jetons qu'il contenait : https://github.com/settings/tokens");
        } else {
            garde = true;
        }
    }
    if garde {
        cfg.set("store_garde", true);
        cfg.sauver();
    }
}

pub fn chemin_gh() -> Option<PathBuf> {
    if let Some(p) = ou_est("gh") {
        return Some(p);
    }
    if cfg!(windows) {
        // juste apres un winget install, le PATH n'est pas a jour
        for base in [std::env::var("ProgramFiles").unwrap_or(r"C:\Program Files".into()),
                     std::env::var("LOCALAPPDATA").unwrap_or_default()] {
            let cand = Path::new(&base).join("GitHub CLI").join("gh.exe");
            if !base.is_empty() && cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

static CACHE_GCM: OnceLock<Option<String>> = OnceLock::new();

/// Git Credential Manager du poste : manager ou manager-core, sinon None.
pub fn helper_gcm() -> Option<String> {
    CACHE_GCM.get_or_init(|| {
        ["manager", "manager-core"].into_iter()
            .find(|nom| git(&[&format!("credential-{nom}"), "--version"], None).0 == 0)
            .map(str::to_string)
    }).clone()
}

pub fn methode_dispo(cfg: &Config) -> Option<&'static str> {
    let pref = cfg.texte(&crate::session::cle("methode", None));
    if pref == "gh" && chemin_gh().is_some() {
        return Some("gh");
    }
    if pref == "gcm" && helper_gcm().is_some() {
        return Some("gcm");
    }
    if chemin_gh().is_some() {
        return Some("gh");
    }
    helper_gcm().map(|_| "gcm")
}

fn sans_navigateur() -> bool {
    std::env::var_os("GITMANAGER_SANS_NAVIGATEUR").is_some_and(|v| !v.is_empty())
}

fn bureau_graphique() -> bool {
    cfg!(any(windows, target_os = "macos"))
        || std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// Ouvre la page dans le navigateur et affiche son adresse, utile sans
/// navigateur ou sur un poste distant.
pub fn ouvrir_navigateur(url: &str) {
    info(&format!("Ouverture dans le navigateur : {url}"));
    if sans_navigateur() {
        return;
    }
    if !bureau_graphique() && std::env::var_os("BROWSER").is_none() || open::that_detached(url).is_err() {
        warn("Aucun navigateur disponible : copier l'adresse ci-dessus.");
    }
}

/// Ouvre le dossier dans le gestionnaire de fichiers du systeme (Explorateur
/// sous Windows, Finder sous macOS, celui du bureau sous Linux), sans bloquer.
pub fn ouvrir_explorateur(chemin: &Path) {
    if sans_navigateur() || !bureau_graphique() {
        info(&format!("Pas de bureau graphique : dossier {}", chemin.display()));
        return;
    }
    match open::that_detached(chemin) {
        Ok(()) => ok(&format!("Dossier ouvert dans le gestionnaire de fichiers : {}", chemin.display())),
        Err(_) => warn(&format!("Ouverture du dossier impossible : {}", chemin.display())),
    }
}
