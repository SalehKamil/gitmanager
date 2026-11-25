//! Synchronisation d'un repo : commit, fetch, integration du serveur, push, pull,
//! lien avec un repo GitHub, nouveau repo, dossier sans git.

use crate::affichage::{err, info, ljust, ok, titre, warn};
use crate::config::Config;
use crate::connexion::{assurer_connexion, connexion_interactive, ecrire_identite, identite_par_defaut,
                       mettre_a_jour_permissions_push, rafraichir_session};
use crate::erreur::{Arret, R};
use crate::git::*;
use crate::github::{api, host, liste_repos, message_api, texte};
use crate::resultat::{executer, fait, Res};
use crate::saisie::{ask, ask_libre, ask_yn, menu_numerote, message_defaut, pause};
use crate::session;
use crate::urls::{avec_https, cle_depot, contient_token, est_ssh, infos_url, sans_identifiants, url_web, verifier_url};
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::LazyLock;

pub const GITIGNORE: &str = "__pycache__/
*.py[cod]
*.so
.venv/
venv/
env/
.env
.env.*
!.env.example
*.log
.pytest_cache/
.mypy_cache/
.ruff_cache/
dist/
build/
*.egg-info/
node_modules/
.idea/
.vscode/
.DS_Store
Thumbs.db
";

// Fichiers a ne jamais envoyer par erreur (secrets, cles privees)
pub static SENSIBLES: LazyLock<Regex> = LazyLock::new(|| Regex::new(
    r"(?i)(^|/)(\.env(\.[^/]*)?|id_(rsa|dsa|ecdsa|ed25519)[^/]*|[^/]*\.(pem|key|p12|pfx)|credentials\.json|secrets?\.(json|ya?ml|toml))$").unwrap());
pub static SENSIBLES_OK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\.env\.(example|sample|template)|\.pub)$").unwrap());
const LIMITE_GITHUB: u64 = 100 * 1024 * 1024; // GitHub refuse les fichiers > 100 Mo

pub fn est_sensible(f: &str) -> bool {
    SENSIBLES.is_match(f) && !SENSIBLES_OK.is_match(f)
}

fn p(chemin: &Path) -> Option<&Path> {
    Some(chemin)
}

/// Repare ce que l'ancienne version du script laissait dans .git/config.
pub fn nettoyer_depot(chemin: &Path, bavard: bool) {
    let noms = remotes(chemin);
    for r in &noms {
        // un identifiant en clair dans .git/config est un secret expose
        let url = url_remote(r, chemin);
        if contient_token(&url) {
            git(&["remote", "set-url", r, &sans_identifiants(&url)], p(chemin));
            if bavard {
                info(&format!("Identifiant en clair : retrait OK dans l'adresse du remote '{r}'."));
            }
        }
    }
    // "git push -u <url>" ecrivait l'URL comme amont de la branche
    let (_, out, _) = git(&["config", "--local", "--get-regexp", r"^branch\..*\.remote$"], p(chemin));
    for ligne in out.lines() {
        let (cle, valeur) = ligne.split_once(' ').unwrap_or((ligne, ""));
        if noms.iter().any(|n| n == valeur) || !(valeur.contains("://") || valeur.contains('@')) {
            continue;
        }
        let cible = noms.iter().find(|r| cle_depot(&url_remote(r, chemin)) == cle_depot(valeur)).cloned();
        match &cible {
            Some(c) => {
                git(&["config", "--local", cle, c], p(chemin));
            }
            None => {
                git(&["config", "--local", "--unset", cle], p(chemin));
                git(&["config", "--local", "--unset", &format!("{}.merge", cle.trim_end_matches(".remote"))], p(chemin));
            }
        }
        if bavard {
            let branche = cle.split('.').nth(1).unwrap_or("");
            info(&format!("Suivi de branche : correction OK, {branche} -> {}.", cible.as_deref().unwrap_or("aucun")));
        }
    }
    // Nom / email vides en local : ils masquaient la config globale -> commit refuse
    for cle in ["user.name", "user.email"] {
        let (code, val, _) = git(&["config", "--local", "--get", cle], p(chemin));
        if code == 0 && val.trim().is_empty() {
            git(&["config", "--local", "--unset-all", cle], p(chemin));
            if bavard {
                info(&format!("{cle} vide : retrait OK, il bloquait les commits."));
            }
        }
    }
}

pub fn assurer_identite(cfg: &mut Config, chemin: &Path, auto: bool) -> R<()> {
    for cle in ["user.name", "user.email"] {
        let (_, val, _) = git(&["config", "--get", cle], p(chemin));
        if !val.trim().is_empty() {
            continue;
        }
        let mut valeur = identite_par_defaut(cfg, &session::user().unwrap_or(json!({})), cle);
        if valeur.is_empty() && auto {
            return Err(Arret::new("erreur", format!("git {cle} manque dans la config de Git")));
        }
        if valeur.is_empty() {
            valeur = ask(if cle == "user.name" { "Nom pour les commits" } else { "Email pour les commits" }, "");
        }
        ecrire_identite(cfg, cle, &valeur);
    }
    Ok(())
}

