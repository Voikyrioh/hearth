<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import { portLabel } from "@/services/format";

// Tous les ports d'un service, quand la ligne n'a la place d'en montrer que quelques-uns : le texte complet
// n'est ainsi plus réservé à l'infobulle de la souris (inaccessible au clavier et aux lecteurs d'écran). Même
// modèle que `AuditDetailDialog` : élément `<dialog>` natif (piège à focus, Échap), le focus revient à la ligne
// qui l'a ouvert. Texte interpolé, jamais du HTML. Les ports coupés par l'agent sont comptés, jamais listés.
const props = defineProps<{ service: ServiceInfo | null }>();
const emit = defineEmits<{ close: [] }>();

const titleId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
const closeButton = ref<{ $el: HTMLElement } | null>(null);
let opener: HTMLElement | null = null;

watch(
  () => props.service,
  (service, previous) => {
    if (service && !previous) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
  },
  { immediate: true, flush: "pre" },
);

watch(
  () => props.service !== null,
  async (open) => {
    if (open) {
      await nextTick();
      dialog.value?.showModal();
      closeButton.value?.$el.querySelector("button")?.focus();
    } else {
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true, flush: "post" },
);

onBeforeUnmount(() => {
  opener?.focus();
  opener = null;
});

function onNativeClose() {
  if (props.service) emit("close");
}
</script>

<template>
  <Teleport to="body">
    <dialog
      v-if="service"
      ref="dialog"
      class="ports"
      data-ports-dialog
      :aria-labelledby="titleId"
      @cancel.prevent="emit('close')"
      @close="onNativeClose"
    >
      <h2 :id="titleId" class="ports__title">
        {{ t("services.portsDialogTitle", { name: service.name }) }}
      </h2>
      <ul class="ports__list">
        <li v-for="(port, index) in service.ports" :key="index" class="ports__item">
          {{ portLabel(port) }}
        </li>
        <li v-if="service.portsOmitted > 0" class="ports__item ports__item--omitted">
          {{ t("services.portsOnlyOmitted", { n: service.portsOmitted }) }}
        </li>
      </ul>
      <div class="ports__actions">
        <HButton ref="closeButton" variant="secondary" @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.ports {
  width: var(--ports-dialog-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  max-height: calc(100vh - var(--page-pad) * 2);
  padding: var(--space-5);
  overflow-y: auto;
  border: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  color: var(--tx);
  box-shadow: var(--card-edge);
}

.ports::backdrop {
  background: var(--scrim);
}

.ports__title {
  overflow-wrap: anywhere;
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.ports__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: var(--space-4) 0 0;
  padding: 0;
  list-style: none;
}

.ports__item {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.ports__item--omitted {
  color: var(--tx2);
  font-family: inherit;
}

.ports__actions {
  display: flex;
  justify-content: flex-end;
  margin-top: var(--space-5);
}
</style>
