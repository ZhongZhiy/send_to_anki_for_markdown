use lazy_static::lazy_static;
use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag};
use regex::Regex;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

lazy_static! {
    static ref RE_CLOZE_EXISTING: Regex = Regex::new(r"\{\{c(\d+)::.*?\}\}").unwrap();
    static ref RE_HIGHLIGHT: Regex = Regex::new(r"==([^=]+)==").unwrap();
    static ref RE_PRE_OPEN: Regex = Regex::new(r"(?s)\A<pre[^>]*>").unwrap();
    static ref RE_STYLE_ATTR: Regex = Regex::new(r#"\sstyle="[^"]*""#).unwrap();
    static ref RE_CLASS_ATTR: Regex = Regex::new(r#"\sclass="([^"]*)""#).unwrap();
    static ref RE_WIKI_LINK: Regex = Regex::new(r"\[\[([^\[\]]+?)\]\]").unwrap();
    static ref RE_HASH_TAG: Regex = Regex::new(r"(^|\s)#([A-Za-z0-9_\-\/\.]+)").unwrap();
}

/// Convert `==text==` to `{{cN::text}}`.
///
/// If the input already contains `{{c1::...}}`-style clozes, numbering continues from
/// the largest existing index to avoid collisions.
pub fn convert_highlights_to_clozes(text: &str) -> String {
    let mut max_idx = 0;
    for cap in RE_CLOZE_EXISTING.captures_iter(text) {
        if let Ok(idx) = cap[1].parse::<usize>() {
            if idx > max_idx {
                max_idx = idx;
            }
        }
    }

    let mut counter = max_idx + 1;
    let mut result = String::new();
    let mut last_match = 0;

    for cap in RE_HIGHLIGHT.captures_iter(text) {
        let m = cap.get(0).unwrap();
        result.push_str(&text[last_match..m.start()]);
        result.push_str(&format!("{{{{c{}::{}}}}}", counter, &cap[1]));
        counter += 1;
        last_match = m.end();
    }
    result.push_str(&text[last_match..]);
    result
}

fn normalize_syntect_pre(highlighted_html: &str) -> String {
    if let Some(m) = RE_PRE_OPEN.find(highlighted_html) {
        let open_tag = &highlighted_html[m.start()..m.end()];
        let mut open_tag = RE_STYLE_ATTR.replace_all(open_tag, "").into_owned();
        if let Some(caps) = RE_CLASS_ATTR.captures(&open_tag) {
            let existing = caps.get(1).unwrap().as_str();
            if !existing.split_whitespace().any(|c| c == "anki-code") {
                open_tag = RE_CLASS_ATTR
                    .replace(&open_tag, format!(" class=\"{} anki-code\"", existing))
                    .into_owned();
            }
        } else {
            open_tag = open_tag.replacen("<pre", "<pre class=\"anki-code\"", 1);
        }
        return format!("{}{}", open_tag, &highlighted_html[m.end()..]);
    }
    highlighted_html.to_string()
}

fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn normalize_task_list_sugar(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if let Some(stripped) = trimmed.strip_prefix("- [] ") {
        let prefix_len = line.len() - trimmed.len();
        let mut out = String::new();
        out.push_str(&line[..prefix_len]);
        out.push_str("- [ ] ");
        out.push_str(stripped);
        return Some(out);
    }
    if let Some(stripped) = trimmed.strip_prefix("* [] ") {
        let prefix_len = line.len() - trimmed.len();
        let mut out = String::new();
        out.push_str(&line[..prefix_len]);
        out.push_str("* [ ] ");
        out.push_str(stripped);
        return Some(out);
    }
    None
}

fn normalize_callout_sugar(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let patterns = [
        ("note", "Note"),
        ("warning", "Warning"),
        ("tip", "Tip"),
        ("info", "Info"),
        ("abstract", "Abstract"),
        ("quote", "Quote"),
        ("example", "Example"),
        ("success", "Success"),
        ("failure", "Failure"),
        ("bug", "Bug"),
        ("important", "Important"),
    ];
    for (key, label) in patterns {
        let p1 = format!(">[!{}]", key);
        let p2 = format!("> [!{}]", key);
        if lower.starts_with(&p1) || lower.starts_with(&p2) {
            let after = if let Some(idx) = trimmed.find(']') {
                trimmed[idx + 1..].trim_start()
            } else {
                ""
            };
            let mut out = String::new();
            out.push_str("> **");
            out.push_str(label);
            out.push_str(":**");
            if !after.is_empty() {
                out.push(' ');
                out.push_str(after);
            }
            return Some(out);
        }
    }
    None
}

fn convert_inline_dollar_math(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut in_inline_code = false;

    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch == '`' {
            in_inline_code = !in_inline_code;
            out.push(ch);
            i += 1;
            continue;
        }

        if ch == '$' && !in_inline_code {
            if i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                out.push_str("$$");
                i += 2;
                continue;
            }

            let mut j = i + 1;
            let mut found = None;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    break;
                }
                if bytes[j] == b'$' {
                    if j > 0 && bytes[j - 1] == b'\\' {
                        j += 1;
                        continue;
                    }
                    if j + 1 < bytes.len() && bytes[j + 1] == b'$' {
                        j += 1;
                        continue;
                    }
                    found = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(end) = found {
                let inner = &line[i + 1..end];
                out.push_str("\\\\(");
                out.push_str(inner);
                out.push_str("\\\\)");
                i = end + 1;
                continue;
            }
        }

        out.push(ch);
        i += 1;
    }

    out
}

