//! Arret : interrompt une operation avec un statut et une explication,
//! comme l'exception Arret de la version Python.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arret {
    pub statut: &'static str,
    pub detail: String,
}

impl Arret {
    pub fn new(statut: &'static str, detail: impl Into<String>) -> Self {
        Arret { statut, detail: detail.into() }
    }
}

pub type R<T> = Result<T, Arret>;
