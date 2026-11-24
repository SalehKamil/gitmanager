#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""
gitmanager.py : coder a la maison, pousser au bureau, gerer les repos GitHub
en local comme en ligne.

Quand l'entreprise n'autorise pas le transfert de fichiers, gitmanager permet
de travailler sur ses projets depuis l'ordinateur perso, plus confortable,
puis de les envoyer sur l'ordinateur pro. Il sert aussi a gerer les repos
GitHub, en local comme en ligne. Il remplace les commandes Git par des menus.

Au premier lancement, il installe ce qui manque (Git, GitHub CLI) dans la
derniere version disponible et affiche l'empreinte SHA-256 de Git pour
verification.

Lancement (avec uv) :
    uv run gitmanager.py

Commande gitmanager sur n'importe quel poste (avec uv) :
    uv tool install https://github.com/SalehKamil/gitmanager/archive/refs/heads/main.zip

Au lancement : logo et accroche, puis le menu principal. Chaque option
demande d'abord la connexion a son compte GitHub ; sans connexion, retour au
menu principal. Les deux comptes restent separes.

  1. Gerer les repos GitHub perso, connexion au compte GitHub perso
     - Gerer les repos GitHub : fichiers, commits, public / prive, ouvrir la
       copie locale ou cloner le projet en local, voir les acces, supprimer.
     - Gerer les repos locaux : envoyer, c'est-a-dire commit + push ; mettre
       a jour par pull ; ecraser par force push ; push ou pull de tous les
       repos d'un dossier ; sans repo local, envoi d'un dossier sur GitHub
       avec nom, description et visibilite.
     - Compte GitHub : moyen de connexion, serveur, permissions.

  2. Entreprise, connexion au compte GitHub de l'entreprise
     - Importer un repo GitHub par son URL. Un repo public s'importe sans
       identifiants ; un repo prive demande la connexion au compte perso le
       temps de l'import, et peut devenir public le temps du clone, avec
       accord, puis redevenir prive apres 2 minutes. Le projet importe peut
       ensuite etre envoye dans un repo GitHub de l'entreprise, choisi dans
       la liste ou par son URL : mettre a jour, sans rien supprimer, ou
       ecraser, contenu exact. L'envoi passe par une copie locale du repo de
       l'entreprise sur le Bureau, gardee apres l'envoi.
     - Gerer les repos GitHub de l'entreprise : les memes choix que
       l'option 1, avec le compte de l'entreprise.

Connexion : trois moyens, du plus sur au moins sur, avec un avertissement
pour les moyens a risque. Par code ou par jeton : connexion gardee en memoire
seulement, oubliee apres 15 minutes sans activite et a la fermeture du
programme. Par le navigateur : GitHub CLI ou Git Credential Manager,
connexion enregistree sur le poste, a eviter sur un poste partage ou pro.

Securite : HTTPS ou SSH seulement, certificats toujours verifies, aucun
jeton ecrit sur le disque, dans une URL ni sur la ligne de commande ; alerte
si Git ecrit des identifiants en clair.
"""

import base64
import filecmp
import getpass
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import webbrowser
from concurrent.futures import ThreadPoolExecutor
from datetime import date, datetime, timezone
from pathlib import Path

__version__ = "2.0.0"
CONFIG = Path.home() / ".gitmanager.json"

GITIGNORE = """__pycache__/
*.py[cod]
*.so
.venv/
venv/
env/
.env
.env.*
!.env.example
*.log
.pytest_cache/
.mypy_cache/
.ruff_cache/
dist/
build/
*.egg-info/
node_modules/
.idea/
.vscode/
.DS_Store
Thumbs.db
"""

# Fichiers a ne jamais envoyer par erreur (secrets, cles privees)
SENSIBLES = re.compile(
    r"(^|/)(\.env(\.[^/]*)?|id_(rsa|dsa|ecdsa|ed25519)[^/]*|[^/]*\.(pem|key|p12|pfx)"
    r"|credentials\.json|secrets?\.(json|ya?ml|toml))$", re.I)
SENSIBLES_OK = re.compile(r"(\.env\.(example|sample|template)|\.pub)$", re.I)
LIMITE_GITHUB = 100 * 1024 * 1024  # GitHub refuse les fichiers > 100 Mo

# Dossiers ignores quand on cherche des projets
DOSSIERS_IGNORES = {"node_modules", ".venv", "venv", "env", "__pycache__",
                    "dist", "build", ".git", "AppData", "Library", "Applications",
                    "Program Files", "Program Files (x86)", "Windows"}
# Dossiers jamais copies vers l'entreprise (propres a chaque ordinateur)
EXCLUS_COPIE = {".git", "__pycache__", ".venv", "venv", "node_modules",
                ".mypy_cache", ".pytest_cache", ".ruff_cache"}
# Endroits ou chercher les projets de l'ordinateur (relatifs au dossier perso)
EMPLACEMENTS = ("", "Documents", "Desktop", "Bureau", "Projects", "Projets",
                "projets", "dev", "code", "src", "repos", "source/repos", "GitHub",
                "Documents/GitHub", "OneDrive/Documents", "workspace")

BRUIT_GIT = re.compile(r"(LF will be replaced by CRLF|CRLF will be replaced by LF)", re.I)
STATUTS = {"A": "ajoute", "M": "modifie", "D": "supprime", "R": "renomme",
           "C": "copie", "T": "modifie", "U": "conflit"}


# Affichage

def _ansi_on():
    """Couleurs : console Windows avec le mode VT, terminaux Unix, Git Bash."""
    if os.environ.get("NO_COLOR"):
        return False
    if os.name == "nt":
        try:
            import ctypes
            k = ctypes.windll.kernel32
            k.SetConsoleMode(k.GetStdHandle(-11), 7)
        except Exception:
            pass
        if os.environ.get("MSYSTEM") or os.environ.get("TERM"):  # Git Bash, mintty
            return True
    return sys.stdout.isatty() or bool(os.environ.get("FORCE_COLOR"))


def _sortie_utf8():
    """Le logo utilise des blocs Unicode : sortie en UTF-8 si besoin, par
    exemple dans Git Bash sous Windows."""
    if "utf" not in (getattr(sys.stdout, "encoding", "") or "").lower():
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass


_sortie_utf8()


COLOR = _ansi_on()


def c(txt, code):
    return f"\033[{code}m{txt}\033[0m" if COLOR else txt


def titre(t):
    print()
    print(c("=" * 66, "36"))
    print(c(f"  {t}", "1;36"))
    print(c("=" * 66, "36"))


def ok(t):
    print(c(f"  [OK] {t}", "32"))


def warn(t):
    print(c(f"  [!]  {t}", "33"))


def err(t):
    print(c(f"  [X]  {t}", "31"))


def info(t):
    print(c(f"  ->   {t}", "90"))


def taille(n):
    n = float(n or 0)
    for unite in ("o", "Ko", "Mo", "Go"):
        if n < 1024 or unite == "Go":
            return f"{n:.0f} {unite}" if unite == "o" else f"{n:.1f} {unite}"
        n /= 1024
    return ""


# Saisie

DELAI_INACTIVITE = 15 * 60  # connexion en memoire oubliee apres 15 minutes sans saisie


def saisie(invite):
    """input() qui oublie la connexion en memoire apres une longue inactivite."""
    debut = time.time()
    rep = input(invite)
    if time.time() - debut > DELAI_INACTIVITE:
        oublier_connexion_memoire()
    return rep


def ask(question, defaut="", obligatoire=True):
    """Entree vide -> valeur par defaut. Une valeur par defaut vide ne
    valide jamais une question obligatoire."""
    suffixe = f" [{defaut}]" if defaut else ""
    while True:
        rep = saisie(c(f"  {question}{suffixe} : ", "1")).strip()
        if rep:
            return rep
        if defaut:
            return defaut
        if not obligatoire:
            return ""
        warn("Une valeur est requise.")


def ask_yn(question, defaut=False):
    d = "O/n" if defaut else "o/N"
    rep = saisie(c(f"  {question} [{d}] : ", "1")).strip().lower()
    if not rep:
        return defaut
    return rep in ("o", "oui", "y", "yes")


def pause():
    saisie(c("\n  Entree pour continuer", "90"))


def message_defaut():
    return "Mise a jour du " + datetime.now().strftime("%Y-%m-%d %H:%M")


# Configuration locale (aucun secret n'y est stocke)

def charger():
    if CONFIG.exists():
        try:
            return json.loads(CONFIG.read_text(encoding="utf-8"))
        except Exception:
            return {}
    return {}


def sauver(cfg):
    CONFIG.write_text(json.dumps(cfg, indent=2), encoding="utf-8")
    try:
        os.chmod(CONFIG, 0o600)
    except Exception:
        pass


def migrer_ancienne_config(cfg):
    """Supprime le token laisse par l'ancienne version du script."""
    ancien = cfg.pop("token", None)
    api_ent = cfg.pop("api", None)
    if api_ent:
        m = re.match(r"https://([^/]+)/api/v3", api_ent)
        if m:
            cfg["host"] = m.group(1)
    cfg.pop("nom", None)
    cfg.pop("email", None)
    if ancien:
        nettoyes = 0
        fichier = Path.home() / ".git-credentials"
        if fichier.exists():
            try:
                lignes = fichier.read_text(encoding="utf-8").splitlines()
                gardees = [l for l in lignes if ancien not in l]
                nettoyes = len(lignes) - len(gardees)
                if nettoyes:
                    fichier.write_text("\n".join(gardees) + ("\n" if gardees else ""),
                                       encoding="utf-8")
            except Exception:
                pass
        warn("Ancien token supprime de ~/.gitmanager.json"
             + (" et de ~/.git-credentials." if nettoyes else "."))
        info("Il n'est plus utilise. Le revoquer ici : https://github.com/settings/tokens")
    sauver(cfg)
    if os.environ.pop("GITHUB_TOKEN", None):
        warn("La variable GITHUB_TOKEN est ignoree : elle bloquerait la connexion par le navigateur.")


# Git

class Arret(Exception):
    """Interrompt une operation avec un statut et une explication."""

    def __init__(self, statut, detail):
        super().__init__(detail)
        self.statut, self.detail = statut, detail


# Aide d'identification git qui lit le jeton dans l'environnement du processus :
# le jeton n'apparait ni dans la ligne de commande, ni dans un fichier.
AIDE_JETON = ('!f() { test "$1" = get || exit 0; '
              'echo "username=${GM_UTILISATEUR:-x-access-token}"; '
              'echo "password=$GM_JETON"; }; f')


def identite_courante():
    """(utilisateur, jeton) a utiliser pour git, ou None (gestionnaire du PC)."""
    if SESSION["methode"] in EN_MEMOIRE and SESSION["token"]:
        return ("", SESSION["token"])
    return None


def _env_git(identite=None, anonyme=False):
    """Environnement de chaque commande git : HTTPS ou SSH seulement,
    certificats verifies, identifiants jamais sur la ligne de commande."""
    env = dict(os.environ)
    env.pop("GIT_SSL_NO_VERIFY", None)
    env["GIT_ALLOW_PROTOCOL"] = "https:ssh:file"  # pas de http:// ni git:// en clair
    config = [("http.sslVerify", "true")]
    identite = None if anonyme else (identite or identite_courante())
    if anonyme or identite:
        config.append(("credential.helper", ""))  # ignore les gestionnaires du PC
    if identite:
        config.append(("credential.helper", AIDE_JETON))
        env["GM_UTILISATEUR"], env["GM_JETON"] = identite
    try:
        base = int(env.get("GIT_CONFIG_COUNT") or 0)
    except ValueError:
        base = 0
    for i, (cle, valeur) in enumerate(config, base):
        env[f"GIT_CONFIG_KEY_{i}"], env[f"GIT_CONFIG_VALUE_{i}"] = cle, valeur
    env["GIT_CONFIG_COUNT"] = str(base + len(config))
    # Jamais d'invite "Username / Password" : l'acces passe par la connexion
    # navigateur, GitHub CLI ou Git Credential Manager, ou par un jeton.
    env["GIT_TERMINAL_PROMPT"] = "0"
    if anonyme:
        env["GCM_INTERACTIVE"] = "never"
    return env


def _affiche(args):
    return " ".join(f'"{a}"' if (" " in str(a) or not str(a)) else str(a) for a in args)


def git(*args, cwd=None, quiet=False, env=None):
    """Execute une commande git. Retourne (code, stdout, stderr)."""
    cmd = ["git", "-c", "core.quotepath=off"] + [str(a) for a in args]
    if not quiet:
        print(c("  $ git " + _affiche(args), "90"))
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True,
                       encoding="utf-8", errors="replace", env=env or _env_git())
    out, errs = (p.stdout or "").rstrip(), (p.stderr or "").strip()
    if not quiet:
        for flux in (out, errs):
            for ligne in flux.strip().splitlines():
                if not BRUIT_GIT.search(ligne):
                    print("     " + ligne)
    return p.returncode, out, errs


def git_direct(*args, cwd=None):
    """git avec affichage en direct (progression d'un clone, etc.)."""
    print(c("  $ git " + _affiche(args), "90"))
    return subprocess.run(["git"] + [str(a) for a in args], cwd=cwd,
                          env=_env_git()).returncode


def derniere_ligne(texte):
    lignes = [l.strip() for l in (texte or "").splitlines()
              if l.strip() and not BRUIT_GIT.search(l)]
    if not lignes:
        return ""
    return re.sub(r"^(fatal|error|erreur|remote):\s*", "", lignes[-1], flags=re.I)


def racine_repo(chemin):
    code, out, _ = git("rev-parse", "--show-toplevel", cwd=chemin, quiet=True)
    return os.path.normpath(out) if code == 0 and out else None


def branche_courante(chemin):
    """Nom de la branche (meme sans aucun commit). None si HEAD detachee."""
    code, out, _ = git("symbolic-ref", "--short", "-q", "HEAD", cwd=chemin, quiet=True)
    return out if code == 0 and out else None


def branches_locales(chemin):
    code, out, _ = git("branch", "--format=%(refname:short)", cwd=chemin, quiet=True)
    return [b.strip() for b in out.splitlines() if b.strip()] if code == 0 else []


def a_des_commits(chemin):
    return git("rev-parse", "--verify", "-q", "HEAD", cwd=chemin, quiet=True)[0] == 0


def ref_existe(chemin, ref):
    return git("rev-parse", "--verify", "-q", ref, cwd=chemin, quiet=True)[0] == 0


def nb_commits(chemin, plage="HEAD"):
    code, out, _ = git("rev-list", "--count", plage, cwd=chemin, quiet=True)
    return int(out) if code == 0 and out.isdigit() else 0


def remotes(chemin):
    code, out, _ = git("remote", cwd=chemin, quiet=True)
    return [r.strip() for r in out.splitlines() if r.strip()] if code == 0 else []


def urls_remotes(chemin):
    """{remote: adresse} du projet, en une seule commande git."""
    _, out, _ = git("config", "--get-regexp", r"^remote\..*\.url$", cwd=chemin, quiet=True)
    urls = {}
    for ligne in out.splitlines():
        cle, _, url = ligne.partition(" ")
        urls[cle[len("remote."):-len(".url")]] = url
    return urls


def adresse_principale(chemin):
    urls = urls_remotes(chemin)
    return urls.get("origin") or next(iter(urls.values()), "")


def lien_court(url):
    """proprietaire/repo, ou la fin de l'adresse, ou 'non relie'."""
    return (cle_depot(url) or url[-30:]) if url else c("non relie", "90")


def apercu_projets(projets, avec_branche=False):
    """[(adresse, nb de modifications, branche)] calcules en parallele : la
    liste s'affiche vite meme avec beaucoup de projets (surtout sous Windows)."""
    def apercu(p):
        return (adresse_principale(p), len(fichiers_modifies(p)),
                (branche_courante(p) or "?") if avec_branche else "")
    with ThreadPoolExecutor(max_workers=8) as pool:
        return list(pool.map(apercu, projets))


def remote_principal(chemin):
    rs = remotes(chemin)
    return "origin" if "origin" in rs else (rs[0] if rs else None)


def url_remote(nom, chemin):
    code, out, _ = git("remote", "get-url", nom, cwd=chemin, quiet=True)
    return out.strip() if code == 0 else ""


def fichiers_modifies(chemin):
    code, out, _ = git("status", "--porcelain", cwd=chemin, quiet=True)
    return [l for l in out.splitlines() if l.strip()] if code == 0 else []


def fichiers_en_conflit(chemin):
    _, out, _ = git("diff", "--name-only", "--diff-filter=U", cwd=chemin, quiet=True)
    return [l for l in out.splitlines() if l.strip()]


