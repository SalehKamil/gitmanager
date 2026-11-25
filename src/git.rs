//! Git : execution, affichage des commandes, messages en francais, etat d'un repo.

use crate::affichage::{c, err, info, ok, sortie, warn};
use crate::saisie::ask_yn;
use crate::session;
use crate::urls::cle_depot;
use regex::Regex;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

/// Modifications de l'environnement d'une commande : (variable, valeur ou retrait).
pub type Env = Vec<(String, Option<String>)>;

// Aide d'identification git qui lit le jeton dans l'environnement du processus :
// le jeton n'apparait ni dans la ligne de commande, ni dans un fichier.
const AIDE_JETON: &str = "!f() { test \"$1\" = get || exit 0; \
echo \"username=${GM_UTILISATEUR:-x-access-token}\"; echo \"password=$GM_JETON\"; }; f";

/// Environnement de chaque commande git : HTTPS ou SSH seulement, certificats
/// verifies, identifiants jamais sur la ligne de commande.
pub fn env_git(identite: Option<(String, String)>, anonyme: bool) -> Env {
    let mut env: Env = vec![("GIT_SSL_NO_VERIFY".into(), None),
                            ("GIT_ALLOW_PROTOCOL".into(), Some("https:ssh:file".into()))];
    let mut config = vec![("http.sslVerify".to_string(), "true".to_string())];
    let identite = if anonyme { None } else { identite.or_else(session::identite_courante) };
    if anonyme || identite.is_some() {
        config.push(("credential.helper".into(), String::new())); // ignore les gestionnaires du PC
    }
    if let Some((u, j)) = &identite {
        config.push(("credential.helper".into(), AIDE_JETON.into()));
        env.push(("GM_UTILISATEUR".into(), Some(u.clone())));
        env.push(("GM_JETON".into(), Some(j.clone())));
    }
    let base: usize = std::env::var("GIT_CONFIG_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    for (i, (cle, valeur)) in config.iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{}", base + i), Some(cle.clone())));
        env.push((format!("GIT_CONFIG_VALUE_{}", base + i), Some(valeur.clone())));
    }
    env.push(("GIT_CONFIG_COUNT".into(), Some((base + config.len()).to_string())));
    // Jamais d'invite "Username / Password" : l'acces passe par la connexion
    // navigateur, GitHub CLI ou Git Credential Manager, ou par un jeton.
    env.push(("GIT_TERMINAL_PROMPT".into(), Some("0".into())));
    if anonyme {
        env.push(("GCM_INTERACTIVE".into(), Some("never".into())));
    }
    env
}

pub fn appliquer_env(cmd: &mut Command, env: &Env) {
    for (cle, valeur) in env {
        match valeur {
            Some(v) => cmd.env(cle, v),
            None => cmd.env_remove(cle),
        };
    }
}

/// Lance une commande ; retourne (code, sortie, erreurs). Sans le programme : code 127.
pub fn run(cmd: &[&str], entree: Option<&str>, env: Option<&Env>) -> (i32, String, String) {
    let mut c = Command::new(cmd[0]);
    c.args(&cmd[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.stdin(if entree.is_some() { Stdio::piped() } else { Stdio::null() });
    if let Some(env) = env {
        appliquer_env(&mut c, env);
    }
    let Ok(mut enfant) = c.spawn() else { return (127, String::new(), String::new()) };
    if let (Some(texte), Some(mut stdin)) = (entree, enfant.stdin.take()) {
        use std::io::Write;
        let _ = stdin.write_all(texte.as_bytes());
    }
    match enfant.wait_with_output() {
        Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(),
                  String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(_) => (127, String::new(), String::new()),
    }
}

fn affiche(args: &[&str]) -> String {
    args.iter().map(|a| if a.contains(' ') || a.is_empty() { format!("\"{a}\"") } else { a.to_string() })
        .collect::<Vec<_>>().join(" ")
}

pub fn sous_commande<'a>(args: &[&'a str]) -> &'a str {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-c" {
            i += 2;
        } else if args[i].starts_with('-') {
            i += 1;
        } else {
            return args[i];
        }
    }
    ""
}

/// $ git <action> <reste> : l'action Git officielle en rouge, le reste en cyan.
pub fn ligne_commande(args: &[&str], actif: bool) -> String {
    use crate::affichage::peindre;
    let sc = sous_commande(args);
    let i = args.iter().position(|a| *a == sc).map(|p| p + 1).unwrap_or(0);
    let avant = if i > 1 { affiche(&args[..i - 1]) } else { String::new() };
    let apres = affiche(&args[i.min(args.len())..]);
    let mut ligne = peindre("  $ git ", "36", actif);
    if !avant.is_empty() {
        ligne += &peindre(&format!("{avant} "), "36", actif);
    }
    ligne += &peindre(sc, "1;31", actif);
    if !apres.is_empty() {
        ligne += &peindre(&format!(" {apres}"), "36", actif);
    }
    ligne
}

