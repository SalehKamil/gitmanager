//! Comparer en ligne et en local : tableau des differences, fichiers avec
//! changements, puis les deux versions d'un fichier cote a cote.

use crate::affichage::{c, cote_a_cote, info, largeur_terminal, ok, tableau, taille, titre, warn};
use crate::config::Config;
use crate::erreur::{Arret, R};
use crate::git::{a_des_commits, branche_courante, git, git_brut, lignes_statut, nb_commits, ref_existe, remote_principal};
use crate::projet::recuperer;
use crate::resultat::Res;
use crate::saisie::{ask_libre, pause};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// Lignes d'un texte, comme splitlines : \n, \r\n ou \r.
pub fn decouper(texte: &str) -> Vec<String> {
    let mut lignes = vec![];
    let mut courante = String::new();
    let mut car = texte.chars().peekable();
    while let Some(ch) = car.next() {
        match ch {
            '\n' => lignes.push(std::mem::take(&mut courante)),
            '\r' => {
                if car.peek() == Some(&'\n') {
                    car.next();
                }
                lignes.push(std::mem::take(&mut courante));
            }
            _ => courante.push(ch),
        }
    }
    if !courante.is_empty() {
        lignes.push(courante);
    }
    lignes
}

/// Lignes d'un fichier texte, ou None pour un binaire, un lien ou un trop gros fichier.
pub fn lignes_texte(fichier: &Path) -> Option<Vec<String>> {
    let m = std::fs::symlink_metadata(fichier).ok()?;
    if m.file_type().is_symlink() || m.len() > 2_000_000 {
        return None;
    }
    let data = std::fs::read(fichier).ok()?;
    if data.contains(&0) {
        return None;
    }
    Some(decouper(&String::from_utf8_lossy(&data)))
}

/// Lignes du fichier dans ref, ou None pour un binaire.
pub fn contenu_en_ligne(chemin: &Path, r: &str, fichier: &str) -> Option<Vec<String>> {
    let (code, out) = git_brut(&["show", &format!("{r}:{fichier}")], Some(chemin));
    if code != 0 || out.contains(&0) {
        return None;
    }
    Some(decouper(&String::from_utf8_lossy(&out)))
}

pub struct InfosCommit {
    pub court: String,
    pub epoch: i64,
    pub date: String,
    pub auteur: String,
    pub message: String,
}

/// Dernier commit de ref.
pub fn infos_commit(chemin: &Path, r: &str) -> Option<InfosCommit> {
    let (code, out, _) = git(&["log", "-1", "--format=%h%x1f%ct%x1f%cd%x1f%an%x1f%s", "--date=format:%Y-%m-%d %H:%M", r],
                             Some(chemin));
    if code != 0 || out.is_empty() {
        return None;
    }
    let mut p = out.split('\x1f').map(str::to_string).chain(std::iter::repeat(String::new()));
    Some(InfosCommit { court: p.next()?, epoch: p.next()?.parse().unwrap_or(0), date: p.next()?,
                       auteur: p.next()?, message: p.next()? })
}

/// {fichier: taille} des fichiers de ref.
pub fn fichiers_en_ligne(chemin: &Path, r: &str) -> BTreeMap<String, u64> {
    let (_, out, _) = git(&["ls-tree", "-r", "-l", "-z", r], Some(chemin));
    let mut fichiers = BTreeMap::new();
    for rec in out.split('\0') {
        let (meta, nom) = rec.split_once('\t').unwrap_or((rec, ""));
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if !nom.is_empty() && parts.len() == 4 && parts[1] == "blob" {
            fichiers.insert(nom.to_string(), parts[3].parse().unwrap_or(0));
        }
    }
    fichiers
}

