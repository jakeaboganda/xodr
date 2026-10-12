//! The scene `bake` writes has every key the page reads, in the shape it
//! reads it, whatever the map holds.

use serde_json::Value;

/// The scene of `map`, a file in `tests/data`, as the page reads it.
fn scene(map: &str) -> Value {
    let xodr = std::fs::read_to_string(format!("../tests/data/{map}.xodr")).unwrap();
    let none = |_: &str| Err("no file here".to_string());
    let scene = xodr_viewer::bake(&xodr, none, |_| Err("no CRG here".into())).unwrap();
    let mut out = Vec::new();
    scene.write(&mut out).unwrap();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn a_map_with_no_objects_still_has_the_object_mesh_and_its_spans() {
    for map in ["demo", "objects"] {
        let scene = scene(map);
        for mesh in ["mesh", "objectMesh"] {
            for key in ["positions", "normals", "indices", "spans"] {
                assert!(scene[mesh][key].is_array(), "{map}: {mesh}.{key}");
            }
        }
        assert!(
            scene["lanes"].is_array() && scene["objects"].is_array(),
            "{map}"
        );
    }
}