pub fn afficher_commande(args: &[&str]) {
    println!("{}", ligne_commande(args, crate::affichage::couleur_active()));
}

// Commandes Git affichees avec leur sortie : celles qui changent un repo, sa
// config ou parlent au serveur, jamais les simples lectures.
const ACTIONS_GIT: [&str; 23] = ["add", "commit", "push", "pull", "fetch", "clone", "init", "merge", "rebase",
    "revert", "restore", "reset", "rm", "remote", "checkout", "switch", "branch", "stash", "tag", "update-ref",
    "checkout-index", "cherry-pick", "mv"];

/// Commande Git qui change un repo, sa config ou parle au serveur.
pub fn commande_visible(args: &[&str]) -> bool {
    let sc = sous_commande(args);
    let suite: Vec<&str> = match args.iter().position(|a| *a == sc) {
        Some(p) if !sc.is_empty() => args[p + 1..].to_vec(),
        _ => vec![],
    };
    match sc {
        "config" => !suite.iter().any(|a| a.starts_with("--get") || a.starts_with("--list") || a.starts_with("-l")),
        "stash" => suite.first().is_some_and(|a| ["push", "pop", "drop", "apply"].contains(a)),
        "remote" => suite.first().is_some_and(|a| ["add", "set-url", "remove", "rm", "rename"].contains(a)),
        "branch" => suite.iter().any(|a| ["-m", "-M", "-d", "-D", "--set-upstream-to"].contains(a)
                                         || a.starts_with("--set-upstream-to")),
        // git symbolic-ref HEAD refs/heads/x change la branche ; sinon lecture
        "symbolic-ref" => suite.iter().filter(|a| !a.starts_with('-')).count() == 2,
        _ => ACTIONS_GIT.contains(&sc) || sc == "clean",
    }
}

static BRUIT_GIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(LF will be replaced by CRLF|CRLF will be replaced by LF)").unwrap()
});

// Messages courants de git, en francais ; les autres ne s'affichent pas
static TRADUCTIONS_GIT: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"^\[(\S+)(?: \(root-commit\))? ([0-9a-f]{7,})\] (.*)$", "commit {2} sur {1} : {3}"),
        (r"^(\d+) files? changed(?:, (\d+) insertions?\(\+\))?(?:, (\d+) deletions?\(-\))?",
         "{1} fichier(s), lignes en plus : {2}, en moins : {3}"),
        (r"^rename (.+) \(\d+%\)$", "nouveau nom : {1}"),
        (r"^mode change \d+ => \d+ (.+)$", "mode : {1}"),
        (r"^branch '(.+)' set up to track '(.+)'\.?$", "la branche {1} suit {2}"),
        (r"^To (.+)$", "serveur : {1}"),
        (r"^From (.+)$", "serveur : {1}"),
        (r"^\+?\s*([0-9a-f]+)\.\.\.?([0-9a-f]+)\s+(\S+)\s+->\s+(\S+)\s+\(forced update\)$",
         "{4} : {1} -> {2}, force push"),
        (r"^([0-9a-f]+)\.\.([0-9a-f]+)\s+(\S+)\s+->\s+(\S+)$", "{4} : {1} -> {2}"),
        (r"^\*\s+\[new branch\]\s+(\S+)\s+->\s+(\S+)$", "nouvelle branche : {2}"),
        (r"^\*\s+\[new tag\]\s+(\S+)\s+->\s+(\S+)$", "nouveau tag : {2}"),
        (r"^\*\s+branch\s+(\S+)\s+->\s+(\S+)$", "branche lue : {1}"),
        (r"^Everything up-to-date$", "rien de nouveau"),
        (r"^Already up.to.date\.?$", "rien de nouveau"),
        (r"^Successfully rebased and updated (.+)\.$", "rebase OK"),
        (r"^Fast-forward$", "avance simple"),
        (r"^Updating ([0-9a-f]+)\.\.([0-9a-f]+)$", "{1} -> {2}"),
        (r"^(?:Initialized empty|Reinitialized existing) Git repository in (.+)$", "repo git : {1}"),
        (r"^Created autostash: ([0-9a-f]+)$", "changements, commit manquant : mis en attente"),
        (r"^Applied autostash\.?$", "changements, commit manquant : repris"),
        (r"^Cloning into '(.+)'\.\.\.$", "clone dans {1}"),
        (r"^Auto-merging (.+)$", "fusion auto : {1}"),
        (r"^CONFLICT \(.+\): Merge conflict in (.+)$", "conflit : {1}"),
        (r"^rm '(.+)'$", "retrait : {1}"),
        (r"^nothing to commit", "rien pour un commit"),
        (r"^remote: Create a pull request for '(.+)' on GitHub by visiting:$", "pull request pour {1} sur GitHub :"),
        (r"^remote:\s+(https://\S+)$", "{1}"),
    ]
    .into_iter()
    .map(|(m, t)| (Regex::new(m).unwrap(), t))
    .collect()
});

static GROUPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{(\d)\}").unwrap());

/// Ligne de git en francais, ou None : rien en anglais ne s'affiche.
pub fn traduire_git(ligne: &str) -> Option<String> {
    let ligne = ligne.trim();
    if ligne.is_empty() || BRUIT_GIT.is_match(ligne) {
        return None;
    }
    for (motif, modele) in TRADUCTIONS_GIT.iter() {
        if let Some(m) = motif.captures(ligne) {
            let texte = GROUPE.replace_all(modele, |g: &regex::Captures| {
                let n: usize = g[1].parse().unwrap_or(0);
                m.get(n).map_or("0", |x| x.as_str()).to_string()
            });
            return Some(texte.into_owned());
        }
    }
    None
}

pub fn derniere_ligne(texte: &str) -> String {
    let derniere = texte.lines().map(str::trim).filter(|l| !l.is_empty() && !BRUIT_GIT.is_match(l)).last();
    let Some(l) = derniere else { return String::new() };
    Regex::new(r"(?i)^(fatal|error|erreur|remote):\s*").unwrap().replace(l, "").into_owned()
}

fn executer_git(args: &[&str], cwd: Option<&Path>, env: Option<&Env>) -> (i32, String, String) {
    let montrer = session::voir_git() && commande_visible(args);
    if montrer {
        afficher_commande(args);
    }
    let mut cmd = Command::new("git");
    cmd.arg("-c").arg("core.quotepath=off").args(args).stdin(Stdio::null());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    appliquer_env(&mut cmd, env.unwrap_or(&env_git(None, false)));
    let (code, out, errs) = match cmd.output() {
        Ok(o) => (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).trim_end().to_string(),
                  String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => (127, String::new(), e.to_string()),
    };
    if montrer && code == 0 {
        // apres une erreur, l'explication vient du programme
        for flux in [&out, &errs] {
            for ligne in flux.trim().lines() {
                if let Some(t) = traduire_git(ligne) {
                    println!("     {t}");
                }
            }
        }
    }
    (code, out, errs)
}

/// Execute une commande git. Retourne (code, sortie, erreurs). Les commandes
/// qui changent un repo s'affichent toujours, l'action Git en rouge.
pub fn git(args: &[&str], cwd: Option<&Path>) -> (i32, String, String) {
    executer_git(args, cwd, None)
}

/// git avec un environnement donne : clone anonyme ou avec un jeton.
pub fn git_env(args: &[&str], cwd: Option<&Path>, env: &Env) -> (i32, String, String) {
    executer_git(args, cwd, Some(env))
}

const PHASES_GIT: [(&str, &str); 7] = [("Enumerating objects", "Liste des objets"), ("Counting objects", "Comptage"),
    ("Compressing objects", "Compression"), ("Receiving objects", "Copie des objets"),
    ("Resolving deltas", "Assemblage"), ("Updating files", "Fichiers"), ("Writing objects", "Envoi des objets")];

/// Etape de progression de git en francais : (nom, pourcentage) ou None.
pub fn progression_git(ligne: &str) -> Option<(&'static str, String)> {
    for (anglais, francais) in PHASES_GIT {
        if let Some(pos) = ligne.find(&format!("{anglais}:")) {
            let reste = ligne[pos + anglais.len() + 1..].trim_start();
            let chiffres: String = reste.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !chiffres.is_empty() && reste[chiffres.len()..].starts_with('%') {
                return Some((francais, chiffres));
            }
        }
    }
    None
}

