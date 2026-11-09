//! Decode supported files once; callers keep line/paragraph policy.
use anyhow::{bail, Context, Result};
pub fn decode_text(bytes: Vec<u8>, encoding: Option<&str>) -> Result<String> {
    if let Some(name) = encoding {
        if name.eq_ignore_ascii_case("latin1") || name.eq_ignore_ascii_case("iso-8859-1") {
            return Ok(bytes.iter().map(|&b| char::from(b)).collect());
        }
        let codec = encoding_rs::Encoding::for_label(name.as_bytes())
            .context("지원하지 않는 --input-encoding")?;
        let (text, _, bad) = codec.decode(&bytes);
        if bad {
            bail!("입력 파일 디코딩 실패: {name}");
        }
        return Ok(text.into_owned());
    }
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let codec = if bytes[0] == 0xff {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let (text, _, bad) = codec.decode(&bytes[2..]);
        anyhow::ensure!(!bad, "UTF-16 디코딩 오류");
        Ok(text.into_owned())
    } else {
        String::from_utf8(bytes).context("UTF-8/UTF-16(BOM)이 아니면 --input-encoding을 지정하세요")
    }
}
