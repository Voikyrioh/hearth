---
id: ADR-0040
titre: Visionneuse de journaux d'un service : tampon non réactif de 5 000 lignes, fenêtre de lignes à hauteurs estimées puis mesurées, événements avec accusés, rien de durable
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-67)
date: 2026-10-14
portee: repo
remplace: le brouillon `2026-10-13-adr-visionneuse-de-journaux-brouillon` (canal Tauri remplacé par des événements)
liens: [ADR-0010, ADR-0013, ADR-0019, ADR-0038, BR-SLOG-002, BR-SLOG-004, BR-SLOG-005, BR-SLOG-006, BR-SLOG-007, BR-SLOG-013, BR-SLOG-019, BR-SLOG-026, BR-SLOG-027]
---

# ADR-0040 : Visionneuse de journaux

## Contexte
Un service peut écrire 1 000 lignes par seconde ; l'interface doit rester réactive, garder 5 000 lignes au plus, ne rien écrire sur le disque et faire passer les lignes longues à la ligne. La liste du journal d'activité (ADR-0019) est à hauteur de ligne fixe : elle ne convient pas.

## Décision
1. **Liaison** : un socket par lecture (`Transport::open_service_logs`, `LinkManager::open_service_logs` rend un `ServiceLogReader`). Le lecteur s'authentifie comme le flux, rend les messages par un canal borné (un consommateur lent fait attendre le socket : l'agent compte la perte, `gap { slow }`), bat le rythme pendant l'attente, se reprend seul après une coupure par le curseur de la dernière ligne (délai croissant, six essais), attend le `Retry-After` d'un `503`. **Tout refus de l'agent est une fin** (jamais retenté) : `LOGS_RATE_LIMITED` porte son délai, `LOGS_ACCOUNT_LIMIT` demande de fermer un onglet. Libérer le lecteur ferme le socket.
2. **Coquille** : trois commandes (`open_service_logs`, `close_service_logs`, `ack_service_logs`), un seul événement `link://service-logs` (la charge porte la session). Au plus 3 sessions (le plafond du compte) et 20 lots sans accusé : au-delà la coquille cesse de lire, donc l'agent compte la perte au lieu que la mémoire enfle. Aucune ligne n'est gardée, écrite ni tracée.
3. **Tampon** : objet non réactif (5 000 lignes et 2 millions de caractères au plus, retrait par le haut compté). Le magasin signale `version` au plus toutes les 100 ms. En pause : les opérations arrivées sont gardées (5 000 lignes au plus), le compteur est exact, un trou est dit à la reprise.
4. **Rendu** : fenêtre de lignes (une soixantaine de nœuds). Hauteur estimée par les colonnes (police à chasse fixe, coupure à tout caractère), corrigée par la mesure des lignes dessinées ; sommes cumulées pour trouver la fenêtre. Pas un composant Vue par ligne.
5. **Texte** : l'agent nettoie, le client RENETTOIE (`logs/text.ts` : séquences de terminal retirées, contrôles et bidirectionnels rendus visibles, 10 000 caractères au plus, repli à 600). Interpolé, jamais `v-html`, aucun lien fabriqué. Région `aria-live="off"`.
6. **Rien de durable** : fermer (onglet quitté, détail fermé, autre service, session finie, droit retiré, fenêtre fermée) remplace le tampon et ferme la lecture.

## Quand NE PAS l'appliquer / limites
- Une liste à lignes de hauteur fixe garde ADR-0019.
- L'estimation suppose une police à chasse fixe ; les caractères larges sont corrigés par la mesure.
- La sélection native ne couvre que les lignes dessinées : « Copier la sélection » copie ce qui est sélectionné à l'écran.
- L'émission réelle vers la WebView n'est pas prouvée sur le poste de développement.

## Alternatives rejetées
- Un composant par ligne, les 5 000 lignes dans le DOM, une bibliothèque de virtualisation, une hauteur fixe sans retour à la ligne.
- Un canal Tauri par lecture : les événements existent déjà, un accusé borne le débit.
