// Tiny i18n layer: all UI strings go through t(key, params). English
// messages ship with each app; other languages can be added as JSON maps.

type Messages = Record<string, string>;
let messages: Messages = {};

export function setMessages(m: Messages) {
  messages = { ...messages, ...m };
}

/** Translate `key`; `{name}` placeholders are replaced from `params`.
 *  Unknown keys fall back to the key itself (English source strings). */
export function t(key: string, params?: Record<string, string | number>): string {
  let s = messages[key] ?? key;
  if (params) for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
  return s;
}
