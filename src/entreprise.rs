//! Option 2 : importer un repo GitHub, puis l'envoyer dans un repo de l'entreprise.

use crate::affichage::{c, err, info, ljust, ok, sortie, titre, warn};
use crate::config::{maison, Config};
use crate::connexion::demander_secret;
use crate::en_ligne::sensibles_sur_github;
use crate::erreur::{Arret, R};
use crate::git::*;
use crate::github::{api, host, liste_repos, message_api, noter_ouverture, requete, texte};
use crate::local::{nettoyer_chemin, projets_de_la_machine};
use crate::outils::sortie_cmd;
use crate::projet::{committer, preparer, pousser, recuperer};
use crate::resultat::{executer, fait, Res};
use crate::saisie::{ask, ask_libre, ask_yn, pause};
use crate::session::{self, EN_MEMOIRE};
use crate::urls::{avec_https, cle_depot, est_ssh, infos_url, sans_guillemets, url_web, verifier_url};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Dossiers jamais copies vers l'entreprise (propres a chaque ordinateur)
const EXCLUS_COPIE: [&str; 8] = [".git", "__pycache__", ".venv", "venv", "node_modules", ".mypy_cache", ".pytest_cache", ".ruff_cache"];
const DUREE_PUBLIC: u64 = 120; // secondes pendant lesquelles le repo reste public apres le clone

fn maintenant() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Saisie qui designe un dossier de cette machine, pas une adresse en ligne.
fn ressemble_a_un_dossier(saisie: &str) -> bool {
    let s = sans_guillemets(saisie);
    ["/", "~", ".", "\\", "$", "%"].iter().any(|p| s.starts_with(p))
        || regex::Regex::new(r"^[A-Za-z]:[\\/]").unwrap().is_match(s)
}

enum Source {
    Dossier(PathBuf),
    Adresse(String),
}

/// compte/nom -> repo GitHub ; sinon URL https / SSH ou dossier local.
fn url_source(saisie: &str) -> R<Source> {
    let s = sans_guillemets(saisie);
    let local = nettoyer_chemin(s);
    if local.is_dir() {
        return Ok(Source::Dossier(local));
    }
    if regex::Regex::new(r"^[\w.-]+/[\w.-]+$").unwrap().is_match(s) {
        return Ok(Source::Adresse(format!("https://github.com/{}.git", s.strip_suffix(".git").unwrap_or(s))));
    }
    Ok(Source::Adresse(verifier_url(&avec_https(s))?))
}

fn p(chemin: &Path) -> Option<&Path> {
    Some(chemin)
}

/// Clone leger (dernier commit seulement) du repo a recuperer : sans identifiant
/// s'il est public, sinon avec le compte GitHub connecte.
fn recuperer_source(cfg: &Config, url: &str, dossier: &Path) -> R<PathBuf> {
    let cible = dossier.join("projet");
    let cible_t = cible.display().to_string();
    let args = ["clone", "--depth", "1", "--no-tags", "--quiet", url, &cible_t];
    info(&format!("Clone de {}...", if url_web(url).is_empty() { url.to_string() } else { url_web(url) }));
    let (code, _, e) = git_env(&args, None, &env_git(None, true));
    if code != 0 {
        if est_ssh(url) || !(erreur_auth(&e) || e.to_lowercase().contains("not found")) {
            return Err(Arret::new("erreur", format!("clone impossible : {}", expliquer(&e))));
        }
        let serveur = infos_url(url).0.unwrap_or_else(|| "github.com".into());
        let jeton = if !session::token().is_empty() && serveur == host(cfg, None) {
            info("Repo non public : clone avec le compte GitHub en session.");
            session::token()
        } else {
            warn("Ce repo demande une connexion : il est non public, ou son nom est faux.");
            println!("    1. Se connecter au compte perso pour ce clone seulement");
            println!("    0. Annuler");
            if ask("Choix", "0") != "1" {
                return Err(Arret::new("ignore", "abandon : aucun changement"));
            }
            demander_secret(&serveur, false, true).1
        };
        if cible.exists() {
            supprimer_dossier(&cible);
        }
        let (code, _, e) = git_env(&args, None, &env_git(Some((String::new(), jeton)), false));
        if code != 0 {
            return Err(Arret::new("erreur", format!("clone impossible : {}", expliquer(&e))));
        }
    }
    if !a_des_commits(&cible) {
        return Err(Arret::new("erreur", "le repo du clone est vide"));
    }
    let (_, dernier, _) = git(&["log", "-1", "--format=%h  %s"], p(&cible));
    ok(&format!("Clone OK : {}, dernier commit {dernier}", cle_depot(url).unwrap_or_else(|| url.to_string())));
    Ok(cible)
}

