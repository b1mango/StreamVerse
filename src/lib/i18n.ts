import { writable, derived, get } from "svelte/store";
import type { LanguageCode } from "./types";
import zhCN from "./locales/zh-CN.json";
import en from "./locales/en.json";

const translations: Record<LanguageCode, Record<string, string>> = {
  "zh-CN": zhCN,
  en: en
};

let currentLang: LanguageCode = "zh-CN";
const langStore = writable<LanguageCode>("zh-CN");

export function setLanguage(lang: LanguageCode) {
  currentLang = lang;
  langStore.set(lang);
}

/** Svelte-store version: use as `$t('key')` in templates for reactive i18n. */
export const t = derived(langStore, ($lang) => {
  return (key: string): string => {
    return translations[$lang]?.[key] ?? translations["zh-CN"]?.[key] ?? key;
  };
});

/** Imperative version for use outside Svelte templates (e.g. in callbacks). */
export function tRaw(key: string): string {
  return translations[currentLang]?.[key] ?? translations["zh-CN"]?.[key] ?? key;
}

export function getLanguage(): LanguageCode {
  return currentLang;
}
