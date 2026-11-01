use crate::{engine::Translator,filter::Pipeline,site::{Episode,Work}};
use anyhow::{Context, Result};
use std::{fs::{self,File},io::{BufWriter,Write},path::Path};

pub fn format_episode(pipeline:&Pipeline, engine:&mut dyn Translator, episode:&Episode,
                      number:usize,total:usize,include_japanese:bool)->Result<String>{
    let mut output=String::new();
    let ko_title=pipeline.translate(engine,&episode.title)?;
    output.push_str(&format!("\n\n[{number}/{total}] {ko_title} / {}\n\n",episode.title));
    for line in &episode.lines {
        if include_japanese && !line.trim().is_empty(){output.push_str(line);output.push('\n');}
        output.push_str(&pipeline.translate(engine,line)?);output.push('\n');
    }
    Ok(output)
}
pub fn format_header(pipeline:&Pipeline,engine:&mut dyn Translator,work:&Work)->Result<String>{
    let title=pipeline.translate(engine,&work.title)?;
    Ok(format!("{title}\n{}\n{}\n\n{}\n\n",work.title,work.url,work.description))
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