use super::super::{
    args::{flag, num, require, value},
    factory::translation_session,
    options,
    progress::emit_progress,
};
use crate::storage::StreamingFile;
use crate::{
    config::Config,
    novel::{
        network::Fetcher,
        site::{
            self, canonicalize, index_page_urls, parse_episode_with_comments, parse_work_pages,
            Site, Work,
        },
    },
    translation::output,
};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fetcher_for(args: &[String], config: &Config) -> Result<Fetcher> {
    let (site, _) = canonicalize(&require(args, "--url")?)?;
    let delay = if site == Site::Hameln {
        config.hameln_page_download_delay
    } else {
        config.page_download_delay
    };
    Fetcher::with_ca(
        delay,
        config.max_retry_count,
        &config.user_agent,
        config.ca_bundle.as_deref().map(Path::new),
    )
}
fn load_work(args: &[String], fetcher: &Fetcher) -> Result<Work> {
    use std::collections::{BTreeMap, HashSet};
    let url = require(args, "--url")?;
    let first = match value(args, "--html") {
        Some(path) => {
            fs::read_to_string(&path).with_context(|| format!("목차 HTML 읽기 실패: {path}"))?
        }
        None => fetcher.get(&canonicalize(&url)?.1)?,
    };
    let mut pages = vec![first];
    let mut pending = BTreeMap::new();
    let mut seen = HashSet::new();
    loop {
        for link in index_page_urls(&url, pages.last().unwrap())? {
            if !seen.insert(link.clone()) {
                continue;
            }
            let parsed = url::Url::parse(&link)?;
            let number = parsed
                .query_pairs()
                .find(|(key, _)| key == "p")
                .context("목차 페이지 번호 없음")?
                .1
                .parse::<usize>()?;
            if !(2..=1000).contains(&number) {
                bail!("목차 페이지 범위 초과: {number}");
            }
            pending.insert(number, link);
        }
        let Some((number, link)) = pending.pop_first() else {
            break;
        };
        let html = if let Some(directory) = value(args, "--index-html-dir") {
            let path = Path::new(&directory).join(format!("{number}.html"));
            fs::read_to_string(&path)
                .with_context(|| format!("목차 {number}페이지 HTML 없음: {}", path.display()))?
        } else if value(args, "--html").is_some() {
            bail!("추가 목차가 필요합니다. --index-html-dir에 2.html, 3.html 등을 제공하세요");
        } else {
            if !flag(args, "--no-delay") {
                fetcher.rest();
            }
            fetcher.get(&link)?
        };
        pages.push(html);
    }
    if pages.len() == 1 {
        site::parse_work(&url, &pages[0])
    } else {
        parse_work_pages(&url, &pages)
    }
}
pub(crate) fn inspect(args: &[String]) -> Result<()> {
    let config = options::settings(args)?;
    let work = load_work(args, &fetcher_for(args, &config)?)?;
    println!("{}", serde_json::to_string_pretty(&work)?);
    Ok(())
}
pub(crate) fn download(args: &[String]) -> Result<()> {
    let config = options::settings(args)?;
    let fetcher = fetcher_for(args, &config)?;
    let work = load_work(args, &fetcher)?;
    if let Some(directory) = value(args, "--cache-html-dir") {
        output::atomic_write(
            &Path::new(&directory).join("work.json"),
            &serde_json::to_string_pretty(&work)?,
        )?;
    }
    let first = num(args, "--start", 1)?;
    if first == 0 || first > work.episodes.len() {
        bail!("--start가 작품 화수 범위를 벗어났습니다");
    }
    let end = num(args, "--end", work.episodes.len())?.min(work.episodes.len());
    if first > end {
        bail!("--end가 --start보다 작습니다");
    }
    let mut session = translation_session(args, &config)?;
    let output = if let Some(path) = value(args, "--out") {
        PathBuf::from(path)
    } else if flag(args, "--auto-output")
        || value(args, "--output-dir").is_some()
        || config.save_without_file_chooser
    {
        let directory = value(args, "--output-dir")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.download_directory());
        directory.join(output::suggested_filename(
            &session.translate(&work.title)?,
            &config,
        )?)
    } else {
        bail!("필수 옵션 --out 누락; 자동 이름은 --auto-output 또는 --output-dir을 지정하세요");
    };
    let mut out = StreamingFile::create(&output)?;
    out.write_all(output::format_header(&mut session, &work)?.as_bytes())?;
    out.flush()?;
    for i in first..=end {
        let file =
            value(args, "--episode-html-dir").map(|dir| Path::new(&dir).join(format!("{i}.html")));
        let page = match file {
            Some(file) => fs::read_to_string(&file)
                .with_context(|| format!("에피소드 HTML 없음: {}", file.display()))?,
            None => {
                if !flag(args, "--no-delay") {
                    fetcher.rest();
                }
                fetcher.get(&work.episodes[i - 1])?
            }
        };
        if let Some(directory) = value(args, "--cache-html-dir") {
            output::atomic_write(&Path::new(&directory).join(format!("{i}.html")), &page)?;
        }
        let episode = parse_episode_with_comments(
            work.site,
            &page,
            config.include_furigana,
            config.include_comment,
            &config.comment_separator,
        )
        .with_context(|| format!("에피소드 {i} 파싱 실패: {}", work.episodes[i - 1]))?;
        output::write_episode(&mut out, &mut session, &episode, i, end, &config)?;
        eprintln!("[{i}/{end}] 저장 완료: {}", episode.title);
        emit_progress(args, i - first + 1, end - first + 1, "chapters");
    }
    out.finish()?;
    println!("Saved {} episodes to {}", end - first + 1, output.display());
    Ok(())
}
