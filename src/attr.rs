use std::sync::LazyLock;

use pcre2::bytes::Regex;

#[derive(Debug)]
pub enum Attribute {
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
    ClassMod,
    See {
        link: String,
        desc: Option<String>,
    },
    Alias {
        types: Vec<(String, Option<String>)>,
    },
}

impl Attribute {
    pub fn to_ldoc_string(&self) -> String {
        match self {
            Attribute::Param { name, ty, desc } => {
                let ty = if ty.starts_with("fun(") {
                    "function"
                } else if ty.starts_with('{') {
                    "table"
                } else {
                    ty
                };
                format!(
                    "---@tparam {ty} {name}{}",
                    desc.as_ref()
                        .map(|desc| {
                            let mut ret = String::from(" ");
                            ret.push_str(desc);
                            ret
                        })
                        .unwrap_or("".to_string())
                )
            }
            Attribute::Return { ty, name: _, desc } => {
                format!(
                    "---@treturn {ty}{}",
                    desc.as_ref()
                        .map(|desc| {
                            let mut ret = String::from(" ");
                            ret.push_str(desc);
                            ret
                        })
                        .unwrap_or("".to_string())
                )
            }
            Attribute::Class { ty } => {
                format!("---\n---@module {ty}")
            }
            Attribute::ClassMod => "---@classmod".to_string(), // TODO:
            Attribute::See { link, desc: _ } => {
                format!("---@see {link}")
            }
            Attribute::Alias { types } => todo!(),
        }
    }
}

pub struct AttrRegexes {
    pub param: Regex,
    pub ret: Regex,
    pub see: Regex,
    pub class: Regex,
    pub classmod: Regex,
    pub alias: Regex,
}

pub static ATTR_REGEXES: LazyLock<AttrRegexes> = LazyLock::new(|| {
    AttrRegexes {
        // This is not fun
        param: Regex::new(
            r#"^[ \t]*---[ \t]*@param[ \t]+(?<name>\w+|\.\.\.)[ \t]+(?<ty>(((\{.*\}|table\<(?2),[ \t]*(?2)\>|fun\((\w+:[ \t]*(?2))?(,[ \t]*(?6))*[ \t]*\)(:[ \t]*(?2))?|\w+|".*")(\[\])?\??)|\((?2)\)(\[\])?\??)([ \t]*\|[ \t]*(?2))*)([ \t]+(?<desc>.*$))?"#
        ).unwrap(),
        ret: Regex::new(
            r#"^[ \t]*---[ \t]*@return[ \t]+(?<ty>(((\{.*\}|table\<(?1),[ \t]*(?1)\>|fun\((\w+:[ \t]*(?1))?(,[ \t]*(?5))*[ \t]*\)(:[ \t]*(?1))?|\w+|".*")(\[\])?\??)|\((?1)\)(\[\])?\??)([ \t]*\|[ \t]*(?1))*)([ \t]+(?<name>\w+)([ \t]+(?<desc>.*$))?)?"#
        ).unwrap(),
        see: Regex::new(r"^[ \t]*---[ \t]*@see[ \t]+(?<link>\w+(\.\w+)?)([ \t]+(?<desc>.*$))?")
            .unwrap(),
        class: Regex::new(r"^[ \t]*---[ \t]*@class[ \t]+(?<ty>\w+)").unwrap(),
        classmod: Regex::new(r"^[ \t]*---[ \t]*@classmod").unwrap(),
        alias: Regex::new(
            r#"^[ \t]*---[ \t]*@alias[ \t]+(?<name>\w+)[ \t]+(?<ty>(((\{.*\}|table\<(?2),[ \t]*(?2)\>|fun\((\w+:[ \t]*(?2))?(,[ \t]*(?6))*[ \t]*\)(:[ \t]*(?2))?|\w+|".*")(\[\])?\??)|\((?2)\)(\[\])?\??)([ \t]*\|[ \t]*(?2))*)"#
        ).unwrap(),
    }
});

/// Extract all @alias from the source, removing them and returning them as [`Attribute`]s.
pub fn extract_alias(source: &mut String) -> Vec<Attribute> {
    let new_source = source.clone();
    let lines = new_source.lines();
    vec![]
}

// fn parse_alias(lines: Vec<String>)
