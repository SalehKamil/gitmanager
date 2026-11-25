//! Repos GitHub en ligne : liste, menu d'un repo, visibilite, suppression.

use crate::affichage::{c, err, info, ok, taille, titre, warn};
use crate::comparer::comparer_copies;
use crate::config::Config;
use crate::connexion::{assurer_connexion, demander_secret, gh_par_navigateur, rafraichir_session, token_silencieux};
use crate::erreur::R;
use crate::git::{git, supprimer_dossier, urls_remotes};
use crate::github::{api, host, liste_repos, message_api, requete, texte};
use crate::local::{cloner, index_locaux, menu_projet};
use crate::menus::ligne_compte;
use crate::outils::{chemin_gh, ouvrir_explorateur, ouvrir_navigateur};
use crate::projet::est_sensible;
use crate::resultat::{executer, Res};
use crate::saisie::{ask, ask_libre, ask_yn, pause};
use crate::session;
use crate::urls::cle_depot;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Encodage d'un chemin dans une adresse : les "/" restent.
pub fn quoter(t: &str) -> String {
    t.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

pub fn act_depots(cfg: &mut Config) -> R<Res> {
    titre("Repos GitHub en ligne");
    if !assurer_connexion(cfg, true) {
        return Ok(Res::Rien);
    }
    let mut locaux = index_locaux(cfg);
    let mut filtre = String::new();
    loop {
        // relue a chaque affichage : un repo ou une permission ajoutes au jeton
        // sur GitHub apparaissent sans reconnexion
        info("Chargement de la liste...");
        let Some(repos) = liste_repos(cfg, "pushed") else { return Ok(Res::Rien) };
        let vue: Vec<&Value> = repos.iter().filter(|d| texte(d, "full_name").to_lowercase().contains(&filtre.to_lowercase())).collect();
        println!();
        ligne_compte(cfg);
        println!();
        let entete = format!("{} repo(s){}", vue.len(), if filtre.is_empty() { String::new() } else { format!(" contenant '{filtre}'") });
        println!("{}", c(&format!("    {entete}"), "1"));
        println!("{}", c(&format!("    {:>3}  {:<42} {:<7} {:<12}  En local", "#", "Repo", "Public", "Dernier push"), "1"));
        for (i, d) in vue.iter().enumerate() {
            let full = texte(d, "full_name");
            let nom = if full.chars().count() <= 42 { full.clone() } else { format!("{}~", full.chars().take(41).collect::<String>()) };
            let vis = if d.get("private").and_then(Value::as_bool) == Some(true) { "non" } else { "oui" };
            let mut date = texte(d, "pushed_at");
            if date.is_empty() {
                date = texte(d, "updated_at");
            }
            let date: String = date.chars().take(10).collect();
            let loc = if locaux.contains_key(&full.to_lowercase()) { "oui" } else { "" };
            println!("    {:>3}. {nom:<42} {vis:<7} {date:<12}  {loc}", i + 1);
        }
        println!();
        let retour = if filtre.is_empty() { "Enter pour revenir" } else { "Enter pour toute la liste" };
        let choix = ask_libre(&format!("Choix : # du repo, ou mot pour chercher, {retour}"), "");
        if choix.is_empty() {
            if filtre.is_empty() {
                return Ok(Res::Rien);
            }
            filtre.clear();
        } else if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=vue.len()).contains(n)) {
            let d = vue[n - 1].clone();
            menu_depot(cfg, d, &mut locaux);
            if session::menu_principal() {
                return Ok(Res::Rien);
            }
        } else if choix.chars().all(|c| c.is_ascii_digit()) && !repos.iter().any(|d| texte(d, "full_name").contains(&choix)) {
            warn(&format!("Choix invalide : numero de 1 a {}.", vue.len())); // pas une recherche vide
        } else {
            filtre = choix;
        }
    }
}

