// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![feature(lazy_cell)]

mod attr;
mod chunk;

use std::{collections::HashMap, path::PathBuf};

use attr::{Attribute, ATTR_REGEXES};
use chunk::Chunk;
use clap::Parser;
use pcre2::bytes::Regex;
use tree_sitter::{Node, TreeCursor};
use walkdir::WalkDir;

const OUTPUT_DIR: &str = ".ldoc_gen";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(tree_sitter_lua::language())?;

    let out_dir = args.out_dir.join(OUTPUT_DIR);

    std::fs::create_dir_all(&out_dir)?;

    for entry in WalkDir::new(&args.path).into_iter().filter_entry(|entry| {
        // skip output_dir
        entry.file_name() != OUTPUT_DIR
    }) {
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

        // Replace all ? with |nil to make LDoc happy,
        // and remove @type to fix warnings/errors
        let mut contents = std::fs::read_to_string(entry.path())?
            .replace('?', "|nil")
            .replace("@type", "");

        // TODO:
        let _ = crate::attr::extract_alias(&mut contents);

        let Some(tree) = parser.parse(&contents, None) else {
            eprintln!("Failed to parse {}", entry.file_name().to_string_lossy());
            continue;
        };

        let mut cursor = tree.walk();

        let mut chunks = Vec::<Chunk>::new();

        let mut comments = Vec::<Node>::new();

        let mut prev_line: Option<usize> = None;

        // parse files into chunks
        // A chunk is a bunch of comments annotating some function or declaration.
        // TODO: parse @alias
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
                    let mut cursor = child.walk();
                    let decl = node_to_decl(child, &mut cursor, contents.as_bytes());
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

        let (mods_and_classes, rest): (Vec<_>, _) = chunks
            .iter()
            .filter(|chunk| {
                !chunk
                    .attributes
                    .iter()
                    .any(|attr| matches!(attr, Attribute::NoDoc))
            })
            .partition(|chunk| {
                chunk
                    .attributes
                    .iter()
                    .any(|attr| matches!(attr, Attribute::Class { .. }))
            });

        let mut methods = HashMap::<&str, Vec<&Chunk>>::new();
        const NO_NAME: &str = "_NO_NAME";
        methods.insert(NO_NAME, vec![]);

        for chunk in mods_and_classes.iter() {
            let (Declaration::Function(Some(name), _) | Declaration::Variable(name, _)) =
                &chunk.decl
            else {
                continue;
            };
            methods.insert(name, vec![]);
        }

        for chunk in rest.iter() {
            if let Declaration::Function(Some(name), _) | Declaration::Variable(name, _) =
                &chunk.decl
            {
                if let Some(v) = methods.get_mut(name.as_str()) {
                    v.push(chunk);
                } else {
                    methods.get_mut(NO_NAME).unwrap().push(chunk);
                }
            } else {
                methods.get_mut(NO_NAME).unwrap().push(chunk);
            }
        }

        let mut ldoc_text = String::new();

        // We have to place functions in a module/class in sections under the
        // corresponding LDoc annotation. The loop below orders functions correctly
        // as to not screw up LDoc generation.
        for chunk in mods_and_classes {
            let (Declaration::Function(Some(name), _) | Declaration::Variable(name, _)) =
                &chunk.decl
            else {
                continue;
            };

            // println!("{}", chunk.to_ldoc_string(contents.as_bytes()));
            ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
            // println!("{ldoc_text}");
            if let Some(chunks) = methods.get(name.as_str()) {
                for chunk in chunks.iter() {
                    ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
                }
            }
        }

        for chunk in methods.get(NO_NAME).unwrap() {
            ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
        }

        // TODO: also follow relative directory, not just file name

        crate::attr::replace_examples(&mut ldoc_text);

        crate::attr::replace_fences(&mut ldoc_text);

        std::fs::write(out_dir.join(entry.file_name()), ldoc_text)?;
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_os_t = PathBuf::from("."))]
    path: PathBuf,
    #[arg(short, long, default_value_os_t = PathBuf::from("."))]
    out_dir: PathBuf,
}

