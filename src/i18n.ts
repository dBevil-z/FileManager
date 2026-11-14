import { useSyncExternalStore } from "react";
import zhCN from "./locales/zh-CN.json";
import en from "./locales/en.json";
import ja from "./locales/ja.json";
import de from "./locales/de.json";
import fr from "./locales/fr.json";
import es from "./locales/es.json";

type Resource = {
  locale: string;
  fallback: string;
  complete: boolean;
  messages: Record<string, string>;
};
export const languageResources: Record<string, Resource> = {
  "zh-CN": zhCN,
  en,
  ja,
  de,
  fr,
  es,
};
let currentLanguage = "zh-CN";
const listeners = new Set<() => void>();
export function setLanguage(language: string) {
  const next = languageResources[language] ? language : "zh-CN";
  if (currentLanguage === next) return;
  currentLanguage = next;
  if (typeof document !== "undefined") document.documentElement.lang = next;
  listeners.forEach((listener) => listener());
}
export function getLocale() {
  return currentLanguage === "zh-CN"
    ? "zh-CN"
    : languageResources[currentLanguage].complete
      ? currentLanguage
      : "en";
}
export function useLanguage() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => currentLanguage,
  );
}
export function translate(key: string, language: string, ...values: unknown[]) {
  const resource = languageResources[language] || zhCN;
  const trimmed = key.trim();
  const translated =
    language === "zh-CN"
      ? trimmed
      : resource.messages[trimmed] ||
        en.messages[trimmed as keyof typeof en.messages] ||
        trimmed;
  const leading = key.match(/^\s*/)?.[0] || "";
  const trailing = key.match(/\s*$/)?.[0] || "";
  // Substitution happens once: file names and user text are never interpreted as messages.
  return (
    leading +
    translated.replace(/\{(\d+)\}/g, (match, index) =>
      Number(index) < values.length
        ? String(values[Number(index)] ?? "")
        : match,
    ) +
    trailing
  );
}
export function t(key: string, ...values: unknown[]) {
  return translate(key, currentLanguage, ...values);
}
