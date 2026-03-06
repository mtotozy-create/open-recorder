import { messages, type Locale, type TranslationKey } from "./messages";

export type TranslationParams = Record<string, string | number | undefined>;
export type Translator = (key: TranslationKey, params?: TranslationParams) => string;

const STORAGE_KEY = "open-recorder.locale";

export function isLocale(value: string): value is Locale {
  return value === "zh-CN" || value === "en-US";
}

export function getInitialLocale(): Locale {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value && isLocale(value)) {
      return value;
    }
  } catch {
    // Ignore storage errors and use default locale.
  }

  return "zh-CN";
}

export function persistLocale(locale: Locale): void {
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // Ignore storage errors.
  }
}

export function createTranslator(locale: Locale): Translator {
  return (key, params) => {
    const template = messages[locale][key] ?? messages["en-US"][key] ?? key;
    if (!params) {
      return template;
    }

    return Object.entries(params).reduce((result, [paramKey, value]) => {
      return result.replaceAll(`{${paramKey}}`, String(value ?? ""));
    }, template);
  };
}
