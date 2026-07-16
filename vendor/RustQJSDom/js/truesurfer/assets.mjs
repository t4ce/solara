const ASSET_BACKEND = 'truesurfer-assets@1';

function safeString(value) {
  return value === null || value === undefined ? '' : String(value);
}

function attrValue(node, name) {
  const wanted = safeString(name).toLowerCase();
  const attrs = Array.isArray(node && node.attrs) ? node.attrs : [];
  for (const attr of attrs) {
    if (safeString(attr && attr.name).toLowerCase() === wanted) return safeString(attr.value);
  }
  return '';
}

function textContent(node) {
  if (!node || typeof node !== 'object') return '';
  if (node.nodeName === '#text') return safeString(node.value);
  let out = '';
  const children = Array.isArray(node.childNodes) ? node.childNodes : [];
  for (const child of children) out += textContent(child);
  return out;
}

function relTokens(node) {
  return attrValue(node, 'rel').toLowerCase().split(/\s+/).filter(Boolean);
}

function parseSrcset(input) {
  const source = safeString(input);
  const urls = [];
  let cursor = 0;
  while (cursor < source.length) {
    while (cursor < source.length && /[\s,]/.test(source[cursor])) cursor += 1;
    if (cursor >= source.length) break;
    const start = cursor;
    const dataUrl = source.slice(cursor, cursor + 5).toLowerCase() === 'data:';
    while (cursor < source.length) {
      const ch = source[cursor];
      if (/\s/.test(ch) || (!dataUrl && ch === ',')) break;
      cursor += 1;
    }
    const url = source.slice(start, cursor).trim();
    if (url) urls.push(url);
    while (cursor < source.length && source[cursor] !== ',') cursor += 1;
    if (source[cursor] === ',') cursor += 1;
  }
  return urls;
}

function cssUrls(input) {
  const source = safeString(input);
  const urls = [];
  const pattern = /url\(\s*(?:"([^"]*)"|'([^']*)'|([^)]*))\s*\)/gi;
  let match;
  while ((match = pattern.exec(source))) {
    const url = safeString(match[1] ?? match[2] ?? match[3]).trim();
    if (url) urls.push(url);
  }
  return urls;
}

function pushRequest(requests, details) {
  const rawUrl = safeString(details.rawUrl).trim();
  if (!rawUrl || rawUrl.startsWith('#')) return;
  requests.push({
    path: safeString(details.path),
    tag: safeString(details.tag),
    attribute: safeString(details.attribute),
    rawUrl,
    baseUrl: safeString(details.baseUrl),
    kind: safeString(details.kind || 'asset'),
    initiator: safeString(details.initiator || 'html'),
    mediaType: safeString(details.mediaType),
  });
}

function pushAttribute(requests, node, path, attribute, kind, baseUrl) {
  pushRequest(requests, {
    path,
    tag: safeString(node && node.tagName).toLowerCase(),
    attribute,
    rawUrl: attrValue(node, attribute),
    baseUrl,
    kind,
    initiator: 'html',
    mediaType: attrValue(node, 'type'),
  });
}

function pushSrcset(requests, node, path, baseUrl) {
  const candidates = parseSrcset(attrValue(node, 'srcset'));
  for (const rawUrl of candidates) {
    pushRequest(requests, {
      path,
      tag: safeString(node && node.tagName).toLowerCase(),
      attribute: 'srcset',
      rawUrl,
      baseUrl,
      kind: 'image',
      initiator: 'html',
      mediaType: attrValue(node, 'type'),
    });
  }
}

function pushCssUrls(requests, css, path, tag, attribute, baseUrl) {
  for (const rawUrl of cssUrls(css)) {
    pushRequest(requests, {
      path,
      tag,
      attribute,
      rawUrl,
      baseUrl,
      kind: 'css-url',
      initiator: 'css',
      mediaType: '',
    });
  }
}