fn cle_rel(rel: &str) -> String {
    if cfg!(windows) { rel.to_lowercase() } else { rel.to_string() }
}

fn exclu(rel: &str) -> bool {
    rel.split('/').any(|m| EXCLUS_COPIE.contains(&m))
}

/// {chemin relatif: chemin absolu} des fichiers a copier. Un lien vers un dossier
/// n'est pas suivi.
fn lister_source(source: &Path) -> BTreeMap<String, PathBuf> {
    fn parcourir(d: &Path, source: &Path, fichiers: &mut BTreeMap<String, PathBuf>) {
        let Ok(lecture) = std::fs::read_dir(d) else { return };
        let mut entrees: Vec<_> = lecture.flatten().collect();
        entrees.sort_by_key(|e| e.file_name());
        for e in entrees {
            let chemin = e.path();
            let nom = e.file_name().to_string_lossy().to_string();
            let lien = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
            let dossier = chemin.is_dir();
            if dossier && !lien {
                if !EXCLUS_COPIE.contains(&nom.as_str()) {
                    parcourir(&chemin, source, fichiers);
                }
            } else if !dossier {
                let rel = chemin.strip_prefix(source).unwrap_or(&chemin).to_string_lossy().replace('\\', "/");
                if !exclu(&rel) {
                    fichiers.insert(rel, chemin);
                }
            }
        }
    }
    let mut fichiers = BTreeMap::new();
    parcourir(source, source, &mut fichiers);
    fichiers
}

/// Compare deux fichiers ; un lien symbolique est compare a sa cible ecrite,
/// jamais suivi (il pourrait pointer hors du projet).
fn meme_contenu(a: &Path, b: &Path) -> bool {
    let lien = |x: &Path| x.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
    if lien(a) || lien(b) {
        return lien(a) && lien(b) && std::fs::read_link(a).ok() == std::fs::read_link(b).ok();
    }
    b.is_file() && std::fs::read(a).ok() == std::fs::read(b).ok()
}

struct Plan {
    src: BTreeMap<String, PathBuf>,
    nouveaux: Vec<String>,
    modifies: Vec<String>,
    identiques: usize,
    en_trop: Vec<String>,
}

fn plan_copie(source: &Path, dest: &Path) -> Plan {
    let src = lister_source(source);
    let (mut nouveaux, mut modifies, mut identiques) = (vec![], vec![], 0);
    for (rel, chemin) in &src {
        let d = dest.join(rel);
        if d.symlink_metadata().is_err() {
            nouveaux.push(rel.clone());
        } else if meme_contenu(chemin, &d) {
            identiques += 1;
        } else {
            modifies.push(rel.clone());
        }
    }
    // Fichiers suivis par git dans la destination mais absents du projet
    let (_, out, _) = git(&["ls-files", "-z"], p(dest));
    let cles: HashSet<String> = src.keys().map(|r| cle_rel(r)).collect();
    let en_trop = out.split('\0').filter(|f| !f.is_empty() && !cles.contains(&cle_rel(f)) && !exclu(f)).map(str::to_string).collect();
    Plan { src, nouveaux, modifies, identiques, en_trop }
}

fn dans_dossier(chemin: &Path, racine: &Path) -> bool {
    let reel = std::fs::canonicalize(chemin).unwrap_or(chemin.to_path_buf());
    let base = std::fs::canonicalize(racine).unwrap_or(racine.to_path_buf());
    reel.starts_with(&base)
}

