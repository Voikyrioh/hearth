<script setup lang="ts">
import { crashOnce } from "./crashOnce";

// Page de DIAGNOSTIC du build de test (`vite build --mode e2e`, voir `router/index.ts`) : son
// rendu plante la première fois pour vérifier, sur un build de production, que la frontière
// d'erreur affiche son repli et que « Réessayer » remonte la page. Absente du build livré
// (`npm run check:dist`). Textes en dur voulus : cette page n'existe jamais chez l'utilisateur.
crashOnce();
</script>

<template>
  <p class="diag">Diagnostic rétabli</p>
</template>

<style scoped>
.diag {
  padding: var(--page-pad);
}
</style>
