<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HTag from "@/components/atoms/HTag.vue";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";

// Les repères d'un service (BR-SVC-003) : « Créé hors de Hearth » (le seul signe d'origine, jamais déduit du
// nom ou de l'image), « Géré par le système ({unité}) » et « Pile Compose {projet} ». Les deux derniers
// décrivent le service sans dire qui l'a créé. `inline` : le premier repère, sur la ligne du nom ; sinon, les
// deux autres, sur la seconde ligne de la cellule.
const props = defineProps<{
  service: Pick<ServiceInfo, "origin" | "driver" | "unit" | "composeProject"> &
    Partial<Pick<ServiceInfo, "adopted">>;
  part: "origin" | "details";
}>();

const external = computed(() => props.part === "origin" && props.service.origin === "external");
/** « Adopté » (BR-SVC-003) : le service créé hors de Hearth a été confié à Hearth. */
const adopted = computed(() => props.part === "origin" && !!props.service.adopted);
const managed = computed(() =>
  props.part === "details" && props.service.driver === "supervisor" && props.service.unit
    ? t("services.managed", { unit: safeText(props.service.unit) })
    : null,
);
const compose = computed(() =>
  props.part === "details" && props.service.composeProject
    ? t("services.compose", { project: safeText(props.service.composeProject) })
    : null,
);
</script>

<template>
  <template v-if="part === 'origin'">
    <HTag v-if="external" :title="t('services.externalHelp')">{{ t("services.external") }}</HTag>
    <HTag v-if="adopted">{{ t("services.adopted") }}</HTag>
  </template>
  <template v-else>
    <HTag v-if="managed" class="origin-tag" :title="managed">{{ managed }}</HTag>
    <HTag v-if="compose" class="origin-tag" :title="compose">{{ compose }}</HTag>
  </template>
</template>

<style scoped>
/* Un nom d'unité ou de projet venu du serveur n'élargit jamais la ligne : borné, coupé par « … », entier au survol. */
.origin-tag {
  max-width: var(--tag-max-width);
  overflow: hidden;
  text-overflow: ellipsis;
  vertical-align: bottom;
}
</style>