/// Copie sans jamais ecrire hors du repo (lien symbolique piege).
fn copier_fichier(src: &Path, dst: &Path, racine: &Path) -> std::io::Result<()> {
    let parent = dst.parent().unwrap_or(racine);
    std::fs::create_dir_all(parent)?;
    if !dans_dossier(parent, racine) {
        return Err(std::io::Error::other("chemin hors du repo, par un lien symbolique"));
    }
    if let Ok(m) = dst.symlink_metadata() {
        if m.file_type().is_symlink() {
            std::fs::remove_file(dst)?;
        } else if m.permissions().readonly() {
            let mut perm = m.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            std::fs::set_permissions(dst, perm)?;
        }
    }
    if src.symlink_metadata()?.file_type().is_symlink() {
        let cible = std::fs::read_link(src)?;
        #[cfg(unix)]
        return std::os::unix::fs::symlink(cible, dst);
        #[cfg(windows)]
        return std::os::windows::fs::symlink_file(cible, dst);
    }
    std::fs::copy(src, dst).map(|_| ())
}

fn supprimer_avec_dossiers_vides(fichier: &Path, limite: &Path) -> std::io::Result<()> {
    let parent = fichier.parent().unwrap_or(limite);
    if !dans_dossier(parent, limite) {
        return Err(std::io::Error::other("chemin hors du repo, par un lien symbolique"));
    }
    std::fs::remove_file(fichier)?;
    let mut d = parent.to_path_buf();
    while d.starts_with(limite) && d != limite {
        if std::fs::remove_dir(&d).is_err() {
            break;
        }
        d = d.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    Ok(())
}

/// Copie le projet importe dans le repo de l'entreprise, hors .git. Avec
/// supprimer, le repo devient identique au projet ; sans, rien n'est supprime.
/// Retourne false si l'utilisateur annule.
fn remplacer_contenu(source: &Path, dest: &Path, supprimer: bool) -> bool {
    let plan = plan_copie(source, dest);
    let a_copier: Vec<String> = plan.nouveaux.iter().chain(&plan.modifies).cloned().collect();
    let a_supprimer: Vec<String> = if supprimer { plan.en_trop.clone() } else { vec![] };
    println!();
    info("Comparaison avec le repo de l'entreprise :");
    println!("       {:>5} identique(s)", plan.identiques);
    println!("       {:>5} avec changements", plan.modifies.len());
    println!("       {:>5} nouveau(x)", plan.nouveaux.len());
    if supprimer {
        println!("       {:>5} en moins, absents du projet", a_supprimer.len());
    } else if !plan.en_trop.is_empty() {
        println!("       {:>5} restent, absents du projet", plan.en_trop.len());
    }
    if a_copier.is_empty() && a_supprimer.is_empty() {
        info("Le repo de l'entreprise contient ce projet tel quel.");
        return true;
    }
    println!();
    for (libelle, liste) in [("modif   ", &plan.modifies), ("nouveau ", &plan.nouveaux), ("retrait ", &a_supprimer)] {
        for rel in liste.iter().take(20) {
            println!("       {libelle}  {rel}");
        }
        if liste.len() > 20 {
            println!("       ... et {} autre(s)", liste.len() - 20);
        }
    }
    println!();
    if !ask_yn(&format!("Appliquer au repo de l'entreprise : {} copie(s), {} suppression(s) ?", a_copier.len(), a_supprimer.len()), true) {
        return false;
    }
    let mut echecs = vec![];
    for rel in &a_supprimer {
        // d'abord : un ancien lien symbolique disparait avant la copie
        if let Err(e) = supprimer_avec_dossiers_vides(&dest.join(rel), dest) {
            echecs.push(format!("{rel} : {e}"));
        }
    }
    for rel in &a_copier {
        if let Err(e) = copier_fichier(&plan.src[rel], &dest.join(rel), dest) {
            echecs.push(format!("{rel} : {e}"));
        }
    }
    if !echecs.is_empty() {
        warn(&format!("{} fichier(s) sans copie ni suppression possible :", echecs.len()));
        for x in echecs.iter().take(10) {
            println!("       {x}");
        }
    }
    ok(&format!("Nouveau contenu : {} copie(s), {} suppression(s){}", a_copier.len(), a_supprimer.len(),
                if echecs.is_empty() { ".".into() } else { format!(", {} erreur(s).", echecs.len()) }));
    true
}

/// Aligne la destination sur le serveur avant la copie (fast-forward).
fn mettre_a_jour_destination(cfg: &mut Config, racine: &Path, remote: &str, branche: &str) -> R<()> {
    let modifs = fichiers_modifies(racine);
    if !modifs.is_empty() {
        warn(&format!("La destination a {} changement(s), commit manquant :", modifs.len()));
        for l in modifs.iter().take(10) {
            println!("       {} {}", ljust(&statut_porcelaine(l), 9), chemin_porcelaine(l));
        }
        info("Elles partiront avec ce commit.");
        if !ask_yn("Continuer ?", true) {
            return Err(Arret::new("ignore", "abandon"));
        }
    }
    info("Pull de la destination depuis le serveur...");
    recuperer(cfg, racine, remote, false)?;
    if !modifs.is_empty() || !ref_existe(racine, &format!("refs/remotes/{remote}/{branche}")) {
        return Ok(());
    }
    let ar = avance_retard(racine, remote, branche);
    if !a_des_commits(racine) || ar.is_some_and(|(avance, retard)| retard > 0 && avance == 0) {
        let (code, _, e) = git(&["merge", "--ff-only", &format!("{remote}/{branche}")], p(racine));
        if code != 0 {
            return Err(Arret::new("erreur", format!("pull de la destination impossible : {}", expliquer(&e))));
        }
        ok("Destination pareille au serveur.");
    }
    Ok(())
}

/// Forks, etoiles, vues et clones comptes par GitHub, sans aucune question.
fn compteurs(cfg: &Config, full: &str) -> BTreeMap<&'static str, i64> {
    let mut etat = BTreeMap::new();
    let (code, rep, _) = requete(cfg, "GET", &format!("/repos/{full}"), None, None);
    if code == 200 {
        etat.insert("forks", rep["forks_count"].as_i64().unwrap_or(0));
        etat.insert("etoiles", rep["stargazers_count"].as_i64().unwrap_or(0));
    }
    for (cle, chemin) in [("vues", "views"), ("clones", "clones")] {
        let (code, rep, _) = requete(cfg, "GET", &format!("/repos/{full}/traffic/{chemin}"), None, None);
        if code == 200 && rep.is_object() {
            etat.insert(cle, rep["count"].as_i64().unwrap_or(0));
        }
    }
    etat
}

