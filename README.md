# GITManager

Code chez toi, pousse au bureau, et gère tes dépôts GitHub en local comme en
ligne.

Code at home, push at work, and manage your GitHub repos locally and online.

## Description

Ton entreprise n'autorise pas le transfert de fichiers. Tu voudrais pourtant
travailler sur tes projets depuis ton ordinateur perso, plus confortable,
puis les envoyer sur ton ordinateur pro. Ou tu veux simplement gérer tes
dépôts GitHub, sur ta machine comme en ligne.

GITManager est fait pour ça. C'est un petit programme Python qui remplace les
commandes Git par des menus. Tu tapes un numéro, et le programme lance les
bonnes commandes à ta place.

## Comment l'utiliser

Il te faut seulement Python 3.9 ou plus récent. Tu peux aussi installer uv,
un outil qui télécharge Python tout seul.

1. Récupère le projet. Sur la page GitHub du projet, clique sur Code, puis
   sur Download ZIP, et décompresse le fichier. Si Git est déjà installé, tu
   peux aussi le cloner :

   ```bash
   git clone https://github.com/SalehKamil/GITManager.git
   ```

2. Ouvre un terminal dans le dossier du projet et lance le programme :

   ```bash
   uv run gitmanager.py
   ```

   Ou, si tu utilises Python directement :

   ```bash
   python gitmanager.py
   ```

Au premier lancement, le programme vérifie que Git est installé, ainsi qu'un
outil pour te connecter à GitHub par le navigateur, comme GitHub CLI. S'il en
manque un, le programme te propose de l'installer dans sa dernière version
disponible.

Après l'installation de Git, le programme affiche son empreinte SHA-256.
C'est une suite de chiffres et de lettres qui permet de vérifier que le
fichier installé est bien l'original. Ensuite, une fois par semaine, le
programme regarde si une nouvelle version de Git existe et te propose la mise
à jour.

Le programme te demande alors : « Que veux-tu faire ? ». Tape 1 sur ton
ordinateur perso, ou 2 sur l'ordinateur de l'entreprise. Il retient tes
réponses : au lancement suivant, appuie sur Entrée pour garder la réponse
proposée.

## Option 1 : gérer et parcourir tes dépôts GitHub

Utilise cette option sur ton ordinateur perso. Le programme te connecte
d'abord à ton compte GitHub : une page s'ouvre dans ton navigateur pour
valider la connexion. Si ce n'est pas possible, il te demande un jeton
d'accès, c'est-à-dire un code secret que tu crées dans les paramètres de
GitHub.

Le choix « Envoyer un projet de cet ordinateur » cherche les projets Git
présents sur ta machine. Il te les montre du plus récent au plus ancien. Tape
le numéro du projet, puis 1 pour l'envoyer sur GitHub. Si ton projet n'est
pas dans la liste, tape son chemin. S'il n'est pas encore sur GitHub, le
programme crée le dépôt pour toi, en privé par défaut.

Le choix « Parcourir mes dépôts GitHub » affiche la liste de tes dépôts. Pour
chacun, tu peux voir les fichiers et l'historique, le cloner sur ta machine,
le rendre public ou privé, ou le supprimer.

Le menu permet aussi de mettre à jour tous tes projets d'un coup et de gérer
ta connexion à GitHub.

## Option 2 : synchroniser un dépôt sur l'ordinateur de l'entreprise

Utilise cette option sur ton ordinateur pro. Elle copie ton projet depuis
GitHub vers le dépôt de l'entreprise.

Avant de commencer, rends ton dépôt public avec l'option 1, sur ton
ordinateur perso. Le programme peut alors le récupérer sans te connecter à
ton compte perso. Si ton dépôt reste privé, le programme te demande un jeton
d'accès de ton compte perso, seulement pour cette récupération.

Le programme suit ces étapes :

1. Il te demande le nom de ton dépôt, par exemple `SalehKamil/mon-projet`.
2. Il te demande où se trouve le dossier Git du dépôt de l'entreprise sur ton
   ordinateur. S'il n'y est pas encore, donne son adresse et le programme le
   télécharge.
3. Il vérifie ta connexion au serveur de l'entreprise. Si besoin, il te
   demande de te connecter : c'est une deuxième connexion, séparée de ton
   compte perso.
4. Il télécharge ton dépôt dans un dossier temporaire. Il te montre les
   fichiers qui vont être copiés ou supprimés et te demande de confirmer. Le
   dossier de l'entreprise devient alors identique à ton projet, et son
   historique Git est conservé.
5. Il enregistre les changements, les envoie sur le serveur de l'entreprise,
   puis efface le dossier temporaire.

Quand c'est fini, remets ton dépôt en privé depuis ton ordinateur perso.

## Sécurité

Le programme communique seulement par des connexions chiffrées. Il n'écrit
aucun mot de passe ni jeton sur le disque : un jeton reste en mémoire et
disparaît quand le programme se ferme. Avant chaque envoi, il retire les
fichiers secrets, comme les fichiers `.env` et les clés privées. Avant de
rendre un dépôt public, il te prévient s'il y trouve ce genre de fichier.
