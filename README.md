# Icaros

A Rust-based interactive file tree viewer that helps you manage which files AI assistants can edit. It displays a folder/file tree with toggles to lock/unlock files and expandable directories.

## Features

- Interactive file tree visualization with TUI (Terminal User Interface)
- **Simple permission system**:
  - Lock files/directories (🔒) - prevents all edits, deletes, and creates
  - Allow create in locked directories (➕) - exception for creating new files in locked dirs
- Expand/collapse directories
- Compact state file using glob patterns
- Auto-saves immediately after each change
- Ignores common directories like `.git`, `target`, `node_modules`
- Ready for future Tauri integration

## Installation

```bash
cargo build --release
```

## Usage

```bash
# Run in current directory
cargo run

# Run in specific directory
cargo run -- /path/to/directory

# Specify custom state file
cargo run -- --state-file custom_state.json

# Add custom ignore patterns
cargo run -- --ignore "*.tmp" --ignore "cache/*"

# Initialize CLAUDE.md and ICAROS.md files
cargo run -- init
```

## Commands

### `icaros init`
Creates or updates `CLAUDE.md` and `ICAROS.md` files in the current directory with instructions for AI assistants about the file lock system. Templates are customizable - see the Template System section below.

### Interactive Mode Controls

- **↑/↓**: Navigate through the file tree
- **Space**: Toggle lock/unlock on selected file/directory
- **c**: Toggle "allow create" on locked directories only
- **Enter**: Expand/collapse directories
- **h**: Toggle hidden files visibility
- **r**: Refresh file tree
- **a**: Toggle animations
- **q**: Quit

## Auto-Save

The tool automatically saves the state immediately after each change. You don't need to worry about losing your locked files or expanded directories - every toggle is instantly persisted to the state file.

## State File

The tool saves a compact state file to `.icaros` using glob patterns:

```json
{
  "root_path": "/path/to/project",
  "locked_patterns": [
    "src/**",
    "tests/important_test.rs",
    "README.md"
  ],
  "unlocked_patterns": ["**"],
  "allow_create_patterns": [
    "src",
    "tests"
  ],
  "expanded_dirs": ["/path/to/project/src"]
}
```

**Pattern Rules:**
- `**` - Default pattern meaning everything is unlocked
- `dir/**` - Lock entire directory and all contents
- `file.ext` - Lock specific file
- Patterns in `allow_create_patterns` are directories where new files can be created even though they're locked

## Visual Indicators

- **▶/▼**: Collapsed/Expanded directory
- **🔒**: Locked (no edits, deletes, or creates allowed)
- **🔒 ➕**: Locked directory but new files can be created
- **Blue**: Directories
- **Red**: Locked items
- **White**: Regular unlocked files

## Testing

Run the test suite:

```bash
cargo test
```

## Lock File Integration

This tool creates a `.icaros` file that tracks locked files. When using Claude or other AI assistants, they should check this file and avoid modifying any files listed in the `locked_files` array.

Example integration:
1. Read `.icaros`
2. Parse the `locked_files` array
3. Before modifying any file, check if its absolute path is in the locked files list
4. If locked, skip the modification and inform the user

## Template System

The markdown prompts used by `icaros init` are extracted into separate `.md` files for easy customization.

### Template Structure

```
prompts/
├── README.md         # Documentation for the template system
├── ICAROS.md        # Main file lock system guide
├── CLAUDE.md        # Template for new CLAUDE.md files
└── CLAUDE_UPDATE.md # Template for updating existing CLAUDE.md
```

### Template Loading Priority

The system loads templates in this order:

1. **User Config Directory** (if exists)
   - macOS: `~/Library/Application Support/icaros/prompts/`
   - Linux: `~/.config/icaros/prompts/`
   - Windows: `%APPDATA%\icaros\prompts\`

2. **Application Directory**
   - `./prompts/` relative to the binary

3. **Embedded Defaults**
   - Compiled into the binary using `include_str!`

### Customization Methods

#### Method 1: Edit Application Templates

Simply edit the files in the `prompts/` directory before building:

```bash
# Edit the templates
vim prompts/ICAROS.md

# Build with your changes
cargo build --release
```

#### Method 2: User-Specific Templates

Create your own templates that override the defaults:

```bash
# macOS example
mkdir -p ~/Library/Application\ Support/icaros/prompts
cp prompts/ICAROS.md ~/Library/Application\ Support/icaros/prompts/
# Edit with your customizations
vim ~/Library/Application\ Support/icaros/prompts/ICAROS.md
```

#### Method 3: Fork and Customize

Fork the repository and maintain your own version with custom templates.

### Template Variables

The `CLAUDE_UPDATE.md` template supports:
- `{existing_content}` - Replaced with existing CLAUDE.md content

### Benefits

1. **Easy Customization** - No need to edit Rust code
2. **User Overrides** - Personal templates without modifying the app
3. **Version Control** - Track template changes separately
4. **Distribution** - Bundle templates with the binary
5. **Flexibility** - Different templates for different users/orgs

### Example Use Cases

- Add organization-specific guidelines to ICAROS.md
- Include project templates in CLAUDE.md
- Customize the update behavior for existing files
- Create templates in different languages

## Future Enhancements

- Tauri integration for a native GUI
- Search functionality
- Filter options
- Export locked files list in different formats
- Integration with AI tool configurations