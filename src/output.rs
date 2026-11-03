use crate::{config::{Config,format_number},session::Session,site::{Episode,Work}};
use anyhow::{Context, Result};
use std::{fs::{self,File},io::{BufWriter,Write},path::Path};

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
pub fn atomic_write(path:&Path,body:&str)->Result<()> {
    let parent=path.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name=path.file_name().context("출력 파일명이 없습니다")?.to_string_lossy();
    let tmp=parent.join(format!(".{name}.{}.tmp",std::process::id()));
    let result=(||->Result<()>{
        let mut writer=BufWriter::new(File::create(&tmp)?);
        writer.write_all(body.as_bytes())?;writer.flush()?;
        writer.get_ref().sync_all()?;
        if path.exists(){
            let backup=path.with_extension("backup");
            if backup.exists(){anyhow::bail!("백업 파일이 이미 존재합니다: {}",backup.display());}
            fs::rename(path,&backup).context("백업 생성 실패")?;
            match fs::rename(&tmp,path){
                Ok(())=>{fs::remove_file(&backup).ok();Ok(())},
                Err(e)=>{let _=fs::rename(&backup,path);Err(e.into())},
            }
        } else {fs::rename(&tmp,path)?;Ok(())}
    })();
    if result.is_err(){let _=fs::remove_file(&tmp);}
    result
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn writes_temp_then_rename(){
        let path=std::env::temp_dir().join(format!("syousetu-test-{}.txt",std::process::id()));
        atomic_write(&path,"안녕\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(),"안녕\n");
        fs::remove_file(path).unwrap();
    }
}