fn horodatage(texte: &str) -> f64 {
    chrono::NaiveDateTime::parse_from_str(texte, "%Y-%m-%dT%H:%M:%SZ").map(|d| d.and_utc().timestamp() as f64).unwrap_or(0.0)
}

/// Ce qui s'est passe pendant l'ouverture publique du repo.
fn rapport_exposition(cfg: &Config, full: &str, avant: &BTreeMap<&str, i64>, debut: f64, fin: f64) {
    let apres = compteurs(cfg, full);
    let duree = (fin - debut) as i64;
    println!();
    info(&format!("Ouverture publique de {full} : {} min {:02} s.", duree / 60, duree % 60));
    let mut vu = false;
    let (code, forks, _) = requete(cfg, "GET", &format!("/repos/{full}/forks?sort=newest&per_page=30"), None, None);
    let nouveaux: Vec<String> = if code == 200 {
        forks.as_array().map(|a| a.iter().filter(|f| horodatage(&texte(f, "created_at")) >= debut - 5.0)
            .map(|f| texte(f, "full_name")).collect()).unwrap_or_default()
    } else { vec![] };
    if !nouveaux.is_empty() {
        vu = true;
        err("Fork fait pendant l'ouverture : il reste public, avec le repo non public aussi.");
        for n in &nouveaux {
            println!("       {n}");
        }
    } else {
        println!("       forks : 0");
    }
    for (cle, libelle, propres) in [("etoiles", "stars en plus", 0), ("vues", "vues selon GitHub", 0), ("clones", "autres clones selon GitHub", 1)] {
        if let (Some(a), Some(b)) = (avant.get(cle), apres.get(cle)) {
            let ecart = (b - a - propres).max(0); // le clone de gitmanager est retire
            println!("       {libelle} : {ecart}");
            vu = vu || ecart > 0;
        }
    }
    if !apres.contains_key("vues") {
        info("Vues et clones : pas de chiffres avec cette connexion.");
    }
    if vu {
        warn("Visite ou copie du repo pendant l'ouverture.");
    } else {
        ok("Aucune visite pour l'instant.");
    }
    info("GitHub compte les vues et les clones avec plusieurs heures de retard. Tout");
    info(&format!("reste visible sur https://{}/{full}/graphs/traffic", host(cfg, None)));
}