fn ajouter_au_gitignore(chemin: &Path, lignes: &[String]) -> bool {
    let gi = chemin.join(".gitignore");
    let existant = std::fs::read_to_string(&gi).unwrap_or_default();
    let ajout: String = lignes.iter().filter(|l| !existant.lines().any(|e| e == l.as_str()))
        .map(|l| format!("{l}\n")).collect();
    if ajout.is_empty() {
        return false;
    }
    let sep = if existant.is_empty() || existant.ends_with('\n') { "" } else { "\n" };
    let _ = std::fs::write(&gi, format!("{existant}{sep}{ajout}"));
    git(&["add", ".gitignore"], p(chemin));
    true
}

/// Retire de l'envoi les secrets (.env, cles) et les fichiers > 100 Mo.
pub fn proteger_fichiers(chemin: &Path, auto: bool) -> Vec<String> {
    let liste = |filtre: &str| -> Vec<String> {
        let (_, out, _) = git(&["diff", "--cached", "--name-only", "-z", filtre], p(chemin));
        out.split('\0').filter(|f| !f.is_empty()).map(str::to_string).collect()
    };
    let (nouveaux, modifies) = (liste("--diff-filter=A"), liste("--diff-filter=M"));
    let trop_gros = |f: &String| std::fs::metadata(chemin.join(f)).is_ok_and(|m| m.is_file() && m.len() > LIMITE_GITHUB);
    let a_retirer: Vec<String> = nouveaux.iter().filter(|f| est_sensible(f) || trop_gros(f)).cloned().collect();
    let gros_modifies: Vec<String> = modifies.iter().filter(|f| trop_gros(f)).cloned().collect();
    if a_retirer.is_empty() && gros_modifies.is_empty() {
        return vec![];
    }
    let tous: Vec<String> = a_retirer.iter().chain(&gros_modifies).cloned().collect();
    if !auto {
        println!();
        warn("Ces fichiers ne devraient pas aller sur le serveur, secrets ou plus de 100 Mo :");
        for f in &tous {
            println!("       {f}");
        }
        if !ask_yn("Les exclure de l'envoi et du suivi git ?", true) {
            return vec![];
        }
    }
    if !a_retirer.is_empty() {
        let mut args = vec!["rm", "--cached", "-q", "--"];
        args.extend(a_retirer.iter().map(String::as_str));
        git(&args, p(chemin));
    }
    if !gros_modifies.is_empty() {
        let mut args = vec!["reset", "-q", "--"];
        args.extend(gros_modifies.iter().map(String::as_str));
        git(&args, p(chemin));
    }
    if !auto && !a_retirer.is_empty() {
        let lignes: Vec<String> = a_retirer.iter().map(|f| format!("/{f}")).collect();
        if ajouter_au_gitignore(chemin, &lignes) {
            info("Ajout OK dans le .gitignore.");
        }
    }
    tous
}

/// Dossiers qui ont leur propre .git : Git n'en garde qu'un lien, sans leurs
/// fichiers, et le repo en ligne montre un dossier vide. Le programme le dit et
/// propose de les laisser hors du commit.
pub fn verifier_sous_repos(chemin: &Path, auto: bool) {
    let (_, out, _) = git(&["diff", "--cached", "--raw", "-z", "--diff-filter=A"], p(chemin));
    let morceaux: Vec<&str> = out.split('\0').collect();
    let liens: Vec<String> = (0..morceaux.len().saturating_sub(1)).step_by(2)
        .filter(|&i| morceaux[i].split_whitespace().nth(1) == Some("160000"))
        .map(|i| morceaux[i + 1].to_string()).collect();
    if liens.is_empty() {
        return;
    }
    if !auto {
        println!();
        warn("Ces dossiers ont leur propre .git : Git n'en garde qu'un lien, sans leurs fichiers.");
        warn("En ligne, ils apparaissent vides :");
        for d in liens.iter().take(20) {
            println!("       {d}");
        }
        info("Pour avoir leurs fichiers en ligne : envoyer chaque dossier dans son propre repo,");
        info("ou supprimer son .git avant.");
        println!();
        println!("    1. Les laisser hors du commit : ajout au .gitignore");
        println!("    2. Garder seulement le lien : sous-repo git");
        if ask("Choix", "1") == "2" {
            return;
        }
    }
    let mut args = vec!["reset", "--quiet", "--"];
    args.extend(liens.iter().map(String::as_str));
    git(&args, p(chemin)); // hors du commit, rien d'efface
    ajouter_au_gitignore(chemin, &liens.iter().map(|d| format!("/{d}/")).collect::<Vec<_>>());
    if !auto {
        ok(&format!("{} dossier(s) hors du commit, notes dans le .gitignore.", liens.len()));
    }
}

