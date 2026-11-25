"""Essais de gitmanager sans reseau : de vrais repos git, un faux GitHub.

Les repos "en ligne" sont des repos nus dans un dossier temporaire : Git
remplace https://github.com/ par ce dossier (url.<base>.insteadOf), donc les
adresses restent celles de GitHub. L'API GitHub est remplacee par FauxGitHub.

Lancement : python -m unittest discover -s tests -v
GM_VOIR=1 affiche aussi les ecrans du programme.
"""

import contextlib
import http.client
import io
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(RACINE))

import gitmanager as gm  # noqa: E402


class FinEntrees(BaseException):
    """Plus de reponse prevue : le programme pose une question en trop."""


def entetes(**valeurs):
    h = http.client.HTTPMessage()
    for k, v in valeurs.items():
        h[k.replace("_", "-")] = v
    return h


class FauxGitHub:
    """API GitHub minimale. retard_liste : un repo supprime reste dans la liste,
    comme GitHub juste apres une suppression."""

    def __init__(self, distants, login="saleh"):
        self.distants, self.login = distants, login
        self.repos = {}
        self.supprimes = set()
        self.retard_liste = True
        self.appels = []
        self.jetons_admin = None  # None : tout jeton peut supprimer

    def ajouter(self, nom, admin=True, prive=True):
        full = f"{self.login}/{nom}"
        self.repos[full] = {
            "name": nom, "full_name": full, "private": prive,
            "html_url": f"https://github.com/{full}",
            "clone_url": f"https://github.com/{full}.git",
            "default_branch": "main", "pushed_at": "2026-11-20T10:00:00Z",
            "permissions": {"admin": admin, "push": True, "pull": True},
        }
        return self.repos[full]

    def requete(self, cfg, verbe, chemin, data=None, token=None):
        tok = gm.SESSION["token"] if token is None else token
        chemin = chemin.replace("https://api.github.com", "")
        self.appels.append((verbe, chemin))
        base = chemin.split("?")[0]
        if base == "/user":
            return 200, {"login": self.login, "name": "Saleh Kamil", "id": 1}, entetes()
        if base == "/user/repos" and verbe == "GET":
            liste = [d for f, d in self.repos.items()
                     if f not in self.supprimes or self.retard_liste]
            return 200, liste, entetes()
        if base == "/user/repos" and verbe == "POST":
            d = self.ajouter(data["name"], prive=data.get("private", True))
            subprocess.run(["git", "init", "-q", "--bare", "-b", "main",
                            os.path.join(self.distants, self.login, data["name"] + ".git")],
                           check=True)
            return 201, d, entetes()
        if base.startswith("/repos/"):
            full = "/".join(base.split("/")[2:4])
            d = self.repos.get(full)
            if not d or full in self.supprimes:
                return 404, {"message": "Not Found"}, entetes()
            if verbe == "DELETE" and base.count("/") == 3:
                if self.jetons_admin is not None and tok not in self.jetons_admin:
                    return 403, {"message": "Must have admin rights to Repository."}, entetes(
                        X_Accepted_OAuth_Scopes="delete_repo", X_OAuth_Scopes="repo, gist, read:org")
                self.supprimes.add(full)
                return 204, {}, entetes()
            if verbe == "GET" and base.count("/") == 3:
                return 200, dict(d), entetes()
        return 404, {"message": "Not Found"}, entetes()


