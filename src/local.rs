//! Repos GitHub en local : liste, menu d'un repo et ses actions.

use crate::affichage::warn;
use crate::config::Config;
use crate::erreur::R;
use crate::resultat::Res;

pub fn act_depots_locaux(_cfg: &mut Config) -> R<Res> {
    warn("Repos GitHub en local : a venir dans la version Rust.");
    Ok(Res::Rien)
}