fn normcase(chemin: &Path) -> String {
    let t = chemin.display().to_string();
    if cfg!(windows) { t.to_lowercase().replace('/', "\\") } else { t }
}

/// Propose un .gitignore standard une fois par dossier : apres un refus, la
/// question ne revient pas a chaque envoi.
pub fn gitignore_standard(cfg: &mut Config, chemin: &Path) {
    let gi = chemin.join(".gitignore");
    let mut refus: Vec<Value> = cfg.get("sans_gitignore").and_then(Value::as_array).cloned().unwrap_or_default();
    let cle = normcase(chemin);
    if gi.exists() || refus.iter().any(|r| r.as_str() == Some(cle.as_str())) {
        return;
    }
    warn("Pas de .gitignore : risque d'envoyer .venv, node_modules, caches...");
    if ask_yn("Ajouter un .gitignore standard pour Python et Node ?", true) {
        let _ = std::fs::write(&gi, GITIGNORE);
        ok("Nouveau .gitignore");
    } else {
        refus.push(cle.into());
        cfg.set("sans_gitignore", refus);
        cfg.sauver();
        info("Plus de question sur le .gitignore pour ce dossier.");
    }
}

/// git add -A + git commit. Retourne le nombre de fichiers commites.
pub fn committer(cfg: &mut Config, chemin: &Path, message: Option<&str>, auto: bool, proposer_gitignore: bool) -> R<usize> {
    if fichiers_modifies(chemin).is_empty() {
        if !auto {
            info("Aucun changement depuis le dernier commit.");
        }
        return Ok(0);
    }
    if !auto && proposer_gitignore {
        gitignore_standard(cfg, chemin);
    }
    let (code, _, e) = git(&["add", "-A"], p(chemin));
    if code != 0 {
        return Err(Arret::new("erreur", format!("erreur de git add : {}", expliquer(&e))));
    }
    verifier_sous_repos(chemin, auto);
    let exclus = proteger_fichiers(chemin, auto);
    if git(&["diff", "--cached", "--quiet"], p(chemin)).0 == 0 {
        if !auto {
            info("Rien pour un commit.");
        }
        return Ok(0);
    }
    let (_, ns, _) = git(&["diff", "--cached", "--name-status", "-M"], p(chemin));
    let changements = lire_name_status(&ns);
    let mut message = message.filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(message_defaut);
    if !auto {
        println!();
        info(&format!("Changements du commit : {}", bilan_changements(&changements)));
        afficher_changements(&changements, 40);
        message = ask("Message du commit, Enter pour garder", &message);
    }
    assurer_identite(cfg, chemin, auto)?;
    let (code, out, e) = git(&["commit", "-m", &message], p(chemin));
    if code != 0 {
        return Err(Arret::new("erreur", format!("refus du commit : {}", expliquer(if e.is_empty() { &out } else { &e }))));
    }
    if !auto && !exclus.is_empty() {
        info(&format!("{} fichier(s) exclu(s) de l'envoi.", exclus.len()));
    }
    Ok(changements.len())
}

/// git fetch (avec reconnexion ou passage SSH -> HTTPS).
pub fn recuperer(cfg: &mut Config, chemin: &Path, remote: &str, auto: bool) -> R<()> {
    for tentative in [1, 2] {
        let (code, _, e) = git(&["fetch", remote], p(chemin));
        if code == 0 {
            return Ok(());
        }
        let url = url_remote(remote, chemin);
        if tentative == 1 && !auto {
            if est_ssh(&url) && (e.contains("publickey") || e.contains("Permission denied") || e.contains("Host key")) {
                warn("Le serveur SSH refuse la connexion : pas de SSH pour GitHub sur ce poste.");
                if let (Some(h), Some(o), Some(r)) = infos_url(&url) {
                    if ask_yn("Passer ce projet en HTTPS, avec connexion par le navigateur ?", true) {
                        git(&["remote", "set-url", remote, &format!("https://{h}/{o}/{r}.git")], p(chemin));
                        continue;
                    }
                }
            } else if erreur_auth(&e) {
                if rafraichir_session(cfg) {
                    continue;
                }
                warn("GitHub demande une connexion.");
                session::set_token("");
                if connexion_interactive(cfg, false) {
                    continue;
                }
            }
        }
        return Err(Arret::new("erreur", expliquer(&e)));
    }
    Ok(())
}