fn rendre_prive(cfg: &Config, full: &str) -> bool {
    for _ in 0..3 {
        if requete(cfg, "PATCH", &format!("/repos/{full}"), Some(&json!({"private": true})), None).0 == 200 {
            noter_ouverture(cfg, None);
            ok(&format!("{full} est de nouveau non public."));
            return true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    err(&format!("{full} EST TOUJOURS PUBLIC : le rendre non public sur https://{}/{full}/settings", host(cfg, None)));
    false
}

/// Compte a rebours ; Entree l'arrete, Ctrl+C aussi. Retourne (Entree pressee,
/// fil qui attend encore une Entree).
fn compte_a_rebours(secondes: u64, full: &str) -> (bool, std::sync::mpsc::Receiver<()>) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ligne = String::new();
        let _ = std::io::stdin().read_line(&mut ligne);
        let _ = tx.send(());
    });
    let fin = Instant::now() + Duration::from_secs(secondes);
    let mut presse = false;
    loop {
        let reste = fin.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as u64;
        if reste == 0 || crate::interrompu() {
            break;
        }
        print!("\r  {full} public encore {}:{:02}, Enter pour le rendre non public tout de suite ", reste / 60, reste % 60);
        sortie();
        if rx.recv_timeout(Duration::from_secs(1)).is_ok() {
            presse = true;
            break;
        }
    }
    println!();
    (presse, rx)
}

/// Repo non public : public le temps du clone, puis non public apres un compte a
/// rebours. Sans accord, il est recupere avec le compte connecte.
fn recuperer_avec_ouverture(cfg: &mut Config, d: &Value, dossier: &Path) -> R<PathBuf> {
    let full = texte(d, "full_name");
    println!();
    warn(&format!("{full} est non public."));
    info("Pour le cloner sans identifiants, il peut devenir public le temps du clone,");
    let duree = if DUREE_PUBLIC % 60 == 0 { format!("{} minutes", DUREE_PUBLIC / 60) } else { format!("{DUREE_PUBLIC} secondes") };
    info(&format!("puis redevenir non public au bout de {duree}. Pendant ce temps, tout le"));
    info("monde peut le voir et le copier, historique compris.");
    let sensibles = sensibles_sur_github(cfg, d);
    if !sensibles.is_empty() {
        err("Ce repo contient des fichiers secrets, qui deviendraient publics :");
        for chemin in sensibles.iter().take(20) {
            println!("       {chemin}");
        }
    }
    if !ask_yn("Rendre le repo public le temps du clone ?", false) {
        info("Le repo reste non public : clone avec le compte GitHub en session.");
        return recuperer_source(cfg, &texte(d, "clone_url"), dossier);
    }
    let avant = compteurs(cfg, &full);
    noter_ouverture(cfg, Some(&full)); // une sortie brutale le rend non public
    let (code, rep, _) = api(cfg, "PATCH", &format!("/repos/{full}"), Some(&json!({"private": false})));
    if code != 200 {
        noter_ouverture(cfg, None);
        return Err(Arret::new("erreur", format!("le repo n'a pas pu devenir public, code {code} : {}", message_api(&rep))));
    }
    let debut = maintenant();
    warn(&format!("{full} est public."));
    let source = recuperer_source(cfg, &texte(d, "clone_url"), dossier);
    let attente = source.is_ok().then(|| compte_a_rebours(DUREE_PUBLIC, &full));
    let fin = maintenant();
    rendre_prive(cfg, &full); // meme apres une erreur ou un Ctrl+C
    crate::prendre_interruption();
    if let Some((false, rx)) = attente {
        info("Enter pour continuer.");
        let _ = rx.recv();
    }
    rapport_exposition(cfg, &full, &avant, debut, fin);
    source
}

