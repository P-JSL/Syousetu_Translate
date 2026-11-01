mod engine;mod filter;mod network;mod output;mod site;
use anyhow::{bail,Context,Result};
use std::{env,fs,path::{Path,PathBuf}};
use engine::{EngineConfig,EngineMode,create_engine};
use filter::{Dictionary,Pipeline};
use network::Fetcher;
use site::{parse_work,parse_episode,canonicalize};

fn value(args:&[String],key:&str)->Option<String>{args.windows(2).find(|x|x[0]==key).map(|x|x[1].clone())}
fn flag(args:&[String],key:&str)->bool{args.iter().any(|x|x==key)}
fn num(args:&[String],key:&str,default:usize)->Result<usize>{
    value(args,key).map(|v|v.parse::<usize>().context(format!("잘못된 숫자: {key}"))).unwrap_or(Ok(default))
}
fn engine_from_args(args:&[String])->Result<Box<dyn engine::Translator>>{
    let dll_path=value(args,"--eztrans-dir").map(PathBuf::from);
    let engine=match value(args,"--engine").as_deref() {
        Some("native")=>EngineMode::Native,
        Some("eztrans")=>EngineMode::EzTrans,
        Some("passthrough")=>EngineMode::Passthrough,
        Some(name)=>bail!("잘못된 --engine: {name}; native|eztrans|passthrough"),
        None if flag(args,"--passthrough")=>EngineMode::Passthrough,
        None if dll_path.is_some()=>EngineMode::EzTrans,
        None=>EngineMode::Native,
    };
    if flag(args,"--passthrough") && engine != EngineMode::Passthrough {
        bail!("--passthrough와 --engine 값이 충돌합니다");
    }
    create_engine(&EngineConfig {
        mode:engine,
        eztrans_dir:dll_path,
        native_dictionary:value(args,"--native-dictionary").map(PathBuf::from),
        strict_native:flag(args,"--native-strict"),
    })
}
fn pipeline_from_args(args:&[String])->Result<Pipeline>{
    let dict=match value(args,"--dictionary") {Some(file)=>Dictionary::from_file(Path::new(&file))?,None=>Dictionary::empty()};
    Ok(Pipeline::new(dict))
}
fn require(args:&[String],key:&str)->Result<String>{value(args,key).context(format!("필수 옵션 {key} 누락"))}
fn inspect(args:&[String])->Result<()> {
    let url=require(args,"--url")?;
    let html=match value(args,"--html") {Some(path)=>fs::read_to_string(&path).context("HTML 파일 읽기 오류")?,
        None=>Fetcher::new(num(args,"--delay-ms",1000)? as u64,1)?.get(&canonicalize(&url)?.1)?};
    let work=parse_work(&url,&html)?;
    println!("{}",serde_json::to_string_pretty(&work)?);
    Ok(())
}
fn download(args:&[String])->Result<()> {
    let url=require(args,"--url")?;
    let output=PathBuf::from(require(args,"--out")?);
    let fetcher=Fetcher::new(num(args,"--delay-ms",1200)?.max(500) as u64,num(args,"--retry",2)? as u32)?;
    let html=match value(args,"--html") {Some(path)=>fs::read_to_string(path)?,None=>fetcher.get(&canonicalize(&url)?.1)?};
    let work=parse_work(&url,&html)?;
    let first=num(args,"--start",1)?;
    if first==0 || first>work.episodes.len(){bail!("--start가 작품 화수 범위를 벗어났습니다");}
    let end=num(args,"--end",work.episodes.len())?.min(work.episodes.len());
    if first>end{bail!("--end가 --start보다 작습니다");}
    let pipeline=pipeline_from_args(args)?;
    let mut translator=engine_from_args(args)?;
    let mut out=output::format_header(&pipeline,&mut *translator,&work)?;
    for i in first..=end {
        let file=value(args,"--episode-html-dir")
            .map(|dir|Path::new(&dir).join(format!("{i}.html")));
        let page=match file {Some(file)=>fs::read_to_string(&file).with_context(||format!("에피소드 HTML 없음: {}",file.display()))?,
            None=>fetcher.get(&work.episodes[i-1])?};
        let episode=parse_episode(work.site,&page,flag(args,"--furigana"))
            .with_context(||format!("에피소드 {i} 파싱 실패: {}",work.episodes[i-1]))?;
        out.push_str(&output::format_episode(&pipeline,&mut *translator,&episode,i,work.episodes.len(),flag(args,"--japanese"))?);
        eprintln!("[{i}/{end}] 저장 대기: {}",episode.title);
        if i!=end && !flag(args,"--no-delay"){fetcher.rest();}
    }
    output::atomic_write(&output,&out)?;
    println!("Saved {} episodes to {}",end-first+1,output.display());
    Ok(())
}
fn file_translate(args:&[String])->Result<()> {
    let input=PathBuf::from(require(args,"--input")?);
    let output=PathBuf::from(require(args,"--out")?);
    if input==output{bail!("입력 파일에 직접 덮어쓸 수 없습니다. 다른 출력 경로를 지정하세요");}
    let bytes=fs::read(&input)?;
    let contents=if bytes.starts_with(&[0xff,0xfe]) {
        let (v,_,invalid)=encoding_rs::UTF_16LE.decode(&bytes[2..]);if invalid{bail!("UTF-16LE 디코딩 오류");}v.into_owned()
    } else if bytes.starts_with(&[0xfe,0xff]) {
        let (v,_,invalid)=encoding_rs::UTF_16BE.decode(&bytes[2..]);if invalid{bail!("UTF-16BE 디코딩 오류");}v.into_owned()
    } else {String::from_utf8(bytes).context("입력 인코딩은 UTF-8 또는 UTF-16(BOM)만 지원")?};
    let pipeline=pipeline_from_args(args)?;
    let mut translator=engine_from_args(args)?;
    let mut out=String::with_capacity(contents.len());
    for line in contents.split_inclusive('\n') {
        let raw=line.trim_end_matches('\n').trim_end_matches('\r');
        if flag(args,"--japanese") && !raw.is_empty(){out.push_str(raw);out.push('\n');}
        out.push_str(&pipeline.translate(&mut *translator,raw)?);
        if line.ends_with('\n'){out.push('\n');}
    }
    output::atomic_write(&output,&out)?;
    println!("Saved {}",output.display());
    Ok(())
}
fn help() {
    println!("{}", r#"Syousetu Translate (Rust)
  inspect --url URL [--html saved.html]
  inspect-dat --dir PATH [--out inspection.json]
  inspect-root-dll --dll PATH [--out root-report.json]
  download --url URL --out novel.txt [--start N --end N]
  file --input novel.txt --out translated.txt

  Engine: --engine native|eztrans|passthrough (default: native)
  Native: --native-dictionary PATH [--native-strict]
  Legacy: --engine eztrans [--eztrans-dir PATH]
  Common: --dictionary PATH, --japanese, --passthrough
  Download: --furigana --delay-ms 1200 --retry 2

  Native mode is limited glossary translation and is not identical to ezTrans.
  --passthrough is only for parser tests, not Japanese-to-Korean translation.
  Do not redistribute proprietary vendor assets without permission."#);
}
fn inspect_dat(args:&[String])->Result<()> {
    let directory=PathBuf::from(require(args,"--dir")?);
    let report=syousetu_translate::legacy_dat::inspect(&directory)?;
    let json=serde_json::to_string_pretty(&report)?;
    if let Some(path)=value(args,"--out") {
        output::atomic_write(Path::new(&path),&json)?;
    } else {println!("{json}");}
    Ok(())
}
fn inspect_root_dll(args:&[String])->Result<()> {
    let path=PathBuf::from(require(args,"--dll")?);
    let bytes=fs::read(&path).with_context(||format!("DLL 파일 읽기 실패: {}",path.display()))?;
    let report=syousetu_translate::root_dll::inspect(&bytes)?;
    if !report.matching_known_root_exports {
        bail!("지정된 파일은 최상위 EHND/J2KEngine.dll의 검증된 25개 export와 일치하지 않습니다");
    }
    let json=serde_json::to_string_pretty(&report)?;
    if let Some(output)=value(args,"--out") {
        output::atomic_write(Path::new(&output),&json)?;
    } else {
        println!("{json}");
    }
    Ok(())
}
fn run()->Result<()> {
    let args:Vec<String>=env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("inspect")=>inspect(&args[2..]),
        Some("inspect-dat")=>inspect_dat(&args[2..]),
        Some("inspect-root-dll")=>inspect_root_dll(&args[2..]),
        Some("download")=>download(&args[2..]),
        Some("file")=>file_translate(&args[2..]),
        _=>{help();Ok(())}
    }
}
fn main(){if let Err(e)=run(){eprintln!("ERROR: {e:#}");std::process::exit(1)}}
