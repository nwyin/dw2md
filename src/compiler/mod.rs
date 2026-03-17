pub mod json;
pub mod markdown;

use std::time::Duration;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

use crate::mcp::McpClient;
use crate::wiki::{filter, merge_content, split_pages, Page, WikiStructure};

/// Strip C0/C1 control characters from a string before terminal output.
///
/// Removes bytes 0x00-0x1F (except \n 0x0A and \t 0x09), 0x7F (DEL),
/// and 0x80-0x9F (C1 control codes). This prevents ANSI escape injection
/// from remote-derived data like page titles.
pub fn sanitize_for_terminal(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            if c == '\n' || c == '\t' {
                return true;
            }
            let code = c as u32;
            // Drop C0 (0x00-0x1F), DEL (0x7F), and C1 (0x80-0x9F)
            !(code <= 0x1F || code == 0x7F || (0x80..=0x9F).contains(&code))
        })
        .collect()
}

/// Configuration for the compilation pipeline.
pub struct CompileConfig {
    pub repo: String,
    pub timeout: Duration,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub quiet: bool,
    pub verbose: bool,
}

/// Fetch just the wiki structure (table of contents), without page contents.
pub async fn fetch_structure(config: &CompileConfig) -> Result<Vec<Page>> {
    let client = connect(config).await?;

    let structure = fetch_structure_inner(&client, config).await?;

    let pages = filter::filter_pages(structure.pages, &config.include, &config.exclude);

    Ok(pages)
}

/// Fetch the wiki structure and all page contents.
pub async fn fetch_wiki(config: &CompileConfig) -> Result<Vec<Page>> {
    let client = connect(config).await?;

    let structure = fetch_structure_inner(&client, config).await?;

    let mut pages = filter::filter_pages(structure.pages, &config.include, &config.exclude);
    let total_pages = pages.len();

    if !config.quiet {
        eprintln!("[dw2md] {} pages to fetch", total_pages);
    }

    fetch_and_merge_contents(&client, config, &mut pages).await?;

    let matched = pages.iter().filter(|p| p.content.is_some()).count();
    let unmatched = total_pages - matched;

    if !config.quiet {
        eprintln!(
            "[dw2md] Done: {} pages matched, {} unmatched",
            matched, unmatched
        );
    }

    if unmatched > 0 && config.verbose {
        for page in &pages {
            if page.content.is_none() {
                eprintln!(
                    "[dw2md] Warning: No content matched for '{}'",
                    sanitize_for_terminal(&page.title)
                );
            }
        }
    }

    Ok(pages)
}

/// Fetch wiki structure and all page contents, but only keep pages in `selected_slugs`.
pub async fn fetch_wiki_selected(
    config: &CompileConfig,
    selected_slugs: &[String],
) -> Result<Vec<Page>> {
    let client = connect(config).await?;

    let structure = fetch_structure_inner(&client, config).await?;

    // Keep only pages whose slug is in the selected set
    let mut pages: Vec<Page> = structure
        .pages
        .into_iter()
        .filter(|p| selected_slugs.contains(&p.slug))
        .collect();

    let total_pages = pages.len();

    if !config.quiet {
        eprintln!("[dw2md] {} pages selected", total_pages);
    }

    fetch_and_merge_contents(&client, config, &mut pages).await?;

    let matched = pages.iter().filter(|p| p.content.is_some()).count();
    let unmatched = total_pages - matched;

    if !config.quiet {
        eprintln!(
            "[dw2md] Done: {} pages matched, {} unmatched",
            matched, unmatched
        );
    }

    Ok(pages)
}

async fn connect(config: &CompileConfig) -> Result<McpClient> {
    let client = McpClient::connect(config.timeout)
        .await
        .context("Failed to connect to DeepWiki MCP server")?;

    if config.verbose {
        eprintln!("[dw2md] Connected to MCP server");
    }

    Ok(client)
}

