# Architecture — transfers

Les textes et les règles du panneau « Transferts » (HRT-96, `BR-FILES-013`, `BR-FILES-019`). Fonctions pures, aucune E/S.

```
transfers/
├── messages.ts       → `formatSize` (octets binaires : « 600 Mio »), `ratioOf`, `progressText`, `isOpen`, `canResume` / `canCancel` / `canDismiss` (les gestes qui ont un sens), `statusText`, `refusalText` (refus du serveur pendant l'envoi), `depositRefusalText` (refus à l'ouverture du dépôt) ; aucun texte ne vient de l'agent
└── messages.test.ts  → tailles, gestes, phrases
```

Le magasin est `stores/transfers.ts` (relecture tant qu'un envoi envoie), le panneau `components/organisms/TransfersStrip.vue`, le pont `link/transfers.ts` et `link/simulated-transfers.ts`, la coquille `src-tauri/src/transfers/`, la liaison `hearth-link/src/manager/transfers.rs`.
