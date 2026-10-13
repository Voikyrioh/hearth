---
id: BR-SLOG-013
domaine: SLOG
titre: Le texte d'une ligne est nettoyé et borné par l'agent avant l'envoi : codes de couleur et contrôles retirés, retour chariot seul = fin de ligne, octet invalide remplacé, ligne coupée à 10 000 caractères
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-013 — Le texte d'une ligne est nettoyé et borné par l'agent avant l'envoi : codes de couleur et contrôles retirés, retour chariot seul = fin de ligne, octet invalide remplacé, ligne coupée à 10 000 caractères

## Règle
UNE implémentation, `hearth_proto::log_text` : `clean` retire les séquences d'échappement du terminal (CSI, OSC, jeux de caractères, `U+009B`), les caractères de contrôle (tabulation exceptée : sans elle les sorties en colonnes se collent) et les caractères de formatage Unicode invisibles ou qui renversent l'écriture (la même liste que les textes de conteneur, `limits::bounded`) ; `bound` coupe à 10 000 caractères et annonce le total (`cut`). L'assembleur de lignes (`LineAssembler`) fait d'un retour chariot seul une fin de ligne, remplace un octet invalide par U+FFFD, et ne garde jamais plus de 64 Kio bruts d'une ligne (le reste est compté, jamais gardé). `cut` est le nombre de caractères de la ligne BRUTE (codes d'échappement et caractères retirés compris), à un caractère près au bord de la coupe. Les caractères retirés comprennent les étiquettes U+E0000 à U+E007F (texte invisible qui part avec un copier-coller), U+180E, U+FFF9 à U+FFFB, U+1D173 à U+1D17A et les caractères de remplissage U+115F, U+1160, U+3164, U+FFA0. Les joints U+200C et U+200D sont retirés aussi : un émoji composé se défait et le persan perd ses séparations, choix assumé de la règle (la liste est celle des noms de conteneur). Sur un conteneur à terminal, l'horodatage que le moteur écrit tous les 16 Kio d'une longue ligne est retiré de la même façon qu'au début de ligne. Le marqueur « … (ligne tronquée, {n} caractères au total) » est composé par le client. Écart avec le texte de la règle : la tabulation est conservée (voir l'historique).

## Application (code)
- `crates/hearth-proto/src/log_text.rs::{clean, bound}`
- `crates/hearth-agent/src/domain/services/logs.rs::LineAssembler`
- `crates/hearth-agent/src/domain/services/limits.rs::bounded`

## Vérification
- `crates/hearth-proto/src/log_text.rs::tests (couleurs, titres de fenêtre, contrôles, formatage invisible, coupe)`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_giant_line_is_cut_at_ten_thousand_characters_and_the_rest_is_counted`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::memory_stays_bounded_for_a_line_that_never_ends`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::lines_end_on_lf_crlf_and_a_lone_cr_and_blank_lines_are_kept`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::colour_codes_control_characters_and_bad_bytes_are_cleaned`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_text_is_cleaned_cut_and_counted_before_it_leaves_the_agent`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::tests::a_huge_terminal_line_is_bounded_and_counted_never_buffered`

## Règles liées
- BR-SLOG-012, BR-SVC-021

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — écart assumé avec le texte de la spec : la tabulation est conservée (les contrôles sont retirés, sauf elle) ; sans elle, les sorties en colonnes (Go, `ls -l`) se collent.
- 2026-10-13 — corrections de la review locale de HRT-66 : liste des caractères invisibles complétée ; `cut` compte des caractères ; horodatages de 16 Kio retirés en mode terminal ; ZWJ et ZWNJ retirés (choix assumé).
