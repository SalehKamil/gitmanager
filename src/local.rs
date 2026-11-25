//! Repos GitHub en local : liste des repos de la machine, menu d'un repo et ses
//! actions (envoyer, recevoir, changements, branches, annuler, stash...).

use crate::affichage::{c, cote_a_cote, err, info, ljust, ok, titre, warn};
use crate::comparer::{comparer_copies, contenu_en_ligne, lignes_texte};
use crate::config::{maison, Config};
use crate::connexion::assurer_connexion;
use crate::erreur::{Arret, R};
use crate::git::*;
use crate::github::{api, message_api};
use crate::outils::{ouvrir_explorateur, ouvrir_navigateur};
use crate::projet::*;
use crate::resultat::{afficher_resultat, executer, fait, Res};
use crate::saisie::{ask, ask_libre, ask_yn, message_defaut, pause};
use crate::session;
use crate::urls::{cle_depot, infos_url, sans_guillemets, url_web};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

// Dossiers ignores quand on cherche des projets
const DOSSIERS_IGNORES: [&str; 13] = ["node_modules", ".venv", "venv", "env", "__pycache__", "dist", "build", ".git",
    "AppData", "Library", "Applications", "Program Files", "Windows"];
// Endroits ou chercher les projets de l'ordinateur (relatifs au dossier perso)
const EMPLACEMENTS: [&str; 16] = ["", "Documents", "Desktop", "Bureau", "Projects", "Projets", "projets", "dev", "code",
    "src", "repos", "source/repos", "GitHub", "Documents/GitHub", "OneDrive/Documents", "workspace"];

fn p(chemin: &Path) -> Option<&Path> {
    Some(chemin)
}

fn ignore(nom: &str) -> bool {
    DOSSIERS_IGNORES.contains(&nom) || nom == "Program Files (x86)"
}

/// Chemin saisi : guillemets retires, ~ et variables remplaces, puis normalise.
pub fn nettoyer_chemin(saisie: &str) -> PathBuf {
    let mut s = sans_guillemets(saisie).to_string();
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        s = format!("{}{}", maison().display(), &s[1..]);
    }
    let variable = regex::Regex::new(r"\$(\w+)|\$\{([^}]+)\}|%([^%]+)%").unwrap();
    s = variable.replace_all(&s, |m: &regex::Captures| {
        let nom = m.get(1).or(m.get(2)).or(m.get(3)).unwrap().as_str();
        if m.get(3).is_some() && !cfg!(windows) {
            return m[0].to_string();
        }
        std::env::var(nom).unwrap_or_else(|_| m[0].to_string())
    }).into_owned();
    let mut res = PathBuf::new();
    for comp in Path::new(&s).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(res.components().last(), Some(Component::Normal(_))) {
                    res.pop();
                } else if !res.has_root() {
                    res.push("..");
                }
            }
            autre => res.push(autre.as_os_str()),
        }
    }
    if res.as_os_str().is_empty() { PathBuf::from(".") } else { res }
}

/// Dossier saisi, cinq essais au plus ; None pour revenir d'un niveau. Au
/// premier essai, Enter garde le dossier propose et 0 revient.
pub fn choisir_dossier(cfg: &mut Config, question: &str, cle: &str) -> Option<PathBuf> {
    let defaut = Some(cfg.texte(cle)).filter(|d| !d.is_empty())
        .unwrap_or_else(|| std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default());
    for essai in 1..=5 {
        let saisie = if essai == 1 {
            ask_libre(&format!("{question}, 0 pour revenir"), &defaut)
        } else {
            ask_libre(&format!("{question}, Enter pour revenir"), "")
        };
        if saisie.is_empty() || saisie == "0" {
            return None;
        }
        let chemin = nettoyer_chemin(&saisie);
        if chemin.is_dir() {
            cfg.set(cle, chemin.display().to_string());
            cfg.sauver();
            return Some(chemin);
        }
        err(&format!("Dossier introuvable : {}{}", chemin.display(),
                     if essai < 5 { format!(", essai {essai} sur 5.") } else { ".".into() }));
    }
    info("5 essais sans dossier : retour.");
    None
}

/// Repos git dans ce dossier et ses sous-dossiers (2 niveaux).
pub fn trouver_projets(dossier: &Path, profondeur: u32) -> Vec<PathBuf> {
    if dossier.join(".git").exists() {
        return vec![dossier.to_path_buf()];
    }
    if profondeur == 0 {
        return vec![];
    }
    let Ok(lecture) = std::fs::read_dir(dossier) else { return vec![] };
    let mut entrees: Vec<_> = lecture.flatten().collect();
    entrees.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    let mut trouves = vec![];
    for e in entrees {
        let nom = e.file_name().to_string_lossy().to_string();
        if e.path().is_dir() && !nom.starts_with('.') && !ignore(&nom) {
            trouves.extend(trouver_projets(&e.path(), profondeur - 1));
        }
    }
    trouves
}

/// Date du dernier travail git sur ce projet (pour trier la liste).
fn recent(projet: &Path) -> std::time::SystemTime {
    for f in ["index", "HEAD"] {
        if let Ok(t) = std::fs::metadata(projet.join(".git").join(f)).and_then(|m| m.modified()) {
            return t;
        }
    }
    std::time::UNIX_EPOCH
}

fn normcase(chemin: &Path) -> String {
    let t = chemin.display().to_string();
    if cfg!(windows) { t.to_lowercase().replace('/', "\\") } else { t }
}

/// Projets git trouves aux endroits habituels, les plus recents d'abord.
pub fn projets_de_la_machine(cfg: &Config) -> Vec<PathBuf> {
    let m = maison();
    let dernier = cfg.texte("dernier_dossier");
    let mut racines: Vec<PathBuf> = vec![PathBuf::from(cfg.texte("espace")),
        Path::new(&dernier).parent().map(Path::to_path_buf).unwrap_or_default()];
    racines.extend(EMPLACEMENTS.iter().map(|d| if d.is_empty() { m.clone() } else { m.join(d) }));
    let (mut vus, mut projets) = (std::collections::HashSet::new(), vec![]);
    for r in racines {
        if r.as_os_str().is_empty() || !r.is_dir() {
            continue;
        }
        for p in trouver_projets(&r, 2) {
            let absolu = std::path::absolute(&p).unwrap_or(p.clone());
            if vus.insert(normcase(&absolu)) {
                projets.push(p);
            }
        }
    }
    let mut avec_date: Vec<(std::time::SystemTime, PathBuf)> = projets.into_iter().map(|p| (recent(&p), p)).collect();
    avec_date.sort_by(|a, b| b.0.cmp(&a.0));
    avec_date.into_iter().map(|(_, p)| p).collect()
}

