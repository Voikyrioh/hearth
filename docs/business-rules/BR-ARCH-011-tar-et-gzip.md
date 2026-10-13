---
id: BR-ARCH-011
domaine: ARCH
titre: Un tar est lu en ustar et pax ; un gzip a un seul membre, sans octet après sa fin
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-011 — Un tar est lu en ustar et pax ; un gzip a un seul membre, sans octet après sa fin

## Règle
Formats `ustar` et `pax` seulement. Un en-tête pax ou GNU de nom long redéfinit le chemin et est jugé comme tout autre nom ; deux noms pour une entrée, deux clés `path`, les attributs creux et `hdrcharset` sont refusés. Une taille négative, mal formée ou qui déborde est un en-tête invalide ; une taille supérieure aux données est un fichier tronqué. La somme de contrôle de l'en-tête est vérifiée, les champs de nom ne cachent rien après leur nul, la fin exige deux blocs nuls puis que des octets nuls, au plus 1 Mio. Un `.tar.gz` a un seul membre : en-tête lu avec des bornes (nom et commentaire de 4 Kio au plus), pied vérifié, un second membre ou des octets après la fin sont refusés, un gzip tronqué est refusé.

## Application (code)
- `crates/hearth-agent/src/domain/archive/tar.rs::TarWalker`
- `crates/hearth-agent/src/domain/archive/gzip.rs::GzipReader`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_pax_header_may_rename_an_entry_but_the_new_name_is_judged_like_any_other`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_gnu_long_name_is_judged_like_any_other_name`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::two_extended_names_for_one_entry_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_negative_or_malformed_size_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_premature_end_is_refused_whatever_the_cut`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::bytes_after_the_end_blocks_are_refused_unless_they_are_zero`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::the_zero_padding_after_the_end_of_a_tar_is_capped`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_gzip_with_several_members_or_trailing_bytes_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_truncated_gzip_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::a_gzip_header_with_reserved_flags_or_an_endless_name_is_refused`

## Cas limites
- Les entrées creuses ne sont pas lues, elles sont refusées.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — remplissage nul de fin plafonné à 1 Mio.
