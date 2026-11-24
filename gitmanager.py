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

Au lancement : logo et accroche ; un compte garde sur cette machine ouvre sa
session tout seul. Sinon, chaque option demande la connexion a son compte
GitHub ; sans connexion, retour au menu principal. Les deux comptes restent
separes. Les textes du programme evitent les mots qui prennent un accent.

  1. Repos GitHub perso, connexion au compte GitHub perso
     - Repos GitHub en ligne : recherche, fichiers, commits, public ou non
       public, ouvrir la copie sur cette machine ou cloner le projet, ouvrir
       la copie en ligne, comparer en ligne et en local dans un tableau a
       deux colonnes puis les deux versions d'un fichier, voir les droits,
       supprimer ; C pour changer de compte, D pour fermer la session.
     - Repos GitHub en local : Envoyer, commit + push avec le message du
       commit au choix ; Recevoir, pull ; Remplacer GitHub par ce dossier,
       force push ; ouvrir la copie sur cette machine ou en ligne ; push ou
       pull de tous les repos d'un dossier ; envoi d'un dossier sur GitHub ;
       dossier sans git : lien avec son repo en ligne, puis pull.
     - Compte GitHub : moyen de connexion, compte, serveur, permissions,
       fin de session.

  2. Entreprise, connexion au compte GitHub de l'entreprise
     - Importer un repo GitHub par son URL, compte/nom ou un dossier de la
       machine. Une copie du repo deja sur la machine sert sans connexion au
       compte perso ; sinon, sans connexion s'il est public, avec le compte
       perso pour un repo non public ; ouverture publique de 2 minutes au
       plus pour le clone, avec accord. Puis envoi dans un repo GitHub de
       l'entreprise : ajouter, sans rien enlever, ou remplacer. L'envoi passe
       par la copie du repo de l'entreprise sur la machine, ou par une copie
       sur le Bureau, qui reste.
     - Repos GitHub de l'entreprise : les choix de l'option 1.

  3. Se connecter, pour un compte sans session.

Les identifiants se gerent dans les menus des repos GitHub, perso ou de
l'entreprise, avec une session ouverte : Fermer la session, puis garder le
compte sur cette machine ou le supprimer ; Supprimer les
identifiants de cette machine, quand il y en a. Le menu principal n'en parle
pas.

Connexion : 1. Se connecter sur le navigateur, avec GitHub CLI ou Git
Credential Manager : le compte reste sur cette machine pour un temps au
choix, cette session, 8 heures, 7 jours, 30 jours ou toujours, puis il est
supprime ; avec GitHub CLI 2.103 ou plus, jeton qui expire et se
renouvelle seul. 2. Saisir un jeton : dans la session seulement, jamais sur
le disque, fin apres 15 minutes sans action et a la fermeture du programme.
La connexion par code apparait quand l'application GitHub de gitmanager est
configuree. Les commandes Git qui changent un repo s'affichent avec leur
resultat.

Tout s'affiche en francais : les messages en anglais de Git, de GitHub CLI,
des installateurs et de GitHub sont traduits ou caches. GITMANAGER_DEBUG=1
garde le detail technique des erreurs.

Securite : HTTPS ou SSH seulement, certificats toujours verifies, aucun
jeton ecrit sur le disque, dans une URL ni sur la ligne de commande ; alerte
si Git ecrit des identifiants en clair.
"""

import base64
import difflib
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

__version__ = "2.11.1"
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

# GITMANAGER_DEBUG=1 : les erreurs gardent leur detail technique, en anglais
DEBUG = bool(os.environ.get("GITMANAGER_DEBUG"))
BRUIT_GIT = re.compile(r"(LF will be replaced by CRLF|CRLF will be replaced by LF)", re.I)
STATUTS = {"A": "nouveau", "M": "modif", "D": "retrait", "R": "nouveau nom",
           "C": "copie", "T": "modif", "U": "conflit"}


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
    print(f"  ->   {t}")


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
    saisie(c("\n  Enter pour continuer", "1"))


def message_defaut():
    return "Commit du " + datetime.now().strftime("%Y-%m-%d %H:%M")


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
        warn("Ancien token : plus dans ~/.gitmanager.json"
             + (" ni dans ~/.git-credentials." if nettoyes else "."))
        info("Il ne sert plus. Le retirer sur GitHub : https://github.com/settings/tokens")
    sauver(cfg)
    if os.environ.pop("GITHUB_TOKEN", None):
        warn("La variable GITHUB_TOKEN ne sert pas : elle bloquerait la connexion par le navigateur.")


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


# Commandes Git affichees avec leur sortie : celles qui changent un repo, sa
# config ou parlent au serveur, jamais les simples lectures.
VOIR_GIT = {"actif": True}
ACTIONS_GIT = {"add", "commit", "push", "pull", "fetch", "clone", "init", "merge", "rebase",
               "reset", "rm", "remote", "checkout", "switch", "branch", "stash", "tag"}


def sous_commande(args):
    args = [str(a) for a in args]
    i = 0
    while i < len(args):
        if args[i] == "-c":
            i += 2
        elif args[i].startswith("-"):
            i += 1
        else:
            return args[i]
    return ""


def commande_visible(args):
    """Commande Git qui change un repo, sa config ou parle au serveur."""
    args = [str(a) for a in args]
    sc = sous_commande(args)
    suite = args[args.index(sc) + 1:] if sc in args else []
    if sc == "config":
        return not any(a.startswith(("--get", "--list", "-l")) for a in suite)
    if sc == "remote":
        return bool(suite) and suite[0] in ("add", "set-url", "remove", "rm", "rename")
    if sc == "branch":
        return any(a in ("-m", "-M", "-d", "-D", "--set-upstream-to") or a.startswith("--set-upstream-to")
                   for a in suite)
    return sc in ACTIONS_GIT


def git(*args, cwd=None, quiet=False, env=None):
    """Execute une commande git. Retourne (code, stdout, stderr)."""
    cmd = ["git", "-c", "core.quotepath=off"] + [str(a) for a in args]
    montrer = VOIR_GIT["actif"] and commande_visible(args)
    if montrer:
        print(c("  $ git " + _affiche(args), "36"))
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True,
                       encoding="utf-8", errors="replace", env=env or _env_git())
    out, errs = (p.stdout or "").rstrip(), (p.stderr or "").strip()
    if montrer:
        for flux in (out, errs):
            for ligne in flux.strip().splitlines():
                texte = traduire_git(ligne)
                if texte:
                    print("     " + texte)
    return p.returncode, out, errs


def git_direct(*args, cwd=None):
    """git avec progression en direct, en francais : clone d'un repo."""
    if VOIR_GIT["actif"]:
        print(c("  $ git " + _affiche(args), "36"))
    args = [str(a) for a in args]
    if args and args[0] == "clone":
        args.insert(1, "--progress")
    p = subprocess.Popen(["git"] + args, cwd=cwd, env=_env_git(), stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                         encoding="utf-8", errors="replace")
    phase, lignes = "", []
    for ligne in p.stdout:  # les retours chariot de la progression coupent aussi les lignes
        ligne = ligne.strip()
        if not ligne:
            continue
        lignes.append(ligne)
        etape = progression_git(ligne)
        if etape:
            nom, pourcent = etape
            if nom != phase and phase:
                print()
            phase = nom
            print(f"\r     {nom} : {pourcent} %   ", end="", flush=True)
    code = p.wait()
    if phase:
        print()
    if code != 0:
        info(expliquer("\n".join(lignes[-5:])))
    return code


PHASES_GIT = (("Enumerating objects", "Liste des objets"), ("Counting objects", "Comptage"),
              ("Compressing objects", "Compression"), ("Receiving objects", "Copie des objets"),
              ("Resolving deltas", "Assemblage"), ("Updating files", "Fichiers"),
              ("Writing objects", "Envoi des objets"))


def progression_git(ligne):
    """Etape de progression de git en francais : (nom, pourcentage) ou None."""
    for anglais, francais in PHASES_GIT:
        m = re.search(anglais + r":\s+(\d+)%", ligne)
        if m:
            return francais, m.group(1)
    return None


# Messages courants de git, en francais ; les autres ne s'affichent pas
TRADUCTIONS_GIT = (
    (r"^\[(\S+)(?: \(root-commit\))? ([0-9a-f]{7,})\] (.*)$", "commit {2} sur {1} : {3}"),
    (r"^(\d+) files? changed(?:, (\d+) insertions?\(\+\))?(?:, (\d+) deletions?\(-\))?",
     "{1} fichier(s), lignes en plus : {2}, en moins : {3}"),
    (r"^rename (.+) \(\d+%\)$", "nouveau nom : {1}"),
    (r"^mode change \d+ => \d+ (.+)$", "mode : {1}"),
    (r"^branch '(.+)' set up to track '(.+)'\.?$", "la branche {1} suit {2}"),
    (r"^To (.+)$", "serveur : {1}"),
    (r"^From (.+)$", "serveur : {1}"),
    (r"^\+?\s*([0-9a-f]+)\.\.\.?([0-9a-f]+)\s+(\S+)\s+->\s+(\S+)\s+\(forced update\)$",
     "{4} : {1} -> {2}, force push"),
    (r"^([0-9a-f]+)\.\.([0-9a-f]+)\s+(\S+)\s+->\s+(\S+)$", "{4} : {1} -> {2}"),
    (r"^\*\s+\[new branch\]\s+(\S+)\s+->\s+(\S+)$", "nouvelle branche : {2}"),
    (r"^\*\s+\[new tag\]\s+(\S+)\s+->\s+(\S+)$", "nouveau tag : {2}"),
    (r"^\*\s+branch\s+(\S+)\s+->\s+(\S+)$", "branche lue : {1}"),
    (r"^Everything up-to-date$", "rien de nouveau"),
    (r"^Already up.to.date\.?$", "rien de nouveau"),
    (r"^Successfully rebased and updated (.+)\.$", "rebase OK"),
    (r"^Fast-forward$", "avance simple"),
    (r"^Updating ([0-9a-f]+)\.\.([0-9a-f]+)$", "{1} -> {2}"),
    (r"^(?:Initialized empty|Reinitialized existing) Git repository in (.+)$", "repo git : {1}"),
    (r"^Switched to (?:a new )?branch '(.+)'$", "branche : {1}"),
    (r"^Created autostash: ([0-9a-f]+)$", "changements, commit manquant : mis en attente"),
    (r"^Applied autostash\.?$", "changements, commit manquant : repris"),
    (r"^Cloning into '(.+)'\.\.\.$", "clone dans {1}"),
    (r"^Auto-merging (.+)$", "fusion auto : {1}"),
    (r"^CONFLICT \(.+\): Merge conflict in (.+)$", "conflit : {1}"),
    (r"^Deleted branch (\S+)", "branche {1} : suppression OK"),
    (r"^rm '(.+)'$", "retrait : {1}"),
    (r"^nothing to commit", "rien pour un commit"),
    (r"^remote: Create a pull request for '(.+)' on GitHub by visiting:$",
     "pull request pour {1} sur GitHub :"),
    (r"^remote:\s+(https://\S+)$", "{1}"),
)


def traduire_git(ligne):
    """Ligne de git en francais, ou None : rien en anglais ne s'affiche."""
    ligne = ligne.strip()
    if not ligne or BRUIT_GIT.search(ligne):
        return None
    for motif, modele in TRADUCTIONS_GIT:
        m = re.search(motif, ligne)
        if m:
            groupes = [""] + [g or "0" for g in m.groups()]
            return re.sub(r"\{(\d)\}", lambda x: groupes[int(x.group(1))], modele)
    return None


def derniere_ligne(texte):
    lignes = [l.strip() for l in (texte or "").splitlines()
              if l.strip() and not BRUIT_GIT.search(l)]
    if not lignes:
        return ""
    return re.sub(r"^(fatal|error|erreur|remote):\s*", "", lignes[-1], flags=re.I)


def racine_repo(chemin):
    code, out, _ = git("rev-parse", "--show-toplevel", cwd=chemin, quiet=True)
    return os.path.normpath(out) if code == 0 and out else None


def acces_repo(chemin):
    """Racine du repo git du dossier. Quand un .git existe mais que Git refuse de
    le lire, le programme dit pourquoi et propose la solution. Retourne
    (racine ou None, etat) : ok, absent, bloque ou illisible."""
    for essai in (1, 2):
        code, out, e = git("rev-parse", "--show-toplevel", cwd=chemin, quiet=True)
        if code == 0 and out:
            return os.path.normpath(out), "ok"
        if "dubious ownership" not in e.lower() or essai == 2:
            break
        # Git bloque un repo dont le dossier appartient a un autre compte du poste
        m = re.search(r"safe\.directory\s+'?([^'\n]+?)'?\s*$", e, re.M)
        cible = m.group(1) if m else chemin
        print()
        warn("Git bloque ce repo : son dossier appartient a un autre compte de cette machine.")
        info("Git protege ainsi les dossiers des autres comptes, par exemple apres une copie")
        info("depuis un autre Mac ou un clone fait avec sudo.")
        if not ask_yn("Faire confiance au dossier ? Git le note dans safe.directory.", True):
            return None, "bloque"
        git("config", "--global", "--add", "safe.directory", cible, quiet=True)
    if not os.path.lexists(os.path.join(chemin, ".git")):
        return None, "absent"
    s = e.lower()
    print()
    err(f"Git ne peut pas lire le dossier .git de {chemin}.")
    if "operation not permitted" in s or "permission denied" in s:
        info("Le systeme bloque ce dossier pour ce terminal : sur macOS, l'autoriser dans")
        info("Reglages Systeme, Confidentialite et securite, Fichiers et dossiers.")
        return None, "bloque"
    if "dubious ownership" in s:
        info("Git bloque encore ce dossier : voir son proprietaire.")
        return None, "bloque"
    info("Le dossier .git est sans doute incomplet, par exemple avec iCloud ou OneDrive.")
    return None, "illisible"


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
    return (cle_depot(url) or url[-30:]) if url else c("sans lien", "33")


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
    ok(f"Nouveau repo git dans {chemin}")


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
    return [("nouveau", f) for f in out.splitlines() if f]


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


def cle(nom, profil=None):
    """Cle de configuration du compte actif, ou du profil donne : les reglages de
    connexion du compte de l'entreprise sont gardes a part de ceux du compte perso."""
    return nom if (profil or PROFIL["nom"]) == "perso" else f"{nom}_entreprise"


def nom_compte(profil=None):
    return "perso" if (profil or PROFIL["nom"]) == "perso" else "de l'entreprise"


# Apres une deconnexion, les menus se referment jusqu'au menu principal
NAVIGATION = {"menu_principal": False}


def cles_enregistrees(cfg, profil=None):
    """Moyen, gh ou gcm, dont les cles de connexion sont enregistrees sur ce poste
    pour le compte actif ou le profil donne, sinon None."""
    profil = profil or PROFIL["nom"]
    m = cfg.get(cle("cles", profil))
    if not m and cfg.get(cle("enregistrer_connexion", profil)):
        m = cfg.get(cle("methode", profil))
    if not m:
        m = PROFILS[profil]["methode"]
    return m if m in ("gh", "gcm") else None