/// {compte/repo: dossier local} des copies sur cette machine, cherchees aux memes
/// endroits que la liste Repos GitHub en local. La plus recente passe en premier.
pub fn index_locaux(cfg: &Config) -> HashMap<String, PathBuf> {
    let projets = projets_de_la_machine(cfg);
    let mut index = HashMap::new();
    for (p, urls) in projets.iter().zip(en_parallele(&projets, |p| urls_remotes(p))) {
        for (_, url) in urls {
            if let Some(cle) = cle_depot(&url) {
                index.entry(cle).or_insert_with(|| p.clone());
            }
        }
    }
    index
}

pub fn nom_court(chemin: &Path) -> String {
    let m = maison().display().to_string();
    let t = chemin.display().to_string();
    let sep = std::path::MAIN_SEPARATOR.to_string();
    if normcase(chemin).starts_with(&(normcase(Path::new(&m)) + &sep)) { format!("~{}", &t[m.len()..]) } else { t }
}

pub fn entete_projet(chemin: &Path) {
    let branche = branche_courante(chemin);
    let remote = remote_principal(chemin);
    let url = remote.as_ref().map(|r| url_remote(r, chemin)).unwrap_or_default();
    let modifs = fichiers_modifies(chemin);
    println!("{}", c(&format!("  Projet  : {}", chemin.display()), "1"));
    let serveur = if !url_web(&url).is_empty() { url_web(&url) } else if !url.is_empty() { url.clone() } else { c("sans lien", "33") };
    println!("  Branche : {}     Serveur : {serveur}", branche.as_deref().unwrap_or("HEAD hors branche"));
    // seulement ce qui differe : rien a 0, pour ne pas faire croire a un ecart
    let mut etat: Vec<String> = if modifs.is_empty() { vec![] } else { vec![format!("{} fichier(s), commit manquant", modifs.len())] };
    if let (Some(r), Some(b)) = (&remote, &branche) {
        match avance_retard(chemin, r, b) {
            None => etat.push(if a_des_commits(chemin) { "aucun push sur le serveur" } else { "aucun commit" }.into()),
            Some((push, pull)) => {
                for (action, n) in [("push", push), ("pull", pull)] {
                    if n > 0 {
                        etat.push(format!("{action} : {n} commit(s)"));
                    }
                }
            }
        }
    }
    if etat.is_empty() {
        etat.push(if remote.is_some() && branche.is_some() { "pareil en local et en ligne" } else { "aucun changement depuis le dernier commit" }.into());
    }
    println!("  Statut  : {}", etat.join(" | "));
    if let Some(op) = operation_en_cours(chemin) {
        warn(&format!("Un {op} est en cours : conflit pas fini."));
    }
}

/// Voir les changements : les commits, ou les differences en local et en ligne.
pub fn act_voir_changements(cfg: &mut Config, chemin: &Path) -> R<Res> {
    loop {
        titre("Voir les changements");
        println!("    1. Voir les commits : log");
        println!("    2. Voir les differences en local et en ligne : fetch, diff");
        println!("    0. Retour");
        let res = match ask("Choix", "0").as_str() {
            "1" => executer(|| voir_commits_locaux(chemin)),
            "2" => executer(|| act_comparer_local(cfg, chemin)),
            _ => return Ok(Res::Retour),
        };
        if res != Some(Res::Retour) {
            pause();
        }
    }
}

/// Commits en avance de chaque cote, seulement s'il y en a, puis l'historique.
pub fn voir_commits_locaux(chemin: &Path) -> R<Res> {
    let (branche, remote) = (branche_courante(chemin), remote_principal(chemin));
    println!();
    if !a_des_commits(chemin) {
        info("Aucun commit dans ce repo.");
        return Ok(Res::Rien);
    }
    if let Some(r) = &remote {
        if git(&["fetch", r], p(chemin)).0 != 0 {
            warn("Serveur injoignable : les commits en ligne datent du dernier fetch.");
        }
    }
    let r = match (&remote, &branche) { (Some(r), Some(b)) => format!("{r}/{b}"), _ => String::new() };
    if !r.is_empty() && ref_existe(chemin, &format!("refs/remotes/{r}")) {
        let mut ecart = false;
        for (plage, libelle) in [(format!("{r}..HEAD"), "En local, commits pas encore en ligne : Envoyer"),
                                 (format!("HEAD..{r}"), "En ligne, commits pas encore ici : Recevoir")] {
            let (_, log, _) = git(&["log", "--format=%h  %ad  %s", "--date=short", "-15", &plage], p(chemin));
            if !log.is_empty() {
                ecart = true;
                warn(libelle);
                for l in log.lines() {
                    println!("       {l}");
                }
                println!();
            }
        }
        if !ecart {
            ok("Commits pareils en local et en ligne.");
            println!();
        }
    } else if remote.is_some() {
        info("Branche pas encore en ligne : Envoyer la met en ligne.");
        println!();
    }
    info(&format!("Historique de la branche {}, 15 derniers commits :", branche.as_deref().unwrap_or("actuelle")));
    let (_, log, _) = git(&["log", "--format=%h  %ad  %an  %s", "--date=short", "-15"], p(chemin));
    for l in log.lines() {
        println!("       {l}");
    }
    Ok(Res::Rien)
}

fn dans_le_dernier_commit(chemin: &Path, fichier: &str) -> bool {
    git(&["cat-file", "-e", &format!("HEAD:{fichier}")], p(chemin)).0 == 0
}

/// Les deux versions d'un fichier : dernier commit a gauche, cette machine a droite.
fn voir_fichier_local(chemin: &Path, fichier: &str) {
    let gauche = if dans_le_dernier_commit(chemin, fichier) { contenu_en_ligne(chemin, "HEAD", fichier) } else { Some(vec![]) };
    let complet = chemin.join(fichier);
    let droite = if complet.is_file() { lignes_texte(&complet) } else { Some(vec![]) };
    println!();
    println!("{}", c(&format!("  {fichier}"), "1;36"));
    let (Some(g), Some(d)) = (gauche, droite) else {
        info("Fichier binaire : pas de comparaison ligne par ligne.");
        return;
    };
    info("En jaune : lignes avec changements. Case vide : ligne absente de ce fichier.");
    println!();
    cote_a_cote(&g, &d, if g.is_empty() { "absent" } else { "" }, if complet.is_file() { "" } else { "absent" },
                ("Dernier commit", "Cette machine"));
}

