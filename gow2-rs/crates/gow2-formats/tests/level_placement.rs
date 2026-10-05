//! Level placement against the Python exporter's report for RHOD10 (`analysis/levels/RHOD10_gltf/report.json`):
//! 38 ref instances, 7 nodes bound to models, 25 rigged models.
use std::path::PathBuf;

use gow2_formats::{level, wad};

#[test]
fn rhod10_placement_matches_the_python_report() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../extracted/pak/RHOD10.WAD");
    let Ok(data) = std::fs::read(p) else { return };
    let recs: Vec<wad::Record> = wad::records(&data).collect();
    let pl = level::parse(&recs);
    eprintln!("{} instances, {} drawable models, {} rigged, {} plain", pl.instances.len(), pl.models.len(), pl.rigged.len(), pl.plain_models().len());
    assert_eq!(pl.instances.len(), 38);
    assert_eq!(pl.instances.iter().map(|i| i.model.as_str()).collect::<std::collections::HashSet<_>>().len(), 7);
    // the report counts rigged models that also have a drawable mesh: 25 without the models whose mesh record name is cut to 19 characters
    // (`wad::mesh_record_name`, the Python report misses them too), 37 with them
    assert_eq!(pl.models.iter().filter(|m| pl.rigged.contains(*m)).count(), 37);
    assert_eq!(pl.models.len(), 82);
    assert!(pl.models.iter().any(|m| m == "storageRoomMain" || m == "ballistaPlatform"));
    assert!(pl.models.iter().any(|m| m == "innerPillar"));
}
