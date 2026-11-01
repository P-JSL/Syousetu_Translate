//! User-owned dictionary loaders for the independent Rust translation engine.
//!
//! Fixed-width .jk support is based only on the record structure observed in
//! the supplied UserDict.jk sample (five 110-byte records). It does NOT decode
//! proprietary JisJK.Inf, JisKJ.da, ROMKJ.dat, or language-transfer models.
use anyhow::{bail, Context, Result};
use encoding_rs::{EUC_KR, SHIFT_JIS, UTF_16BE, UTF_16LE};
use std::{fs, path::Path};

pub fn read(path: &Path) -> Result<Vec<(String, String)>> {
    let data = fs::read(path)
        .with_context(|| format!("사전 파일 읽기 실패: {}", path.display()))?;
    let is_jk = path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jk"));
    if is_jk { return read_fixed_jk(&data); }
    let text = if data.starts_with(&[0xff, 0xfe]) {
        let (text, _, bad) = UTF_16LE.decode(&data[2..]);
        if bad { bail!("UTF-16LE 사전 디코딩 실패"); }
        text.into_owned()
    } else if data.starts_with(&[0xfe, 0xff]) {
        let (text, _, bad) = UTF_16BE.decode(&data[2..]);
        if bad { bail!("UTF-16BE 사전 디코딩 실패"); }
        text.into_owned()
    } else {
        String::from_utf8(data).context("사전은 UTF-8 또는 UTF-16 BOM 형식이어야 합니다")?
    };
    read_tsv(&text)
}

fn read_tsv(text: &str) -> Result<Vec<(String, String)>> {
    let mut entries = Vec::new();
    for (idx, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with("//")
            || line.trim_start().starts_with('#') { continue; }
        let mut fields = line.split('\t');
        let src = fields.next().unwrap_or("").trim();
        let dst = fields.next().unwrap_or("").trim().trim_end_matches("#N");
        if src.is_empty() || dst.is_empty() {
            bail!("사전 {}행: 일본어<TAB>한국어 형식이 아닙니다", idx + 1);
        }
        if src.contains('\0') || dst.contains('\0') { bail!("사전 {}행에 NUL 바이트 포함",idx+1); }
        entries.push((src.to_owned(), dst.to_owned()));
    }
    Ok(entries)
}

/// Reverse-engineered fixed-size UserDict.jk *sample record*:
/// flag 1B, Japanese Shift-JIS 31B (offset 1), Korean CP949 31B
/// (offset 32), and trailing 47B metadata of unknown semantics.
/// Only active records (status=1) with valid text are loaded.
/// No assumptions are made about the unknown metadata fields.
pub fn read_fixed_jk(data: &[u8]) -> Result<Vec<(String, String)>> {
    const SIZE: usize = 110;
    if data.len() % SIZE != 0 {
        bail!("UserDict.jk는 110바이트 레코드 단위여야 합니다 ({}바이트)",data.len());
    }
    let mut entries = Vec::new();
    for (idx, rec) in data.chunks_exact(SIZE).enumerate() {
        match rec[0] {
            0 => continue, // presumed inactive; not used
            1 => {},
            status => bail!("UserDict.jk 레코드 {}: 알 수 없는 상태값 {}",idx+1,status),
        }
        let jp_bytes = trim_null(&rec[1..32]);
        let ko_bytes = trim_null(&rec[32..63]);
        if jp_bytes.is_empty() || ko_bytes.is_empty() {continue;}
        let (jp,_,invalid_jp) = SHIFT_JIS.decode(jp_bytes);
        let (ko,_,invalid_ko) = EUC_KR.decode(ko_bytes);
        if invalid_jp || invalid_ko {
            bail!("UserDict.jk 레코드 {} 문자 디코딩 오류",idx+1);
        }
        if !jp.trim().is_empty() && !ko.trim().is_empty() {
            entries.push((jp.trim().to_owned(),ko.trim().to_owned()));
        }
    }
    Ok(entries)
}
fn trim_null(b: &[u8]) -> &[u8] {
    b.split(|&x|x==0).next().unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_binary_record_parser() {
        let mut record=[0u8;110];
        record[0]=1;
        let (jp,_,jp_bad)=SHIFT_JIS.encode("魔法");
        let (ko,_,ko_bad)=EUC_KR.encode("마법");
        assert!(!jp_bad && !ko_bad);
        record[1..1+jp.len()].copy_from_slice(&jp);
        record[32..32+ko.len()].copy_from_slice(&ko);
        let mut data=record.to_vec();
        let mut disabled=record;
        disabled[0]=0;
        data.extend_from_slice(&disabled);
        assert_eq!(read_fixed_jk(&data).unwrap(),vec![("魔法".to_owned(),"마법".to_owned())]);
    }
    #[test]
    fn binary_rejects_malformed_records() {
        assert!(read_fixed_jk(&[1,2,3]).is_err());
        let mut rec=[0;110];rec[0]=17;
        assert!(read_fixed_jk(&rec).is_err());
    }
    #[test]
    fn tsv_overrides_and_validation() {
        let values=read_tsv("#comment\n私\t나#N\n空\t하늘\textra\n").unwrap();
        assert_eq!(values[0],("私".into(),"나".into()));
        assert_eq!(values[1],("空".into(),"하늘".into()));
        assert!(read_tsv("invalid-no-tab\n").is_err());
    }
}