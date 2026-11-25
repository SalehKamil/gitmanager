//! Connexion GitHub : par le navigateur (GitHub CLI ou Git Credential Manager),
//! par jeton, par code. Temps de garde du compte, fin de session, suppression.

use crate::affichage::{c, err, info, ok, warn};
use crate::config::{maison, vrai, Config};
use crate::erreur::R;
use crate::git::{git, run, Env};
use crate::github::{formulaire, host, liste_repos, message_api, requete, entete};
use crate::outils::{chemin_gh, helper_gcm, installer_outil, methode_dispo, ouvrir_navigateur};
use crate::resultat::Res;
use crate::saisie::{ask, ask_libre, ask_yn, lire_secret, menu_numerote};
use crate::session::{self, cle, nom_compte, nom_methode, EN_MEMOIRE, PROFILS};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn maintenant() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn login_de(user: &Value) -> String {
    user.get("login").and_then(Value::as_str).unwrap_or("").to_string()
}

/// Moyen, gh ou gcm, dont les cles de connexion sont enregistrees sur ce poste
/// pour le compte actif ou le profil donne, sinon None.
pub fn cles_enregistrees(cfg: &Config, profil: Option<&str>) -> Option<String> {
    let profil: &str = match profil { Some(p) => p, None => session::profil() };
    let mut m = cfg.texte(&cle("cles", Some(profil)));
    if m.is_empty() && cfg.vrai(&cle("enregistrer_connexion", Some(profil))) {
        m = cfg.texte(&cle("methode", Some(profil)));
    }
    if m.is_empty() {
        m = session::compte_de(profil).methode.unwrap_or_default();
    }
    (m == "gh" || m == "gcm").then_some(m)
}

fn cmd_gcm(action: &str) -> Vec<String> {
    vec!["git".into(), "-c".into(), "credential.helper=".into(), "-c".into(),
         format!("credential.helper={}", helper_gcm().unwrap_or_default()), "credential".into(), action.into()]
}

fn run_gcm(action: &str, entree: &str, env: Option<&Env>) -> (i32, String, String) {
    let cmd = cmd_gcm(action);
    let refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    run(&refs, Some(entree), env)
}

/// Demande les identifiants a GCM. Interactif : ouvre le navigateur si besoin.
pub fn credential_gcm(cfg: &Config, interactif: bool) -> String {
    let entree = format!("protocol=https\nhost={}\n\n", host(cfg, None));
    let env: Env = if interactif { vec![] } else {
        vec![("GCM_INTERACTIVE".into(), Some("never".into())), ("GIT_TERMINAL_PROMPT".into(), Some("0".into()))]
    };
    let (code, out, _) = run_gcm("fill", &entree, Some(&env));
    if code != 0 {
        return String::new();
    }
    let tok = out.lines().find_map(|l| l.strip_prefix("password=")).unwrap_or("").to_string();
    if !tok.is_empty() && interactif {
        run_gcm("approve", &out, Some(&env)); // memorise pour les push
    }
    tok
}

pub fn oublier_gcm(cfg: &Config) {
    run_gcm("reject", &format!("protocol=https\nhost={}\n\n", host(cfg, None)), None);
}

/// Fait passer git push/pull par GCM pour ce serveur (comme gh auth setup-git).
pub fn configurer_helper_gcm(serveur: &str) {
    let cle_git = format!("credential.https://{serveur}.helper");
    git(&["config", "--global", "--unset-all", &cle_git], None);
    git(&["config", "--global", "--add", &cle_git, ""], None);
    git(&["config", "--global", "--add", &cle_git, &helper_gcm().unwrap_or_default()], None);
}

pub fn token_silencieux(cfg: &Config, methode: &str) -> String {
    match methode {
        "gh" => {
            let Some(gh) = chemin_gh() else { return String::new() };
            let (code, out, _) = run(&[&gh.display().to_string(), "auth", "token", "-h", &host(cfg, None)], None, None);
            if code == 0 { out.trim().to_string() } else { String::new() }
        }
        "gcm" => credential_gcm(cfg, false),
        _ => String::new(),
    }
}

pub fn identite_par_defaut(cfg: &Config, user: &Value, cle_git: &str) -> String {
    let champ = |k: &str| user.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if cle_git == "user.name" {
        let nom = champ("name");
        return if nom.is_empty() { champ("login") } else { nom };
    }
    let id = user.get("id").and_then(Value::as_i64);
    if host(cfg, None) == "github.com" && id.is_some() && !champ("login").is_empty() {
        // Adresse privee fournie par GitHub : pas de blocage "email prive" au push
        return format!("{}+{}@users.noreply.github.com", id.unwrap(), champ("login"));
    }
    champ("email")
}

/// Ecrit user.name ou user.email dans la configuration globale de Git et note
/// que gitmanager l'a ecrit pour ce compte : la suppression du compte l'efface.
pub fn ecrire_identite(cfg: &mut Config, cle_git: &str, valeur: &str) {
    git(&["config", "--global", cle_git, valeur], None);
    let mut notes = cfg.get("identite_ecrite").cloned().filter(|v| v.is_object()).unwrap_or(json!({}));
    notes[cle_git] = json!({"valeur": valeur, "profil": session::profil()});
    cfg.set("identite_ecrite", notes);
    cfg.sauver();
}

