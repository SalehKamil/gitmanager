//! Etat de la session : les deux comptes GitHub, perso et entreprise, le
//! compte actif et les reperes de navigation. Global, comme en Python.

use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Debug, Default)]
pub struct Compte {
    pub token: String,
    pub user: Option<Value>,
    pub methode: Option<String>,
}

const VIDE: Compte = Compte { token: String::new(), user: None, methode: None };

struct Etat {
    perso: Compte,
    entreprise: Compte,
    actif: &'static str,
    menu_principal: bool,
    voir_git: bool,
    supprimes: BTreeSet<String>,
    accueil: bool,
    en_saisie: bool,
}

static ETAT: Mutex<Etat> = Mutex::new(Etat {
    perso: VIDE,
    entreprise: VIDE,
    actif: "perso",
    menu_principal: false,
    voir_git: true,
    supprimes: BTreeSet::new(),
    accueil: false,
    en_saisie: false,
});

fn etat() -> MutexGuard<'static, Etat> {
    ETAT.lock().unwrap_or_else(|e| e.into_inner())
}

pub const PROFILS: [&str; 2] = ["perso", "entreprise"];
/// Connexions jamais enregistrees sur le poste
pub const EN_MEMOIRE: [&str; 2] = ["jeton", "code"];

pub fn nom_methode(m: &str) -> &'static str {
    match m {
        "gh" => "GitHub CLI",
        "gcm" => "Git Credential Manager",
        "code" => "code, pour cette session",
        "jeton" => "jeton, pour cette session",
        _ => "?",
    }
}

pub fn profil() -> &'static str {
    etat().actif
}

pub fn activer_profil(nom: &str) {
    etat().actif = if nom == "entreprise" { "entreprise" } else { "perso" };
}

fn choisir<'a>(e: &'a mut Etat, nom: &str) -> &'a mut Compte {
    if nom == "entreprise" { &mut e.entreprise } else { &mut e.perso }
}

/// Compte actif (copie).
pub fn compte() -> Compte {
    let mut e = etat();
    let nom = e.actif;
    choisir(&mut e, nom).clone()
}

pub fn compte_de(nom: &str) -> Compte {
    choisir(&mut etat(), nom).clone()
}

/// Change le compte actif.
pub fn maj(f: impl FnOnce(&mut Compte)) {
    let mut e = etat();
    let nom = e.actif;
    f(choisir(&mut e, nom));
}

pub fn maj_de(nom: &str, f: impl FnOnce(&mut Compte)) {
    f(choisir(&mut etat(), nom));
}

pub fn remettre(c: Compte) {
    maj(|x| *x = c);
}

pub fn token() -> String {
    compte().token
}

pub fn set_token(t: &str) {
    maj(|c| c.token = t.to_string());
}

pub fn user() -> Option<Value> {
    compte().user
}

pub fn login() -> Option<String> {
    user().and_then(|u| u.get("login").and_then(|l| l.as_str()).map(str::to_string))
}

pub fn methode() -> Option<String> {
    compte().methode
}

pub fn ouvrir(token: &str, user: Value, methode: &str) {
    maj(|c| {
        c.token = token.to_string();
        c.user = Some(user);
        c.methode = Some(methode.to_string());
    });
}

/// Fin de session du compte actif.
pub fn vider() {
    maj(|c| *c = Compte::default());
}

pub fn vider_de(nom: &str) {
    maj_de(nom, |c| *c = Compte::default());
}

/// Cle de configuration du compte actif, ou du profil donne : les reglages de
/// connexion du compte de l'entreprise sont gardes a part de ceux du compte perso.
pub fn cle(nom: &str, profil_: Option<&str>) -> String {
    if profil_.map_or_else(|| profil() == "perso", |p| p == "perso") { nom.to_string() } else { format!("{nom}_entreprise") }
}

pub fn nom_compte(profil_: Option<&str>) -> &'static str {
    if profil_.map_or_else(|| profil() == "perso", |p| p == "perso") { "perso" } else { "de l'entreprise" }
}

/// (utilisateur, jeton) pour git, ou None : gestionnaire du poste.
pub fn identite_courante() -> Option<(String, String)> {
    let c = compte();
    match c.methode.as_deref() {
        Some(m) if EN_MEMOIRE.contains(&m) && !c.token.is_empty() => Some((String::new(), c.token)),
        _ => None,
    }
}

// Apres une deconnexion, les menus se referment jusqu'au menu principal
pub fn menu_principal() -> bool {
    etat().menu_principal
}

pub fn set_menu_principal(v: bool) {
    etat().menu_principal = v;
}

/// Commandes git affichees ou non
pub fn voir_git() -> bool {
    etat().voir_git
}

pub fn set_voir_git(v: bool) {
    etat().voir_git = v;
}

// Repos supprimes pendant cette session : juste apres la suppression, GitHub les
// liste encore un moment ; ils restent caches jusqu'a la fin du programme.
pub fn est_supprime(full: &str) -> bool {
    etat().supprimes.contains(&full.to_lowercase())
}

pub fn ajouter_supprime(full: &str) {
    etat().supprimes.insert(full.to_lowercase());
}

pub fn retirer_supprime(full: &str) {
    etat().supprimes.remove(&full.to_lowercase());
}

// Logo juste au-dessus : le menu principal qui suit ne le repete pas
pub fn accueil() -> bool {
    etat().accueil
}

pub fn set_accueil(v: bool) {
    etat().accueil = v;
}

/// Une question attend une reponse : Ctrl+C quitte alors le programme.
pub fn en_saisie() -> bool {
    etat().en_saisie
}

pub fn set_en_saisie(v: bool) {
    etat().en_saisie = v;
}

/// Oubli de la connexion gardee en memoire, apres 15 minutes sans action.
pub fn oublier_connexion_memoire() {
    let c = compte();
    if !c.token.is_empty() && c.methode.as_deref().is_some_and(|m| EN_MEMOIRE.contains(&m)) {
        vider();
        crate::affichage::warn("Fin de session : 15 minutes sans action.");
    }
}
