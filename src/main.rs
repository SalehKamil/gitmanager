//! gitmanager : coder a la maison, pousser au bureau, gerer les repos GitHub
//! en local comme en ligne. Les menus remplacent les commandes Git.
#![allow(dead_code)] // le temps de la reecriture : retire a la fin de la tache 5

mod affichage;
mod config;
mod connexion;
mod en_ligne;
mod entreprise;
mod erreur;
mod git;
mod github;
mod local;
mod menus;
mod outils;
mod resultat;
mod saisie;
mod session;
mod urls;

use std::sync::atomic::{AtomicBool, Ordering};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GITMANAGER_DEBUG=1 : les erreurs gardent leur detail technique, en anglais
pub fn debug() -> bool {
    std::env::var_os("GITMANAGER_DEBUG").is_some_and(|v| !v.is_empty())
}

static INTERROMPU: AtomicBool = AtomicBool::new(false);
static FIN: AtomicBool = AtomicBool::new(false);

/// Ctrl+C pendant une action : l'action s'arrete et le menu revient.
pub fn interrompu() -> bool {
    INTERROMPU.load(Ordering::SeqCst)
}

pub fn prendre_interruption() -> bool {
    INTERROMPU.swap(false, Ordering::SeqCst)
}

/// Fin du programme sans passer par le menu principal : fin de l'entree
/// standard ou Ctrl+C pendant une question. Un repo rendu public le temps d'un
/// clone redevient non public ; les comptes pour cette session partent.
pub fn fin_brutale(code: i32) -> ! {
    if FIN.swap(true, Ordering::SeqCst) {
        loop {
            std::thread::park(); // la premiere fin est en cours
        }
    }
    github::rendre_prive_urgence();
    let mut cfg = config::Config::charger();
    menus::fin_des_sessions(&mut cfg);
    println!("\n\n  Interrompu.\n");
    affichage::sortie();
    std::process::exit(code)
}

fn main() {
    if std::env::args().skip(1).any(|a| a == "--version") {
        println!("gitmanager {VERSION}");
        return;
    }
    if !debug() {
        std::panic::set_hook(Box::new(|_| {})); // le menu dit qu'une erreur est arrivee
    }
    let _ = ctrlc::set_handler(|| {
        if session::en_saisie() {
            fin_brutale(130);
        }
        INTERROMPU.store(true, Ordering::SeqCst);
    });
    menus::principal();
}
