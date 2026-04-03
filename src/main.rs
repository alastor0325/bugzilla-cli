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

fn get_client() -> anyhow::Result<BmoClient> {
    let api_key = std::env::var("BUGZILLA_BOT_API_KEY").map_err(|_| {
        anyhow::anyhow!("BUGZILLA_BOT_API_KEY is not set. Run 'bugzilla-cli setup' first.")
    })?;
    Ok(BmoClient::new(&api_key))
}

fn prompt(label: &str) -> anyhow::Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[derive(Parser)]
#[command(
    name = "bugzilla-cli",
    about = "Thin BMO REST client for Firefox A/V triage."
)]
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
}

fn cmd_setup() -> anyhow::Result<()> {
    println!("=== bugzilla-cli setup ===");

    let url_input = prompt(&format!("BMO base URL [{}]: ", BMO_BASE))?;
    let url = if url_input.is_empty() {
        BMO_BASE.to_string()
    } else {
        url_input
    };

    let api_key = prompt("API key: ")?;
    if api_key.is_empty() {
        anyhow::bail!("API key is required.");
    }

    let test_client = BmoClient::new_with_base(&api_key, &url);
    println!("Verifying API key with BMO...");
    let me = test_client.whoami().context("Authentication failed")?;
    println!(
        "Authenticated as: {} <{}>",
        me["real_name"].as_str().unwrap_or("?"),
        me["name"].as_str().unwrap_or("?")
    );

    let default_triage = triage_dir().display().to_string();
    let triage_input = prompt(&format!("Triage directory [{}]: ", default_triage))?;
    let triage_path = PathBuf::from(if triage_input.is_empty() {
        default_triage
    } else {
        triage_input
    });
    for sub in ["bugs", "pending", "reports", "archive"] {
        std::fs::create_dir_all(triage_path.join(sub))?;
    }
    println!("Created triage directories under {}", triage_path.display());

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
    println!("API key written to {} (chmod 600)", secrets_file.display());

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

fn cmd_fetch(start: Option<String>, end: Option<String>) -> anyhow::Result<()> {
    let client = get_client()?;

    let start_date = start.unwrap_or_else(|| monday_of_current_week().to_string());
    let creation_time_val = format!("{start_date}T00:00:00Z");

    let params: Vec<(&str, &str)> = vec![
        ("savedsearch", "media-meta"),
        ("include_fields", "_default"),
        ("creation_time", &creation_time_val),
    ];

    let mut bugs = client.search(&params)?;

    if let Some(end_str) = &end {
        let end_dt: NaiveDate = end_str.parse().context("invalid end date")?;
        bugs.retain(|b| {
            let ct = b["creation_time"].as_str().unwrap_or("");
            let date_part = ct.get(..10).unwrap_or("");
            date_part
                .parse::<NaiveDate>()
                .map(|d| d <= end_dt)
                .unwrap_or(false)
        });
    }

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
    }
    Ok(())
}
