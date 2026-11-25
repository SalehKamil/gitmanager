//! Repos GitHub en ligne : liste, suppression, visibilite ; option 2, entreprise.
mod commun;
use commun::Banc;

/// Liste en ligne (option 1, jeton, 1), ces reponses, puis Enter, 0, 0.
fn en_ligne(b: &Banc, reponses: &[&str]) -> commun::Sortie {
    let mut tout = vec!["1"];
    tout.extend_from_slice(reponses);
    tout.extend_from_slice(&["", "0", "0"]);
    b.session(&tout)
}

/// Derniere liste affichee : du dernier "repo(s)" au choix qui suit.
fn derniere_liste(texte: &str) -> String {
    let fin = texte.rsplit_once("repo(s)").map(|x| x.1).unwrap_or("");
    fin.split("Choix").next().unwrap_or("").to_string()
}

#[test]
fn liste_sans_choix_du_compte_ni_clone_par_url() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let s = en_ligne(&b, &[]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("saleh/demo"));
    for absent in ["U. Cloner", "C. Changer de compte", "D. Fermer la session"] {
        assert!(!s.texte.contains(absent), "{absent}");
    }
}

#[test]
fn numero_hors_liste_pas_une_recherche() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let s = en_ligne(&b, &["15"]);
    assert!(s.texte.contains("Choix invalide : numero de 1 a 1."), "{}", s.texte);
    assert!(!s.texte.contains("contenant '15'"));
}

#[test]
fn colonne_en_local_avec_les_memes_copies_que_la_liste_locale() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    b.distant("autre", &[], &[]);
    let ailleurs = b.maison.join("Documents").join("demo");
    b.sh(&["git", "clone", "-q", "https://github.com/saleh/demo.git", ailleurs.to_str().unwrap()], None);
    let s = en_ligne(&b, &[]);
    let liste = derniere_liste(&s.texte);
    let ligne = |nom: &str| liste.lines().find(|l| l.contains(nom)).unwrap_or("").trim_end().to_string();
    assert!(ligne("saleh/demo").ends_with("oui"), "{liste}");
    assert!(!ligne("saleh/autre").ends_with("oui"), "{liste}");
}

#[test]
fn repo_supprime_ne_s_affiche_plus() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    b.distant("autre", &[], &[]);
    // 1 demo ; 8 supprimer ; nom ; Enter apres
    let s = en_ligne(&b, &["1", "8", "demo", ""]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("saleh/demo : suppression OK sur GitHub."));
    let liste = derniere_liste(&s.texte);
    assert!(!liste.contains("saleh/demo") && liste.contains("saleh/autre"), "{liste}");
}

#[test]
fn confirmation_avec_compte_slash_nom() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let s = en_ligne(&b, &["1", "8", "saleh/demo", ""]);
    assert!(s.texte.contains("suppression OK sur GitHub"), "{}", s.texte);
}

#[test]
fn copie_locale_gardee_sans_lien_apres_suppression() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    // 1 ; 8 ; nom ; copie : Enter = la garder sans lien ; Enter
    let s = en_ligne(&b, &["1", "8", "demo", "", ""]);
    assert!(s.fini(), "{}", s.texte);
    assert!(copie.is_dir());
    assert!(b.sh(&["git", "remote"], Some(&copie)).trim().is_empty(), "plus de lien");
}

#[test]
fn sans_droit_admin_aucune_tentative() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    b.github.lock().unwrap().ajouter("demo", false);
    let s = en_ligne(&b, &["1", "8", ""]);
    assert!(s.texte.contains("il faut le role admin"), "{}", s.texte);
    assert!(!b.github.lock().unwrap().appels.iter().any(|(v, _)| v == "DELETE"));
}

#[test]
fn deja_supprime_sur_github() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    // le repo disparait de GitHub juste apres son affichage
    b.github.lock().unwrap().jetons_admin = Some(Default::default());
    b.github.lock().unwrap().disparait = Some("saleh/demo".into());
    let s = en_ligne(&b, &["1", "8", "demo", ""]);
    assert!(s.texte.contains("n'existe plus sur GitHub"), "{}", s.texte);
    assert!(!s.texte.contains("permission de supprimer"));
}

