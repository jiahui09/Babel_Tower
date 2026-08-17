//! CommonMark Markdown adapter with byte-preserving patch export.

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    ops::Range,
};

use babel_adapter_protocol::{
    ADAPTER_PROTOCOL_MAJOR, ADAPTER_PROTOCOL_MINOR, Adapter, AdapterError, AdapterManifest,
    CapabilityIo, Cursor, ExecutionContext, ExportPlan, ExtractedUnit, InventoryItem,
    MaterializeProgress, ObjectHandle, Operation, OverlayUnit, Page, ProbeResult, ProtocolRange,
    SafetyLimits, StagingHandle, VerificationReport,
};
use babel_domain::{
    core::{GenerationId, ResourceId},
    identity::{IDENTITY_VERSION, SourceUnit},
};
use babel_resource_graph::{
    EdgeKind, Locator, ResourceEdge, ResourceKind, ResourceNode, resource_key,
};
use babel_tir::{TIR_SCHEMA_VERSION, Token, UnitContent};
use comrak::{
    Arena, Options,
    nodes::{AstNode, NodeValue, Sourcepos},
    parse_document,
};
use sha2::{Digest, Sha256};

const ADAPTER_ID: &str = "org.babel-tower.markdown";
const ADAPTER_BUILD: &str = "phase4.0";
const FORMAT_ID: &str = "markdown";
const READ_CHUNK: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct MarkdownAdapter {
    manifest: AdapterManifest,
}

impl Default for MarkdownAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownAdapter {
    pub fn new() -> Self {
        Self {
            manifest: AdapterManifest {
                adapter_id: ADAPTER_ID.to_owned(),
                adapter_build: ADAPTER_BUILD.to_owned(),
                protocol_range: ProtocolRange {
                    major: ADAPTER_PROTOCOL_MAJOR,
                    minimum_minor: ADAPTER_PROTOCOL_MINOR,
                    maximum_minor: ADAPTER_PROTOCOL_MINOR,
                },
                identity_version: IDENTITY_VERSION,
                mime_types: vec!["text/markdown".to_owned(), "text/x-markdown".to_owned()],
                extensions: vec!["md".to_owned(), "markdown".to_owned()],
                resource_kinds: vec![
                    ResourceKind::Document,
                    ResourceKind::TextStream,
                    ResourceKind::Image,
                ],
                export_fidelity_tier: "byte-preserving-markdown-spans".to_owned(),
                deterministic_stages: vec![
                    Operation::Probe,
                    Operation::Inventory,
                    Operation::Extract,
                    Operation::PlanExport,
                    Operation::Materialize,
                    Operation::VerifyOutput,
                ],
                safety_limits: SafetyLimits {
                    maximum_input_bytes: 1024 * 1024 * 1024,
                    maximum_output_bytes: 1024 * 1024 * 1024,
                    maximum_nodes_per_page: 100_000,
                    maximum_page_bytes: 64 * 1024 * 1024,
                },
            },
        }
    }

