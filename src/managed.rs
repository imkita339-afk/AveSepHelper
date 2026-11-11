//! Read-only PE/CLI metadata inspection. Never loads or executes an assembly.
use crate::Result;

struct Bytes<'a>(&'a [u8]);
impl<'a> Bytes<'a> {
    fn slice(&self, p: usize, n: usize) -> Result<&'a [u8]> {
        self.0
            .get(p..p.checked_add(n).ok_or("Переполнение смещения DLL")?)
            .ok_or_else(|| "Повреждённая или неполная DLL".into())
    }
    fn u16(&self, p: usize) -> Result<usize> {
        Ok(u16::from_le_bytes(self.slice(p, 2)?.try_into().unwrap()) as usize)
    }
    fn u32(&self, p: usize) -> Result<usize> {
        Ok(u32::from_le_bytes(self.slice(p, 4)?.try_into().unwrap()) as usize)
    }
    fn index(&self, p: usize, wide: usize) -> Result<usize> {
        if wide == 2 {
            self.u16(p)
        } else {
            self.u32(p)
        }
    }
}
fn idx(rows: &[usize; 64], tables: &[usize], tag_bits: u32) -> usize {
    if tables.iter().map(|i| rows[*i]).max().unwrap_or(0) >= (1 << (16 - tag_bits)) {
        4
    } else {
        2
    }
}
fn full_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.into()
    } else {
        format!("{namespace}.{name}")
    }
}