def operation_en_cours(chemin):
    """'rebase' ou 'merge' si une operation est restee bloquee, sinon None."""
    for nom, typ in (("rebase-merge", "rebase"), ("rebase-apply", "rebase"),
                     ("MERGE_HEAD", "merge")):
        code, out, _ = git("rev-parse", "--git-path", nom, cwd=chemin, quiet=True)
        if code == 0 and os.path.exists(os.path.join(chemin, out)):
            return typ
    return None


def avance_retard(chemin, remote, branche):
    """(commits locaux a envoyer, commits du serveur a recuperer) ou None."""
    code, out, _ = git("rev-list", "--left-right", "--count",
                       f"{remote}/{branche}...HEAD", cwd=chemin, quiet=True)
    if code != 0 or len(out.split()) != 2:
        return None
    retard, avance = (int(x) for x in out.split())
    return avance, retard


def branche_defaut_distante(chemin, remote):
    code, out, _ = git("ls-remote", "--symref", remote, "HEAD", cwd=chemin, quiet=True)
    m = re.search(r"ref:\s*refs/heads/(\S+)\s+HEAD", out)
    return m.group(1) if code == 0 and m else None


def init_depot(chemin):
    if git("init", "-b", "main", cwd=chemin)[0] != 0:  # git < 2.28
        git("init", cwd=chemin)
        git("symbolic-ref", "HEAD", "refs/heads/main", cwd=chemin, quiet=True)
    ok(f"Repo git initialise dans {chemin}")


def lire_name_status(texte):
    """[(statut, fichier)] depuis 'git diff --name-status'."""
    res = []
    for l in texte.splitlines():
        parts = l.split("\t")
        if len(parts) >= 2:
            res.append((STATUTS.get(parts[0][:1], parts[0]), " -> ".join(parts[1:])))
    return res


def bilan_changements(changements):
    compte = {}
    for s, _ in changements:
        compte[s] = compte.get(s, 0) + 1
    return ", ".join(f"{n} {s}(s)" for s, n in compte.items()) or "aucun changement"


def afficher_changements(changements, limite=40):
    for s, f in changements[:limite]:
        print(f"       {s:<9} {f}")
    if len(changements) > limite:
        print(f"       ... et {len(changements) - limite} autre(s)")


def changements_entre(chemin, avant, apres="HEAD"):
    if avant:
        _, ns, _ = git("diff", "--name-status", "-M", avant, apres, cwd=chemin, quiet=True)
        return lire_name_status(ns)
    _, out, _ = git("ls-tree", "-r", "--name-only", apres, cwd=chemin, quiet=True)
    return [("ajoute", f) for f in out.splitlines() if f]


def statut_porcelaine(ligne):
    xy = ligne[:2]
    if xy == "??":
        return "nouveau"
    if "U" in xy or xy in ("AA", "DD"):
        return "conflit"
    for ch in xy:
        if ch != " ":
            return STATUTS.get(ch, ch)
    return xy


def supprimer_dossier(chemin):
    """rmtree qui passe aussi sur les fichiers en lecture seule (Windows)."""
    def forcer(func, path, _exc):
        try:
            os.chmod(path, stat.S_IWRITE)
            func(path)
        except Exception:
            pass
    if sys.version_info >= (3, 12):
        shutil.rmtree(chemin, onexc=forcer)
    else:
        shutil.rmtree(chemin, onerror=forcer)


# URL

def sans_identifiants(url):
    return re.sub(r"^(https?://)[^@/]+@", r"\1", url)


def contient_token(url):
    m = re.match(r"^https?://([^@/]+)@", url)
    return bool(m) and (":" in m.group(1) or
                        bool(re.match(r"(ghp|gho|ghu|ghs|github_pat)_", m.group(1))))


def est_ssh(url):
    return url.startswith("git@") or url.startswith("ssh://")


def infos_url(url):
    """(host, owner, repo) d'une URL https ou ssh."""
    url = sans_identifiants(url.strip()).rstrip("/")
    m = re.match(r"^(?:https?://|ssh://(?:[^@/]+@)?|[^@/]+@)([^/:]+)(?::\d+)?[:/](.+)$", url)
    if not m:
        return None, None, None
    chemin = m.group(2)
    if chemin.endswith(".git"):
        chemin = chemin[:-4]
    morceaux = chemin.split("/")
    if len(morceaux) < 2:
        return m.group(1), None, None
    return m.group(1).lower(), morceaux[-2], morceaux[-1]


def cle_depot(url):
    h, o, r = infos_url(url)
    return f"{o}/{r}".lower() if o and r else None


def url_web(url):
    h, o, r = infos_url(url)
    return f"https://{h}/{o}/{r}" if o and r else ""


# Connexion GitHub par le navigateur (gh ou Git Credential Manager)

# Deux comptes separes : le compte GitHub perso, option 1, et celui de
# l'entreprise, option 2. SESSION designe toujours le compte actif.
PROFILS = {"perso": {"token": "", "user": None, "methode": None},
           "entreprise": {"token": "", "user": None, "methode": None}}
PROFIL = {"nom": "perso"}
SESSION = PROFILS["perso"]


def activer_profil(nom):
    global SESSION
    PROFIL["nom"] = nom
    SESSION = PROFILS[nom]


def cle(nom):
    """Cle de configuration du compte actif : les reglages de connexion du compte
    de l'entreprise sont gardes a part de ceux du compte perso."""
    return nom if PROFIL["nom"] == "perso" else f"{nom}_entreprise"


def nom_compte():
    return "perso" if PROFIL["nom"] == "perso" else "de l'entreprise"
EN_MEMOIRE = ("jeton", "code")  # connexions jamais enregistrees sur le poste
NOMS_METHODES = {"gh": "GitHub CLI", "gcm": "Git Credential Manager",
                 "code": "code, en memoire seulement",
                 "jeton": "jeton en memoire"}
_CACHE_GCM = []


def host(cfg):
    return (cfg.get(cle("host")) or "github.com").strip().lower()


def api_base(cfg):
    h = host(cfg)
    return "https://api.github.com" if h == "github.com" else f"https://{h}/api/v3"


def run(cmd, entree=None, env=None):
    return subprocess.run(cmd, input=entree, capture_output=True, text=True,
                          encoding="utf-8", errors="replace", env=env)


# Installation des outils : Git (derniere version) et GitHub CLI

NOMS_OUTILS = {"git": "Git", "gh": "GitHub CLI"}
PAQUETS = {
    "git": {"winget": "Git.Git", "brew": "git", "apt-get": "git", "dnf": "git",
            "pacman": "git", "zypper": "git"},
    "gh": {"winget": "GitHub.cli", "brew": "gh", "apt-get": "gh", "dnf": "gh",
           "pacman": "github-cli", "zypper": "gh"},
}
VERIF_GIT_JOURS = 7  # recherche d'une nouvelle version de Git une fois par semaine


def gestionnaire_paquets():
    """Gestionnaire de paquets du systeme. Il verifie lui-meme la signature ou
    l'empreinte de ce qu'il telecharge."""
    if os.name == "nt":
        candidats = ("winget",)
    elif sys.platform == "darwin":
        candidats = ("brew",)
    else:
        candidats = ("apt-get", "dnf", "pacman", "zypper")
    return next((g for g in candidats if shutil.which(g)), None)


def commandes_paquet(g, paquet, mise_a_jour=False):
    """Commandes qui installent (ou mettent a jour) la derniere version d'un paquet."""
    sudo = [] if os.name == "nt" or g == "brew" or os.geteuid() == 0 else ["sudo"]
    if g == "winget":
        return [["winget", "upgrade" if mise_a_jour else "install", "--id", paquet, "-e",
                 "--source", "winget", "--accept-source-agreements",
                 "--accept-package-agreements"]]
    if g == "brew":
        return [["brew", "install", paquet]]  # installe, ou met a jour si deja la
    if g == "apt-get":
        return [sudo + ["apt-get", "update"], sudo + ["apt-get", "install", "-y", paquet]]
    if g == "dnf":
        return [sudo + ["dnf", "upgrade" if mise_a_jour else "install", "-y", paquet]]
    if g == "pacman":
        return [sudo + ["pacman", "-S", "--noconfirm", "--needed", paquet]]
    if g == "zypper":
        return [sudo + ["zypper", "--non-interactive", "install", paquet]]
    return []


def version_git():
    """(majeur, mineur) de Git, ou None s'il n'est pas installe."""
    if not shutil.which("git"):
        return None
    m = re.search(r"(\d+)\.(\d+)", run(["git", "--version"]).stdout)
    return (int(m.group(1)), int(m.group(2))) if m else (0, 0)


def actualiser_path():
    """Juste apres une installation sous Windows, le PATH du terminal n'est pas a jour."""
    if os.name != "nt":
        return
    for d in (os.path.join(os.environ.get("ProgramFiles", r"C:\Program Files"), "Git", "cmd"),
              os.path.join(os.environ.get("LOCALAPPDATA", ""), "Programs", "Git", "cmd")):
        if os.path.isdir(d) and d not in os.environ.get("PATH", ""):
            os.environ["PATH"] = d + os.pathsep + os.environ.get("PATH", "")


def _sortie(cmd):
    """Sortie d'une commande d'information, sans bloquer plus d'une minute."""
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8",
                           errors="replace", timeout=60)
        return p.returncode, p.stdout
    except (OSError, subprocess.SubprocessError):
        return -1, ""


def empreinte_fichier(chemin):
    h = hashlib.sha256()
    with open(chemin, "rb") as f:
        for bloc in iter(lambda: f.read(1 << 20), b""):
            h.update(bloc)
    return h.hexdigest()


def empreinte_paquet(g):
    """Empreinte SHA-256 du paquet Git, celle que le gestionnaire a verifiee."""
    if g == "winget":
        _, out = _sortie(["winget", "show", "--id", "Git.Git", "-e", "--source", "winget",
                          "--accept-source-agreements"])
        m = re.search(r"\b[0-9A-Fa-f]{64}\b", out)
        return m.group(0).lower() if m else ""
    if g == "apt-get":
        _, version = _sortie(["dpkg-query", "-W", "-f=${Version}", "git"])
        _, out = _sortie(["apt-cache", "show", "git"])
        for bloc in out.split("\n\n"):
            if version.strip() and f"\nVersion: {version.strip()}\n" in f"\n{bloc}\n":
                m = re.search(r"^SHA256: ([0-9a-f]{64})$", bloc, re.M)
                return m.group(1) if m else ""
    return ""


def afficher_empreinte_git(g):
    """Version installee et empreintes SHA-256, pour verifier l'installation."""
    _, version, _ = git("--version", quiet=True)
    ok(f"{version} est installe.")
    paquet = empreinte_paquet(g)
    if paquet:
        info(f"Empreinte SHA-256 du paquet, verifiee par {g} :")
        print(f"       {paquet}")
        if g == "winget":
            info("A comparer avec https://github.com/git-for-windows/git/releases")
    exe = shutil.which("git")
    if exe:
        info("Empreinte SHA-256 du programme git installe :")
        print(f"       {empreinte_fichier(os.path.realpath(exe))}")


def mise_a_jour_git_dispo(g):
    """True si le gestionnaire de paquets propose une version plus recente de Git."""
    if g == "winget":
        code, out = _sortie(["winget", "list", "--id", "Git.Git", "-e",
                             "--upgrade-available", "--accept-source-agreements"])
        return code == 0 and "Git.Git" in out
    if g == "brew":
        return "git" in _sortie(["brew", "outdated", "--quiet", "--formula"])[1].split()
    if g == "apt-get":
        return any(l.startswith("git/") for l in
                   _sortie(["apt", "list", "--upgradable"])[1].splitlines())
    if g == "dnf":
        return _sortie(["dnf", "-q", "check-update", "git"])[0] == 100
    if g == "pacman":
        return _sortie(["pacman", "-Qu", "git"])[0] == 0
    if g == "zypper":
        return bool(re.search(r"\|\s*git\s*\|", _sortie(["zypper", "-q", "list-updates"])[1]))
    return False


def installer_outil(outil, mise_a_jour=False):
    """Installe ou met a jour un outil avec le gestionnaire de paquets du systeme.
    Retourne True si l'outil est pret."""
    nom = NOMS_OUTILS[outil]
    g = gestionnaire_paquets()
    if not g:
        if outil == "git" and sys.platform == "darwin":
            info("Les outils de developpement d'Apple contiennent Git : une fenetre s'ouvre.")
            if ask_yn("Les installer maintenant ?", True):
                subprocess.run(["xcode-select", "--install"])
                info("Relancer le programme quand l'installation est terminee.")
            return False
        info(f"Installer {nom} depuis " + ("https://git-scm.com/downloads" if outil == "git"
                                          else "https://cli.github.com") + ", puis relancer.")
        return False
    verbe = "Mettre a jour" if mise_a_jour else "Installer"
    if not ask_yn(f"{verbe} {nom}, derniere version disponible, avec {g} ?", True):
        return False
    for cmd in commandes_paquet(g, PAQUETS[outil][g], mise_a_jour):
        print(c("  $ " + " ".join(cmd), "90"))
        subprocess.run(cmd)
    actualiser_path()
    if outil == "gh":
        pret = bool(chemin_gh())
        if pret:
            ok("GitHub CLI est installe.")
    else:
        pret = (version_git() or (0, 0)) >= (2, 31)
        if pret:
            afficher_empreinte_git(g)
    if not pret and os.name == "nt":
        warn(f"{nom} est installe mais pas encore visible : fermer ce terminal, "
             "le rouvrir et relancer le programme.")
    elif not pret:
        err(f"L'installation de {nom} avec {g} n'a pas abouti.")
        info("L'installer depuis " + ("https://git-scm.com/downloads" if outil == "git"
                                      else "https://cli.github.com") + ", puis relancer.")
    return pret


def preparer_outils(cfg):
    """Au premier lancement, installe ce qui manque pour que tout soit pret.
    Ensuite, une fois par semaine, propose la derniere version de Git."""
    version = version_git()
    aujourd_hui = date.today()
    if version is None or version < (2, 31):
        warn("Git n'est pas installe." if version is None else
             f"Git {version[0]}.{version[1]} est trop ancien : il faut la version 2.31 ou plus.")
        if not installer_outil("git", mise_a_jour=version is not None):
            return False
        cfg["verif_git"] = aujourd_hui.isoformat()
    else:
        try:
            ecart = (aujourd_hui - date.fromisoformat(cfg.get("verif_git", ""))).days
        except ValueError:
            ecart = VERIF_GIT_JOURS
        g = gestionnaire_paquets()
        if ecart >= VERIF_GIT_JOURS and g:
            cfg["verif_git"] = aujourd_hui.isoformat()
            sauver(cfg)
            info("Recherche d'une nouvelle version de Git...")
            if mise_a_jour_git_dispo(g):
                warn("Une nouvelle version de Git est disponible.")
                installer_outil("git", mise_a_jour=True)
    sauver(cfg)
    verifier_identifiants_en_clair(cfg)
    return True


def verifier_identifiants_en_clair(cfg):
    """Alerte si Git ecrit des identifiants en clair sur le poste : reglage
    credential.helper store et fichier ~/.git-credentials."""
    _, out, _ = git("config", "--global", "--get-regexp", r"^credential(\..+)?\.helper$",
                    quiet=True)
    stores = [l.split(" ", 1)[0] for l in out.splitlines()
              if " " in l and l.split(" ", 1)[1].strip().startswith("store")]
    fichier = Path.home() / ".git-credentials"
    if not stores and not fichier.exists():
        return
    warn("Git enregistre des identifiants en clair sur ce poste.")
    if cfg.get("store_garde"):
        info("Reglage garde a la demande : credential.helper store, ~/.git-credentials.")
        return
    garde = False
    if stores:
        if ask_yn("Retirer le reglage credential.helper store de Git ?", True):
            for cle in stores:
                git("config", "--global", "--unset-all", cle, "^store", quiet=True)
            ok("Reglage retire.")
        else:
            garde = True
    if fichier.exists():
        if ask_yn(f"Supprimer {fichier}, qui contient ces identifiants en clair ?", True):
            os.remove(fichier)
            ok("Fichier supprime.")
            info("Revoquer aussi les jetons qu'il contenait : https://github.com/settings/tokens")
        else:
            garde = True
    if garde:
        cfg["store_garde"] = True
        sauver(cfg)


