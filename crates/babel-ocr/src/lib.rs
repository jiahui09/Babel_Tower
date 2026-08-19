//! Offline OCR asset closure contracts.
//!
//! This crate deliberately does not contain an OCR engine. It validates the
//! immutable runtime/model bundle before an OCR worker is allowed to start.

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const OCR_ASSET_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const OCR_DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const OCR_PROFILE_SCHEMA_VERSION: u32 = 1;
pub const MAX_ASSET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_OCR_LANGUAGES: usize = 32;
pub const MAX_OCR_REGIONS_PER_PAGE: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrAssetManifest {
    pub schema_version: u32,
    pub engine: String,
    pub engine_version: String,
    pub runtime: String,
    pub runtime_version: String,
    pub assets: Vec<OcrAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrAsset {
    pub id: String,
    pub kind: OcrAssetKind,
    pub relative_path: String,
    pub byte_length: u64,
    pub sha256_hex: String,
    pub license_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrAssetKind {
    Runtime,
    DetectionModel,
    RecognitionModel,
    Dictionary,
    Font,
    License,
}

/// 输入资源的语义类型。引擎选择依据能力，而不是文件扩展名。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrInputKind {
    #[default]
    Image,
    PdfPage,
    DocumentPage,
    VideoFrame,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrLanguageMode {
    #[default]
    Auto,
    Explicit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrOrientation {
    #[default]
    Auto,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrLayoutMode {
    #[default]
    Auto,
    Plain,
    Document,
    Comic,
    GameUi,
    Table,
}

/// 可复现的场景参数。Profile 属于识别请求，不属于某个具体引擎。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrProfile {
    pub schema_version: u32,
    pub language_mode: OcrLanguageMode,
    pub languages: Vec<String>,
    pub orientation: OcrOrientation,
    pub layout: OcrLayoutMode,
    pub preserve_coordinates: bool,
    pub confidence_threshold_millionths: u32,
}

impl Default for OcrProfile {
    fn default() -> Self {
        Self {
            schema_version: OCR_PROFILE_SCHEMA_VERSION,
            language_mode: OcrLanguageMode::Auto,
            languages: Vec::new(),
            orientation: OcrOrientation::Auto,
            layout: OcrLayoutMode::Auto,
            preserve_coordinates: true,
            confidence_threshold_millionths: 600_000,
        }
    }
}

impl OcrProfile {
    pub fn validate(&self) -> Result<(), OcrEngineError> {
        if self.schema_version != OCR_PROFILE_SCHEMA_VERSION {
            return Err(OcrEngineError::InvalidRequest(format!(
                "unsupported OCR profile schema version {}",
                self.schema_version
            )));
        }
        if self.languages.len() > MAX_OCR_LANGUAGES {
            return Err(OcrEngineError::InvalidRequest(
                "too many OCR language hints".to_owned(),
            ));
        }
        if self
            .languages
            .iter()
            .any(|language| language.trim().is_empty())
        {
            return Err(OcrEngineError::InvalidRequest(
                "OCR language hints cannot be empty".to_owned(),
            ));
        }
        if self.language_mode == OcrLanguageMode::Explicit && self.languages.is_empty() {
            return Err(OcrEngineError::InvalidRequest(
                "explicit OCR language mode requires at least one language".to_owned(),
            ));
        }
        if self.confidence_threshold_millionths > 1_000_000 {
            return Err(OcrEngineError::InvalidRequest(
                "OCR confidence threshold must be at most one millionth".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrCapabilities {
    pub input_kinds: Vec<OcrInputKind>,
    pub languages: Vec<String>,
    pub language_detection: bool,
    pub layout_analysis: bool,
    pub tables: bool,
    pub vertical_text: bool,
    pub coordinates: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrEngineDescriptor {
    pub engine_id: String,
    pub engine_version: String,
    pub runtime: String,
    pub runtime_version: String,
    pub model_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrRequest {
    pub input_kind: OcrInputKind,
    pub media_type: String,
    pub source_hash_hex: String,
    pub image_bytes: Vec<u8>,
    pub profile: OcrProfile,
}

impl OcrRequest {
    pub fn validate(&self) -> Result<(), OcrEngineError> {
        self.profile.validate()?;
        if self.media_type.trim().is_empty() {
            return Err(OcrEngineError::InvalidRequest(
                "OCR media type is required".to_owned(),
            ));
        }
        if self.image_bytes.is_empty() {
            return Err(OcrEngineError::InvalidRequest(
                "OCR image bytes cannot be empty".to_owned(),
            ));
        }
        if self.source_hash_hex.len() != 64 || hex::decode(&self.source_hash_hex).is_err() {
            return Err(OcrEngineError::InvalidRequest(
                "OCR source hash must be a SHA-256 hex string".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrDocument {
    pub schema_version: u32,
    pub source_hash_hex: String,
    pub input_kind: OcrInputKind,
    pub profile: OcrProfile,
    pub engine: OcrEngineDescriptor,
    pub pages: Vec<OcrPage>,
}

impl OcrDocument {
    pub fn validate(&self) -> Result<(), OcrEngineError> {
        if self.schema_version != OCR_DOCUMENT_SCHEMA_VERSION {
            return Err(OcrEngineError::InvalidRequest(format!(
                "unsupported OCR document schema version {}",
                self.schema_version
            )));
        }
        if self.source_hash_hex.len() != 64 || hex::decode(&self.source_hash_hex).is_err() {
            return Err(OcrEngineError::InvalidRequest(
                "OCR document source hash must be a SHA-256 hex string".to_owned(),
            ));
        }
        self.profile.validate()?;
        for page in &self.pages {
            if page.width == 0 || page.height == 0 {
                return Err(OcrEngineError::InvalidRequest(
                    "OCR page dimensions must be non-zero".to_owned(),
                ));
            }
            if page.regions.len() > MAX_OCR_REGIONS_PER_PAGE {
                return Err(OcrEngineError::InvalidRequest(
                    "OCR page contains too many regions".to_owned(),
                ));
            }
            for region in &page.regions {
                if region.confidence_millionths > 1_000_000 || region.polygon.len() < 3 {
                    return Err(OcrEngineError::InvalidRequest(
                        "OCR region has invalid confidence or polygon".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrPage {
    pub page_index: u32,
    pub width: u32,
    pub height: u32,
    pub regions: Vec<OcrRegion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrRegion {
    pub reading_order: u32,
    pub polygon: Vec<OcrPoint>,
    pub block_type: OcrBlockType,
    pub language: Option<String>,
    pub text: String,
    pub normalized_text: String,
    pub confidence_millionths: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrBlockType {
    #[default]
    Text,
    Heading,
    Table,
    Caption,
    Dialogue,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OcrPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum OcrEngineError {
    #[error("invalid OCR request: {0}")]
    InvalidRequest(String),
    #[error("OCR engine is unavailable: {0}")]
    Unavailable(String),
    #[error("OCR inference failed: {0}")]
    Inference(String),
}

/// 所有 OCR 引擎的最小稳定边界。引擎不得直接写入项目权威存储。
pub trait OcrEngine {
    fn descriptor(&self) -> &OcrEngineDescriptor;
    fn capabilities(&self) -> &OcrCapabilities;
    fn recognize(&mut self, request: OcrRequest) -> Result<OcrDocument, OcrEngineError>;
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedOcrAsset {
    pub id: String,
    pub kind: OcrAssetKind,
    pub path: PathBuf,
    pub byte_length: u64,
    pub sha256: [u8; 32],
}

#[derive(Debug, Error)]
pub enum OcrAssetError {
    #[error("cannot read OCR asset manifest: {0}")]
    ManifestIo(#[from] io::Error),
    #[error("invalid OCR asset manifest: {0}")]
    InvalidManifest(String),
    #[error("OCR asset `{id}` is invalid: {reason}")]
    InvalidAsset { id: String, reason: String },
}

impl OcrAssetManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, OcrAssetError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| OcrAssetError::InvalidManifest(error.to_string()))?;
        manifest.validate_shape()?;
        Ok(manifest)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, OcrAssetError> {
        Self::from_json(&std::fs::read(path)?)
    }

    pub fn verify_files(
        &self,
        root: impl AsRef<Path>,
    ) -> Result<Vec<VerifiedOcrAsset>, OcrAssetError> {
        self.validate_shape()?;
        let root = root.as_ref();
        let mut verified = Vec::with_capacity(self.assets.len());
        for asset in &self.assets {
            let path = safe_join(root, &asset.relative_path).map_err(|reason| {
                OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason,
                }
            })?;
            let metadata =
                std::fs::metadata(&path).map_err(|error| OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: error.to_string(),
                })?;
            if !metadata.is_file() {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: "asset path is not a regular file".to_owned(),
                });
            }
            if metadata.len() != asset.byte_length {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: format!(
                        "length mismatch: expected {}, got {}",
                        asset.byte_length,
                        metadata.len()
                    ),
                });
            }
            let mut file = File::open(&path)?;
            let (hash, byte_length) = hash_reader(&mut file)?;
            if hash
                != parse_hash(&asset.sha256_hex).map_err(|reason| OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason,
                })?
            {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: "SHA-256 mismatch".to_owned(),
                });
            }
            verified.push(VerifiedOcrAsset {
                id: asset.id.clone(),
                kind: asset.kind,
                path,
                byte_length,
                sha256: hash,
            });
        }
        Ok(verified)
    }

    pub fn asset(&self, kind: OcrAssetKind) -> Option<&OcrAsset> {
        self.assets.iter().find(|asset| asset.kind == kind)
    }

    fn validate_shape(&self) -> Result<(), OcrAssetError> {
        if self.schema_version != OCR_ASSET_MANIFEST_SCHEMA_VERSION {
            return Err(OcrAssetError::InvalidManifest(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        if self.engine.trim().is_empty()
            || self.engine_version.trim().is_empty()
            || self.runtime.trim().is_empty()
            || self.runtime_version.trim().is_empty()
        {
            return Err(OcrAssetError::InvalidManifest(
                "engine and runtime versions are required".to_owned(),
            ));
        }
        if self.assets.is_empty() {
            return Err(OcrAssetError::InvalidManifest(
                "asset list cannot be empty".to_owned(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for asset in &self.assets {
            if asset.id.trim().is_empty() || !ids.insert(&asset.id) {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: "duplicate or empty id".to_owned(),
                });
            }
            if asset.byte_length == 0 || asset.byte_length > MAX_ASSET_BYTES {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: "asset size is outside the allowed range".to_owned(),
                });
            }
            parse_hash(&asset.sha256_hex).map_err(|reason| OcrAssetError::InvalidAsset {
                id: asset.id.clone(),
                reason,
            })?;
            if asset.license_ids.is_empty() {
                return Err(OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason: "license_ids cannot be empty".to_owned(),
                });
            }
            safe_join(Path::new("."), &asset.relative_path).map_err(|reason| {
                OcrAssetError::InvalidAsset {
                    id: asset.id.clone(),
                    reason,
                }
            })?;
        }
        Ok(())
    }
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.is_empty() {
        return Err("relative_path must be a non-empty relative path".to_owned());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("relative_path cannot escape the asset root".to_owned());
    }
    Ok(root.join(path))
}

fn parse_hash(value: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(value).map_err(|error| format!("invalid SHA-256 hex: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| "SHA-256 must contain exactly 32 bytes".to_owned())
}

fn hash_reader(reader: &mut impl Read) -> Result<([u8; 32], u64), io::Error> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_ASSET_BYTES {
            return Err(io::Error::other("OCR asset exceeds size limit"));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hasher.finalize().into(), total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn verifies_manifest_files_and_hashes() {
        let root = tempdir().unwrap();
        std::fs::write(root.path().join("model.onnx"), b"model").unwrap();
        let hash = Sha256::digest(b"model");
        let manifest = OcrAssetManifest {
            schema_version: 1,
            engine: "PaddleOCR".to_owned(),
            engine_version: "PP-OCRv6".to_owned(),
            runtime: "ONNX Runtime".to_owned(),
            runtime_version: "frozen".to_owned(),
            assets: vec![OcrAsset {
                id: "recognizer".to_owned(),
                kind: OcrAssetKind::RecognitionModel,
                relative_path: "model.onnx".to_owned(),
                byte_length: 5,
                sha256_hex: hex::encode(hash),
                license_ids: vec!["model-license".to_owned()],
            }],
        };
        let verified = manifest.verify_files(root.path()).unwrap();
        assert_eq!(verified[0].byte_length, 5);
    }

    #[test]
    fn rejects_path_escape_and_hash_mismatch() {
        let manifest = OcrAssetManifest {
            schema_version: 1,
            engine: "PaddleOCR".to_owned(),
            engine_version: "v6".to_owned(),
            runtime: "ONNX Runtime".to_owned(),
            runtime_version: "v1".to_owned(),
            assets: vec![OcrAsset {
                id: "model".to_owned(),
                kind: OcrAssetKind::DetectionModel,
                relative_path: "../model.onnx".to_owned(),
                byte_length: 1,
                sha256_hex: "00".repeat(32),
                license_ids: vec!["license".to_owned()],
            }],
        };
        assert!(manifest.verify_files(".").is_err());
    }

    #[test]
    fn profile_requires_explicit_languages_and_bounded_thresholds() {
        let mut profile = OcrProfile {
            language_mode: OcrLanguageMode::Explicit,
            ..OcrProfile::default()
        };
        assert!(profile.validate().is_err());
        profile.languages.push("zh-CN".to_owned());
        profile.confidence_threshold_millionths = 1_000_001;
        assert!(profile.validate().is_err());
        profile.confidence_threshold_millionths = 800_000;
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn request_and_document_preserve_source_provenance() {
        let bytes = b"source";
        let request = OcrRequest {
            input_kind: OcrInputKind::PdfPage,
            media_type: "image/png".to_owned(),
            source_hash_hex: sha256_hex(bytes),
            image_bytes: bytes.to_vec(),
            profile: OcrProfile::default(),
        };
        assert!(request.validate().is_ok());
        let document = OcrDocument {
            schema_version: OCR_DOCUMENT_SCHEMA_VERSION,
            source_hash_hex: request.source_hash_hex.clone(),
            input_kind: request.input_kind,
            profile: request.profile,
            engine: OcrEngineDescriptor {
                engine_id: "test".to_owned(),
                engine_version: "1".to_owned(),
                runtime: "test".to_owned(),
                runtime_version: "1".to_owned(),
                model_ids: vec!["model".to_owned()],
            },
            pages: vec![OcrPage {
                page_index: 0,
                width: 100,
                height: 100,
                regions: vec![OcrRegion {
                    reading_order: 0,
                    polygon: vec![
                        OcrPoint { x: 0.0, y: 0.0 },
                        OcrPoint { x: 10.0, y: 0.0 },
                        OcrPoint { x: 10.0, y: 10.0 },
                    ],
                    block_type: OcrBlockType::Text,
                    language: None,
                    text: "recognized".to_owned(),
                    normalized_text: "recognized".to_owned(),
                    confidence_millionths: 900_000,
                }],
            }],
        };
        assert!(document.validate().is_ok());
    }
}
