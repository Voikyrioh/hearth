<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import { type FicheCard, getLinkBridge } from "@/link";
import { isSteamAppId } from "@/link/illustrations";

// L'image d'un jeu, ou son visuel neutre (HRT-239, ADR-0061, BR-FICHE-062). La source est choisie ici et nulle part ailleurs :
// l'identifiant Steam du jeu d'abord (en-tête du magasin, que la COQUILLE va chercher et garde), sinon l'illustration livrée
// avec l'agent, sinon rien (visuel neutre tout de suite). La page n'ouvre aucune connexion : l'image n'est qu'une `<img>` du
// protocole de la coquille. Une image qui échoue (absente, refusée, hors ligne sans copie, réglage coupé) donne le visuel
// neutre pour toute la visite. Le rapport 460:215 est réservé d'avance : rien ne saute quand l'image arrive. Le visuel
// neutre est décoratif (texte alternatif vide : le nom du jeu est dans la carte) ; l'image a pour texte « Image du jeu {nom} ».
// `banner` (fiche) : l'image à sa taille, centrée, sur un fond fait de la même image floutée et assombrie ; `card` (grille).
const props = withDefaults(
  defineProps<{
    serverId: string;
    card: FicheCard;
    variant?: "card" | "banner";
  }>(),
  { variant: "card" },
);

const failed = ref(false);
let reported = false;

const source = computed(() => {
  const bridge = getLinkBridge();
  const appId = props.card.steamAppId;
  if (appId !== undefined && appId !== null && isSteamAppId(appId)) {
    return bridge.steamHeaderUrl(appId);
  }
  const illustration = props.card.illustration;
  if (illustration) {
    return bridge.agentIllustrationUrl(props.serverId, props.card.id, illustration.digest);
  }
  return "";
});

// Une autre fiche ou une autre source : on réessaie (l'échec ne vaut que pour la source qui a échoué).
watch(source, () => {
  failed.value = false;
});

const shown = computed(() => source.value !== "" && !failed.value);

function onError() {
  failed.value = true;
  if (!reported) {
    reported = true;
    logUiError(new Error("image de jeu illisible"), "catalog:image");
  }
}
</script>

<template>
  <div :class="['fv', `fv--${variant}`]" data-fiche-visual>
    <template v-if="shown">
      <img
        v-if="variant === 'banner'"
        class="fv__blur"
        :src="source"
        alt=""
        aria-hidden="true"
        decoding="async"
      />
      <img
        class="fv__img"
        :src="source"
        :alt="t('catalogue.imageAlt', { name: safeText(card.name) })"
        loading="lazy"
        decoding="async"
        data-fiche-image
        @error="onError"
      />
    </template>
    <div v-else class="fv__neutral" aria-hidden="true" data-fiche-neutral></div>
  </div>
</template>

<style scoped>
.fv {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  width: 100%;
  background: var(--card-2);
}

.fv--card {
  aspect-ratio: 460 / 215;
}

.fv--banner {
  height: var(--banner-height);
}

.fv__img {
  position: relative;
  display: block;
}

.fv--card .fv__img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.fv--banner .fv__img {
  height: 100%;
  width: auto;
  max-width: 100%;
  aspect-ratio: 460 / 215;
  object-fit: contain;
}

.fv__blur {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  filter: blur(var(--banner-blur)) brightness(0.45);
  transform: scale(1.2);
}

.fv__neutral {
  width: 100%;
  height: 100%;
  background:
    radial-gradient(circle at 72% 28%, var(--ember-glow), transparent 60%),
    linear-gradient(135deg, var(--card-2), var(--card));
}
</style>
