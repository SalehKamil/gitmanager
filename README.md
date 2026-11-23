<p align="center"><img src=".github/logo.svg" width="96" alt="Logo de GITManager"></p>

<h1 align="center">GITManager</h1>

Ton entreprise n'autorise pas le transfert de fichiers. Tu voudrais pourtant
travailler sur tes projets depuis ton ordinateur perso, plus confortable,
puis les envoyer sur ton ordinateur pro. Ou tu veux simplement gérer tes
dépôts GitHub, sur ta machine comme en ligne.

GITManager est fait pour ça. C'est un petit programme Python qui remplace les
commandes Git par des menus : tu tapes un numéro, il lance les bonnes
commandes à ta place.

## Comment l'utiliser

Tu n'as rien à installer avant : le programme installe lui-même ce qui
manque.

1. Récupère le projet : sur cette page, clique sur Code, puis sur Download
   ZIP, et décompresse le fichier.
2. Lance le programme : sous Windows, double-clique sur `gitmanager.cmd`. Sur
   macOS ou Linux, tape dans un terminal ouvert dans le dossier :

   ```bash
   sh gitmanager.sh
   ```

Au premier lancement, il installe ce qui manque (Python, Git, GitHub CLI)
dans la dernière version disponible, puis affiche l'empreinte SHA-256 de Git
pour que tu puisses la vérifier. Il te demande ensuite : « Que veux-tu
faire ? ».

### Option 1 : gérer et parcourir tes dépôts GitHub (ordinateur perso)

- Envoyer un projet de cet ordinateur : choisis un projet trouvé sur ta
  machine, il part sur GitHub.
- Parcourir mes dépôts GitHub : fichiers, historique, clone, public ou privé,
  suppression.
- Tout mettre à jour : tous les projets d'un dossier envoyés d'un coup.
- Compte GitHub : connexion par le navigateur ou par jeton d'accès,
  déconnexion.

### Option 2 : synchroniser un dépôt sur l'ordinateur de l'entreprise (ordinateur pro)

- Dépôt à récupérer : ton dépôt GitHub, rendu public le temps de la
  synchronisation, par exemple `SalehKamil/mon-projet`.
- Dossier de l'entreprise : le dossier Git du dépôt de l'entreprise, ou son
  adresse pour le cloner.
- Connexion : le compte de l'entreprise, séparé de ton compte perso.
- Confirmation : la liste des fichiers copiés ou supprimés, puis l'envoi.

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
