//! Affichage : couleurs, titres, messages, logo, tableaux, cote a cote.

use similar::{capture_diff_slices, group_diff_ops, Algorithm, DiffTag};
use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

static COULEUR: OnceLock<bool> = OnceLock::new();

fn env_non_vide(nom: &str) -> bool {
    std::env::var_os(nom).is_some_and(|v| !v.is_empty())
}

/// Couleurs : console Windows avec le mode VT, terminaux Unix, Git Bash.
fn ansi_on() -> bool {
    if env_non_vide("NO_COLOR") {
        return false;
    }
    #[cfg(windows)]
    {
        activer_console_windows();
        if env_non_vide("MSYSTEM") || env_non_vide("TERM") {
            return true; // Git Bash, mintty
        }
    }
    std::io::stdout().is_terminal() || env_non_vide("FORCE_COLOR")
}

#[cfg(windows)]
fn activer_console_windows() {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, SetConsoleOutputCP, STD_OUTPUT_HANDLE,
    };
    // ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(h, &mut mode) != 0 {
            SetConsoleMode(h, mode | 7);
        }
        SetConsoleOutputCP(65001);
    }
}

pub fn couleur_active() -> bool {
    *COULEUR.get_or_init(ansi_on)
}

pub fn peindre(txt: &str, code: &str, actif: bool) -> String {
    if actif { format!("\x1b[{code}m{txt}\x1b[0m") } else { txt.to_string() }
}

pub fn c(txt: &str, code: &str) -> String {
    peindre(txt, code, couleur_active())
}

pub fn sortie() {
    let _ = std::io::stdout().flush();
}

pub fn titre(t: &str) {
    println!();
    println!("{}", c(&"=".repeat(66), "36"));
    println!("{}", c(&format!("  {t}"), "1;36"));
    println!("{}", c(&"=".repeat(66), "36"));
}

pub fn ok(t: &str) {
    println!("{}", c(&format!("  [OK] {t}"), "32"));
}

pub fn warn(t: &str) {
    println!("{}", c(&format!("  [!]  {t}"), "33"));
}

pub fn err(t: &str) {
    println!("{}", c(&format!("  [X]  {t}"), "31"));
}

pub fn info(t: &str) {
    println!("  ->   {t}");
}

pub fn taille(n: u64) -> String {
    let mut n = n as f64;
    for unite in ["o", "Ko", "Mo", "Go"] {
        if n < 1024.0 || unite == "Go" {
            return if unite == "o" { format!("{n:.0} {unite}") } else { format!("{n:.1} {unite}") };
        }
        n /= 1024.0;
    }
    String::new()
}

pub fn nb_car(t: &str) -> usize {
    t.chars().count()
}

/// Texte complete d'espaces a droite jusqu'a largeur caracteres.
pub fn ljust(t: &str, largeur: usize) -> String {
    let n = nb_car(t);
    if n >= largeur { t.to_string() } else { format!("{t}{}", " ".repeat(largeur - n)) }
}

pub fn couper(texte: &str, largeur: usize) -> String {
    if nb_car(texte) <= largeur {
        return texte.to_string();
    }
    let debut: String = texte.chars().take(largeur.saturating_sub(1)).collect();
    format!("{debut}~")
}

/// Tabulations remplacees par des espaces, une colonne tous les 4 caracteres.
pub fn tabulations(texte: &str) -> String {
    let mut res = String::new();
    let mut col = 0;
    for ch in texte.chars() {
        match ch {
            '\t' => {
                let n = 4 - col % 4;
                res.push_str(&" ".repeat(n));
                col += n;
            }
            '\n' | '\r' => {
                res.push(ch);
                col = 0;
            }
            _ => {
                res.push(ch);
                col += 1;
            }
        }
    }
    res
}