EN_MEMOIRE = ("jeton", "code")  # connexions jamais enregistrees sur le poste
NOMS_METHODES = {"gh": "GitHub CLI", "gcm": "Git Credential Manager",
                 "code": "code, pour cette session",
                 "jeton": "jeton, pour cette session"}
_CACHE_GCM = []


def host(cfg, profil=None):
    return (cfg.get(cle("host", profil)) or "github.com").strip().lower()


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
    ok(f"{version} : installation OK.")
    paquet = empreinte_paquet(g)
    if paquet:
        info(f"Empreinte SHA-256 du paquet, selon {g} :")
        print(f"       {paquet}")
        if g == "winget":
            info("Pour comparer : https://github.com/git-for-windows/git/releases")
    exe = shutil.which("git")
    if exe:
        info("Empreinte SHA-256 du programme git :")
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
            info("Les outils Xcode d'Apple contiennent Git : suivre les instructions.")
            if ask_yn("Les installer maintenant ?", True):
                run(["xcode-select", "--install"])
                info("Relancer le programme une fois l'installation finie.")
            return False
        info(f"Installer {nom} depuis " + ("https://git-scm.com/downloads" if outil == "git"
                                          else "https://cli.github.com") + ", puis relancer.")
        return False
    verbe = "Actualiser" if mise_a_jour else "Installer"
    if not ask_yn(f"{verbe} {nom}, version la plus neuve, avec {g} ?", True):
        return False
    info("Installation en cours, cela peut prendre quelques minutes...")
    if any(cmd[:1] == ["sudo"] for cmd in commandes_paquet(g, PAQUETS[outil][g], mise_a_jour)):
        info("sudo peut demander le mot de passe de cette machine.")
    for cmd in commandes_paquet(g, PAQUETS[outil][g], mise_a_jour):
        print(c("  $ " + " ".join(cmd), "36"))
        p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                           encoding="utf-8", errors="replace")
        if p.returncode != 0:
            warn(f"{g} : code de sortie {p.returncode}.")
            if DEBUG:
                print(p.stdout[-2000:])
    actualiser_path()
    if outil == "gh":
        pret = bool(chemin_gh())
        if pret:
            ok("GitHub CLI : installation OK.")
    else:
        pret = (version_git() or (0, 0)) >= (2, 31)
        if pret:
            afficher_empreinte_git(g)
    if not pret and os.name == "nt":
        warn(f"{nom} : installation OK, mais pas encore visible. Fermer ce terminal, "
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
        warn("Git manque sur ce poste." if version is None else
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
    warn("Git garde des identifiants en clair sur ce poste.")
    if cfg.get("store_garde"):
        info("Option credential.helper store toujours active, sur demande : ~/.git-credentials.")
        return
    garde = False
    if stores:
        if ask_yn("Retirer l'option credential.helper store de Git ?", True):
            for cle in stores:
                git("config", "--global", "--unset-all", cle, "^store", quiet=True)
            ok("Option store : retrait OK.")
        else:
            garde = True
    if fichier.exists():
        if ask_yn(f"Supprimer {fichier}, qui contient ces identifiants en clair ?", True):
            os.remove(fichier)
            ok("Fichier : suppression OK.")
            info("Retirer aussi sur GitHub les jetons qu'il contenait : https://github.com/settings/tokens")
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
    except (urllib.error.URLError, OSError):
        return 0, {"message": "aucune connexion internet"}, {}


def identite_par_defaut(cfg, user, cle):
    user = user or {}
    if cle == "user.name":
        return user.get("name") or user.get("login") or ""
    if host(cfg) == "github.com" and user.get("id") and user.get("login"):
        # Adresse privee fournie par GitHub : pas de blocage "email prive" au push
        return f"{user['id']}+{user['login']}@users.noreply.github.com"
    return user.get("email") or ""


def ecrire_identite(cfg, cle_git, valeur):
    """Ecrit user.name ou user.email dans la configuration globale de Git et note
    que gitmanager l'a ecrit pour ce compte : la suppression du compte l'efface."""
    git("config", "--global", cle_git, valeur, quiet=True)
    cfg.setdefault("identite_ecrite", {})[cle_git] = {"valeur": valeur, "profil": PROFIL["nom"]}
    sauver(cfg)


def identite_ecrite(cfg, profil=None):
    """Cles user.name / user.email ecrites par gitmanager pour ce compte et
    toujours en place avec la meme valeur."""
    profil = profil or PROFIL["nom"]
    restes = []
    for cle_git, note in (cfg.get("identite_ecrite") or {}).items():
        if note.get("profil") == profil:
            _, val, _ = git("config", "--global", "--get", cle_git, quiet=True)
            if val.strip() == note.get("valeur"):
                restes.append(cle_git)
    return restes


def configurer_identite(cfg, user):
    """Renseigne user.name / user.email globaux s'ils manquent."""
    for cle in ("user.name", "user.email"):
        _, val, _ = git("config", "--global", "--get", cle, quiet=True)
        if val.strip():
            continue
        valeur = identite_par_defaut(cfg, user, cle) or ask("Email pour les commits")
        ecrire_identite(cfg, cle, valeur)
    _, branche, _ = git("config", "--global", "--get", "init.defaultBranch", quiet=True)
    if not branche.strip():
        git("config", "--global", "init.defaultBranch", "main", quiet=True)


def demarrer_session(cfg, methode, tok, user):
    SESSION.update(token=tok, user=user, methode=methode)
    cfg[cle("methode")], cfg[cle("login")] = methode, user.get("login")
    if methode in ("gh", "gcm"):
        cfg[cle("cles")] = methode
    sauver(cfg)
    configurer_identite(cfg, user)


def demander_secret(serveur, avec_utilisateur=False):
    """Jeton (ou mot de passe) saisi sans affichage. Il reste en memoire :
    rien n'est ecrit sur le disque. Retourne (utilisateur, secret)."""
    info(f"Faire un jeton sur https://{serveur}/settings/tokens : jeton fin, pour les")
    info("seuls repos utiles, valable peu de temps, jamais dans un fichier.")
    info("Le jeton reste dans la session, jamais sur le disque.")
    utilisateur = ask("Nom d'utilisateur, Enter pour un jeton GitHub", "",
                      obligatoire=False) if avec_utilisateur else ""
    try:
        secret = getpass.getpass(c("  Jeton ou mot de passe, sans affichage : ", "1"))
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
        warn("Fin de session : 15 minutes sans action.")


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
        err(f"Connexion impossible, code {code or 'aucun'} : {message_api(user)}")
        return False
    SESSION.update(token=tok, user=user, methode=methode)
    configurer_identite(cfg, user)
    ok(f"Connexion OK : {user.get('login')}")
    return True


def connexion_par_code(cfg):
    """Connexion par code : un code a saisir sur github.com/login/device, depuis
    n'importe quel appareil. Le jeton reste en memoire et expire en 8 heures."""
    cid, h = client_id(cfg), host(cfg)
    try:
        dem = _formulaire(f"https://{h}/login/device/code", {"client_id": cid})
    except (urllib.error.URLError, OSError, ValueError):
        err("Connexion par code impossible : aucune connexion internet.")
        return False
    if "user_code" not in dem:
        err("Connexion par code impossible : " + str(dem.get("error_description") or dem.get("error")))
        return False
    print()
    info("Ouvrir cette adresse sur un autre appareil, un mobile par exemple :")
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
            err("Connexion impossible : " + str(rep.get("error_description") or erreur))
            return False
        if not ouvrir_session_memoire(cfg, "code", rep["access_token"]):
            return False
        if APP_GITHUB:
            code, inst, _ = requete(cfg, "GET", "/user/installations")
            if code == 200 and not inst.get("installations"):
                warn("L'application gitmanager manque sur ce compte : l'ajouter ici,")
                info(f"https://{h}/apps/{APP_GITHUB}/installations/new")
        return True
    err("Code trop ancien : recommencer la connexion.")
    return False


def connexion_interactive(cfg, autre_compte=False):
    """Connexion au compte GitHub : par le navigateur, puis par jeton. La
    connexion par code apparait seulement quand l'application GitHub de
    gitmanager est configuree. autre_compte : les cles enregistrees sur cette
    machine ne servent pas."""
    code_dispo = bool(client_id(cfg))
    while True:
        print()
        print(f"  Connexion au compte GitHub {nom_compte()} :")
        print()
        print("    1. Se connecter sur le navigateur")
        print("    2. Saisir un jeton")
        if code_dispo:
            print("    3. Se connecter par code")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix == "1":
            warn("Le compte reste sur cette machine, pour le temps choisi ensuite.")
            if not ask_yn("Continuer ?", True):
                continue
            connecte = connexion_navigateur(cfg, autre_compte)
        elif choix == "2":
            connecte = connexion_jeton(cfg)
        elif choix == "3" and code_dispo:
            connecte = connexion_par_code(cfg)
        elif choix in ("0", "q"):
            return False
        else:
            warn("Choix invalide.")
            continue
        if connecte:
            cfg[cle("enregistrer_connexion")] = choix == "1"
            cfg.pop(cle("moyen_connexion"), None)
            sauver(cfg)
            return True


def gh_par_navigateur(args):
    """Commande GitHub CLI qui passe par le navigateur, sans son texte en anglais :
    le code a usage unique et l'adresse s'affichent en francais, et la page
    s'ouvre dans le navigateur. Retourne le code de sortie de GitHub CLI."""
    env = dict(os.environ, GH_PROMPT_DISABLED="1", NO_COLOR="1", GH_NO_UPDATE_NOTIFIER="1")
    p = subprocess.Popen([chemin_gh()] + args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace",
                         env=env)
    code_vu, adresse_vue = False, False
    for ligne in p.stdout:
        code = re.search(r"\b([A-Z0-9]{4}-[A-Z0-9]{4})\b", ligne)
        adresse = re.search(r"(https://\S+)", ligne)
        if code and not code_vu:
            code_vu = True
            print()
            info("Code pour la page GitHub :")
            print(c(f"       {code.group(1)}", "1;33"))
        elif adresse and not adresse_vue and "device" in adresse.group(1):
            adresse_vue = True
            ouvrir_navigateur(adresse.group(1).rstrip("."))
            info("Coller le code dans la page, puis cliquer sur 'Authorize'.")
            info("Attente de la validation, Ctrl+C pour annuler...")
        elif "Authentication complete" in ligne:
            ok("Autorisation GitHub OK.")
        elif DEBUG and ligne.strip():
            print("     " + ligne.rstrip())
    return p.wait()


def gh_jetons_expirants():
    """GitHub CLI 2.103 ou plus sait demander un jeton qui expire et se renouvelle
    seul : option --expiring, plus sure qu'un jeton sans fin."""
    p = run([chemin_gh(), "--version"])
    m = re.search(r"(\d+)\.(\d+)\.\d+", p.stdout or "")
    return bool(m) and (int(m.group(1)), int(m.group(2))) >= (2, 103)


def rafraichir_session(cfg):
    """Nouveau jeton sans question pour un compte de cette machine, par exemple
    quand le jeton de GitHub CLI a expire : GitHub CLI le renouvelle."""
    methode = SESSION["methode"]
    if methode not in ("gh", "gcm"):
        return False
    tok = token_silencieux(cfg, methode)
    code, user, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, {}, {})
    if code != 200:
        return False
    SESSION.update(token=tok, user=user)
    return True


def connexion_navigateur(cfg, autre_compte=False):
    """Connexion interactive : ouvre le navigateur. Les cles deja enregistrees sur
    ce poste servent sans nouvelle autorisation, sauf pour changer de compte.
    Retourne True si connecte."""
    cles = None if autre_compte else cles_enregistrees(cfg)
    if (cles == "gh" and chemin_gh()) or (cles == "gcm" and helper_gcm()):
        tok = token_silencieux(cfg, cles)
        code, user, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, {}, {})
        if code == 200:
            demarrer_session(cfg, cles, tok, user)
            ok(f"Connexion OK : {user.get('login')}, compte de cette machine")
            return True
    methode = methode_dispo(cfg)
    if not methode:
        warn("Aucun outil de connexion par le navigateur : ni GitHub CLI, ni Git Credential Manager.")
        if not installer_outil("gh"):
            return False
        methode = "gh"
    h = host(cfg)
    print()
    info(f"Connexion avec {NOMS_METHODES[methode]} sur {h}...")
    tok = ""
    if methode == "gh":
        cmd = ["auth", "login", "--web", "-h", h, "-p", "https"]
        if gh_jetons_expirants():
            cmd.append("--expiring")
            info("Jeton valable quelques heures, puis GitHub CLI en demande un autre tout seul.")
        if gh_par_navigateur(cmd) != 0:
            err("Connexion impossible ou abandon.")
            return False
        run([chemin_gh(), "auth", "setup-git", "-h", h])
        tok = token_silencieux(cfg, "gh")
    else:
        info("GitHub s'ouvre : choisir 'Sign in with your browser'.")
        configurer_helper_gcm(h)
        if autre_compte:
            oublier_gcm(cfg)
        tok = credential_gcm(cfg, interactif=True)
        code, _, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, None, None)
        if code == 401:  # identifiants memorises mais revoques : nouvelle connexion
            oublier_gcm(cfg)
            tok = credential_gcm(cfg, interactif=True)
    code, user, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, {}, {})
    if code != 200:
        err(f"Connexion impossible, code {code or 'aucun'} : {user.get('message', 'pas de retour')}")
        return False
    demarrer_session(cfg, methode, tok, user)
    ok(f"Connexion OK : {user.get('login')}")
    choisir_garde(cfg)
    return True


# Temps de garde d'un compte sur cette machine, choisi a la connexion par le
# navigateur : pour la session, un nombre d'heures ou de jours, ou toujours. A la
# fin du temps, le compte est supprime, comme avec Fermer la session.
GARDES = (("Pour cette session : suppression en fin de programme", "session"),
          ("8 heures", 8 * 3600), ("7 jours", 7 * 86400), ("30 jours", 30 * 86400),
          ("Toujours", None))


def choisir_garde(cfg):
    """Temps de garde du compte actif sur cette machine."""
    print()
    print("  Temps de garde du compte sur cette machine :")
    for i, (libelle, _) in enumerate(GARDES, 1):
        print(f"    {i}. {libelle}")
    choix = ask("Choix", "1")
    if not (choix.isdigit() and 1 <= int(choix) <= len(GARDES)):
        warn("Choix invalide : pour cette session.")
        choix = "1"
    valeur = GARDES[int(choix) - 1][1]
    if valeur is None:
        cfg.pop(cle("garde"), None)
    else:
        cfg[cle("garde")] = valeur if valeur == "session" else int(time.time()) + valeur
    sauver(cfg)
    ok(f"Compte sur cette machine : {texte_garde(cfg) or 'toujours'}.")


