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

Chaque option demande d'abord la connexion à son compte GitHub : le compte
perso pour l'option 1, le compte de l'entreprise pour l'option 2. Les deux
comptes restent séparés. Sans connexion, le programme revient au menu
principal.

La connexion propose trois moyens, du plus sûr au moins sûr, avec un
avertissement pour les moyens à risque :

- Par code : un code à saisir sur `github.com/login/device`, depuis n'importe
  quel appareil ; la connexion reste en mémoire et vaut 8 heures. Ce moyen
  demande l'application GitHub de gitmanager.
- Par jeton d'accès : la connexion reste en mémoire.
- Par le navigateur : la connexion reste enregistrée sur le poste, à éviter
  sur un poste partagé ou pro.

Une connexion gardée en mémoire s'efface à la fermeture du programme et après
15 minutes sans activité. Quand une action demande une permission que le jeton
n'a pas, le programme la nomme : une fois ajoutée sur GitHub, elle s'applique
au nouvel essai, sans reconnexion.

Dans chaque menu, Entrée revient en arrière d'un niveau ; au menu principal,
Entrée quitte le programme.

### Option 1 : gérer les repos GitHub

Connexion au compte GitHub perso.

1. Repos GitHub en ligne : la liste des repos sur GitHub, avec une
   recherche par nom ; le numéro d'un repo l'ouvre.
   - Parcourir les fichiers et voir les commits.
   - Rendre public ou privé.
   - Ouvrir la copie sur cette machine : le dossier s'ouvre dans le
     gestionnaire de fichiers du système et son menu dans le terminal.
   - Cloner le projet en local, quand aucune copie n'existe.
   - Ouvrir la copie en ligne : la page du repo dans le navigateur.
   - Comparer la copie en ligne et la copie sur cette machine : les commits
     absents de chaque côté, les fichiers différents, puis le détail ligne
     par ligne d'un fichier.
   - Voir les accès à ce repo : le rôle du compte et ses droits.
   - Supprimer.
2. Repos GitHub en local : la liste des repos trouvés sur l'ordinateur.
   - Envoyer : commit et push.
   - Mettre à jour : pull.
   - Écraser : force push, GitHub prend le contenu du dossier.
   - Voir les modifications, rendre public ou privé, relier à un autre
     repo, supprimer le `.git`.
   - Tous les repos d'un dossier : push ou pull de chacun, en une fois.
   - Envoyer un dossier : sans repo local, ou pour un nouveau projet ; le
     programme demande le nom, la description et la visibilité du repo.
3. Compte GitHub : changer de moyen de connexion, se déconnecter, changer de
   serveur, voir les permissions de la connexion.

### Option 2 : entreprise

Connexion au compte GitHub de l'entreprise.

1. Importer un repo GitHub : saisir son URL. Un repo public s'importe sans
   connexion.
   - Repo privé : connexion au compte GitHub perso le temps de l'import.
   - Ouverture : avec accord, le repo privé devient public le temps du clone,
     puis redevient privé après deux minutes, ou tout de suite avec Entrée.
     Le programme signale avant les fichiers secrets qu'il contient.
   - Rapport : forks, étoiles, vues et clones comptés par GitHub pendant
     l'ouverture. GitHub peut mettre plusieurs heures à compter les vues.
   - Envoyer dans un repo GitHub de l'entreprise : numéro d'un repo de la
     liste, ou U pour saisir son URL.
   - Mettre à jour : fichiers ajoutés ou remplacés, aucun supprimé.
   - Écraser : le repo prend exactement le contenu du projet importé,
     historique gardé.
   - Copie sur le Bureau : avant l'envoi, le repo de l'entreprise est copié
     sur le Bureau, sous Windows, macOS ou Linux. La copie reste après
     l'envoi.
2. Gérer les repos GitHub de l'entreprise : les mêmes choix que l'option 1,
   avec le compte de l'entreprise, dont Voir les accès à ce repo.

## Sécurité

<p>
<img src="https://img.shields.io/badge/Connexions-HTTPS%20ou%20SSH-2ea44f" alt="Connexions : HTTPS ou SSH">
<img src="https://img.shields.io/badge/Certificats-toujours%20v%C3%A9rifi%C3%A9s-2ea44f" alt="Certificats : toujours vérifiés">
<img src="https://img.shields.io/badge/Poste%20pro-aucune%20connexion%20enregistr%C3%A9e-2ea44f" alt="Poste pro : aucune connexion enregistrée">
<img src="https://img.shields.io/badge/Inactivit%C3%A9-connexion%20oubli%C3%A9e%20apr%C3%A8s%2015%20min-2ea44f" alt="Inactivité : connexion oubliée après 15 min">
<img src="https://img.shields.io/badge/Identifiants%20en%20clair-alerte-2ea44f" alt="Identifiants en clair : alerte">
<img src="https://img.shields.io/badge/Fichiers%20secrets-exclus%20de%20l%27envoi-2ea44f" alt="Fichiers secrets : exclus de l'envoi">
<img src="https://img.shields.io/badge/Repo%20public-alerte%20si%20secrets-2ea44f" alt="Repo public : alerte si secrets">
<img src="https://img.shields.io/badge/Git-empreinte%20SHA--256%20affich%C3%A9e-2ea44f" alt="Git : empreinte SHA-256 affichée">
</p>
