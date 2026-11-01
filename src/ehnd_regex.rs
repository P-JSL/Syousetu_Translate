use crate::ehnd::{Ehnd,Phase,RuleType};
use anyhow::{bail,Context,Result};
use fancy_regex::{Regex,RegexBuilder};
use serde::Serialize;

const MAX_INPUT:usize=1024*1024;
const MAX_OUTPUT:usize=2*1024*1024;
const BACKTRACK_LIMIT:usize=100_000;

#[derive(Debug,Clone,Serialize,Default)]
pub struct RegexAudit {
    pub literals:usize,
    pub regexes:usize,
    pub rejected:usize,
    pub directives:usize,
    pub unknown:usize,
    pub failures:Vec<String>,
    pub original_engine_equivalent:bool,
}
enum Compiled {Literal(String,String),Regex(Regex,String)}
struct Step {
    phase:Phase,priority:u64,order:usize,location:String,operation:Compiled,
}
pub struct Executor {
    steps:Vec<Step>,
    pub audit:RegexAudit,
    pre_enabled:bool,
    post_enabled:bool,
}
fn normalize_groups(template:&str)->String {
    let mut result=String::new();
    let mut it=template.chars().peekable();
    while let Some(c)=it.next() {
        if c!='$' {result.push(c);continue;}
        if it.peek()==Some(&'$') {it.next();result.push_str("$$");continue;}
        let mut digits=String::new();
        while it.peek().is_some_and(char::is_ascii_digit) {
            digits.push(it.next().unwrap());
        }
        if digits.is_empty(){result.push('$');}
        else {
            result.push('$');
            result.push('{');
            result.push_str(&digits);
            result.push('}');
        }
    }
    result
}
fn compile_pattern(pattern:&str)->Result<Regex>{
    let mut builder=RegexBuilder::new(pattern);
    builder.backtrack_limit(BACKTRACK_LIMIT)
        .delegate_size_limit(1<<20)
        .delegate_dfa_size_limit(1<<18);
    builder.build().context("fancy-regex compile")
}
impl Executor {
    pub fn compile(ehnd:&Ehnd,strict:bool)->Result<Self>{
        let mut steps=Vec::new();
        let mut audit=RegexAudit::default();
        for (order,rule) in ehnd.rules.iter().enumerate() {
            match rule.kind {
                RuleType::Directive=>{audit.directives+=1;continue;}
                RuleType::Unknown=>{audit.unknown+=1;continue;}
                _=>{}
            }
            let (Some(priority),Some(search),Some(replace))=
                (rule.priority,rule.fields.first(),rule.fields.get(1))
                else {audit.unknown+=1;continue;};
            let operation=match rule.kind {
                RuleType::Literal=>{
                    if search.is_empty(){audit.unknown+=1;continue;}
                    audit.literals+=1;
                    Compiled::Literal(search.clone(),replace.clone())
                }
                RuleType::Regex=>{
                    match compile_pattern(search){
                        Ok(regex)=>{
                            audit.regexes+=1;
                            Compiled::Regex(regex,normalize_groups(replace))
                        }
                        Err(error)=>{
                            audit.rejected+=1;
                            if audit.failures.len()<100 {
                                audit.failures.push(format!("{}:{}: {error}",
                                    rule.origin,rule.line));
                            }
                            continue;
                        }
                    }
                }
                _=>unreachable!(),
            };
            steps.push(Step{
                phase:rule.phase,priority,order,
                location:format!("{}:{}",rule.origin,rule.line),operation,
            });
        }
        steps.sort_by_key(|step|(step.priority,step.order));
        if strict&&(audit.directives>0||audit.unknown>0||audit.rejected>0){
            bail!("EHND 원본 동등성 미충족: directives={}, unknown={}, rejected={}",
                audit.directives,audit.unknown,audit.rejected);
        }
        let enabled=|name:&str| !ehnd.config.iter().any(|(key,value)|
            key==name && value.eq_ignore_ascii_case("OFF"));
        Ok(Self{steps,audit,
            pre_enabled:enabled("PREFILTER_SWITCH"),
            post_enabled:enabled("POSTFILTER_SWITCH")})
    }
    pub fn apply(&self,phase:Phase,input:&str)->Result<String>{
        if input.len()>MAX_INPUT{bail!("EHND 입력 용량 초과");}
        let enabled=match phase{
            Phase::Pre=>self.pre_enabled,Phase::Post=>self.post_enabled
        };
        if !enabled{return Ok(input.to_owned());}
        let mut output=input.to_owned();
        for step in &self.steps {
            if step.phase!=phase{continue;}
            match &step.operation {
                Compiled::Literal(from,to)=>{
                    if output.contains(from){output=output.replace(from,to);}
                }
                Compiled::Regex(regex,to)=>{
                    output=regex.try_replacen(&output,0,to.as_str())
                        .with_context(||format!("EHND 실행 실패 {} (backtrack limit {})",
                            step.location,BACKTRACK_LIMIT))?.into_owned();
                }
            }
            if output.len()>MAX_OUTPUT {
                bail!("EHND 결과 크기 초과 {}",step.location);
            }
        }
        Ok(output)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn regex_lookaround_and_capture_replacements(){
        assert_eq!(normalize_groups("$0x$1_"),concat!("$", "{0}", "x", "$", "{1}", "_"));
        let pattern=compile_pattern(r"(?<=猫)(は)(?=学生)").unwrap();
        assert_eq!(pattern.try_replacen("猫は学生",0,normalize_groups("$1が").as_str()).unwrap(),"猫はが学生");
        let pattern=compile_pattern(r"([ー─])\1+").unwrap();
        assert_eq!(pattern.try_replacen("ーー─",0,"$1").unwrap(),"ー─");
    }
    #[test]
    fn safe_failure_on_invalid_regex(){
        assert!(compile_pattern("(").is_err());
    }
}