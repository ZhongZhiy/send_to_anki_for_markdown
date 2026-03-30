use lazy_static::lazy_static;
use regex::Regex;

/// A raw "card section" extracted from the source Markdown.
///
/// The section is defined by a level-2 heading `## ...` followed by its body text
/// until the next `## ...` or end of file.
pub struct RawCard {
    pub title: String,
    pub content: String,
}

/// Deck metadata extracted from the Markdown file.
pub struct RawDeck {
    pub name: String,
    pub cards: Vec<RawCard>,
}

/// Parse a Markdown document into a deck + card sections.
///
/// - The first level-1 heading `# ...` is used as the deck name. If not found, `Default` is used.
/// - Each level-2 heading `## ...` starts a new card section.
pub fn parse_markdown(markdown: &str) -> anyhow::Result<RawDeck> {
    lazy_static! {
        static ref RE_H1: Regex = Regex::new(r"(?m)^#\s+(.+)$").unwrap();
    }

    let deck_name = match RE_H1.captures(markdown) {
        Some(caps) => caps.get(1).unwrap().as_str().trim().to_string(),
        None => "Default".to_string(),
    };

    // Split by `## ` at the start of a line
    let mut cards = Vec::new();
    let mut current_title = String::new();
    let mut current_content = String::new();
    let mut in_card = false;

    for line in markdown.lines() {
        // Keep this lightweight: the full Markdown sanitation happens in the renderer.
        if line.starts_with(":::") || line.starts_with("[^") {
            eprintln!("Warning: Unsupported extended syntax detected and will be treated as plain text: {}", line);
        }

        if let Some(stripped) = line.strip_prefix("## ") {
            if in_card {
                cards.push(RawCard {
                    title: current_title.clone(),
                    content: current_content.trim().to_string(),
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

    if in_card {
        cards.push(RawCard {
            title: current_title,
            content: current_content.trim().to_string(),
        });
    }

    Ok(RawDeck {
        name: deck_name,
        cards,
    })
}