    pub fn extract_all(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Vec<ExtractedUnit>, AdapterError> {
        let bytes = self.read_input(input, io, context)?;
        self.validate_text_resource(input, resource_id)?;
        let markdown = std::str::from_utf8(&bytes).map_err(|_| {
            AdapterError::InvalidInput("Markdown adapter requires UTF-8 input".to_owned())
        })?;
        let parsed = parse_markdown(markdown)?;
        if parsed.spans.len() > context.budget.maximum_nodes as usize {
            return Err(AdapterError::BudgetExceeded);
        }
        let (items, _) = extracted_units(
            &parsed,
            input.object_hash,
            generation_id,
            resource_id,
            0,
            parsed.spans.len(),
            context.budget.maximum_bytes,
            context,
            &self.manifest,
        )?;
        Ok(items)
    }

    fn validate_text_resource(
        &self,
        input: &ObjectHandle,
        resource_id: ResourceId,
    ) -> Result<(), AdapterError> {
        let expected_resource_id = id_from_hash(resource_key(
            &input.object_hash,
            &self.manifest.adapter_id,
            self.manifest.identity_version,
            "document/text",
        ));
        if resource_id != expected_resource_id {
            return Err(AdapterError::InvalidInput(
                "Markdown extraction requires the text stream resource".to_owned(),
            ));
        }
        Ok(())
    }

    fn read_input(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Vec<u8>, AdapterError> {
        context.checkpoint()?;
        if input.byte_length > context.budget.maximum_bytes
            || input.byte_length > self.manifest.safety_limits.maximum_input_bytes
        {
            return Err(AdapterError::BudgetExceeded);
        }
        let mut reader = io.open_object(input)?;
        let mut bytes = Vec::with_capacity(input.byte_length.min(READ_CHUNK as u64) as usize);
        let mut buffer = [0_u8; READ_CHUNK];
        loop {
            context.checkpoint()?;
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if bytes.len() as u64 > context.budget.maximum_bytes {
                return Err(AdapterError::BudgetExceeded);
            }
        }
        Ok(bytes)
    }
}

impl Adapter for MarkdownAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn probe(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<ProbeResult, AdapterError> {
        let bytes = self.read_input(input, io, context)?;
        if looks_binary(&bytes) {
            return Ok(ProbeResult {
                confidence_millionths: 0,
                detected_media_type: None,
                reason_code: "binary-nul-or-control".to_owned(),
            });
        }
        let markdown = std::str::from_utf8(&bytes).map_err(|_| {
            AdapterError::InvalidInput("Markdown adapter requires UTF-8 input".to_owned())
        })?;
        parse_markdown(markdown)?;
        Ok(ProbeResult {
            confidence_millionths: markdown_confidence(markdown),
            detected_media_type: Some("text/markdown".to_owned()),
            reason_code: "commonmark-utf8".to_owned(),
        })
    }

    fn inventory(
        &self,
        input: &ObjectHandle,
        _generation_id: GenerationId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError> {
        let bytes = self.read_input(input, io, context)?;
        let markdown = std::str::from_utf8(&bytes).map_err(|_| {
            AdapterError::InvalidInput("Markdown adapter requires UTF-8 input".to_owned())
        })?;
        let parsed = parse_markdown(markdown)?;
        let document_key = resource_key(
            &input.object_hash,
            &self.manifest.adapter_id,
            self.manifest.identity_version,
            "document",
        );
        let stream_key = resource_key(
            &input.object_hash,
            &self.manifest.adapter_id,
            self.manifest.identity_version,
            "document/text",
        );
        let document_id = id_from_hash(document_key);
        let stream_id = id_from_hash(stream_key);
        let mut items = vec![
            InventoryItem::Node(ResourceNode {
                resource_id: document_id,
                resource_key: document_key,
                kind: ResourceKind::Document,
                semantic_path: "document".to_owned(),
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: 0,
                    end: input.byte_length,
                },
            }),
            InventoryItem::Node(ResourceNode {
                resource_id: stream_id,
                resource_key: stream_key,
                kind: ResourceKind::TextStream,
                semantic_path: "document/text".to_owned(),
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: 0,
                    end: input.byte_length,
                },
            }),
            InventoryItem::Edge(ResourceEdge {
                from: document_id,
                to: stream_id,
                kind: EdgeKind::Contains,
                ordinal: 0,
            }),
        ];
        for (ordinal, image) in parsed.images.iter().enumerate() {
            let image_key = resource_key(
                &input.object_hash,
                &self.manifest.adapter_id,
                self.manifest.identity_version,
                &format!("image/{}", image.url),
            );
            let image_id = id_from_hash(image_key);
            items.push(InventoryItem::Node(ResourceNode {
                resource_id: image_id,
                resource_key: image_key,
                kind: ResourceKind::Image,
                semantic_path: format!("image/{}", image.url),
                locator: Locator::StructuralPath {
                    resource_id: document_id,
                    path_segments: vec!["image".to_owned(), image.url.clone()],
                    attribute: Some("destination".to_owned()),
                },
            }));
            items.push(InventoryItem::Edge(ResourceEdge {
                from: stream_id,
                to: image_id,
                kind: EdgeKind::References,
                ordinal: ordinal as u32,
            }));
        }
        paginate_items(items, cursor, context, &self.manifest)
    }

    fn extract(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<ExtractedUnit>, AdapterError> {
        let bytes = self.read_input(input, io, context)?;
        self.validate_text_resource(input, resource_id)?;
        let markdown = std::str::from_utf8(&bytes).map_err(|_| {
            AdapterError::InvalidInput("Markdown adapter requires UTF-8 input".to_owned())
        })?;
        let parsed = parse_markdown(markdown)?;
        let start = decode_u64_cursor(cursor)? as usize;
        if start > parsed.spans.len() {
            return Err(AdapterError::InvalidCursor);
        }
        let node_limit = context.budget.bounded_page_nodes(&self.manifest) as usize;
        let byte_limit = context.budget.bounded_page_bytes(&self.manifest);
        let (items, emitted_bytes) = extracted_units(
            &parsed,
            input.object_hash,
            generation_id,
            resource_id,
            start,
            node_limit,
            byte_limit,
            context,
            &self.manifest,
        )?;
        let next = start + items.len();
        Ok(Page {
            items,
            next_cursor: (next < parsed.spans.len()).then(|| encode_u64_cursor(next as u64)),
            emitted_bytes,
        })
    }

    fn plan_export(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        frozen_commit_sequence: i64,
        overlays: &[OverlayUnit],
        context: &ExecutionContext<'_>,
    ) -> Result<ExportPlan, AdapterError> {
        context.checkpoint()?;
        validate_overlay_ranges(input.object_hash, overlays)?;
        let overlay_hash = hash_overlays(overlays);
        let ordered_overlay_hashes = overlays.iter().map(hash_overlay).collect::<Vec<_>>();
        let mut plan = Sha256::new();
        plan.update(b"babel-markdown-export-plan-v1");
        plan.update(generation_id.as_bytes());
        plan.update(input.object_hash);
        plan.update(frozen_commit_sequence.to_be_bytes());
        plan.update(overlay_hash);
        Ok(ExportPlan {
            plan_id: plan.finalize().into(),
            generation_id,
            source_object_hash: input.object_hash,
            frozen_commit_sequence,
            overlay_hash,
            ordered_source_unit_keys: overlays
                .iter()
                .map(|overlay| overlay.source_unit_key)
                .collect(),
            ordered_overlay_hashes,
        })
    }

    fn materialize(
        &self,
        plan: &ExportPlan,
        input: &ObjectHandle,
        overlays: &[OverlayUnit],
        staging: &StagingHandle,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<MaterializeProgress, AdapterError> {
        context.checkpoint()?;
        if cursor.is_some() {
            return Err(AdapterError::InvalidCursor);
        }
        validate_plan(plan, input, overlays)?;
        validate_overlay_ranges(input.object_hash, overlays)?;
        let source = self.read_input(input, io, context)?;
        let source_markdown = std::str::from_utf8(&source).map_err(|_| {
            AdapterError::InvalidInput("Markdown adapter requires UTF-8 input".to_owned())
        })?;
        let parsed_source = parse_markdown(source_markdown)?;
        validate_overlays_match_source(
            input.object_hash,
            overlays,
            &parsed_source,
            &self.manifest,
        )?;
        let patches = overlays
            .iter()
            .map(|overlay| {
                let (start, end) = byte_span(&overlay.source_locator, input.object_hash)?;
                if end > source.len() {
                    return Err(AdapterError::InvalidInput(
                        "source locator is outside source object".to_owned(),
                    ));
                }
                Ok(BytePatch {
                    start,
                    end,
                    replacement: overlay.translated_text.as_bytes().to_vec(),
                })
            })
            .collect::<Result<Vec<_>, AdapterError>>()?;
        let output = apply_patches(&source, &patches, context)?;
        let output_markdown = std::str::from_utf8(&output).map_err(|_| {
            AdapterError::InvalidInput("Markdown export generated invalid UTF-8".to_owned())
        })?;
        let parsed_output = parse_markdown(output_markdown)?;
        if parsed_source.protection_signature != parsed_output.protection_signature {
            return Err(AdapterError::InvalidInput(
                "translated Markdown changes protected structure".to_owned(),
            ));
        }
        let mut existing = io.open_staging(staging)?;
        let existing_length = existing.seek(SeekFrom::End(0))?;
        match existing_length {
            0 => io.write_staging_at(staging, 0, &output)?,
            length if length == output.len() as u64 => {
                existing.seek(SeekFrom::Start(0))?;
                let mut written = Vec::new();
                existing.read_to_end(&mut written)?;
                if written != output {
                    return Err(AdapterError::InvalidCursor);
                }
            }
            _ => return Err(AdapterError::InvalidCursor),
        }
        Ok(MaterializeProgress {
            next_cursor: None,
            bytes_written: output.len() as u64,
            complete: true,
        })
    }

    fn verify_output(
        &self,
        candidate: &StagingHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<VerificationReport, AdapterError> {
        context.checkpoint()?;
        let mut reader = io.open_staging(candidate)?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        if bytes.len() as u64 > context.budget.maximum_bytes {
            return Err(AdapterError::BudgetExceeded);
        }
        let mut issue_codes = Vec::new();
        if looks_binary(&bytes) {
            issue_codes.push("binary-nul-or-control".to_owned());
        }
        match std::str::from_utf8(&bytes) {
            Ok(markdown) => {
                if parse_markdown(markdown).is_err() {
                    issue_codes.push("invalid-commonmark".to_owned());
                }
            }
            Err(_) => issue_codes.push("invalid-utf8".to_owned()),
        }
        Ok(VerificationReport {
            valid: issue_codes.is_empty(),
            output_hash: Sha256::digest(&bytes).into(),
            byte_length: bytes.len() as u64,
            issue_codes,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn extracted_units(
    parsed: &ParsedMarkdown,
    object_hash: [u8; 32],
    generation_id: GenerationId,
    resource_id: ResourceId,
    start: usize,
    node_limit: usize,
    byte_limit: u64,
    context: &ExecutionContext<'_>,
    manifest: &AdapterManifest,
) -> Result<(Vec<ExtractedUnit>, u64), AdapterError> {
    let stable_unit_resource_key = format!(
        "{}:document/text:identity-v{}",
        manifest.adapter_id, manifest.identity_version
    );
    let mut items = Vec::with_capacity(node_limit.min(parsed.spans.len().saturating_sub(start)));
    let mut emitted_bytes = 0_u64;
    for (index, span) in parsed.spans.iter().enumerate().skip(start) {
        context.checkpoint()?;
        let size = (span.range.end - span.range.start) as u64;
        if !items.is_empty() && (items.len() >= node_limit || emitted_bytes + size > byte_limit) {
            break;
        }
        if size > byte_limit {
            return Err(AdapterError::BudgetExceeded);
        }
        let previous = index
            .checked_sub(1)
            .and_then(|previous| parsed.spans.get(previous))
            .map(|previous| previous.text.as_str());
        let next = parsed.spans.get(index + 1).map(|next| next.text.as_str());
        let source_unit = SourceUnit::new(
            FORMAT_ID,
            stable_unit_resource_key.clone(),
            span.path.clone(),
            &span.text,
            previous,
            next,
        );
        let content = UnitContent {
            schema_version: TIR_SCHEMA_VERSION,
            tokens: span.tokens.clone(),
        };
        content
            .validate()
            .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
        items.push(ExtractedUnit {
            generation_id,
            resource_id,
            source_unit_key: source_unit.source_key,
            locator: Locator::ByteSpan {
                object_hash,
                start: span.range.start as u64,
                end: span.range.end as u64,
            },
            content,
        });
        emitted_bytes += size;
    }
    Ok((items, emitted_bytes))
}

#[derive(Clone, Debug)]
struct ParsedMarkdown {
    spans: Vec<TextSpan>,
    images: Vec<ImageRef>,
    protection_signature: Vec<String>,
}

#[derive(Clone, Debug)]
struct TextSpan {
    range: Range<usize>,
    text: String,
    path: Vec<String>,
    tokens: Vec<Token>,
}

#[derive(Clone, Debug)]
struct ImageRef {
    url: String,
}

#[derive(Clone, Debug)]
struct BytePatch {
    start: usize,
    end: usize,
    replacement: Vec<u8>,
}

fn parse_markdown(markdown: &str) -> Result<ParsedMarkdown, AdapterError> {
    let arena = Arena::new();
    let options = Options::default();
    let root = parse_document(&arena, markdown, &options);
    let line_index = LineIndex::new(markdown);
    let mut spans = Vec::new();
    let mut images = Vec::new();
    let mut protection_signature = Vec::new();
    collect_nodes(
        root,
        &line_index,
        &mut Vec::new(),
        &mut spans,
        &mut images,
        &mut protection_signature,
    )?;
    Ok(ParsedMarkdown {
        spans,
        images,
        protection_signature,
    })
}

fn collect_nodes<'a>(
    node: &'a AstNode<'a>,
    line_index: &LineIndex,
    path: &mut Vec<String>,
    spans: &mut Vec<TextSpan>,
    images: &mut Vec<ImageRef>,
    protection_signature: &mut Vec<String>,
) -> Result<(), AdapterError> {
    let node_data = node.data.borrow();
    let segment = node_segment(&node_data.value);
    if let Some(part) = &segment {
        path.push(part.clone());
    }
    if let Some(signature) = protected_node_signature(&node_data.value) {
        protection_signature.push(signature);
    }
    match &node_data.value {
        NodeValue::Image(link) => {
            images.push(ImageRef {
                url: link.url.to_string(),
            });
        }
        NodeValue::Text(text) if !text.is_empty() => {
            let range = line_index.sourcepos_range(node_data.sourcepos)?;
            let text = text.to_string();
            spans.push(TextSpan {
                range,
                tokens: protected_context_tokens(node, text.clone()),
                text,
                path: span_path(path, spans.len()),
            });
        }
        _ => {}
    }
    drop(node_data);
    for child in node.children() {
        collect_nodes(child, line_index, path, spans, images, protection_signature)?;
    }
    if segment.is_some() {
        path.pop();
    }
    Ok(())
}

fn protected_node_signature(value: &NodeValue) -> Option<String> {
    match value {
        NodeValue::Link(link) => Some(format!(
            "link:{}:{}",
            hash_text(&link.url),
            hash_text(&link.title)
        )),
        NodeValue::Image(link) => Some(format!(
            "image:{}:{}",
            hash_text(&link.url),
            hash_text(&link.title)
        )),
        NodeValue::Code(code) => Some(format!("inline-code:{}", hash_text(&code.literal))),
        NodeValue::CodeBlock(code) => Some(format!(
            "code-block:{}:{}:{}:{}",
            code.fenced,
            code.fence_char,
            hash_text(&code.info),
            hash_text(&code.literal)
        )),
        NodeValue::HtmlInline(html) => Some(format!("html-inline:{}", hash_text(html))),
        NodeValue::HtmlBlock(html) => Some(format!("html-block:{}", hash_text(&html.literal))),
        _ => None,
    }
}

fn node_segment(value: &NodeValue) -> Option<String> {
    match value {
        NodeValue::Paragraph => Some("paragraph".to_owned()),
        NodeValue::Heading(heading) => Some(format!("heading:{}", heading.level)),
        NodeValue::Item(_) => Some("list-item".to_owned()),
        NodeValue::BlockQuote => Some("blockquote".to_owned()),
        NodeValue::Link(link) => Some(format!("link:{}", hash_text(&link.url))),
        NodeValue::Image(link) => Some(format!("image-alt:{}", hash_text(&link.url))),
        NodeValue::Emph => Some("emph".to_owned()),
        NodeValue::Strong => Some("strong".to_owned()),
        NodeValue::Code(_) => Some("code".to_owned()),
        NodeValue::HtmlInline(_) => Some("html-inline".to_owned()),
        NodeValue::HtmlBlock(_) => Some("html-block".to_owned()),
        _ => None,
    }
}

fn span_path(path: &[String], index: usize) -> Vec<String> {
    let mut result = path.to_vec();
    result.push(format!("text:{index:016x}"));
    result
}

fn protected_context_tokens<'a>(node: &'a AstNode<'a>, text: String) -> Vec<Token> {
    let mut opens = Vec::new();
    let mut atoms = Vec::new();
    for ancestor in node.ancestors() {
        let value = &ancestor.data.borrow().value;
        match value {
            NodeValue::Emph => opens.push(("em".to_owned(), Some("*".to_owned()))),
            NodeValue::Strong => opens.push(("strong".to_owned(), Some("**".to_owned()))),
            NodeValue::Link(link) => atoms.push(Token::ProtectedAtom {
                atom_key: format!("link-url:{}", hash_text(&link.url)),
                display_hint: Some(link.url.to_string()),
            }),
            NodeValue::Image(link) => atoms.push(Token::ProtectedAtom {
                atom_key: format!("image-url:{}", hash_text(&link.url)),
                display_hint: Some(link.url.to_string()),
            }),
            _ => {}
        }
    }
    let mut tokens = Vec::new();
    for (tag_key, display_hint) in &opens {
        tokens.push(Token::ProtectedOpen {
            tag_key: tag_key.clone(),
            display_hint: display_hint.clone(),
        });
    }
    tokens.extend(atoms);
    tokens.push(Token::Text {
        text,
        style_hint: None,
    });
    for (tag_key, _) in opens.iter().rev() {
        tokens.push(Token::ProtectedClose {
            tag_key: tag_key.clone(),
        });
    }
    tokens
}

#[derive(Clone, Debug)]
struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self {
            starts,
            len: text.len(),
        }
    }

    fn sourcepos_range(&self, sourcepos: Sourcepos) -> Result<Range<usize>, AdapterError> {
        if sourcepos.start.line == 0 || sourcepos.end.line == 0 {
            return Err(AdapterError::InvalidInput(
                "Markdown node has no source position".to_owned(),
            ));
        }
        let start = self.offset(sourcepos.start.line, sourcepos.start.column)?;
        let end = self.offset(sourcepos.end.line, sourcepos.end.column)? + 1;
        if start > end || end > self.len {
            return Err(AdapterError::InvalidInput(
                "Markdown node source position is outside input".to_owned(),
            ));
        }
        Ok(start..end)
    }

    fn offset(&self, line: usize, column: usize) -> Result<usize, AdapterError> {
        let line_start = *self.starts.get(line - 1).ok_or_else(|| {
            AdapterError::InvalidInput("Markdown source position line is invalid".to_owned())
        })?;
        Ok((line_start + column.saturating_sub(1)).min(self.len))
    }
}

fn markdown_confidence(markdown: &str) -> u32 {
    if markdown
        .lines()
        .any(|line| line.starts_with('#') || line.starts_with("- ") || line.contains("]("))
    {
        930_000
    } else {
        650_000
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.contains(&0) {
        return true;
    }
    let checked = bytes.iter().take(4096).count().max(1);
    let controls = bytes
        .iter()
        .take(4096)
        .filter(|byte| matches!(byte, 0x01..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f))
        .count();
    controls * 100 / checked > 5
}

fn apply_patches(
    source: &[u8],
    patches: &[BytePatch],
    context: &ExecutionContext<'_>,
) -> Result<Vec<u8>, AdapterError> {
    let mut output = Vec::with_capacity(source.len());
    let mut cursor = 0;
    for patch in patches {
        context.checkpoint()?;
        if patch.start < cursor || patch.start > patch.end || patch.end > source.len() {
            return Err(AdapterError::InvalidInput(
                "Markdown export patch ranges overlap or exceed input".to_owned(),
            ));
        }
        output.extend_from_slice(&source[cursor..patch.start]);
        output.extend_from_slice(&patch.replacement);
        cursor = patch.end;
    }
    output.extend_from_slice(&source[cursor..]);
    if output.len() as u64 > context.budget.maximum_bytes {
        return Err(AdapterError::BudgetExceeded);
    }
    Ok(output)
}

fn validate_overlay_ranges(
    expected_hash: [u8; 32],
    overlays: &[OverlayUnit],
) -> Result<(), AdapterError> {
    let mut previous_end = 0;
    for overlay in overlays {
        let (start, end) = byte_span(&overlay.source_locator, expected_hash)?;
        if start < previous_end || start > end {
            return Err(AdapterError::InvalidInput(
                "Markdown overlay locators must be ordered and non-overlapping".to_owned(),
            ));
        }
        previous_end = end;
    }
    Ok(())
}

fn validate_overlays_match_source(
    expected_hash: [u8; 32],
    overlays: &[OverlayUnit],
    parsed: &ParsedMarkdown,
    manifest: &AdapterManifest,
) -> Result<(), AdapterError> {
    let stable_unit_resource_key = format!(
        "{}:document/text:identity-v{}",
        manifest.adapter_id, manifest.identity_version
    );
    let by_key = parsed
        .spans
        .iter()
        .enumerate()
        .map(|(index, span)| {
            let previous = index
                .checked_sub(1)
                .and_then(|previous| parsed.spans.get(previous))
                .map(|previous| previous.text.as_str());
            let next = parsed.spans.get(index + 1).map(|next| next.text.as_str());
            let source_unit = SourceUnit::new(
                FORMAT_ID,
                stable_unit_resource_key.clone(),
                span.path.clone(),
                &span.text,
                previous,
                next,
            );
            (source_unit.source_key, span.range.clone())
        })
        .collect::<HashMap<_, _>>();
    for overlay in overlays {
        let expected_range = by_key.get(&overlay.source_unit_key).ok_or_else(|| {
            AdapterError::InvalidInput(
                "overlay references an unknown Markdown source unit".to_owned(),
            )
        })?;
        let (start, end) = byte_span(&overlay.source_locator, expected_hash)?;
        if expected_range.start != start || expected_range.end != end {
            return Err(AdapterError::InvalidInput(
                "overlay locator does not match Markdown source unit".to_owned(),
            ));
        }
    }
    Ok(())
}

fn byte_span(locator: &Locator, expected_hash: [u8; 32]) -> Result<(usize, usize), AdapterError> {
    match locator {
        Locator::ByteSpan {
            object_hash,
            start,
            end,
        } if *object_hash == expected_hash && start <= end => Ok((*start as usize, *end as usize)),
        _ => Err(AdapterError::InvalidInput(
            "source locator does not match export source".to_owned(),
        )),
    }
}

fn validate_plan(
    plan: &ExportPlan,
    input: &ObjectHandle,
    overlays: &[OverlayUnit],
) -> Result<(), AdapterError> {
    if plan.source_object_hash != input.object_hash
        || plan.ordered_source_unit_keys.len() != overlays.len()
        || plan.ordered_overlay_hashes.len() != overlays.len()
        || plan.overlay_hash != hash_overlays(overlays)
    {
        return Err(AdapterError::InvalidInput(
            "overlay changed after export snapshot".to_owned(),
        ));
    }
    for (index, overlay) in overlays.iter().enumerate() {
        if overlay.source_unit_key != plan.ordered_source_unit_keys[index]
            || hash_overlay(overlay) != plan.ordered_overlay_hashes[index]
        {
            return Err(AdapterError::InvalidInput(
                "overlay changed after export snapshot".to_owned(),
            ));
        }
    }
    Ok(())
}

fn hash_overlays(overlays: &[OverlayUnit]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for overlay in overlays {
        hasher.update(hash_overlay(overlay));
    }
    hasher.finalize().into()
}

fn hash_overlay(overlay: &OverlayUnit) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(overlay.source_unit_key);
    let locator = serde_json::to_vec(&overlay.source_locator).expect("locator serializes");
    hasher.update((locator.len() as u64).to_be_bytes());
    hasher.update(locator);
    hasher.update((overlay.translated_text.len() as u64).to_be_bytes());
    hasher.update(overlay.translated_text.as_bytes());
    hasher.finalize().into()
}

fn paginate_items<T: Clone + serde::Serialize>(
    items: Vec<T>,
    cursor: Option<&Cursor>,
    context: &ExecutionContext<'_>,
    manifest: &AdapterManifest,
) -> Result<Page<T>, AdapterError> {
    let start = decode_u64_cursor(cursor)? as usize;
    if start > items.len() {
        return Err(AdapterError::InvalidCursor);
    }
    let node_limit = context.budget.bounded_page_nodes(manifest) as usize;
    let byte_limit = context.budget.bounded_page_bytes(manifest);
    let mut page = Vec::new();
    let mut emitted = 0;
    for item in items.iter().skip(start) {
        let size = serde_json::to_vec(item)
            .map_err(|error| AdapterError::InvalidInput(error.to_string()))?
            .len() as u64;
        if !page.is_empty() && (page.len() >= node_limit || emitted + size > byte_limit) {
            break;
        }
        if size > byte_limit {
            return Err(AdapterError::BudgetExceeded);
        }
        page.push(item.clone());
        emitted += size;
    }
    let next = start + page.len();
    Ok(Page {
        items: page,
        next_cursor: (next < items.len()).then(|| encode_u64_cursor(next as u64)),
        emitted_bytes: emitted,
    })
}

fn id_from_hash(hash: [u8; 32]) -> ResourceId {
    ResourceId::from_bytes(hash[..16].try_into().expect("hash prefix is 16 bytes"))
}

fn hash_text(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn decode_u64_cursor(cursor: Option<&Cursor>) -> Result<u64, AdapterError> {
    match cursor {
        None => Ok(0),
        Some(Cursor(bytes)) if bytes.len() == 8 => Ok(u64::from_be_bytes(
            bytes.as_slice().try_into().expect("checked cursor length"),
        )),
        Some(_) => Err(AdapterError::InvalidCursor),
    }
}

fn encode_u64_cursor(value: u64) -> Cursor {
    Cursor(value.to_be_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use babel_adapter_host::CapabilityRegistry;
    use babel_adapter_protocol::{CancellationToken, TaskBudget};
    use tempfile::TempDir;

    use super::*;

    fn context<'a>(budget: &'a TaskBudget, token: &'a CancellationToken) -> ExecutionContext<'a> {
        ExecutionContext::new(budget, token)
    }

    fn budget(page_nodes: u32, page_bytes: u64) -> TaskBudget {
        TaskBudget {
            timeout_ms: 10_000,
            maximum_bytes: 64 * 1024 * 1024,
            maximum_nodes: 1_000_000,
            page_bytes,
            page_nodes,
        }
    }

    fn fixture(bytes: &[u8]) -> (TempDir, CapabilityRegistry, ObjectHandle) {
        let temp = TempDir::new().unwrap();
        let objects = temp.path().join("objects");
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        let hex = hex::encode(hash);
        let path = objects.join("sha256").join(&hex[..2]).join(&hex[2..]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        let registry = CapabilityRegistry::new(&objects, temp.path().join("staging")).unwrap();
        let handle = registry.grant_object(hash, bytes.len() as u64).unwrap();
        (temp, registry, handle)
    }

    fn extract_all(
        adapter: &MarkdownAdapter,
        handle: &ObjectHandle,
        registry: &CapabilityRegistry,
    ) -> Vec<ExtractedUnit> {
        let token = CancellationToken::default();
        let budget = budget(2, 1024);
        let execution = context(&budget, &token);
        let generation_id = GenerationId::new();
        let inventory = adapter
            .inventory(handle, generation_id, None, registry, &execution)
            .unwrap();
        let resource_id = inventory
            .items
            .into_iter()
            .find_map(|item| match item {
                InventoryItem::Node(node) if node.kind == ResourceKind::TextStream => {
                    Some(node.resource_id)
                }
                _ => None,
            })
            .expect("Markdown inventory contains a text stream");
        let mut cursor = None;
        let mut units = Vec::new();
        loop {
            let page = adapter
                .extract(
                    handle,
                    generation_id,
                    resource_id,
                    cursor.as_ref(),
                    registry,
                    &execution,
                )
                .unwrap();
            units.extend(page.items);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        units
    }

    #[test]
    fn extracts_text_spans_and_image_references() {
        let bytes = b"# Title\n\nHello **world** and ![cover text](images/cover.png).\n";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = MarkdownAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        assert!(units.iter().any(|unit| text(unit) == "Title"));
        assert!(units.iter().any(|unit| text(unit) == "world"));
        assert!(units.iter().any(|unit| text(unit) == "cover text"));
        let token = CancellationToken::default();
        let budget = budget(100, 4096);
        let execution = context(&budget, &token);
        let inventory = adapter
            .inventory(&handle, GenerationId::new(), None, &registry, &execution)
            .unwrap();
        assert!(inventory.items.iter().any(|item| matches!(
            item,
            InventoryItem::Node(ResourceNode {
                kind: ResourceKind::Image,
                semantic_path,
                ..
            }) if semantic_path == "image/images/cover.png"
        )));
    }

    #[test]
    fn byte_patch_export_preserves_unmodified_markdown() {
        let bytes = b"# Title\n\nHello **world** and [site](https://example.test).\n";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = MarkdownAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        let world = units.iter().find(|unit| text(unit) == "world").unwrap();
        let overlays = vec![OverlayUnit {
            source_unit_key: world.source_unit_key,
            source_locator: world.locator.clone(),
            translated_text: "世界".to_owned(),
        }];
        let token = CancellationToken::default();
        let budget = budget(10, 4096);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 3, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap();
        let output = registry.staging_bytes(&staging).unwrap();
        assert_eq!(
            output,
            b"# Title\n\nHello **\xe4\xb8\x96\xe7\x95\x8c** and [site](https://example.test).\n"
        );
        let report = adapter
            .verify_output(&staging, &registry, &execution)
            .unwrap();
        assert!(report.valid);
    }

    #[test]
    fn multibyte_sourcepos_maps_to_utf8_byte_span() {
        let bytes = "开头\n\n你好 **世界**。\n".as_bytes();
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = MarkdownAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        let world = units.iter().find(|unit| text(unit) == "世界").unwrap();
        assert_eq!(
            world.locator,
            Locator::ByteSpan {
                object_hash: handle.object_hash,
                start: 17,
                end: 23,
            }
        );
        let overlays = vec![OverlayUnit {
            source_unit_key: world.source_unit_key,
            source_locator: world.locator.clone(),
            translated_text: "monde".to_owned(),
        }];
        let token = CancellationToken::default();
        let budget = budget(10, 4096);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 3, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap();
        let output = String::from_utf8(registry.staging_bytes(&staging).unwrap()).unwrap();
        assert_eq!(output, "开头\n\n你好 **monde**。\n");
    }

    #[test]
    fn export_refuses_link_label_that_changes_protected_destination() {
        let bytes = b"[label](https://example.test)\n";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = MarkdownAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        let label = units.iter().find(|unit| text(unit) == "label").unwrap();
        let overlays = vec![OverlayUnit {
            source_unit_key: label.source_unit_key,
            source_locator: label.locator.clone(),
            translated_text: "bad](https://evil.test".to_owned(),
        }];
        let token = CancellationToken::default();
        let budget = budget(10, 4096);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 3, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        let error = adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap_err();
        assert!(
            matches!(error, AdapterError::InvalidInput(message) if message.contains("protected structure"))
        );
    }

    #[test]
    fn export_rejects_overlapping_or_changed_snapshot() {
        let bytes = b"alpha beta";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = MarkdownAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        let overlays = units
            .iter()
            .map(|unit| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: text(unit),
            })
            .collect::<Vec<_>>();
        let token = CancellationToken::default();
        let budget = budget(10, 4096);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 3, &overlays, &execution)
            .unwrap();
        let mut changed = overlays.clone();
        changed[0].translated_text = "changed".to_owned();
        let staging = registry.create_staging().unwrap();
        assert!(
            adapter
                .materialize(
                    &plan, &handle, &changed, &staging, None, &registry, &execution
                )
                .is_err()
        );
    }

    fn text(unit: &ExtractedUnit) -> String {
        unit.content
            .tokens
            .iter()
            .find_map(|token| match token {
                Token::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .unwrap()
    }
}