/// Comparer en ligne et en local, depuis l'ecran d'un repo local.
pub fn act_comparer_local(cfg: &mut Config, chemin: &Path) -> R<Res> {
    let Some(remote) = remote_principal(chemin) else {
        return Err(Arret::new("ignore", "aucun lien avec un repo en ligne : choix 11, Lier avec GitHub"));
    };
    let url = url_remote(&remote, chemin);
    let defaut = branche_defaut_distante(chemin, &remote).or_else(|| branche_courante(chemin));
    let d = json!({"default_branch": defaut, "html_url": if url_web(&url).is_empty() { url.clone() } else { url_web(&url) }});
    comparer_copies(cfg, &d, chemin)
}

/// Branches : passer sur une branche, en faire une, fusionner, supprimer.
pub fn act_branches(cfg: &mut Config, chemin: &Path) -> R<Res> {
    loop {
        titre("Branches");
        let (ici, remote) = (branche_courante(chemin), remote_principal(chemin));
        let (locales, distantes) = (branches_locales(chemin), branches_en_ligne(chemin, remote.as_deref()));
        let mut liste: Vec<(String, &str)> = locales.iter()
            .map(|b| (b.clone(), if distantes.contains(b) { "ici et en ligne" } else { "ici seulement" })).collect();
        liste.extend(distantes.iter().filter(|b| !locales.contains(b)).map(|b| (b.clone(), "en ligne seulement")));
        println!();
        for (i, (b, ou)) in liste.iter().enumerate() {
            let etoile = if Some(b) == ici.as_ref() { '*' } else { ' ' };
            println!("    {:>3}. {etoile} {:<40} {ou}", i + 1, b.chars().take(40).collect::<String>());
        }
        println!("         * : branche actuelle");
        println!();
        println!("      N. Nouvelle branche : switch -c");
        println!();
        let choix = ask_libre("Choix : # de la branche ou N, Enter pour revenir", "").to_lowercase();
        if choix.is_empty() {
            return Ok(Res::Retour);
        }
        if choix == "n" {
            nouvelle_branche(chemin);
        } else if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=liste.len()).contains(n)) {
            let (b, ou) = liste[n - 1].clone();
            if action_branche(cfg, chemin, remote.as_deref().unwrap_or(""), ici.as_deref().unwrap_or(""), &b, ou) {
                continue;
            }
        } else {
            warn("Choix invalide.");
            continue;
        }
        pause();
    }
}

/// Parcourir les fichiers d'une branche du repo local, sans changer de branche.
fn parcourir_ref(chemin: &Path, r: &str, nom_branche: &str) {
    let mut dossier = String::new();
    loop {
        let cible = if dossier.is_empty() { r.to_string() } else { format!("{r}:{dossier}") };
        let (_, out, _) = git(&["ls-tree", "-z", &cible], p(chemin));
        let mut entrees: Vec<(String, String)> = out.split('\0').filter(|x| !x.is_empty()).filter_map(|rec| {
            let (meta, nom) = rec.split_once('\t')?;
            Some((nom.to_string(), meta.split_whitespace().nth(1)?.to_string())) // blob, tree ou commit
        }).collect();
        entrees.sort_by_key(|(nom, genre)| (genre != "tree", nom.to_lowercase()));
        println!();
        let en_ligne = if nom_branche != r { ", en ligne" } else { "" };
        println!("{}", c(&format!("  Branche {nom_branche}{en_ligne} : /{dossier}"), "1;36"));
        if !dossier.is_empty() {
            println!("      0. ..  dossier parent, ou Enter");
        }
        for (i, (nom, genre)) in entrees.iter().enumerate() {
            let fin = match genre.as_str() { "tree" => "/", "commit" => " : sous-repo git", _ => "" };
            println!("    {:>3}. {nom}{fin}", i + 1);
        }
        let choix = ask_libre("Choix : # du dossier ou du fichier, Enter pour revenir", "");
        if choix.is_empty() && dossier.is_empty() {
            return;
        }
        if ["", "0", ".."].contains(&choix.as_str()) && !dossier.is_empty() {
            dossier = dossier.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
        } else if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=entrees.len()).contains(n)) {
            let (nom, genre) = &entrees[n - 1];
            let chemin_fichier = if dossier.is_empty() { nom.clone() } else { format!("{dossier}/{nom}") };
            if genre == "tree" {
                dossier = chemin_fichier;
                continue;
            }
            if genre == "commit" {
                info(&format!("{nom} : sous-repo git, ses fichiers sont dans son propre repo."));
                continue;
            }
            let texte = contenu_en_ligne(chemin, r, &chemin_fichier);
            println!();
            println!("{}", c(&format!("  {chemin_fichier}, branche {nom_branche}"), "1"));
            match texte {
                None => info("Fichier binaire : pas d'affichage ici."),
                Some(t) => {
                    for (i, l) in t.iter().take(80).enumerate() {
                        println!("{}{l}", c(&format!("  {:>4} ", i + 1), "36"));
                    }
                    if t.len() > 80 {
                        info(&format!("... {} ligne(s) de plus", t.len() - 80));
                    }
                }
            }
            pause();
        }
    }
}

fn nouvelle_branche(chemin: &Path) {
    let nom = ask_libre("Nom de la nouvelle branche, Enter pour revenir", "");
    if nom.is_empty() {
        return;
    }
    if git(&["check-ref-format", "--branch", &nom], p(chemin)).0 != 0 {
        err("Nom de branche invalide : pas d'espace ni de signe comme ~ ^ : ? *");
        return;
    }
    let (code, _, e) = git(&["switch", "-c", &nom], p(chemin));
    if code != 0 {
        err(&format!("Nouvelle branche impossible : {}", expliquer(&e)));
        return;
    }
    ok(&format!("Nouvelle branche : {nom}, maintenant la branche actuelle. Envoyer la met en ligne."));
}

