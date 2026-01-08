use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::Value;

use crate::feed::LOCATIONS_GEOJSON_FILE;
use crate::{NoticeSeverity, ValidationNotice};

#[derive(Debug, Clone, Default)]
pub struct LocationsGeoJson {
    pub location_ids: HashSet<gtfs_guru_model::StringId>,
    pub bounds_by_id: HashMap<gtfs_guru_model::StringId, BoundingBox>,
    pub feature_index_by_id: HashMap<gtfs_guru_model::StringId, usize>,
    pub notices: Vec<ValidationNotice>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeoJsonFeatureCollection {
    #[serde(default, rename = "type")]
    pub collection_type: Option<String>,
    #[serde(default)]
    pub features: Option<Vec<GeoJsonFeature>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeoJsonFeature {
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default, rename = "type")]
    pub feature_type: Option<String>,
    #[serde(default)]
    pub properties: Option<Value>,
    #[serde(default)]
    pub geometry: Option<GeoJsonGeometry>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeoJsonGeometry {
    #[serde(default, rename = "type")]
    pub geometry_type: Option<String>,
    #[serde(default)]
    pub coordinates: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn overlaps(&self, other: &BoundingBox) -> bool {
        !(self.max_x <= other.min_x
            || self.min_x >= other.max_x
            || self.max_y <= other.min_y
            || self.min_y >= other.max_y)
    }
}

impl LocationsGeoJson {
    pub(crate) fn new(
        collection: GeoJsonFeatureCollection,
        pool: &crate::string_pool::StringPool,
    ) -> Self {
        let mut location_ids = HashSet::new();
        let mut bounds_by_id = HashMap::new();
        let mut feature_index_by_id = HashMap::new();
        let mut notices = Vec::new();
        let mut seen_geometries = HashSet::new();
        for key in collection.extra.keys() {
            notices.push(geojson_unknown_element_notice(key));
        }

        match collection.collection_type.as_deref() {
            Some("FeatureCollection") => {}
            Some(value) => {
                notices.push(unsupported_geojson_type_notice(value));
                return Self {
                    location_ids,
                    bounds_by_id,
                    feature_index_by_id,
                    notices,
                };
            }
            None => {
                notices.push(missing_required_element_notice(None, "type", None));
                return Self {
                    location_ids,
                    bounds_by_id,
                    feature_index_by_id,
                    notices,
                };
            }
        }

        if collection.features.is_none() {
            notices.push(missing_required_element_notice(None, "features", None));
            return Self {
                location_ids,
                bounds_by_id,
                feature_index_by_id,
                notices,
            };
        }

        let mut first_index_by_id: HashMap<gtfs_guru_model::StringId, usize> = HashMap::new();
        for (index, feature) in collection.features.unwrap().into_iter().enumerate() {
            for key in feature.extra.keys() {
                notices.push(geojson_unknown_element_notice(key));
            }

            let feature_id = feature.id.and_then(value_to_id);
            let mut missing_required_fields = Vec::new();
            if feature_id
                .as_deref()
                .map(|id| id.trim().is_empty())
                .unwrap_or(true)
            {
                missing_required_fields.push("features.id");
            }
            let feature_type = feature.feature_type.as_deref();
            if feature_type.is_none() {
                missing_required_fields.push("features.type");
            } else if feature_type != Some("Feature") {
                notices.push(unsupported_feature_type_notice(
                    index,
                    feature_id.as_deref(),
                    feature_type.unwrap_or_default(),
                ));
            }
            if feature.properties.is_none() {
                missing_required_fields.push("features.properties");
            }

            let geometry = feature.geometry.as_ref();
            if geometry.is_none() {
                missing_required_fields.push("features.geometry");
            } else {
                for key in geometry
                    .and_then(|geo| Some(geo.extra.keys()))
                    .into_iter()
                    .flatten()
                {
                    notices.push(geojson_unknown_element_notice(key));
                }
                if geometry
                    .and_then(|geo| geo.geometry_type.as_deref())
                    .is_none()
                {
                    missing_required_fields.push("features.geometry.type");
                }
                if geometry.and_then(|geo| geo.coordinates.as_ref()).is_none() {
                    missing_required_fields.push("features.geometry.coordinates");
                }
            }

            if !missing_required_fields.is_empty() {
                for missing in missing_required_fields {
                    notices.push(missing_required_element_notice(
                        feature_id.as_deref(),
                        missing,
                        Some(index),
                    ));
                }
                continue;
            }

            let Some(feature_id_str) = feature_id else {
                continue;
            };
            let feature_id = pool.intern(&feature_id_str);

            if let Some(first_index) = first_index_by_id.get(&feature_id).copied() {
                notices.push(duplicate_geojson_key_notice(
                    &feature_id_str,
                    first_index,
                    index,
                ));
            } else {
                first_index_by_id.insert(feature_id, index);
                feature_index_by_id.insert(feature_id, index);
            }

            location_ids.insert(feature_id);
            if let Some(geometry) = geometry {
                let geometry_type = geometry.geometry_type.as_deref().unwrap_or("");
                match geometry_type {
                    "Polygon" => {
                        if let Some(coords) = geometry.coordinates.as_ref() {
                            let signature = coords.to_string();
                            if !seen_geometries.insert(signature.clone()) {
                                notices.push(geojson_duplicated_element_notice(&signature));
                            }
                            match points_from_polygon(coords) {
                                Ok(points) => {
                                    check_points(
                                        &mut notices,
                                        feature_id_str.as_str(),
                                        index,
                                        &points,
                                    );
                                    // Check for self-intersecting polygon
                                    if let Some(ring) = coords.as_array().and_then(|a| a.first()) {
                                        if let Some(ring_points) = ring.as_array() {
                                            if is_self_intersecting(ring_points) {
                                                notices.push(invalid_geometry_notice(
                                                    feature_id_str.as_str(),
                                                    index,
                                                    geometry_type,
                                                    "polygon ring is self-intersecting".into(),
                                                ));
                                            }
                                        }
                                    }
                                    if let Some(bounds) = bounds_from_points(points) {
                                        bounds_by_id.entry(feature_id).or_insert(bounds);
                                    }
                                }
                                Err(message) => notices.push(invalid_geometry_notice(
                                    feature_id_str.as_str(),
                                    index,
                                    geometry_type,
                                    message,
                                )),
                            }
                        }
                    }
                    "MultiPolygon" => {
                        if let Some(coords) = geometry.coordinates.as_ref() {
                            let signature = coords.to_string();
                            if !seen_geometries.insert(signature.clone()) {
                                notices.push(geojson_duplicated_element_notice(&signature));
                            }
                            match points_from_multipolygon(coords) {
                                Ok(points) => {
                                    check_points(
                                        &mut notices,
                                        feature_id_str.as_str(),
                                        index,
                                        &points,
                                    );
                                    if let Some(bounds) = bounds_from_points(points) {
                                        bounds_by_id.entry(feature_id).or_insert(bounds);
                                    }
                                }
                                Err(message) => notices.push(invalid_geometry_notice(
                                    feature_id_str.as_str(),
                                    index,
                                    geometry_type,
                                    message,
                                )),
                            }
                        }
                    }
                    other => {
                        notices.push(unsupported_geometry_type_notice(
                            index,
                            feature_id_str.as_str(),
                            other,
                        ));
                    }
                }
            }
        }
        Self {
            location_ids,
            bounds_by_id,
            feature_index_by_id,
            notices,
        }
    }
}

impl LocationsGeoJson {
    pub fn malformed_json(message: impl Into<String>) -> Self {
        let mut notice = ValidationNotice::new(
            "malformed_json",
            NoticeSeverity::Error,
            "malformed JSON file",
        );
        notice.insert_context_field("filename", LOCATIONS_GEOJSON_FILE);
        notice.insert_context_field("message", message.into());
        notice.field_order = vec!["filename".into(), "message".into()];
        Self {
            location_ids: HashSet::new(),
            bounds_by_id: HashMap::new(),
            feature_index_by_id: HashMap::new(),
            notices: vec![notice],
        }
    }

