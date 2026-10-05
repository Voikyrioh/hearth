import { createMemoryHistory } from "vue-router";
import { vNeedsLink } from "@/directives/needsLink";
import type { SimulatedOptions } from "@/link";
import { createAppRouter } from "@/router";
import { startedApp } from "./app";

/** Application complète en mémoire : routeur (historique mémoire), pinia, directive. */
export async function mountContext(options: SimulatedOptions = {}) {
  const ctx = await startedApp(options);
  const router = createAppRouter(createMemoryHistory());
  return {
    ...ctx,
    router,
    global: { plugins: [ctx.pinia, router], directives: { "needs-link": vNeedsLink } },
  };
}