def texte_garde(cfg, profil=None):
    garde = cfg.get(cle("garde", profil))
    if garde == "session":
        return "pour cette session"
    if isinstance(garde, int):
        return "jusqu'au " + datetime.fromtimestamp(garde).strftime("%Y-%m-%d %H:%M")
    return ""


def verifier_gardes(cfg, demarrage=False, fin=False):
    """Supprime les comptes dont le temps de garde est fini. Un compte
    pour cette session part en fin de programme, ou au demarrage suivant si le
    programme s'est arrete sans finir."""
    actif = PROFIL["nom"]
    for nom in PROFILS:
        garde = cfg.get(cle("garde", nom))
        if garde is None or (garde == "session" and not (demarrage or fin)) \
                or (isinstance(garde, int) and time.time() < garde):
            continue
        activer_profil(nom)
        login = (SESSION["user"] or {}).get("login") or cfg.get(cle("login"))
        SESSION.update(token="", user=None, methode=None)
        print()
        warn(f"Compte GitHub {nom_compte()} {login or ''} : fin du temps de garde sur cette machine.")
        voir, VOIR_GIT["actif"] = VOIR_GIT["actif"], False  # une seule ligne, sans les commandes
        try:
            supprimer_compte(cfg, login, silencieux=True)
        finally:
            VOIR_GIT["actif"] = voir
        if nom == actif:
            NAVIGATION["menu_principal"] = True
    activer_profil(actif)


def connexion_enregistree(cfg):
    """Session ouverte sans question avec le compte de cette machine, s'il
    existe et si GitHub l'accepte encore."""
    cles = cles_enregistrees(cfg)
    if not ((cles == "gh" and chemin_gh()) or (cles == "gcm" and helper_gcm())):
        return False
    tok = token_silencieux(cfg, cles)
    code, user, _ = requete(cfg, "GET", "/user", token=tok) if tok else (0, {}, {})
    if code != 200:
        return False
    demarrer_session(cfg, cles, tok, user)
    cfg[cle("enregistrer_connexion")] = True
    sauver(cfg)
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
    """Deconnexion du compte actif. Quand le compte a laisse des identifiants sur cette
    machine, le programme propose de le garder ou de le supprimer. Retourne
    False si la deconnexion est annulee."""
    login = (SESSION["user"] or {}).get("login") or cfg.get(cle("login"))
    traces = traces_compte(cfg)
    supprimer = False
    if traces:
        print()
        info(f"Sur cette machine, pour le compte GitHub {nom_compte()} {login or ''} :")
        for t in traces:
            print(f"       - {t}")
        print()
        print("  Garder le compte sur cette machine ?")
        print("    1. Garder le compte")
        print("    2. Supprimer le compte")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix not in ("1", "2"):
            info("Abandon : la session reste ouverte.")
            return False
        supprimer = choix == "2"
    SESSION.update(token="", user=None, methode=None)
    NAVIGATION["menu_principal"] = True
    print()
    ok(f"Fin de session : compte GitHub {nom_compte()}.")
    if supprimer:
        supprimer_compte(cfg, login)
    else:
        cfg[cle("enregistrer_connexion")] = False
        sauver(cfg)
        info("Le compte reste sur cette machine." if traces
             else "Ce compte ne laisse rien sur cette machine.")
    return True


def fichiers_identifiants():
    """Fichiers ou Git peut ecrire des identifiants en clair."""
    xdg = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return [Path.home() / ".git-credentials", Path(xdg) / "git" / "credentials"]


def identifiants_en_clair(serveur, login):
    """Lignes d'identifiants en clair de ce compte : liste de (fichier, ligne)."""
    trouves = []
    for fichier in fichiers_identifiants():
        try:
            lignes = fichier.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for ligne in lignes:
            try:
                u = urllib.parse.urlsplit(ligne.strip())
                if (u.hostname or "").lower() == serveur and \
                        urllib.parse.unquote(u.username or "").lower() == login.lower():
                    trouves.append((fichier, ligne))
            except ValueError:
                continue
    return trouves


def traces_compte(cfg, profil=None):
    """Ce qui reste du compte sur cette machine : cles de connexion, identifiants
    en clair, identite des commits ecrite par gitmanager, nom du compte dans la
    configuration de gitmanager."""
    profil = profil or PROFIL["nom"]
    login = (PROFILS[profil]["user"] or {}).get("login") or cfg.get(cle("login", profil))
    traces = []
    m = cles_enregistrees(cfg, profil)
    if m:
        traces.append(f"connexion par {NOMS_METHODES[m]}")
    if login and identifiants_en_clair(host(cfg, profil), login):
        traces.append("identifiants en clair, dans le fichier credentials de Git")
    ident = identite_ecrite(cfg, profil)
    if ident:
        traces.append("nom et email des commits dans la config Git : " + ", ".join(ident))
    if any(cfg.get(cle(k, profil)) for k in ("login", "methode", "cles")):
        traces.append("nom du compte dans la configuration de gitmanager")
    return traces


def retirer_helper(serveur, motif):
    """Retire de la configuration globale de Git l'aide de connexion pour ce
    serveur dont la valeur contient motif, et la ligne vide qui la precede."""
    for cle_git in (f"credential.https://{serveur}.helper", f"credential.https://gist.{serveur}.helper"):
        _, valeurs, _ = git("config", "--global", "--get-all", cle_git, quiet=True)
        valeurs = valeurs.splitlines()
        if not any(motif in v for v in valeurs):
            continue
        if all(motif in v or not v.strip() for v in valeurs):
            git("config", "--global", "--unset-all", cle_git, quiet=True)
        else:
            git("config", "--global", "--unset-all", cle_git, re.escape(motif), quiet=True)


def supprimer_compte(cfg, login=None, silencieux=False):
    """Supprime de cette machine tous les identifiants du compte actif : cles de
    GitHub CLI et de Git Credential Manager, identifiants en clair, identite des
    commits ecrite par gitmanager, reglages du compte dans gitmanager."""
    profil, h = PROFIL["nom"], host(cfg)
    login = login or cfg.get(cle("login"))
    methode = cles_enregistrees(cfg)
    autre = "entreprise" if profil == "perso" else "perso"
    partage = host(cfg, autre) == h and cles_enregistrees(cfg, autre) is not None
    sans_invite = dict(os.environ, GIT_TERMINAL_PROMPT="0", GCM_INTERACTIVE="never")
    restes = []
    motif_login = re.compile(rf"(?<![\w-]){re.escape(login)}(?![\w-])", re.I) if login else None
    gh = chemin_gh()
    if gh:
        etat = run([gh, "auth", "status", "-h", h])
        if (motif_login and motif_login.search(etat.stdout + etat.stderr)) or \
                (methode == "gh" and not login):
            if not login or run([gh, "auth", "logout", "-h", h, "-u", login]).returncode != 0:
                run([gh, "auth", "logout", "-h", h])
            etat = run([gh, "auth", "status", "-h", h])
            if motif_login and motif_login.search(etat.stdout + etat.stderr):
                restes.append("connexion dans GitHub CLI")
        if "Logged in" not in etat.stdout + etat.stderr:
            retirer_helper(h, "auth git-credential")
    if helper_gcm():
        if login:
            run(["git", f"credential-{helper_gcm()}", "github", "logout", login], env=sans_invite)
            run(_cmd_gcm("reject"), f"protocol=https\nhost={h}\nusername={login}\n\n", sans_invite)
        if methode == "gcm":
            run(_cmd_gcm("reject"), f"protocol=https\nhost={h}\n\n", sans_invite)
            if not partage:
                retirer_helper(h, helper_gcm())
    if login:
        run(["git", "credential", "reject"], f"protocol=https\nhost={h}\nusername={login}\n\n",
            sans_invite)
        for fichier in {f for f, _ in identifiants_en_clair(h, login)}:
            a_retirer = {l for f, l in identifiants_en_clair(h, login) if f == fichier}
            gardees = [l for l in fichier.read_text(encoding="utf-8").splitlines()
                       if l not in a_retirer]
            if gardees:
                fichier.write_text("\n".join(gardees) + "\n", encoding="utf-8")
            else:
                fichier.unlink()
    for cle_git in identite_ecrite(cfg):
        git("config", "--global", "--unset", cle_git, quiet=True)
    notes = cfg.get("identite_ecrite") or {}
    for cle_git in [k for k, v in notes.items() if v.get("profil") == profil]:
        notes.pop(cle_git)
    if not notes:
        cfg.pop("identite_ecrite", None)
    for k in ("login", "methode", "cles", "enregistrer_connexion", "moyen_connexion", "garde"):
        cfg.pop(cle(k), None)
    sauver(cfg)
    restes += traces_compte(cfg)
    if restes:
        warn("Ces identifiants du compte restent sur cette machine :")
        for t in restes:
            warn(f"  - {t}")
    elif not silencieux:
        ok(f"Compte {login or ''} : suppression OK sur cette machine.".replace("  ", " "))
    if (methode or login) and not silencieux:
        info("Sur GitHub, les autorisations restent visibles ici, pour les retirer :")
        info(f"https://{h}/settings/applications et https://{h}/settings/tokens")
    return not restes


def act_supprimer_identifiants(cfg):
    """Session ouverte : supprime les identifiants du compte de cette machine,
    et ferme la session."""
    login = (SESSION["user"] or {}).get("login") or cfg.get(cle("login"))
    traces = traces_compte(cfg)
    print()
    info(f"Sur cette machine, pour le compte GitHub {nom_compte()} {login or ''} :")
    for t in traces:
        print(f"       - {t}")
    if not ask_yn("Supprimer ces identifiants ? La session se ferme aussi.", False):
        info("Abandon.")
        return
    SESSION.update(token="", user=None, methode=None)
    NAVIGATION["menu_principal"] = True
    print()
    ok(f"Fin de session : compte GitHub {nom_compte()}.")
    supprimer_compte(cfg, login)


def act_supprimer_compte(cfg):
    """Sans connexion en cours : supprime le compte enregistre sur cette machine."""
    login = cfg.get(cle("login")) or ""
    traces = traces_compte(cfg)
    print()
    if not traces:
        info(f"Aucun identifiant du compte GitHub {nom_compte()} sur cette machine.")
        return
    info(f"Sur cette machine, pour le compte GitHub {nom_compte()} {login} :")
    for t in traces:
        print(f"       - {t}")
    if ask_yn("Supprimer ce compte de cette machine ?", False):
        supprimer_compte(cfg, login or None)


def changer_serveur(cfg):
    nouveau = ask("Serveur, par exemple github.com ou github.monentreprise.com", host(cfg))
    cfg[cle("host")] = re.sub(r"^https?://", "", nouveau).strip("/").lower()
    sauver(cfg)
    SESSION.update(token="", user=None, methode=None)
    connexion_interactive(cfg)


def menu_numerote(actions, debut=1, sortie="Retour"):
    """Affiche les choix numerotes a partir de debut, puis 0 ; retourne la
    fonction choisie, ou None pour Entree, 0 ou un choix invalide."""
    for i, (libelle, _) in enumerate(actions, debut):
        print(f"    {i}. {libelle}")
    print(f"    0. {sortie}")
    choix = ask("Choix", "0")
    if choix.isdigit() and debut <= int(choix) < debut + len(actions):
        return actions[int(choix) - debut][1]
    if choix not in ("0", "q", "quit", "exit"):
        warn("Choix invalide.")
    return None

def changer_moyen(cfg):
    """Nouvelle connexion du compte actif par le moyen choisi ; la connexion
    d'avant reste en place si la nouvelle est annulee."""
    avant = dict(SESSION)
    SESSION.update(token="", user=None, methode=None)
    if connexion_interactive(cfg):
        return True
    SESSION.update(avant)
    info("Connexion sans changement.")
    return False


def changer_compte(cfg):
    """Connexion a un autre compte GitHub a la place du compte actif ; le compte
    d'avant reste connecte si la nouvelle connexion est annulee."""
    avant = dict(SESSION)
    ancien = (avant["user"] or {}).get("login")
    print()
    info(f"Changer de compte GitHub {nom_compte()}, compte actuel : {ancien or 'aucun'}.")
    info("Par code ou par le navigateur, GitHub autorise le compte ouvert dans le")
    info("navigateur : s'y connecter d'abord au bon compte, ou utiliser un autre profil.")
    SESSION.update(token="", user=None, methode=None)
    if not connexion_interactive(cfg, autre_compte=True):
        SESSION.update(avant)
        info(f"Compte sans changement : {ancien}." if ancien else "Aucun compte en session.")
        return False
    nouveau = SESSION["user"].get("login")
    if ancien and nouveau == ancien:
        warn(f"Toujours le compte {nouveau}. Quitter ce compte GitHub dans le navigateur,")
        warn("puis recommencer.")
    else:
        ok(f"Compte GitHub {nom_compte()} : {nouveau}")
    return True


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
        if gh_par_navigateur(["auth", "refresh", "-h", host(cfg), "-s",
                              ",".join(s.strip() for s in scopes.split(",") if s.strip())]) == 0:
            SESSION["token"] = token_silencieux(cfg, "gh")
            return bool(SESSION["token"])
        return False
    if SESSION["methode"] == "code":
        info("Ajouter cette permission dans l'application GitHub de gitmanager, puis")
        info(f"accepter le changement sur https://{host(cfg)}/settings/installations.")
    else:
        info(f"Ajouter cette permission au jeton sur https://{host(cfg)}/settings/tokens,")
        info("sans en faire un nouveau : le changement compte tout de suite.")
    return ask_yn("Nouvel essai ?", True)


def api(cfg, verbe, chemin, data=None):
    """Appel API avec la session courante : reconnexion si elle a expire, et
    nouvel essai apres l'ajout d'une permission manquante."""
    if not assurer_connexion(cfg):
        return 0, {"message": "pas de session GitHub"}, {}
    for _ in range(3):
        code, rep, hdr = requete(cfg, verbe, chemin, data)
        if code == 401:
            if rafraichir_session(cfg):
                continue
            SESSION["token"] = ""
            if SESSION["methode"] == "gcm":
                oublier_gcm(cfg)
            warn("Session GitHub finie : nouvelle connexion.")
            if connexion_interactive(cfg):
                continue
            break
        besoin = permission_manquante(code, hdr)
        if not (besoin and mettre_a_jour_permissions(cfg, besoin, hdr)):
            break
    return code, rep, hdr


MESSAGES_API = (("aucune connexion internet", "aucune connexion internet"),
                ("bad credentials", "identifiants faux ou trop anciens"),
                ("rate limit", "limite d'appels GitHub : attendre un peu"),
                ("name already exists", "ce nom existe sur ce compte"),
                ("resource not accessible", "permission manquante"),
                ("must have admin", "droits admin requis"),
                ("requires authentication", "connexion requise"),
                ("repository creation failed", "nouveau repo impossible"),
                ("validation failed", "demande invalide"),
                ("not found", "introuvable"))


