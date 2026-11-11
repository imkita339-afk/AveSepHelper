//! Find components by their contents, retaining exact relative path spelling.
use crate::{excluded, game_dll, is_link, managed, read_metadata, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const LANGUAGES: &[&str] = &[
    "ko-KR", "en-US", "ja-JP", "zh-CN", "zh-TW", "de-DE", "fr-FR", "es-ES", "it-IT", "pl-PL",
    "pt-BR", "ru-RU", "sv-SE", "th-TH", "tr-TR",
];
#[derive(Debug, Clone)]
pub struct FoundModule {
    pub module: Value,
    pub reason: String,
    pub automatic: bool,
    pub entry_classes: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Package {
    pub root: PathBuf,
    pub title: String,
}
#[derive(Debug, Clone)]
pub struct Discovery {
    pub root: PathBuf,
    pub metadata: Option<Value>,
    pub packages: Vec<Package>,
    pub modules: Vec<FoundModule>,
    pub libraries: Vec<String>,
    pub covers: Vec<String>,
    pub notes: Vec<String>,
    pub file_count: usize,
}
fn rel(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
    dirs: &mut Vec<PathBuf>,
    notes: &mut Vec<String>,
) -> Result<()> {
    if depth > 32 {
        return Err("Слишком глубокая вложенность папок (больше 32)".into());
    }
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .take(20_001)
        .collect::<std::io::Result<_>>()
        .map_err(|e| e.to_string())?;
    if entries.len() > 20_000 {
        return Err("Слишком много файлов в одной папке. Выберите конкретный мод.".into());
    }
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().to_string();
        if excluded(relative)
            || name.starts_with(".sephiria-")
            || matches!(name.as_str(), "AddOns" | "BepInEx" | "Sephiria_Data")
        {
            continue;
        }
        if files.len() + dirs.len() >= 20_000 {
            return Err("Найдено больше 20 000 объектов. Выберите папку конкретного мода.".into());
        }
        let m = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if is_link(&m) {
            notes.push(format!("Пропущена ссылка/junction: {}", relative.display()));
            continue;
        }
        if m.is_dir() {
            dirs.push(relative.to_owned());
            walk(root, &path, depth + 1, files, dirs, notes)?;
        } else if m.is_file() {
            files.push(relative.to_owned());
        }
    }
    Ok(())
}
fn json_file(path: &Path) -> Result<Value> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
        return Err("JSON больше 16 МБ".into());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}