/// Menu d'une branche. Retourne true pour revenir a la liste sans pause.
fn action_branche(cfg: &mut Config, chemin: &Path, remote: &str, ici: &str, branche: &str, ou: &str) -> bool {
    println!();
    println!("{}", c(&format!("  Branche {branche} : {ou}"), "1"));
    println!();
    let mut actions = vec![("Parcourir les fichiers de cette branche".to_string(), "parcourir"),
                           ("Voir les commits de cette branche".to_string(), "commits")];
    if branche != ici {
        actions.push(("Passer sur cette branche : switch".into(), "passer"));
        actions.push((format!("Fusionner {branche} dans {ici} : merge"), "fusionner"));
        actions.push(("Supprimer cette branche : branch -d".into(), "supprimer"));
    }
    for (i, (libelle, _)) in actions.iter().enumerate() {
        println!("    {}. {libelle}", i + 1);
    }
    println!("    0. Retour");
    let choix = ask("Choix", "0");
    let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=actions.len()).contains(n)) else { return true };
    let action = actions[n - 1].1;
    let locale = ou != "en ligne seulement";
    let r = if locale { branche.to_string() } else { format!("{remote}/{branche}") };
    match action {
        "parcourir" => {
            parcourir_ref(chemin, &r, branche);
            return true;
        }
        "commits" => {
            let (_, log, _) = git(&["log", "--format=%h  %ad  %an  %s", "--date=short", "-30", &r], p(chemin));
            println!();
            info(&format!("Commits de la branche {branche}, 30 derniers :"));
            for l in log.lines() {
                println!("       {l}");
            }
        }
        "passer" => {
            let suivi = format!("{remote}/{branche}");
            let args: Vec<&str> = if locale { vec!["switch", branche] } else { vec!["switch", "-c", branche, "--track", &suivi] };
            let (code, _, e) = git(&args, p(chemin));
            if code != 0 {
                err(&format!("Changement de branche impossible : {}", expliquer(&e)));
                info("Les changements au commit manquant peuvent bloquer : Envoyer, ou les ranger");
                info("pour plus tard avec git stash, choix 6.");
            } else {
                ok(&format!("Branche actuelle : {branche}"));
            }
        }
        "fusionner" => {
            let (code, out, e) = git(&["merge", "--no-edit", "-m", &format!("Fusion de {branche} dans {ici}"), &r], p(chemin));
            if code == 0 {
                ok(&format!("Fusion OK : {branche} dans {ici}. Envoyer la met en ligne."));
                return false;
            }
            let conflits = fichiers_en_conflit(chemin);
            if conflits.is_empty() {
                err(&format!("Fusion impossible : {}", expliquer(if e.is_empty() { &out } else { &e })));
                return false;
            }
            warn("Conflit : des lignes ont des changements dans les deux branches :");
            for f in conflits.iter().take(20) {
                println!("       {f}");
            }
            if ask_yn("Annuler la fusion ? Rien n'est perdu.", true) {
                git(&["merge", "--abort"], p(chemin));
                info("Fusion : abandon, retour comme avant.");
            } else {
                info("Ouvrir ces fichiers, garder la bonne version entre <<<<<<< et >>>>>>>,");
                info("puis : git add .   et   git commit");
            }
        }
        _ => {
            if locale {
                let (mut code, _, mut e) = git(&["branch", "-d", branche], p(chemin));
                if code != 0 && e.to_lowercase().contains("not fully merged") {
                    if !ask_yn("Supprimer cette branche et ses commits sans fusion ?", false) {
                        return false;
                    }
                    (code, _, e) = git(&["branch", "-D", branche], p(chemin));
                }
                if code != 0 {
                    err(&format!("Suppression impossible : {}", expliquer(&e)));
                    return false;
                }
                ok(&format!("Branche {branche} : suppression OK ici."));
            }
            let question = if locale { format!("Supprimer aussi la branche {branche} en ligne ?") }
                           else { format!("Supprimer la branche {branche} en ligne ? Sans retour.") };
            if ou != "ici seulement" && ask_yn(&question, false) {
                let (mut code, _, mut e) = git(&["push", remote, "--delete", branche], p(chemin));
                if code != 0 {
                    // repli : l'API GitHub supprime la branche
                    if let (_, Some(o), Some(rp)) = infos_url(&url_remote(remote, chemin)) {
                        if !session::token().is_empty() {
                            let (code2, rep, _) = api(cfg, "DELETE", &format!("/repos/{o}/{rp}/git/refs/heads/{branche}"), None);
                            if code2 == 204 { (code, e) = (0, String::new()) } else { e = message_api(&rep) }
                        }
                    }
                }
                if code == 0 {
                    git(&["fetch", "--prune", remote], p(chemin));
                    ok(&format!("Branche {branche} : suppression OK en ligne."));
                } else {
                    err(&format!("Suppression en ligne impossible : {}", if e.contains('\n') { expliquer(&e) } else { e }));
                }
            }
        }
    }
    false
}

/// Annuler les changements d'un fichier, ou le dernier commit.
pub fn act_annuler(cfg: &mut Config, chemin: &Path) -> R<Res> {
    loop {
        titre("Annuler");
        println!("    1. Annuler les changements d'un fichier, retour au dernier commit : restore");
        println!("    2. Annuler le dernier commit, ici et en ligne : revert + push");
        println!("    0. Retour");
        match ask("Choix", "0").as_str() {
            "1" => annuler_fichier(chemin),
            "2" => annuler_commit(cfg, chemin),
            _ => return Ok(Res::Retour),
        }
        pause();
    }
}

fn annuler_fichier(chemin: &Path) {
    let modifs = fichiers_modifies(chemin);
    println!();
    if modifs.is_empty() {
        info("Aucun changement depuis le dernier commit.");
        return;
    }
    for (i, l) in modifs.iter().take(60).enumerate() {
        println!("    {:>3}. {} {}", i + 1, ljust(&statut_porcelaine(l), 9), chemin_porcelaine(l));
    }
    let choix = ask_libre("Choix : # du fichier, Enter pour revenir", "");
    let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=modifs.len().min(60)).contains(n)) else { return };
    let ligne = &modifs[n - 1];
    let fichier = chemin_porcelaine(ligne);
    let complet = chemin.join(&fichier);
    if complet.is_dir() {
        info("Dossier nouveau : le supprimer dans le gestionnaire de fichiers si besoin.");
        return;
    }
    voir_fichier_local(chemin, &fichier);
    println!();
    if !ask_yn("Annuler les changements de ce fichier ? Sans retour.", false) {
        info("Abandon.");
        return;
    }
    let (code, e) = if dans_le_dernier_commit(chemin, &fichier) {
        let (code, _, e) = git(&["restore", "--source=HEAD", "--staged", "--worktree", "--", &fichier], p(chemin));
        (code, e)
    } else {
        // fichier nouveau : il disparait
        if !ligne.starts_with("??") {
            git(&["rm", "--cached", "--quiet", "--ignore-unmatch", "--", &fichier], p(chemin));
        }
        match std::fs::remove_file(&complet) {
            Ok(()) => (0, String::new()),
            Err(e) => (1, e.to_string()),
        }
    };
    if code == 0 {
        ok(&format!("{fichier} : retour au dernier commit OK."));
    } else {
        err(&format!("Annulation impossible : {}", expliquer(&e)));
    }
}

