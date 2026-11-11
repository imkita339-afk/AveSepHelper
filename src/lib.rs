use image::ImageReader;
pub mod discovery;
mod managed;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, String>;
pub const TAGS: &[&str] = &[
    "Item",
    "Costume",
    "Weapon",
    "NPC",
    "Enemy",
    "Gameplay",
    "QoL",
    "Library",
    "Localization",
];
pub const TYPES: &[&str] = &["dll", "localization", "costume", "data"];
const COVER: &str = "workshop-prep-cover.jpg";

pub fn template() -> Value {
    json!({"info":{"title":"","description":""},"mod":{"schemaVersion":1,"modId":"author.my-mod","version":"1.0.0","author":"","multiplayer":{"installation":"everyone"},"modules":[]},"tags":[]})
}
pub fn read_metadata(root: &Path) -> Result<Value> {
    let bytes = fs::read(root.join("metadata.json"))
        .map_err(|e| format!("Не удалось прочитать metadata.json: {e}"))?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|e| e.to_string())?
        .trim_start_matches('\u{feff}');
    serde_json::from_str(text).map_err(|e| format!("Ошибка JSON: {e}"))
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
        && s != "."
        && s != ".."
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
pub fn text(v: &Value) -> String {
    v.as_str()
        .or_else(|| v.get("en-US").and_then(Value::as_str))
        .unwrap_or("")
        .to_owned()
}
pub fn set_text(v: &mut Value, value: String) {
    if v.is_object() {
        v["en-US"] = Value::String(value);
    } else {
        *v = Value::String(value);
    }
}
fn plain_path(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(['\\', ':', '\0'])
        && !s.starts_with('/')
        && s.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}
