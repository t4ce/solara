import { BLOCK_TAGS } from './htmlDefaults.mjs';

// Renderer-neutral user-agent defaults. Solara remains responsible for turning
// these computed values into pixels; this module never emits paint operations.
const FONT_PX = 14;
const FONT_COLOR = '#1f1f1f';

const TAG_STYLE_DEFAULTS = Object.freeze({
  a: Object.freeze({ color: '#0000bf', display: 'inline-block' }),
  b: Object.freeze({ fontWeight: 'bold' }),
  strong: Object.freeze({ fontWeight: 'bold' }),
  em: Object.freeze({ fontStyle: 'italic' }),
  i: Object.freeze({ fontStyle: 'italic' }),
  h1: Object.freeze({ fontSizePx: 30, lineHeightPx: 38, fontWeight: 'bold' }),
  h2: Object.freeze({ fontSizePx: 22, lineHeightPx: 30, fontWeight: 'bold' }),
  h3: Object.freeze({ fontSizePx: 18, lineHeightPx: 26, fontWeight: 'bold' }),
  h4: Object.freeze({ fontSizePx: 15, lineHeightPx: 22, fontWeight: 'bold' }),
  h5: Object.freeze({ fontSizePx: 12, lineHeightPx: 20, fontWeight: 'bold' }),
  h6: Object.freeze({ fontSizePx: 10, lineHeightPx: 20, fontWeight: 'bold' }),
  button: Object.freeze({ display: 'inline-block' }),
  img: Object.freeze({ display: 'inline-block' }),
  canvas: Object.freeze({ display: 'inline-block' }),
  iframe: Object.freeze({ display: 'inline-block' }),
});

export const INHERITED_STYLE_FIELDS = Object.freeze([
  'color',
  'fontSizePx',
  'lineHeightPx',
  'fontWeight',
  'fontStyle',
  'textAlign',
  'whiteSpace',
]);

export function defaultDisplayForTag(tagName) {
  const tag = String(tagName || '').toLowerCase();
  if (!tag) return 'inline';
  if (tag === 'li') return 'list-item';
  if (tag === 'img' || tag === 'canvas' || tag === 'iframe') return 'inline-block';
  if (BLOCK_TAGS.has(tag)) return 'block';
  return 'inline';
}

export function createComputedStyle(tagName = '', path = '', parentStyle = null) {
  const tag = String(tagName || '').toLowerCase();
  const style = {
    path: String(path || ''),
    tag,
    display: defaultDisplayForTag(tag),
    color: FONT_COLOR,
    backgroundColor: 'transparent',
    fontSizePx: FONT_PX,
    lineHeightPx: 18,
    fontWeight: 'normal',
    fontStyle: 'normal',
    textAlign: 'left',
    whiteSpace: tag === 'pre' ? 'pre' : 'normal',
    marginLeftPx: 0,
    marginTopPx: 0,
    marginRightPx: 0,
    marginBottomPx: 0,
    paddingLeftPx: 0,
    paddingTopPx: 0,
    paddingRightPx: 0,
    paddingBottomPx: 0,
    borderWidthPx: 0,
    borderColor: 'transparent',
    authoredProperties: [],
    source: {
      matchedRules: [],
      inline: false,
    },
  };

  if (parentStyle && typeof parentStyle === 'object') {
    for (const key of INHERITED_STYLE_FIELDS) {
      if (parentStyle[key] != null) style[key] = parentStyle[key];
    }
  }

  const defaults = TAG_STYLE_DEFAULTS[tag];
  if (defaults) Object.assign(style, defaults);
  return style;
}
