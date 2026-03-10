---
name: fetch-deepwiki
description: Fetch and compile DeepWiki documentation for a GitHub repository into a single markdown file. Use when the user wants to understand an open-source project's architecture, pull repo documentation from DeepWiki, or needs structured documentation for spec-drafting and design work.
---

# fetch-deepwiki

Fetch structured documentation from [DeepWiki](https://deepwiki.com) for any public GitHub repository using the `dw2md` CLI tool. The output is a single markdown document optimized for LLM context windows — with a tree-structured table of contents and grep-friendly section delimiters.

## When to use this skill

- User wants to understand the architecture of an open-source project
- User is drafting a spec or design doc and needs reference documentation
- User mentions DeepWiki or wants repo-level documentation
- User wants to study how another project implements something
- User is comparing approaches across repositories

## Prerequisites

`dw2md` must be installed. If not available, install with:

```bash
cargo install dw2md
```

Or on macOS/Linux:

```bash
brew install tnguyen21/dw2md/dw2md
```

## Usage

### Basic — fetch entire wiki

```bash
dw2md <owner/repo> -o <output-file>.md
```

### List available sections first

When the user wants to explore what's available before downloading everything:

```bash
dw2md <owner/repo> --list
```

This prints a tree of all sections with their slugs. Use the slugs with `-p` to fetch specific sections.

### Fetch specific sections only

When only certain topics are relevant (saves time and tokens):

```bash
dw2md <owner/repo> -p "slug1,slug2,slug3" -o <output-file>.md
```

### Exclude sections

When most content is needed but some sections are irrelevant:

```bash
dw2md <owner/repo> -x "slug-to-skip" -o <output-file>.md
```

### Minimal output (no metadata, no TOC)

When you just need the raw content with section delimiters:

```bash
dw2md <owner/repo> --no-toc --no-metadata
```

## Workflow

1. **Always run `--list` first** to show the user what sections are available, unless they've asked for the full wiki
2. **Suggest filtering** if the wiki has many pages — fetching only relevant sections saves time and keeps context focused
3. **Write to a file** with `-o` so the output persists and can be referenced later
4. After fetching, **read the output file** and use it to inform your work (spec-drafting, architecture review, etc.)

## Output format

The markdown output uses `<<< SECTION: Title [slug] >>>` delimiters between sections. To find specific sections:

```bash
grep "^<<< SECTION:" <output-file>.md
```

## Input formats

The repo argument accepts any of:
- `owner/repo` (e.g., `tinygrad/tinygrad`)
- `https://deepwiki.com/owner/repo`
- A full DeepWiki page URL (extracts owner/repo automatically)
