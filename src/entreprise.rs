//! Option 2 : importer un repo GitHub, l'envoyer dans un repo de l'entreprise.

use crate::affichage::warn;
use crate::config::Config;
use crate::erreur::R;
use crate::resultat::Res;

pub fn act_importer(_cfg: &mut Config) -> R<Res> {
    warn("Import : a venir dans la version Rust.");
    Ok(Res::Rien)
}
