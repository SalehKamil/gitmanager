//! Configuration locale ~/.gitmanager.json, la meme que la version Python.
//! Aucun secret n'y est stocke.

use crate::affichage::{info, warn};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Dossier perso : HOME, ou USERPROFILE sous Windows.
#[allow(deprecated)]
pub fn maison() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Valeur vraie au sens de Python : ni vide, ni zero, ni faux.
pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub data: Map<String, Value>,
}

impl Config {
    pub fn chemin() -> PathBuf {
        maison().join(".gitmanager.json")
    }

    pub fn charger() -> Config {
        let data = std::fs::read_to_string(Self::chemin())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        Config { data }
    }

    pub fn sauver(&self) {
        let chemin = Self::chemin();
        let texte = serde_json::to_string_pretty(&Value::Object(self.data.clone())).unwrap_or_default();
        if std::fs::write(&chemin, texte).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    pub fn get(&self, cle: &str) -> Option<&Value> {
        self.data.get(cle)
    }

    /// Texte de la cle, vide si elle manque.
    pub fn texte(&self, cle: &str) -> String {
        self.data.get(cle).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }

    pub fn vrai(&self, cle: &str) -> bool {
        self.data.get(cle).is_some_and(vrai)
    }

    pub fn set(&mut self, cle: &str, v: impl Into<Value>) {
        self.data.insert(cle.to_string(), v.into());
    }

    pub fn pop(&mut self, cle: &str) -> Option<Value> {
        self.data.shift_remove(cle)
    }
}

/// Supprime le token laisse par l'ancienne version du script.
pub fn migrer_ancienne_config(cfg: &mut Config) {
    let ancien = cfg.pop("token").and_then(|v| v.as_str().map(str::to_string)).filter(|s| !s.is_empty());
    if let Some(api_ent) = cfg.pop("api").and_then(|v| v.as_str().map(str::to_string)) {
        if let Some(h) = api_ent.strip_prefix("https://").and_then(|r| r.strip_suffix("/api/v3")) {
            if !h.contains('/') {
                cfg.set("host", h);
            }
        }
    }
    cfg.pop("nom");
    cfg.pop("email");
    if let Some(ancien) = ancien {
        let mut nettoyes = 0;
        let fichier = maison().join(".git-credentials");
        if let Ok(texte) = std::fs::read_to_string(&fichier) {
            let lignes: Vec<&str> = texte.lines().collect();
            let gardees: Vec<&str> = lignes.iter().copied().filter(|l| !l.contains(&ancien)).collect();
            nettoyes = lignes.len() - gardees.len();
            if nettoyes > 0 {
                let fin = if gardees.is_empty() { "" } else { "\n" };
                let _ = std::fs::write(&fichier, gardees.join("\n") + fin);
            }
        }
        warn(&format!("Ancien token : plus dans ~/.gitmanager.json{}",
                      if nettoyes > 0 { " ni dans ~/.git-credentials." } else { "." }));
        info("Il ne sert plus. Le retirer sur GitHub : https://github.com/settings/tokens");
    }
    cfg.sauver();
    if std::env::var_os("GITHUB_TOKEN").is_some() {
        std::env::remove_var("GITHUB_TOKEN");
        warn("La variable GITHUB_TOKEN ne sert pas : elle bloquerait la connexion par le navigateur.");
    }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn valeurs_vraies_comme_en_python() {
        assert!(!vrai(&Value::Null) && !vrai(&"".into()) && !vrai(&0.into()) && !vrai(&false.into()));
        assert!(vrai(&"gh".into()) && vrai(&1.into()) && vrai(&true.into()));
        let mut cfg = Config::default();
        cfg.set("methode", "gh");
        cfg.set("garde", 1700000000);
        assert_eq!(cfg.texte("methode"), "gh");
        assert_eq!(cfg.texte("garde"), "");
        assert!(cfg.vrai("garde"));
        assert_eq!(cfg.pop("methode"), Some(Value::from("gh")));
        assert!(!cfg.vrai("methode"));
    }
}
