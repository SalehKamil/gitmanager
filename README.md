<p align="center"><img src=".github/logo.svg" width="96" alt="Logo de gitmanager"></p>

<h1 align="center">gitmanager</h1>

Quand l'entreprise n'autorise pas le transfert de fichiers, gitmanager permet
de travailler sur ses projets depuis l'ordinateur perso, plus confortable,
puis de les envoyer sur l'ordinateur pro. Il sert aussi à gérer ses dépôts
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
même installation avec `--reinstall` le met à jour.

Au premier lancement, le programme installe Git et GitHub CLI s'ils manquent,
dans la dernière version disponible, puis affiche l'empreinte SHA-256 de Git
pour la vérifier. Il affiche ensuite le menu principal : taper 1 pour
l'option 1, ou 2 pour l'option 2.

### Option 1 : gérer les dépôts (ordinateur perso)

La connexion au compte GitHub perso se fait toujours en premier, par le
navigateur ou par jeton d'accès.

1. Gérer les dépôts GitHub : la liste des dépôts en ligne.
   - Parcourir les fichiers et voir les commits.
   - Rendre public ou privé.
   - Cloner sur l'ordinateur ou ouvrir dans le navigateur.
   - Supprimer.
2. Gérer les dépôts locaux : la liste des dépôts trouvés sur l'ordinateur.
   - Envoyer : commit et push.
   - Mettre à jour : pull.
   - Écraser : force push, GitHub prend le contenu du dossier.
   - Voir les modifications, commit seul, push seul, public ou privé,
     relier à un autre dépôt, supprimer le `.git`.
   - Tout mettre à jour : tous les dépôts d'un dossier en une fois.
   - Envoyer un dossier : sans dépôt local, ou pour un nouveau projet ; le
     programme demande le nom, la description et la visibilité du dépôt.
3. Compte GitHub : connexion, déconnexion, changement de serveur.

### Option 2 : synchroniser un dépôt sur l'ordinateur de l'entreprise (ordinateur pro)

Le programme demande d'abord le dépôt à récupérer, par exemple
`SalehKamil/nom-du-projet`, rendu public le temps de la synchronisation. Il
demande ensuite où trouver le dépôt de l'entreprise :

1. Le dossier Git est déjà sur ce PC : indiquer son emplacement.
2. Le cloner depuis le serveur de l'entreprise : indiquer son adresse.

La suite se fait sans chiffre à taper :

- Connexion : compte de l'entreprise, séparé du compte perso.
- Confirmation : liste des fichiers copiés ou supprimés, puis envoi.

## Sécurité

<p>
<img src="https://img.shields.io/badge/Connexions-HTTPS%20ou%20SSH-2ea44f" alt="Connexions : HTTPS ou SSH">
<img src="https://img.shields.io/badge/Certificats-toujours%20v%C3%A9rifi%C3%A9s-2ea44f" alt="Certificats : toujours vérifiés">
<img src="https://img.shields.io/badge/Mots%20de%20passe-jamais%20sur%20le%20disque-2ea44f" alt="Mots de passe : jamais sur le disque">
<img src="https://img.shields.io/badge/Jetons-en%20m%C3%A9moire%20seulement-2ea44f" alt="Jetons : en mémoire seulement">
<img src="https://img.shields.io/badge/Fichiers%20secrets-exclus%20de%20l%27envoi-2ea44f" alt="Fichiers secrets : exclus de l'envoi">
<img src="https://img.shields.io/badge/D%C3%A9p%C3%B4t%20public-alerte%20si%20secrets-2ea44f" alt="Dépôt public : alerte si secrets">
<img src="https://img.shields.io/badge/Git-empreinte%20SHA--256%20affich%C3%A9e-2ea44f" alt="Git : empreinte SHA-256 affichée">
</p>
