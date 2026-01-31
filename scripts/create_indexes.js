db = db.getSiblingDB('omakasem');

db.sessions.createIndex({ "expires_at": 1 }, { expireAfterSeconds: 0 });
print("✓ Created TTL index on sessions.expires_at");

db.curricula.createIndex({ "session_id": 1 });
print("✓ Created index on curricula.session_id");

db.curricula.createIndex({ "student_id": 1 });
print("✓ Created index on curricula.student_id");

db.tasks.createIndex({ "curriculum_id": 1 });
print("✓ Created index on tasks.curriculum_id");

db.tasks.createIndex({ "curriculum_id": 1, "status": 1 });
print("✓ Created compound index on tasks.curriculum_id + status");

db.grade_jobs.createIndex({ "curriculum_id": 1, "task_id": 1 });
print("✓ Created compound index on grade_jobs.curriculum_id + task_id");

db.grade_jobs.createIndex({ "status": 1 });
print("✓ Created index on grade_jobs.status");

print("\n✅ All MongoDB indexes created successfully!");
