//! API REST de GitHub : appels, pages, permissions manquantes, messages en francais.

use crate::affichage::{err, info, warn};
use crate::config::Config;
use crate::connexion;
use crate::saisie::ask_yn;
use crate::session;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// En-tetes d'une reponse, noms en minuscules.
pub type Entetes = HashMap<String, String>;

pub fn entete<'a>(h: &'a Entetes, nom: &str) -> &'a str {
    h.get(&nom.to_lowercase()).map(String::as_str).unwrap_or("")
}

pub fn host(cfg: &Config, profil: Option<&str>) -> String {
    let h = cfg.texte(&session::cle("host", profil)).trim().to_lowercase();
    if h.is_empty() { "github.com".into() } else { h }
}

pub fn api_base(cfg: &Config) -> String {
    if let Ok(a) = std::env::var("GITMANAGER_API") {
        if !a.is_empty() {
            return a.trim_end_matches('/').to_string(); // essais et debogage
        }
    }
    let h = host(cfg, None);
    if h == "github.com" { "https://api.github.com".into() } else { format!("https://{h}/api/v3") }
}

/// Client HTTPS : certificats verifies par le systeme (magasin Windows, trousseau
/// macOS, certificats de Linux), proxy de HTTPS_PROXY ou du systeme sous Windows.
static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .proxy(ureq::Proxy::try_from_env())
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .user_agent("gitmanager-script")
        .build()
        .into()
});

fn json_de(texte: &str) -> Value {
    if texte.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str(texte).unwrap_or_else(|_| json!({"message": texte.chars().take(400).collect::<String>()}))
}

