function callHost(name, css) {
  const callback = globalThis[name];
  if (typeof callback !== 'function') {
    return {
      ok: false,
      backend: 'unavailable',
      error: `${name} host callback is not installed`,
      warnings: [],
    };
  }
  return callback(String(css || ''));
}

export function parseInlineStyle(css) {
  return callHost('__rustQjsDomLightningCssParseInlineStyle', css);
}

export function parseStylesheet(css) {
  return callHost('__rustQjsDomLightningCssParseStylesheet', css);
}

export const isAvailable = true;