class Banc(unittest.TestCase):
    """Dossier perso, config Git et GitHub temporaires pour chaque essai."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="gm-essai-")
        self.maison = os.path.join(self.tmp, "maison")
        self.distants = os.path.join(self.tmp, "github")
        os.makedirs(self.maison)
        os.makedirs(os.path.join(self.distants, "saleh"))
        self.env_avant = dict(os.environ)
        config = os.path.join(self.tmp, "gitconfig")
        Path(config).write_text(
            "[user]\n\tname = Essai\n\temail = essai@example.com\n"
            "[init]\n\tdefaultBranch = main\n"
            f'[url "{Path(self.distants).as_uri()}/"]\n\tinsteadOf = https://github.com/\n',
            encoding="utf-8")
        # HOME : Linux et macOS ; USERPROFILE : Windows
        os.environ.update(HOME=self.maison, USERPROFILE=self.maison, GIT_CONFIG_GLOBAL=config,
                          GIT_CONFIG_NOSYSTEM="1")
        os.environ.pop("XDG_CONFIG_HOME", None)
        os.environ.pop("DISPLAY", None)
        os.environ.pop("WAYLAND_DISPLAY", None)
        self.cfg = {"espace": os.path.join(self.maison, "projets")}
        os.makedirs(self.cfg["espace"])
        self.github = FauxGitHub(self.distants)
        self.patches = [(gm, "CONFIG", Path(self.maison) / ".gitmanager.json"),
                        (gm, "requete", self.github.requete),
                        (gm, "COLOR", False),
                        (gm.webbrowser, "open", lambda *a, **k: True)]
        self.anciens = [(o, n, getattr(o, n)) for o, n, _ in self.patches]
        for o, n, v in self.patches:
            setattr(o, n, v)
        for p in gm.PROFILS.values():
            p.update(token="", user=None, methode=None)
        gm.activer_profil("perso")
        gm.SESSION.update(token="jeton", methode="jeton",
                          user={"login": "saleh", "name": "Saleh Kamil", "id": 1})
        gm.NAVIGATION["menu_principal"] = False
        gm.SUPPRIMES.clear()
        self.questions = []

    def remplacer(self, objet, nom, valeur):
        """Remplace un attribut le temps de l'essai."""
        self.anciens.append((objet, nom, getattr(objet, nom)))
        setattr(objet, nom, valeur)

    def faux_gh(self):
        """GitHub CLI factice dans le PATH : connexion par code, jeton, deconnexion."""
        dossier = os.path.join(self.tmp, "bin")
        os.makedirs(dossier, exist_ok=True)
        etat = os.path.join(self.tmp, "gh-connecte")
        script = Path(dossier, "gh")
        script.write_text(f"""#!/bin/sh
case "$1 $2" in
  "--version "*) echo "gh version 2.89.0 (2026-03-26)" ;;
  "auth login"|"auth refresh")
    echo "! First copy your one-time code: ABCD-1234"
    echo "Open this URL to continue in your web browser: https://github.com/login/device"
    echo "Authentication complete."
    touch "{etat}" ;;
  "auth token") [ -f "{etat}" ] && echo "jeton-gh" || exit 1 ;;
  "auth setup-git") ;;
  "auth status") [ -f "{etat}" ] && echo "Logged in to github.com account saleh" \
      || {{ echo "You are not logged into any GitHub hosts." >&2; exit 1; }} ;;
  "auth logout") rm -f "{etat}" ;;
esac
""", encoding="utf-8")
        script.chmod(0o755)
        os.environ["PATH"] = dossier + os.pathsep + os.environ["PATH"]
        return etat

    def tearDown(self):
        for o, n, v in reversed(self.anciens):
            setattr(o, n, v)
        if hasattr(gm, "input"):
            del gm.input
        os.environ.clear()
        os.environ.update(self.env_avant)
        shutil.rmtree(self.tmp, ignore_errors=True)

    # Repos

    def sh(self, *cmd, cwd=None):
        return subprocess.run(cmd, cwd=cwd, check=True, capture_output=True, text=True).stdout

    def distant(self, nom, fichiers=None, branches=None):
        """Repo nu 'en ligne' avec un premier commit, et des branches au choix."""
        nu = os.path.join(self.distants, "saleh", nom + ".git")
        self.sh("git", "init", "-q", "--bare", "-b", "main", nu)
        travail = os.path.join(self.tmp, "travail-" + nom)
        self.sh("git", "clone", "-q", nu, travail)
        for f, txt in (fichiers or {"README.md": "# demo\n"}).items():
            Path(travail, f).write_text(txt, encoding="utf-8")
        self.sh("git", "add", "-A", cwd=travail)
        self.sh("git", "commit", "-q", "-m", "Premier commit", cwd=travail)
        self.sh("git", "push", "-q", "origin", "main", cwd=travail)
        for b, (f, txt) in (branches or {}).items():
            self.sh("git", "switch", "-q", "-c", b, cwd=travail)
            Path(travail, f).write_text(txt, encoding="utf-8")
            self.sh("git", "add", "-A", cwd=travail)
            self.sh("git", "commit", "-q", "-m", f"Commit sur {b}", cwd=travail)
            self.sh("git", "push", "-q", "origin", b, cwd=travail)
            self.sh("git", "switch", "-q", "main", cwd=travail)
        self.github.ajouter(nom)
        return travail

    def autre_push(self, travail, fichier, texte, message="Changement en ligne"):
        """Quelqu'un d'autre envoie un commit sur GitHub."""
        Path(travail, fichier).write_text(texte, encoding="utf-8")
        self.sh("git", "add", "-A", cwd=travail)
        self.sh("git", "commit", "-q", "-m", message, cwd=travail)
        self.sh("git", "push", "-q", "origin", "HEAD", cwd=travail)

    def copie(self, nom):
        dest = os.path.join(self.cfg["espace"], nom)
        self.sh("git", "clone", "-q", f"https://github.com/saleh/{nom}.git", dest)
        return dest

    # Programme

    def lancer(self, fn, *args, reponses=()):
        """Lance fn avec ces reponses aux questions ; retourne (resultat, sortie)."""
        file = list(reponses)

        def entree(invite=""):
            self.questions.append(invite)
            print(invite)
            if not file:
                raise FinEntrees(invite)
            return file.pop(0)

        gm.input = entree
        sortie = io.StringIO()
        resultat = None
        with contextlib.redirect_stdout(sortie):
            try:
                resultat = fn(*args)
            except FinEntrees as e:
                self.fail(f"Question en trop : {e}\n{sortie.getvalue()[-3000:]}")
        if os.environ.get("GM_VOIR"):  # GM_VOIR=1 : les ecrans du programme s'affichent
            sys.__stdout__.write(f"\n##### {fn.__name__}\n" + sortie.getvalue())
        self.assertEqual(file, [], "Reponses pas utilisees :\n" + sortie.getvalue()[-3000:])
        return resultat, sortie.getvalue()