/// Bureau de l'utilisateur, selon le systeme ; le dossier perso a defaut.
fn dossier_bureau() -> PathBuf {
    let m = maison();
    if cfg!(target_os = "linux") && crate::outils::ou_est("xdg-user-dir").is_some() {
        let (code, out) = sortie_cmd(&["xdg-user-dir", "DESKTOP"]);
        let d = PathBuf::from(out.trim());
        if code == 0 && !out.trim().is_empty() && d.is_dir() && d != m {
            return d;
        }
    }
    for d in [m.join("Desktop"), m.join("OneDrive").join("Desktop"), m.join("OneDrive").join("Bureau"), m.join("Bureau")] {
        if d.is_dir() {
            return d;
        }
    }
    m
}

/// Copie locale du repo de l'entreprise sur le Bureau : reutilisee si elle existe
/// deja, sinon clonee. Retourne son dossier.
fn copie_sur_bureau(url: &str, nom: &str) -> R<PathBuf> {
    let bureau = dossier_bureau();
    let mut dest = bureau.join(nom);
    let mut n = 2;
    let cle = |u: &str| cle_depot(u).unwrap_or_else(|| u.to_string());
    while dest.exists() {
        if dest.join(".git").is_dir() && cle(&adresse_principale(&dest)) == cle(url) {
            info(&format!("Copie locale sur le Bureau : {}", dest.display()));
            return Ok(dest);
        }
        dest = bureau.join(format!("{nom}-{n}"));
        n += 1;
    }
    info(&format!("Copie locale du repo de l'entreprise sur le Bureau : {}", dest.display()));
    if git_direct(&["clone", url, &dest.display().to_string()], None) != 0 {
        return Err(Arret::new("erreur", "clone du repo de l'entreprise impossible : voir l'adresse et les droits"));
    }
    Ok(dest)
}

/// Repo non public : connexion au compte GitHub perso le temps de l'import, puis
/// oubli de cette connexion si elle etait gardee en memoire.
fn import_prive(cfg: &mut Config, url: &str, dossier: &Path) -> R<PathBuf> {
    let deja = !session::compte_de("perso").token.is_empty();
    session::activer_profil("perso");
    let res = (|| {
        if !crate::menus::connexion_obligatoire(cfg) {
            return Err(Arret::new("ignore", "abandon de l'import"));
        }
        let (_, o, r) = infos_url(url);
        let (code, d, _) = requete(cfg, "GET", &format!("/repos/{}/{}", o.unwrap_or_default(), r.unwrap_or_default()), None, None);
        if code == 200 && d["private"].as_bool() == Some(true) && d["permissions"]["admin"].as_bool() == Some(true) {
            return recuperer_avec_ouverture(cfg, &d, dossier);
        }
        recuperer_source(cfg, url, dossier)
    })();
    if !deja && session::methode().is_some_and(|m| EN_MEMOIRE.contains(&m.as_str())) {
        session::vider();
    }
    session::activer_profil("entreprise");
    res
}

/// Copie de ce repo sur cette machine, trouvee par son adresse en ligne, ou None.
fn copie_locale_de(cfg: &Config, url: &str) -> Option<PathBuf> {
    let cherchee = cle_depot(url)?;
    let projets = projets_de_la_machine(cfg);
    projets.iter().zip(en_parallele(&projets, |p| urls_remotes(p)))
        .find(|(_, urls)| urls.iter().any(|(_, u)| cle_depot(u).as_deref() == Some(cherchee.as_str())))
        .map(|(p, _)| p.clone())
}

