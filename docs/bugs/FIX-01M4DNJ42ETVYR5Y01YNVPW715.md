---
id: FIX-01M4DNJ42ETVYR5Y01YNVPW715
titre: Dans le journal, la recherche ne filtrait qu'avec Entrée ou le bouton éloigné (C28)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DNJ42ETVYR5Y01YNVPW715 : Dans le journal, la recherche ne filtrait qu'avec Entrée ou le bouton éloigné (C28)

## Symptôme
On tape dans « Rechercher », la liste ne bouge pas : il faut Entrée ou « Appliquer les filtres », bouton loin du champ ; la carte de filtres passait sur deux lignes à 1100 px. À la fin sans résultat, « Effacer les filtres » apparaissait deux fois.

## Reproduction
`e2e/hrt43-44.spec.ts` « journal à … la recherche s'applique à la frappe, sans bouton » : après la frappe seule, l'état « Aucun événement ne correspond » apparaît en moins de 3 s et « Effacer les filtres » n'existe qu'une fois. Rouge avant : rien ne change, deux boutons.

## Cause root
Tous les filtres passaient par « Appliquer » ; l'état vide ajoutait son propre « Effacer les filtres ».

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Course corrigée dans le même lot
Sans bouton, deux applications pouvaient se chevaucher ; la plus lente, dépassée, était traitée comme un échec (filtre d'avant remis, erreur affichée). `load` rend `ok`, `failed` ou `superseded` ; une application dépassée ne restaure rien (test du magasin « deux applications qui se chevauchent », rouge avant : `failed` rendu). Entrée dans la recherche annule le délai de frappe et le rappel du délai revérifie le brouillon (test de page qui compte les lectures, rouge quand les deux garde-fous sont retirés).

Mesure de la rangée unique : la hauteur de la carte de filtres SANS filtre actif est aussi mesurée (une rangée tient en moins de 140 px), pour que l'état d'avant rougisse à 1100 sans passer par le bouton « Appliquer ».

## Correction
La recherche s'applique seule 350 ms après la frappe (`Audit.vue`) ; les listes s'appliquent au choix, une période personnalisée dès que ses deux dates sont valides, et le bouton « Appliquer les filtres » est retiré (la carte tient sur une rangée) ; l'état vide n'a plus de bouton (celui de la carte de filtres suffit). `// FIX:01M4DNJ42ETVYR5Y01YNVPW715`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 / HRT-44 (revue UX du 2026-10-08)
- Code : `pages/Audit.vue`
