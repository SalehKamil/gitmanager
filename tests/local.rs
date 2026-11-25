//! Repos GitHub en local : envoyer, recevoir, branches, annuler, stash, comparer.
mod commun;
use commun::Banc;

/// Ouvre la liste locale (option 1, jeton, 2), puis ces reponses, puis Enter
/// pour revenir et 0, 0 pour quitter.
fn local(b: &Banc, reponses: &[&str]) -> commun::Sortie {
    let mut tout = vec!["2"];
    tout.extend_from_slice(reponses);
    tout.extend_from_slice(&["", "0", "0"]);
    b.session(&tout)
}

#[test]
fn aucune_difference_apres_le_pull() {
    let b = Banc::new();
    let travail = b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    b.autre_push(&travail, "a.txt", "nouveau\n");
    // repo 1 ; 2 recevoir ; Enter ; 3 voir ; 1 commits ; Enter ; 2 differences ; Enter ; 0 ; 0
    let s = local(&b, &["1", "2", "", "3", "1", "", "2", "", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(copie.join("a.txt").exists());
    let apres = s.texte.split("Pull : 1 nouveau(s)").nth(1).expect("pull");
    assert!(!apres.contains("push : 0") && !apres.contains("pull : 0"), "{apres}");
    assert!(apres.contains("pareil en local et en ligne"));
    assert!(apres.contains("Commits pareils en local et en ligne"));
    assert!(apres.contains("Les deux copies sont pareilles"));
}

#[test]
fn statut_montre_seulement_les_ecarts() {
    let b = Banc::new();
    let travail = b.distant("demo", &[], &[]);
    b.copie("demo");
    b.autre_push(&travail, "a.txt", "nouveau\n");
    let s = local(&b, &["1", "0"]);
    assert!(s.texte.contains("Statut  : pull : 1 commit(s)"), "{}", s.texte);
    assert!(!s.texte.contains("push : 0"));
}

#[test]
fn envoyer_commit_et_push() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    std::fs::write(copie.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(copie.join("b.txt"), "b\n").unwrap();
    // 1 envoyer ; message : Enter ; Enter ; 0
    let s = local(&b, &["1", "1", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("$ git push -u origin main"), "{}", s.texte);
    let log = b.sh(&["git", "log", "--format=%s", "-1", "origin/main"], Some(&copie));
    assert!(log.starts_with("Commit du"), "{log}");
}

#[test]
fn parcourir_une_branche_en_ligne() {
    let b = Banc::new();
    b.distant("demo", &[], &[("feature", "f.txt", "ligne de la branche\n")]);
    b.copie("demo");
    // 1 ; 4 branches ; 2 feature ; 1 parcourir ; 1 f.txt ; Enter ; Enter ; Enter ; 0
    let s = local(&b, &["1", "4", "2", "1", "1", "", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("ligne de la branche"));
    assert!(s.texte.contains("Branche feature, en ligne : /"));
}

#[test]
fn supprimer_une_branche_seulement_en_ligne() {
    let b = Banc::new();
    b.distant("demo", &[], &[("feature", "f.txt", "x\n")]);
    let copie = b.copie("demo");
    // 1 ; 4 ; 2 feature ; 5 supprimer ; o ; Enter ; Enter ; 0
    let s = local(&b, &["1", "4", "2", "5", "o", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Supprimer la branche feature en ligne ? Sans retour."));
    assert!(!b.sh(&["git", "ls-remote", "--heads", "origin"], Some(&copie)).contains("feature"));
}

fn commit_en_ligne(b: &Banc) -> std::path::PathBuf {
    b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    std::fs::write(copie.join("b.txt"), "b\n").unwrap();
    b.sh(&["git", "add", "-A"], Some(&copie));
    b.sh(&["git", "commit", "-q", "-m", "Ajout de b"], Some(&copie));
    b.sh(&["git", "push", "-q"], Some(&copie));
    copie
}

#[test]
fn annuler_un_commit_en_ligne() {
    let b = Banc::new();
    let copie = commit_en_ligne(&b);
    // 1 ; 5 annuler ; 2 dernier commit ; Enter oui ; Enter ; 0 ; 0
    let s = local(&b, &["1", "5", "2", "", "", "0", "0"]);
    assert!(s.texte.contains("Annulation du commit OK, ici et en ligne"), "{}", s.texte);
    let log = b.sh(&["git", "log", "--format=%s", "-1", "origin/main"], Some(&copie));
    assert!(log.starts_with("Annulation du commit"), "{log}");
    assert!(!copie.join("b.txt").exists());
}

#[test]
fn travail_en_cours_hors_de_l_annulation() {
    let b = Banc::new();
    let copie = commit_en_ligne(&b);
    std::fs::write(copie.join("c.txt"), "en cours\n").unwrap();
    b.sh(&["git", "add", "c.txt"], Some(&copie));
    let s = local(&b, &["1", "5", "2", "", "", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    let fichiers = b.sh(&["git", "show", "--name-only", "--format=", "origin/main"], Some(&copie));
    assert!(!fichiers.contains("c.txt"), "le travail en cours ne doit pas partir");
    assert!(copie.join("c.txt").exists());
}

#[test]
fn annulation_dont_le_push_echoue() {
    let b = Banc::new();
    let copie = commit_en_ligne(&b);
    let nu = b.distants.join("saleh").join("demo.git");
    std::fs::rename(&nu, nu.with_extension("ailleurs")).unwrap(); // serveur injoignable
    let s = local(&b, &["1", "5", "2", "", "", "0", "0"]);
    assert!(s.texte.contains("Annulation faite ici seulement"), "{}", s.texte);
    assert!(b.sh(&["git", "log", "-1", "--format=%s"], Some(&copie)).starts_with("Annulation du commit"));
}

#[test]
fn changements_pour_plus_tard() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    std::fs::write(copie.join("README.md"), "en cours\n").unwrap();
    // 1 ; 6 ; m ; nom : Enter ; Enter ; 1 ; 1 reprendre ; Enter ; Enter ; 0
    let s = local(&b, &["1", "6", "m", "", "", "1", "1", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Rangement OK") && s.texte.contains("Changements repris"));
    assert_eq!(std::fs::read_to_string(copie.join("README.md")).unwrap(), "en cours\n");
}

#[test]
fn comparer_avec_des_ecarts() {
    let b = Banc::new();
    let travail = b.distant("demo", &[("a.txt", "1\n2\n3\n")], &[]);
    let copie = b.copie("demo");
    b.autre_push(&travail, "a.txt", "1\nDEUX\n3\n");
    std::fs::write(copie.join("b.txt"), "local\n").unwrap();
    // 1 ; 3 ; 2 differences ; fichier 1 ; Enter ; Enter ; 0 ; 0
    let s = local(&b, &["1", "3", "2", "1", "", "", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Commits en avance") && s.texte.contains("DEUX") && s.texte.contains("b.txt"));
}

#[test]
fn cinq_essais_puis_un_niveau_en_arriere() {
    let b = Banc::new();
    let faux: Vec<String> = (0..5).map(|i| b.tmp.join(format!("absent{i}")).display().to_string()).collect();
    let mut r = vec!["n"];
    r.extend(faux.iter().map(String::as_str));
    let s = local(&b, &r);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("essai 4 sur 5") && s.texte.contains("5 essais sans dossier : retour."));
    assert_eq!(s.texte.matches("\n  Repos GitHub en local\n").count(), 2, "retour d'un seul niveau");
}

#[test]
fn zero_pour_revenir_quand_un_dossier_est_propose() {
    let b = Banc::new();
    let s = local(&b, &["n", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert_eq!(s.texte.matches("\n  Repos GitHub en local\n").count(), 2);
}

#[test]
fn nouveau_repo_sans_question_en_double() {
    let b = Banc::new();
    let dossier = b.espace.join("nouveau");
    std::fs::create_dir_all(&dossier).unwrap();
    std::fs::write(dossier.join("main.py"), "print(1)\n").unwrap();
    // n ; dossier ; 2 git init + nouveau ; .gitignore : n ; nom, description,
    // organisation, public : Enter ; message : Enter ; Enter ; 0 retour du repo
    let s = local(&b, &["n", dossier.to_str().unwrap(), "2", "n", "", "", "", "", "", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert_eq!(s.texte.matches("Ajouter un .gitignore").count(), 1, "{}", s.texte);
    assert!(b.sh(&["git", "ls-remote", "--heads", "origin"], Some(&dossier)).contains("main"));
}

#[test]
fn lier_avec_le_repo_du_meme_nom_sans_redemander() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let dossier = b.espace.join("demo");
    std::fs::create_dir_all(&dossier).unwrap();
    // n ; dossier ; 1 lier avec saleh/demo ; Enter ; 0 retour du repo
    let s = local(&b, &["n", dossier.to_str().unwrap(), "1", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(!s.texte.contains("compte/nom ou URL"));
    assert!(dossier.join("README.md").exists());
}

#[test]
fn tout_envoyer() {
    let b = Banc::new();
    for nom in ["un", "deux"] {
        b.distant(nom, &[], &[]);
        std::fs::write(b.copie(nom).join("x.txt"), nom).unwrap();
    }
    // t ; dossier propose : Enter ; 1 tout envoyer ; message : Enter ; Enter
    let s = local(&b, &["t", "", "1", "", ""]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Bilan : 2 OK"), "{}", s.texte);
}

#[test]
fn chemins_avec_espaces() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    let copie = b.copie("demo");
    std::fs::create_dir_all(copie.join("sous repo")).unwrap();
    b.sh(&["git", "init", "-q", copie.join("sous repo").to_str().unwrap()], None);
    std::fs::write(copie.join("a b.txt"), "x\n").unwrap();
    // 1 ; 5 annuler ; 1 un fichier ; 1 a b.txt ; o ; Enter ; 0 ; 0
    let s = local(&b, &["1", "5", "1", "1", "o", "", "0", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Statut  : 1 fichier(s), commit manquant"), "{}", s.texte);
    assert!(!copie.join("a b.txt").exists());
    assert!(!s.texte.contains("git rm"));
}

#[test]
fn cloner_par_url_dans_la_liste_locale() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    // u ; adresse sans https ; dossier propose : Enter ; 0 retour du repo
    let s = local(&b, &["u", "github.com/saleh/demo", "", "0"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("U. Cloner un repo par son URL : git clone"));
    assert!(b.espace.join("demo").join(".git").is_dir());
}

#[test]
fn adresse_http_refusee_sans_quitter_la_liste() {
    let b = Banc::new();
    let s = local(&b, &["u", "http://github.com/saleh/demo"]);
    assert!(s.fini(), "{}", s.texte);
    assert!(s.texte.contains("Adresse refusee"));
    assert_eq!(s.texte.matches("\n  Repos GitHub en local\n").count(), 2);
}

#[test]
fn menus_nomment_les_commandes_git() {
    let b = Banc::new();
    b.distant("demo", &[], &[]);
    b.copie("demo");
    let s = local(&b, &["1", "0"]);
    for libelle in ["commit + push", "Recevoir : pull", "log, diff", "branch, switch, merge",
                    "restore, revert", "git stash", "force push", " : remote"] {
        assert!(s.texte.contains(libelle), "{libelle}");
    }
}
