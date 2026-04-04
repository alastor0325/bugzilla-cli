use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Context;
use chrono::{Datelike, Local, NaiveDate};
use clap::{Parser, Subcommand};
use serde_json::json;

use bugzilla_cli::client::{BMO_BASE, BmoClient};
use bugzilla_cli::watchlist::{WATCH_FILE, WatchList};

fn watch_file_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(WATCH_FILE)
}

fn triage_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join("firefox-triage")
}

fn monday_of_current_week() -> NaiveDate {
    let today = Local::now().date_naive();
    let days_from_monday = today.weekday().num_days_from_monday();
    today - chrono::Duration::days(days_from_monday as i64)
}

fn read_secrets_file() -> Option<String> {
    let path = dirs::home_dir()?.join(".config/triage/secrets");
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        let kv = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        if let Some(val) = kv.strip_prefix("BUGZILLA_BOT_API_KEY=")
            && !val.is_empty()
        {
            return Some(val.to_string());
        }
    }
    None
}

fn is_configured() -> bool {
    if let Ok(key) = std::env::var("BUGZILLA_BOT_API_KEY")
        && !key.is_empty()
    {
        return true;
    }
    read_secrets_file().is_some()
}

fn get_client() -> anyhow::Result<BmoClient> {
    if let Ok(key) = std::env::var("BUGZILLA_BOT_API_KEY")
        && !key.is_empty()
    {
        return Ok(BmoClient::new(&key));
    }
    if let Some(key) = read_secrets_file() {
        return Ok(BmoClient::new(&key));
    }
    anyhow::bail!("Not configured. Run `bugzilla-cli setup`.")
}

fn prompt(label: &str) -> anyhow::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[derive(Parser)]
#[command(name = "bugzilla-cli", about = "Thin BMO REST client.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the interactive setup wizard (API key, triage directory, secrets file).
    Setup,

    /// Show a bug's metadata and optionally its full comment thread.
    Get {
        /// Bug ID to fetch.
        id: u64,
        /// Omit comments (only show bug metadata).
        #[arg(long = "no-comments", action = clap::ArgAction::SetFalse)]
        comments: bool,
    },

    /// Fetch triage-queue bugs filed in a date range (defaults to the current ISO week).
    Fetch {
        /// Start date (inclusive). Defaults to Monday of the current week.
        #[arg(long, value_name = "YYYY-MM-DD")]
        start: Option<String>,
        /// End date (exclusive). Defaults to the following Monday.
        #[arg(long, value_name = "YYYY-MM-DD")]
        end: Option<String>,
    },

    /// Post a comment on a bug.
    PostComment {
        /// Bug ID to comment on.
        id: u64,
        /// Comment text.
        text: String,
    },

    /// Set one or more needinfo flags on a bug in a single PUT.
    SetNi {
        /// Bug ID.
        id: u64,
        /// One or more email addresses to needinfo.
        #[arg(required = true)]
        email: Vec<String>,
    },

    /// Update structured fields on a bug (priority, severity, resolution, blocks, keywords).
    SetFields {
        /// Bug ID.
        id: u64,
        /// Set priority (P1–P5 or -- to clear).
        #[arg(long, value_parser = ["P1", "P2", "P3", "P4", "P5", "--"])]
        priority: Option<String>,
        /// Set severity (S1–S4 or -- to clear).
        #[arg(long, value_parser = ["S1", "S2", "S3", "S4", "--"])]
        severity: Option<String>,
        /// Set bug status (e.g. RESOLVED, NEW, ASSIGNED).
        #[arg(long, value_parser = ["UNCONFIRMED", "NEW", "ASSIGNED", "REOPENED", "RESOLVED", "VERIFIED"])]
        status: Option<String>,
        /// Set resolution code (FIXED, DUPLICATE, WONTFIX, etc.). BMO requires both --status RESOLVED and --resolution to close a bug.
        #[arg(long)]
        resolution: Option<String>,
        /// Add one or more bug IDs to the blocks list.
        #[arg(long, num_args = 1..)]
        blocks_add: Vec<u64>,
        /// Add one or more keywords.
        #[arg(long, num_args = 1..)]
        keywords_add: Vec<String>,
        /// Add one or more email addresses to the CC list.
        #[arg(long, num_args = 1..)]
        cc_add: Vec<String>,
    },

    /// Apply a pending draft from ~/firefox-triage/pending/bug-{id}.json (comment, NI, field updates).
    Apply {
        /// Bug ID whose draft to apply.
        id: u64,
    },

    /// Start watching a bug for needinfo replies.
    WatchAdd {
        /// Bug ID to watch.
        id: u64,
        /// Short title for display in poll output.
        #[arg(long, required = true)]
        title: String,
        /// Email address(es) that were needinfo'd.
        #[arg(long = "ni", required = true, num_args = 1..)]
        ni: Vec<String>,
    },

    /// Stop watching a bug (remove from the NI watch list).
    WatchRemove {
        /// Bug ID to stop watching.
        id: u64,
    },

    /// Poll all watched bugs and report which NI targets have replied or gone stale (≥7 days).
    WatchPoll,

    /// Print the BMO login (email) associated with the stored API key.
    Whoami,

    /// Search open bugs by summary substring. Use --full-text to also search comments.
    Search {
        /// Substring to search for in bug summaries.
        query: String,
        /// Filter by component (repeatable).
        #[arg(long, num_args = 1..)]
        component: Vec<String>,
        /// Filter by product (default: all products).
        #[arg(long)]
        product: Option<String>,
        /// Maximum number of results to return.
        #[arg(long, default_value = "25")]
        limit: u32,
        /// Also search comments and descriptions (slower, more results).
        #[arg(long)]
        full_text: bool,
        /// Include resolved/closed bugs (default: open bugs only).
        #[arg(long)]
        all_statuses: bool,
    },
}

