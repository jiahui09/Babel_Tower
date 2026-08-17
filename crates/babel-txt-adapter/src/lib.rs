//! Production TXT adapter with reversible byte locators and conservative encoding detection.

use std::{
    borrow::Cow,
    io::{Read, Seek, SeekFrom},
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
use encoding_rs::{Encoding, GB18030};
use sha2::{Digest, Sha256};

const ADAPTER_ID: &str = "org.babel-tower.txt";
const ADAPTER_BUILD: &str = "phase3.2";
const FORMAT_ID: &str = "txt";
const READ_CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxtEncoding {
    Utf8,
    Utf8Bom,
    Utf16LeBom,
    Utf16BeBom,
    Legacy(&'static Encoding),
}

#[derive(Clone, Debug)]
pub struct TxtAdapter {
    manifest: AdapterManifest,
    explicit_legacy_encoding: Option<&'static Encoding>,
}

impl Default for TxtAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TxtAdapter {
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
                mime_types: vec!["text/plain".to_owned()],
                extensions: vec!["txt".to_owned()],
                resource_kinds: vec![ResourceKind::Document, ResourceKind::TextStream],
                export_fidelity_tier: "byte-preserving-line-endings".to_owned(),
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
            explicit_legacy_encoding: None,
        }
    }

    pub fn with_explicit_legacy_encoding(label: &str) -> Result<Self, AdapterError> {
        let normalized = label.trim().to_ascii_lowercase();
        let encoding = match normalized.as_str() {
            "gb18030" | "gbk" => GB18030,
            _ => {
                return Err(AdapterError::InvalidInput(format!(
                    "unsupported explicit TXT encoding: {label}"
                )));
            }
        };
        Ok(Self {
            explicit_legacy_encoding: Some(encoding),
            ..Self::new()
        })
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

impl Adapter for TxtAdapter {
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
        let detection = detect_encoding(&bytes, self.explicit_legacy_encoding);
        Ok(match detection {
            Ok(encoding) => ProbeResult {
                confidence_millionths: if matches!(encoding, TxtEncoding::Legacy(_)) {
                    700_000
                } else {
                    990_000
                },
                detected_media_type: Some("text/plain".to_owned()),
                reason_code: encoding.reason_code().to_owned(),
            },
            Err(error) => ProbeResult {
                confidence_millionths: 0,
                detected_media_type: None,
                reason_code: error,
            },
        })
    }

    fn inventory(
        &self,
        input: &ObjectHandle,
        _generation_id: GenerationId,
        cursor: Option<&Cursor>,
        _io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError> {
        context.checkpoint()?;
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
        paginate_items(
            vec![
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
            ],
            cursor,
            context,
            &self.manifest,
        )
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
        context.checkpoint()?;
        let start_index = decode_u64_cursor(cursor)? as usize;
        let bytes = self.read_input(input, io, context)?;
        let encoding = detect_encoding(&bytes, self.explicit_legacy_encoding)
            .map_err(AdapterError::InvalidInput)?;
        let lines = split_lines(&bytes, encoding)?;
        if start_index > lines.len() {
            return Err(AdapterError::InvalidCursor);
        }
        let node_limit = context.budget.bounded_page_nodes(&self.manifest) as usize;
        let byte_limit = context.budget.bounded_page_bytes(&self.manifest);
        let stable_unit_resource_key = format!(
            "{}:document/text:identity-v{}",
            self.manifest.adapter_id, self.manifest.identity_version
        );
        let mut items = Vec::new();
        let mut emitted_bytes = 0_u64;
        for (index, line) in lines.iter().enumerate().skip(start_index) {
            context.checkpoint()?;
            let text = decode_segment(&bytes[line.text_start..line.text_end], encoding)?;
            let size = line.end - line.start;
            if !items.is_empty() && (items.len() >= node_limit || emitted_bytes + size > byte_limit)
            {
                break;
            }
            if size > byte_limit {
                return Err(AdapterError::BudgetExceeded);
            }
            let previous = index
                .checked_sub(1)
                .and_then(|i| lines.get(i))
                .map(|line| decode_segment(&bytes[line.text_start..line.text_end], encoding))
                .transpose()?;
            let next = lines
                .get(index + 1)
                .map(|line| decode_segment(&bytes[line.text_start..line.text_end], encoding))
                .transpose()?;
            let source_unit = SourceUnit::new(
                FORMAT_ID,
                stable_unit_resource_key.clone(),
                vec![format!("line:{index:016x}")],
                &text,
                previous.as_deref(),
                next.as_deref(),
            );
            items.push(ExtractedUnit {
                generation_id,
                resource_id,
                source_unit_key: source_unit.source_key,
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: line.start,
                    end: line.end,
                },
                content: UnitContent {
                    schema_version: TIR_SCHEMA_VERSION,
                    tokens: vec![Token::Text {
                        text,
                        style_hint: None,
                    }],
                },
            });
            emitted_bytes += size;
        }
        let next = start_index + items.len();
        Ok(Page {
            items,
            next_cursor: (next < lines.len()).then(|| encode_u64_cursor(next as u64)),
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
        let overlay_hash = hash_overlays(overlays);
        let ordered_overlay_hashes = overlays.iter().map(hash_overlay).collect::<Vec<_>>();
        let mut plan = Sha256::new();
        plan.update(b"babel-txt-export-plan-v1");
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
        validate_plan(plan, input, overlays)?;
        let bytes = self.read_input(input, io, context)?;
        let encoding = detect_encoding(&bytes, self.explicit_legacy_encoding)
            .map_err(AdapterError::InvalidInput)?;
        let (mut index, offset) = decode_materialize_cursor(cursor)?;
        if index > overlays.len() {
            return Err(AdapterError::InvalidCursor);
        }
        let mut chunk = Vec::new();
        if index == 0 {
            chunk.extend_from_slice(encoding.bom());
        }
        let start_index = index;
        let node_limit = context.budget.bounded_page_nodes(&self.manifest) as usize;
        let byte_limit = context.budget.bounded_page_bytes(&self.manifest) as usize;
        while index < overlays.len() && index - start_index < node_limit {
            context.checkpoint()?;
            let overlay = &overlays[index];
            let (line_start, line_end) = byte_span(&overlay.source_locator, input.object_hash)?;
            if line_end > bytes.len() || line_start > line_end {
                return Err(AdapterError::InvalidInput(
                    "source locator is outside source object".to_owned(),
                ));
            }
            let line =
                split_single_line(&bytes[line_start..line_end], encoding, line_start as u64)?;
            let mut encoded = encode_text(&overlay.translated_text, encoding)?;
            encoded.extend_from_slice(&bytes[line.newline_start as usize..line.end as usize]);
            if !chunk.is_empty() && chunk.len() + encoded.len() > byte_limit {
                break;
            }
            if encoded.len() > byte_limit {
                return Err(AdapterError::BudgetExceeded);
            }
            chunk.extend_from_slice(&encoded);
            index += 1;
        }
        let next_offset = offset + chunk.len() as u64;
        if next_offset > context.budget.maximum_bytes
            || next_offset > self.manifest.safety_limits.maximum_output_bytes
        {
            return Err(AdapterError::BudgetExceeded);
        }
        let mut existing = io.open_staging(staging)?;
        let existing_length = existing.seek(SeekFrom::End(0))?;
        match existing_length {
            length if length == offset => io.write_staging_at(staging, offset, &chunk)?,
            length if length == next_offset => {
                existing.seek(SeekFrom::Start(offset))?;
                let mut written = vec![0; chunk.len()];
                existing.read_exact(&mut written)?;
                if written != chunk {
                    return Err(AdapterError::InvalidCursor);
                }
            }
            _ => return Err(AdapterError::InvalidCursor),
        }
        let complete = index == overlays.len();
        Ok(MaterializeProgress {
            next_cursor: (!complete).then(|| encode_materialize_cursor(index, next_offset)),
            bytes_written: chunk.len() as u64,
            complete,
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
        match detect_encoding(&bytes, self.explicit_legacy_encoding) {
            Ok(encoding) => {
                if split_lines(&bytes, encoding).is_err() {
                    issue_codes.push("invalid-text".to_owned());
                }
            }
            Err(reason) => issue_codes.push(reason),
        }
        Ok(VerificationReport {
            valid: issue_codes.is_empty(),
            output_hash: Sha256::digest(&bytes).into(),
            byte_length: bytes.len() as u64,
            issue_codes,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct LineSpan {
    start: u64,
    text_start: usize,
    text_end: usize,
    newline_start: u64,
    end: u64,
}

impl TxtEncoding {
    fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8Bom => &[0xef, 0xbb, 0xbf],
            Self::Utf16LeBom => &[0xff, 0xfe],
            Self::Utf16BeBom => &[0xfe, 0xff],
            Self::Utf8 | Self::Legacy(_) => &[],
        }
    }

    fn body_start(self) -> usize {
        self.bom().len()
    }

    fn reason_code(self) -> &'static str {
        match self {
            Self::Utf8 => "strict-utf8",
            Self::Utf8Bom => "utf8-bom",
            Self::Utf16LeBom => "utf16le-bom",
            Self::Utf16BeBom => "utf16be-bom",
            Self::Legacy(_) => "explicit-legacy-encoding",
        }
    }
}

fn detect_encoding(
    bytes: &[u8],
    explicit_legacy: Option<&'static Encoding>,
) -> Result<TxtEncoding, String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        decode_segment(&bytes[3..], TxtEncoding::Utf8Bom).map_err(|_| "invalid-utf8-bom")?;
        return Ok(TxtEncoding::Utf8Bom);
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        decode_segment(&bytes[2..], TxtEncoding::Utf16LeBom).map_err(|_| "invalid-utf16le-bom")?;
        return Ok(TxtEncoding::Utf16LeBom);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        decode_segment(&bytes[2..], TxtEncoding::Utf16BeBom).map_err(|_| "invalid-utf16be-bom")?;
        return Ok(TxtEncoding::Utf16BeBom);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Ok(TxtEncoding::Utf8);
    }
    if let Some(encoding) = explicit_legacy {
        let (_, had_errors) = encoding.decode_without_bom_handling(bytes);
        return if had_errors {
            Err("invalid-explicit-legacy-encoding".to_owned())
        } else {
            Ok(TxtEncoding::Legacy(encoding))
        };
    }
    Err("ambiguous-non-utf8-requires-explicit-encoding".to_owned())
}

fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
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

fn split_lines(bytes: &[u8], encoding: TxtEncoding) -> Result<Vec<LineSpan>, AdapterError> {
    match encoding {
        TxtEncoding::Utf16LeBom => split_utf16_lines(bytes, true),
        TxtEncoding::Utf16BeBom => split_utf16_lines(bytes, false),
        TxtEncoding::Utf8 | TxtEncoding::Utf8Bom | TxtEncoding::Legacy(_) => {
            split_byte_lines(bytes, encoding)
        }
    }
}

fn split_byte_lines(bytes: &[u8], encoding: TxtEncoding) -> Result<Vec<LineSpan>, AdapterError> {
    let mut result = Vec::new();
    let mut start = encoding.body_start();
    let mut index = start;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' || byte == b'\r' {
            let newline_start = index;
            index += 1;
            if byte == b'\r' && bytes.get(index) == Some(&b'\n') {
                index += 1;
            }
            result.push(LineSpan {
                start: start as u64,
                text_start: start,
                text_end: newline_start,
                newline_start: newline_start as u64,
                end: index as u64,
            });
            start = index;
        } else {
            index += 1;
        }
    }
    if start < bytes.len() {
        result.push(LineSpan {
            start: start as u64,
            text_start: start,
            text_end: bytes.len(),
            newline_start: bytes.len() as u64,
            end: bytes.len() as u64,
        });
    }
    for line in &result {
        decode_segment(&bytes[line.text_start..line.text_end], encoding)?;
    }
    Ok(result)
}

fn split_utf16_lines(bytes: &[u8], little_endian: bool) -> Result<Vec<LineSpan>, AdapterError> {
    split_utf16_lines_from(bytes, little_endian, 2)
}

fn split_utf16_lines_from(
    bytes: &[u8],
    little_endian: bool,
    body_start: usize,
) -> Result<Vec<LineSpan>, AdapterError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(AdapterError::InvalidInput(
            "UTF-16 byte length is odd".to_owned(),
        ));
    }
    let mut result = Vec::new();
    let mut start = body_start;
    let mut index = start;
    while index < bytes.len() {
        let unit = read_u16(&bytes[index..index + 2], little_endian);
        if unit == 0x000a || unit == 0x000d {
            let newline_start = index;
            index += 2;
            if unit == 0x000d && index < bytes.len() {
                let next = read_u16(&bytes[index..index + 2], little_endian);
                if next == 0x000a {
                    index += 2;
                }
            }
            result.push(LineSpan {
                start: start as u64,
                text_start: start,
                text_end: newline_start,
                newline_start: newline_start as u64,
                end: index as u64,
            });
            start = index;
        } else {
            index += 2;
        }
    }
    if start < bytes.len() {
        result.push(LineSpan {
            start: start as u64,
            text_start: start,
            text_end: bytes.len(),
            newline_start: bytes.len() as u64,
            end: bytes.len() as u64,
        });
    }
    for line in &result {
        decode_segment(
            &bytes[line.text_start..line.text_end],
            if little_endian {
                TxtEncoding::Utf16LeBom
            } else {
                TxtEncoding::Utf16BeBom
            },
        )?;
    }
    Ok(result)
}