/// Import depuis un dossier de cette machine, sans connexion : le dernier commit
/// d'un repo git, ou les fichiers d'un dossier sans git.
fn importer_local(chemin: &Path, dossier: &Path, nom: Option<&str>) -> R<(PathBuf, String)> {
    let nom = nom.map(str::to_string).unwrap_or_else(|| chemin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    if racine_repo(chemin).is_none() {
        ok(&format!("Import OK : {nom}, fichiers du dossier {}", chemin.display()));
        return Ok((chemin.to_path_buf(), nom));
    }
    let modifs = fichiers_modifies(chemin);
    if !modifs.is_empty() {
        warn(&format!("{} fichier(s), commit manquant, dans cette copie : ils ne font pas partie de l'import.", modifs.len()));
    }
    let cible = dossier.join("projet");
    let (code, _, e) = git(&["clone", "--quiet", "--no-hardlinks", &chemin.display().to_string(), &cible.display().to_string()], None);
    if code != 0 {
        return Err(Arret::new("erreur", format!("import impossible : {}", expliquer(&e))));
    }
    let (_, dernier, _) = git(&["log", "-1", "--format=%h  %s"], p(&cible));
    ok(&format!("Import OK : {nom}, dernier commit {dernier}, depuis cette machine"));
    Ok((cible, nom))
}

/// Import d'un repo GitHub par son URL, sans identifiants s'il est public.
/// Retourne (dossier du projet importe, nom du repo), ou None pour revenir.
fn importer_repo(cfg: &mut Config, dossier: &Path) -> R<Option<(PathBuf, String)>> {
    let mut saisie = String::new();
    let mut trouve = false;
    for essai in 1..=5 {
        saisie = ask_libre("Repo pour l'import : URL GitHub, compte/nom ou dossier sur cette machine, Enter pour revenir", "");
        if saisie.is_empty() {
            return Ok(None);
        }
        let chemin = nettoyer_chemin(&saisie);
        if !(ressemble_a_un_dossier(&saisie) && !chemin.is_dir()) {
            trouve = true;
            break;
        }
        err(&format!("Dossier introuvable : {}{}", chemin.display(), if essai < 5 { format!(", essai {essai} sur 5.") } else { ".".into() }));
    }
    if !trouve {
        info("5 essais sans dossier : retour.");
        return Ok(None);
    }
    let url = match url_source(&saisie)? {
        Source::Dossier(d) => return importer_local(&d, dossier, None).map(Some), // aucune connexion
        Source::Adresse(u) => u,
    };
    let nom = cle_depot(&url).unwrap_or_else(|| {
        let base = url.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string();
        base.strip_suffix(".git").unwrap_or(&base).to_string()
    });
    if let Some(copie) = copie_locale_de(cfg, &url) {
        let (_, dernier, _) = git(&["log", "-1", "--format=%h, %cd : %s", "--date=format:%Y-%m-%d %H:%M"], p(&copie));
        println!();
        info(&format!("Copie de {nom} sur cette machine : {}", copie.display()));
        info(&format!("Dernier commit de cette copie : {}", if dernier.is_empty() { "aucun" } else { &dernier }));
        println!();
        println!("    1. Utiliser cette copie, sans connexion au compte perso");
        println!("    2. Importer depuis GitHub");
        println!("    0. Retour");
        match ask("Choix", "1").as_str() {
            "1" => return importer_local(&copie, dossier, Some(&nom)).map(Some),
            "2" => {}
            _ => return Ok(None),
        }
    }
    let mut cible = dossier.join("projet");
    info(&format!("Import de {}...", if url_web(&url).is_empty() { url.clone() } else { url_web(&url) }));
    let (code, _, e) = git_env(&["clone", "--depth", "1", "--no-tags", "--quiet", &url, &cible.display().to_string()], None,
                               &env_git(None, true));
    if code != 0 {
        if est_ssh(&url) || !(erreur_auth(&e) || e.to_lowercase().contains("not found")) {
            return Err(Arret::new("erreur", format!("import impossible : {}", expliquer(&e))));
        }
        warn("Ce repo demande une connexion : il est non public, ou son nom est faux.");
        if !ask_yn("Se connecter au compte GitHub perso pour cet import ?", true) {
            return Err(Arret::new("ignore", "abandon de l'import"));
        }
        if cible.exists() {
            supprimer_dossier(&cible);
        }
        cible = import_prive(cfg, &url, dossier)?;
    }
    let (_, dernier, _) = git(&["log", "-1", "--format=%h  %s"], p(&cible));
    ok(&format!("Import OK : {nom}, dernier commit {dernier}"));
    Ok(Some((cible, nom)))
}

/// Envoi du projet importe dans un repo GitHub de l'entreprise, a partir d'une
/// copie locale sur le Bureau ou deja sur la machine.
fn envoyer_vers_entreprise(cfg: &mut Config, source: &Path) -> R<Res> {
    titre("Envoyer dans un repo GitHub de l'entreprise");
    info("Chargement des repos du compte GitHub de l'entreprise...");
    let repos = liste_repos(cfg, "pushed").unwrap_or_default();
    println!();
    if repos.is_empty() {
        info("Aucun repo sur ce compte.");
    } else {
        println!("{}", c(&format!("    {:>3}  {:<44} Droits", "#", "Repo"), "1"));
        for (i, d) in repos.iter().enumerate() {
            let droits = if d["permissions"]["push"].as_bool() == Some(true) { "push" } else { "lecture" };
            let nom: String = texte(d, "full_name").chars().take(44).collect();
            println!("    {:>3}. {nom:<44} {droits}", i + 1);
        }
    }
    println!();
    println!("      U. Saisir l'URL du repo de l'entreprise");
    let choix = ask_libre("Choix : # du repo ou U, Enter pour revenir", "").to_lowercase();
    let (url, nom) = if let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=repos.len()).contains(n)) {
        (texte(&repos[n - 1], "clone_url"), texte(&repos[n - 1], "name"))
    } else if choix == "u" {
        let saisie = ask_libre("URL du repo de l'entreprise, https://... ou git@..., Enter pour revenir", "");
        if saisie.is_empty() {
            return Ok(Res::Retour);
        }
        let url = verifier_url(&saisie)?;
        let nom = infos_url(&url).2.unwrap_or_else(|| {
            let base = url.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string();
            base.strip_suffix(".git").unwrap_or(&base).to_string()
        });
        (url, nom)
    } else if !choix.is_empty() {
        warn("Choix invalide.");
        return fait("ignore", "abandon");
    } else {
        return Ok(Res::Retour);
    };
    println!();
    println!("    1. Ajouter au repo, sans rien enlever");
    println!("    2. Remplacer le repo par le projet");
    println!("    0. Retour");
    let mode = ask("Choix", "0");
    if mode != "1" && mode != "2" {
        return Ok(Res::Retour);
    }
    let copie = match copie_locale_de(cfg, &url) {
        Some(e) if ask_yn(&format!("Copie de ce repo sur cette machine : {}. L'utiliser pour l'envoi ?", e.display()), true) => e,
        _ => copie_sur_bureau(&url, &nom)?,
    };
    let (branche, remote) = preparer(cfg, &copie, false, true)?;
    mettre_a_jour_destination(cfg, &copie, &remote, &branche)?;
    if !remplacer_contenu(source, &copie, mode == "2") {
        return fait("ignore", "abandon : aucun changement dans le repo de l'entreprise");
    }
    println!();
    let fait_ = committer(cfg, &copie, None, false, false)?;
    let res = pousser(cfg, &copie, &remote, &branche, false, fait_ > 0);
    info(&format!("La copie locale reste ici : {}", copie.display()));
    res
}

