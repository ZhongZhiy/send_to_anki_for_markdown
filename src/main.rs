mod anki;
mod parser;
mod render;

use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool to parse Markdown into Anki flashcards via Anki-Connect", long_about = None)]
struct Args {
    /// Path to the Markdown file
    #[arg(short, long)]
    file: PathBuf,

    /// Tags to add to the generated cards (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    tags: Vec<String>,

    /// Anki-Connect URL
    #[arg(long, default_value = "http://127.0.0.1:8765")]
    anki_url: String,

    /// Do not send anything to Anki; only build the request
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Print the final JSON request payload
    #[arg(long, default_value_t = false)]
    print_json: bool,
}

fn split_front_back(title: &str, content: &str) -> (String, String) {
    // Supports multiline front/back.
    //
    // Priority:
    // 1) Explicit separator line: `---` (outside code fences).
    // 2) First blank line in the section body.
    // 3) Fallback to `title` as front and full `content` as back.
    let mut in_fence = false;
    let mut front_lines: Vec<String> = Vec::new();
    let mut back_lines: Vec<String> = Vec::new();
    let mut found_split = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }

        if !in_fence && trimmed == "---" && !found_split {
            found_split = true;
            continue;
        }

        if found_split {
            back_lines.push(line.to_string());
        } else {
            front_lines.push(line.to_string());
        }
    }

    let (front_body, back_body) = if found_split {
        (
            front_lines.join("\n").trim().to_string(),
            back_lines.join("\n").trim().to_string(),
        )
    } else {
        let mut f: Vec<String> = Vec::new();
        let mut b: Vec<String> = Vec::new();
        let mut in_front = true;

        for line in content.lines() {
            if in_front && line.trim().is_empty() {
                in_front = false;
                continue;
            }
            if in_front {
                f.push(line.to_string());
            } else {
                b.push(line.to_string());
            }
        }

        (
            f.join("\n").trim().to_string(),
            b.join("\n").trim().to_string(),
        )
    };

    let mut front_md = title.trim().to_string();
    let (front_body, back_body) = if !found_split && back_body.is_empty() {
        (String::new(), content.trim().to_string())
    } else {
        (front_body, back_body)
    };

    if !front_body.is_empty() {
        if !front_md.is_empty() {
            front_md.push_str("\n\n");
        }
        front_md.push_str(&front_body);
    }

    let back_md = back_body;
    (front_md.trim().to_string(), back_md.trim().to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let markdown = fs::read_to_string(&args.file)?;
    let raw_deck = parser::parse_markdown(&markdown)?;

    println!("Deck: {}", raw_deck.name);
    println!("Found {} cards", raw_deck.cards.len());

    let mut anki_notes = Vec::new();

    for raw_card in raw_deck.cards {
        let (front_md, back_md) = split_front_back(&raw_card.title, &raw_card.content);
        let is_cloze = render::has_cloze(&front_md) || render::has_cloze(&back_md);

        let mut fields = HashMap::new();
        let model_name;

        if is_cloze {
            model_name = "Cloze".to_string();
            // Support multiline cloze content:
            // - Text: front (title + optional body)
            // - Back Extra: remaining content (if any)
            let split_token = "\n\nANKI_CLI_SPLIT\n\n";
            let combined_md = format!("{}{}{}", front_md, split_token, back_md);
            let combined_with_clozes = render::convert_highlights_to_clozes(&combined_md);
            let (front_converted, back_converted) =
                match combined_with_clozes.split_once(split_token.trim()) {
                    Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
                    None => (combined_with_clozes, String::new()),
                };

            let text_html = render::render_markdown_to_html(&front_converted);
            let back_extra_html = if back_converted.is_empty() {
                String::new()
            } else {
                render::render_markdown_to_html(&back_converted)
            };
            fields.insert("Text".to_string(), text_html);
            fields.insert("Back Extra".to_string(), back_extra_html);
        } else {
            model_name = "Basic".to_string();
            let front_html = render::render_markdown_to_html(&front_md);
            let back_html = render::render_markdown_to_html(&back_md);
            fields.insert("Front".to_string(), front_html);
            fields.insert("Back".to_string(), back_html);
        }

        anki_notes.push(anki::AnkiNote {
            deck_name: raw_deck.name.clone(),
            model_name,
            fields,
            tags: args.tags.clone(),
        });
    }

    if anki_notes.is_empty() {
        println!("No cards to add.");
        return Ok(());
    }

    if args.dry_run {
        println!("Dry-run: built {} notes (not sent).", anki_notes.len());
        anki::add_notes(&args.anki_url, anki_notes, args.print_json, true).await?;
        return Ok(());
    }

    println!("Sending {} notes to Anki...", anki_notes.len());
    let result = anki::add_notes(&args.anki_url, anki_notes, args.print_json, false).await?;
    println!("Successfully added notes to Anki! ({})", result.len());

    Ok(())
}