def chemin_gh():
    trouve = shutil.which("gh")
    if trouve:
        return trouve
    if os.name == "nt":  # juste apres un winget install, le PATH n'est pas a jour
        for base in (os.environ.get("ProgramFiles", r"C:\Program Files"),
                     os.environ.get("LOCALAPPDATA", "")):
            cand = os.path.join(base, "GitHub CLI", "gh.exe") if base else ""
            if cand and os.path.isfile(cand):
                return cand
    return None


def helper_gcm():
    if not _CACHE_GCM:
        trouve = None
        for nom in ("manager", "manager-core"):
            if git(f"credential-{nom}", "--version", quiet=True)[0] == 0:
                trouve = nom
                break
        _CACHE_GCM.append(trouve)
    return _CACHE_GCM[0]


def methode_dispo(cfg):
    pref = cfg.get(cle("methode"))
    if pref == "gh" and chemin_gh():
        return "gh"
    if pref == "gcm" and helper_gcm():
        return "gcm"
    if chemin_gh():
        return "gh"
    if helper_gcm():
        return "gcm"
    return None


def _cmd_gcm(action):
    return ["git", "-c", "credential.helper=", "-c",
            f"credential.helper={helper_gcm()}", "credential", action]


def credential_gcm(cfg, interactif):
    """Demande les identifiants a GCM. Interactif : ouvre le navigateur si besoin."""
    entree = f"protocol=https\nhost={host(cfg)}\n\n"
    env = dict(os.environ)
    if not interactif:
        env["GCM_INTERACTIVE"] = "never"
        env["GIT_TERMINAL_PROMPT"] = "0"
    p = run(_cmd_gcm("fill"), entree, env)
    if p.returncode != 0:
        return ""
    champs = dict(l.split("=", 1) for l in p.stdout.splitlines() if "=" in l)
    tok = champs.get("password", "")
    if tok and interactif:
        run(_cmd_gcm("approve"), p.stdout, env)  # memorise pour les push
    return tok


def oublier_gcm(cfg):
    run(_cmd_gcm("reject"), f"protocol=https\nhost={host(cfg)}\n\n")


def configurer_helper_gcm(serveur):
    """Fait passer git push/pull par GCM pour ce serveur (comme gh auth setup-git)."""
    cle = f"credential.https://{serveur}.helper"
    git("config", "--global", "--unset-all", cle, quiet=True)
    git("config", "--global", "--add", cle, "", quiet=True)
    git("config", "--global", "--add", cle, helper_gcm(), quiet=True)


def token_silencieux(cfg, methode):
    if methode == "gh":
        p = run([chemin_gh(), "auth", "token", "-h", host(cfg)])
        return p.stdout.strip() if p.returncode == 0 else ""
    if methode == "gcm":
        return credential_gcm(cfg, interactif=False)
    return ""


def _json(brut):
    texte = brut.decode("utf-8", "replace") if brut else ""
    if not texte.strip():
        return {}
    try:
        return json.loads(texte)
    except json.JSONDecodeError:
        return {"message": texte[:400]}