/// Prend en compte les commits du serveur. Retourne (branche, nb recus).
/// pour_envoi : propose 'ce dossier fait foi' si le serveur a de l'avance.
pub fn integrer(chemin: &Path, remote: &str, branche: &str, auto: bool, pour_envoi: bool) -> R<(String, usize)> {
    let mut branche = branche.to_string();
    if !ref_existe(chemin, &format!("refs/remotes/{remote}/{branche}")) {
        let defaut = branche_defaut_distante(chemin, remote);
        let Some(defaut) = defaut.filter(|d| *d != branche && ref_existe(chemin, &format!("refs/remotes/{remote}/{d}"))) else {
            return Ok((branche, 0)); // branche pas encore sur le serveur : premier envoi
        };
        if a_des_commits(chemin) && git(&["merge-base", "HEAD", &format!("{remote}/{defaut}")], p(chemin)).0 == 0 {
            // branche faite a partir du serveur : elle part en ligne comme nouvelle branche
            if !auto {
                info(&format!("Nouvelle branche en ligne : {branche}."));
            }
            return Ok((branche, 0));
        }
        if auto {
            return Err(Arret::new("ignore", format!("le serveur a la branche '{defaut}', celle de ce dossier s'appelle '{branche}' : ouvrir le projet")));
        }
        println!();
        warn(&format!("Sur le serveur la branche principale est '{defaut}', ici c'est '{branche}'."));
        println!("    1. Renommer '{branche}' en '{defaut}' et continuer");
        println!("    2. Garder '{branche}' comme autre branche sur le serveur");
        println!("    0. Annuler");
        let choix = ask("Choix", "0");
        if choix != "1" && choix != "2" {
            return Err(Arret::new("ignore", "abandon de l'envoi"));
        }
        if choix == "2" {
            return Ok((branche, 0));
        }
        if branches_locales(chemin).contains(&defaut) {
            return Err(Arret::new("erreur", format!("une branche locale '{defaut}' existe aussi : faire 'git switch {defaut}' puis recommencer")));
        }
        git(&["branch", "-M", &defaut], p(chemin));
        branche = defaut;
    }
    let distante = format!("{remote}/{branche}");
    if !a_des_commits(chemin) {
        // rien en local : on prend la version du serveur
        let (code, _, e) = git(&["merge", "--ff-only", &distante], p(chemin));
        if code != 0 {
            return Err(Arret::new("erreur", format!("pull impossible : {}", expliquer(&e))));
        }
        return Ok((branche, nb_commits(chemin, "HEAD")));
    }
    let retard = avance_retard(chemin, remote, &branche).map_or(0, |a| a.1);
    if retard == 0 {
        return Ok((branche, 0));
    }
    if pour_envoi && !auto {
        println!();
        warn(&format!("Le serveur contient {retard} commit(s) absent(s) de ce dossier :"));
        let (_, log, _) = git(&["log", "--oneline", "-10", &format!("HEAD..{distante}")], p(chemin));
        for l in log.lines() {
            println!("       {l}");
        }
        println!("    1. Les prendre et les combiner avec ce dossier");
        println!("    2. Ce dossier fait foi : le serveur prend son contenu exact");
        println!("       fichiers absents d'ici : en moins sur le serveur, historique intact");
        println!("    0. Annuler");
        let choix = ask("Choix", "0");
        if choix != "1" && choix != "2" {
            return Err(Arret::new("ignore", "abandon de l'envoi"));
        }
        if choix == "2" {
            let (code, _, e) = git(&["merge", "-s", "ours", "--allow-unrelated-histories", "-m",
                                     "Remplacement par la version de ce dossier", &distante], p(chemin));
            if code != 0 {
                return Err(Arret::new("erreur", format!("remplacement impossible : {}", expliquer(&e))));
            }
            return Ok((branche, 0));
        }
    }
    if !auto {
        info(&format!("Combinaison de {retard} commit(s) du serveur..."));
    }
    let (code, out, e) = git(&["rebase", "--autostash", &distante], p(chemin));
    if code == 0 {
        return Ok((branche, retard));
    }
    if operation_en_cours(chemin) != Some("rebase") {
        return Err(Arret::new("erreur", format!("combinaison impossible : {}", expliquer(if e.is_empty() { &out } else { &e }))));
    }
    let conflits = fichiers_en_conflit(chemin);
    let liste = conflits.iter().take(5).cloned().collect::<Vec<_>>().join(", ") + if conflits.len() > 5 { " ..." } else { "" };
    if !auto {
        println!();
        warn("Conflit : des lignes ont des changements ici et sur le serveur.");
        for f in &conflits {
            println!("       {f}");
        }
        if pour_envoi {
            info("Astuce : avec l'option 2, 'ce dossier fait foi', pas de conflit.");
        }
        if !ask_yn("Annuler ? Rien n'est perdu, les commits restent ici.", true) {
            info("Ouvrir ces fichiers, garder la bonne version entre <<<<<<< et >>>>>>>,");
            info("puis : git add .   et   git rebase --continue");
            return Err(Arret::new("conflit", format!("conflit en cours : {liste}")));
        }
    }
    git(&["rebase", "--abort"], p(chemin));
    Err(Arret::new("conflit", format!("conflit sur {liste} : aucun envoi, le travail local est intact")))
}

