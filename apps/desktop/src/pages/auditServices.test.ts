import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emptyDraft } from "@/audit/filters";
import AuditFilters from "@/components/organisms/AuditFilters.vue";
import { AUDIT_KINDS, EMPTY_AUDIT_FILTER } from "@/link";
import { useAuditStore } from "@/stores/audit";
import { startedApp } from "@/test/app";

// HRT-78 : le filtre du journal d'activité gagne « Gestion des services » et « Droits sur les services »
// (`ActionKind::Services` et `ServicesRights` existaient côté liaison sans entrée à l'écran), avec les libellés
// d'action des entrées correspondantes.
beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
});
afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

describe("le filtre de type d'action", () => {
  it("garde les cinq types d'avant et ajoute les deux nouveaux, dans cet ordre", () => {
    expect([...AUDIT_KINDS]).toEqual([
      "login_ok",
      "login_denied",
      "accounts",
      "update",
      "denied",
      "services",
      "services_rights",
    ]);
  });

  it("propose « Gestion des services » et « Droits sur les services » à côté des autres", async () => {
    const wrapper = mount(AuditFilters, {
      props: { draft: emptyDraft(), accounts: [], active: false, busy: false, now: Date.now() },
      attachTo: document.body,
    });
    const kind = wrapper.findAll(".multi").find((box) => box.text().includes("Type d'action"));
    if (!kind) throw new Error("le filtre de type d'action est absent");
    await kind.get("button").trigger("click");
    const labels = kind.findAll("label").map((label) => label.text());
    expect(labels).toContain("Gestion des services");
    expect(labels).toContain("Droits sur les services");
    expect(labels).toContain("Mise à jour de l'agent");
    wrapper.unmount();
  });
});

describe("filtrer sur les droits sur les services", () => {
  async function seeded() {
    const ctx = await startedApp();
    const audit = ctx.bridge.audit;
    audit.add("forge", { action: "account.services.grant", account: null, target: "paul" });
    audit.add("forge", { action: "account.services.revoke", account: null, target: "paul" });
    audit.add("forge", { action: "service.env.reveal", account: "marie", target: "service web" });
    audit.add("forge", { action: "login" });
    return ctx;
  }

  it("« Droits sur les services » ne garde que l'octroi et le retrait, avec leur libellé d'action", async () => {
    await seeded();
    const store = useAuditStore();
    await store.open("forge");
    await store.apply({ ...emptyDraft(), kinds: ["services_rights"] }, Date.now());
    await flushPromises();
    expect(store.entries.map((entry) => entry.actionLabel).sort()).toEqual([
      "Droit de gérer les services accordé",
      "Droit de gérer les services retiré",
    ]);
  });

  it("« Gestion des services » ne garde que la révélation des valeurs", async () => {
    await seeded();
    const store = useAuditStore();
    await store.open("forge");
    await store.apply({ ...emptyDraft(), kinds: ["services"] }, Date.now());
    await flushPromises();
    expect(store.entries.map((entry) => entry.actionLabel)).toEqual([
      "Valeurs des variables révélées",
    ]);
  });

  it("la liste de types transmise à la coquille porte les deux nouvelles valeurs", async () => {
    const ctx = await seeded();
    const store = useAuditStore();
    await store.open("forge");
    await store.apply({ ...emptyDraft(), kinds: ["services", "services_rights"] }, Date.now());
    const last = ctx.bridge.audit.reads.at(-1);
    expect(last?.filter.kinds).toEqual(["services", "services_rights"]);
    expect(EMPTY_AUDIT_FILTER.kinds).toEqual([]);
  });
});
