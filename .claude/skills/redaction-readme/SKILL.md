---
name: redaction-readme
description: Écrire ou refaire le README d'un projet de Kamil pour une personne qui découvre le projet et veut s'en servir. Structure fixe (titre, accroche, Description, Comment l'utiliser avec une sous-partie courte par option, Sécurité courte), français simple, sans tableau, sans sommaire, sans dépannage ni limites. À lire avant d'écrire ou de réécrire un README.
---

# Rédaction d'un README

Ce skill complète `redaction-simple-claire` : ses règles de style s'appliquent
aussi ici.

## Le lecteur

Le lecteur est une personne qui découvre le projet. Il ne connaît ni le code
ni son histoire. Il veut savoir à quoi sert l'outil et comment s'en servir.

Le README le guide pas à pas. Il n'explique pas le fonctionnement interne et
il n'est pas écrit pour une IA.

## La structure

Le README suit cet ordre, et rien d'autre :

1. Le titre : le nom du projet.
2. L'accroche : la phrase convenue avec Kamil, juste sous le titre. Si Kamil
   a donné une version anglaise, elle vient sur la ligne suivante.
3. `## Description` : la description donnée par Kamil, reformulée
   proprement. Elle dit à quoi sert l'outil, pour qui et dans quelle
   situation.
4. `## Comment l'utiliser` : ce qu'il faut avoir sur son ordinateur, comment
   récupérer le projet, comment le lancer, ce qui se passe au premier
   lancement. Les options de l'outil viennent à la fin de cette partie, en
   sous-parties : `### Option 1 : ...`, `### Option 2 : ...`.
5. Chaque option tient en deux ou trois paragraphes courts : le numéro à
   taper et où l'utiliser, ce que l'outil demande et fait, ce que tu
   obtiens. Seulement l'essentiel : pas de liste d'étapes détaillée.
6. `## Sécurité` : un court paragraphe avec l'essentiel. Pas d'énumération
   de toutes les protections.

Ne pas ajouter de sommaire, de tableau, ni de partie dépannage, limites,
tests, fichiers, licence ou historique.

## Le style

- Français simple, avec des mots courants.
- Une idée par phrase. Sujet, verbe, complément. Voix active.
- S'adresser au lecteur avec « tu », comme dans la description de Kamil.
- Expliquer en quelques mots chaque terme technique à sa première
  apparition, par exemple ce qu'est un jeton d'accès ou une empreinte
  SHA-256.
- Paragraphes courts : deux ou trois phrases.
- Une liste numérotée seulement pour une suite d'étapes.
- Chaque commande dans son propre bloc de code, pour pouvoir la copier.
- Pas de gras dans le texte, pas d'émoji, pas de tiret long, pas
  d'analogie, pas de superlatif.
- Pas d'image si les dossiers d'images sont masqués par le `.gitignore`.

## L'honnêteté

Le README décrit seulement ce que l'outil fait vraiment. Avant d'écrire une
option, vérifier dans le code ce qu'elle fait. Ce qui n'est pas fait n'est
pas annoncé, et rien n'est exagéré.

## La méthode

1. Relire la description et l'accroche données par Kamil. Les reprendre en
   corrigeant les fautes, sans changer le sens.
2. Lire le code pour connaître chaque option, chaque question posée à
   l'utilisateur et ce qui se passe au premier lancement.
3. Écrire en suivant la structure et le style.
4. Relire comme quelqu'un qui découvre le projet : chaque phrase se comprend
   à la première lecture.
5. Vérifier avant de livrer :
   - l'accroche et la description sont présentes ;
   - aucun tableau, aucun sommaire, aucune partie en trop ;
   - chaque phrase a un sujet et un verbe ;
   - aucune promesse que le code ne tient pas.

## Le modèle

```markdown
# Nom du projet

Accroche en français.

Accroche en anglais.

## Description

Deux ou trois phrases : la situation du lecteur, ce que l'outil lui permet.

## Comment l'utiliser

Ce qu'il faut avoir, en une phrase.

1. Récupère le projet :

   (commande)

2. Lance-le :

   (commande)

Ce qui se passe au premier lancement, en deux phrases.

### Option 1 : ...

Le numéro à taper et où l'utiliser. Ce que l'outil demande et fait, ce que
tu obtiens.

### Option 2 : ...

Même chose.

## Sécurité

Un court paragraphe.
```