/// git avec progression en direct, en francais : clone d'un repo.
pub fn git_direct(args: &[&str], cwd: Option<&Path>) -> i32 {
    if session::voir_git() {
        afficher_commande(args);
    }
    let mut a: Vec<&str> = args.to_vec();
    if a.first() == Some(&"clone") {
        a.insert(1, "--progress");
    }
    let mut cmd = Command::new("git");
    cmd.args(&a).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    appliquer_env(&mut cmd, &env_git(None, false));
    let Ok(mut enfant) = cmd.spawn() else {
        info(&expliquer(""));
        return 127;
    };
    let mut flux = enfant.stderr.take().unwrap();
    let (mut phase, mut lignes, mut dernier): (&str, Vec<String>, Option<(&str, String)>) = ("", vec![], None);
    let mut morceau = Vec::new();
    let mut octet = [0u8; 4096];
    let traiter = |brut: &[u8], phase: &mut &str, lignes: &mut Vec<String>, dernier: &mut Option<(&str, String)>| {
        let ligne = String::from_utf8_lossy(brut).trim().to_string();
        if ligne.is_empty() {
            return;
        }
        let etape = progression_git(&ligne);
        lignes.push(ligne);
        if let Some(e) = etape {
            if dernier.as_ref() != Some(&e) {
                if e.0 != *phase && !phase.is_empty() {
                    println!();
                }
                *phase = e.0;
                print!("\r     {} : {} %   ", e.0, e.1);
                sortie();
                *dernier = Some(e);
            }
        }
    };
    // les retours chariot de la progression coupent aussi les lignes
    while let Ok(n) = flux.read(&mut octet) {
        if n == 0 {
            break;
        }
        for &b in &octet[..n] {
            if b == b'\r' || b == b'\n' {
                traiter(&morceau, &mut phase, &mut lignes, &mut dernier);
                morceau.clear();
            } else {
                morceau.push(b);
            }
        }
    }
    traiter(&morceau, &mut phase, &mut lignes, &mut dernier);
    let code = enfant.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
    if !phase.is_empty() {
        println!();
    }
    if code != 0 {
        let debut = lignes.len().saturating_sub(5);
        info(&expliquer(&lignes[debut..].join("\n")));
    }
    code
}

pub fn erreur_auth(sortie: &str) -> bool {
    let s = sortie.to_lowercase();
    ["could not read username", "terminal prompts disabled", "authentication failed",
     "invalid username or password", "returned error: 401", "returned error: 403"].iter().any(|k| s.contains(k))
}

pub fn expliquer(sortie: &str) -> String {
    let s = sortie.to_lowercase();
    let r = |t: &str| t.to_string();
    if s.contains("repository not found") {
        return r("repo introuvable sur le serveur : nom faux, repo absent ou droits manquants");
    }
    if s.contains("permission to") && s.contains("denied") {
        return r("ce compte n'a pas le droit de push dans ce repo : c'est sans doute celui \
de l'autre compte, perso ou entreprise");
    }
    if s.contains("returned error: 403") {
        return r("refus du serveur : droit manquant pour cette action sur ce repo");
    }
    if erreur_auth(sortie) {
        return r("refus de GitHub : nouvelle connexion avec le menu Compte GitHub");
    }
    if s.contains("publickey") {
        return r("le serveur SSH refuse la connexion : relancer pour passer le projet en HTTPS");
    }
    if s.contains("could not resolve host") || s.contains("timed out") || s.contains("failed to connect") {
        return r("serveur injoignable : voir la connexion internet ou le VPN de l'entreprise");
    }
    if s.contains("gh001") || s.contains("file size limit") || s.contains("large files") {
        return r("un fichier fait plus de 100 Mo, la limite de GitHub");
    }
    if s.contains("rejected") || s.contains("fetch first") || s.contains("non-fast-forward") {
        return r("le serveur a des changements plus neufs : relancer l'envoi");
    }
    if s.contains("does not appear to be a git repository") || s.contains("does not exist") {
        return r("repo introuvable : voir l'adresse");
    }
    if s.contains("not a git repository") {
        return r("ce dossier n'est pas un repo git");
    }
    if s.contains("please tell me who you are") || s.contains("empty ident") {
        return r("nom et email des commits manquants");
    }
    if s.contains("index.lock") {
        return r("un autre programme git tourne sur ce repo : le fermer, puis recommencer");
    }
    if s.contains("would be overwritten") || s.contains("your local changes") {
        return r("des changements, commit manquant, bloquent l'action : faire un commit avant");
    }
    if s.contains("unrelated histories") {
        return r("les deux historiques n'ont aucun lien");
    }
    if s.contains("ssl certificate") || s.contains("certificate verify failed") {
        return r("certificat SSL invalide : voir le proxy de l'entreprise");
    }
    if s.contains("does not match any") {
        return r("aucun commit sur cette branche");
    }
    if s.contains("conflict") {
        return r("conflit entre ce dossier et le serveur");
    }
    if crate::debug() && !derniere_ligne(sortie).is_empty() {
        return format!("erreur de git : {}", derniere_ligne(sortie));
    }
    r("erreur de git")
}

