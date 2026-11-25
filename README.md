<p align="center"><img src=".github/logo.svg" width="96" alt="Logo de gitmanager"></p>

<h1 align="center">gitmanager</h1>

Quand l'entreprise n'autorise pas le transfert de fichiers, gitmanager permet
de travailler sur ses projets depuis l'ordinateur perso, plus confortable,
puis de les envoyer sur l'ordinateur pro. Il sert aussi à gérer ses repos
GitHub, en local comme en ligne.

C'est un petit programme Python qui remplace les commandes Git par des menus :
un numéro suffit pour lancer les bonnes commandes.

## Comment l'utiliser

Le programme se lance avec uv, qui fournit Python :

```bash
uv run gitmanager.py
```

Pour le retrouver sur n'importe quel poste, l'installer comme commande :

```bash
uv tool install https://github.com/SalehKamil/gitmanager/archive/refs/heads/main.zip
```

Il se lance ensuite en tapant `gitmanager`, depuis n'importe quel dossier. La
même installation avec `--reinstall` le met à jour, et `gitmanager --version`
affiche la version installée. Pour le désinstaller :

```bash
uv tool uninstall gitmanager
```

Au lancement, le programme affiche son logo et son accroche. Il installe Git
s'il manque, dans la dernière version disponible, et affiche son empreinte
SHA-256 pour la vérifier.

Un compte gardé sur la machine ouvre sa session tout seul au démarrage.
Sinon, chaque option demande d'abord la connexion à son compte GitHub : le
compte perso pour l'option 1, le compte de l'entreprise pour l'option 2. Les
deux comptes restent séparés. Sans connexion, le programme revient au menu
principal.

La connexion propose deux moyens :

1. Se connecter sur le navigateur : le programme affiche un code à
   copier, puis ouvre le navigateur après Enter. Le compte reste sur la
   machine pour un temps au choix, cette session, 8 heures, 7 jours, 30
   jours ou toujours. À la fin du temps, le compte est supprimé. Avec
   GitHub CLI 2.103 ou plus, le jeton expire au bout de quelques heures et
   GitHub CLI le renouvelle seul.
2. Saisir un jeton : la connexion reste dans la session, jamais sur le
   disque.

Une connexion par jeton s'efface à la fermeture du programme et après 15
minutes sans action. Quand une action demande une permission que le jeton n'a
pas, le programme la nomme : une fois ajoutée sur GitHub, elle compte au
nouvel essai, sans nouvelle connexion.

Le menu principal propose aussi, sous les deux options :

3. Se connecter : au compte perso ou à celui de l'entreprise, pour un compte
   sans session.

Les identifiants gardés sur la machine se gèrent seulement dans les menus des
repos GitHub, perso ou de l'entreprise, avec une session ouverte.

Chaque commande Git qui change un repo s'affiche, l'action Git en rouge,
avec son résultat en français : `git add`, `git commit`, `git push`,
`git pull` et les autres. Tout
le programme s'affiche en français : les messages en anglais de Git, de GitHub
CLI, des installateurs et de GitHub sont traduits ou cachés.

Dans chaque menu, Enter revient en arrière d'un niveau ; au menu principal,
Enter quitte le programme.

### Option 1 : repos GitHub perso

Connexion au compte GitHub perso. Le titre du menu porte le nom du compte
connecté, par exemple Repos GitHub de Saleh Kamil.

1. Repos GitHub en ligne : la liste des repos sur GitHub, avec une
   recherche par nom ; le numéro d'un repo l'ouvre.
   - Parcourir les fichiers : d'abord le choix de la branche.
   - Voir les commits : d'abord le choix de la branche.
   - Rendre public ou non public.
   - Ouvrir la copie sur cette machine : le dossier s'ouvre dans le
     gestionnaire de fichiers du système et son menu dans le terminal.
   - Cloner le projet en local, quand aucune copie n'existe. Si le dossier
     existe déjà, le programme propose de le lier avec le repo, puis pull.
   - Ouvrir la copie en ligne : la page du repo dans le navigateur.
   - Comparer en ligne et en local : seulement les différences, avec la
     même branche en ligne ; puis les fichiers avec changements, la version
     la plus neuve en vert ; puis les deux versions d'un fichier, côte à
     côte. Sans différence, une seule ligne le dit.
   - Voir les droits sur ce repo : le rôle du compte et ses droits.
   - Supprimer de GitHub.
   - U : cloner un repo par son URL ou compte/nom, même hors de la liste.
   - C : changer de compte.
   - D : fermer la session, en gardant ou en supprimant le compte.
