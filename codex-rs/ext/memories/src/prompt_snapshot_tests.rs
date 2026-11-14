// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::*;
#[tokio::test]
async fn snapshot_prompt_pins_derived_and_original_note_paths_and_reports_uncertainty() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let source = AbsolutePathBuf::from_absolute_path(temp.path().join("source"))?;
    let derived = AbsolutePathBuf::from_absolute_path(temp.path().join("generation"))?;
    tokio::fs::create_dir_all(&source).await?;
    tokio::fs::create_dir_all(&derived).await?;
    tokio::fs::write(derived.join("memory_summary.md"), "synthetic historic summary").await?;
    for version in [MemoryVersion::V1, MemoryVersion::V2] {
        let rendered = build_memory_instructions_from_snapshot(&derived, &source, version, Some(false), Some("consolidated"), None)
            .await?.expect("summary");
        assert!(rendered.contains(&format!("{}/rollout_summaries/", derived.display())));
        assert!(rendered.contains(&format!("{}/extensions/ad_hoc/notes/", source.display())));
        assert!(rendered.contains("do not assume their corrections are reflected"));
        assert!(rendered.contains("current instructions and approvals"));
        assert!(rendered.contains("synthetic historic summary"));
        let legacy = build_memory_instructions_from_snapshot(&derived, &source, version, None, Some("legacy_import"), None).await?.unwrap();
        assert!(legacy.contains("completeness and correction application are unverified"));
    }
    Ok(())
}
