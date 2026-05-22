---
name: atlassian-cli
description: Custom Rust CLI for Jira and Confluence REST APIs located at ~/dev/atlassian-cli. Use when working on or with the atlassian-cli project — creating issues, epics, listing tickets, TUI views, Confluence pages, and Jira workflows.
license: MIT
metadata:
  author: hsun
  version: "1.0"
---

# atlassian-cli

Rust CLI for interacting with the Jira and Confluence REST APIs. Located at `~/dev/atlassian-cli`. Uses `clap` for argument parsing and `reqwest` for HTTP.

## Build, Test & Run

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo test                     # run all tests
cargo run -- <subcommand>      # e.g.: cargo run -- issue get PROJ-123
cargo clippy -- -D warnings    # lint
cargo fmt --check              # check formatting (CI enforcer)
cargo fmt                      # auto-format
```

## Architecture

```
src/
  main.rs          - entry point; all command dispatch, JQL builders, fetch_children, open_browser
  cli/             - one module per top-level subcommand
    issue.rs       - `issue` subcommand: get, list, create, move, view
    epic.rs        - `epic` subcommand: list (with optional epic-key for child issues), create
    project.rs     - `project` subcommand: list, get
    me.rs          - `me` subcommand: show current user
    confluence.rs  - `confluence` subcommand: page list/get/view, space list, search
    mod.rs         - CLI tree root; JiraCommands enum
  client/
    mod.rs         - JiraClient struct, auth, base URL; all API types (Issue, IssueFields, Comment, etc.)
    confluence.rs  - ConfluenceClient struct; all Confluence API types
  output/
    mod.rs         - adf_to_text, format_field_value, color helpers, print_issue_detail
    table.rs       - OutputOptions + 11-column table (CSV/delimiter/column selection)
    tui.rs         - ratatui TUI: run_tui (list+preview), run_issue_view (full detail), click maps
  config.rs        - Config + CustomField structs; reads ~/.config/atlassian-cli/config.toml
  error.rs         - AppError enum
```

- **Jira auth**: Basic auth (`email:token`) against `https://api.atlassian.com/ex/jira/<cloudId>/rest/api/3/...`. `cloud_id` in config; `JIRA_API_TOKEN` env var always overrides config.
- **Confluence auth**: Basic auth (`email:token`) against `https://api.atlassian.com/ex/confluence/<cloudId>/wiki/rest/api/...`. Same `cloud_id` as Jira; `CONFLUENCE_API_TOKEN` env var always overrides config.

## Key Conventions