pub fn largeur_terminal() -> usize {
    let colonnes = std::env::var("COLUMNS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0);
    let n = colonnes
        .or_else(|| terminal_size::terminal_size().map(|(w, _)| w.0 as usize))
        .unwrap_or(100);
    n.clamp(80, 160)
}

/// Tableau aligne : premiere colonne pour le parametre, puis les colonnes de
/// valeurs. couleurs : {(ligne, colonne): code} pour mettre une case en avant.
pub fn tableau(entetes: &[String], lignes: &[Vec<String>], couleurs: &HashMap<(usize, usize), String>) {
    let total = largeur_terminal() as i64 - 4;
    let mut larg: Vec<usize> = (0..entetes.len())
        .map(|i| {
            std::iter::once(&entetes[i])
                .chain(lignes.iter().map(|l| &l[i]))
                .map(|t| nb_car(t))
                .max()
                .unwrap_or(0)
        })
        .collect();
    if !larg.is_empty() {
        larg[0] = larg[0].min(32);
    }
    loop {
        let somme: usize = larg.iter().sum::<usize>() + 2 * (larg.len().saturating_sub(1));
        let max = *larg.iter().max().unwrap_or(&0);
        if (somme as i64) <= total || max <= 8 {
            break;
        }
        let i = larg.iter().position(|&l| l == max).unwrap();
        larg[i] -= 1; // la colonne la plus large cede la place
    }
    let rangee = |cases: &[String], n: Option<usize>| -> String {
        let morceaux: Vec<String> = cases
            .iter()
            .enumerate()
            .map(|(i, case)| {
                let texte = ljust(&couper(case, larg[i]), larg[i]);
                match n.and_then(|n| couleurs.get(&(n, i))) {
                    Some(code) if !code.is_empty() => c(&texte, code),
                    _ => texte,
                }
            })
            .collect();
        format!("    {}", morceaux.join("  ")).trim_end().to_string()
    };
    println!("{}", c(&rangee(entetes, None), "1"));
    println!("    {}", larg.iter().map(|&l| "-".repeat(l)).collect::<Vec<_>>().join("  "));
    for (n, l) in lignes.iter().enumerate() {
        println!("{}", rangee(l, Some(n)));
    }
}

/// Les deux versions d'un fichier cote a cote, en ligne a gauche et en local
/// a droite. Les lignes differentes sont en jaune ; une case vide signifie que
/// la ligne n'existe pas de ce cote.
pub fn cote_a_cote(gauche: &[String], droite: &[String], titre_gauche: &str, titre_droite: &str,
                   noms: (&str, &str)) {
    let (contexte, maxi) = (3, 400);
    let col = (largeur_terminal() - 4 - 3 - 2 * 6) / 2;
    let case = |num: Option<usize>, texte: &str, jaune: bool| -> String {
        let Some(num) = num else { return " ".repeat(6 + col) };
        let texte = ljust(&couper(&tabulations(texte).replace('\r', ""), col), col);
        format!("{}{}", c(&format!("{num:>5} "), "36"), if jaune { c(&texte, "1;33") } else { texte })
    };
    println!("{}", c(&format!("    {:6}{} | {:6}{}", "", ljust(noms.0, col), "", noms.1), "1"));
    println!("    {:6}{} | {:6}{}", "", ljust(&couper(titre_gauche, col), col), "", couper(titre_droite, col));
    println!("    {}-+-{}", "-".repeat(6 + col), "-".repeat(6 + col));
    let ops = capture_diff_slices(Algorithm::Myers, gauche, droite);
    let groupes: Vec<_> = group_diff_ops(ops, contexte)
        .into_iter()
        .filter(|g| g.iter().any(|op| op.tag() != DiffTag::Equal))
        .collect();
    if groupes.is_empty() {
        info("Contenu identique.");
        return;
    }
    let mut affichees = 0;
    for (g, groupe) in groupes.iter().enumerate() {
        if g > 0 {
            println!("    {:>5}{} | {:>5}", "...", " ".repeat(col + 1), "...");
        }
        for op in groupe {
            let (tag, a, b) = op.as_tag_tuple();
            for k in 0..(a.len().max(b.len())) {
                let i = (a.start + k < a.end).then_some(a.start + k);
                let j = (b.start + k < b.end).then_some(b.start + k);
                let jaune = tag != DiffTag::Equal;
                println!("    {} | {}",
                         case(i.map(|i| i + 1), i.map_or("", |i| gauche[i].as_str()), jaune),
                         case(j.map(|j| j + 1), j.map_or("", |j| droite[j].as_str()), jaune));
                affichees += 1;
                if affichees >= maxi {
                    info(&format!("Fin de l'affichage : {maxi} lignes au plus."));
                    return;
                }
            }
        }
    }
}

// Accueil : le logo de .github/logo.svg, dessine en 24 x 24 points
// (. transparent, f fond, b bord, o orange, w blanc)
const LOGO: [&str; 24] = [
    "....bbbbbbbbbbbbbbbb....",
    "..bbffffffffffffffffbb..",
    ".bffffffffffffffffffffb.",
    ".bffffffffffffffffffffb.",
    "bffffffooffffffffffffffb",
    "bfffffooooffffoofffffffb",
    "bfffffoooofffooooffffffb",
    "bffffffooffffooooffffffb",
    "bffffffoofffffoofffffffb",
    "bffffffooffffooffffffffb",
    "bffffffoofffoofffffffffb",
    "bffffffooffooffffffffffb",
    "bffffffoofoofffffffffffb",
    "bffffffooooffffffffffffb",
    "bffffffooffffffffffffffb",
    "bfffffoooofffffffffffffb",
    "bfffffooooffwffffffffffb",
    "bffffffooffffwfffffffffb",
    "bfffffffffffffwffffffffb",
    "bffffffffffffwfffffffffb",
    ".bffffffffffwffwwwwfffb.",
    ".bffffffffffffffffffffb.",
    "..bbffffffffffffffffbb..",
    "....bbbbbbbbbbbbbbbb....",
];

/// Couleurs exactes (24 bits) et leur equivalent en 256 couleurs
fn couleur_logo(nom: char, fond: bool) -> String {
    let (rgb, n256) = match nom {
        'f' => ((0x16, 0x1b, 0x22), 234),
        'b' => ((0x30, 0x36, 0x3d), 237),
        'o' => ((0xF0, 0x50, 0x32), 203),
        _ => ((0xf0, 0xf6, 0xfc), 255),
    };
    let vraies = matches!(std::env::var("COLORTERM").as_deref(), Ok("truecolor") | Ok("24bit"))
        || std::env::var_os("WT_SESSION").is_some()
        || matches!(std::env::var("TERM_PROGRAM").as_deref(), Ok("iTerm.app") | Ok("vscode") | Ok("WezTerm"));
    let plan = if fond { 48 } else { 38 };
    if vraies {
        format!("{plan};2;{};{};{}", rgb.0, rgb.1, rgb.2)
    } else {
        format!("{plan};5;{n256}")
    }
}

/// Logo, nom et accroche : au lancement, puis a chaque retour au menu principal.
pub fn afficher_accueil() {
    crate::session::set_accueil(true);
    println!();
    let mut lignes = Vec::new();
    for paire in LOGO.chunks(2) {
        let (haut, bas) = (paire[0].chars().collect::<Vec<_>>(), paire[1].chars().collect::<Vec<_>>());
        let mut ligne = String::from("    ");
        for (h, b) in haut.into_iter().zip(bas) {
            if !couleur_active() {
                // sans couleurs : le trace du logo, en noir et blanc
                let (ph, pb) = ("bow".contains(h), "bow".contains(b));
                ligne.push(if ph && pb { '\u{2588}' } else if ph { '\u{2580}' } else if pb { '\u{2584}' } else { ' ' });
            } else if h == '.' && b == '.' {
                ligne.push(' ');
            } else if h == '.' {
                ligne.push_str(&format!("\x1b[{}m\u{2584}\x1b[0m", couleur_logo(b, false)));
            } else if b == '.' {
                ligne.push_str(&format!("\x1b[{}m\u{2580}\x1b[0m", couleur_logo(h, false)));
            } else {
                ligne.push_str(&format!("\x1b[{};{}m\u{2580}\x1b[0m", couleur_logo(h, false), couleur_logo(b, true)));
            }
        }
        lignes.push(if couleur_active() { ligne } else { ligne.trim_end().to_string() });
    }
    println!("{}", lignes.join("\n"));
    println!();
    println!("{}", c(&format!("    gitmanager {}", env!("CARGO_PKG_VERSION")), "1;36"));
    println!("{}", c("    Coder chez soi, push au bureau, les repos GitHub", "36"));
    println!("{}", c("    en local comme en ligne.", "36"));
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn tailles_en_francais() {
        assert_eq!(taille(0), "0 o");
        assert_eq!(taille(900), "900 o");
        assert_eq!(taille(1536), "1.5 Ko");
        assert_eq!(taille(5 * 1024 * 1024), "5.0 Mo");
        assert_eq!(taille(3 * 1024 * 1024 * 1024 * 1024), "3072.0 Go");
    }

    #[test]
    fn texte_coupe_avec_un_tilde() {
        assert_eq!(couper("abcdef", 4), "abc~");
        assert_eq!(couper("été", 3), "été");
        assert_eq!(couper("abc", 0), "~");
    }

    #[test]
    fn tabulations_tous_les_4() {
        assert_eq!(tabulations("a\tb"), "a   b");
        assert_eq!(tabulations("\tx"), "    x");
        assert_eq!(tabulations("abcd\te"), "abcd    e");
    }

    #[test]
    fn couleur_seulement_si_active() {
        assert_eq!(peindre("commit", "1;31", true), "\x1b[1;31mcommit\x1b[0m");
        assert_eq!(peindre("commit", "1;31", false), "commit");
    }
}
