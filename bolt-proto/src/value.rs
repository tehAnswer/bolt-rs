use std::convert::{TryFrom, TryInto};
use std::hash::{Hash, Hasher};
use std::panic::catch_unwind;
use std::sync::{Arc, Mutex};

use bytes::{Buf, Bytes};

pub(crate) use boolean::Boolean;
pub(crate) use byte_array::ByteArray;
pub(crate) use date::Date;
pub(crate) use date_time_offset::DateTimeOffset;
pub(crate) use date_time_zoned::DateTimeZoned;
pub(crate) use float::Float;
pub(crate) use integer::Integer;
pub(crate) use list::List;
pub(crate) use map::Map;
pub use node::Node;
pub(crate) use null::Null;
pub use path::Path;
pub use relationship::Relationship;
pub(crate) use string::String;
pub(crate) use time::Time;
pub use unbound_relationship::UnboundRelationship;

use crate::error::*;
use crate::serialization::*;

pub(crate) mod boolean;
pub(crate) mod byte_array;
pub(crate) mod conversions;
pub(crate) mod date;
pub(crate) mod date_time_offset;
pub(crate) mod date_time_zoned;
pub(crate) mod float;
pub(crate) mod integer;
pub(crate) mod list;
pub(crate) mod map;
pub(crate) mod node;
pub(crate) mod null;
pub(crate) mod path;
pub(crate) mod relationship;
pub(crate) mod string;
pub(crate) mod time;
pub(crate) mod unbound_relationship;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // V1-compatible value types
    Boolean(bool),
    Integer(Integer),
    Float(f64),
    // Bytes was added with Neo4j 3.2, no mention of it in the Bolt v1 docs!
    Bytes(ByteArray),
    List(List),
    Map(Map),
    Null,
    String(std::string::String),
    Node(Node),
    Relationship(Relationship),
    Path(Path),
    UnboundRelationship(UnboundRelationship),
    // A date without a time zone, a.k.a. LocalDate
    Date(Date),
    // A time with a UTC offset, a.k.a. OffsetTime
    Time(Time),
    // TODO: Other V2-compatible value types + tests
    //// A date-time with a UTC offset, a.k.a. OffsetDateTime
    // DateTimeOffset(DateTimeOffset),
    //// A date-time with a time zone ID, a.k.a. ZonedDateTime
    // DateTimeZoned(DateTimeZoned),
    //// A time without a time zone
    // LocalTime,
    //// A date-time without a time zone
    // LocalDateTime,
    // Duration,
    // Point,
}

#[allow(clippy::derive_hash_xor_eq)]
// We implement Hash here despite deriving PartialEq because f64 and HashMap cannot be hashed and must panic
impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Float(_)
            | Value::Bytes(_)
            | Value::Map(_)
            | Value::Node(_)
            | Value::Relationship(_)
            | Value::UnboundRelationship(_)
            | Value::Path(_) => panic!("Cannot hash a {:?}", self),
            Value::Boolean(boolean) => boolean.hash(state),
            Value::Integer(integer) => integer.hash(state),
            Value::List(list) => list.hash(state),
            Value::Null => Null.hash(state),
            Value::String(string) => string.hash(state),
            Value::Date(date) => date.hash(state),
            Value::Time(time) => time.hash(state),
        }
    }
}

impl Eq for Value {
    fn assert_receiver_is_total_eq(&self) {
        if let Value::Float(_) = self {
            panic!("Floats do not impl Eq")
        }
    }
}

