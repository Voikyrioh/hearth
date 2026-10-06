---
id: ADR-0026
titre: Case « Lancer Hearth au démarrage de Windows » dans l'installateur : rappels SHOW et LEAVE de la page d'accueil, via les crochets du modèle de Tauri
type: architecture
statut: acceptée
date: 2026-10-07
portee: projet
remplace: —
liens: [ADR-0002, ADR-0010, ADR-0017, BR-CLIENT-006, BR-CLIENT-007, BR-CLIENT-008, BR-CLIENT-010, HRT-21]
---

# ADR-0026 : Case de démarrage dans l'installateur NSIS

## Contexte

La spec veut que l'installateur propose le lancement au démarrage de Windows, décoché par défaut (BR-CLIENT-006). Le réglage existe déjà dans l'application : le greffon `tauri-plugin-autostart` écrit la valeur `Hearth` de `HKCU\...\Run` (argument `--minimized`) et la relit ; c'est la seule source de vérité. L'installateur NSIS est celui du modèle de Tauri (`installerHooks` = `installer/hooks.nsh`). Contraintes : l'installation silencieuse (`/S`) garde le défaut ; la mise à jour automatique lance l'installateur en passif (`/UPDATE /P`, ADR-0017) et ne doit jamais écraser le choix fait dans l'application (BR-CLIENT-008).

## Décision

1. **La case est ajoutée à la page d'accueil du modèle** par les rappels `MUI_PAGE_CUSTOMFUNCTION_SHOW` et `_LEAVE` définis dans `hooks.nsh` (le fichier est inclus avant les pages : une `Page custom` s'y placerait avant l'accueil, et le modèle ne donne pas d'autre crochet de page). C'est le geste que le modèle fait lui-même pour la case « Tout effacer » de la désinstallation. Le texte d'accueil est redéfini (`MUI_WELCOMEPAGE_TEXT`, tutoiement, court) pour laisser la place sous lui.
2. **Aucun stockage nouveau** : l'installateur écrit (case cochée) ou retire (décochée) la valeur `Run` que lit le réglage de l'application, dans le crochet `NSIS_HOOK_POSTINSTALL`, seulement si la page a été franchie (`LEAVE` le consigne). Même valeur que le greffon (nom du produit, `--minimized`), avec le chemin entre guillemets, et la valeur « activé » du Gestionnaire des tâches comme le greffon.
3. **Silencieux et mise à jour : aucune écriture.** `/S` n'affiche aucune page ; le mode passif saute l'accueil (rappel PRE du modèle, que nous ne redéfinissons pas). La variable « page franchie » reste vide et le crochet ne fait rien : défaut désactivé sur une installation neuve, choix de l'utilisateur intact sur une mise à jour.
4. **Réinstallation à la main** : la case est cochée si la valeur existe (lue à l'ouverture de l'interface, avant la page de réinstallation qui peut désinstaller l'ancienne version et sa valeur) ; l'utilisateur confirme ou change.
5. **Désinstallation** : inchangée, le modèle retire la valeur hors mise à jour (BR-CLIENT-010).

## Alternatives écartées

- **Gabarit complet** (`bundle.windows.nsis.template`) : permet une vraie page, mais recopie ~1000 lignes du modèle, à rebaser à chaque version de l'outil (corrections de sécurité de l'installateur comprises). Trop intrusif pour une case.
- **Case sur la page de fin** : l'installation est déjà faite, et la page est sautée en silencieux ; le modèle y occupe déjà ses deux cases.
- **Option de ligne de commande** (`/AUTOSTART`) : pas demandée ; ajoutable plus tard sans changer cette décision.
- **Second réglage lu par l'application** (fichier, clé propre) : une deuxième source de vérité, rejetée (BR-CLIENT-006).

## Conséquences

- Le script dépend du modèle : si une version de Tauri définit elle-même un rappel SHOW/LEAVE ou `MUI_WELCOMEPAGE_TEXT` avant l'accueil, la compilation NSIS échoue (redéfinition) ; le job `desktop` de la CI construit l'installateur à chaque PR, l'échec est donc immédiat.
- Le nom de valeur est répété dans `hooks.nsh` (`PRODUCTNAME` n'est défini qu'après son inclusion) ; un garde de compilation et un test (`tests/installer.rs`) le comparent au nom du produit.
- Limite : ni l'exécution de l'installateur ni le rendu de la case ne sont vérifiés en test (runbook `publier-une-version-du-client`, « Premier essai », point 8).