fn add(modules: &mut Vec<FoundModule>, kind: &str, path: &Path, reason: &str, automatic: bool) {
    let path = rel(path);
    if path.is_empty() || path == "." {
        return;
    }
    if !modules
        .iter()
        .any(|m| m.module["type"] == kind && m.module["path"] == path)
    {
        modules.push(FoundModule {
            module: json!({"type":kind,"path":path}),
            reason: reason.into(),
            automatic,
            entry_classes: Vec::new(),
        });
    }
}
pub fn discover(selected: &Path) -> Result<Discovery> {
    if !selected.is_dir() {
        return Err("Выберите существующую папку мода".into());
    }
    let mut notes = Vec::new();
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    walk(selected, selected, 0, &mut files, &mut dirs, &mut notes)?;
    let mut packages = Vec::new();
    for file in &files {
        if file.file_name().is_some_and(|n| n == "metadata.json") {
            let root = selected.join(file).parent().unwrap().to_owned();
            match read_metadata(&root) {
                Ok(m) if m["info"].is_object() && m["mod"].is_object() => packages.push(Package {
                    root,
                    title: crate::text(&m["info"]["title"]),
                }),
                Err(e) if file == Path::new("metadata.json") => return Err(e),
                _ => notes.push(format!(
                    "Не удалось распознать описание мода: {}",
                    file.display()
                )),
            }
        }
    }
    if !selected.join("metadata.json").exists()
        && packages.len() == 1
        && packages[0].root != selected
    {
        let mut d = discover(&packages[0].root)?;
        d.notes.insert(
            0,
            format!("Мод найден во вложенной папке: {}", d.root.display()),
        );
        return Ok(d);
    }
    if selected.join("metadata.json").exists() && packages.len() > 1 {
        // Never merge another published package into the selected package.
        return Err("Внутри мода есть другие metadata.json. Выберите отдельную папку мода без вложенных пакетов.".into());
    }
    let metadata = if selected.join("metadata.json").exists() {
        Some(read_metadata(selected)?)
    } else {
        None
    };
    if metadata
        .as_ref()
        .is_some_and(|m| !m["info"].is_object() || !m["mod"].is_object())
    {
        return Err("В metadata.json нужны объекты info и mod".into());
    }
    let mut d = Discovery {
        root: selected.to_owned(),
        metadata,
        packages,
        modules: Vec::new(),
        libraries: Vec::new(),
        covers: Vec::new(),
        notes,
        file_count: files.len(),
    };
    if d.metadata.is_none() && d.packages.len() > 1 {
        return Ok(d);
    }
    let mut costume_folders = BTreeSet::new();
    let mut managed_helpers = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        if file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            let stem = file.file_stem().unwrap_or_default().to_string_lossy();
            if LANGUAGES.contains(&stem.as_ref()) {
                match json_file(&selected.join(file)) {
                    Ok(v)
                        if v.as_object()
                            .is_some_and(|m| m.values().all(Value::is_string)) =>
                    {
                        let parent = file.parent().unwrap();
                        if parent.as_os_str().is_empty() {
                            d.notes.push(format!("{} лежит в корне. Поместите языковые файлы в отдельную папку, например Localization, и повторите поиск.", file.display()));
                        } else {
                            add(
                                &mut d.modules,
                                "localization",
                                parent,
                                "Найдена таблица перевода: ключ → строка",
                                true,
                            );
                        }
                    }
                    _ => d.notes.push(format!(
                        "Похож на перевод, но нужен JSON-объект со строками: {}",
                        file.display()
                    )),
                }
            }
            if name == "costume.json" || name == "skin.json" {
                match json_file(&selected.join(file)) {
                    Ok(v)
                        if v["id"].is_string()
                            && v["name"].is_string()
                            && v["animations"].is_object() =>
                    {
                        if let Some(parent) = file
                            .parent()
                            .and_then(Path::parent)
                            .filter(|p| !p.as_os_str().is_empty())
                        {
                            costume_folders.insert(parent.to_owned());
                            add(
                                &mut d.modules,
                                "costume",
                                parent,
                                "Найдены описания костюмов/скинов и анимаций",
                                true,
                            );
                        } else {
                            d.notes.push(format!(
                                "Для {} нужна структура ПапкаКостюмов/ИмяКостюма/{}",
                                file.display(),
                                name
                            ));
                        }
                    }
                    _ => d
                        .notes
                        .push(format!("Проверьте описание костюма: {}", file.display())),
                }
            }
        }
        if file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            if game_dll(file) {
                d.notes.push(format!(
                    "Игровая/системная DLL не добавлена: {}",
                    file.display()
                ));
                continue;
            }
            let full = selected.join(file);
            if fs::metadata(&full).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
                d.notes.push(format!(
                    "DLL больше 64 МБ, укажите её вручную: {}",
                    file.display()
                ));
                continue;
            }
            match fs::read(full).map_err(|e| e.to_string()).and_then(|b| managed::entries(&b)) {
                Ok(classes) if !classes.is_empty() => {
                    let automatic = classes.len() == 1;
                    d.modules.push(FoundModule { module: json!({"type":"dll","path":rel(file),"entryClass":if automatic { classes[0].clone() } else { String::new() }}), reason: if automatic { "Найден наследник WorkshopModBase/HorayModBase в DLL" } else { "Несколько точек входа — выберите класс" }.into(), automatic, entry_classes: classes });
                }
                Ok(_) => managed_helpers.push(rel(file)),
                Err(e) => d.notes.push(format!("DLL не распознана автоматически: {} — {e}. При необходимости добавьте вручную.", file.display())),
            }
        }
    }
    let classes: Vec<String> = d
        .modules
        .iter()
        .filter(|m| m.module["type"] == "dll")
        .filter_map(|m| {
            m.module["entryClass"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .collect();
    for m in &mut d.modules {
        if m.module["type"] == "dll"
            && classes
                .iter()
                .filter(|s| m.module["entryClass"].as_str() == Some(s))
                .count()
                > 1
        {
            m.automatic = false;
            m.reason = "Разные сборки с одинаковым классом запуска — выберите одну".into();
        }
    }
    if d.modules.iter().any(|m| m.module["type"] == "dll") {
        d.libraries = managed_helpers;
    } else {
        for path in managed_helpers {
            d.notes.push(format!("В DLL {path} не найдена однозначная точка входа. Если это мод, добавьте DLL и entryClass вручную."));
        }
    }
    for dir in &dirs {
        if matches!(
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str(),
            "packs" | "data"
        ) && !costume_folders.iter().any(|c| dir.starts_with(c))
        {
            add(
                &mut d.modules,
                "data",
                dir,
                "Название похоже на папку данных — подтвердите назначение",
                false,
            );
        }
    }
    let mut covers = Vec::new();
    for file in &files {
        if !file.extension().is_some_and(|e| {
            ["png", "jpg", "jpeg"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        }) || costume_folders.iter().any(|c| file.starts_with(c))
        {
            continue;
        }
        let stem = file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let priority = match stem.as_str() {
            "thumbnail" => 0,
            "preview" => 1,
            "cover" => 2,
            "banner" => 3,
            _ => 4,
        };
        if priority < 4 || file.parent().is_some_and(|p| p.as_os_str().is_empty()) {
            if let Ok(reader) = image::ImageReader::open(selected.join(file)) {
                if let Ok(reader) = reader.with_guessed_format() {
                    if let Ok((w, h)) = reader.into_dimensions() {
                        if w > 0 && h > 0 {
                            covers.push((priority, file.components().count(), rel(file)));
                        }
                    }
                }
            }
        }
    }
    covers.sort();
    d.covers = covers.into_iter().map(|(_, _, p)| p).take(32).collect();
    d.modules.sort_by_key(|m| {
        (
            m.module["type"].as_str().unwrap_or("").to_owned(),
            m.module["path"].as_str().unwrap_or("").to_owned(),
        )
    });
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Folder(PathBuf);
    impl Folder {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(".test-data")
                .join(format!(
                    "discovery-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn put(&self, name: &str, bytes: &[u8]) {
            let p = self.0.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, bytes).unwrap();
        }
    }
    impl Drop for Folder {
        fn drop(&mut self) {
            if self
                .0
                .starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join(".test-data"))
            {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
    #[test]
    fn finds_nested_components_by_content() {
        let f = Folder::new();
        f.put("Assets/Text/ru-RU.json", br#"{"hello":"World"}"#);
        f.put(
            "Assets/Characters/Cat/costume.json",
            br#"{"id":"cat","name":"Cat_Name","animations":{"idle":{"file":"idle.png"}}}"#,
        );
        f.put("obj/ru-RU.json", b"{}");
        f.put("Other/en-US.json", br#"{"bad":42}"#);
        let d = discover(&f.0).unwrap();
        assert!(d
            .modules
            .iter()
            .any(|m| m.module["path"] == "Assets/Text" && m.module["type"] == "localization"));
        assert!(d
            .modules
            .iter()
            .any(|m| m.module["path"] == "Assets/Characters" && m.module["type"] == "costume"));
        assert_eq!(d.modules.len(), 2);
        assert!(!d.notes.is_empty());
    }
    #[test]
    fn finds_managed_entry_and_dependencies_and_duplicate_builds() {
        let f = Folder::new();
        f.put(
            "bin/Release/Mod.dll",
            &crate::managed::fixture("WorkshopModBase", 1, false),
        );
        f.put(
            "bin/Release/Helper.dll",
            &crate::managed::fixture("Object", 1, false),
        );
        let d = discover(&f.0).unwrap();
        assert_eq!(d.modules[0].module["entryClass"], "Example.Entry");
        assert!(d.modules[0].automatic);
        assert_eq!(d.libraries, ["bin/Release/Helper.dll"]);
        f.put(
            "bin/Debug/Mod.dll",
            &crate::managed::fixture("WorkshopModBase", 1, false),
        );
        let d = discover(&f.0).unwrap();
        assert_eq!(d.modules.len(), 2);
        assert!(d.modules.iter().all(|m| !m.automatic));
    }
    #[test]
    fn chooses_single_nested_package_and_keeps_metadata() {
        let f = Folder::new();
        let mut m = crate::template();
        m["mod"]["modId"] = json!("author.published");
        m["custom"] = json!(42);
        f.put(
            "Wrapped/MyMod/metadata.json",
            &serde_json::to_vec(&m).unwrap(),
        );
        f.put("Wrapped/MyMod/uploadticket.json", b"ticket");
        let d = discover(&f.0).unwrap();
        assert_eq!(d.root, f.0.join("Wrapped/MyMod"));
        assert_eq!(d.metadata.unwrap(), m);
        assert_eq!(
            fs::read(f.0.join("Wrapped/MyMod/uploadticket.json")).unwrap(),
            b"ticket"
        );
    }
    #[test]
    fn asks_when_several_packages_are_present() {
        let f = Folder::new();
        let m = serde_json::to_vec(&crate::template()).unwrap();
        f.put("A/metadata.json", &m);
        f.put("B/metadata.json", &m);
        let d = discover(&f.0).unwrap();
        assert_eq!(d.packages.len(), 2);
        assert!(d.modules.is_empty());
        assert!(d.metadata.is_none());
    }
    #[test]
    fn finds_nested_cover_and_does_not_guess_data_semantics() {
        let f = Folder::new();
        fs::create_dir_all(f.0.join("Art")).unwrap();
        image::RgbImage::new(8, 8)
            .save(f.0.join("Art/preview.png"))
            .unwrap();
        f.put("Resources/Packs/content.json", b"{}");
        let d = discover(&f.0).unwrap();
        assert_eq!(d.covers, ["Art/preview.png"]);
        assert!(d
            .modules
            .iter()
            .any(|m| m.module["type"] == "data" && !m.automatic));
    }
    #[test]
    fn corrupt_dll_is_reported_and_never_loaded() {
        let f = Folder::new();
        f.put("unknown.dll", b"MZgarbage");
        let d = discover(&f.0).unwrap();
        assert!(d.modules.is_empty());
        assert_eq!(d.notes.len(), 1);
    }
}