def message_api(rep):
    """Message d'erreur de l'API GitHub, en francais."""
    if not isinstance(rep, dict) or not rep:
        return "pas de retour"
    texte = json.dumps(rep).lower()
    for anglais, francais in MESSAGES_API:
        if anglais in texte:
            return francais
    return "refus de GitHub" + (f" : {rep.get('message')}" if DEBUG and rep.get("message") else "")


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
        return "repo introuvable sur le serveur : nom faux, repo absent ou droits manquants"
    if "permission to" in s and "denied" in s:
        return "ce compte n'a pas le droit de push dans ce repo"
    if erreur_auth(sortie):
        return "refus de GitHub : nouvelle connexion avec le menu Compte GitHub"
    if "publickey" in s:
        return "le serveur SSH refuse la connexion : relancer pour passer le projet en HTTPS"
    if "could not resolve host" in s or "timed out" in s or "failed to connect" in s:
        return "serveur injoignable : voir la connexion internet ou le VPN de l'entreprise"
    if "gh001" in s or "file size limit" in s or "large files" in s:
        return "un fichier fait plus de 100 Mo, la limite de GitHub"
    if "rejected" in s or "fetch first" in s or "non-fast-forward" in s:
        return "le serveur a des changements plus neufs : relancer l'envoi"
    if "does not appear to be a git repository" in s or "does not exist" in s:
        return "repo introuvable : voir l'adresse"
    if "not a git repository" in s:
        return "ce dossier n'est pas un repo git"
    if "please tell me who you are" in s or "empty ident" in s:
        return "nom et email des commits manquants"
    if "index.lock" in s:
        return "un autre programme git tourne sur ce repo : le fermer, puis recommencer"
    if "would be overwritten" in s or "your local changes" in s:
        return "des changements, commit manquant, bloquent l'action : faire un commit avant"
    if "unrelated histories" in s:
        return "les deux historiques n'ont aucun lien"
    if "ssl certificate" in s or "certificate verify failed" in s:
        return "certificat SSL invalide : voir le proxy de l'entreprise"
    if "does not match any" in s:
        return "aucun commit sur cette branche"
    if "conflict" in s:
        return "conflit entre ce dossier et le serveur"
    if DEBUG and derniere_ligne(sortie):
        return "erreur de git : " + derniere_ligne(sortie)
    return "erreur de git"


def nettoyer_depot(chemin, bavard=True):
    """Repare ce que l'ancienne version du script laissait dans .git/config."""
    noms = remotes(chemin)
    for r in noms:  # un identifiant en clair dans .git/config est un secret expose
        url = url_remote(r, chemin)
        if contient_token(url):
            git("remote", "set-url", r, sans_identifiants(url), cwd=chemin, quiet=True)
            if bavard:
                info(f"Identifiant en clair : retrait OK dans l'adresse du remote '{r}'.")
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
            info(f"Suivi de branche : correction OK, {cle.split('.')[1]} -> {cible or 'aucun'}.")
    # Nom / email vides en local : ils masquaient la config globale -> commit refuse
    for cle in ("user.name", "user.email"):
        code, val, _ = git("config", "--local", "--get", cle, cwd=chemin, quiet=True)
        if code == 0 and not val.strip():
            git("config", "--local", "--unset-all", cle, cwd=chemin, quiet=True)
            if bavard:
                info(f"{cle} vide : retrait OK, il bloquait les commits.")


def assurer_identite(cfg, chemin, auto):
    for cle in ("user.name", "user.email"):
        _, val, _ = git("config", "--get", cle, cwd=chemin, quiet=True)
        if val.strip():
            continue
        valeur = identite_par_defaut(cfg, SESSION["user"], cle)
        if not valeur and auto:
            raise Arret("erreur", f"git {cle} manque dans la config de Git")
        if not valeur:
            valeur = ask("Nom pour les commits" if cle == "user.name"
                         else "Email pour les commits")
        ecrire_identite(cfg, cle, valeur)


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
            info("Ajout OK dans le .gitignore.")
    return tous


def committer(cfg, chemin, message=None, auto=False, proposer_gitignore=True):
    """git add -A + git commit. Retourne le nombre de fichiers commites."""
    if not fichiers_modifies(chemin):
        if not auto:
            info("Aucun changement depuis le dernier commit.")
        return 0
    if not auto and proposer_gitignore and not (Path(chemin) / ".gitignore").exists():
        warn("Pas de .gitignore : risque d'envoyer .venv, node_modules, caches...")
        if ask_yn("Ajouter un .gitignore standard pour Python et Node ?", True):
            (Path(chemin) / ".gitignore").write_text(GITIGNORE, encoding="utf-8")
            ok("Nouveau .gitignore")
    code, _, e = git("add", "-A", cwd=chemin, quiet=auto)
    if code != 0:
        raise Arret("erreur", "erreur de git add : " + expliquer(e))
    exclus = proteger_fichiers(chemin, auto)
    if git("diff", "--cached", "--quiet", cwd=chemin, quiet=True)[0] == 0:
        if not auto:
            info("Rien pour un commit.")
        return 0
    _, ns, _ = git("diff", "--cached", "--name-status", "-M", cwd=chemin, quiet=True)
    changements = lire_name_status(ns)
    if not auto:
        print()
        info(f"Changements du commit : {bilan_changements(changements)}")
        afficher_changements(changements)
        message = ask("Message du commit, Enter pour garder", message or message_defaut())
    assurer_identite(cfg, chemin, auto)
    code, out, e = git("commit", "-m", message or message_defaut(), cwd=chemin, quiet=auto)
    if code != 0:
        raise Arret("erreur", "refus du commit : " + expliquer(e or out))
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
                warn("Le serveur SSH refuse la connexion : pas de SSH pour GitHub sur ce poste.")
                h, o, r = infos_url(url)
                if o and ask_yn("Passer ce projet en HTTPS, avec connexion par le navigateur ?", True):
                    git("remote", "set-url", remote, f"https://{h}/{o}/{r}.git", cwd=chemin)
                    continue
            elif erreur_auth(e):
                if rafraichir_session(cfg):
                    continue
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
            raise Arret("ignore", f"le serveur a la branche '{defaut}', "
                                  f"celle de ce dossier s'appelle '{branche}' : ouvrir le projet")
        print()
        warn(f"Sur le serveur la branche principale est '{defaut}', ici c'est '{branche}'.")
        print(f"    1. Renommer '{branche}' en '{defaut}' et continuer")
        print(f"    2. Garder '{branche}' comme autre branche sur le serveur")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix not in ("1", "2"):
            raise Arret("ignore", "abandon de l'envoi")
        if choix == "2":
            return branche, 0
        if defaut in branches_locales(chemin):
            raise Arret("erreur", f"une branche locale '{defaut}' existe aussi : "
                                  f"faire 'git switch {defaut}' puis recommencer")
        git("branch", "-M", defaut, cwd=chemin)
        branche = defaut

    distante = f"{remote}/{branche}"
    if not a_des_commits(chemin):  # rien en local : on prend la version du serveur
        code, _, e = git("merge", "--ff-only", distante, cwd=chemin, quiet=auto)
        if code != 0:
            raise Arret("erreur", "pull impossible : " + expliquer(e))
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
        print("    1. Les prendre et les combiner avec ce dossier")
        print("    2. Ce dossier fait foi : le serveur prend son contenu exact")
        print("       fichiers absents d'ici : en moins sur le serveur, historique intact")
        print("    0. Annuler")
        choix = ask("Choix", "0")
        if choix not in ("1", "2"):
            raise Arret("ignore", "abandon de l'envoi")
        if choix == "2":
            code, _, e = git("merge", "-s", "ours", "--allow-unrelated-histories", "-m",
                             "Remplacement par la version de ce dossier", distante, cwd=chemin)
            if code != 0:
                raise Arret("erreur", "remplacement impossible : " + expliquer(e))
            return branche, 0

    if not auto:
        info(f"Combinaison de {retard} commit(s) du serveur...")
    code, out, e = git("rebase", "--autostash", distante, cwd=chemin, quiet=auto)
    if code == 0:
        return branche, retard
    if operation_en_cours(chemin) != "rebase":
        raise Arret("erreur", "combinaison impossible : " + expliquer(e or out))
    conflits = fichiers_en_conflit(chemin)
    liste = ", ".join(conflits[:5]) + (" ..." if len(conflits) > 5 else "")
    if not auto:
        print()
        warn("Conflit : des lignes ont des changements ici et sur le serveur.")
        for f in conflits:
            print("       " + f)
        if pour_envoi:
            info("Astuce : avec l'option 2, 'ce dossier fait foi', pas de conflit.")
        if not ask_yn("Annuler ? Rien n'est perdu, les commits restent ici.", True):
            info("Ouvrir ces fichiers, garder la bonne version entre <<<<<<< et >>>>>>>,")
            info("puis : git add .   et   git rebase --continue")
            raise Arret("conflit", f"conflit en cours : {liste}")
    git("rebase", "--abort", cwd=chemin, quiet=True)
    raise Arret("conflit", f"conflit sur {liste} : aucun envoi, le travail local est intact")


def pousser(cfg, chemin, remote, branche, auto=False, commit_affiche=False):
    """git push (apres integration du serveur). Retourne (statut, detail)."""
    if not auto:
        info("Lecture du serveur...")
    recuperer(cfg, chemin, remote, auto)
    branche, recus = integrer(cfg, chemin, remote, branche, auto, pour_envoi=True)
    if not a_des_commits(chemin):
        raise Arret("ignore", "aucun commit : rien pour le push")
    ref = f"refs/remotes/{remote}/{branche}"
    avant = git("rev-parse", ref, cwd=chemin, quiet=True)[1] if ref_existe(chemin, ref) else None
    ar = avance_retard(chemin, remote, branche) if avant else None
    a_envoyer = ar[0] if ar else nb_commits(chemin)
    web = url_web(url_remote(remote, chemin))
    if a_envoyer == 0:
        git("branch", "-q", "--set-upstream-to", f"{remote}/{branche}", branche,
            cwd=chemin, quiet=True)
        if recus:
            return "ok", f"pull : {recus} commit(s) ; le serveur et ce dossier sont pareils"
        return "a_jour", "rien de nouveau : le serveur a ce contenu"

    for tentative in (1, 2, 3):
        code, _, e = git("push", "-u", remote, branche, cwd=chemin, quiet=auto)
        if code == 0:
            break
        refus = "permission to" in e.lower() and "denied" in e.lower()
        if tentative < 3 and not auto and refus \
                and SESSION["methode"] in EN_MEMOIRE + ("gh",):
            if mettre_a_jour_permissions(cfg, "contents=write, push dans ce repo"):
                continue
        if tentative == 1 and erreur_auth(e) and not refus and rafraichir_session(cfg):
            continue
        if tentative == 1 and not auto and erreur_auth(e) and not refus:
            SESSION["token"] = ""
            if connexion_interactive(cfg):
                continue
        raise Arret("erreur", expliquer(e))

    changements = changements_entre(chemin, avant)
    # commit_affiche : le seul commit envoye vient d'etre montre, sa liste suffit
    if not auto and not (commit_affiche and a_envoyer == 1 and avant and not recus):
        print()
        info(("Premier envoi" if not avant else "Contenu du push")
             + f" : {bilan_changements(changements)}")
        afficher_changements(changements, 25)
        if avant:
            info("Les autres fichiers sont pareils. Sur GitHub, chaque fichier montre le")
            info("dernier commit qui le change vraiment : c'est normal.")
    detail = f"push : {a_envoyer} commit(s)" + (f", {bilan_changements(changements)}" if auto else "")
    if recus:
        detail += f", pull : {recus}"
    return "ok", detail + (f"  ->  {web}" if web else "")


def relier(cfg, chemin):
    return lier_a_github(cfg, chemin)


def preparer(cfg, chemin, auto, lier=True):
    """Verifie branche, remote et operation en cours. Retourne (branche, remote)."""
    nettoyer_depot(chemin, bavard=not auto)
    op = operation_en_cours(chemin)
    if op:
        if auto:
            raise Arret("ignore", f"{op} en cours : conflit pas fini")
        warn(f"Un {op} est en cours : conflit pas fini.")
        if not ask_yn("L'annuler pour revenir comme avant ?", True):
            raise Arret("conflit", f"pour le finir : git add .  puis  git {op} --continue")
        git(op, "--abort", cwd=chemin)
    branche = branche_courante(chemin)
    if not branche:
        raise Arret("erreur", "HEAD hors branche : passer sur une branche avec git switch main")
    remote = remote_principal(chemin)
    if not remote:
        if auto or not lier:
            raise Arret("ignore", "aucun lien avec un repo sur le serveur")
        remote = relier(cfg, chemin)
        if not remote:
            raise Arret("erreur", "aucun repo en ligne pour ce dossier")
    return branche, remote


def recuperer_projet(cfg, chemin, auto=False):
    """git pull (fetch + rebase). Retourne (statut, detail)."""
    branche, remote = preparer(cfg, chemin, auto, lier=False)
    avant = git("rev-parse", "HEAD", cwd=chemin, quiet=True)[1] if a_des_commits(chemin) else None
    if not auto:
        info("Lecture du serveur...")
    recuperer(cfg, chemin, remote, auto)
    branche, recus = integrer(cfg, chemin, remote, branche, auto)
    if not recus:
        return "a_jour", "rien de nouveau sur le serveur"
    changements = changements_entre(chemin, avant)
    if not auto:
        info(f"Pull : {bilan_changements(changements)}")
        afficher_changements(changements, 25)
    return "ok", f"pull : {recus} commit(s)" + (f", {bilan_changements(changements)}" if auto else "")


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
              "ignore": ("[-] ", "36"), "erreur": ("[X] ", "31")}


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
    print("    1. Nouveau repo sur GitHub")
    print("    2. Lier avec un repo GitHub qui existe")
    print("    0. Annuler")
    choix = ask("Choix", "0")
    if choix not in ("1", "2") or not assurer_connexion(cfg):
        return None
    login = SESSION["user"]["login"]
    if choix == "2":
        cible = ask("Repo : nom, compte/nom ou URL")
        if "://" in cible or cible.startswith("git@"):
            url = sans_identifiants(cible)
        else:
            if "/" not in cible:
                cible = f"{login}/{cible}"
            url = f"https://{host(cfg)}/{cible.removesuffix('.git')}.git"
    else:
        defaut = re.sub(r"[^A-Za-z0-9._-]+", "-", os.path.basename(chemin)).strip("-")
        nom = ask("Nom du repo", defaut or "projet")
        description = ask("Description du repo, Enter pour aucune", "", obligatoire=False)
        org = ask("Organisation, vide pour le compte personnel", "", obligatoire=False)
        prive = not ask_yn("Repo public ?", False)
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
        ok(f"Nouveau repo : {rep.get('html_url')}, {'non public' if prive else 'public'}")
        return rep.get("clone_url")
    if code == 422 and "already exists" in json.dumps(rep):
        proprio = org or SESSION["user"]["login"]
        warn(f"{proprio}/{nom} existe sur GitHub.")
        if ask_yn("L'utiliser pour ce projet ?", True):
            code2, rep2, _ = api(cfg, "GET", f"/repos/{proprio}/{nom}")
            if code2 == 200:
                return rep2.get("clone_url")
        return None
    err(f"Nouveau repo impossible, code {code} : {message_api(rep)}")
    return None


