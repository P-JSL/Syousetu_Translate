//! Lossless, read-only structural readers for legacy ezTrans data files.
//! The bytecode grammar and lexicon semantics remain unknown. These parsers
//! MUST NOT be represented as equivalent to the proprietary engine.
use anyhow::{bail,Context,Result};
use serde::Serialize;
use std::{fs,ops::Range,path::Path};
const RECORD_BYTES:usize=128;
const PAGE_BYTES:usize=1024;
const SIGNATURE:&[u8]=b"ChangShinSoft Co. 1998 Cache of MainDic (C)";
fn u32le(data:&[u8],pos:usize)->Result<usize>{
    let bytes:[u8;4]=data.get(pos..pos.checked_add(4).context("offset overflow")?)
        .context("truncated 32-bit field")?.try_into().unwrap();
    Ok(u32::from_le_bytes(bytes) as usize)
}
pub struct JisJk<'a>{data:&'a [u8]}
impl<'a> JisJk<'a>{
    pub fn parse(data:&'a[u8])->Result<Self>{
        if data.is_empty()||data.len()%RECORD_BYTES!=0 {
            bail!("JisJK.Inf must consist of 128-byte records");
        } Ok(Self{data})
    }
    pub fn count(&self)->usize{self.data.len()/RECORD_BYTES}
    pub fn record(&self,i:usize)->Option<&'a[u8]> {
        let start=i.checked_mul(RECORD_BYTES)?;
        self.data.get(start..start.checked_add(RECORD_BYTES)?)
    }
}
pub struct RuleKj<'a>{data:&'a[u8],count:usize}
impl<'a> RuleKj<'a>{
    pub fn parse(data:&'a[u8])->Result<Self>{
        let count=u32le(data,0)?;
        let total=count.checked_mul(PAGE_BYTES).and_then(|x|x.checked_add(4))
            .context("RuleKJ count overflow")?;
        if data.len()!=total{bail!("RuleKJ page count mismatch: expected {total}, found {}",data.len());}
        Ok(Self{data,count})
    }
    pub fn count(&self)->usize{self.count}
    pub fn page(&self,i:usize)->Option<&'a[u8]>{
        if i>=self.count{return None;}
        let start=4usize.checked_add(i.checked_mul(PAGE_BYTES)?)?;
        self.data.get(start..start.checked_add(PAGE_BYTES)?)
    }
}
pub struct CRuleKj<'a>{data:&'a[u8],spans:Vec<Range<usize>>}
impl<'a> CRuleKj<'a>{
    pub fn parse(data:&'a[u8])->Result<Self>{
        let count=u32le(data,0)?;
        if count>data.len().saturating_sub(4)/4 {
            bail!("CRuleKJ record count exceeds available headers");
        }
        let mut spans=Vec::with_capacity(count);
        let mut pos=4usize;
        for i in 0..count {
            let length=u32le(data,pos).with_context(||format!("CRuleKJ record {i}"))?;
            pos=pos.checked_add(4).context("CRuleKJ offset overflow")?;
            let end=pos.checked_add(length).context("CRuleKJ length overflow")?;
            if end>data.len(){bail!("CRuleKJ record {i} exceeds EOF");}
            spans.push(pos..end);
            pos=end;
        }
        if pos!=data.len(){bail!("CRuleKJ trailing data: {}",data.len()-pos);}
        Ok(Self{data,spans})
    }
    pub fn count(&self)->usize{self.spans.len()}
    pub fn record(&self,i:usize)->Option<&'a[u8]>{
        self.data.get(self.spans.get(i)?.clone())
    }
}
pub struct YongUn<'a>{data:&'a[u8],marker_offset:usize}
impl<'a> YongUn<'a>{
    pub fn parse(data:&'a[u8])->Result<Self>{
        if !data.starts_with(SIGNATURE){bail!("YongUn invalid signature");}
        if data.get(SIGNATURE.len())!=Some(&0x1a) {
            bail!("YongUn lacks observed header separator");
        }
        let marker_offset=data.windows(4).take(4096)
            .position(|x|x==b"KD2G").context("YongUn missing KD2G marker")?;
        Ok(Self{data,marker_offset})
    }
    pub fn marker_offset(&self)->usize{self.marker_offset}
    pub fn bytes(&self)->&'a[u8]{self.data}
}
#[derive(Debug,Serialize)]
pub struct Asset {
    pub filename:String,
    pub byte_count:usize,
    pub record_count:Option<usize>,
    pub record_bytes:Option<usize>,
    pub marker_offset:Option<usize>,
    pub semantics_decoded:bool,
}
#[derive(Debug,Serialize)]
pub struct Inspection {
    pub assets:Vec<Asset>,
    pub binary_layout_verified:bool,
    pub grammar_equivalence_verified:bool,
    pub warnings:Vec<&'static str>,
}
pub fn inspect(dir:&Path)->Result<Inspection>{
    let data_dir=if dir.join("Dat").is_dir(){dir.join("Dat")}else{dir.to_path_buf()};
    let mut assets=Vec::new();
    let path=data_dir.join("JisJK.Inf");
    let data=fs::read(&path).with_context(||format!("open {}",path.display()))?;
    let table=JisJk::parse(&data)?;
    assets.push(Asset{filename:"JisJK.Inf".into(),byte_count:data.len(),
        record_count:Some(table.count()),record_bytes:Some(RECORD_BYTES),
        marker_offset:None,semantics_decoded:false});
    let path=data_dir.join("RuleKJ.Dat");
    let data=fs::read(&path).with_context(||format!("open {}",path.display()))?;
    let table=RuleKj::parse(&data)?;
    assets.push(Asset{filename:"RuleKJ.Dat".into(),byte_count:data.len(),
        record_count:Some(table.count()),record_bytes:Some(PAGE_BYTES),
        marker_offset:None,semantics_decoded:false});
    let path=data_dir.join("CRuleKJ.Dat");
    let data=fs::read(&path).with_context(||format!("open {}",path.display()))?;
    let table=CRuleKj::parse(&data)?;
    assets.push(Asset{filename:"CRuleKJ.Dat".into(),byte_count:data.len(),
        record_count:Some(table.count()),record_bytes:None,
        marker_offset:None,semantics_decoded:false});
    let path=data_dir.join("YongUn.Dat");
    let data=fs::read(&path).with_context(||format!("open {}",path.display()))?;
    let table=YongUn::parse(&data)?;
    assets.push(Asset{filename:"YongUn.Dat".into(),byte_count:data.len(),
        record_count:None,record_bytes:None,marker_offset:Some(table.marker_offset()),
        semantics_decoded:false});
    Ok(Inspection{
        assets,binary_layout_verified:true,grammar_equivalence_verified:false,
        warnings:vec![
            "This reader preserves binary content without decoding proprietary grammar semantics",
            "JisJK 128-byte alignment is known; its field offsets are not",
            "YongUn cache contents and cross-references are not decoded",
            "CRuleKJ and RuleKJ have different record counts; index parity is not established",
        ],
    })
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn jisjk_alignment(){
        let mut b=vec![0u8;256];b[128]=42;
        let j=JisJk::parse(&b).unwrap();assert_eq!(j.count(),2);
        assert_eq!(j.record(1).unwrap()[0],42);
        assert!(j.record(2).is_none());
        assert!(JisJk::parse(&b[..255]).is_err());
    }
    #[test]fn fixed_rule_pages(){
        let mut b=2u32.to_le_bytes().to_vec();
        b.extend(vec![1;1024]);b.extend(vec![2;1024]);
        let r=RuleKj::parse(&b).unwrap();assert_eq!(r.count(),2);
        assert_eq!(r.page(1).unwrap()[0],2);
        assert!(r.page(2).is_none());
        assert!(RuleKj::parse(&b[..b.len()-1]).is_err());
    }
    #[test]fn compact_rule_roundtrip(){
        let mut b=2u32.to_le_bytes().to_vec();
        for value in [&b"abc"[..],&b"qwerty"[..]]{
            b.extend((value.len() as u32).to_le_bytes());b.extend(value);
        }
        let r=CRuleKj::parse(&b).unwrap();assert_eq!(r.count(),2);
        assert_eq!(r.record(1),Some(&b"qwerty"[..]));
        assert!(CRuleKj::parse(&b[..b.len()-1]).is_err());
        b.push(0);assert!(CRuleKj::parse(&b).is_err());
    }
    #[test]fn cache_header(){
        let mut b=SIGNATURE.to_vec();b.push(0x1a);b.resize(138,0);
        b.extend(b"KD2G");b.resize(1024,0);
        assert_eq!(YongUn::parse(&b).unwrap().marker_offset(),138);
        b[0]=0;assert!(YongUn::parse(&b).is_err());
    }
}