fn annuler_commit(cfg: &mut Config, chemin: &Path) {
    println!();
    if !a_des_commits(chemin) {
        info("Aucun commit dans ce repo.");
        return;
    }
    let (_, dernier, _) = git(&["log", "-1", "--format=%h  %s"], p(chemin));
    info(&format!("Dernier commit : {dernier}"));
    let distante = match (remote_principal(chemin), branche_courante(chemin)) {
        (Some(r), Some(b)) => format!("{r}/{b}"),
        _ => "@{u}".into(),
    };
    let en_ligne = git(&["merge-base", "--is-ancestor", "HEAD", &distante], p(chemin)).0 == 0;
    if en_ligne {
        info("Ce commit est en ligne : un commit inverse efface ses changements ici, puis le");
        info("push le met en ligne, git revert. L'historique garde les deux commits.");
        if !ask_yn("Annuler ce commit, ici et en ligne ?", true) {
            info("Abandon.");
            return;
        }
        if git(&["diff", "--cached", "--quiet"], p(chemin)).0 != 0 {
            // changements deja dans l'index (git add) : ils resteraient dans le commit
            // d'annulation ; ils repassent en commit manquant, rien ne change sur le disque
            info("Changements en cours : hors de l'annulation, ils restent sur cette machine.");
            git(&["reset", "-q"], p(chemin));
        }
        let fusion = git(&["rev-parse", "--verify", "-q", "HEAD^2"], p(chemin)).0 == 0;
        // un commit de fusion s'annule par rapport a sa premiere branche : -m 1
        let mut args = vec!["revert", "--no-commit"];
        if fusion {
            args.extend(["-m", "1"]);
        }
        args.push("HEAD");
        let (mut code, _, mut e) = git(&args, p(chemin));
        if code == 0 {
            let message = format!("Annulation du commit {}", dernier.splitn(2, "  ").collect::<Vec<_>>().join(" : "));
            (code, _, e) = git(&["commit", "-m", &message], p(chemin));
        }
        if code != 0 {
            git(&["revert", "--abort"], p(chemin));
            err(&format!("Annulation impossible : {}", expliquer(&e)));
            return;
        }
        let res = preparer(cfg, chemin, false, true)
            .and_then(|(b, r)| pousser(cfg, chemin, &r, &b, false, true));
        match res {
            Ok(Res::Fait(statut, detail)) => {
                afficher_resultat(statut, &detail, "");
                if statut == "ok" {
                    ok("Annulation du commit OK, ici et en ligne.");
                }
            }
            Ok(_) => {}
            Err(a) => {
                afficher_resultat(a.statut, &a.detail, "");
                warn("Annulation faite ici seulement : son commit attend, Envoyer le met en ligne.");
            }
        }
        return;
    }
    info("Ce commit n'est pas en ligne : l'annulation se fait ici seulement.");
    if !ask_yn("Annuler ce commit ? Ses fichiers restent, commit manquant.", true) {
        info("Abandon.");
        return;
    }
    let a_un_parent = git(&["rev-parse", "--verify", "-q", "HEAD~1"], p(chemin)).0 == 0;
    let (code, _, e) = if a_un_parent {
        git(&["reset", "--soft", "HEAD~1"], p(chemin))
    } else {
        git(&["update-ref", "-d", "HEAD"], p(chemin)) // premier commit du repo
    };
    if code == 0 {
        ok("Annulation du commit OK : ses fichiers restent, commit manquant.");
    } else {
        err(&format!("Annulation impossible : {}", expliquer(&e)));
    }
}

/// Changements pour plus tard : git stash.
pub fn act_stash(_cfg: &mut Config, chemin: &Path) -> R<Res> {
    let texte_git = regex::Regex::new(r"^(?:On|WIP on) ([^:]+): (.*)$").unwrap(); // On <branche>: <nom>
    loop {
        titre("Changements pour plus tard : git stash");
        info("git stash range les changements au commit manquant pour plus tard : le dossier");
        info("revient au dernier commit, et les changements peuvent revenir quand il faut.");
        let (_, out, _) = git(&["stash", "list", "--format=%gd%x1f%ci%x1f%gs"], p(chemin));
        let attentes: Vec<(String, String, String)> = out.lines().filter(|l| l.matches('\x1f').count() == 2).map(|l| {
            let v: Vec<&str> = l.split('\x1f').collect();
            let nom = texte_git.captures(v[2]).map(|m| format!("{}, branche {}", &m[2], &m[1])).unwrap_or_else(|| v[2].to_string());
            (v[0].to_string(), v[1].chars().take(16).collect(), nom)
        }).collect();
        println!();
        if attentes.is_empty() {
            info("Aucun changement pour plus tard.");
        }
        for (i, (_, quand, nom)) in attentes.iter().enumerate() {
            println!("    {:>3}. {quand}  {nom}", i + 1);
        }
        let modifs = fichiers_modifies(chemin);
        println!();
        if !modifs.is_empty() {
            println!("      M. Ranger les changements actuels pour plus tard : {} fichier(s)", modifs.len());
            println!();
        }
        let choix = ask_libre(&format!("Choix : # pour reprendre ou supprimer{}, Enter pour revenir",
                                       if modifs.is_empty() { "" } else { ", M" }), "").to_lowercase();
        if choix.is_empty() {
            return Ok(Res::Retour);
        }
        if choix == "m" && !modifs.is_empty() {
            let nom = ask("Nom de ces changements, Enter pour garder", "changements en cours"); // date : dans la liste
            let (code, _, e) = git(&["stash", "push", "--include-untracked", "-m", &nom], p(chemin));
            if code == 0 { ok("Rangement OK pour plus tard : le dossier revient au dernier commit.") }
            else { err(&format!("Rangement impossible : {}", expliquer(&e))) }
        } else if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=attentes.len()).contains(n)) {
            let (r, quand, nom) = &attentes[n - 1];
            let (code, mut fichiers, _) = git(&["stash", "show", "--include-untracked", "--name-only", r], p(chemin));
            if code != 0 {
                fichiers = git(&["stash", "show", "--name-only", r], p(chemin)).1; // Git 2.31 : sans les fichiers nouveaux
            }
            println!();
            info(&format!("{quand}  {nom} :"));
            for f in fichiers.lines().take(30) {
                println!("       {f}");
            }
            println!();
            println!("    1. Reprendre ces changements : git stash pop");
            println!("    2. Supprimer ces changements : git stash drop");
            println!("    0. Retour");
            match ask("Choix", "0").as_str() {
                "1" => {
                    let (code, _, e) = git(&["stash", "pop", r], p(chemin));
                    if code == 0 { ok("Changements repris dans le dossier.") } else { err(&format!("Reprise impossible : {}", expliquer(&e))) }
                }
                "2" if ask_yn("Supprimer ces changements ? Sans retour.", false) => {
                    let (code, _, e) = git(&["stash", "drop", r], p(chemin));
                    if code == 0 { ok("Changements : suppression OK.") } else { err(&format!("Suppression impossible : {}", expliquer(&e))) }
                }
                _ => continue,
            }
        } else {
            warn("Choix invalide.");
            continue;
        }
        pause();
    }
}

