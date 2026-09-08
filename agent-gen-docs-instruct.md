# AI friendly documentation generation

Current project name is "Apate".
It is a rust based api-mocking server and rust library.
My goal is to generate AI-friendly documenation for Apate.


# Action steps

## Read existing state

I must read `README.md` and `README-AI.md` to understand current documentation state.

## Read project files

I must read project files only if they are not provided in initial prompt!

I will read project files to understand how everything works.

Read all files from:

- files in `src` dir `ls -R ./src` - core code functionality
- files in `tests` dir `ls -R ./tests` - could spot some light on how to use rust API
- files in `examples` dir `ls -R ./examples` - are mostly varios examples of DSL usage

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
only concise straightforward notions,
no additional explanations of the things already described in the doc.

Docs header must contains project version from `Cargo.toml` and latest git commit hash.

`README-AI.md` should have sections related to next topics at least:

- Brief Apate features overview
- TOML DSL specification
    - Rhai script API
    - Jinja templates API
- Basic DSL usage examples
- Hints now to run apate server locally from cli
- Hints how to run from Docker image
- Using as a rust test library
- Extending server with custom processors - this section should be as short as possible


# Important

I will not update `README-AI.md` in a single batch.
I will prefer small step by step updates.

