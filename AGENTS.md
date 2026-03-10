# AGENTS.md

## dw2md — DeepWiki to Markdown

This repository contains `dw2md`, a CLI tool that compiles an entire [DeepWiki](https://deepwiki.com) wiki into a single, LLM-optimized markdown file.

### When to use dw2md

Use `dw2md` when you need structured documentation for any public GitHub repository — for architecture review, spec-drafting, design work, or understanding how a project is built. DeepWiki generates high-quality documentation for open-source repos; `dw2md` makes it accessible in one command.

### Quick reference

```bash
# List available sections for a repo
dw2md owner/repo --list

# Fetch entire wiki to a file
dw2md owner/repo -o wiki.md

# Fetch specific sections only (use slugs from --list)
dw2md owner/repo -p "1-overview,3-architecture" -o wiki.md

# Exclude sections you don't need
dw2md owner/repo -x "7-developer-tools" -o wiki.md

# Minimal output — just content with section delimiters
dw2md owner/repo --no-toc --no-metadata

# Pipe directly to clipboard (macOS)
dw2md owner/repo | pbcopy
```

### Output format

The output uses `<<< SECTION: Title [slug] >>>` delimiters, designed for grep and programmatic extraction:

```bash
grep "^<<< SECTION:" wiki.md              # List all sections
```

### Install

```bash
cargo install dw2md          # From crates.io
brew install tnguyen21/dw2md/dw2md  # Homebrew
```

### Tips for agents

- Run `--list` first to see what's available before fetching everything
- Use `-p` to fetch only the sections relevant to your current task — this saves time and keeps context focused
- Write output to a file with `-o` so it persists across turns
- The JSON format (`-f json`) is useful if you need to process individual pages separately
