# MongoDB Index Setup

## Prerequisites

- MongoDB connection string (from Railway or local instance)
- MongoDB CLI (`mongosh`) installed

## Usage

Run the index creation script:

```bash
mongosh "your-mongodb-connection-string" < scripts/create_indexes.js
```

For Railway MongoDB:
```bash
mongosh "$MONGODB_URL" < scripts/create_indexes.js
```

## What It Does

Creates the following indexes for optimal query performance:

### sessions
- TTL index on `expires_at` for automatic document expiration

### curricula
- Index on `session_id` for lookup by originating session
- Index on `student_id` for user-specific queries

### tasks
- Index on `curriculum_id` for fetching all tasks in a curriculum
- Compound index on `curriculum_id` + `status` for status filtering

### grade_jobs
- Compound index on `curriculum_id` + `task_id` for linked grading queries
- Index on `status` for job queue management

## Verification

After running, verify indexes were created:

```bash
mongosh "$MONGODB_URL"
```

Then in the MongoDB shell:
```javascript
use omakasem;
db.sessions.getIndexes();
db.curricula.getIndexes();
db.tasks.getIndexes();
db.grade_jobs.getIndexes();
```
