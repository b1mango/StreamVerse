---
name: update-docs
description: Sync docs with code: analyze diff→update README/API/changelog→quality checks (examples, links, terminology)
---
# Update Docs

Keep documentation synchronized with code changes.

## When to Use
- After implementing new features
- After modifying existing APIs or interfaces
- User asks to "update docs" or "sync documentation"

## Workflow

### 1. Analyze Changes
- Review git diff to identify what changed
- Categorize: new feature, modification, deprecation, removal
- Identify which documentation files are affected

### 2. Update Existing Docs
- Update descriptions to match new behavior
- Update code examples to reflect changes
- Mark deprecated features clearly

### 3. Create New Docs (for new features)
- Add feature description and purpose
- Include usage examples with code snippets
- Link from relevant existing docs

### 4. Per-Doc Checklist
**README.md**: Installation still accurate? Usage examples up to date? Dependencies listed?
**API Documentation**: New endpoints documented? Parameters and return types accurate?
**Changelog**: Version bumped? Changes categorized (Added/Changed/Fixed)? Breaking changes highlighted?

### 5. Quality Checks
- Verify code examples actually work
- Check for broken internal links
- Ensure consistent terminology
