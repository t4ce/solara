import { parse as parseHtml } from 'parse5';
import {
  collectWidgetStats,
  domToWidgets,
} from './widlib/index.mjs';
import { buildCssStyleRefIndex } from './truesurfer/css.mjs';
import { collectAssetRequests } from './truesurfer/assets.mjs';
import { extractDocumentArtifacts } from './truesurfer/truesurfer_extract.mjs';

const SCHEMA_NAME = 'rustqjsdom.artifact';
const SCHEMA_VERSION = 2;

function safeString(value) {
  return value === null || value === undefined ? '' : String(value);
}

function normalizeAttribute(attribute) {
  const out = {
    name: safeString(attribute && attribute.name),
    value: safeString(attribute && attribute.value),
  };
  if (attribute && attribute.namespace) out.namespace = safeString(attribute.namespace);
  if (attribute && attribute.prefix) out.prefix = safeString(attribute.prefix);
  return out;
}

// Parse5 nodes point back to their parents. This converts that graph into an
// acyclic, JSON-safe document while retaining namespaces, attributes, template
// contents, doctypes, comments, and text nodes.
function normalizeNode(node) {
  if (!node || typeof node !== 'object') return null;

  const out = { nodeName: safeString(node.nodeName) };
  if (node.tagName != null) out.tagName = safeString(node.tagName);
  if (node.namespaceURI != null) out.namespaceURI = safeString(node.namespaceURI);
  if (Array.isArray(node.attrs) && node.attrs.length > 0) {
    out.attrs = node.attrs.map(normalizeAttribute);
  }
  if (node.value != null) out.value = safeString(node.value);
  if (node.data != null) out.data = safeString(node.data);
  if (node.name != null) out.name = safeString(node.name);
  if (node.publicId != null) out.publicId = safeString(node.publicId);
  if (node.systemId != null) out.systemId = safeString(node.systemId);
  if (node.mode != null) out.mode = safeString(node.mode);
  if (node.__trueosStyleRef != null) out.styleRef = Math.max(0, Number(node.__trueosStyleRef) || 0);

  if (Array.isArray(node.childNodes) && node.childNodes.length > 0) {
    out.children = node.childNodes.map(normalizeNode).filter(Boolean);
  }
  if (node.content && typeof node.content === 'object') {
    out.content = normalizeNode(node.content);
  }
  return out;
}

function attrValue(node, name) {
  const attrs = Array.isArray(node && node.attrs) ? node.attrs : [];
  const wanted = safeString(name).toLowerCase();
  for (const attr of attrs) {
    if (safeString(attr && attr.name).toLowerCase() === wanted) return safeString(attr.value);
  }
  return '';
}

function walkElements(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (typeof node.tagName === 'string') visit(node);
  const children = Array.isArray(node.childNodes) ? node.childNodes : [];
  for (const child of children) walkElements(child, visit);
  if (node.content) walkElements(node.content, visit);
}

function collectExternalStylesheets(document, documentUrl) {
  let baseHref = '';
  const links = [];
  walkElements(document, (node) => {
    const tag = safeString(node.tagName).toLowerCase();
    if (tag === 'base' && !baseHref) {
      baseHref = attrValue(node, 'href');
      return;
    }
    if (tag !== 'link') return;
    const rel = attrValue(node, 'rel').toLowerCase().split(/\s+/).filter(Boolean);
    const href = attrValue(node, 'href');
    if (href && rel.includes('stylesheet')) links.push(href);
  });

  const load = globalThis.__rustQjsDomLoadStylesheet;
  return links.map((href) => {
    if (typeof load !== 'function') {
      return { href, resolvedUrl: '', css: '', error: 'stylesheet loader is not installed' };
    }
    try {
      const result = load(safeString(documentUrl), baseHref || null, href);
      return {
        href,
        resolvedUrl: safeString(result && result.resolvedUrl),
        css: safeString(result && result.css),
        error: safeString(result && result.error),
      };
    } catch (error) {
      return {
        href,
        resolvedUrl: '',
        css: '',
        error: error && error.stack ? String(error.stack) : safeString(error),
      };
    }
  });
}

function countLines(text) {
  if (text.length === 0) return 1;
  let lines = 1;
  for (let index = 0; index < text.length; index += 1) {
    if (text.charCodeAt(index) === 10) lines += 1;
  }
  return lines;
}

function compactExtractedArtifacts(parsed) {
  return {
    title: parsed.title || null,
    faviconHref: parsed.faviconHref || null,
    shellHtml: parsed.shellHtml,
    bodyHtml: parsed.bodyHtml,
    bodyHierarchy: parsed.bodyHierarchy,
    bodyHierarchySummary: parsed.bodyHierarchySummary,
    styleCount: parsed.styleCount,
    styleBytes: parsed.styleBytes,
    scriptCount: parsed.scriptCount,
    scriptBytes: parsed.scriptBytes,
    styles: parsed.styles,
    scripts: parsed.scripts,
  };
}

globalThis.__rustQjsDomParseJson = function parseDomJson(inputHtml, inputUrl, inputBytes) {
  const html = safeString(inputHtml);
  const url = safeString(inputUrl);
  const startedAt = Date.now();
  const parsedDocument = parseHtml(html);
  const parse5Ms = Date.now() - startedAt;
  const cssStartedAt = Date.now();
  const externalStylesheets = collectExternalStylesheets(parsedDocument, url);
  const styleIndex = buildCssStyleRefIndex(parsedDocument, { externalStylesheets });
  const lightningCssMs = Date.now() - cssStartedAt;
  const assetIndex = collectAssetRequests(parsedDocument, url, externalStylesheets);
  const widgetTree = domToWidgets(parsedDocument);
  const extracted = extractDocumentArtifacts(html, { styleIndex, styleIndexMs: lightningCssMs });

  return {
    schema: SCHEMA_NAME,
    schemaVersion: SCHEMA_VERSION,
    source: {
      url,
      bytes: Math.max(0, Number(inputBytes) || 0),
      lines: countLines(html),
    },
    timings: {
      parse5Ms,
      lightningCssMs,
      totalMs: Date.now() - startedAt,
    },
    document: normalizeNode(parsedDocument),
    styleIndex,
    assetIndex,
    widgetTree,
    widgetStats: collectWidgetStats(widgetTree),
    extracted: compactExtractedArtifacts(extracted),
  };
};

globalThis.__rustQjsDomReady = true;