    pub fn has_fatal_errors(&self) -> bool {
        self.notices
            .iter()
            .any(|notice| match notice.code.as_str() {
                "malformed_json" | "unsupported_geo_json_type" => true,
                "missing_required_element" => {
                    let missing = notice
                        .context
                        .get("missingElement")
                        .and_then(|value| value.as_str());
                    let feature_index = notice.context.get("featureIndex");
                    missing == Some("type") && feature_index.map_or(false, |value| value.is_null())
                }
                _ => false,
            })
    }
}

fn value_to_id(value: Value) -> Option<String> {
    match value {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn bounds_from_points(points: Vec<(f64, f64)>) -> Option<BoundingBox> {
    let mut iter = points.into_iter();
    let (first_x, first_y) = iter.next()?;
    let mut min_x = first_x;
    let mut max_x = first_x;
    let mut min_y = first_y;
    let mut max_y = first_y;
    for (x, y) in iter {
        if x < min_x {
            min_x = x;
        }
        if x > max_x {
            max_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if y > max_y {
            max_y = y;
        }
    }
    Some(BoundingBox {
        min_x,
        min_y,
        max_x,
        max_y,
    })
}

fn points_from_polygon(coords: &Value) -> Result<Vec<(f64, f64)>, String> {
    let rings = coords
        .as_array()
        .ok_or_else(|| String::from("polygon coordinates must be an array"))?;
    let mut points = Vec::new();
    for ring in rings {
        let ring_points = ring
            .as_array()
            .ok_or_else(|| String::from("polygon ring must be an array"))?;
        for point in ring_points {
            let coords = point
                .as_array()
                .ok_or_else(|| String::from("polygon point must be an array"))?;
            if coords.len() < 2 {
                return Err(String::from("polygon point must have two coordinates"));
            }
            let lon = coords[0]
                .as_f64()
                .ok_or_else(|| String::from("longitude must be a number"))?;
            let lat = coords[1]
                .as_f64()
                .ok_or_else(|| String::from("latitude must be a number"))?;
            points.push((lon, lat));
        }
    }
    Ok(points)
}

fn points_from_multipolygon(coords: &Value) -> Result<Vec<(f64, f64)>, String> {
    let polygons = coords
        .as_array()
        .ok_or_else(|| String::from("multipolygon coordinates must be an array"))?;
    let mut points = Vec::new();
    for polygon in polygons {
        let polygon_points = points_from_polygon(polygon)?;
        points.extend(polygon_points);
    }
    Ok(points)
}

fn check_points(
    notices: &mut Vec<ValidationNotice>,
    feature_id: &str,
    feature_index: usize,
    points: &[(f64, f64)],
) {
    for (lon, lat) in points {
        if crate::validation_context::thorough_mode_enabled()
            && lat.abs() <= 1.0
            && lon.abs() <= 1.0
        {
            notices.push(point_near_origin_notice(
                feature_id,
                *lat,
                *lon,
                feature_index,
            ));
        }
        if lat.abs() >= 89.0 {
            notices.push(point_near_pole_notice(
                feature_id,
                *lat,
                *lon,
                feature_index,
            ));
        }
    }
}

/// Check if a polygon ring is self-intersecting by checking if any two non-adjacent edges cross
fn is_self_intersecting(ring: &[Value]) -> bool {
    let points: Vec<(f64, f64)> = ring
        .iter()
        .filter_map(|p| {
            let arr = p.as_array()?;
            if arr.len() < 2 {
                return None;
            }
            Some((arr[0].as_f64()?, arr[1].as_f64()?))
        })
        .collect();

    if points.len() < 4 {
        return false;
    }

    let n = points.len();
    for i in 0..n - 1 {
        for j in i + 2..n - 1 {
            // Skip adjacent edges
            if i == 0 && j == n - 2 {
                continue;
            }
            if segments_intersect(points[i], points[i + 1], points[j], points[j + 1]) {
                return true;
            }
        }
    }
    false
}

/// Check if two line segments (p1-p2) and (p3-p4) properly intersect
fn segments_intersect(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    let d1 = direction(p3, p4, p1);
    let d2 = direction(p3, p4, p2);
    let d3 = direction(p1, p2, p3);
    let d4 = direction(p1, p2, p4);

    // Check if segments properly cross
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }

    false
}

/// Compute the cross product of vectors (p3-p1) and (p2-p1)
fn direction(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    (p3.0 - p1.0) * (p2.1 - p1.1) - (p2.0 - p1.0) * (p3.1 - p1.1)
}

fn geojson_unknown_element_notice(unknown: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "geo_json_unknown_element",
        NoticeSeverity::Info,
        "unknown element in geojson file",
    );
    notice.insert_context_field("filename", LOCATIONS_GEOJSON_FILE);
    notice.insert_context_field("unknownElement", unknown);
    notice.field_order = vec!["filename".into(), "unknownElement".into()];
    notice
}

fn geojson_duplicated_element_notice(duplicated: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "geo_json_duplicated_element",
        NoticeSeverity::Error,
        "duplicated element in geojson file",
    );
    notice.insert_context_field("duplicatedElement", duplicated);
    notice.insert_context_field("filename", LOCATIONS_GEOJSON_FILE);
    notice.field_order = vec!["duplicatedElement".into(), "filename".into()];
    notice
}

fn unsupported_geojson_type_notice(value: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unsupported_geo_json_type",
        NoticeSeverity::Error,
        "unsupported GeoJSON type",
    );
    notice.insert_context_field("geoJsonType", value);
    notice.insert_context_field(
        "message",
        format!(
            "Unsupported GeoJSON type: {}. Use 'FeatureCollection' instead.",
            value
        ),
    );
    notice.field_order = vec!["geoJsonType".into(), "message".into()];
    notice
}

