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
    Setup,

    Get {
        id: u64,
        #[arg(long = "no-comments", action = clap::ArgAction::SetFalse)]
        comments: bool,
    },

    Fetch {
        #[arg(long, value_name = "YYYY-MM-DD")]
        start: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD")]
        end: Option<String>,
    },

    PostComment {
        id: u64,
        text: String,
    },

    SetNi {
        id: u64,
        #[arg(required = true)]
        email: Vec<String>,
    },

    SetFields {
        id: u64,
        #[arg(long, value_parser = ["P1", "P2", "P3", "P4", "P5", "--"])]
        priority: Option<String>,
        #[arg(long, value_parser = ["S1", "S2", "S3", "S4", "--"])]
        severity: Option<String>,
        #[arg(long)]
        resolution: Option<String>,
        #[arg(long, num_args = 1..)]
        blocks_add: Vec<u64>,
        #[arg(long, num_args = 1..)]
        keywords_add: Vec<String>,
    },

    Apply {
        id: u64,
    },

    WatchAdd {
        id: u64,
        #[arg(long, required = true)]
        title: String,
        #[arg(long = "ni", required = true, num_args = 1..)]
        ni: Vec<String>,
    },

    WatchRemove {
        id: u64,
    },

    WatchPoll,

    Search {
        query: String,
        #[arg(long, num_args = 1..)]
        component: Vec<String>,
        #[arg(long)]
        product: Option<String>,
        #[arg(long, default_value = "25")]
        limit: u32,
        #[arg(long)]
        full_text: bool,
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
    println!(
        "\u{2713} Authenticated as {} <{}>",
        me["real_name"].as_str().unwrap_or("?"),
        me["name"].as_str().unwrap_or("?")
    );

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

fn cmd_get(id: u64, comments: bool) -> anyhow::Result<()> {
    let client = get_client()?;
    let data = client.get_bug(id, comments)?;
    let bug = &data["bug"];
    println!(
        "Bug {}: {}",
        bug["id"],
        bug["summary"].as_str().unwrap_or("?")
    );
    println!(
        "  Status:   {} {}",
        bug["status"].as_str().unwrap_or("?"),
        bug["resolution"].as_str().unwrap_or("")
    );
    println!(
        "  Priority: {}  Severity: {}",
        bug["priority"].as_str().unwrap_or("?"),
        bug["severity"].as_str().unwrap_or("?")
    );
    println!("  Assigned: {}", bug["assigned_to"].as_str().unwrap_or("?"));
    println!(
        "  Creator:  {} ({})",
        bug["creator"].as_str().unwrap_or("?"),
        bug["creator_detail"]["real_name"].as_str().unwrap_or("?")
    );
    let qa_wb = bug["cf_qa_whiteboard"].as_str().unwrap_or("");
    if !qa_wb.is_empty() {
        println!("  QA:       {qa_wb}");
    }
    if comments {
        let empty = vec![];
        let clist = data["comments"].as_array().unwrap_or(&empty);
        println!("\n--- {} comment(s) ---", clist.len());
        for c in clist {
            println!(
                "\n[{}] {}:",
                c["creation_time"].as_str().unwrap_or("?"),
                c["creator"].as_str().unwrap_or("?")
            );
            let text = c["text"].as_str().unwrap_or("");
            if text.len() > 500 {
                print!("{}", &text[..500]);
                println!("...");
            } else {
                println!("{text}");
            }
        }
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

fn cmd_set_fields(
    id: u64,
    priority: Option<&str>,
    severity: Option<&str>,
    resolution: Option<&str>,
    blocks_add: &[u64],
    keywords_add: &[String],
) -> anyhow::Result<()> {
    let client = get_client()?;
    let mut body = serde_json::Map::new();
    if let Some(p) = priority {
        body.insert("priority".into(), json!(p));
    }
    if let Some(s) = severity {
        body.insert("severity".into(), json!(s));
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
        "Fields: priority={}, severity={}, blocks_add={}, keywords_add={}",
        draft["priority"], draft["severity"], draft["blocks_add"], draft["keywords_add"]
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

    let mut field_body = serde_json::Map::new();
    if let Some(p) = draft["priority"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "null")
    {
        field_body.insert("priority".into(), json!(p));
    }
    if let Some(s) = draft["severity"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "null")
    {
        field_body.insert("severity".into(), json!(s));
    }
    if let Some(r) = draft["resolution"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "null")
    {
        field_body.insert("resolution".into(), json!(r));
    }
    if let Some(arr) = draft["blocks_add"].as_array().filter(|a| !a.is_empty()) {
        field_body.insert("blocks".into(), json!({"add": arr}));
    }
    if let Some(arr) = draft["keywords_add"].as_array().filter(|a| !a.is_empty()) {
        field_body.insert("keywords".into(), json!({"add": arr}));
    }
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
            resolution,
            blocks_add,
            keywords_add,
        } => {
            cmd_set_fields(
                id,
                priority.as_deref(),
                severity.as_deref(),
                resolution.as_deref(),
                &blocks_add,
                &keywords_add,
            )?;
        }
        Commands::Apply { id } => cmd_apply(id)?,
        Commands::WatchAdd { id, title, ni } => cmd_watch_add(id, &title, &ni)?,
        Commands::WatchRemove { id } => cmd_watch_remove(id)?,
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
