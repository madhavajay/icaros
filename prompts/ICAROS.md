# ICAROS.md - File Lock System Guide

> Icaros are sacred songs and chants used in Central and South American traditions to guide and protect people during spiritual journeys and vision quests.

You are the Shaman-in-the-loop. Use Icaros to guide your Agentic People Spirits on their journey and keep them from 1-shotting themselves.

## CRITICAL: File Lock System

**BEFORE making ANY file modifications in this project:**

1. **ALWAYS** read `.icaros` first
2. Check the `locked_patterns` array
3. Lock rules:
   - If a file/directory matches a pattern in `locked_patterns` → REFUSE all operations (edit, delete, create)
   - Exception: If directory is in `allow_create_patterns` → ALLOW creating new files only
4. Default: Everything is unlocked unless explicitly in `locked_patterns`
5. If locked, inform user that the file/directory is locked

## Lock File Location
- Primary: `.icaros` (in project root)
- Alternative: Check `--state-file` argument if specified

## Example Workflow
```yaml
Before any file operation:
1. Read .icaros
2. Parse locked_patterns array
3. Check if target path matches any pattern:
   - "src/**" matches src/main.rs, src/lib.rs, src/utils/helper.rs
   - "README.md" matches only README.md
4. If matched in locked_patterns:
   - For create operation: Check allow_create_patterns
   - For edit/delete: Always refuse
5. If not matched → proceed with operation
```

## Pattern Matching
- `**` wildcard matches any number of directories
- `dir/**` locks entire directory tree
- Specific files use exact paths relative to root
- Compact representation: if entire dir is locked, just show `dir/**`

## Remember
- The lock file uses absolute paths
- Lock state is saved immediately after changes
- Locked directories lock all their children
- This system helps users control which files AI can modify