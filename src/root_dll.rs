//! PE32 inspection for the top-level ezTrans XP J2KEngine.dll (EHND wrapper).
//! This inspects an authorized local file; it does not execute or bundle it.
//! Verified sample is x86, contains 25 exports and loads J2KEngine.dlx.
use anyhow::{bail,Context,Result};
use serde::Serialize;

const MAX_EXPORTS: usize = 4096;
const ROOT_EXPORTS: &[&str] = &[
    "?GetJ2KMainDir",
    "J2K_FreeMem","J2K_GetJ2KMainDir","J2K_GetPriorDict","J2K_GetProperty",
    "J2K_Initialize","J2K_InitializeEx","J2K_ReloadUserDict",
    "J2K_SetDelJPN","J2K_SetField","J2K_SetHnj2han","J2K_SetJWin",
    "J2K_SetPriorDict","J2K_SetProperty","J2K_StopTranslation",
    "J2K_Terminate","J2K_TranslateChat","J2K_TranslateFM",
    "J2K_TranslateMM","J2K_TranslateMMEx","J2K_TranslateMMNT",
    "J2K_TranslateMMNTW","msvcrt_fopen","msvcrt_free","msvcrt_malloc",
];

#[derive(Debug,Clone,Serialize)]
pub struct Export {
    pub name:String,
    pub ordinal:u32,
    pub rva:u32,
    pub forwarded:bool,
}
#[derive(Debug,Serialize)]
pub struct RootDllReport {
    pub declared_name:String,
    pub machine:u16,
    pub pe32:bool,
    pub exports:Vec<Export>,
    pub matching_known_root_exports:bool,
    pub references_j2kengine_dlx:bool,
    pub notes:Vec<&'static str>,
}
fn bytes(data:&[u8],offset:usize,len:usize)->Result<&[u8]>{
    data.get(offset..offset.checked_add(len).context("PE offset overflow")?)
        .context("PE file truncated")
}
fn u16le(data:&[u8],offset:usize)->Result<u16>{
    Ok(u16::from_le_bytes(bytes(data,offset,2)?.try_into().unwrap()))
}
fn u32le(data:&[u8],offset:usize)->Result<u32>{
    Ok(u32::from_le_bytes(bytes(data,offset,4)?.try_into().unwrap()))
}
struct Section {virtual_address:u32,virtual_size:u32,raw_pointer:u32,raw_size:u32}
fn rva_to_offset(data:&[u8],rva:u32,sections:&[Section])->Result<usize>{
    for s in sections {
        let width=s.virtual_size.max(s.raw_size);
        if let Some(relative)=rva.checked_sub(s.virtual_address) {
            if relative<width && relative<s.raw_size {
                let pos=s.raw_pointer.checked_add(relative).context("section raw offset overflow")? as usize;
                bytes(data,pos,1)?;
                return Ok(pos);
            }
        }
    }
    bail!("PE RVA 0x{rva:08x} is outside file-backed sections")
}
fn read_cstr(data:&[u8],start:usize)->Result<String>{
    let remain=data.get(start..).context("string offset outside file")?;
    let len=remain.iter().take(512).position(|&x|x==0).context("unterminated PE export name")?;
    Ok(std::str::from_utf8(&remain[..len]).context("non UTF-8 PE export name")?.to_owned())
}
fn word_offset(data:&[u8],sections:&[Section],base:u32,index:usize,stride:u32)->Result<usize>{
    let delta=(index as u32).checked_mul(stride).context("export index overflow")?;
    rva_to_offset(data,base.checked_add(delta).context("export RVA overflow")?,sections)
}
fn has_utf16le_ascii_case_insensitive(data:&[u8],needle:&str)->bool{
    let pattern:Vec<u8>=needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    data.windows(pattern.len()).any(|chunk|{
        chunk.iter().step_by(2).zip(pattern.iter().step_by(2))
            .all(|(&actual,&expected)|actual.eq_ignore_ascii_case(&expected))
        && chunk.iter().skip(1).step_by(2).all(|&b|b==0)
    })
}