#[derive(Debug)]
pub enum Declaration<'a> {
    Function(Option<String>, Node<'a>),
    Variable(String, Node<'a>),
    Other(Node<'a>),
}

/// Parse comment blocks into two vectors: the first is a vector of summary/body comments
/// as their nodes, and the second is a vector of attribute comments converted into
/// [`Attribute`]s.
fn parse_comments<'a>(
    comments: &[Node<'a>],
    source: &[u8],
) -> anyhow::Result<(Vec<Node<'a>>, Vec<Attribute>)> {
    // filter actual comments
    let re = Regex::new(r"^[ \t]*---[ \t]*(@|\|)?").unwrap();
    let comments = comments
        .iter()
        .filter(|comment| {
            comment
                .utf8_text(source)
                .is_ok_and(|s| re.is_match(s.as_bytes()).is_ok_and(|ret| ret))
        })
        .collect::<Vec<_>>();

    let mut body = Vec::<Node>::new();
    let mut attributes = Vec::<Attribute>::new();
    for comment in comments {
        let text = comment.utf8_text(source)?; // TODO: not ?, continue
        let attr = if let Ok(Some(captures)) = ATTR_REGEXES.param.captures(text.as_bytes()) {
            (|| {
                Some(Attribute::Param {
                    name: std::str::from_utf8(captures.name("name")?.as_bytes())
                        .ok()?
                        .to_string(),
                    ty: std::str::from_utf8(captures.name("ty")?.as_bytes())
                        .ok()?
                        .to_string(),
                    desc: captures.name("desc").and_then(|desc| {
                        Some(std::str::from_utf8(desc.as_bytes()).ok()?.to_string())
                    }),
                })
            })()
        } else if let Ok(Some(captures)) = ATTR_REGEXES.ret.captures(text.as_bytes()) {
            (|| {
                Some(Attribute::Return {
                    ty: std::str::from_utf8(captures.name("ty")?.as_bytes())
                        .ok()?
                        .to_string(),
                    name: captures.name("name").and_then(|desc| {
                        Some(std::str::from_utf8(desc.as_bytes()).ok()?.to_string())
                    }),
                    desc: captures.name("desc").and_then(|desc| {
                        Some(std::str::from_utf8(desc.as_bytes()).ok()?.to_string())
                    }),
                })
            })()
        } else if let Ok(Some(captures)) = ATTR_REGEXES.see.captures(text.as_bytes()) {
            (|| {
                Some(Attribute::See {
                    link: std::str::from_utf8(captures.name("link")?.as_bytes())
                        .ok()?
                        .to_string(),
                    desc: captures.name("desc").and_then(|desc| {
                        Some(std::str::from_utf8(desc.as_bytes()).ok()?.to_string())
                    }),
                })
            })()
        } else if let Ok(Some(captures)) = ATTR_REGEXES.class.captures(text.as_bytes()) {
            (|| {
                Some(Attribute::Class {
                    ty: std::str::from_utf8(captures.name("ty")?.as_bytes())
                        .ok()?
                        .to_string(),
                })
            })()
        } else if let Ok(true) = ATTR_REGEXES.classmod.is_match(text.as_bytes()) {
            Some(Attribute::ClassMod)
        } else if ATTR_REGEXES.nodoc.is_match(text) {
            Some(Attribute::NoDoc)
        } else if let Ok(true) = ATTR_REGEXES.alias.is_match(text.as_bytes()) {
            panic!("@aliases weren't processed before parsing comments");
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

fn node_to_decl<'a>(node: Node<'a>, cursor: &mut TreeCursor<'a>, source: &[u8]) -> Declaration<'a> {
    match node.kind() {
        // local var
        // local var = {}
        "variable_declaration" => {
            let asm_stmt = node
                .children(cursor)
                .find(|child| child.kind() == "assignment_statement");
            if let Some(asm_stmt) = asm_stmt {
                let name = asm_stmt
                    .children(cursor)
                    .find(|child| child.kind() == "variable_list")
                    .and_then(|var_list| var_list.child_by_field_name("name"))
                    .expect("var decl had no name")
                    .utf8_text(source)
                    .expect("no name");
                Declaration::Variable(name.to_string(), node)
            } else if let Some(var_list) = node
                .children(cursor)
                .find(|child| child.kind() == "variable_list")
            {
                let name = var_list
                    .child_by_field_name("name")
                    .expect("var decl had no name")
                    .utf8_text(source)
                    .expect("no name");
                Declaration::Variable(name.to_string(), node)
            } else {
                Declaration::Other(node)
            }
        }
        // global = {}
        "assignment_statement" => {
            if let Some(var_list) = node
                .children(cursor)
                .find(|child| child.kind() == "variable_list")
            {
                let name = var_list
                    .child_by_field_name("name")
                    .expect("var decl had no name")
                    .utf8_text(source)
                    .expect("no name");
                Declaration::Variable(name.to_string(), node)
            } else {
                Declaration::Other(node)
            }
        }
        "function_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                match name.kind() {
                    index_expr if index_expr.ends_with("index_expression") => {
                        let name = name
                            .child_by_field_name("table")
                            .expect("no table")
                            .utf8_text(source)
                            .expect("no name");
                        Declaration::Function(Some(name.to_string()), node)
                    }
                    "identifier" => {
                        let name = name.utf8_text(source).expect("no name");
                        Declaration::Function(Some(name.to_string()), node)
                    }
                    _ => panic!("name isn't index expression or identifier"),
                }
            } else {
                Declaration::Other(node)
            }
        }
        _ => Declaration::Other(node),
    }
}