fn is_link(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
pub fn exact_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if !plain_path(relative) {
        return Err(format!("Недопустимый относительный путь: {relative}"));
    }
    let mut current = root.to_owned();
    for part in relative.split('/') {
        let entries = fs::read_dir(&current).map_err(|e| format!("{}: {e}", current.display()))?;
        let mut found = None;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == std::ffi::OsStr::new(part) {
                found = Some(entry.path());
                break;
            }
        }
        current =
            found.ok_or_else(|| format!("Файл/папка не найдены с точным регистром: {relative}"))?;
        if is_link(&fs::symlink_metadata(&current).map_err(|e| e.to_string())?) {
            return Err(format!("Ссылка или junction запрещены: {relative}"));
        }
    }
    Ok(current)
}
fn excluded(path: &Path) -> bool {
    path.components().any(|c| {
        let n = c.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            n.as_str(),
            ".git" | ".vs" | ".idea" | "obj" | "target" | "node_modules" | "__pycache__"
        )
    }) || path
        .file_name()
        .map(|s| {
            let s = s.to_string_lossy().to_lowercase();
            s.ends_with(".pdb")
                || s.ends_with(".csproj")
                || s.ends_with(".sln")
                || s.ends_with(".user")
                || s.ends_with(".tmp")
                || matches!(s.as_str(), "mod-guard-manifest.json" | "mod-guard.sig.json")
        })
        .unwrap_or(false)
}
fn game_dll(path: &Path) -> bool {
    let n = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    n == "assembly-csharp.dll"
        || n == "assembly-csharp-firstpass.dll"
        || n == "mscorlib.dll"
        || n == "netstandard.dll"
        || n.starts_with("unityengine") && n.ends_with(".dll")
        || n.starts_with("unityeditor") && n.ends_with(".dll")
}
#[derive(Default, Debug)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub files: Vec<PathBuf>,
    pub bytes: u64,
}
impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}
fn scan(
    root: &Path,
    dir: &Path,
    report: &mut Report,
    selected_dlls: Option<&HashSet<String>>,
) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_owned();
        if excluded(&rel) {
            report
                .warnings
                .push(format!("Исключено из сборки: {}", rel.display()));
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if is_link(&meta) {
            report
                .errors
                .push(format!("Ссылка или junction: {}", rel.display()));
            continue;
        }
        if meta.is_dir() {
            scan(root, &path, report, selected_dlls)?;
        } else if meta.is_file() {
            if rel
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
                && selected_dlls
                    .is_some_and(|set| !set.contains(&rel.to_string_lossy().replace('\\', "/")))
            {
                report.warnings.push(format!(
                    "DLL не выбрана и не попадёт в пакет: {}",
                    rel.display()
                ));
                continue;
            }
            if game_dll(&rel) {
                report
                    .errors
                    .push(format!("Уберите игровую/системную DLL: {}", rel.display()));
            }
            if rel
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
            {
                report.warnings.push(format!(
                    "EXE в пакете: {} — проверьте необходимость",
                    rel.display()
                ));
            }
            report.bytes += meta.len();
            report.files.push(rel);
        } else {
            report
                .errors
                .push(format!("Неподдерживаемый объект: {}", rel.display()));
        }
    }
    Ok(())
}
fn image_bytes(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::metadata(path).map_err(|e| format!("Обложка: {e}"))?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err("Исходная обложка больше 64 МБ".into());
    }
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let img = reader
        .decode()
        .map_err(|e| format!("Изображение не читается: {e}"))?
        .thumbnail(1024, 1024)
        .to_rgb8();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&img)
        .map_err(|e| e.to_string())?;
    if bytes.len() >= 1_000_000 {
        return Err("Подготовленная обложка превышает 1 МБ".into());
    }
    Ok(bytes)
}
fn validate_cover(path: &Path) -> Result<()> {
    let bytes = fs::read(path).map_err(|e| format!("Обложка: {e}"))?;
    if bytes.len() >= 1_000_000 {
        return Err("Обложка должна быть меньше 1 МБ".into());
    }
    let format = image::guess_format(&bytes).map_err(|_| "Обложка не является PNG/JPG")?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err("Обложка должна быть PNG/JPG".into());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Повреждённая обложка: {e}"))?;
    Ok(())
}
fn check_text(value: &Value, name: &str, max: usize, errors: &mut Vec<String>) {
    let values: Vec<&Value> = if let Some(map) = value.as_object() {
        map.values().collect()
    } else {
        vec![value]
    };
    if values.is_empty() {
        errors.push(format!("{name}: пустая языковая карта"));
    }
    for v in values {
        match v.as_str() {
            Some(s) if s.chars().count() <= max => (),
            _ => errors.push(format!("{name}: нужна строка длиной до {max} символов")),
        }
    }
}
pub fn validate(root: &Path, meta: &Value, cover: Option<&Path>) -> Report {
    validate_impl(root, meta, cover, false)
}
pub fn validate_selected(root: &Path, meta: &Value, cover: Option<&Path>) -> Report {
    validate_impl(root, meta, cover, true)
}
fn validate_impl(root: &Path, meta: &Value, cover: Option<&Path>, selected: bool) -> Report {
    let mut r = Report::default();
    if !root.is_dir() {
        r.errors.push("Выберите существующую папку мода".into());
        return r;
    }
    let selected_dlls: HashSet<String> = meta["mod"]["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["type"] == "dll")
        .filter_map(|m| m["path"].as_str().map(str::to_owned))
        .chain(
            meta["mod"]["libraries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().or_else(|| v["path"].as_str()).map(str::to_owned)),
        )
        .collect();
    if let Err(e) = scan(
        root,
        root,
        &mut r,
        if selected { Some(&selected_dlls) } else { None },
    ) {
        r.errors.push(e);
    }
    if !meta["info"].is_object() || !meta["mod"].is_object() {
        r.errors.push("Нужны объекты info и mod".into());
        return r;
    }
    check_text(&meta["info"]["title"], "Название", 128, &mut r.errors);
    check_text(
        &meta["info"]["description"],
        "Описание",
        7999,
        &mut r.errors,
    );
    if meta["info"]["title"].is_object() && meta["info"]["title"].get("en-US").is_none() {
        r.warnings
            .push("Добавьте en-US: этот язык используется для страницы Workshop".into());
    }
    let m = &meta["mod"];
    if !valid_id(string(m, "modId")) {
        r.errors.push(
            "modId: 1–64 строчных латинских символа, цифры, точки, дефисы, подчёркивания".into(),
        );
    }
    if string(m, "version").trim().is_empty() {
        r.errors.push("Укажите версию мода".into());
    }
    if m["schemaVersion"] != 1 {
        r.errors.push("Поддерживается schemaVersion = 1".into());
    }
    if let Some(mode) = m.get("multiplayer") {
        if !["everyone", "local_only", "host_only"].contains(&string(mode, "installation")) {
            r.errors
                .push("Некорректное multiplayer.installation".into());
        }
    }
    let mut declared = HashSet::new();
    let mut entries = HashSet::new();
    match m["modules"].as_array() {
        Some(modules) if !modules.is_empty() => {
            for module in modules {
                let ty = string(module, "type");
                let path = string(module, "path");
                if !TYPES.contains(&ty) {
                    r.errors.push(format!("Неизвестный тип модуля: {ty}"));
                }
                match exact_path(root, path) {
                    Ok(p) => {
                        if excluded(Path::new(path)) {
                            r.errors
                                .push(format!("Модуль попадает в исключения: {path}"));
                        }
                        if ty == "dll" {
                            if !p.is_file() || !path.to_lowercase().ends_with(".dll") {
                                r.errors
                                    .push(format!("Модуль DLL должен указывать на .dll: {path}"));
                            }
                            declared.insert(path.to_owned());
                        } else if !p.is_dir() {
                            r.errors
                                .push(format!("Модуль {ty} должен указывать на папку: {path}"));
                        }
                    }
                    Err(e) => r.errors.push(e),
                }
                if ty == "dll" {
                    let class = string(module, "entryClass");
                    if class.trim().is_empty() || !entries.insert(class) {
                        r.errors
                            .push("Для DLL нужны непустые и разные entryClass".into());
                    }
                }
                if ty == "localization" {
                    if let Ok(p) = exact_path(root, path) {
                        if let Ok(files) = fs::read_dir(p) {
                            for f in files
                                .flatten()
                                .filter(|f| f.path().extension().is_some_and(|e| e == "json"))
                            {
                                if let Err(e) = fs::read_to_string(f.path())
                                    .map_err(|e| e.to_string())
                                    .and_then(|s| {
                                        serde_json::from_str::<Value>(
                                            s.trim_start_matches('\u{feff}'),
                                        )
                                        .map_err(|e| e.to_string())
                                    })
                                {
                                    r.errors.push(format!(
                                        "JSON локализации {}: {e}",
                                        f.path().display()
                                    ));
                                }
                            }
                        }
                    }
                }
                if let Some(consumer) = module
                    .get("consumer")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    if !m["dependencies"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|d| string(d, "modId") == consumer))
                    {
                        r.errors
                            .push(format!("consumer {consumer} отсутствует в dependencies"));
                    }
                }
            }
        }
        _ => r.errors.push("Добавьте хотя бы один модуль".into()),
    }
    if let Some(libraries) = m.get("libraries") {
        if let Some(libraries) = libraries.as_array() {
            for lib in libraries {
                let path = lib.as_str().unwrap_or_else(|| string(lib, "path"));
                if !path.to_lowercase().ends_with(".dll") || !declared.insert(path.to_owned()) {
                    r.errors
                        .push(format!("Некорректная/повторная библиотека: {path}"));
                }
                match exact_path(root, path) {
                    Ok(p) if p.is_file() && !excluded(Path::new(path)) => (),
                    Ok(_) => r
                        .errors
                        .push(format!("Библиотека не является включаемым файлом: {path}")),
                    Err(e) => r.errors.push(e),
                }
            }
        } else {
            r.errors.push("libraries должен быть массивом".into());
        }
    }
    if let Some(deps) = m.get("dependencies") {
        if let Some(deps) = deps.as_array() {
            for dep in deps {
                if !valid_id(string(dep, "modId")) {
                    r.errors.push("У зависимости некорректный modId".into());
                }
            }
        } else {
            r.errors.push("dependencies должен быть массивом".into());
        }
    }
    if let Some(tags) = meta.get("tags") {
        if let Some(tags) = tags.as_array() {
            for tag in tags {
                if !tag.as_str().is_some_and(|s| TAGS.contains(&s)) {
                    r.errors.push(format!("Неизвестный тег: {tag}"));
                }
            }
        } else {
            r.errors.push("tags должен быть массивом".into());
        }
    }
    for file in &r.files {
        if file
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("dll"))
        {
            let p = file.to_string_lossy().replace('\\', "/");
            if !declared.contains(&p) {
                r.errors
                    .push(format!("DLL не объявлена в modules/libraries: {p}"));
            }
        }
    }
    if !declared.is_empty() {
        r.warnings.push("DLL отправляются на проверку кнопкой в игре. Лимит проверки — 20 МБ в сжатом виде; программа не измеряет серверный архив".into());
    }
    let result = if let Some(cover) = cover {
        image_bytes(cover).map(|_| ())
    } else {
        let preview = meta["info"].get("preview");
        let path = if let Some(preview) = preview {
            exact_path(root, preview.as_str().unwrap_or(""))
        } else {
            [
                "thumbnail.png",
                "preview.png",
                "thumbnail.jpg",
                "preview.jpg",
            ]
            .iter()
            .find_map(|s| exact_path(root, s).ok())
            .ok_or_else(|| "Добавьте обложку PNG/JPG меньше 1 МБ".into())
        };
        path.and_then(|p| validate_cover(&p))
    };
    if let Err(e) = result {
        r.errors.push(e);
    }
    r.files.sort();
    r
}