/// {fichier: taille} des fichiers du dossier, suivis par Git ou nouveaux, sans
/// les fichiers ignores.
pub fn fichiers_en_local(chemin: &Path) -> BTreeMap<String, u64> {
    let (_, out, _) = git(&["ls-files", "--cached", "--others", "--exclude-standard", "-z"], Some(chemin));
    let mut fichiers = BTreeMap::new();
    for nom in out.split('\0').filter(|f| !f.is_empty()) {
        let complet = chemin.join(nom);
        let Ok(m) = std::fs::symlink_metadata(&complet) else { continue };
        if m.is_dir() {
            continue; // sous-repo git : pas un fichier, comme en ligne
        }
        fichiers.insert(nom.to_string(), m.len());
    }
    fichiers
}

/// {fichier: code XY de git status} des fichiers au commit manquant.
pub fn etats_locaux(chemin: &Path) -> HashMap<String, String> {
    lignes_statut(chemin).into_iter().map(|l| (l[3..].to_string(), l[..2].to_string())).collect()
}

/// Fichiers dont le contenu differe entre ref et le dossier, fichiers sans
/// commit compris : (fichier, present en ligne, present en local).
pub fn ecarts_fichiers(chemin: &Path, r: &str) -> Vec<(String, bool, bool)> {
    let (_, ns, _) = git(&["diff", "--no-renames", "--name-only", "-z", "--ignore-submodules=all", r, "--"], Some(chemin));
    let (_, autres, _) = git(&["ls-files", "--others", "--exclude-standard", "-z"], Some(chemin));
    // "dossier/" : un repo git dans ce repo, hors comparaison comme les sous-repos
    let mut noms: BTreeSet<String> = ns.split('\0').filter(|f| !f.is_empty()).map(str::to_string).collect();
    noms.extend(autres.split('\0').filter(|f| !f.is_empty() && !f.ends_with('/')).map(str::to_string));
    let en_ligne = fichiers_en_ligne(chemin, r);
    let mut noms: Vec<String> = noms.into_iter().collect();
    noms.sort_by_key(|f| f.to_lowercase());
    let mut ecarts = vec![];
    for f in noms {
        let (present_ligne, present_local) = (en_ligne.contains_key(&f), chemin.join(&f).symlink_metadata().is_ok());
        if present_ligne && present_local {
            // meme texte, seules les fins de ligne changent : Windows, CRLF
            let gauche = contenu_en_ligne(chemin, r, &f);
            if gauche.is_some() && gauche == lignes_texte(&chemin.join(&f)) {
                continue;
            }
        }
        ecarts.push((f, present_ligne, present_local));
    }
    ecarts
}

/// Noms des repos git dans ce repo : lien en ligne, mode 160000, ou dossier avec
/// son propre .git sur cette machine.
pub fn sous_repos(chemin: &Path, r: &str) -> Vec<String> {
    let (_, out, _) = git(&["ls-tree", "-r", "-z", r], Some(chemin));
    let mut noms: BTreeSet<String> = out.split('\0').filter(|rec| rec.starts_with("160000 "))
        .filter_map(|rec| rec.split_once('\t').map(|(_, n)| n.to_string())).collect();
    let (_, autres, _) = git(&["ls-files", "--others", "--exclude-standard", "--directory", "-z"], Some(chemin));
    noms.extend(autres.split('\0').filter(|f| f.ends_with('/') && chemin.join(f).join(".git").exists())
        .map(|f| f.trim_end_matches('/').to_string()));
    noms.into_iter().filter(|n| !n.is_empty()).collect()
}

/// Date du dernier commit qui a touche ce fichier dans ref : (texte, epoch).
pub fn date_fichier(chemin: &Path, r: &str, fichier: &str) -> (String, i64) {
    let (_, out, _) = git(&["log", "-1", "--format=%ct %cd", "--date=format:%Y-%m-%d %H:%M", r, "--", fichier], Some(chemin));
    let (epoch, texte) = out.trim().split_once(' ').unwrap_or((out.trim(), ""));
    (texte.to_string(), epoch.parse().unwrap_or(0))
}

