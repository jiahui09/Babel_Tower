use std::{env, io, process};

use anyhow::{Context, Result};
use babel_ocr::{
    OcrAssetKind, OcrAssetManifest, OcrBlockType, OcrCapabilities, OcrDocument, OcrEngine,
    OcrEngineDescriptor, OcrEngineError, OcrInputKind, OcrPage, OcrPoint, OcrProfile, OcrRegion,
    OcrRequest, sha256_hex,
};
use babel_runtime::ipc::{
    Handshake, PROTOCOL_MAJOR, PROTOCOL_MINOR, WorkerRequest, WorkerResponse, read_frame,
    validate_handshake, write_frame,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
enum OcrWorkerRequest {
    ProbeAssets,
    Recognize {
        image_bytes: Vec<u8>,
        #[serde(default)]
        input_kind: OcrInputKind,
        #[serde(default = "default_media_type")]
        media_type: String,
        #[serde(default)]
        source_hash_hex: String,
        #[serde(default)]
        profile: OcrProfile,
    },
}

#[derive(Debug, Serialize)]
struct ProbeAssetsReply {
    engine: String,
    engine_version: String,
    runtime: String,
    runtime_version: String,
    asset_count: usize,
    verified: bool,
}

#[derive(Debug, Serialize)]
struct RecognizeReply {
    document: OcrDocument,
}

fn default_media_type() -> String {
    "application/octet-stream".to_owned()
}

struct PaddleOcrEngine {
    ocr: ppocr_rs::OcrLite,
    descriptor: OcrEngineDescriptor,
    capabilities: OcrCapabilities,
}

impl OcrEngine for PaddleOcrEngine {
    fn descriptor(&self) -> &OcrEngineDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> &OcrCapabilities {
        &self.capabilities
    }

    fn recognize(&mut self, request: OcrRequest) -> Result<OcrDocument, OcrEngineError> {
        request.validate()?;
        let image = image::load_from_memory(&request.image_bytes)
            .map_err(|error| OcrEngineError::Inference(format!("decode input image: {error}")))?
            .to_rgb8();
        let width = image.width();
        let height = image.height();
        let result = self
            .ocr
            .detect(&image, 10, 960, 0.6, 0.3, 1.6, false, false)
            .map_err(|error| OcrEngineError::Inference(error.to_string()))?;
        let language = if request.profile.languages.len() == 1 {
            request.profile.languages.first().cloned()
        } else {
            None
        };
        let regions = result
            .text_blocks
            .into_iter()
            .enumerate()
            .map(|(reading_order, block)| OcrRegion {
                reading_order: reading_order as u32,
                polygon: block
                    .box_points
                    .into_iter()
                    .map(|point| OcrPoint {
                        x: point.x as f32,
                        y: point.y as f32,
                    })
                    .collect(),
                block_type: OcrBlockType::Text,
                language: language.clone(),
                normalized_text: block.text.trim().to_owned(),
                text: block.text,
                confidence_millionths: (block.text_score.clamp(0.0, 1.0) * 1_000_000.0) as u32,
            })
            .filter(|region| {
                region.confidence_millionths >= request.profile.confidence_threshold_millionths
            })
            .collect();
        let document = OcrDocument {
            schema_version: babel_ocr::OCR_DOCUMENT_SCHEMA_VERSION,
            source_hash_hex: request.source_hash_hex,
            input_kind: request.input_kind,
            profile: request.profile,
            engine: self.descriptor.clone(),
            pages: vec![OcrPage {
                page_index: 0,
                width,
                height,
                regions,
            }],
        };
        document.validate()?;
        Ok(document)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let manifest_path =
        env::var_os("BABEL_OCR_MANIFEST").context("BABEL_OCR_MANIFEST is required")?;
    let asset_root =
        env::var_os("BABEL_OCR_ASSET_ROOT").context("BABEL_OCR_ASSET_ROOT is required")?;
    let manifest = OcrAssetManifest::load(manifest_path)?;
    let verified_assets = manifest.verify_files(&asset_root)?;
    let asset_path = |kind: OcrAssetKind| {
        let asset = manifest
            .asset(kind)
            .with_context(|| format!("missing OCR asset kind {kind:?}"))?;
        verified_assets
            .iter()
            .find(|verified| verified.id == asset.id)
            .map(|verified| verified.path.clone())
            .with_context(|| format!("asset `{}` was not verified", asset.id))
    };
    let det_path = asset_path(OcrAssetKind::DetectionModel)?;
    let rec_path = asset_path(OcrAssetKind::RecognitionModel)?;
    let dict_path = asset_path(OcrAssetKind::Dictionary)?;
    let mut ocr = ppocr_rs::OcrLite::new();
    ocr.init_models_no_angle(
        det_path
            .to_str()
            .context("detection model path is not UTF-8")?,
        rec_path
            .to_str()
            .context("recognition model path is not UTF-8")?,
        dict_path
            .to_str()
            .context("OCR dictionary path is not UTF-8")?,
        2,
    )
    .map_err(|error| anyhow::anyhow!("initialize PP-OCR models: {error}"))?;
    let descriptor = OcrEngineDescriptor {
        engine_id: manifest.engine.clone(),
        engine_version: manifest.engine_version.clone(),
        runtime: manifest.runtime.clone(),
        runtime_version: manifest.runtime_version.clone(),
        model_ids: manifest
            .assets
            .iter()
            .filter(|asset| {
                matches!(
                    asset.kind,
                    OcrAssetKind::DetectionModel
                        | OcrAssetKind::RecognitionModel
                        | OcrAssetKind::Dictionary
                )
            })
            .map(|asset| asset.id.clone())
            .collect(),
    };
    let capabilities = OcrCapabilities {
        input_kinds: vec![
            OcrInputKind::Image,
            OcrInputKind::PdfPage,
            OcrInputKind::DocumentPage,
        ],
        languages: Vec::new(),
        language_detection: false,
        layout_analysis: false,
        tables: false,
        vertical_text: false,
        coordinates: true,
    };
    let mut engine = PaddleOcrEngine {
        ocr,
        descriptor,
        capabilities,
    };
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let handshake: Handshake = read_frame(&mut stdin).context("read OCR worker handshake")?;
    validate_handshake(
        &handshake,
        &handshake.session_nonce,
        &handshake.capability_token,
    )
    .context("validate OCR worker handshake")?;
    write_frame(
        &mut stdout,
        &Handshake {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_nonce: handshake.session_nonce,
            capability_token: handshake.capability_token,
        },
    )?;
    loop {
        let request: WorkerRequest = match read_frame(&mut stdin) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };
        let request_id = request.request_id;
        let response = match serde_json::from_slice::<OcrWorkerRequest>(&request.payload) {
            Ok(OcrWorkerRequest::ProbeAssets) => WorkerResponse {
                request_id,
                status: 0,
                payload: serde_json::to_vec(&ProbeAssetsReply {
                    engine: manifest.engine.clone(),
                    engine_version: manifest.engine_version.clone(),
                    runtime: manifest.runtime.clone(),
                    runtime_version: manifest.runtime_version.clone(),
                    asset_count: verified_assets.len(),
                    verified: true,
                })?,
                diagnostic: String::new(),
            },
            Ok(OcrWorkerRequest::Recognize {
                image_bytes,
                input_kind,
                media_type,
                source_hash_hex,
                profile,
            }) => {
                let source_hash_hex = if source_hash_hex.is_empty() {
                    sha256_hex(&image_bytes)
                } else {
                    source_hash_hex
                };
                let request = OcrRequest {
                    input_kind,
                    media_type,
                    source_hash_hex,
                    image_bytes,
                    profile,
                };
                match engine.recognize(request) {
                    Ok(reply) => WorkerResponse {
                        request_id,
                        status: 0,
                        payload: serde_json::to_vec(&RecognizeReply { document: reply })?,
                        diagnostic: String::new(),
                    },
                    Err(error) => WorkerResponse {
                        request_id,
                        status: 1,
                        payload: Vec::new(),
                        diagnostic: error.to_string(),
                    },
                }
            }
            Err(error) => WorkerResponse {
                request_id,
                status: 1,
                payload: Vec::new(),
                diagnostic: format!("invalid OCR worker request: {error}"),
            },
        };
        write_frame(&mut stdout, &response)?;
    }
}
