use super::*;
use serde_json::Value;
#[test]
fn canonical_validation() {
    assert_eq!(
        canonicalize("https://www.alphapolis.co.jp/novel/767156331/284095783")
            .unwrap()
            .1,
        "https://www.alphapolis.co.jp/novel/767156331/284095783/"
    );
    assert_eq!(
        canonicalize("https://kakuyomu.jp/works/2912051603596422650/episodes/123")
            .unwrap()
            .1,
        "https://kakuyomu.jp/works/2912051603596422650"
    );
    assert!(canonicalize("https://kakuyomu.jp.evil.test/works/1").is_err());
    assert!(canonicalize("file:///etc/passwd").is_err());
}
#[test]
fn alphapolis_toc_and_episode() {
    let work=parse_work("https://www.alphapolis.co.jp/novel/767156331/284095783",
            "<div class='content-main'><h1 class='title'>作品名</h1></div><div class='episode'><a href='/novel/767156331/284095783/episode/123'>1話</a></div>").unwrap();
    assert_eq!(work.episodes.len(), 1);
    let episode=parse_episode(Site::Alphapolis,
            "<h1 class='episode-title'>第一話</h1><div id='novelBody'><p>本文<ruby>漢<rt>かん</rt></ruby>字</p><p>次の行</p></div>",false).unwrap();
    assert_eq!(episode.title, "第一話");
    assert!(episode.lines.join(" ").contains("本文漢字"));
    assert!(!episode.lines.join(" ").contains("かん"));
}
#[test]
fn meta_title_and_description() {
    let index = r#"<meta property="og:title" content="テスト作品"><meta name="description" content="紹介文">
          <a href="/novel/767156331/284095783/episode/42">本文へ</a>"#;
    let work = parse_work(
        "https://www.alphapolis.co.jp/novel/767156331/284095783",
        index,
    )
    .unwrap();
    assert_eq!(work.title, "テスト作品");
    assert_eq!(work.description, "紹介文");
}
#[test]
fn preview_only_toc_is_rejected() {
    let url = "https://kakuyomu.jp/works/2912051603596422650";
    let html = r#"<h1>本文</h1><div>連載中 全118話</div>
            <a href="/works/2912051603596422650/episodes/1">1</a>
            <a href="/works/2912051603596422650/episodes/2">2</a>"#;
    assert!(parse_work(url, html)
        .unwrap_err()
        .to_string()
        .contains("수집 2화 / 사이트 표시 118화"));
}
#[test]
fn kakuyomu_toc() {
    let work=parse_work("https://kakuyomu.jp/works/2912051603596422650",
            "<h1>タイトル</h1><a href='/works/2912051603596422650/episodes/1'>A</a><a href='/works/2912051603596422650/episodes/2'>B</a>").unwrap();
    assert_eq!(work.episodes.len(), 2);
}
#[test]
fn syosetu_body_is_separate_from_author_comments() {
    for site in [Site::Syosetu, Site::Syosetu18] {
        for html in [
                "<h1 class='p-novel__title p-novel__title--rensai'>第一話</h1><div class='js-novel-text p-novel__text p-novel__text--preface'><p>前書き</p></div><div class='js-novel-text p-novel__text'><p id='L1'>本<ruby>文<rt>ぶん</rt></ruby></p></div><div class='js-novel-text p-novel__text p-novel__text--afterword'><p>後書き</p></div>",
                "<div class='novel_subtitle'>第一話</div><div id='novel_p'><p>前書き</p></div><div id='novel_honbun'><p>本<ruby>文<rt>ぶん</rt></ruby></p></div><div id='novel_a'><p>後書き</p></div>",
            ] {
                let body = parse_episode(site, html, false).unwrap();
                assert_eq!(body.title, "第一話");
                assert_eq!(body.lines, ["本文"]);
                let comments = parse_episode_with_comments(site, html, false, true, "----").unwrap();
                assert_eq!(comments.lines, ["前書き", "----", "本文", "----", "後書き"]);
                assert!(parse_episode(site, html, true).unwrap().lines.join("").contains("ぶん"));
            }
    }
}
#[test]
fn author_comments_cannot_substitute_for_missing_body() {
    let html = "<h1>第一話</h1><div class='js-novel-text p-novel__text p-novel__text--preface'><p>前書きのみ</p></div>";
    assert!(parse_episode_with_comments(Site::Syosetu, html, false, true, "---").is_err());
}
#[test]
fn hameln_title_summary_and_comments() {
    let index = "<div id='maind'><div class='ss'><span itemprop='name'>作品名</span></div><div class='ss'>作品紹介</div><div class='ss'><a class='episode-list__link' href='./1.html'>第一話</a><a href='./?mode=review'>感想</a></div></div>";
    let work = parse_work("https://syosetu.org/novel/123/", index).unwrap();
    assert_eq!(work.title, "作品名");
    assert_eq!(work.description, "作品紹介");
    assert_eq!(work.episodes, ["https://syosetu.org/novel/123/1.html"]);
    let html = "<div class='ss'><span style='font-size:120%'>作品名</span><span style='font-size: 120%;'>第一話</span><div id='maegaki'>前書き<br>続き</div><div id='honbun'><p>本文</p></div><div id='atogaki'><p>後書き</p></div></div>";
    let episode = parse_episode_with_comments(Site::Hameln, html, false, true, "***").unwrap();
    assert_eq!(episode.title, "第一話");
    assert_eq!(
        episode.lines,
        ["前書き", "続き", "***", "本文", "***", "後書き"]
    );
    assert_eq!(
        parse_episode(Site::Hameln, html, false).unwrap().lines,
        ["本文"]
    );
}
#[test]
fn both_novelup_index_layouts_are_supported() {
    for index in [
            "<h1 class='storyTitle'>作品名</h1><div class='novel_synopsis'>紹介文</div><span class='totalEpisode'>全2エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a><a class='episodeTitle' href='/story/123/12'>第二話</a></div>",
            "<div id='section_works_info'><h2 class='novel_title'>作品名</h2></div><span class='total_episode_num'>2</span><div id='section_episode'><div class='episode_link'><a href='/story/123/11'>第一話</a></div><div class='episode_link'><a href='/story/123/12'>第二話</a></div></div>",
        ] {
            let work = parse_work("https://novelup.plus/story/123", index).unwrap();
            assert_eq!(work.title, "作品名");
            assert_eq!(work.episodes, ["https://novelup.plus/story/123/11", "https://novelup.plus/story/123/12"]);
        }
}
#[test]
fn novelup_author_comments_and_ruby() {
    let html = "<div class='novel_foreword'>前書き</div><div id='js-scroll-area'><h2 class='episode_title'>第一話</h2><div class='content'>本文<ruby>字<rp>(</rp><rt>じ</rt><rp>)</rp></ruby><br>次の行</div></div><div class='novel_afterword'>後書き</div>";
    let episode = parse_episode_with_comments(Site::Novelup, html, false, true, "***").unwrap();
    assert_eq!(episode.title, "第一話");
    assert_eq!(
        episode.lines,
        ["前書き", "***", "本文字", "次の行", "***", "後書き"]
    );
}
fn novelup_page(start: usize, end: usize) -> String {
    let mut page = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全201エピソード</span><div class='episodeList'>".to_owned();
    for episode in start..=end {
        page.push_str(&format!(
            "<a class='episodeTitle' href='/story/123/{episode}'>第{episode}話</a>"
        ));
    }
    page.push_str("</div>");
    page
}
#[test]
fn novelup_pagination_requires_real_episode_links_from_all_pages() {
    let first = novelup_page(1, 100);
    assert_eq!(
        index_page_urls("https://novelup.plus/story/123", &first).unwrap(),
        [
            "https://novelup.plus/story/123?p=2",
            "https://novelup.plus/story/123?p=3"
        ]
    );
    assert!(parse_work("https://novelup.plus/story/123", &first)
        .unwrap_err()
        .to_string()
        .contains("수집 100화 / 사이트 표시 201화"));
    assert!(parse_work_pages(
        "https://novelup.plus/story/123",
        &[first.clone(), novelup_page(101, 200)]
    )
    .is_err());
    let work = parse_work_pages(
        "https://novelup.plus/story/123",
        &[first, novelup_page(101, 200), novelup_page(201, 201)],
    )
    .unwrap();
    assert_eq!(work.episodes.len(), 201);
    assert_eq!(work.episodes[100], "https://novelup.plus/story/123/101");
    assert_eq!(work.episodes[200], "https://novelup.plus/story/123/201");
}
#[test]
fn pagination_only_uses_same_work_and_does_not_create_episodes() {
    let first = "<h1>作品名</h1><a href='/n1234ab/1/'>第一話</a><a href='?p=3'>最後へ</a><a href='https://evil.test/n1234ab/?p=8'>外部</a><a href='/n9999zz/?p=10'>別作品</a><a href='/n1234ab/review/'>感想</a><a href='/n1234ab/1/?tracking=1#top'>第一話再掲</a>".to_owned();
    let url = "https://ncode.syosetu.com/n1234ab/";
    assert_eq!(
        index_page_urls(url, &first).unwrap(),
        [
            "https://ncode.syosetu.com/n1234ab/?p=2",
            "https://ncode.syosetu.com/n1234ab/?p=3"
        ]
    );
    assert!(parse_work(url, &first).is_err());
    let work = parse_work_pages(
        url,
        &[
            first,
            "<a href='/n1234ab/2/'>第二話</a>".to_owned(),
            "<a href='/n1234ab/3/'>第三話</a>".to_owned(),
        ],
    )
    .unwrap();
    assert_eq!(
        work.episodes,
        [
            "https://ncode.syosetu.com/n1234ab/1/",
            "https://ncode.syosetu.com/n1234ab/2/",
            "https://ncode.syosetu.com/n1234ab/3/"
        ]
    );
}
#[test]
fn repeated_toc_pages_do_not_hide_missing_episodes() {
    let first = "<h1>作品名</h1><a href='/n1234ab/1/'>第一話</a><a href='?p=2'>次へ</a>".to_owned();
    assert!(parse_work_pages(
        "https://ncode.syosetu.com/n1234ab/",
        &[first.clone(), first]
    )
    .is_err());
}
#[test]
fn empty_or_repeated_toc_pages_are_rejected_even_when_count_matches() {
    let complete = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全1エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a></div>".to_owned();
    assert!(parse_work_pages(
        "https://novelup.plus/story/123",
        &[complete.clone(), complete.clone()]
    )
    .is_err());
    assert!(
        parse_work_pages("https://novelup.plus/story/123", &[complete, String::new()]).is_err()
    );
    assert!(parse_work_pages("https://novelup.plus/story/123", &[]).is_err());
}
#[test]
fn syosetu_short_story_and_l1_body() {
    let url = "https://ncode.syosetu.com/N1234AB/";
    let html = "<h1 class='p-novel__title'>短編作品</h1><section><p id='L1'>a</p></section>";
    let work = parse_work(url, html).unwrap();
    assert_eq!(work.episodes, ["https://ncode.syosetu.com/n1234ab/"]);
    assert_eq!(
        parse_episode(Site::Syosetu, html, false).unwrap().lines,
        ["a"]
    );
    assert!(parse_work(url, "<h1>作品名</h1><a href='?p=2'>次へ</a>").is_err());
}
fn kakuyomu_json_page(state: Value) -> String {
    let data = serde_json::json!({"props":{"pageProps":{"__APOLLO_STATE__":state}}});
    format!("<script id='__NEXT_DATA__' type='application/json'>{data}</script>")
}
#[test]
fn kakuyomu_resolves_ordered_toc_and_ignores_chapter_and_other_work_ids() {
    let html = kakuyomu_json_page(serde_json::json!({
        "Work:123": {"title":"作品名", "introduction":"全999話を予定\n二行目", "tableOfContents":[{"__ref":"TOC:1"}]},
        "Work:1234": {"title":"別作品", "introduction":"別紹介文"},
        "TOC:1": {"chapter":{"__ref":"Chapter:999"}, "episodeUnions":[{"__ref":"Episode:20"},{"__ref":"Episode:3"},{"__ref":"Episode:20"}]},
        "Chapter:999": {"id":"999", "__typename":"Chapter"},
        "Episode:20": {"id":"20"}, "Episode:3": {"id":"3"}, "Episode:10": {"id":"10"}
    }));
    let work = parse_work("https://kakuyomu.jp/works/123", &html).unwrap();
    assert_eq!(work.title, "作品名");
    assert_eq!(work.description, "全999話を予定\n二行目");
    assert_eq!(
        work.episodes,
        [
            "https://kakuyomu.jp/works/123/episodes/20",
            "https://kakuyomu.jp/works/123/episodes/3"
        ]
    );
}
#[test]
fn unresolved_kakuyomu_toc_and_unscoped_episode_cache_are_rejected() {
    for info in [
        serde_json::json!({"title":"作品名", "tableOfContents":[{"__ref":"Missing:1"},{"__ref":"Episode:20"}]}),
        serde_json::json!({"title":"作品名"}),
    ] {
        let html =
            kakuyomu_json_page(serde_json::json!({"Work:123":info,"Episode:20":{"id":"20"}}));
        assert!(parse_work("https://kakuyomu.jp/works/123", &html).is_err());
    }
}
#[test]
fn conflicting_index_counts_and_unknown_episode_unions_are_rejected() {
    let first = "<h1 class='storyTitle'>作品名</h1><span class='totalEpisode'>全2エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/11'>第一話</a></div>".to_owned();
    let second = "<span class='totalEpisode'>全3エピソード</span><div class='episodeList'><a class='episodeTitle' href='/story/123/12'>第二話</a></div>".to_owned();
    assert!(
        parse_work_pages("https://novelup.plus/story/123", &[first, second])
            .unwrap_err()
            .to_string()
            .contains("페이지마다 다릅니다")
    );
    let html = kakuyomu_json_page(serde_json::json!({
        "Work:123": {"title":"作品名", "tableOfContents":[{"episodeUnions":[
            {"__ref":"Episode:1"}, {"id":"2","__typename":"UnknownEpisode"}
        ]}]}
    }));
    assert!(parse_work("https://kakuyomu.jp/works/123", &html).is_err());
}
fn kakuyomu_v2_state() -> Value {
    serde_json::json!({
        "Work:123": {"__typename":"Work", "id":"123", "title":"合成作品", "introduction":"合成紹介文", "publicEpisodeCount":3,
            "tableOfContentsV2":[{"__ref":"TableOfContentsChapter:2"},{"__ref":"TableOfContentsChapter:1"}],
            "tableOfContents":[{"__ref":"Episode:4"}]},
        "TableOfContentsChapter:2": {"__typename":"TableOfContentsChapter", "id":"2", "chapter":{"__ref":"Chapter:999"},
            "episodeUnions":[{"__ref":"Episode:20"},{"__ref":"Episode:3"}]},
        "TableOfContentsChapter:1": {"__typename":"TableOfContentsChapter", "id":"1", "chapter":null,
            "episodeUnions":[{"__ref":"Episode:1"}]},
        "Episode:20":{"__typename":"Episode", "id":"20"}, "Episode:3":{"__typename":"Episode", "id":"3"},
        "Episode:1":{"__typename":"Episode", "id":"1"}, "Episode:4":{"__typename":"Episode", "id":"4"},
        "Chapter:999":{"__typename":"Chapter", "id":"999"}
    })
}
#[test]
fn kakuyomu_v2_uses_defined_episode_unions_in_chapter_and_episode_order() {
    let html = format!(
        "{}<a href='/works/123/episodes/20'>合成部分目次</a>",
        kakuyomu_json_page(kakuyomu_v2_state())
    );
    let work = parse_work("https://kakuyomu.jp/works/123", &html).unwrap();
    assert_eq!(work.title, "合成作品");
    assert_eq!(
        work.episodes,
        [
            "https://kakuyomu.jp/works/123/episodes/20",
            "https://kakuyomu.jp/works/123/episodes/3",
            "https://kakuyomu.jp/works/123/episodes/1"
        ]
    );
}
#[test]
fn kakuyomu_v2_rejects_missing_or_mismatched_nodes_and_unknown_unions() {
    for change in 0..5 {
        let mut state = kakuyomu_v2_state();
        match change {
            0 => {
                state.as_object_mut().unwrap().remove("Episode:3");
            }
            1 => state["Episode:3"]["id"] = serde_json::json!("999"),
            2 => state["Episode:3"]["__typename"] = serde_json::json!("UnknownEpisode"),
            3 => state["TableOfContentsChapter:2"]["__typename"] = serde_json::json!("Chapter"),
            _ => state["TableOfContentsChapter:2"]["episodeUnions"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
        }
        let html = kakuyomu_json_page(state);
        assert!(
            parse_work("https://kakuyomu.jp/works/123", &html).is_err(),
            "invalid V2 case {change}"
        );
    }
}
fn alphapolis_json_page(chapters: Value) -> String {
    alphapolis_cover_page(serde_json::json!({
        "content":{"id":2, "url":"/novel/1/2/", "title":"合成作品", "isNovel":true},
        "chapterEpisodes":chapters, "isPartialEpisode":false, "isPreview":false
    }))
}
fn alphapolis_cover_page(data: Value) -> String {
    let json = data.to_string().replace('/', "\\/");
    format!("<div id='app-cover-episode'></div><script type='application/json' id='app-cover-data'>{json}</script>")
}
fn alphapolis_public_chapter(episodes: Value) -> Value {
    serde_json::json!({"isRental":false, "rentalBookId":null, "book":null, "episodes":episodes, "extraEpisode":null})
}
fn alphapolis_episode(id: u64, order: u64, public: bool, rental: Value) -> Value {
    serde_json::json!({"episodeNo":id, "url":format!("/novel/1/2/episode/{id}"), "dispOrder":order, "isPublic":public, "rental":rental})
}
#[test]
fn alphapolis_cover_json_only_reads_explicitly_public_non_rental_episodes() {
    let free = alphapolis_public_chapter(serde_json::json!([
        alphapolis_episode(20,2,true,Value::Null), alphapolis_episode(10,1,true,Value::Null),
        alphapolis_episode(30,3,false,Value::Null), alphapolis_episode(40,4,true,serde_json::json!({"id":4})),
        {"episodeNo":80,"url":"/novel/1/2/episode/80","dispOrder":8,"rental":null}
    ]));
    let mut rental = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
        50,
        5,
        true,
        Value::Null
    )]));
    rental["isRental"] = serde_json::json!(true);
    let mut rental_book = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
        60,
        6,
        true,
        Value::Null
    )]));
    rental_book["rentalBookId"] = serde_json::json!(6);
    let mut book = alphapolis_public_chapter(serde_json::json!([alphapolis_episode(
        70,
        7,
        true,
        Value::Null
    )]));
    book["book"] = serde_json::json!({"id":7});
    let html = format!("{}<div class='episode'><a href='/novel/1/2/episode/30'>非公開</a><a href='/novel/1/2/episode/40'>レンタル</a><a href='/novel/1/2/episode/50'>書籍</a></div>", alphapolis_json_page(serde_json::json!([free,rental,rental_book,book])));
    assert!(html.contains(r"\/novel\/1\/2\/episode\/20"));
    let work = parse_work("https://www.alphapolis.co.jp/novel/1/2/", &html).unwrap();
    assert_eq!(work.title, "合成作品");
    assert_eq!(
        work.episodes,
        [
            "https://www.alphapolis.co.jp/novel/1/2/episode/10",
            "https://www.alphapolis.co.jp/novel/1/2/episode/20"
        ]
    );
}
#[test]
fn alphapolis_cover_json_does_not_fall_back_to_rental_dom_links() {
    let chapter = alphapolis_public_chapter(serde_json::json!([
        alphapolis_episode(10, 1, false, Value::Null),
        alphapolis_episode(20, 2, true, serde_json::json!({"id":2}))
    ]));
    let html = format!(
        "{}<h1>合成作品</h1><a href='/novel/1/2/episode/20'>レンタル</a>",
        alphapolis_json_page(serde_json::json!([chapter]))
    );
    assert!(parse_work("https://www.alphapolis.co.jp/novel/1/2/", &html).is_err());
}
#[test]
fn alphapolis_cover_json_rejects_partial_mismatched_and_ambiguous_data() {
    for change in 0..5 {
        let mut data = serde_json::json!({
            "content":{"id":2,"url":"/novel/1/2/","title":"合成作品","isNovel":true},
            "chapterEpisodes":[alphapolis_public_chapter(serde_json::json!([alphapolis_episode(10,1,true,Value::Null)]))],
            "isPartialEpisode":false,"isPreview":false
        });
        match change {
            0 => data["isPartialEpisode"] = serde_json::json!(true),
            1 => data["content"]["url"] = serde_json::json!("/novel/1/999/"),
            2 => {
                data["chapterEpisodes"][0]["episodes"][0]["url"] =
                    serde_json::json!("/novel/1/2/episode/999")
            }
            3 => {
                let duplicate = data["chapterEpisodes"][0]["episodes"][0].clone();
                data["chapterEpisodes"][0]["episodes"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            _ => {
                data["chapterEpisodes"][0]["extraEpisode"] =
                    serde_json::json!({"url":"/novel/1/2/episode/30"})
            }
        }
        assert!(
            parse_work(
                "https://www.alphapolis.co.jp/novel/1/2/",
                &alphapolis_cover_page(data)
            )
            .is_err(),
            "invalid cover case {change}"
        );
    }
}
#[test]
fn alphapolis_saved_source_table_and_multiline_summary() {
    let source = "<table><tbody><tr><td class='line-number'>1</td><td class='line-content'>&lt;h1 class='episode-title'&gt;第一話&lt;/h1&gt;</td></tr><tr><td class='line-number'>2</td><td class='line-content'>&lt;div id='novelBody'&gt;&lt;p&gt;本文&lt;/p&gt;&lt;/div&gt;</td></tr></tbody></table>";
    let episode = parse_episode(Site::Alphapolis, source, false).unwrap();
    assert_eq!(episode.title, "第一話");
    assert_eq!(episode.lines, ["本文"]);
    let work = parse_work("https://www.alphapolis.co.jp/novel/1/2", "<h1>作品名</h1><div class='abstract'>一行目<br>二行目</div><a href='https://alphapolis.co.jp/novel/1/2/episode/3'>第一話</a>").unwrap();
    assert_eq!(work.description, "一行目\n二行目");
    assert_eq!(
        work.episodes,
        ["https://www.alphapolis.co.jp/novel/1/2/episode/3"]
    );
}
