import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import { useDeployFormStore } from "./deployForm";

// La saisie d'un déploiement vit en mémoire seulement et s'efface (HRT-69, BR-DEPLOY-029).
beforeEach(() => setActivePinia(createPinia()));

describe("les brouillons de déploiement", () => {
  it("un brouillon par serveur, vide à la création", () => {
    const store = useDeployFormStore();
    const draft = store.of("forge");
    expect(draft).toMatchObject({ name: "", image: "", tag: "", command: "", env: [], ports: [] });
    expect(store.filled("forge")).toBe(false);
    expect(store.of("forge")).toBe(draft);
    expect(store.of("salon")).not.toBe(draft);
  });

  it("une rangée a toujours une identité neuve, jamais réutilisée", () => {
    const store = useDeployFormStore();
    const ids = [store.rowId(), store.rowId(), store.rowId()];
    expect(new Set(ids).size).toBe(3);
  });

  it("effacer écrase les valeurs puis lâche le brouillon", () => {
    const store = useDeployFormStore();
    const draft = store.of("forge");
    draft.name = "web";
    draft.command = "run --token SENTINEL";
    draft.env.push({ id: store.rowId(), name: "DB_PASSWORD", value: "SENTINEL", reveal: null });
    const row = draft.env[0];
    expect(store.filled("forge")).toBe(true);
    store.wipe("forge");
    expect(store.has("forge")).toBe(false);
    expect(store.filled("forge")).toBe(false);
    expect(row?.value).toBe("");
    expect(row?.name).toBe("");
    expect(draft.command).toBe("");
    expect(draft.env).toEqual([]);
  });

  it("tout effacer vide tous les serveurs", () => {
    const store = useDeployFormStore();
    store.of("forge").name = "a";
    store.of("salon").name = "b";
    store.wipeAll();
    expect(store.has("forge") || store.has("salon")).toBe(false);
  });

  it("ne laisse rien dans l'état sérialisable du magasin (instantané, outils de développement)", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useDeployFormStore();
    store.of("forge").command = "SENTINEL-CMD";
    store
      .of("forge")
      .env.push({ id: store.rowId(), name: "A", value: "SENTINEL-VALUE", reveal: null });
    expect(JSON.stringify(pinia.state.value)).not.toContain("SENTINEL");
    expect(JSON.stringify(Object.entries(localStorage))).not.toContain("SENTINEL");
    expect(JSON.stringify(Object.entries(sessionStorage))).not.toContain("SENTINEL");
  });
});