async fn fetch_structure_inner(
    client: &McpClient,
    config: &CompileConfig,
) -> Result<WikiStructure> {
    if !config.quiet {
        eprintln!("[dw2md] Fetching wiki structure for {}...", config.repo);
    }

    let structure_text = call_with_retry(client, "read_wiki_structure", &config.repo, 3)
        .await
        .context(format!(
            "Failed to fetch wiki structure. Repository '{}' may not be indexed on DeepWiki. \
             Visit https://deepwiki.com to request indexing.",
            config.repo
        ))?;

    if config.verbose {
        eprintln!(
            "[dw2md] Raw structure ({} bytes):\n{}",
            structure_text.len(),
            sanitize_for_terminal(&structure_text[..structure_text.len().min(500)])
        );
    }

    let structure =
        WikiStructure::parse(&structure_text).context("Failed to parse wiki structure")?;

    if config.verbose {
        eprintln!("[dw2md] Found {} pages in structure", structure.pages.len());
    }

    Ok(structure)
}

async fn fetch_and_merge_contents(
    client: &McpClient,
    config: &CompileConfig,
    pages: &mut [Page],
) -> Result<()> {
    let progress = if !config.quiet {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Fetching page contents...");
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    let contents_text = call_with_retry(client, "read_wiki_contents", &config.repo, 3)
        .await
        .context("Failed to fetch wiki contents")?;

    if let Some(pb) = &progress {
        pb.set_message("Parsing pages...");
    }

    if config.verbose {
        eprintln!("[dw2md] Raw content ({} bytes)", contents_text.len());
    }

    let content_pages = split_pages(&contents_text);

    if config.verbose {
        eprintln!(
            "[dw2md] Found {} pages in content response",
            content_pages.len()
        );
        for (title, content) in &content_pages {
            eprintln!(
                "  - \"{}\" ({} bytes)",
                sanitize_for_terminal(title),
                content.len()
            );
        }
    }

    merge_content(pages, &content_pages);

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    Ok(())
}

/// Call an MCP tool with retry logic (exponential backoff).
async fn call_with_retry(
    client: &McpClient,
    tool: &str,
    repo: &str,
    max_retries: u32,
) -> Result<String> {
    let mut last_err = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let backoff = Duration::from_secs(1 << (attempt - 1));
            tokio::time::sleep(backoff).await;
        }

        match client
            .call_tool(tool, serde_json::json!({"repoName": repo}))
            .await
        {
            Ok(content) => return Ok(content),
            Err(err) => {
                last_err = Some(err);
            }
        }
    }

    Err(last_err.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_preserves_normal_text() {
        assert_eq!(sanitize_for_terminal("hello world"), "hello world");
    }

    #[test]
    fn test_sanitize_preserves_newlines_and_tabs() {
        assert_eq!(sanitize_for_terminal("a\tb\nc"), "a\tb\nc");
    }

    #[test]
    fn test_sanitize_strips_ansi_escape() {
        // ESC [ 2 J = clear screen
        assert_eq!(sanitize_for_terminal("title\x1b[2J"), "title[2J");
    }

    #[test]
    fn test_sanitize_strips_null_and_bel() {
        assert_eq!(sanitize_for_terminal("a\x00b\x07c"), "abc");
    }

    #[test]
    fn test_sanitize_strips_c1_control_codes() {
        // C1 range: 0x80-0x9F (e.g., 0x9B = CSI)
        let input = format!("before{}after", '\u{009B}');
        assert_eq!(sanitize_for_terminal(&input), "beforeafter");
    }

    #[test]
    fn test_sanitize_strips_del() {
        assert_eq!(sanitize_for_terminal("a\x7Fb"), "ab");
    }

    #[test]
    fn test_sanitize_passes_unicode() {
        assert_eq!(sanitize_for_terminal("hello 🌍 world"), "hello 🌍 world");
    }
}
