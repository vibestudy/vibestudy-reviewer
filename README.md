# Code Review & Grade API Server

A Rust API server that analyzes git repositories with rule-based checks, AI-powered feedback, and automated grading of code submissions against acceptance criteria.

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

- **Code Grading System** (NEW)
  - Task-based acceptance criteria evaluation
  - Weighted scoring with confidence levels
  - Code reference evidence for each criterion
  - Korean grade output (우수/양호/보통/미흡/불합격)

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

## Grade Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              POST /api/grade                                │
│                  { "repo_url": "...", "tasks": [...] }                      │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  1. CREATE GRADE (status: pending)                                          │
│     - Generate UUID                                                         │
│     - Emit: GradeStarted { task_count, total_criteria }                     │
│     - Return: { "grade_id": "...", "status": "pending" }                    │
│     - Spawn background task                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  2. CLONE REPOSITORY (status: cloning)                                      │
│     - Emit: CloningStarted                                                  │
│     - Validate GitHub URL, clone with --depth 1                             │
│     - Emit: CloningCompleted { duration_ms }                                │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  3. ANALYZE CODE (status: analyzing)                                        │
│     - Emit: AnalysisStarted                                                 │
│     - Read source files (max 50 files, configurable)                        │
│     - Filter: .rs, .ts, .tsx, .js, .py, .go, etc.                          │
│     - Skip: node_modules, target, dist, hidden files                        │
│     - Emit: AnalysisCompleted { file_count, total_lines }                   │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  4. GRADE TASKS (status: grading) - Parallel Processing                     │
│     ┌──────────────────────────────────────────────────────────────────┐    │
│     │  For each GradeTask (parallel, max 3 concurrent):                │    │
│     │    - Emit: TaskStarted { task_index, task_title, criteria_count }│    │
│     │    ┌────────────────────────────────────────────────────────┐    │    │
│     │    │  For each Criterion (parallel, max 5 concurrent):      │    │    │
│     │    │    - CriteriaChecker.check_criterion() via LLM         │    │    │
│     │    │    - Emit: CriterionChecked { passed, confidence }     │    │    │
│     │    │    - Output: CriterionResult with code_references      │    │    │
│     │    └────────────────────────────────────────────────────────┘    │    │
│     │    - Calculate weighted task score                               │    │
│     │    - Emit: TaskCompleted { score, status, passed/total }        │    │
│     └──────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  5. COMPLETE (status: completed)                                            │
│     - Calculate overall score: average of task scores                       │
│     - Determine grade:                                                      │
│         90-100% → 우수 | 75-89% → 양호 | 60-74% → 보통                      │
│         40-59% → 미흡 | <40% → 불합격                                        │
│     - Emit: GradeCompleted { overall_score, percentage, grade, summary }    │
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
| `/api/review` | POST | Create code review |
| `/api/review/{id}` | GET | Get review status and results |
| `/api/review/{id}/stream` | GET | SSE stream of review events |
| `/api/grade` | POST | Create grade job |
| `/api/grade/{id}` | GET | Get grade status and results |
| `/api/grade/{id}/stream` | GET | SSE stream of grade events |

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

### Create Grade

```bash
curl -X POST http://localhost:8080/api/grade \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/user/repo",
    "tasks": [
      {
        "title": "Implement User Authentication",
        "description": "Add login/logout functionality",
        "acceptance_criteria": [
          { "description": "Login form exists with email and password fields", "weight": 1.0 },
          { "description": "Passwords are hashed before storage", "weight": 2.0 },
          { "description": "JWT tokens are used for session management", "weight": 1.5 }
        ]
      }
    ],
    "config": {
      "max_files": 50,
      "max_chars_per_file": 4000,
      "max_parallel_tasks": 3,
      "max_parallel_criteria": 5
    }
  }'
```

Response:
```json
{
  "grade_id": "uuid-here",
  "status": "pending"
}
```

### Get Grade Status

```bash
curl http://localhost:8080/api/grade/{id}
```

