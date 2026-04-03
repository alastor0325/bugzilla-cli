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

    #[test]
    #[ignore]
    fn test_real_search_by_summary() {
        let c = client().expect("BUGZILLA_BOT_API_KEY not set");
        // Search for a well-known stable string that should always have open results.
        let bugs = c
            .search(&[
                ("query_format", "advanced"),
                ("f1", "short_desc"),
                ("o1", "substring"),
                ("v1", "crash"),
                ("bug_status", "NEW"),
                ("limit", "5"),
            ])
            .expect("search failed");
        assert!(!bugs.is_empty(), "expected at least one open crash bug");
        for bug in &bugs {
            assert!(bug["id"].is_number());
            assert!(bug["summary"].is_string());
        }
    }

    #[test]
    #[ignore]
    fn test_real_search_full_text() {
        let c = client().expect("BUGZILLA_BOT_API_KEY not set");
        // Full-text OR group: summary OR longdesc.
        let bugs = c
            .search(&[
                ("query_format", "advanced"),
                ("f1", "OP"),
                ("j1", "OR"),
                ("f2", "short_desc"),
                ("o2", "substring"),
                ("v2", "crash"),
                ("f3", "longdesc"),
                ("o3", "substring"),
                ("v3", "crash"),
                ("f4", "CP"),
                ("bug_status", "NEW"),
                ("limit", "5"),
            ])
            .expect("search failed");
        assert!(!bugs.is_empty());
    }
}
