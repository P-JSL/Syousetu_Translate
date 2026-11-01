use anyhow::{Context, Result, bail};
use std::{thread, time::Duration};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use url::Url;

pub struct Fetcher { client: Client, pub delay_ms: u64, pub retries: u32 }
impl Fetcher {
    pub fn new(delay_ms:u64,retries:u32)->Result<Self>{
        let client=Client::builder().timeout(Duration::from_secs(25))
            .redirect(Policy::none())
            .user_agent("SyousetuTranslateRust/0.1 (personal web novel reader)")
            .build()?;
        Ok(Self {client,delay_ms,retries:retries.min(4)})
    }
    pub fn get(&self,url:&str)->Result<String>{
        let original=Url::parse(url)?;
        match original.scheme(){"https"|"http"=>{},_=>bail!("잘못된 URL scheme")}
        let mut last=None;
        for attempt in 0..=self.retries {
            if attempt>0 {thread::sleep(Duration::from_millis(700u64.saturating_mul(1u64 << attempt)));}
            match self.client.get(url).send() {
                Ok(resp) => {
                    let status=resp.status();
                    if status.is_success(){
                        let content=resp.text().context("응답 디코딩 실패")?;
                        if content.contains("Just a moment...") || content.contains("challenge-container") || content.contains("Human Verification") {
                            bail!("사이트 보안 확인 화면입니다. 인증을 직접 완료한 뒤 저장한 HTML을 사용하세요");
                        }
                        return Ok(content);
                    }
                    if status.is_redirection(){bail!("HTTP 리다이렉트 {} (자동 교차 도메인 이동 차단)",status)}
                    if status.as_u16()==401 || status.as_u16()==403 || status.as_u16()==404 || status.as_u16()==429 {
                        bail!("페이지 접근 실패: HTTP {} (권한 또는 제한 확인 필요)", status);
                    }
                    last=Some(anyhow::anyhow!("HTTP {}: {}",status,url));
                }
                Err(e)=>last=Some(e.into()),
            }
        }
        Err(last.unwrap_or_else(||anyhow::anyhow!("요청 실패")))
    }
    pub fn rest(&self){thread::sleep(Duration::from_millis(self.delay_ms));}
}