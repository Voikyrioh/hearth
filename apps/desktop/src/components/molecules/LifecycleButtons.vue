<script setup lang="ts">
import { computed, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";
import { type Controls, type Gesture, LABEL_KEYS } from "@/services/lifecycle";

// Les boutons de pilotage d'un service (HRT-65, BR-LIFE-003, 013, BR-GRANT-014) : Démarrer, Arrêter, Redémarrer.
// Un geste que l'état n'offre pas est ABSENT, pas grisé. Un geste que le droit, le moteur, le lien ou le verrou d'une
// action bloquent reste visible, `aria-disabled`, focusable, et la raison est écrite sous le groupe en texte
// permanent (jamais une infobulle seule) ; le lien coupé garde l'infobulle du socle (`needs-link`). Quand aucun
// geste n'est offert, la phrase qui le dit tient leur place.
const props = defineProps<{ controls: Controls; name: string }>();
const emit = defineEmits<{ gesture: [gesture: Gesture] }>();

const reasonId = useId();
const reason = computed(() => {
  const { reason: text, by } = props.controls;
  if (text === null) return null;
  return by === null ? text : `${text} ${by}`;
});
</script>

<template>
  <div class="life" data-lifecycle>
    <div
      v-if="controls.gestures.length > 0"
      class="life__group"
      role="group"
      :aria-label="t('lifecycle.groupLabel')"
    >
      <HButton
        v-for="gesture in controls.gestures"
        :key="gesture"
        :variant="gesture === 'start' ? 'primary' : 'secondary'"
        size="sm"
        needs-link
        :disabled="controls.disabled"
        :described-by="reason ? reasonId : undefined"
        :data-lifecycle-action="gesture"
        @click="emit('gesture', gesture)"
      >
        <HIcon :name="gesture === 'start' ? 'play' : gesture === 'stop' ? 'stop' : 'refresh'" size="sm" />
        {{ t(LABEL_KEYS[gesture]) }}
      </HButton>
    </div>
    <p v-if="reason" :id="reasonId" class="life__reason" data-lifecycle-reason>{{ reason }}</p>
    <p v-if="controls.note" class="life__note" data-lifecycle-note>
      <HIcon name="info" size="sm" />
      {{ controls.note }}
    </p>
  </div>
</template>

<style scoped>
.life {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  min-width: 0;
}

.life__group {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.life__reason,
.life__note {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.life__note {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
</style>
