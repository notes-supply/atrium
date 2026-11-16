# Mneme's example responses

Copied from `tests/fixtures/api/` in Mneme at commit `8f40b541dd5fd2cb4adffdc730662a2d730e64f7`. Mneme's own tests check every real response against these shapes, both ways, so Atrium's stub serves them as they are, but for the machines' names, which are the demo's: `ironbark` and `willow`. `docs/api.md` in Mneme describes each route. `report.json` is the body an autosave service posts to `/git/status`; the test against a real Mneme posts it.

`files.json`, `file.json` and `file-changes.json` are copied as they are from Mneme at commit `eb0519639e2c78ee7033b6db50a49e0e318ce986`, after files arrived in notes-supply/mneme#11. Atrium's stub serves bytes for each file of `files.json` as long as its `size`.

Copy them again when Mneme's API changes, and note the new commit here.
