// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use tree_sitter::Node;

use crate::{attr::Attribute, Declaration};

#[derive(Debug)]
pub struct Chunk<'a> {
    /// The summary bits
    pub body: Vec<Node<'a>>,
    /// The bits that start with ---@attr
    pub attributes: Vec<Attribute>,
    /// The thing being annotated
    pub decl: Declaration<'a>,
}

impl Chunk<'_> {
    pub fn to_ldoc_string(&self, source: &[u8]) -> String {
        let mut ret = String::new();
        ret.push('\n');

        for node in self.body.iter() {
            let comment = node.utf8_text(source).unwrap();
            ret.push_str(comment);
            ret.push('\n');
        }

        for attr in self.attributes.iter() {
            if let Attribute::ClassMod = attr {
                continue;
            } else if let Attribute::Class { ty } = attr {
                // println!("got class {ty}");
                if self
                    .attributes
                    .iter()
                    .any(|a| matches!(a, Attribute::ClassMod))
                {
                    // println!("pushing ---@classmod {ty}");
                    ret.push_str(&format!("---@classmod {ty}"));
                } else {
                    // println!("pushing {}", attr.to_ldoc_string());
                    ret.push_str(&attr.to_ldoc_string());
                }
            } else {
                // println!("pushing {}", attr.to_ldoc_string());
                ret.push_str(&attr.to_ldoc_string());
            }
            ret.push('\n');
        }

        let decl = match self.decl {
            Declaration::Function(_, decl) => {
                let ret = decl.utf8_text(source).unwrap();
                if let Some(body) = decl.child_by_field_name("body") {
                    ret.replace(body.utf8_text(source).unwrap(), "")
                } else {
                    ret.to_string()
                }
            }
            Declaration::Variable(_, decl) | Declaration::Other(decl) => {
                decl.utf8_text(source).unwrap().to_string()
            }
        };

        ret.push_str(&decl);
        ret.push('\n');

        ret
    }
}