def depot_du_projet(cfg, chemin):
    """Infos GitHub (API) du repo relie a ce projet, ou None."""
    remote = remote_principal(chemin)
    h, o, r = infos_url(url_remote(remote, chemin)) if remote else (None, None, None)
    if not o or h != host(cfg):
        err("Ce projet n'a pas de lien avec un repo du compte GitHub en session.")
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
    print(f"  Branche : {branche or 'HEAD hors branche'}     "
          f"Serveur : {url_web(url) or url or c('sans lien', '33')}")
    etat = [f"{len(modifs)} fichier(s), commit manquant"]
    if remote and branche:
        ar = avance_retard(chemin, remote, branche)
        if ar is None:
            etat.append("aucun push sur le serveur" if a_des_commits(chemin)
                        else "aucun commit")
        else:
            etat += [f"push : {ar[0]} commit(s)", f"pull : {ar[1]}"]
    print("  Statut  : " + " | ".join(etat))
    op = operation_en_cours(chemin)
    if op:
        warn(f"Un {op} est en cours : conflit pas fini.")


def voir_modifs(cfg, chemin):
    modifs = fichiers_modifies(chemin)
    print()
    if not modifs:
        info("Aucun changement depuis le dernier commit.")
    else:
        info(f"{len(modifs)} fichier(s) avec changements depuis le dernier commit :")
        for l in modifs[:60]:
            print(f"       {statut_porcelaine(l):<9} {l[3:]}")
        if len(modifs) > 60:
            print(f"       ... et {len(modifs) - 60} autre(s)")
    branche, remote = branche_courante(chemin), remote_principal(chemin)
    if branche and remote and ref_existe(chemin, f"refs/remotes/{remote}/{branche}"):
        for plage, libelle in ((f"{remote}/{branche}..HEAD", "Commits pour le push"),
                               (f"HEAD..{remote}/{branche}", "Commits pour le pull")):
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
    fait = committer(cfg, chemin)
    return pousser(cfg, chemin, remote, branche, commit_affiche=fait > 0)


def act_pull(cfg, chemin):
    return recuperer_projet(cfg, chemin)


def act_ecraser(cfg, chemin):
    warn("Remplace TOUT l'historique du serveur par celui de ce dossier. Sans retour.")
    info("Pour garder l'historique : Envoyer, puis 'ce dossier fait foi'.")
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
    if ask("Taper REMPLACER en majuscules pour confirmer", "", obligatoire=False) != "REMPLACER":
        return "ignore", "abandon"
    committer(cfg, chemin)
    if not a_des_commits(chemin):
        raise Arret("ignore", "aucun commit : rien pour le push")
    code, _, e = git("push", "--force-with-lease", "-u", remote, branche, cwd=chemin)
    if code != 0:
        raise Arret("erreur", expliquer(e))
    return "ok", "force push OK : le serveur a le contenu de ce dossier"


def act_visibilite_projet(cfg, chemin):
    d = depot_du_projet(cfg, chemin)
    if d:
        changer_visibilite(cfg, d)


def act_relier(cfg, chemin):
    remote = relier(cfg, chemin)
    if remote:
        return "ok", f"lien avec {url_web(url_remote(remote, chemin)) or url_remote(remote, chemin)}"
    return "ignore", "abandon"


def act_supprimer_git(cfg, chemin):
    """Supprime le dossier .git (le code reste). Retourne True si fait."""
    dot_git = os.path.join(chemin, ".git")
    warn("Suppression de l'historique git LOCAL, le dossier .git. Les fichiers du code restent.")
    info("Le dossier perd son lien avec GitHub. Les envois faits restent sur GitHub.")
    modifs = fichiers_modifies(chemin)
    remote, branche = remote_principal(chemin), branche_courante(chemin)
    ar = avance_retard(chemin, remote, branche) if remote and branche else None
    if modifs or (ar and ar[0]) or (remote and branche and ar is None and a_des_commits(chemin)):
        warn("Attention : une partie du travail n'est PAS sur GitHub : "
             f"{len(modifs)} fichier(s), commit manquant, "
             f"{ar[0] if ar else 'des'} commit(s) sans push.")
    if ask("Taper SUPPRIMER en majuscules pour confirmer", "", obligatoire=False) != "SUPPRIMER":
        info("Abandon.")
        return False
    if os.path.isdir(dot_git):
        supprimer_dossier(dot_git)
    elif os.path.isfile(dot_git):
        os.remove(dot_git)
    if os.path.exists(dot_git):
        err("Suppression pas finie : fermer les programmes qui ouvrent ce dossier, puis recommencer.")
        return False
    ok(f"Suppression du .git OK : {chemin} est maintenant un simple dossier.")
    return True


def menu_init(cfg, chemin):
    """Dossier sans git. Retourne True si un repo a ete cree."""
    titre("Nouveau projet git")
    info(f"{chemin} n'est pas un repo git.")
    meme = repo_du_meme_nom(cfg, chemin)
    if meme:
        info(f"Repo en ligne avec ce nom : {meme['full_name']}")
    print()
    print("    1. " + (f"Lier avec {meme['full_name']}, puis pull" if meme
                       else "Lier avec un repo GitHub en ligne, puis pull"))
    print("    2. git init, nouveau repo GitHub, premier commit et push")
    print("    3. git init seulement")
    print("    0. Retour")
    choix = ask("Choix", "0")
    if choix == "1":
        if not assurer_connexion(cfg):
            return False
        cible = ask("Repo en ligne : compte/nom ou URL", meme["full_name"] if meme else "")
        executer(lier_et_recevoir, cfg, chemin, url_repo_en_ligne(cfg, cible))
        pause()
        return racine_repo(chemin) is not None
    if choix not in ("2", "3"):
        return False
    init_depot(chemin)
    gi = Path(chemin) / ".gitignore"
    if not gi.exists() and ask_yn("Ajouter un .gitignore standard pour Python et Node ?", True):
        gi.write_text(GITIGNORE, encoding="utf-8")
        ok("Nouveau .gitignore")
    if choix == "2":
        executer(act_commit_push, cfg, chemin)
        pause()
    return True


def repo_du_meme_nom(cfg, chemin):
    """Repo GitHub du compte en session qui porte le nom du dossier, ou None."""
    if not SESSION["token"]:
        return None
    nom = os.path.basename(os.path.normpath(chemin)).lower()
    repos = api_liste(cfg, "/user/repos?per_page=100"
                           "&affiliation=owner,collaborator,organization_member") or []
    return next((d for d in repos if (d.get("name") or "").lower() == nom), None)


def url_repo_en_ligne(cfg, cible):
    """nom, compte/nom ou URL : adresse du repo en ligne."""
    cible = cible.strip()
    if "://" in cible or cible.startswith("git@"):
        return verifier_url(cible)
    if "/" not in cible:
        cible = f"{SESSION['user']['login']}/{cible}"
    return f"https://{host(cfg)}/{cible.removesuffix('.git')}.git"


def lier_et_recevoir(cfg, chemin, url):
    """Dossier qui contient le meme projet qu'un repo en ligne : lien avec ce repo,
    puis pull de sa version la plus neuve. Les fichiers de ce dossier qui ne sont
    pas comme la version en ligne partent dans git stash, au choix ; ceux absents
    du repo en ligne restent."""
    if racine_repo(chemin) is None:
        init_depot(chemin)
    elif a_des_commits(chemin):
        raise Arret("ignore", "ce dossier a son propre historique git : l'ouvrir dans "
                              "Repos GitHub en local, puis Lier avec GitHub")
    if "origin" in remotes(chemin):
        git("remote", "set-url", "origin", url, cwd=chemin)
    else:
        git("remote", "add", "origin", url, cwd=chemin)
    info("Lecture du repo en ligne...")
    recuperer(cfg, chemin, "origin", auto=False)
    branche = branche_defaut_distante(chemin, "origin") or "main"
    ref = f"origin/{branche}"
    if not ref_existe(chemin, f"refs/remotes/{ref}"):
        return "ignore", "le repo en ligne est vide : utiliser Envoyer"
    # la branche locale passe sur la version en ligne, les fichiers du dossier ne bougent pas
    git("symbolic-ref", "HEAD", f"refs/heads/{branche}", cwd=chemin, quiet=True)
    git("reset", "--mixed", "--quiet", ref, cwd=chemin)
    git("branch", "--set-upstream-to", ref, branche, cwd=chemin, quiet=True)
    modifs = fichiers_modifies(chemin)
    suivis = [l for l in modifs if not l.startswith("??")]
    nouveaux = len(modifs) - len(suivis)
    print()
    if not suivis:
        ok("Ce dossier et la version en ligne sont pareils.")
    else:
        warn(f"{len(suivis)} fichier(s) de ce dossier ne sont pas comme la version en ligne :")
        for l in suivis[:20]:
            print(f"       {statut_porcelaine(l):<9} {l[3:]}")
        if len(suivis) > 20:
            print(f"       ... et {len(suivis) - 20} autre(s)")
        print()
        print("    1. Prendre la version en ligne : pull, ces fichiers partent dans git stash")
        print("    2. Garder les fichiers de ce dossier, commit manquant")
        if ask("Choix", "1") == "1":
            git("stash", "push", "-m", "gitmanager : fichiers avant le pull", cwd=chemin)
            ok("Version en ligne OK dans ce dossier.")
            info("Fichiers d'avant dans git stash : git stash pop pour les reprendre.")
        else:
            info("Les fichiers de ce dossier restent, commit manquant.")
    if nouveaux:
        info(f"{nouveaux} fichier(s) absent(s) du repo en ligne restent, commit manquant.")
    return "ok", f"lien avec {url_web(url) or url}, branche {branche}"


def menu_projet(cfg, chemin):
    chemin = os.path.normpath(chemin)
    racine, etat = acces_repo(chemin)
    if etat == "illisible" and ask_yn("Mettre ce .git de cote en .git-illisible, puis lier le dossier avec "
                                      "son repo en ligne ?", True):
        cible, n = os.path.join(chemin, ".git-illisible"), 2
        while os.path.lexists(cible):
            cible, n = os.path.join(chemin, f".git-illisible-{n}"), n + 1
        os.rename(os.path.join(chemin, ".git"), cible)
        etat = "absent"
    if racine is None and (etat != "absent" or not menu_init(cfg, chemin)):
        return
    nettoyer_depot(chemin)
    remote = remote_principal(chemin)
    if remote and git("fetch", remote, cwd=chemin, quiet=True)[0] != 0:
        warn("Serveur injoignable : le statut push / pull peut dater.")
    actions = {
        "1": act_commit_push, "2": act_pull, "3": act_ecraser, "4": voir_modifs,
        "7": act_visibilite_projet, "8": act_relier,
    }
    while True:
        titre("Repo local")
        entete_projet(chemin)
        relie = bool(remote_principal(chemin))
        print()
        print("    1. Envoyer : commit + push")
        print("    2. Recevoir : pull")
        print("    3. Remplacer GitHub par ce dossier : force push")
        print("    4. Voir les changements")
        print("    5. Ouvrir la copie sur cette machine")
        print("    6. Ouvrir la copie en ligne")
        print("    7. Rendre public ou non public")
        print("    8. " + ("Lier avec un autre repo GitHub" if relie else "Lier avec GitHub"))
        print("    9. Supprimer le .git de ce dossier")
        print("    0. Retour")
        choix = ask("Choix", "0")
        if choix == "5":
            print()
            ouvrir_explorateur(chemin)
            continue
        if choix == "6":
            print()
            ouvrir_en_ligne(chemin)
            continue
        if choix == "9":
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
            warn("Interruption.")
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
    chemin = choisir_dossier(cfg, "Dossier pour l'envoi, ou repo hors liste")
    if chemin:
        ouvrir_dossier(cfg, chemin)


def act_depots_locaux(cfg):
    titre("Repos GitHub en local")
    info("Recherche des repos git sur cette machine...")
    projets = projets_de_la_machine(cfg)[:30]
    print()
    if not projets:
        info("Aucun repo git sur cette machine.")
        if ask_yn("Envoyer un dossier sur GitHub ?", True):
            act_autre_dossier(cfg)
        return
    print(c(f"    {'#':>3}  {'Repo':<44} {'Commit manquant':>15}  GitHub", "1"))
    for i, (p, (url, modifs, _)) in enumerate(zip(projets, apercu_projets(projets)), 1):
        nom = nom_court(p)
        nom = nom if len(nom) <= 44 else "~" + nom[-43:]
        print(f"    {i:>3}. {nom:<44} {modifs:>15}  {lien_court(url)}")
    print()
    print("      N. Envoyer un autre dossier sur GitHub")
    print("      T. Tous les repos d'un dossier : push ou pull")
    choix = ask("Choix : # du repo, N ou T, Enter pour revenir", "", obligatoire=False).lower()
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
        warn("Aucun projet git dans ce dossier ni dans ses sous-dossiers.")
        return
    print()
    print(c(f"    {'Projet':<28} {'Branche':<12} {'Commit manquant':>15}  Serveur", "1"))
    for p, (url, modifs, branche) in zip(projets, apercu_projets(projets, True)):
        nom = os.path.relpath(p, dossier) if p != dossier else os.path.basename(p)
        print(f"    {nom[:28]:<28} {branche[:12]:<12} {modifs:>15}  {lien_court(url)}")
    print()
    print("    1. Tout envoyer : commit + push de chaque repo")
    print("    2. Tout recevoir : pull de chaque repo")
    print("    0. Annuler")
    mode = ask("Choix", "0")
    if mode not in ("1", "2"):
        return
    message = ask("Message du commit pour les projets avec changements, Enter pour garder", message_defaut()) \
        if mode == "1" else None
    if not assurer_connexion(cfg):
        return
    print()
    bilan = {}
    for i, p in enumerate(projets, 1):
        nom = os.path.relpath(p, dossier) if p != dossier else os.path.basename(p)
        print(f"  [{i}/{len(projets)}] {nom}...", end="\r", flush=True)
        statut, detail = synchroniser(cfg, p, message=message, envoyer=(mode == "1"))
        print(" " * 70, end="\r")
        afficher_resultat(statut, detail, nom[:28])
        bilan[statut] = bilan.get(statut, 0) + 1
    print()
    resume = {"ok": "OK", "a_jour": "sans rien de nouveau", "conflit": "en conflit",
              "ignore": "sans action", "erreur": "en erreur"}
    info("Bilan : " + ", ".join(f"{n} {resume[s]}" for s, n in bilan.items()))
    if bilan.get("conflit"):
        info("Projets en conflit : les ouvrir dans Repos GitHub en local et utiliser Envoyer.")


# Repos GitHub : liste, navigation, visibilite, suppression

