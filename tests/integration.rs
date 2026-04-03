// Integration tests against real BMO. Run with: cargo test -- --ignored
// Requires BUGZILLA_BOT_API_KEY env var.

#[cfg(test)]
mod integration {
    use bugzilla_cli::client::BmoClient;

    fn client() -> Option<BmoClient> {
        let key = std::env::var("BUGZILLA_BOT_API_KEY").ok()?;
        Some(BmoClient::new(&key))
    }

    #[test]
    #[ignore]
    fn test_real_whoami() {
        let c = client().expect("BUGZILLA_BOT_API_KEY not set");
        let me = c.whoami().expect("whoami failed");
        assert!(me["id"].is_number(), "expected id field in whoami response");
    }

    #[test]
    #[ignore]
    fn test_real_get_bug() {
        let c = client().expect("BUGZILLA_BOT_API_KEY not set");
        let data = c.get_bug(1, false).expect("get_bug failed");
        assert!(data["bug"]["id"].is_number());
    }

    #[test]
    #[ignore]
    fn test_real_search() {
        let c = client().expect("BUGZILLA_BOT_API_KEY not set");
        let bugs = c.search(&[("id", "1")]).expect("search failed");
        assert!(!bugs.is_empty());
    }
}