fn reponse(r: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> (u16, Value, Entetes) {
    match r {
        Ok(mut rep) => {
            let code = rep.status().as_u16();
            let entetes = rep.headers().iter()
                .map(|(k, v)| (k.as_str().to_lowercase(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let texte = rep.body_mut().with_config().limit(64 * 1024 * 1024).read_to_string().unwrap_or_default();
            (code, json_de(&texte), entetes)
        }
        Err(_) => (0, json!({"message": "aucune connexion internet"}), Entetes::new()),
    }
}

/// Appel REST GitHub brut. Retourne (status, json, en-tetes) ; 0 sans reseau.
pub fn requete(cfg: &Config, verbe: &str, chemin: &str, data: Option<&Value>, token: Option<&str>) -> (u16, Value, Entetes) {
    let tok = token.map(str::to_string).unwrap_or_else(session::token);
    let url = if chemin.starts_with("http") { chemin.to_string() } else { api_base(cfg) + chemin };
    let a = &*AGENT;
    macro_rules! entetes {
        ($r:expr) => {{
            let mut r = $r.header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
            if !tok.is_empty() {
                r = r.header("Authorization", format!("Bearer {tok}"));
            }
            r
        }};
    }
    let corps = data.map(|d| d.to_string());
    let r = match (verbe, corps) {
        ("POST", Some(c)) => entetes!(a.post(&url)).header("Content-Type", "application/json").send(c),
        ("PATCH", Some(c)) => entetes!(a.patch(&url)).header("Content-Type", "application/json").send(c),
        ("PUT", Some(c)) => entetes!(a.put(&url)).header("Content-Type", "application/json").send(c),
        ("POST", None) => entetes!(a.post(&url)).send_empty(),
        ("DELETE", _) => entetes!(a.delete(&url)).call(),
        _ => entetes!(a.get(&url)).call(),
    };
    reponse(r)
}

/// POST de formulaire vers GitHub, reponse JSON (connexion par code).
pub fn formulaire(url: &str, champs: &[(&str, &str)]) -> Option<Value> {
    let r = AGENT.post(url).header("Accept", "application/json").send_form(champs.iter().copied());
    match r {
        Ok(mut rep) => serde_json::from_str(&rep.body_mut().read_to_string().ok()?).ok(),
        Err(_) => None,
    }
}

/// Permission demandee par GitHub quand il refuse une action, sinon ''.
pub fn permission_manquante(code: u16, hdr: &Entetes) -> String {
    if code != 403 || hdr.is_empty() || entete(hdr, "X-RateLimit-Remaining") == "0" {
        return String::new();
    }
    let fines = entete(hdr, "X-Accepted-GitHub-Permissions");
    if !fines.is_empty() {
        return fines.to_string(); // jeton fin ou application GitHub
    }
    let liste = |t: &str| -> Vec<String> {
        let mut v: Vec<String> = t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    };
    let accepte = liste(entete(hdr, "X-Accepted-OAuth-Scopes"));
    let possede = liste(entete(hdr, "X-OAuth-Scopes"));
    if !accepte.is_empty() && !accepte.iter().any(|s| possede.contains(s)) { accepte.join(", ") } else { String::new() }
}

/// Permission manquante : la faire ajouter, puis reessayer avec le meme jeton.
/// Une permission modifiee sur GitHub s'applique sans reconnexion.
pub fn mettre_a_jour_permissions(cfg: &mut Config, besoin: &str, hdr: Option<&Entetes>) -> bool {
    warn(&format!("Permission manquante pour cette action : {besoin}"));
    let scopes = hdr.map(|h| entete(h, "X-Accepted-OAuth-Scopes").to_string()).unwrap_or_default();
    let methode = session::methode().unwrap_or_default();
    if methode == "gh" && !scopes.is_empty() {
        info("GitHub CLI demande cette permission dans le navigateur.");
        let liste: Vec<&str> = scopes.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if connexion::gh_par_navigateur(&["auth", "refresh", "-h", &host(cfg, None), "-s", &liste.join(",")]) == 0 {
            let t = connexion::token_silencieux(cfg, "gh");
            session::set_token(&t);
            return !t.is_empty();
        }
        return false;
    }
    if methode == "code" {
        info("Ajouter cette permission dans l'application GitHub de gitmanager, puis");
        info(&format!("accepter le changement sur https://{}/settings/installations.", host(cfg, None)));
    } else if methode == "gcm" || methode == "gh" {
        // connexion par le navigateur : pas de jeton a modifier sur GitHub
        info(&format!("{} ne donne pas cette permission : menu Compte", session::nom_methode(&methode)));
        info("GitHub, Changer de moyen de connexion, puis Saisir un jeton qui l'a.");
        return false;
    } else {
        info(&format!("Ajouter cette permission au jeton sur https://{}/settings/tokens,", host(cfg, None)));
        info("sans en faire un nouveau : le changement compte tout de suite.");
    }
    ask_yn("Nouvel essai ?", true)
}

/// Appel API avec la session courante : reconnexion si elle a expire, et
/// nouvel essai apres l'ajout d'une permission manquante.
pub fn api(cfg: &mut Config, verbe: &str, chemin: &str, data: Option<&Value>) -> (u16, Value, Entetes) {
    if !connexion::assurer_connexion(cfg, true) {
        return (0, json!({"message": "pas de session GitHub"}), Entetes::new());
    }
    let mut res = (0, json!({}), Entetes::new());
    for _ in 0..3 {
        res = requete(cfg, verbe, chemin, data, None);
        let (code, _, hdr) = &res;
        if *code == 401 {
            if connexion::rafraichir_session(cfg) {
                continue;
            }
            session::set_token("");
            if session::methode().as_deref() == Some("gcm") {
                connexion::oublier_gcm(cfg);
            }
            warn("Session GitHub finie : nouvelle connexion.");
            if connexion::connexion_interactive(cfg, false) {
                continue;
            }
            break;
        }
        let besoin = permission_manquante(*code, hdr);
        let hdr = hdr.clone();
        if besoin.is_empty() || !mettre_a_jour_permissions(cfg, &besoin, Some(&hdr)) {
            break;
        }
    }
    res
}

const MESSAGES_API: [(&str, &str); 10] = [
    ("aucune connexion internet", "aucune connexion internet"),
    ("bad credentials", "identifiants faux ou trop anciens"),
    ("rate limit", "limite d'appels GitHub : attendre un peu"),
    ("name already exists", "ce nom existe sur ce compte"),
    ("resource not accessible", "permission manquante"),
    ("must have admin", "droits admin requis"),
    ("requires authentication", "connexion requise"),
    ("repository creation failed", "nouveau repo impossible"),
    ("validation failed", "demande invalide"),
    ("not found", "introuvable"),
];

/// Message d'erreur de l'API GitHub, en francais.
pub fn message_api(rep: &Value) -> String {
    let vide = rep.as_object().is_none_or(|o| o.is_empty());
    if vide {
        return "pas de retour".into();
    }
    let texte = rep.to_string().to_lowercase();
    for (anglais, francais) in MESSAGES_API {
        if texte.contains(anglais) {
            return francais.into();
        }
    }
    let message = rep.get("message").and_then(Value::as_str).unwrap_or("");
    if crate::debug() && !message.is_empty() { format!("refus de GitHub : {message}") } else { "refus de GitHub".into() }
}

/// GET pagine : suit les en-tetes Link jusqu'a la derniere page.
pub fn api_liste(cfg: &mut Config, chemin: &str) -> Option<Vec<Value>> {
    let mut tout = vec![];
    let mut url = Some(chemin.to_string());
    let suivante = regex::Regex::new(r#"<([^>]+)>;\s*rel="next""#).unwrap();
    while let Some(u) = url {
        let (code, rep, hdr) = api(cfg, "GET", &u, None);
        let Some(liste) = rep.as_array().filter(|_| code == 200) else {
            err(&format!("Lecture impossible, code {code} : {}", message_api(&rep)));
            return None;
        };
        tout.extend(liste.iter().cloned());
        url = suivante.captures(entete(&hdr, "Link")).map(|m| m[1].to_string());
    }
    Some(tout)
}

/// Repos du compte en session, sans ceux supprimes pendant cette session.
pub fn liste_repos(cfg: &mut Config, tri: &str) -> Option<Vec<Value>> {
    let tri = if tri.is_empty() { String::new() } else { format!("&sort={tri}") };
    let repos = api_liste(cfg, &format!("/user/repos?per_page=100{tri}&affiliation=owner,collaborator,organization_member"))?;
    Some(repos.into_iter().filter(|d| !session::est_supprime(d["full_name"].as_str().unwrap_or(""))).collect())
}

pub fn texte(v: &Value, cle: &str) -> String {
    v.get(cle).and_then(Value::as_str).unwrap_or("").to_string()
}

// Repo rendu public le temps d'un clone : (adresse de l'API, jeton, compte/nom).
// Avant toute sortie du programme, il redevient non public.
static OUVERTURE: Mutex<Option<(String, String, String)>> = Mutex::new(None);

pub fn noter_ouverture(cfg: &Config, full: Option<&str>) {
    *OUVERTURE.lock().unwrap_or_else(|e| e.into_inner()) =
        full.map(|f| (api_base(cfg), session::token(), f.to_string()));
}

/// Sortie brutale pendant l'ouverture publique : le repo redevient non public.
pub fn rendre_prive_urgence() {
    let ouverture = OUVERTURE.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((base, jeton, full)) = ouverture {
        let r = AGENT.patch(&format!("{base}/repos/{full}"))
            .header("Accept", "application/vnd.github+json")
            .header("Authorization", format!("Bearer {jeton}"))
            .send(json!({"private": true}).to_string());
        if matches!(&r, Ok(rep) if rep.status().as_u16() == 200) {
            println!("  [OK] {full} est de nouveau non public.");
        } else {
            println!("  [X]  {full} EST TOUJOURS PUBLIC : le rendre non public sur GitHub, Settings.");
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn permission_qui_manque() {
        let mut h = Entetes::new();
        h.insert("x-accepted-oauth-scopes".into(), "delete_repo".into());
        h.insert("x-oauth-scopes".into(), "repo, gist".into());
        assert_eq!(permission_manquante(403, &h), "delete_repo");
        assert_eq!(permission_manquante(404, &h), "");
        h.insert("x-oauth-scopes".into(), "repo, delete_repo".into());
        assert_eq!(permission_manquante(403, &h), "");
        h.insert("x-accepted-github-permissions".into(), "administration=write".into());
        assert_eq!(permission_manquante(403, &h), "administration=write");
    }

    #[test]
    fn messages_de_github_en_francais() {
        assert_eq!(message_api(&json!({"message": "Not Found"})), "introuvable");
        assert_eq!(message_api(&json!({})), "pas de retour");
        assert_eq!(message_api(&json!({"message": "Must have admin rights to Repository."})), "droits admin requis");
    }
}