fn cmd_setup() -> anyhow::Result<()> {
    println!("=== bugzilla-cli setup ===");

    // Step 1: BMO URL
    println!();
    println!("Step 1: BMO server URL");
    println!("  This is the base URL of the Bugzilla instance you want to connect to.");
    println!("  For Mozilla's BMO, just press Enter to accept the default.");
    let url_input = prompt(&format!(
        "  BMO URL (press Enter for default: {}): ",
        BMO_BASE
    ))?;
    let url = if url_input.is_empty() {
        BMO_BASE.to_string()
    } else {
        let trimmed = url_input.trim_end_matches('/');
        if trimmed.ends_with("/rest") {
            trimmed.to_string()
        } else {
            format!("{trimmed}/rest")
        }
    };
    println!("  \u{2713} Using {url}");

    // Step 2: API key
    println!();
    println!("Step 2: BMO API key");
    println!("  Generate one at: https://bugzilla.mozilla.org/userprefs.cgi?tab=apikey");
    println!("  The key will be verified against BMO before being saved.");
    let api_key = prompt("  API key: ")?;
    if api_key.is_empty() {
        anyhow::bail!("API key is required.");
    }
    let test_client = BmoClient::new_with_base(&api_key, &url);
    print!("  Verifying... ");
    io::stdout().flush()?;
    let me = test_client.whoami().context("Authentication failed")?;
    println!("\u{2713} Authenticated as {}", format_whoami(&me));

    // Step 3: Triage directory
    let default_triage = triage_dir().display().to_string();
    println!();
    println!("Step 3: Local triage directory");
    println!("  A local folder where fetched bug snapshots, pending comment drafts,");
    println!("  and triage reports will be stored. Created automatically if it doesn't exist.");
    println!("  Press Enter to accept the default, or type a different path.");
    let triage_input = prompt(&format!("  Triage directory [{}]: ", default_triage))?;
    let triage_path = PathBuf::from(if triage_input.is_empty() {
        default_triage
    } else {
        triage_input
    });
    for sub in ["bugs", "pending", "reports", "archive", "knowledge"] {
        std::fs::create_dir_all(triage_path.join(sub))?;
    }
    println!(
        "  \u{2713} Directories created under {}",
        triage_path.display()
    );

    // Step 4: Secrets file
    println!();
    println!("Step 4: Saving credentials");
    println!("  Your API key will be written to ~/.config/triage/secrets (chmod 600).");
    println!("  That file is outside this repo and never committed.");
    let secrets_file = dirs::home_dir().unwrap().join(".config/triage/secrets");
    std::fs::create_dir_all(secrets_file.parent().unwrap())?;
    std::fs::write(
        &secrets_file,
        format!("export BUGZILLA_BOT_API_KEY={api_key}\n"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&secrets_file, std::fs::Permissions::from_mode(0o600))?;
    }
    println!("  \u{2713} API key saved to {}", secrets_file.display());

    println!();
    println!("Add this to your ~/.zshrc:");
    println!("  source {}", secrets_file.display());
    println!();
    println!("Setup complete.");
    Ok(())
}

