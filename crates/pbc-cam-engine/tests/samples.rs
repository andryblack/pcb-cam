//! End-to-end smoke test for the bundled demo project.

use pbc_cam_engine::Session;
use serde_json::json;

#[test]
fn demo_project_imports_exports_and_undoes() {
    let mut session = Session::new();
    session
        .execute("project.new", &json!({"name": "Demo Board"}))
        .unwrap();

    let gerber = include_str!("../../../assets/samples/demo-F_Cu.gbr");
    let outline = include_str!("../../../assets/samples/demo-Edge_Cuts.gbr");
    let drill = include_str!("../../../assets/samples/demo.drl");

    session
        .execute(
            "file.import_gerber",
            &json!({"content": gerber, "name": "Top Copper"}),
        )
        .unwrap();
    session
        .execute(
            "file.import_gerber",
            &json!({"content": outline, "name": "Outline"}),
        )
        .unwrap();
    let imported = session
        .execute(
            "file.import_excellon",
            &json!({"content": drill, "name": "Drills"}),
        )
        .unwrap();
    let layer = imported["layer"].as_u64().unwrap();

    assert_eq!(session.project().layers().len(), 3);
    assert_eq!(session.project().tools().len(), 3);
    assert_eq!(session.project().object_count(), 30);

    let operation = session
        .execute("operation.add_drill", &json!({"layer": layer}))
        .unwrap();
    assert_eq!(operation["name"], "Drill Drills");
    assert_eq!(session.project().enabled_drill_count(), 1);

    let path = std::env::temp_dir().join(format!("pbc-cam-demo-{}.nc", std::process::id()));
    let exported = session
        .execute("file.export_gcode", &json!({"path": path}))
        .unwrap();
    assert_eq!(exported["holes"], 9);
    assert_eq!(exported["tools"], 3);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("G21"));
    assert!(text.contains("T3 M6"));
    assert!(text.contains("M30"));
    let _ = std::fs::remove_file(path);

    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().operations().len(), 0);
    assert_eq!(session.project().layers().len(), 3);
    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().layers().len(), 2);
}
