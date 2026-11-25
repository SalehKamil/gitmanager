//! Repos GitHub en ligne : liste, menu d'un repo, suppression.

use crate::affichage::warn;
use crate::config::Config;
use crate::erreur::R;
use crate::resultat::Res;

pub fn act_depots(_cfg: &mut Config) -> R<Res> {
    warn("Repos GitHub en ligne : a venir dans la version Rust.");
    Ok(Res::Rien)
}
