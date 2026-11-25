//! Repos GitHub en ligne : liste, menu d'un repo, visibilite, suppression.

use crate::affichage::{err, info, ok, warn};
use crate::config::Config;
use crate::erreur::R;
use crate::github::{api, message_api, texte};
use crate::projet::est_sensible;
use crate::resultat::Res;
use crate::saisie::{ask_libre, ask_yn};
use serde_json::{json, Value};

pub fn act_depots(_cfg: &mut Config) -> R<Res> {
    warn("Repos GitHub en ligne : a venir dans la version Rust.");
    Ok(Res::Rien)
}

/// Fichiers secrets (.env, cles privees...) presents dans la branche principale.
pub fn sensibles_sur_github(cfg: &mut Config, d: &Value) -> Vec<String> {
    let branche = d.get("default_branch").and_then(Value::as_str).filter(|b| !b.is_empty()).unwrap_or("main");
    let (code, rep, _) = api(cfg, "GET", &format!("/repos/{}/git/trees/{branche}?recursive=1", texte(d, "full_name")), None);
    if code != 200 {
        return vec![];
    }
    rep.get("tree").and_then(Value::as_array).map(|t| t.iter()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("blob"))
        .map(|e| texte(e, "path")).filter(|p| est_sensible(p)).collect()).unwrap_or_default()
}

pub fn changer_visibilite(cfg: &mut Config, d: &mut Value) {
    let full = texte(d, "full_name");
    let vers_prive = d.get("private").and_then(Value::as_bool) != Some(true);
    if !vers_prive {
        warn("Public : tout le code ET tout l'historique des commits deviennent visibles.");
        let sensibles = sensibles_sur_github(cfg, d);
        if !sensibles.is_empty() {
            err("Ce repo contient des fichiers secrets, qui deviendraient publics :");
            for f in sensibles.iter().take(20) {
                println!("       {f}");
            }
            info("Les retirer et changer les secrets qu'ils contiennent avant de publier.");
            if ask_libre("Taper PUBLIC pour confirmer", "") != "PUBLIC" {
                info("Abandon.");
                return;
            }
        } else if !ask_yn(&format!("Rendre {full} PUBLIC ?"), false) {
            info("Abandon.");
            return;
        }
    } else if !ask_yn(&format!("Rendre {full} NON PUBLIC ?"), true) {
        info("Abandon.");
        return;
    }
    let (code, rep, _) = api(cfg, "PATCH", &format!("/repos/{full}"), Some(&json!({"private": vers_prive})));
    if code == 200 {
        d["private"] = vers_prive.into();
        ok(&format!("{full} est maintenant {}.", if vers_prive { "non public" } else { "public" }));
    } else {
        err(&format!("Erreur, code {code} : {}", message_api(&rep)));
    }
}