fn menu_depot(cfg: &mut Config, d: Value, locaux: &mut HashMap<String, PathBuf>) {
    let mut d = d;
    loop {
        let full = texte(&d, "full_name");
        let (code, frais, _) = api(cfg, "GET", &format!("/repos/{full}"), None);
        if code == 200 && frais.is_object() {
            d = frais;
        }
        let local = locaux.get(&full.to_lowercase()).cloned();
        let prive = d.get("private").and_then(Value::as_bool) == Some(true);
        titre(&format!("{full} : {}", if prive { "non public" } else { "public" }));
        if !texte(&d, "description").is_empty() {
            info(&texte(&d, "description"));
        }
        info(&texte(&d, "html_url"));
        let branche = texte(&d, "default_branch");
        let pousse: String = texte(&d, "pushed_at").chars().take(10).collect();
        info(&format!("Branche principale : {}    Dernier push : {}", if branche.is_empty() { "-" } else { &branche },
                      if pousse.is_empty() { "-" } else { &pousse }));
        info(&format!("Copie sur cette machine : {}", local.as_ref().map_or("aucune".into(), |l| l.display().to_string())));
        println!();
        println!("    1. Parcourir les fichiers");
        println!("    2. Voir les commits");
        println!("    3. Rendre {}", if prive { "public" } else { "non public" });
        println!("    4. {}", if local.is_some() { "Ouvrir la copie sur cette machine" } else { "Cloner le projet en local : clone" });
        println!("    5. Ouvrir la copie en ligne");
        println!("    6. Comparer en ligne et en local");
        println!("    7. Voir les droits sur ce repo");
        println!("    8. Supprimer de GitHub");
        println!("    0. Retour");
        match ask("Choix", "0").as_str() {
            "1" => parcourir_depot(cfg, &d),
            "2" => voir_commits(cfg, &d),
            "3" => {
                changer_visibilite(cfg, &mut d);
                pause();
            }
            "4" => {
                let mut local = local;
                if local.is_none() {
                    local = cloner(cfg, &d);
                    if let Some(l) = &local {
                        locaux.insert(full.to_lowercase(), l.clone());
                    }
                }
                if let Some(l) = local {
                    ouvrir_explorateur(&l);
                    menu_projet(cfg, &l);
                }
            }
            "5" => {
                println!();
                ouvrir_navigateur(&texte(&d, "html_url"));
            }
            "6" => match local {
                None => {
                    warn("Aucune copie sur cette machine : la cloner avec le choix 4.");
                    pause();
                }
                Some(l) => {
                    if executer(|| comparer_copies(cfg, &d, &l)) != Some(Res::Retour) {
                        pause();
                    }
                }
            },
            "7" => {
                afficher_acces(cfg, &d);
                pause();
            }
            "8" => {
                let fait_ = supprimer_depot(cfg, &d, locaux);
                pause();
                if fait_ {
                    return;
                }
            }
            _ => return,
        }
    }
}

fn contenu(cfg: &mut Config, full: &str, chemin: &str, r: &str) -> (u16, Value, crate::github::Entetes) {
    let suffixe = if chemin.is_empty() { String::new() } else { format!("/{}", quoter(chemin)) };
    api(cfg, "GET", &format!("/repos/{full}/contents{suffixe}?ref={}", quoter(r).replace('/', "%2F")), None)
}

/// Branche du repo en ligne : question seulement quand il y en a plusieurs.
fn choisir_branche_en_ligne(cfg: &mut Config, d: &Value) -> String {
    let full = texte(d, "full_name");
    let mut r = texte(d, "default_branch");
    if r.is_empty() {
        r = "main".into();
    }
    let (code, branches, _) = api(cfg, "GET", &format!("/repos/{full}/branches?per_page=100"), None);
    let noms: Vec<String> = if code == 200 { branches.as_array().map(|a| a.iter().map(|b| texte(b, "name")).collect()).unwrap_or_default() } else { vec![] };
    if noms.len() > 1 {
        println!();
        for (i, nom) in noms.iter().enumerate() {
            println!("    {:>3}. {nom}{}", i + 1, if *nom == r { "  : branche principale" } else { "" });
        }
        let choix = ask_libre("Choix : # de la branche, Enter pour la branche principale", "");
        if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=noms.len()).contains(n)) {
            r = noms[n - 1].clone();
        }
    }
    r
}