fn convert_dollar_math_to_anki_mathjax(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    let mut in_display_math = false;
    let mut display_lines: Vec<String> = Vec::new();

    for line in markdown.lines() {
        if is_fence_line(line) {
            in_fence = !in_fence;
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if in_fence {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let trimmed = line.trim();
        if in_display_math {
            if trimmed.starts_with("$$") {
                in_display_math = false;
                let expr = display_lines.join("\n");
                out.push_str("\\\\[\n");
                out.push_str(expr.trim());
                out.push_str("\n\\\\]\n");
                display_lines.clear();
                continue;
            }
            display_lines.push(line.to_string());
            continue;
        }

        if trimmed == "$$" {
            in_display_math = true;
            continue;
        }

        if trimmed.starts_with("$$") && trimmed.ends_with("$$") && trimmed.len() > 4 {
            let inner = trimmed
                .trim_start_matches("$$")
                .trim_end_matches("$$")
                .trim();
            out.push_str("\\\\[");
            out.push_str(inner);
            out.push_str("\\\\]\n");
            continue;
        }

        out.push_str(&convert_inline_dollar_math(line));
        out.push('\n');
    }

    if in_display_math {
        out.push_str("$$\n");
        for l in display_lines {
            out.push_str(&l);
            out.push('\n');
        }
    }

    out
}

fn convert_wikilinks_and_tags(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if is_fence_line(line) {
            in_fence = !in_fence;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fence {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Avoid inline code while converting.
        let mut converted = String::new();
        let mut in_inline_code = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '`' {
                in_inline_code = !in_inline_code;
                converted.push(ch);
                i += 1;
                continue;
            }
            if !in_inline_code && ch == '[' && i + 1 < chars.len() && chars[i + 1] == '[' {
                // Wiki link
                let start = i + 2;
                let mut j = start;
                let mut found = None;
                while j + 1 < chars.len() {
                    if chars[j] == ']' && chars[j + 1] == ']' {
                        found = Some(j);
                        break;
                    }
                    j += 1;
                }
                if let Some(end) = found {
                    let inner: String = chars[start..end].iter().collect();
                    let display = if let Some(pos) = inner.find('|') {
                        inner[pos + 1..].trim().to_string()
                    } else {
                        inner.trim().to_string()
                    };
                    converted.push_str(&format!("<span class=\"wikilink\">{}</span>", display));
                    i = end + 2;
                    continue;
                }
            }
            // Tags: boundary + #word
            if !in_inline_code && ch == '#' {
                // Check boundary
                let boundary = i == 0 || chars[i - 1].is_whitespace();
                if boundary {
                    let mut j = i + 1;
                    while j < chars.len()
                        && (chars[j].is_ascii_alphanumeric() || "-_/.".contains(chars[j]))
                    {
                        j += 1;
                    }
                    if j > i + 1 {
                        let tag: String = chars[i..j].iter().collect();
                        converted.push_str(&format!("<span class=\"tag\">{}</span>", tag));
                        i = j;
                        continue;
                    }
                }
            }
            converted.push(ch);
            i += 1;
        }
        out.push_str(&converted);
        out.push('\n');
    }
    out
}

/// Normalize a few common Markdown "sugar" extensions, and convert `$...$` / `$$...$$`
/// math delimiters into Anki's MathJax-friendly `\\( ... \\)` and `\\[ ... \\]` form.
pub fn preprocess_markdown(markdown: &str) -> String {
    let mut out = String::new();

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(":::") || trimmed.starts_with("[^") {
            eprintln!("Warning: Unsupported extended syntax skipped: {}", line);
            continue;
        }

        if let Some(normalized) = normalize_callout_sugar(line) {
            eprintln!("Warning: Callout syntax normalized: {}", line);
            out.push_str(&normalized);
            out.push('\n');
            continue;
        }

        if let Some(normalized) = normalize_task_list_sugar(line) {
            out.push_str(&normalized);
            out.push('\n');
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    let out = convert_dollar_math_to_anki_mathjax(&out);
    convert_wikilinks_and_tags(&out)
}

pub fn render_markdown_to_html(markdown: &str) -> String {
    let markdown = preprocess_markdown(markdown);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(&markdown, options);

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = ts
        .themes
        .get("base16-ocean.dark")
        .or_else(|| ts.themes.get("InspiredGitHub"))
        .unwrap();

    let mut events = Vec::new();
    let mut in_code_block = false;
    let mut code_language = String::new();
    let mut code_content = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(ref kind)) => {
                in_code_block = true;
                if let CodeBlockKind::Fenced(ref lang) = kind {
                    code_language = lang.to_string();
                } else {
                    code_language = String::new();
                }
            }
            Event::End(Tag::CodeBlock(_)) => {
                in_code_block = false;
                let syntax = ss
                    .find_syntax_by_token(&code_language)
                    .unwrap_or_else(|| ss.find_syntax_plain_text());

                let highlighted = highlighted_html_for_string(&code_content, &ss, syntax, theme)
                    .map(|s| normalize_syntect_pre(&s))
                    .unwrap_or_else(|_| {
                        format!(
                            "<pre class=\"anki-code\"><code>{}</code></pre>",
                            code_content
                        )
                    });

                events.push(Event::Html(highlighted.into()));
                code_content.clear();
                code_language.clear();
            }
            Event::Text(ref text) if in_code_block => {
                code_content.push_str(text);
            }
            _ => {
                if !in_code_block {
                    events.push(event);
                }
            }
        }
    }

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());

    let css = r#"