pub fn entries(bytes: &[u8]) -> Result<Vec<String>> {
    let b = Bytes(bytes);
    if b.slice(0, 2)? != b"MZ" {
        return Err("Не является PE/DLL".into());
    }
    let pe = b.u32(0x3c)?;
    if b.slice(pe, 4)? != b"PE\0\0" {
        return Err("Некорректный PE-заголовок".into());
    }
    let sections = b.u16(pe + 6)?;
    let optional_size = b.u16(pe + 20)?;
    if sections > 96 {
        return Err("Слишком много секций PE".into());
    }
    let optional = pe.checked_add(24).ok_or("Смещение PE")?;
    b.slice(optional, optional_size)?;
    let directories = match b.u16(optional)? {
        0x10b => 96,
        0x20b => 112,
        _ => return Err("Неизвестный формат PE".into()),
    };
    if optional_size < directories + 15 * 8 {
        return Err("Нет каталога CLI".into());
    }
    let section_table = optional + optional_size;
    let rva = |address: usize| -> Result<usize> {
        if address == 0 {
            return Err("В DLL нет управляемых метаданных .NET".into());
        }
        for i in 0..sections {
            let p = section_table + i * 40;
            let va = b.u32(p + 12)?;
            let raw_size = b.u32(p + 16)?;
            if address >= va && address - va < raw_size {
                let p = b
                    .u32(p + 20)?
                    .checked_add(address - va)
                    .ok_or("Смещение секции PE")?;
                b.slice(p, 1)?;
                return Ok(p);
            }
        }
        Err("RVA вне секций DLL".into())
    };
    let cli = rva(b.u32(optional + directories + 14 * 8)?)?;
    let metadata = rva(b.u32(cli + 8)?)?;
    let metadata_size = b.u32(cli + 12)?;
    let m = Bytes(b.slice(metadata, metadata_size)?);
    if m.slice(0, 4)? != b"BSJB" {
        return Err("Некорректная сигнатура CLI".into());
    }
    let version_len = m.u32(12)?;
    let p = 16_usize
        .checked_add(version_len)
        .and_then(|v| v.checked_add(3))
        .ok_or("Смещение версии CLI")?
        & !3;
    let streams = m.u16(p + 2)?;
    if streams > 64 {
        return Err("Слишком много потоков CLI".into());
    }
    let mut cursor = p + 4;
    let mut strings = None;
    let mut tables = None;
    for _ in 0..streams {
        let offset = m.u32(cursor)?;
        let size = m.u32(cursor + 4)?;
        let start = cursor + 8;
        let mut end = start;
        while m.slice(end, 1)?[0] != 0 {
            end += 1;
            if end - start > 32 {
                return Err("Имя потока CLI слишком длинное".into());
            }
        }
        let name = m.slice(start, end - start)?;
        let data = m.slice(offset, size)?;
        if name == b"#Strings" {
            strings = Some(data);
        }
        if name == b"#~" || name == b"#-" {
            tables = Some(data);
        }
        cursor = (end + 4) & !3;
    }
    let strings = strings.ok_or("Нет #Strings")?;
    let t = Bytes(tables.ok_or("Нет таблиц CLI")?);
    let heap = t.slice(6, 1)?[0];
    let s = if heap & 1 != 0 { 4 } else { 2 };
    let g = if heap & 2 != 0 { 4 } else { 2 };
    let valid = u64::from_le_bytes(t.slice(8, 8)?.try_into().unwrap());
    let mut rows = [0_usize; 64];
    let mut p = 24;
    for (i, row) in rows.iter_mut().enumerate() {
        if valid & (1 << i) != 0 {
            *row = t.u32(p)?;
            p += 4;
        }
    }
    if rows[1] > 200_000 || rows[2] > 200_000 {
        return Err("Слишком много типов в DLL".into());
    }
    let string = |index: usize| -> Result<String> {
        let tail = strings.get(index..).ok_or("Строка вне #Strings")?;
        let len = tail
            .iter()
            .position(|b| *b == 0)
            .ok_or("Незавершённая строка CLI")?;
        if len > 4096 {
            return Err("Имя типа CLI слишком длинное".into());
        }
        std::str::from_utf8(&tail[..len])
            .map(str::to_owned)
            .map_err(|e| e.to_string())
    };
    let scope = idx(&rows, &[0, 26, 35, 1], 2);
    let extends_size = idx(&rows, &[2, 1, 27], 2);
    p += rows[0].checked_mul(2 + s + g * 3).ok_or("Размер Module")?;
    let mut refs = Vec::new();
    for _ in 0..rows[1] {
        refs.push(full_name(
            &string(t.index(p + scope + s, s)?)?,
            &string(t.index(p + scope, s)?)?,
        ));
        p += scope + s * 2;
    }
    let type_size = 4 + s * 2 + extends_size + idx(&rows, &[4], 0) + idx(&rows, &[6], 0);
    let mut types = Vec::new();
    for _ in 0..rows[2] {
        let flags = t.u32(p)?;
        let name = string(t.index(p + 4, s)?)?;
        let namespace = string(t.index(p + 4 + s, s)?)?;
        let base = t.index(p + 4 + s * 2, extends_size)?;
        types.push((flags, full_name(&namespace, &name), base));
        p += type_size;
    }
    let is_base = |name: &str| {
        matches!(
            name.rsplit('.').next().unwrap_or(""),
            "WorkshopModBase" | "HorayModBase"
        )
    };
    let mut result = Vec::new();
    for (i, (flags, name, _)) in types.iter().enumerate() {
        // Nested names need NestedClass-table resolution; never guess a wrong name.
        if flags & 7 != 1 || flags & (0x80 | 0x20) != 0 || name.contains('`') {
            continue;
        }
        let mut current = i;
        let mut seen = std::collections::HashSet::new();
        while seen.insert(current) {
            let base = types[current].2;
            let row = (base >> 2).wrapping_sub(1);
            match base & 3 {
                1 => {
                    if refs.get(row).is_some_and(|n| is_base(n)) {
                        result.push(name.clone());
                    }
                    break;
                }
                0 if row < types.len() => current = row,
                _ => break,
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

#[cfg(test)]
pub(crate) fn fixture(base_name: &str, flags: u32, inherited: bool) -> Vec<u8> {
    fn u16(b: &mut [u8], p: usize, v: u16) {
        b[p..p + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn u32(b: &mut [u8], p: usize, v: u32) {
        b[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    let mut b = vec![0; 0x900];
    b[..2].copy_from_slice(b"MZ");
    u32(&mut b, 0x3c, 0x80);
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    u16(&mut b, 0x86, 1);
    u16(&mut b, 0x94, 224);
    u16(&mut b, 0x98, 0x10b);
    u32(&mut b, 0x98 + 96 + 14 * 8, 0x2000);
    u32(&mut b, 0x178 + 12, 0x2000);
    u32(&mut b, 0x178 + 16, 0x700);
    u32(&mut b, 0x178 + 20, 0x200);
    u32(&mut b, 0x208, 0x2080);
    u32(&mut b, 0x20c, 0x600);
    let m = &mut b[0x280..];
    m[..4].copy_from_slice(b"BSJB");
    u32(m, 12, 4);
    u16(m, 22, 2);
    u32(m, 24, 0x80);
    u32(m, 28, 0x100);
    m[32..36].copy_from_slice(b"#~\0\0");
    u32(m, 36, 0x200);
    u32(m, 40, 0x200);
    m[44..53].copy_from_slice(b"#Strings\0");
    let mut strings = vec![0];
    let mut add = |s: &str| {
        let p = strings.len() as u16;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        p
    };
    let base = add(base_name);
    let entry = add("Entry");
    let ns = add("Example");
    let derived = add("Child");
    m[0x200..0x200 + strings.len()].copy_from_slice(&strings);
    let t = &mut m[0x80..0x180];
    t[8..16].copy_from_slice(&6_u64.to_le_bytes());
    u32(t, 24, 1);
    u32(t, 28, if inherited { 3 } else { 2 });
    u16(t, 34, base);
    let p = 38 + 14;
    u32(t, p, flags);
    u16(t, p + 4, entry);
    u16(t, p + 6, ns);
    u16(t, p + 8, 5);
    if inherited {
        let p = p + 14;
        u32(t, p, 1);
        u16(t, p + 4, derived);
        u16(t, p + 6, ns);
        u16(t, p + 8, 8);
    }
    b
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Set AVE_TEST_DLL_SAMPLE to the official PhysicalDamagePlus.dll example"]
    fn official_sample() {
        let p = std::env::var("AVE_TEST_DLL_SAMPLE").expect("AVE_TEST_DLL_SAMPLE");
        let names = entries(&std::fs::read(p).unwrap()).unwrap();
        assert!(
            names.iter().any(|n| n == "PhysicalDamagePlus.Entry"),
            "{names:?}"
        );
    }
    #[test]
    fn malformed_data_never_panics() {
        for len in [0, 1, 2, 63, 64, 512, 4096] {
            let mut b = vec![0xff; len];
            if len >= 2 {
                b[..2].copy_from_slice(b"MZ");
            }
            assert!(entries(&b).is_err());
        }
    }
    #[test]
    fn reads_direct_and_inherited_entry_points_without_execution() {
        assert_eq!(
            entries(&fixture("WorkshopModBase", 1, false)).unwrap(),
            ["Example.Entry"]
        );
        assert_eq!(
            entries(&fixture("HorayModBase", 1, true)).unwrap(),
            ["Example.Child", "Example.Entry"]
        );
        assert!(entries(&fixture("Object", 1, false)).unwrap().is_empty());
        for flags in [0, 2, 0x81, 0x21] {
            assert!(entries(&fixture("WorkshopModBase", flags, false))
                .unwrap()
                .is_empty());
        }
    }
    #[test]
    fn every_truncation_is_handled() {
        let bytes = fixture("WorkshopModBase", 1, false);
        for len in 0..bytes.len() {
            let _ = entries(&bytes[..len]);
        }
    }
}
