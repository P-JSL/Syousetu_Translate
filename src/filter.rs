use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use anyhow::{Context, Result};
use std::{collections::HashMap,fs,path::Path};
use crate::engine::Translator;

#[derive(Debug, Clone)]
pub struct DictionaryEntry {pub source:String, pub target:String, pub name:bool}
pub struct Dictionary { entries:Vec<DictionaryEntry>, matcher:Option<AhoCorasick> }
impl Dictionary {
    pub fn empty()->Self{ Self{entries:Vec::new(),matcher:None} }
    pub fn parse(text:&str)->Result<Self>{
        let mut entries=Vec::<DictionaryEntry>::new();let mut indices=HashMap::new();
        for line in text.trim_start_matches('\u{feff}').lines().map(|x|x.trim_end_matches('\r')) {
            if line.trim().is_empty() || line.trim_start().starts_with("//") {continue;}
            let mut parts=line.split('\t');
            let raw_source=parts.next().unwrap_or("");
            let source=raw_source.trim_end_matches("#N").to_owned();
            if source.is_empty(){continue;}
            let raw_target=parts.next().unwrap_or("");
            let (target,name)=if raw_target.is_empty(){
                let converted=hiragana_to_hangul(&source);
                if is_japanese(&converted){continue;}
                (converted,raw_source.ends_with("#N"))
            }else{(raw_target.trim_end_matches("#N").to_owned(),raw_source.ends_with("#N")||raw_target.ends_with("#N"))};
            if target.is_empty(){continue;}
            let entry=DictionaryEntry{source:source.clone(),target,name};
            if let Some(&idx)=indices.get(&source){entries[idx]=entry;}
            else {indices.insert(source,entries.len()); entries.push(entry);}
        }
        entries.sort_by(|a,b|a.source.cmp(&b.source));
        let matcher=if entries.is_empty(){None}else{Some(AhoCorasickBuilder::new().match_kind(MatchKind::LeftmostLongest)
            .build(entries.iter().map(|e|e.source.as_str()))?)};
        Ok(Self{entries,matcher})
    }
    pub fn from_file(path:&Path)->Result<Self>{
        let bytes=fs::read(path).with_context(||format!("사전 파일: {}",path.display()))?;
        let string=if bytes.starts_with(&[0xFF,0xFE]) {
            let (cow,_,errors)=encoding_rs::UTF_16LE.decode(&bytes[2..]);
            if errors {anyhow::bail!("UTF-16LE 사전 디코딩 오류");}cow.into_owned()
        } else if bytes.starts_with(&[0xFE,0xFF]) {
            let (cow,_,errors)=encoding_rs::UTF_16BE.decode(&bytes[2..]);
            if errors {anyhow::bail!("UTF-16BE 사전 디코딩 오류");}cow.into_owned()
        } else {String::from_utf8(bytes).context("UTF-8 사전 디코딩 오류")?};
        Self::parse(&string)
    }
}
fn hiragana_to_hangul(input:&str)->String{
    const TABLE:&str="あ아,い이,う우,え에,お오,か카,き키,く쿠,け케,こ코,さ사,し시,す스,せ세,そ소,た타,ち치,つ츠,て테,と토,な나,に니,ぬ누,ね네,の노,は하,ひ히,ふ후,へ헤,ほ호,ま마,み미,む무,め메,も모,や야,ゆ유,よ요,ら라,り리,る루,れ레,ろ로,わ와,を오,んㄴ,が가,ぎ기,ぐ구,げ게,ご고,ざ자,じ지,ず즈,ぜ제,ぞ조,だ다,ぢ지,づ즈,で데,ど도,ば바,び비,ぶ부,べ베,ぼ보,ぱ파,ぴ피,ぷ푸,ぺ페,ぽ포";
    let map:HashMap<char,&str>=TABLE.split(',').filter_map(|x|{let mut c=x.chars();Some((c.next()?,c.as_str()))}).collect();
    let converted:String=input.chars().map(|c| if ('ァ'..='ヶ').contains(&c){char::from_u32(c as u32-0x60).unwrap_or(c)}else{c}).collect();
    let digraphs=[("きゃ","캬"),("きゅ","큐"),("きょ","쿄"),("しゃ","샤"),("しゅ","슈"),("しょ","쇼"),("ちゃ","챠"),("ちゅ","츄"),("ちょ","쵸"),("にゃ","냐"),("にゅ","뉴"),("にょ","뇨"),("ひゃ","햐"),("ひゅ","휴"),("ひょ","효"),("みゃ","먀"),("みゅ","뮤"),("みょ","묘"),("りゃ","랴"),("りゅ","류"),("りょ","료"),("ぎゃ","갸"),("ぎゅ","규"),("ぎょ","교"),("じゃ","자"),("じゅ","주"),("じょ","조"),("びゃ","뱌"),("びゅ","뷰"),("びょ","뵤"),("ぴゃ","퍄"),("ぴゅ","퓨"),("ぴょ","표")];
    let mut out=String::new();let mut chars=converted.chars().peekable();
    while let Some(c)=chars.next(){
        if c=='っ' && chars.peek().is_some() {if let Some(next)=chars.peek(){if let Some(v)=map.get(next){out.push_str(v.chars().next().map(|x|x.to_string()).unwrap_or_default().as_str());}} continue;}
        if let Some(next)=chars.peek(){let mut pair=String::new();pair.push(c);pair.push(*next);
            if let Some((_,kor))=digraphs.iter().find(|(jp,_)|*jp==pair){out.push_str(kor);chars.next();continue;}}
        if let Some(kor)=map.get(&c){out.push_str(kor);} else {out.push(c);}
    }
    out
}
fn is_japanese(s:&str)->bool{s.chars().any(|c|matches!(c,'\u{3400}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'))}
fn is_emoji(c:char)->bool{matches!(c,'\u{1F300}'..='\u{1F77F}'|'\u{2600}'..='\u{26FF}'|'\u{2700}'..='\u{27BF}')}
fn choose_josa(last:Option<char>, suffix:&str)->&str {
    let syllable=last.unwrap_or(' ');
    let jong=if ('가'..='힣').contains(&syllable){(syllable as u32-0xAC00)%28}else{0};
    match suffix {
        "은"|"는"=>if jong!=0{"은"}else{"는"},
        "이"|"가"=>if jong!=0{"이"}else{"가"},
        "을"|"를"=>if jong!=0{"을"}else{"를"},
        "과"|"와"=>if jong!=0{"과"}else{"와"},
        "로"|"으로"=>if jong==0||jong==8{"로"}else{"으로"},
        "야"|"아"=>if jong!=0{"아"}else{"야"},
        "이야"=>if jong!=0{"이야"}else{"야"},
        _=>suffix,
    }
}
fn boundary(c:Option<char>)->bool{c.is_none_or(|c|c.is_whitespace()||".,!?;:)]}'\"…".contains(c))}
fn resolve_token(replacement:&str, remaining:&str)->(String,usize) {
    let (remaining, marker_len)=if let Some(rest)=remaining.strip_prefix("#N") {(rest,2)} else {(remaining,0)};
    let candidates=["이야","으로","은","는","이","가","을","를","과","와","로","야","아"];
    for candidate in candidates {
        if let Some(rest)=remaining.strip_prefix(candidate) {
            if boundary(rest.chars().next()) {
                return (format!("{}{}",replacement,choose_josa(replacement.chars().last(),candidate)),candidate.len()+marker_len);
            }
        }
    }
    (replacement.to_owned(),marker_len)
}
pub struct Pipeline {pub dictionary:Dictionary,strip_engine_brackets:bool}
impl Pipeline {
    pub fn new(dictionary:Dictionary)->Self{Self{dictionary,strip_engine_brackets:false}}
    pub fn legacy(dictionary:Dictionary)->Self{Self{dictionary,strip_engine_brackets:true}}
    pub fn translate(&self,engine:&mut dyn Translator,input:&str)->Result<String>{
        self.translate_inner(engine,input,0)
    }
    fn translate_inner(&self,engine:&mut dyn Translator,input:&str,depth:u8)->Result<String>{
        if input.is_empty()||!is_japanese(input){return Ok(input.to_owned());}
        if depth>12 {return engine.translate(input);}
        let mut special_prefix="s$".to_owned();
        while input.contains(&special_prefix){special_prefix.push('_');}
        let mut dictionary_prefix="c$".to_owned();
        while input.contains(&dictionary_prefix){dictionary_prefix.push('_');}
        let mut protected=Vec::<(String,String,bool)>::new();
        let mut work=String::with_capacity(input.len());
        let mut chars=input.char_indices().peekable();
        while let Some((_,c))=chars.next() {
            match c {
                '～'=>work.push('~'),'（'=>work.push('('),'）'=>work.push(')'),'？'=>work.push('?'),
                '('|'[' => {
                    let close=if c=='(' {')'} else {']'};
                    let mut part=String::new();let mut found=false;
                    while let Some((_,next))=chars.peek().copied(){
                        if next==c {break;}
                        chars.next(); if next==close{found=true;break;}part.push(next);
                    }
                    if found{
                        let inner=self.translate_inner(engine,&part,depth+1)?;
                        let token=format!("{special_prefix}{}$s",protected.len());
                        work.push_str(&token);protected.push((token,format!("{c}{inner}{close}"),false));
                    }else{work.push(c);work.push_str(&part);}
                }
                c if is_emoji(c)=>{let token=format!("{special_prefix}{}$s",protected.len());work.push_str(&token);protected.push((token,c.to_string(),false));}
                _=>work.push(c)
            }
        }
        if let Some(matcher)=&self.dictionary.matcher {
            let mut output=String::with_capacity(work.len());let mut end=0;
            for hit in matcher.find_iter(&work){
                output.push_str(&work[end..hit.start()]);
                let item=&self.dictionary.entries[hit.pattern().as_usize()];
                let token=format!("{dictionary_prefix}{}$c",protected.len());
                output.push_str(&token);
                if item.name{output.push_str("#N");}
                protected.push((token,item.target.clone(),true));end=hit.end();
            }
            output.push_str(&work[end..]);work=output;
        }
        let mut translated=engine.translate(&work)?;
        if self.strip_engine_brackets {
            let pattern=regex::Regex::new(r"\[[^\[\]]*\]|\([^()]*\)")?;
            loop {
                let stripped=pattern.replace_all(&translated, "");
                if stripped.len()==translated.len(){break;}
                translated=stripped.into_owned();
            }
        }
        if protected.is_empty(){return Ok(translated.trim_end().to_owned());}
        let tokens:Vec<&str>=protected.iter().map(|x|x.0.as_str()).collect();
        let matcher=AhoCorasickBuilder::new().match_kind(MatchKind::LeftmostLongest).build(&tokens)?;
        let mut output=String::new();let mut end=0;
        for hit in matcher.find_iter(&translated) {
            output.push_str(&translated[end..hit.start()]);
            let (_,replacement,dictionary)=&protected[hit.pattern().as_usize()];
            let tail=&translated[hit.end()..];
            let (text,skip)=if *dictionary{resolve_token(replacement,tail)}else{(replacement.to_owned(),0)};
            output.push_str(&text);end=hit.end()+skip;
        }
        output.push_str(&translated[end..]);
        Ok(output.trim_end().to_owned())
    }
}
#[cfg(test)] mod tests {
    #[test] fn java_dictionary_readings_bom_and_extra_columns(){
        let dictionary=super::Dictionary::parse("\u{feff}カナ#N\t가나\textra\n未知漢字\nきゃ\n").unwrap();
        assert_eq!(dictionary.entries.len(),2);
        assert!(dictionary.entries.iter().any(|e|e.source=="カナ"&&e.target=="가나"&&e.name));
        assert!(dictionary.entries.iter().any(|e|e.source=="きゃ"&&e.target=="캬"));
    }
    #[test] fn literal_placeholder_strings_are_not_overwritten(){
        let pipeline=super::Pipeline::new(super::Dictionary::parse("猫\t고양이").unwrap());
        let mut engine=crate::engine::Passthrough;
        assert_eq!(pipeline.translate(&mut engine,"c$0$c 猫 s$0$s 😀").unwrap(),"c$0$c 고양이 s$0$s 😀");
        assert!(super::Dictionary::parse("っ\n").unwrap().entries.is_empty());
    }
    #[test] fn legacy_annotations_removed_before_protected_values_return(){
        struct Annotated;
        impl crate::engine::Translator for Annotated {
            fn translate(&mut self,text:&str)->anyhow::Result<String>{Ok(format!("{text}[annotation](note)"))}
        }
        let pipeline=super::Pipeline::legacy(super::Dictionary::parse("猫\t고양이[이름]").unwrap());
        assert_eq!(pipeline.translate(&mut Annotated,"猫 (犬)").unwrap(),"고양이[이름] (犬)");
    }
    use super::*;
    use crate::engine::Passthrough;
    #[test] fn dictionary_overlaps(){
        let dict=Dictionary::parse("天\t하늘\n天気\t날씨\n").unwrap();
        let pipe=Pipeline::new(dict);
        assert_eq!(pipe.translate(&mut Passthrough,"天気はいい").unwrap(),"날씨はいい");
    }
    #[test] fn unicode_and_josa(){
        assert_eq!(choose_josa(Some('길'),"으로"),"로");
        assert_eq!(choose_josa(Some('집'),"으로"),"으로");
        assert_eq!(choose_josa(Some('사'),"은"),"는");
        assert_eq!(hiragana_to_hangul("きゃ"),"캬");
    }
    #[test] fn native_dictionary_names_keep_korean_josa(){
        use crate::engine::{create_engine, EngineConfig, EngineMode};
        let custom=Dictionary::parse("猫\t사자#N\n").unwrap();
        let pipeline=Pipeline::new(custom);
        let mut native=create_engine(&EngineConfig{
            mode:EngineMode::Native,
            eztrans_dir:None,
            native_dictionary:None,
            strict_native:false,
        }).unwrap();
        assert_eq!(pipeline.translate(&mut *native,"猫は学生です。").unwrap(),
            "사자는 학생입니다。");
    }
    #[test] fn preserve_emoji_and_brackets(){
        let pipe=Pipeline::new(Dictionary::empty());
        let result=pipe.translate(&mut Passthrough,"こんにちは 😀 (世界)").unwrap();
        assert_eq!(result,"こんにちは 😀 (世界)");
    }
}
