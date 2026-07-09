# atlassian-cli

A Rust CLI for interacting with the Jira and Confluence REST APIs. Built with [`clap`](https://github.com/clap-rs/clap) for argument parsing and [`reqwest`](https://github.com/seanmonstar/reqwest) for HTTP.

## Installation

```bash
cargo build --release
# Binary is at target/release/atlassian
```

## Configuration

Create `~/.config/atlassian-cli/config.toml`:

```toml
cloud_id       = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
email          = "you@example.com"
jira_api_token = "your-jira-api-token-here"         # or JIRA_API_TOKEN env var
confluence_api_token = "your-confluence-api-token-here"  # or CONFLUENCE_API_TOKEN env var

server          = "https://yourcompany.atlassian.net"  # required for browser open
default_project = "PROJ"

# Optional: custom fields shown in issue preview/detail
[[custom_fields]]
name  = "Acceptance Criteria"
id    = "customfield_11100"   # instance-specific; discover with /rest/api/3/field
lines = 5

[[custom_fields]]
name     = "Story Points"
id       = "customfield_10010"
markdown = true   # numeric field — skip ADF parsing
```

Generate API tokens with scopes at: https://id.atlassian.com/manage-profile/security/api-tokens  
Choose **"Create API token with scopes"** (not the legacy token).

Required scopes:
- **Jira token**: `read:jira-work`, `write:jira-work`, `read:jira-user`
- **Confluence token**: `read:confluence-content.all`, `write:confluence-content`, `read:confluence-space.summary`

Discover custom field IDs:
```sh
curl -u email:$JIRA_API_TOKEN \
  "https://api.atlassian.com/ex/jira/<cloud_id>/rest/api/3/field" \
  | jq '[.[] | select(.custom==true) | {name, id}] | sort_by(.name)'
```

## Jira Commands

### Issue

```sh
# List
atlassian jira issue list
atlassian jira issue list --project PROJ
atlassian jira issue list -a$(atlassian jira me) -yHigh -sopen
atlassian jira issue list -s~Done --created-before -4w
atlassian jira issue list --order-by rank --reverse

# Get / View
atlassian jira issue get PROJ-123
atlassian jira issue view PROJ-123
atlassian jira issue view PROJ-123 --comments 10

# Create
atlassian jira issue create -p PROJ -t Story -s "My story"
atlassian jira issue create -p PROJ -t Story -s "Child" --parent PROJ-1

# Edit
atlassian jira issue edit PROJ-123 -s "New summary"
atlassian jira issue edit PROJ-123 -b "Updated description"
atlassian jira issue edit PROJ-123 -y High
atlassian jira issue edit PROJ-123 -l frontend -l -old-label    # add/remove labels
atlassian jira issue edit PROJ-123 -C Backend -C -OldTeam       # add/remove components

# Assign
atlassian jira issue assign PROJ-123 "Jane Doe"   # search by name
atlassian jira issue assign PROJ-123 x            # unassign
atlassian jira issue assign PROJ-123 default      # default assignee

# Move (transition status)
atlassian jira issue move PROJ-123

# Link
atlassian jira issue link PROJ-1 PROJ-2
atlassian jira issue link PROJ-1 PROJ-2 "Blocks"
atlassian jira issue unlink PROJ-1 PROJ-2

# Attachments
atlassian jira issue attachment PROJ-123          # list attachments
atlassian jira issue attachment PROJ-123 --open 1 # open attachment #1 in default app
atlassian jira issue attachment PROJ-123 --open 1 --save  # save to current directory

# Comments
atlassian jira issue comment list PROJ-123        # list all comments
atlassian jira issue comment list PROJ-123 -n 5   # last 5 comments
atlassian jira issue comment add PROJ-123 "My comment"
atlassian jira issue comment add PROJ-123 --template /path/to/file.txt
atlassian jira issue comment add PROJ-123 --template -   # read from stdin
echo "comment" | atlassian jira issue comment add PROJ-123
atlassian jira issue comment add PROJ-123 --web          # open in browser after
```

> **Markdown support:** comment and description text is written in Markdown and
> automatically converted to Jira's native ADF (Atlassian Document Format).
> Supported: headings, **bold**, *italic*, `inline code`, fenced code blocks,
> bullet/ordered lists, blockquotes, and `[links](url)`. Applies to
> `issue comment add`, `issue create --description`, and `issue edit --body`.

#### `issue list` flags

| Flag | Short | Description |
|---|---|---|
| `--project` | `-p` | Filter by project key |
| `--status` | `-s` | Filter by status; prefix with `~` to negate |
| `--assignee` | `-a` | Filter by assignee; `x` = unassigned, `~x` = any assignee |
| `--priority` | `-y` | Filter by priority |
| `--type` | `-t` | Filter by issue type |
| `--label` | `-l` | Filter by label |
| `--created` | | Created since (e.g. `-7d`, `-2w`) |
| `--created-before` | | Created before |
| `--updated` | | Updated since |
| `--updated-before` | | Updated before |
| `--order-by` | | Sort field (e.g. `rank`, `created`) |
| `--reverse` | | Reverse sort order |
| `--watching` | `-w` | Issues you are watching |
| `--plain` | | Plain text output (no TUI) |
| `--table` | | Table output |
| `--csv` | | CSV output |

### Epic

```sh
atlassian jira epic list -p PROJ
atlassian jira epic list PROJ-123            # child issues of this epic
atlassian jira epic list PROJ-123 --plain
atlassian jira epic create -p PROJ -s "My Epic"
```

### Project

```sh
atlassian jira project list
atlassian jira project get PROJ
```

### Me

```sh
atlassian jira me
```

## Confluence Commands

```sh
# Spaces
atlassian confluence space list
atlassian confluence space list --type personal
atlassian confluence space list -n "Engineering"

# Pages
atlassian confluence page list                  # recent pages (TUI)
atlassian confluence page list --space ENGR     # pages in space (TUI)
atlassian confluence page get PAGE-ID           # plain text output
atlassian confluence page view PAGE-ID          # TUI full-screen view
atlassian confluence page view PAGE-ID --raw    # print raw ADF JSON

# Search
atlassian confluence search "my query"
atlassian confluence search "my query" --space ENGR
```

## TUI Key Bindings

### Jira Issue List / Epic List

| Key | Action |
|---|---|
| `↑` / `↓` / `j` / `k` | Navigate rows |
| `Enter` | Open issue in browser (stay in TUI) |
| `v` | Open full-screen detail view |
| `g` / `G` | Jump to top / bottom |
| `?` | Toggle help overlay |
| `q` / `Esc` | Quit |

### Jira Issue Detail View

| Key | Action |
|---|---|
| `↑` / `↓` / `j` / `k` | Scroll |
| `Enter` / click link | Open linked ticket in browser |
| `q` / `Esc` | Back to list (cursor restored) |

### Confluence Page List

| Key | Action |
|---|---|
| `↑` / `↓` / `j` / `k` | Navigate |
| `/` | Enter search mode (CQL title search) |
| `Esc` | Cancel search / quit |
| `Enter` | Open page in browser |
| `v` | Full-screen page view |
| `g` / `G` | Jump to top / bottom |
| `q` | Quit |

## Development

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo test                     # run all tests
cargo clippy -- -D warnings    # lint
cargo fmt                      # format
```
