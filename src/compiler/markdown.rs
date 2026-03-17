use crate::wiki::Page;

/// Escape content lines that could spoof section delimiters.
///
/// Any line starting with `<<< SECTION:` is prefixed with a zero-width space,
/// making it visually identical but non-matching to the `^<<< SECTION:` regex
/// used for grep/sed extraction.
fn escape_section_delimiters(content: &str) -> String {
    let escaped: String = content
        .lines()
        .map(|line| {
            if line.starts_with("<<< SECTION:") {
                format!("\u{200B}{}", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if content.ends_with('\n') {
        escaped + "\n"
    } else {
        escaped
    }
}

/// Compile pages into a single markdown document with tree TOC and grep-able section delimiters.
pub fn compile(repo: &str, pages: &[Page], include_toc: bool, include_metadata: bool) -> String {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let page_count = pages.iter().filter(|p| p.content.is_some()).count();
    let mut output = String::new();

    // Metadata comment
    if include_metadata {
        output.push_str(&format!(
            "<!-- dw2md v{} | {} | {} | {} pages -->\n\n",
            env!("CARGO_PKG_VERSION"),
            repo,
            now,
            page_count
        ));
    }

    // Title
    output.push_str(&format!("# {} — DeepWiki\n\n", repo));

    if include_metadata {
        output.push_str(&format!(
            "> Compiled from https://deepwiki.com/{}\n> Generated: {} | Pages: {}\n\n",
            repo, now, page_count
        ));
        output.push_str("## Format\n\n");
        output.push_str("Sections are delimited by `<<< SECTION: Title [slug] >>>` lines.\n");
        output.push_str("Grep for `^<<< SECTION:` to list all sections.\n");
        output.push_str("The Structure tree below shows hierarchy; slugs in brackets are unique identifiers.\n\n");
    }

    // Tree-style structure
    if include_toc {
        let tree_pages: Vec<Page> = pages
            .iter()
            .filter(|p| p.content.is_some() || p.error.is_some())
            .cloned()
            .collect();
        output.push_str("## Structure\n\n");
        output.push_str(&render_tree(&tree_pages, false));
        output.push('\n');
    }

    // Section contents
    let has_content = pages
        .iter()
        .any(|p| p.content.is_some() || p.error.is_some());
    if has_content {
        if include_toc {
            output.push_str("## Contents\n\n");
        }
        for page in pages {
            if let Some(content) = &page.content {
                output.push_str(&format!(
                    "<<< SECTION: {} [{}] >>>\n\n",
                    page.title, page.slug
                ));
                output.push_str(&escape_section_delimiters(content));
                if !content.ends_with('\n') {
                    output.push('\n');
                }
                output.push('\n');
            } else if let Some(err) = &page.error {
                output.push_str(&format!(
                    "<<< SECTION: {} [{}] >>>\n\n",
                    page.title, page.slug
                ));
                output.push_str(&format!("> **Failed to fetch this page:** {}\n\n", err));
            }
        }
    }

    output.trim_end().to_string()
}

/// Render pages as an ASCII tree.
///
/// When `show_slugs` is true, appends `[slug]` after each title (useful for `--list` mode).
pub fn render_tree(pages: &[Page], show_slugs: bool) -> String {
    let mut result = String::new();
    let n = pages.len();

    for i in 0..n {
        let page = &pages[i];
        let mut prefix = String::new();

        // Ancestor depth prefixes
        for d in 0..page.depth {
            if has_future_at_depth(pages, i, d) {
                prefix.push_str("│   ");
            } else {
                prefix.push_str("    ");
            }
        }

        // Connector
        if is_last_sibling(pages, i) {
            prefix.push_str("└── ");
        } else {
            prefix.push_str("├── ");
        }

        result.push_str(&prefix);
        result.push_str(&page.title);
        if show_slugs {
            result.push_str(&format!(" [{}]", page.slug));
        }
        result.push('\n');
    }

    result
}

/// Check if the page at `index` is the last sibling at its depth.
fn is_last_sibling(pages: &[Page], index: usize) -> bool {
    let depth = pages[index].depth;
    for page in &pages[(index + 1)..] {
        if page.depth < depth {
            return true;
        }
        if page.depth == depth {
            return false;
        }
    }
    true
}

/// Check if there's a future page at `target_depth` after `current`.
fn has_future_at_depth(pages: &[Page], current: usize, target_depth: usize) -> bool {
    for page in &pages[(current + 1)..] {
        if page.depth < target_depth {
            return false;
        }
        if page.depth == target_depth {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_tree_basic() {
        let pages = vec![
            Page {
                slug: "1-overview".into(),
                title: "1 Overview".into(),
                depth: 0,
                content: None,
                error: None,
            },
            Page {
                slug: "1-1-repo".into(),
                title: "1.1 Repo Structure".into(),
                depth: 1,
                content: None,
                error: None,
            },
            Page {
                slug: "2-setup".into(),
                title: "2 Setup".into(),
                depth: 0,
                content: None,
                error: None,
            },
        ];

        let result = render_tree(&pages, false);
        assert_eq!(
            result,
            "├── 1 Overview\n│   └── 1.1 Repo Structure\n└── 2 Setup\n"
        );
    }

    #[test]
    fn test_render_tree_with_slugs() {
        let pages = vec![
            Page {
                slug: "1-overview".into(),
                title: "1 Overview".into(),
                depth: 0,
                content: None,
                error: None,
            },
            Page {
                slug: "2-setup".into(),
                title: "2 Setup".into(),
                depth: 0,
                content: None,
                error: None,
            },
        ];

        let result = render_tree(&pages, true);
        assert_eq!(
            result,
            "├── 1 Overview [1-overview]\n└── 2 Setup [2-setup]\n"
        );
    }

    #[test]
    fn test_render_tree_deep_nesting() {
        let pages = vec![
            Page {
                slug: "1-a".into(),
                title: "1 A".into(),
                depth: 0,
                content: None,
                error: None,
            },
            Page {
                slug: "1-1-b".into(),
                title: "1.1 B".into(),
                depth: 1,
                content: None,
                error: None,
            },
            Page {
                slug: "1-1-1-c".into(),
                title: "1.1.1 C".into(),
                depth: 2,
                content: None,
                error: None,
            },
            Page {
                slug: "1-1-2-d".into(),
                title: "1.1.2 D".into(),
                depth: 2,
                content: None,
                error: None,
            },
            Page {
                slug: "1-2-e".into(),
                title: "1.2 E".into(),
                depth: 1,
                content: None,
                error: None,
            },
            Page {
                slug: "2-f".into(),
                title: "2 F".into(),
                depth: 0,
                content: None,
                error: None,
            },
        ];

        let result = render_tree(&pages, false);
        let expected = "\
├── 1 A
│   ├── 1.1 B
│   │   ├── 1.1.1 C
│   │   └── 1.1.2 D
│   └── 1.2 E
└── 2 F
";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_tree_single() {
        let pages = vec![Page {
            slug: "1-overview".into(),
            title: "1 Overview".into(),
            depth: 0,
            content: None,
            error: None,
        }];

        let result = render_tree(&pages, false);
        assert_eq!(result, "└── 1 Overview\n");
    }

    #[test]
    fn test_compile_basic() {
        let pages = vec![
            Page {
                slug: "1-overview".into(),
                title: "1 Overview".into(),
                depth: 0,
                content: Some("# Intro\n\nHello world.".into()),
                error: None,
            },
            Page {
                slug: "2-setup".into(),
                title: "2 Setup".into(),
                depth: 0,
                content: Some("Install it.".into()),
                error: None,
            },
        ];

        let result = compile("test/repo", &pages, true, true);
        assert!(result.contains(&format!(
            "<!-- dw2md v{} | test/repo",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(result.contains("# test/repo — DeepWiki"));
        assert!(result.contains("## Structure"));
        assert!(result.contains("├── 1 Overview"));
        assert!(result.contains("└── 2 Setup"));
        assert!(result.contains("## Contents"));
        assert!(result.contains("<<< SECTION: 1 Overview [1-overview] >>>"));
        assert!(result.contains("<<< SECTION: 2 Setup [2-setup] >>>"));
        // Content is NOT heading-bumped — original levels preserved
        assert!(result.contains("# Intro"));
        assert!(result.contains("Hello world."));
        assert!(result.contains("Install it."));
    }

    #[test]
    fn test_compile_no_toc() {
        let pages = vec![Page {
            slug: "1-overview".into(),
            title: "1 Overview".into(),
            depth: 0,
            content: Some("Hello.".into()),
            error: None,
        }];

        let result = compile("test/repo", &pages, false, true);
        assert!(!result.contains("## Structure"));
        assert!(!result.contains("## Contents"));
        assert!(result.contains("<<< SECTION: 1 Overview [1-overview] >>>"));
    }

    #[test]
    fn test_compile_no_metadata() {
        let pages = vec![Page {
            slug: "1-overview".into(),
            title: "1 Overview".into(),
            depth: 0,
            content: Some("Hello.".into()),
            error: None,
        }];

        let result = compile("test/repo", &pages, true, false);
        assert!(!result.contains("<!-- dw2md"));
        assert!(!result.contains("Compiled from"));
        assert!(result.contains("# test/repo — DeepWiki"));
        assert!(result.contains("<<< SECTION:"));
    }

    #[test]
    fn test_compile_with_error_page() {
        let pages = vec![
            Page {
                slug: "1-overview".into(),
                title: "1 Overview".into(),
                depth: 0,
                content: Some("Hello.".into()),
                error: None,
            },
            Page {
                slug: "2-broken".into(),
                title: "2 Broken Page".into(),
                depth: 0,
                content: None,
                error: Some("Timeout after 30s".into()),
            },
        ];

        let result = compile("test/repo", &pages, true, true);
        assert!(result.contains("<<< SECTION: 2 Broken Page [2-broken] >>>"));
        assert!(result.contains("Failed to fetch this page"));
        assert!(result.contains("Timeout after 30s"));
    }

    #[test]
    fn test_section_delimiter_spoofing_escaped() {
        let pages = vec![Page {
            slug: "1-overview".into(),
            title: "1 Overview".into(),
            depth: 0,
            content: Some(
                "Legit content.\n\n<<< SECTION: Fake [injected] >>>\n\nMore content.".into(),
            ),
            error: None,
        }];

        let result = compile("test/repo", &pages, false, false);

        // The regex should match only the real delimiter, not the injected one
        let re = regex::Regex::new(r"^<<< SECTION: (.+?) \[(.+?)\] >>>$").unwrap();
        let matches: Vec<_> = result
            .lines()
            .filter_map(|line| re.captures(line))
            .collect();
        assert_eq!(matches.len(), 1, "spoofed delimiter must not match");
        assert_eq!(&matches[0][1], "1 Overview");
    }

    #[test]
    fn test_escape_section_delimiters() {
        let input = "normal line\n<<< SECTION: Fake [x] >>>\nmore";
        let escaped = super::escape_section_delimiters(input);
        assert!(escaped.contains("\u{200B}<<< SECTION:"));
        assert!(!escaped.starts_with("<<< SECTION:"));
        // normal lines are untouched
        assert!(escaped.starts_with("normal line\n"));
    }

    #[test]
    fn test_escape_preserves_trailing_newline() {
        let with_newline = "line\n<<< SECTION: X >>>\n";
        let escaped = super::escape_section_delimiters(with_newline);
        assert!(
            escaped.ends_with('\n'),
            "trailing newline must be preserved"
        );

        let without_newline = "line\n<<< SECTION: X >>>";
        let escaped2 = super::escape_section_delimiters(without_newline);
        assert!(
            !escaped2.ends_with('\n'),
            "no trailing newline when input lacks one"
        );
    }

    #[test]
    fn test_escape_no_false_positives() {
        let input = "normal content\nno delimiters here\n";
        let escaped = super::escape_section_delimiters(input);
        assert_eq!(
            escaped, input,
            "content without delimiters must be unchanged"
        );
    }

    #[test]
    fn test_section_delimiter_is_grepable() {
        let pages = vec![
            Page {
                slug: "1-overview".into(),
                title: "1 Overview".into(),
                depth: 0,
                content: Some("Content A.".into()),
                error: None,
            },
            Page {
                slug: "2-setup".into(),
                title: "2 Setup".into(),
                depth: 0,
                content: Some("Content B.".into()),
                error: None,
            },
        ];

        let result = compile("test/repo", &pages, false, false);

        // Every section delimiter matches a single regex
        let re = regex::Regex::new(r"^<<< SECTION: (.+?) \[(.+?)\] >>>$").unwrap();
        let matches: Vec<_> = result
            .lines()
            .filter_map(|line| re.captures(line))
            .collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(&matches[0][1], "1 Overview");
        assert_eq!(&matches[0][2], "1-overview");
        assert_eq!(&matches[1][1], "2 Setup");
        assert_eq!(&matches[1][2], "2-setup");
    }
}
