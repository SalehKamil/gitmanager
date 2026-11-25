//! Adresses des repos : https, ssh, compte/nom.

use crate::erreur::{Arret, R};
use regex::Regex;
use std::sync::LazyLock;

static IDENTIFIANTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(https?://)[^@/]+@").unwrap());
static ADRESSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:https?://|ssh://(?:[^@/]+@)?|[^@/]+@)([^/:]+)(?::\d+)?[:/](.+)$").unwrap()
});
static HOTE_COMPTE_NOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:www\.)?[\w-]+(?:\.[\w-]+)+/[\w.-]+/[\w.-]+$").unwrap()
});

/// Retire tout identifiant ecrit dans une adresse https.
pub fn sans_identifiants(url: &str) -> String {
    IDENTIFIANTS.replace(url, "$1").into_owned()
}

/// Adresse https qui contient un mot de passe ou un jeton GitHub.
pub fn contient_token(url: &str) -> bool {
    let Some(m) = Regex::new(r"^https?://([^@/]+)@").unwrap().captures(url) else { return false };
    let qui = &m[1];
    qui.contains(':') || ["ghp_", "gho_", "ghu_", "ghs_", "github_pat_"].iter().any(|p| qui.starts_with(p))
}

pub fn est_ssh(url: &str) -> bool {
    url.starts_with("git@") || url.starts_with("ssh://")
}

/// (serveur, compte, repo) d'une adresse https ou ssh.
pub fn infos_url(url: &str) -> (Option<String>, Option<String>, Option<String>) {
    let propre = sans_identifiants(url.trim());
    let propre = propre.trim_end_matches('/');
    let Some(m) = ADRESSE.captures(propre) else { return (None, None, None) };
    let mut chemin = m[2].to_string();
    if chemin.ends_with(".git") {
        chemin.truncate(chemin.len() - 4);
    }
    let morceaux: Vec<&str> = chemin.split('/').collect();
    if morceaux.len() < 2 {
        return (Some(m[1].to_string()), None, None);
    }
    let n = morceaux.len();
    (Some(m[1].to_lowercase()), Some(morceaux[n - 2].to_string()), Some(morceaux[n - 1].to_string()))
}

/// compte/repo en minuscules : la cle qui relie une copie locale a son repo en ligne.
pub fn cle_depot(url: &str) -> Option<String> {
    match infos_url(url) {
        (_, Some(o), Some(r)) if !o.is_empty() && !r.is_empty() => Some(format!("{o}/{r}").to_lowercase()),
        _ => None,
    }
}

/// Page du repo dans le navigateur.
pub fn url_web(url: &str) -> String {
    match infos_url(url) {
        (Some(h), Some(o), Some(r)) if !o.is_empty() && !r.is_empty() => format!("https://{h}/{o}/{r}"),
        _ => String::new(),
    }
}

/// Retire les guillemets autour d'une saisie, comme strip('"').strip("'").
pub fn sans_guillemets(saisie: &str) -> &str {
    saisie.trim().trim_matches('"').trim_matches('\'')
}

/// github.com/compte/nom, colle sans https:// : adresse https complete.
pub fn avec_https(saisie: &str) -> String {
    let s = sans_guillemets(saisie).trim_end_matches('/');
    if HOTE_COMPTE_NOM.is_match(s) {
        return format!("https://{}", s.strip_prefix("www.").unwrap_or(s));
    }
    s.to_string()
}

/// Refuse les adresses non chiffrees (http://, git://) et retire tout
/// identifiant ecrit dans l'adresse.
pub fn verifier_url(url: &str) -> R<String> {
    let url = sans_guillemets(url);
    let bas = url.to_lowercase();
    if bas.starts_with("http://") || bas.starts_with("git://") {
        return Err(Arret::new("erreur", "adresse sans chiffrement : utiliser https:// ou SSH"));
    }
    Ok(sans_identifiants(url))
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn adresses_https_et_ssh() {
        assert_eq!(infos_url("https://github.com/SalehKamil/gitmanager.git"),
                   (Some("github.com".into()), Some("SalehKamil".into()), Some("gitmanager".into())));
        assert_eq!(infos_url("git@github.com:o/r.git").2.as_deref(), Some("r"));
        assert_eq!(infos_url("ssh://git@ghe.example.com:2222/o/r").0.as_deref(), Some("ghe.example.com"));
        assert_eq!(cle_depot("https://x@github.com/SalehKamil/Gitmanager/").as_deref(), Some("salehkamil/gitmanager"));
        assert_eq!(url_web("git@github.com:o/r.git"), "https://github.com/o/r");
        assert!(est_ssh("git@github.com:o/r.git") && est_ssh("ssh://github.com/o/r") && !est_ssh("https://github.com/o/r"));
    }

    #[test]
    fn identifiants_dans_l_adresse() {
        assert_eq!(sans_identifiants("https://user:tok@github.com/o/r"), "https://github.com/o/r");
        assert!(contient_token("https://user:secret@github.com/o/r"));
        assert!(contient_token("https://ghp_abc@github.com/o/r"));
        assert!(!contient_token("https://user@github.com/o/r"));
    }

    #[test]
    fn adresse_collee_sans_https() {
        assert_eq!(avec_https("www.github.com/saleh/demo/"), "https://github.com/saleh/demo");
        assert_eq!(avec_https("github.com/saleh/demo"), "https://github.com/saleh/demo");
        assert_eq!(avec_https("saleh/demo"), "saleh/demo");
    }

    #[test]
    fn adresse_sans_chiffrement_refusee() {
        assert!(verifier_url("http://github.com/o/r").is_err());
        assert!(verifier_url("git://github.com/o/r").is_err());
        assert_eq!(verifier_url(" \"https://u:p@github.com/o/r\" ").unwrap(), "https://github.com/o/r");
    }
}
