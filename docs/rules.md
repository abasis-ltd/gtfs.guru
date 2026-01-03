# Validation Rules

The validator implements **88 validation rules** covering:

## File Structure
- Required files (agency.txt, stops.txt, routes.txt, trips.txt, stop_times.txt)
- Recommended files (feed_info.txt, shapes.txt)
- File encoding (UTF-8)
- CSV parsing

## Data Integrity
- Primary key uniqueness
- Foreign key references
- Required fields
- Data types and formats

## Geographic Validation
- Coordinate ranges (latitude/longitude)
- Stop-to-shape distance
- Travel speed between stops
- Shape geometry

## Schedule Validation
- Stop time sequences
- Arrival/departure times
- Calendar validity
- Service coverage
- Overlapping frequencies

## Accessibility & Quality
- Route color contrast
- Stop naming
- Pathway connectivity
- Fare system consistency
