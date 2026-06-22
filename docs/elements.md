# HTML 元素支持与 Demo 清单

实现来源：[`src/gpu_ui/html/node.rs`](../src/gpu_ui/html/node.rs)

Demo 来源：[demoui.html](./demoui.html)

`HtmlTag` 覆盖当前节点模型识别的全部标准 HTML 元素。未识别的标签名称会保留为 `HtmlTag::Custom`，以支持自定义元素。`ElementKind::Element` 为尚无专用渲染逻辑的标签提供通用子节点容器；因此“可表示”不等于已经实现该元素的全部浏览器行为。

## `HtmlTag` 标准元素清单

共 **113** 个标签，按字母顺序列出：

| 范围 | 标签 |
|------|------|
| A-B | `a`, `abbr`, `address`, `area`, `article`, `aside`, `audio`, `b`, `base`, `bdi`, `bdo`, `blockquote`, `body`, `br`, `button` |
| C-D | `canvas`, `caption`, `cite`, `code`, `col`, `colgroup`, `data`, `datalist`, `dd`, `del`, `details`, `dfn`, `dialog`, `div`, `dl`, `dt` |
| E-H | `em`, `embed`, `fieldset`, `figcaption`, `figure`, `footer`, `form`, `h1`, `h2`, `h3`, `h4`, `h5`, `h6`, `head`, `header`, `hgroup`, `hr`, `html` |
| I-M | `i`, `iframe`, `img`, `input`, `ins`, `kbd`, `label`, `legend`, `li`, `link`, `main`, `map`, `mark`, `menu`, `meta`, `meter` |
| N-P | `nav`, `noscript`, `object`, `ol`, `optgroup`, `option`, `output`, `p`, `picture`, `pre`, `progress` |
| Q-S | `q`, `rp`, `rt`, `ruby`, `s`, `samp`, `script`, `search`, `section`, `select`, `selectedcontent`, `slot`, `small`, `source`, `span`, `strong`, `style`, `sub`, `summary`, `sup` |
| T-W | `table`, `tbody`, `td`, `template`, `textarea`, `tfoot`, `th`, `thead`, `time`, `title`, `tr`, `track`, `u`, `ul`, `var`, `video`, `wbr` |

其中 metadata 元素可通过 `HtmlTag::is_metadata()` 判断，void 元素可通过 `HtmlTag::is_void()` 判断。标签解析不区分 ASCII 大小写，例如 `TextArea` 会规范化为 `textarea`。

## Demo 使用的元素

以下章节汇总 demo 中实际出现的 HTML 元素，包括主文档、`<iframe srcdoc>` 内嵌页面以及 `<svg>` 内的 SVG 元素。

---

## 文档结构

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<!DOCTYPE html>` | 文档类型声明 |
| `<html>` | 根元素（主文档 + iframe 内各 1 处） |
| `<head>` | 文档元信息容器 |
| `<meta>` | 字符集 `charset="UTF-8"` |
| `<title>` | 页面标题 |
| `<body>` | 文档主体（主文档 + iframe 内各 1 处） |

---

## 章节与文本

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<h1>` | 主标题 |
| `<h2>` | 各区块小标题（Buttons and Inputs、Date/Time Inputs、Table 等） |
| `<p>` | 段落、表单字段分组、说明文字 |
| `<b>` | 粗体文本 |
| `<i>` | 斜体文本 |
| `<hr>` | 水平分隔线 |

---

## 链接与列表

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<a>` | 导航链接（`/helloworld`、`/svg-demo`） |
| `<ol>` | 有序列表 |
| `<ul>` | 无序列表 |
| `<li>` | 列表项 |

---

## 折叠内容

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<details>` | 可展开/折叠区块（树形目录、表单、表格等） |
| `<summary>` | 折叠区块标题行 |

---

## 表单

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<form>` | 表单容器 |
| `<label>` | 输入控件标签 |
| `<input>` | 各类输入控件（见下表） |
| `<select>` | 下拉选择 |
| `<option>` | 下拉选项 |
| `<textarea>` | 多行文本输入 |
| `<button>` | 按钮（见下表） |

### `<input>` 的 `type`

| type | 出现位置 |
|------|----------|
| `checkbox` | 树形目录 summary、订阅 newsletter |
| `text` | 表单 name、底部单行输入、dialog 输入、iframe 内嵌输入 |
| `password` | 表单 password |
| `time` | 时间选择 |
| `date` | 日期选择 |
| `month` | 月份选择 |
| `week` | 周选择 |
| `datetime-local` | 本地日期时间 |
| `radio` | 颜色单选（Red / Blue / Green） |

### `<button>` 的 `type`

| type | 用途 |
|------|------|
| `submit` | 提交表单 |
| `reset` | 重置表单 |
| `button` | 普通按钮（含 dialog OK/Cancel） |

---

## 表格

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<table>` | 数据表格 |
| `<tr>` | 表格行 |
| `<th>` | 表头单元格 |
| `<td>` | 数据单元格 |

---

## 媒体与嵌入

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<img>` | 内联 base64 PNG 图片 |
| `<canvas>` | 画布占位 |
| `<iframe>` | 嵌入带 srcdoc 的子 HTML 文档 |
| `<svg>` | 矢量图容器 |

### `<svg>` 内元素

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<rect>` | 圆角矩形背景 |
| `<circle>` | 圆形 |
| `<path>` | 折线路径 |

---

## 语义与交互

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<dialog>` | 浮动对话框（主文档 + iframe 内各 1 处） |
| `<progress>` | 下载进度条 |
| `<meter>` | 技能等级指示 |
| `<footer>` | 页脚 |

---

## 容器

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<div>` | 树形目录展开后的内容容器 |

---

## Demo 专用 / 自定义元素

以下标签在 demo 中使用了 Solara 专用行为：

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<search>` | 标准 HTML 搜索容器；demo 额外将 `value`、`width` 解释为搜索框属性 |
| `<color>` | Solara 自定义颜色占位元素 |
| `<slider>` | Solara 自定义滑块元素 |

---

## Demo 覆盖汇总

`demoui.html` 现包含上文列出的全部 **113** 个标准 HTML 标签。加上 SVG 的 `svg`、`rect`、`circle`、`path` 和 Solara 自定义的 `color`、`slider`，共 **119** 个 distinct 标签名，不含 `<!DOCTYPE html>`。

`demoui.css` 的基础选择器同步覆盖全部 113 个标准标签，并按 metadata、块级内容、文本语义、列表、媒体、表格、表单和交互元素提供默认样式。

---

## iframe `srcdoc` 内额外出现的元素

内嵌 HTML 使用 `html`, `body`, `h2`, `p`, `input`, `dialog`, `label`, `button`，这些元素也全部出现在主文档中。

其余标准 HTML 元素以及 SVG、自定义元素仅出现在主文档中。
