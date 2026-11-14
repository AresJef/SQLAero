//! Coordinates preserve full names but bound only their diagnostic display.

use sqlaero_transcode::diagnostic::{Coordinate, Indices, Path};

#[test]
fn field_and_positional_coordinates_are_shared_without_allocating_a_tree() {
    let root = Path::root("data");
    let orders = root.child(Coordinate::Field {
        name: "orders",
        ordinal: 7,
    });
    let row = orders.child(Coordinate::Index(0));
    let amount = row.child(Coordinate::Field {
        name: "amount",
        ordinal: 2,
    });
    assert_eq!(amount.to_string(), "data.orders[0].amount");
    assert_eq!(Indices::new("args", &[1, 2]).to_string(), "args[1][2]");
    assert_eq!(root.child(Coordinate::Key(3)).to_string(), "data[key#3]");
}

#[test]
fn unusual_and_long_names_are_unambiguous_without_losing_suffix_policy() {
    let root = Path::root("data");
    assert_eq!(
        root.child(Coordinate::Field {
            name: "a\0x",
            ordinal: 0
        })
        .to_string(),
        "data[\"a\\0x\"]"
    );
    assert_eq!(
        root.child(Coordinate::Field {
            name: "a\0y",
            ordinal: 1
        })
        .to_string(),
        "data[\"a\\0y\"]"
    );
    let name = "😀".repeat(10000);
    let rendered = root
        .child(Coordinate::Field {
            name: &name,
            ordinal: 9,
        })
        .to_string();
    assert!(rendered.contains("…; entry#9"));
    assert!(rendered.len() < 100);
    assert_eq!(name.len(), 40000);
}
