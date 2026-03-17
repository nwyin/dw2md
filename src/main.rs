mod compiler;
mod mcp;
mod wiki;

use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use dialoguer::theme::ColorfulTheme;
use dialoguer::MultiSelect;

/// Crawl a DeepWiki repository and compile all pages into a single, LLM-friendly markdown file.
#[derive(Parser, Debug)]
#[command(name = "dw2md", version, about)]
struct Cli {
    /// Repository identifier: owner/repo, https://deepwiki.com/owner/repo, or a full page URL
    repo: String,

    /// Write output to a file instead of stdout
    #[arg(short, long)]
    output: Option<String>,

    /// Output format: markdown or json
    #[arg(short, long, default_value = "markdown")]
    format: OutputFormat,

    /// Per-request timeout in seconds
    #[arg(short, long, default_value = "30")]
    timeout: u64,

    /// Comma-separated page slugs to include
    #[arg(short, long, value_delimiter = ',')]
    pages: Option<Vec<String>>,

    /// Comma-separated page slugs to exclude
    #[arg(short = 'x', long, value_delimiter = ',')]
    exclude: Option<Vec<String>>,

    /// Omit the structure tree from output
    #[arg(long)]
    no_toc: bool,

    /// Omit the metadata header block
    #[arg(long)]
    no_metadata: bool,

    /// Suppress progress output on stderr
    #[arg(short, long)]
    quiet: bool,

    /// Show detailed progress and debug info
    #[arg(short, long)]
    verbose: bool,

    /// Print only the table of contents, then exit
    #[arg(short, long)]
    list: bool,

    /// Interactively select which sections to include
    #[arg(short, long)]
    interactive: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
}

/// Parse a repository identifier from various input formats.
///
/// Accepts:
/// - `owner/repo`
/// - `https://deepwiki.com/owner/repo`
/// - `https://deepwiki.com/owner/repo/3.1-some-page`
fn parse_repo(input: &str) -> Result<String> {
    let input = input.trim().trim_end_matches('/');

    // Try URL format
    if input.starts_with("http://") || input.starts_with("https://") {
        let url = input
            .strip_prefix("https://deepwiki.com/")
            .or_else(|| input.strip_prefix("http://deepwiki.com/"))
            .context("URL must be a deepwiki.com URL (e.g., https://deepwiki.com/owner/repo)")?;

        let parts: Vec<&str> = url.splitn(3, '/').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            bail!("URL must contain owner/repo (e.g., https://deepwiki.com/owner/repo)");
        }

        return Ok(format!("{}/{}", parts[0], parts[1]));
    }

    // Try owner/repo format
    let parts: Vec<&str> = input.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "Invalid repository identifier '{}'. Expected owner/repo or https://deepwiki.com/owner/repo",
            input
        );
    }

    Ok(input.to_string())
}

/// Format a page title with indentation for display.
fn format_page_label(page: &wiki::Page) -> String {
    let indent = "  ".repeat(page.depth);
    format!("{}{}", indent, page.title)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo = parse_repo(&cli.repo)?;

    let config = compiler::CompileConfig {
        repo: repo.clone(),
        timeout: Duration::from_secs(cli.timeout),
        include: cli.pages,
        exclude: cli.exclude,
        quiet: cli.quiet,
        verbose: cli.verbose,
    };

    // --list: just print the TOC and exit
    if cli.list {
        let pages = compiler::fetch_structure(&config).await?;
        let tree = compiler::markdown::render_tree(&pages, true);
        print!("{}", compiler::sanitize_for_terminal(&tree));
        return Ok(());
    }

    // --interactive: show TOC, let user pick, then fetch only those
    if cli.interactive {
        let all_pages = compiler::fetch_structure(&config).await?;

        if all_pages.is_empty() {
            bail!("No pages found for {}", repo);
        }

        let labels: Vec<String> = all_pages.iter().map(format_page_label).collect();

        // All selected by default
        let defaults: Vec<bool> = vec![true; all_pages.len()];

        eprintln!();
        let selections = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select pages to include (space to toggle, enter to confirm)")
            .items(&labels)
            .defaults(&defaults)
            .max_length(20)
            .interact()?;

        if selections.is_empty() {
            bail!("No pages selected");
        }

        let selected_slugs: Vec<String> = selections
            .iter()
            .map(|&i| all_pages[i].slug.clone())
            .collect();

        let selected_count = selected_slugs.len();
        if !cli.quiet {
            eprintln!(
                "[dw2md] {} of {} pages selected",
                selected_count,
                all_pages.len()
            );
        }

        let pages = compiler::fetch_wiki_selected(&config, &selected_slugs).await?;

        let output = match cli.format {
            OutputFormat::Markdown => {
                compiler::markdown::compile(&repo, &pages, !cli.no_toc, !cli.no_metadata)
            }
            OutputFormat::Json => compiler::json::compile(&repo, &pages),
        };

        return write_output(&output, cli.output.as_deref(), cli.quiet);
    }

    // Default: fetch everything
    let pages = compiler::fetch_wiki(&config).await?;

    let output = match cli.format {
        OutputFormat::Markdown => {
            compiler::markdown::compile(&repo, &pages, !cli.no_toc, !cli.no_metadata)
        }
        OutputFormat::Json => compiler::json::compile(&repo, &pages),
    };

    write_output(&output, cli.output.as_deref(), cli.quiet)
}