fn split_single_line(
    bytes: &[u8],
    encoding: TxtEncoding,
    base: u64,
) -> Result<LineSpan, AdapterError> {
    let mut lines = match encoding {
        TxtEncoding::Utf16LeBom => split_utf16_lines_from(bytes, true, 0)?,
        TxtEncoding::Utf16BeBom => split_utf16_lines_from(bytes, false, 0)?,
        TxtEncoding::Utf8Bom => split_byte_lines(bytes, TxtEncoding::Utf8)?,
        other => split_lines(bytes, other)?,
    };
    if lines.len() != 1 || lines[0].start != 0 {
        return Err(AdapterError::InvalidInput(
            "source locator must contain exactly one TXT line".to_owned(),
        ));
    }
    let mut line = lines.remove(0);
    line.start += base;
    line.newline_start += base;
    line.end += base;
    Ok(line)
}

fn decode_segment(bytes: &[u8], encoding: TxtEncoding) -> Result<String, AdapterError> {
    match encoding {
        TxtEncoding::Utf8 | TxtEncoding::Utf8Bom => std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| AdapterError::InvalidInput("invalid UTF-8".to_owned())),
        TxtEncoding::Utf16LeBom | TxtEncoding::Utf16BeBom => {
            if !bytes.len().is_multiple_of(2) {
                return Err(AdapterError::InvalidInput(
                    "UTF-16 segment has odd byte length".to_owned(),
                ));
            }
            let little_endian = matches!(encoding, TxtEncoding::Utf16LeBom);
            let units = bytes
                .chunks_exact(2)
                .map(|chunk| read_u16(chunk, little_endian))
                .collect::<Vec<_>>();
            String::from_utf16(&units)
                .map_err(|_| AdapterError::InvalidInput("invalid UTF-16".to_owned()))
        }
        TxtEncoding::Legacy(encoding) => {
            let (decoded, had_errors) = encoding.decode_without_bom_handling(bytes);
            if had_errors {
                Err(AdapterError::InvalidInput(
                    "invalid explicit legacy encoding".to_owned(),
                ))
            } else {
                Ok(decoded.into_owned())
            }
        }
    }
}

