# Project Watch (prototype)
Native Rust desktop app (egui/eframe). No webview, no Electron. White background, #1d1d1d ink. Single user, one computer.

Run: `cargo run --release [projects.csv ...] [--sample]`
- Linux: import a CSV by command-line argument or drag-and-drop. Windows: "Import CSV" dialog (untested).
- Data is saved on every change to `%APPDATA%\ProjectWatch` (Windows) or `~/.local/share/project-watch`. Override with PROJECT_WATCH_HOME.
- "My workspace" and "Sample" are separate stores. Sample data is invented and labelled SAMPLE.

Workflow: flag -> blocker (owner, due date) -> progress update with evidence -> recovery plan with a committed date -> forecast tracked against it. History log per project. Export report writes a text file.

CSV columns: project_id, project, contractor, budget_lakh, spent_lakh, task_id, task, start, end, pct_complete. Optional: owner, rejected_lots, depends_on. Re-importing a project id applies changed progress, spend and dates and logs them.

Corrections: nothing is silently deleted. Projects and tasks are archived or removed with a confirm click and can be restored; blockers can be edited, withdrawn (kept, reopenable) or reopened; every change is in the project history. Evidence fields are text references only (no files are attached or stored). A .bak copy of the data file is kept on each save. CSV import rejects bad dates, pct outside 0-100, negative amounts, duplicate task ids and unknown predecessors before changing anything.

Limits: rules are explainable flags for human review, not a validated prediction. SAP is not connected. No sync, no alerts, nothing is sent. Windows exe not built.
Font: Inter (OFL), see assets/Inter-LICENSE.txt.
