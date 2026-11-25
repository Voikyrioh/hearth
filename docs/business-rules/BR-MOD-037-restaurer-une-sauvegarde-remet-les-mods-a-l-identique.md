---
id: BR-MOD-037
domaine: MOD
titre: Restaurer une sauvegarde d'un serveur à mods remet le dossier mods à l'identique, sans laisser d'autres mods
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-24
---

# BR-MOD-037 : Restauration des mods d'un serveur à mods

## Règle
Pour un service dont la fiche déclare `[mods.jars]`, restaurer une sauvegarde remet `/data/mods` À L'IDENTIQUE de la sauvegarde, quel que soit le mode de restauration choisi (BR-BACKUP-018, amendé) :
- un fichier de `/data/mods` absent de la sauvegarde n'est pas supprimé : il est mis dans « Mods retirés » (`/data/mods-retires`), nom libre `-2`, `-3`… comme BR-MOD-024, donc récupérable ;
- les noms `.jar.disabled` reviennent tels quels (état activé de BR-MOD-032) ;
- la fenêtre de restauration (BR-BACKUP-021) montre AVANT la confirmation la comparaison de BR-MOD-036 sous le titre « Les mods du serveur redeviendront ceux de cette sauvegarde », avec ce qui sera mis de côté, ce qui revient, les versions qui changent et les états qui changent ; une case « Je comprends que les mods du serveur vont changer » est à cocher pour activer « Restaurer » ;
- la fenêtre rappelle que les joueurs doivent avoir les mêmes mods et propose « Ce que vos joueurs doivent installer » (BR-MOD-029) ;
- une sauvegarde sans liste (faite avant la 0.2.2) se restaure aussi : la fenêtre dit « Hearth ne connaît pas la liste des mods de cette sauvegarde : les mods redeviendront ceux qu'elle contient. » ;
- après la restauration, l'origine d'un mod suit BR-MOD-033 (correspondance par identité de fichier).

Décidé par Voiky le 2026-11-17 (Q-V5, recommandation acceptée). Pourquoi : avec « Remplacer les fichiers de la sauvegarde », un mod ajouté depuis resterait, et un mod remplacé depuis serait présent deux fois, ce que le jeu refuse au démarrage.

## Application (code)
- Pas encore appliquée : la restauration elle-même n'existe pas dans la 0.2.2 (HRT-99 et HRT-100 au backlog, décision Q-V6 du 2026-11-17). À l'implémentation, le point d'application sera `restore_plan` (HRT-99, prévu). Il réutilisera le déplacement vers `mods-retires` de `crates/hearth-agent/src/domain/mods/apply.rs::plan` (`retire_one`, BR-MOD-024) et la comparaison `crates/hearth-agent/src/domain/mods/backup_list.rs::compare` (BR-MOD-036).
- Dette : tant que HRT-99 n'est pas livrée, aucun code n'applique cette règle.

## Vérification
- À écrire avec HRT-99 : un test de domaine sur `restore_plan` (fichier en trop mis dans `mods-retires`, jamais supprimé ; nom pris → suffixe `-2`) et un test HTTP de restauration en mode « Remplacer ».
- À la main (quand livré) : sur un serveur à mods, ajouter un jar, restaurer une sauvegarde sans ce jar : le jar est dans « Mods retirés », le dossier `mods` égale la liste de la sauvegarde.

## Cas limites
- Mode « Remplacer » ou « Fusionner » choisi → le dossier `mods` est quand même remis à l'identique.
- Un jar de la sauvegarde présent aussi aujourd'hui (même nom) → remplacé par la version de la sauvegarde.
- Sauvegarde sans liste de mods → restauration sans comparaison, message dédié (voir Règle).
- Un jeu sans `[mods.jars]` → cette règle ne s'applique pas.

## Règles liées
- BR-MOD-024 (retirer ne supprime rien), BR-MOD-029 (ce que les joueurs doivent installer), BR-MOD-032 (état activé = nom du fichier), BR-MOD-033 (origine), BR-MOD-036 (comparaison), BR-BACKUP-018 et BR-BACKUP-021 (modes et fenêtre de restauration, amendés par cette règle)

## Historique
- 2026-11-17 : décision Q-V5 de Voiky (dossier `mods` remis à l'identique), écrite dans la spec fonctionnelle des mods.
- 2026-11-24 : création de la fiche (référence de l'ADR-0062, point 10). Règle non encore appliquée (HRT-99 au backlog).