def act_depots(cfg):
    titre("Repos GitHub en ligne")
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
        ligne_compte(cfg)
        print()
        entete = f"{len(vue)} repo(s)" + (f" contenant '{filtre}'" if filtre else "")
        print(c(f"    {entete}", "1"))
        print(c(f"    {'#':>3}  {'Repo':<42} {'Public':<7} {'Envoi':<10}  En local", "1"))
        for i, d in enumerate(vue, 1):
            nom = d["full_name"] if len(d["full_name"]) <= 42 else d["full_name"][:41] + "~"
            vis = "non" if d.get("private") else "oui"
            date = (d.get("pushed_at") or d.get("updated_at") or "")[:10]
            loc = "oui" if d["full_name"].lower() in locaux else ""
            print(f"    {i:>3}. {nom:<42} {vis:<7} {date:<10}  {loc}")
        print()
        print("      C. Changer de compte")
        print("      D. Fermer la session")
        print()
        retour = "Enter pour toute la liste" if filtre else "Enter pour revenir"
        choix = ask(f"Choix : # du repo, lettre, ou mot pour chercher, {retour}",
                    "", obligatoire=False)
        if not choix:
            if not filtre:
                return
            filtre = ""
        elif choix.isdigit() and 1 <= int(choix) <= len(vue):
            menu_depot(cfg, vue[int(choix) - 1], locaux)
        elif choix.lower() == "c":
            changer_compte(cfg)
            filtre = ""
            pause()
        elif choix.lower() == "d":
            if deconnexion(cfg):
                pause()
                return
        else:
            filtre = choix


def ouvrir_navigateur(url):
    """Ouvre la page dans le navigateur et affiche son adresse, utile sans
    navigateur ou sur un poste distant."""
    info(f"Ouverture dans le navigateur : {url}")
    try:
        if not webbrowser.open(url):
            warn("Aucun navigateur disponible : copier l'adresse ci-dessus.")
    except webbrowser.Error:
        warn("Aucun navigateur disponible : copier l'adresse ci-dessus.")


def ouvrir_en_ligne(chemin):
    """Page GitHub du repo auquel ce dossier est relie."""
    remote = remote_principal(chemin)
    if not remote:
        warn("Ce dossier n'a aucun lien avec un repo en ligne : le faire avec le choix 8.")
        return
    url = url_web(url_remote(remote, chemin))
    if not url:
        warn(f"Adresse du repo en ligne inconnue : {url_remote(remote, chemin)}")
        return
    ouvrir_navigateur(url)


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
    except OSError:
        warn(f"Ouverture du dossier impossible : {chemin}")


def menu_depot(cfg, d, locaux):
    while True:
        code, frais, _ = api(cfg, "GET", f"/repos/{d['full_name']}")
        if code == 200 and isinstance(frais, dict):
            d = frais
        local = locaux.get(d["full_name"].lower())
        vis = "non public" if d.get("private") else "public"
        titre(f"{d['full_name']} : {vis}")
        if d.get("description"):
            info(d["description"])
        info(d["html_url"])
        info(f"Branche principale : {d.get('default_branch') or '-'}    "
             f"Dernier envoi : {(d.get('pushed_at') or '-')[:10]}")
        info(f"Copie sur cette machine : {local or 'aucune'}")
        print()
        print("    1. Parcourir les fichiers")
        print("    2. Voir les commits")
        print("    3. Rendre " + ("public" if d.get("private") else "non public"))
        print("    4. " + ("Ouvrir la copie sur cette machine" if local else "Cloner le projet en local"))
        print("    5. Ouvrir la copie en ligne")
        print("    6. Comparer en ligne et en local")
        print("    7. Voir les droits sur ce repo")
        print("    8. Supprimer de GitHub")
        print("    0. Retour")
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
            print()
            ouvrir_navigateur(d["html_url"])
        elif choix == "6":
            if not local:
                warn("Aucune copie sur cette machine : la cloner avec le choix 4.")
                pause()
            elif executer(comparer_copies, cfg, d, local) != "retour":
                pause()
        elif choix == "7":
            afficher_acces(cfg, d)
            pause()
        elif choix == "8":
            if supprimer_depot(cfg, d):
                pause()
                return "supprime"
            pause()
        else:
            return None


def lignes_texte(fichier, maxi=2_000_000):
    """Lignes d'un fichier texte, ou None pour un binaire, un lien ou un trop gros fichier."""
    try:
        if os.path.islink(fichier) or os.path.getsize(fichier) > maxi:
            return None
        with open(fichier, "rb") as f:
            data = f.read()
    except OSError:
        return None
    if b"\0" in data:
        return None
    return data.decode("utf-8", errors="replace").splitlines()


def largeur_terminal():
    return max(80, min(160, shutil.get_terminal_size((100, 24)).columns))


def couper(texte, largeur):
    texte = str(texte)
    return texte if len(texte) <= largeur else texte[:max(largeur - 1, 0)] + "~"


def tableau(entetes, lignes, couleurs=None):
    """Tableau aligne : premiere colonne pour le parametre, puis les colonnes
    de valeurs. couleurs : {(ligne, colonne): code} pour mettre une case en avant."""
    couleurs = couleurs or {}
    total = largeur_terminal() - 4
    larg = [max(len(str(l[i])) for l in [entetes] + lignes) for i in range(len(entetes))]
    larg[0] = min(larg[0], 32)
    while sum(larg) + 2 * (len(larg) - 1) > total and max(larg) > 8:
        larg[larg.index(max(larg))] -= 1  # la colonne la plus large cede la place
    def rangee(cases, n=None):
        morceaux = []
        for i, case in enumerate(cases):
            texte = couper(case, larg[i]).ljust(larg[i])
            code = couleurs.get((n, i))
            morceaux.append(c(texte, code) if code and n is not None else texte)
        return "    " + "  ".join(morceaux).rstrip()
    print(c(rangee(entetes), "1"))
    print("    " + "  ".join("-" * l for l in larg))
    for n, l in enumerate(lignes):
        print(rangee(l, n))


def infos_commit(chemin, ref):
    """Dernier commit de ref : dict avec court, date, epoch, auteur, message."""
    code, out, _ = git("log", "-1", "--format=%h%x1f%ct%x1f%cd%x1f%an%x1f%s",
                       "--date=format:%Y-%m-%d %H:%M", ref, cwd=chemin, quiet=True)
    if code != 0 or not out:
        return None
    court, epoch, date_c, auteur, message = (out.split("\x1f") + [""] * 5)[:5]
    return {"court": court, "epoch": int(epoch or 0), "date": date_c, "auteur": auteur,
            "message": message}


def fichiers_en_ligne(chemin, ref):
    """{fichier: taille} des fichiers de ref."""
    _, out, _ = git("ls-tree", "-r", "-l", "-z", ref, cwd=chemin, quiet=True)
    fichiers = {}
    for rec in out.split("\0"):
        meta, _, nom = rec.partition("\t")
        parts = meta.split()
        if nom and len(parts) == 4 and parts[1] == "blob":
            fichiers[nom] = int(parts[3]) if parts[3].isdigit() else 0
    return fichiers


def fichiers_en_local(chemin):
    """{fichier: taille} des fichiers du dossier, suivis par Git ou nouveaux,
    sans les fichiers ignores."""
    _, out, _ = git("ls-files", "--cached", "--others", "--exclude-standard", "-z",
                    cwd=chemin, quiet=True)
    fichiers = {}
    for nom in (f for f in out.split("\0") if f):
        try:
            fichiers[nom] = os.lstat(os.path.join(chemin, nom)).st_size
        except OSError:
            pass
    return fichiers


def etats_locaux(chemin):
    """{fichier: code XY de git status} des fichiers au commit manquant."""
    _, out, _ = git("status", "--porcelain", "-z", cwd=chemin, quiet=True)
    etats = {}
    for rec in (r for r in out.split("\0") if len(r) > 3):
        etats[rec[3:]] = rec[:2]
    return etats


def ecarts_fichiers(chemin, ref):
    """Fichiers dont le contenu differe entre ref et le dossier, fichiers sans
    commit compris : liste de (fichier, present en ligne, present en local)."""
    _, ns, _ = git("diff", "--no-renames", "--name-only", "-z", ref, "--", cwd=chemin, quiet=True)
    _, autres, _ = git("ls-files", "--others", "--exclude-standard", "-z", cwd=chemin, quiet=True)
    noms = {f for f in ns.split("\0") if f} | {f for f in autres.split("\0") if f}
    en_ligne = fichiers_en_ligne(chemin, ref)
    return sorted(((f, f in en_ligne, os.path.lexists(os.path.join(chemin, f))) for f in noms),
                  key=lambda e: e[0].lower())


def contenu_en_ligne(chemin, ref, fichier):
    """Lignes du fichier dans ref, ou None pour un binaire."""
    code, out, _ = git("show", f"{ref}:{fichier}", cwd=chemin, quiet=True)
    if code != 0 or "\0" in out:
        return None
    return out.splitlines()


def date_fichier(chemin, ref, fichier):
    """Date du dernier commit qui a touche ce fichier dans ref : (texte, epoch)."""
    _, out, _ = git("log", "-1", "--format=%ct %cd", "--date=format:%Y-%m-%d %H:%M", ref, "--",
                    fichier, cwd=chemin, quiet=True)
    epoch, _, texte = out.strip().partition(" ")
    return texte, int(epoch) if epoch.isdigit() else 0


def case_fichier(nb_lignes, taille_octets, date_texte, etat=""):
    debut = "binaire, " + taille(taille_octets) if nb_lignes is None else \
        f"{nb_lignes} ligne{'s' if nb_lignes > 1 else ''}"
    return ", ".join(x for x in (debut, date_texte, etat) if x)


def cote_a_cote(gauche, droite, titre_gauche, titre_droite, contexte=3, maxi=400):
    """Les deux versions d'un fichier cote a cote, en ligne a gauche et en local
    a droite. Les lignes differentes sont en jaune ; une case vide signifie que
    la ligne n'existe pas de ce cote."""
    col = (largeur_terminal() - 4 - 3 - 2 * 6) // 2
    def case(num, texte):
        if num is None:
            return " " * (6 + col)
        texte = couper(texte.expandtabs(4).replace("\r", ""), col)
        return c(f"{num:>5} ", "36") + texte.ljust(col)
    def jaune(num, texte):
        if num is None:
            return " " * (6 + col)
        texte = couper(texte.expandtabs(4).replace("\r", ""), col)
        return c(f"{num:>5} ", "36") + c(texte.ljust(col), "1;33")
    print(c(f"    {'':6}{'En ligne':<{col}} | {'':6}En local", "1"))
    print(f"    {'':6}{couper(titre_gauche, col):<{col}} | {'':6}{couper(titre_droite, col)}")
    print(f"    {'-' * (6 + col)}-+-{'-' * (6 + col)}")
    groupes = list(difflib.SequenceMatcher(None, gauche, droite, autojunk=False)
                   .get_grouped_opcodes(contexte))
    if not groupes:
        info("Contenu identique.")
        return
    affichees = 0
    for g, groupe in enumerate(groupes):
        if g:
            print(f"    {'...':>5}{' ' * (col + 1)} | {'...':>5}")
        for tag, i1, i2, j1, j2 in groupe:
            for k in range(max(i2 - i1, j2 - j1)):
                i = i1 + k if i1 + k < i2 else None
                j = j1 + k if j1 + k < j2 else None
                fmt = case if tag == "equal" else jaune
                print("    " + fmt(None if i is None else i + 1, gauche[i] if i is not None else "")
                      + " | "
                      + fmt(None if j is None else j + 1, droite[j] if j is not None else ""))
                affichees += 1
                if affichees >= maxi:
                    info(f"Fin de l'affichage : {maxi} lignes au plus.")
                    return


