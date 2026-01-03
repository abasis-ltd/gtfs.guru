
    #[test]
    fn detects_duplicate_ids() {
        let json = r#"{
          "type": "FeatureCollection",
          "features": [
            {
              "type": "Feature",
              "id": "location_zone1",
              "properties": { "stop_name": "Zone One" },
              "geometry": {
                "type": "Polygon",
                "coordinates": [[[-74.01, 40.71], [-74.00, 40.71], [-74.00, 40.72], [-74.01, 40.72], [-74.01, 40.71]]]
              }
            },
            {
              "type": "Feature",
              "id": "location_zone1",
              "properties": { "stop_name": "Duplicate Zone" },
              "geometry": {
                "type": "Polygon",
                "coordinates": [[[-74.02, 40.72], [-74.01, 40.72], [-74.01, 40.73], [-74.02, 40.73], [-74.02, 40.72]]]
              }
            }
          ]
        }"#;

        let collection: GeoJsonFeatureCollection = serde_json::from_str(json).expect("parse");
        let locations = LocationsGeoJson::from(collection);

        assert_eq!(locations.notices.len(), 1);
        assert_eq!(locations.notices[0].code, "duplicate_geo_json_key");
    }