- **Error handling**: use `anyhow::Result` for all fallible operations; avoid `.unwrap()` outside tests.
- **Async runtime**: `tokio` with `#[tokio::main]`. All network calls async; config.rs and CLI parsing sync.
- **Output modes**: TUI (default, TTY) → `--plain` / `--table` / `--csv` / `--raw` for non-interactive.
- **Pagination**: `nextPageToken` cursor-based; always support `--max-results` and `--paginate <from>:<limit>`.
- **Config file**: `~/.config/atlassian-cli/config.toml` (XDG path checked first; see Pitfall #1).

## Implemented Commands

| Command | Description |
|---|---|
| `jira issue list [flags]` | List issues with full jira-cli flag parity |
| `jira issue get KEY` | Show issue detail (non-TUI) |
| `jira issue create [-t TYPE] [-s SUMMARY] [--parent KEY]` | Create issue; `--parent` sets epic/parent |
| `jira issue edit KEY [-s SUMMARY] [-b BODY] [-y PRIORITY] [-l LABEL] [-C COMP] [-a ASSIGNEE]` | Edit issue fields; prefix label/component/fix-version with `-` to remove |
| `jira issue assign KEY [USER]` | Assign by name/account ID; `x` = unassign, `default` = default assignee |
| `jira issue move KEY` | Transition issue status |
| `jira issue link ISSUE-1 ISSUE-2 [TYPE]` | Link two issues; prompts for link type if omitted |
| `jira issue unlink ISSUE-1 ISSUE-2` | Remove link(s) between two issues |
| `jira issue view KEY [--comments N]` | TUI detail view; all fields, comments, subtasks, work items, custom fields |
| `jira issue attachment KEY` | List attachments on an issue |
| `jira issue attachment KEY --open N [--save]` | Download attachment N; open in default app, or save to cwd with `--save` |
| `jira issue comment list KEY [-n N]` | List comments; `-n` limits to last N |
| `jira issue comment add KEY [BODY]` | Add comment; reads from arg, `--template FILE`, `--template -` (stdin), piped stdin, or interactive prompt; `--web` opens browser after |
| `jira epic list [EPIC-KEY] [flags]` | List epics, or child issues of an epic |
| `jira epic create [-s SUMMARY] [-p PROJ]` | Create a new epic |
| `jira project list / get KEY` | List / show projects |
| `jira me` | Show current user |
| `confluence space list [--type global\|personal] [-n NAME]` | List Confluence spaces |
| `confluence page list [-s KEY]` | TUI page list; `/` to search, `v` to view |
| `confluence page get PAGE-ID` | Show page detail (non-TUI) |
| `confluence page view PAGE-ID [--raw]` | Full-screen TUI page view; `--raw` prints raw ADF JSON |
| `confluence search QUERY [-s SPACE]` | CQL title search |

## CLI Design Examples

```sh
atlassian jira issue list --project PROJ
atlassian jira issue list -a$(atlassian jira me) -yHigh -sopen
atlassian jira issue list -s~Done --created-before -4w
atlassian jira issue create -pPROJ -t Story -s "My story" --parent PROJ-1
atlassian jira epic list -pPROJ
atlassian jira epic list PROJ-123 --plain
atlassian jira epic create -pPROJ -s "My Epic"
atlassian jira issue attachment PROJ-123           # list attachments
atlassian jira issue attachment PROJ-123 --open 1  # open #1 in default app
atlassian jira issue attachment PROJ-123 --open 1 --save  # save to cwd
atlassian jira issue comment list PROJ-123         # list all comments
atlassian jira issue comment list PROJ-123 -n 5    # last 5 comments
atlassian jira issue comment add PROJ-123 "My comment"
echo "comment" | atlassian jira issue comment add PROJ-123
atlassian jira issue comment add PROJ-123 --template /path/to/file.txt
atlassian jira issue comment add PROJ-123 --web    # open browser after
```

### Clap quirks
- Values starting with `-` (like `-7d`) need `allow_hyphen_values = true`.
- `~` negation: `~Done` → `status NOT IN ("Done")`; `~x` on assignee → `assignee is not EMPTY`.

## TUI Key Bindings (Jira)

| Key | List view | Detail view |
|---|---|---|
| `↑↓` / `j`/`k` | navigate rows | scroll content |
| `Enter` | open in browser (stay) | open current issue in browser (stay) |
| `v` | open full detail view | — |
| `g` / `G` | jump to top / bottom | jump to top |
| `?` | toggle help overlay | — |
| `q` / `Esc` | quit | back to list (cursor restored) |

**TuiAction variants**: `Browse(key)`, `Detail(key)`, `Back(idx)`, `Quit`, `Search(query)`

**Signatures:**
```rust
pub fn run_tui(issues: &[Issue], total: u32, initial_index: usize, custom_fields: &[CustomField]) -> io::Result<TuiAction>
pub fn run_issue_view(issue: &Issue, children: &[Issue], comments_limit: Option<usize>, back_index: usize, custom_fields: &[CustomField]) -> io::Result<TuiAction>
pub fn run_page_list_tui(pages: Arc<Mutex<Vec<Page>>>, loading: Arc<AtomicBool>, config: &Config) -> io::Result<PageTuiAction>
pub fn run_page_view(page: &Page, config: &Config) -> io::Result<()>
```

## `create_issue` API Signature

```rust
pub async fn create_issue(
    &self,
    project_key: &str,
    issue_type: &str,
    summary: &str,
    description: Option<&str>,
    assignee_id: Option<&str>,
    priority: Option<&str>,
    labels: &[String],
    components: &[String],
    parent_key: Option<&str>,   // sets "parent": {"key": ...} — Next-gen projects only
) -> Result<Issue>
```

## Config File (`~/.config/atlassian-cli/config.toml`)

```toml
cloud_id  = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
email     = "you@example.com"
jira_api_token = "your-jira-api-token-here"               # or JIRA_API_TOKEN env var (scoped token required)
confluence_api_token = "your-confluence-token"  # or CONFLUENCE_API_TOKEN env var (scoped token required)
server          = "https://yourcompany.atlassian.net"
default_project = "PROJ"

[[custom_fields]]
name     = "Acceptance Criteria"
id       = "customfield_11100"
lines    = 5

[[custom_fields]]
name     = "Story Points"
id       = "customfield_10010"
markdown = true   # numeric field — skip ADF parsing
```

## ADF Rendering (`output::adf_to_text`)

| Node type | Rendered as |
|---|---|
| `paragraph` | text + newline |
| `heading` | `## heading\n` |
| `bulletList` / `listItem` | `• item` |
| `orderedList` / `listItem` | `1. item` |
| `taskList` / `taskItem` | `☑ done` / `☐ todo` |
| `text` with `link` mark | `text (url)` |
| `mention` | `@name` |
| `blockquote` | `│ text` |
| `codeBlock` | indented |

## Known Pitfalls

1. **macOS config path** — `dirs::config_dir()` → `~/Library/Application Support/`. Always check XDG (`~/.config/`) first.
2. **Env vars always win** — `JIRA_API_TOKEN` / `CONFLUENCE_API_TOKEN` override config file values.
3. **ANSI-aware padding** — `{:<width$}` counts bytes. Strip ANSI before measuring column width.
4. **Use `GET /search/jql`** — `POST /search` is HTTP 410. Use `GET /rest/api/3/search/jql?jql=…&fields=*all`. Pagination via `nextPageToken`.
5. **Unbounded JQL rejected** — always include at least one filter; default to `assignee = currentUser()`.
6. **Epic child JQL** — `parent = "PROJ-123" OR "Epic Link" = "PROJ-123"` (covers Classic + Next-gen).
7. **Custom field IDs are instance-specific** — look up via `/rest/api/3/field`.
8. **Custom fields via serde flatten** — `#[serde(flatten)] pub custom: HashMap<String, Value>` captures all `customfield_XXXXX`.
9. **ratatui 0.27** — use `f.size()` not `f.area()`; `Paragraph::scroll((row, col))` takes `(u16, u16)`.
10. **`--parent` on `issue create`** — sets `"parent": {"key": ...}`; works for Next-gen projects only. Classic projects use a custom Epic Link field.
11. **Confluence `atlas_doc_format.value` is a JSON string** — must `serde_json::from_str` before passing to `adf_to_text`.
12. **Confluence v2 API forbidden** — stick to `/wiki/rest/api/...`; `/wiki/api/v2/...` returns 401.
13. **Attachment filename sanitization** — always use `Path::file_name()` on `att.filename` before writing to disk; API filenames may contain path traversal sequences.
14. **Attachment download URL validation** — `download_attachment` validates the URL starts with the configured Jira instance prefix (`base_url.trim_end_matches("/rest/api/3")`); prevents SSRF and credential leakage to arbitrary hosts.
15. **`--open 0` on attachment** — 1-based index; `0` is explicitly rejected with an error (not silently treated as index 1).
