use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub const IDENTITY_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceUnit {
    pub source_key: [u8; 32],
    pub format: String,
    pub resource_key: String,
    pub structural_path: Vec<String>,
    pub normalized_text: String,
    pub content_hash: [u8; 32],
    pub neighborhood_hash: [u8; 32],
}

impl SourceUnit {
    pub fn new(
        format: impl Into<String>,
        resource_key: impl Into<String>,
        structural_path: Vec<String>,
        text: &str,
        previous_text: Option<&str>,
        next_text: Option<&str>,
    ) -> Self {
        let format = format.into();
        let resource_key = resource_key.into();
        let normalized_text = normalize_text(text);
        let content_hash = hash_parts(&[normalized_text.as_bytes()]);
        let normalized_path = structural_path
            .into_iter()
            .map(|part| normalize_path_part(&part))
            .collect::<Vec<_>>();
        let previous = previous_text.map(normalize_text).unwrap_or_default();
        let next = next_text.map(normalize_text).unwrap_or_default();
        let neighborhood_hash = hash_parts(&[
            previous.as_bytes(),
            normalized_text.as_bytes(),
            next.as_bytes(),
        ]);
        let mut source_key_parts = Vec::with_capacity(normalized_path.len() + 4);
        let identity_version = IDENTITY_VERSION.to_be_bytes();
        source_key_parts.push(identity_version.as_slice());
        source_key_parts.push(format.as_bytes());
        source_key_parts.push(resource_key.as_bytes());
        for part in &normalized_path {
            source_key_parts.push(part.as_bytes());
        }
        source_key_parts.push(content_hash.as_slice());
        let source_key = hash_parts(&source_key_parts);

        Self {
            source_key,
            format,
            resource_key,
            structural_path: normalized_path,
            normalized_text,
            content_hash,
            neighborhood_hash,
        }
    }