fn missing_required_element_notice(
    feature_id: Option<&str>,
    missing: &str,
    feature_index: Option<usize>,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "missing_required_element",
        NoticeSeverity::Error,
        "missing required element",
    );
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("featureId", feature_id);
    notice.insert_context_field("missingElement", missing);
    notice.field_order = vec![
        "featureId".into(),
        "featureIndex".into(),
        "missingElement".into(),
    ];
    notice
}

fn unsupported_feature_type_notice(
    feature_index: usize,
    feature_id: Option<&str>,
    feature_type: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unsupported_feature_type",
        NoticeSeverity::Error,
        "unsupported feature type",
    );
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("featureId", feature_id);
    notice.insert_context_field("featureType", feature_type);
    notice.field_order = vec![
        "featureId".into(),
        "featureIndex".into(),
        "featureType".into(),
    ];
    notice
}

fn unsupported_geometry_type_notice(
    feature_index: usize,
    feature_id: &str,
    geometry_type: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "unsupported_geometry_type",
        NoticeSeverity::Error,
        "unsupported geometry type",
    );
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("featureId", feature_id);
    notice.insert_context_field("geometryType", geometry_type);
    notice.field_order = vec![
        "featureId".into(),
        "featureIndex".into(),
        "geometryType".into(),
    ];
    notice
}