pub struct Built {
    pub destination: PathBuf,
    pub backup: Option<PathBuf>,
}
fn canonical(path: &Path) -> Result<PathBuf> {
    let full = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    {
        // Normal drive paths also work with sandboxed Windows file permissions.
        let text = full.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{unc}")));
        }
        if let Some(drive) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(drive));
        }
    }
    Ok(full)
}
fn stamp() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
pub fn build(source: &Path, parent: &Path, meta: &Value, cover: Option<&Path>) -> Result<Built> {
    build_impl(source, parent, meta, cover, false)
}
pub fn build_selected(
    source: &Path,
    parent: &Path,
    meta: &Value,
    cover: Option<&Path>,
) -> Result<Built> {
    build_impl(source, parent, meta, cover, true)
}
fn build_impl(
    source: &Path,
    parent: &Path,
    meta: &Value,
    cover: Option<&Path>,
    selected: bool,
) -> Result<Built> {
    let report = validate_impl(source, meta, cover, selected);
    if !report.ok() {
        return Err(report.errors.join("\n"));
    }
    let source = canonical(source)?;
    let id = string(&meta["mod"], "modId");
    if source.join("uploadticket.json").exists() {
        let original = read_metadata(&source)?;
        if original["mod"]["modId"] != meta["mod"]["modId"] {
            return Err("Нельзя менять modId у исходника с uploadticket.json".into());
        }
    }
    // Prevent reserved Windows directory names even when building on another OS.
    let base = id.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(
        base.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) || id.ends_with('.')
    {
        return Err("modId нельзя использовать как имя папки Windows".into());
    }
    if !parent.is_dir() {
        return Err(
            "Выберите существующую папку назначения (WorkshopLocal или отдельную папку сборок)"
                .into(),
        );
    }
    let parent = canonical(parent)?;
    let destination = parent.join(id);
    if destination.starts_with(&source)
        || source.starts_with(&destination)
        || parent.starts_with(&source)
    {
        return Err("Исходник и папка сборки не должны быть вложены друг в друга".into());
    }
    let mut ticket = None;
    let mut backup = None;
    if destination.exists() {
        if is_link(&fs::symlink_metadata(&destination).map_err(|e| e.to_string())?) {
            return Err("Папка назначения является ссылкой/junction".into());
        }
        let old = read_metadata(&destination)?;
        if old["mod"]["modId"] != meta["mod"]["modId"] {
            return Err("modId существующей публикации отличается".into());
        }
        if old["mod"]["version"] == meta["mod"]["version"] {
            return Err("Для обновления измените mod.version. Существующая папка сохранена".into());
        }
        let ticket_path = destination.join("uploadticket.json");
        if ticket_path.exists() {
            exact_path(&destination, "uploadticket.json")?;
            ticket = Some(fs::read(ticket_path).map_err(|e| e.to_string())?);
        }
    }
    let stage = parent.join(format!(".sephiria-stage-{}", stamp()));
    fs::create_dir(&stage).map_err(|e| format!("Создание {}: {e}", stage.display()))?;
    let prepared = (|| -> Result<()> {
        for rel in report.files {
            if rel == Path::new("metadata.json") {
                continue;
            }
            let from = exact_path(&source, &rel.to_string_lossy().replace('\\', "/"))?;
            let to = stage.join(&rel);
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            fs::copy(&from, &to)
                .map_err(|e| format!("Копирование {} → {}: {e}", from.display(), to.display()))?;
        }
        // Retain empty module directories too.
        for module in meta["mod"]["modules"].as_array().into_iter().flatten() {
            if string(module, "type") != "dll" {
                fs::create_dir_all(stage.join(string(module, "path")))
                    .map_err(|e| e.to_string())?;
            }
        }
        let mut output = meta.clone();
        if let Some(cover) = cover {
            fs::write(stage.join(COVER), image_bytes(cover)?).map_err(|e| e.to_string())?;
            output["info"]["preview"] = json!(COVER);
        }
        if let Some(ticket) = ticket {
            fs::write(stage.join("uploadticket.json"), ticket).map_err(|e| e.to_string())?;
        }
        fs::write(
            stage.join("metadata.json"),
            serde_json::to_vec_pretty(&output).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let final_report = validate(&stage, &output, None);
        if !final_report.ok() {
            return Err(final_report.errors.join("\n"));
        }
        Ok(())
    })();
    if let Err(e) = prepared {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    if destination.exists() {
        let backups = parent.join(".sephiria-prep-backups");
        if backups.exists() && is_link(&fs::symlink_metadata(&backups).map_err(|e| e.to_string())?)
        {
            return Err("Каталог резервных копий является ссылкой".into());
        }
        fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
        let old_path = backups.join(format!("{id}-{}", stamp()));
        fs::rename(&destination, &old_path).map_err(|e| e.to_string())?;
        backup = Some(old_path);
    }
    if let Err(e) = fs::rename(&stage, &destination) {
        if let Some(old) = &backup {
            if let Err(rollback) = fs::rename(old, &destination) {
                return Err(format!(
                    "{e}; восстановление не удалось: {rollback}. Копия: {}",
                    old.display()
                ));
            }
        }
        let _ = fs::remove_dir_all(&stage);
        return Err(format!("Размещение {}: {e}", destination.display()));
    }
    Ok(Built {
        destination,
        backup,
    })
}
