---
name: pr-creator
description: Standardized PR creation: pre-flight→type(scope):desc→template(what/why/how/testing)→gh pr create
---
# PR Creator

Create high-quality, standardized pull requests.

## When to Use
- User asks to "create a PR" or "make a pull request"

## Workflow

### 1. Pre-flight Checks
- Ensure all changes are committed
- Run linting: `npm run check`
- Run tests: `cargo test --manifest-path src-tauri/Cargo.toml`
- Check for merge conflicts with target branch

### 2. PR Title Format
`type(scope): description`

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`

Examples:
- `feat(auth): add OAuth2 login support`
- `fix(api): handle null response in user endpoint`
- `docs(readme): update installation instructions`

### 3. PR Description Template
```markdown
## What
Brief description of what changed and why.

## Why
Context and motivation for the change.

## How
Technical approach and key decisions.

## Testing
- [ ] Unit tests pass
- [ ] Manual testing performed

## Checklist
- [ ] Code follows project conventions
- [ ] Self-review completed
- [ ] Documentation updated
```

### 4. Create PR
```bash
gh pr create --title "type(scope): description" --body-file pr_body.md --base main
```