fn format_bug_header(bug: &serde_json::Value) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Bug {}: {}\n",
        bug["id"],
        bug["summary"].as_str().unwrap_or("?")
    ));
    out.push_str(&format!(
        "  Status:   {} {}\n",
        bug["status"].as_str().unwrap_or("?"),
        bug["resolution"].as_str().unwrap_or("")
    ));
    out.push_str(&format!(
        "  Priority: {}  Severity: {}\n",
        bug["priority"].as_str().unwrap_or("?"),
        bug["severity"].as_str().unwrap_or("?")
    ));
    out.push_str(&format!(
        "  Assigned: {}\n",
        bug["assigned_to"].as_str().unwrap_or("?")
    ));
    out.push_str(&format!(
        "  Creator:  {} ({})\n",
        bug["creator"].as_str().unwrap_or("?"),
        bug["creator_detail"]["real_name"].as_str().unwrap_or("?")
    ));
    let qa_wb = bug["cf_qa_whiteboard"].as_str().unwrap_or("");
    if !qa_wb.is_empty() {
        out.push_str(&format!("  QA:       {qa_wb}\n"));
    }
    out
}

fn format_comments(comments: &[serde_json::Value]) -> String {
    let mut out = format!("\n--- {} comment(s) ---", comments.len());
    for c in comments {
        out.push_str(&format!(
            "\n\n[{}] {}:\n{}",
            c["creation_time"].as_str().unwrap_or("?"),
            c["creator"].as_str().unwrap_or("?"),
            c["text"].as_str().unwrap_or("")
        ));
    }
    out
}

fn cmd_get(id: u64, comments: bool) -> anyhow::Result<()> {
    let client = get_client()?;
    let data = client.get_bug(id, comments)?;
    print!("{}", format_bug_header(&data["bug"]));
    if comments {
        let empty = vec![];
        let clist = data["comments"].as_array().unwrap_or(&empty);
        println!("{}", format_comments(clist));
    }
    Ok(())
}

const TRIAGE_COMPONENTS: &[&str] = &[
    "Audio/Video",
    "Audio/Video: cubeb",
    "Audio/Video: GMP",
    "Audio/Video: MediaStreamGraph",
    "Audio/Video: Playback",
    "Audio/Video: Recording",
    "Web Audio",
    "Audio/Video: Web Codecs",
];

fn cmd_fetch(start: Option<String>, end: Option<String>) -> anyhow::Result<()> {
    let client = get_client()?;

    let start_date = start.unwrap_or_else(|| monday_of_current_week().to_string());
    let end_date =
        end.unwrap_or_else(|| (monday_of_current_week() + chrono::Duration::days(7)).to_string());

    // Use Bugzilla advanced query format, mirroring the canonical triage search URL.
    let mut params: Vec<(&str, &str)> = vec![
        ("query_format", "advanced"),
        ("emailassigned_to1", "1"),
        ("email1", "nobody@mozilla.org"),
        ("emailtype1", "exact"),
        ("keywords_type", "nowords"),
        ("keywords", "meta"),
        // f1/f4: open/close paren grouping for date range
        ("f1", "OP"),
        ("f2", "creation_ts"),
        ("o2", "changedafter"),
        ("v2", &start_date),
        ("f3", "creation_ts"),
        ("o3", "changedafter"),
        ("n3", "1"),
        ("v3", &end_date),
        ("f4", "CP"),
        // severity not yet set
        ("f5", "bug_severity"),
        ("o5", "equals"),
        ("v5", "--"),
        // defects only
        ("f6", "bug_type"),
        ("o6", "equals"),
        ("v6", "defect"),
        // exclude security bugs
        ("f9", "bug_group"),
        ("o9", "notsubstring"),
        ("v9", "core-security"),
    ];
    for component in TRIAGE_COMPONENTS {
        params.push(("component", component));
    }
    for status in ["UNCONFIRMED", "NEW", "ASSIGNED", "REOPENED"] {
        params.push(("bug_status", status));
    }

    let mut bugs = client.search(&params)?;

    bugs.sort_by(|a, b| {
        let ta = a["creation_time"].as_str().unwrap_or("");
        let tb = b["creation_time"].as_str().unwrap_or("");
        ta.cmp(tb)
    });

    println!("{}", serde_json::to_string_pretty(&bugs)?);
    eprintln!("\n# {} bug(s) fetched", bugs.len());
    Ok(())
}

