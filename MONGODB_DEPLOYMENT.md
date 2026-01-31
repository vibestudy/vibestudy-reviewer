# MongoDB Deployment Guide

## Railway Configuration

### Prerequisites

1. Railway project: `omakasem` (Project ID: d703d704-2339-4e10-aa94-0a68e95e4a46)
2. MongoDB service already exists in the project
3. Three backend services: `planner-api`, `code-reviewer-api`, and `web`

### Step 1: Get MongoDB Connection String

1. Go to Railway project dashboard
2. Click on the MongoDB service
3. Navigate to "Variables" tab
4. Copy the `MONGO_URL` or connection string value

### Step 2: Configure Planner Service

Add these environment variables to `planner-api` service:

| Variable | Value | Description |
|----------|-------|-------------|
| `MONGODB_URL` | `<MongoDB connection string>` | Connection string from MongoDB service |
| `MONGODB_DB_NAME` | `omakasem` | Database name (default) |

### Step 3: Configure Code-Reviewer Service

Add these environment variables to `code-reviewer-api` service:

| Variable | Value | Description |
|----------|-------|-------------|
| `MONGODB_URL` | `<MongoDB connection string>` | Connection string from MongoDB service |
| `MONGODB_DB_NAME` | `omakasem` | Database name (default) |

### Step 4: Create MongoDB Indexes

After both services are deployed and running:

1. Install `mongosh` locally if not already installed
2. Run the index creation script:

```bash
mongosh "$MONGODB_URL" < scripts/create_indexes.js
```

Where `$MONGODB_URL` is the connection string from Step 1.

### Step 5: Verify Integration

#### Test Planner Integration

```bash
# Create a session
curl -X POST https://planner-api.railway.app/v1/sessions \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Test Course",
    "description": "Test",
    "total_weeks": 4,
    "hours_per_week": 10,
    "goals": ["Learn"]
  }'

# Complete the session (through normal flow)
# Then save curriculum
curl -X POST https://planner-api.railway.app/v1/curricula \
  -H "Content-Type: application/json" \
  -d '{"session_id": "<session_id_from_above>"}'

# Response should include curriculum_id
```

#### Test Code-Reviewer Integration

```bash
# Submit a grade request with curriculum linking
curl -X POST https://code-reviewer-api.railway.app/api/grade \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/test/repo",
    "curriculum_id": "<curriculum_id_from_planner>",
    "task_id": "<task_id_from_curriculum>",
    "tasks": [{
      "title": "Test Task",
      "acceptance_criteria": [
        {"description": "Works", "weight": 1.0}
      ]
    }]
  }'

# After grading completes, verify task status was updated
curl https://planner-api.railway.app/v1/curricula/<curriculum_id>

# Check that the task now has grade_result populated
```

### Monitoring

Check logs for MongoDB connection:

**Planner logs should show:**
```
Using MongoSessionStore
CurriculumRepository initialized
```

**Code-Reviewer logs should show:**
```
MongoDB connected for grade persistence
```

### Troubleshooting

#### Connection Failures

If services fail to connect to MongoDB:

1. Verify `MONGODB_URL` is correct and accessible
2. Check MongoDB service is running in Railway
3. Ensure firewall rules allow connections (Railway handles this automatically)

#### Missing Indexes

If queries are slow:

1. Verify indexes were created: `mongosh "$MONGODB_URL"`
2. Run: `db.tasks.getIndexes()` to check
3. Re-run the index creation script if needed

#### Session Storage Fallback

Planner will automatically fall back to Redis if MongoDB fails:
- First tries: MongoDB
- Then tries: Redis (if REDIS_URL is set)
- Finally: In-memory (development only)

#### Grade Persistence Disabled

If code-reviewer logs show "Grade persistence disabled":
- MongoDB connection failed
- Grading will still work (in-memory only)
- Results won't persist to database
- Task statuses won't update

### Rollback Plan

If MongoDB integration causes issues:

1. Remove `MONGODB_URL` and `MONGODB_DB_NAME` variables from both services
2. Services will fall back to previous behavior:
   - Planner: Uses Redis or in-memory
   - Code-reviewer: In-memory only (no persistence)
3. No code changes needed - graceful degradation is built-in

### Success Criteria

✅ All checks passed when:

1. Planner can save curricula and retrieve them with tasks
2. Code-reviewer can grade with curriculum_id/task_id
3. After grading, task status updates in curricula collection
4. Both services show MongoDB connection success in logs
5. Indexes exist in all collections

## Schema Summary

### Collections Created

| Collection | Purpose |
|------------|---------|
| `sessions` | Planner session state (TTL: 1 hour) |
| `curricula` | Approved course plans |
| `tasks` | Individual tasks (flat, not nested) |
| `grade_jobs` | Grading history and results |

### Indexes Created

Run `scripts/create_indexes.js` to create all required indexes for optimal performance.
