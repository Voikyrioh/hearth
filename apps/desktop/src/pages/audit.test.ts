import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App.vue";
import { expectedFile } from "@/assets/illustrations/screens";
import { emptyDraft } from "@/audit/filters";
import { CONTROL_MARK } from "@/audit/text";
import AuditDetailDialog from "@/components/molecules/AuditDetailDialog.vue";
import MultiSelect from "@/components/molecules/MultiSelect.vue";
import AuditFilters from "@/components/organisms/AuditFilters.vue";
import AuditTable from "@/components/organisms/AuditTable.vue";
import type { AuditEntry } from "@/link";
import { AUDIT_LIVE_DEBOUNCE_MS } from "@/stores/audit";
import { useToastsStore } from "@/stores/toasts";
import { mountContext } from "@/test/mount";

// HRT-14 : la page Journal d'activité et ses composants (BR-AUDIT-001, 010, 013 à 020).

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
  // `<dialog>` n'a pas de `showModal` dans happy-dom.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

async function boot(seed = 120) {
  const ctx = await mountContext();
  ctx.bridge.audit.seed("forge", seed);
  await ctx.router.push("/servers/forge/audit");
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

let nextId = 1000;
function entry(over: Partial<AuditEntry> = {}): AuditEntry {
  const atMs = Date.UTC(2026, 9, 4, 10, 0, 0);
  return {
    id: nextId++,
    at: new Date(atMs).toISOString(),
    atMs,
    account: "marie",
    origin: { kind: "client", name: "poste", addr: "10.0.0.7", text: "10.0.0.7 (poste)" },
    action: "account.create",
    actionLabel: "Création de compte",
    target: "paul",
    outcome: "ok",
    reason: null,
    repeatCount: 0,
    ...over,
  };
}

describe("page Journal d'activité", () => {
  it("montre le tableau (colonnes de la spec), le compteur et l'indicateur de conservation", async () => {
    const { wrapper } = await boot(120);
    const headers = wrapper.findAll('[role="columnheader"]').map((h) => h.text());
    expect(headers).toEqual([
      "Date et heure",
      "Compte",
      "Origine",
      "Action",
      "Cible",
      "Résultat",
      "Raison",
    ]);
    expect(wrapper.text()).toContain("100 événements ou plus");
    expect(wrapper.text()).toContain("Journal conservé pendant 90 jours ou 50 000 entrées");
    expect(wrapper.text()).toContain("Filtres");
    wrapper.unmount();
  });

  it("ne dessine que les lignes visibles : des milliers d'entrées restent des dizaines de nœuds", async () => {
    const { wrapper } = await boot(400);
    const rows = wrapper.findAll('[role="row"][data-row-key]');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(80);
    wrapper.unmount();
  });

  it("journal vide : « Aucune activité enregistrée pour l'instant », sans tableau", async () => {
    const { wrapper } = await boot(0);
    expect(wrapper.text()).toContain("Aucune activité enregistrée pour l'instant");
    expect(wrapper.get("img").attributes("src")).toContain(expectedFile("journal"));
    expect(wrapper.find('[role="grid"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("pas de bouton « Appliquer » ; Entrée dans la recherche filtre ; « Effacer les filtres » revient au journal complet", async () => {
    const { wrapper } = await boot(60);
    expect(wrapper.text()).not.toContain("Appliquer");
    expect(wrapper.text()).not.toContain("Effacer les filtres");
    await wrapper.find('input[type="search"]').setValue("zzz-introuvable");
    await wrapper.find("form").trigger("submit");
    await flushPromises();
    expect(wrapper.text()).toContain("Aucun événement ne correspond");
    const clear = wrapper.findAll('button[aria-label="Effacer les filtres"]');
    expect(clear.length).toBeGreaterThan(0);
    await clear[0]?.trigger("click");
    await flushPromises();
    expect(wrapper.text()).not.toContain("Aucun événement ne correspond");
    expect((wrapper.find('input[type="search"]').element as HTMLInputElement).value).toBe("");
    expect(wrapper.find('[role="grid"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("Entrée dans la recherche : une seule lecture, le délai de frappe déjà armé ne relance rien", async () => {
    const { wrapper, bridge } = await boot(60);
    const original = bridge.readAudit.bind(bridge);
    let reads = 0;
    bridge.readAudit = async (...args) => {
      reads += 1;
      return original(...args);
    };
    await wrapper.find('input[type="search"]').setValue("marie");
    await wrapper.find("form").trigger("submit");
    await flushPromises();
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(reads).toBe(1);
    wrapper.unmount();
  });

  it("échec de lecture pendant la frappe : filtres et liste précédents gardés, une seule notification, le champ garde la saisie", async () => {
    const { wrapper, bridge } = await boot(60);
    const before = wrapper.findAll('[role="row"][data-row-key]').length;
    const original = bridge.readAudit.bind(bridge);
    bridge.readAudit = async (...args) => {
      if (args[1].text) throw new Error("lien coupé");
      return original(...args);
    };
    await wrapper.find('input[type="search"]').setValue("marie");
    await vi.advanceTimersByTimeAsync(1000);
    await flushPromises();
    expect(document.body.textContent).toContain("Impossible de charger le journal");
    expect(
      useToastsStore().items.filter(
        (toast) => toast.message === "Impossible de charger le journal",
      ),
    ).toHaveLength(1);
    expect((wrapper.find('input[type="search"]').element as HTMLInputElement).value).toBe("marie");
    expect(wrapper.findAll('[role="row"][data-row-key]').length).toBe(before);
    wrapper.unmount();
  });

  it("une rafale de refus arrive en direct : regroupée « X tentatives refusées en Y min », déployable", async () => {
    const { wrapper, bridge } = await boot(30);
    bridge.audit.addBurst("forge", 6, "203.0.113.9", 10_000);
    await flushPromises();
    expect(wrapper.text()).toContain("6 tentatives refusées en 1 min");
    const burst = wrapper.find('[role="row"][aria-expanded]');
    expect(burst.attributes("aria-expanded")).toBe("false");
    await burst.trigger("click");
    expect(wrapper.find('[role="row"][aria-expanded]').attributes("aria-expanded")).toBe("true");
    const addr = wrapper
      .findAll('[role="gridcell"]')
      .filter((c) => c.text().includes("203.0.113.9 (inconnu)"));
    expect(addr.length).toBeGreaterThanOrEqual(6);
    wrapper.unmount();
  });

  it("hors ligne : le bandeau du gabarit dit tout, la page n'ajoute aucun message ni second « Réessayer » ; au retour, « Lien rétabli » et rattrapage (HRT-38, C30)", async () => {
    const { wrapper, bridge } = await boot(40);
    bridge.setState("forge", "offline");
    const missed = bridge.audit.add("forge", { account: "léa", actionLabel: "Manquée" });
    await flushPromises();
    // Un seul « Réessayer » : celui du bandeau.
    expect(wrapper.get(".banner").text()).toContain("Réessayer maintenant");
    expect(wrapper.text()).not.toContain("Périmé");
    expect(wrapper.text()).not.toContain("Données périmées, serveur injoignable");
    expect(wrapper.text()).not.toContain("Rechargement manuel");
    expect(wrapper.text()).not.toContain(
      "Indisponible tant que le lien avec le serveur n'est pas établi.",
    );
    expect(wrapper.findAll("button").filter((b) => /^Réessayer/.test(b.text()))).toHaveLength(1);
    // Le gabarit marque la page périmée UNE fois (la page ne s'enveloppe pas elle-même).
    expect(wrapper.findAll('[data-stale="true"]')).toHaveLength(1);
    expect(wrapper.text()).not.toContain("Manquée");
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.text()).toContain("Manquée");
    expect(wrapper.text()).toContain("Lien rétabli, données à jour");
    expect(bridge.audit.reads.at(-1)?.before).toBeNull();
    expect(missed.id).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it("« Exporter » : désactivé hors connexion et sans entrée, sinon exporte le résultat filtré appliqué", async () => {
    const { wrapper, bridge } = await boot(20);
    const button = () => wrapper.findAll("button").find((b) => b.text().includes("Exporter"));
    expect(button()?.attributes("aria-disabled")).toBeUndefined();
    await button()?.trigger("click");
    await flushPromises();
    expect(bridge.audit.exports).toHaveLength(1);
    expect(wrapper.text()).toContain("Export terminé");
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(button()?.attributes("aria-disabled")).toBe("true");
    await button()?.trigger("click");
    expect(bridge.audit.exports).toHaveLength(1);
    wrapper.unmount();
  });

  it("annonce les nouvelles entrées dans une zone polie, sans bouton quand on est en haut", async () => {
    const { wrapper, bridge } = await boot(30);
    bridge.audit.add("forge");
    await flushPromises();
    expect(wrapper.find(".audit__sr").text()).toBe("1 nouvelle entrée");
    expect(wrapper.text()).not.toContain("nouvelles entrées");
    wrapper.unmount();
  });

  it("fermer la page relâche l'écoute du direct", async () => {
    const { wrapper, bridge, router } = await boot(30);
    expect(bridge.audit.listenerCount("forge")).toBe(1);
    await router.push("/servers/forge/dashboard");
    await flushPromises();
    expect(bridge.audit.listenerCount("forge")).toBe(0);
    wrapper.unmount();
  });

  it("filtre actif et entrée en direct : l'agent décide (une relecture), l'entrée hors filtre n'apparaît pas", async () => {
    const { wrapper, bridge } = await boot(30);
    await wrapper.find('input[type="search"]').setValue("léa");
    await wrapper.find("form").trigger("submit");
    await flushPromises();
    bridge.audit.add("forge", { account: "paul", actionLabel: "Hors filtre" });
    bridge.audit.add("forge", { account: "léa", actionLabel: "Dans le filtre" });
    await vi.advanceTimersByTimeAsync(AUDIT_LIVE_DEBOUNCE_MS + 50);
    await flushPromises();
    expect(wrapper.text()).toContain("Dans le filtre");
    expect(wrapper.text()).not.toContain("Hors filtre");
    wrapper.unmount();
  });
});

describe("AuditTable", () => {
  function mountTable(entries: AuditEntry[], extra: Record<string, unknown> = {}) {
    return mount(AuditTable, {
      props: {
        entries,
        busy: false,
        hasMore: false,
        loadingMore: false,
        scrollSignal: 0,
        ...extra,
      },
      attachTo: document.body,
    });
  }

  it("rend les valeurs hostiles comme du TEXTE : aucun HTML, caractères de contrôle neutralisés", async () => {
    const hostile = entry({
      account: '<img src=x onerror="alert(1)">',
      target: `a${String.fromCharCode(0x202e)}b\nc`,
      reason: "<script>alert(1)</script>",
      origin: { kind: "client", name: "<b>x</b>", addr: "10.0.0.7", text: "10.0.0.7 (<b>x</b>)" },
    });
    const wrapper = mountTable([hostile]);
    await flushPromises();
    expect(wrapper.find("img").exists()).toBe(false);
    expect(wrapper.find("script").exists()).toBe(false);
    expect(wrapper.find("b").exists()).toBe(false);
    expect(wrapper.text()).toContain('<img src=x onerror="alert(1)">');
    expect(wrapper.text()).toContain(`a${CONTROL_MARK}b${CONTROL_MARK}c`);
    expect(wrapper.html()).not.toContain(String.fromCharCode(0x202e));
    wrapper.unmount();
  });

  it("tronque les valeurs longues proprement et garde le texte complet dans l'infobulle et le détail", async () => {
    const long = "x".repeat(300);
    const wrapper = mountTable([entry({ reason: long })]);
    await flushPromises();
    const cells = wrapper.findAll('[role="gridcell"]');
    const reason = cells[6];
    expect(reason?.text().length).toBeLessThan(100);
    expect(reason?.text().endsWith(String.fromCharCode(0x2026))).toBe(true);
    expect(reason?.attributes("title")).toBe(`X${"x".repeat(299)}`);
    wrapper.unmount();
  });

  it("l'heure est celle du PC, la source UTC reste dans l'infobulle", async () => {
    const wrapper = mountTable([entry()]);
    await flushPromises();
    const when = wrapper.find("time");
    expect(when.attributes("datetime")).toBe("2026-10-04T10:00:00.000Z");
    expect(when.element.parentElement?.getAttribute("title")).toContain(
      "UTC 2026-10-04T10:00:00.000Z",
    );
    wrapper.unmount();
  });

  it("clavier : flèches, Début, Fin ; Entrée ouvre le détail ; une seule ligne dans l'ordre de tabulation", async () => {
    const list = [entry(), entry(), entry()].reverse();
    const wrapper = mountTable(list);
    await flushPromises();
    const grid = wrapper.find('[role="grid"]');
    const rows = () => wrapper.findAll('[role="row"][data-row-key]');
    expect(rows().filter((r) => r.attributes("tabindex") === "0")).toHaveLength(1);
    await grid.trigger("keydown", { key: "ArrowDown" });
    await flushPromises();
    expect(rows()[1]?.attributes("tabindex")).toBe("0");
    await grid.trigger("keydown", { key: "End" });
    await flushPromises();
    expect(rows()[2]?.attributes("tabindex")).toBe("0");
    await grid.trigger("keydown", { key: "Home" });
    await flushPromises();
    expect(rows()[0]?.attributes("tabindex")).toBe("0");
    await grid.trigger("keydown", { key: "Enter" });
    expect(wrapper.emitted("open")?.[0]?.[0]).toMatchObject({ id: list[0]?.id });
    wrapper.unmount();
  });

  it("annonce l'état au lecteur d'écran : grille, nombre de lignes, occupé", async () => {
    const wrapper = mountTable([entry(), entry()], { busy: true });
    const grid = wrapper.find('[role="grid"]');
    expect(grid.attributes("aria-rowcount")).toBe("3");
    expect(grid.attributes("aria-colcount")).toBe("7");
    expect(grid.attributes("aria-busy")).toBe("true");
    wrapper.unmount();
  });
});

describe("AuditFilters et MultiSelect", () => {
  it("période personnalisée : messages exacts de la spec", async () => {
    const wrapper = mount(AuditFilters, {
      props: {
        draft: { ...emptyDraft(), period: "custom", fromDate: "2026-10-03", toDate: "2026-10-01" },
        accounts: [],
        active: true,
        busy: false,
        now: new Date(2026, 9, 4, 12).getTime(),
      },
    });
    expect(wrapper.text()).toContain("La fin de la période doit suivre le début");
    await wrapper.setProps({
      draft: { ...emptyDraft(), period: "custom", fromDate: "2026-06-01" },
    });
    expect(wrapper.text()).toContain("La date dépasse l'historique conservé (90 jours)");
    wrapper.unmount();
  });

  it("MultiSelect : choix multiples dans l'ordre des options, Échap ferme et rend le focus", async () => {
    const wrapper = mount(MultiSelect, {
      props: {
        modelValue: [] as string[],
        label: "Résultat",
        options: [
          { value: "ok", label: "Réussi" },
          { value: "denied", label: "Refusé" },
        ],
      },
      attachTo: document.body,
    });
    expect(wrapper.find("button").text()).toContain("Tous");
    await wrapper.find("button").trigger("click");
    const boxes = wrapper.findAll('input[type="checkbox"]');
    await boxes[1]?.setValue(true);
    expect(wrapper.emitted("update:modelValue")?.[0]?.[0]).toEqual(["denied"]);
    await wrapper.setProps({ modelValue: ["denied"] });
    await boxes[0]?.setValue(true);
    expect(wrapper.emitted("update:modelValue")?.[1]?.[0]).toEqual(["ok", "denied"]);
    await wrapper.find(".multi").trigger("keydown", { key: "Escape" });
    expect(wrapper.find('[role="group"]').exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("AuditDetailDialog", () => {
  it("montre l'entrée en entier, valeurs non fiables neutralisées, et se ferme", async () => {
    const wrapper = mount(AuditDetailDialog, {
      props: { entry: entry({ reason: `<b>très</b>\nlong`, target: "x".repeat(200) }) },
      attachTo: document.body,
    });
    await flushPromises();
    const text = document.body.textContent ?? "";
    expect(text).toContain("x".repeat(200));
    expect(text).toContain(`<b>très</b>${CONTROL_MARK}long`);
    expect(text).toContain("Date source (UTC)");
    expect(document.body.querySelector("dialog b")).toBeNull();
    const close = [...document.body.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("Fermer"),
    );
    close?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(wrapper.emitted("close")).toBeTruthy();
    wrapper.unmount();
  });
});