/// Noms des branches du serveur, lus au dernier fetch.
pub fn branches_en_ligne(chemin: &Path, remote: Option<&str>) -> Vec<String> {
    let Some(remote) = remote else { return vec![] };
    let (_, out, _) = git(&["branch", "-r", "--format=%(refname:short)"], p(chemin));
    let mut v: Vec<String> = out.split_whitespace()
        .filter(|b| b.starts_with(&format!("{remote}/")) && !b.ends_with("/HEAD"))
        .map(|b| b[remote.len() + 1..].to_string()).collect();
    v.sort();
    v
}

/// git push (apres integration du serveur). Retourne le resultat (statut, detail).
pub fn pousser(cfg: &mut Config, chemin: &Path, remote: &str, branche: &str, auto: bool, commit_affiche: bool) -> R<Res> {
    if !auto {
        info("Lecture du serveur...");
    }
    recuperer(cfg, chemin, remote, auto)?;
    let (branche, recus) = integrer(chemin, remote, branche, auto, true)?;
    if !a_des_commits(chemin) {
        return Err(Arret::new("ignore", "aucun commit : rien pour le push"));
    }
    let r = format!("refs/remotes/{remote}/{branche}");
    let mut avant = if ref_existe(chemin, &r) { Some(git(&["rev-parse", &r], p(chemin)).1) } else { None };
    let ar = if avant.is_some() { avance_retard(chemin, remote, &branche) } else { None };
    let mut a_envoyer = ar.map_or_else(|| nb_commits(chemin, "HEAD"), |a| a.0);
    let mut nouvelle = false;
    if avant.is_none() {
        // nouvelle branche faite a partir du serveur : comparer avec la plus proche
        let mut proches = vec![];
        for autre in branches_en_ligne(chemin, Some(remote)) {
            let (code, base, _) = git(&["merge-base", "HEAD", &format!("{remote}/{autre}")], p(chemin));
            if code == 0 && !base.is_empty() {
                proches.push((nb_commits(chemin, &format!("{base}..HEAD")), base));
            }
        }
        if let Some((n, base)) = proches.into_iter().min() {
            a_envoyer = n;
            avant = Some(base);
            nouvelle = true;
        }
    }
    let web = url_web(&url_remote(remote, chemin));
    if a_envoyer == 0 {
        git(&["branch", "-q", "--set-upstream-to", &format!("{remote}/{branche}"), &branche], p(chemin));
        if recus > 0 {
            return fait("ok", format!("pull : {recus} commit(s) ; le serveur et ce dossier sont pareils"));
        }
        return fait("a_jour", "rien de nouveau : le serveur a ce contenu");
    }
    for tentative in [1, 2, 3] {
        let (code, _, e) = git(&["push", "-u", remote, &branche], p(chemin));
        if code == 0 {
            break;
        }
        let bas = e.to_lowercase();
        let refus = bas.contains("permission to") && bas.contains("denied");
        let methode = session::methode().unwrap_or_default();
        if tentative < 3 && !auto && refus && ["jeton", "code", "gh"].contains(&methode.as_str())
            && mettre_a_jour_permissions_push(cfg) {
            continue;
        }
        if tentative == 1 && erreur_auth(&e) && !refus && rafraichir_session(cfg) {
            continue;
        }
        if tentative == 1 && !auto && erreur_auth(&e) && !refus {
            session::set_token("");
            if connexion_interactive(cfg, false) {
                continue;
            }
        }
        return Err(Arret::new("erreur", expliquer(&e)));
    }
    let changements = changements_entre(chemin, avant.as_deref(), "HEAD");
    // commit_affiche : le seul commit envoye vient d'etre montre, sa liste suffit
    if !auto && !(commit_affiche && a_envoyer == 1 && avant.is_some() && recus == 0) {
        println!();
        let debut = if avant.is_none() { "Premier envoi" } else if nouvelle { "Nouvelle branche, en plus de sa base" } else { "Contenu du push" };
        info(&format!("{debut} : {}", bilan_changements(&changements)));
        afficher_changements(&changements, 25);
        if avant.is_some() {
            info("Les autres fichiers sont pareils. Sur GitHub, chaque fichier montre le");
            info("dernier commit qui le change vraiment : c'est normal.");
        }
    }
    let mut detail = format!("push : {a_envoyer} commit(s)");
    if auto {
        detail += &format!(", {}", bilan_changements(&changements));
    }
    if recus > 0 {
        detail += &format!(", pull : {recus}");
    }
    if !web.is_empty() {
        detail += &format!("  ->  {web}");
    }
    fait("ok", detail)
}

