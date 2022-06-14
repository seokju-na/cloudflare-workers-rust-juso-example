use serde::{Deserialize, Serialize};
use std::str::FromStr;
use worker::*;

mod utils;

fn log_request(req: &Request) {
  console_log!(
    "{} - [{}], located at: {:?}, within: {}",
    Date::now().to_string(),
    req.path(),
    req.cf().coordinates().unwrap_or_default(),
    req.cf().region().unwrap_or_else(|| "unknown region".into())
  );
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
  log_request(&req);
  utils::set_panic_hook();

  let router = Router::new();
  router
    .post_async("/search", search_juso)
    .run(req, env)
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaceDocument {
  pub id: String,
  pub place_name: String,
  pub category_name: String,
  pub category_group_code: String,
  pub category_group_name: String,
  pub phone: String,
  pub address_name: String,
  pub road_address_name: String,
  pub x: String,
  pub y: String,
  pub place_url: String,
  pub distance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaceSearchMetadata {
  pub total_count: i32,
  pub pageable_count: i32,
  pub is_end: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaceSearchResult {
  pub meta: PlaceSearchMetadata,
  pub documents: Vec<PlaceDocument>,
}

async fn search_juso(req: Request, ctx: RouteContext<()>) -> Result<Response> {
  let keyword = req
    .url()?
    .query_pairs()
    .find_map(|(key, value)| {
      if key == "keyword" {
        return Some(value.to_string());
      }
      None
    })
    .unwrap_or_default();

  // 캐시가 있다면 반환
  let cache = ctx.kv("JUSO_CACHE")?;
  if let Ok(Some(result)) = cache.get(&keyword).json::<PlaceSearchResult>().await {
    console_log!("hit cache!: {}", keyword);
    return Response::from_json(&result);
  }

  // 카카오API 호출
  let kakao_api_key = ctx.secret("KAKAO_API_KEY")?.to_string();
  let mut headers = Headers::new();
  headers.append("Authorization", &format!("KakaoAK {}", kakao_api_key))?;

  let mut req_init = RequestInit::new();
  req_init.with_method(Method::Post).with_headers(headers);

  let mut req_url = Url::from_str("https://dapi.kakao.com/v2/local/search/keyword.json")?;
  let query = "query=".to_owned() + &keyword;
  req_url.set_query(Some(&query));

  let req = Request::new_with_init(req_url.as_str(), &req_init)?;
  let mut res = Fetch::Request(req).send().await?;

  match res.status_code() {
    200 => {
      // 결과값을 캐시에 저장
      let result = res.json::<PlaceSearchResult>().await?;
      console_log!("missed cache: {}", keyword);
      cache
        .put(&keyword, &result)?
        .expiration_ttl(60)
        .execute()
        .await?;

      Response::from_json(&result)
    }
    _ => Ok(res),
  }
}
