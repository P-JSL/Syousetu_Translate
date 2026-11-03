mod config;mod engine;mod filter;mod network;mod options;mod output;mod session;mod site;
use anyhow::{bail,Context,Result};
use std::{env,fs,path::{Path,PathBuf}};
use engine::{EngineConfig,EngineMode,create_engine};
use config::Config;
use session::Session;
use syousetu_translate::ehnd::{Ehnd, Phase};
use syousetu_translate::ehnd_regex::Executor as EhndRegexExecutor;
struct EhndRegexComparison {inner:Box<dyn engine::Translator>,executor:EhndRegexExecutor}
impl engine::Translator for EhndRegexComparison {
    fn translate(&mut self,text:&str)->Result<String> {
        let pre=self.executor.apply(Phase::Pre,text)?;
        let middle=self.inner.translate(&pre)?;
        self.executor.apply(Phase::Post,&middle)
    }
}

struct EhndLiteralComparison { inner:Box<dyn engine::Translator>, filters:Ehnd }
impl engine::Translator for EhndLiteralComparison {
    fn translate(&mut self,text:&str)->Result<String> {
        let pre=self.filters.apply_literal(Phase::Pre,text)?;
        let translated=self.inner.translate(&pre)?;
        self.filters.apply_literal(Phase::Post,&translated)
    }
}

use filter::{Dictionary,Pipeline};
use network::Fetcher;
use site::{canonicalize,parse_work_pages,parse_episode_with_comments,index_page_urls,Site,Work};