/// Verifie branche, remote et operation en cours. Retourne (branche, remote).
pub fn preparer(cfg: &mut Config, chemin: &Path, auto: bool, lier: bool) -> R<(String, String)> {
    nettoyer_depot(chemin, !auto);
    if let Some(op) = operation_en_cours(chemin) {
        if auto {
            return Err(Arret::new("ignore", format!("{op} en cours : conflit pas fini")));
        }
        warn(&format!("Un {op} est en cours : conflit pas fini."));
        if !ask_yn("L'annuler pour revenir comme avant ?", true) {
            return Err(Arret::new("conflit", format!("pour le finir : git add .  puis  git {op} --continue")));
        }
        git(&[op, "--abort"], p(chemin));
    }
    let Some(branche) = branche_courante(chemin) else {
        return Err(Arret::new("erreur", "HEAD hors branche : passer sur une branche avec git switch main"));
    };
    let remote = match remote_principal(chemin) {
        Some(r) => r,
        None => {
            if auto || !lier {
                return Err(Arret::new("ignore", "aucun lien avec un repo sur le serveur"));
            }
            lier_a_github(cfg, chemin, false).ok_or_else(|| Arret::new("erreur", "aucun repo en ligne pour ce dossier"))?
        }
    };
    Ok((branche, remote))
}

/// git pull (fetch + rebase).
pub fn recuperer_projet(cfg: &mut Config, chemin: &Path, auto: bool) -> R<Res> {
    let (branche, remote) = preparer(cfg, chemin, auto, false)?;
    let avant = if a_des_commits(chemin) { Some(git(&["rev-parse", "HEAD"], p(chemin)).1) } else { None };
    if !auto {
        info("Lecture du serveur...");
    }
    recuperer(cfg, chemin, &remote, auto)?;
    let (_, recus) = integrer(chemin, &remote, &branche, auto, false)?;
    if recus == 0 {
        return fait("a_jour", "rien de nouveau sur le serveur");
    }
    let changements = changements_entre(chemin, avant.as_deref(), "HEAD");
    if !auto {
        info(&format!("Pull : {}", bilan_changements(&changements)));
        afficher_changements(&changements, 25);
    }
    let bilan = if auto { format!(", {}", bilan_changements(&changements)) } else { String::new() };
    fait("ok", format!("pull : {recus} commit(s){bilan}"))
}

/// Version sans question, pour tous les repos d'un dossier.
pub fn synchroniser(cfg: &mut Config, chemin: &Path, message: Option<&str>, envoyer: bool) -> (&'static str, String) {
    let res = (|| -> R<Res> {
        if !envoyer {
            return recuperer_projet(cfg, chemin, true);
        }
        let (branche, remote) = preparer(cfg, chemin, true, true)?;
        committer(cfg, chemin, message, true, true)?;
        pousser(cfg, chemin, &remote, &branche, true, false)
    })();
    match res {
        Ok(Res::Fait(s, d)) => (s, d),
        Ok(_) => ("ok", String::new()),
        Err(a) => (a.statut, a.detail),
    }
}

/// Choix pour lier un dossier.
#[derive(Clone, Debug)]
pub enum Lien {
    Repo(Value),
    Autre,
    Nouveau,
    Init,
}

/// Repo GitHub du compte en session qui porte le nom du dossier, ou None.
pub fn repo_du_meme_nom(cfg: &mut Config, chemin: &Path) -> Option<Value> {
    if session::token().is_empty() {
        return None;
    }
    let nom = chemin.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    liste_repos(cfg, "").unwrap_or_default().into_iter().find(|d| texte(d, "name").to_lowercase() == nom)
}

/// Choix pour lier un dossier : le repo du meme nom s'il existe, un autre repo
/// en ligne, ou un nouveau repo. Le repo nomme n'est jamais redemande.
pub fn choix_lien(cfg: &mut Config, chemin: &Path, suite: &str) -> Vec<(String, Lien)> {
    let meme = repo_du_meme_nom(cfg, chemin);
    let mut choix = vec![];
    if let Some(m) = &meme {
        choix.push((format!("Lier avec {}{suite}", texte(m, "full_name")), Lien::Repo(m.clone())));
    }
    let autre = if meme.is_some() { "Lier avec un autre repo GitHub en ligne" } else { "Lier avec un repo GitHub en ligne" };
    choix.push((format!("{autre}{suite}"), Lien::Autre));
    choix.push(("Nouveau repo GitHub".into(), Lien::Nouveau));
    choix
}

/// Adresse du repo choisi dans choix_lien, ou None pour revenir.
pub fn adresse_du_choix(cfg: &mut Config, choix: &Lien) -> Option<String> {
    if let Lien::Repo(d) = choix {
        let url = texte(d, "clone_url");
        return if url.is_empty() { url_repo_en_ligne(cfg, &texte(d, "full_name")).ok() } else { Some(url) };
    }
    let cible = ask_libre("Repo en ligne : compte/nom ou URL, Enter pour revenir", "");
    if cible.is_empty() { None } else { adresse_saisie(cfg, &cible) }
}

