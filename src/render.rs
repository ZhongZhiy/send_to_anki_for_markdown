use regex::{Captures, Regex};
use lazy_static::lazy_static;
use pulldown_cmark::{html, Options, Parser};

lazy_static! {
    // 匹配代码块 ```...``` 和行内代码 `...`
    static ref RE_CODE_BLOCK: Regex = Regex::new(r"(?s)```.*?```|`[^`\n]+`").unwrap();
    // 匹配填空语法 ==内容==
    static ref RE_HIGHLIGHT: Regex = Regex::new(r"==([^=]+)==").unwrap();
    // 匹配已有填空标记 {{c1::...}}
    static ref RE_CLOZE_EXISTING: Regex = Regex::new(r"\{\{c(\d+)::").unwrap();
    // 匹配 GitHub Callouts
    static ref RE_CALLOUT: Regex = Regex::new(
        r"(?s)<blockquote>\s*<p>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*<br\s*/?>?\s*(.*?)</p>\s*</blockquote>"
    ).unwrap();
}

/// 检查是否包含填空（跳过代码块）
pub fn has_cloze(text: &str) -> bool {
    let text_without_code = RE_CODE_BLOCK.replace_all(text, "");
    RE_CLOZE_EXISTING.is_match(&text_without_code) || RE_HIGHLIGHT.is_match(&text_without_code)
}

/// 将 ==内容== 替换为 {{c1::内容}}，自动保护代码块中的 == 运算符
pub fn convert_highlights_to_clozes(text: &str) -> String {
    let mut protected_blocks = Vec::new();

    // 1. 保护代码块
    let text_with_placeholders = RE_CODE_BLOCK.replace_all(text, |caps: &Captures| {
        let code_str = caps.get(0).unwrap().as_str().to_string();
        let placeholder = format!("___PROTECTED_CODE_BLOCK_{}___", protected_blocks.len());
        protected_blocks.push(code_str);
        placeholder
    });

    // 2. 计算现有最大填空序号
    let mut max_idx = 0;
    for cap in RE_CLOZE_EXISTING.captures_iter(&text_with_placeholders) {
        if let Ok(idx) = cap[1].parse::<usize>() {
            if idx > max_idx {
                max_idx = idx;
            }
        }
    }

    let mut counter = max_idx + 1;
    let mut result = String::new();
    let mut last_match = 0;

    // 3. 转换非代码区域的 ==内容==
    for cap in RE_HIGHLIGHT.captures_iter(&text_with_placeholders) {
        let m = cap.get(0).unwrap();
        result.push_str(&text_with_placeholders[last_match..m.start()]);
        result.push_str(&format!("{{{{c{}::{}}}}}", counter, &cap[1]));
        counter += 1;
        last_match = m.end();
    }
    result.push_str(&text_with_placeholders[last_match..]);

    // 4. 还原代码块
    for (i, code_block) in protected_blocks.iter().enumerate() {
        let placeholder = format!("___PROTECTED_CODE_BLOCK_{}___", i);
        result = result.replace(&placeholder, code_block);
    }

    result
}

/// 支持 GitHub 风格的 Callout 提示框
pub fn convert_callouts(html: &str) -> String {
    RE_CALLOUT.replace_all(html, |caps: &Captures| {
        let callout_type = caps[1].to_lowercase();
        let title = &caps[1];
        let content = caps[2].trim();

        format!(
            r#"<div class="callout callout-{}"><div class="callout-title">{}</div><div class="callout-body">{}</div></div>"#,
            callout_type, title, content
        )
    }).into_owned()
}

/// 将 Markdown 转换为 HTML
pub fn render_markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    convert_callouts(&html_output)
}