function visitElement(requests, node, path, documentBaseUrl) {
  const tag = safeString(node && node.tagName).toLowerCase();
  switch (tag) {
    case 'img':
      pushAttribute(requests, node, path, 'src', 'image', documentBaseUrl);
      pushSrcset(requests, node, path, documentBaseUrl);
      break;
    case 'source':
      pushAttribute(requests, node, path, 'src', 'media', documentBaseUrl);
      pushSrcset(requests, node, path, documentBaseUrl);
      break;
    case 'audio':
    case 'video':
      pushAttribute(requests, node, path, 'src', 'media', documentBaseUrl);
      if (tag === 'video') pushAttribute(requests, node, path, 'poster', 'image', documentBaseUrl);
      break;
    case 'track':
      pushAttribute(requests, node, path, 'src', 'text-track', documentBaseUrl);
      break;
    case 'iframe':
      pushAttribute(requests, node, path, 'src', 'document', documentBaseUrl);
      break;
    case 'embed':
      pushAttribute(requests, node, path, 'src', 'embed', documentBaseUrl);
      break;
    case 'object':
      pushAttribute(requests, node, path, 'data', 'object', documentBaseUrl);
      break;
    case 'script':
      pushAttribute(requests, node, path, 'src', 'script', documentBaseUrl);
      break;
    case 'input':
      if (attrValue(node, 'type').toLowerCase() === 'image') {
        pushAttribute(requests, node, path, 'src', 'image', documentBaseUrl);
      }
      break;
    case 'link': {
      const rel = relTokens(node);
      let kind = '';
      if (rel.includes('icon') || rel.includes('shortcut') || rel.includes('apple-touch-icon')) kind = 'favicon';
      else if (rel.includes('stylesheet')) kind = 'stylesheet';
      else if (rel.includes('manifest')) kind = 'manifest';
      else if (rel.includes('modulepreload')) kind = 'module';
      else if (rel.includes('preload') || rel.includes('prefetch')) kind = attrValue(node, 'as') || 'preload';
      if (kind) pushAttribute(requests, node, path, 'href', kind, documentBaseUrl);
      break;
    }
    case 'style':
      pushCssUrls(requests, textContent(node), path, tag, 'text', documentBaseUrl);
      break;
    default:
      break;
  }

  const inlineStyle = attrValue(node, 'style');
  if (inlineStyle) pushCssUrls(requests, inlineStyle, path, tag, 'style', documentBaseUrl);
}

function walk(node, path, documentBaseUrl, requests) {
  if (!node || typeof node !== 'object') return;
  if (typeof node.tagName === 'string') visitElement(requests, node, path, documentBaseUrl);
  const children = Array.isArray(node.childNodes) ? node.childNodes : [];
  for (let index = 0; index < children.length; index += 1) {
    walk(children[index], `${path}.${index}`, documentBaseUrl, requests);
  }
  if (node.content) walk(node.content, `${path}.content`, documentBaseUrl, requests);
}

function firstBaseHref(document) {
  let baseHref = '';
  const find = (node) => {
    if (!node || typeof node !== 'object' || baseHref) return;
    if (safeString(node.tagName).toLowerCase() === 'base') baseHref = attrValue(node, 'href');
    const children = Array.isArray(node.childNodes) ? node.childNodes : [];
    for (const child of children) find(child);
  };
  find(document);
  return baseHref;
}

export function collectAssetRequests(document, documentUrl, externalStylesheets = []) {
  const baseHref = firstBaseHref(document);
  // The browser host resolves this pair with its standards-aware URL library.
  const documentBaseUrl = baseHref || safeString(documentUrl);
  const requests = [];
  walk(document, 'root', documentBaseUrl, requests);

  for (let index = 0; index < externalStylesheets.length; index += 1) {
    const sheet = externalStylesheets[index] || {};
    pushCssUrls(
      requests,
      sheet.css,
      `externalStylesheet.${index}`,
      'link',
      'stylesheet',
      safeString(sheet.resolvedUrl) || documentBaseUrl,
    );
  }

  const kindCounts = Object.create(null);
  for (const request of requests) {
    kindCounts[request.kind] = Number(kindCounts[request.kind] || 0) + 1;
  }
  return {
    backend: ASSET_BACKEND,
    baseHref: baseHref || null,
    requests,
    requestCount: requests.length,
    kindCounts,
  };
}
