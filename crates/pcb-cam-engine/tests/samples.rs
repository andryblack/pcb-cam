//! End-to-end smoke test for the bundled demo project and the user tool library.

use pcb_cam_core::ToolKind;
use pcb_cam_engine::Session;
use serde_json::json;

#[test]
fn demo_project_uses_source_filters_and_machining_tools() {
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
    assert_eq!(drill["source_tools"], 3);
    assert!(session.tool_library().len() >= 6);

    let drill_layer = drill["layer"].as_u64().unwrap();
    let outline_layer = outline["layer"].as_u64().unwrap();
    let silk_layer = silk["layer"].as_u64().unwrap();
    let copper_layer = copper["layer"].as_u64().unwrap();

    let small_drill_tool = session
        .tool_library()
        .closest(ToolKind::Drill, 0.8)
        .unwrap()
        .value();
    let end_mill = session
        .tool_library()
        .closest(ToolKind::EndMill, 1.0)
        .unwrap()
        .value();
    let v_bit = session
        .tool_library()
        .closest(ToolKind::VBit, 0.2)
        .unwrap()
        .value();

    // Source tool T1 (0.8 mm) with a 0.8 mm drill.
    session
        .execute(
            "task.add_drill",
            &json!({
                "layer": drill_layer,
                "tool": small_drill_tool,
                "source_tools": [1],
                "depth": 1.6,
                "name": "Drill 0.8",
            }),
        )
        .unwrap();
    // Source tools T2/T3 (1.0 and 3.0 mm) with a 1.0 mm end mill.
    session
        .execute(
            "task.add_hole_mill",
            &json!({
                "layer": drill_layer,
                "tool": end_mill,
                "source_tools": [2, 3],
                "depth": 1.8,
                "name": "Hole mill 1.0 and 3.0",
            }),
        )
        .unwrap();
    session
        .execute(
            "task.add_cutout",
            &json!({"layer": outline_layer, "tool": end_mill}),
        )
        .unwrap();
    session
        .execute(
            "task.add_engrave",
            &json!({"layer": silk_layer, "tool": v_bit}),
        )
        .unwrap();

    assert_eq!(session.project().tasks().len(), 4);
    assert_eq!(session.project().enabled_task_count(), 4);

    // A single task produces a standalone program.
    let cutout_task = session.project().tasks()[2].id();
    let single_path =
        std::env::temp_dir().join(format!("pcb-cam-demo-cutout-{}.nc", std::process::id()));
    let single = session
        .execute(
            "file.export_task_gcode",
            &json!({"path": single_path, "task": cutout_task.value()}),
        )
        .unwrap();
    assert_eq!(single["tasks"], 1);
    assert_eq!(single["paths"], 1);
    let single_text = std::fs::read_to_string(&single_path).unwrap();
    assert!(single_text.contains("Edge cut"));
    assert!(single_text.contains("M6"));
    let _ = std::fs::remove_file(single_path);

    // The combined program contains all tasks, source filters and tool changes.
    let all_path = std::env::temp_dir().join(format!("pcb-cam-demo-all-{}.nc", std::process::id()));
    let combined = session
        .execute("file.export_gcode", &json!({"path": all_path}))
        .unwrap();
    assert_eq!(combined["tasks"], 4);
    assert_eq!(combined["holes"], 9);
    assert_eq!(combined["helical_holes"], 2);
    assert_eq!(combined["tools"], 3);
    let text = std::fs::read_to_string(&all_path).unwrap();
    assert!(text.contains("G21"));
    assert!(text.contains("Source tools: T1"));
    assert!(text.contains("Source tools: T2, T3"));
    assert!(text.contains("Helical mill"));
    assert!(text.contains("(Edge cut"));
    assert!(text.contains("(Clearance"));
    assert!(text.contains("M30"));
    assert!(text.matches("M6").count() >= 3);
    let _ = std::fs::remove_file(all_path);

    assert!(
        session
            .project()
            .layer(pcb_cam_core::LayerId::new(copper_layer))
            .is_some()
    );

    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().tasks().len(), 3);
    session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(session.project().tasks().len(), 2);
}
