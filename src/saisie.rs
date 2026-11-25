//! Saisie : questions, oui/non, pause, jeton sans affichage.

use crate::affichage::{c, sortie, warn};
use crate::session;
use std::io::{BufRead, IsTerminal};
use std::time::{Duration, Instant};

/// Connexion en memoire oubliee apres 15 minutes sans saisie
const DELAI_INACTIVITE: Duration = Duration::from_secs(15 * 60);

#[cfg(test)]
thread_local! {
    static ESSAI: std::cell::RefCell<std::collections::VecDeque<String>> = Default::default();
}

#[cfg(test)]
pub fn reponses_essai(r: &[&str]) {
    ESSAI.with(|e| *e.borrow_mut() = r.iter().map(|s| s.to_string()).collect());
}

/// Une ligne de l'entree standard, sans le retour a la ligne ; None a la fin.
fn lire_ligne(echo: bool) -> Option<String> {
    #[cfg(test)]
    {
        let _ = echo;
        return ESSAI.with(|e| e.borrow_mut().pop_front());
    }
    #[cfg(not(test))]
    {
        let mut ligne = String::new();
        match std::io::stdin().lock().read_line(&mut ligne) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let ligne = ligne.trim_end_matches(['\n', '\r']).to_string();
                // sans terminal, par exemple avec des reponses ecrites a l'avance :
                // la reponse s'affiche, pour que la suite reste lisible
                if echo && !std::io::stdin().is_terminal() {
                    println!("{ligne}");
                }
                Some(ligne)
            }
        }
    }
}

/// Saisie qui oublie la connexion en memoire apres une longue inactivite. A la
/// fin de l'entree standard, le programme s'arrete proprement.
pub fn saisie(invite: &str) -> String {
    print!("{invite}");
    sortie();
    let debut = Instant::now();
    session::set_en_saisie(true);
    let ligne = lire_ligne(true);
    session::set_en_saisie(false);
    let Some(rep) = ligne else {
        #[cfg(test)]
        panic!("plus de reponse pour : {invite}");
        #[cfg(not(test))]
        crate::fin_brutale(0)
    };
    if debut.elapsed() > DELAI_INACTIVITE {
        session::oublier_connexion_memoire();
    }
    rep
}

fn demander(question: &str, defaut: &str, obligatoire: bool) -> String {
    let suffixe = if defaut.is_empty() { String::new() } else { format!(" [{defaut}]") };
    loop {
        let rep = saisie(&c(&format!("  {question}{suffixe} : "), "1")).trim().to_string();
        if !rep.is_empty() {
            return rep;
        }
        if !defaut.is_empty() {
            return defaut.to_string();
        }
        if !obligatoire {
            return String::new();
        }
        warn("Une valeur est requise.");
    }
}

/// Entree vide : valeur par defaut. Une valeur par defaut vide ne valide jamais
/// cette question : il faut une reponse.
pub fn ask(question: &str, defaut: &str) -> String {
    demander(question, defaut, true)
}

/// Question a laquelle Enter peut repondre vide, par exemple pour revenir.
pub fn ask_libre(question: &str, defaut: &str) -> String {
    demander(question, defaut, false)
}

/// Oui ou non ; Enter garde le choix en majuscule. Une autre reponse, une faute
/// de frappe par exemple, repose la question au lieu de valoir non.
pub fn ask_yn(question: &str, defaut: bool) -> bool {
    let d = if defaut { "O/n" } else { "o/N" };
    loop {
        let rep = saisie(&c(&format!("  {question} [{d}] : "), "1")).trim().to_lowercase();
        match rep.as_str() {
            "" => return defaut,
            "o" | "oui" | "y" | "yes" => return true,
            "n" | "non" | "no" => return false,
            _ => warn("Repondre o pour oui, n pour non, ou Enter."),
        }
    }
}

pub fn pause() {
    saisie(&c("\n  Enter pour continuer", "1"));
}

pub fn message_defaut() -> String {
    chrono::Local::now().format("Commit du %Y-%m-%d %H:%M").to_string()
}

/// Affiche les choix numerotes a partir de debut, puis 0 ; retourne le choix,
/// ou None pour Entree, 0 ou un choix invalide.
pub fn menu_numerote<T: Clone>(actions: &[(String, T)], debut: usize, sortie_: &str) -> Option<T> {
    for (i, (libelle, _)) in actions.iter().enumerate() {
        println!("    {}. {libelle}", i + debut);
    }
    println!("    0. {sortie_}");
    let choix = ask("Choix", "0");
    if let Ok(n) = choix.parse::<usize>() {
        if n >= debut && n < debut + actions.len() {
            return Some(actions[n - debut].1.clone());
        }
    }
    if !["0", "q", "quit", "exit"].contains(&choix.as_str()) {
        warn("Choix invalide.");
    }
    None
}

/// Jeton ou mot de passe saisi sans affichage ; vide si rien n'est saisi.
pub fn lire_secret(invite: &str) -> String {
    let invite = c(invite, "1");
    if std::io::stdin().is_terminal() {
        session::set_en_saisie(true);
        let rep = rpassword::prompt_password(invite).unwrap_or_default();
        session::set_en_saisie(false);
        return rep.trim().to_string();
    }
    print!("{invite}");
    sortie();
    let rep = lire_ligne(false).unwrap_or_default();
    println!();
    rep.trim().to_string()
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn oui_non_redemande_une_faute_de_frappe() {
        reponses_essai(&["oiu", "n"]);
        assert!(!ask_yn("Continuer ?", true));
        reponses_essai(&[""]);
        assert!(ask_yn("Continuer ?", true));
        reponses_essai(&["OUI"]);
        assert!(ask_yn("Continuer ?", false));
    }
}
