# AI friendly documentation generation

Current project name is "Apate".
It is a rust based api-mocking server and rust library.
My goal is to generate AI-friendly documenation for Apate.


# Action steps

## Read project files

I must read project files only if they were not provided in initial prompt!

### Batch files read (preferred)

Must try run next cli command to pack all project in a single file:
`code2prompt . --include="*.rs,*.toml,*.md,Dockerfile" --exclude="agent-gen-docs-instruct.md" -O ./prompt-codebase.txt && echo "Total lines count: $(wc -l ./prompt-codebase.txt)"`

Will read exact lines count from the file `./prompt-codebase.txt` 
If read tool will return trimmed content I will try to read again in batches untill the last file line!

### Read existing state (if code2prompt fails)

I must read first`README.md` and `README-AI.md`.

After then must read all files from:
- `src` dir `ls -R ./src` - core code functionality
- `tests` dir `ls -R ./tests` - could spot some light on how to use rust API
- `examples` dir `ls -R ./examples` - are mostly varios examples of DSL usage

## Validate existing docuementation

Will validate current docs to check if there any discrepancy with an actual project functionality.

Shold also check what files changed since commit provided in the `README-AI.md`
by executing `git diff --name-only GIT_COMMIT_HASH` 

## Update existing documentation

Will align `README-AI.md` with existing project features.
Will reformat docs style and structure according to rules provided below.

If `README-AI.md` address all project functionality then there is no need to update it
but I can ask user to improve document style if needed.

## Final check

I will validate that updated `README-AI.md` is not containing any new errors.


# Documentation structure rules

Documentation must be in AI-friendly format:

 - all info must be in one place must not require to read other project docs & examples
 - only concise straightforward notions
 - do not duplicate information
 - only AI-friendly explanation and formatting allowed
 - I must not copy examples as-is I must generalize things do avoid dummy duplication

Docs header must contains project version from `Cargo.toml` and latest git commit hash.

`README-AI.md` should have sections related to next topics at least:

- Brief Apate features overview
- TOML DSL specification
    - Rhai script API
    - Jinja templates API
- Basic DSL usage examples
- Hints now to run apate server locally from cli (including how to install from cargo)
- Hints how to run from Docker image
- Using as a rust test library
- Extending server with custom processors - this section should be as short as possible
- MCP protocol description must briefly describe only tools not whole MCP protocol
- Section with important notes should clarify important corner cases and DSL usage decision hints

# Important

I will not update `README-AI.md` in a single batch.
I will prefer small step by step updates.