/// Envoyer : commit des fichiers modifies, puis push.
pub fn act_commit_push(cfg: &mut Config, chemin: &Path) -> R<Res> {
    let (branche, remote) = preparer(cfg, chemin, false, true)?;
    let fait_ = committer(cfg, chemin, None, false, true)?;
    pousser(cfg, chemin, &remote, &branche, false, fait_ > 0)
}

pub fn act_pull(cfg: &mut Config, chemin: &Path) -> R<Res> {
    recuperer_projet(cfg, chemin, false)
}

pub fn act_ecraser(cfg: &mut Config, chemin: &Path) -> R<Res> {
    warn("Remplace TOUT l'historique du serveur par celui de ce dossier. Sans retour.");
    info("Pour garder l'historique : Envoyer, puis 'ce dossier fait foi'.");
    let (branche, remote) = preparer(cfg, chemin, false, true)?;
    recuperer(cfg, chemin, &remote, false)?;
    if ref_existe(chemin, &format!("refs/remotes/{remote}/{branche}")) {
        let (_, perdus, _) = git(&["log", "--oneline", &format!("HEAD..{remote}/{branche}")], p(chemin));
        if !perdus.is_empty() {
            warn("Ces commits du serveur seront PERDUS :");
            for l in perdus.lines().take(20) {
                println!("       {l}");
            }
        }
    }
    println!();
    let web = url_web(&url_remote(&remote, chemin));
    warn(&format!("Cible : {}  branche {branche}", if web.is_empty() { &remote } else { &web }));
    if ask_libre("Taper REMPLACER en majuscules pour confirmer", "") != "REMPLACER" {
        return fait("ignore", "abandon");
    }
    committer(cfg, chemin, None, false, true)?;
    if !a_des_commits(chemin) {
        return Err(Arret::new("ignore", "aucun commit : rien pour le push"));
    }
    let (code, _, e) = git(&["push", "--force-with-lease", "-u", &remote, &branche], p(chemin));
    if code != 0 {
        return Err(Arret::new("erreur", expliquer(&e)));
    }
    fait("ok", "force push OK : le serveur a le contenu de ce dossier")
}

pub fn act_visibilite_projet(cfg: &mut Config, chemin: &Path) -> R<Res> {
    if let Some(mut d) = depot_du_projet(cfg, chemin) {
        crate::en_ligne::changer_visibilite(cfg, &mut d);
    }
    Ok(Res::Rien)
}

pub fn act_relier(cfg: &mut Config, chemin: &Path) -> R<Res> {
    let Some(remote) = lier_a_github(cfg, chemin, false) else { return fait("ignore", "abandon") };
    let url = url_remote(&remote, chemin);
    if !a_des_commits(chemin) {
        return lier_et_recevoir(cfg, chemin, &url); // dossier sans historique : version en ligne
    }
    let lien = if url_web(&url).is_empty() { url.clone() } else { url_web(&url) };
    fait("ok", format!("lien avec {lien} : Envoyer ou Recevoir pour la suite"))
}

/// Supprime le dossier .git (le code reste). Retourne true si fait.
fn act_supprimer_git(chemin: &Path) -> bool {
    let dot_git = chemin.join(".git");
    warn("Suppression de l'historique git LOCAL, le dossier .git. Les fichiers du code restent.");
    info("Le dossier perd son lien avec GitHub. Les envois faits restent sur GitHub.");
    let modifs = fichiers_modifies(chemin);
    let (remote, branche) = (remote_principal(chemin), branche_courante(chemin));
    let ar = match (&remote, &branche) { (Some(r), Some(b)) => avance_retard(chemin, r, b), _ => None };
    if !modifs.is_empty() || ar.is_some_and(|a| a.0 > 0) || (remote.is_some() && branche.is_some() && ar.is_none() && a_des_commits(chemin)) {
        warn(&format!("Attention : une partie du travail n'est PAS sur GitHub : {} fichier(s), commit manquant, {} commit(s) sans push.",
                      modifs.len(), ar.map_or("des".into(), |a| a.0.to_string())));
    }
    if ask_libre("Taper SUPPRIMER en majuscules pour confirmer", "") != "SUPPRIMER" {
        info("Abandon.");
        return false;
    }
    if dot_git.is_dir() {
        supprimer_dossier(&dot_git);
    } else if dot_git.is_file() {
        let _ = std::fs::remove_file(&dot_git);
    }
    if dot_git.exists() {
        err("Suppression pas finie : fermer les programmes qui ouvrent ce dossier, puis recommencer.");
        return false;
    }
    ok(&format!("Suppression du .git OK : {} est maintenant un simple dossier.", chemin.display()));
    true
}

/// Page GitHub du repo auquel ce dossier est relie.
fn ouvrir_en_ligne(chemin: &Path) {
    let Some(remote) = remote_principal(chemin) else {
        warn("Ce dossier n'a aucun lien avec un repo en ligne : choix 11, Lier avec GitHub.");
        return;
    };
    let url = url_web(&url_remote(&remote, chemin));
    if url.is_empty() {
        warn(&format!("Adresse du repo en ligne inconnue : {}", url_remote(&remote, chemin)));
        return;
    }
    ouvrir_navigateur(&url);
}

