use crate::models::{Episode, WorkPage};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};

pub const ENDPOINT: &str = "https://api.annict.com/graphql";
pub const WORK_QUERY: &str = "query($titles: [String!], $after: String) { searchWorks(titles: $titles, first: 20, after: $after) { nodes { annictId title media episodesCount noEpisodes seasonYear seasonName } pageInfo { endCursor hasNextPage } } }";
pub const EPISODE_QUERY: &str = "query($ids: [Int!], $after: String) { searchWorks(annictIds: $ids, first: 1) { nodes { episodes(first: 100, after: $after, orderBy: {field: SORT_NUMBER, direction: ASC}) { nodes { annictId number numberText sortNumber title } pageInfo { endCursor hasNextPage } } } } }";

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Animeta/0.1")
        .build()
        .map_err(|_| "HTTPクライアントを初期化できません".into())
}
pub async fn query(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    for attempt in 0..3 {
        let response = match client
            .post(endpoint)
            .bearer_auth(token)
            .json(&json!({"query":query,"variables":variables}))
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
                continue;
            }
            Err(_) => {
                return Err(
                    "Annictに接続できません。ネットワークを確認して再試行してください".into(),
                )
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err("Annictトークンが無効、または読み取り権限がありません".into());
        }
        if status.as_u16() == 429 || status.is_server_error() {
            if attempt < 2 {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(attempt + 1)
                    .clamp(1, 10);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            return Err(if status.as_u16() == 429 {
                "Annictの利用制限に達しました。時間を置いて再試行してください"
            } else {
                "Annictでエラーが発生しました。時間を置いて再試行してください"
            }
            .into());
        }
        if !status.is_success() {
            return Err(format!("Annictの応答エラー（HTTP {}）", status.as_u16()));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "Annictから不正な応答を受け取りました")?;
        if let Some(errors) = body
            .get("errors")
            .and_then(Value::as_array)
            .filter(|e| !e.is_empty())
        {
            let message = errors
                .iter()
                .filter_map(|e| e.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" / ");
            return Err(format!(
                "Annict API: {}",
                message
                    .replace(token, "[redacted]")
                    .chars()
                    .take(500)
                    .collect::<String>()
            ));
        }
        return body
            .get("data")
            .filter(|v| !v.is_null())
            .cloned()
            .ok_or("Annictの応答にデータがありません".into());
    }
    Err("Annictへの接続に失敗しました".into())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    end_cursor: Option<String>,
    has_next_page: bool,
}
pub async fn search(
    client: &reqwest::Client,
    token: &str,
    title: &str,
    after: Option<String>,
) -> Result<WorkPage, String> {
    let data = query(
        client,
        ENDPOINT,
        token,
        WORK_QUERY,
        json!({"titles":[title],"after":after}),
    )
    .await?;
    let connection = data
        .get("searchWorks")
        .filter(|v| !v.is_null())
        .ok_or("Annictの作品検索結果を取得できません")?;
    let works = serde_json::from_value(
        connection
            .get("nodes")
            .cloned()
            .ok_or("作品一覧がありません")?,
    )
    .map_err(|_| "作品データの形式が不正です")?;
    let page: PageInfo = serde_json::from_value(
        connection
            .get("pageInfo")
            .cloned()
            .ok_or("ページ情報がありません")?,
    )
    .map_err(|_| "ページ情報の形式が不正です")?;
    if page.has_next_page && page.end_cursor.is_none() {
        return Err("Annictのページ情報にカーソルがありません".into());
    }
    Ok(WorkPage {
        works,
        end_cursor: page.end_cursor,
        has_next_page: page.has_next_page,
    })
}
pub async fn episodes_at(
    client: &reqwest::Client,
    endpoint: &str,
    token: &str,
    work_id: i64,
) -> Result<Vec<Episode>, String> {
    let mut all = Vec::new();
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let data = query(
            client,
            endpoint,
            token,
            EPISODE_QUERY,
            json!({"ids":[work_id],"after":after}),
        )
        .await?;
        let work = data
            .pointer("/searchWorks/nodes/0")
            .ok_or("Annictに作品が見つかりません")?;
        let connection = work.get("episodes").ok_or("エピソード一覧がありません")?;
        let nodes: Vec<Episode> = serde_json::from_value(
            connection
                .get("nodes")
                .cloned()
                .ok_or("エピソード一覧がありません")?,
        )
        .map_err(|_| "エピソードデータの形式が不正です")?;
        all.extend(nodes);
        let page: PageInfo = serde_json::from_value(
            connection
                .get("pageInfo")
                .cloned()
                .ok_or("ページ情報がありません")?,
        )
        .map_err(|_| "ページ情報の形式が不正です")?;
        if !page.has_next_page {
            break;
        }
        let cursor = page
            .end_cursor
            .ok_or("Annictのページ情報にカーソルがありません")?;
        if !seen.insert(cursor.clone()) || seen.len() > 1000 {
            return Err("Annictのページ情報が重複しています".into());
        }
        after = Some(cursor);
    }
    all.sort_by_key(|e| e.sort_number);
    let mut ids = HashSet::new();
    all.retain(|e| ids.insert(e.annict_id));
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{body_partial_json, method},
        Mock, MockServer, ResponseTemplate,
    };
    #[tokio::test]
    async fn episodes_paginate_and_allow_nulls() {
        let server = MockServer::start().await;
        for (after, cursor, next, id, number) in [
            (Value::Null, json!("next"), true, 1, Value::Null),
            (json!("next"), Value::Null, false, 2, json!(2)),
        ] {
            Mock::given(method("POST")).and(body_partial_json(json!({"variables":{"after":after}}))).respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"searchWorks":{"nodes":[{"episodes":{"nodes":[{"annictId":id,"number":number,"numberText":null,"title":null,"sortNumber":id}],"pageInfo":{"endCursor":cursor,"hasNextPage":next}}}]}}}))).expect(1).mount(&server).await;
        }
        let result = episodes_at(&client().unwrap(), &server.uri(), "test", 10)
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].number, None);
    }
    #[tokio::test]
    async fn authentication_and_graphql_errors_are_not_empty_results() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        assert!(query(
            &client().unwrap(),
            &server.uri(),
            "secret",
            "query {}",
            json!({})
        )
        .await
        .unwrap_err()
        .contains("トークン"));
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"errors":[{"message":"bad secret"}]})),
            )
            .mount(&server)
            .await;
        let err = query(
            &client().unwrap(),
            &server.uri(),
            "secret",
            "query {}",
            json!({}),
        )
        .await
        .unwrap_err();
        assert!(!err.contains("secret"));
    }
    #[tokio::test]
    async fn transient_failure_retries() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"ok":true}})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            query(
                &client().unwrap(),
                &server.uri(),
                "test",
                "query {}",
                json!({})
            )
            .await
            .unwrap()["ok"],
            true
        );
    }
}