pub fn racine_repo(chemin: &Path) -> Option<PathBuf> {
    let (code, out, _) = git(&["rev-parse", "--show-toplevel"], Some(chemin));
    (code == 0 && !out.is_empty()).then(|| PathBuf::from(out))
}

/// Racine du repo git du dossier. Quand un .git existe mais que Git refuse de
/// le lire, le programme dit pourquoi et propose la solution. Retourne
/// (racine ou None, etat) : ok, absent, bloque ou illisible.
pub fn acces_repo(chemin: &Path) -> (Option<PathBuf>, &'static str) {
    let mut e = String::new();
    for essai in [1, 2] {
        let (code, out, erreurs) = git(&["rev-parse", "--show-toplevel"], Some(chemin));
        if code == 0 && !out.is_empty() {
            return (Some(PathBuf::from(out)), "ok");
        }
        e = erreurs;
        if !e.to_lowercase().contains("dubious ownership") || essai == 2 {
            break;
        }
        // Git bloque un repo dont le dossier appartient a un autre compte du poste
        let cible = Regex::new(r"(?m)safe\.directory\s+'?([^'\n]+?)'?\s*$").unwrap()
            .captures(&e).map(|m| m[1].to_string()).unwrap_or_else(|| chemin.display().to_string());
        println!();
        warn("Git bloque ce repo : son dossier appartient a un autre compte de cette machine.");
        info("Git protege ainsi les dossiers des autres comptes, par exemple apres une copie");
        info("depuis un autre Mac ou un clone fait avec sudo.");
        if !ask_yn("Faire confiance au dossier ? Git le note dans safe.directory.", true) {
            return (None, "bloque");
        }
        git(&["config", "--global", "--add", "safe.directory", &cible], None);
    }
    let dot = chemin.join(".git");
    if dot.symlink_metadata().is_err() {
        return (None, "absent");
    }
    let s = e.to_lowercase();
    println!();
    err(&format!("Git ne peut pas lire le dossier .git de {}.", chemin.display()));
    if s.contains("operation not permitted") || s.contains("permission denied") {
        info("Le systeme bloque ce dossier pour ce terminal : sur macOS, l'autoriser dans");
        info("Reglages Systeme, Confidentialite et securite, Fichiers et dossiers.");
        return (None, "bloque");
    }
    if s.contains("dubious ownership") {
        info("Git bloque encore ce dossier : voir son proprietaire.");
        return (None, "bloque");
    }
    info("Le dossier .git est sans doute incomplet, par exemple avec iCloud ou OneDrive.");
    (None, "illisible")
}

/// Nom de la branche (meme sans aucun commit). None si HEAD detachee.
pub fn branche_courante(chemin: &Path) -> Option<String> {
    let (code, out, _) = git(&["symbolic-ref", "--short", "-q", "HEAD"], Some(chemin));
    (code == 0 && !out.is_empty()).then_some(out)
}

pub fn branches_locales(chemin: &Path) -> Vec<String> {
    let (code, out, _) = git(&["branch", "--format=%(refname:short)"], Some(chemin));
    if code != 0 {
        return vec![];
    }
    out.lines().map(str::trim).filter(|b| !b.is_empty()).map(str::to_string).collect()
}

pub fn a_des_commits(chemin: &Path) -> bool {
    git(&["rev-parse", "--verify", "-q", "HEAD"], Some(chemin)).0 == 0
}

pub fn ref_existe(chemin: &Path, r: &str) -> bool {
    git(&["rev-parse", "--verify", "-q", r], Some(chemin)).0 == 0
}

pub fn nb_commits(chemin: &Path, plage: &str) -> usize {
    let (code, out, _) = git(&["rev-list", "--count", plage], Some(chemin));
    if code == 0 { out.trim().parse().unwrap_or(0) } else { 0 }
}

pub fn remotes(chemin: &Path) -> Vec<String> {
    let (code, out, _) = git(&["remote"], Some(chemin));
    if code != 0 {
        return vec![];
    }
    out.lines().map(str::trim).filter(|r| !r.is_empty()).map(str::to_string).collect()
}