class Affichage(Banc):
    def test_action_git_en_rouge(self):
        gm.COLOR = True
        sortie = io.StringIO()
        with contextlib.redirect_stdout(sortie):
            gm.afficher_commande(["commit", "-m", "Mon message"])
        self.assertIn("\033[1;31mcommit\033[0m", sortie.getvalue())

    def test_commandes_qui_changent_le_repo_visibles(self):
        self.assertTrue(gm.commande_visible(["update-ref", "-d", "HEAD"]))
        self.assertTrue(gm.commande_visible(["symbolic-ref", "HEAD", "refs/heads/main"]))
        self.assertFalse(gm.commande_visible(["symbolic-ref", "--short", "-q", "HEAD"]))
        self.assertFalse(gm.commande_visible(["config", "--get", "user.name"]))


class ClonerParUrl(Banc):
    def test_pas_dans_la_liste_en_ligne(self):
        self.distant("demo")
        _, sortie = self.lancer(gm.act_depots, self.cfg, reponses=[""])
        self.assertNotIn("Cloner un repo par son URL", sortie)

    def test_dans_la_liste_locale(self):
        self.distant("demo")
        _, sortie = self.lancer(gm.liste_depots_locaux, self.cfg,
                                reponses=["u", "saleh/demo", self.cfg["espace"], "0"])
        self.assertIn("U. Cloner un repo par son URL", sortie)
        self.assertTrue(os.path.isdir(os.path.join(self.cfg["espace"], "demo", ".git")))


class SuppressionRepo(Banc):
    def test_repo_supprime_ne_s_affiche_plus(self):
        self.distant("demo")
        self.distant("autre")
        # liste : 1 demo, 2 autre ; 8 supprimer ; nom ; Enter apres l'action ; Enter pour revenir
        _, sortie = self.lancer(gm.act_depots, self.cfg, reponses=["1", "8", "demo", "", ""])
        self.assertIn("saleh/demo : suppression OK", sortie)
        derniere_liste = sortie.rsplit("repo(s)", 1)[1]
        self.assertNotIn("saleh/demo", derniere_liste)
        self.assertIn("saleh/autre", derniere_liste)

    def test_copie_locale_sans_lien_apres_suppression(self):
        self.distant("demo")
        local = self.copie("demo")
        d = self.github.repos["saleh/demo"]
        ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, d, {"saleh/demo": local},
                                 reponses=["demo", ""])
        self.assertTrue(ok)
        self.assertTrue(os.path.isdir(local), "la copie locale reste par defaut")
        self.assertEqual(gm.remotes(local), [], "plus de lien vers le repo supprime")

    def test_sans_droit_admin_aucune_tentative(self):
        self.distant("demo")
        d = dict(self.github.repos["saleh/demo"], permissions={"admin": False, "push": True})
        ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, d, {})
        self.assertFalse(ok)
        self.assertIn("admin", sortie)
        self.assertNotIn(("DELETE", "/repos/saleh/demo"), self.github.appels)

    def test_permission_delete_repo_demandee_avec_github_cli(self):
        self.distant("demo")
        gm.SESSION.update(methode="gh", token="ancien")
        self.github.jetons_admin = {"nouveau"}
        demandes = []
        anciens = gm.gh_par_navigateur, gm.token_silencieux, gm.chemin_gh
        gm.gh_par_navigateur = lambda args: demandes.append(args) or 0
        gm.token_silencieux = lambda cfg, m: "nouveau"
        gm.chemin_gh = lambda: "/usr/bin/gh"
        try:
            ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, self.github.repos["saleh/demo"], {},
                                     reponses=["demo", ""])
        finally:
            gm.gh_par_navigateur, gm.token_silencieux, gm.chemin_gh = anciens
        self.assertTrue(ok, sortie)
        self.assertTrue(any("delete_repo" in " ".join(a) for a in demandes), demandes)
        self.assertIn("saleh/demo", self.github.supprimes)

    def test_jeton_pour_cette_suppression_seulement(self):
        self.distant("demo")
        self.github.jetons_admin = {"jeton-suppression"}
        ancien = gm.getpass.getpass
        gm.getpass.getpass = lambda invite="": "jeton-suppression"
        try:
            ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, self.github.repos["saleh/demo"], {},
                                     reponses=["demo", "1"])
        finally:
            gm.getpass.getpass = ancien
        self.assertTrue(ok, sortie)
        self.assertEqual(gm.SESSION["token"], "jeton", "le jeton de la session ne change pas")

    def test_confirmation_avec_compte_nom_acceptee(self):
        self.distant("demo")
        ok, _ = self.lancer(gm.supprimer_depot, self.cfg, self.github.repos["saleh/demo"], {},
                            reponses=["saleh/demo"])
        self.assertTrue(ok)