/// Cles user.name / user.email ecrites par gitmanager pour ce compte et
/// toujours en place avec la meme valeur.
pub fn identite_ecrite(cfg: &Config, profil: Option<&str>) -> Vec<String> {
    let profil: &str = match profil { Some(p) => p, None => session::profil() };
    let mut restes = vec![];
    if let Some(notes) = cfg.get("identite_ecrite").and_then(Value::as_object) {
        for (cle_git, note) in notes {
            if note.get("profil").and_then(Value::as_str) == Some(profil) {
                let (_, val, _) = git(&["config", "--global", "--get", cle_git], None);
                if Some(val.trim()) == note.get("valeur").and_then(Value::as_str) {
                    restes.push(cle_git.clone());
                }
            }
        }
    }
    restes
}

/// Renseigne user.name / user.email globaux s'ils manquent.
pub fn configurer_identite(cfg: &mut Config, user: &Value) {
    for cle_git in ["user.name", "user.email"] {
        let (_, val, _) = git(&["config", "--global", "--get", cle_git], None);
        if !val.trim().is_empty() {
            continue;
        }
        let mut valeur = identite_par_defaut(cfg, user, cle_git);
        if valeur.is_empty() {
            valeur = ask("Email pour les commits", "");
        }
        ecrire_identite(cfg, cle_git, &valeur);
    }
    let (_, branche, _) = git(&["config", "--global", "--get", "init.defaultBranch"], None);
    if branche.trim().is_empty() {
        git(&["config", "--global", "init.defaultBranch", "main"], None);
    }
}

pub fn demarrer_session(cfg: &mut Config, methode: &str, tok: &str, user: Value) {
    session::ouvrir(tok, user.clone(), methode);
    cfg.set(&cle("methode", None), methode);
    cfg.set(&cle("login", None), login_de(&user));
    if methode == "gh" || methode == "gcm" {
        cfg.set(&cle("cles", None), methode);
    }
    cfg.sauver();
    configurer_identite(cfg, &user);
}

/// Jeton (ou mot de passe) saisi sans affichage. Il reste en memoire : rien
/// n'est ecrit sur le disque. Retourne (utilisateur, secret).
pub fn demander_secret(serveur: &str, avec_utilisateur: bool, aide: bool) -> (String, String) {
    if aide {
        info(&format!("Faire un jeton sur https://{serveur}/settings/tokens : jeton fin, pour les"));
        info("seuls repos utiles, valable peu de temps, jamais dans un fichier.");
        info("Le jeton reste dans la session, jamais sur le disque.");
    }
    let utilisateur = if avec_utilisateur { ask_libre("Nom d'utilisateur, Enter pour un jeton GitHub", "") } else { String::new() };
    (utilisateur, lire_secret("  Jeton ou mot de passe, sans affichage : "))
}

/// Connexion par jeton d'acces, garde en memoire le temps de la session.
pub fn connexion_jeton(cfg: &mut Config) -> bool {
    let (_, tok) = demander_secret(&host(cfg, None), false, true);
    if tok.is_empty() {
        warn("Aucun jeton saisi.");
        return false;
    }
    ouvrir_session_memoire(cfg, "jeton", &tok)
}

// Connexion par code, gardee en memoire : GitHub App de gitmanager. Le Client ID
// n'est pas un secret. Les jetons obtenus expirent au bout de 8 heures.
const CLIENT_ID_GITHUB: &str = "";
const APP_GITHUB: &str = ""; // nom de la GitHub App dans son adresse : github.com/apps/<nom>

pub fn client_id(cfg: &Config) -> String {
    for v in [std::env::var("GITMANAGER_CLIENT_ID").unwrap_or_default(), cfg.texte("client_id"), CLIENT_ID_GITHUB.into()] {
        if !v.is_empty() {
            return v;
        }
    }
    String::new()
}

/// Verifie le jeton et ouvre une session gardee en memoire seulement.
pub fn ouvrir_session_memoire(cfg: &mut Config, methode: &str, tok: &str) -> bool {
    let (code, user, _) = requete(cfg, "GET", "/user", None, Some(tok));
    if code != 200 {
        err(&format!("Connexion impossible, code {} : {}", if code == 0 { "aucun".into() } else { code.to_string() },
                     message_api(&user)));
        return false;
    }
    session::ouvrir(tok, user.clone(), methode);
    configurer_identite(cfg, &user);
    ok(&format!("Connexion OK : {}", login_de(&user)));
    true
}