type Action = fn(&mut Config, &Path) -> R<Res>;

pub fn menu_projet(cfg: &mut Config, chemin: &Path) {
    let chemin = nettoyer_chemin(&chemin.display().to_string());
    let chemin = chemin.as_path();
    let (racine, mut etat) = acces_repo(chemin);
    if etat == "illisible" && ask_yn("Renommer ce .git en .git-illisible, puis lier le dossier avec son repo en ligne ?", true) {
        let (mut cible, mut n) = (chemin.join(".git-illisible"), 2);
        while cible.symlink_metadata().is_ok() {
            cible = chemin.join(format!(".git-illisible-{n}"));
            n += 1;
        }
        let _ = std::fs::rename(chemin.join(".git"), cible);
        etat = "absent";
    }
    if racine.is_none() && (etat != "absent" || !menu_init(cfg, chemin)) {
        return;
    }
    nettoyer_depot(chemin, true);
    if let Some(r) = remote_principal(chemin) {
        if git(&["fetch", &r], p(chemin)).0 != 0 {
            warn("Serveur injoignable : le statut push / pull peut dater.");
        }
    }
    loop {
        titre("Repo local");
        entete_projet(chemin);
        let relie = remote_principal(chemin).is_some();
        println!();
        println!("     1. Envoyer : commit + push");
        println!("     2. Recevoir : pull");
        println!("     3. Voir les changements : log, diff");
        println!("     4. Branches : branch, switch, merge");
        println!("     5. Annuler un commit ou des changements : restore, revert");
        println!("     6. Changements pour plus tard : git stash");
        println!("     7. Ouvrir la copie sur cette machine");
        println!("     8. Ouvrir la copie en ligne");
        println!("     9. Remplacer GitHub par ce dossier : force push");
        println!("    10. Rendre public ou non public");
        println!("    11. {} : remote", if relie { "Lier avec un autre repo GitHub" } else { "Lier avec GitHub" });
        println!("    12. Supprimer le .git de ce dossier : historique local");
        println!("     0. Retour");
        let choix = ask("Choix", "0");
        let action: Action = match choix.as_str() {
            "7" => {
                println!();
                ouvrir_explorateur(chemin);
                continue;
            }
            "8" => {
                println!();
                ouvrir_en_ligne(chemin);
                continue;
            }
            "12" => {
                let fin = act_supprimer_git(chemin);
                pause();
                if fin {
                    return;
                }
                continue;
            }
            "0" | "q" => return,
            "1" => act_commit_push,
            "2" => act_pull,
            "3" => act_voir_changements,
            "4" => act_branches,
            "5" => act_annuler,
            "6" => act_stash,
            "9" => act_ecraser,
            "10" => act_visibilite_projet,
            "11" => act_relier,
            _ => {
                warn("Choix invalide.");
                continue;
            }
        };
        if executer(|| action(cfg, chemin)) != Some(Res::Retour) {
            pause();
        }
    }
}

/// Ouvre le menu d'un repo ; un dossier sans git propose de l'envoyer sur GitHub.
pub fn ouvrir_dossier(cfg: &mut Config, chemin: &Path) {
    let mut chemin = chemin.to_path_buf();
    if let Some(racine) = racine_repo(&chemin) {
        let reel = |x: &Path| normcase(&std::fs::canonicalize(x).unwrap_or(x.to_path_buf()));
        if reel(&racine) != reel(&chemin) {
            info(&format!("Ce dossier fait partie du repo git : {}", racine.display()));
            chemin = racine;
        }
    }
    cfg.set("dernier_dossier", chemin.display().to_string());
    cfg.sauver();
    menu_projet(cfg, &chemin);
}

fn act_autre_dossier(cfg: &mut Config) {
    titre("Ouvrir un autre dossier");
    if let Some(chemin) = choisir_dossier(cfg, "Dossier : repo hors liste, ou dossier pour l'envoi sur GitHub", "dernier_dossier") {
        ouvrir_dossier(cfg, &chemin);
    }
}

pub fn act_depots_locaux(cfg: &mut Config) -> R<Res> {
    while !session::menu_principal() {
        if liste_depots_locaux(cfg) {
            return Ok(Res::Retour);
        }
    }
    Ok(Res::Rien)
}

/// Liste des repos de la machine ; true pour revenir.
fn liste_depots_locaux(cfg: &mut Config) -> bool {
    titre("Repos GitHub en local");
    info("Recherche des repos git sur cette machine...");
    let tous = projets_de_la_machine(cfg);
    let projets: Vec<PathBuf> = tous.iter().take(30).cloned().collect();
    println!();
    if projets.is_empty() {
        info("Aucun repo git sur cette machine.");
    } else {
        println!("{}", c(&format!("    {:>3}  {:<44} {:>15}  GitHub", "#", "Repo", "Commit manquant"), "1"));
        for (i, (p, (url, modifs, _))) in projets.iter().zip(apercu_projets(&projets, false)).enumerate() {
            let mut nom = nom_court(p);
            if nom.chars().count() > 44 {
                let n = nom.chars().count();
                nom = format!("~{}", nom.chars().skip(n - 43).collect::<String>());
            }
            println!("    {:>3}. {nom:<44} {modifs:>15}  {}", i + 1, lien_court(&url));
        }
        if tous.len() > projets.len() {
            info(&format!("... et {} autre(s), moins recents : N pour en ouvrir un.", tous.len() - projets.len()));
        }
    }
    println!();
    println!("      N. Ouvrir un autre dossier : repo hors liste, ou envoi sur GitHub");
    println!("      T. Tous les repos d'un dossier : push ou pull");
    println!("      U. Cloner un repo par son URL : git clone");
    let choix = ask_libre(&format!("Choix : {}N, T ou U, Enter pour revenir", if projets.is_empty() { "" } else { "# du repo, " }), "")
        .to_lowercase();
    if choix.is_empty() {
        return true;
    }
    if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=projets.len()).contains(n)) {
        ouvrir_dossier(cfg, &projets[n - 1]);
    } else if choix == "n" {
        act_autre_dossier(cfg);
    } else if choix == "t" {
        executer(|| act_tout(cfg));
        pause();
    } else if choix == "u" {
        cloner_par_url(cfg);
    } else {
        warn("Choix invalide.");
    }
    false
}