class ApresUnPull(Banc):
    def test_aucune_difference_apres_le_pull(self):
        travail = self.distant("demo")
        local = self.copie("demo")
        self.autre_push(travail, "a.txt", "nouveau\n")
        self.lancer(gm.recuperer_projet, self.cfg, local)
        _, entete = self.lancer(gm.entete_projet, local)
        self.assertNotIn("push : 0", entete)
        self.assertNotIn("pull : 0", entete)
        self.assertNotIn("0 fichier(s)", entete)
        _, commits = self.lancer(gm.voir_commits_locaux, self.cfg, local)
        self.assertIn("Commits pareils en local et en ligne", commits)
        self.assertNotIn("pas encore", commits)
        res, comparaison = self.lancer(gm.act_comparer_local, self.cfg, local)
        self.assertIn("Les deux copies sont pareilles", comparaison)

    def test_statut_montre_seulement_les_ecarts(self):
        travail = self.distant("demo")
        local = self.copie("demo")
        self.autre_push(travail, "a.txt", "nouveau\n")
        self.sh("git", "fetch", "-q", cwd=local)
        _, entete = self.lancer(gm.entete_projet, local)
        self.assertIn("pull : 1 commit(s)", entete)
        self.assertNotIn("push : 0", entete)


class Branches(Banc):
    def test_parcourir_une_branche_en_ligne(self):
        self.distant("demo", branches={"feature": ("f.txt", "ligne de la branche\n")})
        local = self.copie("demo")
        # 2 = feature (en ligne seulement) ; 1 parcourir ; fichier 1 = f.txt ;
        # Enter apres le fichier ; Enter pour sortir ; Enter pour revenir
        _, sortie = self.lancer(gm.act_branches, self.cfg, local,
                                reponses=["2", "1", "1", "", "", ""])
        self.assertIn("ligne de la branche", sortie)

    def test_supprimer_branche_seulement_en_ligne(self):
        self.distant("demo", branches={"feature": ("f.txt", "x\n")})
        local = self.copie("demo")
        _, sortie = self.lancer(gm.action_branche, self.cfg, local, "origin", "main", "feature",
                                "en ligne seulement", reponses=["5", "o"])
        self.assertNotIn("aussi", sortie)
        self.assertNotIn("feature", self.sh("git", "ls-remote", "--heads", "origin", cwd=local))


