//! Minimal translation intermediate representation and structural validator.

use std::collections::HashMap;

use babel_domain::core::ResourceId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const TIR_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakKind {
    Soft,
    Hard,
    Paragraph,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaceholderRule {
    ExactlyOnce,
    PreserveCount,
    Optional,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Token {
    Text {
        text: String,
        style_hint: Option<String>,
    },
    ProtectedOpen {
        tag_key: String,
        display_hint: Option<String>,
    },
    ProtectedClose {
        tag_key: String,
    },
    ProtectedAtom {
        atom_key: String,
        display_hint: Option<String>,
    },
    Placeholder {
        name: String,
        rule: PlaceholderRule,
    },
    Break {
        kind: BreakKind,
    },
    Reference {
        resource_id: ResourceId,
        relation: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitContent {
    pub schema_version: u32,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TirError {
    #[error("unsupported TIR schema version {0}")]
    UnsupportedSchema(u32),
    #[error("protected token nesting is invalid at token {0}")]
    InvalidNesting(usize),
    #[error("protected token sequence changed")]
    ProtectedSequenceChanged,
    #[error("placeholder {0} violates its preservation rule")]
    PlaceholderViolation(String),
    #[error("placeholder {0} declares inconsistent preservation rules")]
    InconsistentPlaceholderRule(String),
}

impl UnitContent {
    pub fn validate(&self) -> Result<(), TirError> {
        if self.schema_version != TIR_SCHEMA_VERSION {
            return Err(TirError::UnsupportedSchema(self.schema_version));
        }
        let mut stack = Vec::new();
        for (index, token) in self.tokens.iter().enumerate() {
            match token {
                Token::ProtectedOpen { tag_key, .. } => stack.push(tag_key),
                Token::ProtectedClose { tag_key } if stack.pop() != Some(tag_key) => {
                    return Err(TirError::InvalidNesting(index));
                }
                _ => {}
            }
        }
        if !stack.is_empty() {
            return Err(TirError::InvalidNesting(self.tokens.len()));
        }
        Ok(())
    }
}

pub fn validate_overlay(source: &UnitContent, overlay: &UnitContent) -> Result<(), TirError> {
    source.validate()?;
    overlay.validate()?;
    if protected_signature(source) != protected_signature(overlay) {
        return Err(TirError::ProtectedSequenceChanged);
    }
    let source_placeholders = placeholders(source)?;
    let overlay_placeholders = placeholders(overlay)?;
    for (name, (rule, count)) in &source_placeholders {
        let overlay_entry = overlay_placeholders.get(name);
        if overlay_entry.is_some_and(|(overlay_rule, _)| overlay_rule != rule) {
            return Err(TirError::PlaceholderViolation(name.clone()));
        }
        let overlay_count = overlay_entry.map(|(_, count)| *count).unwrap_or(0);
        let valid = match rule {
            PlaceholderRule::ExactlyOnce => *count == 1 && overlay_count == 1,
            PlaceholderRule::PreserveCount => overlay_count == *count,
            PlaceholderRule::Optional => overlay_count <= *count,
        };
        if !valid {
            return Err(TirError::PlaceholderViolation(name.clone()));
        }
    }
    if overlay_placeholders
        .keys()
        .any(|name| !source_placeholders.contains_key(name))
    {
        return Err(TirError::PlaceholderViolation(
            "unexpected placeholder".to_owned(),
        ));
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
enum ProtectedSignature<'a> {
    Open(&'a str),
    Close(&'a str),
    Atom(&'a str),
    Reference(ResourceId, &'a str),
}

fn protected_signature(content: &UnitContent) -> Vec<ProtectedSignature<'_>> {
    content
        .tokens
        .iter()
        .filter_map(|token| match token {
            Token::ProtectedOpen { tag_key, .. } => Some(ProtectedSignature::Open(tag_key)),
            Token::ProtectedClose { tag_key } => Some(ProtectedSignature::Close(tag_key)),
            Token::ProtectedAtom { atom_key, .. } => Some(ProtectedSignature::Atom(atom_key)),
            Token::Reference {
                resource_id,
                relation,
            } => Some(ProtectedSignature::Reference(*resource_id, relation)),
            _ => None,
        })
        .collect()
}

fn placeholders(
    content: &UnitContent,
) -> Result<HashMap<String, (PlaceholderRule, usize)>, TirError> {
    let mut result = HashMap::new();
    for token in &content.tokens {
        if let Token::Placeholder { name, rule } = token {
            let entry = result.entry(name.clone()).or_insert((rule.clone(), 0));
            if entry.0 != *rule {
                return Err(TirError::InconsistentPlaceholderRule(name.clone()));
            }
            entry.1 += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(tokens: Vec<Token>) -> UnitContent {
        UnitContent {
            schema_version: TIR_SCHEMA_VERSION,
            tokens,
        }
    }

    #[test]
    fn protected_tokens_must_stay_nested_and_identical() {
        let source = content(vec![
            Token::ProtectedOpen {
                tag_key: "em".to_owned(),
                display_hint: None,
            },
            Token::Text {
                text: "source".to_owned(),
                style_hint: None,
            },
            Token::ProtectedClose {
                tag_key: "em".to_owned(),
            },
        ]);
        let overlay = content(vec![Token::Text {
            text: "translation".to_owned(),
            style_hint: None,
        }]);
        assert_eq!(
            validate_overlay(&source, &overlay),
            Err(TirError::ProtectedSequenceChanged)
        );
    }

    #[test]
    fn placeholder_count_is_enforced() {
        let source = content(vec![Token::Placeholder {
            name: "player".to_owned(),
            rule: PlaceholderRule::ExactlyOnce,
        }]);
        let overlay = content(Vec::new());
        assert_eq!(
            validate_overlay(&source, &overlay),
            Err(TirError::PlaceholderViolation("player".to_owned()))
        );
    }

    #[test]
    fn one_placeholder_name_cannot_mix_rules() {
        let source = content(vec![
            Token::Placeholder {
                name: "player".to_owned(),
                rule: PlaceholderRule::ExactlyOnce,
            },
            Token::Placeholder {
                name: "player".to_owned(),
                rule: PlaceholderRule::Optional,
            },
        ]);
        assert_eq!(
            validate_overlay(&source, &content(Vec::new())),
            Err(TirError::InconsistentPlaceholderRule("player".to_owned()))
        );
    }

    #[test]
    fn resource_references_cannot_disappear_from_an_overlay() {
        let source = content(vec![Token::Reference {
            resource_id: ResourceId::new(),
            relation: "image".to_owned(),
        }]);
        assert_eq!(
            validate_overlay(&source, &content(Vec::new())),
            Err(TirError::ProtectedSequenceChanged)
        );
    }
}