2. Repos GitHub en local : la liste des repos trouvés sur la machine ; le
   numéro d'un repo ouvre son menu, avec tous ses choix sur un seul écran.
   - 1 Envoyer : commit + push ; le programme demande le message du commit,
     Enter garde le message proposé.
   - 2 Recevoir : pull.
   - 3 Voir les changements : voir les commits, ou voir les différences en
     local et en ligne. Les commits en avance d'un côté s'affichent
     seulement s'il y en a.
   - 4 Branches : parcourir les fichiers d'une branche, voir ses commits,
     passer dessus, en faire une nouvelle, la fusionner dans la branche
     actuelle, la supprimer ici ou en ligne.
   - 5 Annuler un commit ou des changements : les changements d'un
     fichier, ou le dernier commit, ici et en ligne en une seule action.
   - 6 Changements pour plus tard : `git stash` range les changements pas
     encore en commit, pour les reprendre plus tard ou les supprimer.
   - 7 Ouvrir la copie sur cette machine : le dossier dans le gestionnaire
     de fichiers du système.
   - 8 Ouvrir la copie en ligne : la page du repo dans le navigateur.
   - 9 Remplacer GitHub par ce dossier : force push.
   - 10 Rendre public ou non public.
   - 11 Lier avec un autre repo GitHub.
   - 12 Supprimer le `.git` de ce dossier.
   - N : envoyer un dossier sans repo local, ou un nouveau projet ; le
     programme demande le nom, la description et si le repo est public.
   - T : tous les repos d'un dossier, push ou pull de chacun en une fois.
   - Dossier introuvable : le programme le redemande, cinq essais en tout,
     puis revient d'un niveau.
   - Dossier sans git, ou au `.git` illisible : le programme propose de le
     lier avec un repo GitHub en ligne, celui du même nom s'il existe, puis
     de faire un pull ; ou de créer un nouveau repo GitHub. Les fichiers du
     dossier qui ne sont pas comme la version en ligne partent dans
     `git stash`, ou restent, au choix.
   - Repo git dans un autre repo : avant le commit, le programme le signale
     et propose de le laisser hors du commit, dans le `.gitignore`. Sans
     ce choix, GitHub montre un dossier vide à sa place. La comparaison en
     ligne et en local le laisse de côté.

### Option 2 : entreprise

Connexion au compte GitHub de l'entreprise.

1. Importer un repo GitHub : saisir son URL, compte/nom, ou un dossier de la
   machine. Un repo public s'importe sans connexion.
   - Copie du repo sur la machine : le programme la trouve et propose de
     l'utiliser, sans connexion au compte perso ; seul son dernier commit
     part, les fichiers au commit manquant restent.
   - Repo non public, sans copie sur la machine : connexion au compte GitHub
     perso le temps de l'import.
   - Ouverture : avec accord, le repo devient public le temps du clone, puis
     redevient non public au bout de deux minutes, ou tout de suite avec
     Enter. Le programme signale avant les fichiers secrets qu'il contient.
   - Rapport : forks, stars, vues et clones comptés par GitHub pendant
     l'ouverture. GitHub peut mettre plusieurs heures à compter les vues.
   - Envoyer dans un repo GitHub de l'entreprise : numéro d'un repo de la
     liste, ou U pour saisir son URL.
   - Ajouter au repo, sans rien enlever.
   - Remplacer le repo par le projet : historique gardé.
   - Copie du repo de l'entreprise : si elle existe déjà sur la machine, le
     programme propose de l'utiliser ; sinon, il la fait sur le Bureau, sous
     Windows, macOS ou Linux. La copie reste après l'envoi.
2. Repos GitHub de l'entreprise : les mêmes choix que l'option 1, avec le
   compte de l'entreprise.

## Sécurité

<p>
<img src="https://img.shields.io/badge/Connexions-HTTPS%20ou%20SSH-2ea44f" alt="Connexions : HTTPS ou SSH">
<img src="https://img.shields.io/badge/Certificats-toujours%20v%C3%A9rifi%C3%A9s-2ea44f" alt="Certificats : toujours vérifiés">
<img src="https://img.shields.io/badge/Poste%20pro-aucune%20connexion%20enregistr%C3%A9e-2ea44f" alt="Poste pro : aucune connexion enregistrée">
<img src="https://img.shields.io/badge/Inactivit%C3%A9-connexion%20oubli%C3%A9e%20apr%C3%A8s%2015%20min-2ea44f" alt="Inactivité : connexion oubliée après 15 min">
<img src="https://img.shields.io/badge/Identifiants%20en%20clair-alerte-2ea44f" alt="Identifiants en clair : alerte">
<img src="https://img.shields.io/badge/Compte%20supprim%C3%A9-tout%20effac%C3%A9-2ea44f" alt="Compte supprimé : tout effacé">
<img src="https://img.shields.io/badge/Temps%20de%20garde%20du%20compte-au%20choix-2ea44f" alt="Temps de garde du compte : au choix">
<img src="https://img.shields.io/badge/Jeton%20GitHub%20CLI-expire%20et%20se%20renouvelle-2ea44f" alt="Jeton GitHub CLI : expire et se renouvelle">
<img src="https://img.shields.io/badge/Fichiers%20secrets-exclus%20de%20l%27envoi-2ea44f" alt="Fichiers secrets : exclus de l'envoi">
<img src="https://img.shields.io/badge/Repo%20public-alerte%20si%20secrets-2ea44f" alt="Repo public : alerte si secrets">
<img src="https://img.shields.io/badge/Git-empreinte%20SHA--256%20affich%C3%A9e-2ea44f" alt="Git : empreinte SHA-256 affichée">
</p>
