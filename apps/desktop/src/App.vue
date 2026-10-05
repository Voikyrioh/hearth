<script setup lang="ts">
import { RouterView } from "vue-router";
import ServerRail from "@/components/organisms/ServerRail.vue";
</script>

<template>
  <div class="shell">
    <ServerRail />
    <div class="shell__content">
      <RouterView />
    </div>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
}

.shell__content {
  flex: 1;
  min-width: 0;
}
</style>