fn act_tout(cfg: &mut Config) -> R<Res> {
    titre("Tous les repos d'un dossier");
    if cfg.texte("espace").is_empty() {
        let dernier = cfg.texte("dernier_dossier");
        let defaut = Path::new(&dernier).parent().map(|d| d.display().to_string()).filter(|d| !d.is_empty())
            .unwrap_or_else(|| std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default());
        cfg.set("espace", defaut);
    }
    let Some(dossier) = choisir_dossier(cfg, "Dossier qui contient les projets", "espace") else { return Ok(Res::Rien) };
    let projets = trouver_projets(&dossier, 2);
    if projets.is_empty() {
        warn("Aucun projet git dans ce dossier ni dans ses sous-dossiers.");
        return Ok(Res::Rien);
    }
    let nom_de = |p: &Path| if p == dossier { p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() }
                            else { p.strip_prefix(&dossier).unwrap_or(p).display().to_string() };
    println!();
    println!("{}", c(&format!("    {:<28} {:<12} {:>15}  Serveur", "Projet", "Branche", "Commit manquant"), "1"));
    for (p, (url, modifs, branche)) in projets.iter().zip(apercu_projets(&projets, true)) {
        let nom: String = nom_de(p).chars().take(28).collect();
        let b: String = branche.chars().take(12).collect();
        println!("    {nom:<28} {b:<12} {modifs:>15}  {}", lien_court(&url));
    }
    println!();
    println!("    1. Tout envoyer : commit + push de chaque repo");
    println!("    2. Tout recevoir : pull de chaque repo");
    println!("    0. Annuler");
    let mode = ask("Choix", "0");
    if mode != "1" && mode != "2" {
        return Ok(Res::Rien);
    }
    let message = if mode == "1" {
        Some(ask("Message du commit pour les projets avec changements, Enter pour garder", &message_defaut()))
    } else {
        None
    };
    if !assurer_connexion(cfg, true) {
        return Ok(Res::Rien);
    }
    println!();
    let mut bilan: Vec<(&str, usize)> = vec![];
    for (i, p) in projets.iter().enumerate() {
        let nom = nom_de(p);
        // une ligne par repo : les commandes git s'affichent dessous, sans s'y melanger
        println!("{}", c(&format!("  [{}/{}] {nom}", i + 1, projets.len()), "1"));
        let (statut, detail) = synchroniser(cfg, p, message.as_deref(), mode == "1");
        afficher_resultat(statut, &detail, &nom.chars().take(28).collect::<String>());
        println!();
        match bilan.iter_mut().find(|(s, _)| *s == statut) {
            Some((_, n)) => *n += 1,
            None => bilan.push((statut, 1)),
        }
    }
    println!();
    let resume = |s: &str| match s { "ok" => "OK", "a_jour" => "sans rien de nouveau", "conflit" => "en conflit",
                                      "ignore" => "sans action", _ => "en erreur" };
    info(&format!("Bilan : {}", bilan.iter().map(|(s, n)| format!("{n} {}", resume(s))).collect::<Vec<_>>().join(", ")));
    if bilan.iter().any(|(s, _)| *s == "conflit") {
        info("Projets en conflit : les ouvrir dans Repos GitHub en local et utiliser Envoyer.");
    }
    Ok(Res::Rien)
}

/// Clone d'un repo dans le dossier choisi ; un dossier qui existe deja peut etre
/// lie avec le repo, puis pull. Retourne le dossier du projet, ou None.
pub fn cloner(cfg: &mut Config, d: &Value) -> Option<PathBuf> {
    let champ = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let defaut = Some(cfg.texte("espace")).filter(|e| !e.is_empty())
        .unwrap_or_else(|| std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default());
    let saisie = ask_libre("Cloner dans le dossier, 0 pour revenir", &defaut);
    if saisie.is_empty() || saisie == "0" {
        return None;
    }
    let parent = nettoyer_chemin(&saisie);
    if !parent.is_dir() && !ask_yn(&format!("Dossier introuvable : {}. Le creer ?", parent.display()), true) {
        return None;
    }
    let dest = parent.join(champ("name"));
    let occupe = dest.exists() && (!dest.is_dir() || std::fs::read_dir(&dest).is_ok_and(|mut r| r.next().is_some()));
    if occupe {
        if dest.is_dir() && ask_yn(&format!("{} existe : le lier avec {}, puis pull ?", dest.display(), champ("full_name")), true) {
            let res = executer(|| crate::projet::lier_et_recevoir(cfg, &dest, &champ("clone_url")));
            if matches!(res, Some(Res::Fait("ok", _))) {
                cfg.set("dernier_dossier", dest.display().to_string());
                cfg.sauver();
                return Some(dest);
            }
            return None;
        }
        err(&format!("{} existe et n'est pas vide.", dest.display()));
        return None;
    }
    let _ = std::fs::create_dir_all(&parent);
    if git_direct(&["clone", &champ("clone_url"), &dest.display().to_string()], None) != 0 {
        err("Clone impossible : voir la connexion et les droits, puis recommencer depuis le menu Compte GitHub.");
        return None;
    }
    cfg.set("dernier_dossier", dest.display().to_string());
    cfg.sauver();
    ok(&format!("Clone dans {}", dest.display()));
    Some(dest)
}

/// Clone d'un repo qui n'est pas dans la liste du compte : URL ou compte/nom.
pub fn cloner_par_url(cfg: &mut Config) {
    let saisie = ask_libre("Repo : URL ou compte/nom, Enter pour revenir", "");
    let Some(url) = (if saisie.is_empty() { None } else { adresse_saisie(cfg, &saisie) }) else { return };
    let base = url.trim_end_matches('/').rsplit(['/', ':']).next().unwrap_or("").to_string();
    let nom = infos_url(&url).2.unwrap_or(base);
    let nom = nom.strip_suffix(".git").unwrap_or(&nom).to_string();
    let d = json!({"name": nom, "full_name": cle_depot(&url).unwrap_or(nom.clone()), "clone_url": url});
    if let Some(dest) = cloner(cfg, &d) {
        menu_projet(cfg, &dest);
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn chemins_nettoyes() {
        assert_eq!(nettoyer_chemin(" \"/a/b/../c/./d/\" "), PathBuf::from("/a/c/d"));
        assert_eq!(nettoyer_chemin("~"), maison());
        assert_eq!(nettoyer_chemin("x/../.."), PathBuf::from(".."));
    }
}