fn encode_text(text: &str, encoding: TxtEncoding) -> Result<Vec<u8>, AdapterError> {
    match encoding {
        TxtEncoding::Utf8 | TxtEncoding::Utf8Bom => Ok(text.as_bytes().to_vec()),
        TxtEncoding::Utf16LeBom => Ok(text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()),
        TxtEncoding::Utf16BeBom => Ok(text
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()),
        TxtEncoding::Legacy(encoding) => {
            let (encoded, _, had_errors) = encoding.encode(text);
            if had_errors {
                return Err(AdapterError::InvalidInput(
                    "translation contains characters not representable in explicit legacy encoding"
                        .to_owned(),
                ));
            }
            Ok(match encoded {
                Cow::Borrowed(bytes) => bytes.to_vec(),
                Cow::Owned(bytes) => bytes,
            })
        }
    }
}

fn read_u16(bytes: &[u8], little_endian: bool) -> u16 {
    let pair = [bytes[0], bytes[1]];
    if little_endian {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    }
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

fn decode_materialize_cursor(cursor: Option<&Cursor>) -> Result<(usize, u64), AdapterError> {
    match cursor {
        None => Ok((0, 0)),
        Some(Cursor(bytes)) if bytes.len() == 16 => {
            let index = u64::from_be_bytes(bytes[..8].try_into().expect("checked cursor length"));
            let offset = u64::from_be_bytes(bytes[8..].try_into().expect("checked cursor length"));
            Ok((index as usize, offset))
        }
        Some(_) => Err(AdapterError::InvalidCursor),
    }
}

fn encode_materialize_cursor(index: usize, offset: u64) -> Cursor {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&(index as u64).to_be_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    Cursor(bytes)
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
        adapter: &TxtAdapter,
        handle: &ObjectHandle,
        registry: &CapabilityRegistry,
    ) -> Vec<ExtractedUnit> {
        let token = CancellationToken::default();
        let budget = budget(2, 1024);
        let execution = context(&budget, &token);
        let mut cursor = None;
        let mut units = Vec::new();
        loop {
            let page = adapter
                .extract(
                    handle,
                    GenerationId::new(),
                    ResourceId::new(),
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
    fn utf8_bom_and_mixed_newlines_roundtrip_shape() {
        let bytes = b"\xef\xbb\xbfone\r\ntwo\nthree\rfour";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = TxtAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        assert_eq!(units.len(), 4);
        assert_eq!(text(&units[0]), "one");
        assert_eq!(
            units[0].locator,
            Locator::ByteSpan {
                object_hash: handle.object_hash,
                start: 3,
                end: 8
            }
        );

        let overlays = units
            .iter()
            .enumerate()
            .map(|(index, unit)| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: format!("T{index}"),
            })
            .collect::<Vec<_>>();
        let token = CancellationToken::default();
        let budget = budget(1, 8);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 7, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        let mut cursor = None;
        loop {
            let page = adapter
                .materialize(
                    &plan,
                    &handle,
                    &overlays,
                    &staging,
                    cursor.as_ref(),
                    &registry,
                    &execution,
                )
                .unwrap();
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(
            registry.staging_bytes(&staging).unwrap(),
            b"\xef\xbb\xbfT0\r\nT1\nT2\rT3"
        );
    }

    #[test]
    fn utf16le_bom_extracts_and_exports() {
        let mut bytes = vec![0xff, 0xfe];
        for unit in "one\r\ntwo".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (_temp, registry, handle) = fixture(&bytes);
        let adapter = TxtAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        assert_eq!(units.len(), 2);
        assert_eq!(text(&units[1]), "two");
        let overlays = units
            .iter()
            .map(|unit| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: "译".to_owned(),
            })
            .collect::<Vec<_>>();
        let token = CancellationToken::default();
        let budget = budget(10, 1024);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 1, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap();
        let output = registry.staging_bytes(&staging).unwrap();
        assert!(output.starts_with(&[0xff, 0xfe]));
        assert!(
            adapter
                .verify_output(&staging, &registry, &execution)
                .unwrap()
                .valid
        );
    }

    #[test]
    fn utf16be_bom_is_detected_and_preserves_newlines() {
        let mut bytes = vec![0xfe, 0xff];
        for unit in "甲\r\n乙\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        let (_temp, registry, handle) = fixture(&bytes);
        let adapter = TxtAdapter::new();
        let token = CancellationToken::default();
        let budget = budget(10, 1024);
        let execution = context(&budget, &token);
        let probe = adapter.probe(&handle, &registry, &execution).unwrap();
        assert_eq!(probe.reason_code, "utf16be-bom");

        let units = extract_all(&adapter, &handle, &registry);
        assert_eq!(units.len(), 2);
        assert_eq!(text(&units[0]), "甲");
        assert_eq!(text(&units[1]), "乙");
        let overlays = units
            .iter()
            .map(|unit| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: text(unit),
            })
            .collect::<Vec<_>>();
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 1, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap();
        assert_eq!(registry.staging_bytes(&staging).unwrap(), bytes);
    }

    #[test]
    fn empty_file_and_final_newline_do_not_create_phantom_units() {
        let (_temp, registry, handle) = fixture(b"");
        assert!(extract_all(&TxtAdapter::new(), &handle, &registry).is_empty());

        let bytes = b"one\n\ntwo\n";
        let (_temp, registry, handle) = fixture(bytes);
        let units = extract_all(&TxtAdapter::new(), &handle, &registry);
        assert_eq!(units.len(), 3);
        assert_eq!(text(&units[0]), "one");
        assert_eq!(text(&units[1]), "");
        assert_eq!(text(&units[2]), "two");
    }

    #[test]
    fn final_newline_is_preserved_when_exporting_last_unit() {
        let bytes = b"one\ntwo\n";
        let (_temp, registry, handle) = fixture(bytes);
        let adapter = TxtAdapter::new();
        let units = extract_all(&adapter, &handle, &registry);
        assert_eq!(units.len(), 2);
        let overlays = units
            .iter()
            .enumerate()
            .map(|(index, unit)| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: format!("T{index}"),
            })
            .collect::<Vec<_>>();
        let token = CancellationToken::default();
        let budget = budget(10, 1024);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 1, &overlays, &execution)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &execution,
            )
            .unwrap();
        assert_eq!(registry.staging_bytes(&staging).unwrap(), b"T0\nT1\n");
    }

    #[test]
    fn ambiguous_legacy_bytes_are_rejected_without_explicit_encoding() {
        let gb = GB18030.encode("中文").0.into_owned();
        let (_temp, registry, handle) = fixture(&gb);
        let adapter = TxtAdapter::new();
        let token = CancellationToken::default();
        let budget = budget(10, 1024);
        let execution = context(&budget, &token);
        let probe = adapter.probe(&handle, &registry, &execution).unwrap();
        assert_eq!(
            probe.reason_code,
            "ambiguous-non-utf8-requires-explicit-encoding"
        );
        let explicit = TxtAdapter::with_explicit_legacy_encoding("gb18030").unwrap();
        let units = extract_all(&explicit, &handle, &registry);
        assert_eq!(text(&units[0]), "中文");
    }

    #[test]
    fn binary_nul_is_not_misdetected_as_text() {
        let (_temp, registry, handle) = fixture(b"abc\0def");
        let adapter = TxtAdapter::new();
        let token = CancellationToken::default();
        let budget = budget(10, 1024);
        let execution = context(&budget, &token);
        let probe = adapter.probe(&handle, &registry, &execution).unwrap();
        assert_eq!(probe.detected_media_type, None);
    }

    #[test]
    fn export_snapshot_rejects_live_overlay_change_and_staging_mismatch() {
        let (_temp, registry, handle) = fixture(b"one\ntwo\n");
        let adapter = TxtAdapter::new();
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
        let budget = budget(1, 128);
        let execution = context(&budget, &token);
        let plan = adapter
            .plan_export(&handle, GenerationId::new(), 1, &overlays, &execution)
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

    #[test]
    fn ten_mib_corpus_extracts_one_hundred_thousand_lines() {
        let mut bytes = Vec::new();
        for index in 0..100_000 {
            bytes.extend_from_slice(format!("line-{index:06}: ").as_bytes());
            bytes.extend_from_slice(
                b"Babel Tower keeps long offline translation work deterministic and recoverable.",
            );
            bytes.extend_from_slice(b" Context matters. Manual text stays authoritative.\n");
        }
        assert!(bytes.len() >= 10 * 1024 * 1024);
        let (_temp, registry, handle) = fixture(&bytes);
        let adapter = TxtAdapter::new();
        let token = CancellationToken::default();
        let budget = TaskBudget {
            timeout_ms: 30_000,
            maximum_bytes: 64 * 1024 * 1024,
            maximum_nodes: 200_000,
            page_bytes: 64 * 1024 * 1024,
            page_nodes: 100_000,
        };
        let execution = context(&budget, &token);
        let mut cursor = None;
        let mut items = Vec::new();
        loop {
            let page = adapter
                .extract(
                    &handle,
                    GenerationId::new(),
                    ResourceId::new(),
                    cursor.as_ref(),
                    &registry,
                    &execution,
                )
                .unwrap();
            items.extend(page.items);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(items.len(), 100_000);
        assert_eq!(
            text(items.first().unwrap()),
            "line-000000: Babel Tower keeps long offline translation work deterministic and recoverable. Context matters. Manual text stays authoritative."
        );
        assert_eq!(
            text(items.last().unwrap()),
            "line-099999: Babel Tower keeps long offline translation work deterministic and recoverable. Context matters. Manual text stays authoritative."
        );
    }

    fn text(unit: &ExtractedUnit) -> String {
        match &unit.content.tokens[0] {
            Token::Text { text, .. } => text.clone(),
            _ => unreachable!(),
        }
    }
}
