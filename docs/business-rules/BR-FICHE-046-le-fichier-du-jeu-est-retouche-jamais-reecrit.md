---
id: BR-FICHE-046
domaine: FICHE
titre: Le fichier du jeu est la vérité de ses octets : il est lu et retouché sans perte, jamais rendu par-dessus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-046), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-046 — Le fichier du jeu est la vérité de ses octets : il est lu et retouché sans perte, jamais rendu par-dessus

## Règle
Deux formats écrits à la main, sans bibliothèque : `kv_lines` et `xml_properties`. Un fichier est découpé en morceaux dont la concaténation redonne les octets d'origine. P1 : `render(parse(octets)) == octets`. P2 : régler une valeur ne change que les octets de cette valeur (une clé nouvelle ajoute une ligne en fin de fichier, avec la fin de ligne dominante). P3 : un fichier que le format ne sait pas découper est refusé ENTIER, jamais réécrit : octet nul, UTF-8 invalide (UTF-16 compris), retour chariot isolé, ligne de plus de 64 Kio, plus de 20 000 lignes, plus de 2 Mio, guillemet non refermé ; XML : DOCTYPE ou ENTITY, encodage autre qu'UTF-8, entité inconnue, racine absente ou non refermée, attribut absent ou en double, balise `property` qui n'est pas auto-fermante. Les bornes sont vérifiées avant toute allocation. Une valeur écrite ne peut pas injecter une seconde ligne ni une seconde propriété (fin de ligne, `"`, `<`, `&`, `]]>`, `-->` : écrits échappés, relus identiques, ou refusés). Une valeur de plus de 16 Kio, ou qui rendrait le fichier illisible, est refusée.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/formats/mod.rs::{ConfigFormat, Doc, Unreadable, Refused}`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::KvLines`
- `crates/hearth-agent/src/domain/catalog/formats/xml_properties.rs::XmlProperties`

## Vérification
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p1_identity_render_of_parse_gives_the_same_bytes (suite de contrat, les deux formats)`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p2_locality_setting_a_value_changes_only_its_bytes`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::p3_a_file_the_format_cannot_split_is_refused_whole`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::a_special_value_is_read_back_identical_or_refused_without_writing_anything`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::ten_successive_settings_keep_comments_unknown_lines_commented_lines_and_line_endings`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::tests::identite_octets_d_un_fichier_a_la_terraria`
- `crates/hearth-agent/src/domain/catalog/formats/xml_properties.rs::tests::entities_and_doctype_and_encodings_are_refused_whole`

## Cas limites
- Un doublon de clé : la DERNIÈRE entrée est lue et retouchée (hypothèse sur le jeu, non vérifiée) ; `duplicate_keys` le signale.
- Aucun troisième format (`json`, `ini_sections`, `cfg_lines`) : ils viennent avec leur premier jeu.

## Règles liées
- BR-FICHE-047, BR-FICHE-048

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
