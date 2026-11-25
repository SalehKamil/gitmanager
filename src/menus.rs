//! Menus : menu principal, repos GitHub du compte, entreprise.

use crate::affichage::{afficher_accueil, c, err, info, titre, warn};
use crate::config::Config;
use crate::connexion::{self, assurer_connexion, cles_enregistrees, texte_garde, traces_compte, verifier_gardes};
use crate::erreur::R;
use crate::github::host;
use crate::resultat::{executer, Resultat};
use crate::saisie::{ask, pause};
use crate::session::{self, nom_compte, nom_methode, PROFILS};
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Lance une action du menu ; une erreur inattendue ne ferme pas le programme.
pub fn lancer<T: Resultat>(f: impl FnOnce(&mut Config) -> R<T>, cfg: &mut Config, avec_pause: bool) {
    let res = catch_unwind(AssertUnwindSafe(|| executer(|| f(cfg))));
    let retour = match res {
        Ok(r) => r.is_some_and(|r| r.est_retour()),
        Err(e) => {
            err("Erreur inattendue dans le programme : relancer l'action.");
            if crate::debug() {
                let texte = e.downcast_ref::<String>().cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                err(&texte);
            }
            false
        }
    };
    if avec_pause && !retour {
        pause(); // Retour : rien a lire, pas de pause
    }
}