#[test]
fn jeton_pour_cette_suppression_seulement() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    b.github.lock().unwrap().jetons_admin = Some(["jeton-suppression".to_string()].into());
    // 1 ; 8 ; nom ; 1 saisir un jeton ; le jeton ; Enter
    let s = en_ligne(&b, &["1", "8", "demo", "1", "jeton-suppression", ""]);
    assert!(s.texte.contains("suppression OK sur GitHub"), "{}", s.texte);
    assert!(!s.texte.contains("jeton-suppression"), "le jeton ne s'affiche pas");
}

#[test]
fn repo_recree_avec_le_meme_nom_visible() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let dossier = b.espace.join("demo");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("a.txt"), "a").unwrap();
    // suppression, puis liste locale : n, dossier, 3 git init + nouveau repo,
    // .gitignore n, nom/description/organisation/public Enter, message Enter, Enter, 0
    let s = b.session(&["1", "1", "8", "demo", "", "", "2", "n", dossier.to_str().unwrap(), "2", "n", "", "", "",
                        "", "", "", "0", "", "1", "", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(derniere_liste(&s.texte).contains("saleh/demo"), "{}", s.texte);
}

#[test]
fn rendre_public_puis_non_public() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    // 1 ; 3 rendre public ; o ; Enter ; 3 rendre non public ; Enter oui ; Enter ; 0
    let s = en_ligne(&b, &["1", "3", "o", "", "3", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("saleh/demo est maintenant public."));
    assert!(s.texte.contains("saleh/demo est maintenant non public."));
}

#[cfg(unix)]
#[test]
fn permission_delete_repo_demandee_avec_github_cli() {
    let mut b = Banc::new();
    let etat = b.faux_gh();
    std::fs::write(&etat, "").unwrap();
    b.distant("demo", &[], &[]);
    b.github.lock().unwrap().jetons_admin = Some(["jeton-gh".to_string()].into());
    let mut cfg = b.config();
    for (k, v) in [("cles", "gh"), ("login", "saleh"), ("methode", "gh")] {
        cfg[k] = v.into();
    }
    b.ecrire_config(&cfg);
    // session ouverte au demarrage ; 1 ; 1 en ligne ; 1 demo ; 8 ; nom ; la demander :
    // Enter ; code copie : Enter ; Enter ; Enter ; 0 ; 0
    let s = b.lancer(&["1", "1", "1", "8", "demo", "", "", "", "", "0", "0"]);
    assert!(s.texte.contains("suppression OK sur GitHub"), "{}", s.texte);
}

/// Option 2 : connexion du compte de l'entreprise par jeton, puis ces reponses.
fn entreprise(b: &Banc, reponses: &[&str]) -> commun::Sortie {
    let mut tout = vec!["2", "2", "jeton"];
    tout.extend_from_slice(reponses);
    tout.extend_from_slice(&["0", "0"]);
    b.lancer(&tout)
}

#[test]
fn import_puis_envoi_dans_le_repo_de_l_entreprise() {
    let b = Banc::new();
    b.distant("demo", &[("code.py", "print('perso')\n")], &[]);
    let source = b.copie("demo");
    b.distant("entre", &[("README.md", "# entreprise\n")], &[]);
    // 1 importer ; dossier source ; 1 envoyer ; repo 2 = entre ; 1 ajouter ;
    // appliquer : Enter ; message : Enter ; Enter ; 0 retour du projet
    let s = entreprise(&b, &["1", source.to_str().unwrap(), "1", "2", "1", "", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("push : 1 commit(s)"), "{}", s.texte);
    let verif = b.tmp.join("verif");
    b.sh(&["git", "clone", "-q", "https://github.com/saleh/entre.git", verif.to_str().unwrap()], None);
    assert!(verif.join("code.py").exists() && verif.join("README.md").exists());
}

#[test]
fn import_dossier_introuvable_nouvel_essai() {
    let b = Banc::new();
    let absent = b.tmp.join("absent");
    let s = entreprise(&b, &["1", absent.to_str().unwrap(), ""]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Dossier introuvable") && s.texte.contains("essai 1 sur 5"));
}
