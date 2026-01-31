# Code Review API Server

A Rust API server that analyzes git repositories with rule-based checks and AI-powered feedback.

## Features

- **Rule-Based Checkers**
  - Linting (JavaScript/TypeScript via OXC)
  - TODO/FIXME/HACK comment detection
  - Common typo detection
  - Formatting issues (whitespace, indentation, line length)

- **AI-Powered Analysis**
  - Typo validation (filters false positives)
  - Comment prioritization
  - Architectural suggestions (CodeOracle)
  - Product hardening recommendations

- **Streaming Results** via Server-Sent Events (SSE)

## Quick Start

### Local Development

```bash
cargo run
```

### With AI Features

```bash
ANTHROPIC_API_KEY="sk-ant-..." cargo run
```

### Docker

```bash
docker compose up --build
```

## Review Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            POST /api/review                                  │
│                         { "repo_url": "..." }                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  1. CREATE REVIEW                                                           │
│     - Generate UUID                                                         │
│     - Emit: ReviewStarted                                                   │
│     - Return: { "review_id": "..." } (immediate response)                   │
│     - Spawn background task                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  2. VALIDATE & CLONE (status: cloning)                                      │
│     - GitHub API validation (fails fast on 404)                             │
│     - git clone --depth 1 to temp directory                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  3. RULE-BASED CHECKERS (status: running)                                   │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  Linter         → JS/TS lint (OXC): NoDebugger, NoConsole, etc. │    │
│     │  CommentChecker → TODO/FIXME/HACK/NOTE detection                │    │
│     │  TyposChecker   → Common typo detection (dictionary-based)      │    │
│     │  FormatChecker  → Trailing whitespace, line length, indent      │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│     - Emits: CheckStarted / CheckCompleted per checker                      │
│     - Output: Vec<Diagnostic>                                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  4. AI VALIDATORS (when LLM configured)                                     │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  TypoValidator    → Filter typo false positives                 │    │
│     │  CommentValidator → Remove unnecessary comment markers          │    │
│     │  Prioritizer      → AI-based severity adjustment                │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│     - Emits: ValidationStarted / ValidationCompleted per validator          │
│     - Output: Filtered Vec<Diagnostic>                                      │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  5. AI REVIEWERS (when LLM configured)                                      │
│     - Build CodeContext (repo_url, up to 20 files, diagnostics)             │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  CodeOracle           → Architecture/performance/security/quality│    │
│     │  ProductIdeasReviewer → Product hardening/deployment/UX ideas   │    │
│     └──────────────────────────────────────────────────────────────────┘    │
│     - Emits: ReviewerStarted / ReviewerCompleted per reviewer               │
│     - Output: Vec<Suggestion>                                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  6. COMPLETE (status: completed)                                            │
│     - Emit: ReviewCompleted (summary with counts and duration)              │
│     - Store: results (diagnostics) + suggestions                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### LLM Provider Priority

```
Anthropic (sk-ant-oat* → OAuth, otherwise → API Key)
    ↓ (if not configured)
OpenAI
    ↓ (if not configured)
OpenCode
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/review` | POST | Create review `{"repo_url": "..."}` |
| `/api/review/{id}` | GET | Get review status and results |
| `/api/review/{id}/stream` | GET | SSE stream of review events |

### Create Review

```bash
curl -X POST http://localhost:8080/api/review \
  -H "Content-Type: application/json" \
  -d '{"repo_url": "https://github.com/user/repo"}'
```

Response:
```json
{"review_id": "uuid-here"}
```

### Get Review Status

```bash
curl http://localhost:8080/api/review/{id}
```

Response:
```json
{
  "id": "uuid",
  "status": "completed",
  "repo_url": "https://github.com/user/repo",
  "results": [...],
  "suggestions": [...],
  "error": null
}
```

### SSE Events

| Event | Description |
|-------|-------------|
| `review_started` | Review initiated |
| `check_started` | Checker began |
| `check_completed` | Checker finished with diagnostics |
| `validation_started` | AI validator began |
| `validation_completed` | AI validator finished |
| `reviewer_started` | AI reviewer began |
| `reviewer_completed` | AI reviewer finished with suggestions |
| `review_completed` | All processing done |
| `review_failed` | Error occurred |

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `8080` | Server port |
| `ANTHROPIC_API_KEY` | - | Anthropic API key or OAuth token |
| `OPENAI_API_KEY` | - | OpenAI API key (fallback) |
| `OPENCODE_API_KEY` | - | OpenCode API key (fallback) |
| `OPENCODE_BASE_URL` | - | Custom OpenCode endpoint |
| `REVIEW_TTL_SECS` | `3600` | Review data TTL (1 hour) |
| `MAX_CONCURRENT_CHECKS` | `4` | Max parallel checkers |
| `RUST_LOG` | `api_server=info` | Log level |

## Architecture

```
src/
├── ai/
│   ├── mod.rs          # Validator, Reviewer traits, CodeContext
│   ├── validators.rs   # TypoValidator, CommentValidator, Prioritizer
│   └── reviewers.rs    # CodeOracle, ProductIdeasReviewer
├── checkers/
│   ├── mod.rs          # run_all_checkers orchestration
│   ├── linter.rs       # JS/TS linting with OXC
│   ├── comments.rs     # TODO/FIXME/HACK detection
│   ├── typos.rs        # Common typo detection
│   └── format.rs       # Formatting checks
├── llm/
│   ├── mod.rs          # ModelClient trait
│   ├── anthropic.rs    # Anthropic client (API key + OAuth)
│   ├── openai.rs       # OpenAI client
│   ├── opencode.rs     # OpenCode client
│   ├── retry.rs        # Retry configuration
│   └── tokens.rs       # Token management
├── api.rs              # HTTP endpoints
├── orchestrator.rs     # Review coordination and state
├── git.rs              # Repository cloning with validation
├── config.rs           # Configuration loading
├── types.rs            # Data models and events
├── error.rs            # Error types
└── main.rs             # Server entry point
```

## License

MIT
