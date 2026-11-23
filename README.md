<p align="center"><img src="docs/logo.svg" width="96" alt="Icône de gitmanager"></p>

<h1 align="center">gitmanager</h1>

<p align="center">
  gitmanager gère les dépôts Git et GitHub par des menus numérotés,
  dans le terminal.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/python-3.9%2B-3776AB?logo=python&logoColor=white" alt="Python 3.9 ou plus"> <img src="https://img.shields.io/badge/d%C3%A9pendances-aucune-2ea44f" alt="Aucune dépendance"> <img src="https://img.shields.io/badge/licence-MIT-blue" alt="Licence MIT">
</p>

<p align="center"><img src="docs/apercu.png" width="720" alt="Les deux premiers menus de gitmanager dans un terminal"></p>

`gitmanager.py` est un script Python qui remplace les commandes Git par des
menus. Il sert à envoyer et récupérer ses projets, et à gérer ses dépôts
GitHub sans retenir de commande.

Le script tient en un seul fichier et n'utilise que la bibliothèque standard
de Python. Il se lance avec `uv run gitmanager.py` ou `python gitmanager.py`.

La connexion à GitHub passe par le navigateur, avec GitHub CLI ou Git
Credential Manager, si bien qu'aucun jeton n'est à créer ni à coller. Au
démarrage, le script demande sur quel ordinateur il tourne, perso ou
entreprise, et chaque choix ouvre ses propres menus.

## Prérequis

- Python 3.9 ou plus, ou bien `uv`, qui fournit l'interpréteur ;
- Git ;
- pour le mode perso, GitHub CLI (`gh`) ou Git Credential Manager, qui est
  inclus dans Git pour Windows.

```bash
winget install --id Git.Git -e    # Windows : Git et Git Credential Manager
brew install git gh               # macOS
sudo apt install git gh           # Debian, Ubuntu
```

## Installation et lancement

1. Récupérer le script :

   ```bash
   git clone https://github.com/SalehKamil/gitmanager.git
   cd gitmanager
   ```

2. Le lancer :

   ```bash
   uv run gitmanager.py    # uv fournit Python si besoin
   python gitmanager.py    # ou avec le Python déjà installé
   ```

3. Choisir l'ordinateur, `1` pour perso ou `2` pour entreprise. Le choix est
   proposé par défaut au lancement suivant.

Le script garde ses réglages dans `~/.gitmanager.json` : le mode, les
derniers dossiers ouverts, le serveur et le login. Ce fichier ne contient
aucun mot de passe ni jeton.

## Utilisation

Chaque menu attend un numéro suivi d'Entrée, et une réponse vide garde la
valeur proposée entre crochets. Le tableau donne, depuis le premier écran, la
suite de réponses pour les tâches courantes.

| Pour | Réponses à taper | Ce qui se passe |
|---|---|---|
| publier un dossier sur GitHub | `1`, `2`, le chemin du dossier, `1` | `git init`, création du dépôt GitHub (privé par défaut), premier commit et push |
| envoyer ses modifications | `1`, `2`, le chemin du projet, `4` | commit des fichiers modifiés, puis push |
| récupérer les changements | `1`, `2`, le chemin du projet, `5` | pull des commits présents sur GitHub |
| mettre à jour tous ses projets | `1`, `3`, le dossier qui les contient, `1` | commit et push de chaque projet Git trouvé, puis un bilan |
| rendre un dépôt public ou privé | `1`, `1`, le numéro du dépôt, `3` | changement de visibilité, après confirmation |
| cloner un de ses dépôts | `1`, `1`, le numéro du dépôt, `4` | clone dans le dossier choisi, puis ouverture du projet |
| envoyer un projet au dépôt de l'entreprise | `2`, `1`, le dossier du projet, le dossier du clone interne | copie des fichiers, commit et push |

Le menu <ins>Mes dépôts GitHub</ins> sert aussi à parcourir les fichiers d'un
dépôt, à lire ses commits et à le supprimer. Le menu 4 gère la connexion.

Le mode entreprise envoie un projet vers un dépôt interne déjà cloné sur le
poste. Il copie le projet dans ce clone, sans `gitmanager.py` ni `.git`, puis
fait commit et push avec les identifiants Git du poste, sans passer par
GitHub. Les dossiers `.venv`, `node_modules`, `__pycache__` et les caches ne
sont jamais copiés.

## Garde-fous

<ins>Secrets et gros fichiers.</ins> Avant chaque commit, le script repère les
fichiers `.env`, les clés privées (`id_rsa`, `.pem`, `.key`, `.p12`),
`credentials.json` et les fichiers de plus de 100 Mo, puis les retire de
l'envoi. En mode interactif, il demande confirmation et ajoute les fichiers
retirés au `.gitignore`.

<ins>Mot à taper.</ins> Les actions qu'on ne peut pas annuler demandent de
taper un mot : `ECRASER` pour le force push, `SUPPRIMER` pour effacer le
`.git`, le nom du dépôt pour le supprimer de GitHub.

<ins>Conflits.</ins> Si le serveur a des commits plus récents, le script les
intègre avant d'envoyer. En cas de conflit, il propose d'annuler : rien n'est
envoyé et le travail local reste intact.

## Limites

À connaître avant de s'en servir.

<ins>Une branche à la fois.</ins> Le script travaille sur la branche courante.
Il ne crée pas de branche et ne fusionne pas de branches entre elles.

<ins>Le `.gitignore` en mode entreprise.</ins> Si le projet copié n'a pas de
`.gitignore`, celui que le script crée dans le clone apparaît au passage
suivant parmi les fichiers présents seulement dans la destination. Ajouter un
`.gitignore` au projet évite la question.

## Dépannage

<ins>« git est introuvable dans le PATH »</ins> : installer Git avec la commande
que le script affiche, puis rouvrir le terminal.

<ins>« Aucun outil de connexion GitHub trouve »</ins> : ni `gh` ni Git
Credential Manager n'est présent. Accepter l'installation proposée, ou
installer `gh` avec le gestionnaire de paquets, puis relancer.

<ins>« GitHub refuse l'acces »</ins> : la connexion a expiré ou a été
révoquée. Se reconnecter avec le menu 4.

<ins>« cle SSH refusee »</ins> : le projet utilise une adresse SSH sans clé
configurée. Accepter le passage en HTTPS que propose le script.

## Ce qui a été vérifié

Le script a été lancé sous Linux avec Python 3.13 et Git 2.43, directement et
par `uv run`. Le mode entreprise a été testé avec un dépôt local comme
serveur, sur deux envois successifs : le second contenait un fichier modifié
et un fichier supprimé, et le `.env` du projet n'a pas été envoyé. Cette
vérification ne couvre pas la connexion à GitHub, ni Windows et macOS.

## Fichiers

| Fichier | Rôle |
|---|---|
| `gitmanager.py` | le script complet |
| `docs/` | l'icône et la capture du terminal |
| `LICENSE` | la licence MIT |
