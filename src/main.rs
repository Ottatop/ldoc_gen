// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use clap::Parser;
use walkdir::WalkDir;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(tree_sitter_lua::language())?;

    for entry in WalkDir::new(args.path) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Failed to get entry: {err}");
                continue;
            }
        };

        if !(entry.file_type().is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .to_string()
                .ends_with(".lua"))
        {
            continue;
        }

        let contents = std::fs::read_to_string(entry.path())?;
        let Some(tree) = parser.parse(&contents, None) else {
            eprintln!("Failed to parse {}", entry.file_name().to_string_lossy());
            continue;
        };

        let mut cursor = tree.walk();

        let mut chunks = Vec::<String>::new();

        let mut chunk = String::new();
        let mut prev_line: Option<usize> = None;
        for child in cursor.node().children(&mut cursor) {
            let start_line = child.range().start_point.row;
            if child.kind() == "comment" {
                if let Some(line) = prev_line {
                    if start_line != line + 1 {
                        chunk.clear();
                    }
                }

                chunk.push_str(child.utf8_text(contents.as_bytes())?);
                chunk.push('\n');
                prev_line = Some(start_line);
            } else if let Some(line) = prev_line {
                if start_line == line + 1 {
                    chunk.push_str(child.utf8_text(contents.as_bytes())?);
                    chunk.push('\n');
                    chunks.push(chunk);
                    chunk = String::new();
                    prev_line = None;
                }
            } else {
                chunk.clear();
                prev_line = None;
            }
        }

        for chunk in chunks.iter() {
            println!("{chunk}");
        }
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_os_t = PathBuf::from("."))]
    path: PathBuf,
}

enum Declaration {
    Function(String),
    Variable(String),
}

struct Chunk {
    body: String,
    attributes: Vec<(String, String)>,
    decl: Declaration,
}
