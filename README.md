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
SHA-256 pour la vérifier. Il demande ensuite la connexion au compte GitHub :
sans connexion, il s'arrête.

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

Le menu principal propose ensuite deux options. Dans chaque menu, Entrée
revient en arrière d'un niveau ; au menu principal, Entrée quitte le
programme.

### Option 1 : gérer les repos

À utiliser sur l'ordinateur perso.

1. Gérer les repos GitHub : la liste des repos en ligne.
   - Parcourir les fichiers et voir les commits.
   - Rendre public ou privé.
   - Ouvrir la copie : le dossier s'ouvre dans le gestionnaire de fichiers
     du système et son menu dans le terminal.
   - Cloner le projet en local, quand aucune copie n'existe.
   - Ouvrir dans le navigateur.
   - Supprimer.
2. Gérer les repos locaux : la liste des repos trouvés sur l'ordinateur.
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

### Option 2 : synchroniser un repo sur l'ordinateur de l'entreprise

À utiliser sur l'ordinateur pro. Le programme demande d'abord le repo à
récupérer, par exemple `SalehKamil/nom-du-projet`. Un repo privé est
récupéré avec le compte GitHub connecté. Il demande ensuite où trouver le
repo de l'entreprise :

1. Le dossier Git est déjà sur ce PC : indiquer son emplacement.
2. Le cloner depuis le serveur de l'entreprise : indiquer son adresse.

La suite se fait sans chiffre à taper :

- Connexion : compte de l'entreprise, séparé du compte perso.
- Confirmation : liste des fichiers copiés ou supprimés, puis envoi.

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