impl Marker for Value {
    fn get_marker(&self) -> Result<u8> {
        match self {
            Value::Boolean(boolean) => Boolean::from(*boolean).get_marker(),
            Value::Integer(integer) => integer.get_marker(),
            Value::Float(float) => Float::from(*float).get_marker(),
            Value::Bytes(byte_array) => byte_array.get_marker(),
            Value::List(list) => list.get_marker(),
            Value::Map(map) => map.get_marker(),
            Value::Null => Null.get_marker(),
            // TODO: This could incur a large runtime cost since we copy the whole thing
            Value::String(string) => String::from(string.as_str()).get_marker(),
            Value::Node(node) => node.get_marker(),
            Value::Relationship(rel) => rel.get_marker(),
            Value::Path(path) => path.get_marker(),
            Value::UnboundRelationship(unbound_rel) => unbound_rel.get_marker(),
            Value::Date(date) => date.get_marker(),
            Value::Time(time) => time.get_marker(),
        }
    }
}

impl Serialize for Value {}

impl TryInto<Bytes> for Value {
    type Error = Error;

    fn try_into(self) -> Result<Bytes> {
        match self {
            Value::Boolean(boolean) => Boolean::from(boolean).try_into(),
            Value::Integer(integer) => integer.try_into(),
            Value::Float(float) => Float::from(float).try_into(),
            Value::Bytes(byte_array) => byte_array.try_into(),
            Value::List(list) => list.try_into(),
            Value::Map(map) => map.try_into(),
            Value::Null => Null.try_into(),
            Value::String(string) => String::from(string).try_into(),
            Value::Node(node) => node.try_into(),
            Value::Relationship(rel) => rel.try_into(),
            Value::Path(path) => path.try_into(),
            Value::UnboundRelationship(unbound_rel) => unbound_rel.try_into(),
            Value::Date(date) => date.try_into(),
            Value::Time(time) => time.try_into(),
        }
    }
}

impl Deserialize for Value {}

impl TryFrom<Arc<Mutex<Bytes>>> for Value {
    type Error = Error;

    fn try_from(input_arc: Arc<Mutex<Bytes>>) -> Result<Self> {
        let result: Result<Value> = catch_unwind(move || {
            let marker = input_arc.lock().unwrap().clone().get_u8();

            match marker {
                null::MARKER => {
                    input_arc.lock().unwrap().advance(1);
                    Ok(Value::Null)
                }
                boolean::MARKER_FALSE => {
                    input_arc.lock().unwrap().advance(1);
                    Ok(Value::Boolean(false))
                }
                boolean::MARKER_TRUE => {
                    input_arc.lock().unwrap().advance(1);
                    Ok(Value::Boolean(true))
                }
                // Tiny int
                marker if (-16..=127).contains(&(marker as i8)) => {
                    input_arc.lock().unwrap().advance(1);
                    Ok(Value::Integer(Integer::from(marker as i8)))
                }
                // Other int types
                integer::MARKER_INT_8
                | integer::MARKER_INT_16
                | integer::MARKER_INT_32
                | integer::MARKER_INT_64 => Ok(Value::Integer(Integer::try_from(input_arc)?)),
                float::MARKER => Ok(Value::Float(Float::try_from(input_arc)?.value)),
                byte_array::MARKER_SMALL | byte_array::MARKER_MEDIUM | byte_array::MARKER_LARGE => {
                    Ok(Value::Bytes(ByteArray::try_from(input_arc)?))
                }
                // Tiny string
                marker
                    if (string::MARKER_TINY..=(string::MARKER_TINY | 0x0F)).contains(&marker) =>
                {
                    Ok(Value::String(String::try_from(input_arc)?.value))
                }
                string::MARKER_SMALL | string::MARKER_MEDIUM | string::MARKER_LARGE => {
                    Ok(Value::String(String::try_from(input_arc)?.value))
                }
                // Tiny list
                marker if (list::MARKER_TINY..=(list::MARKER_TINY | 0x0F)).contains(&marker) => {
                    Ok(Value::List(List::try_from(input_arc)?))
                }
                list::MARKER_SMALL | list::MARKER_MEDIUM | list::MARKER_LARGE => {
                    Ok(Value::List(List::try_from(input_arc)?))
                }
                // Tiny map
                marker if (map::MARKER_TINY..=(map::MARKER_TINY | 0x0F)).contains(&marker) => {
                    Ok(Value::Map(Map::try_from(input_arc)?))
                }
                map::MARKER_SMALL | map::MARKER_MEDIUM | map::MARKER_LARGE => {
                    Ok(Value::Map(Map::try_from(input_arc)?))
                }
                // Tiny structure
                marker if (STRUCT_MARKER_TINY..=(STRUCT_MARKER_TINY | 0x0F)).contains(&marker) => {
                    deserialize_structure(input_arc)
                }
                STRUCT_MARKER_SMALL | STRUCT_MARKER_MEDIUM => deserialize_structure(input_arc),
                _ => Err(DeserializeError(format!("Invalid marker byte: {:x}", marker)).into()),
            }
        })
        .map_err(|_| DeserializeError("Panicked during deserialization".to_string()))?;

        Ok(result.map_err(|err: Error| {
            DeserializeError(format!("Error creating Value from Bytes: {}", err))
        })?)
    }
}