/// Connexion par code : un code a saisir sur github.com/login/device, depuis
/// n'importe quel appareil. Le jeton reste en memoire et expire en 8 heures.
pub fn connexion_par_code(cfg: &mut Config) -> bool {
    let (cid, h) = (client_id(cfg), host(cfg, None));
    let Some(dem) = formulaire(&format!("https://{h}/login/device/code"), &[("client_id", &cid)]) else {
        err("Connexion par code impossible : aucune connexion internet.");
        return false;
    };
    let champ = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if dem.get("user_code").is_none() {
        let e = champ(&dem, "error_description");
        err(&format!("Connexion par code impossible : {}", if e.is_empty() { champ(&dem, "error") } else { e }));
        return false;
    }
    println!();
    info("Ouvrir cette adresse sur un autre appareil, un mobile par exemple :");
    println!("{}", c(&format!("       {}", champ(&dem, "verification_uri")), "1"));
    info("puis saisir ce code :");
    println!("{}", c(&format!("       {}", champ(&dem, "user_code")), "1;33"));
    info("En attente de la validation, Ctrl+C pour annuler...");
    let mut intervalle = dem.get("interval").and_then(Value::as_u64).unwrap_or(5);
    let fin = maintenant() + dem.get("expires_in").and_then(Value::as_i64).unwrap_or(900);
    let code_appareil = champ(&dem, "device_code");
    while maintenant() < fin {
        std::thread::sleep(std::time::Duration::from_secs(intervalle));
        if crate::interrompu() {
            return false;
        }
        let Some(rep) = formulaire(&format!("https://{h}/login/oauth/access_token"),
                                   &[("client_id", &cid), ("device_code", &code_appareil),
                                     ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")]) else { continue };
        let erreur = champ(&rep, "error");
        if erreur == "authorization_pending" {
            continue;
        }
        if erreur == "slow_down" {
            intervalle = rep.get("interval").and_then(Value::as_u64).unwrap_or(intervalle + 5);
            continue;
        }
        let jeton = champ(&rep, "access_token");
        if !erreur.is_empty() || jeton.is_empty() {
            let e = champ(&rep, "error_description");
            err(&format!("Connexion impossible : {}", if e.is_empty() { erreur } else { e }));
            return false;
        }
        if !ouvrir_session_memoire(cfg, "code", &jeton) {
            return false;
        }
        if !APP_GITHUB.is_empty() {
            let (code, inst, _) = requete(cfg, "GET", "/user/installations", None, None);
            if code == 200 && inst.get("installations").and_then(Value::as_array).is_none_or(|a| a.is_empty()) {
                warn("L'application gitmanager manque sur ce compte : l'ajouter ici,");
                info(&format!("https://{h}/apps/{APP_GITHUB}/installations/new"));
            }
        }
        return true;
    }
    err("Code trop ancien : recommencer la connexion.");
    false
}

/// Connexion au compte GitHub : par le navigateur, puis par jeton. La connexion
/// par code apparait seulement quand l'application GitHub de gitmanager est
/// configuree. autre_compte : les cles enregistrees sur cette machine ne servent pas.
pub fn connexion_interactive(cfg: &mut Config, autre_compte: bool) -> bool {
    let code_dispo = !client_id(cfg).is_empty();
    loop {
        println!();
        println!("  Connexion au compte GitHub {} :", nom_compte(None));
        println!();
        println!("    1. Se connecter sur le navigateur");
        println!("    2. Saisir un jeton");
        if code_dispo {
            println!("    3. Se connecter par code");
        }
        println!("    0. Annuler");
        let choix = ask("Choix", "0");
        let connecte = match choix.as_str() {
            "1" => {
                warn("Le compte reste sur cette machine, pour le temps choisi ensuite.");
                if !ask_yn("Continuer ?", true) {
                    continue;
                }
                connexion_navigateur(cfg, autre_compte)
            }
            "2" => connexion_jeton(cfg),
            "3" if code_dispo => connexion_par_code(cfg),
            "0" | "q" => return false,
            _ => {
                warn("Choix invalide.");
                continue;
            }
        };
        if connecte {
            cfg.set(&cle("enregistrer_connexion", None), choix == "1");
            cfg.pop(&cle("moyen_connexion", None));
            cfg.sauver();
            return true;
        }
    }
}

/// Commande GitHub CLI qui passe par le navigateur, sans son texte en anglais :
/// le code a usage unique et l'adresse s'affichent en francais, et la page
/// s'ouvre dans le navigateur. Retourne le code de sortie de GitHub CLI.
pub fn gh_par_navigateur(args: &[&str]) -> i32 {
    let Some(gh) = chemin_gh() else { return 127 };
    let Ok(mut enfant) = Command::new(gh).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .env("GH_PROMPT_DISABLED", "1").env("NO_COLOR", "1").env("GH_NO_UPDATE_NOTIFIER", "1").spawn() else { return 127 };
    // les deux flux de GitHub CLI arrivent ligne a ligne, dans l'ordre
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut fils = vec![];
    if let Some(out) = enfant.stdout.take() {
        let tx = tx.clone();
        fils.push(std::thread::spawn(move || BufReader::new(out).lines().map_while(Result::ok).for_each(|l| { let _ = tx.send(l); })));
    }
    if let Some(e) = enfant.stderr.take() {
        let tx = tx.clone();
        fils.push(std::thread::spawn(move || BufReader::new(e).lines().map_while(Result::ok).for_each(|l| { let _ = tx.send(l); })));
    }
    drop(tx);
    let code_re = regex::Regex::new(r"\b([A-Z0-9]{4}-[A-Z0-9]{4})\b").unwrap();
    let adresse_re = regex::Regex::new(r"(https://\S+)").unwrap();
    let (mut code_vu, mut adresse_vue) = (false, false);
    for ligne in rx {
        let code = code_re.captures(&ligne);
        let adresse = adresse_re.captures(&ligne);
        if code.is_some() && !code_vu {
            code_vu = true;
            println!();
            info("Code pour la page GitHub :");
            println!("{}", c(&format!("       {}", &code.unwrap()[1]), "1;33"));
            ask_libre("Copier ce code, puis Enter pour ouvrir le navigateur", "");
        } else if adresse.as_ref().is_some_and(|a| a[1].contains("device")) && !adresse_vue {
            adresse_vue = true;
            ouvrir_navigateur(adresse.unwrap()[1].trim_end_matches('.'));
            info("Coller le code dans la page, puis cliquer sur 'Authorize'.");
            info("Attente de la validation, Ctrl+C pour annuler...");
        } else if ligne.contains("Authentication complete") {
            ok("Autorisation GitHub OK.");
        } else if crate::debug() && !ligne.trim().is_empty() {
            println!("     {}", ligne.trim_end());
        }
    }
    for f in fils {
        let _ = f.join();
    }
    enfant.wait().ok().and_then(|s| s.code()).unwrap_or(-1)
}

/// GitHub CLI 2.103 ou plus sait demander un jeton qui expire et se renouvelle
/// seul : option --expiring, plus sure qu'un jeton sans fin.
pub fn gh_jetons_expirants() -> bool {
    let Some(gh) = chemin_gh() else { return false };
    let (_, out, _) = run(&[&gh.display().to_string(), "--version"], None, None);
    regex::Regex::new(r"(\d+)\.(\d+)\.\d+").unwrap().captures(&out)
        .is_some_and(|m| (m[1].parse::<u32>().unwrap_or(0), m[2].parse::<u32>().unwrap_or(0)) >= (2, 103))
}

/// Nouveau jeton sans question pour un compte de cette machine, par exemple
/// quand le jeton de GitHub CLI a expire : GitHub CLI le renouvelle.
pub fn rafraichir_session(cfg: &mut Config) -> bool {
    let methode = session::methode().unwrap_or_default();
    if methode != "gh" && methode != "gcm" {
        return false;
    }
    let tok = token_silencieux(cfg, &methode);
    if tok.is_empty() {
        return false;
    }
    let (code, user, _) = requete(cfg, "GET", "/user", None, Some(&tok));
    if code != 200 {
        return false;
    }
    session::maj(|c| {
        c.token = tok;
        c.user = Some(user);
    });
    true
}

/// Connexion interactive : ouvre le navigateur. Les cles deja enregistrees sur
/// ce poste servent sans nouvelle autorisation, sauf pour changer de compte.
pub fn connexion_navigateur(cfg: &mut Config, autre_compte: bool) -> bool {
    let cles = if autre_compte { None } else { cles_enregistrees(cfg, None) };
    if (cles.as_deref() == Some("gh") && chemin_gh().is_some()) || (cles.as_deref() == Some("gcm") && helper_gcm().is_some()) {
        let cles = cles.unwrap();
        let tok = token_silencieux(cfg, &cles);
        if !tok.is_empty() {
            let (code, user, _) = requete(cfg, "GET", "/user", None, Some(&tok));
            if code == 200 {
                let login = login_de(&user);
                demarrer_session(cfg, &cles, &tok, user);
                ok(&format!("Connexion OK : {login}, compte de cette machine"));
                return true;
            }
        }
    }
    let methode = match methode_dispo(cfg) {
        Some(m) => m,
        None => {
            warn("Aucun outil de connexion par le navigateur : ni GitHub CLI, ni Git Credential Manager.");
            if !installer_outil("gh", false) {
                return false;
            }
            "gh"
        }
    };
    let h = host(cfg, None);
    println!();
    info(&format!("Connexion avec {} sur {h}...", nom_methode(methode)));
    let mut tok;
    if methode == "gh" {
        let mut cmd = vec!["auth", "login", "--web", "-h", &h, "-p", "https"];
        if gh_jetons_expirants() {
            cmd.push("--expiring");
            info("Jeton valable quelques heures, puis GitHub CLI en demande un autre tout seul.");
        }
        if gh_par_navigateur(&cmd) != 0 {
            err("Connexion impossible ou abandon.");
            return false;
        }
        if let Some(gh) = chemin_gh() {
            run(&[&gh.display().to_string(), "auth", "setup-git", "-h", &h], None, None);
        }
        tok = token_silencieux(cfg, "gh");
    } else {
        info("GitHub s'ouvre : choisir 'Sign in with your browser'.");
        configurer_helper_gcm(&h);
        if autre_compte {
            oublier_gcm(cfg);
        }
        tok = credential_gcm(cfg, true);
        let code = if tok.is_empty() { 0 } else { requete(cfg, "GET", "/user", None, Some(&tok)).0 };
        if code == 401 {
            // identifiants memorises mais revoques : nouvelle connexion
            oublier_gcm(cfg);
            tok = credential_gcm(cfg, true);
        }
    }
    let (code, user, _) = if tok.is_empty() { (0, json!({}), Default::default()) } else { requete(cfg, "GET", "/user", None, Some(&tok)) };
    if code != 200 {
        let m = user.get("message").and_then(Value::as_str).unwrap_or("pas de retour").to_string();
        err(&format!("Connexion impossible, code {} : {m}", if code == 0 { "aucun".into() } else { code.to_string() }));
        return false;
    }
    let login = login_de(&user);
    demarrer_session(cfg, methode, &tok, user);
    ok(&format!("Connexion OK : {login}"));
    choisir_garde(cfg);
    true
}

// Temps de garde d'un compte sur cette machine, choisi a la connexion par le
// navigateur : pour la session, un nombre d'heures ou de jours, ou toujours. A la
// fin du temps, le compte est supprime, comme avec Fermer la session.
const GARDES: [(&str, Option<i64>); 5] = [
    ("Pour cette session : suppression en fin de programme", Some(-1)),
    ("8 heures", Some(8 * 3600)), ("7 jours", Some(7 * 86400)), ("30 jours", Some(30 * 86400)), ("Toujours", None),
];

/// Temps de garde du compte actif sur cette machine.
pub fn choisir_garde(cfg: &mut Config) {
    println!();
    println!("  Temps de garde du compte sur cette machine :");
    for (i, (libelle, _)) in GARDES.iter().enumerate() {
        println!("    {}. {libelle}", i + 1);
    }
    let mut choix = ask("Choix", "1");
    if !choix.parse::<usize>().is_ok_and(|n| (1..=GARDES.len()).contains(&n)) {
        warn("Choix invalide : pour cette session.");
        choix = "1".into();
    }
    match GARDES[choix.parse::<usize>().unwrap() - 1].1 {
        None => {
            cfg.pop(&cle("garde", None));
        }
        Some(-1) => cfg.set(&cle("garde", None), "session"),
        Some(s) => cfg.set(&cle("garde", None), maintenant() + s),
    }
    cfg.sauver();
    let t = texte_garde(cfg, None);
    ok(&format!("Compte sur cette machine : {}.", if t.is_empty() { "toujours".into() } else { t }));
}

pub fn texte_garde(cfg: &Config, profil: Option<&str>) -> String {
    match cfg.get(&cle("garde", profil)) {
        Some(Value::String(s)) if s == "session" => "pour cette session".into(),
        Some(Value::Number(n)) => n.as_i64()
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .map(|d| format!("jusqu'au {}", d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Supprime les comptes dont le temps de garde est fini. Un compte pour cette
/// session part en fin de programme, ou au demarrage suivant si le programme
/// s'est arrete sans finir.
pub fn verifier_gardes(cfg: &mut Config, demarrage: bool, fin: bool) {
    let actif = session::profil();
    for nom in PROFILS {
        let garde = cfg.get(&cle("garde", Some(nom))).cloned();
        let fini = match &garde {
            None => false,
            Some(Value::String(s)) if s == "session" => demarrage || fin,
            Some(Value::Number(n)) => n.as_i64().is_some_and(|t| maintenant() >= t),
            Some(_) => true,
        };
        if !fini {
            continue;
        }
        session::activer_profil(nom);
        let login = session::login().unwrap_or_else(|| cfg.texte(&cle("login", None)));
        session::vider();
        println!();
        warn(&format!("Compte GitHub {} {login} : fin du temps de garde sur cette machine.", nom_compte(None)));
        let voir = session::voir_git();
        session::set_voir_git(false); // une seule ligne, sans les commandes
        supprimer_compte(cfg, Some(&login).filter(|l| !l.is_empty()).map(String::as_str), true);
        session::set_voir_git(voir);
        if nom == actif {
            session::set_menu_principal(true);
        }
    }
    session::activer_profil(actif);
}

/// Session ouverte sans question avec le compte de cette machine, s'il
/// existe et si GitHub l'accepte encore.
pub fn connexion_enregistree(cfg: &mut Config) -> bool {
    let Some(cles) = cles_enregistrees(cfg, None) else { return false };
    if (cles == "gh" && chemin_gh().is_none()) || (cles == "gcm" && helper_gcm().is_none()) {
        return false;
    }
    let tok = token_silencieux(cfg, &cles);
    if tok.is_empty() {
        return false;
    }
    let (code, user, _) = requete(cfg, "GET", "/user", None, Some(&tok));
    if code != 200 {
        return false;
    }
    demarrer_session(cfg, &cles, &tok, user);
    cfg.set(&cle("enregistrer_connexion", None), true);
    cfg.sauver();
    true
}

/// Reutilise la connexion en cours ou enregistree, sinon en demande une.
pub fn assurer_connexion(cfg: &mut Config, interactif: bool) -> bool {
    if !session::token().is_empty() {
        return true;
    }
    let methode = if cfg.vrai(&cle("enregistrer_connexion", None)) { methode_dispo(cfg) } else { None };
    if let Some(methode) = methode {
        let tok = token_silencieux(cfg, methode);
        if !tok.is_empty() {
            let (code, user, _) = requete(cfg, "GET", "/user", None, Some(&tok));
            if code == 200 {
                demarrer_session(cfg, methode, &tok, user);
                return true;
            }
            if code == 0 {
                warn(&format!("GitHub injoignable : {}.", user.get("message").and_then(Value::as_str).unwrap_or("")));
                return false;
            }
        }
    }
    interactif && connexion_interactive(cfg, false)
}

/// Deconnexion du compte actif. Quand le compte a laisse des identifiants sur
/// cette machine, le programme propose de le garder ou de le supprimer.
/// Retourne false si la deconnexion est annulee.
pub fn deconnexion(cfg: &mut Config) -> R<bool> {
    let login = session::login().unwrap_or_else(|| cfg.texte(&cle("login", None)));
    let traces = traces_compte(cfg, None);
    let mut supprimer = false;
    if !traces.is_empty() {
        println!();
        info(&format!("Sur cette machine, pour le compte GitHub {} {login} :", nom_compte(None)));
        for t in &traces {
            println!("       - {t}");
        }
        println!();
        println!("  Garder le compte sur cette machine ?");
        println!("    1. Garder le compte");
        println!("    2. Supprimer le compte");
        println!("    0. Annuler");
        let choix = ask("Choix", "0");
        if choix != "1" && choix != "2" {
            info("Abandon : la session reste ouverte.");
            return Ok(false);
        }
        supprimer = choix == "2";
    }
    session::vider();
    session::set_menu_principal(true);
    println!();
    ok(&format!("Fin de session : compte GitHub {}.", nom_compte(None)));
    if supprimer {
        supprimer_compte(cfg, Some(&login).filter(|l| !l.is_empty()).map(String::as_str), false);
    } else {
        cfg.set(&cle("enregistrer_connexion", None), false);
        cfg.sauver();
        info(if traces.is_empty() { "Ce compte ne laisse rien sur cette machine." } else { "Le compte reste sur cette machine." });
    }
    Ok(true)
}

/// Fichiers ou Git peut ecrire des identifiants en clair.
pub fn fichiers_identifiants() -> Vec<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok().filter(|v| !v.is_empty())
        .map(PathBuf::from).unwrap_or_else(|| maison().join(".config"));
    vec![maison().join(".git-credentials"), xdg.join("git").join("credentials")]
}

/// Lignes d'identifiants en clair de ce compte : liste de (fichier, ligne).
pub fn identifiants_en_clair(serveur: &str, login: &str) -> Vec<(PathBuf, String)> {
    let mut trouves = vec![];
    let adresse = regex::Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*://([^:@/]*)(?::[^@/]*)?@([^/:?#]+)").unwrap();
    for fichier in fichiers_identifiants() {
        let Ok(texte) = std::fs::read_to_string(&fichier) else { continue };
        for ligne in texte.lines() {
            if let Some(m) = adresse.captures(ligne.trim()) {
                let qui = percent_decode(&m[1]);
                if m[2].to_lowercase() == serveur && qui.to_lowercase() == login.to_lowercase() {
                    trouves.push((fichier.clone(), ligne.to_string()));
                }
            }
        }
    }
    trouves
}

fn percent_decode(t: &str) -> String {
    let octets = t.as_bytes();
    let mut res = vec![];
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' && i + 2 < octets.len() {
            if let Ok(v) = u8::from_str_radix(&t[i + 1..i + 3], 16) {
                res.push(v);
                i += 3;
                continue;
            }
        }
        res.push(octets[i]);
        i += 1;
    }
    String::from_utf8_lossy(&res).into_owned()
}

/// Ce qui reste du compte sur cette machine : cles de connexion, identifiants
/// en clair, identite des commits ecrite par gitmanager, nom du compte dans la
/// configuration de gitmanager.
pub fn traces_compte(cfg: &Config, profil: Option<&str>) -> Vec<String> {
    let profil: &str = match profil { Some(p) => p, None => session::profil() };
    let login = session::compte_de(profil).user.map(|u| login_de(&u)).filter(|l| !l.is_empty())
        .unwrap_or_else(|| cfg.texte(&cle("login", Some(profil))));
    let mut traces = vec![];
    if let Some(m) = cles_enregistrees(cfg, Some(profil)) {
        traces.push(format!("connexion par {}", nom_methode(&m)));
    }
    if !login.is_empty() && !identifiants_en_clair(&host(cfg, Some(profil)), &login).is_empty() {
        traces.push("identifiants en clair, dans le fichier credentials de Git".into());
    }
    let ident = identite_ecrite(cfg, Some(profil));
    if !ident.is_empty() {
        traces.push(format!("nom et email des commits dans la config Git : {}", ident.join(", ")));
    }
    if ["login", "methode", "cles"].iter().any(|k| cfg.get(&cle(k, Some(profil))).is_some_and(vrai)) {
        traces.push("nom du compte dans la configuration de gitmanager".into());
    }
    traces
}

/// Retire de la configuration globale de Git l'aide de connexion pour ce
/// serveur dont la valeur contient motif, et la ligne vide qui la precede.
pub fn retirer_helper(serveur: &str, motif: &str) {
    for cle_git in [format!("credential.https://{serveur}.helper"), format!("credential.https://gist.{serveur}.helper")] {
        let (_, valeurs, _) = git(&["config", "--global", "--get-all", &cle_git], None);
        let valeurs: Vec<&str> = valeurs.lines().collect();
        if !valeurs.iter().any(|v| v.contains(motif)) {
            continue;
        }
        if valeurs.iter().all(|v| v.contains(motif) || v.trim().is_empty()) {
            git(&["config", "--global", "--unset-all", &cle_git], None);
        } else {
            git(&["config", "--global", "--unset-all", &cle_git, &regex::escape(motif)], None);
        }
    }
}

/// Le login apparait comme un mot entier, sans tenir compte des majuscules.
fn contient_login(texte: &str, login: &str) -> bool {
    let (t, l) = (texte.to_lowercase(), login.to_lowercase());
    let bord = |ch: Option<char>| ch.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
    let mut depart = 0;
    while let Some(pos) = t[depart..].find(&l) {
        let i = depart + pos;
        if bord(t[..i].chars().last()) && bord(t[i + l.len()..].chars().next()) {
            return true;
        }
        depart = i + l.len().max(1);
    }
    false
}

/// Supprime de cette machine tous les identifiants du compte actif : cles de
/// GitHub CLI et de Git Credential Manager, identifiants en clair, identite des
/// commits ecrite par gitmanager, reglages du compte dans gitmanager.
pub fn supprimer_compte(cfg: &mut Config, login: Option<&str>, silencieux: bool) -> bool {
    let (profil, h) = (session::profil(), host(cfg, None));
    let login = login.map(str::to_string).filter(|l| !l.is_empty())
        .or_else(|| Some(cfg.texte(&cle("login", None))).filter(|l| !l.is_empty()));
    let methode = cles_enregistrees(cfg, None);
    let autre = if profil == "perso" { "entreprise" } else { "perso" };
    let partage = host(cfg, Some(autre)) == h && cles_enregistrees(cfg, Some(autre)).is_some();
    let sans_invite: Env = vec![("GIT_TERMINAL_PROMPT".into(), Some("0".into())), ("GCM_INTERACTIVE".into(), Some("never".into()))];
    let mut restes = vec![];
    if let Some(gh) = chemin_gh() {
        let gh = gh.display().to_string();
        let statut = |h: &str| { let (_, o, e) = run(&[&gh, "auth", "status", "-h", h], None, None); o + &e };
        let mut etat = statut(&h);
        let present = login.as_deref().is_some_and(|l| contient_login(&etat, l));
        if present || (methode.as_deref() == Some("gh") && login.is_none()) {
            let ok_logout = login.as_deref().is_some_and(|l| run(&[&gh, "auth", "logout", "-h", &h, "-u", l], None, None).0 == 0);
            if !ok_logout {
                run(&[&gh, "auth", "logout", "-h", &h], None, None);
            }
            etat = statut(&h);
            if login.as_deref().is_some_and(|l| contient_login(&etat, l)) {
                restes.push("connexion dans GitHub CLI".to_string());
            }
        }
        if !etat.contains("Logged in") {
            retirer_helper(&h, "auth git-credential");
        }
    }
    if let Some(helper) = helper_gcm() {
        if let Some(l) = &login {
            run(&["git", &format!("credential-{helper}"), "github", "logout", l], None, Some(&sans_invite));
            run_gcm("reject", &format!("protocol=https\nhost={h}\nusername={l}\n\n"), Some(&sans_invite));
        }
        if methode.as_deref() == Some("gcm") {
            run_gcm("reject", &format!("protocol=https\nhost={h}\n\n"), Some(&sans_invite));
            if !partage {
                retirer_helper(&h, &helper);
            }
        }
    }
    if let Some(l) = &login {
        run(&["git", "credential", "reject"], Some(&format!("protocol=https\nhost={h}\nusername={l}\n\n")), Some(&sans_invite));
        let trouves = identifiants_en_clair(&h, l);
        let mut fichiers: Vec<&PathBuf> = trouves.iter().map(|(f, _)| f).collect();
        fichiers.dedup();
        for fichier in fichiers {
            let a_retirer: Vec<&String> = trouves.iter().filter(|(f, _)| f == fichier).map(|(_, l)| l).collect();
            let gardees: Vec<String> = std::fs::read_to_string(fichier).unwrap_or_default().lines()
                .filter(|l| !a_retirer.iter().any(|r| r.as_str() == *l)).map(str::to_string).collect();
            if gardees.is_empty() {
                let _ = std::fs::remove_file(fichier);
            } else {
                let _ = std::fs::write(fichier, gardees.join("\n") + "\n");
            }
        }
    }
    for cle_git in identite_ecrite(cfg, None) {
        git(&["config", "--global", "--unset", &cle_git], None);
    }
    let mut notes = cfg.get("identite_ecrite").and_then(Value::as_object).cloned().unwrap_or_default();
    notes.retain(|_, v| v.get("profil").and_then(Value::as_str) != Some(profil));
    if notes.is_empty() {
        cfg.pop("identite_ecrite");
    } else {
        cfg.set("identite_ecrite", Value::Object(notes));
    }
    for k in ["login", "methode", "cles", "enregistrer_connexion", "moyen_connexion", "garde"] {
        cfg.pop(&cle(k, None));
    }
    cfg.sauver();
    restes.extend(traces_compte(cfg, None));
    if !restes.is_empty() {
        warn("Ces identifiants du compte restent sur cette machine :");
        for t in &restes {
            warn(&format!("  - {t}"));
        }
    } else if !silencieux {
        ok(&format!("Compte {} : suppression OK sur cette machine.", login.as_deref().unwrap_or("")).replace("  ", " "));
    }
    if (methode.is_some() || login.is_some()) && !silencieux {
        info("Sur GitHub, les autorisations restent visibles ici, pour les retirer :");
        info(&format!("https://{h}/settings/applications et https://{h}/settings/tokens"));
    }
    restes.is_empty()
}

/// Session ouverte : supprime les identifiants du compte de cette machine, et
/// ferme la session.
pub fn act_supprimer_identifiants(cfg: &mut Config) -> R<Res> {
    let login = session::login().unwrap_or_else(|| cfg.texte(&cle("login", None)));
    let traces = traces_compte(cfg, None);
    println!();
    info(&format!("Sur cette machine, pour le compte GitHub {} {login} :", nom_compte(None)));
    for t in &traces {
        println!("       - {t}");
    }
    if !ask_yn("Supprimer ces identifiants ? La session se ferme aussi.", false) {
        info("Abandon.");
        return Ok(Res::Rien);
    }
    session::vider();
    session::set_menu_principal(true);
    println!();
    ok(&format!("Fin de session : compte GitHub {}.", nom_compte(None)));
    supprimer_compte(cfg, Some(&login).filter(|l| !l.is_empty()).map(String::as_str), false);
    Ok(Res::Rien)
}

/// Sans connexion en cours : supprime le compte enregistre sur cette machine.
pub fn act_supprimer_compte(cfg: &mut Config) {
    let login = cfg.texte(&cle("login", None));
    let traces = traces_compte(cfg, None);
    println!();
    if traces.is_empty() {
        info(&format!("Aucun identifiant du compte GitHub {} sur cette machine.", nom_compte(None)));
        return;
    }
    info(&format!("Sur cette machine, pour le compte GitHub {} {login} :", nom_compte(None)));
    for t in &traces {
        println!("       - {t}");
    }
    if ask_yn("Supprimer ce compte de cette machine ?", false) {
        supprimer_compte(cfg, Some(&login).filter(|l| !l.is_empty()).map(String::as_str), false);
    }
}

pub fn changer_serveur(cfg: &mut Config) {
    let nouveau = ask("Serveur, par exemple github.com ou github.monentreprise.com", &host(cfg, None));
    let n = nouveau.trim_start_matches("https://").trim_start_matches("http://").trim_matches('/').to_lowercase();
    cfg.set(&cle("host", None), n);
    cfg.sauver();
    session::vider();
    connexion_interactive(cfg, false);
}

/// Nouvelle connexion du compte actif par le moyen choisi ; la connexion
/// d'avant reste en place si la nouvelle est annulee.
pub fn changer_moyen(cfg: &mut Config) {
    let avant = session::compte();
    session::vider();
    if connexion_interactive(cfg, false) {
        return;
    }
    session::remettre(avant);
    info("Connexion sans changement.");
}

/// Connexion a un autre compte GitHub a la place du compte actif ; le compte
/// d'avant reste connecte si la nouvelle connexion est annulee.
pub fn changer_compte(cfg: &mut Config) {
    let avant = session::compte();
    let ancien = avant.user.as_ref().map(login_de).filter(|l| !l.is_empty());
    println!();
    info(&format!("Changer de compte GitHub {}, compte actuel : {}.", nom_compte(None), ancien.as_deref().unwrap_or("aucun")));
    info("Par code ou par le navigateur, GitHub autorise le compte ouvert dans le");
    info("navigateur : s'y connecter d'abord au bon compte, ou utiliser un autre profil.");
    session::vider();
    if !connexion_interactive(cfg, true) {
        session::remettre(avant);
        info(&match &ancien { Some(a) => format!("Compte sans changement : {a}."), None => "Aucun compte en session.".into() });
        return;
    }
    let nouveau = session::login().unwrap_or_default();
    if ancien.as_deref() == Some(nouveau.as_str()) {
        warn(&format!("Toujours le compte {nouveau}. Quitter ce compte GitHub dans le navigateur,"));
        warn("puis recommencer.");
    } else {
        ok(&format!("Compte GitHub {} : {nouveau}", nom_compte(None)));
    }
}

/// Permissions de la connexion, relues sur GitHub a chaque affichage.
pub fn afficher_permissions(cfg: &mut Config) {
    let (code, _, hdr) = requete(cfg, "GET", "/user", None, None);
    if code != 200 {
        err(&format!("Lecture impossible, code {code}."));
        return;
    }
    println!();
    let scopes = entete(&hdr, "X-OAuth-Scopes");
    if !scopes.is_empty() {
        info(&format!("Permissions du jeton : {scopes}"));
    } else if session::methode().is_some_and(|m| EN_MEMOIRE.contains(&m.as_str())) {
        info("Jeton fin : permissions au choix, repo par repo, sur GitHub.");
    }
    let expiration = entete(&hdr, "GitHub-Authentication-Token-Expiration").to_string();
    if !expiration.is_empty() {
        info(&format!("Expiration du jeton : {expiration}"));
    }
    if let Some(repos) = liste_repos(cfg, "") {
        let ecriture = repos.iter().filter(|r| r["permissions"]["push"].as_bool() == Some(true)).count();
        info(&format!("Repos ouverts pour ce compte : {}, dont {ecriture} avec droit de push.", repos.len()));
    }
    info("Ces informations sont lues a chaque fois : un changement de permission sur");
    info("GitHub compte tout de suite, sans nouvelle connexion.");
}

/// Menu Compte GitHub. Fermer la session : choix 4 du menu d'avant, pas de doublon ici.
pub fn act_compte(cfg: &mut Config) -> R<Res> {
    crate::affichage::titre("Compte GitHub");
    assurer_connexion(cfg, false);
    let h = host(cfg, None);
    let user = session::user();
    if user.is_some() {
        ok(&format!("En session : {}, {}, {h}", session::login().unwrap_or_default(),
                    nom_methode(&session::methode().unwrap_or_default())));
    } else {
        warn(&format!("Aucune session sur {h}."));
    }
    let cles = cles_enregistrees(cfg, None);
    if cles.is_some() {
        let t = texte_garde(cfg, None);
        info(&format!("Identifiants sur cette machine : {}.", if t.is_empty() { "toujours" } else { &t }));
    }
    println!();
    type Action = fn(&mut Config);
    let mut actions: Vec<(String, Action)> = vec![];
    if user.is_some() {
        actions.push(("Changer de moyen de connexion".into(), changer_moyen));
        actions.push(("Changer de compte".into(), changer_compte));
        actions.push(("Changer de serveur GitHub, par exemple GitHub Enterprise".into(), changer_serveur));
        actions.push(("Voir les permissions de la connexion".into(), afficher_permissions));
        if cles.is_some() {
            actions.push(("Temps de garde du compte sur cette machine".into(), choisir_garde));
        }
    } else {
        actions.push(("Se connecter".into(), |cfg| { connexion_interactive(cfg, false); }));
        actions.push(("Changer de serveur GitHub, par exemple GitHub Enterprise".into(), changer_serveur));
        if !traces_compte(cfg, None).is_empty() {
            actions.push(("Supprimer les identifiants de cette machine".into(), act_supprimer_compte));
        }
    }
    match menu_numerote(&actions, 1, "Retour") {
        Some(f) => {
            f(cfg);
            Ok(Res::Rien)
        }
        None => Ok(Res::Retour),
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn login_en_mot_entier() {
        assert!(contient_login("Logged in to github.com account Saleh (keyring)", "saleh"));
        assert!(!contient_login("Logged in to github.com account saleh-pro", "saleh"));
        assert!(!contient_login("account xsaleh", "saleh"));
    }

    #[test]
    fn pourcent_decode() {
        assert_eq!(percent_decode("saleh%40mail"), "saleh@mail");
        assert_eq!(percent_decode("abc"), "abc");
    }
}
