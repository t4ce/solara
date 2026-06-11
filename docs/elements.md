# demoui.html 使用的 HTML 元素

来源：[demoui.html](./demoui.html)

本文档汇总该 demo 页面中出现的全部 HTML 元素，含主文档、`<iframe srcdoc>` 内嵌页面，以及 `<svg>` 内的 SVG 元素。

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

## 自定义 / 非标准元素

以下标签出现在 demo 中，并非全部浏览器原生支持，可能为 Solara 实验性组件：

| 元素 | 用途（在 demo 中） |
|------|-------------------|
| `<search>` | 搜索框占位（带 `value`、`width` 属性） |
| `<color>` | 颜色相关占位（空标签，位于 `<details>` 内） |
| `<slider>` | 滑块占位（带 `value`） |

---

## 元素清单（按字母序）

共 **44** 个 distinct 标签名（含 SVG 子元素与自定义标签，不含 `<!DOCTYPE html>`）：

`a`, `b`, `body`, `button`, `canvas`, `circle`, `color`, `details`, `dialog`, `div`, `footer`, `form`, `h1`, `h2`, `head`, `hr`, `html`, `i`, `iframe`, `img`, `input`, `label`, `li`, `meta`, `meter`, `ol`, `option`, `p`, `path`, `progress`, `rect`, `search`, `select`, `slider`, `summary`, `svg`, `table`, `td`, `textarea`, `th`, `title`, `tr`, `ul`

---

## iframe `srcdoc` 内额外出现的元素

内嵌 HTML 与主文档重叠的元素：`html`, `body`, `h2`, `p`, `input`, `dialog`, `label`, `button`。

内嵌文档未出现、仅主文档有的元素：`search`, `color`, `slider`, `svg` 子元素, `canvas`, `table` 系列, `progress`, `meter`, `footer`, `img`, `ol`/`ul`/`li`, `a`, `h1`, `details`/`summary` 树形结构等。