/// Nom du compte GitHub connecte, sinon son identifiant.
pub fn nom_affiche(user: &Option<Value>) -> String {
    let champ = |k: &str| user.as_ref().and_then(|u| u.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
    let nom = champ("name");
    if nom.is_empty() { champ("login") } else { nom }
}

pub fn ligne_compte(cfg: &Config) {
    println!("{}", c(&format!("  Compte GitHub {}, {}, {}", session::login().unwrap_or_default(),
                              nom_methode(&session::methode().unwrap_or_default()), host(cfg, None)), "36"));
}

/// Compte vise par un choix du menu principal : la question est posee
/// seulement quand plusieurs comptes sont concernes.
fn compte_a_choisir(noms: &[&'static str], action: &str) -> Option<&'static str> {
    if noms.len() <= 1 {
        return noms.first().copied();
    }
    println!();
    println!("  {action}, pour quel compte GitHub ?");
    for (i, nom) in noms.iter().enumerate() {
        let login = session::compte_de(nom).user.and_then(|u| u.get("login").and_then(Value::as_str).map(str::to_string));
        println!("    {}. Compte GitHub {}{}", i + 1, nom_compte(Some(nom)),
                 login.map(|l| format!(" : {l}")).unwrap_or_default());
    }
    println!("    0. Retour");
    let choix = ask("Choix", "0");
    choix.parse::<usize>().ok().filter(|n| (1..=noms.len()).contains(n)).map(|n| noms[n - 1])
}

/// Menu principal. Retourne le profil de l'option choisie, ou None pour quitter.
pub fn choisir_fonction(cfg: &mut Config) -> Option<&'static str> {
    loop {
        verifier_gardes(cfg, false, false);
        if !session::accueil() {
            afficher_accueil(); // au lancement, le logo vient d'etre affiche
        }
        session::set_accueil(false);
        titre("MENU PRINCIPAL");
        // Choix sous les deux options : Se connecter, pour un compte sans session
        let libres: Vec<&'static str> = PROFILS.iter().copied().filter(|n| session::compte_de(n).user.is_none()).collect();
        let (perso, pro) = (session::compte_de("perso").user, session::compte_de("entreprise").user);
        println!();
        println!("    1. {}", if perso.is_some() { format!("Repos GitHub de {}", nom_affiche(&perso)) }
                               else { "Gerer les repos GitHub".into() });
        println!("    2. Gerer et synchroniser sur le poste Entreprise{}",
                 if pro.is_some() { format!(", compte {}", nom_affiche(&pro)) } else { String::new() });
        if !libres.is_empty() {
            println!("    3. Se connecter");
        }
        println!("    0. Quitter");
        let choix = ask("Choix", "0");
        match choix.as_str() {
            "1" => return Some("perso"),
            "2" => return Some("entreprise"),
            "0" | "q" | "quit" | "exit" => return None,
            "3" if !libres.is_empty() => lancer(|cfg| {
                if let Some(nom) = compte_a_choisir(&libres, "Se connecter") {
                    session::activer_profil(nom);
                    assurer_connexion(cfg, true);
                    pause();
                }
                Ok(())
            }, cfg, false),
            _ => warn("Choix invalide."),
        }
    }
}

/// La connexion au compte GitHub de l'option passe toujours en premier : sans
/// elle, l'option ne s'ouvre pas.
pub fn connexion_obligatoire(cfg: &mut Config) -> bool {
    session::user().is_some() || assurer_connexion(cfg, true)
}

/// Repos GitHub en ligne, en local et compte, pour le compte actif.
pub fn menu_repos(cfg: &mut Config, nom_menu: fn(&str) -> String) {
    while !session::menu_principal() && connexion_obligatoire(cfg) {
        verifier_gardes(cfg, false, false);
        if session::menu_principal() {
            return;
        }
        titre(&nom_menu(&nom_affiche(&session::user())));
        ligne_compte(cfg);
        if cles_enregistrees(cfg, None).is_some() {
            let t = texte_garde(cfg, None);
            println!("{}", c(&format!("  Identifiants sur cette machine : {}", if t.is_empty() { "toujours" } else { &t }), "36"));
        }
        let identifiants = !traces_compte(cfg, None).is_empty();
        println!();
        println!("    1. Repos GitHub en ligne");
        println!("    2. Repos GitHub en local");
        println!("    3. Compte GitHub");
        println!("    4. Fermer la session");
        if identifiants {
            println!("    5. Supprimer les identifiants de cette machine");
        }
        println!("    0. Retour");
        let choix = ask("Choix", "0");
        match choix.as_str() {
            "0" | "q" | "quit" | "exit" => return,
            "4" => {
                if executer(|| connexion::deconnexion(cfg)) == Some(true) {
                    pause();
                }
            }
            "5" if identifiants => lancer(connexion::act_supprimer_identifiants, cfg, true),
            "1" => lancer(crate::en_ligne::act_depots, cfg, false),
            "2" => lancer(crate::local::act_depots_locaux, cfg, false),
            "3" => lancer(connexion::act_compte, cfg, true),
            _ => warn("Choix invalide."),
        }
    }
}

pub fn menu_perso(cfg: &mut Config) {
    menu_repos(cfg, |nom| format!("Repos GitHub de {nom}"));
}

pub fn menu_entreprise(cfg: &mut Config) {
    while !session::menu_principal() && connexion_obligatoire(cfg) {
        titre("Gerer et synchroniser sur le poste Entreprise");
        ligne_compte(cfg);
        println!();
        println!("    1. Importer un repo GitHub");
        println!("    2. Repos GitHub de l'entreprise");
        println!("    0. Retour");
        let choix = ask("Choix", "0");
        match choix.as_str() {
            "0" | "q" | "quit" | "exit" => return,
            "1" => lancer(crate::entreprise::act_importer, cfg, false),
            "2" => menu_repos(cfg, |nom| format!("Repos GitHub de l'entreprise, compte de {nom}")),
            _ => warn("Choix invalide."),
        }
    }
}

/// Le programme : accueil, outils, comptes de cette machine, menu principal.
pub fn principal() {
    let mut cfg = Config::charger();
    afficher_accueil();
    crate::config::migrer_ancienne_config(&mut cfg);
    if !crate::outils::preparer_outils(&mut cfg) {
        std::process::exit(1);
    }
    cfg.pop("voir_git");
    verifier_gardes(&mut cfg, true, false);
    for nom in PROFILS {
        // comptes de cette machine : session ouverte au demarrage
        session::activer_profil(nom);
        if cles_enregistrees(&cfg, None).is_some() && connexion::connexion_enregistree(&mut cfg) {
            crate::affichage::ok(&format!("Session ouverte : compte GitHub {} {}.", nom_compte(None),
                                          session::login().unwrap_or_default()));
        }
    }
    session::activer_profil("perso");
    loop {
        let option = choisir_fonction(&mut cfg);
        session::set_menu_principal(false);
        let Some(option) = option else { break };
        session::activer_profil(option);
        if !connexion_obligatoire(&mut cfg) {
            info("Pas de connexion : retour au menu principal.");
            continue;
        }
        if option == "perso" {
            menu_perso(&mut cfg);
        } else {
            menu_entreprise(&mut cfg);
        }
    }
    fin_des_sessions(&mut cfg);
    println!("\n  Fin du programme.\n");
}

/// Comptes pour cette session supprimes, jetons des deux comptes oublies.
pub fn fin_des_sessions(cfg: &mut Config) {
    verifier_gardes(cfg, false, true);
    for nom in PROFILS {
        session::vider_de(nom);
    }
}
