//! EPUB 2/3 adapter with bounded container parsing and member-local byte patches.

use std::{
    collections::{HashMap, HashSet},
    io::{Cursor as IoCursor, Read, Seek, SeekFrom, Write},
    ops::Range,
    path::Path,
};

use babel_adapter_protocol::{
    ADAPTER_PROTOCOL_MAJOR, ADAPTER_PROTOCOL_MINOR, Adapter, AdapterError, AdapterManifest,
    CapabilityIo, Cursor, ExecutionContext, ExportPlan, ExtractedUnit, ImageOverlay, InventoryItem,
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
use image::DynamicImage;
use quick_xml::{Reader, XmlVersion, escape, events::Event};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const ADAPTER_ID: &str = "org.babel-tower.epub";
const ADAPTER_BUILD: &str = "phase5.0";
const FORMAT_ID: &str = "epub";
const EPUB_MEDIA_TYPE: &str = "application/epub+zip";
const MAX_ENTRIES: usize = 20_000;
const MAX_EXPANDED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 512 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 32 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 1_000;
const MAX_XML_DEPTH: usize = 256;
const MAX_XML_ATTRIBUTES: usize = 1_024;
const MAX_XML_TEXT_NODES: usize = 1_000_000;

#[derive(Clone, Debug)]
pub struct EpubAdapter {
    manifest: AdapterManifest,
}

#[derive(Clone, Debug)]
pub struct PreparedEpub {
    source_hash: [u8; 32],
    book: Book,
}

impl Default for EpubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl EpubAdapter {
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
                mime_types: vec![EPUB_MEDIA_TYPE.to_owned()],
                extensions: vec!["epub".to_owned()],
                resource_kinds: vec![
                    ResourceKind::Container,
                    ResourceKind::Document,
                    ResourceKind::TextStream,
                    ResourceKind::Image,
                    ResourceKind::Font,
                    ResourceKind::Stylesheet,
                    ResourceKind::BinaryAttachment,
                ],
                export_fidelity_tier: "member-preserving-epub-xhtml-spans".to_owned(),
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
                    maximum_output_bytes: 2 * 1024 * 1024 * 1024,
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
        let prepared = self.prepare(input, io, context)?;
        let mut cursor = None;
        let mut units = Vec::new();
        loop {
            let page = self.extract_prepared(
                &prepared,
                input,
                generation_id,
                resource_id,
                cursor.as_ref(),
                io,
                context,
            )?;
            units.extend(page.items);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(units);
            }
        }
    }

    pub fn extract_all_resources(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<HashMap<[u8; 16], Vec<ExtractedUnit>>, AdapterError> {
        let book = self.parse_book(input, io, context)?;
        let mut archive = ZipArchive::new(io.open_object(input)?).map_err(zip_input)?;
        let mut resources = HashMap::with_capacity(book.documents.len());
        for document in &book.documents {
            context.checkpoint()?;
            let resource_id = document.stream_id(input.object_hash);
            let bytes = read_member(&mut archive, &document.path, MAX_MEMBER_BYTES, context)?;
            let spans = parse_xhtml_spans(&bytes, context)?;
            resources.insert(
                *resource_id.as_bytes(),
                extracted_document_units(
                    input.object_hash,
                    generation_id,
                    resource_id,
                    document,
                    &spans,
                    context,
                )?,
            );
        }
        Ok(resources)
    }

    pub fn prepare(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<PreparedEpub, AdapterError> {
        Ok(PreparedEpub {
            source_hash: input.object_hash,
            book: self.parse_book(input, io, context)?,
        })
    }

    pub fn inventory_prepared(
        &self,
        prepared: &PreparedEpub,
        input: &ObjectHandle,
        cursor: Option<&Cursor>,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError> {
        validate_prepared(prepared, input)?;
        paginate(
            inventory_items(
                &prepared.book,
                input.object_hash,
                self.manifest.identity_version,
            ),
            cursor,
            context,
            &self.manifest,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn extract_prepared(
        &self,
        prepared: &PreparedEpub,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<ExtractedUnit>, AdapterError> {
        paginate(
            self.extract_resource_prepared(
                prepared,
                input,
                generation_id,
                resource_id,
                io,
                context,
            )?,
            cursor,
            context,
            &self.manifest,
        )
    }

    pub fn extract_resource_prepared(
        &self,
        prepared: &PreparedEpub,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Vec<ExtractedUnit>, AdapterError> {
        validate_prepared(prepared, input)?;
        let document = prepared
            .book
            .documents
            .iter()
            .find(|document| document.stream_id(input.object_hash) == resource_id)
            .ok_or_else(|| {
                AdapterError::InvalidInput(
                    "EPUB extraction requires a spine text stream".to_owned(),
                )
            })?;
        let mut archive = ZipArchive::new(io.open_object(input)?).map_err(zip_input)?;
        let bytes = read_member(&mut archive, &document.path, MAX_MEMBER_BYTES, context)?;
        let spans = parse_xhtml_spans(&bytes, context)?;
        extracted_document_units(
            input.object_hash,
            generation_id,
            resource_id,
            document,
            &spans,
            context,
        )
    }

    fn parse_book(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Book, AdapterError> {
        context.checkpoint()?;
        if input.byte_length > self.manifest.safety_limits.maximum_input_bytes {
            return Err(AdapterError::BudgetExceeded);
        }
        parse_archive(
            ZipArchive::new(io.open_object(input)?).map_err(zip_input)?,
            context,
        )
    }
}

impl Adapter for EpubAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn probe(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<ProbeResult, AdapterError> {
        match self.parse_book(input, io, context) {
            Ok(_) => Ok(ProbeResult {
                confidence_millionths: 1_000_000,
                detected_media_type: Some(EPUB_MEDIA_TYPE.to_owned()),
                reason_code: "epub-ocf-container".to_owned(),
            }),
            Err(AdapterError::InvalidInput(reason)) => Ok(ProbeResult {
                confidence_millionths: 0,
                detected_media_type: None,
                reason_code: reason,
            }),
            Err(error) => Err(error),
        }
    }

    fn inventory(
        &self,
        input: &ObjectHandle,
        _generation_id: GenerationId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError> {
        let prepared = self.prepare(input, io, context)?;
        self.inventory_prepared(&prepared, input, cursor, context)
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
        let prepared = self.prepare(input, io, context)?;
        self.extract_prepared(
            &prepared,
            input,
            generation_id,
            resource_id,
            cursor,
            io,
            context,
        )
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
        validate_overlay_locators(input.object_hash, overlays)?;
        let overlay_hash = hash_overlays(overlays);
        let ordered_overlay_hashes = overlays.iter().map(hash_overlay).collect::<Vec<_>>();
        let mut plan = Sha256::new();
        plan.update(b"babel-epub-export-plan-v1");
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
        let book = self.parse_book(input, io, context)?;
        let patches = build_member_patches(&book, input, overlays, io, context)?;
        let reader = io.open_object(input)?;
        let mut archive = ZipArchive::new(reader).map_err(zip_input)?;
        let output = io.open_staging_writer(staging)?;
        let mut writer = ZipWriter::new(output);
        for index in 0..archive.len() {
            context.checkpoint()?;
            let entry = archive.by_index(index).map_err(zip_input)?;
            let name = entry.name().to_owned();
            if let Some(bytes) = patches.get(&name) {
                let options = SimpleFileOptions::default()
                    .compression_method(entry.compression())
                    .last_modified_time(entry.last_modified().unwrap_or_default());
                writer.start_file(&name, options).map_err(zip_input)?;
                writer.write_all(bytes)?;
            } else {
                writer.raw_copy_file(entry).map_err(zip_input)?;
            }
        }
        let mut output = writer.finish().map_err(zip_input)?;
        let output_len = output.seek(SeekFrom::End(0))?;
        output.flush()?;
        if output_len > self.manifest.safety_limits.maximum_output_bytes
            || output_len > context.budget.maximum_bytes
        {
            return Err(AdapterError::BudgetExceeded);
        }
        Ok(MaterializeProgress {
            next_cursor: None,
            bytes_written: output_len,
            complete: true,
        })
    }

    fn apply_image_overlays(
        &self,
        _plan: &ExportPlan,
        input: &ObjectHandle,
        overlays: &[ImageOverlay],
        staging: &StagingHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<(), AdapterError> {
        let mut replacements = HashMap::<String, Vec<&ImageOverlay>>::new();
        for overlay in overlays {
            let Locator::ArchiveMemberByteSpan {
                object_hash,
                member_path,
                ..
            } = &overlay.source_locator
            else {
                return Err(invalid(
                    "EPUB image overlay requires an archive member locator",
                ));
            };
            if *object_hash != input.object_hash {
                return Err(invalid(
                    "EPUB image overlay references a different source object",
                ));
            }
            let Locator::SpatialRegion { .. } = &overlay.region_locator else {
                return Err(invalid(
                    "EPUB image overlay requires a spatial region locator",
                ));
            };
            replacements
                .entry(member_path.clone())
                .or_default()
                .push(overlay);
        }
        let mut staging_reader = io.open_staging(staging)?;
        let mut staging_bytes = Vec::new();
        staging_reader.read_to_end(&mut staging_bytes)?;
        let mut archive = ZipArchive::new(IoCursor::new(staging_bytes)).map_err(zip_input)?;
        let output = io.open_staging_writer(staging)?;
        let mut writer = ZipWriter::new(output);
        let mut replaced = HashSet::new();
        for index in 0..archive.len() {
            context.checkpoint()?;
            let mut entry = archive.by_index(index).map_err(zip_input)?;
            let name = entry.name().to_owned();
            let options = SimpleFileOptions::default()
                .compression_method(entry.compression())
                .last_modified_time(entry.last_modified().unwrap_or_default());
            if let Some(overlays) = replacements.get(&name) {
                let mut source_bytes = Vec::new();
                entry.read_to_end(&mut source_bytes)?;
                let bytes = compose_image_overlays(&source_bytes, overlays, io, context)?;
                if bytes.is_empty() || bytes.len() as u64 > MAX_MEMBER_BYTES {
                    return Err(AdapterError::BudgetExceeded);
                }
                writer.start_file(&name, options).map_err(zip_input)?;
                writer.write_all(&bytes)?;
                replaced.insert(name);
            } else {
                writer.raw_copy_file(entry).map_err(zip_input)?;
            }
        }
        if replaced.len() != replacements.len() {
            return Err(invalid(
                "EPUB image overlay member is missing from the exported archive",
            ));
        }
        writer.finish().map_err(zip_input)?;
        Ok(())
    }

    fn verify_output(
        &self,
        candidate: &StagingHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<VerificationReport, AdapterError> {
        context.checkpoint()?;
        let mut reader = io.open_staging(candidate)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut byte_length = 0_u64;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            byte_length = byte_length
                .checked_add(read as u64)
                .ok_or(AdapterError::BudgetExceeded)?;
            if byte_length > self.manifest.safety_limits.maximum_output_bytes
                || byte_length > context.budget.maximum_bytes
            {
                return Err(AdapterError::BudgetExceeded);
            }
            hasher.update(&buffer[..read]);
        }
        reader.seek(SeekFrom::Start(0))?;
        let valid = parse_archive(ZipArchive::new(reader).map_err(zip_input)?, context).is_ok();
        Ok(VerificationReport {
            valid,
            output_hash: hasher.finalize().into(),
            byte_length,
            issue_codes: if valid {
                Vec::new()
            } else {
                vec!["invalid-epub-candidate".to_owned()]
            },
        })
    }
}

#[derive(Clone, Debug)]
struct EntryMeta {
    path: String,
    size: u64,
    media_type: Option<String>,
}

#[derive(Clone, Debug)]
struct ManifestItem {
    id: String,
    path: String,
    media_type: String,
    properties: String,
}

#[derive(Clone, Debug)]
struct Package {
    version_major: u8,
    manifest: Vec<ManifestItem>,
    spine: Vec<String>,
    spine_toc: Option<String>,
}

#[derive(Clone, Debug)]
struct Document {
    path: String,
}

impl Document {
    fn document_id(&self, source_hash: [u8; 32]) -> ResourceId {
        id_from_hash(resource_key(
            &source_hash,
            ADAPTER_ID,
            IDENTITY_VERSION,
            &format!("document/{}", self.path),
        ))
    }

    fn stream_id(&self, source_hash: [u8; 32]) -> ResourceId {
        id_from_hash(resource_key(
            &source_hash,
            ADAPTER_ID,
            IDENTITY_VERSION,
            &format!("text/{}", self.path),
        ))
    }
}

#[derive(Clone, Debug)]
struct Book {
    package_path: String,
    entries: Vec<EntryMeta>,
    documents: Vec<Document>,
    references: Vec<(String, String)>,
}

fn validate_prepared(prepared: &PreparedEpub, input: &ObjectHandle) -> Result<(), AdapterError> {
    if prepared.source_hash != input.object_hash {
        return Err(invalid(
            "prepared EPUB belongs to a different source object",
        ));
    }
    Ok(())
}

fn parse_archive<R: Read + Seek>(
    mut archive: ZipArchive<R>,
    context: &ExecutionContext<'_>,
) -> Result<Book, AdapterError> {
    validate_archive_entries(&mut archive, context)?;
    let mimetype = read_member(&mut archive, "mimetype", 128, context)?;
    if mimetype != EPUB_MEDIA_TYPE.as_bytes() {
        return Err(invalid("EPUB mimetype content is invalid"));
    }
    let container = read_member(
        &mut archive,
        "META-INF/container.xml",
        MAX_XML_BYTES,
        context,
    )?;
    let package_path = parse_container(&container)?;
    let package = read_member(&mut archive, &package_path, MAX_XML_BYTES, context)?;
    let package = parse_package(&package, &package_path)?;
    let by_id = package
        .manifest
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    for item in &package.manifest {
        if !archive_contains(&mut archive, &item.path) {
            return Err(invalid("EPUB manifest references a missing member"));
        }
    }
    let navigation = if package.version_major == 3 {
        package
            .manifest
            .iter()
            .find(|item| {
                item.properties
                    .split_ascii_whitespace()
                    .any(|value| value == "nav")
            })
            .ok_or_else(|| invalid("EPUB 3 package has no navigation document"))?
    } else {
        let toc = package
            .spine_toc
            .as_deref()
            .ok_or_else(|| invalid("EPUB 2 spine has no NCX toc reference"))?;
        let item = by_id
            .get(toc)
            .ok_or_else(|| invalid("EPUB 2 spine references a missing NCX manifest item"))?;
        if item.media_type != "application/x-dtbncx+xml" {
            return Err(invalid("EPUB 2 toc item is not NCX"));
        }
        item
    };
    let mut documents = Vec::with_capacity(package.spine.len());
    for idref in &package.spine {
        let item = by_id
            .get(idref.as_str())
            .ok_or_else(|| invalid("EPUB spine references a missing manifest item"))?;
        if !is_xhtml_media_type(&item.media_type) {
            return Err(invalid("EPUB spine item is not XHTML/XML"));
        }
        documents.push(Document {
            path: item.path.clone(),
        });
    }
    if documents.is_empty() {
        return Err(invalid("EPUB spine is empty"));
    }
    let media_types = package
        .manifest
        .iter()
        .map(|item| (item.path.as_str(), item.media_type.as_str()))
        .collect::<HashMap<_, _>>();
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(zip_input)?;
        if !entry.is_dir() {
            entries.push(EntryMeta {
                path: entry.name().to_owned(),
                size: entry.size(),
                media_type: media_types
                    .get(entry.name())
                    .map(|value| (*value).to_owned()),
            });
        }
    }
    let entry_paths = entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<HashSet<_>>();
    let mut references = Vec::new();
    for item in &package.manifest {
        if !is_xml_reference_source(&item.media_type) {
            continue;
        }
        let bytes = read_member(&mut archive, &item.path, MAX_MEMBER_BYTES, context)?;
        let base = Path::new(&item.path)
            .parent()
            .and_then(Path::to_str)
            .unwrap_or("");
        for target in parse_xml_references(&bytes, base, context)? {
            if !entry_paths.contains(target.as_str()) {
                return Err(invalid("EPUB XML references a missing member"));
            }
            if target != item.path {
                references.push((item.path.clone(), target));
            }
        }
    }
    if !references.iter().any(|(from, _)| from == &navigation.path) {
        return Err(invalid("EPUB navigation document has no internal targets"));
    }
    Ok(Book {
        package_path,
        entries,
        documents,
        references,
    })
}

fn validate_archive_entries<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    context: &ExecutionContext<'_>,
) -> Result<(), AdapterError> {
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Err(AdapterError::BudgetExceeded);
    }
    let mut names = HashSet::with_capacity(archive.len());
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        context.checkpoint()?;
        let entry = archive.by_index(index).map_err(zip_input)?;
        std::str::from_utf8(entry.name_raw())
            .map_err(|_| invalid("EPUB member names must be UTF-8"))?;
        let name = entry.name().to_owned();
        validate_member_path(&name)?;
        if !names.insert(name.clone()) {
            return Err(invalid("EPUB contains duplicate member paths"));
        }
        if entry.encrypted() {
            return Err(invalid("encrypted EPUB members are not supported"));
        }
        if !entry.is_dir()
            && !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(invalid(
                "EPUB member uses an unsupported compression method",
            ));
        }
        if entry.size() > MAX_MEMBER_BYTES {
            return Err(AdapterError::BudgetExceeded);
        }
        expanded = expanded
            .checked_add(entry.size())
            .ok_or(AdapterError::BudgetExceeded)?;
        if expanded > MAX_EXPANDED_BYTES || expanded > context.budget.maximum_bytes {
            return Err(AdapterError::BudgetExceeded);
        }
        if entry.compressed_size() == 0 {
            if entry.size() != 0 {
                return Err(AdapterError::BudgetExceeded);
            }
        } else if entry.size() / entry.compressed_size() > MAX_COMPRESSION_RATIO {
            return Err(AdapterError::BudgetExceeded);
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid("symbolic-link EPUB members are not supported"));
        }
        if index == 0 && (name != "mimetype" || entry.compression() != CompressionMethod::Stored) {
            return Err(invalid("EPUB mimetype must be the first stored member"));
        }
    }
    Ok(())
}

fn validate_member_path(path: &str) -> Result<(), AdapterError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.as_bytes().contains(&0)
        || path.split('/').any(|part| part == "." || part == "..")
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(invalid("EPUB member path is unsafe"));
    }
    Ok(())
}

fn parse_container(bytes: &[u8]) -> Result<String, AdapterError> {
    validate_xml_limits(bytes)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    loop {
        match reader.read_event().map_err(xml_input)? {
            Event::Start(start) | Event::Empty(start)
                if local_name(start.name().as_ref()) == b"rootfile" =>
            {
                if let Some(path) = attribute(&reader, &start, b"full-path")? {
                    validate_member_path(&path)?;
                    return Ok(path);
                }
            }
            Event::DocType(_) => {
                return Err(invalid("DOCTYPE is not allowed in EPUB container XML"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(invalid("EPUB container has no rootfile"))
}

fn parse_package(bytes: &[u8], package_path: &str) -> Result<Package, AdapterError> {
    validate_xml_limits(bytes)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let base = Path::new(package_path)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or("");
    let mut manifest = Vec::new();
    let mut spine = Vec::new();
    let mut version_major = None;
    let mut spine_toc = None;
    loop {
        match reader.read_event().map_err(xml_input)? {
            Event::Start(start) | Event::Empty(start) => match local_name(start.name().as_ref()) {
                b"package" => {
                    let version = required_attribute(&reader, &start, b"version")?;
                    version_major = version
                        .split('.')
                        .next()
                        .and_then(|value| value.parse().ok());
                }
                b"item" => {
                    let id = required_attribute(&reader, &start, b"id")?;
                    let href = required_attribute(&reader, &start, b"href")?;
                    let media_type = required_attribute(&reader, &start, b"media-type")?;
                    let properties = attribute(&reader, &start, b"properties")?.unwrap_or_default();
                    let path = resolve_member_path(base, &href)?;
                    manifest.push(ManifestItem {
                        id,
                        path,
                        media_type,
                        properties,
                    });
                }
                b"spine" => spine_toc = attribute(&reader, &start, b"toc")?,
                b"itemref" => spine.push(required_attribute(&reader, &start, b"idref")?),
                _ => {}
            },
            Event::DocType(_) => return Err(invalid("DOCTYPE is not allowed in EPUB package XML")),
            Event::Eof => break,
            _ => {}
        }
    }
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    if manifest.is_empty()
        || manifest
            .iter()
            .any(|item| !ids.insert(item.id.clone()) || !paths.insert(item.path.clone()))
    {
        return Err(invalid("EPUB manifest is empty or contains duplicates"));
    }
    let version_major = version_major
        .filter(|version| matches!(version, 2 | 3))
        .ok_or_else(|| invalid("only EPUB package versions 2 and 3 are supported"))?;
    Ok(Package {
        version_major,
        manifest,
        spine,
        spine_toc,
    })
}

fn inventory_items(
    book: &Book,
    source_hash: [u8; 32],
    identity_version: u32,
) -> Vec<InventoryItem> {
    let container_id = resource_id(source_hash, "container", identity_version);
    let package_id = resource_id(
        source_hash,
        &format!("package/{}", book.package_path),
        identity_version,
    );
    let mut items = vec![
        InventoryItem::Node(ResourceNode {
            resource_id: container_id,
            resource_key: resource_key(&source_hash, ADAPTER_ID, identity_version, "container"),
            kind: ResourceKind::Container,
            semantic_path: "container".to_owned(),
            locator: Locator::ByteSpan {
                object_hash: source_hash,
                start: 0,
                end: 0,
            },
        }),
        InventoryItem::Node(ResourceNode {
            resource_id: package_id,
            resource_key: resource_key(
                &source_hash,
                ADAPTER_ID,
                identity_version,
                &format!("package/{}", book.package_path),
            ),
            kind: ResourceKind::Document,
            semantic_path: format!("package/{}", book.package_path),
            locator: Locator::ArchiveMemberByteSpan {
                object_hash: source_hash,
                member_path: book.package_path.clone(),
                start: 0,
                end: book
                    .entries
                    .iter()
                    .find(|entry| entry.path == book.package_path)
                    .map(|entry| entry.size)
                    .unwrap_or(0),
            },
        }),
        InventoryItem::Edge(ResourceEdge {
            from: container_id,
            to: package_id,
            kind: EdgeKind::Contains,
            ordinal: 0,
        }),
    ];
    let spine_paths = book
        .documents
        .iter()
        .map(|document| document.path.as_str())
        .collect::<HashSet<_>>();
    let mut previous_document = None;
    for (ordinal, document) in book.documents.iter().enumerate() {
        let document_id = document.document_id(source_hash);
        let stream_id = document.stream_id(source_hash);
        let size = book
            .entries
            .iter()
            .find(|entry| entry.path == document.path)
            .map(|entry| entry.size)
            .unwrap_or(0);
        items.push(InventoryItem::Node(ResourceNode {
            resource_id: document_id,
            resource_key: resource_key(
                &source_hash,
                ADAPTER_ID,
                identity_version,
                &format!("document/{}", document.path),
            ),
            kind: ResourceKind::Document,
            semantic_path: format!("document/{}", document.path),
            locator: member_locator(source_hash, &document.path, size),
        }));
        items.push(InventoryItem::Node(ResourceNode {
            resource_id: stream_id,
            resource_key: resource_key(
                &source_hash,
                ADAPTER_ID,
                identity_version,
                &format!("text/{}", document.path),
            ),
            kind: ResourceKind::TextStream,
            semantic_path: format!("text/{}", document.path),
            locator: member_locator(source_hash, &document.path, size),
        }));
        items.push(InventoryItem::Edge(ResourceEdge {
            from: package_id,
            to: document_id,
            kind: EdgeKind::Contains,
            ordinal: ordinal as u32,
        }));
        items.push(InventoryItem::Edge(ResourceEdge {
            from: document_id,
            to: stream_id,
            kind: EdgeKind::Contains,
            ordinal: 0,
        }));
        if let Some(previous) = previous_document {
            items.push(InventoryItem::Edge(ResourceEdge {
                from: previous,
                to: document_id,
                kind: EdgeKind::ReadingOrderAfter,
                ordinal: ordinal as u32,
            }));
        }
        previous_document = Some(document_id);
    }
    for entry in &book.entries {
        if entry.path == "mimetype"
            || entry.path == "META-INF/container.xml"
            || entry.path == book.package_path
            || spine_paths.contains(entry.path.as_str())
        {
            continue;
        }
        let kind = resource_kind(entry.media_type.as_deref());
        let semantic_path = format!("asset/{}", entry.path);
        let id = resource_id(source_hash, &semantic_path, identity_version);
        items.push(InventoryItem::Node(ResourceNode {
            resource_id: id,
            resource_key: resource_key(&source_hash, ADAPTER_ID, identity_version, &semantic_path),
            kind,
            semantic_path,
            locator: member_locator(source_hash, &entry.path, entry.size),
        }));
        items.push(InventoryItem::Edge(ResourceEdge {
            from: package_id,
            to: id,
            kind: EdgeKind::Contains,
            ordinal: items.len() as u32,
        }));
    }
    for (ordinal, (from_path, to_path)) in book.references.iter().enumerate() {
        items.push(InventoryItem::Edge(ResourceEdge {
            from: member_resource_id(book, source_hash, from_path, identity_version),
            to: member_resource_id(book, source_hash, to_path, identity_version),
            kind: EdgeKind::References,
            ordinal: ordinal as u32,
        }));
    }
    items
}

fn member_resource_id(
    book: &Book,
    source_hash: [u8; 32],
    path: &str,
    identity_version: u32,
) -> ResourceId {
    if book.documents.iter().any(|document| document.path == path) {
        resource_id(source_hash, &format!("document/{path}"), identity_version)
    } else {
        resource_id(source_hash, &format!("asset/{path}"), identity_version)
    }
}

fn build_member_patches(
    book: &Book,
    input: &ObjectHandle,
    overlays: &[OverlayUnit],
    io: &dyn CapabilityIo,
    context: &ExecutionContext<'_>,
) -> Result<HashMap<String, Vec<u8>>, AdapterError> {
    let mut grouped = HashMap::<String, Vec<&OverlayUnit>>::new();
    for overlay in overlays {
        let Locator::ArchiveMemberByteSpan {
            object_hash,
            member_path,
            ..
        } = &overlay.source_locator
        else {
            return Err(invalid("EPUB overlay has a non-member locator"));
        };
        if *object_hash != input.object_hash {
            return Err(invalid("EPUB overlay references a different source object"));
        }
        grouped
            .entry(member_path.clone())
            .or_default()
            .push(overlay);
    }
    let document_paths = book
        .documents
        .iter()
        .map(|document| document.path.as_str())
        .collect::<HashSet<_>>();
    let mut archive = ZipArchive::new(io.open_object(input)?).map_err(zip_input)?;
    let mut result = HashMap::new();
    for (path, mut member_overlays) in grouped {
        context.checkpoint()?;
        if !document_paths.contains(path.as_str()) {
            return Err(invalid("EPUB overlay references a non-spine member"));
        }
        member_overlays.sort_unstable_by_key(|overlay| {
            member_span(&overlay.source_locator, input.object_hash, &path)
                .map(|(start, _)| start)
                .unwrap_or(usize::MAX)
        });
        let source = read_member(&mut archive, &path, MAX_MEMBER_BYTES, context)?;
        let spans = parse_xhtml_spans(&source, context)?;
        validate_overlays_for_member(input.object_hash, &path, member_overlays.as_slice(), &spans)?;
        let mut output = Vec::with_capacity(source.len());
        let mut offset = 0_usize;
        for overlay in member_overlays {
            let (start, end) = member_span(&overlay.source_locator, input.object_hash, &path)?;
            output.extend_from_slice(&source[offset..start]);
            output.extend_from_slice(escape::escape(&overlay.translated_text).as_bytes());
            offset = end;
        }
        output.extend_from_slice(&source[offset..]);
        parse_xhtml_spans(&output, context)?;
        result.insert(path, output);
    }
    Ok(result)
}

#[derive(Clone, Debug)]
struct TextSpan {
    range: Range<usize>,
    text: String,
    path: Vec<String>,
}

fn extracted_document_units(
    source_hash: [u8; 32],
    generation_id: GenerationId,
    resource_id: ResourceId,
    document: &Document,
    spans: &[TextSpan],
    context: &ExecutionContext<'_>,
) -> Result<Vec<ExtractedUnit>, AdapterError> {
    let mut units = Vec::with_capacity(spans.len());
    for (index, span) in spans.iter().enumerate() {
        context.checkpoint()?;
        let previous = index
            .checked_sub(1)
            .and_then(|previous| spans.get(previous))
            .map(|span| span.text.as_str());
        let next = spans.get(index + 1).map(|span| span.text.as_str());
        let source_unit = SourceUnit::new(
            FORMAT_ID,
            format!(
                "{}:member:{}:identity-v{}",
                ADAPTER_ID, document.path, IDENTITY_VERSION
            ),
            span.path.clone(),
            &span.text,
            previous,
            next,
        );
        let content = UnitContent {
            schema_version: TIR_SCHEMA_VERSION,
            tokens: vec![Token::Text {
                text: span.text.clone(),
                style_hint: None,
            }],
        };
        content
            .validate()
            .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
        units.push(ExtractedUnit {
            generation_id,
            resource_id,
            source_unit_key: source_unit.source_key,
            locator: Locator::ArchiveMemberByteSpan {
                object_hash: source_hash,
                member_path: document.path.clone(),
                start: span.range.start as u64,
                end: span.range.end as u64,
            },
            content,
        });
    }
    Ok(units)
}

fn parse_xhtml_spans(
    bytes: &[u8],
    context: &ExecutionContext<'_>,
) -> Result<Vec<TextSpan>, AdapterError> {
    if bytes.len() as u64 > MAX_MEMBER_BYTES || !is_utf8_xml(bytes) {
        return Err(invalid("editable EPUB XHTML must be UTF-8"));
    }
    validate_xml_limits(bytes)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut stack = Vec::<String>::new();
    let mut sibling_counts = HashMap::<String, usize>::new();
    let mut text_counts = HashMap::<String, usize>::new();
    let mut body_depth = None;
    let mut excluded_depth = 0_usize;
    let mut spans = Vec::new();
    loop {
        context.checkpoint()?;
        match reader.read_event().map_err(xml_input)? {
            Event::Start(start) => {
                let name = String::from_utf8_lossy(local_name(start.name().as_ref())).into_owned();
                let parent = stack.join("/");
                let counter_key = format!("{parent}/{name}");
                let ordinal = sibling_counts.entry(counter_key).or_default();
                let segment = format!("{name}[{ordinal}]");
                *ordinal += 1;
                if name == "body" {
                    body_depth = Some(stack.len());
                }
                if matches!(name.as_str(), "script" | "style" | "title" | "head") {
                    excluded_depth += 1;
                }
                stack.push(segment);
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(local_name(end.name().as_ref())).into_owned();
                if matches!(name.as_str(), "script" | "style" | "title" | "head") {
                    excluded_depth = excluded_depth.saturating_sub(1);
                }
                if name == "body" {
                    body_depth = None;
                }
                stack.pop();
            }
            Event::Text(text) if body_depth.is_some() && excluded_depth == 0 => {
                let end = reader.buffer_position() as usize;
                let raw: &[u8] = text.as_ref();
                let raw_start = end
                    .checked_sub(raw.len())
                    .ok_or_else(|| invalid("invalid XHTML text position"))?;
                let leading = raw
                    .iter()
                    .take_while(|byte| byte.is_ascii_whitespace())
                    .count();
                let trailing = raw
                    .iter()
                    .rev()
                    .take_while(|byte| byte.is_ascii_whitespace())
                    .count();
                if leading + trailing < raw.len() {
                    let start = raw_start + leading;
                    let end = end - trailing;
                    let decoded = text.decode().map_err(xml_input)?;
                    let unescaped = escape::unescape(&decoded).map_err(xml_input)?;
                    let visible = unescaped.trim_matches(char::is_whitespace).to_owned();
                    if !visible.is_empty() {
                        let parent = stack.join("/");
                        let ordinal = text_counts.entry(parent.clone()).or_default();
                        let mut path = stack.clone();
                        path.push(format!("text[{ordinal}]"));
                        *ordinal += 1;
                        spans.push(TextSpan {
                            range: start..end,
                            text: visible,
                            path,
                        });
                    }
                }
            }
            Event::DocType(_) => return Err(invalid("DOCTYPE is not allowed in editable XHTML")),
            Event::Eof => break,
            _ => {}
        }
        if spans.len() > context.budget.maximum_nodes as usize {
            return Err(AdapterError::BudgetExceeded);
        }
    }
    Ok(spans)
}

fn validate_overlays_for_member(
    source_hash: [u8; 32],
    path: &str,
    overlays: &[&OverlayUnit],
    spans: &[TextSpan],
) -> Result<(), AdapterError> {
    let stable_key = format!(
        "{}:member:{}:identity-v{}",
        ADAPTER_ID, path, IDENTITY_VERSION
    );
    let by_key = spans
        .iter()
        .enumerate()
        .map(|(index, span)| {
            let previous = index
                .checked_sub(1)
                .and_then(|previous| spans.get(previous))
                .map(|span| span.text.as_str());
            let next = spans.get(index + 1).map(|span| span.text.as_str());
            let unit = SourceUnit::new(
                FORMAT_ID,
                &stable_key,
                span.path.clone(),
                &span.text,
                previous,
                next,
            );
            (unit.source_key, span.range.clone())
        })
        .collect::<HashMap<_, _>>();
    let mut previous_end = 0_usize;
    for overlay in overlays {
        let expected = by_key
            .get(&overlay.source_unit_key)
            .ok_or_else(|| invalid("EPUB overlay references an unknown source unit"))?;
        let (start, end) = member_span(&overlay.source_locator, source_hash, path)?;
        if expected.start != start || expected.end != end || start < previous_end {
            return Err(invalid(
                "EPUB overlay locator does not match its source unit",
            ));
        }
        previous_end = end;
    }
    Ok(())
}

fn validate_overlay_locators(
    source_hash: [u8; 32],
    overlays: &[OverlayUnit],
) -> Result<(), AdapterError> {
    let mut locators = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        let Locator::ArchiveMemberByteSpan {
            object_hash,
            member_path,
            start,
            end,
        } = &overlay.source_locator
        else {
            return Err(invalid("EPUB overlays require archive-member byte spans"));
        };
        if *object_hash != source_hash || start > end {
            return Err(invalid("EPUB overlay locator is invalid"));
        }
        locators.push((member_path.as_str(), *start as usize, *end as usize));
    }
    locators.sort_unstable_by(|left, right| {
        left.0
            .cmp(right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    for pair in locators.windows(2) {
        if pair[0].0 == pair[1].0 && pair[1].1 < pair[0].2 {
            return Err(invalid("EPUB overlay locators overlap"));
        }
    }
    Ok(())
}

fn member_span(
    locator: &Locator,
    source_hash: [u8; 32],
    path: &str,
) -> Result<(usize, usize), AdapterError> {
    match locator {
        Locator::ArchiveMemberByteSpan {
            object_hash,
            member_path,
            start,
            end,
        } if *object_hash == source_hash && member_path == path && start <= end => {
            Ok((*start as usize, *end as usize))
        }
        _ => Err(invalid("EPUB member locator does not match the source")),
    }
}

fn validate_plan(
    plan: &ExportPlan,
    input: &ObjectHandle,
    overlays: &[OverlayUnit],
) -> Result<(), AdapterError> {
    if plan.source_object_hash != input.object_hash
        || plan.overlay_hash != hash_overlays(overlays)
        || plan.ordered_source_unit_keys.len() != overlays.len()
        || plan.ordered_overlay_hashes.len() != overlays.len()
    {
        return Err(invalid("EPUB overlay changed after export snapshot"));
    }
    for (index, overlay) in overlays.iter().enumerate() {
        if plan.ordered_source_unit_keys[index] != overlay.source_unit_key
            || plan.ordered_overlay_hashes[index] != hash_overlay(overlay)
        {
            return Err(invalid("EPUB overlay order changed after export snapshot"));
        }
    }
    Ok(())
}

fn read_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
    maximum_bytes: u64,
    context: &ExecutionContext<'_>,
) -> Result<Vec<u8>, AdapterError> {
    let mut entry = archive.by_name(path).map_err(zip_input)?;
    if entry.size() > maximum_bytes {
        return Err(AdapterError::BudgetExceeded);
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        context.checkpoint()?;
        let read = entry.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() as u64 > maximum_bytes {
            return Err(AdapterError::BudgetExceeded);
        }
    }
    if bytes.len() as u64 != entry.size() || bytes.len() as u64 > maximum_bytes {
        return Err(AdapterError::BudgetExceeded);
    }
    Ok(bytes)
}

fn attribute(
    reader: &Reader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, AdapterError> {
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(xml_input)?;
        if local_name(attribute.key.as_ref()) == name {
            return Ok(Some(
                attribute
                    .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                    .map_err(xml_input)?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn required_attribute(
    reader: &Reader<&[u8]>,
    start: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<String, AdapterError> {
    attribute(reader, start, name)?.ok_or_else(|| invalid("required EPUB XML attribute is missing"))
}

fn resolve_member_path(base: &str, href: &str) -> Result<String, AdapterError> {
    let href = href.split(['#', '?']).next().unwrap_or("");
    let decoded = percent_decode(href)?;
    let mut parts = if decoded.starts_with('/') {
        return Err(invalid("absolute EPUB references are not supported"));
    } else {
        base.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(invalid("EPUB reference escapes the container root"));
                }
            }
            value => parts.push(value.to_owned()),
        }
    }
    let path = parts.join("/");
    validate_member_path(&path)?;
    Ok(path)
}

fn parse_xml_references(
    bytes: &[u8],
    base: &str,
    context: &ExecutionContext<'_>,
) -> Result<Vec<String>, AdapterError> {
    if bytes.len() as u64 > MAX_MEMBER_BYTES {
        return Err(AdapterError::BudgetExceeded);
    }
    validate_xml_limits(bytes)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut references = Vec::new();
    loop {
        context.checkpoint()?;
        match reader.read_event().map_err(xml_input)? {
            Event::Start(start) | Event::Empty(start) => {
                for name in [b"href".as_slice(), b"src".as_slice()] {
                    let Some(value) = attribute(&reader, &start, name)? else {
                        continue;
                    };
                    if value.is_empty()
                        || value.starts_with('#')
                        || value.starts_with("//")
                        || has_uri_scheme(&value)
                    {
                        continue;
                    }
                    references.push(resolve_member_path(base, &value)?);
                }
            }
            Event::DocType(_) => return Err(invalid("DOCTYPE is not allowed in EPUB XML")),
            Event::Eof => return Ok(references),
            _ => {}
        }
    }
}

fn validate_xml_limits(bytes: &[u8]) -> Result<(), AdapterError> {
    if bytes.len() as u64 > MAX_XML_BYTES {
        return Err(AdapterError::BudgetExceeded);
    }
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_end_names = true;
    let mut depth = 0_usize;
    let mut text_nodes = 0_usize;
    loop {
        match reader.read_event().map_err(xml_input)? {
            Event::Start(start) => {
                depth += 1;
                if depth > MAX_XML_DEPTH
                    || start.attributes().with_checks(true).count() > MAX_XML_ATTRIBUTES
                {
                    return Err(AdapterError::BudgetExceeded);
                }
            }
            Event::Empty(start) => {
                if start.attributes().with_checks(true).count() > MAX_XML_ATTRIBUTES {
                    return Err(AdapterError::BudgetExceeded);
                }
            }
            Event::Text(_) | Event::CData(_) => {
                text_nodes += 1;
                if text_nodes > MAX_XML_TEXT_NODES {
                    return Err(AdapterError::BudgetExceeded);
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::DocType(_) => return Err(invalid("DOCTYPE is not allowed in EPUB XML")),
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && matches!(byte, b'+' | b'-' | b'.' | b'0'..=b'9'))
        })
}

fn percent_decode(value: &str) -> Result<String, AdapterError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(invalid("EPUB reference contains invalid percent encoding"));
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            output.push(high << 4 | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| invalid("EPUB reference is not UTF-8"))
}

fn hex_value(byte: u8) -> Result<u8, AdapterError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(invalid("EPUB reference contains invalid percent encoding")),
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn is_utf8_xml(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
        && !bytes.starts_with(&[0xff, 0xfe])
        && !bytes.starts_with(&[0xfe, 0xff])
}

fn is_xhtml_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/xhtml+xml" | "text/html" | "application/xml"
    )
}

fn is_xml_reference_source(media_type: &str) -> bool {
    is_xhtml_media_type(media_type) || media_type == "application/x-dtbncx+xml"
}

fn resource_kind(media_type: Option<&str>) -> ResourceKind {
    match media_type.unwrap_or_default() {
        value if value.starts_with("image/") => ResourceKind::Image,
        value
            if value.starts_with("font/")
                || matches!(
                    value,
                    "application/font-sfnt" | "application/vnd.ms-opentype"
                ) =>
        {
            ResourceKind::Font
        }
        "text/css" => ResourceKind::Stylesheet,
        "application/xhtml+xml" | "application/x-dtbncx+xml" | "application/xml" => {
            ResourceKind::Document
        }
        _ => ResourceKind::BinaryAttachment,
    }
}

fn archive_contains<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> bool {
    archive.by_name(path).is_ok()
}

fn member_locator(source_hash: [u8; 32], path: &str, size: u64) -> Locator {
    Locator::ArchiveMemberByteSpan {
        object_hash: source_hash,
        member_path: path.to_owned(),
        start: 0,
        end: size,
    }
}

fn resource_id(source_hash: [u8; 32], semantic_path: &str, identity_version: u32) -> ResourceId {
    id_from_hash(resource_key(
        &source_hash,
        ADAPTER_ID,
        identity_version,
        semantic_path,
    ))
}

fn id_from_hash(hash: [u8; 32]) -> ResourceId {
    ResourceId::from_bytes(hash[..16].try_into().expect("hash prefix"))
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

fn paginate<T: Clone + serde::Serialize>(
    items: Vec<T>,
    cursor: Option<&Cursor>,
    context: &ExecutionContext<'_>,
    manifest: &AdapterManifest,
) -> Result<Page<T>, AdapterError> {
    let start = decode_cursor(cursor)? as usize;
    if start > items.len() {
        return Err(AdapterError::InvalidCursor);
    }
    let node_limit = context.budget.bounded_page_nodes(manifest) as usize;
    let byte_limit = context.budget.bounded_page_bytes(manifest);
    let mut page = Vec::new();
    let mut emitted_bytes = 0_u64;
    for item in items.iter().skip(start) {
        context.checkpoint()?;
        let size = serde_json::to_vec(item)
            .map_err(|error| invalid(error.to_string()))?
            .len() as u64;
        if !page.is_empty()
            && (page.len() >= node_limit || emitted_bytes.saturating_add(size) > byte_limit)
        {
            break;
        }
        if size > byte_limit {
            return Err(AdapterError::BudgetExceeded);
        }
        page.push(item.clone());
        emitted_bytes += size;
    }
    let next = start + page.len();
    Ok(Page {
        items: page,
        next_cursor: (next < items.len()).then(|| encode_cursor(next as u64)),
        emitted_bytes,
    })
}

fn encode_cursor(value: u64) -> Cursor {
    Cursor(value.to_be_bytes().to_vec())
}

fn decode_cursor(cursor: Option<&Cursor>) -> Result<u64, AdapterError> {
    match cursor {
        None => Ok(0),
        Some(cursor) if cursor.0.len() == 8 => Ok(u64::from_be_bytes(
            cursor.0.as_slice().try_into().expect("length checked"),
        )),
        Some(_) => Err(AdapterError::InvalidCursor),
    }
}

fn invalid(message: impl Into<String>) -> AdapterError {
    AdapterError::InvalidInput(message.into())
}

fn compose_image_overlays(
    source_bytes: &[u8],
    overlays: &[&ImageOverlay],
    io: &dyn CapabilityIo,
    context: &ExecutionContext<'_>,
) -> Result<Vec<u8>, AdapterError> {
    let format = image::guess_format(source_bytes)
        .map_err(|error| invalid(format!("EPUB image member format is unsupported: {error}")))?;
    let mut composed = image::load_from_memory(source_bytes)
        .map_err(|error| invalid(format!("EPUB image member cannot be decoded: {error}")))?
        .to_rgba8();
    for overlay in overlays {
        context.checkpoint()?;
        let Locator::SpatialRegion { polygon, .. } = &overlay.region_locator else {
            return Err(invalid(
                "EPUB image overlay requires a spatial region locator",
            ));
        };
        let mut derived_bytes = Vec::new();
        io.open_object(&overlay.derived_object)?
            .read_to_end(&mut derived_bytes)?;
        let derived = image::load_from_memory(&derived_bytes)
            .map_err(|error| invalid(format!("derived image cannot be decoded: {error}")))?
            .to_rgba8();
        if derived.dimensions() != composed.dimensions() {
            return Err(invalid(
                "derived image dimensions do not match the source image",
            ));
        }
        for y in 0..composed.height() {
            for x in 0..composed.width() {
                if point_in_polygon(x as f32 + 0.5, y as f32 + 0.5, polygon) {
                    *composed.get_pixel_mut(x, y) = *derived.get_pixel(x, y);
                }
            }
        }
    }
    let mut output = Vec::new();
    DynamicImage::ImageRgba8(composed)
        .write_to(&mut IoCursor::new(&mut output), format)
        .map_err(|error| invalid(format!("composed image cannot be encoded: {error}")))?;
    Ok(output)
}

fn point_in_polygon(x: f32, y: f32, polygon: &[[f32; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let [current_x, current_y] = polygon[current];
        let [previous_x, previous_y] = polygon[previous];
        let crosses = (current_y > y) != (previous_y > y);
        if crosses {
            let intersection =
                (previous_x - current_x) * (y - current_y) / (previous_y - current_y) + current_x;
            if x < intersection {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn zip_input(error: zip::result::ZipError) -> AdapterError {
    invalid(format!("invalid EPUB ZIP container: {error}"))
}

fn xml_input(error: impl std::fmt::Display) -> AdapterError {
    invalid(format!("invalid EPUB XML: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use babel_adapter_host::CapabilityRegistry;
    use babel_adapter_protocol::{CancellationToken, TaskBudget};
    use std::{
        fs,
        io::Cursor as IoCursor,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tempfile::TempDir;

    fn fixture(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
        let cursor = IoCursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        for (name, bytes, method) in entries {
            writer
                .start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(*method),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn epub3() -> Vec<u8> {
        fixture(&[
            ("mimetype", EPUB_MEDIA_TYPE.as_bytes(), CompressionMethod::Stored),
            (
                "META-INF/container.xml",
                br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/package.opf",
                br#"<?xml version="1.0"?><package version="3.0"><manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="css" href="style.css" media-type="text/css"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#,
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/chapter1.xhtml",
                br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Hidden</title></head><body><h1>First &amp; foremost</h1><p>Hello <em>world</em>.</p></body></html>"#,
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/chapter2.xhtml",
                br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body><p>Second chapter</p></body></html>"#,
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/nav.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><nav><a href="chapter1.xhtml">One</a></nav></body></html>"#,
                CompressionMethod::Deflated,
            ),
            ("EPUB/style.css", b"body { color: black; }", CompressionMethod::Deflated),
        ])
    }

    fn epub2() -> Vec<u8> {
        fixture(&[
            ("mimetype", EPUB_MEDIA_TYPE.as_bytes(), CompressionMethod::Stored),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
                CompressionMethod::Deflated,
            ),
            (
                "OEBPS/content.opf",
                r#"<package version="2.0"><manifest><item id="chapter" href="章节.xhtml" media-type="application/xhtml+xml"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx"><itemref idref="chapter"/></spine></package>"#.as_bytes(),
                CompressionMethod::Deflated,
            ),
            (
                "OEBPS/章节.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Chapter text</p></body></html>"#,
                CompressionMethod::Deflated,
            ),
            (
                "OEBPS/toc.ncx",
                r#"<ncx><navMap><navPoint><content src="章节.xhtml"/></navPoint></navMap></ncx>"#.as_bytes(),
                CompressionMethod::Deflated,
            ),
        ])
    }

    fn compressed_member(bytes: &[u8], name: &str) -> Vec<u8> {
        let mut archive = ZipArchive::new(IoCursor::new(bytes)).unwrap();
        let entry = archive.by_name(name).unwrap();
        let start = entry.data_start().unwrap() as usize;
        let end = start + entry.compressed_size() as usize;
        bytes[start..end].to_vec()
    }

    #[test]
    fn image_overlays_compose_multiple_regions_in_stable_order() {
        let encode = |pixels: [[u8; 4]; 2]| {
            let mut bytes = Vec::new();
            DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(2, 1, pixels.into_iter().flatten().collect()).unwrap(),
            )
            .write_to(&mut IoCursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
            bytes
        };
        let source = encode([[255, 0, 0, 255], [255, 0, 0, 255]]);
        let first = encode([[0, 255, 0, 255], [255, 0, 0, 255]]);
        let second = encode([[255, 0, 0, 255], [0, 0, 255, 255]]);
        let (temp, registry, _source_handle) = capability(&source);
        let add_object = |bytes: &[u8]| {
            let hash: [u8; 32] = Sha256::digest(bytes).into();
            let encoded = hex::encode(hash);
            let path = temp
                .path()
                .join("objects/sha256")
                .join(&encoded[..2])
                .join(&encoded[2..]);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
            registry.grant_object(hash, bytes.len() as u64).unwrap()
        };
        let first_handle = add_object(&first);
        let second_handle = add_object(&second);
        let token = CancellationToken::default();
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        let source_hash: [u8; 32] = Sha256::digest(&source).into();
        let first_overlay = ImageOverlay {
            image_resource_id: ResourceId::from_bytes([1; 16]),
            source_locator: Locator::ArchiveMemberByteSpan {
                object_hash: source_hash,
                member_path: "image.png".to_owned(),
                start: 0,
                end: source.len() as u64,
            },
            region_locator: Locator::SpatialRegion {
                resource_id: ResourceId::from_bytes([1; 16]),
                polygon: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                coordinate_space: "pixel".to_owned(),
            },
            derived_object: first_handle,
            media_type: "image/png".to_owned(),
        };
        let second_overlay = ImageOverlay {
            image_resource_id: ResourceId::from_bytes([1; 16]),
            source_locator: first_overlay.source_locator.clone(),
            region_locator: Locator::SpatialRegion {
                resource_id: ResourceId::from_bytes([2; 16]),
                polygon: vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]],
                coordinate_space: "pixel".to_owned(),
            },
            derived_object: second_handle,
            media_type: "image/png".to_owned(),
        };
        let result = compose_image_overlays(
            &source,
            &[&first_overlay, &second_overlay],
            &registry,
            &context,
        )
        .unwrap();
        let decoded = image::load_from_memory(&result).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 0, 255, 255]);
    }

    fn capability(bytes: &[u8]) -> (TempDir, CapabilityRegistry, ObjectHandle) {
        let temp = TempDir::new().unwrap();
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        let encoded = hex::encode(hash);
        let path = temp
            .path()
            .join("objects/sha256")
            .join(&encoded[..2])
            .join(&encoded[2..]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        let registry =
            CapabilityRegistry::new(temp.path().join("objects"), temp.path().join("staging"))
                .unwrap();
        let handle = registry.grant_object(hash, bytes.len() as u64).unwrap();
        (temp, registry, handle)
    }

    fn budget() -> TaskBudget {
        TaskBudget {
            timeout_ms: 10_000,
            maximum_bytes: 64 * 1024 * 1024,
            maximum_nodes: 100_000,
            page_bytes: 8 * 1024 * 1024,
            page_nodes: 10_000,
        }
    }

    #[test]
    fn imports_spine_order_and_extracts_member_local_text() {
        let bytes = epub3();
        let (_temp, registry, handle) = capability(&bytes);
        let adapter = EpubAdapter::new();
        let token = CancellationToken::default();
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        assert_eq!(
            adapter
                .probe(&handle, &registry, &context)
                .unwrap()
                .detected_media_type
                .as_deref(),
            Some(EPUB_MEDIA_TYPE)
        );
        let generation = GenerationId::new();
        let inventory = adapter
            .inventory(&handle, generation, None, &registry, &context)
            .unwrap();
        let streams = inventory
            .items
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Node(node) if node.kind == ResourceKind::TextStream => {
                    Some(node.resource_id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(streams.len(), 2);
        let first = adapter
            .extract(&handle, generation, streams[0], None, &registry, &context)
            .unwrap();
        let texts = first
            .items
            .iter()
            .map(|unit| match &unit.content.tokens[0] {
                Token::Text { text, .. } => text.as_str(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(texts, ["First", "foremost", "Hello", "world", "."]);
        assert!(first.items.iter().all(|unit| matches!(
            unit.locator,
            Locator::ArchiveMemberByteSpan { ref member_path, .. }
                if member_path == "EPUB/chapter1.xhtml"
        )));
    }

    #[test]
    fn exports_changed_xhtml_and_preserves_untouched_member_payload() {
        let bytes = epub3();
        let (_temp, registry, handle) = capability(&bytes);
        let adapter = EpubAdapter::new();
        let token = CancellationToken::default();
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        let generation = GenerationId::new();
        let inventory = adapter
            .inventory(&handle, generation, None, &registry, &context)
            .unwrap();
        let stream = inventory
            .items
            .iter()
            .find_map(|item| match item {
                InventoryItem::Node(node) if node.kind == ResourceKind::TextStream => {
                    Some(node.resource_id)
                }
                _ => None,
            })
            .unwrap();
        let extracted = adapter
            .extract(&handle, generation, stream, None, &registry, &context)
            .unwrap();
        let mut overlays = extracted
            .items
            .iter()
            .map(|unit| OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: unit.locator.clone(),
                translated_text: match &unit.content.tokens[0] {
                    Token::Text { text, .. } => format!("译文-{text}"),
                    _ => unreachable!(),
                },
            })
            .collect::<Vec<_>>();
        overlays.reverse();
        let plan = adapter
            .plan_export(&handle, generation, 7, &overlays, &context)
            .unwrap();
        let staging = registry.create_staging().unwrap();
        adapter
            .materialize(
                &plan, &handle, &overlays, &staging, None, &registry, &context,
            )
            .unwrap();
        let verification = adapter
            .verify_output(&staging, &registry, &context)
            .unwrap();
        assert!(verification.valid);
        let output = registry.staging_bytes(&staging).unwrap();
        assert_eq!(
            compressed_member(&bytes, "EPUB/style.css"),
            compressed_member(&output, "EPUB/style.css")
        );
        let mut archive = ZipArchive::new(IoCursor::new(output)).unwrap();
        let chapter = read_member(
            &mut archive,
            "EPUB/chapter1.xhtml",
            MAX_MEMBER_BYTES,
            &context,
        )
        .unwrap();
        assert!(
            String::from_utf8(chapter)
                .unwrap()
                .contains("译文-First &amp; 译文-foremost")
        );
        let untouched = archive.by_name("EPUB/style.css").unwrap();
        let original = ZipArchive::new(IoCursor::new(bytes))
            .unwrap()
            .by_name("EPUB/style.css")
            .unwrap()
            .crc32();
        assert_eq!(untouched.crc32(), original);
    }

    #[test]
    fn rejects_duplicate_and_traversal_members() {
        let mut duplicate = fixture(&[
            (
                "mimetypf",
                EPUB_MEDIA_TYPE.as_bytes(),
                CompressionMethod::Stored,
            ),
            (
                "mimetype",
                EPUB_MEDIA_TYPE.as_bytes(),
                CompressionMethod::Stored,
            ),
        ]);
        for offset in 0..duplicate.len().saturating_sub(8) {
            if &duplicate[offset..offset + 8] == b"mimetypf" {
                duplicate[offset..offset + 8].copy_from_slice(b"mimetype");
            }
        }
        let (_temp, registry, handle) = capability(&duplicate);
        let adapter = EpubAdapter::new();
        let token = CancellationToken::default();
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        assert_eq!(
            adapter
                .probe(&handle, &registry, &context)
                .unwrap()
                .confidence_millionths,
            0
        );

        let traversal = fixture(&[
            (
                "mimetype",
                EPUB_MEDIA_TYPE.as_bytes(),
                CompressionMethod::Stored,
            ),
            ("../evil", b"x", CompressionMethod::Stored),
        ]);
        let (_temp, registry, handle) = capability(&traversal);
        assert_eq!(
            adapter
                .probe(&handle, &registry, &context)
                .unwrap()
                .confidence_millionths,
            0
        );
    }

    #[test]
    fn supports_epub2_ncx_and_utf8_member_paths() {
        let bytes = epub2();
        let (_temp, registry, handle) = capability(&bytes);
        let adapter = EpubAdapter::new();
        let token = CancellationToken::default();
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        let inventory = adapter
            .inventory(&handle, GenerationId::new(), None, &registry, &context)
            .unwrap();
        assert!(inventory.items.iter().any(|item| matches!(
            item,
            InventoryItem::Node(node)
                if node.kind == ResourceKind::TextStream
                    && node.semantic_path == "text/OEBPS/章节.xhtml"
        )));
        assert!(inventory.items.iter().any(|item| matches!(
            item,
            InventoryItem::Edge(edge) if edge.kind == EdgeKind::References
        )));
    }

    #[test]
    fn rejects_epub3_without_nav_missing_manifest_members_and_broken_nav_targets() {
        let invalid_packages = [
            (
                br#"<package version="3.0"><manifest><item id="c" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#.as_slice(),
                br#"<html><body><a href="chapter.xhtml">Chapter</a></body></html>"#.as_slice(),
                true,
            ),
            (
                br#"<package version="3.0"><manifest><item id="c" href="missing.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="c"/></spine></package>"#.as_slice(),
                br#"<html><body><a href="missing.xhtml">Chapter</a></body></html>"#.as_slice(),
                false,
            ),
            (
                br#"<package version="3.0"><manifest><item id="c" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="c"/></spine></package>"#.as_slice(),
                br#"<html><body><a href="absent.xhtml">Missing</a></body></html>"#.as_slice(),
                true,
            ),
        ];
        for (package, nav, include_chapter) in invalid_packages {
            let mut entries = vec![
                ("mimetype", EPUB_MEDIA_TYPE.as_bytes(), CompressionMethod::Stored),
                (
                    "META-INF/container.xml",
                    br#"<container><rootfiles><rootfile full-path="EPUB/package.opf"/></rootfiles></container>"#.as_slice(),
                    CompressionMethod::Deflated,
                ),
                ("EPUB/package.opf", package, CompressionMethod::Deflated),
                ("EPUB/nav.xhtml", nav, CompressionMethod::Deflated),
            ];
            if include_chapter {
                entries.push((
                    "EPUB/chapter.xhtml",
                    br#"<html><body><p>Text</p></body></html>"#.as_slice(),
                    CompressionMethod::Deflated,
                ));
            }
            let bytes = fixture(&entries);
            let (_temp, registry, handle) = capability(&bytes);
            let adapter = EpubAdapter::new();
            let token = CancellationToken::default();
            let budget = budget();
            let context = ExecutionContext::new(&budget, &token);
            assert_eq!(
                adapter
                    .probe(&handle, &registry, &context)
                    .unwrap()
                    .confidence_millionths,
                0
            );
        }
    }

    #[test]
    fn rejects_compression_bombs_and_excessive_xml_depth() {
        let bomb_payload = vec![0_u8; 8 * 1024 * 1024];
        let compressed_bomb = fixture(&[
            (
                "mimetype",
                EPUB_MEDIA_TYPE.as_bytes(),
                CompressionMethod::Stored,
            ),
            (
                "EPUB/bomb.bin",
                bomb_payload.as_slice(),
                CompressionMethod::Deflated,
            ),
        ]);
        let mut deep_container = String::new();
        for _ in 0..=MAX_XML_DEPTH {
            deep_container.push_str("<n>");
        }
        for _ in 0..=MAX_XML_DEPTH {
            deep_container.push_str("</n>");
        }
        let deep_xml = fixture(&[
            (
                "mimetype",
                EPUB_MEDIA_TYPE.as_bytes(),
                CompressionMethod::Stored,
            ),
            (
                "META-INF/container.xml",
                deep_container.as_bytes(),
                CompressionMethod::Deflated,
            ),
        ]);
        let adapter = EpubAdapter::new();
        for bytes in [compressed_bomb, deep_xml] {
            let (_temp, registry, handle) = capability(&bytes);
            let token = CancellationToken::default();
            let budget = budget();
            let context = ExecutionContext::new(&budget, &token);
            match adapter.probe(&handle, &registry, &context) {
                Ok(probe) => assert_eq!(probe.confidence_millionths, 0),
                Err(AdapterError::BudgetExceeded) => {}
                Err(error) => panic!("unexpected rejection: {error}"),
            }
        }
    }

    #[test]
    fn member_decompression_observes_cancellation_between_chunks() {
        struct CancellingReader {
            inner: IoCursor<Vec<u8>>,
            cancellation: CancellationToken,
            enabled: Arc<AtomicBool>,
        }

        impl Read for CancellingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let read = self.inner.read(buffer)?;
                if read > 0 && self.enabled.load(Ordering::Acquire) {
                    self.cancellation.cancel();
                }
                Ok(read)
            }
        }

        impl Seek for CancellingReader {
            fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
                self.inner.seek(position)
            }
        }

        let bytes = fixture(&[(
            "large.bin",
            vec![7_u8; 256 * 1024].as_slice(),
            CompressionMethod::Stored,
        )]);
        let token = CancellationToken::default();
        let enabled = Arc::new(AtomicBool::new(false));
        let reader = CancellingReader {
            inner: IoCursor::new(bytes),
            cancellation: token.clone(),
            enabled: Arc::clone(&enabled),
        };
        let mut archive = ZipArchive::new(reader).unwrap();
        enabled.store(true, Ordering::Release);
        let budget = budget();
        let context = ExecutionContext::new(&budget, &token);
        assert!(matches!(
            read_member(&mut archive, "large.bin", MAX_MEMBER_BYTES, &context),
            Err(AdapterError::Cancelled)
        ));
    }
}