<style>
table { width: 100%; margin: 0.6em 0 1em; border-collapse: separate; border-spacing: 0; border: 1px solid #d0d7de; border-radius: 10px; overflow: hidden; }
th, td { padding: 8px 10px; border-bottom: 1px solid #d0d7de; border-right: 1px solid #d0d7de; vertical-align: top; }
tr:last-child td { border-bottom: 0; }
th:last-child, td:last-child { border-right: 0; }
th { font-weight: 600; }
blockquote { border-left: 4px solid #d0d7de; margin: 0.6em 0; padding: 0.2em 0.8em; color: #57606a; border-radius: 8px; }
code { background-color: #f6f8fa; color: #24292f; padding: 2px 4px; border-radius: 4px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; }
.anki-code { background-color: #0d1117 !important; color: #c9d1d9 !important; border: 1px solid #30363d; border-radius: 10px; padding: 12px 14px; overflow-x: auto; line-height: 1.6; font-size: 14px; tab-size: 4; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace; }
.anki-code code { background-color: transparent; color: inherit; padding: 0; border-radius: 0; font-family: inherit; font-size: inherit; }
.wikilink { color: #0969da; text-decoration: none; border-bottom: 1px dotted #9cbef5; }
.tag { display: inline-block; background: #e9eef9; color: #334e96; border: 1px solid #c8d5f0; border-radius: 6px; padding: 0 6px; margin: 0 2px; font-size: 12px; }
</style>
"#;

    format!("{}{}", css, html_output)
}

pub fn has_cloze(text: &str) -> bool {
    RE_CLOZE_EXISTING.is_match(text) || RE_HIGHLIGHT.is_match(text)
}