fn cmd_post_comment(id: u64, text: &str) -> anyhow::Result<()> {
    let client = get_client()?;
    let body = json!({"comment": text});
    let result = client.post(&format!("/bug/{id}/comment"), &body)?;
    println!("Comment {} posted to bug {}.", result["id"], id);
    Ok(())
}

fn cmd_set_ni(id: u64, emails: &[String]) -> anyhow::Result<()> {
    let client = get_client()?;
    let flags: Vec<serde_json::Value> = emails
        .iter()
        .map(|email| json!({"name": "needinfo", "status": "?", "requestee": email}))
        .collect();
    client.put(&format!("/bug/{id}"), &json!({"flags": flags}))?;
    println!("NI set on bug {id} for: {}", emails.join(", "));
    Ok(())
}

fn build_set_fields_body(
    priority: Option<&str>,
    severity: Option<&str>,
    status: Option<&str>,
    resolution: Option<&str>,
    blocks_add: &[u64],
    keywords_add: &[String],
    cc_add: &[String],
) -> serde_json::Map<String, serde_json::Value> {
    let mut body = serde_json::Map::new();
    if let Some(p) = priority {
        body.insert("priority".into(), json!(p));
    }
    if let Some(s) = severity {
        body.insert("severity".into(), json!(s));
    }
    if let Some(st) = status {
        body.insert("status".into(), json!(st));
    }
    if let Some(r) = resolution {
        body.insert("resolution".into(), json!(r));
    }
    if !blocks_add.is_empty() {
        body.insert("blocks".into(), json!({"add": blocks_add}));
    }
    if !keywords_add.is_empty() {
        body.insert("keywords".into(), json!({"add": keywords_add}));
    }
    if !cc_add.is_empty() {
        body.insert("cc".into(), json!({"add": cc_add}));
    }
    body
}

fn cmd_set_fields(id: u64, body: serde_json::Map<String, serde_json::Value>) -> anyhow::Result<()> {
    let client = get_client()?;
    if body.is_empty() {
        println!("Nothing to update.");
        return Ok(());
    }
    client.put(
        &format!("/bug/{id}"),
        &serde_json::Value::Object(body.clone()),
    )?;
    println!("Bug {id} updated: {:?}", body.keys().collect::<Vec<_>>());
    Ok(())
}

fn build_apply_field_body(draft: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut body = serde_json::Map::new();
    for field in ["priority", "severity", "status", "resolution"] {
        if let Some(v) = draft[field]
            .as_str()
            .filter(|s| !s.is_empty() && *s != "null")
        {
            body.insert(field.into(), json!(v));
        }
    }
    if let Some(arr) = draft["blocks_add"].as_array().filter(|a| !a.is_empty()) {
        body.insert("blocks".into(), json!({"add": arr}));
    }
    if let Some(arr) = draft["keywords_add"].as_array().filter(|a| !a.is_empty()) {
        body.insert("keywords".into(), json!({"add": arr}));
    }
    if let Some(arr) = draft["cc_add"].as_array().filter(|a| !a.is_empty()) {
        body.insert("cc".into(), json!({"add": arr}));
    }
    body
}

