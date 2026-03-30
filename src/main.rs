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

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let mut zero_odd = 0usize;
    let mut checked = 0usize;
    for (idx, b) in bytes.iter().take(2048).enumerate() {
        if idx % 2 == 1 {
            checked += 1;
            if *b == 0 {
                zero_odd += 1;
            }
        }
    }
    checked > 0 && (zero_odd * 100 / checked) >= 30
}

fn looks_like_utf16_be(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let mut zero_even = 0usize;
    let mut checked = 0usize;
    for (idx, b) in bytes.iter().take(2048).enumerate() {
        if idx % 2 == 0 {
            checked += 1;
            if *b == 0 {
                zero_even += 1;
            }
        }
    }
    checked > 0 && (zero_even * 100 / checked) >= 30
}

fn read_markdown_file(path: &PathBuf) -> anyhow::Result<String> {
    let bytes = fs::read(path)?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok(String::from_utf8(bytes[3..].to_vec())?);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return Ok(text.into_owned());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return Ok(text.into_owned());
    }

    if let Ok(text) = String::from_utf8(bytes.clone()) {
        return Ok(text);
    }

    if looks_like_utf16_le(&bytes) {
        let (text, _, _) = encoding_rs::UTF_16LE.decode(&bytes);
        return Ok(text.into_owned());
    }
    if looks_like_utf16_be(&bytes) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes);
        return Ok(text.into_owned());
    }

    let (text, _, had_errors) = encoding_rs::GBK.decode(&bytes);
    if had_errors {
        eprintln!("Warning: file encoding decode had errors; falling back to best-effort GBK decode: {:?}", path);
    }
    Ok(text.into_owned())
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

    let markdown = read_markdown_file(&args.file)?;
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
