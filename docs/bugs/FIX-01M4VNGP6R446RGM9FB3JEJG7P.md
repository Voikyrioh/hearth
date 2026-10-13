---
id: FIX-01M4VNGP6R446RGM9FB3JEJG7P
titre: Après un échec d'écriture, la lecture des candidats du même compte était servie sans aucune entrée au journal
date_découverte: 2026-10-13
date_correction: 2026-10-14
---

# FIX-01M4VNGP6R446RGM9FB3JEJG7P : `record_grouped_required` laissait le groupe de répétition ouvert après un échec (HRT-80, revue de la PR 89)

## Symptôme
`GET /services/external` répond `503 BUSY` quand l'entrée admise ne peut pas s'écrire (« pas de trace, pas de réponse »). Mais `RepeatFilter::admit` avait déjà ouvert le groupe : la lecture suivante du même compte, dans la même fenêtre, était « comptée dans un groupe déjà ouvert », n'écrivait rien et était SERVIE, sans aucune entrée (seulement une synthèse écrite au mieux à la fin de la fenêtre). Le message « Réessaie dans un instant » invitait à ce geste.

## Reproduction
`audit_use_cases::a_failed_grouped_write_leaves_no_open_group_and_the_retry_is_refused_or_written` (déclencheur SQL qui refuse l'écriture, puis levé), `repeat::tests::a_withdrawn_event_leaves_no_open_group_and_its_retry_is_written_again`.

## Cause root
`AuditRecorder::record_grouped_required` n'annulait pas l'ouverture du groupe quand l'écriture de l'entrée admise échouait.

## Impacté
`GET /services/external` (BR-ADOPT-005) ; toute autre voie qui utiliserait `record_grouped_required`.

## Workaround
Aucun.

## Correction
`RepeatFilter::withdraw` referme le groupe ouvert par l'événement dont l'écriture a échoué (la famille aussi) et rend, comme synthèse à écrire, ce qui y avait été compté pendant l'écriture ; l'enregistreur l'appelle sur l'échec de l'entrée propre. Le réessai est refusé tant que le journal refuse d'écrire, ou écrit son entrée.

## Références
- Ticket : HRT-80 (revue de la PR 89)
- BR : BR-ADOPT-005, BR-AUDIT-007
