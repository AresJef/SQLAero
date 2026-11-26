//! MySQL spatial calls and community vector conversion functions.

use super::aggregate::distinct_calls;
use super::calls::{fixed_calls, optional_calls, variadic_calls};
use super::{Kind, SqlFunction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::{Form, Part, Role};

fixed_calls! {
    Point, "Point", (x, y), "Construct a point from its x and y coordinates.";
    MbrContains, "MBRContains", (outer, inner), "Test whether the outer bounding rectangle contains the inner rectangle.";
    MbrCoveredBy, "MBRCoveredBy", (inner, outer), "Test whether the inner bounding rectangle is covered by the outer rectangle.";
    MbrCovers, "MBRCovers", (outer, inner), "Test whether the outer bounding rectangle covers the inner rectangle.";
    MbrDisjoint, "MBRDisjoint", (left, right), "Test whether the bounding rectangles are disjoint.";
    MbrEquals, "MBREquals", (left, right), "Test whether the bounding rectangles are equal.";
    MbrIntersects, "MBRIntersects", (left, right), "Test whether the bounding rectangles intersect.";
    MbrOverlaps, "MBROverlaps", (left, right), "Test whether the bounding rectangles overlap.";
    MbrTouches, "MBRTouches", (left, right), "Test whether the bounding rectangles touch.";
    MbrWithin, "MBRWithin", (inner, outer), "Test whether the inner bounding rectangle is within the outer rectangle.";
    StArea, "ST_Area", (geometry), "Return the area of the polygon or multipolygon.";
    StCentroid, "ST_Centroid", (geometry), "Return the centroid of the geometry.";
    StContains, "ST_Contains", (outer, inner), "Test whether the first geometry contains the second.";
    StConvexHull, "ST_ConvexHull", (geometry), "Return the convex hull of the geometry.";
    StCrosses, "ST_Crosses", (left, right), "Test whether the geometries cross.";
    StDifference, "ST_Difference", (left, right), "Return the points of the first geometry outside the second.";
    StDimension, "ST_Dimension", (geometry), "Return the topological dimension of the geometry.";
    StDisjoint, "ST_Disjoint", (left, right), "Test whether the geometries are disjoint.";
    StEndPoint, "ST_EndPoint", (line), "Return the endpoint of the line.";
    StEnvelope, "ST_Envelope", (geometry), "Return the minimum bounding geometry.";
    StEquals, "ST_Equals", (left, right), "Test whether the geometries represent the same set of points.";
    StExteriorRing, "ST_ExteriorRing", (polygon), "Return the exterior ring of the polygon.";
    StGeometryN, "ST_GeometryN", (collection, position), "Return the geometry at the one-based collection position.";
    StGeometryType, "ST_GeometryType", (geometry), "Return the name of the geometry type.";
    StInteriorRingN, "ST_InteriorRingN", (polygon, position), "Return the interior ring at the one-based position.";
    StIntersection, "ST_Intersection", (left, right), "Return the intersection of two geometries.";
    StIntersects, "ST_Intersects", (left, right), "Test whether the geometries intersect.";
    StIsClosed, "ST_IsClosed", (geometry), "Test whether the line or multiline is closed.";
    StIsEmpty, "ST_IsEmpty", (geometry), "Test whether the geometry is empty.";
    StIsSimple, "ST_IsSimple", (geometry), "Test whether the geometry is simple.";
    StIsValid, "ST_IsValid", (geometry), "Test whether the geometry is valid.";
    StLatFromGeoHash, "ST_LatFromGeoHash", (geohash), "Decode the latitude from a geohash.";
    StLineInterpolatePoint, "ST_LineInterpolatePoint", (line, fraction), "Return the point at a fraction of the line length.";
    StLineInterpolatePoints, "ST_LineInterpolatePoints", (line, fraction), "Return the points at successive fractions of the line length.";
    StLongFromGeoHash, "ST_LongFromGeoHash", (geohash), "Decode the longitude from a geohash.";
    StMakeEnvelope, "ST_MakeEnvelope", (first_point, second_point), "Return the envelope defined by two points.";
    StNumGeometries, "ST_NumGeometries", (collection), "Return the number of geometries in the collection.";
    StNumInteriorRings, "ST_NumInteriorRings", (polygon), "Return the number of interior rings in the polygon.";
    StNumPoints, "ST_NumPoints", (line), "Return the number of points in the line.";
    StOverlaps, "ST_Overlaps", (left, right), "Test whether the two shapes overlap.";
    StPointAtDistance, "ST_PointAtDistance", (line, distance), "Return the point at a distance along the line.";
    StPointFromGeoHash, "ST_PointFromGeoHash", (geohash, srid), "Decode a geohash into a point in the selected reference system.";
    StPointN, "ST_PointN", (line, position), "Return the point at the one-based position in the line.";
    StSimplify, "ST_Simplify", (geometry, maximum_distance), "Simplify the geometry within the supplied distance.";
    StStartPoint, "ST_StartPoint", (line), "Return the start point of the line.";
    StSwapXy, "ST_SwapXY", (geometry), "Return a copy with the x and y coordinates exchanged.";
    StSymDifference, "ST_SymDifference", (left, right), "Return the points belonging to exactly one of the two geometries.";
    StTouches, "ST_Touches", (left, right), "Test whether the two geometries touch.";
    StTransform, "ST_Transform", (geometry, target_srid), "Transform the geometry into the target reference system.";
    StUnion, "ST_Union", (left, right), "Return the union of the two geometries.";
    StValidate, "ST_Validate", (geometry), "Return a valid geometry, or NULL if the geometry is invalid.";
    StWithin, "ST_Within", (inner, outer), "Test whether the first geometry lies within the second.";
    StringToVector, "STRING_TO_VECTOR", (value), "Convert a bracketed list of numbers to a MySQL vector.";
    VectorDim, "VECTOR_DIM", (vector), "Return the number of entries in a MySQL vector.";
    VectorToString, "VECTOR_TO_STRING", (vector), "Return the textual representation of a MySQL vector.";
}

optional_calls! {
    StAsBinary, "ST_AsBinary", (geometry), (options), "(geometry, options=...)",
        "Convert a geometry to WKB, optionally specifying axis order.";
    StAsGeoJson, "ST_AsGeoJSON", (geometry), (maximum_decimal_digits, options), "(geometry, maximum_decimal_digits=..., options=...)",
        "Convert a geometry to GeoJSON with optional precision and formatting flags.";
    StAsText, "ST_AsText", (geometry), (options), "(geometry, options=...)",
        "Convert a geometry to WKT, optionally specifying axis order.";
    StBuffer, "ST_Buffer", (geometry, distance), (strategy1, strategy2, strategy3), "(geometry, distance, strategy1=..., strategy2=..., strategy3=...)",
        "Return the areas within the distance using up to three buffer strategies.";
    StBufferStrategy, "ST_Buffer_Strategy", (strategy), (points_per_circle), "(strategy, points_per_circle=...)",
        "Construct a binary strategy for ST_Buffer.";
    StDistance, "ST_Distance", (left, right), (unit), "(left, right, unit=...)",
        "Return the shortest distance between geometries in the optional unit.";
    StDistanceSphere, "ST_Distance_Sphere", (left, right), (radius), "(left, right, radius=...)",
        "Return spherical distance in metres, optionally specifying a radius.";
    StFrechetDistance, "ST_FrechetDistance", (left, right), (unit), "(left, right, unit=...)",
        "Return the discrete Frechet distance between line geometries.";
    StGeoHash, "ST_GeoHash", (longitude_or_point, latitude_or_maximum_length), (maximum_length), "(longitude_or_point, latitude_or_maximum_length, maximum_length=...)",
        "Encode a point or longitude and latitude as a geohash of the given maximum length.";
    StGeomCollFromText, "ST_GeomCollFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a geometry collection from WKT.";
    StGeomCollFromWkb, "ST_GeomCollFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a geometry collection from WKB.";
    StGeomFromGeoJson, "ST_GeomFromGeoJSON", (document), (options, srid), "(document, options=..., srid=...)",
        "Convert a GeoJSON document into a geometry.";
    StGeomFromText, "ST_GeomFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a geometry from WKT.";
    StGeomFromWkb, "ST_GeomFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a geometry from WKB.";
    StHausdorffDistance, "ST_HausdorffDistance", (left, right), (unit), "(left, right, unit=...)",
        "Return the discrete Hausdorff distance between geometries.";
    StLatitude, "ST_Latitude", (point), (latitude), "(point, latitude=...)",
        "Get a geographic latitude or return a point with the given latitude.";
    StLength, "ST_Length", (geometry), (unit), "(geometry, unit=...)",
        "Return the length of a line or multiline geometry.";
    StLineFromText, "ST_LineFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a line string from WKT.";
    StLineFromWkb, "ST_LineFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a line string from WKB.";
    StLongitude, "ST_Longitude", (point), (longitude), "(point, longitude=...)",
        "Get a geographic longitude or return a point with the given longitude.";
    StMLineFromText, "ST_MLineFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a multiline string from WKT.";
    StMLineFromWkb, "ST_MLineFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a multiline string from WKB.";
    StMPointFromText, "ST_MPointFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a multipoint from WKT.";
    StMPointFromWkb, "ST_MPointFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a multipoint from WKB.";
    StMPolyFromText, "ST_MPolyFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a multipolygon from WKT.";
    StMPolyFromWkb, "ST_MPolyFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a multipolygon from WKB.";
    StPointFromText, "ST_PointFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a point from WKT.";
    StPointFromWkb, "ST_PointFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a point from WKB.";
    StPolyFromText, "ST_PolyFromText", (wkt), (srid, options), "(wkt, srid=..., options=...)",
        "Construct a polygon from WKT.";
    StPolyFromWkb, "ST_PolyFromWKB", (wkb), (srid, options), "(wkb, srid=..., options=...)",
        "Construct a polygon from WKB.";
    StSrid, "ST_SRID", (geometry), (srid), "(geometry, srid=...)",
        "Get the spatial reference ID or set it without transforming the coordinates.";
    StX, "ST_X", (point), (x), "(point, x=...)",
        "Get the x coordinate or return a point with the given x coordinate.";
    StY, "ST_Y", (point), (y), "(point, y=...)",
        "Get the y coordinate or return a point with the given y coordinate.";
}

variadic_calls! {
    LineString, "LineString", (point), points, "Construct a line string from points or point WKB.";
    MultiLineString, "MultiLineString", (line), lines, "Construct a multiline string from lines or line WKB.";
    MultiPoint, "MultiPoint", (point), points, "Construct a multipoint from points or point WKB.";
    MultiPolygon, "MultiPolygon", (polygon), polygons, "Construct a multipolygon from polygons or polygon WKB.";
    Polygon, "Polygon", (ring), rings, "Construct a polygon from its exterior ring and any interior rings.";
}

distinct_calls! {
    StCollect, "ST_Collect", "Collect geometries in a group or window, optionally removing duplicates.";
}

/// Construct a geometry collection, optionally with zero components.
#[pyclass(extends = SqlFunction, name = "GeometryCollection", module = "sqlaero.sqlfunc")]
struct GeometryCollection;

#[pymethods]
impl GeometryCollection {
    #[new]
    #[pyo3(signature = (*geometries))]
    fn new(geometries: &Bound<'_, PyTuple>) -> PyResult<PyClassInitializer<Self>> {
        let expression = SqlFunction::new(
            "GeometryCollection",
            Kind::Scalar,
            Form::Call("GeometryCollection("),
            geometries.clone(),
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    register_variadic(module)?;
    register_distinct(module)?;
    module.add_class::<GeometryCollection>()
}