def requete(cfg, verbe, chemin, data=None, token=None):
    """Appel REST GitHub brut. Retourne (status, json, en-tetes)."""
    tok = SESSION["token"] if token is None else token
    url = chemin if chemin.startswith("http") else api_base(cfg) + chemin
    corps = json.dumps(data).encode() if data is not None else None
    req = urllib.request.Request(url, data=corps, method=verbe)
    if tok:
        req.add_header("Authorization", f"Bearer {tok}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("X-GitHub-Api-Version", "2022-11-28")
    req.add_header("User-Agent", "gitmanager-script")
    if corps:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, _json(r.read()), r.headers
    except urllib.error.HTTPError as e:
        return e.code, _json(e.read()), (e.headers or {})
    except (urllib.error.URLError, OSError) as e:
        return 0, {"message": f"reseau : {getattr(e, 'reason', e)}"}, {}


def identite_par_defaut(cfg, user, cle):
    user = user or {}
    if cle == "user.name":
        return user.get("name") or user.get("login") or ""
    if host(cfg) == "github.com" and user.get("id") and user.get("login"):
        # Adresse privee fournie par GitHub : pas de blocage "email prive" au push
        return f"{user['id']}+{user['login']}@users.noreply.github.com"
    return user.get("email") or ""


def configurer_identite(cfg, user):
    """Renseigne user.name / user.email globaux s'ils manquent."""
    for cle in ("user.name", "user.email"):
        _, val, _ = git("config", "--global", "--get", cle, quiet=True)
        if val.strip():
            continue
        valeur = identite_par_defaut(cfg, user, cle) or ask("Email pour les commits")
        git("config", "--global", cle, valeur, quiet=True)
        ok(f"git {cle} = {valeur}")
    _, branche, _ = git("config", "--global", "--get", "init.defaultBranch", quiet=True)
    if not branche.strip():
        git("config", "--global", "init.defaultBranch", "main", quiet=True)


def demarrer_session(cfg, methode, tok, user):
    SESSION.update(token=tok, user=user, methode=methode)
    cfg[cle("methode")], cfg[cle("login")] = methode, user.get("login")
    sauver(cfg)
    configurer_identite(cfg, user)


def demander_secret(serveur, avec_utilisateur=False):
    """Jeton (ou mot de passe) saisi sans affichage. Il reste en memoire :
    rien n'est ecrit sur le disque. Retourne (utilisateur, secret)."""
    info(f"Jeton a creer sur https://{serveur}/settings/tokens. Il reste en memoire,")
    info("rien n'est ecrit sur le disque.")
    utilisateur = ask("Nom d'utilisateur, Entree pour un jeton GitHub", "",
                      obligatoire=False) if avec_utilisateur else ""
    try:
        secret = getpass.getpass(c("  Jeton ou mot de passe, saisie masquee : ", "1"))
    except (EOFError, KeyboardInterrupt):
        secret = ""
    return utilisateur, secret.strip()


def connexion_jeton(cfg):
    """Connexion par jeton d'acces, garde en memoire le temps de la session."""
    _, tok = demander_secret(host(cfg))
    if not tok:
        warn("Aucun jeton saisi.")
        return False
    return ouvrir_session_memoire(cfg, "jeton", tok)


# Connexion par code, gardee en memoire : GitHub App de gitmanager. Le Client ID
# n'est pas un secret ; la connexion par code ne demande aucun mot de passe de
# l'application. Les jetons obtenus expirent au bout de 8 heures.
CLIENT_ID_GITHUB = ""
APP_GITHUB = ""  # nom de la GitHub App dans son adresse : github.com/apps/<nom>


def client_id(cfg):
    return os.environ.get("GITMANAGER_CLIENT_ID") or cfg.get("client_id") or CLIENT_ID_GITHUB


def oublier_connexion_memoire():
    if SESSION["token"] and SESSION["methode"] in EN_MEMOIRE:
        SESSION.update(token="", user=None, methode=None)
        warn("Connexion oubliee apres 15 minutes sans activite.")


def _formulaire(url, champs):
    """POST de formulaire vers GitHub, reponse JSON (connexion par code)."""
    req = urllib.request.Request(url, data=urllib.parse.urlencode(champs).encode(),
                                 method="POST", headers={"Accept": "application/json",
                                                         "User-Agent": "gitmanager"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read().decode("utf-8") or "{}")


def ouvrir_session_memoire(cfg, methode, tok):
    """Verifie le jeton et ouvre une session gardee en memoire seulement."""
    code, user, _ = requete(cfg, "GET", "/user", token=tok)
    if code != 200:
        err(f"Connexion refusee, code {code or 'reseau'} : {message_api(user)}")
        return False
    SESSION.update(token=tok, user=user, methode=methode)
    cfg[cle("login")] = user.get("login")
    sauver(cfg)
    configurer_identite(cfg, user)
    ok(f"Connecte a GitHub : {user.get('login')}, connexion gardee en memoire seulement.")
    return True


def connexion_par_code(cfg):
    """Connexion par code : un code a saisir sur github.com/login/device, depuis
    n'importe quel appareil. Le jeton reste en memoire et expire en 8 heures."""
    cid, h = client_id(cfg), host(cfg)
    try:
        dem = _formulaire(f"https://{h}/login/device/code", {"client_id": cid})
    except (urllib.error.URLError, OSError, ValueError) as e:
        err(f"Connexion par code impossible : {getattr(e, 'reason', e)}")
        return False
    if "user_code" not in dem:
        err("Connexion par code refusee : " + str(dem.get("error_description") or dem.get("error")))
        return False
    print()
    info("Ouvrir cette adresse sur n'importe quel appareil, un telephone par exemple :")
    print(c(f"       {dem['verification_uri']}", "1"))
    info("puis saisir ce code :")
    print(c(f"       {dem['user_code']}", "1;33"))
    info("En attente de la validation, Ctrl+C pour annuler...")
    intervalle, fin = int(dem.get("interval", 5)), time.time() + int(dem.get("expires_in", 900))
    while time.time() < fin:
        time.sleep(intervalle)
        try:
            rep = _formulaire(f"https://{h}/login/oauth/access_token", {
                "client_id": cid, "device_code": dem["device_code"],
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"})
        except (urllib.error.URLError, OSError, ValueError):
            continue
        erreur = rep.get("error")
        if erreur == "authorization_pending":
            continue
        if erreur == "slow_down":
            intervalle = int(rep.get("interval", intervalle + 5))
            continue
        if erreur or not rep.get("access_token"):
            err("Connexion refusee : " + str(rep.get("error_description") or erreur))
            return False
        if not ouvrir_session_memoire(cfg, "code", rep["access_token"]):
            return False
        if APP_GITHUB:
            code, inst, _ = requete(cfg, "GET", "/user/installations")
            if code == 200 and not inst.get("installations"):
                warn("L'application gitmanager n'est installee sur aucun compte : l'installer ici,")
                info(f"https://{h}/apps/{APP_GITHUB}/installations/new")
        return True
    err("Code expire : recommencer la connexion.")
    return False


def connexion_interactive(cfg):
    """Connexion au compte GitHub : trois moyens, du plus sur au moins sur."""
    code_dispo = bool(client_id(cfg))
    while True:
        print()
        print(f"  Connexion au compte GitHub {nom_compte()}, du moyen le plus sur au moins sur :")
        print()
        print("    1. Par code, gardee en memoire et valable 8 heures"
              + ("" if code_dispo else " : pas encore disponible"))
        print("    2. Par jeton d'acces, garde en memoire")
        print("    3. Par le navigateur, enregistree sur ce poste")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix == "1":
            if not code_dispo:
                warn("La connexion par code demande l'application GitHub de gitmanager,")
                warn("pas encore configuree : choisir un autre moyen.")
                continue
            connecte = connexion_par_code(cfg)
        elif choix == "2":
            warn("Un jeton ouvre le compte tant qu'il n'est pas revoque. Choisir un jeton fin,")
            warn("limite aux repos utiles, qui expire vite, et ne jamais l'ecrire dans un fichier.")
            connecte = connexion_jeton(cfg)
        elif choix == "3":
            warn("Moyen le moins sur : la connexion reste enregistree sur ce poste apres la")
            warn("fermeture du programme. A eviter sur un poste partage ou pro.")
            if not ask_yn("Continuer avec ce moyen ?", False):
                continue
            connecte = connexion_navigateur(cfg)
        else:
            return False
        if connecte:
            cfg[cle("moyen_connexion")] = choix
            cfg[cle("enregistrer_connexion")] = choix == "3"
            sauver(cfg)
            return True


def connexion_navigateur(cfg):
    """Connexion interactive : ouvre le navigateur. Retourne True si connecte."""
    methode = methode_dispo(cfg)
    if not methode:
        warn("Aucun outil de connexion par le navigateur : ni GitHub CLI, ni Git Credential Manager.")
        if not installer_outil("gh"):
            return False
        methode = "gh"
    h = host(cfg)
    print()
    info(f"Connexion a {h} via {NOMS_METHODES[methode]}...")
    tok = ""
    if methode == "gh":
        info("Copier le code a 8 caracteres affiche, appuyer sur Entree,")
        info("puis le coller dans la page qui s'ouvre et cliquer sur 'Authorize'.")
        print()
        p = subprocess.run([chemin_gh(), "auth", "login", "--web", "-h", h, "-p", "https"])
        if p.returncode != 0:
            err("Connexion annulee ou echouee.")
            return False
        run([chemin_gh(), "auth", "setup-git", "-h", h])
        tok = token_silencieux(cfg, "gh")
    else:
        info("Une fenetre GitHub va s'ouvrir : choisir 'Sign in with your browser'.")
        configurer_helper_gcm(h)
        tok = credential_gcm(cfg, interactif=True)
        code, _, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, None, None)
        if code == 401:  # identifiants memorises mais revoques : nouvelle connexion
            oublier_gcm(cfg)
            tok = credential_gcm(cfg, interactif=True)
    code, user, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, {}, {})
    if code != 200:
        err(f"Connexion echouee, code {code or 'reseau'} : {user.get('message', 'aucune reponse')}")
        return False
    demarrer_session(cfg, methode, tok, user)
    ok(f"Connecte a GitHub en tant que {user.get('login')}")
    return True


def assurer_connexion(cfg, interactif=True):
    """Reutilise la connexion en cours ou enregistree, sinon en demande une."""
    if SESSION["token"]:
        return True
    methode = methode_dispo(cfg) if cfg.get(cle("enregistrer_connexion")) else None
    if methode:
        tok = token_silencieux(cfg, methode)
        if tok:
            code, user, _ = requete(cfg, "GET", "/user", token=tok)
            if code == 200:
                demarrer_session(cfg, methode, tok, user)
                return True
            if code == 0:
                warn(f"GitHub injoignable : {user.get('message')}.")
                return False
    if not interactif:
        return False
    return connexion_interactive(cfg)


def deconnexion(cfg):
    methode = SESSION["methode"] or methode_dispo(cfg)
    if methode == "gh":
        subprocess.run([chemin_gh(), "auth", "logout", "-h", host(cfg)])
    elif methode == "gcm":
        oublier_gcm(cfg)
    SESSION.update(token="", user=None, methode=None)
    cfg.pop(cle("login"), None)
    cfg[cle("enregistrer_connexion")] = False
    sauver(cfg)
    ok("Deconnecte de GitHub" + (", connexion effacee de ce poste." if methode in ("gh", "gcm") else "."))


def permission_manquante(code, hdr):
    """Permission demandee par GitHub quand il refuse une action, sinon ''."""
    if code != 403 or not hdr or hdr.get("X-RateLimit-Remaining") == "0":
        return ""
    if hdr.get("X-Accepted-GitHub-Permissions"):  # jeton fin ou application GitHub
        return hdr.get("X-Accepted-GitHub-Permissions")
    accepte = {s.strip() for s in (hdr.get("X-Accepted-OAuth-Scopes") or "").split(",") if s.strip()}
    possede = {s.strip() for s in (hdr.get("X-OAuth-Scopes") or "").split(",") if s.strip()}
    return ", ".join(sorted(accepte)) if accepte and not accepte & possede else ""


def mettre_a_jour_permissions(cfg, besoin, hdr=None):
    """Permission manquante : la faire ajouter, puis reessayer avec le meme jeton.
    Une permission modifiee sur GitHub s'applique sans reconnexion."""
    warn(f"Permission manquante pour cette action : {besoin}")
    scopes = (hdr or {}).get("X-Accepted-OAuth-Scopes") if hdr else ""
    if SESSION["methode"] == "gh" and scopes:
        info("GitHub CLI demande cette permission dans le navigateur.")
        p = subprocess.run([chemin_gh(), "auth", "refresh", "-h", host(cfg), "-s",
                            ",".join(s.strip() for s in scopes.split(",") if s.strip())])
        if p.returncode == 0:
            SESSION["token"] = token_silencieux(cfg, "gh")
            return bool(SESSION["token"])
        return False
    if SESSION["methode"] == "code":
        info("Ajouter cette permission a l'application GitHub de gitmanager, puis accepter")
        info(f"la mise a jour sur https://{host(cfg)}/settings/installations.")
    else:
        info(f"Ajouter cette permission au jeton sur https://{host(cfg)}/settings/tokens,")
        info("sans le regenerer : la modification s'applique tout de suite.")
    return ask_yn("Reessayer apres la modification ?", True)


def api(cfg, verbe, chemin, data=None):
    """Appel API avec la session courante : reconnexion si elle a expire, et
    nouvel essai apres l'ajout d'une permission manquante."""
    if not assurer_connexion(cfg):
        return 0, {"message": "non connecte a GitHub"}, {}
    for _ in range(3):
        code, rep, hdr = requete(cfg, verbe, chemin, data)
        if code == 401:
            SESSION["token"] = ""
            if SESSION["methode"] == "gcm":
                oublier_gcm(cfg)
            warn("Session GitHub expiree : reconnexion.")
            if connexion_interactive(cfg):
                continue
            break
        besoin = permission_manquante(code, hdr)
        if not (besoin and mettre_a_jour_permissions(cfg, besoin, hdr)):
            break
    return code, rep, hdr


def message_api(rep):
    return rep.get("message", "") if isinstance(rep, dict) else ""


def api_liste(cfg, chemin):
    """GET pagine : suit les en-tetes Link jusqu'a la derniere page."""
    tout, url = [], chemin
    while url:
        code, rep, hdr = api(cfg, "GET", url)
        if code != 200 or not isinstance(rep, list):
            err(f"Lecture impossible, code {code} : {message_api(rep)}")
            return None
        tout += rep
        m = re.search(r'<([^>]+)>;\s*rel="next"', hdr.get("Link", "") or "")
        url = m.group(1) if m else None
    return tout


# Etapes git : commit, recuperation, integration, envoi

def erreur_auth(sortie):
    s = (sortie or "").lower()
    return any(k in s for k in (
        "could not read username", "terminal prompts disabled",
        "authentication failed", "invalid username or password",
        "returned error: 401", "returned error: 403"))


def expliquer(sortie):
    s = (sortie or "").lower()
    if "repository not found" in s:
        return "repo introuvable sur le serveur : supprime, renomme, ou droits manquants"
    if "permission to" in s and "denied" in s:
        return "ce compte n'a pas le droit d'ecrire dans ce repo"
    if erreur_auth(sortie):
        return "GitHub refuse l'acces : se reconnecter avec le menu Compte GitHub"
    if "publickey" in s:
        return "cle SSH refusee : relancer pour passer le projet en HTTPS"
    if "could not resolve host" in s or "timed out" in s or "failed to connect" in s:
        return "serveur injoignable : verifier la connexion, le VPN ou le reseau de l'entreprise"
    if "gh001" in s or "file size limit" in s or "large files" in s:
        return "un fichier depasse 100 Mo, la limite de GitHub"
    if "rejected" in s:
        return "le serveur a des changements plus recents : relancer l'envoi"
    return derniere_ligne(sortie) or "erreur inconnue"


def nettoyer_depot(chemin, bavard=True):
    """Repare ce que l'ancienne version du script laissait dans .git/config."""
    noms = remotes(chemin)
    for r in noms:  # un identifiant en clair dans .git/config est un secret expose
        url = url_remote(r, chemin)
        if contient_token(url):
            git("remote", "set-url", r, sans_identifiants(url), cwd=chemin, quiet=True)
            if bavard:
                info(f"Identifiant en clair retire de l'adresse du remote '{r}'.")
    # "git push -u <url>" ecrivait l'URL comme amont de la branche
    _, out, _ = git("config", "--local", "--get-regexp", r"^branch\..*\.remote$",
                    cwd=chemin, quiet=True)
    for ligne in out.splitlines():
        cle, _, valeur = ligne.partition(" ")
        if valeur in noms or not ("://" in valeur or "@" in valeur):
            continue
        cible = next((r for r in noms
                      if cle_depot(url_remote(r, chemin)) == cle_depot(valeur)), None)
        if cible:
            git("config", "--local", cle, cible, cwd=chemin, quiet=True)
        else:
            git("config", "--local", "--unset", cle, cwd=chemin, quiet=True)
            git("config", "--local", "--unset", cle[:-len(".remote")] + ".merge",
                cwd=chemin, quiet=True)
        if bavard:
            info(f"Suivi de branche repare : {cle.split('.')[1]} -> {cible or 'aucun'}.")
    # Nom / email vides en local : ils masquaient la config globale -> commit refuse
    for cle in ("user.name", "user.email"):
        code, val, _ = git("config", "--local", "--get", cle, cwd=chemin, quiet=True)
        if code == 0 and not val.strip():
            git("config", "--local", "--unset-all", cle, cwd=chemin, quiet=True)
            if bavard:
                info(f"{cle} vide retire du projet : il bloquait les commits.")


def assurer_identite(cfg, chemin, auto):
    for cle in ("user.name", "user.email"):
        _, val, _ = git("config", "--get", cle, cwd=chemin, quiet=True)
        if val.strip():
            continue
        valeur = identite_par_defaut(cfg, SESSION["user"], cle)
        if not valeur and auto:
            raise Arret("erreur", f"git {cle} n'est pas configure")
        if not valeur:
            valeur = ask("Nom pour les commits" if cle == "user.name"
                         else "Email pour les commits")
        git("config", "--global", cle, valeur, quiet=True)
        if not auto:
            ok(f"git {cle} = {valeur}")


def proteger_fichiers(chemin, auto):
    """Retire de l'envoi les secrets (.env, cles) et les fichiers > 100 Mo."""
    _, nouveaux, _ = git("diff", "--cached", "--name-only", "-z", "--diff-filter=A",
                         cwd=chemin, quiet=True)
    _, modifies, _ = git("diff", "--cached", "--name-only", "-z", "--diff-filter=M",
                         cwd=chemin, quiet=True)
    nouveaux = [f for f in nouveaux.split("\0") if f]
    modifies = [f for f in modifies.split("\0") if f]

    def trop_gros(f):
        p = os.path.join(chemin, f)
        return os.path.isfile(p) and os.path.getsize(p) > LIMITE_GITHUB

    a_retirer = [f for f in nouveaux
                 if (SENSIBLES.search(f) and not SENSIBLES_OK.search(f)) or trop_gros(f)]
    gros_modifies = [f for f in modifies if trop_gros(f)]
    if not a_retirer and not gros_modifies:
        return []
    tous = a_retirer + gros_modifies
    if not auto:
        print()
        warn("Ces fichiers ne devraient pas aller sur le serveur, secrets ou plus de 100 Mo :")
        for f in tous:
            print("       " + f)
        if not ask_yn("Les exclure de l'envoi et du suivi git ?", True):
            return []
    if a_retirer:
        git("rm", "--cached", "-q", "--", *a_retirer, cwd=chemin, quiet=True)
    if gros_modifies:
        git("reset", "-q", "--", *gros_modifies, cwd=chemin, quiet=True)
    if not auto and a_retirer:
        gi = Path(chemin) / ".gitignore"
        existant = gi.read_text(encoding="utf-8") if gi.exists() else ""
        ajout = "".join(f"/{f}\n" for f in a_retirer if f"/{f}" not in existant.splitlines())
        if ajout:
            sep = "" if not existant or existant.endswith("\n") else "\n"
            gi.write_text(existant + sep + ajout, encoding="utf-8")
            git("add", ".gitignore", cwd=chemin, quiet=True)
            info("Ajoutes au .gitignore.")
    return tous


def committer(cfg, chemin, message=None, auto=False, proposer_gitignore=True):
    """git add -A + git commit. Retourne le nombre de fichiers commites."""
    if not fichiers_modifies(chemin):
        if not auto:
            info("Aucune modification : tous les fichiers sont identiques au dernier commit.")
        return 0
    if not auto and proposer_gitignore and not (Path(chemin) / ".gitignore").exists():
        warn("Pas de .gitignore : risque d'envoyer .venv, node_modules, caches...")
        if ask_yn("En creer un standard pour Python et Node ?", True):
            (Path(chemin) / ".gitignore").write_text(GITIGNORE, encoding="utf-8")
            ok(".gitignore cree")
    code, _, e = git("add", "-A", cwd=chemin, quiet=auto)
    if code != 0:
        raise Arret("erreur", "git add a echoue : " + derniere_ligne(e))
    exclus = proteger_fichiers(chemin, auto)
    if git("diff", "--cached", "--quiet", cwd=chemin, quiet=True)[0] == 0:
        if not auto:
            info("Rien a committer.")
        return 0
    _, ns, _ = git("diff", "--cached", "--name-status", "-M", cwd=chemin, quiet=True)
    changements = lire_name_status(ns)
    if not auto:
        print()
        info(f"Changements a enregistrer : {bilan_changements(changements)}")
        afficher_changements(changements)
        message = ask("Message du commit", message or message_defaut())
    assurer_identite(cfg, chemin, auto)
    code, out, e = git("commit", "-m", message or message_defaut(), cwd=chemin, quiet=auto)
    if code != 0:
        raise Arret("erreur", "commit refuse : " + derniere_ligne(e or out))
    if not auto and exclus:
        info(f"{len(exclus)} fichier(s) exclu(s) de l'envoi.")
    return len(changements)


def recuperer(cfg, chemin, remote, auto):
    """git fetch (avec reconnexion ou passage SSH -> HTTPS en mode perso)."""
    for tentative in (1, 2):
        code, _, e = git("fetch", remote, cwd=chemin, quiet=True)
        if code == 0:
            return
        url = url_remote(remote, chemin)
        if tentative == 1 and not auto:
            if est_ssh(url) and ("publickey" in e or "Permission denied" in e
                                 or "Host key" in e):
                warn("Connexion SSH refusee : aucune cle SSH configuree pour GitHub.")
                h, o, r = infos_url(url)
                if o and ask_yn("Passer ce projet en HTTPS, avec connexion par le navigateur ?", True):
                    git("remote", "set-url", remote, f"https://{h}/{o}/{r}.git", cwd=chemin)
                    continue
            elif erreur_auth(e):
                warn("GitHub demande une connexion.")
                SESSION["token"] = ""
                if connexion_interactive(cfg):
                    continue
        raise Arret("erreur", expliquer(e))


def integrer(cfg, chemin, remote, branche, auto, pour_envoi=False):
    """Prend en compte les commits du serveur. Retourne (branche, nb recus).
    pour_envoi : propose 'ce dossier fait foi' si le serveur a de l'avance."""
    if not ref_existe(chemin, f"refs/remotes/{remote}/{branche}"):
        defaut = branche_defaut_distante(chemin, remote)
        if not defaut or defaut == branche or \
                not ref_existe(chemin, f"refs/remotes/{remote}/{defaut}"):
            return branche, 0  # branche pas encore sur le serveur : premier envoi
        if auto:
            raise Arret("ignore", f"le serveur utilise la branche '{defaut}', "
                                  f"celle de ce dossier s'appelle '{branche}' : ouvrir le projet")
        print()
        warn(f"Sur le serveur la branche principale est '{defaut}', ici c'est '{branche}'.")
        print(f"    1. Renommer '{branche}' en '{defaut}' et continuer")
        print(f"    2. Garder '{branche}' comme branche separee sur le serveur")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix not in ("1", "2"):
            raise Arret("ignore", "envoi annule")
        if choix == "2":
            return branche, 0
        if defaut in branches_locales(chemin):
            raise Arret("erreur", f"une branche locale '{defaut}' existe deja : "
                                  f"faire 'git switch {defaut}' puis recommencer")
        git("branch", "-M", defaut, cwd=chemin)
        branche = defaut

    distante = f"{remote}/{branche}"
    if not a_des_commits(chemin):  # rien en local : on prend la version du serveur
        code, _, e = git("merge", "--ff-only", distante, cwd=chemin, quiet=auto)
        if code != 0:
            raise Arret("erreur", "recuperation impossible : " + derniere_ligne(e))
        return branche, nb_commits(chemin)

    _, retard = avance_retard(chemin, remote, branche) or (0, 0)
    if not retard:
        return branche, 0

    if pour_envoi and not auto:
        print()
        warn(f"Le serveur contient {retard} commit(s) absent(s) de ce dossier :")
        _, log, _ = git("log", "--oneline", "-10", f"HEAD..{distante}", cwd=chemin, quiet=True)
        for l in log.splitlines():
            print("       " + l)
        print("    1. Les recuperer et les combiner avec ce dossier")
        print("    2. Ce dossier fait foi : le serveur prend exactement son contenu")
        print("       les fichiers absents d'ici y seront supprimes, l'historique est garde")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix not in ("1", "2"):
            raise Arret("ignore", "envoi annule")
        if choix == "2":
            code, _, e = git("merge", "-s", "ours", "--allow-unrelated-histories", "-m",
                             "Remplacement par la version de ce dossier", distante, cwd=chemin)
            if code != 0:
                raise Arret("erreur", "remplacement impossible : " + derniere_ligne(e))
            return branche, 0

    if not auto:
        info(f"{retard} commit(s) du serveur a integrer...")
    code, out, e = git("rebase", "--autostash", distante, cwd=chemin, quiet=auto)
    if code == 0:
        return branche, retard
    if operation_en_cours(chemin) != "rebase":
        raise Arret("erreur", "integration impossible : " + derniere_ligne(e or out))
    conflits = fichiers_en_conflit(chemin)
    liste = ", ".join(conflits[:5]) + (" ..." if len(conflits) > 5 else "")
    if not auto:
        print()
        warn("Conflit : le meme passage a ete modifie ici et sur le serveur.")
        for f in conflits:
            print("       " + f)
        if pour_envoi:
            info("Astuce : l'option 2 'ce dossier fait foi' evite ce conflit.")
        if not ask_yn("Annuler ? Rien n'est perdu, les commits restent ici.", True):
            info("Ouvrir ces fichiers, garder la bonne version entre <<<<<<< et >>>>>>>,")
            info("puis : git add .   et   git rebase --continue")
            raise Arret("conflit", f"conflit en cours de resolution : {liste}")
    git("rebase", "--abort", cwd=chemin, quiet=True)
    raise Arret("conflit", f"conflit sur {liste} : rien envoye, le travail local est intact")


def pousser(cfg, chemin, remote, branche, auto=False):
    """git push (apres integration du serveur). Retourne (statut, detail)."""
    if not auto:
        info("Verification du serveur avec git fetch...")
    recuperer(cfg, chemin, remote, auto)
    branche, recus = integrer(cfg, chemin, remote, branche, auto, pour_envoi=True)
    if not a_des_commits(chemin):
        raise Arret("ignore", "aucun commit : rien a envoyer")
    ref = f"refs/remotes/{remote}/{branche}"
    avant = git("rev-parse", ref, cwd=chemin, quiet=True)[1] if ref_existe(chemin, ref) else None
    ar = avance_retard(chemin, remote, branche) if avant else None
    a_envoyer = ar[0] if ar else nb_commits(chemin)
    web = url_web(url_remote(remote, chemin))
    if a_envoyer == 0:
        git("branch", "-q", "--set-upstream-to", f"{remote}/{branche}", branche,
            cwd=chemin, quiet=True)
        if recus:
            return "ok", f"{recus} commit(s) recupere(s) ; le serveur est identique a ce dossier"
        return "a_jour", "rien a envoyer : le serveur a deja exactement ce contenu"

    for tentative in (1, 2, 3):
        code, _, e = git("push", "-u", remote, branche, cwd=chemin, quiet=auto)
        if code == 0:
            break
        refus = "permission to" in e.lower() and "denied" in e.lower()
        if tentative < 3 and not auto and refus \
                and SESSION["methode"] in EN_MEMOIRE + ("gh",):
            if mettre_a_jour_permissions(cfg, "contents=write, ecriture dans ce repo"):
                continue
        if tentative == 1 and not auto and erreur_auth(e) and not refus:
            SESSION["token"] = ""
            if connexion_interactive(cfg):
                continue
        raise Arret("erreur", expliquer(e))

    changements = changements_entre(chemin, avant)
    if not auto:
        print()
        info(("Premier envoi" if not avant else "Ce que le serveur a recu")
             + f" : {bilan_changements(changements)}")
        afficher_changements(changements, 25)
        if avant:
            info("Les fichiers non listes n'ont pas change. Sur GitHub, chaque fichier")
            info("affiche le dernier commit qui l'a vraiment modifie : c'est normal.")
    detail = f"{a_envoyer} commit(s) envoye(s) : {bilan_changements(changements)}"
    if recus:
        detail += f", {recus} recupere(s)"
    return "ok", detail + (f"  ->  {web}" if web else "")


def relier(cfg, chemin):
    return lier_a_github(cfg, chemin)


def preparer(cfg, chemin, auto, lier=True):
    """Verifie branche, remote et operation en cours. Retourne (branche, remote)."""
    nettoyer_depot(chemin, bavard=not auto)
    op = operation_en_cours(chemin)
    if op:
        if auto:
            raise Arret("ignore", f"{op} en cours : conflit a terminer a la main")
        warn(f"Une operation '{op}' est restee en cours : conflit non termine.")
        if not ask_yn("L'annuler pour revenir a l'etat d'avant ?", True):
            raise Arret("conflit", f"pour la terminer : git add .  puis  git {op} --continue")
        git(op, "--abort", cwd=chemin)
    branche = branche_courante(chemin)
    if not branche:
        raise Arret("erreur", "HEAD detachee : se placer sur une branche avec git switch main")
    remote = remote_principal(chemin)
    if not remote:
        if auto or not lier:
            raise Arret("ignore", "pas encore relie a un repo sur le serveur")
        remote = relier(cfg, chemin)
        if not remote:
            raise Arret("erreur", "aucun repo distant associe")
    return branche, remote


def recuperer_projet(cfg, chemin, auto=False):
    """git pull (fetch + rebase). Retourne (statut, detail)."""
    branche, remote = preparer(cfg, chemin, auto, lier=False)
    avant = git("rev-parse", "HEAD", cwd=chemin, quiet=True)[1] if a_des_commits(chemin) else None
    if not auto:
        info("Recuperation depuis le serveur avec git fetch...")
    recuperer(cfg, chemin, remote, auto)
    branche, recus = integrer(cfg, chemin, remote, branche, auto)
    if not recus:
        return "a_jour", "deja a jour : rien de nouveau sur le serveur"
    changements = changements_entre(chemin, avant)
    if not auto:
        info(f"Recu : {bilan_changements(changements)}")
        afficher_changements(changements, 25)
    return "ok", f"{recus} commit(s) recupere(s) : {bilan_changements(changements)}"


def synchroniser(cfg, chemin, message=None, envoyer=True):
    """Version sans question, pour tous les repos d'un dossier."""
    try:
        if not envoyer:
            return recuperer_projet(cfg, chemin, auto=True)
        branche, remote = preparer(cfg, chemin, auto=True)
        committer(cfg, chemin, message, auto=True)
        return pousser(cfg, chemin, remote, branche, auto=True)
    except Arret as a:
        return a.statut, a.detail


ETIQUETTES = {"ok": ("[OK]", "32"), "a_jour": ("[=] ", "36"), "conflit": ("[!] ", "33"),
              "ignore": ("[-] ", "90"), "erreur": ("[X] ", "31")}


def afficher_resultat(statut, detail, nom=""):
    etiquette, couleur = ETIQUETTES.get(statut, ("[?] ", "0"))
    prefixe = f"{nom:<28} " if nom else ""
    print(c(f"  {etiquette} {prefixe}{detail}", couleur))


def executer(fn, *args):
    """Lance une action ; affiche son resultat ou la raison de son arret."""
    try:
        res = fn(*args)
        if isinstance(res, tuple):
            print()
            afficher_resultat(*res)
        return res
    except Arret as a:
        print()
        afficher_resultat(a.statut, a.detail)
        return None


# Dossiers / projets locaux

def nettoyer_chemin(saisie):
    return os.path.normpath(os.path.expandvars(os.path.expanduser(
        saisie.strip().strip('"').strip("'"))))


def choisir_dossier(cfg, question="Dossier du projet", cle="dernier_dossier"):
    defaut = cfg.get(cle) or os.getcwd()
    chemin = nettoyer_chemin(ask(question, defaut))
    if not os.path.isdir(chemin):
        err(f"Dossier introuvable : {chemin}")
        return None
    cfg[cle] = chemin
    sauver(cfg)
    return chemin


def trouver_projets(dossier, profondeur=2):
    """Repos git dans ce dossier et ses sous-dossiers (2 niveaux)."""
    if os.path.exists(os.path.join(dossier, ".git")):
        return [dossier]
    trouves = []
    if profondeur <= 0:
        return trouves
    try:
        entrees = sorted(os.scandir(dossier), key=lambda e: e.name.lower())
    except OSError:
        return trouves
    for e in entrees:
        if e.is_dir() and not e.name.startswith(".") and e.name not in DOSSIERS_IGNORES:
            trouves += trouver_projets(e.path, profondeur - 1)
    return trouves


def index_locaux(cfg):
    """{owner/repo: dossier local} pour les projets deja connus."""
    index = {}
    dossiers = {cfg.get("espace"), os.path.dirname(cfg.get("dernier_dossier") or "")}
    projets = [p for d in filter(None, dossiers) if os.path.isdir(d)
               for p in trouver_projets(d)]
    with ThreadPoolExecutor(max_workers=8) as pool:
        for p, urls in zip(projets, pool.map(urls_remotes, projets)):
            for url in urls.values():
                cle = cle_depot(url)
                if cle:
                    index.setdefault(cle, p)
    return index


def lier_a_github(cfg, chemin):
    """Cree le repo GitHub ou relie un repo existant. Retourne 'origin'."""
    print()
    print("    1. Creer un nouveau repo sur GitHub")
    print("    2. Le relier a un repo GitHub existant")
    print("    0. Annuler")
    choix = ask("Choix", "0")
    if choix not in ("1", "2") or not assurer_connexion(cfg):
        return None
    login = SESSION["user"]["login"]
    if choix == "2":
        cible = ask("Repo : nom, proprietaire/nom ou URL")
        if "://" in cible or cible.startswith("git@"):
            url = sans_identifiants(cible)
        else:
            if "/" not in cible:
                cible = f"{login}/{cible}"
            url = f"https://{host(cfg)}/{cible.removesuffix('.git')}.git"
    else:
        defaut = re.sub(r"[^A-Za-z0-9._-]+", "-", os.path.basename(chemin)).strip("-")
        nom = ask("Nom du repo", defaut or "projet")
        description = ask("Description du repo, facultative", "", obligatoire=False)
        org = ask("Organisation, vide pour le compte personnel", "", obligatoire=False)
        prive = ask_yn("Repo prive ?", True)
        url = creer_depot(cfg, nom, prive, org, description)
        if not url:
            return None
    if "origin" in remotes(chemin):
        git("remote", "set-url", "origin", url, cwd=chemin)
    else:
        git("remote", "add", "origin", url, cwd=chemin)
    return "origin"


def creer_depot(cfg, nom, prive, org="", description=""):
    chemin = f"/orgs/{org}/repos" if org else "/user/repos"
    donnees = {"name": nom, "private": prive}
    if description:
        donnees["description"] = description
    code, rep, _ = api(cfg, "POST", chemin, donnees)
    if code == 201:
        ok(f"Repo cree : {rep.get('html_url')}, {'prive' if prive else 'public'}")
        return rep.get("clone_url")
    if code == 422 and "already exists" in json.dumps(rep):
        proprio = org or SESSION["user"]["login"]
        warn(f"{proprio}/{nom} existe deja sur GitHub.")
        if ask_yn("L'utiliser pour ce projet ?", True):
            code2, rep2, _ = api(cfg, "GET", f"/repos/{proprio}/{nom}")
            if code2 == 200:
                return rep2.get("clone_url")
        return None
    err(f"Creation impossible, code {code} : {message_api(rep)}")
    return None


def depot_du_projet(cfg, chemin):
    """Infos GitHub (API) du repo relie a ce projet, ou None."""
    remote = remote_principal(chemin)
    h, o, r = infos_url(url_remote(remote, chemin)) if remote else (None, None, None)
    if not o or h != host(cfg):
        err("Ce projet n'est pas relie a un repo du compte GitHub connecte.")
        return None
    code, rep, _ = api(cfg, "GET", f"/repos/{o}/{r}")
    if code != 200:
        err(f"Repo {o}/{r} introuvable, code {code} : {message_api(rep)}")
        return None
    return rep


def entete_projet(chemin):
    branche = branche_courante(chemin)
    remote = remote_principal(chemin)
    url = url_remote(remote, chemin) if remote else ""
    modifs = fichiers_modifies(chemin)
    print(c(f"  Projet  : {chemin}", "1"))
    print(f"  Branche : {branche or 'HEAD detachee'}     "
          f"Serveur : {url_web(url) or url or c('non relie', '33')}")
    etat = [f"{len(modifs)} fichier(s) modifie(s) non commite(s)"]
    if remote and branche:
        ar = avance_retard(chemin, remote, branche)
        if ar is None:
            etat.append("pas encore envoye sur le serveur" if a_des_commits(chemin)
                        else "aucun commit")
        else:
            etat += [f"{ar[0]} commit(s) a envoyer", f"{ar[1]} a recuperer"]
    print("  Etat    : " + " | ".join(etat))
    op = operation_en_cours(chemin)
    if op:
        warn(f"Une operation '{op}' est en cours : conflit non termine.")


def voir_modifs(cfg, chemin):
    modifs = fichiers_modifies(chemin)
    print()
    if not modifs:
        info("Aucune modification : les fichiers sont identiques au dernier commit.")
    else:
        info(f"{len(modifs)} fichier(s) modifie(s) depuis le dernier commit :")
        for l in modifs[:60]:
            print(f"       {statut_porcelaine(l):<9} {l[3:]}")
        if len(modifs) > 60:
            print(f"       ... et {len(modifs) - 60} autre(s)")
    branche, remote = branche_courante(chemin), remote_principal(chemin)
    if branche and remote and ref_existe(chemin, f"refs/remotes/{remote}/{branche}"):
        for plage, libelle in ((f"{remote}/{branche}..HEAD", "Commits a envoyer"),
                               (f"HEAD..{remote}/{branche}", "Commits a recuperer")):
            _, log, _ = git("log", "--oneline", "-15", plage, cwd=chemin, quiet=True)
            if log:
                print()
                info(f"{libelle} :")
                for l in log.splitlines():
                    print("       " + l)
    if a_des_commits(chemin):
        print()
        info("Derniers commits de ce dossier :")
        _, log, _ = git("log", "--format=%h  %ad  %s", "--date=short", "-8",
                        cwd=chemin, quiet=True)
        for l in log.splitlines():
            print("       " + l)


def act_commit_push(cfg, chemin):
    """Envoyer : commit des fichiers modifies, puis push."""
    branche, remote = preparer(cfg, chemin, auto=False)
    committer(cfg, chemin)
    return pousser(cfg, chemin, remote, branche)


def act_pull(cfg, chemin):
    return recuperer_projet(cfg, chemin)


def act_ecraser(cfg, chemin):
    warn("Remplace TOUT l'historique du serveur par celui de ce dossier. Irreversible.")
    info("Pour garder l'historique, preferer Envoyer puis 'ce dossier fait foi'.")
    branche, remote = preparer(cfg, chemin, auto=False)
    recuperer(cfg, chemin, remote, auto=False)
    if ref_existe(chemin, f"refs/remotes/{remote}/{branche}"):
        _, perdus, _ = git("log", "--oneline", f"HEAD..{remote}/{branche}",
                           cwd=chemin, quiet=True)
        if perdus:
            warn("Ces commits du serveur seront PERDUS :")
            for l in perdus.splitlines()[:20]:
                print("       " + l)
    print()
    warn(f"Cible : {url_web(url_remote(remote, chemin)) or remote}  branche {branche}")
    if ask("Taper ECRASER en majuscules pour confirmer", "", obligatoire=False) != "ECRASER":
        return "ignore", "annule"
    committer(cfg, chemin)
    if not a_des_commits(chemin):
        raise Arret("ignore", "aucun commit : rien a envoyer")
    code, _, e = git("push", "--force-with-lease", "-u", remote, branche, cwd=chemin)
    if code != 0:
        raise Arret("erreur", expliquer(e))
    return "ok", "le serveur a ete remplace par ce dossier"


def act_visibilite_projet(cfg, chemin):
    d = depot_du_projet(cfg, chemin)
    if d:
        changer_visibilite(cfg, d)


def act_relier(cfg, chemin):
    remote = relier(cfg, chemin)
    if remote:
        return "ok", f"relie a {url_web(url_remote(remote, chemin)) or url_remote(remote, chemin)}"
    return "ignore", "annule"


def act_supprimer_git(cfg, chemin):
    """Supprime le dossier .git (le code reste). Retourne True si fait."""
    dot_git = os.path.join(chemin, ".git")
    warn("Suppression de l'historique git LOCAL, le dossier .git. Les fichiers de code ne sont pas touches.")
    info("Le dossier ne sera plus relie a GitHub. Ce qui a deja ete envoye reste sur GitHub.")
    modifs = fichiers_modifies(chemin)
    remote, branche = remote_principal(chemin), branche_courante(chemin)
    ar = avance_retard(chemin, remote, branche) if remote and branche else None
    if modifs or (ar and ar[0]) or (remote and branche and ar is None and a_des_commits(chemin)):
        warn("Attention : une partie du travail n'est PAS sur GitHub : "
             f"{len(modifs)} fichier(s) non commite(s), "
             f"{ar[0] if ar else 'des'} commit(s) non envoye(s).")
    if ask("Taper SUPPRIMER en majuscules pour confirmer", "", obligatoire=False) != "SUPPRIMER":
        info("Annule.")
        return False
    if os.path.isdir(dot_git):
        supprimer_dossier(dot_git)
    elif os.path.isfile(dot_git):
        os.remove(dot_git)
    if os.path.exists(dot_git):
        err("Suppression incomplete : fermer les programmes qui utilisent ce dossier et reessayer.")
        return False
    ok(f".git supprime : {chemin} est maintenant un simple dossier.")
    return True


def menu_init(cfg, chemin):
    """Dossier sans git. Retourne True si un repo a ete cree."""
    titre("Nouveau projet git")
    info(f"{chemin} n'est pas encore un repo git.")
    print()
    print("    1. git init, creation du repo GitHub, premier commit et push")
    print("    2. git init seulement")
    print("    0. Retour")
    choix = ask("Choix", "0")
    if choix not in ("1", "2"):
        return False
    init_depot(chemin)
    gi = Path(chemin) / ".gitignore"
    if not gi.exists() and ask_yn("Creer un .gitignore standard pour Python et Node ?", True):
        gi.write_text(GITIGNORE, encoding="utf-8")
        ok(".gitignore cree")
    if choix == "1":
        executer(act_commit_push, cfg, chemin)
        pause()
    return True


def menu_projet(cfg, chemin):
    chemin = os.path.normpath(chemin)
    if racine_repo(chemin) is None and not menu_init(cfg, chemin):
        return
    nettoyer_depot(chemin)
    remote = remote_principal(chemin)
    if remote and git("fetch", remote, cwd=chemin, quiet=True)[0] != 0:
        warn("Serveur injoignable : l'etat 'a envoyer / a recuperer' peut etre ancien.")
    actions = {
        "1": act_commit_push, "2": act_pull, "3": act_ecraser, "4": voir_modifs,
        "5": act_visibilite_projet, "6": act_relier,
    }
    while True:
        titre("Repo local")
        entete_projet(chemin)
        relie = bool(remote_principal(chemin))
        print()
        print("    1. Envoyer : commit + push")
        print("    2. Mettre a jour : pull")
        print("    3. Ecraser GitHub avec ce dossier : force push")
        print("    4. Voir les modifications")
        print("    5. Rendre le repo public / prive")
        print("    6. " + ("Relier a un autre repo GitHub" if relie else "Relier a GitHub"))
        print("    7. Supprimer le .git de ce dossier")
        print("    0. Retour")
        choix = ask("Choix", "0")
        if choix == "7":
            if act_supprimer_git(cfg, chemin):
                pause()
                return
            pause()
            continue
        if choix in ("0", "q"):
            return
        fn = actions.get(choix)
        if not fn:
            warn("Choix invalide.")
            continue
        try:
            executer(fn, cfg, chemin)
        except KeyboardInterrupt:
            print()
            warn("Operation interrompue.")
        pause()


def _recent(projet):
    """Date du dernier travail git sur ce projet (pour trier la liste)."""
    for f in ("index", "HEAD"):
        try:
            return os.path.getmtime(os.path.join(projet, ".git", f))
        except OSError:
            pass
    return 0


def projets_de_la_machine(cfg):
    """Projets git trouves aux endroits habituels, les plus recents d'abord."""
    maison = Path.home()
    racines = [cfg.get("espace"), os.path.dirname(cfg.get("dernier_dossier") or "")]
    racines += [str(maison / d) if d else str(maison) for d in EMPLACEMENTS]
    vus, projets = set(), []
    for r in racines:
        if not r or not os.path.isdir(r):
            continue
        for p in trouver_projets(r):
            cle = os.path.normcase(os.path.abspath(p))
            if cle not in vus:
                vus.add(cle)
                projets.append(p)
    return sorted(projets, key=_recent, reverse=True)


def nom_court(chemin):
    maison = str(Path.home())
    if os.path.normcase(chemin).startswith(os.path.normcase(maison) + os.sep):
        return "~" + chemin[len(maison):]
    return chemin


def ouvrir_dossier(cfg, chemin):
    """Ouvre le menu d'un repo ; un dossier sans git propose de l'envoyer sur GitHub."""
    racine = racine_repo(chemin)
    if racine and os.path.normcase(racine) != os.path.normcase(chemin):
        info(f"Ce dossier fait partie du repo git : {racine}")
        chemin = racine
    cfg["dernier_dossier"] = chemin
    sauver(cfg)
    menu_projet(cfg, chemin)


def act_autre_dossier(cfg):
    titre("Envoyer un dossier sur GitHub")
    chemin = choisir_dossier(cfg, "Dossier a envoyer ou repo non liste")
    if chemin:
        ouvrir_dossier(cfg, chemin)


def act_depots_locaux(cfg):
    titre("Gerer les repos locaux")
    info("Recherche des repos git de cet ordinateur...")
    projets = projets_de_la_machine(cfg)[:30]
    print()
    if not projets:
        info("Aucun repo git trouve sur cet ordinateur.")
        if ask_yn("Envoyer un dossier sur GitHub ?", True):
            act_autre_dossier(cfg)
        return
    print(c(f"    {'#':>3}  {'Repo':<44} {'A commiter':>10}  GitHub", "90"))
    for i, (p, (url, modifs, _)) in enumerate(zip(projets, apercu_projets(projets)), 1):
        nom = nom_court(p)
        nom = nom if len(nom) <= 44 else "~" + nom[-43:]
        print(f"    {i:>3}. {nom:<44} {modifs:>10}  {lien_court(url)}")
    print()
    print("      N. Envoyer un autre dossier sur GitHub")
    print("      T. Tous les repos d'un dossier : push ou pull")
    choix = ask("Numero d'un repo, N ou T, Entree pour revenir", "", obligatoire=False).lower()
    if choix.isdigit() and 1 <= int(choix) <= len(projets):
        ouvrir_dossier(cfg, projets[int(choix) - 1])
    elif choix == "n":
        act_autre_dossier(cfg)
    elif choix == "t":
        executer(act_tout, cfg)
        pause()
    elif choix:
        warn("Choix invalide.")


def act_tout(cfg):
    titre("Tous les repos d'un dossier")
    defaut = cfg.get("espace") or os.path.dirname(cfg.get("dernier_dossier") or "") or os.getcwd()
    cfg.setdefault("espace", defaut)
    dossier = choisir_dossier(cfg, "Dossier qui contient les projets", "espace")
    if not dossier:
        return
    projets = trouver_projets(dossier)
    if not projets:
        warn("Aucun projet git trouve dans ce dossier ni dans ses sous-dossiers.")
        return
    print()
    print(c(f"    {'Projet':<28} {'Branche':<12} {'A commiter':>10}  Serveur", "1"))
    for p, (url, modifs, branche) in zip(projets, apercu_projets(projets, True)):
        nom = os.path.relpath(p, dossier) if p != dossier else os.path.basename(p)
        print(f"    {nom[:28]:<28} {branche[:12]:<12} {modifs:>10}  {lien_court(url)}")
    print()
    print("    1. Tout envoyer : commit + push de chaque repo")
    print("    2. Tout mettre a jour : pull de chaque repo")
    print("    0. Annuler")
    mode = ask("Choix", "0")
    if mode not in ("1", "2"):
        return
    message = ask("Message de commit pour les projets modifies", message_defaut()) \
        if mode == "1" else None
    if not assurer_connexion(cfg):
        return
    print()
    bilan = {}
    for i, p in enumerate(projets, 1):
        nom = os.path.relpath(p, dossier) if p != dossier else os.path.basename(p)
        print(c(f"  [{i}/{len(projets)}] {nom}...", "90"), end="\r", flush=True)
        statut, detail = synchroniser(cfg, p, message=message, envoyer=(mode == "1"))
        print(" " * 70, end="\r")
        afficher_resultat(statut, detail, nom[:28])
        bilan[statut] = bilan.get(statut, 0) + 1
    print()
    resume = {"ok": "mis a jour", "a_jour": "deja a jour", "conflit": "en conflit",
              "ignore": "ignore(s)", "erreur": "en erreur"}
    info("Bilan : " + ", ".join(f"{n} {resume[s]}" for s, n in bilan.items()))
    if bilan.get("conflit"):
        info("Projets en conflit : les ouvrir dans Gerer les repos locaux et utiliser Envoyer.")


# Repos GitHub : liste, navigation, visibilite, suppression

def act_depots(cfg):
    titre("Repos GitHub")
    if not assurer_connexion(cfg):
        return
    locaux = index_locaux(cfg)
    filtre = ""
    while True:
        # relue a chaque affichage : un repo ou une permission ajoutes au jeton
        # sur GitHub apparaissent sans reconnexion
        info("Chargement de la liste...")
        repos = api_liste(cfg, "/user/repos?per_page=100&sort=pushed"
                                "&affiliation=owner,collaborator,organization_member")
        if repos is None:
            return
        vue = [d for d in repos if filtre.lower() in d["full_name"].lower()]
        print()
        entete = f"{len(vue)} repo(s)" + (f" contenant '{filtre}'" if filtre else "")
        print(c(f"    {entete}", "1"))
        print(c(f"    {'#':>3}  {'Repo':<42} {'Visib.':<7} {'Envoi':<10}  Sur ce PC", "90"))
        for i, d in enumerate(vue, 1):
            nom = d["full_name"] if len(d["full_name"]) <= 42 else d["full_name"][:41] + "~"
            vis = "prive" if d.get("private") else "public"
            date = (d.get("pushed_at") or d.get("updated_at") or "")[:10]
            loc = "oui" if d["full_name"].lower() in locaux else ""
            print(f"    {i:>3}. {nom:<42} {vis:<7} {date:<10}  {loc}")
        print()
        choix = ask("Numero d'un repo, texte pour filtrer, * pour tout, Entree pour revenir",
                    "", obligatoire=False)
        if not choix:
            return
        if choix.isdigit() and 1 <= int(choix) <= len(vue):
            menu_depot(cfg, vue[int(choix) - 1], locaux)
        else:
            filtre = "" if choix == "*" else choix


def ouvrir_explorateur(chemin):
    """Ouvre le dossier dans le gestionnaire de fichiers du systeme (Explorateur
    sous Windows, Finder sous macOS, celui du bureau sous Linux), sans bloquer."""
    try:
        if os.name == "nt":
            os.startfile(chemin)
        elif sys.platform == "darwin":
            subprocess.Popen(["open", chemin], stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
        else:
            outil = shutil.which("xdg-open") or shutil.which("gio")
            if not outil or not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
                info(f"Pas de bureau graphique : dossier {chemin}")
                return
            cmd = [outil, "open", chemin] if os.path.basename(outil) == "gio" else [outil, chemin]
            subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             start_new_session=True)
        ok(f"Dossier ouvert dans le gestionnaire de fichiers : {chemin}")
    except OSError as e:
        warn(f"Ouverture du dossier impossible, {e} : {chemin}")


def menu_depot(cfg, d, locaux):
    while True:
        code, frais, _ = api(cfg, "GET", f"/repos/{d['full_name']}")
        if code == 200 and isinstance(frais, dict):
            d = frais
        local = locaux.get(d["full_name"].lower())
        vis = "prive" if d.get("private") else "public"
        titre(f"{d['full_name']} : {vis}")
        if d.get("description"):
            info(d["description"])
        info(d["html_url"])
        info(f"Branche principale : {d.get('default_branch') or '-'}    "
             f"Dernier envoi : {(d.get('pushed_at') or '-')[:10]}")
        info(f"Copie sur cet ordinateur : {local or 'aucune'}")
        print()
        print("    1. Parcourir les fichiers")
        print("    2. Voir les commits")
        print("    3. Rendre " + ("PUBLIC" if d.get("private") else "PRIVE"))
        print("    4. " + ("Ouvrir la copie" if local else "Cloner le projet en local"))
        print("    5. Ouvrir dans le navigateur")
        print("    6. Supprimer de GitHub")
        print("    7. Voir les acces a ce repo")
        print("    0. Retour a la liste")
        choix = ask("Choix", "0")
        if choix == "1":
            parcourir_depot(cfg, d)
        elif choix == "2":
            voir_commits(cfg, d)
        elif choix == "3":
            changer_visibilite(cfg, d)
            pause()
        elif choix == "4":
            if not local:
                local = cloner(cfg, d)
                if local:
                    locaux[d["full_name"].lower()] = local
            if local:
                ouvrir_explorateur(local)
                menu_projet(cfg, local)
        elif choix == "5":
            webbrowser.open(d["html_url"])
        elif choix == "7":
            afficher_acces(cfg, d)
            pause()
        elif choix == "6":
            if supprimer_depot(cfg, d):
                pause()
                return "supprime"
            pause()
        else:
            return None


def _contenu(cfg, full, chemin, ref):
    suffixe = f"/{urllib.parse.quote(chemin)}" if chemin else ""
    return api(cfg, "GET", f"/repos/{full}/contents{suffixe}?ref={urllib.parse.quote(ref)}")


def parcourir_depot(cfg, d):
    full, ref = d["full_name"], d.get("default_branch") or "main"
    chemin = ""
    while True:
        code, rep, _ = _contenu(cfg, full, chemin, ref)
        if code == 404 and not chemin:
            info("Ce repo est vide : aucun fichier envoye pour l'instant.")
            pause()
            return
        if code != 200 or not isinstance(rep, list):
            err(f"Lecture impossible, code {code} : {message_api(rep)}")
            pause()
            return
        entrees = sorted(rep, key=lambda e: (e.get("type") != "dir", e["name"].lower()))
        print()
        print(c(f"  {full} : /{chemin}    branche {ref}", "1;36"))
        if chemin:
            print("      0. ..  dossier parent, ou Entree")
        for i, e in enumerate(entrees, 1):
            if e.get("type") == "dir":
                print(f"    {i:>3}. {e['name']}/")
            else:
                print(f"    {i:>3}. {e['name']:<48} {taille(e.get('size')):>9}")
        choix = ask("Numero d'un dossier ou d'un fichier, Entree pour revenir", "",
                    obligatoire=False)
        if not choix and not chemin:
            return
        if choix in ("", "0", "..") and chemin:
            chemin = chemin.rsplit("/", 1)[0] if "/" in chemin else ""
        elif choix.isdigit() and 1 <= int(choix) <= len(entrees):
            e = entrees[int(choix) - 1]
            if e.get("type") == "dir":
                chemin = e["path"]
            elif e.get("type") == "file":
                afficher_fichier(cfg, full, e["path"], ref, d)
            else:
                info(f"{e['name']} : {e.get('type')}, sous-module ou lien, non affichable.")


def afficher_fichier(cfg, full, chemin, ref, d):
    code, rep, _ = _contenu(cfg, full, chemin, ref)
    if code != 200 or not isinstance(rep, dict):
        err(f"Lecture impossible, code {code} : {message_api(rep)}")
        return
    print()
    print(c(f"  {chemin}   {taille(rep.get('size'))}", "1"))
    code2, commits, _ = api(cfg, "GET", f"/repos/{full}/commits?path="
                                        f"{urllib.parse.quote(chemin)}&sha="
                                        f"{urllib.parse.quote(ref)}&per_page=1")
    if code2 == 200 and isinstance(commits, list) and commits:
        cm = commits[0]["commit"]
        info(f"Dernier commit sur ce fichier : {cm['message'].splitlines()[0][:60]}"
             f", {cm['author']['name']}, {cm['author']['date'][:10]}")
    brut = None
    if rep.get("encoding") == "base64" and rep.get("content"):
        brut = base64.b64decode(rep["content"])
    if brut is None:
        info("Fichier trop gros pour etre affiche ici : plus de 1 Mo.")
    elif b"\0" in brut[:8000]:
        info("Fichier binaire, image ou archive : non affichable ici.")
    else:
        lignes = brut.decode("utf-8", "replace").splitlines()
        print()
        for i, l in enumerate(lignes[:80], 1):
            print(c(f"  {i:>4} ", "90") + l)
        if len(lignes) > 80:
            info(f"... {len(lignes) - 80} ligne(s) de plus : o pour tout voir sur GitHub")
    if ask("o pour ouvrir sur GitHub, Entree pour revenir", "", obligatoire=False).lower() == "o":
        webbrowser.open(rep.get("html_url") or d["html_url"])


def voir_commits(cfg, d):
    full = d["full_name"]
    code, rep, _ = api(cfg, "GET", f"/repos/{full}/commits?per_page=25")
    if code == 409:
        info("Repo vide : aucun commit.")
        pause()
        return
    if code != 200 or not isinstance(rep, list):
        err(f"Lecture impossible, code {code} : {message_api(rep)}")
        pause()
        return
    while True:
        print()
        print(c(f"  Derniers commits de {full}", "1;36"))
        for i, cm in enumerate(rep, 1):
            infos = cm["commit"]["author"]
            auteur = (cm.get("author") or {}).get("login") or infos.get("name") or "?"
            msg = cm["commit"]["message"].splitlines()[0][:48]
            print(f"    {i:>3}. {cm['sha'][:7]}  {infos['date'][:10]}  {auteur[:14]:<14} {msg}")
        choix = ask("Numero d'un commit pour voir les fichiers changes, Entree pour revenir",
                    "", obligatoire=False)
        if not choix:
            return
        if choix.isdigit() and 1 <= int(choix) <= len(rep):
            details_commit(cfg, full, rep[int(choix) - 1]["sha"])


def details_commit(cfg, full, sha):
    code, cm, _ = api(cfg, "GET", f"/repos/{full}/commits/{sha}")
    if code != 200 or not isinstance(cm, dict):
        err(f"Lecture impossible, code {code} : {message_api(cm)}")
        return
    libelles = {"added": "ajoute", "modified": "modifie", "removed": "supprime",
                "renamed": "renomme", "changed": "modifie", "copied": "copie"}
    infos = cm["commit"]["author"]
    print()
    print(c(f"  {sha[:7]}  {cm['commit']['message'].splitlines()[0]}", "1"))
    info(f"{infos.get('name')}, {infos['date'][:16].replace('T', ' ')}")
    fichiers = cm.get("files") or []
    info(f"{len(fichiers)} fichier(s) change(s) par ce commit :")
    for f in fichiers[:60]:
        nom = f["filename"]
        if f.get("previous_filename"):
            nom = f"{f['previous_filename']} -> {nom}"
        print(f"       {libelles.get(f['status'], f['status']):<9} {nom}"
              f"   (+{f.get('additions', 0)} -{f.get('deletions', 0)})")
    if len(fichiers) > 60:
        print(f"       ... et {len(fichiers) - 60} autre(s)")
    pause()


def cloner(cfg, d):
    defaut = cfg.get("espace") or os.getcwd()
    parent = nettoyer_chemin(ask("Cloner dans le dossier", defaut))
    dest = os.path.join(parent, d["name"])
    if os.path.exists(dest) and (not os.path.isdir(dest) or os.listdir(dest)):
        err(f"{dest} existe deja et n'est pas vide.")
        return None
    os.makedirs(parent, exist_ok=True)
    if git_direct("clone", d["clone_url"], dest) != 0:
        err("Clone echoue : verifier la connexion et les droits, puis reessayer apres le menu Compte GitHub.")
        return None
    cfg["dernier_dossier"] = dest
    sauver(cfg)
    ok(f"Clone dans {dest}")
    return dest


def sensibles_sur_github(cfg, d):
    """Fichiers secrets (.env, cles privees...) presents dans la branche principale."""
    ref = urllib.parse.quote(d.get("default_branch") or "main")
    code, rep, _ = api(cfg, "GET", f"/repos/{d['full_name']}/git/trees/{ref}?recursive=1")
    if code != 200 or not isinstance(rep, dict):
        return []
    return [e["path"] for e in rep.get("tree", []) if e.get("type") == "blob"
            and SENSIBLES.search(e["path"]) and not SENSIBLES_OK.search(e["path"])]


def changer_visibilite(cfg, d):
    vers_prive = not d.get("private")
    if not vers_prive:
        warn("Rendre public expose tout le code ET tout l'historique des commits.")
        sensibles = sensibles_sur_github(cfg, d)
        if sensibles:
            err("Ce repo contient des fichiers secrets, qui deviendraient publics :")
            for f in sensibles[:20]:
                print("       " + f)
            info("Les retirer et changer les secrets qu'ils contiennent avant de publier.")
            if ask("Taper PUBLIC pour le rendre public quand meme", "",
                   obligatoire=False) != "PUBLIC":
                info("Annule.")
                return
        elif not ask_yn(f"Rendre {d['full_name']} PUBLIC ?", False):
            info("Annule.")
            return
    elif not ask_yn(f"Rendre {d['full_name']} PRIVE ?", True):
        info("Annule.")
        return
    code, rep, _ = api(cfg, "PATCH", f"/repos/{d['full_name']}", {"private": vers_prive})
    if code == 200:
        d["private"] = vers_prive
        ok(f"{d['full_name']} est maintenant {'prive' if vers_prive else 'public'}.")
    else:
        err(f"Echec, code {code} : {message_api(rep)}")


def supprimer_depot(cfg, d):
    warn("Suppression DEFINITIVE sur GitHub : code, issues et historique.")
    info("Les copies sur cet ordinateur ne sont pas touchees.")
    if ask(f"Taper '{d['name']}' pour confirmer", "", obligatoire=False) != d["name"]:
        info("Annule.")
        return False
    code, rep, _ = api(cfg, "DELETE", f"/repos/{d['full_name']}")
    if code == 204:
        ok(f"{d['full_name']} supprime.")
        return True
    if code == 403:
        warn("Cette connexion n'a pas le droit de supprimer des repos.")
        info("La page des parametres s'ouvre : tout en bas, 'Delete this repository'.")
        webbrowser.open(d["html_url"] + "/settings")
        return False
    err(f"Echec, code {code} : {message_api(rep)}")
    return False


def afficher_permissions(cfg):
    """Permissions de la connexion, relues sur GitHub a chaque affichage."""
    code, _, hdr = requete(cfg, "GET", "/user")
    if code != 200:
        err(f"Lecture impossible, code {code}.")
        return
    print()
    scopes = hdr.get("X-OAuth-Scopes")
    if scopes:
        info(f"Permissions du jeton : {scopes}")
    elif SESSION["methode"] in EN_MEMOIRE:
        info("Jeton fin : permissions choisies repo par repo sur GitHub.")
    expiration = hdr.get("GitHub-Authentication-Token-Expiration")
    if expiration:
        info(f"Expiration du jeton : {expiration}")
    repos = api_liste(cfg, "/user/repos?per_page=100"
                           "&affiliation=owner,collaborator,organization_member")
    if repos is not None:
        ecriture = sum(1 for r in repos if (r.get("permissions") or {}).get("push"))
        info(f"Repos accessibles : {len(repos)}, dont {ecriture} en ecriture.")
    info("Ces informations sont relues a chaque fois : une permission modifiee sur")
    info("GitHub s'applique sans reconnexion.")


def act_compte(cfg):
    titre("Compte GitHub")
    assurer_connexion(cfg, interactif=False)
    user = SESSION["user"]
    if user:
        ok(f"Connecte : {user.get('login')}, {NOMS_METHODES[SESSION['methode']]}, {host(cfg)}")
    else:
        warn(f"Non connecte a {host(cfg)}.")
    print()
    print("    1. Changer de moyen de connexion")
    print("    2. Se deconnecter")
    print("    3. Changer de serveur GitHub Entreprise")
    print("    4. Voir les permissions de la connexion")
    print("    0. Retour")
    choix = ask("Choix", "0")
    if choix == "4":
        afficher_permissions(cfg)
    elif choix == "1":
        SESSION.update(token="", user=None, methode=None)
        connexion_interactive(cfg)
    elif choix == "2":
        deconnexion(cfg)
    elif choix == "3":
        nouveau = ask("Serveur, par exemple github.com ou github.monentreprise.com", host(cfg))
        cfg[cle("host")] = re.sub(r"^https?://", "", nouveau).strip("/").lower()
        sauver(cfg)
        SESSION.update(token="", user=None, methode=None)
        connexion_interactive(cfg)


# Ordinateur entreprise : repo recupere depuis GitHub -> repo de l'entreprise

def verifier_url(url):
    """Refuse les adresses non chiffrees (http://, git://) et retire tout
    identifiant ecrit dans l'adresse."""
    url = url.strip().strip('"').strip("'")
    if re.match(r"^(http|git)://", url, re.I):
        raise Arret("erreur", "adresse non chiffree refusee : utiliser https:// ou SSH")
    return sans_identifiants(url)


def url_source(saisie):
    """proprietaire/nom -> repo GitHub ; sinon URL https / SSH ou dossier local."""
    s = saisie.strip().strip('"').strip("'")
    local = nettoyer_chemin(s)
    if os.path.isdir(local):
        return local
    if re.fullmatch(r"[\w.-]+/[\w.-]+", s):
        return f"https://github.com/{s.removesuffix('.git')}.git"
    return verifier_url(s)


def recuperer_source(cfg, url, dossier):
    """Clone leger (dernier commit seulement) du repo a recuperer : sans
    identifiant s'il est public, sinon avec le compte GitHub connecte.
    Retourne (dossier du projet, True si un jeton a servi)."""
    cible = os.path.join(dossier, "projet")
    args = ("clone", "--depth", "1", "--no-tags", "--quiet", url, cible)
    info(f"Recuperation de {url_web(url) or url}...")
    code, _, e = git(*args, quiet=True, env=_env_git(anonyme=True))
    avec_jeton = False
    if code != 0:
        if est_ssh(url) or not (erreur_auth(e) or "not found" in e.lower()):
            raise Arret("erreur", "recuperation impossible : " + expliquer(e))
        serveur = infos_url(url)[0] or "github.com"
        if SESSION["token"] and serveur == host(cfg):
            info("Repo prive : recuperation avec le compte GitHub connecte.")
            jeton = SESSION["token"]
        else:
            warn("Ce repo n'est pas accessible sans identifiants : il est prive, ou son")
            warn("nom est faux.")
            print("    1. Se connecter au compte perso pour cette recuperation seulement")
            print("    0. Annuler")
            if ask("Choix", "0") != "1":
                raise Arret("ignore", "annule : rien n'a ete modifie")
            _, jeton = demander_secret(serveur)
        if os.path.exists(cible):
            supprimer_dossier(cible)
        code, _, e = git(*args, quiet=True, env=_env_git(identite=("", jeton)))
        jeton = None
        if code != 0:
            raise Arret("erreur", "recuperation impossible : " + expliquer(e))
        avec_jeton = True
    if not a_des_commits(cible):
        raise Arret("erreur", "le repo recupere est vide")
    _, dernier, _ = git("log", "-1", "--format=%h  %s", cwd=cible, quiet=True)
    ok(f"Recupere : {cle_depot(url) or url}, dernier commit {dernier}")
    return cible, avec_jeton


def _cle(rel):
    return rel.lower() if os.name == "nt" else rel


def _exclu(rel):
    return any(m in EXCLUS_COPIE for m in rel.split("/"))


def lister_source(source):
    """{chemin relatif: chemin absolu} des fichiers a copier."""
    fichiers = {}
    for racine, dossiers, noms in os.walk(source):
        dossiers[:] = sorted(d for d in dossiers if d not in EXCLUS_COPIE)
        for n in sorted(noms):
            p = os.path.join(racine, n)
            rel = os.path.relpath(p, source).replace(os.sep, "/")
            if not _exclu(rel):
                fichiers[rel] = p
    return fichiers


def meme_contenu(a, b):
    """Compare deux fichiers ; un lien symbolique est compare a sa cible ecrite,
    jamais suivi (il pourrait pointer hors du projet)."""
    if os.path.islink(a) or os.path.islink(b):
        return os.path.islink(a) and os.path.islink(b) and os.readlink(a) == os.readlink(b)
    return os.path.isfile(b) and filecmp.cmp(a, b, shallow=False)


def plan_copie(source, dest):
    src = lister_source(source)
    nouveaux, modifies, identiques = [], [], 0
    for rel, p in src.items():
        d = os.path.join(dest, *rel.split("/"))
        if not os.path.lexists(d):
            nouveaux.append(rel)
        elif meme_contenu(p, d):
            identiques += 1
        else:
            modifies.append(rel)
    # Fichiers suivis par git dans la destination mais absents du projet
    _, out, _ = git("ls-files", "-z", cwd=dest, quiet=True)
    cles_src = {_cle(r) for r in src}
    en_trop = [f for f in out.split("\0")
               if f and _cle(f) not in cles_src and not _exclu(f)]
    return {"src": src, "nouveaux": nouveaux, "modifies": modifies,
            "identiques": identiques, "en_trop": en_trop}


def dans_dossier(chemin, racine):
    reel = os.path.normcase(os.path.realpath(chemin))
    base = os.path.normcase(os.path.realpath(racine))
    return reel == base or reel.startswith(base.rstrip(os.sep) + os.sep)


def copier_fichier(src, dst, racine):
    """Copie sans jamais ecrire hors du repo (lien symbolique piege)."""
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if not dans_dossier(os.path.dirname(dst), racine):
        raise OSError("chemin hors du repo, par un lien symbolique")
    if os.path.islink(dst):
        os.remove(dst)
    elif os.path.isfile(dst) and not os.access(dst, os.W_OK):
        os.chmod(dst, stat.S_IWRITE | stat.S_IREAD)
    shutil.copy2(src, dst, follow_symlinks=False)


def supprimer_avec_dossiers_vides(fichier, limite):
    if not dans_dossier(os.path.dirname(fichier), limite):
        raise OSError("chemin hors du repo, par un lien symbolique")
    os.remove(fichier)
    d = os.path.dirname(fichier)
    limite = os.path.normcase(os.path.abspath(limite))
    while os.path.normcase(os.path.abspath(d)).startswith(limite + os.sep):
        try:
            os.rmdir(d)
        except OSError:
            break
        d = os.path.dirname(d)


def remplacer_contenu(source, dest, supprimer=True):
    """Copie le projet importe dans le repo de l'entreprise, hors .git. Avec
    supprimer, le repo devient identique au projet : ecraser ; sans, rien n'est
    supprime : mettre a jour. Retourne False si l'utilisateur annule."""
    plan = plan_copie(source, dest)
    a_copier = plan["nouveaux"] + plan["modifies"]
    a_supprimer = plan["en_trop"] if supprimer else []
    print()
    info("Comparaison avec le repo de l'entreprise :")
    print(f"       {plan['identiques']:>5} identique(s)")
    print(f"       {len(plan['modifies']):>5} modifie(s)")
    print(f"       {len(plan['nouveaux']):>5} nouveau(x)")
    if supprimer:
        print(f"       {len(a_supprimer):>5} a supprimer, absents du projet importe")
    elif plan["en_trop"]:
        print(f"       {len(plan['en_trop']):>5} garde(s), absents du projet importe")
    if not a_copier and not a_supprimer:
        info("Le repo de l'entreprise contient deja ce projet.")
        return True
    print()
    for libelle, liste in (("modifie ", plan["modifies"]), ("nouveau ", plan["nouveaux"]),
                           ("supprime", a_supprimer)):
        for rel in liste[:20]:
            print(f"       {libelle}  {rel}")
        if len(liste) > 20:
            print(f"       ... et {len(liste) - 20} autre(s)")
    print()
    if not ask_yn(f"Appliquer au repo de l'entreprise : {len(a_copier)} "
                  f"copie(s), {len(a_supprimer)} suppression(s) ?", True):
        return False
    echecs = []
    for rel in a_supprimer:  # d'abord : un ancien lien symbolique disparait avant la copie
        try:
            supprimer_avec_dossiers_vides(os.path.join(dest, *rel.split("/")), dest)
        except OSError as e:
            echecs.append(f"{rel} : {e.strerror or e}")
    for rel in a_copier:
        try:
            copier_fichier(plan["src"][rel], os.path.join(dest, *rel.split("/")), dest)
        except OSError as e:
            echecs.append(f"{rel} : {e.strerror or e}")
    if echecs:
        warn(f"{len(echecs)} fichier(s) n'ont pas pu etre copies / supprimes :")
        for x in echecs[:10]:
            print("       " + x)
    ok(f"Contenu remplace : {len(a_copier)} copie(s), {len(a_supprimer)} suppression(s)"
       + (f", {len(echecs)} echec(s)." if echecs else "."))
    return True


def mettre_a_jour_destination(cfg, racine, remote, branche):
    """Aligne la destination sur le serveur avant la copie (fast-forward)."""
    modifs = fichiers_modifies(racine)
    if modifs:
        warn(f"La destination a deja {len(modifs)} modification(s) non commitee(s) :")
        for l in modifs[:10]:
            print(f"       {statut_porcelaine(l):<9} {l[3:]}")
        info("Elles partiront avec ce commit.")
        if not ask_yn("Continuer ?", True):
            raise Arret("ignore", "annule")
    info("Mise a jour de la destination depuis le serveur avec git fetch...")
    recuperer(cfg, racine, remote, auto=False)
    ref = f"refs/remotes/{remote}/{branche}"
    if modifs or not ref_existe(racine, ref):
        return
    ar = avance_retard(racine, remote, branche)
    if not a_des_commits(racine) or (ar and ar[1] and not ar[0]):
        code, _, e = git("merge", "--ff-only", f"{remote}/{branche}", cwd=racine)
        if code != 0:
            raise Arret("erreur", "mise a jour de la destination impossible : "
                        + derniere_ligne(e))
        ok("Destination alignee sur la derniere version du serveur.")


DUREE_PUBLIC = 120  # secondes pendant lesquelles le repo reste public apres le clone


def compteurs(cfg, full):
    """Forks, etoiles, vues et clones comptes par GitHub, sans aucune question."""
    etat = {}
    code, rep, _ = requete(cfg, "GET", f"/repos/{full}")
    if code == 200 and isinstance(rep, dict):
        etat["forks"], etat["etoiles"] = rep.get("forks_count", 0), rep.get("stargazers_count", 0)
    for cle, chemin in (("vues", "views"), ("clones", "clones")):
        code, rep, _ = requete(cfg, "GET", f"/repos/{full}/traffic/{chemin}")
        if code == 200 and isinstance(rep, dict):
            etat[cle] = rep.get("count", 0)
    return etat


def _horodatage(texte):
    try:
        return datetime.strptime(texte, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc).timestamp()
    except (TypeError, ValueError):
        return 0


def rapport_exposition(cfg, full, avant, debut, fin):
    """Ce qui s'est passe pendant l'ouverture publique du repo."""
    apres = compteurs(cfg, full)
    duree = int(fin - debut)
    print()
    info(f"Ouverture publique de {full} : {duree // 60} min {duree % 60:02d} s.")
    vu = False
    code, forks, _ = requete(cfg, "GET", f"/repos/{full}/forks?sort=newest&per_page=30")
    nouveaux = [f["full_name"] for f in forks if _horodatage(f.get("created_at")) >= debut - 5] \
        if code == 200 and isinstance(forks, list) else []
    if nouveaux:
        vu = True
        err("Fork cree pendant l'ouverture : il reste public, meme avec le repo prive.")
        for n in nouveaux:
            print("       " + n)
    else:
        print("       forks crees : 0")
    for cle, libelle, propres in (("etoiles", "etoiles ajoutees", 0), ("vues", "vues comptees par GitHub", 0),
                                  ("clones", "autres clones comptes par GitHub", 1)):
        if cle in avant and cle in apres:
            ecart = max(0, apres[cle] - avant[cle] - propres)  # le clone de gitmanager est retire
            print(f"       {libelle} : {ecart}")
            vu = vu or ecart > 0
    if "vues" not in apres:
        info("Vues et clones non disponibles avec cette connexion.")
    if vu:
        warn("Le repo a ete vu ou copie pendant l'ouverture.")
    else:
        ok("Aucune visite comptee pour l'instant.")
    info("GitHub peut mettre plusieurs heures a compter les vues et les clones : le detail")
    info(f"reste visible sur https://{host(cfg)}/{full}/graphs/traffic")


def rendre_prive(cfg, full):
    for _ in range(3):
        code, _, _ = requete(cfg, "PATCH", f"/repos/{full}", {"private": True})
        if code == 200:
            ok(f"{full} est de nouveau prive.")
            return True
        time.sleep(1)
    err(f"{full} EST TOUJOURS PUBLIC : le rendre prive sur https://{host(cfg)}/{full}/settings")
    return False


def compte_a_rebours(secondes, full):
    """Compte a rebours ; Entree l'arrete. Retourne (Entree pressee, evenement)."""
    appui = threading.Event()

    def lire():
        try:
            sys.stdin.readline()
        except Exception:
            pass
        appui.set()

    threading.Thread(target=lire, daemon=True).start()
    fin = time.time() + secondes
    while not appui.is_set():
        reste = int(fin - time.time() + 0.999)
        if reste <= 0:
            break
        print(f"\r  {full} public encore {reste // 60}:{reste % 60:02d}, Entree pour le rendre "
              "prive maintenant ", end="", flush=True)
        appui.wait(1)
    print()
    return appui.is_set(), appui


def recuperer_avec_ouverture(cfg, d, dossier):
    """Repo prive : public le temps du clone, puis prive apres un compte a rebours.
    Sans accord, il est recupere avec le compte connecte, sans devenir public."""
    full = d["full_name"]
    print()
    warn(f"{full} est prive.")
    info("Pour le cloner sans identifiants, il peut devenir public le temps du clone,")
    duree = f"{DUREE_PUBLIC // 60} minutes" if DUREE_PUBLIC % 60 == 0 else f"{DUREE_PUBLIC} secondes"
    info(f"puis redevenir prive apres {duree}. Pendant ce temps, n'importe")
    info("qui peut le voir et le copier, historique compris.")
    sensibles = sensibles_sur_github(cfg, d)
    if sensibles:
        err("Ce repo contient des fichiers secrets, qui deviendraient publics :")
        for chemin in sensibles[:20]:
            print("       " + chemin)
    if not ask_yn("Accepter que le repo devienne public le temps du clone ?", False):
        info("Le repo reste prive : recuperation avec le compte GitHub connecte.")
        return recuperer_source(cfg, d["clone_url"], dossier)[0]
    avant = compteurs(cfg, full)
    code, rep, _ = api(cfg, "PATCH", f"/repos/{full}", {"private": False})
    if code != 200:
        raise Arret("erreur", f"le repo n'a pas pu devenir public, code {code} : {message_api(rep)}")
    debut = time.time()
    warn(f"{full} est public.")
    presse, appui = False, None
    try:
        source = recuperer_source(cfg, d["clone_url"], dossier)[0]
        presse, appui = compte_a_rebours(DUREE_PUBLIC, full)
    finally:
        fin = time.time()
        rendre_prive(cfg, full)  # meme apres une erreur ou un Ctrl+C
    if appui and not presse:
        info("Entree pour continuer.")
        appui.wait()
    rapport_exposition(cfg, full, avant, debut, fin)
    return source


# Option 2 : importer un repo GitHub, l'envoyer dans un repo de l'entreprise

def dossier_bureau():
    """Bureau de l'utilisateur, selon le systeme ; le dossier perso a defaut."""
    maison = Path.home()
    if sys.platform.startswith("linux") and shutil.which("xdg-user-dir"):
        code, out = _sortie(["xdg-user-dir", "DESKTOP"])
        out = out.strip()
        if code == 0 and out and os.path.isdir(out) and os.path.normpath(out) != str(maison):
            return out
    for d in (maison / "Desktop", maison / "OneDrive" / "Desktop", maison / "OneDrive" / "Bureau",
              maison / "Bureau"):
        if d.is_dir():
            return str(d)
    return str(maison)


def copie_sur_bureau(url, nom):
    """Copie locale du repo de l'entreprise sur le Bureau : reutilisee si elle
    existe deja, sinon clonee. Retourne son dossier."""
    bureau = dossier_bureau()
    dest = os.path.join(bureau, nom)
    n = 2
    while os.path.exists(dest):
        if os.path.isdir(os.path.join(dest, ".git")) and \
                (cle_depot(adresse_principale(dest)) or adresse_principale(dest)) == (cle_depot(url) or url):
            info(f"Copie locale deja sur le Bureau : {dest}")
            return dest
        dest = os.path.join(bureau, f"{nom}-{n}")
        n += 1
    info(f"Copie locale du repo de l'entreprise sur le Bureau : {dest}")
    if git_direct("clone", url, dest) != 0:
        raise Arret("erreur", "clone du repo de l'entreprise impossible : verifier l'adresse et les droits")
    return dest


def import_prive(cfg, url, dossier):
    """Repo prive : connexion au compte GitHub perso le temps de l'import, puis
    oubli de cette connexion si elle etait gardee en memoire."""
    deja = bool(PROFILS["perso"]["token"])
    activer_profil("perso")
    try:
        if not connexion_obligatoire(cfg):
            raise Arret("ignore", "import annule")
        _, o, r = infos_url(url)
        code, d, _ = requete(cfg, "GET", f"/repos/{o}/{r}")
        if code == 200 and isinstance(d, dict) and d.get("private") \
                and (d.get("permissions") or {}).get("admin"):
            return recuperer_avec_ouverture(cfg, d, dossier)
        return recuperer_source(cfg, url, dossier)[0]
    finally:
        if not deja and SESSION["methode"] in EN_MEMOIRE:
            SESSION.update(token="", user=None, methode=None)
        activer_profil("entreprise")


def importer_repo(cfg, dossier):
    """Import d'un repo GitHub par son URL, sans identifiants s'il est public.
    Retourne (dossier du projet importe, nom du repo)."""
    saisie = ask("URL du repo GitHub a importer, ou proprietaire/nom, Entree pour revenir",
                 obligatoire=False)
    if not saisie:
        return None, None
    url = url_source(saisie)
    nom = cle_depot(url) or os.path.basename(url.rstrip("/\\")).removesuffix(".git")
    cible = os.path.join(dossier, "projet")
    info(f"Import de {url_web(url) or url}...")
    code, _, e = git("clone", "--depth", "1", "--no-tags", "--quiet", url, cible,
                     quiet=True, env=_env_git(anonyme=True))
    if code != 0:
        if est_ssh(url) or not (erreur_auth(e) or "not found" in e.lower()):
            raise Arret("erreur", "import impossible : " + expliquer(e))
        warn("Ce repo n'est pas accessible sans identifiants : il est prive, ou son nom est faux.")
        if not ask_yn("Se connecter au compte GitHub perso pour cet import ?", True):
            raise Arret("ignore", "import annule")
        if os.path.exists(cible):
            supprimer_dossier(cible)
        cible = import_prive(cfg, url, dossier)
    _, dernier, _ = git("log", "-1", "--format=%h  %s", cwd=cible, quiet=True)
    ok(f"Importe : {nom}, dernier commit {dernier}")
    return cible, nom


def envoyer_vers_entreprise(cfg, source):
    """Envoi du projet importe dans un repo GitHub de l'entreprise, a partir d'une
    copie locale creee sur le Bureau. Retourne "retour" quand Entree ramene au
    menu precedent."""
    titre("Envoyer dans un repo GitHub de l'entreprise")
    info("Chargement des repos du compte GitHub de l'entreprise...")
    repos = api_liste(cfg, "/user/repos?per_page=100&sort=pushed"
                           "&affiliation=owner,collaborator,organization_member") or []
    print()
    if repos:
        print(c(f"    {'#':>3}  {'Repo':<44} Droits", "90"))
        for i, d in enumerate(repos, 1):
            droits = "ecriture" if (d.get("permissions") or {}).get("push") else "lecture"
            print(f"    {i:>3}. {d['full_name'][:44]:<44} {droits}")
    else:
        info("Aucun repo sur ce compte.")
    print()
    print("      U. Saisir l'URL du repo de l'entreprise")
    choix = ask("Numero d'un repo ou U, Entree pour revenir", "", obligatoire=False).lower()
    if choix.isdigit() and 1 <= int(choix) <= len(repos):
        url, nom = repos[int(choix) - 1]["clone_url"], repos[int(choix) - 1]["name"]
    elif choix == "u":
        saisie = ask("URL du repo de l'entreprise, https://... ou git@..., Entree pour revenir",
                     obligatoire=False)
        if not saisie:
            return "retour"
        url = verifier_url(saisie)
        nom = infos_url(url)[2] or os.path.basename(url.rstrip("/\\")).removesuffix(".git")
    else:
        if choix:
            warn("Choix invalide.")
            return "ignore", "annule"
        return "retour"
    print()
    print("    1. Mettre a jour le repo : fichiers ajoutes ou remplaces, aucun supprime")
    print("    2. Ecraser le repo : il prend exactement le contenu du projet importe")
    print("    0. Retour")
    mode = ask("Choix", "0")
    if mode not in ("1", "2"):
        return "retour"
    copie = copie_sur_bureau(url, nom)
    branche, remote = preparer(cfg, copie, auto=False)
    mettre_a_jour_destination(cfg, copie, remote, branche)
    if not remplacer_contenu(source, copie, supprimer=mode == "2"):
        return "ignore", "annule : le repo de l'entreprise n'a pas ete modifie"
    print()
    committer(cfg, copie, proposer_gitignore=False)
    statut, detail = pousser(cfg, copie, remote, branche)
    info(f"Copie locale gardee sur le Bureau : {copie}")
    return statut, detail


def act_importer(cfg):
    titre("Importer un repo GitHub")
    temp = tempfile.mkdtemp(prefix="gitmanager-")
    try:
        source, nom = importer_repo(cfg, temp)
        if not source:
            return None
        while True:
            titre(f"Projet importe : {nom}")
            print("    1. Envoyer dans un repo GitHub de l'entreprise")
            print("    0. Retour")
            if ask("Choix", "0") != "1":
                return None
            if executer(envoyer_vers_entreprise, cfg, source) != "retour":
                pause()
    finally:
        supprimer_dossier(temp)  # aucune copie du projet importe ne reste hors du Bureau


def afficher_acces(cfg, d):
    """Acces du compte connecte a ce repo, relus sur GitHub."""
    niveaux = (("admin", "administration"), ("maintain", "maintenance"), ("push", "ecriture"),
               ("triage", "tri des issues"), ("pull", "lecture"))
    droits = d.get("permissions") or {}
    role = next((libelle for k, libelle in niveaux if droits.get(k)), "aucun")
    login = (SESSION["user"] or {}).get("login")
    if login:
        code, rep, _ = requete(cfg, "GET", f"/repos/{d['full_name']}/collaborators/{login}/permission")
        if code == 200 and isinstance(rep, dict) and rep.get("role_name"):
            role = rep["role_name"]
    print()
    info(f"Acces de {login or 'ce compte'} a {d['full_name']} : role {role}")
    for k, libelle in niveaux:
        print(f"       {libelle:<16} {'oui' if droits.get(k) else 'non'}")


# Menus

# Accueil : le logo de .github/logo.svg, dessine en 24 x 24 points
# (. transparent, f fond, b bord, o orange, w blanc)
LOGO = (
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
)
# Couleurs exactes (24 bits) et leur equivalent en 256 couleurs
COULEURS_LOGO = {"f": ((0x16, 0x1b, 0x22), 234), "b": ((0x30, 0x36, 0x3d), 237),
                 "o": ((0xF0, 0x50, 0x32), 203), "w": ((0xf0, 0xf6, 0xfc), 255)}


def _couleur(nom, fond):
    """Sequence de couleur : 24 bits si le terminal les gere, sinon 256 couleurs."""
    rgb, n256 = COULEURS_LOGO[nom]
    vraies = (os.environ.get("COLORTERM") in ("truecolor", "24bit") or "WT_SESSION" in os.environ
              or os.environ.get("TERM_PROGRAM") in ("iTerm.app", "vscode", "WezTerm"))
    plan = 48 if fond else 38
    return f"{plan};2;{rgb[0]};{rgb[1]};{rgb[2]}" if vraies else f"{plan};5;{n256}"


def afficher_accueil():
    """Logo, nom et accroche, au lancement."""
    print()
    if not COLOR:  # sans couleurs : le trace du logo, en noir et blanc
        lignes = []
        for haut, bas in zip(LOGO[0::2], LOGO[1::2]):
            ligne = "    "
            for h, b in zip(haut, bas):
                ph, pb = h in "bow", b in "bow"
                ligne += "\u2588" if ph and pb else "\u2580" if ph else "\u2584" if pb else " "
            lignes.append(ligne.rstrip())
        try:
            print("\n".join(lignes))
        except UnicodeEncodeError:
            pass
    else:
        lignes = []
        for haut, bas in zip(LOGO[0::2], LOGO[1::2]):
            ligne = "    "
            for h, b in zip(haut, bas):
                if h == "." and b == ".":
                    ligne += " "
                elif h == ".":
                    ligne += f"\033[{_couleur(b, False)}m\u2584\033[0m"
                elif b == ".":
                    ligne += f"\033[{_couleur(h, False)}m\u2580\033[0m"
                else:
                    ligne += f"\033[{_couleur(h, False)};{_couleur(b, True)}m\u2580\033[0m"
            lignes.append(ligne)
        try:
            print("\n".join(lignes))
        except UnicodeEncodeError:
            pass
    print()
    print(c(f"    gitmanager {__version__}", "1;36"))
    print(c("    Coder a la maison, pousser au bureau, gerer les repos GitHub", "36"))
    print(c("    en local comme en ligne.", "36"))



def lancer(fn, cfg, avec_pause=True):
    try:
        executer(fn, cfg)
    except KeyboardInterrupt:
        print()
        warn("Operation interrompue.")
    except Exception as e:
        err(f"Erreur inattendue : {e}")
    if avec_pause:
        pause()


def ligne_compte(cfg):
    print(c(f"  Compte GitHub {nom_compte()} : {SESSION['user'].get('login')}, "
            f"{NOMS_METHODES[SESSION['methode']]}, {host(cfg)}", "90"))


def choisir_fonction(cfg):
    titre("MENU PRINCIPAL")
    for nom, libelle in (("perso", "perso"), ("entreprise", "de l'entreprise")):
        user = PROFILS[nom]["user"]
        if user:
            print(c(f"  Compte GitHub {libelle} : {user.get('login')}", "90"))
    print()
    print("    1. Gerer les repos GitHub perso")
    print("    2. Entreprise : importer un repo, gerer les repos GitHub de l'entreprise")
    print("    0. Quitter")
    return {"1": "perso", "2": "entreprise"}.get(ask("Choix", "0"))


def connexion_obligatoire(cfg):
    """La connexion au compte GitHub de l'option passe toujours en premier :
    sans elle, l'option ne s'ouvre pas."""
    while not SESSION["user"]:
        try:
            return assurer_connexion(cfg)
        except KeyboardInterrupt:
            print()
            warn("Connexion interrompue.")
    return True


def menu_repos(cfg, nom_menu):
    """Repos GitHub, repos locaux et compte, pour le compte actif."""
    while connexion_obligatoire(cfg):
        titre(nom_menu)
        ligne_compte(cfg)
        print()
        print("    1. Gerer les repos GitHub")
        print("    2. Gerer les repos locaux")
        print("    3. Compte GitHub")
        print("    0. Retour")
        choix = ask("Choix", "0")
        if choix in ("0", "q", "quit", "exit"):
            return
        fn = {"1": act_depots, "2": act_depots_locaux, "3": act_compte}.get(choix)
        if fn:
            lancer(fn, cfg, avec_pause=choix == "3")
        else:
            warn("Choix invalide.")


def menu_perso(cfg):
    menu_repos(cfg, "OPTION 1 : REPOS GITHUB PERSO")


def menu_entreprise(cfg):
    while connexion_obligatoire(cfg):
        titre("OPTION 2 : ENTREPRISE")
        ligne_compte(cfg)
        print()
        print("    1. Importer un repo GitHub")
        print("    2. Gerer les repos GitHub de l'entreprise")
        print("    0. Retour")
        choix = ask("Choix", "0")
        if choix in ("0", "q", "quit", "exit"):
            return
        if choix == "1":
            lancer(act_importer, cfg, avec_pause=False)
        elif choix == "2":
            menu_repos(cfg, "OPTION 2 : REPOS GITHUB DE L'ENTREPRISE")
        else:
            warn("Choix invalide.")


def main():
    if sys.version_info < (3, 9):
        print("gitmanager demande Python 3.9 ou plus : le lancer avec "
              "uv run gitmanager.py")
        sys.exit(1)
    cfg = charger()
    afficher_accueil()
    migrer_ancienne_config(cfg)
    if not preparer_outils(cfg):
        sys.exit(1)
    try:
        while True:
            option = choisir_fonction(cfg)
            if not option:
                break
            activer_profil(option)
            if not connexion_obligatoire(cfg):
                info("Pas de connexion : retour au menu principal.")
                continue
            if option == "perso":
                menu_perso(cfg)
            else:
                menu_entreprise(cfg)
    finally:
        for session in PROFILS.values():  # oubli des jetons des deux comptes
            session.update(token="", user=None, methode=None)
    print("\n  A bientot.\n")


def point_entree():
    """Commande `gitmanager` (installee avec : uv tool install ...)."""
    if "--version" in sys.argv[1:]:
        print(f"gitmanager {__version__}")
        return
    try:
        main()
    except (KeyboardInterrupt, EOFError):
        print("\n\n  Interrompu.\n")


if __name__ == "__main__":
    point_entree()