/// Validate the top-level wrapper's identity and the real PE export directory.
/// A matching export table verifies names, *not* original function semantics.
pub fn inspect(data:&[u8])->Result<RootDllReport>{
    if bytes(data,0,2)? != b"MZ" {bail!("not an MZ executable");}
    let pe=u32le(data,0x3c)? as usize;
    if bytes(data,pe,4)?!=b"PE\0\0"{bail!("invalid PE signature");}
    let coff=pe.checked_add(4).context("COFF header overflow")?;
    let machine=u16le(data,coff)?;
    let sections_count=u16le(data,coff+2)? as usize;
    if sections_count==0 || sections_count>96 {bail!("invalid PE section count");}
    let optional_len=u16le(data,coff+16)? as usize;
    let optional=coff.checked_add(20).context("optional header overflow")?;
    let pe32=u16le(data,optional)?==0x10b;
    if !pe32 || machine!=0x14c {bail!("expected 32-bit x86 PE DLL");}
    if optional_len<104 {bail!("PE optional header lacks export directory");}
    let export_rva=u32le(data,optional+96)?;
    let export_size=u32le(data,optional+100)?;
    if export_rva==0 || export_size==0{bail!("PE has no export table");}
    let section_start=optional.checked_add(optional_len).context("section offset overflow")?;
    let mut sections=Vec::with_capacity(sections_count);
    for i in 0..sections_count {
        let offset=section_start.checked_add(i*40).context("section offset overflow")?;
        bytes(data,offset,40)?;
        sections.push(Section{
            virtual_size:u32le(data,offset+8)?,
            virtual_address:u32le(data,offset+12)?,
            raw_size:u32le(data,offset+16)?,
            raw_pointer:u32le(data,offset+20)?,
        });
    }
    let export_dir=rva_to_offset(data,export_rva,&sections)?;
    bytes(data,export_dir,40)?;
    let dll_name=u32le(data,export_dir+12)?;
    let ordinal_base=u32le(data,export_dir+16)?;
    let functions_count=u32le(data,export_dir+20)? as usize;
    let name_count=u32le(data,export_dir+24)? as usize;
    if functions_count>MAX_EXPORTS || name_count>functions_count {
        bail!("implausible PE export count");
    }
    let functions=u32le(data,export_dir+28)?;
    let names=u32le(data,export_dir+32)?;
    let ordinals=u32le(data,export_dir+36)?;
    let declared_name=read_cstr(data,rva_to_offset(data,dll_name,&sections)?)?;
    let mut exports=Vec::with_capacity(name_count);
    for i in 0..name_count {
        let name_rva=u32le(data,word_offset(data,&sections,names,i,4)?)?;
        let name=read_cstr(data,rva_to_offset(data,name_rva,&sections)?)?;
        let index=u16le(data,word_offset(data,&sections,ordinals,i,2)?)? as usize;
        if index>=functions_count{bail!("export ordinal index outside function table");}
        let rva=u32le(data,word_offset(data,&sections,functions,index,4)?)?;
        let forwarded=rva>=export_rva && rva<export_rva.saturating_add(export_size);
        exports.push(Export{
            name,ordinal:ordinal_base+index as u32,rva,forwarded,
        });
    }
    exports.sort_by(|a,b|a.ordinal.cmp(&b.ordinal));
    let matching_known_root_exports=declared_name.eq_ignore_ascii_case("EHND.dll")
        && exports.len()==ROOT_EXPORTS.len()
        && ROOT_EXPORTS.iter().all(|name|exports.iter().any(|e|e.name==*name));
    Ok(RootDllReport{
        declared_name,machine,pe32,exports,matching_known_root_exports,
        references_j2kengine_dlx:has_utf16le_ascii_case_insensitive(data,"J2KEngine.dlx"),
        notes:vec![
            "A valid PE export table does not prove compatible function signatures",
            "The original top-level DLL dynamically loads J2KEngine.dlx",
            "A complete standalone Rust translation model is not reconstructed",
        ],
    })
}
#[cfg(test)]
mod tests{
    use super::*;
    fn put16(buf:&mut [u8],idx:usize,n:u16){buf[idx..idx+2].copy_from_slice(&n.to_le_bytes());}
    fn put32(buf:&mut [u8],idx:usize,n:u32){buf[idx..idx+4].copy_from_slice(&n.to_le_bytes());}
    #[test]
    fn synthetic_pe_export_and_identity(){
        let mut b=vec![0u8;0x800];
        b[..2].copy_from_slice(b"MZ");put32(&mut b,0x3c,0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        put16(&mut b,0x84,0x14c);put16(&mut b,0x86,1);
        put16(&mut b,0x94,0xe0);put16(&mut b,0x98,0x10b);
        put32(&mut b,0xf8,0x1100);put32(&mut b,0xfc,0x80);
        put32(&mut b,0x180,0x400);put32(&mut b,0x184,0x1000);
        put32(&mut b,0x188,0x400);put32(&mut b,0x18c,0x200);
        put32(&mut b,0x30c,0x1140);put32(&mut b,0x310,1);
        put32(&mut b,0x314,1);put32(&mut b,0x318,1);
        put32(&mut b,0x31c,0x1150);put32(&mut b,0x320,0x1154);
        put32(&mut b,0x324,0x1158);
        b[0x340..0x349].copy_from_slice(b"EHND.dll\0");
        put32(&mut b,0x350,0x1200);put32(&mut b,0x354,0x1160);
        put16(&mut b,0x358,0);
        b[0x360..0x371].copy_from_slice(b"J2K_InitializeEx\0");
        let r=inspect(&b).unwrap();
        assert_eq!(r.machine,0x14c);
        assert_eq!(r.exports.len(),1);
        assert_eq!(r.exports[0].ordinal,1);
        assert!(!r.matching_known_root_exports);
        assert!(!r.references_j2kengine_dlx);
        b[0]=0;assert!(inspect(&b).is_err());
    }
    #[test]
    fn rejects_truncated_and_non_pe(){
        assert!(inspect(&[]).is_err());
        assert!(inspect(&[b'M',b'Z']).is_err());
    }
    #[test]
    fn detects_unicode_loader_name(){
        let mut b=Vec::new();b.extend(b"J2KEngine.dlx".iter().flat_map(|ch|[*ch,0]));
        assert!(has_utf16le_ascii_case_insensitive(&b,"j2kengine.dlx"));
    }
}