def comparer_copies(cfg, d, chemin):
    """Tableau de comparaison entre la copie en ligne, sur sa branche
    principale, et la copie sur cette machine, puis les fichiers differents et
    leurs deux versions cote a cote. Retourne "retour" quand Entree ramene a
    l'ecran du repo."""
    titre("Comparer en ligne et en local")
    remote = remote_principal(chemin)
    if not remote:
        raise Arret("ignore", "la copie sur cette machine n'a aucun lien avec un repo en ligne")
    branche = d.get("default_branch") or branche_courante(chemin) or "main"
    ici = branche_courante(chemin)
    info("Lecture de la copie en ligne...")
    recuperer(cfg, chemin, remote, auto=False)
    ref = f"{remote}/{branche}"
    if not ref_existe(chemin, f"refs/remotes/{ref}"):
        raise Arret("ignore", f"la branche {branche} n'existe pas en ligne")
    ligne_c = infos_commit(chemin, ref)
    local_c = infos_commit(chemin, "HEAD") if a_des_commits(chemin) else None
    f_ligne, f_local = fichiers_en_ligne(chemin, ref), fichiers_en_local(chemin)
    etats = etats_locaux(chemin)
    seulement_ligne = nb_commits(chemin, f"HEAD..{ref}") if local_c else nb_commits(chemin, ref)
    seulement_local = nb_commits(chemin, f"{ref}..HEAD") if local_c else 0
    vide = {"court": "-", "date": "-", "auteur": "-", "message": "aucun commit", "epoch": 0}
    lc, ll = ligne_c or vide, local_c or vide
    lignes = [
        ["Branche", branche, ici or "-"],
        ["Dernier commit", lc["court"], ll["court"]],
        ["Message", lc["message"], ll["message"]],
        ["Date du dernier commit", lc["date"], ll["date"]],
        ["Auteur", lc["auteur"], ll["auteur"]],
        ["Copie la plus neuve", "-", "-"],
        ["Commits", nb_commits(chemin, ref), nb_commits(chemin, "HEAD") if local_c else 0],
        ["Commits en avance", seulement_ligne, seulement_local],
        ["Fichiers", len(f_ligne), len(f_local)],
        ["Taille", taille(sum(f_ligne.values())), taille(sum(f_local.values()))],
        ["Fichiers, commit manquant", "-", len(etats)],
    ]
    couleurs = {}
    if lc["court"] != ll["court"] and lc["epoch"] != ll["epoch"]:
        plus_recent = 1 if lc["epoch"] > ll["epoch"] else 2
        lignes[5][plus_recent] = "oui"
        couleurs[(3, plus_recent)] = couleurs[(5, plus_recent)] = "1;32"
    for n in (7, 10):
        for col in (1, 2):
            if str(lignes[n][col]) not in ("0", "-"):
                couleurs[(n, col)] = "33"
    print()
    tableau(["", "En ligne", "En local"], lignes, couleurs)
    ecarts = ecarts_fichiers(chemin, ref)
    print()
    if not ecarts and not seulement_ligne and not seulement_local:
        ok("Les deux copies sont pareilles.")
        return None
    if seulement_ligne:
        info(f"En ligne : {seulement_ligne} commit(s) en avance, pour les avoir : Recevoir, pull.")
    if seulement_local:
        info(f"En local : {seulement_local} commit(s) en avance, pour les mettre en ligne : Envoyer.")
    if etats:
        info(f"En local : {len(etats)} fichier(s), commit manquant.")
    if not ecarts:
        ok("Les fichiers sont pareils dans les deux copies, seuls les commits changent.")
        return None
    brut, recents = [], {}
    for n, (fichier, present_ligne, present_local) in enumerate(ecarts):
        quand_ligne = quand_local = 0
        if present_ligne:
            texte = contenu_en_ligne(chemin, ref, fichier)
            date_texte, quand_ligne = date_fichier(chemin, ref, fichier)
            d_ligne = (None if texte is None else len(texte), f_ligne.get(fichier), date_texte, "")
        else:
            d_ligne = "absent"
        xy = etats.get(fichier, "")
        if present_local:
            texte = lignes_texte(os.path.join(chemin, fichier))
            if xy:
                quand_local = int(os.lstat(os.path.join(chemin, fichier)).st_mtime)
                date_texte = datetime.fromtimestamp(quand_local).strftime("%Y-%m-%d %H:%M")
                etat = "nouveau, commit manquant" if xy == "??" else "commit manquant"
            else:
                (date_texte, quand_local), etat = date_fichier(chemin, "HEAD", fichier), ""
            d_local = (None if texte is None else len(texte), f_local.get(fichier), date_texte, etat)
        else:
            d_local = "absent, commit manquant" if "D" in xy else "absent"
        if quand_ligne and quand_local and quand_ligne != quand_local:
            recents[(n, 1 if quand_ligne > quand_local else 2)] = "32"
        brut.append((fichier, d_ligne, d_local))
    def cases_fichiers(heure):
        def case(dn):
            if isinstance(dn, str):
                return dn
            nb, t, quand, etat = dn
            return case_fichier(nb, t, quand if heure else quand[:10], etat)
        return [[f, case(a), case(b)] for f, a, b in brut]
    cases = cases_fichiers(True)
    larg = [max(len(f"{len(cases)}. ") + len(cs[0]) for cs in cases)] + \
        [max(len(cs[i]) for cs in cases) for i in (1, 2)]
    if sum(larg) + 4 > largeur_terminal() - 4:
        cases = cases_fichiers(False)  # dates sans l'heure pour tenir dans le terminal
    while True:
        print()
        info(f"Fichiers avec changements : {len(ecarts)}")
        print()
        tableau(["Fichier", "En ligne", "En local"],
                [[f"{i}. {f}", a, b] for i, (f, a, b) in enumerate(cases[:200], 1)],
                recents)
        print("    En vert : la version la plus neuve du fichier.")
        if len(cases) > 200:
            print(f"         ... et {len(cases) - 200} autre(s)")
        print()
        choix = ask("Choix : # du fichier pour voir ses deux versions, Enter pour revenir",
                    "", obligatoire=False)
        if not choix:
            return "retour"
        if not (choix.isdigit() and 1 <= int(choix) <= min(len(cases), 200)):
            warn("Choix invalide.")
            continue
        fichier, c_ligne, c_local = cases[int(choix) - 1]
        present_ligne, present_local = ecarts[int(choix) - 1][1:]
        gauche = contenu_en_ligne(chemin, ref, fichier) if present_ligne else []
        droite = lignes_texte(os.path.join(chemin, fichier)) if present_local else []
        print()
        print(c(f"  {fichier}", "1;36"))
        if gauche is None or droite is None:
            info("Fichier binaire : pas de comparaison ligne par ligne.")
            info(f"En ligne : {c_ligne}")
            info(f"En local : {c_local}")
        else:
            info("En jaune : lignes avec changements. Case vide : ligne absente de ce fichier.")
            print()
            cote_a_cote(gauche, droite, c_ligne, c_local)
        pause()


def _contenu(cfg, full, chemin, ref):
    suffixe = f"/{urllib.parse.quote(chemin)}" if chemin else ""
    return api(cfg, "GET", f"/repos/{full}/contents{suffixe}?ref={urllib.parse.quote(ref)}")