fn parcourir_depot(cfg: &mut Config, d: &Value) {
    let full = texte(d, "full_name");
    let r = choisir_branche_en_ligne(cfg, d);
    let mut chemin = String::new();
    loop {
        let (code, rep, _) = contenu(cfg, &full, &chemin, &r);
        if code == 404 && chemin.is_empty() {
            info("Ce repo est vide : aucun fichier pour l'instant.");
            pause();
            return;
        }
        let Some(liste) = rep.as_array().filter(|_| code == 200) else {
            err(&format!("Lecture impossible, code {code} : {}", message_api(&rep)));
            pause();
            return;
        };
        let mut entrees = liste.clone();
        entrees.sort_by_key(|e| (texte(e, "type") != "dir", texte(e, "name").to_lowercase()));
        println!();
        println!("{}", c(&format!("  {full} : /{chemin}    branche {r}"), "1;36"));
        if !chemin.is_empty() {
            println!("      0. ..  dossier parent, ou Enter");
        }
        for (i, e) in entrees.iter().enumerate() {
            let (genre, nom) = (texte(e, "type"), texte(e, "name"));
            let telechargeable = e.get("download_url").is_some_and(|u| !u.is_null());
            if genre == "dir" {
                println!("    {:>3}. {nom}/", i + 1);
            } else if genre == "submodule" || (genre == "file" && !telechargeable) {
                println!("    {:>3}. {nom} : sous-repo git", i + 1);
            } else {
                println!("    {:>3}. {nom:<48} {:>9}", i + 1, taille(e.get("size").and_then(Value::as_u64).unwrap_or(0)));
            }
        }
        let choix = ask_libre("Choix : # du dossier ou du fichier, Enter pour revenir", "");
        if choix.is_empty() && chemin.is_empty() {
            return;
        }
        if ["", "0", ".."].contains(&choix.as_str()) && !chemin.is_empty() {
            chemin = chemin.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
        } else if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=entrees.len()).contains(n)) {
            let e = &entrees[n - 1];
            let (genre, nom) = (texte(e, "type"), texte(e, "name"));
            let telechargeable = e.get("download_url").is_some_and(|u| !u.is_null());
            if genre == "dir" {
                chemin = texte(e, "path");
            } else if genre == "file" && telechargeable {
                afficher_fichier(cfg, &full, &texte(e, "path"), &r, d);
            } else if genre == "file" || genre == "submodule" {
                info(&format!("{nom} : sous-repo git, ses fichiers sont dans son propre repo."));
            } else {
                info(&format!("{nom} : {genre}, sous-module ou lien, pas d'affichage."));
            }
        }
    }
}

fn afficher_fichier(cfg: &mut Config, full: &str, chemin: &str, r: &str, d: &Value) {
    use base64::Engine;
    let (code, rep, _) = contenu(cfg, full, chemin, r);
    if code != 200 || !rep.is_object() {
        err(&format!("Lecture impossible, code {code} : {}", message_api(&rep)));
        return;
    }
    println!();
    println!("{}", c(&format!("  {chemin}   {}", taille(rep.get("size").and_then(Value::as_u64).unwrap_or(0))), "1"));
    let (code2, commits, _) = api(cfg, "GET", &format!("/repos/{full}/commits?path={}&sha={}&per_page=1", quoter(chemin), quoter(r)), None);
    if let Some(cm) = commits.as_array().and_then(|a| a.first()).filter(|_| code2 == 200).map(|c| &c["commit"]) {
        let message: String = texte(cm, "message").lines().next().unwrap_or("").chars().take(60).collect();
        let date: String = texte(&cm["author"], "date").chars().take(10).collect();
        info(&format!("Dernier commit sur ce fichier : {message}, {}, {date}", texte(&cm["author"], "name")));
    }
    let brut = (texte(&rep, "encoding") == "base64" && !texte(&rep, "content").is_empty())
        .then(|| base64::engine::general_purpose::STANDARD.decode(texte(&rep, "content").replace(['\n', '\r'], "")).ok())
        .flatten();
    match brut {
        None => info("Fichier trop gros pour l'affichage ici : plus de 1 Mo."),
        Some(b) if b.iter().take(8000).any(|&x| x == 0) => info("Fichier binaire, image ou archive : pas d'affichage ici."),
        Some(b) => {
            let lignes = crate::comparer::decouper(&String::from_utf8_lossy(&b));
            println!();
            for (i, l) in lignes.iter().take(80).enumerate() {
                println!("{}{l}", c(&format!("  {:>4} ", i + 1), "36"));
            }
            if lignes.len() > 80 {
                info(&format!("... {} ligne(s) de plus : o pour tout voir sur GitHub", lignes.len() - 80));
            }
        }
    }
    if ask_libre("o pour ouvrir sur GitHub, Enter pour revenir", "").to_lowercase() == "o" {
        let mut url = texte(&rep, "html_url");
        if url.is_empty() {
            url = texte(d, "html_url");
        }
        ouvrir_navigateur(&url);
    }
}