pub fn case_fichier(nb_lignes: Option<usize>, taille_octets: u64, date_texte: &str, etat: &str) -> String {
    let debut = match nb_lignes {
        None => format!("binaire, {}", taille(taille_octets)),
        Some(n) => format!("{n} ligne{}", if n > 1 { "s" } else { "" }),
    };
    [debut.as_str(), date_texte, etat].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(", ")
}

enum Case {
    Absent(&'static str),
    Fichier(Option<usize>, u64, String, &'static str),
}

/// Tableau de comparaison entre la copie en ligne, sur la meme branche que cette
/// machine, ou sinon sa branche principale, et la copie sur cette machine, puis
/// les fichiers differents et leurs deux versions cote a cote. Retour quand
/// Entree ramene a l'ecran du repo.
pub fn comparer_copies(cfg: &mut Config, d: &Value, chemin: &Path) -> R<Res> {
    titre("Comparer en ligne et en local");
    let Some(remote) = remote_principal(chemin) else {
        return Err(Arret::new("ignore", "la copie sur cette machine n'a aucun lien avec un repo en ligne"));
    };
    let ici = branche_courante(chemin);
    info("Lecture de la copie en ligne...");
    recuperer(cfg, chemin, &remote, false)?;
    // meme branche des deux cotes quand elle existe en ligne
    let branche = match &ici {
        Some(b) if ref_existe(chemin, &format!("refs/remotes/{remote}/{b}")) => b.clone(),
        _ => d.get("default_branch").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
            .or(ici.clone()).unwrap_or_else(|| "main".into()),
    };
    let r = format!("{remote}/{branche}");
    if !ref_existe(chemin, &format!("refs/remotes/{r}")) {
        return Err(Arret::new("ignore", format!("la branche {branche} n'existe pas en ligne")));
    }
    let ligne_c = infos_commit(chemin, &r);
    let local_c = if a_des_commits(chemin) { infos_commit(chemin, "HEAD") } else { None };
    let (f_ligne, f_local) = (fichiers_en_ligne(chemin, &r), fichiers_en_local(chemin));
    let etats = etats_locaux(chemin);
    let seulement_ligne = if local_c.is_some() { nb_commits(chemin, &format!("HEAD..{r}")) } else { nb_commits(chemin, &r) };
    let seulement_local = if local_c.is_some() { nb_commits(chemin, &format!("{r}..HEAD")) } else { 0 };
    let vide = || InfosCommit { court: "-".into(), epoch: 0, date: "-".into(), auteur: "-".into(), message: "aucun commit".into() };
    let (lc, ll) = (ligne_c.unwrap_or_else(vide), local_c.as_ref().map(|x| InfosCommit {
        court: x.court.clone(), epoch: x.epoch, date: x.date.clone(), auteur: x.auteur.clone(), message: x.message.clone() }).unwrap_or_else(vide));
    let s = |x: &dyn ToString| x.to_string();
    let mut lignes: Vec<Vec<String>> = vec![
        vec![s(&"Branche"), branche.clone(), ici.clone().unwrap_or_else(|| "-".into())],
        vec![s(&"Dernier commit"), lc.court.clone(), ll.court.clone()],
        vec![s(&"Message"), lc.message.clone(), ll.message.clone()],
        vec![s(&"Date du dernier commit"), lc.date.clone(), ll.date.clone()],
        vec![s(&"Auteur"), lc.auteur.clone(), ll.auteur.clone()],
        vec![s(&"Copie la plus neuve"), s(&"-"), s(&"-")],
        vec![s(&"Commits"), s(&nb_commits(chemin, &r)), s(&if local_c.is_some() { nb_commits(chemin, "HEAD") } else { 0 })],
        vec![s(&"Commits en avance"), s(&seulement_ligne), s(&seulement_local)],
        vec![s(&"Fichiers"), s(&f_ligne.len()), s(&f_local.len())],
        vec![s(&"Taille"), taille(f_ligne.values().sum()), taille(f_local.values().sum())],
        vec![s(&"Fichiers, commit manquant"), s(&"-"), s(&etats.len())],
    ];
    let ecarts = ecarts_fichiers(chemin, &r);
    println!();
    let sous = sous_repos(chemin, &r);
    if !sous.is_empty() {
        info(&format!("Hors comparaison, {} sous-repo(s) git : {}.", sous.len(), sous.join(", ")));
        info("Leur contenu ne part pas avec ce repo : GitHub les montre en dossiers vides.");
        println!();
    }
    if ecarts.is_empty() && seulement_ligne == 0 && seulement_local == 0 {
        ok("Les deux copies sont pareilles : aucune difference en local et en ligne.");
        return Ok(Res::Rien);
    }
    let mut plus_recent = 0;
    if lc.court != ll.court && lc.epoch != ll.epoch {
        plus_recent = if lc.epoch > ll.epoch { 1 } else { 2 };
        lignes[5][plus_recent] = "oui".into();
    }
    // seules les lignes qui different s'affichent
    lignes.retain(|l| l[1] != l[2] && !((l[1] == "-" && l[2] == "0") || (l[1] == "0" && l[2] == "-")));
    let mut couleurs = HashMap::new();
    for (n, l) in lignes.iter().enumerate() {
        for col in [1, 2] {
            if plus_recent == col && (l[0] == "Date du dernier commit" || l[0] == "Copie la plus neuve") {
                couleurs.insert((n, col), "1;32".to_string());
            } else if (l[0] == "Commits en avance" || l[0] == "Fichiers, commit manquant") && l[col] != "0" && l[col] != "-" {
                couleurs.insert((n, col), "33".to_string());
            }
        }
    }
    tableau(&["Difference".into(), "En ligne".into(), "En local".into()], &lignes, &couleurs);
    println!();
    if seulement_ligne > 0 {
        info(&format!("En ligne : {seulement_ligne} commit(s) en avance, pour les avoir : Recevoir, pull."));
    }
    if seulement_local > 0 {
        info(&format!("En local : {seulement_local} commit(s) en avance, pour les mettre en ligne : Envoyer."));
    }
    if !etats.is_empty() {
        info(&format!("En local : {} fichier(s), commit manquant.", etats.len()));
    }
    if ecarts.is_empty() {
        ok("Les fichiers sont pareils dans les deux copies, seuls les commits changent.");
        return Ok(Res::Rien);
    }
    let mut brut = vec![];
    let mut recents = HashMap::new();
    for (n, (fichier, present_ligne, present_local)) in ecarts.iter().enumerate() {
        let (mut quand_ligne, mut quand_local) = (0, 0);
        let d_ligne = if *present_ligne {
            let texte = contenu_en_ligne(chemin, &r, fichier);
            let (date_texte, q) = date_fichier(chemin, &r, fichier);
            quand_ligne = q;
            Case::Fichier(texte.map(|t| t.len()), *f_ligne.get(fichier).unwrap_or(&0), date_texte, "")
        } else {
            Case::Absent("absent")
        };
        let xy = etats.get(fichier).cloned().unwrap_or_default();
        let d_local = if *present_local {
            let complet = chemin.join(fichier);
            let texte = lignes_texte(&complet);
            let (date_texte, etat) = if !xy.is_empty() {
                quand_local = std::fs::symlink_metadata(&complet).ok().and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
                let date = chrono::DateTime::from_timestamp(quand_local, 0)
                    .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
                (date, if xy == "??" { "nouveau, commit manquant" } else { "commit manquant" })
            } else {
                let (t, q) = date_fichier(chemin, "HEAD", fichier);
                quand_local = q;
                (t, "")
            };
            Case::Fichier(texte.map(|t| t.len()), *f_local.get(fichier).unwrap_or(&0), date_texte, etat)
        } else {
            Case::Absent(if xy.contains('D') { "absent, commit manquant" } else { "absent" })
        };
        if quand_ligne != 0 && quand_local != 0 && quand_ligne != quand_local {
            recents.insert((n, if quand_ligne > quand_local { 1 } else { 2 }), "32".to_string());
        }
        brut.push((fichier.clone(), d_ligne, d_local));
    }
    let cases_fichiers = |heure: bool| -> Vec<Vec<String>> {
        let case = |dn: &Case| match dn {
            Case::Absent(t) => t.to_string(),
            Case::Fichier(nb, t, quand, etat) => {
                let q: String = if heure { quand.clone() } else { quand.chars().take(10).collect() };
                case_fichier(*nb, *t, &q, etat)
            }
        };
        brut.iter().map(|(f, a, b)| vec![f.clone(), case(a), case(b)]).collect()
    };
    let mut cases = cases_fichiers(true);
    let larg0 = cases.iter().map(|cs| format!("{}. ", cases.len()).len() + cs[0].chars().count()).max().unwrap_or(0);
    let larg = |i: usize| cases.iter().map(|cs| cs[i].chars().count()).max().unwrap_or(0);
    if larg0 + larg(1) + larg(2) + 4 > largeur_terminal() - 4 {
        cases = cases_fichiers(false); // dates sans l'heure pour tenir dans le terminal
    }
    loop {
        println!();
        info(&format!("Fichiers avec changements : {}", ecarts.len()));
        println!();
        let lignes: Vec<Vec<String>> = cases.iter().take(200).enumerate()
            .map(|(i, cs)| vec![format!("{}. {}", i + 1, cs[0]), cs[1].clone(), cs[2].clone()]).collect();
        tableau(&["Fichier".into(), "En ligne".into(), "En local".into()], &lignes, &recents);
        println!("    En vert : la version la plus neuve du fichier.");
        if cases.len() > 200 {
            println!("         ... et {} autre(s)", cases.len() - 200);
        }
        println!();
        let choix = ask_libre("Choix : # du fichier pour voir ses deux versions, Enter pour revenir", "");
        if choix.is_empty() {
            return Ok(Res::Retour);
        }
        let Some(n) = choix.parse::<usize>().ok().filter(|n| (1..=cases.len().min(200)).contains(n)) else {
            warn("Choix invalide.");
            continue;
        };
        let (fichier, c_ligne, c_local) = (&cases[n - 1][0], &cases[n - 1][1], &cases[n - 1][2]);
        let (_, present_ligne, present_local) = &ecarts[n - 1];
        let gauche = if *present_ligne { contenu_en_ligne(chemin, &r, fichier) } else { Some(vec![]) };
        let droite = if *present_local { lignes_texte(&chemin.join(fichier)) } else { Some(vec![]) };
        println!();
        println!("{}", c(&format!("  {fichier}"), "1;36"));
        match (gauche, droite) {
            (Some(g), Some(d)) => {
                info("En jaune : lignes avec changements. Case vide : ligne absente de ce fichier.");
                println!();
                cote_a_cote(&g, &d, c_ligne, c_local, ("En ligne", "En local"));
            }
            _ => {
                info("Fichier binaire : pas de comparaison ligne par ligne.");
                info(&format!("En ligne : {c_ligne}"));
                info(&format!("En local : {c_local}"));
            }
        }
        pause();
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn lignes_comme_splitlines() {
        assert_eq!(decouper("a\r\nb\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(decouper("a\n\n"), vec!["a", ""]);
        assert_eq!(decouper(""), Vec::<String>::new());
    }

    #[test]
    fn case_d_un_fichier() {
        assert_eq!(case_fichier(Some(3), 9, "2026-11-25 01:17", ""), "3 lignes, 2026-11-25 01:17");
        assert_eq!(case_fichier(Some(1), 9, "", "nouveau, commit manquant"), "1 ligne, nouveau, commit manquant");
        assert_eq!(case_fichier(None, 2048, "", ""), "binaire, 2.0 Ko");
    }
}
