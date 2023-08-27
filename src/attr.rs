use std::sync::LazyLock;

use pcre2::bytes::{Regex, RegexBuilder};

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
        types: String,
    },
}

impl Attribute {
    pub fn to_ldoc_string(&self) -> String {
        match self {
            Attribute::Param { name, ty, desc } => {
                let ty = if ty.starts_with("fun(") {
                    "function".to_string()
                } else if ty.starts_with('{') {
                    "table".to_string()
                } else {
                    let mut ty = ty.to_string().replace('?', "|nil");
                    ty.retain(|c| !c.is_whitespace());
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
                let mut ty = ty.to_string().replace('?', "|nil");
                ty.retain(|c| !c.is_whitespace());
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
                let mut ty = ty.to_string().replace('?', "|nil");
                ty.retain(|c| !c.is_whitespace());
                format!("---\n---@module {ty}")
            }
            Attribute::ClassMod => "---@classmod".to_string(), // TODO:
            Attribute::See { link, desc: _ } => {
                format!("---@see {link}")
            }
            Attribute::Alias { types: _ } => "".to_string(),
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
    pub example: Regex,
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
            r#"^[ \t]*---[ \t]*@alias[ \t]+(?<name>\w+)[ \t]+(?<ty>(((\{.*\}|table\<(?2),[ \t]*(?2)\>|fun\((\w+:[ \t]*(?2))?(,[ \t]*(?6))*[ \t]*\)(:[ \t]*(?2))?|\w+|".*")(\[\])?\??)|\((?2)\)(\[\])?\??)([ \t]*\|[ \t]*(?2))*)(\s+---[ \t]*\|[ \t]*(?2)([ \t]+(#|--)?[ \t]*.*$)?)*"#
        ).unwrap(),
        example: RegexBuilder::new().multi_line(true).build(r"(^[ \t]*---[ \t]*#{1,5}[ \t]*[E|e]xamples?.*$\s*([ \t]*---\s*)*---[ \t]*```.*$(?<example>(.*$\s*)*?)[ \t]*---[ \t]*```\s*)").unwrap(),
    }
});

/// Replace all --- ### Examples with ---@usage
pub fn replace_examples(source: &mut String) {
    let captures = ATTR_REGEXES
        .example
        .captures_iter(source.as_bytes())
        .filter_map(|res| res.ok())
        .collect::<Vec<_>>();
    let mut new_string = source.clone();
    for capture in captures {
        if let Some(example) = capture.name("example") {
            if let Ok(example) = std::str::from_utf8(example.as_bytes()) {
                let mut s = String::new();
                s.push_str("---@usage");
                s.push_str(example);
                new_string = new_string.replace(
                    std::str::from_utf8(capture.get(1).unwrap().as_bytes()).unwrap(),
                    &s,
                );
            }
        } else {
            eprintln!("NO CAPTURES");
        }
    }

    *source = new_string;
}

/// Extract all @alias from the source, removing them and returning them as [`Attribute`]s.
pub fn extract_alias(source: &mut String) -> Vec<Attribute> {
    let new_source = source.clone();
    let mut matches = ATTR_REGEXES
        .alias
        .find_iter(new_source.as_bytes())
        .collect::<Vec<_>>();
    matches.reverse();

    let mut ret = vec![];
    for m in matches {
        let Ok(m) = m else {
            continue;
        };
        ret.push(std::str::from_utf8(m.as_bytes()).unwrap());
        source.replace_range(m.start()..m.end(), "");
    }

    ret.into_iter().filter_map(parse_alias).collect()
}

fn parse_alias(alias: &str) -> Option<Attribute> {
    let mut types = String::new();
    let mut lines = alias.lines();

    types.push_str(
        &ALIAS_FIRST_LINE_REGEX
            .captures(lines.next()?)?
            .name("types")?
            .as_str()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
    );

    for line in lines {
        let Some(captures) = ALIAS_OTHER_LINE_REGEX.captures(line) else {
            continue;
        };
        let Some(ty) = captures.name("type") else {
            continue;
        };
        types.push('|');
        types.push_str(
            &ty.as_str()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        );
    }

    Some(Attribute::Alias { types })
}

static ALIAS_FIRST_LINE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^[ \t]*---@alias[ \t]+(?<types>.*)[ \t]*(#|--)?").unwrap()
});

static ALIAS_OTHER_LINE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^[ \t]*---[ \t]*\|[ \t]*(?<type>.*)[ \t]*(#|--)?").unwrap()
});