fn write_output(output: &str, path: Option<&str>, quiet: bool) -> Result<()> {
    if let Some(path) = path {
        let target = std::path::Path::new(path);

        // Resolve symlinks: if the target already exists, canonicalize to get the real path.
        // This ensures the user sees the actual destination, not a symlink alias.
        let resolved = if target.exists() {
            target
                .canonicalize()
                .with_context(|| format!("Failed to resolve path {}", path))?
        } else {
            // For new files, canonicalize the parent directory to catch symlinked directories
            if let Some(parent) = target.parent() {
                let canon_parent = if parent.as_os_str().is_empty() {
                    std::env::current_dir().context("Failed to get current directory")?
                } else {
                    parent.canonicalize().with_context(|| {
                        format!("Failed to resolve parent directory of {}", path)
                    })?
                };
                canon_parent.join(target.file_name().unwrap_or_default())
            } else {
                target.to_path_buf()
            }
        };

        std::fs::write(&resolved, output)
            .with_context(|| format!("Failed to write output to {}", resolved.display()))?;
        if !quiet {
            eprintln!("[dw2md] Output written to {}", resolved.display());
        }
    } else {
        print!("{}", output);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo_simple() {
        assert_eq!(
            parse_repo("tinygrad/tinygrad").unwrap(),
            "tinygrad/tinygrad"
        );
    }

    #[test]
    fn test_parse_repo_url() {
        assert_eq!(
            parse_repo("https://deepwiki.com/tinygrad/tinygrad").unwrap(),
            "tinygrad/tinygrad"
        );
    }

    #[test]
    fn test_parse_repo_url_with_page() {
        assert_eq!(
            parse_repo("https://deepwiki.com/tokio-rs/tokio/3.1-runtime").unwrap(),
            "tokio-rs/tokio"
        );
    }

    #[test]
    fn test_parse_repo_url_trailing_slash() {
        assert_eq!(
            parse_repo("https://deepwiki.com/owner/repo/").unwrap(),
            "owner/repo"
        );
    }

    #[test]
    fn test_parse_repo_invalid() {
        assert!(parse_repo("just-a-name").is_err());
        assert!(parse_repo("").is_err());
        assert!(parse_repo("/repo").is_err());
        assert!(parse_repo("owner/").is_err());
    }

    #[test]
    fn test_parse_repo_wrong_host() {
        assert!(parse_repo("https://github.com/owner/repo").is_err());
    }

    #[test]
    fn test_format_page_label() {
        let page = wiki::Page {
            slug: "1-overview".into(),
            title: "1 Overview".into(),
            depth: 0,
            content: None,
            error: None,
        };
        assert_eq!(format_page_label(&page), "1 Overview");

        let child = wiki::Page {
            slug: "1-1-features".into(),
            title: "1.1 Features".into(),
            depth: 1,
            content: None,
            error: None,
        };
        assert_eq!(format_page_label(&child), "  1.1 Features");

        let deep = wiki::Page {
            slug: "1-1-1-sub".into(),
            title: "1.1.1 Sub".into(),
            depth: 2,
            content: None,
            error: None,
        };
        assert_eq!(format_page_label(&deep), "    1.1.1 Sub");
    }
}
