//! Resultat d'une action : rien, retour au menu d'avant, ou statut et detail.

use crate::affichage::{c, warn};
use crate::erreur::R;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Res {
    Rien,
    /// Enter ramene au menu d'avant : pas de pause
    Retour,
    /// (statut, detail) : ok, a_jour, conflit, ignore, erreur
    Fait(&'static str, String),
}

pub fn fait(statut: &'static str, detail: impl Into<String>) -> R<Res> {
    Ok(Res::Fait(statut, detail.into()))
}

pub fn afficher_resultat(statut: &str, detail: &str, nom: &str) {
    let (etiquette, couleur) = match statut {
        "ok" => ("[OK]", "32"),
        "a_jour" => ("[=] ", "36"),
        "conflit" => ("[!] ", "33"),
        "ignore" => ("[-] ", "36"),
        "erreur" => ("[X] ", "31"),
        _ => ("[?] ", "0"),
    };
    let prefixe = if nom.is_empty() { String::new() } else { format!("{} ", crate::affichage::ljust(nom, 28)) };
    println!("{}", c(&format!("  {etiquette} {prefixe}{detail}"), couleur));
}

/// Ce qu'une action rend, et ce qui s'affiche apres elle.
pub trait Resultat {
    fn afficher(&self) {}
    fn est_retour(&self) -> bool {
        false
    }
}

impl Resultat for Res {
    fn afficher(&self) {
        if let Res::Fait(s, d) = self {
            println!();
            afficher_resultat(s, d, "");
        }
    }
    fn est_retour(&self) -> bool {
        *self == Res::Retour
    }
}

impl Resultat for () {}
impl Resultat for bool {}
impl Resultat for Option<PathBuf> {}

/// Lance une action ; affiche son resultat ou la raison de son arret. None
/// apres un arret.
pub fn executer<T: Resultat>(f: impl FnOnce() -> R<T>) -> Option<T> {
    let res = f();
    if crate::prendre_interruption() {
        println!();
        warn("Interruption.");
    }
    match res {
        Ok(r) => {
            r.afficher();
            Some(r)
        }
        Err(a) => {
            println!();
            afficher_resultat(a.statut, &a.detail, "");
            None
        }
    }
}
