pub const BMO_BASE: &str = "https://bugzilla.mozilla.org/rest";

pub struct BmoClient {
    api_key: String,
    base: String,
    agent: ureq::Agent,
}

impl BmoClient {
    pub fn new(api_key: &str) -> Self {
        Self::new_with_base(api_key, BMO_BASE)
    }

    pub fn new_with_base(api_key: &str, base: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base: base.trim_end_matches('/').to_string(),
            agent: ureq::Agent::new(),
        }
    }

    pub fn get(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let mut req = self
            .agent
            .get(&url)
            .set("X-BUGZILLA-API-KEY", &self.api_key);
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
            .agent
            .post(&url)
            .set("X-BUGZILLA-API-KEY", &self.api_key)
            .send_json(body.clone())
            .map_err(|e| anyhow::anyhow!("HTTP POST {url}: {e}"))?;
        let val: serde_json::Value = resp.into_json()?;
        Ok(val)
    }

    pub fn put(&self, path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let resp = self
            .agent
            .put(&url)
            .set("X-BUGZILLA-API-KEY", &self.api_key)
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
        let bug_resp = self.get(&format!("/bug/{bug_id}"), &[])?;
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
            .with_body(r#"{"bugs":[{"id":456,"summary":"No comments"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let val = client.get_bug(456, false).unwrap();
        assert_eq!(val["bug"]["id"], 456);
        assert!(val.get("comments").is_none());
    }

    #[test]
    fn test_search() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/bug")
            .match_query(mockito::Matcher::UrlEncoded(
                "savedsearch".into(),
                "media-meta".into(),
            ))
            .with_body(r#"{"bugs":[{"id":1,"summary":"bug one"},{"id":2,"summary":"bug two"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let client = make_client(&server);
        let bugs = client.search(&[("savedsearch", "media-meta")]).unwrap();
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
}
