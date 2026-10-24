import { describe, expect, it } from "vitest";
import wire from "./fixtures/terraria-settings.json";
import {
  toSetting,
  WIRE_QUESTION_KEYS,
  WIRE_SETTING_KEYS,
  type WireSetting,
} from "./simulated-terraria-settings";

// `toSetting` relit le JSON généré depuis la fiche : si l'agent sert un champ de plus (ou perd un champ attendu), ce
// test échoue au lieu de laisser le simulateur s'écarter en silence du vrai catalogue.

const items = (wire as unknown as { settings: WireSetting[] }).settings;
const REQUIRED_SETTING = ["question", "key", "theme", "level", "channel"];
const REQUIRED_QUESTION = ["id", "label", "help", "secret", "kind"];

describe("la fixture Terraria du simulateur suit le fil de l'agent", () => {
  it("ne porte aucune clé que toSetting ignore", () => {
    for (const item of items) {
      for (const name of Object.keys(item)) expect(WIRE_SETTING_KEYS).toHaveProperty(name);
      for (const name of Object.keys(item.question))
        expect(WIRE_QUESTION_KEYS).toHaveProperty(name);
    }
  });

  it("porte les champs obligatoires de chaque réglage", () => {
    expect(items.length).toBeGreaterThan(0);
    for (const item of items) {
      for (const name of REQUIRED_SETTING) expect(item).toHaveProperty(name);
      for (const name of REQUIRED_QUESTION) expect(item.question).toHaveProperty(name);
    }
  });

  it("convertit chaque champ du fil sans perdre une valeur de la source", () => {
    for (const item of items) {
      const setting = toSetting(item);
      const q = item.question;
      expect(setting.key).toBe(item.key);
      expect(setting.theme).toBe(item.theme);
      expect(setting.level).toBe(item.level);
      expect(setting.gameDefault).toBe(item.game_default ?? null);
      expect(setting.imposedWhy).toBe(item.imposed_why ?? null);
      expect(setting.since).toBe(item.since ?? null);
      expect(setting.until).toBe(item.until ?? null);
      expect(setting.replacedBy).toBe(item.replaced_by ?? null);
      expect(setting.list?.separator ?? null).toBe(item.list?.separator ?? null);
      expect(setting.list?.maxItems ?? null).toBe(item.list?.max_items ?? null);
      expect(setting.list?.itemCharset ?? null).toBe(item.list?.item_charset ?? null);
      expect(setting.question).toEqual({
        id: q.id,
        label: q.label,
        help: q.help,
        secret: q.secret,
        proposed: q.proposed ?? null,
        kind: q.kind,
        choices: q.choices ?? [],
        min: q.min ?? null,
        max: q.max ?? null,
        minLen: q.min_len ?? null,
        maxLen: q.max_len ?? null,
        charset: q.charset ?? null,
        frozenAfterCreation: q.frozen_after_creation ?? false,
        variants: q.variants ?? [],
        secretKind: q.secret_kind ?? null,
        obtainUrl: q.obtain_url ?? null,
      });
    }
  });
});
