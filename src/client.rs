pub const BMO_BASE: &str = "https://bugzilla.mozilla.org/rest";

pub struct BmoClient {
    /// `Some` → authenticated (sends `X-BUGZILLA-API-KEY`, sees private bugs,
    /// can write). `None` → anonymous: public reads only, stricter rate limits.
    api_key: Option<String>,
    base: String,
    agent: ureq::Agent,
}

impl BmoClient {
    pub fn new(api_key: &str) -> Self {
        Self::new_with_base(api_key, BMO_BASE)
    }

    pub fn new_with_base(api_key: &str, base: &str) -> Self {
        Self::build(Some(api_key.to_string()), base)
    }

    /// Anonymous client — sends no API key. Reads of **public** bugs only;
    /// writes are rejected by BMO and anonymous requests are rate-limited.
    pub fn anonymous() -> Self {
        Self::anonymous_with_base(BMO_BASE)
    }

    pub fn anonymous_with_base(base: &str) -> Self {
        Self::build(None, base)
    }

    fn build(api_key: Option<String>, base: &str) -> Self {
        Self {
            api_key,
            base: base.trim_end_matches('/').to_string(),
            agent: ureq::Agent::new(),
        }
    }

    /// Whether an API key is attached (write-capable, private bugs visible).
    pub fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }

    /// Attach the API-key header when present; a no-op for anonymous clients.
    fn auth(&self, req: ureq::Request) -> ureq::Request {
        match &self.api_key {
            Some(key) => req.set("X-BUGZILLA-API-KEY", key),
            None => req,
        }
    }

    pub fn get(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let mut req = self.auth(self.agent.get(&url));
        for (k, v) in params {
            req = req.query(k, v);
        }
        let resp = req
            .call()
            .map_err(|e| anyhow::anyhow!("HTTP GET {url}: {e}"))?;
        let val: serde_json::Value = resp.into_json()?;
        Ok(val)
    }

    pub fn post(&self, path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let resp = self
            .auth(self.agent.post(&url))
            .send_json(body.clone())
            .map_err(|e| anyhow::anyhow!("HTTP POST {url}: {e}"))?;
        let val: serde_json::Value = resp.into_json()?;
        Ok(val)
    }

    pub fn put(&self, path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let resp = self
            .auth(self.agent.put(&url))
            .send_json(body.clone())
            .map_err(|e| anyhow::anyhow!("HTTP PUT {url}: {e}"))?;
        let val: serde_json::Value = resp.into_json()?;
        Ok(val)
    }

    pub fn whoami(&self) -> anyhow::Result<serde_json::Value> {
        self.get("/whoami", &[])
    }

    pub fn get_bug(
        &self,
        bug_id: u64,
        include_comments: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let bug_resp = self.get(
            &format!("/bug/{bug_id}"),
            &[("include_fields", "_default,flags")],
        )?;
        let bug = bug_resp["bugs"][0].clone();
        let mut result = serde_json::json!({ "bug": bug });
        if include_comments {
            let comments_resp = self.get(&format!("/bug/{bug_id}/comment"), &[])?;
            let comments = comments_resp["bugs"][bug_id.to_string()]["comments"].clone();
            result["comments"] = comments;
        }
        Ok(result)
    }

    pub fn search(&self, params: &[(&str, &str)]) -> anyhow::Result<Vec<serde_json::Value>> {
        let resp = self.get("/bug", params)?;
        let bugs = resp["bugs"].as_array().cloned().unwrap_or_default();
        Ok(bugs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn make_client(server: &mockito::Server) -> BmoClient {
        BmoClient::new_with_base("test-key", &server.url())
    }

    #[test]
    fn test_whoami() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/whoami")
            .match_header("x-bugzilla-api-key", "test-key")
            .with_body(r#"{"id":1,"name":"bot@mozilla.com","real_name":"Bot"}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let val = client.whoami().unwrap();
        assert_eq!(val["name"], "bot@mozilla.com");
    }

    #[test]
    fn test_get_bug_with_comments() {
        let mut server = Server::new();
        let _m1 = server
            .mock("GET", "/bug/123")
            .match_query(mockito::Matcher::UrlEncoded(
                "include_fields".into(),
                "_default,flags".into(),
            ))
            .with_body(r#"{"bugs":[{"id":123,"summary":"Test bug"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let _m2 = server
            .mock("GET", "/bug/123/comment")
            .with_body(r#"{"bugs":{"123":{"comments":[{"id":1,"creator":"a@b.com","text":"hi","creation_time":"2026-04-01T00:00:00Z"}]}}}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let val = client.get_bug(123, true).unwrap();
        assert_eq!(val["bug"]["id"], 123);
        assert!(val["comments"].is_array());
        assert_eq!(val["comments"][0]["creator"], "a@b.com");
    }

    #[test]
    fn test_get_bug_no_comments() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug/456")
            .match_query(mockito::Matcher::UrlEncoded(
                "include_fields".into(),
                "_default,flags".into(),
            ))
            .with_body(r#"{"bugs":[{"id":456,"summary":"No comments"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let val = client.get_bug(456, false).unwrap();
        assert_eq!(val["bug"]["id"], 456);
        assert!(val.get("comments").is_none());
    }

    #[test]
    fn test_get_bug_includes_flags() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug/789")
            .match_query(mockito::Matcher::UrlEncoded(
                "include_fields".into(),
                "_default,flags".into(),
            ))
            .with_body(r#"{"bugs":[{"id":789,"summary":"Flag bug","flags":[{"name":"needinfo","status":"?","requestee":"dev@mozilla.com"}]}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let val = client.get_bug(789, false).unwrap();
        assert!(val["bug"]["flags"].is_array());
        assert_eq!(val["bug"]["flags"][0]["name"], "needinfo");
    }

    #[test]
    fn test_search() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug")
            .match_query(mockito::Matcher::UrlEncoded(
                "component".into(),
                "Audio/Video".into(),
            ))
            .with_body(r#"{"bugs":[{"id":1,"summary":"bug one"},{"id":2,"summary":"bug two"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let bugs = client.search(&[("component", "Audio/Video")]).unwrap();
        assert_eq!(bugs.len(), 2);
        assert_eq!(bugs[0]["id"], 1);
    }

    #[test]
    fn test_post_comment() {
        let mut server = Server::new();
        let _m = server
            .mock("POST", "/bug/789/comment")
            .with_body(r#"{"id":42}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let body = serde_json::json!({"comment": "Hello"});
        let val = client.post("/bug/789/comment", &body).unwrap();
        assert_eq!(val["id"], 42);
    }

    #[test]
    fn test_put_bug() {
        let mut server = Server::new();
        let _m = server
            .mock("PUT", "/bug/789")
            .with_body(r#"{"bugs":[{"id":789}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let body = serde_json::json!({"priority": "P2"});
        let val = client.put("/bug/789", &body).unwrap();
        assert_eq!(val["bugs"][0]["id"], 789);
    }

    #[test]
    fn test_search_by_summary() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("f1".into(), "short_desc".into()),
                mockito::Matcher::UrlEncoded("o1".into(), "substring".into()),
                mockito::Matcher::UrlEncoded("v1".into(), "mp4 crash".into()),
            ]))
            .with_body(r#"{"bugs":[{"id":100,"summary":"mp4 crash on startup","status":"NEW","priority":"P2"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let bugs = client
            .search(&[
                ("f1", "short_desc"),
                ("o1", "substring"),
                ("v1", "mp4 crash"),
            ])
            .unwrap();
        assert_eq!(bugs.len(), 1);
        assert_eq!(bugs[0]["summary"], "mp4 crash on startup");
        assert_eq!(bugs[0]["status"], "NEW");
    }

    #[test]
    fn test_http_error() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/whoami")
            .with_status(401)
            .with_body(r#"{"error":true,"message":"Auth failed"}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let result = client.whoami();
        assert!(result.is_err());
    }

    #[test]
    fn test_anonymous_client_sends_no_api_key() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug/123")
            .match_header("x-bugzilla-api-key", mockito::Matcher::Missing)
            .match_query(mockito::Matcher::UrlEncoded(
                "include_fields".into(),
                "_default,flags".into(),
            ))
            .with_body(r#"{"bugs":[{"id":123,"summary":"public bug"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = BmoClient::anonymous_with_base(&server.url());
        assert!(!client.is_authenticated());
        let val = client.get_bug(123, false).unwrap();
        assert_eq!(val["bug"]["id"], 123);
    }

    #[test]
    fn test_authenticated_client_reports_authenticated() {
        let client = BmoClient::new_with_base("k", "http://example.invalid");
        assert!(client.is_authenticated());
    }
}