// Might panic. Use this inside a catch_unwind block
fn deserialize_structure(input_arc: Arc<Mutex<Bytes>>) -> Result<Value> {
    let signature = get_signature_from_bytes(&mut *input_arc.lock().unwrap())?;
    match signature {
        node::SIGNATURE => Ok(Value::Node(Node::try_from(input_arc)?)),
        relationship::SIGNATURE => Ok(Value::Relationship(Relationship::try_from(input_arc)?)),
        path::SIGNATURE => Ok(Value::Path(Path::try_from(input_arc)?)),
        unbound_relationship::SIGNATURE => Ok(Value::UnboundRelationship(
            UnboundRelationship::try_from(input_arc)?,
        )),
        date::SIGNATURE => Ok(Value::Date(Date::try_from(input_arc)?.into())),
        time::SIGNATURE => Ok(Value::Time(Time::try_from(input_arc)?.into())),
        _ => Err(DeserializeError(format!("Invalid signature byte: {:x}", signature)).into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::iter::FromIterator;

    use super::*;

    #[test]
    fn null_from_bytes() {
        let null = Null;
        let null_bytes = null.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(null_bytes))).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn boolean_from_bytes() {
        let t = Boolean::from(true);
        let true_bytes = t.clone().try_into_bytes().unwrap();
        let f = Boolean::from(false);
        let false_bytes = f.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(true_bytes))).unwrap(),
            Value::Boolean(true)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(false_bytes))).unwrap(),
            Value::Boolean(false)
        );
    }

    #[test]
    fn integer_from_bytes() {
        let tiny = Integer::from(110_i8);
        let tiny_bytes = tiny.clone().try_into_bytes().unwrap();
        let small = Integer::from(-50_i8);
        let small_bytes = small.clone().try_into_bytes().unwrap();
        let medium = Integer::from(8000_i16);
        let medium_bytes = medium.clone().try_into_bytes().unwrap();
        let medium_negative = Integer::from(-18621_i16);
        let medium_negative_bytes = medium_negative.clone().try_into_bytes().unwrap();
        let large = Integer::from(-1_000_000_000_i32);
        let large_bytes = large.clone().try_into_bytes().unwrap();
        let very_large = Integer::from(9_000_000_000_000_000_000_i64);
        let very_large_bytes = very_large.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(tiny_bytes))).unwrap(),
            Value::Integer(tiny)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(small_bytes))).unwrap(),
            Value::Integer(small)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(medium_bytes))).unwrap(),
            Value::Integer(medium)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(medium_negative_bytes))).unwrap(),
            Value::Integer(medium_negative)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(large_bytes))).unwrap(),
            Value::Integer(large)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(very_large_bytes))).unwrap(),
            Value::Integer(very_large)
        );
    }

    #[test]
    fn float_from_bytes() {
        let min = Float::from(std::f64::MIN_POSITIVE);
        let min_bytes = min.clone().try_into_bytes().unwrap();
        let max = Float::from(std::f64::MAX);
        let max_bytes = max.clone().try_into_bytes().unwrap();
        let e = Float::from(std::f64::consts::E);
        let e_bytes = e.clone().try_into_bytes().unwrap();
        let pi = Float::from(std::f64::consts::PI);
        let pi_bytes = pi.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(min_bytes))).unwrap(),
            Value::Float(min.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(max_bytes))).unwrap(),
            Value::Float(max.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(e_bytes))).unwrap(),
            Value::Float(e.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(pi_bytes))).unwrap(),
            Value::Float(pi.value)
        );
    }

    #[test]
    fn byte_array_from_bytes() {
        let empty_arr: ByteArray = Vec::<u8>::new().into();
        let empty_arr_bytes = empty_arr.clone().try_into_bytes().unwrap();
        let small_arr: ByteArray = vec![1_u8; 100].into();
        let small_arr_bytes = small_arr.clone().try_into_bytes().unwrap();
        let medium_arr: ByteArray = vec![99_u8; 1000].into();
        let medium_arr_bytes = medium_arr.clone().try_into_bytes().unwrap();
        let large_arr: ByteArray = vec![1_u8; 100_000].into();
        let large_arr_bytes = large_arr.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(empty_arr_bytes))).unwrap(),
            Value::Bytes(empty_arr)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(small_arr_bytes))).unwrap(),
            Value::Bytes(small_arr)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(medium_arr_bytes))).unwrap(),
            Value::Bytes(medium_arr)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(large_arr_bytes))).unwrap(),
            Value::Bytes(large_arr)
        );
    }

    #[test]
    fn string_from_bytes() {
        let tiny = String::from("string".repeat(1));
        let tiny_bytes = tiny.clone().try_into_bytes().unwrap();
        let small = String::from("string".repeat(10));
        let small_bytes = small.clone().try_into_bytes().unwrap();
        let medium = String::from("string".repeat(1000));
        let medium_bytes = medium.clone().try_into_bytes().unwrap();
        let large = String::from("string".repeat(100_000));
        let large_bytes = large.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(tiny_bytes))).unwrap(),
            Value::String(tiny.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(small_bytes))).unwrap(),
            Value::String(small.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(medium_bytes))).unwrap(),
            Value::String(medium.value)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(large_bytes))).unwrap(),
            Value::String(large.value)
        );
    }

    #[test]
    fn list_from_bytes() {
        let empty_list: List = Vec::<i32>::new().into();
        let empty_list_bytes = empty_list.clone().try_into_bytes().unwrap();
        let tiny_list: List = vec![100_000_000_000_i64; 10].into();
        let tiny_list_bytes = tiny_list.clone().try_into_bytes().unwrap();
        let small_list: List = vec!["item"; 100].into();
        let small_list_bytes = small_list.clone().try_into_bytes().unwrap();
        let medium_list: List = vec![false; 1000].into();
        let medium_list_bytes = medium_list.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(empty_list_bytes))).unwrap(),
            Value::List(empty_list)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(tiny_list_bytes))).unwrap(),
            Value::List(tiny_list)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(small_list_bytes))).unwrap(),
            Value::List(small_list)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(medium_list_bytes))).unwrap(),
            Value::List(medium_list)
        );
    }

    #[test]
    #[ignore]
    fn large_list_from_bytes() {
        let large_list: List = vec![1_i8; 70_000].into();
        let large_list_bytes = large_list.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(large_list_bytes))).unwrap(),
            Value::List(large_list)
        );
    }

    #[test]
    fn map_from_bytes() {
        let empty_map: Map = HashMap::<&str, i8>::new().into();
        let empty_map_bytes = empty_map.clone().try_into_bytes().unwrap();
        let tiny_map: Map = HashMap::<&str, i8>::from_iter(vec![("a", 1_i8)]).into();
        let tiny_map_bytes = tiny_map.clone().try_into_bytes().unwrap();
        let small_map: Map = HashMap::<&str, i8>::from_iter(vec![
            ("a", 1_i8),
            ("b", 1_i8),
            ("c", 3_i8),
            ("d", 4_i8),
            ("e", 5_i8),
            ("f", 6_i8),
            ("g", 7_i8),
            ("h", 8_i8),
            ("i", 9_i8),
            ("j", 0_i8),
            ("k", 1_i8),
            ("l", 2_i8),
            ("m", 3_i8),
            ("n", 4_i8),
            ("o", 5_i8),
            ("p", 6_i8),
        ])
        .into();
        let small_map_bytes = small_map.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(empty_map_bytes))).unwrap(),
            Value::Map(empty_map)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(tiny_map_bytes))).unwrap(),
            Value::Map(tiny_map)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(small_map_bytes))).unwrap(),
            Value::Map(small_map)
        );
    }

    fn get_node() -> Node {
        Node::new(
            24_i64,
            vec!["TestNode".to_string()],
            HashMap::from_iter(vec![
                ("key1".to_string(), -1_i8),
                ("key2".to_string(), 1_i8),
            ]),
        )
    }

    fn get_rel() -> Relationship {
        Relationship::new(
            24_i64,
            32_i64,
            128_i64,
            "TestRel".to_string(),
            HashMap::from_iter(vec![
                ("key1".to_string(), -2_i8),
                ("key2".to_string(), 2_i8),
            ]),
        )
    }

    fn get_unbound_rel() -> UnboundRelationship {
        UnboundRelationship::new(
            128_i64,
            "TestRel".to_string(),
            HashMap::from_iter(vec![
                ("key1".to_string(), -2_i8),
                ("key2".to_string(), 2_i8),
            ]),
        )
    }

    #[test]
    fn node_from_bytes() {
        let node_bytes: Bytes = get_node().try_into_bytes().unwrap();

        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(node_bytes))).unwrap(),
            Value::Node(get_node())
        );
    }

    #[test]
    fn relationship_from_bytes() {
        let rel_bytes: Bytes = get_rel().try_into_bytes().unwrap();

        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(rel_bytes))).unwrap(),
            Value::Relationship(get_rel())
        );
    }

    #[test]
    fn path_from_bytes() {
        let path = Path::new(vec![get_node()], vec![get_unbound_rel()], 100_i64);
        let path_bytes: Bytes = path.clone().try_into_bytes().unwrap();

        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(path_bytes))).unwrap(),
            Value::Path(path)
        );
    }

    #[test]
    fn unbound_relationship_from_bytes() {
        let unbound_rel_bytes: Bytes = get_unbound_rel().try_into_bytes().unwrap();

        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(unbound_rel_bytes))).unwrap(),
            Value::UnboundRelationship(get_unbound_rel())
        );
    }

    #[test]
    fn date_from_bytes() {
        let christmas = Date::new(2020, 12, 25).unwrap();
        let christmas_bytes: Bytes = christmas.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(christmas_bytes))).unwrap(),
            Value::Date(christmas)
        );
    }

    #[test]
    fn time_from_bytes() {
        let midnight_utc = Time::new(0, 0, 0, 0, (0, 0)).unwrap();
        let midnight_utc_bytes = midnight_utc.clone().try_into_bytes().unwrap();
        let about_four_pm_pacific = Time::new(16, 4, 35, 235, (-8, 0)).unwrap();
        let about_four_pm_pacific_bytes = about_four_pm_pacific.clone().try_into_bytes().unwrap();
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(midnight_utc_bytes))).unwrap(),
            Value::Time(midnight_utc)
        );
        assert_eq!(
            Value::try_from(Arc::new(Mutex::new(about_four_pm_pacific_bytes))).unwrap(),
            Value::Time(about_four_pm_pacific)
        );
    }
}
