use anyhow::{Context, Result, bail};
use std::{io::Read,thread, time::Duration};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use url::Url;

pub struct Fetcher { client: Client, pub delay_ms: u64, pub retries: u32 }
fn redirect_policy()->Policy{
    Policy::custom(|attempt|{
        let Some(first)=attempt.previous().first()else{return attempt.stop();};let next=attempt.url();
        if attempt.previous().len()>=5{return attempt.error("리다이렉트 횟수 초과");}
        let same_origin=next.host_str()==first.host_str()&&(next.port_or_known_default()==first.port_or_known_default()||(first.scheme()=="http"&&first.port_or_known_default()==Some(80)&&next.scheme()=="https"&&next.port_or_known_default()==Some(443)));
        if same_origin&&(next.scheme()=="https"||(first.scheme()=="http"&&next.scheme()=="http"))&&next.username().is_empty()&&next.password().is_none(){attempt.follow()}else{attempt.stop()}
    })
}
impl Fetcher {
    pub fn with_options(delay_ms:u64,retries:u32,user_agent:&str)->Result<Self>{
        let mut builder=Client::builder().timeout(Duration::from_secs(25)).redirect(redirect_policy()).user_agent(user_agent);
        if let Some(path)=std::env::var_os("SSL_CERT_FILE").or_else(||std::env::var_os("REQUESTS_CA_BUNDLE")){
            let bundle=std::fs::read(path).context("HTTPS CA bundle 읽기 실패")?;
            for cert in reqwest::Certificate::from_pem_bundle(&bundle).context("HTTPS CA bundle 해석 실패")?{builder=builder.add_root_certificate(cert);}
        }
        Ok(Self {client:builder.build()?,delay_ms,retries:retries.min(10)})
    }
    pub fn get(&self,url:&str)->Result<String>{
        let original=Url::parse(url)?;
        match original.scheme(){"https"|"http"=>{},_=>bail!("잘못된 URL scheme")}
        if !original.username().is_empty()||original.password().is_some(){bail!("URL 자격 증명을 사용할 수 없습니다");}
        let mut last=None;
        for attempt in 0..=self.retries {
            if attempt>0 {thread::sleep(Duration::from_millis(700u64.saturating_mul(1u64 << attempt).min(8000)));}
            match self.client.get(url).send() {
                Ok(resp) => {
                    let status=resp.status();
                    if status.is_success(){
                        const MAX_PAGE:u64=16*1024*1024;
                        if resp.content_length().is_some_and(|size|size>MAX_PAGE){bail!("페이지 응답 용량 초과");}
                        let content_type=resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value|value.to_str().ok()).unwrap_or("").to_owned();
                        let mut bytes=Vec::new();resp.take(MAX_PAGE+1).read_to_end(&mut bytes).context("응답 읽기 실패")?;
                        if bytes.len()as u64>MAX_PAGE{bail!("페이지 응답 용량 초과");}
                        let charset=content_type.split(';').skip(1).find_map(|part|{let(key,value)=part.trim().split_once('=')?;key.trim().eq_ignore_ascii_case("charset").then_some(value.trim().trim_matches(['\"','\'']))});
                        let encoding=encoding_rs::Encoding::for_bom(&bytes).map(|pair|pair.0).or_else(||charset.and_then(|label|encoding_rs::Encoding::for_label(label.as_bytes()))).unwrap_or(encoding_rs::UTF_8);
                        let(content,_,bad)=encoding.decode(&bytes);if bad{bail!("응답 문자 디코딩 실패");}
                        if content.contains("Just a moment...") || content.contains("challenge-container") || content.contains("Human Verification") {
                            bail!("사이트 보안 확인 화면입니다. 인증을 직접 완료한 뒤 저장한 HTML을 사용하세요");
                        }
                        return Ok(content.into_owned());
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

#[cfg(test)]mod tests{
    use super::*;use std::{io::{BufRead,BufReader,Write},net::TcpListener};
    fn server(responses:Vec<Vec<u8>>)->(String,thread::JoinHandle<()>){let listener=TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();let handle=thread::spawn(move||{for response in responses{let(stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();let mut reader=BufReader::new(stream);let mut header=String::new();loop{header.clear();if reader.read_line(&mut header).unwrap()==0||header=="\r\n"{break;}}reader.get_mut().write_all(&response).unwrap();}});(format!("http://{address}"),handle)}
    fn local_fetcher()->Fetcher{Fetcher{client:Client::builder().no_proxy().timeout(Duration::from_secs(3)).redirect(redirect_policy()).build().unwrap(),delay_ms:0,retries:0}}
    #[test]fn response_charset_is_respected(){let(encoded,_,bad)=encoding_rs::SHIFT_JIS.encode("こんにちは");assert!(!bad);let mut response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=Shift_JIS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",encoded.len()).into_bytes();response.extend(encoded.as_ref());let(url,handle)=server(vec![response]);assert_eq!(local_fetcher().get(&url).unwrap(),"こんにちは");handle.join().unwrap();}
    #[test]fn same_origin_redirects_follow_but_cross_origin_is_rejected(){let redirect=b"HTTP/1.1 302 Found\r\nLocation: /chapter\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();let page=b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".to_vec();let(url,handle)=server(vec![redirect,page]);assert_eq!(local_fetcher().get(&url).unwrap(),"ok");handle.join().unwrap();let(url,handle)=server(vec![b"HTTP/1.1 302 Found\r\nLocation: https://example.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()]);assert!(local_fetcher().get(&url).unwrap_err().to_string().contains("리다이렉트"));handle.join().unwrap();}
    #[test]fn oversized_and_challenge_responses_fail(){let challenge="Just a moment...";let response=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{challenge}",challenge.len()).into_bytes();let(url,handle)=server(vec![response]);assert!(local_fetcher().get(&url).unwrap_err().to_string().contains("보안 확인"));handle.join().unwrap();let(url,handle)=server(vec![b"HTTP/1.1 200 OK\r\nContent-Length: 16777217\r\nConnection: close\r\n\r\n".to_vec()]);assert!(local_fetcher().get(&url).unwrap_err().to_string().contains("용량 초과"));handle.join().unwrap();}
}