fn voir_commits(cfg: &mut Config, d: &Value) {
    let full = texte(d, "full_name");
    let r = choisir_branche_en_ligne(cfg, d);
    let (code, rep, _) = api(cfg, "GET", &format!("/repos/{full}/commits?per_page=25&sha={}", quoter(&r)), None);
    if code == 409 {
        info("Repo vide : aucun commit.");
        pause();
        return;
    }
    let Some(liste) = rep.as_array().filter(|_| code == 200) else {
        err(&format!("Lecture impossible, code {code} : {}", message_api(&rep)));
        pause();
        return;
    };
    loop {
        println!();
        println!("{}", c(&format!("  Derniers commits de {full}, branche {r}"), "1;36"));
        for (i, cm) in liste.iter().enumerate() {
            let infos = &cm["commit"]["author"];
            let mut auteur = cm.get("author").map(|a| texte(a, "login")).unwrap_or_default();
            if auteur.is_empty() {
                auteur = texte(infos, "name");
            }
            if auteur.is_empty() {
                auteur = "?".into();
            }
            let msg: String = texte(&cm["commit"], "message").lines().next().unwrap_or("").chars().take(48).collect();
            let sha: String = texte(cm, "sha").chars().take(7).collect();
            let date: String = texte(infos, "date").chars().take(10).collect();
            let a: String = auteur.chars().take(14).collect();
            println!("    {:>3}. {sha}  {date}  {a:<14} {msg}", i + 1);
        }
        let choix = ask_libre("Choix : # du commit pour voir ses fichiers, Enter pour revenir", "");
        if choix.is_empty() {
            return;
        }
        if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=liste.len()).contains(n)) {
            details_commit(cfg, &full, &texte(&liste[n - 1], "sha"));
        }
    }
}

fn details_commit(cfg: &mut Config, full: &str, sha: &str) {
    let (code, cm, _) = api(cfg, "GET", &format!("/repos/{full}/commits/{sha}"), None);
    if code != 200 || !cm.is_object() {
        err(&format!("Lecture impossible, code {code} : {}", message_api(&cm)));
        return;
    }
    let libelle = |s: &str| match s {
        "added" => "nouveau", "modified" | "changed" => "modif", "removed" => "retrait", "renamed" => "nouveau nom",
        "copied" => "copie", autre => autre,
    }.to_string();
    let infos = &cm["commit"]["author"];
    println!();
    let court: String = sha.chars().take(7).collect();
    println!("{}", c(&format!("  {court}  {}", texte(&cm["commit"], "message").lines().next().unwrap_or("")), "1"));
    let date: String = texte(infos, "date").chars().take(16).collect::<String>().replace('T', " ");
    info(&format!("{}, {date}", texte(infos, "name")));
    let fichiers = cm.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    info(&format!("Fichiers de ce commit : {}", fichiers.len()));
    for f in fichiers.iter().take(60) {
        let mut nom = texte(f, "filename");
        if !texte(f, "previous_filename").is_empty() {
            nom = format!("{} -> {nom}", texte(f, "previous_filename"));
        }
        println!("       {:<9} {nom}   (+{} -{})", libelle(&texte(f, "status")),
                 f.get("additions").and_then(Value::as_u64).unwrap_or(0), f.get("deletions").and_then(Value::as_u64).unwrap_or(0));
    }
    if fichiers.len() > 60 {
        println!("       ... et {} autre(s)", fichiers.len() - 60);
    }
    pause();
}

/// Supprime le repo de GitHub, depuis le terminal. La permission de suppression
/// manque souvent a la connexion : GitHub CLI la demande dans le navigateur, ou un
/// jeton sert pour cette suppression seulement. Retourne true si c'est fait.
fn supprimer_depot(cfg: &mut Config, d: &Value, locaux: &mut HashMap<String, PathBuf>) -> bool {
    let (full, nom) = (texte(d, "full_name"), texte(d, "name"));
    if d["permissions"]["admin"].as_bool() == Some(false) {
        err(&format!("Suppression impossible : il faut le role admin sur {full}."));
        info("Seul un admin du repo peut le supprimer : son proprietaire, ou un admin");
        info("de l'organisation. Voir les droits sur ce repo : choix 7.");
        return false;
    }
    warn("Suppression SANS RETOUR sur GitHub : code, issues, pull requests et historique.");
    let confirme = ask_libre(&format!("Taper {nom} pour confirmer, Enter pour annuler"), "").trim().to_lowercase();
    if confirme != nom.to_lowercase() && confirme != full.to_lowercase() {
        info("Abandon : le repo reste sur GitHub.");
        return false;
    }
    let fait_ = supprimer_sur_github(cfg, d);
    if fait_ == Suppression::Refus {
        return false;
    }
    session::ajouter_supprime(&full);
    if fait_ == Suppression::Absent {
        warn(&format!("{full} n'existe plus sur GitHub : deja supprime, par exemple dans le navigateur."));
    } else {
        ok(&format!("{full} : suppression OK sur GitHub."));
    }
    if let Some(local) = locaux.get(&full.to_lowercase()).cloned().filter(|l| l.is_dir()) {
        copie_apres_suppression(&local, &full, locaux);
    }
    true
}