fn duplicate_geojson_key_notice(
    feature_id: &str,
    first_index: usize,
    second_index: usize,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "duplicate_geo_json_key",
        NoticeSeverity::Error,
        "duplicate geojson key",
    );
    notice.insert_context_field("featureId", feature_id);
    notice.insert_context_field("firstIndex", first_index);
    notice.insert_context_field("secondIndex", second_index);
    notice.field_order = vec![
        "featureId".into(),
        "firstIndex".into(),
        "secondIndex".into(),
    ];
    notice
}

fn invalid_geometry_notice(
    feature_id: &str,
    feature_index: usize,
    geometry_type: &str,
    message: String,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "invalid_geometry",
        NoticeSeverity::Error,
        "invalid geometry",
    );
    notice.insert_context_field("featureId", feature_id);
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("geometryType", geometry_type);
    notice.insert_context_field("message", message);
    notice.field_order = vec![
        "featureId".into(),
        "featureIndex".into(),
        "geometryType".into(),
        "message".into(),
    ];
    notice
}

fn point_near_origin_notice(
    feature_id: &str,
    lat: f64,
    lon: f64,
    feature_index: usize,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "point_near_origin",
        NoticeSeverity::Error,
        "point near origin",
    );
    notice.insert_context_field("filename", LOCATIONS_GEOJSON_FILE);
    notice.insert_context_field("csvRowNumber", Value::Null);
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("entityId", feature_id);
    notice.insert_context_field("latFieldName", Value::Null);
    notice.insert_context_field("latFieldValue", lat);
    notice.insert_context_field("lonFieldName", Value::Null);
    notice.insert_context_field("lonFieldValue", lon);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "entityId".into(),
        "featureIndex".into(),
        "filename".into(),
        "latFieldName".into(),
        "latFieldValue".into(),
        "lonFieldName".into(),
        "lonFieldValue".into(),
    ];
    notice
}

