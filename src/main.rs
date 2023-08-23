// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use clap::Parser;
use regex::Regex;
use tree_sitter::Node;
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

        let mut chunks = Vec::<Chunk>::new();

        // let mut chunks = Vec::<String>::new();
        //
        // let mut chunk = String::new();

        let mut comments = Vec::<Node>::new();

        let mut prev_line: Option<usize> = None;
        for child in cursor.node().children(&mut cursor) {
            let start_line = child.range().start_point.row;
            if child.kind() == "comment" {
                if let Some(line) = prev_line {
                    if start_line != line + 1 {
                        comments.clear();
                    }
                }

                comments.push(child);
                prev_line = Some(start_line);
            } else if let Some(line) = prev_line {
                if start_line == line + 1 {
                    let (body, attributes) = parse_comments(&comments, contents.as_bytes())?;
                    let decl = match child.kind() {
                        "variable_declaration" => Declaration::Variable(child),
                        "function_declaration" => Declaration::Function(child),
                        _ => Declaration::Other(child),
                    };
                    let chunk = Chunk {
                        body,
                        attributes,
                        decl,
                    };
                    chunks.push(chunk);
                    comments.clear();
                    prev_line = None;
                }
            } else {
                comments.clear();
                prev_line = None;
            }
        }

        for chunk in chunks.iter() {
            println!();
            println!("BODY");
            for node in chunk.body.iter() {
                println!("{}", node.utf8_text(contents.as_bytes())?);
            }
            println!("ATTR");
            println!("{:?}", chunk.attributes);
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

#[derive(Debug)]
enum Declaration<'a> {
    Function(Node<'a>),
    Variable(Node<'a>),
    Other(Node<'a>),
}

#[derive(Debug)]
struct Chunk<'a> {
    /// The summary bits
    body: Vec<Node<'a>>,
    /// The bits that start with ---@attrib
    attributes: Vec<Attribute>,
    /// The thing being annotated
    decl: Declaration<'a>,
}

#[derive(Debug)]
enum Attribute {
    Param {
        name: String,
        ty: String,
        desc: Option<String>,
    },
    Return {
        ty: String,
        name: Option<String>,
        desc: Option<String>,
    },
    Class {
        ty: String,
    },
    See {
        link: String,
        desc: Option<String>,
    },
}

fn parse_comments<'a>(
    comments: &[Node<'a>],
    source: &[u8],
) -> anyhow::Result<(Vec<Node<'a>>, Vec<Attribute>)> {
    // filter actual comments
    let re = Regex::new(r"^[ \t]*---[ \t]*(@|\|)?").unwrap();
    let comments = comments
        .iter()
        .filter(|comment| comment.utf8_text(source).is_ok_and(|s| re.is_match(s)))
        .collect::<Vec<_>>();

    let param_re = Regex::new(
        r"^[ \t]*---[ \t]*@param[ \t]+(?<name>\w+)[ \t]+(?<ty>(\{.*\}|\w+)\??(\|(\{.*\}|\w+)\??)*)([ \t]+(?<desc>.*$))?"
    ).unwrap();

    let return_re = Regex::new(
        r"^[ \t]*---[ \t]*@return[ \t]+(?<ty>(\{.*\}|\w+)\??(\|(\{.*\}|\w+)\??)*)([ \t]+(?<name>\w+)([ \t]+(?<desc>.*$))?)?"
    ).unwrap();

    let see_re =
        Regex::new(r"^[ \t]*---[ \t]*@see[ \t]+(?<link>\w+(\.\w+)?)([ \t]+(?<desc>.*$))?").unwrap();

    let class_re = Regex::new(r"^[ \t]*---[ \t]*@class[ \t]+(?<ty>\w+)").unwrap();

    let mut body = Vec::<Node>::new();
    let mut attributes = Vec::<Attribute>::new();
    for comment in comments {
        let text = comment.utf8_text(source)?; // TODO: not ?, continue
        let attr = if let Some(captures) = param_re.captures(text) {
            (|| {
                Some(Attribute::Param {
                    name: captures.name("name")?.as_str().to_string(),
                    ty: captures.name("ty")?.as_str().to_string(),
                    desc: captures.name("desc").map(|desc| desc.as_str().to_string()),
                })
            })()
        } else if let Some(captures) = return_re.captures(text) {
            (|| {
                Some(Attribute::Return {
                    ty: captures.name("ty")?.as_str().to_string(),
                    name: captures.name("name").map(|desc| desc.as_str().to_string()),
                    desc: captures.name("desc").map(|desc| desc.as_str().to_string()),
                })
            })()
        } else if let Some(captures) = see_re.captures(text) {
            (|| {
                Some(Attribute::See {
                    link: captures.name("link")?.as_str().to_string(),
                    desc: captures.name("desc").map(|desc| desc.as_str().to_string()),
                })
            })()
        } else if let Some(captures) = class_re.captures(text) {
            (|| {
                Some(Attribute::Class {
                    ty: captures.name("ty")?.as_str().to_string(),
                })
            })()
        } else {
            body.push(*comment);
            None
        };

        if let Some(attr) = attr {
            attributes.push(attr);
        }
    }
    Ok((body, attributes))
}