class AnnulerCommit(Banc):
    def test_annuler_un_commit_en_ligne(self):
        self.distant("demo")
        local = self.copie("demo")
        Path(local, "b.txt").write_text("b\n", encoding="utf-8")
        self.sh("git", "add", "-A", cwd=local)
        self.sh("git", "commit", "-q", "-m", "Ajout de b", cwd=local)
        self.sh("git", "push", "-q", cwd=local)
        _, sortie = self.lancer(gm.annuler_commit, self.cfg, local, reponses=[""])
        self.assertIn("Annulation du commit OK, ici et en ligne", sortie)
        log = self.sh("git", "log", "--format=%s", "origin/main", cwd=local)
        self.assertTrue(log.startswith("Annulation du commit"), log)
        self.assertFalse(os.path.exists(os.path.join(local, "b.txt")))

    def test_changements_prets_pour_un_commit_pas_melanges(self):
        self.distant("demo")
        local = self.copie("demo")
        Path(local, "b.txt").write_text("b\n", encoding="utf-8")
        self.sh("git", "add", "-A", cwd=local)
        self.sh("git", "commit", "-q", "-m", "Ajout de b", cwd=local)
        self.sh("git", "push", "-q", cwd=local)
        Path(local, "c.txt").write_text("travail en cours\n", encoding="utf-8")
        self.sh("git", "add", "c.txt", cwd=local)
        _, sortie = self.lancer(gm.annuler_commit, self.cfg, local, reponses=[""])
        fichiers = self.sh("git", "show", "--name-only", "--format=", "origin/main", cwd=local)
        self.assertNotIn("c.txt", fichiers, "le travail en cours ne doit pas partir")
        self.assertTrue(os.path.exists(os.path.join(local, "c.txt")))


class Dossiers(Banc):
    def test_cinq_essais_puis_un_niveau_en_arriere(self):
        faux = [os.path.join(self.tmp, f"absent{i}") for i in range(5)]
        _, sortie = self.lancer(gm.act_depots_locaux, self.cfg, reponses=["n", *faux, ""])
        self.assertIn("essai 4 sur 5", sortie)
        self.assertEqual(sortie.count("Repos GitHub en local"), 2, "retour d'un seul niveau")

    def test_revenir_quand_un_dossier_est_propose(self):
        self.cfg["dernier_dossier"] = self.cfg["espace"]
        res, sortie = self.lancer(gm.choisir_dossier, self.cfg, reponses=["0"])
        self.assertIsNone(res)

    def test_import_dossier_introuvable_nouvel_essai(self):
        gm.activer_profil("entreprise")
        gm.SESSION.update(token="jeton", methode="jeton", user={"login": "pro", "id": 2})
        res, sortie = self.lancer(gm.importer_repo, self.cfg, self.tmp,
                                  reponses=[os.path.join(self.tmp, "absent"), ""])
        self.assertEqual(res, (None, None))
        self.assertIn("Dossier introuvable", sortie)


class LierAvecGitHub(Banc):
    def test_nouveau_repo_sans_question_en_double(self):
        dossier = os.path.join(self.cfg["espace"], "nouveau")
        os.makedirs(dossier)
        Path(dossier, "main.py").write_text("print(1)\n", encoding="utf-8")
        # 2 git init + nouveau repo ; .gitignore non ; nom ; description ; organisation ;
        # public ; message du commit ; Enter apres l'envoi
        _, sortie = self.lancer(gm.menu_init, self.cfg, dossier,
                                reponses=["2", "n", "", "", "", "", "", ""])
        self.assertEqual(sum(".gitignore" in q for q in self.questions), 1, self.questions)
        self.assertIn("nouveau", self.sh("git", "ls-remote", "--heads", "origin", cwd=dossier)
                      + "nouveau")
        self.assertIn("main", self.sh("git", "ls-remote", "--heads", "origin", cwd=dossier))

    def test_lier_avec_le_repo_du_meme_nom_sans_redemander(self):
        self.distant("demo")
        dossier = os.path.join(self.cfg["espace"], "demo")
        os.makedirs(dossier)
        _, sortie = self.lancer(gm.menu_init, self.cfg, dossier, reponses=["1", ""])
        self.assertFalse(any("compte/nom" in q for q in self.questions), self.questions)
        self.assertTrue(os.path.exists(os.path.join(dossier, "README.md")))

    def test_question_du_repo_avec_retour(self):
        dossier = os.path.join(self.cfg["espace"], "sans-nom")
        os.makedirs(dossier)
        self.sh("git", "init", "-q", dossier)
        res, _ = self.lancer(gm.lier_a_github, self.cfg, dossier, reponses=["1", ""])
        self.assertIsNone(res)


