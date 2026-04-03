use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};

use crate::client::BmoClient;

pub const WATCH_FILE: &str = "firefox-triage/ni-watch.json";

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
        let mut stale: Vec<String> = Vec::new();
        let mut removed: Vec<String> = Vec::new();

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
                    removed.push(bug_id.clone());
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

            let has_reply = comments.iter().any(|c| {
                let creator = c["creator"].as_str().unwrap_or("");
                let time_str = c["creation_time"].as_str().unwrap_or("");
                let time: Option<DateTime<Utc>> = time_str.parse().ok();
                targets.contains(creator) && time.map(|t| t > ni_date).unwrap_or(false)
            });

            if has_reply {
                replied.push(bug_id.clone());
                self.data.remove(&bug_id);
            } else {
                let age_days = (Utc::now() - ni_date).num_days();
                if age_days >= 7 {
                    stale.push(bug_id.clone());
                }
            }
        }

        self.save()?;

        Ok(serde_json::json!({
            "replied": replied,
            "stale": stale,
            "removed": removed,
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
        let removed = wl.remove("456").unwrap();
        assert!(removed);
        assert!(!wl.all().contains_key("456"));
    }

    #[test]
    fn test_remove_nonexistent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ni-watch.json");
        let mut wl = WatchList::load(&path).unwrap();
        let removed = wl.remove("999").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_poll_detects_reply() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(2);
        let reply_time = Utc::now() - Duration::hours(1);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let reply_time_str = reply_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let _m1 = server
            .mock("GET", "/bug/100")
            .with_body(r#"{"bugs":[{"id":100,"summary":"Test"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let _m2 = server
            .mock("GET", "/bug/100/comment")
            .with_body(format!(
                r#"{{"bugs":{{"100":{{"comments":[{{"id":1,"creator":"reporter@example.com","text":"Here is the info","creation_time":"{reply_time_str}"}}]}}}}}}"#
            ))
            .with_header("content-type", "application/json")
            .create();

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
    fn test_poll_stale_after_7_days() {
        let mut server = Server::new();
        let ni_date = Utc::now() - Duration::days(8);
        let ni_date_str = ni_date.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let _m1 = server
            .mock("GET", "/bug/200")
            .with_body(r#"{"bugs":[{"id":200,"summary":"Stale bug"}]}"#)
            .with_header("content-type", "application/json")
            .create();
        let _m2 = server
            .mock("GET", "/bug/200/comment")
            .with_body(r#"{"bugs":{"200":{"comments":[]}}}"#)
            .with_header("content-type", "application/json")
            .create();

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
        assert!(wl.all().contains_key("200"));
    }
}
