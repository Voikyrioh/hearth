---
id: BR-FICHE-052
domaine: FICHE
titre: Importer des réglages : le texte collé est lu par l'agent, jugé réglage par réglage, appliqué seulement sur demande, jamais gardé ni journalisé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-052), HRT-108
maj: 2026-10-24
---

# BR-FICHE-052 — Un texte collé est lu par l'agent puis oublié

## Règle
`POST /services/{key}/settings/analyze` lit le texte collé (2 Mio au plus) avec le format du fichier de réglages du jeu et rend, par réglage de la fiche, un état : `take` (reconnu, sera repris), `same` (déjà la valeur voulue), `rejected` (valeur hors choix, hors bornes, trop longue... avec la raison courte), `imposed` (ignoré), `frozen` (ignoré), plus le nombre de lignes inconnues (gardées dans le fichier, jamais reprises du texte), de clés en double (la dernière est lue) et les réglages imposés que le texte retire ou change (avertissement). Un texte que le format ne sait pas découper est refusé entier (`422 SETTINGS_FILE_UNREADABLE`). **Rien n'est écrit** et la valeur d'un secret collé n'est jamais rendue. Appliquer est un enregistrement ordinaire (origine `import`) : le client renvoie le texte et, pour chaque ligne retenue, le geste `from_import` ; l'agent relit la valeur dans le texte, ce qui évite qu'un secret collé revienne au client. Le texte n'est ni gardé ni journalisé : l'analyse ne s'inscrit pas au journal, l'enregistrement le fait comme tout enregistrement (BR-FICHE-053). Hearth ne lit jamais de lui-même la configuration d'un autre serveur.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_io.rs::{analyze_import, imported_value}`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::{analyze, save}`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_io.rs::analysing_pasted_text_judges_each_setting_without_writing_and_counts_the_unknown`
- `crates/hearth-agent/tests/game_settings_https.rs::analysing_a_pasted_text_writes_nothing_and_judges_each_setting`
- `crates/hearth-agent/tests/game_settings_https.rs::an_import_applies_the_chosen_lines_reads_the_secret_on_the_agent_and_keeps_the_unknown`

## Cas limites
- Une clé du texte inconnue de la fiche n'est pas ajoutée au fichier ; un réglage inconnu déjà dans le fichier y reste (BR-FICHE-047).
- L'effacement du texte dans la mémoire du client est du client (HRT-109).

## Règles liées
- BR-FICHE-046, BR-FICHE-047, BR-FICHE-053

## Historique
- 2026-10-24 — création (HRT-108).