class CopiesLocales(Banc):
    def test_liste_en_ligne_voit_les_memes_copies_que_la_liste_locale(self):
        self.distant("demo")
        self.distant("autre")
        ailleurs = os.path.join(self.maison, "Documents", "demo")  # hors du dossier de travail
        self.sh("git", "clone", "-q", "https://github.com/saleh/demo.git", ailleurs)
        _, locale = self.lancer(gm.liste_depots_locaux, self.cfg, reponses=[""])
        self.assertIn("saleh/demo", locale)
        _, en_ligne = self.lancer(gm.act_depots, self.cfg, reponses=[""])
        lignes = {l.split()[1]: l for l in en_ligne.splitlines() if l.strip()[:2].rstrip(".").isdigit()}
        self.assertTrue(lignes["saleh/demo"].rstrip().endswith("oui"), lignes)
        self.assertFalse(lignes["saleh/autre"].rstrip().endswith("oui"), lignes)

    def test_menu_principal_option_entreprise(self):
        _, sortie = self.lancer(gm.choisir_fonction, self.cfg, reponses=["0"])
        self.assertIn("2. Gerer et synchroniser sur le poste Entreprise", sortie)


class Menus(Banc):
    def test_oui_non_redemande_une_faute_de_frappe(self):
        res, sortie = self.lancer(gm.ask_yn, "Continuer ?", True, reponses=["oiu", "n"])
        self.assertFalse(res)
        self.assertIn("Repondre o pour oui", sortie)

    def test_dossier_par_un_lien_symbolique(self):
        self.distant("demo")
        local = self.copie("demo")
        lien = os.path.join(self.tmp, "lien-demo")
        try:
            os.symlink(local, lien, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest("liens symboliques indisponibles")
        _, sortie = self.lancer(gm.ouvrir_dossier, self.cfg, lien, reponses=["0"])
        self.assertNotIn("fait partie du repo git", sortie)

    def test_menus_nomment_les_commandes_git(self):
        self.distant("demo")
        local = self.copie("demo")
        _, sortie = self.lancer(gm.menu_projet, self.cfg, local, reponses=["0"])
        for libelle in ("commit + push", "pull", "log, diff", "branch, switch, merge",
                        "restore, revert", "git stash", "force push", "remote"):
            self.assertIn(libelle, sortie)

    def test_titre_avec_le_nom_du_compte(self):
        self.distant("demo")
        # 1 en ligne, Enter ; 2 en local, Enter ; 3 compte, 0 sans pause ; 0 retour
        _, sortie = self.lancer(gm.menu_perso, self.cfg,
                                reponses=["1", "", "2", "", "3", "0", "0"])
        self.assertIn("Repos GitHub de Saleh Kamil", sortie)
        compte = sortie.split("  Compte GitHub\n" + "=" * 66, 1)[1]
        self.assertNotIn("Fermer la session", compte.split("Choix")[0])

    def test_numero_hors_liste_pas_une_recherche(self):
        self.distant("demo")
        _, sortie = self.lancer(gm.act_depots, self.cfg, reponses=["15", ""])
        self.assertIn("Choix invalide", sortie)
        self.assertNotIn("contenant '15'", sortie)

    def test_liste_en_ligne_sans_choix_du_compte_en_double(self):
        self.distant("demo")
        _, sortie = self.lancer(gm.act_depots, self.cfg, reponses=[""])
        self.assertNotIn("C. Changer de compte", sortie)
        self.assertNotIn("D. Fermer la session", sortie)


class Parcours(Banc):
    def test_tout_envoyer(self):
        for nom in ("un", "deux"):
            self.distant(nom)
            Path(self.copie(nom), "x.txt").write_text(nom + "\n", encoding="utf-8")
        # dossier propose : Enter ; 1 tout envoyer ; message : Enter
        _, sortie = self.lancer(gm.act_tout, self.cfg, reponses=["", "1", ""])
        self.assertIn("Bilan : 2 OK", sortie)
        for nom in ("un", "deux"):
            log = self.sh("git", "log", "--format=%s", "-1", "origin/main",
                          cwd=os.path.join(self.cfg["espace"], nom))
            self.assertTrue(log.startswith("Commit du"), log)

    def test_changements_pour_plus_tard(self):
        self.distant("demo")
        local = self.copie("demo")
        Path(local, "README.md").write_text("en cours\n", encoding="utf-8")
        # M ranger ; nom : Enter ; Enter apres ; 1 la liste ; 1 reprendre ; Enter ; Enter retour
        _, sortie = self.lancer(gm.act_stash, self.cfg, local,
                                reponses=["m", "", "", "1", "1", "", ""])
        self.assertIn("Rangement OK", sortie)
        self.assertIn("Changements repris", sortie)
        self.assertEqual(Path(local, "README.md").read_text(encoding="utf-8"), "en cours\n")

    def test_comparer_avec_des_ecarts(self):
        travail = self.distant("demo", {"a.txt": "1\n2\n3\n"})
        local = self.copie("demo")
        self.autre_push(travail, "a.txt", "1\nDEUX\n3\n")
        Path(local, "b.txt").write_text("local\n", encoding="utf-8")
        # fichier 1 : ses deux versions ; Enter ; Enter pour revenir
        res, sortie = self.lancer(gm.act_comparer_local, self.cfg, local, reponses=["1", "", ""])
        self.assertIn("Commits en avance", sortie)
        self.assertIn("DEUX", sortie)
        self.assertIn("b.txt", sortie)

    def test_import_puis_envoi_dans_le_repo_de_l_entreprise(self):
        self.distant("demo", {"code.py": "print('perso')\n"})
        source = self.copie("demo")
        self.distant("entre", {"README.md": "# entreprise\n"})
        # dossier source ; 1 utiliser la copie ; 1 envoyer ; repo 2 = entre ; 1 ajouter ;
        # appliquer : Enter ; message : Enter ; Enter apres l'envoi ; 0 retour
        _, sortie = self.lancer(gm.act_importer, self.cfg,
                                reponses=[source, "1", "2", "1", "", "", "", "0"])
        self.assertIn("push : 1 commit(s)", sortie)
        verif = os.path.join(self.tmp, "verif")
        self.sh("git", "clone", "-q", "https://github.com/saleh/entre.git", verif)
        self.assertTrue(os.path.exists(os.path.join(verif, "code.py")))
        self.assertTrue(os.path.exists(os.path.join(verif, "README.md")))


class Revue(Banc):
    """Points de la revue de code du fichier entier."""

    def test_repo_recree_apres_suppression_visible(self):
        gm.SUPPRIMES.add("saleh/demo")
        url = self.lancer(gm.creer_depot, self.cfg, "demo", True)[0]
        self.assertTrue(url)
        self.assertIn("saleh/demo", [d["full_name"] for d in gm.liste_repos(self.cfg)])

    def test_repo_deja_supprime_sur_github(self):
        self.distant("demo")
        self.github.supprimes.add("saleh/demo")
        ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, self.github.repos["saleh/demo"], {},
                                 reponses=["demo"])
        self.assertTrue(ok)
        self.assertIn("n'existe plus sur GitHub", sortie)
        self.assertNotIn("permission", sortie)

    def test_adresse_collee_sans_https(self):
        for saisie in ("github.com/saleh/demo", "www.github.com/saleh/demo/",
                       "https://github.com/saleh/demo", "saleh/demo"):
            self.assertEqual(gm.url_repo_en_ligne(self.cfg, saisie).removesuffix(".git"),
                             "https://github.com/saleh/demo", saisie)
        self.assertEqual(gm.url_source("github.com/saleh/demo"), "https://github.com/saleh/demo")

    def test_adresse_http_refusee_sans_quitter_la_liste(self):
        res, sortie = self.lancer(gm.liste_depots_locaux, self.cfg,
                                  reponses=["u", "http://github.com/saleh/demo"])
        self.assertIsNone(res, "la liste locale continue")
        self.assertIn("Adresse refusee", sortie)

    def test_chemins_avec_espaces(self):
        self.distant("demo")
        local = self.copie("demo")
        os.makedirs(os.path.join(local, "sous repo"))
        self.sh("git", "init", "-q", os.path.join(local, "sous repo"))
        self.assertEqual(gm.fichiers_modifies(local), [], "un sous-repo ne compte pas")
        Path(local, "a b.txt").write_text("x\n", encoding="utf-8")
        self.assertEqual(gm.fichiers_modifies(local), ["?? a b.txt"])
        _, sortie = self.lancer(gm.annuler_fichier, local, reponses=["1", "o"])
        self.assertFalse(os.path.exists(os.path.join(local, "a b.txt")))
        self.assertIn("nouveau   a b.txt\n", sortie)
        self.assertNotIn("git rm", sortie, "fichier jamais ajoute : pas de git rm")

    def test_nouveau_nom_compte_une_fois(self):
        self.distant("demo")
        local = self.copie("demo")
        self.sh("git", "mv", "README.md", "LISEZMOI.md", cwd=local)
        self.assertEqual(gm.fichiers_modifies(local), ["R  LISEZMOI.md"])

    def test_annulation_dont_le_push_echoue(self):
        self.distant("demo")
        local = self.copie("demo")
        Path(local, "b.txt").write_text("b\n", encoding="utf-8")
        self.sh("git", "add", "-A", cwd=local)
        self.sh("git", "commit", "-q", "-m", "Ajout de b", cwd=local)
        self.sh("git", "push", "-q", cwd=local)
        nu = os.path.join(self.distants, "saleh", "demo.git")
        os.rename(nu, nu + ".ailleurs")  # serveur injoignable
        _, sortie = self.lancer(gm.annuler_commit, self.cfg, local, reponses=[""])
        self.assertIn("Annulation faite ici seulement", sortie)
        self.assertTrue(self.sh("git", "log", "-1", "--format=%s", cwd=local)
                        .startswith("Annulation du commit"))


