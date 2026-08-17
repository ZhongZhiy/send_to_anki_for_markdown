use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    // 匹配 "Tags: ...", "tags: ...", "标签: ..."
    static ref RE_TAGS_PREFIX: Regex = Regex::new(r"(?im)^\s*(?:tags|tag|标签)\s*[:：]\s*(.+)$").unwrap();
    // 匹配单个 #tag（必须有字符，不能是 Markdown 的 '# 标题'）
    static ref RE_HASHTAG: Regex = Regex::new(r"#([\w\-\u4e00-\u9fa5]+)").unwrap();
    // 匹配一整行全是 hashtag 的情况，例如 "   #rust #algorithm   "
    static ref RE_TAGS_ONLY_LINE: Regex = Regex::new(r"^\s*(?:#[\w\-\u4e00-\u9fa5]+\s*)+$").unwrap();
    // 匹配 ___ 分割线 (3个或以上下划线)
    static ref RE_SPLIT_CARD: Regex = Regex::new(r"(?m)^\s*_{3,}\s*$").unwrap();
}

#[derive(Debug, Clone)]
pub struct RawCard {
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RawDeck {
    pub name: String,
    pub cards: Vec<RawCard>,
}

/// 从标题中提取 tags 并返回 (清洗后的标题, tags 列表)
pub fn extract_tags_from_title(title: &str) -> (String, Vec<String>) {
    let mut tags = Vec::new();
    for cap in RE_HASHTAG.captures_iter(title) {
        tags.push(cap[1].to_string());
    }
    // 从标题中清除 #tag，只保留干净的题目标题
    let cleaned_title = RE_HASHTAG.replace_all(title, "").trim().to_string();
    (cleaned_title, tags)
}

/// 从正文中提取 tags，并返回 (tags 列表, 剔除 tags 行后的干净正文)
pub fn extract_tags_from_content(content: &str) -> (Vec<String>, String) {
    let mut tags = Vec::new();
    let mut cleaned_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // 1. 如果是 Markdown 标题行（如 # 标题、## 标题），保持原样跳过
        if trimmed.starts_with("# ") || trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            cleaned_lines.push(line);
            continue;
        }

        // 2. 匹配 "Tags: ..." 声明行（支持 Tags: a, b、Tags: [a, b]、Tags: #a #b）
        if let Some(caps) = RE_TAGS_PREFIX.captures(line) {
            let tags_str = caps.get(1).unwrap().as_str().trim();
            // 提取所有标签并过滤引号、括号和 #
            for part in tags_str.split(|c: char| c == ',' || c == '，' || c == ';' || c == '；' || c.is_whitespace()) {
                let clean = part.trim().trim_matches(|c| c == '"' || c == '\'' || c == '#' || c == '[' || c == ']');
                if !clean.is_empty() {
                    tags.push(clean.to_string());
                }
            }
            // 👈 彻底丢弃此行，不放入 cleaned_lines，避免正文中出现
            continue;
        }

        // 3. 匹配一整行全是 "#tag1 #tag2" 的情况
        if RE_TAGS_ONLY_LINE.is_match(trimmed) {
            for cap in RE_HASHTAG.captures_iter(trimmed) {
                tags.push(cap[1].to_string());
            }
            // 👈 彻底丢弃此行
            continue;
        }

        // 4. 普通正文行原样保留
        cleaned_lines.push(line);
    }

    (tags, cleaned_lines.join("\n").trim().to_string())
}

/// 解析 Markdown
pub fn parse_markdown(markdown: &str) -> RawDeck {
    let mut deck_name = "Default".to_string();

    // 查找 # Deck Name
    for line in markdown.lines() {
        if let Some(stripped) = line.strip_prefix("# ") {
            deck_name = stripped.trim().to_string();
            break;
        }
    }

    let mut cards = Vec::new();
    let mut current_title = String::new();
    let mut current_content = String::new();
    let mut in_card = false;

    for line in markdown.lines() {
        if line.starts_with(":::") || line.starts_with("[^") {
            eprintln!("Warning: Unsupported extended syntax detected: {}", line);
        }

        if let Some(stripped) = line.strip_prefix("## ") {
            if in_card {
                let (title_clean, mut card_tags) = extract_tags_from_title(&current_title);
                let (content_tags, cleaned_content) = extract_tags_from_content(&current_content);
                card_tags.extend(content_tags);
                card_tags.sort();
                card_tags.dedup();

                cards.push(RawCard {
                    title: title_clean,
                    content: cleaned_content,
                    tags: card_tags,
                });
                current_content.clear();
            }
            current_title = stripped.trim().to_string();
            in_card = true;
        } else if in_card {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // 处理最后一张卡片
    if in_card {
        let (title_clean, mut card_tags) = extract_tags_from_title(&current_title);
        let (content_tags, cleaned_content) = extract_tags_from_content(&current_content);
        card_tags.extend(content_tags);
        card_tags.sort();
        card_tags.dedup();

        cards.push(RawCard {
            title: title_clean,
            content: cleaned_content,
            tags: card_tags,
        });
    }

    RawDeck {
        name: deck_name,
        cards,
    }
}

/// 使用 ___ 切割正面与背面
pub fn split_front_back(title: &str, content: &str) -> (String, String) {
    if let Some(mat) = RE_SPLIT_CARD.find(content) {
        let front_body = content[..mat.start()].trim();
        let back_body = content[mat.end()..].trim();

        let front = if front_body.is_empty() {
            title.to_string()
        } else {
            format!("{}\n\n{}", title, front_body)
        };

        (front, back_body.to_string())
    } else {
        (title.to_string(), content.trim().to_string())
    }
}