/// Adresse du repo saisi, ou None apres un message : une adresse refusee,
/// http:// par exemple, ramene d'un seul niveau.
pub fn adresse_saisie(cfg: &Config, cible: &str) -> Option<String> {
    match url_repo_en_ligne(cfg, cible) {
        Ok(u) => Some(u),
        Err(a) => {
            err(&format!("Adresse refusee : {}.", a.detail));
            None
        }
    }
}

/// nom, compte/nom ou URL : adresse du repo en ligne.
pub fn url_repo_en_ligne(cfg: &Config, cible: &str) -> R<String> {
    let mut cible = avec_https(cible);
    if cible.contains("://") || cible.starts_with("git@") {
        return verifier_url(&cible);
    }
    if !cible.contains('/') {
        cible = format!("{}/{cible}", session::login().unwrap_or_default());
    }
    Ok(format!("https://{}/{}.git", host(cfg, None), cible.strip_suffix(".git").unwrap_or(&cible)))
}

/// Lien du dossier avec un repo GitHub qui existe, celui du meme nom propose,
/// ou avec un nouveau repo. nouveau : sans question. Retourne 'origin'.
pub fn lier_a_github(cfg: &mut Config, chemin: &Path, nouveau: bool) -> Option<String> {
    if !assurer_connexion(cfg, true) {
        return None;
    }
    let mut nouveau = nouveau;
    let mut url = None;
    if !nouveau {
        println!();
        let choix = menu_numerote(&choix_lien(cfg, chemin, ""), 1, "Retour")?;
        nouveau = matches!(choix, Lien::Nouveau);
        if !nouveau {
            url = Some(adresse_du_choix(cfg, &choix)?);
        }
    }
    if nouveau {
        let base = chemin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let defaut = Regex::new(r"[^A-Za-z0-9._-]+").unwrap().replace_all(&base, "-").trim_matches('-').to_string();
        let nom = ask("Nom du repo", if defaut.is_empty() { "projet" } else { &defaut });
        let description = ask_libre("Description du repo, Enter pour aucune", "");
        let org = ask_libre("Organisation, vide pour le compte personnel", "");
        let prive = !ask_yn("Repo public ?", false);
        url = Some(creer_depot(cfg, &nom, prive, &org, &description)?);
    }
    let url = url?;
    if remotes(chemin).iter().any(|r| r == "origin") {
        git(&["remote", "set-url", "origin", &url], p(chemin));
    } else {
        git(&["remote", "add", "origin", &url], p(chemin));
    }
    Some("origin".into())
}

pub fn creer_depot(cfg: &mut Config, nom: &str, prive: bool, org: &str, description: &str) -> Option<String> {
    let chemin = if org.is_empty() { "/user/repos".to_string() } else { format!("/orgs/{org}/repos") };
    let mut donnees = json!({"name": nom, "private": prive});
    if !description.is_empty() {
        donnees["description"] = description.into();
    }
    let (code, rep, _) = api(cfg, "POST", &chemin, Some(&donnees));
    if code == 201 {
        session::retirer_supprime(&texte(&rep, "full_name")); // meme nom qu'un repo supprime
        ok(&format!("Nouveau repo : {}, {}", texte(&rep, "html_url"), if prive { "non public" } else { "public" }));
        return Some(texte(&rep, "clone_url"));
    }
    if code == 422 && rep.to_string().contains("already exists") {
        let proprio = if org.is_empty() { session::login().unwrap_or_default() } else { org.to_string() };
        warn(&format!("{proprio}/{nom} existe sur GitHub."));
        if ask_yn("L'utiliser pour ce projet ?", true) {
            let (code2, rep2, _) = api(cfg, "GET", &format!("/repos/{proprio}/{nom}"), None);
            if code2 == 200 {
                return Some(texte(&rep2, "clone_url"));
            }
        }
        return None;
    }
    err(&format!("Nouveau repo impossible, code {code} : {}", message_api(&rep)));
    None
}

/// Infos GitHub (API) du repo relie a ce projet, ou None.
pub fn depot_du_projet(cfg: &mut Config, chemin: &Path) -> Option<Value> {
    let remote = remote_principal(chemin);
    let (h, o, r) = remote.map(|r| infos_url(&url_remote(&r, chemin))).unwrap_or((None, None, None));
    let (Some(o), Some(r)) = (o, r) else {
        err("Ce projet n'a pas de lien avec un repo du compte GitHub en session.");
        return None;
    };
    if h.as_deref() != Some(host(cfg, None).as_str()) {
        err("Ce projet n'a pas de lien avec un repo du compte GitHub en session.");
        return None;
    }
    let (code, rep, _) = api(cfg, "GET", &format!("/repos/{o}/{r}"), None);
    if code != 200 {
        err(&format!("Repo {o}/{r} introuvable, code {code} : {}", message_api(&rep)));
        return None;
    }
    Some(rep)
}

