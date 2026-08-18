//! Format-neutral workbench state shared by every presentation mode.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use crate::core::{ProjectId, ResourceId, UnitId};

pub const NAVIGATION_POSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceView {
    LongForm,
    Units,
    Resources,
}

impl WorkspaceView {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LongForm => "LongForm",
            Self::Units => "Units",
            Self::Resources => "Resources",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "LongForm" => Some(Self::LongForm),
            "Units" => Some(Self::Units),
            "Resources" => Some(Self::Resources),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranslationStatus {
    Untranslated,
    Draft,
    Translated,
    Reviewed,
    Blocked,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationFilters {
    pub query: Option<String>,
    pub statuses: Vec<TranslationStatus>,
    pub only_incomplete: bool,
    pub only_with_issues: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationPosition {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub view: WorkspaceView,
    pub unit_id: Option<UnitId>,
    pub resource_id: Option<ResourceId>,
    pub region_id: Option<ResourceId>,
    pub scroll_anchor_unit_id: Option<UnitId>,
    pub scroll_offset_px: i32,
    pub zoom_millionths: u32,
    pub filters: NavigationFilters,
}

impl NavigationPosition {
    pub fn new(project_id: ProjectId, view: WorkspaceView) -> Self {
        Self {
            schema_version: NAVIGATION_POSITION_SCHEMA_VERSION,
            project_id,
            view,
            unit_id: None,
            resource_id: None,
            region_id: None,
            scroll_anchor_unit_id: None,
            scroll_offset_px: 0,
            zoom_millionths: 1_000_000,
            filters: NavigationFilters::default(),
        }
    }

    pub fn validate(&self) -> Result<(), NavigationPositionError> {
        if self.schema_version != NAVIGATION_POSITION_SCHEMA_VERSION {
            return Err(NavigationPositionError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        if self.region_id.is_some() && self.resource_id.is_none() {
            return Err(NavigationPositionError::RegionWithoutResource);
        }
        if !(100_000..=8_000_000).contains(&self.zoom_millionths) {
            return Err(NavigationPositionError::InvalidZoom);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationPositionError {
    UnsupportedSchema(u32),
    RegionWithoutResource,
    InvalidZoom,
}

impl fmt::Display for NavigationPositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported navigation schema version {version}")
            }
            Self::RegionWithoutResource => {
                formatter.write_str("a navigation region requires an owning resource")
            }
            Self::InvalidZoom => formatter.write_str("navigation zoom is outside safe bounds"),
        }
    }
}

impl Error for NavigationPositionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_region_requires_an_owning_resource() {
        let mut position = NavigationPosition::new(ProjectId::new(), WorkspaceView::Resources);
        position.region_id = Some(ResourceId::new());
        assert_eq!(
            position.validate(),
            Err(NavigationPositionError::RegionWithoutResource)
        );
    }

    #[test]
    fn navigation_defaults_to_a_stable_one_hundred_percent_zoom() {
        let position = NavigationPosition::new(ProjectId::new(), WorkspaceView::LongForm);
        assert_eq!(position.zoom_millionths, 1_000_000);
        assert_eq!(position.validate(), Ok(()));
    }
}