/// [(remote, adresse)] du projet, en une seule commande git.
pub fn urls_remotes(chemin: &Path) -> Vec<(String, String)> {
    let (_, out, _) = git(&["config", "--get-regexp", r"^remote\..*\.url$"], Some(chemin));
    out.lines()
        .filter_map(|l| {
            let (cle, url) = l.split_once(' ')?;
            let nom = cle.strip_prefix("remote.")?.strip_suffix(".url")?;
            Some((nom.to_string(), url.to_string()))
        })
        .collect()
}

pub fn adresse_principale(chemin: &Path) -> String {
    let urls = urls_remotes(chemin);
    urls.iter().find(|(r, _)| r == "origin").or(urls.first()).map(|(_, u)| u.clone()).unwrap_or_default()
}

/// proprietaire/repo, ou la fin de l'adresse, ou 'sans lien'.
pub fn lien_court(url: &str) -> String {
    if url.is_empty() {
        return c("sans lien", "33");
    }
    cle_depot(url).unwrap_or_else(|| {
        let n = url.chars().count();
        url.chars().skip(n.saturating_sub(30)).collect()
    })
}

/// Travail fait en parallele, 8 a la fois, resultats dans l'ordre : la liste
/// s'affiche vite meme avec beaucoup de projets (surtout sous Windows).
pub fn en_parallele<T: Sync, R: Send>(elements: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    if elements.is_empty() {
        return vec![];
    }
    let taille = elements.len().div_ceil(8);
    std::thread::scope(|s| {
        let fils: Vec<_> = elements.chunks(taille).map(|morceau| {
            let f = &f;
            s.spawn(move || morceau.iter().map(f).collect::<Vec<R>>())
        }).collect();
        fils.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// [(adresse, nb de modifications, branche)] calcules en parallele.
pub fn apercu_projets(projets: &[PathBuf], avec_branche: bool) -> Vec<(String, usize, String)> {
    en_parallele(projets, |p| {
        (adresse_principale(p), fichiers_modifies(p).len(),
         if avec_branche { branche_courante(p).unwrap_or_else(|| "?".into()) } else { String::new() })
    })
}

pub fn remote_principal(chemin: &Path) -> Option<String> {
    let rs = remotes(chemin);
    if rs.iter().any(|r| r == "origin") { Some("origin".into()) } else { rs.into_iter().next() }
}

pub fn url_remote(nom: &str, chemin: &Path) -> String {
    let (code, out, _) = git(&["remote", "get-url", nom], Some(chemin));
    if code == 0 { out.trim().to_string() } else { String::new() }
}

/// Lignes 'XY chemin' d'une sortie de git status --porcelain -z : chemins exacts,
/// sans guillemets ni codes, meme avec des espaces. Un nouveau nom compte une fois.
pub fn lire_statut_z(out: &str, sous_repo: impl Fn(&str) -> bool) -> Vec<String> {
    let morceaux: Vec<&str> = out.split('\0').collect();
    let (mut lignes, mut i) = (vec![], 0);
    while i < morceaux.len() {
        let rec = morceaux[i];
        i += 1;
        if rec.chars().count() < 4 {
            continue;
        }
        if rec.starts_with('R') || rec.starts_with('C') {
            i += 1; // l'ancien chemin suit, il ne compte pas a part
        }
        if rec.starts_with("?? ") && rec.ends_with('/') && sous_repo(&rec[3..]) {
            continue;
        }
        lignes.push(rec.to_string());
    }
    lignes
}

/// Lignes 'XY chemin' de git status. Un sous-repo git ne compte pas : son
/// contenu ne part jamais avec ce repo.
pub fn lignes_statut(chemin: &Path) -> Vec<String> {
    let (code, out, _) = git(&["status", "--porcelain", "-z", "--ignore-submodules=all"], Some(chemin));
    if code != 0 {
        return vec![];
    }
    lire_statut_z(&out, |d| chemin.join(d).join(".git").exists())
}

/// Fichiers au commit manquant : lignes 'XY chemin'.
pub fn fichiers_modifies(chemin: &Path) -> Vec<String> {
    lignes_statut(chemin)
}

pub fn chemin_porcelaine(ligne: &str) -> String {
    ligne.get(3..).unwrap_or("").to_string()
}

pub fn fichiers_en_conflit(chemin: &Path) -> Vec<String> {
    let (_, out, _) = git(&["diff", "--name-only", "--diff-filter=U"], Some(chemin));
    out.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect()
}

/// 'rebase' ou 'merge' si une operation est restee bloquee, sinon None.
pub fn operation_en_cours(chemin: &Path) -> Option<&'static str> {
    for (nom, typ) in [("rebase-merge", "rebase"), ("rebase-apply", "rebase"), ("MERGE_HEAD", "merge")] {
        let (code, out, _) = git(&["rev-parse", "--git-path", nom], Some(chemin));
        if code == 0 && chemin.join(&out).exists() {
            return Some(typ);
        }
    }
    None
}

/// (commits locaux a envoyer, commits du serveur a recuperer) ou None.
pub fn avance_retard(chemin: &Path, remote: &str, branche: &str) -> Option<(usize, usize)> {
    let (code, out, _) = git(&["rev-list", "--left-right", "--count", &format!("{remote}/{branche}...HEAD")],
                             Some(chemin));
    let n: Vec<usize> = out.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (code == 0 && n.len() == 2).then(|| (n[1], n[0]))
}

pub fn branche_defaut_distante(chemin: &Path, remote: &str) -> Option<String> {
    let (code, out, _) = git(&["ls-remote", "--symref", remote, "HEAD"], Some(chemin));
    let m = Regex::new(r"ref:\s*refs/heads/(\S+)\s+HEAD").unwrap().captures(&out)?;
    (code == 0).then(|| m[1].to_string())
}

pub fn init_depot(chemin: &Path) {
    if git(&["init", "-b", "main"], Some(chemin)).0 != 0 {
        // git < 2.28
        git(&["init"], Some(chemin));
        git(&["symbolic-ref", "HEAD", "refs/heads/main"], Some(chemin));
    }
    ok(&format!("Nouveau repo git dans {}", chemin.display()));
}

pub fn statut_nom(code: &str) -> String {
    match code {
        "A" => "nouveau", "M" | "T" => "modif", "D" => "retrait", "R" => "nouveau nom", "C" => "copie",
        "U" => "conflit", autre => return autre.to_string(),
    }.to_string()
}

/// [(statut, fichier)] depuis 'git diff --name-status'.
pub fn lire_name_status(texte: &str) -> Vec<(String, String)> {
    texte.lines()
        .filter_map(|l| {
            let parts: Vec<&str> = l.split('\t').collect();
            (parts.len() >= 2).then(|| (statut_nom(&parts[0].chars().take(1).collect::<String>()), parts[1..].join(" -> ")))
        })
        .collect()
}

pub fn bilan_changements(changements: &[(String, String)]) -> String {
    let mut compte: Vec<(String, usize)> = vec![];
    for (s, _) in changements {
        match compte.iter_mut().find(|(x, _)| x == s) {
            Some((_, n)) => *n += 1,
            None => compte.push((s.clone(), 1)),
        }
    }
    if compte.is_empty() {
        return "aucun changement".into();
    }
    compte.iter().map(|(s, n)| format!("{n} {s}(s)")).collect::<Vec<_>>().join(", ")
}

pub fn afficher_changements(changements: &[(String, String)], limite: usize) {
    for (s, f) in changements.iter().take(limite) {
        println!("       {} {f}", crate::affichage::ljust(s, 9));
    }
    if changements.len() > limite {
        println!("       ... et {} autre(s)", changements.len() - limite);
    }
}

pub fn changements_entre(chemin: &Path, avant: Option<&str>, apres: &str) -> Vec<(String, String)> {
    if let Some(avant) = avant {
        let (_, ns, _) = git(&["diff", "--name-status", "-M", avant, apres], Some(chemin));
        return lire_name_status(&ns);
    }
    let (_, out, _) = git(&["ls-tree", "-r", "--name-only", apres], Some(chemin));
    out.lines().filter(|f| !f.is_empty()).map(|f| ("nouveau".to_string(), f.to_string())).collect()
}

pub fn statut_porcelaine(ligne: &str) -> String {
    let xy: String = ligne.chars().take(2).collect();
    if xy == "??" {
        return "nouveau".into();
    }
    if xy.contains('U') || xy == "AA" || xy == "DD" {
        return "conflit".into();
    }
    for ch in xy.chars() {
        if ch != ' ' {
            return statut_nom(&ch.to_string());
        }
    }
    xy
}

/// Suppression d'un dossier, fichiers en lecture seule compris (Windows).
pub fn supprimer_dossier(chemin: &Path) {
    fn vider(d: &Path) {
        if let Ok(entrees) = std::fs::read_dir(d) {
            for e in entrees.flatten() {
                let p = e.path();
                let est_dossier = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if est_dossier {
                    vider(&p);
                    let _ = std::fs::remove_dir(&p);
                } else {
                    if let Ok(m) = std::fs::symlink_metadata(&p) {
                        let mut perm = m.permissions();
                        if perm.readonly() {
                            #[allow(clippy::permissions_set_readonly_false)]
                            perm.set_readonly(false);
                            let _ = std::fs::set_permissions(&p, perm);
                        }
                    }
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
    if std::fs::remove_dir_all(chemin).is_err() {
        vider(chemin);
        let _ = std::fs::remove_dir(chemin);
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn action_git_en_rouge_le_reste_en_cyan() {
        let l = ligne_commande(&["commit", "-m", "Mon message"], true);
        assert!(l.contains("\x1b[1;31mcommit\x1b[0m"), "{l:?}");
        assert!(l.contains("\x1b[36m -m \"Mon message\"\x1b[0m"), "{l:?}");
        assert_eq!(ligne_commande(&["push", "-u", "origin", "main"], false), "  $ git push -u origin main");
        assert_eq!(ligne_commande(&["-c", "a=b", "fetch", "origin"], false), "  $ git -c a=b fetch origin");
    }

    #[test]
    fn commandes_qui_changent_le_repo_visibles() {
        assert!(commande_visible(&["update-ref", "-d", "HEAD"]));
        assert!(commande_visible(&["symbolic-ref", "HEAD", "refs/heads/main"]));
        assert!(!commande_visible(&["symbolic-ref", "--short", "-q", "HEAD"]));
        assert!(!commande_visible(&["config", "--get", "user.name"]));
        assert!(commande_visible(&["config", "--global", "user.name", "x"]));
        assert!(commande_visible(&["stash", "push", "-m", "x"]) && !commande_visible(&["stash", "list"]));
        assert!(commande_visible(&["branch", "-D", "x"]) && !commande_visible(&["branch", "--format=%(refname)"]));
        assert!(!commande_visible(&["status", "--porcelain"]) && commande_visible(&["commit", "-m", "x"]));
    }

    #[test]
    fn messages_de_git_en_francais() {
        assert_eq!(traduire_git("[main abc1234] Mon message").as_deref(), Some("commit abc1234 sur main : Mon message"));
        assert_eq!(traduire_git("[main (root-commit) abc1234] Premier").as_deref(), Some("commit abc1234 sur main : Premier"));
        assert_eq!(traduire_git(" 3 files changed, 10 insertions(+)").as_deref(),
                   Some("3 fichier(s), lignes en plus : 10, en moins : 0"));
        assert_eq!(traduire_git("Everything up-to-date").as_deref(), Some("rien de nouveau"));
        assert_eq!(traduire_git("To https://github.com/o/r.git").as_deref(), Some("serveur : https://github.com/o/r.git"));
        assert_eq!(traduire_git("   abc1234..def5678  main -> main").as_deref(), Some("main : abc1234 -> def5678"));
        assert_eq!(traduire_git("hint: Using 'master' as the name"), None);
        assert_eq!(traduire_git("warning: LF will be replaced by CRLF in a.txt"), None);
    }

    #[test]
    fn statut_z_avec_espaces_nouveaux_noms_et_sous_repos() {
        let out = "?? a b.txt\0R  nouveau.md\0ancien.md\0?? sous repo/\0 M été.txt\0";
        assert_eq!(lire_statut_z(out, |d| d == "sous repo/"),
                   vec!["?? a b.txt", "R  nouveau.md", " M été.txt"]);
    }

    #[test]
    fn erreurs_de_git_expliquees() {
        assert!(expliquer("remote: Repository not found.\nfatal: ...").starts_with("repo introuvable sur le serveur"));
        assert_eq!(expliquer("fatal: Could not read from remote repository.\nPermission denied (publickey)."),
                   "le serveur SSH refuse la connexion : relancer pour passer le projet en HTTPS");
        assert_eq!(expliquer("! [rejected] main -> main (fetch first)"),
                   "le serveur a des changements plus neufs : relancer l'envoi");
        assert_eq!(expliquer("quelque chose d'inconnu"), "erreur de git");
    }

    #[test]
    fn changements_en_francais() {
        assert_eq!(statut_porcelaine("?? x"), "nouveau");
        assert_eq!(statut_porcelaine(" M x"), "modif");
        assert_eq!(statut_porcelaine("UU x"), "conflit");
        let c = lire_name_status("A\ta.txt\nM\tb.txt\nR100\tc.txt\td.txt\nA\te.txt");
        assert_eq!(c[2], ("nouveau nom".to_string(), "c.txt -> d.txt".to_string()));
        assert_eq!(bilan_changements(&c), "2 nouveau(s), 1 modif(s), 1 nouveau nom(s)");
        assert_eq!(bilan_changements(&[]), "aucun changement");
    }
}
