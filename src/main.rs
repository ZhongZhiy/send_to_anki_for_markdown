use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

mod anki;
mod parser;
mod render;

use parser::split_front_back;

#[derive(Parser, Debug)]
#[command(author, version, about = "Markdown to Anki CLI tool")]
pub struct Args {
    #[arg(short, long, help = "Markdown 文件路径")]
    pub file: PathBuf,

    #[arg(short, long, value_delimiter = ',', help = "全局标签，用逗号分隔，如: rust,leetcode")]
    pub tags: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let content = fs::read_to_string(&args.file)?;
    let raw_deck = parser::parse_markdown(&content);

    // 1. 检查 AnkiConnect 连接
    if !anki::can_connect() {
        eprintln!("Error: 无法连接到 AnkiConnect (http://localhost:8765)。请确保 Anki 已经打开且安装了 AnkiConnect 插件。");
        std::process::exit(1);
    }

    // 2. 创建牌组
    anki::create_deck(&raw_deck.name)?;

    let mut anki_notes = Vec::new();

    for raw_card in raw_deck.cards {
        let (front_md, back_md) = split_front_back(&raw_card.title, &raw_card.content);
        let is_cloze = render::has_cloze(&front_md) || render::has_cloze(&back_md);

        let mut fields = HashMap::new();
        let model_name;

        if is_cloze {
            model_name = "Cloze".to_string();

            // 多行填空题：拼接标题与下方多行正文
            let full_md = if back_md.trim().is_empty() {
                front_md.clone()
            } else {
                format!("{}\n\n{}", front_md, back_md)
            };

            let combined_with_clozes = render::convert_highlights_to_clozes(&full_md);
            let text_html = render::render_markdown_to_html(&combined_with_clozes);

            fields.insert("Text".to_string(), text_html);
            fields.insert("Back Extra".to_string(), String::new());
        } else {
            model_name = "rustyQA".to_string();
            let front_html = render::render_markdown_to_html(&front_md);
            let back_html = render::render_markdown_to_html(&back_md);
            fields.insert("Front".to_string(), front_html);
            fields.insert("Back".to_string(), back_html);
        }

        // 合并命令行 tags 和单张卡片的 tags 并去重
        let mut final_tags = args.tags.clone();
        final_tags.extend(raw_card.tags);
        final_tags.sort();
        final_tags.dedup();

        anki_notes.push(anki::AnkiNote {
            deck_name: raw_deck.name.clone(),
            model_name,
            fields,
            tags: final_tags,
        });
    }

    // 3. 发送卡片到 Anki
    for note in &anki_notes {
        anki::add_note(note)?;
    }

    println!("成功导入 {} 张卡片至牌组 '{}'！", anki_notes.len(), raw_deck.name);

    Ok(())
}