def parcourir_depot(cfg, d):
    full, ref = d["full_name"], d.get("default_branch") or "main"
    chemin = ""
    while True:
        code, rep, _ = _contenu(cfg, full, chemin, ref)
        if code == 404 and not chemin:
            info("Ce repo est vide : aucun fichier pour l'instant.")
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
            print("      0. ..  dossier parent, ou Enter")
        for i, e in enumerate(entrees, 1):
            if e.get("type") == "dir":
                print(f"    {i:>3}. {e['name']}/")
            else:
                print(f"    {i:>3}. {e['name']:<48} {taille(e.get('size')):>9}")
        choix = ask("Choix : # du dossier ou du fichier, Enter pour revenir", "",
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
                info(f"{e['name']} : {e.get('type')}, sous-module ou lien, pas d'affichage.")


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
        info("Fichier trop gros pour l'affichage ici : plus de 1 Mo.")
    elif b"\0" in brut[:8000]:
        info("Fichier binaire, image ou archive : pas d'affichage ici.")
    else:
        lignes = brut.decode("utf-8", "replace").splitlines()
        print()
        for i, l in enumerate(lignes[:80], 1):
            print(c(f"  {i:>4} ", "36") + l)
        if len(lignes) > 80:
            info(f"... {len(lignes) - 80} ligne(s) de plus : o pour tout voir sur GitHub")
    if ask("o pour ouvrir sur GitHub, Enter pour revenir", "", obligatoire=False).lower() == "o":
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
        choix = ask("Choix : # du commit pour voir ses fichiers, Enter pour revenir",
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
    libelles = {"added": "nouveau", "modified": "modif", "removed": "retrait",
                "renamed": "nouveau nom", "changed": "modif", "copied": "copie"}
    infos = cm["commit"]["author"]
    print()
    print(c(f"  {sha[:7]}  {cm['commit']['message'].splitlines()[0]}", "1"))
    info(f"{infos.get('name')}, {infos['date'][:16].replace('T', ' ')}")
    fichiers = cm.get("files") or []
    info(f"Fichiers de ce commit : {len(fichiers)}")
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
        if os.path.isdir(dest) and ask_yn(f"{dest} existe : le lier avec {d['full_name']}, "
                                          "puis pull ?", True):
            res = executer(lier_et_recevoir, cfg, dest, d["clone_url"])
            if res and res[0] == "ok":
                cfg["dernier_dossier"] = dest
                sauver(cfg)
                return dest
            return None
        err(f"{dest} existe et n'est pas vide.")
        return None
    os.makedirs(parent, exist_ok=True)
    if git_direct("clone", d["clone_url"], dest) != 0:
        err("Clone impossible : voir la connexion et les droits, puis recommencer depuis le menu Compte GitHub.")
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
        warn("Public : tout le code ET tout l'historique des commits deviennent visibles.")
        sensibles = sensibles_sur_github(cfg, d)
        if sensibles:
            err("Ce repo contient des fichiers secrets, qui deviendraient publics :")
            for f in sensibles[:20]:
                print("       " + f)
            info("Les retirer et changer les secrets qu'ils contiennent avant de publier.")
            if ask("Taper PUBLIC pour confirmer", "",
                   obligatoire=False) != "PUBLIC":
                info("Abandon.")
                return
        elif not ask_yn(f"Rendre {d['full_name']} PUBLIC ?", False):
            info("Abandon.")
            return
    elif not ask_yn(f"Rendre {d['full_name']} NON PUBLIC ?", True):
        info("Abandon.")
        return
    code, rep, _ = api(cfg, "PATCH", f"/repos/{d['full_name']}", {"private": vers_prive})
    if code == 200:
        d["private"] = vers_prive
        ok(f"{d['full_name']} est maintenant {'non public' if vers_prive else 'public'}.")
    else:
        err(f"Erreur, code {code} : {message_api(rep)}")


def supprimer_depot(cfg, d):
    warn("Suppression SANS RETOUR sur GitHub : code, issues et historique.")
    info("Les copies sur cette machine restent.")
    if ask(f"Taper '{d['name']}' pour confirmer", "", obligatoire=False) != d["name"]:
        info("Abandon.")
        return False
    code, rep, _ = api(cfg, "DELETE", f"/repos/{d['full_name']}")
    if code == 204:
        ok(f"{d['full_name']} : suppression OK.")
        return True
    if code == 403:
        warn("Cette connexion n'a pas le droit de supprimer des repos.")
        info("La page Settings du repo s'ouvre : tout en bas, 'Delete this repository'.")
        webbrowser.open(d["html_url"] + "/settings")
        return False
    err(f"Erreur, code {code} : {message_api(rep)}")
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
        info("Jeton fin : permissions au choix, repo par repo, sur GitHub.")
    expiration = hdr.get("GitHub-Authentication-Token-Expiration")
    if expiration:
        info(f"Expiration du jeton : {expiration}")
    repos = api_liste(cfg, "/user/repos?per_page=100"
                           "&affiliation=owner,collaborator,organization_member")
    if repos is not None:
        ecriture = sum(1 for r in repos if (r.get("permissions") or {}).get("push"))
        info(f"Repos ouverts pour ce compte : {len(repos)}, dont {ecriture} avec droit de push.")
    info("Ces informations sont lues a chaque fois : un changement de permission sur")
    info("GitHub compte tout de suite, sans nouvelle connexion.")


def act_compte(cfg):
    titre("Compte GitHub")
    assurer_connexion(cfg, interactif=False)
    user = SESSION["user"]
    if user:
        ok(f"En session : {user.get('login')}, {NOMS_METHODES[SESSION['methode']]}, {host(cfg)}")
    else:
        warn(f"Aucune session sur {host(cfg)}.")
    cles = cles_enregistrees(cfg)
    if cles:
        info(f"Identifiants sur cette machine : {texte_garde(cfg) or 'toujours'}.")
    print()
    if user:
        actions = [("Changer de moyen de connexion", changer_moyen),
                   ("Changer de compte", changer_compte),
                   ("Changer de serveur GitHub Entreprise", changer_serveur),
                   ("Voir les permissions de la connexion", afficher_permissions)]
        if cles:
            actions.append(("Temps de garde du compte sur cette machine", choisir_garde))
        actions.append(("Fermer la session", deconnexion))
    else:
        actions = [("Se connecter", connexion_interactive),
                   ("Changer de serveur GitHub Entreprise", changer_serveur)]
        if traces_compte(cfg):
            actions.append(("Supprimer les identifiants de cette machine", act_supprimer_compte))
    fn = menu_numerote(actions)
    if fn:
        fn(cfg)


# Ordinateur entreprise : repo recupere depuis GitHub -> repo de l'entreprise

def verifier_url(url):
    """Refuse les adresses non chiffrees (http://, git://) et retire tout
    identifiant ecrit dans l'adresse."""
    url = url.strip().strip('"').strip("'")
    if re.match(r"^(http|git)://", url, re.I):
        raise Arret("erreur", "adresse sans chiffrement : utiliser https:// ou SSH")
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
    info(f"Clone de {url_web(url) or url}...")
    code, _, e = git(*args, quiet=True, env=_env_git(anonyme=True))
    avec_jeton = False
    if code != 0:
        if est_ssh(url) or not (erreur_auth(e) or "not found" in e.lower()):
            raise Arret("erreur", "clone impossible : " + expliquer(e))
        serveur = infos_url(url)[0] or "github.com"
        if SESSION["token"] and serveur == host(cfg):
            info("Repo non public : clone avec le compte GitHub en session.")
            jeton = SESSION["token"]
        else:
            warn("Ce repo demande une connexion : il est non public, ou son nom est faux.")
            print("    1. Se connecter au compte perso pour ce clone seulement")
            print("    0. Annuler")
            if ask("Choix", "0") != "1":
                raise Arret("ignore", "abandon : aucun changement")
            _, jeton = demander_secret(serveur)
        if os.path.exists(cible):
            supprimer_dossier(cible)
        code, _, e = git(*args, quiet=True, env=_env_git(identite=("", jeton)))
        jeton = None
        if code != 0:
            raise Arret("erreur", "clone impossible : " + expliquer(e))
        avec_jeton = True
    if not a_des_commits(cible):
        raise Arret("erreur", "le repo du clone est vide")
    _, dernier, _ = git("log", "-1", "--format=%h  %s", cwd=cible, quiet=True)
    ok(f"Clone OK : {cle_depot(url) or url}, dernier commit {dernier}")
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
    print(f"       {len(plan['modifies']):>5} avec changements")
    print(f"       {len(plan['nouveaux']):>5} nouveau(x)")
    if supprimer:
        print(f"       {len(a_supprimer):>5} en moins, absents du projet")
    elif plan["en_trop"]:
        print(f"       {len(plan['en_trop']):>5} restent, absents du projet")
    if not a_copier and not a_supprimer:
        info("Le repo de l'entreprise contient ce projet tel quel.")
        return True
    print()
    for libelle, liste in (("modif   ", plan["modifies"]), ("nouveau ", plan["nouveaux"]),
                           ("retrait ", a_supprimer)):
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
        warn(f"{len(echecs)} fichier(s) sans copie ni suppression possible :")
        for x in echecs[:10]:
            print("       " + x)
    ok(f"Nouveau contenu : {len(a_copier)} copie(s), {len(a_supprimer)} suppression(s)"
       + (f", {len(echecs)} erreur(s)." if echecs else "."))
    return True


def mettre_a_jour_destination(cfg, racine, remote, branche):
    """Aligne la destination sur le serveur avant la copie (fast-forward)."""
    modifs = fichiers_modifies(racine)
    if modifs:
        warn(f"La destination a {len(modifs)} changement(s), commit manquant :")
        for l in modifs[:10]:
            print(f"       {statut_porcelaine(l):<9} {l[3:]}")
        info("Elles partiront avec ce commit.")
        if not ask_yn("Continuer ?", True):
            raise Arret("ignore", "abandon")
    info("Pull de la destination depuis le serveur...")
    recuperer(cfg, racine, remote, auto=False)
    ref = f"refs/remotes/{remote}/{branche}"
    if modifs or not ref_existe(racine, ref):
        return
    ar = avance_retard(racine, remote, branche)
    if not a_des_commits(racine) or (ar and ar[1] and not ar[0]):
        code, _, e = git("merge", "--ff-only", f"{remote}/{branche}", cwd=racine)
        if code != 0:
            raise Arret("erreur", "pull de la destination impossible : "
                        + expliquer(e))
        ok("Destination pareille au serveur.")


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
        err("Fork fait pendant l'ouverture : il reste public, avec le repo non public aussi.")
        for n in nouveaux:
            print("       " + n)
    else:
        print("       forks : 0")
    for cle, libelle, propres in (("etoiles", "stars en plus", 0), ("vues", "vues selon GitHub", 0),
                                  ("clones", "autres clones selon GitHub", 1)):
        if cle in avant and cle in apres:
            ecart = max(0, apres[cle] - avant[cle] - propres)  # le clone de gitmanager est retire
            print(f"       {libelle} : {ecart}")
            vu = vu or ecart > 0
    if "vues" not in apres:
        info("Vues et clones : pas de chiffres avec cette connexion.")
    if vu:
        warn("Visite ou copie du repo pendant l'ouverture.")
    else:
        ok("Aucune visite pour l'instant.")
    info("GitHub compte les vues et les clones avec plusieurs heures de retard. Tout")
    info(f"reste visible sur https://{host(cfg)}/{full}/graphs/traffic")


def rendre_prive(cfg, full):
    for _ in range(3):
        code, _, _ = requete(cfg, "PATCH", f"/repos/{full}", {"private": True})
        if code == 200:
            ok(f"{full} est de nouveau non public.")
            return True
        time.sleep(1)
    err(f"{full} EST TOUJOURS PUBLIC : le rendre non public sur https://{host(cfg)}/{full}/settings")
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
        print(f"\r  {full} public encore {reste // 60}:{reste % 60:02d}, Enter pour le rendre "
              "non public tout de suite ", end="", flush=True)
        appui.wait(1)
    print()
    return appui.is_set(), appui


def recuperer_avec_ouverture(cfg, d, dossier):
    """Repo prive : public le temps du clone, puis prive apres un compte a rebours.
    Sans accord, il est recupere avec le compte connecte, sans devenir public."""
    full = d["full_name"]
    print()
    warn(f"{full} est non public.")
    info("Pour le cloner sans identifiants, il peut devenir public le temps du clone,")
    duree = f"{DUREE_PUBLIC // 60} minutes" if DUREE_PUBLIC % 60 == 0 else f"{DUREE_PUBLIC} secondes"
    info(f"puis redevenir non public au bout de {duree}. Pendant ce temps, tout le")
    info("monde peut le voir et le copier, historique compris.")
    sensibles = sensibles_sur_github(cfg, d)
    if sensibles:
        err("Ce repo contient des fichiers secrets, qui deviendraient publics :")
        for chemin in sensibles[:20]:
            print("       " + chemin)
    if not ask_yn("Rendre le repo public le temps du clone ?", False):
        info("Le repo reste non public : clone avec le compte GitHub en session.")
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
        info("Enter pour continuer.")
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
            info(f"Copie locale sur le Bureau : {dest}")
            return dest
        dest = os.path.join(bureau, f"{nom}-{n}")
        n += 1
    info(f"Copie locale du repo de l'entreprise sur le Bureau : {dest}")
    if git_direct("clone", url, dest) != 0:
        raise Arret("erreur", "clone du repo de l'entreprise impossible : voir l'adresse et les droits")
    return dest


def import_prive(cfg, url, dossier):
    """Repo prive : connexion au compte GitHub perso le temps de l'import, puis
    oubli de cette connexion si elle etait gardee en memoire."""
    deja = bool(PROFILS["perso"]["token"])
    activer_profil("perso")
    try:
        if not connexion_obligatoire(cfg):
            raise Arret("ignore", "abandon de l'import")
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
    saisie = ask("Repo pour l'import : URL GitHub, compte/nom ou dossier sur cette machine, "
                 "Enter pour revenir", obligatoire=False)
    if not saisie:
        return None, None
    url = url_source(saisie)
    if os.path.isdir(url):  # dossier de cette machine : aucune connexion
        return importer_local(url, dossier)
    nom = cle_depot(url) or os.path.basename(url.rstrip("/\\")).removesuffix(".git")
    copie = copie_locale_de(cfg, url)
    if copie:
        _, dernier, _ = git("log", "-1", "--format=%h, %cd : %s", "--date=format:%Y-%m-%d %H:%M",
                            cwd=copie, quiet=True)
        print()
        info(f"Copie de {nom} sur cette machine : {copie}")
        info(f"Dernier commit de cette copie : {dernier or 'aucun'}")
        print()
        print("    1. Utiliser cette copie, sans connexion au compte perso")
        print("    2. Importer depuis GitHub")
        print("    0. Retour")
        choix = ask("Choix", "1")
        if choix == "1":
            return importer_local(copie, dossier, nom)
        if choix != "2":
            return None, None
    cible = os.path.join(dossier, "projet")
    info(f"Import de {url_web(url) or url}...")
    code, _, e = git("clone", "--depth", "1", "--no-tags", "--quiet", url, cible,
                     quiet=True, env=_env_git(anonyme=True))
    if code != 0:
        if est_ssh(url) or not (erreur_auth(e) or "not found" in e.lower()):
            raise Arret("erreur", "import impossible : " + expliquer(e))
        warn("Ce repo demande une connexion : il est non public, ou son nom est faux.")
        if not ask_yn("Se connecter au compte GitHub perso pour cet import ?", True):
            raise Arret("ignore", "abandon de l'import")
        if os.path.exists(cible):
            supprimer_dossier(cible)
        cible = import_prive(cfg, url, dossier)
    _, dernier, _ = git("log", "-1", "--format=%h  %s", cwd=cible, quiet=True)
    ok(f"Import OK : {nom}, dernier commit {dernier}")
    return cible, nom


def copie_locale_de(cfg, url):
    """Copie de ce repo sur cette machine, trouvee par son adresse en ligne, ou None."""
    cle_cherchee = (cle_depot(url) or "").lower()
    if not cle_cherchee:
        return None
    projets = projets_de_la_machine(cfg)
    with ThreadPoolExecutor(max_workers=8) as pool:
        for p, urls in zip(projets, pool.map(urls_remotes, projets)):
            if any((cle_depot(u) or "").lower() == cle_cherchee for u in urls.values()):
                return p
    return None


def importer_local(chemin, dossier, nom=None):
    """Import depuis un dossier de cette machine, sans connexion : le dernier commit
    d'un repo git, ou les fichiers d'un dossier sans git."""
    nom = nom or os.path.basename(os.path.normpath(chemin))
    if racine_repo(chemin) is None:
        ok(f"Import OK : {nom}, fichiers du dossier {chemin}")
        return chemin, nom
    modifs = fichiers_modifies(chemin)
    if modifs:
        warn(f"{len(modifs)} fichier(s), commit manquant, dans cette copie : ils ne font pas partie "
             "de l'import.")
    cible = os.path.join(dossier, "projet")
    code, _, e = git("clone", "--quiet", "--no-hardlinks", chemin, cible, quiet=True)
    if code != 0:
        raise Arret("erreur", "import impossible : " + expliquer(e))
    _, dernier, _ = git("log", "-1", "--format=%h  %s", cwd=cible, quiet=True)
    ok(f"Import OK : {nom}, dernier commit {dernier}, depuis cette machine")
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
        print(c(f"    {'#':>3}  {'Repo':<44} Droits", "1"))
        for i, d in enumerate(repos, 1):
            droits = "push" if (d.get("permissions") or {}).get("push") else "lecture"
            print(f"    {i:>3}. {d['full_name'][:44]:<44} {droits}")
    else:
        info("Aucun repo sur ce compte.")
    print()
    print("      U. Saisir l'URL du repo de l'entreprise")
    choix = ask("Choix : # du repo ou U, Enter pour revenir", "", obligatoire=False).lower()
    if choix.isdigit() and 1 <= int(choix) <= len(repos):
        url, nom = repos[int(choix) - 1]["clone_url"], repos[int(choix) - 1]["name"]
    elif choix == "u":
        saisie = ask("URL du repo de l'entreprise, https://... ou git@..., Enter pour revenir",
                     obligatoire=False)
        if not saisie:
            return "retour"
        url = verifier_url(saisie)
        nom = infos_url(url)[2] or os.path.basename(url.rstrip("/\\")).removesuffix(".git")
    else:
        if choix:
            warn("Choix invalide.")
            return "ignore", "abandon"
        return "retour"
    print()
    print("    1. Ajouter au repo, sans rien enlever")
    print("    2. Remplacer le repo par le projet")
    print("    0. Retour")
    mode = ask("Choix", "0")
    if mode not in ("1", "2"):
        return "retour"
    existante = copie_locale_de(cfg, url)
    if existante and ask_yn(f"Copie de ce repo sur cette machine : {existante}. L'utiliser pour "
                            "l'envoi ?", True):
        copie = existante
    else:
        copie = copie_sur_bureau(url, nom)
    branche, remote = preparer(cfg, copie, auto=False)
    mettre_a_jour_destination(cfg, copie, remote, branche)
    if not remplacer_contenu(source, copie, supprimer=mode == "2"):
        return "ignore", "abandon : aucun changement dans le repo de l'entreprise"
    print()
    fait = committer(cfg, copie, proposer_gitignore=False)
    statut, detail = pousser(cfg, copie, remote, branche, commit_affiche=fait > 0)
    info(f"La copie locale reste ici : {copie}")
    return statut, detail


def act_importer(cfg):
    titre("Importer un repo GitHub")
    temp = tempfile.mkdtemp(prefix="gitmanager-")
    try:
        source, nom = importer_repo(cfg, temp)
        if not source:
            return None
        while True:
            titre(f"Projet : {nom}")
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
    niveaux = (("admin", "admin"), ("maintain", "maintenance"), ("push", "push"),
               ("triage", "tri des issues"), ("pull", "lecture"))
    droits = d.get("permissions") or {}
    role = next((libelle for k, libelle in niveaux if droits.get(k)), "aucun")
    login = (SESSION["user"] or {}).get("login")
    if login:
        code, rep, _ = requete(cfg, "GET", f"/repos/{d['full_name']}/collaborators/{login}/permission")
        if code == 200 and isinstance(rep, dict) and rep.get("role_name"):
            role = rep["role_name"]
    print()
    info(f"Droits de {login or 'ce compte'} sur {d['full_name']} : {role}")
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
    print(c("    Coder chez soi, push au bureau, les repos GitHub", "36"))
    print(c("    en local comme en ligne.", "36"))



def lancer(fn, cfg, avec_pause=True):
    try:
        executer(fn, cfg)
    except KeyboardInterrupt:
        print()
        warn("Interruption.")
    except Exception as e:
        err("Erreur inattendue dans le programme : relancer l'action.")
        if DEBUG:
            err(f"{type(e).__name__} : {e}")
    if avec_pause:
        pause()


def ligne_compte(cfg):
    print(c(f"  Compte GitHub {nom_compte()} : {SESSION['user'].get('login')}, "
            f"{NOMS_METHODES[SESSION['methode']]}, {host(cfg)}", "36"))


def compte_a_choisir(noms, action):
    """Compte vise par un choix du menu principal : la question est posee
    seulement quand plusieurs comptes sont concernes."""
    if len(noms) <= 1:
        return noms[0] if noms else None
    print()
    print(f"  {action}, pour quel compte GitHub ?")
    for i, nom in enumerate(noms, 1):
        user = PROFILS[nom]["user"]
        print(f"    {i}. Compte GitHub {nom_compte(nom)}" + (f" : {user.get('login')}" if user else ""))
    print("    0. Retour")
    choix = ask("Choix", "0")
    return noms[int(choix) - 1] if choix.isdigit() and 1 <= int(choix) <= len(noms) else None


def pour_un_compte(noms, action, fn):
    """Choix du menu principal applique a l'un des comptes donnes."""
    def lancer_pour(cfg):
        nom = compte_a_choisir(noms, action)
        if nom:
            activer_profil(nom)
            fn(cfg)
            pause()
    return lancer_pour


def actions_principales(cfg):
    """Choix du menu principal sous les deux options : Se connecter, pour un compte
    sans session. Les identifiants se gerent dans les menus des repos GitHub."""
    connectes = [n for n in PROFILS if PROFILS[n]["user"]]
    libres = [n for n in PROFILS if n not in connectes]
    actions = []
    if libres:
        actions.append(("Se connecter", pour_un_compte(libres, "Se connecter",
                                                        lambda cfg: assurer_connexion(cfg))))
    return actions, connectes


def choisir_fonction(cfg):
    """Menu principal. Retourne le profil de l'option choisie, ou None pour quitter."""
    while True:
        verifier_gardes(cfg)
        titre("MENU PRINCIPAL")
        actions, connectes = actions_principales(cfg)
        for nom in connectes:
            print(c(f"  Compte GitHub {nom_compte(nom)} : {PROFILS[nom]['user'].get('login')}", "36"))
        print()
        print("    1. Repos GitHub perso")
        print("    2. Entreprise : import et repos GitHub de l'entreprise")
        for i, (libelle, _) in enumerate(actions, 3):
            print(f"    {i}. {libelle}")
        print("    0. Quitter")
        choix = ask("Choix", "0")
        if choix in ("1", "2"):
            return "perso" if choix == "1" else "entreprise"
        if choix in ("0", "q", "quit", "exit"):
            return None
        if choix.isdigit() and 3 <= int(choix) < 3 + len(actions):
            lancer(actions[int(choix) - 3][1], cfg, avec_pause=False)
        else:
            warn("Choix invalide.")


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
    """Repos GitHub en ligne, en local et compte, pour le compte actif."""
    while not NAVIGATION["menu_principal"] and connexion_obligatoire(cfg):
        verifier_gardes(cfg)
        if NAVIGATION["menu_principal"]:
            return
        titre(nom_menu)
        ligne_compte(cfg)
        cles = cles_enregistrees(cfg)
        if cles:
            print(c(f"  Identifiants sur cette machine : {texte_garde(cfg) or 'toujours'}", "36"))
        identifiants = bool(traces_compte(cfg))
        print()
        print("    1. Repos GitHub en ligne")
        print("    2. Repos GitHub en local")
        print("    3. Compte GitHub")
        print("    4. Fermer la session")
        if identifiants:
            print("    5. Supprimer les identifiants de cette machine")
        print("    0. Retour")
        choix = ask("Choix", "0")
        if choix in ("0", "q", "quit", "exit"):
            return
        if choix == "4":
            lancer(deconnexion, cfg)
            continue
        if choix == "5" and identifiants:
            lancer(act_supprimer_identifiants, cfg)
            continue
        fn = {"1": act_depots, "2": act_depots_locaux, "3": act_compte}.get(choix)
        if fn:
            lancer(fn, cfg, avec_pause=choix == "3")
        else:
            warn("Choix invalide.")


def menu_perso(cfg):
    menu_repos(cfg, "OPTION 1 : REPOS GITHUB PERSO")


def menu_entreprise(cfg):
    while not NAVIGATION["menu_principal"] and connexion_obligatoire(cfg):
        titre("OPTION 2 : ENTREPRISE")
        ligne_compte(cfg)
        print()
        print("    1. Importer un repo GitHub")
        print("    2. Repos GitHub de l'entreprise")
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
    cfg.pop("voir_git", None)
    verifier_gardes(cfg, demarrage=True)
    for nom in PROFILS:  # comptes de cette machine : session ouverte au demarrage
        activer_profil(nom)
        if cles_enregistrees(cfg) and connexion_enregistree(cfg):
            ok(f"Session ouverte : compte GitHub {nom_compte()} {SESSION['user'].get('login')}.")
    activer_profil("perso")
    try:
        while True:
            option = choisir_fonction(cfg)
            NAVIGATION["menu_principal"] = False
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
        verifier_gardes(cfg, fin=True)
        for session in PROFILS.values():  # oubli des jetons des deux comptes
            session.update(token="", user=None, methode=None)
    print("\n  Fin du programme.\n")


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