#[derive(PartialEq, Eq)]
enum Suppression {
    Faite,
    Absent,
    Refus,
}

/// DELETE /repos/... ; sans la permission de supprimer, la demande a GitHub CLI,
/// puis propose un jeton pour cette fois ou la page du repo.
fn supprimer_sur_github(cfg: &mut Config, d: &Value) -> Suppression {
    let full = texte(d, "full_name");
    let chemin = format!("/repos/{full}");
    if !assurer_connexion(cfg, true) {
        return Suppression::Refus;
    }
    let (mut code, mut rep, _) = requete(cfg, "DELETE", &chemin, None, None);
    if code == 401 && rafraichir_session(cfg) {
        (code, rep, _) = requete(cfg, "DELETE", &chemin, None, None);
    }
    if code == 204 {
        return Suppression::Faite;
    }
    if code == 404 {
        return Suppression::Absent; // le repo vient d'etre lu : il a ete supprime entre-temps
    }
    if code != 403 {
        err(&format!("Suppression impossible, code {} : {}", if code == 0 { "aucun".into() } else { code.to_string() }, message_api(&rep)));
        return Suppression::Refus;
    }
    if crate::debug() {
        err(&format!("GitHub : {}", texte(&rep, "message")));
    }
    println!();
    warn("GitHub refuse : la connexion n'a pas la permission de supprimer des repos.");
    let methode = session::methode().unwrap_or_default();
    if methode == "gh" && chemin_gh().is_some() {
        info("GitHub CLI peut ajouter la permission delete_repo, dans le navigateur.");
        if ask_yn("La demander maintenant ?", true) {
            if gh_par_navigateur(&["auth", "refresh", "-h", &host(cfg, None), "-s", "delete_repo"]) == 0 {
                let t = token_silencieux(cfg, "gh");
                if !t.is_empty() {
                    session::set_token(&t);
                }
                if requete(cfg, "DELETE", &chemin, None, None).0 == 204 {
                    return Suppression::Faite;
                }
                warn("Toujours refuse : l'organisation interdit peut-etre la suppression des repos.");
            } else {
                warn("Permission pas ajoutee : autorisation annulee ou refusee dans le navigateur.");
            }
        }
    } else if methode == "gcm" {
        info("Git Credential Manager ne donne pas cette permission.");
    }
    println!();
    println!("    1. Saisir un jeton avec la permission de supprimer, pour cette fois seulement");
    println!("    2. Ouvrir la page du repo sur GitHub : Settings, tout en bas, Delete this repository");
    println!("    0. Annuler");
    match ask("Choix", "0").as_str() {
        "1" => {
            let h = host(cfg, None);
            info(&format!("Faire un jeton sur https://{h}/settings/tokens : jeton classique avec"));
            info("delete_repo, ou jeton fin pour ce repo avec Administration, Read and write.");
            info("Il sert pour cette suppression seulement, puis le programme l'oublie.");
            let (_, jeton) = demander_secret(&h, false, false);
            if !jeton.is_empty() {
                let (code, rep, _) = requete(cfg, "DELETE", &chemin, None, Some(&jeton));
                if code == 204 {
                    return Suppression::Faite;
                }
                err(&format!("Suppression impossible avec ce jeton, code {} : {}",
                             if code == 0 { "aucun".into() } else { code.to_string() }, message_api(&rep)));
            }
        }
        "2" => {
            let mut url = texte(d, "html_url");
            if url.is_empty() {
                url = format!("https://{}/{full}", host(cfg, None));
            }
            ouvrir_navigateur(&format!("{url}/settings"));
        }
        _ => {}
    }
    info("Abandon : le repo reste sur GitHub.");
    Suppression::Refus
}