pub fn act_importer(cfg: &mut Config) -> R<Res> {
    titre("Importer un repo GitHub");
    let temp = tempfile::Builder::new().prefix("gitmanager-").tempdir()
        .map_err(|_| Arret::new("erreur", "dossier temporaire impossible"))?.keep();
    let res = (|| -> R<Res> {
        let Some((source, nom)) = importer_repo(cfg, &temp)? else { return Ok(Res::Rien) };
        loop {
            titre(&format!("Projet : {nom}"));
            println!("    1. Envoyer dans un repo GitHub de l'entreprise");
            println!("    0. Retour");
            if ask("Choix", "0") != "1" {
                return Ok(Res::Rien);
            }
            if executer(|| envoyer_vers_entreprise(cfg, &source)) != Some(Res::Retour) {
                pause();
            }
        }
    })();
    supprimer_dossier(&temp); // aucune copie du projet importe ne reste hors du Bureau
    res
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn dossier_ou_adresse() {
        assert!(ressemble_a_un_dossier("/tmp/x") && ressemble_a_un_dossier("~/x") && ressemble_a_un_dossier("C:\\x"));
        assert!(!ressemble_a_un_dossier("saleh/demo") && !ressemble_a_un_dossier("https://github.com/a/b"));
        assert!(matches!(url_source("github.com/saleh/demo"), Ok(Source::Adresse(u)) if u == "https://github.com/saleh/demo"));
        assert!(url_source("http://github.com/a/b").is_err());
    }
}