fn cmd_apply(id: u64) -> anyhow::Result<()> {
    let client = get_client()?;
    let pending_file = triage_dir().join("pending").join(format!("bug-{id}.json"));
    if !pending_file.exists() {
        anyhow::bail!(
            "No pending draft for bug {id} at {}",
            pending_file.display()
        );
    }

    let text = std::fs::read_to_string(&pending_file)?;
    let draft: serde_json::Value = serde_json::from_str(&text)?;

    println!(
        "--- Draft for bug {}: {} ---",
        draft["bug_id"],
        draft["title"].as_str().unwrap_or("?")
    );
    println!("Comment:\n{}", draft["comment"].as_str().unwrap_or(""));
    println!("NI targets: {}", draft["ni_targets"]);
    println!(
        "Fields: priority={}, severity={}, blocks_add={}, keywords_add={}, cc_add={}",
        draft["priority"],
        draft["severity"],
        draft["blocks_add"],
        draft["keywords_add"],
        draft["cc_add"]
    );

    let confirm = prompt("\nApply? [y/N] ")?;
    if confirm.to_lowercase() != "y" {
        println!("Aborted.");
        return Ok(());
    }

    let bug_id = draft["bug_id"].as_u64().unwrap_or(id);

    if let Some(comment) = draft["comment"].as_str().filter(|s| !s.is_empty()) {
        client.post(
            &format!("/bug/{bug_id}/comment"),
            &json!({"comment": comment}),
        )?;
    }

    let field_body = build_apply_field_body(&draft);
    if !field_body.is_empty() {
        client.put(
            &format!("/bug/{bug_id}"),
            &serde_json::Value::Object(field_body),
        )?;
    }

    if let Some(ni_targets) = draft["ni_targets"].as_array().filter(|a| !a.is_empty()) {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let title = draft["title"].as_str().unwrap_or("?");
        let targets: Vec<String> = ni_targets
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        WatchList::load(&watch_file_path())?.add(&bug_id.to_string(), title, &targets, &now)?;
        let flags: Vec<serde_json::Value> = targets
            .iter()
            .map(|e| json!({"name": "needinfo", "status": "?", "requestee": e}))
            .collect();
        client.put(&format!("/bug/{bug_id}"), &json!({"flags": flags}))?;
    }

    std::fs::remove_file(&pending_file)?;
    println!("Applied. Draft removed.");
    Ok(())
}

fn format_whoami(val: &serde_json::Value) -> String {
    format!(
        "{} <{}>",
        val["real_name"].as_str().unwrap_or("?"),
        val["name"].as_str().unwrap_or("?")
    )
}

fn cmd_whoami() -> anyhow::Result<()> {
    let client = get_client()?;
    let val = client.whoami()?;
    println!("{}", format_whoami(&val));
    Ok(())
}

fn build_search_params(
    query: &str,
    components: &[String],
    product: Option<&str>,
    limit: u32,
    full_text: bool,
    all_statuses: bool,
) -> Vec<(String, String)> {
    let mut params: Vec<(String, String)> = vec![
        ("query_format".into(), "advanced".into()),
        ("limit".into(), limit.to_string()),
    ];

    if full_text {
        // OR group: match summary or comments/description
        params.extend([
            ("f1".into(), "OP".into()),
            ("j1".into(), "OR".into()),
            ("f2".into(), "short_desc".into()),
            ("o2".into(), "substring".into()),
            ("v2".into(), query.into()),
            ("f3".into(), "longdesc".into()),
            ("o3".into(), "substring".into()),
            ("v3".into(), query.into()),
            ("f4".into(), "CP".into()),
        ]);
    } else {
        params.extend([
            ("f1".into(), "short_desc".into()),
            ("o1".into(), "substring".into()),
            ("v1".into(), query.into()),
        ]);
    }

    if !all_statuses {
        for status in ["UNCONFIRMED", "NEW", "ASSIGNED", "REOPENED"] {
            params.push(("bug_status".into(), status.into()));
        }
    }

    for component in components {
        params.push(("component".into(), component.clone()));
    }

    if let Some(p) = product {
        params.push(("product".into(), p.into()));
    }

    params
}

