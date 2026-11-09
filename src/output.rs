use crate::{config::{Config,format_number},session::Session,site::{Episode,Work}};
use anyhow::Result;
pub use syousetu_translate::storage::atomic_write;

pub fn format_episode(session:&mut Session, episode:&Episode,
                      number:usize,total:usize,config:&Config)->Result<String>{
    let mut output=String::new();
    let ko_title=session.translate(&episode.title)?;
    let current=format_number(number,&config.page_number_format)?;
    let last=format_number(total,&config.page_number_format)?;
    let tokens=regex::Regex::new(r"\[currentPage\]|\[lastPage\]|\[koreanTitle\]|\[japaneseTitle\]")?;
    let title=tokens.replace_all(&config.title_format,|caps:&regex::Captures<'_>| {
        match &caps[0] {"[currentPage]"=>current.as_str(),"[lastPage]"=>last.as_str(),"[koreanTitle]"=>ko_title.as_str(),_=>episode.title.as_str()}.to_owned()
    });
    output.push_str(&format!("\n\n{title}\n\n"));
    let translated=session.translate_lines(&episode.lines)?;
    anyhow::ensure!(translated.len()==episode.lines.len(),"본문 원문·번역 줄 수가 다릅니다. 불완전한 결과는 저장하지 않습니다");
    for (line,translation) in episode.lines.iter().zip(translated) {
        if config.print_japanese {output.push_str(line);output.push('\n');}
        output.push_str(&translation);output.push('\n');
    }
    Ok(output)
}
pub fn format_header(session:&mut Session,work:&Work)->Result<String>{
    let title=session.translate(&work.title)?;
    let description=session.translate_lines(&work.description.lines().map(str::to_owned).collect::<Vec<_>>())?.join("\n");
    Ok(format!("{title}\n{}\n{}\n\n{description}\n\n",work.title,work.url))
}
pub fn suggested_filename(title:&str,config:&Config)->Result<String>{
    let without_prefix=if config.filter_title_bracket {regex::Regex::new(r"^【[^】]+】")?.replace(title, "").into_owned()}else{title.to_owned()};
    let sanitized=regex::Regex::new(r#"[\\/:*?"<>|]|\[[^\]]*\]"#)?.replace_all(&without_prefix, "");
    let name:String=sanitized.chars().filter(|c|!c.is_control()).take(config.max_title_length).collect();
    let name=name.trim().trim_end_matches('.').trim();
    let mut name=if name.is_empty(){"novel".to_owned()}else{name.to_owned()};
    let base=name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON","PRN","AUX","NUL"].contains(&base.as_str())||(base.len()==4&&(base.starts_with("COM")||base.starts_with("LPT"))&&matches!(base.as_bytes()[3],b'1'..=b'9')){name.insert(0,'_');}
    Ok(format!("{name}.txt"))
}
#[cfg(test)] mod tests {
    use super::*;
    use std::fs;
    #[test] fn writes_temp_then_rename(){
        let path=std::env::temp_dir().join(format!("syousetu-test-{}.txt",std::process::id()));
        atomic_write(&path,"안녕\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(),"안녕\n");
        fs::remove_file(path).unwrap();
    }
    #[test] fn missing_translated_lines_are_rejected(){
        struct Incomplete;
        impl crate::engine::Translator for Incomplete {
            fn translate(&mut self,_:&str)->Result<String>{Ok("제목".into())}
            fn translate_lines(&mut self,_:&[String])->Result<Vec<String>>{Ok(vec!["한 줄만".into()])}
        }
        let mut session=Session::direct(Box::new(Incomplete));
        let episode=Episode{title:"章".into(),lines:vec!["一".into(),"二".into()]};
        assert!(format_episode(&mut session,&episode,1,1,&Config::default()).is_err());
    }
}
