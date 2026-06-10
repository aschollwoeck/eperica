//! Balance-data loading.
//!
//! Numeric and structural balance lives in `specs/balance/` as **data** (not hardcoded in logic, per
//! the constitution). This module embeds that data at compile time and parses it into pure domain
//! types, keeping the domain itself free of serialization concerns.

use eperica_domain::{
    BuildingKind, BuildingSlot, DomainError, ResourceField, ResourceKind, StartingVillage,
};
use serde::Deserialize;

/// Embedded starting-village balance data.
const STARTING_VILLAGE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/starting-village.toml"
));

/// Errors that can occur while loading balance data.
#[derive(Debug, thiserror::Error)]
pub enum BalanceError {
    /// The balance file could not be parsed as TOML.
    #[error("failed to parse balance data: {0}")]
    Parse(#[from] toml::de::Error),
    /// An unknown resource name appeared in the data.
    #[error("unknown resource: {0}")]
    UnknownResource(String),
    /// An unknown building name appeared in the data.
    #[error("unknown building: {0}")]
    UnknownBuilding(String),
    /// The parsed data did not form a valid domain template.
    #[error(transparent)]
    Domain(DomainError),
}

#[derive(Deserialize)]
struct StartingVillageDto {
    fields: Vec<FieldDto>,
    buildings: Vec<BuildingDto>,
}

#[derive(Deserialize)]
struct FieldDto {
    resource: String,
    count: usize,
    level: u8,
}

#[derive(Deserialize)]
struct BuildingDto {
    building: String,
    level: u8,
}

/// Load the starting-village template from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or does not form a valid template.
pub fn starting_village() -> Result<StartingVillage, BalanceError> {
    parse_starting_village(STARTING_VILLAGE_TOML)
}

fn parse_starting_village(toml_src: &str) -> Result<StartingVillage, BalanceError> {
    let dto: StartingVillageDto = toml::from_str(toml_src)?;

    let mut fields = Vec::new();
    for f in &dto.fields {
        let kind = parse_resource(&f.resource)?;
        fields.extend(std::iter::repeat_n(
            ResourceField {
                kind,
                level: f.level,
            },
            f.count,
        ));
    }

    let mut buildings = Vec::with_capacity(dto.buildings.len());
    for b in &dto.buildings {
        buildings.push(BuildingSlot {
            kind: parse_building(&b.building)?,
            level: b.level,
        });
    }

    StartingVillage::new(fields, buildings).map_err(BalanceError::Domain)
}

fn parse_resource(name: &str) -> Result<ResourceKind, BalanceError> {
    match name {
        "wood" => Ok(ResourceKind::Wood),
        "clay" => Ok(ResourceKind::Clay),
        "iron" => Ok(ResourceKind::Iron),
        "crop" => Ok(ResourceKind::Crop),
        other => Err(BalanceError::UnknownResource(other.to_owned())),
    }
}

fn parse_building(name: &str) -> Result<BuildingKind, BalanceError> {
    match name {
        "main_building" => Ok(BuildingKind::MainBuilding),
        "rally_point" => Ok(BuildingKind::RallyPoint),
        other => Err(BalanceError::UnknownBuilding(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_balanced_starting_village() {
        let sv = starting_village().expect("balance data loads");
        assert_eq!(sv.fields().len(), 18);

        let count = |k: ResourceKind| sv.fields().iter().filter(|f| f.kind == k).count();
        assert_eq!(count(ResourceKind::Wood), 4);
        assert_eq!(count(ResourceKind::Clay), 4);
        assert_eq!(count(ResourceKind::Iron), 4);
        assert_eq!(count(ResourceKind::Crop), 6);

        let bkinds: Vec<_> = sv.buildings().iter().map(|b| b.kind).collect();
        assert!(bkinds.contains(&BuildingKind::MainBuilding));
        assert!(bkinds.contains(&BuildingKind::RallyPoint));
    }
}
