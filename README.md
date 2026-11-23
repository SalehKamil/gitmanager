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

Tu n'as rien à installer avant : le programme installe lui-même ce qui
manque.

1. Récupère le projet. Sur la page GitHub du projet, clique sur Code, puis
   sur Download ZIP, et décompresse le fichier. Si Git est déjà installé, tu
   peux aussi le cloner :

   ```bash
   git clone https://github.com/SalehKamil/GITManager.git
   ```

2. Lance le programme. Sous Windows, double-clique sur `gitmanager.cmd`. Sur
   macOS ou Linux, ouvre un terminal dans le dossier du projet et tape :

   ```bash
   sh gitmanager.sh
   ```

Au premier lancement, le lanceur installe Python s'il manque. Le programme
vérifie ensuite que Git est installé, ainsi qu'un outil pour te connecter à
GitHub par le navigateur, comme GitHub CLI. S'il en manque un, il te propose
de l'installer dans sa dernière version disponible.

Après l'installation de Git, le programme affiche son empreinte SHA-256.
C'est une suite de chiffres et de lettres qui permet de vérifier que le
fichier installé est bien l'original. Ensuite, une fois par semaine, le
programme regarde si une nouvelle version de Git existe et te propose la mise
à jour.

Le programme te demande ensuite : « Que veux-tu faire ? ». Il retient tes
réponses : au lancement suivant, appuie sur Entrée pour garder la réponse
proposée.

### Option 1 : gérer et parcourir tes dépôts GitHub

Tape 1 sur ton ordinateur perso. Le programme te connecte à ton compte GitHub
par le navigateur.

Pour envoyer un projet sur GitHub, choisis « Envoyer un projet de cet
ordinateur ». Le programme te montre les projets trouvés sur ta machine. Tape
le numéro du projet, puis 1.

Pour voir tes dépôts en ligne, choisis « Parcourir mes dépôts GitHub ». Tu
peux voir leurs fichiers, les cloner, les rendre publics ou privés, ou les
supprimer.

### Option 2 : synchroniser un dépôt sur l'ordinateur de l'entreprise

Tape 2 sur ton ordinateur pro. Avant, rends ton dépôt public depuis ton
ordinateur perso, avec l'option 1.

Le programme te demande le nom de ton dépôt, puis l'emplacement du dossier
Git de l'entreprise. Si besoin, il te connecte au compte de l'entreprise. Il
te montre les fichiers qui vont être copiés ou supprimés et te demande de
confirmer. Ensuite, il envoie ton projet dans le dépôt de l'entreprise.

Quand c'est fini, remets ton dépôt en privé.

## Sécurité

Le programme communique seulement par des connexions chiffrées. Il n'écrit
aucun mot de passe sur le disque. Si tu te connectes avec un jeton d'accès,
c'est-à-dire un code secret créé sur GitHub, il reste en mémoire et disparaît
quand le programme se ferme. Avant chaque envoi, il retire les
fichiers secrets, comme les fichiers `.env` et les clés privées. Avant de
rendre un dépôt public, il te prévient s'il y trouve ce genre de fichier.
