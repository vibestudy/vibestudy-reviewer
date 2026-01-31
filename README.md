# Code Review API Server

A Rust API server that analyzes git repositories with rule-based checks and AI-powered feedback.

## Features

- **Rule-Based Checkers**
  - Linting (JavaScript/TypeScript via OXC)
  - TODO/FIXME/HACK comment detection
  - Common typo detection
  - Formatting issues (whitespace, indentation, line length)

- **AI-Powered Analysis** (optional)
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

### Docker

```bash
docker compose up --build
```

## API Endpoints

### Health Check

```
GET /api/health
```

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

```
GET /api/review/{id}
```

### Stream Review Events

```
GET /api/review/{id}/stream
```

Events are streamed as SSE with types:
- `review_started`
- `check_started` / `check_completed`
- `validation_started` / `validation_completed`
- `reviewer_started` / `reviewer_completed`
- `review_completed`

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `8080` | Server port |
| `ANTHROPIC_API_KEY` | - | Anthropic API key for AI features |
| `OPENAI_API_KEY` | - | OpenAI API key (fallback) |
| `OPENCODE_API_KEY` | - | OpenCode API key (fallback) |
| `OPENCODE_BASE_URL` | - | Custom OpenCode endpoint |
| `REVIEW_TTL_SECS` | `3600` | Review data TTL |
| `MAX_CONCURRENT_CHECKS` | `4` | Max parallel checkers |
| `RUST_LOG` | `api_server=info` | Log level |

## Architecture

```
src/
├── ai/           # AI validators and reviewers
├── checkers/     # Rule-based code checkers
├── llm/          # LLM provider clients (OpenAI, Anthropic, OpenCode)
├── api.rs        # HTTP endpoints
├── orchestrator.rs # Review coordination
├── git.rs        # Repository cloning
└── main.rs       # Server entry point
```

## License

MIT
