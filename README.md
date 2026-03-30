# anki-cli

把本地 Markdown 转成 Anki 卡片，并通过 Anki-Connect 批量写入指定牌组。

## 前置条件

- 安装 Anki（2.1+）
- 安装并启用 Anki-Connect 插件（代码：2055492159）
- 运行 Anki（Anki-Connect 默认监听 `http://127.0.0.1:8765`）

## 安装与运行

开发运行：

```bash
cargo run -- --file test.md --tags demo
```

编译 release：

```bash
cargo build --release
```

## CLI 用法

```bash
anki-cli --file <path.md> --tags tag1,tag2 [--anki-url http://127.0.0.1:8765] [--dry-run] [--print-json]
```

- `--anki-url`：Anki-Connect 地址，默认 `http://127.0.0.1:8765`
- `--tags`：逗号分隔标签列表
- `--dry-run`：不写入 Anki，只生成 notes
- `--print-json`：打印最终发给 Anki-Connect 的 JSON

## Markdown 结构规则

### 文件编码（中文）

推荐使用 UTF-8 保存 Markdown（可带 BOM）。

在 Windows 上如果你的笔记是 GBK/ANSI 或 UTF-16（部分编辑器默认），本工具会自动尝试识别并转换为 UTF-8（优先 UTF-8/UTF-16 BOM，其次 UTF-8，最后 GBK best-effort）。

### 牌组名

文件中第一个一级标题 `# ...` 作为牌组名；没有则使用 `Default`。

### 卡片分段

每个二级标题 `## ...` 开始一个“卡片段”。

### 多行正面 / 多行背面（Basic）

在 `## ...` 的内容里支持多行正反面，分隔优先级如下：

1. 使用单独一行的 `---` 作为显式分隔：`---` 上方属于正面，下方属于背面（推荐）。
2. 若没有 `---`：用“第一次空行”进行切分，空行上方属于正面，空行下方属于背面。
3. 若没有空行：标题(`## ...`) + 段落内容整体作为正面，背面为空。

### 填空（Cloze）

当正面或背面中出现以下任意一种语法，会自动使用 Anki 的 `Cloze` 模型：

- `==高亮==`：会自动转换成 `{{cN::高亮}}`（自动续号）
- `{{c1::文本}}`：保留原有编号，并与 `==...==` 自动续号衔接

映射方式：

- `Text` 字段：正面（标题 + 正面多行）
- `Back Extra` 字段：背面多行（如果存在）

## Markdown 渲染能力

- 表格、粗体、删除线、引用、嵌套列表、任务列表
- 行内代码与代码块（fenced code）：
  - 代码块会使用 syntect 进行语法高亮，并统一暗色背景与等宽字体

## 数学公式（MathJax）

Anki 官方更推荐 `\(...\)`（行内）与 `\[...\]`（行间）。
为了兼容你现有笔记，本工具会做如下转换：

- `$...$` → `\(...\)`（行内公式）
- `$$...$$` → `\[...\]`（行间公式，支持单行与多行）

转换会避开 fenced code block，避免误处理代码。

## Obsidian 风格语法（部分）

- Callout：`>[!note]` / `>[!warning]` / `>[!tip]` / `>[!important]` 等（会被规范化成加粗标题形式）
- Wiki link：`[[Page]]`、`[[Page|Alias]]`（渲染为链接风格文本）
- 标签：`#tag`（渲染为 tag 样式文本）
- 任务列表简写：`- [] xxx` / `* [] xxx`（归一成 `- [ ] xxx` 以便正常渲染）

## 不支持/会警告的语法

遇到以下扩展语法会发出警告并跳过对应行（避免渲染异常）：

- `:::`（常见于部分 Markdown 扩展/Admonition）
- `[^...]`（脚注）

## 开发

```bash
cargo fmt
cargo clippy -- -D warnings
```
