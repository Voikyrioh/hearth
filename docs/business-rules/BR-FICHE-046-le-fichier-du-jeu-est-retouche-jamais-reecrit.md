---
id: BR-FICHE-046
domaine: FICHE
titre: Le fichier du jeu est la vérité de ses octets : il est lu et retouché sans perte, jamais rendu par-dessus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-046), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-24
---

# BR-FICHE-046 — Le fichier du jeu est la vérité de ses octets : il est lu et retouché sans perte, jamais rendu par-dessus

## Règle
Deux formats écrits à la main, sans bibliothèque : `kv_lines` et `xml_properties`. Un fichier est découpé en morceaux dont la concaténation redonne les octets d'origine. P1 : `render(parse(octets)) == octets`. P2 : régler une valeur ne change que les octets de cette valeur (une clé nouvelle ajoute une ligne en fin de fichier, avec la fin de ligne dominante). P3 : un fichier que le format ne sait pas découper est refusé ENTIER, jamais réécrit : octet nul, UTF-8 invalide (UTF-16 compris), retour chariot isolé, ligne de plus de 64 Kio, plus de 20 000 lignes, plus de 2 Mio, guillemet non refermé ; XML : DOCTYPE ou ENTITY, encodage autre qu'UTF-8, entité inconnue, racine absente ou non refermée, attribut absent ou en double, balise `property` qui n'est pas auto-fermante. Les bornes sont vérifiées avant toute allocation. Une valeur écrite ne peut pas injecter une seconde ligne ni une seconde propriété (fin de ligne, `"`, `<`, `&`, `]]>`, `-->` : écrits échappés, relus identiques, ou refusés). Une valeur de plus de 16 Kio, ou qui rendrait le fichier illisible, est refusée.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/formats/mod.rs::{ConfigFormat, Doc, Unreadable, Refused}`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::KvLines`
- `crates/hearth-agent/src/domain/catalog/formats/xml_properties.rs::XmlProperties`

- `crates/hearth-agent/src/domain/catalog/settings_io.rs::{apply_edits, plan_save}` (HRT-108 : le plan d'un enregistrement ne retouche que les valeurs demandées)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save` (HRT-108 : lecture par `SafeFs`, 2 Mio au plus, un fichier illisible est refusé entier)
- Client (HRT-109) : `apps/desktop/src/components/organisms/RawFileDialog.vue` (bandeau mot pour mot, case « Je comprends » à chaque ouverture), `apps/desktop/src/components/organisms/GameSettingsTab.vue` (bouton absent et phrase du pied quand `raw_edit` est faux)

## Vérification
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p1_identity_render_of_parse_gives_the_same_bytes (suite de contrat, les deux formats)`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p2_locality_setting_a_value_changes_only_its_bytes`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p3_a_file_the_format_cannot_split_is_refused_whole`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::a_special_value_is_read_back_identical_or_refused_without_writing_anything`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::ten_successive_settings_keep_comments_unknown_lines_commented_lines_and_line_endings`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::tests::identite_octets_d_un_fichier_a_la_terraria`
- `crates/hearth-agent/src/domain/catalog/formats/xml_properties.rs::tests::entities_and_doctype_and_encodings_are_refused_whole`

- `crates/hearth-agent/tests/game_settings_https.rs::an_unreadable_file_is_refused_whole_for_a_save_and_told_by_the_view`
- `crates/hearth-agent/src/domain/catalog/tests_io.rs::an_unreadable_file_is_refused_whole_and_nothing_is_planned`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::le fichier brut, la console et les autres réglages`

## Cas limites
- Un doublon de clé : la DERNIÈRE entrée est lue et retouchée (hypothèse sur le jeu, non vérifiée) ; `duplicate_keys` le signale.
- Aucun troisième format (`json`, `ini_sections`, `cfg_lines`) : ils viennent avec leur premier jeu.

- Un fichier qui retire ou défait un réglage imposé avertit : l'analyse d'un texte collé dit `imposed_undone`, la vue dit `imposed_broken`, aucune écriture ne le corrige (BR-FICHE-048, BR-FICHE-052).
- L'éditeur de fichiers de l'onglet « Fichiers » n'existe pas encore : « Ouvrir le fichier » est présent mais inactif, avec sa phrase (suivi : onglet Fichiers).

## Règles liées
- BR-FICHE-047, BR-FICHE-048, BR-FICHE-052

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-24 — HRT-108 : l'agent lit le fichier du jeu et le retouche à l'enregistrement.
- 2026-10-24 — pointeurs du client (HRT-109).
