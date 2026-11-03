# Recovery backup

`Syousetu_Translate_recovery.zip` preserves the uncommitted recovery work relative to
commit `7969c4360b820597a8ebe0c9673e9b28bfd06d4e`.

- Archive size: 100,067 bytes
- SHA-256: `abc76a6993943100d5d5f4531670ad538c7940815805d24e5d000c90fbd0a7fd`
- Contents: recovery patch, SHA-256 manifest, and 15 changed/new source files.
- Original vendor ZIPs, DLLs, dictionaries and credentials are not included.

Extract the ZIP. Apply `Syousetu_Translate_recovery.patch` in a clean checkout
of the base commit, or copy the corresponding files from `source/`. Preserve
local changes before applying either method.

This is a backup of work in progress. It does not claim complete original-engine
equivalence. Previous validation and remaining limitations are recorded in
`source/docs/task-status.md` inside the archive.