fn value(args:&[String],key:&str)->Option<String>{args.windows(2).find(|x|x[0]==key).map(|x|x[1].clone())}
fn flag(args:&[String],key:&str)->bool{args.iter().any(|x|x==key)}
fn num(args:&[String],key:&str,default:usize)->Result<usize>{
    value(args,key).map(|v|v.parse::<usize>().context(format!("잘못된 숫자: {key}"))).unwrap_or(Ok(default))
}
fn engine_from_args(args:&[String],config:&Config)->Result<Box<dyn engine::Translator>>{
    let dll_path=value(args,"--eztrans-dir").or_else(||config.eztrans_path.clone()).map(PathBuf::from);
    let engine=match value(args,"--engine").as_deref() {
        Some("native")=>EngineMode::Native,
        Some("eztrans")=>EngineMode::EzTrans,
        Some("passthrough")=>EngineMode::Passthrough,
        Some(name)=>bail!("잘못된 --engine: {name}; native|eztrans|passthrough"),
        None if flag(args,"--passthrough")=>EngineMode::Passthrough,
        None if value(args,"--eztrans-dir").is_some()=>EngineMode::EzTrans,
        None=>match config.translator_name.as_str(){"eztrans"=>EngineMode::EzTrans,"passthrough"=>EngineMode::Passthrough,_=>EngineMode::Native},
    };
    if flag(args,"--passthrough") && engine != EngineMode::Passthrough {
        bail!("--passthrough와 --engine 값이 충돌합니다");
    }
    if engine!=EngineMode::Native&&(flag(args,"--native-strict")||value(args,"--native-dictionary").is_some()){
        bail!("--native-strict와 --native-dictionary는 --engine native에서만 사용할 수 있습니다");
    }
    if flag(args,"--ehnd-regex-strict") && !flag(args,"--ehnd-regex-experimental") {
        bail!("--ehnd-regex-strict는 --ehnd-regex-experimental과 함께 사용해야 합니다");
    }
    let translator=create_engine(&EngineConfig {
        mode:engine,
        eztrans_dir:dll_path,
        native_dictionary:value(args,"--native-dictionary").map(PathBuf::from),
        strict_native:flag(args,"--native-strict"),
    })?;
    if flag(args,"--ehnd-regex-experimental") {
        if flag(args,"--ehnd-literal-only") {
            bail!("--ehnd-regex-experimental과 --ehnd-literal-only는 동시 지정 불가");
        }
        if engine!=EngineMode::Native {
            bail!("EHND 실험용 필터는 --engine native에서만 사용 가능합니다");
        }
        let filters=Ehnd::load(Path::new(&require(args,"--ehnd-dir")?))?;
        let executor=EhndRegexExecutor::compile(&filters,flag(args,"--ehnd-regex-strict"))?;
        eprintln!("WARNING: 원본 EHND와 다르게 동작할 수 있는 실험 모드입니다. literal {}, regex {}, 컴파일 불가 {}, 미지원 directive {}, 미분류 {}",
            executor.audit.literals,executor.audit.regexes,
            executor.audit.rejected,executor.audit.directives,executor.audit.unknown);
        return Ok(Box::new(EhndRegexComparison{inner:translator,executor}));
    }
    if flag(args,"--ehnd-literal-only") {
        if engine!=EngineMode::Native {bail!("EHND 리터럴 비교는 --engine native에서만 허용됩니다");}
        let dir=require(args,"--ehnd-dir")?;
        let filters=Ehnd::load(Path::new(&dir))?;
        eprintln!("WARNING: EHND 리터럴 {}개만 적용하며 regex·지시문 {}개는 실행하지 않습니다. 원본 번역 재현이 아닙니다",
            filters.counts.literals,filters.counts.regexes+filters.counts.directives+filters.counts.unknown);
        return Ok(Box::new(EhndLiteralComparison{inner:translator,filters}));
    }
    if flag(args,"--ehnd-dir") {bail!("--ehnd-dir에는 명시적인 --ehnd-literal-only가 필요합니다");}
    Ok(translator)
}
fn pipeline_from_args(args:&[String],config:&Config)->Result<Pipeline>{
    let default=Path::new("config/dictionary.txt");
    let passthrough=flag(args,"--passthrough")||value(args,"--engine").as_deref()==Some("passthrough")||(value(args,"--engine").is_none()&&value(args,"--eztrans-dir").is_none()&&config.translator_name=="passthrough");
    let dict=match value(args,"--dictionary"){Some(file)=>Dictionary::from_file(Path::new(&file))?,None if default.is_file()&&!passthrough=>Dictionary::from_file(default)?,None=>Dictionary::empty()};
    let legacy=value(args,"--engine").as_deref()==Some("eztrans")||(value(args,"--engine").is_none()&&!flag(args,"--passthrough")&&(value(args,"--eztrans-dir").is_some()||config.translator_name=="eztrans"));
    Ok(if legacy{Pipeline::legacy(dict)}else{Pipeline::new(dict)})
}
fn local_session(args:&[String],config:&Config)->Result<Session>{Ok(Session::local(pipeline_from_args(args,config)?,engine_from_args(args,config)?))}
fn translation_session(args:&[String],config:&Config)->Result<Session>{if config.workers()>1{Session::parallel(args,config.workers())}else{local_session(args,config)}}
fn require(args:&[String],key:&str)->Result<String>{value(args,key).context(format!("필수 옵션 {key} 누락"))}
fn fetcher_for(args:&[String],config:&Config)->Result<Fetcher>{
    let(site,_)=canonicalize(&require(args,"--url")?)?;
    let delay=if site==Site::Hameln{config.hameln_page_download_delay}else{config.page_download_delay};
    Fetcher::with_options(delay,config.max_retry_count,&config.user_agent)
}
fn load_work(args:&[String],fetcher:&Fetcher)->Result<Work>{
    use std::collections::{BTreeMap,HashSet};
    let url=require(args,"--url")?;
    let first=match value(args,"--html"){Some(path)=>fs::read_to_string(&path).with_context(||format!("목차 HTML 읽기 실패: {path}"))?,None=>fetcher.get(&canonicalize(&url)?.1)?};
    let mut pages=vec![first];let mut pending=BTreeMap::new();let mut seen=HashSet::new();
    loop{
        for link in index_page_urls(&url,pages.last().unwrap())?{
            if !seen.insert(link.clone()){continue;}
            let parsed=url::Url::parse(&link)?;let number=parsed.query_pairs().find(|(key,_)|key=="p").context("목차 페이지 번호 없음")?.1.parse::<usize>()?;
            if !(2..=1000).contains(&number){bail!("목차 페이지 범위 초과: {number}");}pending.insert(number,link);
        }
        let Some((number,link))=pending.pop_first()else{break;};
        let html=if let Some(directory)=value(args,"--index-html-dir"){
            let path=Path::new(&directory).join(format!("{number}.html"));fs::read_to_string(&path).with_context(||format!("목차 {number}페이지 HTML 없음: {}",path.display()))?
        }else if value(args,"--html").is_some(){bail!("추가 목차가 필요합니다. --index-html-dir에 2.html, 3.html 등을 제공하세요");}
        else{if !flag(args,"--no-delay"){fetcher.rest();}fetcher.get(&link)?};pages.push(html);
    }
    if pages.len()==1{site::parse_work(&url,&pages[0])}else{parse_work_pages(&url,&pages)}
}
fn inspect(args:&[String])->Result<()> {
    let config=options::settings(args)?;
    let work=load_work(args,&fetcher_for(args,&config)?)?;
    println!("{}",serde_json::to_string_pretty(&work)?);
    Ok(())
}
fn download(args:&[String])->Result<()> {
    let config=options::settings(args)?;
    let fetcher=fetcher_for(args,&config)?;
    let work=load_work(args,&fetcher)?;
    let first=num(args,"--start",1)?;
    if first==0 || first>work.episodes.len(){bail!("--start가 작품 화수 범위를 벗어났습니다");}
    let end=num(args,"--end",work.episodes.len())?.min(work.episodes.len());
    if first>end{bail!("--end가 --start보다 작습니다");}
    let mut session=translation_session(args,&config)?;
    let output=if let Some(path)=value(args,"--out"){PathBuf::from(path)}else if flag(args,"--auto-output")||value(args,"--output-dir").is_some()||config.save_without_file_chooser{
        let directory=PathBuf::from(value(args,"--output-dir").unwrap_or_else(||".".into()));directory.join(output::suggested_filename(&session.translate(&work.title)?,&config)?)
    }else{bail!("필수 옵션 --out 누락; 자동 이름은 --auto-output 또는 --output-dir을 지정하세요");};
    let mut out=output::format_header(&mut session,&work)?;
    for i in first..=end {
        let file=value(args,"--episode-html-dir")
            .map(|dir|Path::new(&dir).join(format!("{i}.html")));
        let page=match file {Some(file)=>fs::read_to_string(&file).with_context(||format!("에피소드 HTML 없음: {}",file.display()))?,
            None=>{if !flag(args,"--no-delay"){fetcher.rest();}fetcher.get(&work.episodes[i-1])?}};
        let episode=parse_episode_with_comments(work.site,&page,config.include_furigana,config.include_comment,&config.comment_separator)
            .with_context(||format!("에피소드 {i} 파싱 실패: {}",work.episodes[i-1]))?;
        out.push_str(&output::format_episode(&mut session,&episode,i,end,&config)?);
        eprintln!("[{i}/{end}] 저장 대기: {}",episode.title);
    }
    output::atomic_write(&output,&out)?;
    println!("Saved {} episodes to {}",end-first+1,output.display());
    Ok(())
}
fn file_translate(args:&[String])->Result<()> {
    let config=options::settings(args)?;
    let input=PathBuf::from(require(args,"--input")?);
    let output=PathBuf::from(require(args,"--out")?);
    if input==output||output.canonicalize().ok().is_some_and(|path|input.canonicalize().ok()==Some(path)){bail!("입력 파일에 직접 덮어쓸 수 없습니다. 다른 출력 경로를 지정하세요");}
    let bytes=fs::read(&input)?;
    let contents=if let Some(name)=value(args,"--input-encoding"){
        if name.eq_ignore_ascii_case("latin1")||name.eq_ignore_ascii_case("iso-8859-1"){bytes.iter().map(|&byte|char::from(byte)).collect::<String>()}
        else{let encoding=encoding_rs::Encoding::for_label(name.as_bytes()).context("지원하지 않는 --input-encoding")?;let(text,_,bad)=encoding.decode(&bytes);if bad{bail!("입력 파일 디코딩 실패: {name}");}text.into_owned()}
    }else if bytes.starts_with(&[0xff,0xfe]) {
        let (v,_,invalid)=encoding_rs::UTF_16LE.decode(&bytes[2..]);if invalid{bail!("UTF-16LE 디코딩 오류");}v.into_owned()
    } else if bytes.starts_with(&[0xfe,0xff]) {
        let (v,_,invalid)=encoding_rs::UTF_16BE.decode(&bytes[2..]);if invalid{bail!("UTF-16BE 디코딩 오류");}v.into_owned()
    } else {String::from_utf8(bytes).context("UTF-8/UTF-16(BOM)이 아니면 --input-encoding을 지정하세요")?};
    let contents=contents.trim_start_matches('\u{feff}');
    let mut session=translation_session(args,&config)?;
    let mut out=String::with_capacity(contents.len());
    let lines=contents.split_inclusive('\n').collect::<Vec<_>>();
    let raw_lines=lines.iter().map(|line|line.trim_end_matches('\n').trim_end_matches('\r').to_owned()).collect::<Vec<_>>();
    let translated=session.translate_lines(&raw_lines)?;
    for(line,translation)in lines.into_iter().zip(translated){
        let raw=line.trim_end_matches('\n').trim_end_matches('\r');
        if config.print_japanese{out.push_str(raw);out.push('\n');}
        out.push_str(&translation);
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
  inspect-ehnd --dir PATH [--out ehnd-report.json]
  audit-ehnd-regex --dir PATH [--out audit.json] [--strict]
  audit-ehnd-control --dir PATH [--out control-audit.json]
  Experimental: --ehnd-dir PATH --ehnd-regex-experimental [--ehnd-regex-strict]
  compare-engines --eztrans-dir PATH --input corpus.txt [--out comparison.json]
  Experimental: --ehnd-dir PATH --ehnd-literal-only (native only)
  download --url URL --out novel.txt [--start N --end N]
  file --input novel.txt --out translated.txt
  inspect-config [--config config/config.properties]

  Engine: --engine native|eztrans|passthrough (default: native)
  Native: --native-dictionary PATH [--native-strict]
  Legacy: --engine eztrans [--eztrans-dir PATH]
  Common: --config PATH, --dictionary PATH, --japanese, --no-japanese, --jobs 1..16
  File: --input-encoding shift_jis|euc-kr|utf-8|utf-16le|latin1
  Download: --furigana --include-comments --comment-separator TEXT
  Output: --title-format TEXT --page-number-format 000 --output-dir DIR --auto-output
  Network: --delay-ms 1200 --retry 2 --user-agent TEXT
  Offline index pagination: --html first.html --index-html-dir DIR (2.html, 3.html, ...)

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
fn inspect_ehnd(args:&[String])->Result<()> {
    let directory=PathBuf::from(require(args,"--dir")?);
    let rules=Ehnd::load(&directory)?;
    let json=serde_json::to_string_pretty(&rules.report())?;
    if let Some(path)=value(args,"--out"){
        output::atomic_write(Path::new(&path),&json)?;
    } else { println!("{json}"); }
    Ok(())
}
/// Local differential harness: compare user-installed original EHND+DLX
/// against native Rust. No source sentences are uploaded to GitHub.
fn compare_engines(args:&[String])->Result<()> {
    let install=PathBuf::from(require(args,"--eztrans-dir")?);
    let input=PathBuf::from(require(args,"--input")?);
    let text=fs::read_to_string(&input).context("비교 문장 파일은 UTF-8이어야 합니다")?;
    let lines=text.lines().map(str::trim).filter(|x|!x.is_empty()&&!x.starts_with('#'))
        .take(2000).collect::<Vec<_>>();
    if lines.is_empty(){bail!("비교할 문장이 없습니다");}
    // The real library is only loadable on 32-bit Windows.
    let mut reference=create_engine(&EngineConfig{
        mode:EngineMode::EzTrans,
        eztrans_dir:Some(install),
        native_dictionary:None,
        strict_native:false,
    })?;
    let mut native=create_engine(&EngineConfig{
        mode:EngineMode::Native,
        eztrans_dir:None,
        native_dictionary:value(args,"--native-dictionary").map(PathBuf::from),
        strict_native:false,
    })?;
    let mut examples=Vec::new();
    let mut matched=0usize;
    for (index,&source) in lines.iter().enumerate() {
        let reference_text=reference.translate(source)
            .with_context(||format!("원본 DLL 비교 번역 실패: {}번 문장",index+1))?;
        let native_text=native.translate(source)
            .with_context(||format!("Rust 비교 번역 실패: {}번 문장",index+1))?;
        if reference_text==native_text {matched+=1;} else if examples.len()<40 {
            examples.push(serde_json::json!({
                "line":index+1,
                "japanese":source,
                "reference":reference_text,
                "native":native_text
            }));
        }
    }
    let report=serde_json::json!({
        "reference":"user-installed top-level EHND J2KEngine.dll + J2KEngine.dlx",
        "native":"independent Rust glossary engine",
        "compared":lines.len(),
        "exact_matches":matched,
        "exact_match_rate":matched as f64/lines.len() as f64,
        "all_semantics_verified":false,
        "mismatch_examples":examples,
    });
    let serialized=serde_json::to_string_pretty(&report)?;
    if let Some(output)=value(args,"--out") {
        output::atomic_write(Path::new(&output),&serialized)?;
    }else{println!("{serialized}");}
    Ok(())
}
fn audit_ehnd_regex(args:&[String])->Result<()> {
    let filters=Ehnd::load(Path::new(&require(args,"--dir")?))?;
    let executor=EhndRegexExecutor::compile(&filters,flag(args,"--strict"))?;
    let serialized=serde_json::to_string_pretty(&executor.audit)?;
    if let Some(target)=value(args,"--out"){
        output::atomic_write(Path::new(&target),&serialized)?;
    }else{println!("{serialized}");}
    Ok(())
}
fn audit_ehnd_control(args:&[String])->Result<()> {
    let filters=Ehnd::load(Path::new(&require(args,"--dir")?))?;
    let result=syousetu_translate::ehnd_control::audit(&filters);
    let serialized=serde_json::to_string_pretty(&result)?;
    if let Some(path)=value(args,"--out") {
        output::atomic_write(Path::new(&path),&serialized)?;
    }else{println!("{serialized}");}
    Ok(())
}
fn run()->Result<()> {
    let args:Vec<String>=env::args().collect();
    if args.len()==1||args.iter().skip(1).any(|arg|arg=="--help"||arg=="-h")||args.get(1).map(String::as_str)==Some("help"){help();return Ok(());}
    options::validate(&args[1],&args[2..])?;
    match args.get(1).map(String::as_str) {
        Some("worker")=>{let config=options::settings(&args[2..])?;session::serve(local_session(&args[2..],&config)?)},
        Some("inspect-config")=>{println!("{}",serde_json::to_string_pretty(&options::settings(&args[2..])?)?);Ok(())},
        Some("inspect")=>inspect(&args[2..]),
        Some("inspect-dat")=>inspect_dat(&args[2..]),
        Some("inspect-root-dll")=>inspect_root_dll(&args[2..]),
        Some("inspect-ehnd")=>inspect_ehnd(&args[2..]),
        Some("audit-ehnd-regex")=>audit_ehnd_regex(&args[2..]),
        Some("audit-ehnd-control")=>audit_ehnd_control(&args[2..]),
        Some("compare-engines")=>compare_engines(&args[2..]),
        Some("download")=>download(&args[2..]),
        Some("file")=>file_translate(&args[2..]),
        _=>bail!("알 수 없는 명령"),
    }
}
fn main(){if let Err(e)=run(){eprintln!("ERROR: {e:#}");std::process::exit(1)}}