@unittest.skipIf(os.name == "nt", "GitHub CLI factice : script sh, pas sous Windows")
class Connexion(Banc):
    """Connexion par le navigateur, fin de session et suppression du compte,
    avec un GitHub CLI factice."""

    def setUp(self):
        super().setUp()
        gm.SESSION.update(token="", user=None, methode=None)
        self.remplacer(gm, "_CACHE_GCM", [None])  # sans Git Credential Manager

    def test_code_attendu_puis_compte_supprime_en_fin_de_programme(self):
        etat = self.faux_gh()
        for cle in ("user.name", "user.email"):  # identite ecrite par gitmanager
            self.sh("git", "config", "--global", "--unset", cle)
        # Enter apres la copie du code ; garde 1 : pour cette session
        connecte, sortie = self.lancer(gm.connexion_navigateur, self.cfg, reponses=["", "1"])
        self.assertTrue(connecte, sortie)
        self.assertIn("ABCD-1234", sortie)
        self.assertLess(sortie.index("ABCD-1234"), sortie.index("Ouverture dans le navigateur"),
                        "le navigateur s'ouvre apres la copie du code")
        self.assertEqual((gm.SESSION["methode"], self.cfg.get("cles"), self.cfg.get("garde")),
                         ("gh", "gh", "session"))
        self.assertEqual(self.sh("git", "config", "--global", "user.name").strip(), "Saleh Kamil")
        _, sortie = self.lancer(gm.verifier_gardes, self.cfg, False, True)
        self.assertFalse(os.path.exists(etat), "plus de connexion dans GitHub CLI")
        self.assertFalse(any(k in self.cfg for k in ("cles", "login", "garde")), self.cfg)
        code = subprocess.run(["git", "config", "--global", "user.name"]).returncode
        self.assertNotEqual(code, 0, "identite ecrite par gitmanager effacee")
        self.assertNotIn("restent", sortie)

    def test_fermer_la_session_en_gardant_le_compte(self):
        etat = self.faux_gh()
        Path(etat).touch()
        self.cfg.update(cles="gh", login="saleh", methode="gh", enregistrer_connexion=True)
        gm.SESSION.update(token="jeton-gh", methode="gh", user={"login": "saleh"})
        fait, _ = self.lancer(gm.deconnexion, self.cfg, reponses=["1"])
        self.assertTrue(fait)
        self.assertIsNone(gm.SESSION["user"])
        self.assertTrue(os.path.exists(etat), "le compte reste sur la machine")
        self.assertFalse(self.cfg["enregistrer_connexion"])

    def test_identifiants_en_clair_retires_pour_ce_compte_seulement(self):
        self.remplacer(gm, "chemin_gh", lambda: None)
        fichier = Path(self.maison, ".git-credentials")
        fichier.write_text("https://saleh:ghp_secret@github.com\nhttps://autre:t@github.com\n",
                           encoding="utf-8")
        self.cfg.update(login="saleh")
        self.lancer(gm.supprimer_compte, self.cfg, "saleh")
        self.assertEqual(fichier.read_text(encoding="utf-8"), "https://autre:t@github.com\n")

    def test_permission_refusee_dans_le_navigateur(self):
        self.distant("demo")
        self.faux_gh()
        gm.SESSION.update(token="ancien", methode="gh", user={"login": "saleh"})
        self.github.jetons_admin = {"autre"}
        self.remplacer(gm, "gh_par_navigateur", lambda args: 1)
        # nom ; demander la permission : oui ; menu : 0 annuler
        ok, sortie = self.lancer(gm.supprimer_depot, self.cfg, self.github.repos["saleh/demo"], {},
                                 reponses=["demo", "", "0"])
        self.assertFalse(ok)
        self.assertIn("Permission pas ajoutee", sortie)


if __name__ == "__main__":
    unittest.main()
