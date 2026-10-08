---
id: FIX-01M4EPX88BTXFX1PX7E57WGK38
titre: Un point de montage ou un nom de sonde long sortait de sa carte, un nom court s'écrivait en colonne (S1b)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EPX88BTXFX1PX7E57WGK38 : Un point de montage ou un nom de sonde long sortait de sa carte, un nom court s'écrivait en colonne (S1b)

## Symptôme
Un volume Docker (`overlay` monté sur un chemin de plus de 100 caractères) sortait de la carte Disques, son nom s'écrivait une lettre par ligne, le pourcentage passait sur deux lignes ; même chose dans la carte Machine.

## Reproduction
`e2e/hrt47.spec.ts` aux 5 tailles : le pont simulé reçoit un disque `overlay` au chemin très long et une sonde au nom très long ; rien ne dépasse de sa carte, le nom et le pourcentage tiennent sur une ligne ; le chemin est tronqué au milieu et complet au clavier. Rouge avant sur les 6 tests.

## Cause root
Texte brut sans borne dans une rangée flexible (`overflow-wrap: anywhere` sur le nom, aucun `min-width: 0`).

## Impacté
Le client, vu par Voiky lors de son premier smoke sur la forge (HRT-47).

## Workaround
Aucun.

## Correction
Nouvel atome `HMiddleText` : début et fin visibles, milieu en « … », texte complet en `aria-label` et en infobulle au survol et au focus (un arrêt de tabulation seulement quand le texte est tronqué). Utilisé pour le nom et le point de montage des disques, la carte Machine et le libellé des sondes (`StatRow`). Le cas est ajouté au pont simulé en option (`setLongSensor`, `machineOf`) pour ne pas changer les autres tests. `// FIX:01M4EPX88BTXFX1PX7E57WGK38`.

## Règles
- Aucune règle métier modifiée (la liste des disques côté agent est S1a, autre tâche).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-47 ; captures `contexts/hearth/art/smoke-2026-10-08/`
- Code : `components/atoms/HMiddleText.vue, organisms/DisksCard.vue, organisms/MachineCard.vue, molecules/StatRow.vue`
