use sephiria_workshop_prep::{build, exact_path, read_metadata, template, validate};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    out: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".test-data")
            .join(format!(
                "sephiria-prep-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        let source = root.join("source");
        let out = root.join("output");
        fs::create_dir_all(source.join("Localization")).unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(source.join("Localization/en-US.json"), "{}").unwrap();
        image::RgbImage::new(8, 8)
            .save(source.join("thumbnail.png"))
            .unwrap();
        Self { root, source, out }
    }
    fn metadata(&self) -> serde_json::Value {
        let mut m = template();
        m["info"]["title"] = json!("Тест");
        m["mod"]["modules"] = json!([{"type":"localization","path":"Localization"}]);
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self
            .root
            .starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join(".test-data"))
            && self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sephiria-prep-test-")
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[test]
fn selected_export_excludes_other_builds_and_game_dlls() {
    let f = Fixture::new();
    let mut m = f.metadata();
    fs::write(f.source.join("Mod.dll"), b"fixture DLL").unwrap();
    fs::write(f.source.join("Old.dll"), b"unused build").unwrap();
    fs::write(f.source.join("Assembly-CSharp.dll"), b"game copy").unwrap();
    m["mod"]["modules"] = json!([{"type":"dll","path":"Mod.dll","entryClass":"Example.Entry"}]);
    assert!(!validate(&f.source, &m, None).ok());
    let report = sephiria_workshop_prep::validate_selected(&f.source, &m, None);
    assert!(report.ok(), "{:?}", report.errors);
    let built = sephiria_workshop_prep::build_selected(&f.source, &f.out, &m, None).unwrap();
    assert!(built.destination.join("Mod.dll").exists());
    assert!(!built.destination.join("Old.dll").exists());
    assert!(!built.destination.join("Assembly-CSharp.dll").exists());
    assert!(f.source.join("Old.dll").exists());
}
#[test]
fn paths_require_exact_case_and_reject_escape() {
    let f = Fixture::new();
    assert!(exact_path(&f.source, "Localization/en-US.json").is_ok());
    for p in [
        "localization/en-US.json",
        "../source/thumbnail.png",
        "C:/test",
        "/tmp/a",
        "Localization\\en-US.json",
        "./thumbnail.png",
        "Localization//en-US.json",
    ] {
        assert!(exact_path(&f.source, p).is_err(), "{p}");
    }
}
#[test]
fn prepares_clean_package_without_changing_source() {
    let f = Fixture::new();
    let m = f.metadata();
    fs::write(f.source.join("debug.pdb"), "debug").unwrap();
    fs::create_dir(f.source.join("obj")).unwrap();
    fs::write(f.source.join("obj/a.dll"), "build junk").unwrap();
    let r = validate(&f.source, &m, None);
    assert!(r.ok(), "{:?}", r.errors);
    let result = build(&f.source, &f.out, &m, None).unwrap();
    assert!(result.destination.join("metadata.json").is_file());
    assert!(result.destination.join("Localization/en-US.json").is_file());
    assert!(!result.destination.join("debug.pdb").exists());
    assert!(!result.destination.join("obj").exists());
    assert!(f.source.join("debug.pdb").exists());
    assert!(!f.source.join("metadata.json").exists());
    assert_eq!(read_metadata(&result.destination).unwrap(), m);
}
#[test]
fn update_keeps_ticket_and_backup_and_requires_new_version() {
    let f = Fixture::new();
    let mut m = f.metadata();
    let first = build(&f.source, &f.out, &m, None).unwrap();
    fs::write(
        first.destination.join("uploadticket.json"),
        "original-ticket",
    )
    .unwrap();
    fs::write(f.source.join("uploadticket.json"), "stale-source-ticket").unwrap();
    fs::write(
        f.source.join("metadata.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    assert!(build(&f.source, &f.out, &m, None).is_err());
    assert_eq!(
        fs::read_to_string(first.destination.join("uploadticket.json")).unwrap(),
        "original-ticket"
    );
    m["mod"]["version"] = json!("1.0.1");
    let updated = build(&f.source, &f.out, &m, None).unwrap();
    assert_eq!(
        fs::read_to_string(updated.destination.join("uploadticket.json")).unwrap(),
        "original-ticket"
    );
    assert_eq!(
        read_metadata(updated.backup.as_ref().unwrap()).unwrap()["mod"]["version"],
        "1.0.0"
    );
}
#[test]
fn invalid_input_does_not_touch_existing_package() {
    let f = Fixture::new();
    let mut m = f.metadata();
    let first = build(&f.source, &f.out, &m, None).unwrap();
    m["mod"]["version"] = json!("1.0.1");
    m["mod"]["modules"][0]["path"] = json!("missing");
    assert!(build(&f.source, &f.out, &m, None).is_err());
    assert_eq!(
        read_metadata(&first.destination).unwrap()["mod"]["version"],
        "1.0.0"
    );
}
#[test]
fn checks_dll_entry_libraries_and_game_assemblies() {
    let f = Fixture::new();
    let mut m = f.metadata();
    fs::write(f.source.join("mod.dll"), "fixture").unwrap();
    assert!(!validate(&f.source, &m, None).ok());
    m["mod"]["modules"] = json!([{"type":"dll","path":"mod.dll","entryClass":"Mod.Entry"}]);
    assert!(validate(&f.source, &m, None).ok());
    m["mod"]["libraries"] = json!(["mod.dll"]);
    assert!(!validate(&f.source, &m, None).ok());
    m["mod"].as_object_mut().unwrap().remove("libraries");
    fs::write(f.source.join("Assembly-CSharp.dll"), "game fixture").unwrap();
    assert!(!validate(&f.source, &m, None).ok());
}
#[test]
fn cover_conversion_and_corrupt_cover() {
    let f = Fixture::new();
    let m = f.metadata();
    let cover = f.root.join("external.png");
    image::RgbImage::new(1200, 800).save(&cover).unwrap();
    let b = build(&f.source, &f.out, &m, Some(&cover)).unwrap();
    let out = read_metadata(&b.destination).unwrap();
    let p = out["info"]["preview"].as_str().unwrap();
    assert!(fs::metadata(b.destination.join(p)).unwrap().len() < 1_000_000);
    assert!(image::open(b.destination.join(p)).unwrap().width() <= 1024);
    fs::write(f.source.join("thumbnail.png"), "not an image").unwrap();
    assert!(!validate(&f.source, &m, None).ok());
}
#[test]
fn prevents_output_inside_source_and_reserved_names() {
    let f = Fixture::new();
    let mut m = f.metadata();
    let nested = f.source.join("output");
    fs::create_dir(&nested).unwrap();
    assert!(build(&f.source, &nested, &m, None).is_err());
    m["mod"]["modId"] = json!("con.mod");
    assert!(build(&f.source, &f.out, &m, None).is_err());
    assert!(!f.out.join("con.mod").exists());
}
#[test]
fn localized_fields_and_bad_localization_json() {
    let f = Fixture::new();
    let mut m = f.metadata();
    m["info"]["title"] = json!({"en-US":"Example", "ru-RU":"Пример"});
    assert!(validate(&f.source, &m, None).ok());
    sephiria_workshop_prep::set_text(&mut m["info"]["title"], "Updated".into());
    assert_eq!(m["info"]["title"]["ru-RU"], "Пример");
    fs::write(f.source.join("Localization/en-US.json"), "{bad").unwrap();
    assert!(!validate(&f.source, &m, None).ok());
}
#[cfg(windows)]
#[test]
#[ignore = "Requires permission to create a Windows junction; restricted workspace denies mklink"]
fn junction_is_rejected() {
    use std::process::Command;
    let f = Fixture::new();
    let outside = f.root.join("outside");
    fs::create_dir(&outside).unwrap();
    let link = f.source.join("linked");
    // Fixed test-owned paths, no shell interpolation of user input.
    let status = Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!validate(&f.source, &f.metadata(), None).ok());
    assert!(exact_path(&f.source, "linked").is_err());
    // Remove the junction itself, never recursively traverse it.
    fs::remove_dir(&link).unwrap();
    assert!(Path::new(&outside).exists());
}
