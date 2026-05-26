---
name: fix
description: Systematic bug fixing: understand→reproduce→diagnose→fix(minimal, root cause)→verify(tests, regressions)
runAs: subagent
---
# Fix

Systematic approach to diagnosing and fixing code issues.

## When to Use
- User reports a bug or error
- User asks to "fix this", "debug this", or "resolve this error"

## Fix Workflow

### 1. Understand the Problem
- What is the expected behavior? What is the actual behavior?
- When did it start? Can it be reproduced consistently?
- What error messages or logs are available?

### 2. Reproduce
- Identify minimal reproduction steps
- Confirm the issue exists in the current codebase
- Check if environment-specific

### 3. Diagnose
- Read error messages and stack traces carefully
- Identify the root cause vs symptoms
- Check recent changes (git log, git diff)
- Add targeted logging if needed

### 4. Fix
- Make the minimal change that fixes the root cause
- Don't fix symptoms; fix the underlying issue
- Consider edge cases the fix might affect
- Maintain backward compatibility when possible

### 5. Verify
- Confirm the original issue is resolved
- Run existing tests to check for regressions
- Add a test case that would have caught this bug
- Test edge cases related to the fix

### 6. Document
- Note the fix in commit message with issue reference

## Common Bug Patterns
| Pattern | Signs | Typical Fix |
|---------|-------|-------------|
| Null reference | Cannot read property of undefined | Add null checks |
| Race condition | Intermittent failures | Proper async/await, locks |
| State mutation | Unexpected state changes | Immutable updates |
| Stale closure | Old values in callbacks | Use refs or functional updates |
| Memory leak | Growing memory usage | Clean up subscriptions |
| Off-by-one | Wrong array indices | Check loop bounds |
