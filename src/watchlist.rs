use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};

use crate::client::BmoClient;

pub const WATCH_FILE: &str = "firefox-triage/ni-watch.json";

pub fn has_replied(
    comments: &[serde_json::Value],
    targets: &std::collections::HashSet<String>,
    ni_date: DateTime<Utc>,
) -> bool {
    comments.iter().any(|c| {
        let creator = c["creator"].as_str().unwrap_or("");
        let time: Option<DateTime<Utc>> = c["creation_time"].as_str().and_then(|s| s.parse().ok());
        targets.contains(creator) && time.map(|t| t > ni_date).unwrap_or(false)
    })
}

pub fn ni_is_cleared(
    flags: &[serde_json::Value],
    targets: &std::collections::HashSet<String>,
) -> bool {
    !targets.iter().any(|target| {
        flags.iter().any(|f| {
            f["name"].as_str() == Some("needinfo")
                && f["status"].as_str() == Some("?")
                && f["requestee"].as_str() == Some(target.as_str())
        })
    })
}

pub struct WatchList {
    path: PathBuf,
    data: HashMap<String, serde_json::Value>,
}

impl WatchList {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
        } else {
            HashMap::new()
        };
        Ok(Self {
            path: path.to_path_buf(),
            data,
        })
    }

    fn save(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    pub fn add(
        &mut self,
        bug_id: &str,
        title: &str,
        ni_targets: &[String],
        ni_set_date: &str,
    ) -> anyhow::Result<()> {
        self.data.insert(
            bug_id.to_string(),
            serde_json::json!({
                "title": title,
                "ni_targets": ni_targets,
                "ni_set_date": ni_set_date,
            }),
        );
        self.save()
    }

    pub fn remove(&mut self, bug_id: &str) -> anyhow::Result<bool> {
        let existed = self.data.remove(bug_id).is_some();
        if existed {
            self.save()?;
        }
        Ok(existed)
    }

    pub fn all(&self) -> &HashMap<String, serde_json::Value> {
        &self.data
    }

    pub fn poll(&mut self, client: &BmoClient) -> anyhow::Result<serde_json::Value> {
        let mut replied: Vec<String> = Vec::new();
        let mut ni_cleared: Vec<String> = Vec::new();
        let mut stale: Vec<String> = Vec::new();
        let mut auto_removed: Vec<String> = Vec::new();
        let mut inaccessible: Vec<String> = Vec::new();

        let now = Utc::now();
        let bug_ids: Vec<String> = self.data.keys().cloned().collect();

        for bug_id in bug_ids {
            let entry = match self.data.get(&bug_id) {
                Some(e) => e.clone(),
                None => continue,
            };

            let bug_id_u64: u64 = match bug_id.parse() {
                Ok(n) => n,
                Err(_) => continue,
            };

            let data = match client.get_bug(bug_id_u64, true) {
                Ok(d) => d,
                Err(_) => {
                    inaccessible.push(bug_id.clone());
                    self.data.remove(&bug_id);
                    continue;
                }
            };

            let ni_set_date_str = entry["ni_set_date"].as_str().unwrap_or("");
            let ni_date: DateTime<Utc> = match ni_set_date_str.parse() {
                Ok(d) => d,
                Err(_) => continue,
            };

            let targets: std::collections::HashSet<String> = entry["ni_targets"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            let comments = data["comments"].as_array().cloned().unwrap_or_default();
            let flags = data["bug"]["flags"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            if has_replied(&comments, &targets, ni_date) {
                replied.push(bug_id.clone());
                self.data.remove(&bug_id);
            } else if ni_is_cleared(flags, &targets) {
                ni_cleared.push(bug_id.clone());
                self.data.remove(&bug_id);
            } else {
                let age_days = (now - ni_date).num_days();
                if age_days >= 30 {
                    auto_removed.push(bug_id.clone());
                    self.data.remove(&bug_id);
                } else if age_days >= 14 {
                    stale.push(bug_id.clone());
                }
            }
        }

        self.save()?;

        Ok(serde_json::json!({
            "replied": replied,
            "ni_cleared": ni_cleared,
            "stale": stale,
            "auto_removed": auto_removed,
            "inaccessible": inaccessible,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use mockito::Server;
    use tempfile::TempDir;

    fn make_client(server: &mockito::Server) -> BmoClient {
        BmoClient::new_with_base("test-key", &server.url())
    }

    fn targets(emails: &[&str]) -> std::collections::HashSet<String> {
        emails.iter().map(|s| s.to_string()).collect()
    }

    // --- pure function unit tests ---

    #[test]
    fn test_has_replied_true() {
        let ni_date: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let reply_time = "2026-01-02T00:00:00Z";
        let comments = vec![serde_json::json!({
            "creator": "dev@mozilla.com",
            "creation_time": reply_time,
            "text": "done"
        })];
        assert!(has_replied(
            &comments,
            &targets(&["dev@mozilla.com"]),
            ni_date
        ));
    }

    #[test]
    fn test_has_replied_false_wrong_creator() {
        let ni_date: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let comments = vec![serde_json::json!({
            "creator": "other@mozilla.com",
            "creation_time": "2026-01-02T00:00:00Z",
            "text": "unrelated"
        })];
        assert!(!has_replied(
            &comments,
            &targets(&["dev@mozilla.com"]),
            ni_date
        ));
    }

    #[test]
    fn test_has_replied_false_before_ni_date() {
        let ni_date: DateTime<Utc> = "2026-01-10T00:00:00Z".parse().unwrap();
        let comments = vec![serde_json::json!({
            "creator": "dev@mozilla.com",
            "creation_time": "2026-01-05T00:00:00Z",
            "text": "old comment"
        })];
        assert!(!has_replied(
            &comments,
            &targets(&["dev@mozilla.com"]),
            ni_date
        ));
    }

    #[test]
    fn test_ni_is_cleared_true_when_no_active_flag() {
        let flags = vec![serde_json::json!({
            "name": "needinfo", "status": "-", "requestee": "dev@mozilla.com"
        })];
        assert!(ni_is_cleared(&flags, &targets(&["dev@mozilla.com"])));
    }

    #[test]
    fn test_ni_is_cleared_false_when_flag_active() {
        let flags = vec![serde_json::json!({
            "name": "needinfo", "status": "?", "requestee": "dev@mozilla.com"
        })];
        assert!(!ni_is_cleared(&flags, &targets(&["dev@mozilla.com"])));
    }

    #[test]
    fn test_ni_is_cleared_true_when_no_flags_at_all() {
        assert!(ni_is_cleared(&[], &targets(&["dev@mozilla.com"])));
    }

    // --- integration-style poll tests ---

    fn mock_bug(server: &mut Server, id: u64, flags_json: &str, comments_json: &str) {
        server
            .mock("GET", format!("/bug/{id}").as_str())
            .match_query(mockito::Matcher::UrlEncoded(
                "include_fields".into(),
                "_default,flags".into(),
            ))
            .with_body(format!(
                r#"{{"bugs":[{{"id":{id},"summary":"Bug {id}","flags":{flags_json}}}]}}"#
            ))
            .with_header("content-type", "application/json")
            .create();
        server
            .mock("GET", format!("/bug/{id}/comment").as_str())
            .with_body(format!(
                r#"{{"bugs":{{"{id}":{{"comments":{comments_json}}}}}}}"#
            ))
            .with_header("content-type", "application/json")
            .create();
    }

    #[test]
    fn test_add_and_persist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "123",
            "Test bug",
            &["a@b.com".to_string()],
            "2026-04-01T00:00:00Z",
        )
        .unwrap();
        drop(wl);
        let wl2 = WatchList::load(&path).unwrap();
        assert!(wl2.all().contains_key("123"));
        assert_eq!(wl2.all()["123"]["title"], "Test bug");
    }

    #[test]
    fn test_remove_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add("456", "Another bug", &[], "2026-04-01T00:00:00Z")
            .unwrap();
        assert!(wl.remove("456").unwrap());
        assert!(!wl.all().contains_key("456"));
    }

    #[test]
    fn test_remove_nonexistent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        assert!(!wl.remove("999").unwrap());
    }

    #[test]
    fn test_poll_detects_reply() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(2);
        let reply_time = (Utc::now() - Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        mock_bug(
            &mut server,
            100,
            "[]",
            &format!(
                r#"[{{"id":1,"creator":"reporter@example.com","text":"done","creation_time":"{reply_time}"}}]"#
            ),
        );
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "100",
            "Test bug",
            &["reporter@example.com".to_string()],
            &ni_date_str,
        )
        .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(
            result["replied"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("100"))
        );
        assert!(!wl.all().contains_key("100"));
    }

    #[test]
    fn test_poll_stale_after_14_days() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(15);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        mock_bug(
            &mut server,
            200,
            r#"[{"name":"needinfo","status":"?","requestee":"no-reply@example.com"}]"#,
            "[]",
        );
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "200",
            "Stale bug",
            &["no-reply@example.com".to_string()],
            &ni_date_str,
        )
        .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(
            result["stale"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("200"))
        );
        assert!(wl.all().contains_key("200")); // still in list
    }

    #[test]
    fn test_poll_not_stale_before_14_days() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(7); // 7 days < threshold
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        mock_bug(
            &mut server,
            201,
            r#"[{"name":"needinfo","status":"?","requestee":"no-reply@example.com"}]"#,
            "[]",
        );
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "201",
            "Pending bug",
            &["no-reply@example.com".to_string()],
            &ni_date_str,
        )
        .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(result["stale"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_poll_ni_cleared() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(3);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        // flags empty = NI was cleared without a comment
        mock_bug(&mut server, 300, "[]", "[]");
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "300",
            "NI cleared bug",
            &["dev@mozilla.com".to_string()],
            &ni_date_str,
        )
        .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(
            result["ni_cleared"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("300"))
        );
        assert!(!wl.all().contains_key("300")); // removed from list
    }

    #[test]
    fn test_poll_auto_remove_after_30_days() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(31);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        mock_bug(
            &mut server,
            400,
            r#"[{"name":"needinfo","status":"?","requestee":"ghost@mozilla.com"}]"#,
            "[]",
        );
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add(
            "400",
            "Ancient bug",
            &["ghost@mozilla.com".to_string()],
            &ni_date_str,
        )
        .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(
            result["auto_removed"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("400"))
        );
        assert!(!wl.all().contains_key("400")); // purged
    }

    #[test]
    fn test_poll_inaccessible() {
        let mut server = Server::new();
        // 404 → fetch fails → inaccessible
        server
            .mock("GET", "/bug/500")
            .with_status(404)
            .with_body(r#"{"error":true,"message":"Not found"}"#)
            .with_header("content-type", "application/json")
            .create();
        let ni_date = Utc::now() - Duration::days(1);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        wl.add("500", "Gone bug", &["x@y.com".to_string()], &ni_date_str)
            .unwrap();
        let client = make_client(&server);
        let result = wl.poll(&client).unwrap();
        assert!(
            result["inaccessible"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("500"))
        );
        assert!(!wl.all().contains_key("500"));
    }
}