Response:
```json
{
  "id": "uuid",
  "status": "completed",
  "repo_url": "https://github.com/user/repo",
  "overall_score": 0.83,
  "percentage": 83,
  "grade": "양호",
  "tasks": [
    {
      "task_title": "Implement User Authentication",
      "score": 0.83,
      "status": "partial",
      "passed_count": 2,
      "total_count": 3,
      "criteria_results": [
        {
          "criterion": "Login form exists with email and password fields",
          "passed": true,
          "confidence": 0.95,
          "evidence": "Found LoginForm component in src/components/LoginForm.tsx with email and password input fields",
          "code_references": [
            { "file": "src/components/LoginForm.tsx", "line_start": 15, "line_end": 28 }
          ],
          "weight": 1.0
        }
      ]
    }
  ],
  "summary": "전체 점수: 83점 (양호) - 과제 0/1 완료, 기준 2/3 충족",
  "error": null
}
```

## SSE Events

### Review Events

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

### Grade Events

| Event | Description |
|-------|-------------|
| `grade_started` | Grade job initiated with task/criteria counts |
| `cloning_started` | Repository cloning began |
| `cloning_completed` | Repository cloned successfully |
| `analysis_started` | Code analysis began |
| `analysis_completed` | Files read and analyzed |
| `task_started` | Individual task grading began |
| `criterion_checked` | Single criterion evaluated |
| `task_completed` | Task grading finished with score |
| `grade_completed` | All tasks graded, final score calculated |
| `grade_failed` | Error occurred |

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server bind address |
| `PORT` | `8080` | Server port |
| `ANTHROPIC_API_KEY` | - | Anthropic API key or OAuth token |
| `OPENAI_API_KEY` | - | OpenAI API key (fallback) |
| `OPENCODE_API_KEY` | - | OpenCode API key (fallback) |
| `OPENCODE_BASE_URL` | - | Custom OpenCode endpoint |
| `REVIEW_TTL_SECS` | `3600` | Review/Grade data TTL (1 hour) |
| `MAX_CONCURRENT_CHECKS` | `4` | Max parallel checkers |
| `RUST_LOG` | `api_server=info` | Log level |

### Grade Config (per-request)

| Field | Default | Description |
|-------|---------|-------------|
| `max_files` | `50` | Max source files to analyze |
| `max_chars_per_file` | `4000` | Max characters per file sent to LLM |
| `max_parallel_tasks` | `3` | Concurrent tasks to grade |
| `max_parallel_criteria` | `5` | Concurrent criteria to check |

## Architecture

```
src/
├── ai/
│   ├── mod.rs          # Validator, Reviewer, Grader traits, CodeContext
│   ├── validators.rs   # TypoValidator, CommentValidator, Prioritizer
│   ├── reviewers.rs    # CodeOracle, ProductIdeasReviewer
│   └── graders.rs      # CriteriaChecker (grading system)
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
├── api.rs              # HTTP endpoints (review + grade)
├── orchestrator.rs     # Review coordination and state
├── grade_orchestrator.rs # Grade coordination and state
├── git.rs              # Repository cloning with validation
├── config.rs           # Configuration loading
├── types.rs            # Data models, events, and grade types
├── error.rs            # Error types
├── shutdown.rs         # Graceful shutdown handling
├── lib.rs              # Library exports
└── main.rs             # Server entry point
```

## Data Types

### Grade Types

```rust
// Task to be graded
struct GradeTask {
    title: String,
    description: Option<String>,
    acceptance_criteria: Vec<Criterion>,
    estimated_minutes: Option<u32>,
}

// Single acceptance criterion
struct Criterion {
    id: Option<String>,
    description: String,
    weight: f32,  // Default: 1.0
}

// Result of checking a criterion
struct CriterionResult {
    criterion: String,
    passed: bool,
    confidence: f32,  // 0.0 to 1.0
    evidence: String,
    code_references: Vec<CodeRef>,
    weight: f32,
}

// Code location reference
struct CodeRef {
    file: String,
    line_start: u32,
    line_end: u32,
    snippet: Option<String>,
}

// Task grading result
struct TaskGradeResult {
    task_title: String,
    score: f32,  // 0.0 to 1.0
    status: TaskStatus,  // Passed | Partial | Failed
    criteria_results: Vec<CriterionResult>,
    passed_count: usize,
    total_count: usize,
}
```

### Grade Scale

| Score | Grade | Korean |
|-------|-------|--------|
| 90-100% | Excellent | 우수 |
| 75-89% | Good | 양호 |
| 60-74% | Average | 보통 |
| 40-59% | Below Average | 미흡 |
| < 40% | Fail | 불합격 |

## License

MIT