fn cmd_search(
    query: &str,
    components: &[String],
    product: Option<&str>,
    limit: u32,
    full_text: bool,
    all_statuses: bool,
) -> anyhow::Result<()> {
    let client = get_client()?;
    let params = build_search_params(query, components, product, limit, full_text, all_statuses);
    let param_refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let bugs = client.search(&param_refs)?;

    for bug in &bugs {
        let id = bug["id"].as_u64().unwrap_or(0);
        let summary = bug["summary"].as_str().unwrap_or("?");
        let status = bug["status"].as_str().unwrap_or("?");
        let priority = bug["priority"].as_str().unwrap_or("--");
        println!("Bug {id}: [{status} {priority}] {summary}");
    }
    eprintln!("\n# {} bug(s) found", bugs.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_apply_field_body_all_fields() {
        let draft = json!({
            "priority": "P2",
            "severity": "S2",
            "status": "RESOLVED",
            "resolution": "FIXED",
            "blocks_add": [123u64],
            "keywords_add": ["regression"],
            "cc_add": ["dev@mozilla.com"]
        });
        let body = build_apply_field_body(&draft);
        assert_eq!(body["priority"], json!("P2"));
        assert_eq!(body["severity"], json!("S2"));
        assert_eq!(body["status"], json!("RESOLVED"));
        assert_eq!(body["resolution"], json!("FIXED"));
        assert_eq!(body["blocks"], json!({"add": [123u64]}));
        assert_eq!(body["keywords"], json!({"add": ["regression"]}));
        assert_eq!(body["cc"], json!({"add": ["dev@mozilla.com"]}));
    }

    #[test]
    fn test_build_apply_field_body_status_omitted_when_absent() {
        let draft = json!({"resolution": "FIXED"});
        let body = build_apply_field_body(&draft);
        assert!(!body.contains_key("status"));
        assert_eq!(body["resolution"], json!("FIXED"));
    }

    #[test]
    fn test_build_apply_field_body_cc_add_only() {
        let draft = json!({"cc_add": ["a@b.com", "c@d.com"]});
        let body = build_apply_field_body(&draft);
        assert_eq!(body["cc"], json!({"add": ["a@b.com", "c@d.com"]}));
        assert!(!body.contains_key("priority"));
    }

    #[test]
    fn test_build_apply_field_body_empty_cc_omitted() {
        let draft = json!({"cc_add": [], "priority": "P1"});
        let body = build_apply_field_body(&draft);
        assert!(!body.contains_key("cc"));
        assert_eq!(body["priority"], json!("P1"));
    }

    #[test]
    fn test_build_apply_field_body_null_strings_omitted() {
        let draft = json!({"priority": "null", "severity": "", "resolution": "FIXED"});
        let body = build_apply_field_body(&draft);
        assert!(!body.contains_key("priority"));
        assert!(!body.contains_key("severity"));
        assert_eq!(body["resolution"], json!("FIXED"));
    }

    #[test]
    fn test_build_set_fields_body_all_fields() {
        let body = build_set_fields_body(
            Some("P2"),
            Some("S3"),
            None,
            None,
            &[10, 20],
            &["crash".to_string()],
            &["dev@mozilla.com".to_string()],
        );
        assert_eq!(body["priority"], json!("P2"));
        assert_eq!(body["severity"], json!("S3"));
        assert_eq!(body["blocks"], json!({"add": [10, 20]}));
        assert_eq!(body["keywords"], json!({"add": ["crash"]}));
        assert_eq!(body["cc"], json!({"add": ["dev@mozilla.com"]}));
        assert!(!body.contains_key("resolution"));
        assert!(!body.contains_key("status"));
    }

    #[test]
    fn test_build_set_fields_body_cc_only() {
        let body = build_set_fields_body(
            None,
            None,
            None,
            None,
            &[],
            &[],
            &["a@b.com".to_string(), "c@d.com".to_string()],
        );
        assert_eq!(body["cc"], json!({"add": ["a@b.com", "c@d.com"]}));
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn test_build_set_fields_body_empty_cc_omitted() {
        let body = build_set_fields_body(Some("P1"), None, None, None, &[], &[], &[]);
        assert!(!body.contains_key("cc"));
        assert_eq!(body["priority"], json!("P1"));
    }

    #[test]
    fn test_build_set_fields_body_all_empty_returns_empty_map() {
        let body = build_set_fields_body(None, None, None, None, &[], &[], &[]);
        assert!(body.is_empty());
    }

    #[test]
    fn test_build_set_fields_body_close_bug() {
        let body =
            build_set_fields_body(None, None, Some("RESOLVED"), Some("FIXED"), &[], &[], &[]);
        assert_eq!(body["status"], json!("RESOLVED"));
        assert_eq!(body["resolution"], json!("FIXED"));
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn test_build_set_fields_body_status_only() {
        let body = build_set_fields_body(None, None, Some("ASSIGNED"), None, &[], &[], &[]);
        assert_eq!(body["status"], json!("ASSIGNED"));
        assert!(!body.contains_key("resolution"));
    }

    #[test]
    fn test_build_set_fields_body_no_status_omitted() {
        let body = build_set_fields_body(Some("P1"), None, None, Some("FIXED"), &[], &[], &[]);
        assert!(!body.contains_key("status"));
        assert_eq!(body["resolution"], json!("FIXED"));
    }

    #[test]
    fn test_format_whoami() {
        let val = json!({"name": "bot@mozilla.com", "real_name": "Triage Bot", "id": 1});
        assert_eq!(format_whoami(&val), "Triage Bot <bot@mozilla.com>");
    }

    #[test]
    fn test_format_whoami_missing_fields() {
        let val = json!({});
        assert_eq!(format_whoami(&val), "? <?>");
    }

    #[test]
    fn test_format_bug_header_basic() {
        let bug = json!({
            "id": 123,
            "summary": "Video playback crash",
            "status": "NEW",
            "resolution": "",
            "priority": "P2",
            "severity": "S2",
            "assigned_to": "nobody@mozilla.org",
            "creator": "reporter@example.com",
            "creator_detail": {"real_name": "A Reporter"},
            "cf_qa_whiteboard": ""
        });
        let out = format_bug_header(&bug);
        assert!(out.contains("Bug 123: Video playback crash"));
        assert!(out.contains("Status:   NEW"));
        assert!(out.contains("Priority: P2  Severity: S2"));
        assert!(out.contains("Assigned: nobody@mozilla.org"));
        assert!(out.contains("Creator:  reporter@example.com (A Reporter)"));
        assert!(!out.contains("QA:"));
    }

    #[test]
    fn test_format_bug_header_shows_qa_whiteboard() {
        let bug = json!({
            "id": 1, "summary": "s", "status": "NEW", "resolution": "",
            "priority": "--", "severity": "--",
            "assigned_to": "nobody@mozilla.org",
            "creator": "x@x.com", "creator_detail": {"real_name": "X"},
            "cf_qa_whiteboard": "verified-upstream"
        });
        let out = format_bug_header(&bug);
        assert!(out.contains("QA:       verified-upstream"));
    }

    #[test]
    fn test_format_comments_full_text() {
        let comments = vec![json!({
            "creation_time": "2026-01-01T00:00:00Z",
            "creator": "dev@mozilla.com",
            "text": "a".repeat(600)
        })];
        let out = format_comments(&comments);
        assert!(out.contains("1 comment(s)"));
        // full text — no truncation
        assert!(out.contains(&"a".repeat(600)));
    }

    #[test]
    fn test_format_comments_empty() {
        let out = format_comments(&[]);
        assert!(out.contains("0 comment(s)"));
    }

    fn param(params: &[(String, String)], key: &str) -> Option<String> {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    fn all_values<'a>(params: &'a [(String, String)], key: &str) -> Vec<&'a str> {
        params
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    #[test]
    fn test_search_params_default() {
        let params = build_search_params("mp4 crash", &[], None, 25, false, false);
        // summary-only
        assert_eq!(param(&params, "f1").as_deref(), Some("short_desc"));
        assert_eq!(param(&params, "o1").as_deref(), Some("substring"));
        assert_eq!(param(&params, "v1").as_deref(), Some("mp4 crash"));
        // no OR group
        assert!(param(&params, "f3").is_none());
        // open statuses included
        let statuses = all_values(&params, "bug_status");
        assert!(statuses.contains(&"NEW"));
        assert!(statuses.contains(&"UNCONFIRMED"));
        assert!(statuses.contains(&"ASSIGNED"));
        assert!(statuses.contains(&"REOPENED"));
        // default limit
        assert_eq!(param(&params, "limit").as_deref(), Some("25"));
    }

    #[test]
    fn test_search_params_full_text() {
        let params = build_search_params("NS_ERROR_FAILURE", &[], None, 25, true, false);
        assert_eq!(param(&params, "f1").as_deref(), Some("OP"));
        assert_eq!(param(&params, "j1").as_deref(), Some("OR"));
        assert_eq!(param(&params, "f2").as_deref(), Some("short_desc"));
        assert_eq!(param(&params, "o2").as_deref(), Some("substring"));
        assert_eq!(param(&params, "v2").as_deref(), Some("NS_ERROR_FAILURE"));
        assert_eq!(param(&params, "f3").as_deref(), Some("longdesc"));
        assert_eq!(param(&params, "o3").as_deref(), Some("substring"));
        assert_eq!(param(&params, "v3").as_deref(), Some("NS_ERROR_FAILURE"));
        assert_eq!(param(&params, "f4").as_deref(), Some("CP"));
    }

    #[test]
    fn test_search_params_all_statuses() {
        let params = build_search_params("crash", &[], None, 25, false, true);
        assert!(all_values(&params, "bug_status").is_empty());
    }

    #[test]
    fn test_search_params_components_and_product() {
        let components = vec![
            "Audio/Video: Playback".to_string(),
            "Audio/Video: Web Codecs".to_string(),
        ];
        let params = build_search_params("decode", &components, Some("Core"), 10, false, false);
        let comps = all_values(&params, "component");
        assert!(comps.contains(&"Audio/Video: Playback"));
        assert!(comps.contains(&"Audio/Video: Web Codecs"));
        assert_eq!(param(&params, "product").as_deref(), Some("Core"));
        assert_eq!(param(&params, "limit").as_deref(), Some("10"));
    }

    #[test]
    fn test_search_params_no_product() {
        let params = build_search_params("crash", &[], None, 25, false, false);
        assert!(param(&params, "product").is_none());
    }
}

fn cmd_watch_add(id: u64, title: &str, ni: &[String]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    WatchList::load(&watch_file_path())?.add(&id.to_string(), title, ni, &now)?;
    println!("Watching bug {id}.");
    Ok(())
}

fn cmd_watch_remove(id: u64) -> anyhow::Result<()> {
    let removed = WatchList::load(&watch_file_path())?.remove(&id.to_string())?;
    if removed {
        println!("Removed bug {id} from watch list.");
    } else {
        println!("Bug {id} was not in watch list.");
    }
    Ok(())
}

fn cmd_watch_poll() -> anyhow::Result<()> {
    let client = get_client()?;
    let result = WatchList::load(&watch_file_path())?.poll(&client)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if !is_configured() {
        println!("bugzilla-cli is not configured yet. Starting setup...\n");
        cmd_setup()?;
        println!();
    }
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup => cmd_setup()?,
        Commands::Get { id, comments } => cmd_get(id, comments)?,
        Commands::Fetch { start, end } => cmd_fetch(start, end)?,
        Commands::PostComment { id, text } => cmd_post_comment(id, &text)?,
        Commands::SetNi { id, email } => cmd_set_ni(id, &email)?,
        Commands::SetFields {
            id,
            priority,
            severity,
            status,
            resolution,
            blocks_add,
            keywords_add,
            cc_add,
        } => {
            let body = build_set_fields_body(
                priority.as_deref(),
                severity.as_deref(),
                status.as_deref(),
                resolution.as_deref(),
                &blocks_add,
                &keywords_add,
                &cc_add,
            );
            cmd_set_fields(id, body)?;
        }
        Commands::Apply { id } => cmd_apply(id)?,
        Commands::WatchAdd { id, title, ni } => cmd_watch_add(id, &title, &ni)?,
        Commands::WatchRemove { id } => cmd_watch_remove(id)?,
        Commands::Whoami => cmd_whoami()?,
        Commands::WatchPoll => cmd_watch_poll()?,
        Commands::Search {
            query,
            component,
            product,
            limit,
            full_text,
            all_statuses,
        } => cmd_search(
            &query,
            &component,
            product.as_deref(),
            limit,
            full_text,
            all_statuses,
        )?,
    }
    Ok(())
}