    pub fn source_key_hex(&self) -> String {
        hex::encode(self.source_key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    Exact,
    Shifted,
    Ambiguous,
    Orphaned,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingResult {
    pub old_source_key: [u8; 32],
    pub new_source_key: Option<[u8; 32]>,
    pub kind: BindingKind,
    pub candidate_count: usize,
}

pub fn rebind(old: &[SourceUnit], new: &[SourceUnit]) -> Vec<BindingResult> {
    let mut results = Vec::with_capacity(old.len());
    let mut consumed = HashSet::<[u8; 32]>::new();
    let mut pending = Vec::<&SourceUnit>::new();
    let old_order = old
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.source_key, index))
        .collect::<HashMap<_, _>>();

    let mut old_source_counts = HashMap::<[u8; 32], usize>::new();
    let mut new_by_source = HashMap::<[u8; 32], Vec<&SourceUnit>>::new();
    for unit in old {
        *old_source_counts.entry(unit.source_key).or_default() += 1;
    }
    for unit in new {
        new_by_source.entry(unit.source_key).or_default().push(unit);
    }

    for old_unit in old {
        let exact_candidates = new_by_source
            .get(&old_unit.source_key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if old_source_counts.get(&old_unit.source_key) == Some(&1)
            && exact_candidates.len() == 1
            && consumed.insert(exact_candidates[0].source_key)
        {
            results.push(BindingResult {
                old_source_key: old_unit.source_key,
                new_source_key: Some(exact_candidates[0].source_key),
                kind: BindingKind::Exact,
                candidate_count: 1,
            });
        } else {
            pending.push(old_unit);
        }
    }

    let mut old_content_counts = HashMap::<[u8; 32], usize>::new();
    for unit in old {
        *old_content_counts.entry(unit.content_hash).or_default() += 1;
    }

    let mut new_by_content = HashMap::<[u8; 32], Vec<&SourceUnit>>::new();
    for unit in new {
        new_by_content
            .entry(unit.content_hash)
            .or_default()
            .push(unit);
    }

    for old_unit in pending {
        let candidates = new_by_content
            .get(&old_unit.content_hash)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let old_is_unique = old_content_counts.get(&old_unit.content_hash) == Some(&1);

        let unique_candidate = if old_is_unique
            && candidates.len() == 1
            && !consumed.contains(&candidates[0].source_key)
        {
            candidates.first().copied()
        } else {
            None
        };

        if let Some(candidate) = unique_candidate
            && consumed.insert(candidate.source_key)
        {
            results.push(BindingResult {
                old_source_key: old_unit.source_key,
                new_source_key: Some(candidate.source_key),
                kind: BindingKind::Shifted,
                candidate_count: candidates.len(),
            });
        } else {
            results.push(BindingResult {
                old_source_key: old_unit.source_key,
                new_source_key: None,
                kind: if candidates.is_empty() {
                    BindingKind::Orphaned
                } else {
                    BindingKind::Ambiguous
                },
                candidate_count: candidates.len(),
            });
        }
    }

    results.sort_by_key(|result| {
        old_order
            .get(&result.old_source_key)
            .copied()
            .unwrap_or(usize::MAX)
    });
    results
}

pub fn normalize_text(input: &str) -> String {
    let canonical = input.replace("\r\n", "\n").replace('\r', "\n");
    canonical.nfc().collect()
}

fn normalize_path_part(input: &str) -> String {
    input.nfc().collect::<String>().trim().to_owned()
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(path: &str, text: &str) -> SourceUnit {
        SourceUnit::new(
            "markdown",
            "chapter-1.md",
            vec![path.to_owned()],
            text,
            None,
            None,
        )
    }

    #[test]
    fn identity_is_deterministic_and_normalizes_line_endings() {
        let left = unit("section-1", "你好，世界\r\n第二行");
        let right = unit("section-1", "你好，世界\n第二行");
        assert_eq!(left.source_key, right.source_key);
        assert_eq!(left.normalized_text, "你好，世界\n第二行");
    }

    #[test]
    fn meaningful_markdown_whitespace_does_not_collapse() {
        let hard_break = unit("section-1", "第一行  \n第二行");
        let soft_break = unit("section-1", "第一行\n第二行");
        assert_ne!(hard_break.source_key, soft_break.source_key);
    }

    #[test]
    fn unique_moved_content_is_shifted() {
        let old = vec![unit("p-1", "只出现一次的句子")];
        let new = vec![unit("p-9", "只出现一次的句子")];
        let result = rebind(&old, &new);
        assert_eq!(result[0].kind, BindingKind::Shifted);
        assert_eq!(result[0].new_source_key, Some(new[0].source_key));
    }

    #[test]
    fn duplicate_content_is_never_guessed() {
        let old = vec![unit("p-1", "重复句"), unit("p-2", "重复句")];
        let new = vec![unit("p-8", "重复句"), unit("p-9", "重复句")];
        let result = rebind(&old, &new);
        assert!(
            result
                .iter()
                .all(|item| item.kind == BindingKind::Ambiguous)
        );
        assert!(result.iter().all(|item| item.new_source_key.is_none()));
    }

    #[test]
    fn new_duplicate_is_ambiguous_even_when_one_neighborhood_matches() {
        let old = vec![SourceUnit::new(
            "markdown",
            "chapter-1.md",
            vec!["p-1".to_owned()],
            "目标句",
            Some("相同上文"),
            Some("相同下文"),
        )];
        let new = vec![
            SourceUnit::new(
                "markdown",
                "chapter-1.md",
                vec!["p-8".to_owned()],
                "目标句",
                Some("相同上文"),
                Some("相同下文"),
            ),
            SourceUnit::new(
                "markdown",
                "chapter-1.md",
                vec!["p-9".to_owned()],
                "目标句",
                Some("另一上文"),
                Some("另一下文"),
            ),
        ];

        let result = rebind(&old, &new);
        assert_eq!(result[0].kind, BindingKind::Ambiguous);
        assert_eq!(result[0].new_source_key, None);
        assert_eq!(result[0].candidate_count, 2);
    }

    #[test]
    fn changed_content_is_orphaned() {
        let old = vec![unit("p-1", "旧句子")];
        let new = vec![unit("p-1", "作者已经改写")];
        assert_eq!(rebind(&old, &new)[0].kind, BindingKind::Orphaned);
    }

    #[test]
    fn large_reorder_preserves_every_unique_binding_without_false_matches() {
        let old = (0..10_000)
            .map(|index| unit(&format!("p-{index}"), &format!("固定语料第 {index} 条")))
            .collect::<Vec<_>>();
        let mut new = old
            .iter()
            .enumerate()
            .map(|(index, old_unit)| {
                unit(
                    &format!("p-{}", (index + 137) % old.len()),
                    &old_unit.normalized_text,
                )
            })
            .collect::<Vec<_>>();
        new.rotate_left(137);

        let result = rebind(&old, &new);
        assert_eq!(result.len(), old.len());
        assert!(result.iter().all(|item| {
            matches!(item.kind, BindingKind::Exact | BindingKind::Shifted)
                && item.new_source_key.is_some()
        }));
    }
}