fn point_near_pole_notice(
    feature_id: &str,
    lat: f64,
    lon: f64,
    feature_index: usize,
) -> ValidationNotice {
    let mut notice =
        ValidationNotice::new("point_near_pole", NoticeSeverity::Error, "point near pole");
    notice.insert_context_field("filename", LOCATIONS_GEOJSON_FILE);
    notice.insert_context_field("csvRowNumber", Value::Null);
    notice.insert_context_field("featureIndex", feature_index);
    notice.insert_context_field("entityId", feature_id);
    notice.insert_context_field("latFieldName", Value::Null);
    notice.insert_context_field("latFieldValue", lat);
    notice.insert_context_field("lonFieldName", Value::Null);
    notice.insert_context_field("lonFieldValue", lon);
    notice.field_order = vec![
        "csvRowNumber".into(),
        "entityId".into(),
        "featureIndex".into(),
        "filename".into(),
        "latFieldName".into(),
        "latFieldValue".into(),
        "lonFieldName".into(),
        "lonFieldValue".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_location_ids_from_feature_collection() {
        let json = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "id": "L1",
                    "type": "Feature",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[[0,0],[2,0],[2,2],[0,2],[0,0]]]
                    },
                    "properties": {"location_id": "L2"}
                },
                {
                    "id": 42,
                    "type": "Feature",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[[3,3],[4,3],[4,4],[3,4],[3,3]]]
                    },
                    "properties": {}
                },
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Polygon",
                        "coordinates": [[[5,5],[6,5],[6,6],[5,6],[5,5]]]
                    },
                    "properties": {"location_id": "  "}
                }
            ]
        }"#;

        let collection: GeoJsonFeatureCollection = serde_json::from_str(json).expect("parse");
        let pool = crate::string_pool::StringPool::default();
        let locations = LocationsGeoJson::new(collection, &pool);

        assert!(locations.location_ids.contains(&pool.intern("L1")));
        assert!(locations.location_ids.contains(&pool.intern("42")));
        assert!(!locations.location_ids.contains(&pool.intern("L2")));
        assert_eq!(locations.location_ids.len(), 2);
        assert!(locations.bounds_by_id.contains_key(&pool.intern("42")));
    }
}
