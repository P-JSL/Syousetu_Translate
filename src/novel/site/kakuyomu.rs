use super::dom::select;
use scraper::Html;
use serde_json::Value;
use std::collections::HashSet;
use url::Url;
pub(super) struct KakuyomuData {
    pub(super) title: String,
    pub(super) description: String,
    pub(super) episodes: Vec<String>,
    pub(super) episode_count: Option<usize>,
    pub(super) unresolved_toc: bool,
}
pub(super) fn episode_id(value: &Value) -> Option<String> {
    let id = match value {
        Value::String(id) => id.clone(),
        Value::Number(id) => id.as_u64()?.to_string(),
        _ => return None,
    };
    (!id.is_empty() && id.bytes().all(|c| c.is_ascii_digit())).then_some(id)
}
pub(super) fn kakuyomu_next_data(document: &Html, work_url: &Url) -> Option<KakuyomuData> {
    let selector = select("#__NEXT_DATA__").ok()?;
    let raw = document.select(&selector).next()?.inner_html();
    let json: Value = serde_json::from_str(&raw).ok()?;
    let apollo = json.pointer("/props/pageProps/__APOLLO_STATE__")?;
    let work_id = work_url.path_segments()?.nth(1)?;
    let obj = apollo.as_object()?;
    let info = obj.get(&format!("Work:{work_id}")).or_else(|| {
        obj.iter().find_map(|(key, value)| {
            let key = key.strip_prefix("Work:")?;
            let identity: Value = serde_json::from_str(key).ok()?;
            (identity.get("id").and_then(Value::as_str) == Some(work_id)).then_some(value)
        })
    })?;
    let title = info.get("title")?.as_str()?.to_owned();
    let desc = info
        .get("introduction")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut links = Vec::new();
    let episode_count = info
        .get("publicEpisodeCount")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok());
    let unresolved_toc = if let Some(toc) = info.get("tableOfContentsV2") {
        info.get("__typename").and_then(Value::as_str) != Some("Work")
            || info.get("id").and_then(Value::as_str) != Some(work_id)
            || !collect_episode_ids_v2(toc, obj, &mut links)
    } else {
        info.get("tableOfContents").is_some_and(|toc| {
            !collect_episode_ids(toc, obj, &mut links, &mut HashSet::new(), false, 0)
        })
    } || (info.get("publicEpisodeCount").is_some() && episode_count.is_none());
    let mut seen = HashSet::new();
    let links = links
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .map(|episode_id| format!("https://kakuyomu.jp/works/{work_id}/episodes/{episode_id}"))
        .collect();
    Some(KakuyomuData {
        title,
        description: desc,
        episodes: links,
        episode_count,
        unresolved_toc,
    })
}
fn kakuyomu_typed_node<'a>(
    value: &'a Value,
    state: &'a serde_json::Map<String, Value>,
    typename: &str,
) -> Option<&'a Value> {
    let node = if let Some(reference) = value.get("__ref").and_then(Value::as_str) {
        let id = reference.strip_prefix(&format!("{typename}:"))?;
        let node = state.get(reference)?;
        if episode_id(node.get("id")?)?.as_str() != id {
            return None;
        }
        node
    } else {
        value
    };
    if node.get("__typename").and_then(Value::as_str) != Some(typename)
        || episode_id(node.get("id")?).is_none()
    {
        return None;
    }
    Some(node)
}
/// V2 explicitly orders TableOfContentsChapter nodes and their Episode unions.
/// Resolve only those defined types, never unrelated objects in the Apollo cache.
fn collect_episode_ids_v2(
    value: &Value,
    state: &serde_json::Map<String, Value>,
    out: &mut Vec<String>,
) -> bool {
    let Some(chapters) = value.as_array() else {
        return false;
    };
    let mut complete = true;
    for chapter in chapters {
        let Some(chapter) = kakuyomu_typed_node(chapter, state, "TableOfContentsChapter") else {
            complete = false;
            continue;
        };
        let Some(episodes) = chapter.get("episodeUnions").and_then(Value::as_array) else {
            complete = false;
            continue;
        };
        for episode in episodes {
            let Some(episode) = kakuyomu_typed_node(episode, state, "Episode") else {
                complete = false;
                continue;
            };
            if episode.get("isPublic").and_then(Value::as_bool) == Some(false) {
                complete = false;
                continue;
            }
            if let Some(id) = episode.get("id").and_then(episode_id) {
                out.push(id);
            }
        }
    }
    complete
}
/// Follow the selected work's TOC references in array order. A numeric chapter
/// ID or an unrelated cached Episode object is not an episode in this work.
fn collect_episode_ids(
    value: &Value,
    state: &serde_json::Map<String, Value>,
    out: &mut Vec<String>,
    visiting: &mut HashSet<String>,
    episode_context: bool,
    depth: usize,
) -> bool {
    if depth > 64 {
        return false;
    }
    match value {
        Value::Array(xs) => {
            // Visit every node even after a failure: collection has side effects.
            let mut complete = true;
            for x in xs {
                complete = collect_episode_ids(x, state, out, visiting, episode_context, depth + 1)
                    && complete;
            }
            complete
        }
        Value::Object(map) => {
            if let Some(reference) = map.get("__ref").and_then(Value::as_str) {
                if let Some(id) = reference.strip_prefix("Episode:") {
                    let id = state
                        .get(reference)
                        .and_then(|value| value.get("id"))
                        .and_then(episode_id)
                        .or_else(|| episode_id(&Value::String(id.to_owned())));
                    if let Some(id) = id {
                        out.push(id);
                        return true;
                    }
                    return false;
                }
                if !visiting.insert(reference.to_owned()) {
                    return false;
                }
                let complete = state.get(reference).is_some_and(|target| {
                    collect_episode_ids(target, state, out, visiting, episode_context, depth + 1)
                });
                visiting.remove(reference);
                return complete;
            }
            let typename = map.get("__typename").and_then(Value::as_str);
            if typename == Some("Episode") || (episode_context && typename.is_none()) {
                if let Some(id) = map.get("id").and_then(episode_id) {
                    out.push(id);
                    return true;
                }
                if typename == Some("Episode") {
                    return false;
                }
            }
            let before = out.len();
            let complete = map
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "id" | "__typename" | "chapter"))
                .fold(true, |complete, (key, child)| {
                    let context = matches!(key.as_str(), "episode" | "episodes" | "episodeUnions");
                    collect_episode_ids(child, state, out, visiting, context, depth + 1) && complete
                });
            complete && (!episode_context || out.len() > before)
        }
        _ => !episode_context,
    }
}
