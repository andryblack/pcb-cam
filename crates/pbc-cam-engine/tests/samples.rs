//! End-to-end smoke test for the bundled demo project.

use pbc_cam_engine::Session;
use serde_json::json;

#[test]
fn demo_project_imports_builds_tasks_and_exports_combined_gcode() {
    let mut session = Session::new();
    session
        .execute("project.new", &json!({"name": "Demo Board"}))
        .unwrap();

    let copper = include_str!("../../../assets/samples/demo-F_Cu.gbr");
    let outline = include_str!("../../../assets/samples/demo-Edge_Cuts.gbr");
    let silk = include_str!("../../../assets/samples/demo-Silkscreen.gbr");
    let drill = include_str!("../../../assets/samples/demo.drl");

    let copper = session
        .execute(
            "file.import_gerber",
            &json!({"content": copper, "name": "Top Copper"}),
        )
        .unwrap();
    let outline = session
        .execute(
            "file.import_gerber",
            &json!({"content": outline, "name": "Outline"}),
        )
        .unwrap();
    let silk = session
        .execute(
            "file.import_gerber",
            &json!({"content": silk, "name": "Silkscreen"}),
        )
        .unwrap();
    let drill = session
        .execute(
            "file.import_excellon",
            &json!({"content": drill, "name": "Drills"}),
        )
        .unwrap();

    assert_eq!(session.project().layers().len(), 4);
    assert_eq!(session.project().tools().len(), 3);
    assert!(session.project().object_count() > 30);

    let drill_layer = drill["layer"].as_u64().unwrap();
    let outline_layer = outline["layer"].as_u64().unwrap();
    let silk_layer = silk["layer"].as_u64().unwrap();
    let copper_layer = copper["layer"].as_u64().unwrap();

    for (command, params) in [
        ("task.add_drill", json!({"layer": drill_layer})),
        ("task.add_cutout", json!({"layer": outline_layer})),
        ("task.add_engrave", json!({"layer": silk_layer})),
    ] {
        session.execute(command, &params).unwrap();
    }
    assert_eq!(session.project().tasks().len(), 3);
    assert_eq!(session.project().enabled_task_count(), 3);

    // A single task produces a standalone program.
    let cutout_task = session.project().tasks()[1].id();
    let single_path =
        std::env::temp_dir().join(format!("pbc-cam-demo-cutout-{}.nc", std::process::id()));
    let single = session
        .execute(
            "file.export_task_gcode",
            &json!({"path": single_path, "task": cutout_task.value()}),
        )
        .unwrap();
    assert_eq!(single["tasks"], 1);
    assert_eq!(single["paths"], 1);
    let single_text = std::fs::read_to_string(&single_path).unwrap();
    assert!(single_text.contains("Contour cut"));
    assert!(single_text.contains("M6"));
    let _ = std::fs::remove_file(single_path);

    // The combined program contains all tasks and tool changes between them.
    let all_path = std::env::temp_dir().join(format!("pbc-cam-demo-all-{}.nc", std::process::id()));
    let combined = session
        .execute("file.export_gcode", &json!({"path": all_path}))
        .unwrap();
    assert_eq!(combined["tasks"], 3);
    assert_eq!(combined["holes"], 9);
    assert_eq!(combined["tools"], 4);
    let text = std::fs::read_to_string(&all_path).unwrap();
    assert!(text.contains("G21"));
    assert!(text.contains("T3 M6"));
    assert!(text.contains("(Contour cut"));
    assert!(text.contains("(Engrave"));
    assert!(text.contains("M30"));
    assert!(text.matches("M6").count() >= 4);
    let _ = std::fs::remove_file(all_path);

    // The engraved copper layer is only a source; it is not required to be a task.
    assert!(
        session
            .project()
            .layer(pbc_cam_core::LayerId::new(copper_layer))
            .is_some()
    );

    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().tasks().len(), 2);
    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().tasks().len(), 1);
}
