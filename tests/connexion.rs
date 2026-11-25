//! Connexion, fin de session, suppression du compte.
mod commun;
use commun::Banc;

#[test]
fn connexion_par_jeton_puis_titre_avec_le_nom_du_compte() {
    let b = Banc::new();
    let s = b.session(&["0", "0"]);
    assert!(s.texte.contains("Connexion OK : saleh"), "{}", s.texte);
    assert!(s.texte.contains("Repos GitHub de Saleh Kamil"), "{}", s.texte);
    assert!(s.fini(), "{}", s.texte);
}

#[test]
fn fin_de_l_entree_arret_propre() {
    let b = Banc::new();
    let s = b.lancer(&["1"]);
    assert!(s.texte.contains("Interrompu."), "{}", s.texte);
    assert_eq!(s.code, 0);
}

#[test]
fn logo_au_lancement_puis_a_chaque_retour_au_menu_principal() {
    let b = Banc::new();
    let s = b.session(&["0", "0"]);
    assert_eq!(s.texte.matches(concat!("gitmanager ", env!("CARGO_PKG_VERSION"))).count(), 2, "{}", s.texte);
    assert_eq!(s.texte.split("MENU PRINCIPAL").next().unwrap().matches("Coder chez soi").count(), 1);
    assert!(s.texte.contains("2. Gerer et synchroniser sur le poste Entreprise"));
}

#[test]
fn identifiants_en_clair_retires_pour_ce_compte_seulement() {
    let b = Banc::new();
    let fichier = b.maison.join(".git-credentials");
    std::fs::write(&fichier, "https://saleh:ghp_secret@github.com\nhttps://autre:t@github.com\n").unwrap();
    // au demarrage : garder le fichier ; option 1, jeton ; 5 supprimer les
    // identifiants, o ; Enter ; 0 quitter
    let s = b.lancer(&["n", "1", "2", "jeton", "5", "o", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert_eq!(std::fs::read_to_string(&fichier).unwrap(), "https://autre:t@github.com\n");
}

#[cfg(unix)]
#[test]
fn code_attendu_avant_le_navigateur_puis_compte_supprime_en_fin_de_programme() {
    let mut b = Banc::new();
    let etat = b.faux_gh();
    std::fs::write(&b.gitconfig, std::fs::read_to_string(&b.gitconfig).unwrap()
        .replace("\tname = Essai\n\temail = essai@example.com\n", "")).unwrap();
    // 1 ; navigateur 1 ; continuer : Enter ; code copie : Enter ; garde 1 ; 0 ; 0
    let s = b.lancer(&["1", "1", "", "", "1", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    let (code, nav) = (s.texte.find("ABCD-1234").unwrap(), s.texte.find("Ouverture dans le navigateur").unwrap());
    assert!(code < nav, "le navigateur s'ouvre apres la copie du code");
    assert!(s.texte.contains("Compte sur cette machine : pour cette session."));
    assert!(!etat.exists(), "plus de connexion dans GitHub CLI");
    let cfg = b.config();
    assert!(cfg.get("cles").is_none() && cfg.get("login").is_none(), "{cfg}");
    let nom = std::process::Command::new("git").args(["config", "--global", "user.name"])
        .env("GIT_CONFIG_GLOBAL", &b.gitconfig).output().unwrap();
    assert!(!nom.status.success(), "identite ecrite par gitmanager effacee");
}

#[cfg(unix)]
#[test]
fn fermer_la_session_en_gardant_le_compte() {
    let mut b = Banc::new();
    let etat = b.faux_gh();
    std::fs::write(&etat, "").unwrap();
    let mut cfg = b.config();
    for (k, v) in [("cles", "gh"), ("login", "saleh"), ("methode", "gh")] {
        cfg[k] = v.into();
    }
    cfg["enregistrer_connexion"] = true.into();
    b.ecrire_config(&cfg);
    // session ouverte au demarrage ; 1 ; 4 fermer ; 1 garder ; Enter ; 0
    let s = b.lancer(&["1", "4", "1", "", "0"]);
    assert!(s.texte.contains("Session ouverte : compte GitHub perso saleh."), "{}", s.texte);
    assert!(s.fini(), "{}", s.texte);
    assert!(etat.exists(), "le compte reste sur la machine");
    assert_eq!(b.config()["enregistrer_connexion"], false);
}