/// Dossier qui contient le meme projet qu'un repo en ligne : lien avec ce repo,
/// puis pull de sa version la plus neuve. Les fichiers de ce dossier qui ne
/// sont pas comme la version en ligne partent dans git stash, au choix ; ceux
/// absents du repo en ligne restent.
pub fn lier_et_recevoir(cfg: &mut Config, chemin: &Path, url: &str) -> R<Res> {
    if racine_repo(chemin).is_none() {
        init_depot(chemin);
    } else if a_des_commits(chemin) {
        return Err(Arret::new("ignore", "ce dossier a son propre historique git : l'ouvrir dans Repos GitHub en local, puis Lier avec GitHub"));
    }
    if remotes(chemin).iter().any(|r| r == "origin") {
        git(&["remote", "set-url", "origin", url], p(chemin));
    } else {
        git(&["remote", "add", "origin", url], p(chemin));
    }
    info("Lecture du repo en ligne...");
    recuperer(cfg, chemin, "origin", false)?;
    let branche = branche_defaut_distante(chemin, "origin").unwrap_or_else(|| "main".into());
    let r = format!("origin/{branche}");
    if !ref_existe(chemin, &format!("refs/remotes/{r}")) {
        return fait("ignore", "le repo en ligne est vide : utiliser Envoyer");
    }
    // la branche locale passe sur la version en ligne, les fichiers du dossier ne bougent pas
    git(&["symbolic-ref", "HEAD", &format!("refs/heads/{branche}")], p(chemin));
    git(&["reset", "--mixed", "--quiet", &r], p(chemin));
    git(&["branch", "--set-upstream-to", &r, &branche], p(chemin));
    let absents = fichiers_modifies(chemin).iter().filter(|l| l.starts_with(" D")).count();
    if absents > 0 {
        // fichiers en ligne absents du dossier : ils arrivent, rien ne se perd ;
        // sans -f, les fichiers deja la ne bougent pas
        git(&["checkout-index", "--all"], p(chemin));
    }
    let modifs = fichiers_modifies(chemin);
    let suivis: Vec<&String> = modifs.iter().filter(|l| !l.starts_with("??")).collect();
    let nouveaux = modifs.len() - suivis.len();
    println!();
    if absents > 0 {
        ok(&format!("Pull OK : {absents} fichier(s) de la version en ligne dans ce dossier."));
    }
    if suivis.is_empty() {
        ok("Ce dossier et la version en ligne sont pareils.");
    } else {
        warn(&format!("{} fichier(s) de ce dossier ne sont pas comme la version en ligne :", suivis.len()));
        for l in suivis.iter().take(20) {
            println!("       {} {}", ljust(&statut_porcelaine(l), 9), chemin_porcelaine(l));
        }
        if suivis.len() > 20 {
            println!("       ... et {} autre(s)", suivis.len() - 20);
        }
        println!();
        println!("    1. Prendre la version en ligne : pull, ces fichiers partent dans git stash");
        println!("    2. Garder les fichiers de ce dossier, commit manquant");
        if ask("Choix", "1") == "1" {
            git(&["stash", "push", "-m", "gitmanager : fichiers avant le pull"], p(chemin));
            ok("Version en ligne OK dans ce dossier.");
            info("Fichiers d'avant : choix 6 du repo, Changements pour plus tard, pour les reprendre.");
        } else {
            info("Les fichiers de ce dossier restent, commit manquant.");
        }
    }
    if nouveaux > 0 {
        info(&format!("{nouveaux} fichier(s) absent(s) du repo en ligne restent, commit manquant."));
    }
    let lien = if url_web(url).is_empty() { url.to_string() } else { url_web(url) };
    fait("ok", format!("lien avec {lien}, branche {branche}"))
}

/// Dossier sans git. Retourne true si un repo a ete cree.
pub fn menu_init(cfg: &mut Config, chemin: &Path) -> bool {
    titre("Nouveau projet git");
    info(&format!("{} n'est pas un repo git.", chemin.display()));
    println!();
    let mut choix = choix_lien(cfg, chemin, ", puis pull");
    let dernier = choix.len() - 1;
    choix[dernier] = ("git init, nouveau repo GitHub, premier commit et push".into(), Lien::Nouveau);
    choix.push(("git init seulement".into(), Lien::Init));
    let Some(choix) = menu_numerote(&choix, 1, "Retour") else { return false };
    match choix {
        Lien::Repo(_) | Lien::Autre => {
            if !assurer_connexion(cfg, true) {
                return false;
            }
            let Some(url) = adresse_du_choix(cfg, &choix) else { return false };
            executer(|| lier_et_recevoir(cfg, chemin, &url));
            pause();
            racine_repo(chemin).is_some()
        }
        Lien::Nouveau | Lien::Init => {
            init_depot(chemin);
            gitignore_standard(cfg, chemin);
            if matches!(choix, Lien::Nouveau) && lier_a_github(cfg, chemin, true).is_some() {
                executer(|| crate::local::act_commit_push(cfg, chemin));
                pause();
            }
            true
        }
    }
}
