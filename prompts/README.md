# Icaros Prompt Templates

This directory contains the markdown templates used by the `icaros init` command.

## Template Files

- **ICAROS.md** - The main file lock system guide that gets created in project roots
- **CLAUDE.md** - Template for new CLAUDE.md files
- **CLAUDE_UPDATE.md** - Template for updating existing CLAUDE.md files to reference ICAROS.md

## Customization

You can customize these templates in three ways:

1. **Edit the templates in this directory** - Changes will be bundled with the app
2. **Create user-specific templates** - Place them in:
   - macOS: `~/Library/Application Support/icaros/prompts/`
   - Linux: `~/.config/icaros/prompts/`
   - Windows: `%APPDATA%\icaros\prompts\`
3. **Fork and modify** - Make your own version of the app with custom templates

## Template Loading Order

When running `icaros init`, the app looks for templates in this order:
1. User config directory (if exists)
2. Application prompts directory
3. Embedded defaults (compiled into the binary)

## Variables

The `CLAUDE_UPDATE.md` template supports the following variable:
- `{existing_content}` - Replaced with the existing CLAUDE.md content (minus the header)

## Example Customization

To customize the ICAROS.md template for your organization:

```bash
# macOS
mkdir -p ~/Library/Application\ Support/icaros/prompts
cp prompts/ICAROS.md ~/Library/Application\ Support/icaros/prompts/
# Edit the file with your custom content
```

Now when you run `icaros init`, it will use your customized template!