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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(tree_sitter_lua::language())?;

    for entry in WalkDir::new(&args.path) {
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

        std::fs::create_dir_all(&args.out_dir)?;

        let mut cursor = tree.walk();

        let mut chunks = Vec::<Chunk>::new();

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

        let (mods_and_classes, rest): (Vec<_>, _) = chunks.iter().partition(|chunk| {
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

        for chunk in mods_and_classes {
            let (Declaration::Function(Some(name), _) | Declaration::Variable(name, _)) =
                &chunk.decl
            else {
                continue;
            };

            ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
            if let Some(chunks) = methods.get(name.as_str()) {
                for chunk in chunks.iter() {
                    ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
                }
            }
        }

        for chunk in methods.get(NO_NAME).unwrap() {
            ldoc_text.push_str(&chunk.to_ldoc_string(contents.as_bytes()));
        }

        std::fs::write(args.out_dir.join(entry.file_name()), ldoc_text)?;
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_os_t = PathBuf::from("."))]
    path: PathBuf,
    #[arg(short, long, default_value_os_t = PathBuf::from("./.ldoc_gen"))]
    out_dir: PathBuf,
}

#[derive(Debug)]
pub enum Declaration<'a> {
    Function(Option<String>, Node<'a>),
    Variable(String, Node<'a>),
    Other(Node<'a>),
}

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

    let param_re = &ATTR_REGEXES.param;
    let return_re = &ATTR_REGEXES.ret;
    let see_re = &ATTR_REGEXES.see;
    let class_re = &ATTR_REGEXES.class;
    let classmod_re = &ATTR_REGEXES.classmod;

    let mut body = Vec::<Node>::new();
    let mut attributes = Vec::<Attribute>::new();
    for comment in comments {
        let text = comment.utf8_text(source)?; // TODO: not ?, continue
        let attr = if let Ok(Some(captures)) = param_re.captures(text.as_bytes()) {
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
        } else if let Ok(Some(captures)) = return_re.captures(text.as_bytes()) {
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
        } else if let Ok(Some(captures)) = see_re.captures(text.as_bytes()) {
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
        } else if let Ok(Some(captures)) = class_re.captures(text.as_bytes()) {
            (|| {
                Some(Attribute::Class {
                    ty: std::str::from_utf8(captures.name("ty")?.as_bytes())
                        .ok()?
                        .to_string(),
                })
            })()
        } else if let Ok(true) = classmod_re.is_match(text.as_bytes()) {
            Some(Attribute::ClassMod)
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
        last => {
            eprintln!("unknown decl: {last}");
            Declaration::Other(node)
        }
    }
}