/// Copie sur cette machine d'un repo supprime de GitHub : c'est maintenant la
/// derniere copie. Elle reste, sans lien vers le repo supprime, ou part aussi.
fn copie_apres_suppression(local: &Path, full: &str, locaux: &mut HashMap<String, PathBuf>) {
    println!();
    info(&format!("Copie sur cette machine : {}", local.display()));
    info("C'est maintenant la seule copie de ce projet.");
    println!("    1. La garder, sans lien avec le repo supprime : git remote remove");
    println!("    2. La supprimer aussi de cette machine");
    if ask("Choix", "1") == "2" && ask_yn(&format!("Supprimer {} ? Sans retour, c'est la derniere copie.", local.display()), false) {
        supprimer_dossier(local);
        if local.exists() {
            err("Suppression pas finie : fermer les programmes qui ouvrent ce dossier.");
        } else {
            ok(&format!("{} : suppression OK.", local.display()));
            locaux.remove(&full.to_lowercase());
            return;
        }
    }
    for (remote, url) in urls_remotes(local) {
        if cle_depot(&url).as_deref() == Some(full.to_lowercase().as_str()) {
            git(&["remote", "remove", &remote], Some(local));
        }
    }
    locaux.remove(&full.to_lowercase());
    ok("Copie gardee, sans lien avec GitHub : Lier avec GitHub la remet en ligne.");
}

/// Acces du compte connecte a ce repo, relus sur GitHub.
fn afficher_acces(cfg: &mut Config, d: &Value) {
    let niveaux = [("admin", "admin"), ("maintain", "maintenance"), ("push", "push"), ("triage", "tri des issues"), ("pull", "lecture")];
    let droits = &d["permissions"];
    let mut role = niveaux.iter().find(|(k, _)| droits[*k].as_bool() == Some(true)).map_or("aucun", |(_, l)| *l).to_string();
    let login = session::login();
    if let Some(l) = &login {
        let (code, rep, _) = requete(cfg, "GET", &format!("/repos/{}/collaborators/{l}/permission", texte(d, "full_name")), None, None);
        if code == 200 && !texte(&rep, "role_name").is_empty() {
            role = texte(&rep, "role_name");
        }
    }
    println!();
    info(&format!("Droits de {} sur {} : {role}", login.as_deref().unwrap_or("ce compte"), texte(d, "full_name")));
    for (k, libelle) in niveaux {
        println!("       {libelle:<16} {}", if droits[k].as_bool() == Some(true) { "oui" } else { "non" });
    }
}

/// Fichiers secrets (.env, cles privees...) presents dans la branche principale.
pub fn sensibles_sur_github(cfg: &mut Config, d: &Value) -> Vec<String> {
    let branche = d.get("default_branch").and_then(Value::as_str).filter(|b| !b.is_empty()).unwrap_or("main");
    let (code, rep, _) = api(cfg, "GET", &format!("/repos/{}/git/trees/{branche}?recursive=1", texte(d, "full_name")), None);
    if code != 200 {
        return vec![];
    }
    rep.get("tree").and_then(Value::as_array).map(|t| t.iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("blob"))
        .map(|e| texte(e, "path")).filter(|p| est_sensible(p)).collect()).unwrap_or_default()
}

pub fn changer_visibilite(cfg: &mut Config, d: &mut Value) {
    let full = texte(d, "full_name");
    let vers_prive = d.get("private").and_then(Value::as_bool) != Some(true);
    if !vers_prive {
        warn("Public : tout le code ET tout l'historique des commits deviennent visibles.");
        let sensibles = sensibles_sur_github(cfg, d);
        if !sensibles.is_empty() {
            err("Ce repo contient des fichiers secrets, qui deviendraient publics :");
            for f in sensibles.iter().take(20) {
                println!("       {f}");
            }
            info("Les retirer et changer les secrets qu'ils contiennent avant de publier.");
            if ask_libre("Taper PUBLIC pour confirmer", "") != "PUBLIC" {
                info("Abandon.");
                return;
            }
        } else if !ask_yn(&format!("Rendre {full} PUBLIC ?"), false) {
            info("Abandon.");
            return;
        }
    } else if !ask_yn(&format!("Rendre {full} NON PUBLIC ?"), true) {
        info("Abandon.");
        return;
    }
    let (code, rep, _) = api(cfg, "PATCH", &format!("/repos/{full}"), Some(&json!({"private": vers_prive})));
    if code == 200 {
        d["private"] = vers_prive.into();
        ok(&format!("{full} est maintenant {}.", if vers_prive { "non public" } else { "public" }));
    } else {
        err(&format!("Erreur, code {code} : {}", message_api(&rep)));
    }
}
