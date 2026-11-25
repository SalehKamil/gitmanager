#![allow(dead_code)] // le temps de la reecriture : retire a la fin de la tache 5
//! gitmanager : coder a la maison, pousser au bureau, gerer les repos GitHub
//! en local comme en ligne. Les menus remplacent les commandes Git.

mod affichage;
mod config;
mod erreur;
mod saisie;
mod session;
mod urls;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Fin du programme sans passer par le menu principal : fin de l'entree
/// standard ou Ctrl+C pendant une question.
pub fn fin_brutale(code: i32) -> ! {
    println!("\n\n  Interrompu.\n");
    std::process::exit(code)
}

fn main() {
    if std::env::args().skip(1).any(|a| a == "--version") {
        println!("gitmanager {VERSION}");
        return;
    }
    affichage::afficher_accueil();
}
